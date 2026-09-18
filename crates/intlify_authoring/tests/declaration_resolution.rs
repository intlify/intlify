// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Context resolution and declaration facts, through the public API.
//!
//! The cases here follow design 016's resolution rules: an explicit annotation
//! wins, otherwise the context default applies, otherwise the declaration is
//! blocked. Nothing substitutes a requested locale, a host locale, or a
//! language inferred from the text, because a wrong source locale
//! mistranslates silently instead of failing.

use intlify_authoring::test_context::{LocaleRule, TestContext};
use intlify_authoring::{
    resolve_declarations, AnalysisWorkspace, AuthoringContext, AuthoringFailure, AuthoringLimits,
    ByteRange, ContextKind, DeclarationInput, DeclarationMetadata, MessageInput, Occurrence,
    OccurrenceRole, Outcome, OwnerIdentity, OwnerKind, ParameterBinding, ReasonFamily,
    SourceLocaleBasis, SourceSnapshot, SurfaceVocabulary, VersionedIdentity,
};

fn limits() -> AuthoringLimits {
    AuthoringLimits {
        declarations: 1024,
        message_text_bytes: 64 * 1024,
        emitted_mf2_bytes: 128 * 1024,
        extraction_segments: 4096,
        parameter_names: 64,
        parameter_name_bytes: 256,
        metadata_value_bytes: 4096,
        vocabulary_members: 256,
        projection_nodes: 4096,
        projection_depth: 32,
        diagnostics: 256,
    }
    .validate()
    .expect("satisfiable bounds")
}

fn owner() -> OwnerIdentity {
    OwnerIdentity::new(OwnerKind::Application, "storefront").expect("checked project id")
}

fn occurrence_at(start: u64, role: OccurrenceRole) -> Occurrence {
    let source = SourceSnapshot::new(
        owner(),
        "checkout",
        "1",
        VersionedIdentity::literal("intlify-fixture-grammar", "0"),
        4096,
        &format!("sha256:{}", "0".repeat(64)),
    )
    .expect("checked snapshot");
    Occurrence::new(source, ByteRange::new(start, start + 8).unwrap(), role)
        .expect("range inside the snapshot")
}

fn occurrence() -> Occurrence {
    occurrence_at(0, OccurrenceRole::UiLiteral)
}

/// A context with a default locale, a two-member vocabulary, and finite rules.
fn context() -> TestContext {
    TestContext::builder(
        owner(),
        SurfaceVocabulary::new(["checkout", "nav"]).unwrap(),
    )
    .default_source_locale("en")
    .default_surface_class("checkout")
    .rule(LocaleRule::canonical("EN-us", "en-US"))
    .rule(LocaleRule::canonical("ja", "ja"))
    .rule(LocaleRule::invalid("en_US"))
    .build()
    .expect("checked test context")
}

fn declaration(message: MessageInput<'_>) -> DeclarationInput<'_> {
    DeclarationInput {
        occurrence: occurrence(),
        message,
        metadata: DeclarationMetadata::default(),
        usage: None,
        parameters: &[],
    }
}

fn resolve(
    context: &dyn AuthoringContext,
    inputs: &[DeclarationInput<'_>],
) -> Result<intlify_authoring::AuthoringResult, AuthoringFailure> {
    let mut workspace = AnalysisWorkspace::new();
    resolve_declarations(context, inputs, &limits(), &mut workspace)
}

fn resolve_within(
    context: &dyn AuthoringContext,
    inputs: &[DeclarationInput<'_>],
    limits: &AuthoringLimits,
) -> Result<intlify_authoring::AuthoringResult, AuthoringFailure> {
    let mut workspace = AnalysisWorkspace::new();
    resolve_declarations(context, inputs, limits, &mut workspace)
}

fn reasons(result: &intlify_authoring::AuthoringResult) -> Vec<&str> {
    result
        .diagnostics()
        .iter()
        .map(|record| record.origin().code())
        .collect()
}

#[test]
fn an_explicit_locale_is_canonicalized_and_wins_over_the_default() {
    let context = context();
    let mut input = declaration(MessageInput::Literal("Pay now"));
    input.metadata.source_locale = Some("EN-us");

    let result = resolve(&context, &[input]).unwrap();
    let facts = result.checked().expect("a complete result");
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].source_locale_basis(), SourceLocaleBasis::Explicit);
    assert_eq!(
        facts[0].projection().source_locale.as_str(),
        "en-US",
        "the canonical form is recorded, not the authored spelling"
    );
}

#[test]
fn the_context_default_applies_only_when_no_explicit_locale_exists() {
    let context = context();
    let result = resolve(&context, &[declaration(MessageInput::Literal("Pay now"))]).unwrap();
    let facts = result.checked().expect("a complete result");
    assert_eq!(
        facts[0].source_locale_basis(),
        SourceLocaleBasis::ContextDefault
    );
    assert_eq!(facts[0].projection().source_locale.as_str(), "en");
}

#[test]
fn a_declaration_without_any_locale_basis_is_blocked() {
    let without_default =
        TestContext::builder(owner(), SurfaceVocabulary::new(["checkout"]).unwrap())
            .default_surface_class("checkout")
            .build()
            .unwrap();
    let result = resolve(
        &without_default,
        &[declaration(MessageInput::Literal("Pay now"))],
    )
    .unwrap();

    assert_eq!(result.outcome(), Outcome::Blocked);
    assert!(
        result.checked().is_none(),
        "a blocked scope has no complete result"
    );
    assert!(
        reasons(&result).contains(&ReasonFamily::AuthoringSourceLocaleMissing.as_str()),
        "{:?}",
        reasons(&result)
    );
}

#[test]
fn an_invalid_locale_blocks_but_an_unavailable_provider_is_operational() {
    let context = context();
    let mut invalid = declaration(MessageInput::Literal("Pay now"));
    invalid.metadata.source_locale = Some("en_US");
    let result = resolve(&context, &[invalid]).unwrap();
    assert_eq!(result.outcome(), Outcome::Blocked);
    assert!(reasons(&result).contains(&ReasonFamily::AuthoringSourceLocaleInvalid.as_str()));

    // An identifier outside the fixture's coverage is also an authoring-level
    // failure, not a claim that the provider broke.
    let mut uncovered = declaration(MessageInput::Literal("Pay now"));
    uncovered.metadata.source_locale = Some("fr-CA");
    let result = resolve(&context, &[uncovered]).unwrap();
    assert!(reasons(&result).contains(&ReasonFamily::AuthoringSourceLocaleInvalid.as_str()));

    // A provider that cannot answer is operational: the author wrote nothing
    // wrong, so it must never surface as an authoring diagnostic.
    let offline = TestContext::builder(owner(), SurfaceVocabulary::new(["checkout"]).unwrap())
        .default_surface_class("checkout")
        .rule(LocaleRule::canonical("EN-us", "en-US"))
        .unavailable()
        .build()
        .unwrap();
    let mut input = declaration(MessageInput::Literal("Pay now"));
    input.metadata.source_locale = Some("EN-us");
    assert_eq!(
        resolve(&offline, &[input]),
        Err(AuthoringFailure::LocaleProviderUnavailable)
    );
}

#[test]
fn surface_class_falls_back_to_the_default_and_must_be_a_vocabulary_member() {
    let context = context();
    // The invocation default applies when nothing is annotated.
    let result = resolve(&context, &[declaration(MessageInput::Literal("Pay now"))]).unwrap();
    assert_eq!(result.checked().unwrap()[0].surface_class(), "checkout");

    // An explicit assignment wins over the default.
    let mut explicit = declaration(MessageInput::Literal("Pay now"));
    explicit.metadata.surface_class = Some("nav");
    let result = resolve(&context, &[explicit]).unwrap();
    assert_eq!(result.checked().unwrap()[0].surface_class(), "nav");

    // A class outside the exact vocabulary blocks: it is never invented.
    let mut unknown = declaration(MessageInput::Literal("Pay now"));
    unknown.metadata.surface_class = Some("billing");
    let result = resolve(&context, &[unknown]).unwrap();
    assert_eq!(result.outcome(), Outcome::Blocked);
    assert!(reasons(&result).contains(&ReasonFamily::AuthoringSurfaceClassInvalid.as_str()));

    // With neither an annotation nor a default, the declaration is blocked.
    let without_default =
        TestContext::builder(owner(), SurfaceVocabulary::new(["checkout"]).unwrap())
            .default_source_locale("en")
            .build()
            .unwrap();
    let result = resolve(
        &without_default,
        &[declaration(MessageInput::Literal("Pay now"))],
    )
    .unwrap();
    assert!(reasons(&result).contains(&ReasonFamily::AuthoringSurfaceClassInvalid.as_str()));
}

#[test]
fn parameter_mismatches_are_reported_and_a_match_is_accepted() {
    let context = context();
    let expression = occurrence_at(100, OccurrenceRole::ParameterExpression);

    let matched = [ParameterBinding::new("name", expression.clone())];
    let mut input = declaration(MessageInput::Mf2("Hello {$name}"));
    input.parameters = &matched;
    assert_eq!(
        resolve(&context, &[input]).unwrap().outcome(),
        Outcome::Checked
    );

    // Missing: the message requires a name the host did not supply.
    let input = declaration(MessageInput::Mf2("Hello {$name}"));
    let result = resolve(&context, &[input]).unwrap();
    assert_eq!(result.outcome(), Outcome::Blocked);
    assert!(reasons(&result).contains(&ReasonFamily::AuthoringParameterMismatch.as_str()));

    // Extra: the host supplied a name the message does not use.
    let extra = [
        ParameterBinding::new("name", expression.clone()),
        ParameterBinding::new("other", expression.clone()),
    ];
    let mut input = declaration(MessageInput::Mf2("Hello {$name}"));
    input.parameters = &extra;
    let result = resolve(&context, &[input]).unwrap();
    assert_eq!(result.outcome(), Outcome::Blocked);

    // Duplicate: the same name appears twice.
    let duplicated = [
        ParameterBinding::new("name", expression.clone()),
        ParameterBinding::new("name", expression),
    ];
    let mut input = declaration(MessageInput::Mf2("Hello {$name}"));
    input.parameters = &duplicated;
    let result = resolve(&context, &[input]).unwrap();
    assert_eq!(result.outcome(), Outcome::Blocked);
}

#[test]
fn a_blocked_declaration_does_not_suppress_facts_about_the_others() {
    let context = context();
    let good = DeclarationInput {
        occurrence: occurrence_at(0, OccurrenceRole::UiLiteral),
        ..declaration(MessageInput::Literal("Pay now"))
    };
    let mut bad = DeclarationInput {
        occurrence: occurrence_at(50, OccurrenceRole::IntentLiteral),
        ..declaration(MessageInput::Literal("Cancel"))
    };
    bad.metadata.surface_class = Some("billing");

    let result = resolve(&context, &[good, bad]).unwrap();
    assert_eq!(result.outcome(), Outcome::Blocked);
    assert!(result.checked().is_none());
    assert_eq!(
        result.inspection_facts().len(),
        1,
        "the unaffected declaration still produced facts for inspection"
    );
}

#[test]
fn a_description_participates_and_absence_stays_absence() {
    let context = context();
    let mut described = declaration(MessageInput::Literal("Pay now"));
    described.metadata.description = Some("Primary payment action");
    let result = resolve(&context, &[described]).unwrap();
    let facts = result.checked().unwrap();
    assert_eq!(
        facts[0]
            .projection()
            .description
            .as_ref()
            .map(intlify_authoring::NonemptyText::as_str),
        Some("Primary payment action")
    );

    let plain = resolve(&context, &[declaration(MessageInput::Literal("Pay now"))]).unwrap();
    assert!(
        plain.checked().unwrap()[0]
            .projection()
            .description
            .is_none(),
        "there is no shared description default"
    );

    // An empty description is a mistake rather than an absent one.
    let mut empty = declaration(MessageInput::Literal("Pay now"));
    empty.metadata.description = Some("");
    let result = resolve(&context, &[empty]).unwrap();
    assert_eq!(result.outcome(), Outcome::Blocked);
    assert!(reasons(&result).contains(&ReasonFamily::AuthoringMetadataInvalid.as_str()));
}

#[test]
fn usage_is_admitted_only_under_a_registered_profile() {
    let without_profile = context();
    let mut input = declaration(MessageInput::Literal("Pay now"));
    input.usage = Some("button-label");
    let result = resolve(&without_profile, &[input]).unwrap();
    assert_eq!(result.outcome(), Outcome::Blocked);
    assert!(reasons(&result).contains(&ReasonFamily::AuthoringMetadataInvalid.as_str()));

    let with_profile = TestContext::builder(owner(), SurfaceVocabulary::new(["checkout"]).unwrap())
        .default_source_locale("en")
        .default_surface_class("checkout")
        .usage_profile(VersionedIdentity::literal("intlify-usage-test", "0"))
        .build()
        .unwrap();
    let mut input = declaration(MessageInput::Literal("Pay now"));
    input.usage = Some("button-label");
    let result = resolve(&with_profile, &[input]).unwrap();
    let facts = result.checked().expect("a complete result");
    assert_eq!(
        facts[0]
            .projection()
            .usage
            .as_ref()
            .map(|usage| usage.value.as_str()),
        Some("button-label")
    );
}

#[test]
fn facts_are_ordered_canonically_and_duplicate_occurrences_are_rejected() {
    let context = context();
    let later = DeclarationInput {
        occurrence: occurrence_at(100, OccurrenceRole::UiLiteral),
        ..declaration(MessageInput::Literal("Later"))
    };
    let earlier = DeclarationInput {
        occurrence: occurrence_at(10, OccurrenceRole::UiLiteral),
        ..declaration(MessageInput::Literal("Earlier"))
    };
    let result = resolve(&context, &[later, earlier]).unwrap();
    let facts = result.checked().expect("a complete result");
    assert_eq!(
        facts
            .iter()
            .map(|fact| fact.occurrence().range().start())
            .collect::<Vec<_>>(),
        [10, 100],
        "results are canonically ordered, not input ordered"
    );

    let first = declaration(MessageInput::Literal("One"));
    let second = declaration(MessageInput::Literal("Two"));
    assert_eq!(
        resolve(&context, &[first, second]),
        Err(AuthoringFailure::DuplicateOccurrence)
    );
}

#[test]
fn a_reference_occurrence_is_not_a_declaration() {
    let context = context();
    let input = DeclarationInput {
        occurrence: occurrence_at(0, OccurrenceRole::Reference),
        ..declaration(MessageInput::Literal("Pay now"))
    };
    let result = resolve(&context, &[input]).unwrap();
    assert_eq!(result.outcome(), Outcome::Blocked);
    assert!(reasons(&result).contains(&ReasonFamily::AuthoringInputInvalid.as_str()));
}

/// A context that copies a test context but claims a production kind.
struct Relabelled(intlify_authoring::AuthoringBasis, TestContext);

impl AuthoringContext for Relabelled {
    fn basis(&self) -> &intlify_authoring::AuthoringBasis {
        &self.0
    }
    fn owner(&self) -> &OwnerIdentity {
        self.1.owner()
    }
    fn default_source_locale(&self) -> Option<&intlify_authoring::CanonicalLocale> {
        self.1.default_source_locale()
    }
    fn surface_vocabulary(&self) -> &SurfaceVocabulary {
        self.1.surface_vocabulary()
    }
    fn canonicalize(
        &self,
        identifier: &str,
    ) -> Result<intlify_authoring::CanonicalLocale, intlify_authoring::LocaleFailure> {
        self.1.canonicalize(identifier)
    }
    fn usage_profile(&self) -> Option<&VersionedIdentity> {
        self.1.usage_profile()
    }
}

#[test]
fn relabelling_a_test_context_as_production_does_not_admit_it() {
    // The builder always pins a test context, so the only way to claim a
    // production kind is to construct a basis directly. That claim must be
    // rejected: a spelling is not checked production evidence.
    let context = context();
    assert_eq!(context.basis().context_kind(), ContextKind::TestContext);

    let basis = intlify_authoring::AuthoringBasis::new(
        VersionedIdentity::literal("intlify-authoring-phase1-test", "0"),
        ContextKind::ApplicationProfile,
        intlify_authoring::ExactInputBinding::new(
            "storefront",
            "0",
            &format!("sha256:{}", "0".repeat(64)),
        )
        .unwrap(),
        intlify_authoring::ExactInputBinding::new(
            "vocabulary",
            "0",
            &format!("sha256:{}", "0".repeat(64)),
        )
        .unwrap(),
        VersionedIdentity::literal("canonicalization", "0"),
        intlify_authoring::LocaleData::new("data", &format!("sha256:{}", "0".repeat(64))).unwrap(),
        None,
    );
    let relabelled = Relabelled(basis, context);
    assert_eq!(
        resolve(
            &relabelled,
            &[declaration(MessageInput::Literal("Pay now"))]
        ),
        Err(AuthoringFailure::ProductionContextUnsupported(
            ContextKind::ApplicationProfile
        ))
    );
}

#[test]
fn source_owned_by_another_owner_is_outside_this_invocation() {
    // The invocation resolves one owner's declarations. Source belonging to a
    // different owner is not merely unusual: admitting it would attribute the
    // message to the wrong application, which no later stage can detect.
    let context = context();
    let other = SourceSnapshot::new(
        OwnerIdentity::new(OwnerKind::Library, "storefront").expect("checked project id"),
        "checkout",
        "1",
        VersionedIdentity::literal("intlify-fixture-grammar", "0"),
        4096,
        &format!("sha256:{}", "0".repeat(64)),
    )
    .expect("checked snapshot");
    let input = DeclarationInput {
        occurrence: Occurrence::new(
            other,
            ByteRange::new(0, 8).unwrap(),
            OccurrenceRole::UiLiteral,
        )
        .expect("range inside the snapshot"),
        ..declaration(MessageInput::Literal("Pay now"))
    };

    let result = resolve(&context, &[input]).unwrap();
    assert_eq!(result.outcome(), Outcome::Blocked);
    assert!(result.checked().is_none());
    assert!(result.inspection_facts().is_empty());
    assert_eq!(
        reasons(&result),
        [ReasonFamily::AuthoringInputInvalid.as_str()]
    );

    // The owner-local identity alone is equal here, so the rejection is the
    // complete owner pair rather than the identity token.
    let same_owner = declaration(MessageInput::Literal("Pay now"));
    assert_eq!(
        resolve(&context, &[same_owner]).unwrap().outcome(),
        Outcome::Checked
    );
}

#[test]
fn the_diagnostics_budget_bounds_what_is_collected_not_only_what_is_returned() {
    // One declaration reports a diagnostic per unusable parameter, so a caller
    // supplying many would otherwise fill an unbounded vector before the limit
    // was consulted. The failure must arrive from a bounded collector.
    let context = context();
    let parameters = (0..64)
        .map(|index| ParameterBinding::new(&format!("extra{index}"), occurrence()))
        .collect::<Vec<_>>();
    let input = DeclarationInput {
        parameters: &parameters,
        ..declaration(MessageInput::Literal("Pay now"))
    };
    let mut limits = limits();
    limits.diagnostics = 4;
    let limits = limits.validate().expect("satisfiable bounds");

    assert_eq!(
        resolve_within(&context, &[input], &limits).unwrap_err(),
        AuthoringFailure::Limit(intlify_authoring::LimitKind::Diagnostics)
    );

    // Below the budget the same shape still reports every diagnostic.
    let few = parameters[..2].to_vec();
    let input = DeclarationInput {
        parameters: &few,
        ..declaration(MessageInput::Literal("Pay now"))
    };
    let result = resolve_within(&context, &[input], &limits).unwrap();
    assert_eq!(result.outcome(), Outcome::Blocked);
    assert_eq!(result.diagnostics().len(), 2);
}
