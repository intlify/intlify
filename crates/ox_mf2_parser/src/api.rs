// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Public parser API.
//!
//! `parse_source` materialises a [`ParseResult`] against a caller-owned source
//! store. `parse_message` keeps its private source owner and syntax result
//! paired in [`StandaloneParseResult`]. `parse_source_session` reuses a
//! borrowed [`crate::ParseWorkspace`] and returns a [`ParseSessionResult`]
//! tied to the workspace lifetime.
//!
//! Every entry point uses the same parser pipeline; they differ only in result
//! ownership, workspace reuse, and single-source versus batch orchestration.

use crate::diagnostic::{Diagnostic, DiagnosticView};
use crate::error::{BatchParseError, ParseError, ParseResource};
use crate::parser::run_parse;
use crate::source::{SourceFileInput, SourceStore};
use crate::span::SourceId;
use crate::tables::CstTables;
use crate::view::CstView;
use crate::workspace::ParseWorkspace;

/// Parser knobs. See `design/002` for the rationale.
#[derive(Debug, Clone, Copy)]
pub struct ParseOptions {
    /// Recover when input is malformed instead of bailing out at the first
    /// syntax error. Defaults to `true`.
    pub recovery: bool,
    /// Preserve `ws` / `bidi` trivia. Defaults to `true`.
    pub collect_trivia: bool,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            recovery: true,
            collect_trivia: true,
        }
    }
}

/// Owned parse result. Detached from any workspace.
#[derive(Debug, Default, Clone)]
pub struct ParseResult {
    pub source: SourceId,
    pub cst: CstTables,
    pub diagnostics: Vec<Diagnostic>,
    /// Whether whitespace / bidi trivia was collected during parsing.
    /// Snapshot writers use this as a capability proof; an empty trivia
    /// table alone cannot distinguish "collected none" from "not collected".
    pub trivia_collected: bool,
}

/// Borrowed parse result. Lives until the next workspace `clear()` / `reset()`.
#[derive(Debug, Clone, Copy)]
pub struct ParseSessionResult<'a> {
    pub source: SourceId,
    pub cst: CstView<'a>,
    pub diagnostics: DiagnosticView<'a>,
    /// Whether whitespace / bidi trivia was collected during parsing.
    pub trivia_collected: bool,
}

/// Self-contained result for the one-shot [`parse_message`] convenience API.
///
/// The fields stay private so the syntax result cannot be detached from the
/// [`SourceStore`] that assigned its source ids. Source-backed consumers must
/// receive both immutable accessors from the same wrapper.
#[derive(Debug, Clone)]
pub struct StandaloneParseResult {
    sources: SourceStore,
    result: ParseResult,
}

impl StandaloneParseResult {
    #[must_use]
    pub const fn sources(&self) -> &SourceStore {
        &self.sources
    }

    #[must_use]
    pub const fn result(&self) -> &ParseResult {
        &self.result
    }
}

/// Single-source batch input.
#[derive(Debug, Default, Clone)]
pub struct ParseInput<'a> {
    pub source: &'a str,
    pub path: Option<&'a str>,
    pub locale: Option<&'a str>,
    pub message_id: Option<&'a str>,
    pub base_offset: Option<u32>,
}

/// Batch execution mode. The current implementation executes sequentially;
/// requesting any other mode returns a [`BatchParseResult`] whose
/// [`BatchParseResult::execution`] is `Sequential` and whose
/// [`BatchParseResult::degraded`] flag is set so callers can observe the
/// fallback. The `Parallel` variant reserves the future parallel contract.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BatchExecution {
    #[default]
    Sequential,
    /// Reserved for parallel batch execution. Today this falls back to
    /// [`Self::Sequential`] and marks the result as `degraded`.
    Parallel,
}

#[derive(Debug, Clone, Copy)]
pub struct BatchParseOptions {
    pub execution: BatchExecution,
    /// Maximum worker count for parallel execution. Currently ignored.
    pub max_threads: Option<usize>,
    /// Requested result ordering for parallel execution. Currently ignored
    /// because the sequential path always preserves input order.
    pub preserve_order: bool,
    pub parse: ParseOptions,
}

impl Default for BatchParseOptions {
    fn default() -> Self {
        Self {
            execution: BatchExecution::Sequential,
            max_threads: None,
            preserve_order: true,
            parse: ParseOptions::default(),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct BatchParseResult {
    pub sources: SourceStore,
    pub items: Vec<BatchParseItem>,
    /// The execution mode that actually ran. The current implementation always
    /// returns [`BatchExecution::Sequential`] regardless of the request.
    pub execution: BatchExecution,
    /// `true` when the requested execution mode was not honoured. A
    /// [`BatchExecution::Parallel`] request currently degrades to sequential;
    /// inspect this before relying on parallel-only assumptions.
    pub degraded: bool,
}

#[derive(Debug, Default, Clone)]
pub struct BatchParseItem {
    pub source: SourceId,
    pub result: ParseResult,
}

/// Parse `source_id` from `sources` and return an owned [`ParseResult`].
pub fn parse_source(
    sources: &SourceStore,
    source_id: SourceId,
    options: ParseOptions,
) -> Result<ParseResult, ParseError> {
    if sources.get(source_id).is_none() {
        return Err(ParseError::InvalidSourceId { source_id });
    }
    let mut workspace = ParseWorkspace::new();
    let source_len = sources.get(source_id).map_or(0, |f| f.text.len());
    workspace.reserve_for_source_len(source_len);

    run_parse(sources, source_id, &mut workspace, options);

    ensure_parse_complete(&workspace)?;
    Ok(materialise_owned_workspace(
        sources, source_id, workspace, options,
    ))
}

/// One-shot convenience parser that retains the source owner with the syntax
/// result.
pub fn parse_message(source: &str) -> Result<StandaloneParseResult, ParseError> {
    if u32::try_from(source.len()).is_err() {
        return Err(ParseError::SourceTooLarge);
    }
    let mut sources = SourceStore::with_capacity(1);
    let source_id = sources
        .try_add(SourceFileInput {
            source,
            ..Default::default()
        })
        .map_err(|_| ParseError::SourceTooLarge)?;
    let result = parse_source(&sources, source_id, ParseOptions::default())?;
    Ok(StandaloneParseResult { sources, result })
}

/// Reuse `workspace` to parse `source_id`. Returns a [`ParseSessionResult`]
/// borrowed from the workspace.
pub fn parse_source_session<'a>(
    sources: &'a SourceStore,
    source_id: SourceId,
    workspace: &'a mut ParseWorkspace,
    options: ParseOptions,
) -> Result<ParseSessionResult<'a>, ParseError> {
    if sources.get(source_id).is_none() {
        return Err(ParseError::InvalidSourceId { source_id });
    }
    workspace.clear();
    run_parse(sources, source_id, workspace, options);

    let cst = CstView::new(sources, source_id, &workspace.parser.tables);
    let diagnostics = DiagnosticView {
        sources,
        records: &workspace.parser.diagnostics,
    };
    ensure_parse_complete(workspace)?;
    Ok(ParseSessionResult {
        source: source_id,
        cst,
        diagnostics,
        trivia_collected: options.collect_trivia,
    })
}

/// Parse `inputs` sequentially and return owned results in input order.
///
/// The current implementation supports only [`BatchExecution::Sequential`].
/// Requesting [`BatchExecution::Parallel`] falls back to the sequential path
/// and sets [`BatchParseResult::degraded`] so callers can detect the downgrade.
/// `max_threads` and `preserve_order` are reserved for parallel execution and
/// currently ignored.
pub fn parse_batch(
    inputs: &[ParseInput<'_>],
    options: BatchParseOptions,
) -> Result<BatchParseResult, BatchParseError> {
    if inputs.len() > u32::MAX as usize {
        return Err(BatchParseError {
            input_index: u32::MAX as usize,
            error: ParseError::ResourceLimit {
                resource: ParseResource::Sources,
            },
        });
    }
    let mut sources = SourceStore::with_capacity(inputs.len());
    let mut items = Vec::with_capacity(inputs.len());
    let mut workspace = ParseWorkspace::new();

    for (input_index, input) in inputs.iter().enumerate() {
        let source_id = sources
            .try_add(SourceFileInput {
                source: input.source,
                path: input.path,
                locale: input.locale,
                message_id: input.message_id,
                base_offset: input.base_offset,
            })
            .map_err(|_| BatchParseError {
                input_index,
                error: ParseError::SourceTooLarge,
            })?;
        workspace.clear();
        workspace.reserve_for_source_len(input.source.len());
        run_parse(&sources, source_id, &mut workspace, options.parse);
        ensure_parse_complete(&workspace)
            .map_err(|error| BatchParseError { input_index, error })?;
        let result = materialise(&sources, source_id, &workspace, options.parse);
        items.push(BatchParseItem {
            source: source_id,
            result,
        });
    }

    let degraded = !matches!(options.execution, BatchExecution::Sequential);

    Ok(BatchParseResult {
        sources,
        items,
        execution: BatchExecution::Sequential,
        degraded,
    })
}

fn ensure_parse_complete(workspace: &ParseWorkspace) -> Result<(), ParseError> {
    let tables = &workspace.parser.tables;
    if tables.root_id().is_none() {
        return Err(ParseError::MissingRoot);
    }
    let limits = [
        (tables.node_count(), ParseResource::Nodes),
        (tables.edge_count(), ParseResource::Edges),
        (tables.token_count(), ParseResource::Tokens),
        (tables.trivia_count(), ParseResource::Trivia),
        (
            workspace.parser.diagnostics.len(),
            ParseResource::Diagnostics,
        ),
    ];
    if let Some((_, resource)) = limits
        .into_iter()
        .find(|(count, _)| *count > u32::MAX as usize)
    {
        return Err(ParseError::ResourceLimit { resource });
    }
    Ok(())
}

fn materialise(
    sources: &SourceStore,
    source_id: SourceId,
    workspace: &ParseWorkspace,
    options: ParseOptions,
) -> ParseResult {
    let cst = workspace.parser.tables.clone();
    let diagnostics = DiagnosticView {
        sources,
        records: &workspace.parser.diagnostics,
    }
    .iter()
    .collect();
    ParseResult {
        source: source_id,
        cst,
        diagnostics,
        trivia_collected: options.collect_trivia,
    }
}

fn materialise_owned_workspace(
    sources: &SourceStore,
    source_id: SourceId,
    mut workspace: ParseWorkspace,
    options: ParseOptions,
) -> ParseResult {
    let cst = core::mem::take(&mut workspace.parser.tables);
    let diagnostics = DiagnosticView {
        sources,
        records: &workspace.parser.diagnostics,
    }
    .iter()
    .collect();
    ParseResult {
        source: source_id,
        cst,
        diagnostics,
        trivia_collected: options.collect_trivia,
    }
}
