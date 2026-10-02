// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The authoring profile this Producer implements.
//!
//! A profile decides which host syntax counts as authoring: which imports are
//! intrinsics, which receivers are UI, which comments carry metadata. A context
//! pins one by identity and revision, and a Producer implementing a different
//! one refuses to run, rather than reading the source by its own rules while
//! the result carries the context's pin.
//!
//! The caller registers the module exports that are intrinsics, and admits
//! the DOM globals whose calls start a receiver. A profile with no intrinsic
//! recognizes no explicit form, and one that admits no DOM global recognizes
//! nothing automatically. Both are the right answer for a project that does
//! not use them, not a misconfiguration.
//!
//! Text recognized automatically carries a semantic usage from a closed
//! profile, so a context reading such a unit has to register that profile.

use intlify_authoring::VersionedIdentity;

use crate::binding::{BindingError, Bindings, IntrinsicBinding};

/// Identity of the authoring profile this Producer implements.
pub const AUTHORING_PROFILE_IDENTITY: &str = "intlify-js-dom-authoring";

/// Revision of the authoring profile this Producer implements.
pub const AUTHORING_PROFILE_REVISION: &str = "0";

/// Identity of the usage profile automatic recognition assigns from.
pub const USAGE_PROFILE_IDENTITY: &str = "intlify-web-dom-usage";

/// Revision of the usage profile automatic recognition assigns from.
pub const USAGE_PROFILE_REVISION: &str = "0";

/// The usage of text assigned to a proven `textContent` sink.
///
/// It is the only value the usage profile has. Explicit forms take no usage
/// from where they are written, so a message's revision never depends on
/// whether the receiver around it could be proven.
pub const TEXT_CONTENT_USAGE: &str = "text-content";

/// A host global whose calls can start a proven DOM receiver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DomGlobal {
    /// The standard `document`, whose `querySelector` and `createElement`
    /// calls with one static string are the admitted receiver origins.
    Document,
}

/// The authoring profile one invocation reads source under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsAuthoringProfile {
    identity: VersionedIdentity,
    bindings: Bindings,
    dom: Box<[DomGlobal]>,
}

impl JsAuthoringProfile {
    /// Return the profile this Producer implements, with no intrinsic
    /// registered.
    #[must_use]
    pub fn new() -> Self {
        Self {
            identity: VersionedIdentity::literal(
                AUTHORING_PROFILE_IDENTITY,
                AUTHORING_PROFILE_REVISION,
            ),
            bindings: Bindings::default(),
            dom: Box::default(),
        }
    }

    /// Register the module exports that are authoring intrinsics.
    ///
    /// The set replaces any registered before. A set that registers an empty
    /// module or export, one export twice, or one export as two intrinsics is
    /// refused, because a unit read under it would mean different things
    /// depending on which entry won.
    pub fn with_bindings(
        mut self,
        bindings: impl IntoIterator<Item = IntrinsicBinding>,
    ) -> Result<Self, BindingError> {
        self.bindings = Bindings::new(bindings)?;
        Ok(self)
    }

    /// Admit the DOM globals whose calls start a receiver.
    ///
    /// The set replaces any admitted before. A global is admitted only as
    /// itself: a local binding of the same name is still not it.
    #[must_use]
    pub fn with_dom_globals(mut self, globals: impl IntoIterator<Item = DomGlobal>) -> Self {
        let mut globals: Vec<DomGlobal> = globals.into_iter().collect();
        globals.sort_unstable();
        globals.dedup();
        self.dom = globals.into_boxed_slice();
        self
    }

    /// Borrow the admitted DOM globals, in order.
    #[must_use]
    pub fn dom_globals(&self) -> &[DomGlobal] {
        &self.dom
    }

    /// Return the usage profile pin a context reading automatic recognition
    /// has to name.
    #[must_use]
    pub fn usage_profile() -> VersionedIdentity {
        VersionedIdentity::literal(USAGE_PROFILE_IDENTITY, USAGE_PROFILE_REVISION)
    }

    pub(crate) fn admits_document(&self) -> bool {
        self.dom.contains(&DomGlobal::Document)
    }

    /// Borrow the exact pin a context has to name.
    #[must_use]
    pub const fn identity(&self) -> &VersionedIdentity {
        &self.identity
    }

    /// Borrow the registered intrinsics, ordered by module and export.
    #[must_use]
    pub fn bindings(&self) -> &[IntrinsicBinding] {
        self.bindings.entries()
    }

    pub(crate) const fn binding_set(&self) -> &Bindings {
        &self.bindings
    }
}

impl Default for JsAuthoringProfile {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::Intrinsic;

    #[test]
    fn the_pin_is_the_exact_registered_pair() {
        let profile = JsAuthoringProfile::new();
        assert_eq!(
            profile.identity().identity().as_str(),
            "intlify-js-dom-authoring"
        );
        assert_eq!(profile.identity().revision().as_str(), "0");
        assert!(profile.bindings().is_empty());
    }

    #[test]
    fn registering_intrinsics_keeps_the_pin_and_refuses_a_contradiction() {
        let profile = JsAuthoringProfile::new()
            .with_bindings([IntrinsicBinding::new(
                "fixture-authoring",
                "intent",
                Intrinsic::Intent,
            )])
            .unwrap();
        assert_eq!(profile.identity(), JsAuthoringProfile::new().identity());
        assert_eq!(profile.bindings().len(), 1);
        assert_eq!(
            JsAuthoringProfile::new().with_bindings([
                IntrinsicBinding::new("fixture-authoring", "intent", Intrinsic::Intent),
                IntrinsicBinding::new("fixture-authoring", "intent", Intrinsic::NoIntent),
            ]),
            Err(BindingError::Conflict {
                module: "fixture-authoring".into(),
                export: "intent".into()
            })
        );
    }

    #[test]
    fn a_new_set_replaces_the_last_and_is_kept_in_order() {
        let profile = JsAuthoringProfile::default()
            .with_bindings([IntrinsicBinding::new(
                "fixture-authoring",
                "intent",
                Intrinsic::Intent,
            )])
            .unwrap()
            .with_bindings([
                IntrinsicBinding::new("fixture-authoring", "noIntent", Intrinsic::NoIntent),
                IntrinsicBinding::new("fixture-authoring", "mf2", Intrinsic::Mf2),
            ])
            .unwrap();
        let exports: Vec<&str> = profile
            .bindings()
            .iter()
            .map(IntrinsicBinding::export)
            .collect();
        assert_eq!(exports, ["mf2", "noIntent"]);
        // The recognizers read the same set the caller sees.
        assert_eq!(profile.binding_set().entries(), profile.bindings());
        assert_eq!(
            profile.binding_set().find("fixture-authoring", "intent"),
            None
        );
        assert_eq!(JsAuthoringProfile::default(), JsAuthoringProfile::new());
    }

    #[test]
    fn dom_globals_are_a_set_and_the_usage_pin_is_fixed() {
        let profile = JsAuthoringProfile::new();
        assert!(profile.dom_globals().is_empty());
        assert!(!profile.admits_document());
        let profile = profile.with_dom_globals([DomGlobal::Document, DomGlobal::Document]);
        assert_eq!(profile.dom_globals(), [DomGlobal::Document]);
        assert!(profile.admits_document());
        assert_eq!(profile.identity(), JsAuthoringProfile::new().identity());
        let usage = JsAuthoringProfile::usage_profile();
        assert_eq!(usage.identity().as_str(), "intlify-web-dom-usage");
        assert_eq!(usage.revision().as_str(), "0");
    }
}
