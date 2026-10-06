// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Reusable per-worker scratch for unit analysis.
//!
//! The workspace owns the arena a unit's syntax tree is built in, and the
//! shared crate's own scratch for the declarations a unit hands over. The
//! arena is reset before every unit, so nothing one unit allocated survives
//! into the next, and no result points into it: ranges, counts, facts and
//! diagnostics are copied out before the tree is released. The shared scratch
//! clears itself before every use in the same way. A reused workspace and a
//! fresh one therefore give the same result, after a success, a failure, or a
//! cancellation alike.

use intlify_authoring::AnalysisWorkspace;
use oxc_allocator::Allocator;

/// Per-worker scratch for repeated unit analysis.
///
/// One workspace belongs to one worker. Sharing one between workers to reduce
/// allocation is not supported.
#[derive(Default)]
pub struct JsAnalysisWorkspace {
    arena: Allocator,
    analysis: AnalysisWorkspace,
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
    pub(crate) fn fresh(&mut self) -> &Allocator {
        self.arena.reset();
        &self.arena
    }

    /// Lend the shared crate's scratch, which clears itself before each use.
    pub(crate) fn shared(&mut self) -> &mut AnalysisWorkspace {
        &mut self.analysis
    }
}

impl std::fmt::Debug for JsAnalysisWorkspace {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("JsAnalysisWorkspace")
            .field("arena_capacity", &self.arena.capacity())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LARGE: usize = 64 * 1024;

    #[test]
    fn a_new_workspace_owns_no_arena_memory_yet() {
        let workspace = JsAnalysisWorkspace::new();
        assert_eq!(workspace.arena_capacity(), 0);
        assert_eq!(
            format!("{workspace:?}"),
            "JsAnalysisWorkspace { arena_capacity: 0, .. }"
        );
    }

    #[test]
    fn lending_the_scratch_drops_what_the_last_unit_left_and_keeps_its_capacity() {
        let mut workspace = JsAnalysisWorkspace::new();
        let arena = workspace.fresh();
        arena.alloc_str(&"x".repeat(LARGE));
        assert!(arena.used_bytes() >= LARGE);
        assert!(workspace.arena_capacity() >= LARGE);

        // Nothing the previous unit allocated is live in the arena lent to
        // the next one, but the memory it grew into is kept for reuse.
        let arena = workspace.fresh();
        assert_eq!(arena.used_bytes(), 0);
        assert!(workspace.arena_capacity() >= LARGE);
    }
}
