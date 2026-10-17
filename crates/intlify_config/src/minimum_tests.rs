// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Test-only minimum vertical slice. This is not a product resolver entry,
//! formal Resolver Outcome, conformance manifest, or workflow benchmark.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::locale::{CanonicalLocale, CanonicalizationFailure, ProviderFailure};
use crate::materialize::InputFailure;
use crate::profile_fixtures::minimal_config;
use crate::resolver::evaluation::{Actual, Code, Evaluation, Status, Subject};
use crate::resolver::locales::{
    LocaleInvariant, LocaleKey, LocaleSet, LocaleSubject, Location, ResolvedLocales,
};
use crate::structural::selection::SelectionFailure;
use crate::structural::StructuralFailure;

mod harness;
use harness::{FixtureLimits, FixtureRunner, Locales, Outcome, Run, Step};

fn source(value: &Value) -> Arc<[u8]> {
    Arc::from(serde_json::to_vec(value).unwrap())
}

fn runner() -> FixtureRunner {
    FixtureRunner::new(FixtureLimits::finite())
}

fn locales(run: Run) -> Locales {
    let Outcome::Locale(locales) = run.outcome else {
        panic!("minimum slice did not reach locale resolution")
    };
    locales
}

fn names(set: &LocaleSet) -> Vec<&str> {
    set.as_slice().iter().map(CanonicalLocale::as_str).collect()
}

/// Where a spelling differs from its canonical form.
fn corrected(resolved: &ResolvedLocales) -> Vec<Location> {
    resolved
        .occurrences
        .iter()
        .filter(|occurrence| occurrence.corrected)
        .map(|occurrence| occurrence.location.clone())
        .collect()
}

/// Each blocking result as its subject's role and locale, and its code.
fn issues(evaluation: &Evaluation) -> Vec<(String, Code)> {
    let key = |key: &LocaleKey| match key {
        LocaleKey::Canonical(locale) => locale.as_str().to_owned(),
        LocaleKey::Content(content) => content.as_str().to_owned(),
    };
    evaluation
        .issues()
        .map(|issue| {
            let Subject::Locale(subject) = &issue.unit().subject else {
                panic!("only Phase 7 runs in this slice")
            };
            let subject = match subject {
                LocaleSubject::Occurrences => "occurrences".to_owned(),
                LocaleSubject::RequestedLocale(locale) => format!("requested {}", key(locale)),
                LocaleSubject::RequestedLocales => "requested set".to_owned(),
                LocaleSubject::DefaultSourceLocale(locale) => format!("source {}", key(locale)),
                LocaleSubject::DefaultRequestedLocale(locale) => {
                    format!("default {}", key(locale))
                }
                other => panic!("no minimum fixture declares {other:?}"),
            };
            (subject, issue.code())
        })
        .collect()
}

#[test]
fn strict_bytes_reach_only_the_selected_declarations_locale_resolution() {
    let mut value = minimal_config();
    value["profiles"]["app"]["requestedLocales"] = json!(["ja", "EN-us"]);
    value["profiles"]["app"]["defaultRequestedLocale"] = json!("EN-us");
    let run = runner().run(source(&value), None);
    assert_eq!(run.steps, Step::ALL);
    assert_eq!(run.selected.as_ref().unwrap().as_str(), "app");
    let locales = locales(run);
    assert!(issues(&locales.evaluation).is_empty());
    let resolved = locales.result.unwrap();
    let project = resolved.project.as_ref().unwrap();
    assert_eq!(project.source_default(), None);
    assert_eq!(names(project.requested()), ["en-US", "ja"]);
    assert_eq!(project.requested_default().as_str(), "en-US");
    assert_eq!(
        corrected(&resolved),
        [
            Location::RequestedLocale(1),
            Location::DefaultRequestedLocale
        ]
    );
    assert_eq!(locales.evaluation.notices().count(), 2);
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
fn malformed_formal_references_and_legacy_test_tokens_stop_before_locale_work() {
    for replacement in [
        json!({"$testPolicy": "resource-limits"}),
        Value::Null,
        json!("resource-limits"),
        {
            let mut pin = crate::profile_fixtures::reference("resource-limit-policy");
            pin.as_object_mut().unwrap().remove("revision");
            pin
        },
        {
            let mut pin = crate::profile_fixtures::reference("resource-limit-policy");
            pin["kind"] = json!("unknown-policy");
            pin
        },
        {
            let mut pin = crate::profile_fixtures::reference("resource-limit-policy");
            pin["semanticDigest"] = json!("sha256:1");
            pin
        },
        {
            let mut pin = crate::profile_fixtures::reference("resource-limit-policy");
            pin["unexpected"] = json!(true);
            pin
        },
    ] {
        let mut value = minimal_config();
        value["profiles"]["app"]["policies"]["resourceLimits"] = replacement;
        let result = runner().run(source(&value), Some("app"));
        assert_eq!(result.steps, [Step::Materialize, Step::Structural]);
        assert!(result.selected.is_none());
        let Outcome::Structural(analysis) = result.outcome else {
            panic!("formal reference failure did not stop at structural admission")
        };
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
    let run = runner().run(source(&multiple), Some("other"));
    assert_eq!(run.steps, Step::ALL);
    assert_eq!(run.selected.as_ref().unwrap().as_str(), "other");
    let resolved = locales(run).result.unwrap();
    assert_eq!(resolved.project.unwrap().requested_default().as_str(), "ja");
}

#[test]
fn semantic_failures_keep_their_exact_subject_and_leave_no_project_locales() {
    for (source_default, requested, default, expected) in [
        (
            Some("en_US"),
            vec!["en"],
            "en",
            ("source en_US", Code::LocaleInvalid),
        ),
        (
            None,
            vec!["en_US"],
            "en",
            ("requested en_US", Code::LocaleInvalid),
        ),
        (
            None,
            vec!["en"],
            "en_US",
            ("default en_US", Code::LocaleInvalid),
        ),
        (
            None,
            vec!["EN-us", "en-US"],
            "en-US",
            ("requested en-US", Code::LocaleDuplicate),
        ),
        (
            None,
            vec!["en"],
            "ja",
            ("default ja", Code::DefaultRequestedLocaleNotInProject),
        ),
    ] {
        let mut value = minimal_config();
        if let Some(locale) = source_default {
            value["profiles"]["app"]["defaultSourceLocale"] = json!(locale);
        }
        value["profiles"]["app"]["requestedLocales"] = json!(requested);
        value["profiles"]["app"]["defaultRequestedLocale"] = json!(default);
        let run = runner().run(source(&value), None);
        assert_eq!(run.steps, Step::ALL);
        let locales = locales(run);
        assert_eq!(
            issues(&locales.evaluation),
            [(expected.0.to_owned(), expected.1)]
        );
        assert!(locales.result.unwrap().project.is_err(), "{expected:?}");
    }
    // A spelling the finite provider has no answer for is not an author's
    // error: Phase 7 stops as a resolver defect instead of reporting it.
    let mut value = minimal_config();
    value["profiles"]["app"]["requestedLocales"] = json!(["pt-BR"]);
    assert_eq!(
        locales(runner().run(source(&value), None)).result,
        Err(LocaleInvariant::Canonicalization(
            CanonicalizationFailure::Provider(ProviderFailure::UnsupportedInput)
        ))
    );
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
fn file_spelling_and_authoring_permutations_preserve_meaning_and_aliases_only_add_corrections() {
    let context = runner();
    let mut value = minimal_config();
    value["profiles"]["app"]["requestedLocales"] = json!(["en-US", "ja"]);
    value["profiles"]["app"]["defaultRequestedLocale"] = json!("en-US");
    value["profiles"]["app"]["defaultSourceLocale"] = json!("en");
    let baseline = locales(context.run(source(&value), None));
    let mut reordered = value.clone();
    reverse_members(&mut reordered);
    reordered["profiles"]["app"]["requestedLocales"] = json!(["ja", "en-US"]);
    let pretty = serde_json::to_string_pretty(&value).unwrap();
    for bytes in [
        source(&value),
        source(&reordered),
        Arc::from(pretty.as_bytes()),
        Arc::from(format!(" \r\n{}\t ", pretty.replace('\n', "\r\n")).into_bytes()),
    ] {
        let run = context.run(bytes, None);
        assert_eq!(run.steps, Step::ALL);
        let locales = locales(run);
        assert_eq!(locales.evaluation, baseline.evaluation);
        assert_eq!(
            locales.result.as_ref().unwrap().project,
            baseline.result.as_ref().unwrap().project
        );
    }
    let mut aliases = reordered.clone();
    aliases["profiles"]["app"]["requestedLocales"] = json!(["ja", "EN-us"]);
    aliases["profiles"]["app"]["defaultRequestedLocale"] = json!("EN-us");
    aliases["profiles"]["app"]["defaultSourceLocale"] = json!("EN");
    let aliased = locales(context.run(source(&aliases), None));
    assert_eq!(
        aliased.result.as_ref().unwrap().project,
        baseline.result.as_ref().unwrap().project
    );
    assert!(corrected(baseline.result.as_ref().unwrap()).is_empty());
    assert_eq!(baseline.evaluation.notices().count(), 0);
    assert_eq!(
        corrected(aliased.result.as_ref().unwrap()),
        [
            Location::RequestedLocale(1),
            Location::DefaultSourceLocale,
            Location::DefaultRequestedLocale
        ]
    );
    assert_eq!(aliased.evaluation.notices().count(), 3);
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
        let changed = locales(context.run(source(&changed), None));
        let project = changed.result.unwrap().project.unwrap();
        assert_ne!(
            Some(&project),
            baseline.result.as_ref().unwrap().project.as_ref().ok(),
            "{field}"
        );
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

    // The project's requested and default locales, and the browser
    // target's requested locale.
    for maximum in [2, 3] {
        let mut limits = FixtureLimits::finite();
        limits.locale_occurrences = Bound::new(maximum).unwrap();
        let run = FixtureRunner::new(limits).run(Arc::clone(&bytes), None);
        assert_eq!(run.steps, Step::ALL);
        let locales = locales(run);
        let resolved = locales.result.unwrap();
        if maximum == 2 {
            assert_eq!(
                issues(&locales.evaluation),
                [("occurrences".to_owned(), Code::ResourceLimitExceeded)]
            );
            let issue = locales.evaluation.issues().next().unwrap();
            let exceeded = locales.evaluation.limit(issue).unwrap();
            assert_eq!(
                (exceeded.bound.name, exceeded.limit.get(), exceeded.actual),
                ("maxLocaleOccurrences", 2, Actual::Exact(3))
            );
            // Every spelling is recorded, and none is canonicalized.
            assert!(locales.evaluation.statuses().all(|(unit, status)| {
                unit.subject == Subject::Locale(LocaleSubject::Occurrences)
                    || matches!(status, Status::NotEvaluated(_))
            }));
            assert_eq!(resolved.occurrences.len(), 3);
        } else {
            assert!(issues(&locales.evaluation).is_empty());
            assert_eq!(resolved.occurrences.len(), 3);
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
        retained = locales(context.run(bytes, None));
        blocked = context.run(source(&json!({"schemaVersion": "1"})), None);
        let raw_failure = context.run(Arc::from(&b"{"[..]), None);
        assert!(matches!(raw_failure.outcome, Outcome::Materialization(_)));
        let mut duplicate = minimal_config();
        duplicate["profiles"]["app"]["requestedLocales"] = json!(["en", "en"]);
        let failure = locales(context.run(source(&duplicate), None));
        assert!(!issues(&failure.evaluation).is_empty());
        let repeated = locales(context.run(source(&minimal_config()), None));
        assert_eq!(repeated, retained);
    }
    assert_eq!(
        retained
            .result
            .unwrap()
            .project
            .unwrap()
            .requested_default()
            .as_str(),
        "en"
    );
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
            let (expected, run) = worker.join().unwrap();
            assert_eq!(run.steps, Step::ALL);
            assert_eq!(run.selected.as_ref().unwrap().as_str(), "app");
            let resolved = locales(run).result.unwrap();
            let project = resolved.project.as_ref().unwrap();
            assert_eq!(project.requested_default().as_str(), expected);
            assert_eq!(names(project.requested()), [expected]);
            assert!(corrected(&resolved).is_empty());
        }
    });
}
