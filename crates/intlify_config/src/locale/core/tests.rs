// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::sync::Arc;

use serde_json::{json, Value};

use crate::fixtures::{
    minimal_config, FixtureConfig, FixturePolicyReference, FixtureTargetReference,
};
use crate::input_limits::Bound;
use crate::locale::fixtures::{fixture_binding, FixtureProvider};
use crate::locale::Canonicalizer;
use crate::materialize::materialize_file;
use crate::model::ProfileId;
use crate::structural::{AuthoringSchema, StructuralAnalysis, StructuralLimits};

use super::*;

fn analysis(value: &Value) -> StructuralAnalysis<FixturePolicyReference, FixtureTargetReference> {
    let document = materialize_file(
        Arc::from(serde_json::to_vec(value).unwrap()),
        crate::materialize_tests::limits(),
    )
    .unwrap();
    AuthoringSchema::<FixturePolicyReference, FixtureTargetReference>::for_model()
        .unwrap()
        .analyze(
            Arc::new(document),
            StructuralLimits {
                max_profiles: Bound::new(64).unwrap(),
                max_profile_id_bytes: Bound::new(256).unwrap(),
                max_structural_analysis_units: Bound::new(1_000_000).unwrap(),
            },
        )
        .unwrap()
}

fn config(value: &Value) -> FixtureConfig {
    analysis(value)
        .construct()
        .unwrap()
        .expect("complete root required")
}

fn provider() -> Canonicalizer<FixtureProvider> {
    Canonicalizer::bind(
        &fixture_binding(),
        Some(FixtureProvider::new()),
        Bound::new(128).unwrap(),
    )
    .unwrap()
}

fn limits(occurrences: u64, requested: u64) -> Limits {
    Limits {
        max_active_occurrences: Bound::new(occurrences).unwrap(),
        max_requested_locales: Bound::new(requested).unwrap(),
    }
}

fn resolve(value: &Value, limits: Limits) -> Resolution {
    let config = config(value);
    let id: ProfileId = serde_json::from_value(json!("app")).unwrap();
    Input::from_selected(&config, &id)
        .unwrap()
        .resolve(&provider(), limits)
}

#[test]
fn minimal_core_preserves_absence_without_inventing_a_source_default() {
    let resolution = resolve(&minimal_config(), limits(16, 8));
    let core = resolution.value().unwrap();
    assert!(core.source_default().is_none());
    assert_eq!(
        core.requested()
            .iter()
            .map(CanonicalLocale::as_str)
            .collect::<Vec<_>>(),
        ["en"]
    );
    assert_eq!(core.requested_default().as_str(), "en");
    assert!(resolution.corrections().is_empty());
    assert_eq!(resolution.counts().active_occurrences, Some(2));
    assert_eq!(resolution.counts().canonical_requested, Some(1));
}

#[test]
fn canonical_order_and_defaults_are_independent_of_source_order_and_semantic_role() {
    let mut value = minimal_config();
    let app = &mut value["profiles"]["app"];
    app["requestedLocales"] = json!(["ja", "EN-us", "en"]);
    app["defaultRequestedLocale"] = json!("EN-us");
    app["defaultSourceLocale"] = json!("fr");
    let resolution = resolve(&value, limits(16, 8));
    let core = resolution.value().unwrap();
    assert_eq!(core.source_default().unwrap().as_str(), "fr");
    assert_eq!(
        core.requested()
            .iter()
            .map(CanonicalLocale::as_str)
            .collect::<Vec<_>>(),
        ["en", "en-US", "ja"]
    );
    assert_eq!(core.requested_default().as_str(), "en-US");
    assert_eq!(
        resolution
            .corrections()
            .iter()
            .map(|correction| correction.location)
            .collect::<Vec<_>>(),
        [Location::Requested(1), Location::RequestedDefault]
    );
    assert_eq!(resolution.counts().active_occurrences, Some(5));
}

#[test]
fn duplicate_canonical_identities_relate_every_occurrence_and_never_return_a_core() {
    let mut value = minimal_config();
    value["profiles"]["app"]["requestedLocales"] = json!(["EN-us", "en-US", "EN-us", "ja"]);
    value["profiles"]["app"]["defaultRequestedLocale"] = json!("en-US");
    let resolution = resolve(&value, limits(16, 8));
    let Failure::Issues(issues) = resolution.value().unwrap_err() else {
        panic!("semantic failure required")
    };
    assert_eq!(issues.len(), 1);
    let Issue::Duplicate {
        locale,
        occurrences,
    } = &issues[0]
    else {
        panic!("duplicate required")
    };
    assert_eq!(locale.as_str(), "en-US");
    assert_eq!(occurrences, &[0, 1, 2]);
    assert_eq!(resolution.counts().active_occurrences, Some(5));
    assert_eq!(resolution.counts().canonical_requested, Some(2));
}

#[test]
fn requested_default_must_be_an_explicit_member() {
    let mut value = minimal_config();
    value["profiles"]["app"]["defaultRequestedLocale"] = json!("fr");
    let resolution = resolve(&value, limits(16, 8));
    let Failure::Issues(issues) = resolution.value().unwrap_err() else {
        panic!("semantic failure required")
    };
    assert_eq!(issues.len(), 1);
    assert!(matches!(&issues[0], Issue::DefaultNotRequested { locale } if locale.as_str() == "fr"));
}

#[test]
fn permutations_and_aliases_preserve_the_core_but_semantic_mutations_do_not() {
    let mut value = minimal_config();
    value["profiles"]["app"]["requestedLocales"] = json!(["en", "en-US", "he-IL", "ja"]);
    value["profiles"]["app"]["defaultRequestedLocale"] = json!("en-US");
    value["profiles"]["app"]["defaultSourceLocale"] = json!("en");
    let baseline = resolve(&value, limits(16, 8));
    for requested in [
        json!(["ja", "he-IL", "en-US", "en"]),
        json!(["EN-us", "ja", "EN", "iw-IL"]),
        json!(["iw-IL", "en", "ja", "EN-us"]),
    ] {
        let mut permuted = value.clone();
        permuted["profiles"]["app"]["requestedLocales"] = requested;
        permuted["profiles"]["app"]["defaultSourceLocale"] = json!("EN");
        permuted["profiles"]["app"]["defaultRequestedLocale"] = json!("EN-us");
        let resolved = resolve(&permuted, limits(16, 8));
        assert_eq!(resolved.value(), baseline.value());
        assert_eq!(resolved.counts(), baseline.counts());
        assert_ne!(resolved.corrections(), baseline.corrections());
    }
    for (field, changed) in [
        ("defaultSourceLocale", json!("fr")),
        ("defaultRequestedLocale", json!("ja")),
        ("requestedLocales", json!(["en-US", "ja"])),
    ] {
        let mut mutated = value.clone();
        mutated["profiles"]["app"][field] = changed;
        assert_ne!(
            resolve(&mutated, limits(16, 8)).value().unwrap(),
            baseline.value().unwrap()
        );
    }
    value["profiles"]["app"]
        .as_object_mut()
        .unwrap()
        .remove("defaultSourceLocale");
    assert_ne!(
        resolve(&value, limits(16, 8)).value().unwrap(),
        baseline.value().unwrap()
    );
}

#[test]
fn occurrence_admission_counts_all_active_roles_before_any_provider_work() {
    use crate::locale::{ProviderBinding, ProviderFailure};
    use std::cell::Cell;
    use std::rc::Rc;

    struct CountingProvider {
        inner: FixtureProvider,
        calls: Rc<Cell<usize>>,
    }
    impl Provider for CountingProvider {
        type Identity = &'static str;
        fn binding(&self) -> &ProviderBinding<Self::Identity> {
            self.inner.binding()
        }
        fn canonicalize(&self, input: &str) -> Result<Arc<str>, ProviderFailure> {
            self.calls.set(self.calls.get() + 1);
            self.inner.canonicalize(input)
        }
    }
    let calls = Rc::new(Cell::new(0));
    let provider = Canonicalizer::bind(
        &fixture_binding(),
        Some(CountingProvider {
            inner: FixtureProvider::new(),
            calls: Rc::clone(&calls),
        }),
        Bound::new(128).unwrap(),
    )
    .unwrap();
    let mut value = minimal_config();
    value["profiles"]["app"]["requestedLocales"] = json!(["EN-us", "ja"]);
    value["profiles"]["app"]["defaultRequestedLocale"] = json!("EN-us");
    value["profiles"]["app"]["defaultSourceLocale"] = json!("en");
    let config = config(&value);
    let id = serde_json::from_value(json!("app")).unwrap();
    let input = Input::from_selected(&config, &id).unwrap();
    let rejected = input.resolve(&provider, limits(3, 8));
    assert_eq!(
        rejected.value(),
        Err(&Failure::OccurrenceLimit {
            limit: Bound::new(3).unwrap(),
            actual: 4
        })
    );
    assert_eq!(
        rejected.counts(),
        Counts {
            active_occurrences: Some(4),
            canonical_requested: None
        }
    );
    assert!(rejected.corrections().is_empty());
    assert_eq!(calls.get(), 0);
    assert!(input.resolve(&provider, limits(4, 8)).value().is_ok());
    assert_eq!(calls.get(), 4);
    assert_eq!(active_occurrences(u64::MAX - 1, false), Some(u64::MAX));
    assert_eq!(active_occurrences(u64::MAX - 1, true), None);
    assert_eq!(active_occurrences(u64::MAX, false), None);
}

#[test]
fn canonical_cardinality_limits_do_not_silently_deduplicate_or_truncate_input() {
    let mut value = minimal_config();
    value["profiles"]["app"]["requestedLocales"] = json!(["en", "fr", "ja"]);
    assert!(resolve(&value, limits(4, 3)).value().is_ok());
    let over = resolve(&value, limits(4, 2));
    assert_eq!(over.counts().canonical_requested, Some(3));
    assert!(
        matches!(over.value(), Err(Failure::Issues(issues)) if issues == &[Issue::RequestedLimit { limit: Bound::new(2).unwrap(), actual: 3 }])
    );

    value["profiles"]["app"]["requestedLocales"] = json!(["en", "EN", "en-US"]);
    let duplicate = resolve(&value, limits(4, 2));
    assert_eq!(
        duplicate.counts(),
        Counts {
            active_occurrences: Some(4),
            canonical_requested: Some(2)
        }
    );
    assert!(
        matches!(duplicate.value(), Err(Failure::Issues(issues)) if matches!(&issues[..], [Issue::Duplicate { occurrences, .. }] if occurrences == &[0, 1]))
    );
    assert!(matches!(
        resolve(&value, limits(3, 2)).value(),
        Err(Failure::OccurrenceLimit { actual: 4, .. })
    ));
    let both = resolve(&value, limits(4, 1));
    assert!(
        matches!(both.value(), Err(Failure::Issues(issues)) if matches!(&issues[..], [Issue::Duplicate { .. }, Issue::RequestedLimit { actual: 2, .. }]))
    );
}

#[test]
fn independent_errors_remain_visible_without_cascading_incomplete_set_checks() {
    use crate::locale::ProviderFailure;
    let mut value = minimal_config();
    value["profiles"]["app"]["defaultSourceLocale"] = json!("en_US");
    value["profiles"]["app"]["requestedLocales"] = json!(["zz", "EN-us", "en-US"]);
    value["profiles"]["app"]["defaultRequestedLocale"] = json!("fr");
    let resolved = resolve(&value, limits(16, 1));
    let Failure::Issues(issues) = resolved.value().unwrap_err() else {
        panic!("issues required")
    };
    assert!(matches!(&issues[..], [
        Issue::Canonicalization { location: Location::SourceDefault, reason: CanonicalizationFailure::Provider(ProviderFailure::InvalidIdentifier) },
        Issue::Canonicalization { location: Location::Requested(0), reason: CanonicalizationFailure::Provider(ProviderFailure::InvalidIdentifier) },
        Issue::Duplicate { occurrences, .. },
    ] if occurrences == &[1, 2]));
    assert_eq!(resolved.counts().canonical_requested, None);
    assert_eq!(resolved.corrections()[0].location, Location::Requested(1));

    value["profiles"]["app"]["requestedLocales"] = json!(["en"]);
    let resolved = resolve(&value, limits(16, 1));
    assert!(
        matches!(resolved.value(), Err(Failure::Issues(issues)) if matches!(&issues[..], [Issue::Canonicalization { location: Location::SourceDefault, .. }, Issue::DefaultNotRequested { .. }]))
    );
    assert_eq!(resolved.counts().canonical_requested, Some(1));
}

#[test]
fn unsupported_and_invalid_occurrences_remain_distinct_without_retaining_raw_secrets() {
    use crate::locale::ProviderFailure;
    let mut value = minimal_config();
    value["profiles"]["app"]["defaultSourceLocale"] = json!("secret-candidate-not-in-the-provider");
    value["profiles"]["app"]["requestedLocales"] = json!(["en_US", "pt-BR"]);
    value["profiles"]["app"]["defaultRequestedLocale"] = json!("zz");
    let resolved = resolve(&value, limits(16, 8));
    assert!(!format!("{resolved:?}").contains("secret-candidate"));
    let Failure::Issues(issues) = resolved.value().unwrap_err() else {
        panic!("issues required")
    };
    assert_eq!(issues.len(), 4);
    for (issue, expected) in issues.iter().zip([
        ProviderFailure::UnsupportedInput,
        ProviderFailure::InvalidIdentifier,
        ProviderFailure::UnsupportedInput,
        ProviderFailure::InvalidIdentifier,
    ]) {
        assert!(
            matches!(issue, Issue::Canonicalization { reason: CanonicalizationFailure::Provider(actual), .. } if *actual == expected)
        );
    }
    assert!(resolved.corrections().is_empty());
}

#[test]
fn selection_uses_only_the_explicit_declared_profile_of_a_complete_root() {
    use crate::structural::selection::{Selection, SelectorInput};
    let mut value = minimal_config();
    value["profiles"]["other"] = value["profiles"]["app"].clone();
    value["profiles"]["other"]["requestedLocales"] = json!(["zz"]);
    let admitted = analysis(&value);
    let config = admitted.construct().unwrap().unwrap();
    let unknown: ProfileId = serde_json::from_value(json!("unknown")).unwrap();
    assert!(Input::from_selected(&config, &unknown).is_none());
    for (id, succeeds) in [("app", true), ("other", false)] {
        let Selection::Selected(selected) = admitted
            .select(&SelectorInput::string(id, Bound::new(256).unwrap()))
            .unwrap()
        else {
            panic!("explicit selection required")
        };
        assert_eq!(
            Input::from_selected(&config, selected.id())
                .unwrap()
                .resolve(&provider(), limits(16, 8))
                .value()
                .is_ok(),
            succeeds
        );
    }
    // A structural failure in any sibling withholds the complete root. The core
    // cannot bypass that gate by taking an arbitrary typed declaration.
    value["profiles"]["other"]
        .as_object_mut()
        .unwrap()
        .remove("defaultRequestedLocale");
    assert!(analysis(&value).construct().unwrap().is_none());
}

#[test]
fn missing_defaults_nulls_and_empty_requested_sets_never_reach_locale_resolution() {
    let mut missing = minimal_config();
    missing["profiles"]["app"]
        .as_object_mut()
        .unwrap()
        .remove("defaultRequestedLocale");
    let mut null_source = minimal_config();
    null_source["profiles"]["app"]["defaultSourceLocale"] = Value::Null;
    let mut empty = minimal_config();
    empty["profiles"]["app"]["requestedLocales"] = json!([]);
    for value in [missing, null_source, empty] {
        assert!(analysis(&value).construct().unwrap().is_none());
    }
}

#[test]
fn fresh_and_reused_calls_keep_results_owned_and_do_not_retain_failure_state() {
    let canonicalizer = provider();
    let good = config(&minimal_config());
    let id = serde_json::from_value(json!("app")).unwrap();
    let input = Input::from_selected(&good, &id).unwrap();
    let retained = input.resolve(&canonicalizer, limits(16, 8));
    let mut bad = minimal_config();
    bad["profiles"]["app"]["requestedLocales"] = json!(["en", "en"]);
    let bad = config(&bad);
    for _ in 0..3 {
        assert!(Input::from_selected(&bad, &id)
            .unwrap()
            .resolve(&canonicalizer, limits(16, 8))
            .value()
            .is_err());
        assert_eq!(input.resolve(&canonicalizer, limits(16, 8)), retained);
        assert_eq!(input.resolve(&provider(), limits(16, 8)), retained);
    }
    drop(good);
    drop(bad);
    drop(canonicalizer);
    assert_eq!(retained.value().unwrap().requested_default().as_str(), "en");
}

#[test]
fn expanded_identifier_limits_apply_independently_to_requested_and_default_occurrences() {
    use crate::locale::Spelling;
    let mut value = minimal_config();
    value["profiles"]["app"]["requestedLocales"] = json!(["und-u-ca-islamicc"]);
    value["profiles"]["app"]["defaultRequestedLocale"] = json!("und-u-ca-islamicc");
    let config = config(&value);
    let id = serde_json::from_value(json!("app")).unwrap();
    let canonicalizer = Canonicalizer::bind(
        &fixture_binding(),
        Some(FixtureProvider::new()),
        Bound::new(21).unwrap(),
    )
    .unwrap();
    let resolved = Input::from_selected(&config, &id)
        .unwrap()
        .resolve(&canonicalizer, limits(2, 1));
    let Failure::Issues(issues) = resolved.value().unwrap_err() else {
        panic!("issues required")
    };
    assert_eq!(issues.len(), 2);
    for (issue, location) in issues
        .iter()
        .zip([Location::Requested(0), Location::RequestedDefault])
    {
        assert!(
            matches!(issue, Issue::Canonicalization { location: actual_location, reason: CanonicalizationFailure::ByteLimit { spelling: Spelling::Canonical, limit, actual: 22 } } if *actual_location == location && limit.get() == 21)
        );
    }
    assert_eq!(resolved.counts().active_occurrences, Some(2));
    assert_eq!(resolved.counts().canonical_requested, None);
    assert!(resolved.corrections().is_empty());
}

#[test]
fn each_duplicate_group_has_one_canonical_issue_with_all_original_indices() {
    let mut value = minimal_config();
    value["profiles"]["app"]["requestedLocales"] =
        json!(["ja", "EN-us", "EN", "en-US", "ja", "en"]);
    value["profiles"]["app"]["defaultSourceLocale"] = json!("en");
    let resolved = resolve(&value, limits(16, 3));
    let Failure::Issues(issues) = resolved.value().unwrap_err() else {
        panic!("issues required")
    };
    assert_eq!(issues.len(), 3);
    for (issue, (expected_locale, expected_indices)) in
        issues
            .iter()
            .zip([("en", [2, 5]), ("en-US", [1, 3]), ("ja", [0, 4])])
    {
        let Issue::Duplicate {
            locale,
            occurrences,
        } = issue
        else {
            panic!("duplicate group required")
        };
        assert_eq!(locale.as_str(), expected_locale);
        assert_eq!(occurrences, &expected_indices);
    }
    assert_eq!(
        resolved.counts(),
        Counts {
            active_occurrences: Some(8),
            canonical_requested: Some(3)
        }
    );
}

#[test]
fn minimum_input_and_result_types_do_not_acquire_a_serde_artifact_surface() {
    trait AmbiguousIfSerialize<Marker> {
        fn assert_unambiguous() {}
    }
    impl<T> AmbiguousIfSerialize<()> for T {}
    impl<T: serde::Serialize> AmbiguousIfSerialize<u8> for T {}
    trait AmbiguousIfDeserialize<Marker> {
        fn assert_unambiguous() {}
    }
    impl<T> AmbiguousIfDeserialize<()> for T {}
    impl<T: serde::de::DeserializeOwned> AmbiguousIfDeserialize<u8> for T {}
    let _ = <Input<'static> as AmbiguousIfSerialize<_>>::assert_unambiguous;
    let _ = <Core as AmbiguousIfSerialize<_>>::assert_unambiguous;
    let _ = <Resolution as AmbiguousIfSerialize<_>>::assert_unambiguous;
    let _ = <Core as AmbiguousIfDeserialize<_>>::assert_unambiguous;
    let _ = <Resolution as AmbiguousIfDeserialize<_>>::assert_unambiguous;
}
