//! `ParseWorkspace`: reusable allocation scratch space.
//!
//! Public callers see a single [`ParseWorkspace`] that bundles a parser-side
//! workspace (CST tables + diagnostic record buffer) and a semantic-side
//! workspace. `clear()` / `reset()` keep capacity so repeated parsing,
//! batch parsing, benchmark loops, and LSP sessions stay allocation-flat.
//! Capacity is released only on [`Self::shrink_to_fit`] or [`Drop`].

use crate::diagnostic::DiagnosticRecord;
use crate::tables::{CstCapacity, CstEdgeRecord, CstTables};

/// Pre-sizing hint passed to [`ParseWorkspace::with_capacity`].
#[derive(Debug, Default, Clone, Copy)]
pub struct ParseCapacity {
    pub nodes: usize,
    pub edges: usize,
    pub tokens: usize,
    pub trivia: usize,
    pub diagnostics: usize,
}

impl ParseCapacity {
    pub const fn new(nodes: usize, edges: usize, tokens: usize, trivia: usize, diagnostics: usize) -> Self {
        Self {
            nodes,
            edges,
            tokens,
            trivia,
            diagnostics,
        }
    }

    /// Rough heuristic that scales every table linearly with source length.
    /// Used by [`ParseWorkspace::reserve_for_source_len`] when no explicit
    /// hint is provided.
    pub fn for_source_len(source_len: usize) -> Self {
        // Conservative coefficients tuned for typical MF2 messages (mostly
        // short text + a handful of placeholders). Better numbers can drop
        // out of Milestone 11 benchmarks.
        let token_estimate = source_len / 4 + 4;
        Self {
            nodes: token_estimate / 2,
            edges: token_estimate,
            tokens: token_estimate,
            trivia: token_estimate / 4,
            diagnostics: 0,
        }
    }
}

/// Internal parser-side workspace.
///
/// `pending_edges` and `frame_starts` live here (not inside the short-lived
/// `CstBuilder`) so their capacity is preserved across repeated parses —
/// the parser swaps the buffers into a builder for the duration of one
/// `run_parse` and swaps them back when it finishes.
#[derive(Debug, Default)]
pub(crate) struct ParserWorkspace {
    pub tables: CstTables,
    pub diagnostics: Vec<DiagnosticRecord>,
    pub pending_edges: Vec<CstEdgeRecord>,
    pub frame_starts: Vec<u32>,
}

impl ParserWorkspace {
    pub fn clear(&mut self) {
        self.tables.clear();
        self.diagnostics.clear();
        // The staging stacks must be empty after a balanced parse; clearing
        // is a no-op on the success path but keeps capacity reserved.
        self.pending_edges.clear();
        self.frame_starts.clear();
    }

    pub fn reserve(&mut self, capacity: &ParseCapacity) {
        self.tables.reserve(&CstCapacity {
            nodes: capacity.nodes,
            edges: capacity.edges,
            tokens: capacity.tokens,
            trivia: capacity.trivia,
        });
        self.diagnostics.reserve(capacity.diagnostics);
        // Staging-stack peaks are bounded by the live counts at any moment.
        // `pending_edges` is bounded by the deepest unfinished subtree
        // (≤ total edges); reserving the full edge estimate is generous but
        // amortises across reuse. `frame_starts` follows nesting depth, which
        // is typically very small (≤ ~16 for MF2), but we reserve a slot per
        // node estimate as a safe upper bound.
        self.pending_edges.reserve(capacity.edges);
        self.frame_starts.reserve(capacity.nodes);
    }

    pub fn shrink_to_fit(&mut self) {
        self.tables.shrink_to_fit();
        self.diagnostics.shrink_to_fit();
        self.pending_edges.shrink_to_fit();
        self.frame_starts.shrink_to_fit();
    }
}

/// Internal semantic-side workspace.
#[derive(Debug, Default)]
pub(crate) struct SemanticWorkspace {
    pub model: Option<crate::semantic::SemanticModel>,
}

impl SemanticWorkspace {
    pub fn clear(&mut self) {
        if let Some(model) = self.model.as_mut() {
            // Reuse capacity rather than dropping the SemanticModel — vector
            // clears keep their backing allocation so repeated parse loops
            // stay allocation-flat.
            model.declarations.clear();
            model.references.clear();
            model.patterns.clear();
            model.expressions.clear();
            model.markups.clear();
            model.literals.clear();
            model.functions.clear();
            model.options.clear();
            model.attributes.clear();
            model.selectors.clear();
            model.variants.clear();
            model.diagnostics.clear();
        }
    }
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

    pub fn with_capacity(capacity: ParseCapacity) -> Self {
        let mut ws = Self::default();
        ws.parser.reserve(&capacity);
        ws
    }

    /// Pre-size tables based on the byte length of the next source.
    pub fn reserve_for_source_len(&mut self, source_len: usize) {
        let capacity = ParseCapacity::for_source_len(source_len);
        self.parser.reserve(&capacity);
    }

    /// Clear table contents but keep allocated capacity.
    pub fn clear(&mut self) {
        self.parser.clear();
        self.semantic.clear();
    }

    /// Alias for [`Self::clear`]: kept distinct so future versions can
    /// diverge (`reset()` could also reset internal flags / counters).
    pub fn reset(&mut self) {
        self.clear();
    }

    /// Release capacity. Memory is held until this call or [`Drop`].
    pub fn shrink_to_fit(&mut self) {
        self.parser.shrink_to_fit();
    }

    #[doc(hidden)]
    pub fn node_capacity(&self) -> usize {
        self.parser.tables.nodes.capacity()
    }

    #[doc(hidden)]
    pub fn diagnostic_capacity(&self) -> usize {
        self.parser.diagnostics.capacity()
    }

    #[doc(hidden)]
    pub fn pending_edges_capacity(&self) -> usize {
        self.parser.pending_edges.capacity()
    }

    #[doc(hidden)]
    pub fn frame_starts_capacity(&self) -> usize {
        self.parser.frame_starts.capacity()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_preserves_capacity() {
        let mut ws = ParseWorkspace::with_capacity(ParseCapacity::new(128, 256, 256, 128, 16));
        let node_cap = ws.node_capacity();
        let diag_cap = ws.diagnostic_capacity();
        assert!(node_cap >= 128);
        assert!(diag_cap >= 16);

        // Push some scratch then clear — capacity must be preserved.
        ws.parser.tables.nodes.push(crate::tables::CstNodeRecord::default());
        ws.parser.diagnostics.push(DiagnosticRecord::default());
        ws.clear();
        assert_eq!(ws.parser.tables.nodes.len(), 0);
        assert_eq!(ws.parser.diagnostics.len(), 0);
        assert_eq!(ws.node_capacity(), node_cap);
        assert_eq!(ws.diagnostic_capacity(), diag_cap);
    }

    #[test]
    fn shrink_to_fit_releases_capacity() {
        let mut ws = ParseWorkspace::with_capacity(ParseCapacity::new(512, 1024, 1024, 512, 64));
        assert!(ws.node_capacity() >= 512);
        ws.clear();
        ws.shrink_to_fit();
        assert_eq!(ws.node_capacity(), 0);
        assert_eq!(ws.diagnostic_capacity(), 0);
    }

    #[test]
    fn reserve_for_source_len_grows_at_least_to_estimate() {
        let mut ws = ParseWorkspace::new();
        ws.reserve_for_source_len(1024);
        let expected = ParseCapacity::for_source_len(1024);
        assert!(ws.node_capacity() >= expected.nodes);
    }
}
