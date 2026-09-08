// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::sync::Arc;

use serde_json::{json, Value};

use crate::benchmark::clock::{tests::ScriptedClock, MonotonicClock};
use crate::benchmark::quantity::Quantity;
use crate::fixtures::minimal_config;
use crate::input_limits::Bound;
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

pub(crate) fn operations() -> [Prepared; 5] {
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
                    &crate::locale::fixtures::fixture_binding(),
                    Some(crate::locale::fixtures::FixtureProvider::new()),
                    Bound::new(128).unwrap(),
                )
                .unwrap(),
            ),
            input: Arc::from("EN-us"),
        },
    ]
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
                    json!({"$testPolicy":"resource-limits"})
                );
            }
            Output::Locale(Ok(result)) => {
                assert_eq!(result.locale().as_str(), "en-US");
                assert_eq!(result.suggested_replacement(), Some("en-US"));
            }
            _ => panic!("unexpected owner operation result"),
        }
        // Encoding requires no additional clock read and does not destroy output.
        let observed = measured.output.observe().unwrap();
        assert_eq!(observed, measured.output.observe().unwrap());
        assert_eq!(clock.remaining(), 0);
    }
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
