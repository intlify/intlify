// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Reusable invocation scratch.
//!
//! The workspace retains collection capacity across messages and invocations.
//! It never lends storage to a result: every returned value is drained out, so
//! a retained analysis owns its data and cannot observe a later reset.
//!
//! Resetting is semantic as well as physical. After a success, a failure, or a
//! cancellation, the next analysis starts from the same empty state, so a
//! reused workspace and a fresh one produce equivalent logical results.

use crate::diagnostic::Diagnostic;
use crate::message::ExtractionSegment;

/// Per-worker scratch for repeated message analysis.
///
/// One workspace belongs to one worker. Sharing a mutable workspace between
/// workers to reduce allocation count is not supported.
#[derive(Debug, Default)]
pub struct AnalysisWorkspace {
    pub(crate) segments: Vec<ExtractionSegment>,
    pub(crate) names: Vec<String>,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

impl AnalysisWorkspace {
    /// Create an empty workspace.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop retained semantic state while keeping collection capacity.
    ///
    /// This runs before every analysis, so an earlier failure or cancellation
    /// cannot leak partial segments, names, or diagnostics into a later result.
    pub fn clear(&mut self) {
        self.segments.clear();
        self.names.clear();
        self.diagnostics.clear();
    }

    /// Release pathological high-water capacity.
    ///
    /// This is an explicit policy call, deliberately outside the common path.
    pub fn shrink_to_fit(&mut self) {
        self.segments.shrink_to_fit();
        self.names.shrink_to_fit();
        self.diagnostics.shrink_to_fit();
    }

    /// Return the retained capacities, for storage and reuse fixtures.
    #[must_use]
    pub fn capacities(&self) -> WorkspaceCapacities {
        WorkspaceCapacities {
            segments: self.segments.capacity(),
            names: self.names.capacity(),
            diagnostics: self.diagnostics.capacity(),
        }
    }
}

/// Retained capacity of one workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceCapacities {
    /// Retained extraction-segment capacity.
    pub segments: usize,
    /// Retained external-name capacity.
    pub names: usize,
    /// Retained diagnostic capacity.
    pub diagnostics: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clearing_retains_capacity_and_leaves_no_semantic_state() {
        let mut workspace = AnalysisWorkspace::new();
        workspace.names.push("count".into());
        workspace.names.push("name".into());
        let grown = workspace.capacities().names;
        assert!(grown >= 2);
        workspace.clear();
        assert!(workspace.names.is_empty());
        assert_eq!(workspace.capacities().names, grown);
        workspace.shrink_to_fit();
        assert_eq!(workspace.capacities().names, 0);
    }
}
