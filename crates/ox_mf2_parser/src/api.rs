//! Public parser API skeleton.
//!
//! Full behaviour lands across Milestones 3, 6, 7, 8, 9.

use crate::diagnostic::{Diagnostic, DiagnosticView};
use crate::semantic::{SemanticModel, SemanticView};
use crate::source::{SourceFileInput, SourceStore};
use crate::span::SourceId;
use crate::tables::CstTables;
use crate::view::CstView;
use crate::workspace::ParseWorkspace;

#[derive(Debug, Clone, Copy)]
pub struct ParseOptions {
    pub recovery: bool,
    pub parse_semantic: bool,
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

#[derive(Debug, Default, Clone)]
pub struct ParseResult {
    pub cst: CstTables,
    pub semantic: Option<SemanticModel>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Copy)]
pub struct ParseSessionResult<'a> {
    pub cst: CstView<'a>,
    pub semantic: Option<SemanticView<'a>>,
    pub diagnostics: DiagnosticView<'a>,
}

#[derive(Debug, Default, Clone)]
pub struct ParseInput<'a> {
    pub source: &'a str,
    pub path: Option<&'a str>,
    pub locale: Option<&'a str>,
    pub message_id: Option<&'a str>,
    pub base_offset: Option<u32>,
}

#[derive(Debug, Clone, Copy)]
pub enum BatchExecution {
    Sequential,
    Parallel,
}

#[derive(Debug, Clone, Copy)]
pub struct BatchParseOptions {
    pub execution: BatchExecution,
    pub max_threads: Option<usize>,
    pub preserve_order: bool,
}

impl Default for BatchParseOptions {
    fn default() -> Self {
        Self {
            execution: BatchExecution::Sequential,
            max_threads: None,
            preserve_order: true,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct BatchParseResult {
    pub items: Vec<BatchParseItem>,
}

#[derive(Debug, Default, Clone)]
pub struct BatchParseItem {
    pub source: SourceId,
    pub result: ParseResult,
}

pub fn parse_source(
    _sources: &SourceStore,
    _source_id: SourceId,
    _options: ParseOptions,
) -> ParseResult {
    ParseResult::default()
}

pub fn parse_message(_source: &str) -> ParseResult {
    ParseResult::default()
}

pub fn parse_source_session<'a>(
    sources: &'a SourceStore,
    _source_id: SourceId,
    workspace: &'a mut ParseWorkspace,
    _options: ParseOptions,
) -> ParseSessionResult<'a> {
    workspace.clear();
    let cst = CstView::new(sources, &workspace.parser.tables);
    let diagnostics = DiagnosticView {
        sources,
        records: &workspace.parser.diagnostics,
    };
    ParseSessionResult {
        cst,
        semantic: None,
        diagnostics,
    }
}

pub fn parse_batch(inputs: &[ParseInput<'_>], _options: BatchParseOptions) -> BatchParseResult {
    let mut sources = SourceStore::new();
    let mut items = Vec::with_capacity(inputs.len());
    for input in inputs {
        let source_id = sources.add(SourceFileInput {
            source: input.source,
            path: input.path,
            locale: input.locale,
            message_id: input.message_id,
            base_offset: input.base_offset,
        });
        items.push(BatchParseItem {
            source: source_id,
            result: ParseResult::default(),
        });
    }
    BatchParseResult { items }
}
