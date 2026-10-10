// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The closed set of registry powers design 018 grants.
//!
//! Each action is one power and implies no other. Holding `update-registry`
//! does not let a caller initialize a registry or confirm an explicit
//! identity choice, and no action reaches beyond the registry. The spellings
//! name logical powers, not commands or configuration fields.

/// One registry power.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Action {
    /// Admit the scoped source inputs and derive authoring facts from them.
    AnalyzeSource,
    /// Admit the scoped registry, current or explicitly pinned, and replay
    /// its history.
    ReadRegistry,
    /// Publish one checked empty genesis for an admitted new owner binding.
    InitializeRegistry,
    /// Publish one complete checked update against the exact current base.
    UpdateRegistry,
    /// Confirm one exact explicit identity choice for this owner and input
    /// scope.
    ResolveIdentity,
}

impl Action {
    /// Every action, in the order design 018 lists them.
    pub const ALL: [Self; 5] = [
        Self::AnalyzeSource,
        Self::ReadRegistry,
        Self::InitializeRegistry,
        Self::UpdateRegistry,
        Self::ResolveIdentity,
    ];

    /// Return the exact spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AnalyzeSource => "analyze-source",
            Self::ReadRegistry => "read-registry",
            Self::InitializeRegistry => "initialize-registry",
            Self::UpdateRegistry => "update-registry",
            Self::ResolveIdentity => "resolve-identity",
        }
    }

    /// Find an action by its exact spelling. An unknown spelling names no
    /// power; it is never read as the nearest one.
    #[must_use]
    pub fn from_wire(spelling: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|action| action.as_str() == spelling)
    }

    const fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// The actions one principal holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ActionSet(u8);

impl ActionSet {
    /// Return whether the set holds an action.
    #[must_use]
    pub const fn contains(self, action: Action) -> bool {
        self.0 & action.bit() != 0
    }

    /// Add an action, returning whether it was absent.
    #[cfg_attr(
        not(any(test, feature = "test-authority")),
        expect(
            dead_code,
            reason = "only the test-owned entry establishes an authority in this phase"
        )
    )]
    pub(crate) const fn insert(&mut self, action: Action) -> bool {
        let absent = !self.contains(action);
        self.0 |= action.bit();
        absent
    }

    /// Iterate over the actions held, in the order design 018 lists them.
    pub fn iter(self) -> impl Iterator<Item = Action> {
        Action::ALL
            .into_iter()
            .filter(move |action| self.contains(*action))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_action_has_the_spelling_design_018_gives_it() {
        assert_eq!(
            Action::ALL.map(Action::as_str),
            [
                "analyze-source",
                "read-registry",
                "initialize-registry",
                "update-registry",
                "resolve-identity",
            ]
        );
    }

    #[test]
    fn every_action_is_found_only_by_its_exact_spelling() {
        for action in Action::ALL {
            assert_eq!(Action::from_wire(action.as_str()), Some(action));
        }
        // An unknown power, a near miss and a wildcard name nothing.
        for spelling in [
            "publish-release",
            "Update-Registry",
            "update-registry ",
            "*",
            "",
        ] {
            assert_eq!(Action::from_wire(spelling), None, "{spelling:?}");
        }
    }

    #[test]
    fn a_set_holds_exactly_what_was_inserted() {
        let mut set = ActionSet::default();
        assert_eq!(set.iter().count(), 0);
        assert!(set.insert(Action::UpdateRegistry));
        assert!(set.insert(Action::AnalyzeSource));
        // A repeated action is reported, so a grant entry can refuse it.
        assert!(!set.insert(Action::UpdateRegistry));
        for action in Action::ALL {
            let held = matches!(action, Action::AnalyzeSource | Action::UpdateRegistry);
            assert_eq!(set.contains(action), held, "{action:?}");
        }
        assert_eq!(
            set.iter().collect::<Vec<_>>(),
            [Action::AnalyzeSource, Action::UpdateRegistry]
        );
    }
}
