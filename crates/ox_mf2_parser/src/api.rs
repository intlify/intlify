// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Public parser API.
//!
//! `parse_message` and `parse_source` are owned-result entry points. They
//! materialise a [`ParseResult`] that the caller owns. `parse_source_session`
//! reuses a borrowed [`crate::ParseWorkspace`] and returns a
//! [`ParseSessionResult`] tied to the workspace lifetime.
//!
//! Concrete parsing behaviour is filled in by Milestones 5, 6, 7, 8, and 9.

use crate::diagnostic::{Diagnostic, DiagnosticView};
use crate::error::{BatchParseError, ParseError, ParseResource};
use crate::parser::{run_parse, run_parse_text};
use crate::semantic::{lower_into as lower_semantic_into, SemanticModel, SemanticView};
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
    /// Build the optional [`SemanticModel`] after a diagnostic-free parse.
    /// Defaults to `false`. Parser diagnostics always suppress construction.
    pub parse_semantic: bool,
    /// Preserve `ws` / `bidi` trivia. Defaults to `true`.
    pub collect_trivia: bool,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            recovery: true,
            parse_semantic: false,
            collect_trivia: true,
        }
    }
}

/// Owned parse result. Detached from any workspace.
#[derive(Debug, Default, Clone)]
pub struct ParseResult {
    pub source: SourceId,
    pub cst: CstTables,
    pub semantic: Option<SemanticModel>,
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
    pub semantic: Option<SemanticView<'a>>,
    pub diagnostics: DiagnosticView<'a>,
    /// Whether whitespace / bidi trivia was collected during parsing.
    pub trivia_collected: bool,
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

/// Batch execution mode. Phase 1 only implements [`BatchExecution::Sequential`];
/// requesting any other mode returns a [`BatchParseResult`] whose
/// [`BatchParseResult::execution`] is `Sequential` and whose
/// [`BatchParseResult::degraded`] flag is set so callers can observe the
/// fallback. The `Parallel` variant is reserved for a future milestone.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BatchExecution {
    #[default]
    Sequential,
    /// Reserved for Phase 2. Today this falls back to [`Self::Sequential`]
    /// at run time and the result is marked `degraded`.
    Parallel,
}

#[derive(Debug, Clone, Copy)]
pub struct BatchParseOptions {
    pub execution: BatchExecution,
    /// Reserved for Phase 2 parallel execution. Currently ignored.
    pub max_threads: Option<usize>,
    /// Reserved for Phase 2 parallel execution. Currently ignored — the
    /// sequential path always preserves input order.
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
    /// The execution mode that actually ran. Phase 1 always returns
    /// [`BatchExecution::Sequential`] regardless of the request.
    pub execution: BatchExecution,
    /// `true` when the requested execution mode was not honoured (Phase 1
    /// downgrades [`BatchExecution::Parallel`] to sequential). Inspect this
    /// before relying on parallel-only assumptions.
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

/// One-shot convenience parser. Parses `source` directly and returns an owned
/// [`ParseResult`]. The success path does not allocate a temporary
/// [`SourceStore`]; malformed inputs build one only when diagnostics need
/// line/column materialisation.
pub fn parse_message(source: &str) -> Result<ParseResult, ParseError> {
    if u32::try_from(source.len()).is_err() {
        return Err(ParseError::SourceTooLarge);
    }
    let source_id = SourceId::new(0);
    let mut workspace = ParseWorkspace::new();
    workspace.reserve_for_source_len(source.len());
    run_parse_text(source, source_id, &mut workspace, ParseOptions::default());
    ensure_parse_complete(&workspace)?;
    Ok(materialise_one_shot_message(source, source_id, workspace))
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

    if options.parse_semantic && workspace.parser.diagnostics.is_empty() {
        // Reuse the workspace-held SemanticModel's capacity instead of
        // allocating a fresh model every session.
        let model = workspace
            .semantic
            .model
            .get_or_insert_with(SemanticModel::default);
        lower_semantic_into(sources, source_id, &workspace.parser.tables, model);
    } else {
        workspace.semantic.model = None;
    }

    let cst = CstView::new(sources, source_id, &workspace.parser.tables);
    let diagnostics = DiagnosticView {
        sources,
        records: &workspace.parser.diagnostics,
    };
    let semantic = workspace
        .semantic
        .model
        .as_ref()
        .map(|m| SemanticView::new(m, &workspace.parser.tables));
    ensure_parse_complete(workspace)?;
    Ok(ParseSessionResult {
        source: source_id,
        cst,
        semantic,
        diagnostics,
        trivia_collected: options.collect_trivia,
    })
}

/// Parse `inputs` sequentially and return owned results in input order.
///
/// Phase 1 only supports [`BatchExecution::Sequential`]. Requesting
/// [`BatchExecution::Parallel`] falls back to the sequential path and sets
/// [`BatchParseResult::degraded`] so callers can detect the downgrade.
/// `max_threads` and `preserve_order` are reserved for Phase 2 and
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
    let semantic = if options.parse_semantic && workspace.parser.diagnostics.is_empty() {
        let mut model = SemanticModel::default();
        lower_semantic_into(sources, source_id, &cst, &mut model);
        Some(model)
    } else {
        None
    };
    let diagnostics = DiagnosticView {
        sources,
        records: &workspace.parser.diagnostics,
    }
    .iter()
    .collect();
    ParseResult {
        source: source_id,
        cst,
        semantic,
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
    let semantic = if options.parse_semantic && workspace.parser.diagnostics.is_empty() {
        let mut model = SemanticModel::default();
        lower_semantic_into(sources, source_id, &cst, &mut model);
        Some(model)
    } else {
        None
    };
    let diagnostics = DiagnosticView {
        sources,
        records: &workspace.parser.diagnostics,
    }
    .iter()
    .collect();
    ParseResult {
        source: source_id,
        cst,
        semantic,
        diagnostics,
        trivia_collected: options.collect_trivia,
    }
}

fn materialise_one_shot_message(
    source: &str,
    source_id: SourceId,
    mut workspace: ParseWorkspace,
) -> ParseResult {
    let cst = core::mem::take(&mut workspace.parser.tables);
    let diagnostics = if workspace.parser.diagnostics.is_empty() {
        Vec::new()
    } else {
        let mut sources = SourceStore::with_capacity(1);
        let actual_source_id = sources.add(SourceFileInput {
            source,
            ..Default::default()
        });
        debug_assert_eq!(actual_source_id, source_id);
        DiagnosticView {
            sources: &sources,
            records: &workspace.parser.diagnostics,
        }
        .iter()
        .collect()
    };
    ParseResult {
        source: source_id,
        cst,
        semantic: None,
        diagnostics,
        trivia_collected: true,
    }
}
