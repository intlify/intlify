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
    compare_parameters, intent_revision, resolve_declarations,
    resolve_declarations_with_cancellation, AnalysisWorkspace, AuthoringContext, AuthoringFailure,
    AuthoringLimits, ByteRange, ContextKind, DeclarationInput, DeclarationMetadata, Detail,
    InputSegment, MappingError, MessageInput, MessageRange, Occurrence, OccurrenceRole, Outcome,
    OwnerIdentity, OwnerKind, ParameterBinding, ReasonFamily, SourceLocaleBasis, SourceSnapshot,
    SurfaceVocabulary, VersionedIdentity,
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
        input_map: None,
        metadata: DeclarationMetadata::default(),
        usage: None,
        parameters: Some(&[]),
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

/// The causes reported, which is what separates two records of one family.
fn details(result: &intlify_authoring::AuthoringResult) -> Vec<&str> {
    result
        .diagnostics()
        .iter()
        .map(|record| record.detail().map_or("", Detail::as_str))
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
    input.parameters = Some(&matched);
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
    input.parameters = Some(&extra);
    let result = resolve(&context, &[input]).unwrap();
    assert_eq!(result.outcome(), Outcome::Blocked);

    // Duplicate: the same name appears twice.
    let duplicated = [
        ParameterBinding::new("name", expression.clone()),
        ParameterBinding::new("name", expression),
    ];
    let mut input = declaration(MessageInput::Mf2("Hello {$name}"));
    input.parameters = Some(&duplicated);
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
        parameters: Some(&parameters),
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
        parameters: Some(&few),
        ..declaration(MessageInput::Literal("Pay now"))
    };
    let result = resolve_within(&context, &[input], &limits).unwrap();
    assert_eq!(result.outcome(), Outcome::Blocked);
    assert_eq!(result.diagnostics().len(), 2);
}

#[test]
fn the_same_canonical_locale_reached_two_ways_has_one_revision() {
    // 016 keeps which basis established the locale as evidence, not as
    // meaning. A message whose source locale is the same either way must not
    // get two revisions because of how the locale was reached, or every
    // translation would be revisited when a default is introduced.
    let context = TestContext::builder(owner(), SurfaceVocabulary::new(["checkout"]).unwrap())
        .default_source_locale("en-US")
        .default_surface_class("checkout")
        .rule(LocaleRule::canonical("EN-us", "en-US"))
        .build()
        .expect("checked test context");

    let inherited = declaration(MessageInput::Literal("Pay now"));
    let explicit = DeclarationInput {
        metadata: DeclarationMetadata {
            source_locale: Some("EN-us"),
            ..DeclarationMetadata::default()
        },
        ..declaration(MessageInput::Literal("Pay now"))
    };

    let first = resolve(&context, &[inherited]).unwrap();
    let second = resolve(&context, &[explicit]).unwrap();
    let inherited = &first.checked().unwrap()[0];
    let explicit = &second.checked().unwrap()[0];

    assert_eq!(
        inherited.source_locale_basis(),
        SourceLocaleBasis::ContextDefault
    );
    assert_eq!(explicit.source_locale_basis(), SourceLocaleBasis::Explicit);
    assert_eq!(
        intent_revision(inherited.projection()).unwrap(),
        intent_revision(explicit.projection()).unwrap(),
        "the basis is evidence, not meaning"
    );
}

#[test]
fn a_metadata_value_past_its_bound_blocks_rather_than_being_truncated() {
    // A description that does not fit is not a shorter description. Silently
    // trimming one would change what a translator is told about the message.
    let context = context();
    let mut limits = limits();
    limits.metadata_value_bytes = 8;
    let limits = limits.validate().expect("satisfiable bounds");

    let described = |description: &'static str| DeclarationInput {
        metadata: DeclarationMetadata {
            description: Some(description),
            surface_class: Some("checkout"),
            ..DeclarationMetadata::default()
        },
        ..declaration(MessageInput::Literal("Pay now"))
    };

    // The bound is the last accepted length, not the first rejected one, so
    // the two witnesses either side of it are what fixes where it sits.
    let exact = "exactly8";
    assert_eq!(exact.len() as u64, limits.metadata_value_bytes);
    let result = resolve_within(&context, &[described(exact)], &limits).unwrap();
    assert_eq!(result.outcome(), Outcome::Checked);

    let first_over = "exactly89";
    assert_eq!(first_over.len() as u64, limits.metadata_value_bytes + 1);
    let result = resolve_within(&context, &[described(first_over)], &limits).unwrap();
    assert_eq!(result.outcome(), Outcome::Blocked);
    assert!(result.checked().is_none());
    assert!(reasons(&result).contains(&ReasonFamily::AuthoringMetadataInvalid.as_str()));

    // The bound counts bytes, not characters: one multi-byte scalar can put a
    // shorter-looking description past it.
    let multibyte = "日本語";
    assert_eq!(multibyte.chars().count(), 3);
    assert_eq!(multibyte.len() as u64, limits.metadata_value_bytes + 1);
    let result = resolve_within(&context, &[described(multibyte)], &limits).unwrap();
    assert_eq!(result.outcome(), Outcome::Blocked);
}

#[test]
fn a_host_map_moves_the_extraction_map_into_source_and_omitting_one_does_not() {
    // `'a{b'` sitting at bytes 0..=7 of the unit, its content at [1, 4).
    let map = [InputSegment::new(
        ByteRange::new(0, 3).unwrap(),
        ByteRange::new(1, 4).unwrap(),
    )];
    let mut input = declaration(MessageInput::Literal("a{b"));
    input.input_map = Some(&map);
    let result = resolve(&context(), &[input]).expect("a complete invocation");
    let facts = result.checked().expect("a checked result");
    let composed: Vec<(u64, u64)> = facts[0]
        .extraction_map()
        .iter()
        .map(|piece| (piece.source().start(), piece.source().end()))
        .collect();
    assert_eq!(
        composed,
        [(1, 1), (1, 2), (2, 3), (3, 4), (4, 4)],
        "the escaped brace resolves to the one source byte it escaped"
    );

    // Without a map the same analysis reports the only coordinates it has.
    let plain = resolve(&context(), &[declaration(MessageInput::Literal("a{b"))])
        .expect("a complete invocation");
    let decoded: Vec<(u64, u64)> = plain.checked().expect("a checked result")[0]
        .extraction_map()
        .iter()
        .map(|piece| (piece.source().start(), piece.source().end()))
        .collect();
    assert_eq!(decoded, [(0, 0), (0, 1), (1, 2), (2, 3), (3, 3)]);
}

#[test]
fn a_host_map_that_does_not_describe_the_text_fails_the_invocation() {
    // No edit to the analyzed source could fix this, so it is not reported to
    // an author as something to correct.
    let map = [InputSegment::new(
        ByteRange::new(0, 2).unwrap(),
        ByteRange::new(1, 3).unwrap(),
    )];
    let mut input = declaration(MessageInput::Literal("abc"));
    input.input_map = Some(&map);
    assert_eq!(
        resolve(&context(), &[input]),
        Err(AuthoringFailure::InputMap(MappingError::Coverage))
    );
}

#[test]
fn a_bound_exhausted_while_composing_reports_as_the_bound_it_is() {
    // Composing splits one emitted run at each host boundary, so this map needs
    // five segments where the encoder alone needed three. The bound must arrive
    // as that bound: a host matching on the failure to name which limit it hit
    // would otherwise have to know whether it happened to supply a map.
    let map = [
        InputSegment::new(ByteRange::new(0, 1).unwrap(), ByteRange::new(1, 2).unwrap()),
        InputSegment::new(ByteRange::new(1, 2).unwrap(), ByteRange::new(2, 3).unwrap()),
        InputSegment::new(ByteRange::new(2, 3).unwrap(), ByteRange::new(3, 4).unwrap()),
    ];
    let mut input = declaration(MessageInput::Literal("abc"));
    input.input_map = Some(&map);
    let mut limits = limits();
    limits.extraction_segments = 4;
    let limits = limits.validate().expect("satisfiable bounds");
    assert_eq!(
        resolve_within(&context(), &[input], &limits).unwrap_err(),
        AuthoringFailure::Limit(intlify_authoring::LimitKind::ExtractionSegments)
    );
}

#[test]
fn the_three_parameter_causes_report_in_one_fixed_order() {
    // All three share a stage, a location and a reason family, so the cause is
    // what orders them. Pinning it keeps a later rename from silently
    // reshuffling a report.
    let expression = occurrence_at(16, OccurrenceRole::ParameterExpression);
    let supplied = [
        ParameterBinding::new("other", expression.clone()),
        ParameterBinding::new("other", expression),
    ];
    let mut input = declaration(MessageInput::Mf2("Hello {$name}!"));
    input.parameters = Some(&supplied);

    let result = resolve(&context(), &[input]).expect("a complete invocation");
    assert_eq!(result.outcome(), Outcome::Blocked);
    assert_eq!(
        details(&result),
        [
            "parameter-duplicate",
            "parameter-extra",
            "parameter-missing"
        ]
    );
}

#[test]
fn a_broken_host_map_fails_even_when_the_declaration_is_blocked_anyway() {
    // A map that does not describe the text is the host's mistake whatever the
    // author wrote. Checking it only on the way out would report an
    // integration bug for a clean declaration and stay silent for a blocked
    // one, so whether a host heard about its own bug would depend on what the
    // author happened to type.
    let short = [InputSegment::new(
        ByteRange::new(0, 2).unwrap(),
        ByteRange::new(1, 3).unwrap(),
    )];
    let mut input = declaration(MessageInput::Literal("a\u{0}b"));
    input.input_map = Some(&short);
    assert_eq!(
        resolve(&context(), &[input]),
        Err(AuthoringFailure::InputMap(MappingError::Coverage)),
        "the host bug is reported, not the blocked declaration it hid behind"
    );

    // Without the broken map the same text is a blocked declaration, which is
    // what makes the case above about the map rather than about the text.
    let blocked = resolve(&context(), &[declaration(MessageInput::Literal("a\u{0}b"))])
        .expect("an invocation that ran");
    assert_eq!(blocked.outcome(), Outcome::Blocked);
}

#[test]
fn a_declaration_without_a_use_site_does_not_owe_parameters() {
    // A reusable declaration is written before anything references it. Reading
    // that absence as an empty parameter object would block every message with
    // a parameter at the moment it is declared.
    let mut standalone = declaration(MessageInput::Mf2("Hello {$name}!"));
    standalone.parameters = None;
    let result = resolve(&context(), &[standalone]).expect("a complete invocation");
    assert_eq!(result.outcome(), Outcome::Checked);
    assert!(result.diagnostics().is_empty());

    // A use site that supplied nothing is a different fact and still reports.
    let mut supplied = declaration(MessageInput::Mf2("Hello {$name}!"));
    supplied.parameters = Some(&[]);
    let blocked = resolve(&context(), &[supplied]).expect("a complete invocation");
    assert_eq!(blocked.outcome(), Outcome::Blocked);
    assert_eq!(details(&blocked), ["parameter-missing"]);
}

#[test]
fn a_parameter_mismatch_names_which_of_the_three_it_is() {
    let expression = occurrence_at(16, OccurrenceRole::ParameterExpression);
    let supplied = [
        ParameterBinding::new("name", expression.clone()),
        ParameterBinding::new("name", expression.clone()),
        ParameterBinding::new("count", expression.clone()),
    ];
    let required = ["name".to_owned(), "total".to_owned()];
    let mut reported = Vec::new();
    let matched = compare_parameters(&required, &supplied, &expression, &mut |record| {
        reported.push(record);
    });
    assert!(
        !matched,
        "the use site does not match what the message requires"
    );
    let causes: Vec<&str> = reported
        .iter()
        .map(|record| record.detail().map_or("", Detail::as_str))
        .collect();
    assert_eq!(
        causes,
        [
            "parameter-duplicate",
            "parameter-extra",
            "parameter-missing"
        ],
        "three mistakes with three different fixes stay three records"
    );

    // A reference reports at the reference, not at the declaration it shares
    // with every other use of the same message.
    for record in &reported {
        assert_eq!(record.occurrence(), Some(&expression));
    }
    assert!(reported
        .iter()
        .all(|record| record.origin().code() == "authoring-parameter-mismatch"));
}

#[test]
fn displayed_text_no_message_can_carry_is_reported_where_an_author_can_fix_it() {
    // The encoder alone cannot know whether a caller has source to point at,
    // so it reports the position; a caller that does turns it into a record.
    let result = resolve(&context(), &[declaration(MessageInput::Literal("a\u{0}b"))])
        .expect("an invocation that ran");
    assert_eq!(result.outcome(), Outcome::Blocked);
    assert_eq!(reasons(&result), ["authoring-form-unsupported"]);
    assert_eq!(details(&result), ["unrepresentable-scalar"]);
    assert_eq!(
        result.diagnostics()[0].message_range(),
        Some(MessageRange::Supplied(ByteRange::new(1, 2).unwrap())),
        "the position names the one character to remove, in the text supplied"
    );
    assert!(
        result.checked().is_none(),
        "a blocked declaration is not a checked scope"
    );
}

#[test]
fn a_cancelled_invocation_returns_no_facts_at_all() {
    let inputs = [
        declaration(MessageInput::Literal("first")),
        declaration(MessageInput::Literal("second")),
    ];
    // Distinct positions, so the two would otherwise both resolve.
    let mut inputs = inputs.to_vec();
    inputs[1].occurrence = occurrence_at(16, OccurrenceRole::UiLiteral);

    let mut workspace = AnalysisWorkspace::new();
    let cancelled = std::cell::Cell::new(false);
    let probe = || {
        let asked = cancelled.get();
        cancelled.set(true);
        asked
    };
    assert_eq!(
        resolve_declarations_with_cancellation(
            &context(),
            &inputs,
            &limits(),
            &mut workspace,
            &probe
        ),
        Err(AuthoringFailure::Cancelled),
        "stopping yields no partial scope, so nothing can be read as an absence"
    );

    // The same workspace serves a later run as a fresh one would.
    let again = resolve_declarations(&context(), &inputs, &limits(), &mut workspace)
        .expect("a complete invocation");
    assert_eq!(again.checked().expect("a checked result").len(), 2);
}

#[test]
fn a_host_profile_replaces_the_language_neutral_pin() {
    let neutral = context();
    assert_eq!(
        neutral.basis().authoring_profile().identity().as_str(),
        "intlify-authoring-phase1-test"
    );

    // A Producer's facts have to say which rules recognized the syntax, or a
    // host result would claim it came from an analysis that reads no host.
    let hosted = TestContext::builder(owner(), SurfaceVocabulary::new(["checkout"]).unwrap())
        .authoring_profile(VersionedIdentity::literal("intlify-js-dom-authoring", "0"))
        .default_source_locale("en")
        .default_surface_class("checkout")
        .build()
        .expect("checked test context");
    assert_eq!(
        hosted.basis().authoring_profile().identity().as_str(),
        "intlify-js-dom-authoring"
    );
    assert_eq!(hosted.basis().context_kind(), ContextKind::TestContext);
}
