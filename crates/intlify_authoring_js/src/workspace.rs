// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Reusable per-worker scratch for unit analysis.
//!
//! The workspace owns the arena a unit's syntax tree is built in. The arena is
//! reset before every unit, so nothing one unit allocated survives into the
//! next, and no result points into it: ranges, counts and diagnostics are
//! copied out before the tree is released. A reused workspace and a fresh one
//! therefore give the same result, after a success, a failure, or a
//! cancellation alike.

use oxc_allocator::Allocator;

/// Per-worker scratch for repeated unit analysis.
///
/// One workspace belongs to one worker. Sharing one between workers to reduce
/// allocation is not supported.
#[derive(Default)]
pub struct JsAnalysisWorkspace {
    arena: Allocator,
}

impl JsAnalysisWorkspace {
    /// Create an empty workspace.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the arena capacity retained for the next unit, in bytes.
    ///
    /// This is for storage and reuse fixtures. Capacity is kept across units
    /// on purpose; what is not kept is anything allocated in it.
    #[must_use]
    pub fn arena_capacity(&self) -> usize {
        self.arena.capacity()
    }

    /// Release what the previous unit allocated and lend the arena out.
    ///
    /// Resetting here, rather than after a unit, is what makes a workspace
    /// abandoned by a failure or a cancellation safe to reuse: whatever that
    /// unit left behind is dropped before the next one starts.
    pub(crate) fn fresh_arena(&mut self) -> &Allocator {
        self.arena.reset();
        &self.arena
    }
}

impl std::fmt::Debug for JsAnalysisWorkspace {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("JsAnalysisWorkspace")
            .field("arena_capacity", &self.arena.capacity())
            .finish()
    }
}
