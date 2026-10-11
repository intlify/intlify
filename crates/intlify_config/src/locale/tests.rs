// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use super::fixtures::{fixture_binding, fixture_pins, FixtureProvider};
use super::*;
use crate::input_limits::Bound;
use std::cell::Cell;

type PinMutation = fn(&mut SemanticPins<&'static str>);

fn canonicalizer() -> Canonicalizer<FixtureProvider> {
    Canonicalizer::bind(
        &fixture_pins(),
        Some(FixtureProvider::new()),
        Bound::new(128).unwrap(),
    )
    .unwrap()
}

#[test]
fn finite_provider_keeps_canonical_forms_and_corrects_only_declared_aliases() {
    let canonicalizer = canonicalizer();
    for (input, expected) in [
        ("en", "en"),
        ("en-US", "en-US"),
        ("EN-us", "en-US"),
        ("iw-IL", "he-IL"),
        ("en-u-nu-latn-ca-gregory", "en-u-ca-gregory-nu-latn"),
        ("und-u-ca-islamicc", "und-u-ca-islamic-civil"),
        ("zh-Hant-TW", "zh-Hant-TW"),
        ("ar-EG-u-nu-latn", "ar-EG-u-nu-latn"),
    ] {
        let result = canonicalizer.canonicalize(input).unwrap();
        assert_eq!(result.locale().as_str(), expected);
        assert_eq!(
            result.suggested_replacement(),
            (input != expected).then_some(expected)
        );
        let again = canonicalizer
            .canonicalize(result.locale().as_str())
            .unwrap();
        assert_eq!(again.locale(), result.locale());
        assert_eq!(again.suggested_replacement(), None);
    }
    assert_ne!(
        canonicalizer.canonicalize("en").unwrap().locale(),
        canonicalizer.canonicalize("en-US").unwrap().locale()
    );
}

#[test]
fn invalid_inputs_and_inputs_outside_fixture_coverage_are_different() {
    let canonicalizer = canonicalizer();
    for input in [
        "en_US",
        "root",
        "Latn",
        "en_US.UTF-8",
        "en_US@calendar=gregorian",
        "zz",
        "en-u-ca-madeup",
        "en-u-zz-abc",
        "en-x-brand",
        "en-a-foo",
    ] {
        assert_eq!(
            canonicalizer.canonicalize(input).unwrap_err(),
            CanonicalizationFailure::Provider(ProviderFailure::InvalidIdentifier),
            "{input}"
        );
    }
    for input in ["pt-BR", "not-in-this-finite-fixture", ""] {
        assert_eq!(
            canonicalizer.canonicalize(input).unwrap_err(),
            CanonicalizationFailure::Provider(ProviderFailure::UnsupportedInput)
        );
    }
}

#[test]
fn provider_data_is_required_and_every_semantic_pin_is_compared() {
    assert_eq!(
        Canonicalizer::<FixtureProvider>::bind(&fixture_pins(), None, Bound::new(128).unwrap())
            .err()
            .unwrap(),
        BindingFailure::MissingProviderData
    );
    let cases: &[(BindingPart, PinMutation)] = &[
        (BindingPart::SpecificationIdentity, |pins| {
            pins.specification.identity = "other";
        }),
        (BindingPart::SpecificationRevision, |pins| {
            pins.specification.revision = "other";
        }),
        (BindingPart::DatasetIdentity, |pins| {
            pins.dataset.identity = "other";
        }),
        (BindingPart::DatasetDigest, |pins| {
            pins.dataset.semantic_digest = "other";
        }),
    ];
    for (part, change) in cases {
        let mut expected = fixture_pins();
        change(&mut expected);
        assert_eq!(
            Canonicalizer::bind(
                &expected,
                Some(FixtureProvider::new()),
                Bound::new(128).unwrap()
            )
            .err()
            .unwrap(),
            BindingFailure::Mismatch(vec![*part])
        );
    }
}

#[test]
fn admission_facts_are_kept_as_reported_and_never_compared() {
    let mut reported = fixture_binding();
    reported.admission.provider.revision = "1";
    reported.admission.provider_schema.identity = "another-format";
    reported.admission.artifact = "another-artifact-pin";
    let core = Canonicalizer::bind(
        &fixture_pins(),
        Some(ProbeProvider {
            binding: reported.clone(),
            calls: Cell::new(0),
            answer: Ok(Arc::from("en")),
        }),
        Bound::new(128).unwrap(),
    )
    .unwrap();
    assert_eq!(core.binding(), &reported);
    assert_eq!(core.canonicalize("en").unwrap().locale().as_str(), "en");
}

#[test]
fn binding_failures_are_classified_where_015_reports_them() {
    use BindingPart as Part;
    assert_eq!(
        BindingFailure::MissingProviderData.admission(),
        Admission::PreInvocation(EnvelopeReason::MissingInput)
    );
    for parts in [
        vec![Part::SpecificationIdentity],
        vec![Part::SpecificationRevision],
        vec![Part::SpecificationRevision, Part::DatasetDigest],
        vec![Part::DatasetIdentity, Part::SpecificationIdentity],
    ] {
        assert_eq!(
            BindingFailure::Mismatch(parts).admission(),
            Admission::Specification
        );
    }
    for parts in [
        vec![Part::DatasetIdentity],
        vec![Part::DatasetDigest],
        vec![Part::DatasetIdentity, Part::DatasetDigest],
    ] {
        assert_eq!(BindingFailure::Mismatch(parts).admission(), Admission::Data);
    }
    assert_eq!(
        Admission::from(SpecificationUnsupported),
        Admission::Specification
    );
}

fn pins(specification: &'static str, digest: &'static str) -> SemanticPins<&'static str> {
    SemanticPins {
        specification: VersionedIdentity {
            identity: specification,
            revision: "0",
        },
        dataset: DatasetPin {
            identity: "dataset",
            semantic_digest: digest,
        },
    }
}

#[test]
fn a_supported_specification_names_the_one_dataset_it_admits() {
    let supported = Supported::new(vec![pins("first", "a"), pins("second", "b")]).unwrap();
    for (asserted, digest) in [("first", "a"), ("second", "b")] {
        let expected = supported
            .expected(&VersionedIdentity {
                identity: asserted,
                revision: "0",
            })
            .unwrap();
        assert_eq!(expected, &pins(asserted, digest));
    }
    for asserted in [
        VersionedIdentity {
            identity: "third",
            revision: "0",
        },
        VersionedIdentity {
            identity: "first",
            revision: "1",
        },
    ] {
        assert_eq!(
            supported.expected(&asserted).err(),
            Some(SpecificationUnsupported)
        );
    }
    assert!(Supported::<&str>::new(Vec::new())
        .unwrap()
        .expected(&fixture_pins().specification)
        .is_err());
    for duplicate in [
        vec![pins("first", "a"), pins("first", "a")],
        vec![pins("first", "a"), pins("second", "b"), pins("first", "c")],
    ] {
        assert_eq!(
            Supported::new(duplicate).err(),
            Some(DuplicateSpecification)
        );
    }
}

#[test]
fn the_asserted_specification_decides_which_dataset_a_provider_must_carry() {
    let supported = Supported::new(vec![pins("other", "other-digest"), fixture_pins()]).unwrap();
    let bind = |asserted: &VersionedIdentity<&'static str>| {
        let expected = supported.expected(asserted).map_err(Admission::from)?;
        Canonicalizer::bind(
            expected,
            Some(FixtureProvider::new()),
            Bound::new(128).unwrap(),
        )
        .map_err(|failure| failure.admission())
    };
    assert!(bind(&fixture_pins().specification).is_ok());
    assert_eq!(
        bind(&VersionedIdentity {
            identity: "other",
            revision: "0"
        })
        .err(),
        Some(Admission::Specification)
    );
    assert_eq!(
        bind(&VersionedIdentity {
            identity: "unsupported",
            revision: "0"
        })
        .err(),
        Some(Admission::Specification)
    );
    let mut stale = fixture_pins();
    stale.dataset.semantic_digest = "stale-digest";
    let stale_supported = Supported::new(vec![stale]).unwrap();
    let expected = stale_supported
        .expected(&fixture_pins().specification)
        .unwrap();
    let failure = Canonicalizer::bind(
        expected,
        Some(FixtureProvider::new()),
        Bound::new(128).unwrap(),
    )
    .err()
    .unwrap();
    assert_eq!(
        failure,
        BindingFailure::Mismatch(vec![BindingPart::DatasetDigest])
    );
    assert_eq!(failure.admission(), Admission::Data);
}

struct ProbeProvider {
    binding: ProviderBinding<&'static str>,
    calls: Cell<usize>,
    answer: Result<Arc<str>, ProviderFailure>,
}

impl Provider for ProbeProvider {
    type Identity = &'static str;

    fn binding(&self) -> &ProviderBinding<Self::Identity> {
        &self.binding
    }

    fn canonicalize(&self, _: &str) -> Result<Arc<str>, ProviderFailure> {
        self.calls.set(self.calls.get() + 1);
        self.answer.clone()
    }
}

fn probe(answer: Result<Arc<str>, ProviderFailure>, limit: u64) -> Canonicalizer<ProbeProvider> {
    Canonicalizer::bind(
        &fixture_pins(),
        Some(ProbeProvider {
            binding: fixture_binding(),
            calls: Cell::new(0),
            answer,
        }),
        Bound::new(limit).unwrap(),
    )
    .unwrap()
}

#[test]
fn raw_byte_bounds_precede_provider_work_and_count_utf8_bytes() {
    let core = probe(Ok(Arc::from("en")), 2);
    assert_eq!(core.canonicalize("en").unwrap().locale().as_str(), "en");
    assert_eq!(core.provider.calls.get(), 1);
    for input in ["eng", "あ"] {
        assert_eq!(
            core.canonicalize(input).unwrap_err(),
            CanonicalizationFailure::ByteLimit {
                spelling: Spelling::Raw,
                limit: Bound::new(2).unwrap(),
                actual: 3,
            }
        );
        assert_eq!(core.provider.calls.get(), 1);
    }
    let large_bound = probe(Ok(Arc::from("en")), u64::MAX - 1);
    assert_eq!(large_bound.max_identifier_bytes().get(), u64::MAX - 1);
    assert_eq!(
        large_bound.canonicalize("en").unwrap().locale().as_str(),
        "en"
    );
}

#[test]
fn expanding_aliases_recheck_the_complete_canonical_spelling_before_retention() {
    let input = "und-u-ca-islamicc";
    let expected = "und-u-ca-islamic-civil";
    let exact = expected.len() as u64;
    for limit in [input.len() as u64, exact - 1, exact] {
        let core = Canonicalizer::bind(
            &fixture_pins(),
            Some(FixtureProvider::new()),
            Bound::new(limit).unwrap(),
        )
        .unwrap();
        if limit == exact {
            let result = core.canonicalize(input).unwrap();
            assert_eq!(result.locale().as_str(), expected);
            assert_eq!(result.suggested_replacement(), Some(expected));
        } else {
            assert_eq!(
                core.canonicalize(input).unwrap_err(),
                CanonicalizationFailure::ByteLimit {
                    spelling: Spelling::Canonical,
                    limit: Bound::new(limit).unwrap(),
                    actual: exact,
                }
            );
        }
    }
}

#[test]
fn provider_binding_changes_fail_before_any_new_locale_work() {
    let mut core = probe(Ok(Arc::from("en")), 128);
    let retained = core.canonicalize("en").unwrap();
    core.provider.binding.pins.dataset.semantic_digest = "changed-test-pin";
    assert_eq!(
        core.canonicalize("en").unwrap_err(),
        CanonicalizationFailure::ProviderBindingChanged
    );
    assert_eq!(core.provider.calls.get(), 1);
    assert_eq!(core.binding(), &fixture_binding());
    assert_eq!(retained.locale().as_str(), "en");
    // Admission facts never enter semantics, but a provider must not swap the
    // artifact it was admitted with either.
    let mut swapped = probe(Ok(Arc::from("en")), 128);
    swapped.provider.binding.admission.artifact = "another-artifact-pin";
    assert_eq!(
        swapped.canonicalize("en").unwrap_err(),
        CanonicalizationFailure::ProviderBindingChanged
    );
    assert_eq!(swapped.provider.calls.get(), 0);
}

#[test]
fn malformed_provider_output_is_an_adapter_failure_not_a_repaired_locale() {
    for output in ["", "en_US", "en--US", "-en", "en-", "日本語", "en\0"] {
        let core = probe(Ok(Arc::from(output)), 128);
        assert_eq!(
            core.canonicalize("en").unwrap_err(),
            CanonicalizationFailure::ProviderOutputInvariant
        );
    }
    let unavailable = probe(Err(ProviderFailure::Unavailable), 128);
    assert_eq!(
        unavailable.canonicalize("en").unwrap_err(),
        CanonicalizationFailure::Provider(ProviderFailure::Unavailable)
    );
}

#[test]
fn failures_do_not_retain_rejected_authoring_or_mismatching_metadata() {
    let input = "PRIVATE-REJECTED-AUTHORING";
    for failure in [
        ProviderFailure::InvalidIdentifier,
        ProviderFailure::UnsupportedInput,
        ProviderFailure::Unavailable,
    ] {
        let core = probe(Err(failure), 128);
        assert!(!format!("{:?}", core.canonicalize(input).unwrap_err()).contains(input));
    }
    let secret = "PRIVATE-MISMATCHING-METADATA";
    let mut expected = fixture_pins();
    expected.specification.revision = secret;
    expected.dataset.semantic_digest = secret;
    let failure = Canonicalizer::bind(
        &expected,
        Some(FixtureProvider::new()),
        Bound::new(128).unwrap(),
    )
    .err()
    .unwrap();
    assert_eq!(
        failure,
        BindingFailure::Mismatch(vec![
            BindingPart::SpecificationRevision,
            BindingPart::DatasetDigest,
        ])
    );
    assert!(!format!("{failure:?}").contains(secret));
}

#[test]
fn retained_results_share_immutable_storage_and_outlive_the_provider() {
    let core = canonicalizer();
    let alias = core.canonicalize("iw-IL").unwrap().into_locale();
    let canonical = core.canonicalize("he-IL").unwrap().into_locale();
    assert!(Arc::ptr_eq(&alias.0, &canonical.0));
    let result = core.canonicalize("EN-us").unwrap();
    drop(core);
    assert_eq!(alias, canonical);
    assert_eq!(alias.as_str(), "he-IL");
    assert_eq!(result.locale().as_str(), "en-US");
    assert_eq!(result.suggested_replacement(), Some("en-US"));
}

#[test]
fn fresh_and_reused_providers_preserve_results_after_failures_and_source_changes() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<FixtureProvider>();
    send_sync::<Canonicalizer<FixtureProvider>>();
    send_sync::<CanonicalLocale>();
    let reused = canonicalizer();
    for _ in 0..3 {
        for input in ["en-US", "en_US", "pt-BR", "EN-us", "iw-IL", "en"] {
            assert_eq!(
                reused.canonicalize(input),
                canonicalizer().canonicalize(input)
            );
        }
    }
    let from_alias = reused.canonicalize("EN-us").unwrap();
    let canonical = reused.canonicalize("en-US").unwrap();
    assert_eq!(from_alias.locale(), canonical.locale());
    assert_ne!(
        from_alias.suggested_replacement(),
        canonical.suggested_replacement()
    );
}

#[test]
fn canonical_locale_order_is_unsigned_byte_order_and_keeps_prefixes() {
    let core = canonicalizer();
    let mut values: Vec<_> = ["ja", "en-u-ca-gregory-nu-latn", "en-US", "de", "en"]
        .into_iter()
        .map(|input| core.canonicalize(input).unwrap().into_locale())
        .collect();
    values.sort();
    assert_eq!(
        values
            .iter()
            .map(CanonicalLocale::as_str)
            .collect::<Vec<_>>(),
        ["de", "en", "en-US", "en-u-ca-gregory-nu-latn", "ja"]
    );
}
