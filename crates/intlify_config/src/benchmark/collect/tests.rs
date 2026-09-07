// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

#![cfg(any(target_os = "linux", target_os = "macos"))]

use serde_json::{json, Value};

use crate::benchmark::clock::tests::ScriptedClock;
use crate::benchmark::observation::{Digest, Frame};
use crate::benchmark::operation::tests::operations;
use crate::benchmark::quantity::{Quantity, Repetitions};
use crate::benchmark::sample::{CaptureCapacity, CaptureFailureCause, CaptureStage};
use crate::fixtures::minimal_config;
use crate::structural::selection::Selection;

use super::*;

fn sampling() -> Sampling {
    Sampling::admit(
        Quantity::new(1),
        Repetitions::new(2).unwrap(),
        Repetitions::new(2).unwrap(),
        CaptureCapacity {
            warmup_repetitions: Quantity::new(1),
            samples: Repetitions::new(2).unwrap(),
            repetitions_per_sample: Repetitions::new(2).unwrap(),
            total_invocations: Repetitions::new(5).unwrap(),
        },
    )
    .unwrap()
}

fn id(value: &str) -> Digest {
    let mut frame = Frame::new("collector-test-binding");
    frame.text(value);
    frame.finish()
}

fn binding(operation: Operation) -> CaptureBinding {
    CaptureBinding {
        run: id("one-test-run"),
        case: id(operation.boundary()),
    }
}

// Unit-test fixture preparation, not the production registry's eventual oracle.
// The synthetic interval has no physical value; check known fixture semantics
// before deriving its observation. No measured sample supplies its own expected
// checksum, and the fixed fake duration is never included in collected samples.
fn fixture_observation(prepared: &Prepared) -> Observation {
    use crate::benchmark::operation::Output;
    let output = prepared.once(&ScriptedClock::nanos([0, 1])).unwrap().output;
    match &output {
        Output::Entry(Ok(doc)) => {
            assert_eq!(doc.decode::<Value>(doc.root()).unwrap(), minimal_config());
        }
        Output::Structural(Ok(analysis)) => assert!(analysis.is_complete()),
        Output::Authoring(Ok(Some(config))) => {
            assert_eq!(serde_json::to_value(config).unwrap(), minimal_config());
        }
        Output::Select(Ok(Selection::Selected(selected))) => {
            assert_eq!(selected.id().as_str(), "app");
            assert_eq!(
                serde_json::to_value(selected.resource_limits()).unwrap(),
                json!({"$testPolicy": "resource-limits"})
            );
        }
        _ => panic!("fixture preparation did not produce its known result"),
    }
    output.observe().unwrap()
}

#[test]
fn all_four_real_pairs_produce_revalidated_serializable_owner_fragments() {
    let clock = MonotonicClock::acquire().unwrap();
    for prepared in operations() {
        let operation = prepared.operation();
        let expected = fixture_observation(&prepared);
        let collected =
            collect_operation(&clock, &prepared, expected, sampling(), binding(operation)).unwrap();
        assert_eq!(collected.capture.warmup_completed.get(), 1);
        assert_eq!(collected.capture.samples.len(), 2);
        for sample in &collected.capture.samples {
            assert_eq!(sample.repetition_count.get(), 2);
            assert_eq!(sample.semantic_observation, expected);
        }
        let wire = serde_json::to_vec(&collected).unwrap();
        let decoded: CollectedOperation = serde_json::from_slice(&wire).unwrap();
        assert_eq!(decoded, collected);
        assert!(decoded
            .validate(
                operation,
                clock.description(),
                expected,
                sampling(),
                binding(operation)
            )
            .is_empty());
    }
}

#[test]
fn wrong_expected_semantics_returns_failure_not_a_serializable_success_prefix() {
    let clock = MonotonicClock::acquire().unwrap();
    let prepared = operations().into_iter().next().unwrap();
    let mut expected = fixture_observation(&prepared);
    expected.shared = id("deliberately-incorrect-fixture-observation");
    let error = collect_operation(
        &clock,
        &prepared,
        expected,
        sampling(),
        binding(prepared.operation()),
    )
    .unwrap_err();
    let CollectionFailure::Capture(failure) = error else {
        panic!("expected a capture failure");
    };
    assert_eq!(failure.stage, CaptureStage::Warmup);
    assert!(matches!(
        failure.cause,
        CaptureFailureCause::SemanticObservationMismatch(_)
    ));
    assert!(failure.complete_sample_prefix.is_empty());
}

#[test]
fn decoded_fragments_cannot_rebind_themselves_to_a_different_case_or_run() {
    let clock = MonotonicClock::acquire().unwrap();
    let prepared = operations().into_iter().next().unwrap();
    let operation = prepared.operation();
    let expected = fixture_observation(&prepared);
    let collected =
        collect_operation(&clock, &prepared, expected, sampling(), binding(operation)).unwrap();
    let other_case = collected.validate(
        Operation::AuthoringConstruction,
        clock.description(),
        expected,
        sampling(),
        binding(Operation::AuthoringConstruction),
    );
    assert!(other_case
        .iter()
        .any(|issue| matches!(issue, CollectionIssue::Descriptor(_))));
    assert!(other_case
        .iter()
        .any(|issue| matches!(issue, CollectionIssue::Sample(_))));
    let mut other_run = binding(operation);
    other_run.run = id("another-run");
    assert!(!collected
        .validate(
            operation,
            clock.description(),
            expected,
            sampling(),
            other_run
        )
        .is_empty());
}

#[test]
fn missing_unknown_or_tampered_record_parts_never_become_admitted_defaults() {
    let clock = MonotonicClock::acquire().unwrap();
    let prepared = operations().into_iter().next().unwrap();
    let operation = prepared.operation();
    let expected = fixture_observation(&prepared);
    let collected =
        collect_operation(&clock, &prepared, expected, sampling(), binding(operation)).unwrap();
    let original = serde_json::to_value(&collected).unwrap();
    for path in ["", "/capture", "/capture/samples/0"] {
        for key in original.pointer(path).unwrap().as_object().unwrap().keys() {
            let mut missing = original.clone();
            missing
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(key);
            assert!(serde_json::from_value::<CollectedOperation>(missing).is_err());
        }
        let mut extra = original.clone();
        extra
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), json!(true));
        assert!(serde_json::from_value::<CollectedOperation>(extra).is_err());
    }
    for (path, value) in [
        ("/capture/samples", json!([])),
        ("/capture/warmupCompleted", json!("0")),
        ("/descriptors/method/canonicalUnit", json!("millisecond")),
    ] {
        let mut tampered = original.clone();
        *tampered.pointer_mut(path).unwrap() = value;
        let decoded: CollectedOperation = serde_json::from_value(tampered).unwrap();
        assert!(!decoded
            .validate(
                operation,
                clock.description(),
                expected,
                sampling(),
                binding(operation)
            )
            .is_empty());
    }
}
