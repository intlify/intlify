// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

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

use crate::span::{NodeId, SourceId, Span, TokenId, TriviaId, NONE_U32};
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

/// Builder-time staging slot for a pending node. Every active node remembers
/// the offset at which its child edges begin inside the shared
/// [`CstBuilder::pending_edges`] stack. `finish_node` drains the contiguous
/// range `[edge_start, pending_edges.len())` into [`CstTables::edges`] in
/// post-order, so each node's `[first_child, first_child + child_count)`
/// range points strictly at its direct children.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PendingNode {
    pub kind: SyntaxKind,
    pub flags: u16,
    pub span_start: u32,
    pub frame_depth: u32,
    pub edge_start: u32,
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

    // The optional `.get(...)` accessors used to back every public view
    // method; P8 replaced them with `_at` shortcuts (always-in-bounds for
    // parser-produced ids). Keep them `#[allow(dead_code)]` only if a
    // future bounded-lookup caller resurfaces them — for now they would
    // just bit-rot, so drop them.

    // ── crate-internal fast accessors ──────────────────────────────────
    //
    // Callers below trust that the parser only ever hands them ids it
    // produced itself, so the bounds check is amortised by indexing
    // straight into the backing vector. The `_at` accessors panic in
    // `debug` builds and use the slice's bounds check in `release`; the
    // public `view::CstView` keeps the `Option` form for untrusted callers.

    #[inline]
    pub(crate) fn node_at(&self, id: NodeId) -> &CstNodeRecord {
        &self.nodes[id.index()]
    }

    #[inline]
    pub(crate) fn token_at(&self, id: TokenId) -> &TokenRecord {
        &self.tokens[id.index()]
    }

    #[inline]
    pub(crate) fn trivia_at(&self, id: TriviaId) -> &TriviaRecord {
        &self.trivia[id.index()]
    }

    /// Slice covering exactly the direct children of `node`. Replaces the
    /// per-edge `tables.edge(id)?` lookup in hot traversal paths.
    #[inline]
    pub(crate) fn edges_for(&self, node: &CstNodeRecord) -> &[CstEdgeRecord] {
        let start = node.first_child as usize;
        let end = start + node.child_count as usize;
        &self.edges[start..end]
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
///
/// All in-flight child edges share a single staging stack
/// ([`Self::pending_edges`]). Each active `start_node` only records its
/// `edge_start` offset into that stack via [`PendingNode::edge_start`]; the
/// matching `finish_node` drains the contiguous range into
/// [`CstTables::edges`]. Recovery checkpoints snapshot table lengths, frame
/// depth, and the pending-edge stack length, and roll back via
/// [`Self::rollback_to`].
#[derive(Debug, Default)]
pub(crate) struct CstBuilder {
    pub(crate) tables: CstTables,
    /// Single shared staging stack — all open nodes contribute to it. Each
    /// pending node owns the range `[edge_start, pending_edges.len())` at
    /// any given moment.
    pub(crate) pending_edges: Vec<CstEdgeRecord>,
    /// One entry per active `start_node`, holding its `edge_start` offset.
    pub(crate) frame_starts: Vec<u32>,
}

/// Snapshot of all builder lengths at a single moment. Originally used by
/// the parser's speculative checkpoint/rollback path; P2 replaced those
/// sites with non-destructive trivia lookahead, so the parser only reads
/// `trivia` from this struct today. The other fields stay so that
/// `rollback_to` remains a single-shot operation if a future recovery path
/// needs it, and so the struct's stable shape can outlive the current
/// caller mix.
#[derive(Debug, Default, Clone, Copy)]
#[allow(dead_code)] // see above — fields read only by `rollback_to`.
pub(crate) struct BuilderLengths {
    pub nodes: u32,
    pub edges: u32,
    pub tokens: u32,
    pub trivia: u32,
    pub frame_depth: u32,
    pub pending_edges: u32,
}

impl CstBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every staging length at once. The parser hot path now
    /// reads individual fields directly off the backing vectors (see
    /// `commit_token` / `eat_trivia`), so this is only used by the
    /// rollback path and the table-level unit tests.
    #[allow(dead_code)]
    pub fn lengths(&self) -> BuilderLengths {
        BuilderLengths {
            nodes: self.tables.nodes.len() as u32,
            edges: self.tables.edges.len() as u32,
            tokens: self.tables.tokens.len() as u32,
            trivia: self.tables.trivia.len() as u32,
            frame_depth: self.frame_starts.len() as u32,
            pending_edges: self.pending_edges.len() as u32,
        }
    }

    /// Truncate every table back to the captured lengths. Drops any frame
    /// starts above the captured depth and trims the shared edge stack so
    /// speculative children pushed inside the rolled-back branch disappear.
    /// Currently exercised only by the table-level unit test — the parser's
    /// speculative branches were rewritten to use non-destructive lookahead
    /// in P2. Kept for any future recovery path that needs full rollback.
    #[allow(dead_code)]
    pub fn rollback_to(&mut self, lengths: BuilderLengths) {
        self.tables.nodes.truncate(lengths.nodes as usize);
        self.tables.edges.truncate(lengths.edges as usize);
        self.tables.tokens.truncate(lengths.tokens as usize);
        self.tables.trivia.truncate(lengths.trivia as usize);
        self.frame_starts.truncate(lengths.frame_depth as usize);
        self.pending_edges.truncate(lengths.pending_edges as usize);
    }

    /// Begin a node. Records the current edge-stack length as this node's
    /// `edge_start` and pushes it to the frame stack. Subsequent
    /// `push_*_edge` calls append to the shared `pending_edges` stack;
    /// `finish_node` later drains the range belonging to this node.
    pub fn start_node(&mut self, kind: SyntaxKind, span_start: u32) -> PendingNode {
        let edge_start = self.pending_edges.len() as u32;
        let depth = self.frame_starts.len() as u32;
        self.frame_starts.push(edge_start);
        PendingNode {
            kind,
            flags: 0,
            span_start,
            frame_depth: depth,
            edge_start,
        }
    }

    /// Finish the most recently started node. Copies the contiguous range
    /// `[edge_start, pending_edges.len())` into [`CstTables::edges`] and
    /// records the node with the resulting `first_child` / `child_count`.
    ///
    /// The drained range is always a *suffix* of `pending_edges` and
    /// `CstEdgeRecord` is `Copy`, so the previous `extend(drain(..))` is
    /// replaced with `extend_from_slice + truncate`. That avoids the
    /// `Drain` iterator dispatch on every `finish_node` call (one per CST
    /// node) and lets the copy go through the `Vec::extend_from_slice`
    /// specialisation that bulk-copies the `Copy` slice.
    pub fn finish_node(&mut self, pending: PendingNode, span_end: u32) -> NodeId {
        debug_assert_eq!(
            pending.frame_depth as usize,
            self.frame_starts.len() - 1,
            "finish_node out of nesting order"
        );
        let edge_start = self.frame_starts.pop().expect("frame for pending node");
        debug_assert_eq!(edge_start, pending.edge_start);
        let first_child = self.tables.edges.len() as u32;
        let edge_start_us = edge_start as usize;
        let child_count = (self.pending_edges.len() - edge_start_us) as u32;
        self.tables
            .edges
            .extend_from_slice(&self.pending_edges[edge_start_us..]);
        self.pending_edges.truncate(edge_start_us);
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

    /// Append a node-edge to the active pending node's range on the shared
    /// edge stack.
    pub fn push_node_edge(&mut self, child: NodeId) {
        debug_assert!(
            !self.frame_starts.is_empty(),
            "a node must be started before pushing edges"
        );
        self.pending_edges.push(CstEdgeRecord {
            kind: CstEdgeKind::Node as u16,
            flags: 0,
            ref_id: child.raw(),
        });
    }

    /// Append a token-edge to the active pending node's range on the shared
    /// edge stack.
    pub fn push_token_edge(&mut self, token: TokenId) {
        debug_assert!(
            !self.frame_starts.is_empty(),
            "a node must be started before pushing edges"
        );
        self.pending_edges.push(CstEdgeRecord {
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

        let node = b.tables.node_at(node_id);
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
