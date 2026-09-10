// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use super::fixtures::{fixture_binding, FixtureProvider};
use super::*;
use crate::input_limits::Bound;
use std::cell::Cell;

type BindingMutation = fn(&mut ProviderBinding<&'static str>);

fn canonicalizer() -> Canonicalizer<FixtureProvider> {
    Canonicalizer::bind(
        &fixture_binding(),
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
fn provider_data_is_required_and_every_binding_part_is_pinned() {
    assert_eq!(
        Canonicalizer::<FixtureProvider>::bind(&fixture_binding(), None, Bound::new(128).unwrap())
            .err()
            .unwrap(),
        BindingFailure::MissingProviderData
    );
    let original = fixture_binding();
    let cases: &[(BindingPart, BindingMutation)] = &[
        (BindingPart::SpecificationIdentity, |binding| {
            binding.specification.identity = "other";
        }),
        (BindingPart::SpecificationRevision, |binding| {
            binding.specification.revision = "other";
        }),
        (BindingPart::SpecificationDigest, |binding| {
            binding.specification.digest = "other";
        }),
        (BindingPart::DatasetIdentity, |binding| {
            binding.dataset.identity = "other";
        }),
        (BindingPart::DatasetRevision, |binding| {
            binding.dataset.revision = "other";
        }),
        (BindingPart::DatasetDigest, |binding| {
            binding.dataset.digest = "other";
        }),
        (BindingPart::ProviderIdentity, |binding| {
            binding.provider.identity = "other";
        }),
        (BindingPart::ProviderRevision, |binding| {
            binding.provider.revision = "other";
        }),
        (BindingPart::ProviderSchemaIdentity, |binding| {
            binding.provider_schema.identity = "other";
        }),
        (BindingPart::ProviderSchemaRevision, |binding| {
            binding.provider_schema.revision = "other";
        }),
        (BindingPart::TransportDigest, |binding| {
            binding.transport_digest = "other";
        }),
    ];
    for (part, change) in cases {
        let mut expected = original.clone();
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
        &fixture_binding(),
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
            &fixture_binding(),
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
    core.provider.binding.dataset.digest = "changed-test-pin";
    assert_eq!(
        core.canonicalize("en").unwrap_err(),
        CanonicalizationFailure::ProviderBindingChanged
    );
    assert_eq!(core.provider.calls.get(), 1);
    assert_eq!(core.binding(), &fixture_binding());
    assert_eq!(retained.locale().as_str(), "en");
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
    let mut expected = fixture_binding();
    expected.specification.digest = secret;
    expected.dataset.revision = secret;
    expected.transport_digest = secret;
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
            BindingPart::SpecificationDigest,
            BindingPart::DatasetRevision,
            BindingPart::TransportDigest,
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
