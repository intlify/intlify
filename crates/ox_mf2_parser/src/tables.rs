//! Flat indexed CST tables: nodes / edges / tokens / trivia.
//!
//! Phase 1 represents the CST as four `Vec`s of fixed-size records, addressed
//! by `u32` indexes. Spans live inline in node, token, and trivia records.
//! Child relationships are stored as a contiguous `[first_child, first_child +
//! child_count)` range into the edge table; each edge identifies whether it
//! points at a node or a token.
//!
//! Record layouts are pinned by `size_of` tests so accidental field growth
//! shows up at compile time rather than as a cache regression later.

use crate::span::{EdgeId, NodeId, SourceId, Span, TokenId, TriviaId, NONE_U32};
use crate::syntax_kind::SyntaxKind;

/// Edge-payload kind: does the edge point at a node or a token?
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum CstEdgeKind {
    /// `ref_id` is a [`NodeId`].
    Node = 0,
    /// `ref_id` is a [`TokenId`].
    Token = 1,
}

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

/// Builder-time staging slot for a pending node. The node record itself is
/// not pushed into [`CstTables::nodes`] until the children have been
/// resolved; this lets `first_child` and `child_count` be filled in linearly.
#[allow(dead_code)] // wired through the parser in Milestones 6/7.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PendingNode {
    pub kind: SyntaxKind,
    pub flags: u16,
    pub span_start: u32,
    pub edge_mark: u32,
}

/// Flat indexed CST tables — the Phase 1 parser's primary output.
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

    /// Total node count, including recovery / missing / unknown nodes.
    #[inline]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    #[inline]
    pub fn token_count(&self) -> usize {
        self.tokens.len()
    }

    #[inline]
    pub fn trivia_count(&self) -> usize {
        self.trivia.len()
    }

    #[inline]
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    #[inline]
    pub fn root_id(&self) -> Option<NodeId> {
        if self.nodes.is_empty() {
            None
        } else {
            // Builder always appends the root last (post-order), so it's the
            // final node in the table. Convention is captured here so that
            // accessors can stay simple.
            Some(NodeId::new((self.nodes.len() - 1) as u32))
        }
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
        self.edges.clear();
        self.tokens.clear();
        self.trivia.clear();
    }

    pub fn reserve(&mut self, capacity: &CstCapacity) {
        self.nodes.reserve(capacity.nodes);
        self.edges.reserve(capacity.edges);
        self.tokens.reserve(capacity.tokens);
        self.trivia.reserve(capacity.trivia);
    }

    pub fn shrink_to_fit(&mut self) {
        self.nodes.shrink_to_fit();
        self.edges.shrink_to_fit();
        self.tokens.shrink_to_fit();
        self.trivia.shrink_to_fit();
    }

    #[inline]
    pub(crate) fn node(&self, id: NodeId) -> Option<&CstNodeRecord> {
        self.nodes.get(id.index())
    }

    #[inline]
    pub(crate) fn token(&self, id: TokenId) -> Option<&TokenRecord> {
        self.tokens.get(id.index())
    }

    #[inline]
    pub(crate) fn trivia(&self, id: TriviaId) -> Option<&TriviaRecord> {
        self.trivia.get(id.index())
    }

    #[inline]
    pub(crate) fn edge(&self, id: EdgeId) -> Option<&CstEdgeRecord> {
        self.edges.get(id.index())
    }
}

/// Pre-sizing hint for [`CstTables::reserve`].
#[derive(Debug, Default, Clone, Copy)]
pub struct CstCapacity {
    pub nodes: usize,
    pub edges: usize,
    pub tokens: usize,
    pub trivia: usize,
}

/// Builder API used by the parser to commit nodes, edges, tokens, and trivia.
/// Recovery checkpoints capture table lengths and roll back via [`Self::rollback_to`].
#[allow(dead_code)] // wired through the parser in Milestones 6/7.
#[derive(Debug, Default)]
pub(crate) struct CstBuilder {
    pub(crate) tables: CstTables,
}

#[allow(dead_code)] // wired through the parser in Milestones 6/7.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct BuilderLengths {
    pub nodes: u32,
    pub edges: u32,
    pub tokens: u32,
    pub trivia: u32,
}

#[allow(dead_code)] // wired through the parser in Milestones 6/7.
impl CstBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn lengths(&self) -> BuilderLengths {
        BuilderLengths {
            nodes: self.tables.nodes.len() as u32,
            edges: self.tables.edges.len() as u32,
            tokens: self.tables.tokens.len() as u32,
            trivia: self.tables.trivia.len() as u32,
        }
    }

    pub fn rollback_to(&mut self, lengths: BuilderLengths) {
        self.tables.nodes.truncate(lengths.nodes as usize);
        self.tables.edges.truncate(lengths.edges as usize);
        self.tables.tokens.truncate(lengths.tokens as usize);
        self.tables.trivia.truncate(lengths.trivia as usize);
    }

    /// Begin a node. Returns a marker that captures the edge buffer mark; all
    /// edges pushed before [`Self::finish_node`] become this node's children.
    pub fn start_node(&mut self, kind: SyntaxKind, span_start: u32) -> PendingNode {
        PendingNode {
            kind,
            flags: 0,
            span_start,
            edge_mark: self.tables.edges.len() as u32,
        }
    }

    /// Finish a node previously started with [`Self::start_node`]. Returns
    /// the new [`NodeId`].
    pub fn finish_node(&mut self, pending: PendingNode, span_end: u32) -> NodeId {
        let first_child = pending.edge_mark;
        let child_count = self.tables.edges.len() as u32 - first_child;
        let id = NodeId::new(self.tables.nodes.len() as u32);
        self.tables.nodes.push(CstNodeRecord {
            kind: pending.kind.as_u16(),
            flags: pending.flags,
            span_start: pending.span_start,
            span_end,
            first_child,
            child_count,
            data_ref: NONE_U32,
        });
        id
    }

    /// Attach an already-built node as the next child edge of the current
    /// pending node.
    pub fn push_node_edge(&mut self, child: NodeId) {
        self.tables.edges.push(CstEdgeRecord {
            kind: CstEdgeKind::Node as u16,
            flags: 0,
            ref_id: child.raw(),
        });
    }

    /// Attach a token as the next child edge of the current pending node.
    pub fn push_token_edge(&mut self, token: TokenId) {
        self.tables.edges.push(CstEdgeRecord {
            kind: CstEdgeKind::Token as u16,
            flags: 0,
            ref_id: token.raw(),
        });
    }

    /// Commit a token record. Tokens are not children unless wired up via
    /// [`Self::push_token_edge`].
    pub fn push_token(
        &mut self,
        kind: SyntaxKind,
        source: SourceId,
        span: Span,
        first_trivia: u32,
        leading_trivia_count: u16,
        trailing_trivia_count: u16,
    ) -> TokenId {
        let id = TokenId::new(self.tables.tokens.len() as u32);
        self.tables.tokens.push(TokenRecord {
            kind: kind.as_u16(),
            flags: 0,
            source_id: source.raw(),
            span_start: span.start,
            span_end: span.end,
            first_trivia,
            leading_trivia_count,
            trailing_trivia_count,
        });
        id
    }

    pub fn push_trivia(
        &mut self,
        kind: SyntaxKind,
        source: SourceId,
        span: Span,
    ) -> TriviaId {
        let id = TriviaId::new(self.tables.trivia.len() as u32);
        self.tables.trivia.push(TriviaRecord {
            kind: kind.as_u16(),
            flags: 0,
            source_id: source.raw(),
            span_start: span.start,
            span_end: span.end,
        });
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::size_of;

    #[test]
    fn record_sizes_stay_within_budget() {
        // Targets from design/002 §"Record Layout / Size Budget".
        assert_eq!(size_of::<CstNodeRecord>(), 24);
        assert_eq!(size_of::<CstEdgeRecord>(), 8);
        assert_eq!(size_of::<TokenRecord>(), 24);
        assert_eq!(size_of::<TriviaRecord>(), 16);
    }

    #[test]
    fn builder_finishes_node_with_child_range() {
        let mut b = CstBuilder::new();
        let pending = b.start_node(SyntaxKind::Pattern, 0);
        let token = b.push_token(
            SyntaxKind::TextToken,
            SourceId::new(0),
            Span::new(0, 5),
            0,
            0,
            0,
        );
        b.push_token_edge(token);
        let node_id = b.finish_node(pending, 5);

        let node = b.tables.node(node_id).unwrap();
        assert_eq!(node.kind, SyntaxKind::Pattern.as_u16());
        assert_eq!(node.span_start, 0);
        assert_eq!(node.span_end, 5);
        assert_eq!(node.first_child, 0);
        assert_eq!(node.child_count, 1);
        assert_eq!(node.data_ref, NONE_U32);
        assert_eq!(b.tables.token_count(), 1);
        assert_eq!(b.tables.edge_count(), 1);
    }

    #[test]
    fn rollback_truncates_table_lengths() {
        let mut b = CstBuilder::new();
        let mark = b.lengths();
        let pending = b.start_node(SyntaxKind::Pattern, 0);
        let _ = b.push_token(
            SyntaxKind::TextToken,
            SourceId::new(0),
            Span::new(0, 3),
            0,
            0,
            0,
        );
        let _ = b.finish_node(pending, 3);
        assert_eq!(b.tables.node_count(), 1);
        assert_eq!(b.tables.token_count(), 1);

        b.rollback_to(mark);
        assert_eq!(b.tables.node_count(), 0);
        assert_eq!(b.tables.token_count(), 0);
        assert_eq!(b.tables.edge_count(), 0);
    }
}
