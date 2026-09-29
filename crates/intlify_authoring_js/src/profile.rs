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
//! The caller registers the module exports that are intrinsics. A profile with
//! none registered recognizes no explicit form, which is the right answer for
//! a project that does not use them, not a misconfiguration. No DOM global is
//! admitted yet, so nothing is recognized automatically.

use intlify_authoring::VersionedIdentity;

use crate::binding::{BindingError, Bindings, IntrinsicBinding};

/// Identity of the authoring profile this Producer implements.
pub const AUTHORING_PROFILE_IDENTITY: &str = "intlify-js-dom-authoring";

/// Revision of the authoring profile this Producer implements.
pub const AUTHORING_PROFILE_REVISION: &str = "0";

/// The authoring profile one invocation reads source under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsAuthoringProfile {
    identity: VersionedIdentity,
    bindings: Bindings,
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
}
