// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The checked inputs one authoring invocation resolves against.
//!
//! A basis records exact pins: which authoring profile, which project context,
//! which surface vocabulary, and which locale canonicalization produced the
//! values used. A pin is not the input body and cannot stand in for a missing
//! checked input, so the caller supplies the actual values through a context
//! and this crate checks declarations against them.
//!
//! Phase 1 admits only an explicitly test-owned context. Production admission
//! needs the checked 015 inputs and the 017/018 representation and authority
//! work that later phases own, so relabelling a test context as a production
//! one is rejected rather than quietly accepted.

#[cfg(feature = "test-context")]
pub mod test_context;

use intlify_shared_json::token::{Token, VersionedIdentity};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::primitives::{ExactInputBinding, NonemptyText, OwnerIdentity, PrimitiveError};

/// Which kind of owning input a basis was resolved against.
///
/// The three kinds are not interchangeable. A test context exercises the
/// declared subset; it never becomes checked production evidence by being
/// spelled differently.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum ContextKind {
    ApplicationProfile,
    LibraryContext,
    TestContext,
}

impl ContextKind {
    /// Return the exact wire spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ApplicationProfile => "application-profile",
            Self::LibraryContext => "library-context",
            Self::TestContext => "test-context",
        }
    }
}

/// The locale data a canonicalization was performed against.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocaleData {
    identity: Token,
    semantic_digest: crate::primitives::SemanticDigest,
}

impl LocaleData {
    /// Validate and retain one locale dataset pin.
    pub fn new(identity: &str, semantic_digest: &str) -> Result<Self, PrimitiveError> {
        Ok(Self {
            identity: Token::new(identity)?,
            semantic_digest: serde_json::from_value(serde_json::Value::String(
                semantic_digest.to_owned(),
            ))
            .map_err(|_| PrimitiveError::InvalidToken)?,
        })
    }

    /// Borrow the dataset identity.
    #[must_use]
    pub const fn identity(&self) -> &Token {
        &self.identity
    }
}

// The exact dependency pins one invocation resolved against. This is evidence
// about which inputs were used, not the inputs themselves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthoringBasis {
    authoring_profile: VersionedIdentity,
    context_kind: ContextKind,
    context: ExactInputBinding,
    surface_vocabulary: ExactInputBinding,
    locale_canonicalization: VersionedIdentity,
    locale_data: LocaleData,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    default_surface_class: Option<NonemptyText>,
}

impl AuthoringBasis {
    /// Retain one basis.
    #[must_use]
    pub const fn new(
        authoring_profile: VersionedIdentity,
        context_kind: ContextKind,
        context: ExactInputBinding,
        surface_vocabulary: ExactInputBinding,
        locale_canonicalization: VersionedIdentity,
        locale_data: LocaleData,
        default_surface_class: Option<NonemptyText>,
    ) -> Self {
        Self {
            authoring_profile,
            context_kind,
            context,
            surface_vocabulary,
            locale_canonicalization,
            locale_data,
            default_surface_class,
        }
    }

    /// Return which kind of owning input this basis names.
    #[must_use]
    pub const fn context_kind(&self) -> ContextKind {
        self.context_kind
    }

    /// Borrow the pinned authoring profile.
    #[must_use]
    pub const fn authoring_profile(&self) -> &VersionedIdentity {
        &self.authoring_profile
    }

    /// Borrow the pinned owning context.
    #[must_use]
    pub const fn context(&self) -> &ExactInputBinding {
        &self.context
    }

    /// Borrow the pinned surface-class vocabulary.
    #[must_use]
    pub const fn surface_vocabulary(&self) -> &ExactInputBinding {
        &self.surface_vocabulary
    }

    /// Return the invocation-wide default surface class, when one was supplied.
    #[must_use]
    pub fn default_surface_class(&self) -> Option<&str> {
        self.default_surface_class
            .as_ref()
            .map(NonemptyText::as_str)
    }
}

/// One canonical locale, as established by an admitted canonicalization.
///
/// This type exists only to keep a raw authoring spelling from being mistaken
/// for a resolved locale. It can be produced only by a provider, so a caller
/// cannot assert a canonical form by writing the string.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CanonicalLocale(String);

impl CanonicalLocale {
    /// Borrow the exact canonical spelling.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Complete failure of a locale canonicalization.
///
/// The three cases stay separate because they call for different responses.
/// An invalid tag is an authoring mistake; an unsupported one is a gap in the
/// admitted data; an unavailable provider is an operational problem and must
/// never be reported as if the author wrote something wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocaleFailure {
    /// The supplied text is not a well-formed locale identifier.
    InvalidIdentifier,
    /// The identifier is well formed but outside the admitted data.
    UnsupportedInput,
    /// The provider could not answer.
    Unavailable,
}

/// The exact surface classes an invocation may assign.
///
/// Membership is explicit. A class is never invented from a file path, a
/// component name, a DOM tag, or a nearby word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceVocabulary {
    members: Box<[String]>,
}

impl SurfaceVocabulary {
    /// Retain one exact vocabulary, rejecting duplicates and empty members.
    pub fn new<'a>(members: impl IntoIterator<Item = &'a str>) -> Result<Self, PrimitiveError> {
        let mut retained: Vec<String> = Vec::new();
        for member in members {
            if member.is_empty() {
                return Err(PrimitiveError::EmptyText);
            }
            if retained.iter().any(|existing| existing == member) {
                return Err(PrimitiveError::InvalidToken);
            }
            retained.push(member.to_owned());
        }
        Ok(Self {
            members: retained.into_boxed_slice(),
        })
    }

    /// Return whether one exact class is a member.
    #[must_use]
    pub fn admits(&self, class: &str) -> bool {
        self.members.iter().any(|member| member == class)
    }

    /// Return the admitted members in their supplied order.
    #[must_use]
    pub fn members(&self) -> &[String] {
        &self.members
    }
}

/// The checked inputs a declaration is resolved against.
///
/// An implementation supplies the actual values behind the basis pins. This
/// crate never acquires them: there is no filesystem, network, ambient locale,
/// or configuration lookup anywhere below this boundary.
pub trait AuthoringContext {
    /// Return the exact pins this context was built from.
    fn basis(&self) -> &AuthoringBasis;

    /// Return the owning application or library.
    fn owner(&self) -> &OwnerIdentity;

    /// Return the canonical default source locale, when the context has one.
    ///
    /// Absence is meaningful: a declaration without an explicit locale is then
    /// blocked rather than falling back to a requested or host locale.
    fn default_source_locale(&self) -> Option<&CanonicalLocale>;

    /// Return the exact surface-class vocabulary.
    fn surface_vocabulary(&self) -> &SurfaceVocabulary;

    /// Canonicalize one explicitly authored locale identifier.
    fn canonicalize(&self, identifier: &str) -> Result<CanonicalLocale, LocaleFailure>;

    /// Return the semantic-usage profile this context admits, if any.
    ///
    /// Usage is only accepted under a registered profile, so a caller cannot
    /// attach arbitrary meaning to a declaration.
    fn usage_profile(&self) -> Option<&VersionedIdentity>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_kinds_have_distinct_stable_spellings() {
        let kinds = [
            ContextKind::ApplicationProfile,
            ContextKind::LibraryContext,
            ContextKind::TestContext,
        ];
        for kind in kinds {
            assert_eq!(
                serde_json::to_value(kind).unwrap(),
                serde_json::json!(kind.as_str())
            );
        }
        assert_ne!(ContextKind::TestContext, ContextKind::ApplicationProfile);
    }

    #[test]
    fn a_vocabulary_is_exact_and_rejects_duplicates_and_empty_members() {
        let vocabulary = SurfaceVocabulary::new(["checkout", "nav"]).unwrap();
        assert!(vocabulary.admits("checkout"));
        assert!(
            !vocabulary.admits("Checkout"),
            "membership is case sensitive"
        );
        assert!(!vocabulary.admits("checkout "), "membership is exact");
        assert!(!vocabulary.admits("billing"));
        assert_eq!(vocabulary.members(), ["checkout", "nav"]);

        assert_eq!(
            SurfaceVocabulary::new(["checkout", "checkout"]),
            Err(PrimitiveError::InvalidToken)
        );
        assert_eq!(SurfaceVocabulary::new([""]), Err(PrimitiveError::EmptyText));
        assert!(SurfaceVocabulary::new([]).unwrap().members().is_empty());
    }

    #[test]
    fn locale_failures_stay_separable() {
        // Each case calls for a different response, so they must not collapse.
        assert_ne!(
            LocaleFailure::InvalidIdentifier,
            LocaleFailure::UnsupportedInput
        );
        assert_ne!(LocaleFailure::UnsupportedInput, LocaleFailure::Unavailable);
    }
}
