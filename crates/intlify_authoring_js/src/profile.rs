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
//! This profile configures no intrinsic binding and admits no DOM global, so
//! no syntax in a unit is an authoring form and a checked unit has no
//! declarations. That is the complete answer for this configuration, and it is
//! the answer a project using neither explicit forms nor automatic recognition
//! should get.

use intlify_authoring::VersionedIdentity;

/// Identity of the authoring profile this Producer implements.
pub const AUTHORING_PROFILE_IDENTITY: &str = "intlify-js-dom-authoring";

/// Revision of the authoring profile this Producer implements.
pub const AUTHORING_PROFILE_REVISION: &str = "0";

/// The authoring profile one invocation reads source under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsAuthoringProfile {
    identity: VersionedIdentity,
}

impl JsAuthoringProfile {
    /// Return the profile this Producer implements.
    #[must_use]
    pub fn new() -> Self {
        Self {
            identity: VersionedIdentity::literal(
                AUTHORING_PROFILE_IDENTITY,
                AUTHORING_PROFILE_REVISION,
            ),
        }
    }

    /// Borrow the exact pin a context has to name.
    #[must_use]
    pub const fn identity(&self) -> &VersionedIdentity {
        &self.identity
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

    #[test]
    fn the_pin_is_the_exact_registered_pair() {
        let profile = JsAuthoringProfile::new();
        assert_eq!(
            profile.identity().identity().as_str(),
            "intlify-js-dom-authoring"
        );
        assert_eq!(profile.identity().revision().as_str(), "0");
    }
}
