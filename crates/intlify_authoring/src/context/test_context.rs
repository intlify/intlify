// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! An explicitly test-owned authoring context.
//!
//! This module is compiled only under the non-default `test-context` feature,
//! so an ordinary build has no way to construct a context at all. That is the
//! point: 016 permits test-only inputs for the first minimum, and the isolation
//! is what keeps a fixture from being presented later as checked production
//! evidence.
//!
//! Test-only describes how the inputs were established, not what runs. Source
//! recognition, MF2 parsing, parameter checks, context resolution, and the
//! projection all execute the real operations against these values.

use intlify_shared_json::token::VersionedIdentity;

use super::{
    AuthoringBasis, AuthoringContext, CanonicalLocale, ContextKind, LocaleData, LocaleFailure,
    SurfaceVocabulary,
};
use crate::primitives::{ExactInputBinding, NonemptyText, OwnerIdentity, PrimitiveError};

/// One finite canonicalization rule the fixture declares.
///
/// Coverage is explicit. An identifier outside these rules is reported as
/// unsupported rather than as invalid, because a fixture's silence is not
/// evidence that a locale is malformed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocaleRule {
    input: String,
    canonical: Option<String>,
}

impl LocaleRule {
    /// Declare that `input` canonicalizes to `canonical`.
    #[must_use]
    pub fn canonical(input: &str, canonical: &str) -> Self {
        Self {
            input: input.to_owned(),
            canonical: Some(canonical.to_owned()),
        }
    }

    /// Declare that `input` is not a well-formed identifier.
    #[must_use]
    pub fn invalid(input: &str) -> Self {
        Self {
            input: input.to_owned(),
            canonical: None,
        }
    }
}

/// A finite, explicitly declared authoring context for tests.
#[derive(Debug, Clone)]
pub struct TestContext {
    basis: AuthoringBasis,
    owner: OwnerIdentity,
    default_source_locale: Option<CanonicalLocale>,
    vocabulary: SurfaceVocabulary,
    rules: Box<[LocaleRule]>,
    usage_profile: Option<VersionedIdentity>,
    available: bool,
}

/// Builder for a test context, so every input is named at its call site.
#[derive(Debug, Clone)]
pub struct TestContextBuilder {
    owner: OwnerIdentity,
    default_source_locale: Option<String>,
    vocabulary: SurfaceVocabulary,
    default_surface_class: Option<String>,
    rules: Vec<LocaleRule>,
    usage_profile: Option<VersionedIdentity>,
    available: bool,
}

impl TestContext {
    /// Start describing a context for one owner and vocabulary.
    #[must_use]
    pub fn builder(owner: OwnerIdentity, vocabulary: SurfaceVocabulary) -> TestContextBuilder {
        TestContextBuilder {
            owner,
            default_source_locale: None,
            vocabulary,
            default_surface_class: None,
            rules: Vec::new(),
            usage_profile: None,
            available: true,
        }
    }
}

impl TestContextBuilder {
    /// Supply the canonical default source locale.
    ///
    /// The value is taken as already canonical, matching 015, where the default
    /// is resolved before authoring sees it.
    #[must_use]
    pub fn default_source_locale(mut self, canonical: &str) -> Self {
        self.default_source_locale = Some(canonical.to_owned());
        self
    }

    /// Supply the invocation-wide default surface class.
    #[must_use]
    pub fn default_surface_class(mut self, class: &str) -> Self {
        self.default_surface_class = Some(class.to_owned());
        self
    }

    /// Declare one finite canonicalization rule.
    #[must_use]
    pub fn rule(mut self, rule: LocaleRule) -> Self {
        self.rules.push(rule);
        self
    }

    /// Register the semantic-usage profile this context admits.
    #[must_use]
    pub fn usage_profile(mut self, profile: VersionedIdentity) -> Self {
        self.usage_profile = Some(profile);
        self
    }

    /// Make the canonicalization provider report itself unavailable.
    ///
    /// This exists so a fixture can exercise the operational path without
    /// pretending an authoring mistake occurred.
    #[must_use]
    pub const fn unavailable(mut self) -> Self {
        self.available = false;
        self
    }

    /// Build the context, pinning it as a test context.
    pub fn build(self) -> Result<TestContext, PrimitiveError> {
        let digest = format!("sha256:{}", "0".repeat(64));
        let default_surface_class = self
            .default_surface_class
            .as_deref()
            .map(NonemptyText::from_validated)
            .transpose()?;
        let basis = AuthoringBasis::new(
            VersionedIdentity::literal("intlify-authoring-phase1-test", "0"),
            ContextKind::TestContext,
            ExactInputBinding::new("intlify-authoring-test-context", "0", &digest)?,
            ExactInputBinding::new("intlify-authoring-test-vocabulary", "0", &digest)?,
            VersionedIdentity::literal("intlify-authoring-test-canonicalization", "0"),
            LocaleData::new("intlify-authoring-test-locale-data", &digest)?,
            default_surface_class,
        );
        Ok(TestContext {
            basis,
            owner: self.owner,
            default_source_locale: self.default_source_locale.map(CanonicalLocale),
            vocabulary: self.vocabulary,
            rules: self.rules.into_boxed_slice(),
            usage_profile: self.usage_profile,
            available: self.available,
        })
    }
}

impl AuthoringContext for TestContext {
    fn basis(&self) -> &AuthoringBasis {
        &self.basis
    }

    fn owner(&self) -> &OwnerIdentity {
        &self.owner
    }

    fn default_source_locale(&self) -> Option<&CanonicalLocale> {
        self.default_source_locale.as_ref()
    }

    fn surface_vocabulary(&self) -> &SurfaceVocabulary {
        &self.vocabulary
    }

    fn canonicalize(&self, identifier: &str) -> Result<CanonicalLocale, LocaleFailure> {
        if !self.available {
            return Err(LocaleFailure::Unavailable);
        }
        for rule in &self.rules {
            if rule.input == identifier {
                return rule
                    .canonical
                    .as_ref()
                    .map(|canonical| CanonicalLocale(canonical.clone()))
                    .ok_or(LocaleFailure::InvalidIdentifier);
            }
        }
        // Outside the declared coverage. The fixture does not claim that every
        // unlisted identifier is malformed, only that it cannot answer.
        Err(LocaleFailure::UnsupportedInput)
    }

    fn usage_profile(&self) -> Option<&VersionedIdentity> {
        self.usage_profile.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::OwnerKind;

    fn context() -> TestContext {
        TestContext::builder(
            OwnerIdentity::new(OwnerKind::Application, "storefront").unwrap(),
            SurfaceVocabulary::new(["checkout"]).unwrap(),
        )
        .default_source_locale("en")
        .rule(LocaleRule::canonical("EN-us", "en-US"))
        .rule(LocaleRule::canonical("en-US", "en-US"))
        .rule(LocaleRule::invalid("en_US"))
        .build()
        .unwrap()
    }

    #[test]
    fn the_provider_separates_invalid_unsupported_and_unavailable() {
        let context = context();
        assert_eq!(context.canonicalize("EN-us").unwrap().as_str(), "en-US");
        // Canonicalization is idempotent over the declared coverage.
        assert_eq!(context.canonicalize("en-US").unwrap().as_str(), "en-US");
        // An explicitly declared malformed identifier.
        assert_eq!(
            context.canonicalize("en_US"),
            Err(LocaleFailure::InvalidIdentifier)
        );
        // Outside the fixture's coverage: not a claim that it is malformed.
        assert_eq!(
            context.canonicalize("fr-CA"),
            Err(LocaleFailure::UnsupportedInput)
        );

        let offline = TestContext::builder(
            OwnerIdentity::new(OwnerKind::Application, "storefront").unwrap(),
            SurfaceVocabulary::new(["checkout"]).unwrap(),
        )
        .rule(LocaleRule::canonical("EN-us", "en-US"))
        .unavailable()
        .build()
        .unwrap();
        assert_eq!(
            offline.canonicalize("EN-us"),
            Err(LocaleFailure::Unavailable)
        );
    }

    #[test]
    fn a_built_context_is_always_pinned_as_a_test_context() {
        let context = context();
        assert_eq!(context.basis().context_kind(), ContextKind::TestContext);
        assert_eq!(context.default_source_locale().unwrap().as_str(), "en");
        assert!(context.surface_vocabulary().admits("checkout"));
        assert_eq!(context.basis().default_surface_class(), None);
        assert!(context.usage_profile().is_none());
    }

    #[test]
    fn absent_inputs_stay_absent_rather_than_acquiring_a_hidden_default() {
        let bare = TestContext::builder(
            OwnerIdentity::new(OwnerKind::Application, "storefront").unwrap(),
            SurfaceVocabulary::new([]).unwrap(),
        )
        .build()
        .unwrap();
        assert!(bare.default_source_locale().is_none());
        assert!(bare.surface_vocabulary().members().is_empty());
        assert_eq!(bare.basis().default_surface_class(), None);
        // With no rules declared, nothing is claimed about any identifier.
        assert_eq!(
            bare.canonicalize("en"),
            Err(LocaleFailure::UnsupportedInput)
        );
    }
}
