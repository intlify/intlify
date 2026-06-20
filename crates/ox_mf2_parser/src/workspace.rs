//! `ParseWorkspace`: reusable allocation scratch space.
//!
//! Full capacity lifecycle lands in Milestone 3.

use crate::diagnostic::DiagnosticRecord;
use crate::tables::CstTables;

/// Pre-sizing hint passed to [`ParseWorkspace::with_capacity`].
#[derive(Debug, Default, Clone, Copy)]
pub struct ParseCapacity {
    pub nodes: usize,
    pub edges: usize,
    pub tokens: usize,
    pub trivia: usize,
    pub diagnostics: usize,
}

/// Internal parser-side workspace.
#[derive(Debug, Default)]
pub(crate) struct ParserWorkspace {
    pub tables: CstTables,
    pub diagnostics: Vec<DiagnosticRecord>,
}

impl ParserWorkspace {
    pub fn clear(&mut self) {
        self.tables.nodes.clear();
        self.tables.edges.clear();
        self.tables.tokens.clear();
        self.tables.trivia.clear();
        self.diagnostics.clear();
    }
}

/// Internal semantic-side workspace.
#[derive(Debug, Default)]
pub(crate) struct SemanticWorkspace {}

impl SemanticWorkspace {
    pub fn clear(&mut self) {}
}

/// Reusable workspace for repeated parse, batch parse, benchmarks, and LSP.
#[derive(Debug, Default)]
pub struct ParseWorkspace {
    pub(crate) parser: ParserWorkspace,
    pub(crate) semantic: SemanticWorkspace,
}

impl ParseWorkspace {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(_capacity: ParseCapacity) -> Self {
        Self::default()
    }

    pub fn reserve_for_source_len(&mut self, _source_len: usize) {}

    pub fn clear(&mut self) {
        self.parser.clear();
        self.semantic.clear();
    }

    pub fn reset(&mut self) {
        self.clear();
    }

    pub fn shrink_to_fit(&mut self) {
        self.parser.tables.nodes.shrink_to_fit();
        self.parser.tables.edges.shrink_to_fit();
        self.parser.tables.tokens.shrink_to_fit();
        self.parser.tables.trivia.shrink_to_fit();
        self.parser.diagnostics.shrink_to_fit();
    }
}
