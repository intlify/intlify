//! Public parser API.
//!
//! `parse_message` and `parse_source` are owned-result entry points. They
//! materialise a [`ParseResult`] that the caller owns. `parse_source_session`
//! reuses a borrowed [`crate::ParseWorkspace`] and returns a
//! [`ParseSessionResult`] tied to the workspace lifetime.
//!
//! Concrete parsing behaviour is filled in by Milestones 5, 6, 7, 8, and 9.

use crate::diagnostic::{Diagnostic, DiagnosticView};
use crate::parser::run_parse;
use crate::semantic::{lower as lower_semantic, SemanticModel, SemanticView};
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
    /// Build the optional [`SemanticModel`]. Defaults to `false`.
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
}

/// Borrowed parse result. Lives until the next workspace `clear()` / `reset()`.
#[derive(Debug, Clone, Copy)]
pub struct ParseSessionResult<'a> {
    pub source: SourceId,
    pub cst: CstView<'a>,
    pub semantic: Option<SemanticView<'a>>,
    pub diagnostics: DiagnosticView<'a>,
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
) -> ParseResult {
    let mut workspace = ParseWorkspace::new();
    let source_len = sources
        .get(source_id)
        .map_or(0, |f| f.text.len());
    workspace.reserve_for_source_len(source_len);

    run_parse(sources, source_id, &mut workspace, options);

    materialise(sources, source_id, &workspace, options)
}

/// One-shot convenience parser. Registers `source` in a fresh
/// [`SourceStore`] and returns an owned [`ParseResult`].
pub fn parse_message(source: &str) -> ParseResult {
    let mut sources = SourceStore::new();
    let source_id = sources.add(SourceFileInput {
        source,
        ..Default::default()
    });
    parse_source(&sources, source_id, ParseOptions::default())
}

/// Reuse `workspace` to parse `source_id`. Returns a [`ParseSessionResult`]
/// borrowed from the workspace.
pub fn parse_source_session<'a>(
    sources: &'a SourceStore,
    source_id: SourceId,
    workspace: &'a mut ParseWorkspace,
    options: ParseOptions,
) -> ParseSessionResult<'a> {
    workspace.clear();
    run_parse(sources, source_id, workspace, options);

    if options.parse_semantic {
        let model = lower_semantic(sources, source_id, &workspace.parser.tables);
        workspace.semantic.model = Some(model);
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
    ParseSessionResult {
        source: source_id,
        cst,
        semantic,
        diagnostics,
    }
}

/// Parse `inputs` sequentially and return owned results in input order.
///
/// Phase 1 only supports [`BatchExecution::Sequential`]. Requesting
/// [`BatchExecution::Parallel`] falls back to the sequential path and sets
/// [`BatchParseResult::degraded`] so callers can detect the downgrade.
/// `max_threads` and `preserve_order` are reserved for Phase 2 and
/// currently ignored.
pub fn parse_batch(inputs: &[ParseInput<'_>], options: BatchParseOptions) -> BatchParseResult {
    let mut sources = SourceStore::with_capacity(inputs.len());
    let mut items = Vec::with_capacity(inputs.len());
    let mut workspace = ParseWorkspace::new();

    for input in inputs {
        let source_id = sources.add(SourceFileInput {
            source: input.source,
            path: input.path,
            locale: input.locale,
            message_id: input.message_id,
            base_offset: input.base_offset,
        });
        workspace.clear();
        workspace.reserve_for_source_len(input.source.len());
        run_parse(&sources, source_id, &mut workspace, options.parse);
        let result = materialise(&sources, source_id, &workspace, options.parse);
        items.push(BatchParseItem {
            source: source_id,
            result,
        });
    }

    let degraded = !matches!(options.execution, BatchExecution::Sequential);

    BatchParseResult {
        sources,
        items,
        execution: BatchExecution::Sequential,
        degraded,
    }
}

fn materialise(
    sources: &SourceStore,
    source_id: SourceId,
    workspace: &ParseWorkspace,
    options: ParseOptions,
) -> ParseResult {
    let cst = workspace.parser.tables.clone();
    let semantic = if options.parse_semantic {
        Some(lower_semantic(sources, source_id, &cst))
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
    }
}
