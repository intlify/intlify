// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Test-only minimum vertical slice. This is not a product resolver entry,
//! formal Resolver Outcome, conformance manifest, or workflow benchmark.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::fixtures::minimal_config;
use crate::locale::core::{Failure, Issue, Location};
use crate::locale::{CanonicalLocale, CanonicalizationFailure, ProviderFailure};
use crate::materialize::InputFailure;
use crate::structural::selection::SelectionFailure;
use crate::structural::StructuralFailure;

mod harness;
use harness::{FixtureLimits, FixtureRunner, Outcome, Step};

fn source(value: &Value) -> Arc<[u8]> {
    Arc::from(serde_json::to_vec(value).unwrap())
}

fn runner() -> FixtureRunner {
    FixtureRunner::new(FixtureLimits::finite())
}

#[test]
fn strict_bytes_reach_only_the_selected_private_locale_core() {
    let mut value = minimal_config();
    value["profiles"]["app"]["requestedLocales"] = json!(["ja", "EN-us"]);
    value["profiles"]["app"]["defaultRequestedLocale"] = json!("EN-us");
    let result = runner().run(source(&value), None);
    assert_eq!(result.steps, Step::ALL);
    assert_eq!(result.selected.as_ref().unwrap().as_str(), "app");
    let Outcome::Locale(result) = result.outcome else {
        panic!("minimum slice did not reach locale resolution")
    };
    let core = result.value().unwrap();
    assert_eq!(core.source_default(), None);
    assert_eq!(
        core.requested()
            .iter()
            .map(CanonicalLocale::as_str)
            .collect::<Vec<_>>(),
        ["en-US", "ja"]
    );
    assert_eq!(core.requested_default().as_str(), "en-US");
    assert_eq!(
        result
            .corrections()
            .iter()
            .map(|item| (item.location, item.replacement.as_str()))
            .collect::<Vec<_>>(),
        [
            (Location::Requested(1), "en-US"),
            (Location::RequestedDefault, "en-US")
        ]
    );
}

#[test]
fn invalid_raw_inputs_stop_before_structure_selection_and_locale_processing() {
    for (bytes, expected) in [
        (&b"\xff"[..], InputFailure::InvalidUtf8),
        (&b"{"[..], InputFailure::Syntax),
        (&b"{} null"[..], InputFailure::Syntax),
        (&b"{/* comment */}"[..], InputFailure::Syntax),
        (&br#"{"x":1,}"#[..], InputFailure::Syntax),
        (&br#"{"x":1,"\u0078":2}"#[..], InputFailure::DuplicateMember),
        (&br#""\uD800""#[..], InputFailure::NonScalarString),
        (&b"9007199254740992"[..], InputFailure::NonPortableNumber),
    ] {
        let result = runner().run(Arc::from(bytes), Some("app"));
        assert_eq!(result.steps, [Step::Materialize]);
        assert!(result.selected.is_none());
        let Outcome::Materialization(error) = result.outcome else {
            panic!("wrong stopping point")
        };
        assert_eq!(error.reason, expected);
    }
}

#[test]
fn root_and_version_failures_stop_before_model_construction() {
    for (value, expected) in [
        (json!([]), StructuralFailure::RootTypeInvalid),
        (json!({}), StructuralFailure::SchemaVersionMissing),
        (
            json!({"schemaVersion": 0}),
            StructuralFailure::SchemaVersionTypeInvalid,
        ),
        (
            json!({"schemaVersion": "1"}),
            StructuralFailure::SchemaVersionUnsupported,
        ),
    ] {
        let result = runner().run(source(&value), None);
        assert_eq!(result.steps, [Step::Materialize, Step::Structural]);
        assert!(result.selected.is_none());
        let Outcome::Structural(analysis) = result.outcome else {
            panic!("wrong stopping point")
        };
        assert_eq!(analysis.issues().len(), 1);
        assert_eq!(analysis.issues()[0].reason, expected);
        assert!(analysis.construct().unwrap().is_none());
    }
}

#[test]
fn incomplete_root_and_invalid_sibling_never_reach_selected_profile_resolution() {
    for mutation in 0..8 {
        let mut value = minimal_config();
        let app = &mut value["profiles"]["app"];
        match mutation {
            0 => {
                app.as_object_mut()
                    .unwrap()
                    .remove("defaultRequestedLocale");
            }
            1 => app["defaultRequestedLocale"] = Value::Null,
            2 => app["requestedLocales"] = json!([]),
            3 => app["requestedLocales"] = Value::Null,
            4 => app["defaultSourceLocale"] = Value::Null,
            5 => app["unexpected"] = json!(true),
            6 => app["policies"] = Value::Null,
            _ => value["profiles"]["other"] = json!({"broken": true}),
        }
        let result = runner().run(source(&value), Some("app"));
        assert_eq!(
            result.steps,
            [Step::Materialize, Step::Structural],
            "mutation {mutation}"
        );
        assert!(result.selected.is_none());
        let Outcome::Structural(analysis) = result.outcome else {
            panic!("wrong stopping point")
        };
        assert!(analysis.version_selected());
        assert!(analysis.schema_issue_count() > 0);
        assert!(analysis.construct().unwrap().is_none());
    }
}

#[test]
fn named_selection_is_exact_and_failures_never_start_locale_work() {
    let mut multiple = minimal_config();
    let mut other = multiple["profiles"]["app"].clone();
    other["requestedLocales"] = json!(["ja"]);
    other["defaultRequestedLocale"] = json!("ja");
    multiple["profiles"]["other"] = other;
    for (value, selector, expected) in [
        (
            minimal_config(),
            Some("unknown"),
            SelectionFailure::Unknown { bytes: 7 },
        ),
        (
            minimal_config(),
            Some("APP"),
            SelectionFailure::InvalidSyntax { bytes: 3 },
        ),
        (multiple.clone(), None, SelectionFailure::Required),
    ] {
        let result = runner().run(source(&value), selector);
        assert_eq!(result.steps, Step::ALL[..4]);
        assert!(result.selected.is_none());
        let Outcome::Selection(reason) = result.outcome else {
            panic!("wrong stopping point")
        };
        assert_eq!(reason, expected);
    }
    let result = runner().run(source(&multiple), Some("other"));
    assert_eq!(result.steps, Step::ALL);
    assert_eq!(result.selected.unwrap().as_str(), "other");
    let Outcome::Locale(result) = result.outcome else {
        panic!("selected profile did not resolve")
    };
    assert_eq!(result.value().unwrap().requested_default().as_str(), "ja");
}

#[test]
fn semantic_failures_preserve_the_exact_cause_without_a_partial_locale_core() {
    enum Expected<'a> {
        Invalid(Location, ProviderFailure),
        Duplicate(&'a str),
        Outside(&'a str),
    }
    use Expected::{Duplicate, Invalid, Outside};
    for (source_default, requested, default, expected) in [
        (
            Some("en_US"),
            vec!["en"],
            "en",
            Invalid(Location::SourceDefault, ProviderFailure::InvalidIdentifier),
        ),
        (
            None,
            vec!["en_US"],
            "en",
            Invalid(Location::Requested(0), ProviderFailure::InvalidIdentifier),
        ),
        (
            None,
            vec!["en"],
            "en_US",
            Invalid(
                Location::RequestedDefault,
                ProviderFailure::InvalidIdentifier,
            ),
        ),
        (None, vec!["EN-us", "en-US"], "en-US", Duplicate("en-US")),
        (None, vec!["en"], "ja", Outside("ja")),
        (
            None,
            vec!["pt-BR"],
            "en",
            Invalid(Location::Requested(0), ProviderFailure::UnsupportedInput),
        ),
    ] {
        let mut value = minimal_config();
        if let Some(locale) = source_default {
            value["profiles"]["app"]["defaultSourceLocale"] = json!(locale);
        }
        value["profiles"]["app"]["requestedLocales"] = json!(requested);
        value["profiles"]["app"]["defaultRequestedLocale"] = json!(default);
        let result = runner().run(source(&value), None);
        assert_eq!(result.steps, Step::ALL);
        let Outcome::Locale(result) = result.outcome else {
            panic!("wrong stopping point")
        };
        let Failure::Issues(issues) = result.value().unwrap_err() else {
            panic!("wrong semantic failure")
        };
        assert_eq!(issues.len(), 1);
        match (expected, &issues[0]) {
            (
                Invalid(expected_location, provider),
                Issue::Canonicalization { location, reason },
            ) => {
                assert_eq!(*location, expected_location);
                assert_eq!(*reason, CanonicalizationFailure::Provider(provider));
            }
            (
                Duplicate(expected),
                Issue::Duplicate {
                    locale,
                    occurrences,
                },
            ) => {
                assert_eq!(locale.as_str(), expected);
                assert_eq!(occurrences, &[0, 1]);
            }
            (Outside(expected), Issue::DefaultNotRequested { locale }) => {
                assert_eq!(locale.as_str(), expected);
            }
            _ => panic!("wrong semantic reason"),
        }
    }
}

fn reverse_members(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for value in map.values_mut() {
                reverse_members(value);
            }
            *map = map
                .iter()
                .rev()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
        }
        Value::Array(items) => {
            for item in items {
                reverse_members(item);
            }
        }
        _ => {}
    }
}

#[test]
fn file_spelling_and_authoring_permutations_preserve_core_meaning_not_corrections() {
    let context = runner();
    let mut value = minimal_config();
    value["profiles"]["app"]["requestedLocales"] = json!(["en-US", "ja"]);
    value["profiles"]["app"]["defaultRequestedLocale"] = json!("en-US");
    value["profiles"]["app"]["defaultSourceLocale"] = json!("en");
    let Outcome::Locale(baseline) = context.run(source(&value), None).outcome else {
        unreachable!()
    };
    let mut reordered = value.clone();
    reverse_members(&mut reordered);
    reordered["profiles"]["app"]["requestedLocales"] = json!(["ja", "en-US"]);
    let pretty = serde_json::to_string_pretty(&value).unwrap();
    let mut aliases = reordered.clone();
    aliases["profiles"]["app"]["requestedLocales"] = json!(["ja", "EN-us"]);
    aliases["profiles"]["app"]["defaultRequestedLocale"] = json!("EN-us");
    aliases["profiles"]["app"]["defaultSourceLocale"] = json!("EN");
    for bytes in [
        source(&value),
        source(&reordered),
        Arc::from(pretty.as_bytes()),
        Arc::from(format!(" \r\n{}\t ", pretty.replace('\n', "\r\n")).into_bytes()),
        source(&aliases),
    ] {
        let result = context.run(bytes, None);
        assert_eq!(result.steps, Step::ALL);
        let Outcome::Locale(result) = result.outcome else {
            unreachable!()
        };
        assert_eq!(result.value(), baseline.value());
    }
    let Outcome::Locale(aliased) = context.run(source(&aliases), None).outcome else {
        unreachable!()
    };
    assert!(baseline.corrections().is_empty());
    assert_eq!(aliased.corrections().len(), 3);
    for field in [
        "defaultSourceLocale",
        "defaultRequestedLocale",
        "requestedLocales",
    ] {
        let mut changed = value.clone();
        changed["profiles"]["app"][field] = match field {
            "defaultSourceLocale" => json!("fr"),
            "defaultRequestedLocale" => json!("ja"),
            _ => json!(["en-US", "ja", "fr"]),
        };
        let Outcome::Locale(result) = context.run(source(&changed), None).outcome else {
            unreachable!()
        };
        assert!(result.value().is_ok());
        assert_ne!(result.value(), baseline.value(), "{field}");
    }
}

#[test]
fn resource_edges_stop_at_their_owning_stage_without_later_semantic_work() {
    use crate::input_limits::{Bound, InputBound};
    let bytes = source(&minimal_config());
    let mut limits = FixtureLimits::finite();
    limits.input.raw.max_file_bytes = Bound::new(bytes.len() as u64).unwrap();
    assert_eq!(
        FixtureRunner::new(limits)
            .run(Arc::clone(&bytes), None)
            .steps,
        Step::ALL
    );
    limits.input.raw.max_file_bytes = Bound::new(bytes.len() as u64 - 1).unwrap();
    let result = FixtureRunner::new(limits).run(Arc::clone(&bytes), None);
    assert_eq!(result.steps, [Step::Materialize]);
    let Outcome::Materialization(error) = result.outcome else {
        unreachable!()
    };
    let InputFailure::ResourceLimits(violations) = error.reason else {
        unreachable!()
    };
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].bound, InputBound::FileBytes);
    assert_eq!(violations[0].actual, bytes.len() as u64);

    let mut multiple = minimal_config();
    multiple["profiles"]["other"] = multiple["profiles"]["app"].clone();
    let mut limits = FixtureLimits::finite();
    limits.structural.max_profiles = Bound::new(1).unwrap();
    let result = FixtureRunner::new(limits).run(source(&multiple), Some("app"));
    assert_eq!(result.steps, [Step::Materialize, Step::Structural]);
    let Outcome::Structural(analysis) = result.outcome else {
        unreachable!()
    };
    assert_eq!(
        analysis.issues()[0].reason,
        StructuralFailure::ProfilesLimit {
            limit: Bound::new(1).unwrap(),
            actual: 2
        }
    );

    for maximum in [1, 2] {
        let mut limits = FixtureLimits::finite();
        limits.locale.max_active_occurrences = Bound::new(maximum).unwrap();
        let result = FixtureRunner::new(limits).run(Arc::clone(&bytes), None);
        assert_eq!(result.steps, Step::ALL);
        let Outcome::Locale(result) = result.outcome else {
            unreachable!()
        };
        if maximum == 1 {
            assert_eq!(
                result.value().unwrap_err(),
                &Failure::OccurrenceLimit {
                    limit: Bound::new(1).unwrap(),
                    actual: 2
                }
            );
        } else {
            assert!(result.value().is_ok());
        }
    }
}

#[test]
fn owned_results_survive_interleaved_failures_input_release_and_runner_release() {
    let retained;
    let blocked;
    {
        let context = runner();
        let bytes = source(&minimal_config());
        let Outcome::Locale(first) = context.run(bytes, None).outcome else {
            unreachable!()
        };
        retained = first;
        blocked = context.run(source(&json!({"schemaVersion": "1"})), None);
        let raw_failure = context.run(Arc::from(&b"{"[..]), None);
        assert!(matches!(raw_failure.outcome, Outcome::Materialization(_)));
        let mut duplicate = minimal_config();
        duplicate["profiles"]["app"]["requestedLocales"] = json!(["en", "en"]);
        let Outcome::Locale(failure) = context.run(source(&duplicate), None).outcome else {
            unreachable!()
        };
        assert!(failure.value().is_err());
        let Outcome::Locale(repeated) = context.run(source(&minimal_config()), None).outcome else {
            unreachable!()
        };
        assert_eq!(repeated, retained);
    }
    assert_eq!(retained.value().unwrap().requested_default().as_str(), "en");
    let Outcome::Structural(analysis) = blocked.outcome else {
        unreachable!()
    };
    assert_eq!(
        analysis.issues()[0].reason,
        StructuralFailure::SchemaVersionUnsupported
    );
    assert!(analysis.construct().unwrap().is_none());
}

#[test]
#[cfg(not(target_family = "wasm"))]
fn explicit_parallel_callers_keep_fixture_association_and_results_deterministic() {
    let context = runner();
    let inputs = ["en", "ja", "fr"].map(|locale| {
        let mut value = minimal_config();
        value["profiles"]["app"]["requestedLocales"] = json!([locale]);
        value["profiles"]["app"]["defaultRequestedLocale"] = json!(locale);
        (locale, source(&value))
    });
    // Scheduling belongs to the test caller, not an implicit pool in the core.
    std::thread::scope(|scope| {
        let workers = inputs
            .iter()
            .rev()
            .map(|(expected, bytes)| {
                scope.spawn(|| (*expected, context.run(Arc::clone(bytes), None)))
            })
            .collect::<Vec<_>>();
        for worker in workers {
            let (expected, result) = worker.join().unwrap();
            assert_eq!(result.steps, Step::ALL);
            assert_eq!(result.selected.unwrap().as_str(), "app");
            let Outcome::Locale(result) = result.outcome else {
                unreachable!()
            };
            assert_eq!(
                result.value().unwrap().requested_default().as_str(),
                expected
            );
            assert_eq!(result.value().unwrap().requested().len(), 1);
            assert!(result.corrections().is_empty());
        }
    });
}
