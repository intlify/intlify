// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Reusable scratch for resolving an inventory's identities against a
//! registry.
//!
//! The workspace keeps collection capacity between runs. It never lends
//! storage to a result: everything returned is drained out of it, and every
//! run starts from the same empty state, so a reused workspace and a fresh
//! one give the same result.

use intlify_authoring::{Diagnostic, MessageIntentId};

use crate::reconcile::{DeclarationClass, EntryClass};

/// Per-worker scratch for repeated identity resolution.
///
/// One workspace belongs to one worker; sharing a mutable one between
/// workers is not supported.
#[derive(Debug, Default)]
pub struct IdentityWorkspace {
    pub(crate) classes: Vec<Option<DeclarationClass>>,
    pub(crate) entries: Vec<(MessageIntentId, EntryClass)>,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

impl IdentityWorkspace {
    /// Create an empty workspace.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop retained state while keeping collection capacity.
    ///
    /// Every run starts here, so an earlier failure or cancellation cannot
    /// leak classes or diagnostics into a later result.
    pub fn clear(&mut self) {
        self.classes.clear();
        self.entries.clear();
        self.diagnostics.clear();
    }

    /// Release pathological high-water capacity.
    pub fn shrink_to_fit(&mut self) {
        self.classes.shrink_to_fit();
        self.entries.shrink_to_fit();
        self.diagnostics.shrink_to_fit();
    }

    /// Return the retained capacities, for reuse fixtures.
    #[must_use]
    pub fn capacities(&self) -> IdentityCapacities {
        IdentityCapacities {
            classes: self.classes.capacity(),
            entries: self.entries.capacity(),
            diagnostics: self.diagnostics.capacity(),
        }
    }
}

/// Retained capacity of one workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdentityCapacities {
    /// Retained declaration-class capacity.
    pub classes: usize,
    /// Retained entry-class capacity.
    pub entries: usize,
    /// Retained diagnostic capacity.
    pub diagnostics: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clearing_keeps_capacity_and_drops_state() {
        let mut workspace = IdentityWorkspace::new();
        workspace.classes.reserve(8);
        workspace.classes.push(Some(DeclarationClass::New));
        let before = workspace.capacities();
        workspace.clear();
        assert!(workspace.classes.is_empty());
        assert_eq!(workspace.capacities(), before);
        workspace.shrink_to_fit();
        assert_eq!(workspace.capacities().classes, 0);
    }
}
