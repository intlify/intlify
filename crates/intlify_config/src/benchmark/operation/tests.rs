// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::sync::Arc;

use serde_json::{json, Value};

use crate::benchmark::clock::{tests::ScriptedClock, MonotonicClock};
use crate::benchmark::quantity::Quantity;
use crate::input_limits::Bound;
use crate::profile_fixtures::{minimal_config, reference};
use crate::structural::StructuralFailure;

use super::*;

fn capacity() -> StructuralLimits {
    StructuralLimits {
        max_profiles: Bound::new(100).unwrap(),
        max_profile_id_bytes: Bound::new(256).unwrap(),
        max_structural_analysis_units: Bound::new(1_000_000).unwrap(),
    }
}

fn analysis(value: &Value) -> Analysis {
    Schema::for_model()
        .unwrap()
        .analyze(
            Arc::new(
                materialize_file(
                    Arc::from(serde_json::to_vec(value).unwrap()),
                    crate::materialize_tests::limits(),
                )
                .unwrap(),
            ),
            capacity(),
        )
        .unwrap()
}

pub(crate) fn operations() -> [Prepared; 7] {
    let source: Arc<[u8]> = Arc::from(serde_json::to_vec(&minimal_config()).unwrap());
    let doc = Arc::new(
        materialize_file(Arc::clone(&source), crate::materialize_tests::limits()).unwrap(),
    );
    [
        Prepared::Entry {
            source,
            limits: crate::materialize_tests::limits(),
        },
        Prepared::Structural {
            schema: Schema::for_model().unwrap(),
            doc,
            limits: capacity(),
        },
        Prepared::Authoring(analysis(&minimal_config())),
        Prepared::Select {
            analysis: analysis(&minimal_config()),
            selector: SelectorInput::absent(capacity().max_profile_id_bytes),
        },
        Prepared::Locale {
            core: Arc::new(
                crate::locale::Canonicalizer::bind(
                    &crate::locale::fixtures::fixture_pins(),
                    Some(crate::locale::fixtures::FixtureProvider::new()),
                    Bound::new(128).unwrap(),
                )
                .unwrap(),
            ),
            input: Arc::from("EN-us"),
        },
        resolution_operation(&minimal_config()),
        policy_operation(),
    ]
}

/// Phase 4 over the fixture's policy alone, as its case prepares it.
pub(crate) fn policy_operation() -> Prepared {
    use crate::benchmark::cases::{declarations, Recipe, ResourcePolicyRecipe};

    let declaration = declarations()
        .into_iter()
        .find(|case| {
            case.fixture == Recipe::ResourcePolicy(ResourcePolicyRecipe::PolicyOnly)
                && case.limit.is_none()
        })
        .unwrap();
    crate::benchmark::cases::prepare::prepare(&declaration)
        .unwrap()
        .prepared
}

pub(crate) fn resolution_operation(value: &Value) -> Prepared {
    let analysis = analysis(value);
    let config = analysis.construct().unwrap().unwrap();
    let bound = |value| Bound::new(value).unwrap();
    Prepared::LocaleResolution(Box::new(locale_resolution::PreparedLocales {
        analysis,
        config,
        selected: serde_json::from_value(json!("app")).unwrap(),
        policy: crate::resolver::locales::fixture_policy(bound(128), bound(128), bound(64)),
        canonicalizer: locale_resolution::Canonicalizers::Finite(Arc::new(
            crate::locale::Canonicalizer::bind(
                &crate::locale::fixtures::fixture_pins(),
                Some(crate::locale::fixtures::FixtureProvider::new()),
                bound(128),
            )
            .unwrap(),
        )),
    }))
}

#[test]
fn every_active_pair_calls_its_real_core_operation_between_exact_markers() {
    for (expected, prepared) in Operation::ALL.into_iter().zip(operations()) {
        assert_eq!(prepared.operation(), expected);
        assert!(prepared.prerequisites_admitted());
        assert!(!expected.phase().is_empty());
        assert!(!expected.cost().is_empty());
        assert!(!expected.boundary().is_empty());
        let clock = ScriptedClock::nanos([100, 137]);
        let measured = prepared.once(&clock).unwrap();
        assert_eq!(measured.duration, Quantity::new(37));
        assert_eq!(clock.remaining(), 0);
        match &measured.output {
            Output::Entry(Ok(doc)) => {
                let value: Value = doc.decode(doc.root()).unwrap();
                assert_eq!(value, minimal_config());
            }
            Output::Structural(Ok(result)) => assert!(result.is_complete()),
            Output::Authoring(Ok(Some(config))) => {
                assert_eq!(serde_json::to_value(config).unwrap(), minimal_config());
            }
            Output::Select(Ok(Selection::Selected(selected))) => {
                assert_eq!(selected.id().as_str(), "app");
                assert_eq!(
                    serde_json::to_value(selected.resource_limits()).unwrap(),
                    reference("resource-limit-policy")
                );
            }
            Output::Locale(Ok(result)) => {
                assert_eq!(result.locale().as_str(), "en-US");
                assert_eq!(result.suggested_replacement(), Some("en-US"));
            }
            Output::LocaleResolution(resolved) => {
                assert_eq!(resolved.evaluation.issues().count(), 0);
                let locales = resolved.result.as_ref().unwrap();
                let project = locales.project.as_ref().unwrap();
                assert_eq!(project.source_default(), None);
                assert_eq!(
                    project
                        .requested()
                        .as_slice()
                        .iter()
                        .map(crate::locale::CanonicalLocale::as_str)
                        .collect::<Vec<_>>(),
                    ["en"]
                );
                assert_eq!(project.requested_default().as_str(), "en");
            }
            Output::ResourcePolicy(resolved) => {
                assert_eq!(resolved.evaluation.issues().count(), 0);
                assert!(resolved.result.as_ref().unwrap().confirmed.is_ok());
            }
            _ => panic!("unexpected owner operation result"),
        }
        // Encoding requires no additional clock read and does not destroy output.
        let observed = measured.output.observe().unwrap();
        assert_eq!(observed, measured.output.observe().unwrap());
        assert_eq!(clock.remaining(), 0);
    }
}

fn icu4x_operation(engine: Engine, input: &str) -> Prepared {
    Prepared::Icu4xLocale {
        engine: locale_icu4x::PreparedEngine::prepare(engine, Bound::new(128).unwrap()).unwrap(),
        input: Arc::from(input),
    }
}

#[test]
fn both_icu4x_engines_run_the_canonicalization_boundary_between_the_same_markers() {
    for engine in [Engine::Reused, Engine::Fresh] {
        let prepared = icu4x_operation(engine, "iw-IL");
        assert_eq!(prepared.operation(), Operation::LocaleCanonicalization);
        assert_eq!(prepared.icu4x_engine(), Some(engine));
        assert!(prepared.prerequisites_admitted());
        // A fresh engine is admitted before the start read, so the two reads
        // still enclose only the canonicalization.
        let clock = ScriptedClock::nanos([100, 137]);
        let measured = prepared.once(&clock).unwrap();
        assert_eq!(measured.duration, Quantity::new(37));
        assert_eq!(clock.remaining(), 0);
        let Output::Locale(Ok(result)) = &measured.output else {
            panic!("unexpected canonicalization result")
        };
        assert_eq!(result.locale().as_str(), "he-IL");
        assert_eq!(result.suggested_replacement(), Some("he-IL"));
    }
    assert!(operations()
        .iter()
        .all(|prepared| prepared.icu4x_engine().is_none()));
}

#[test]
fn a_fresh_engine_that_binds_otherwise_fails_before_the_start_read() {
    let Prepared::Icu4xLocale {
        engine: locale_icu4x::PreparedEngine::Fresh { mut binding, limit },
        input,
    } = icu4x_operation(Engine::Fresh, "en-US")
    else {
        unreachable!()
    };
    binding.pins.dataset.semantic_digest = format!("sha256:{}", "0".repeat(64));
    let prepared = Prepared::Icu4xLocale {
        engine: locale_icu4x::PreparedEngine::Fresh { binding, limit },
        input,
    };
    let clock = ScriptedClock::nanos([0, 1]);
    assert!(matches!(
        prepared.once(&clock),
        Err(MeasurementFailure::PrerequisiteUnavailable)
    ));
    assert_eq!(clock.remaining(), 2);
}

fn icu4x_resolution_operation(engine: Engine) -> Prepared {
    let mut operation = resolution_operation(&minimal_config());
    let Prepared::LocaleResolution(prepared) = &mut operation else {
        unreachable!()
    };
    prepared.canonicalizer = locale_resolution::Canonicalizers::Icu4x(
        locale_icu4x::PreparedEngine::prepare(engine, Bound::new(128).unwrap()).unwrap(),
    );
    operation
}

#[test]
fn both_icu4x_engines_run_phase_7_between_the_same_markers_as_the_finite_provider() {
    let finite = resolution_operation(&minimal_config())
        .once(&ScriptedClock::nanos([0, 1]))
        .unwrap()
        .output
        .observe()
        .unwrap();
    for engine in [Engine::Reused, Engine::Fresh] {
        let prepared = icu4x_resolution_operation(engine);
        assert_eq!(prepared.operation(), Operation::LocaleResolution);
        assert_eq!(prepared.icu4x_engine(), Some(engine));
        assert!(prepared.prerequisites_admitted());
        // A fresh engine is admitted before the start read, so the two reads
        // still enclose only Phase 7.
        let clock = ScriptedClock::nanos([100, 137]);
        let measured = prepared.once(&clock).unwrap();
        assert_eq!(measured.duration, Quantity::new(37));
        assert_eq!(clock.remaining(), 0);
        assert_eq!(measured.output.observe().unwrap(), finite);
    }
}

#[test]
fn a_fresh_phase_7_engine_that_binds_otherwise_fails_before_the_start_read() {
    let mut operation = icu4x_resolution_operation(Engine::Fresh);
    let Prepared::LocaleResolution(prepared) = &mut operation else {
        unreachable!()
    };
    let locale_resolution::Canonicalizers::Icu4x(locale_icu4x::PreparedEngine::Fresh {
        binding,
        ..
    }) = &mut prepared.canonicalizer
    else {
        unreachable!()
    };
    binding.pins.dataset.semantic_digest = format!("sha256:{}", "0".repeat(64));
    let clock = ScriptedClock::nanos([0, 1]);
    assert!(matches!(
        operation.once(&clock),
        Err(MeasurementFailure::PrerequisiteUnavailable)
    ));
    assert_eq!(clock.remaining(), 2);
}

#[test]
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn real_clock_runs_all_active_pairs_without_numeric_performance_gates() {
    let clock = MonotonicClock::acquire().unwrap();
    for prepared in operations() {
        let first = prepared.once(&clock).unwrap();
        let repeated = prepared.once(&clock).unwrap();
        assert_eq!(
            first.output.observe().unwrap(),
            repeated.output.observe().unwrap()
        );
        // Values are retained but deliberately not compared to a speed threshold.
        let _durations = [first.duration, repeated.duration];
    }
}

#[test]
fn expected_blocked_operations_can_have_valid_completed_observations() {
    let entry = Prepared::Entry {
        source: Arc::from(&b"{bad"[..]),
        limits: crate::materialize_tests::limits(),
    };
    let structural = Prepared::Structural {
        schema: Schema::for_model().unwrap(),
        doc: Arc::new(
            materialize_file(Arc::from(&b"{}"[..]), crate::materialize_tests::limits()).unwrap(),
        ),
        limits: capacity(),
    };
    let select = Prepared::Select {
        analysis: analysis(&minimal_config()),
        selector: SelectorInput::string("unknown", capacity().max_profile_id_bytes),
    };
    for prepared in [entry, structural, select] {
        let measured = prepared.once(&ScriptedClock::nanos([0, 1])).unwrap();
        match &measured.output {
            Output::Entry(Err(error)) => assert!(matches!(
                error.reason,
                crate::materialize::InputFailure::Syntax
            )),
            Output::Structural(Ok(result)) => assert_eq!(
                result.issues()[0].reason,
                StructuralFailure::SchemaVersionMissing
            ),
            Output::Select(Ok(Selection::Rejected(reason))) => {
                assert_eq!(*reason, SelectionFailure::Unknown { bytes: 7 });
            }
            _ => panic!("expected a completed blocked operation"),
        }
        assert!(measured.output.observe().is_ok());
    }
}

#[test]
fn unavailable_authoring_prerequisites_do_not_produce_a_fake_construction_observation() {
    let prepared = Prepared::Authoring(analysis(&json!({})));
    assert!(!prepared.prerequisites_admitted());
    // The future case evaluator must not run this boundary. Even a misuse cannot
    // turn a missing root into a checksum for successfully constructed config.
    let result = prepared.once(&ScriptedClock::nanos([0, 1])).unwrap();
    assert_eq!(
        result.output.observe(),
        Err(OutputFailure::MissingCompleteRoot)
    );
}

#[test]
fn locale_resolution_requires_complete_preparation_and_declared_selection_before_clock_markers() {
    for missing_profile in [false, true] {
        let mut operation = resolution_operation(&minimal_config());
        let Prepared::LocaleResolution(prepared) = &mut operation else {
            unreachable!()
        };
        if missing_profile {
            prepared.selected = serde_json::from_value(json!("missing")).unwrap();
        } else {
            prepared.analysis = analysis(&json!({}));
        }
        assert!(!operation.prerequisites_admitted());
        let clock = ScriptedClock::nanos([0, 1]);
        assert!(matches!(
            operation.once(&clock),
            Err(MeasurementFailure::PrerequisiteUnavailable)
        ));
        assert_eq!(clock.remaining(), 2);
    }
}

#[test]
fn member_permutations_preserve_shared_output_but_not_source_observations() {
    let value = minimal_config();
    let mut reordered = value.clone();
    let app = value["profiles"]["app"].as_object().unwrap();
    reordered["profiles"]["app"] = Value::Object(
        app.iter()
            .rev()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    );
    let first = analysis(&value).benchmark_observation();
    let second = analysis(&reordered).benchmark_observation();
    assert_eq!(first.shared, second.shared);
    assert_ne!(first.entry, second.entry);
    let first = Output::Authoring(analysis(&value).construct())
        .observe()
        .unwrap();
    let second = Output::Authoring(analysis(&reordered).construct())
        .observe()
        .unwrap();
    assert_eq!(first, second);
}

#[test]
fn semantic_mutations_change_complete_authoring_and_structural_observations() {
    let original = minimal_config();
    let baseline = analysis(&original).benchmark_observation();
    let typed = Output::Authoring(analysis(&original).construct())
        .observe()
        .unwrap();
    for (field, changed) in [
        ("projectId", json!("changed")),
        ("requestedLocales", json!(["ja"])),
        ("defaultRequestedLocale", json!("ja")),
    ] {
        let mut value = original.clone();
        value["profiles"]["app"][field] = changed;
        assert_ne!(
            analysis(&value).benchmark_observation().shared,
            baseline.shared
        );
        assert_ne!(
            Output::Authoring(analysis(&value).construct())
                .observe()
                .unwrap()
                .shared,
            typed.shared
        );
    }
}

#[test]
fn unknown_and_rejected_selectors_never_enter_checksums_by_their_raw_content() {
    let bound = capacity().max_profile_id_bytes;
    let analyzed = analysis(&minimal_config());
    for (a, b) in [
        ("secret-a", "secret-b"),
        ("BAD", "NEW"),
        (&"a".repeat(257), &"b".repeat(2000)),
    ] {
        let a = analyzed.select(&SelectorInput::string(a, bound)).unwrap();
        let b = analyzed.select(&SelectorInput::string(b, bound)).unwrap();
        assert_eq!(
            observe_selection(&a).unwrap(),
            observe_selection(&b).unwrap()
        );
    }
}

#[test]
fn document_codec_is_iterative_and_preserves_normalized_numeric_semantics() {
    let input_limits = crate::materialize_tests::limits();
    let a = materialize_file(Arc::from(&b"[-0]"[..]), input_limits).unwrap();
    let b = materialize_file(Arc::from(&b"[0]"[..]), input_limits).unwrap();
    assert_eq!(
        observation::document(&a).shared,
        observation::document(&b).shared
    );
    assert_ne!(
        observation::document(&a).entry,
        observation::document(&b).entry
    );
    let raw = format!("{}null{}", "[".repeat(20_000), "]".repeat(20_000));
    let deep = materialize_file(Arc::from(raw.into_bytes()), input_limits).unwrap();
    assert_eq!(observation::document(&deep), observation::document(&deep));
}

#[test]
fn checksum_codec_has_canonical_encoding_and_unambiguous_framing() {
    let mut a = Frame::new("framing");
    a.text("ab");
    a.text("c");
    let mut b = Frame::new("framing");
    b.text("a");
    b.text("bc");
    assert_ne!(a.finish(), b.finish());
    let result = observation::document(
        &materialize_file(Arc::from(&b"null"[..]), crate::materialize_tests::limits()).unwrap(),
    );
    let encoded = serde_json::to_string(&result).unwrap();
    assert_eq!(
        serde_json::from_str::<Observation>(&encoded).unwrap(),
        result
    );
    assert!(serde_json::from_str::<observation::Digest>("\"ABCDEF\"").is_err());
    assert!(
        serde_json::from_str::<observation::Digest>(&format!("\"{}\"", "A".repeat(64))).is_err()
    );
    assert!(serde_json::from_str::<observation::Digest>("17").is_err());
}
