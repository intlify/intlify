//! Flat indexed CST tables: nodes / edges / tokens / trivia.
//!
//! Full record layout lands in Milestone 2.

use crate::span::Span;
use crate::syntax_kind::SyntaxKind;

#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub(crate) struct CstNodeRecord {
    pub kind: u16,
    pub flags: u16,
    pub span_start: u32,
    pub span_end: u32,
    pub first_child: u32,
    pub child_count: u32,
    pub data_ref: u32,
}

#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub(crate) struct CstEdgeRecord {
    pub kind: u16,
    pub flags: u16,
    pub ref_id: u32,
}

#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub(crate) struct TokenRecord {
    pub kind: u16,
    pub flags: u16,
    pub source_id: u32,
    pub span_start: u32,
    pub span_end: u32,
    pub first_trivia: u32,
    pub leading_trivia_count: u16,
    pub trailing_trivia_count: u16,
}

#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub(crate) struct TriviaRecord {
    pub kind: u16,
    pub flags: u16,
    pub source_id: u32,
    pub span_start: u32,
    pub span_end: u32,
}

/// Flat indexed CST tables, the Phase 1 parser's primary output.
#[derive(Debug, Default, Clone)]
pub struct CstTables {
    pub(crate) nodes: Vec<CstNodeRecord>,
    pub(crate) edges: Vec<CstEdgeRecord>,
    pub(crate) tokens: Vec<TokenRecord>,
    pub(crate) trivia: Vec<TriviaRecord>,
}

impl CstTables {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn token_count(&self) -> usize {
        self.tokens.len()
    }

    pub fn trivia_count(&self) -> usize {
        self.trivia.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    pub(crate) fn root_kind(&self) -> SyntaxKind {
        SyntaxKind::Root
    }

    pub(crate) fn root_span(&self) -> Span {
        Span::EMPTY
    }
}
