// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::sync::Arc;

use crate::benchmark::clock::{tests::ScriptedClock, ClockFailure};
use crate::benchmark::observation;
use crate::materialize::materialize_file;

use super::*;

fn capacity() -> CaptureCapacity {
    CaptureCapacity {
        warmup_repetitions: Quantity::new(8),
        samples: Repetitions::new(8).unwrap(),
        repetitions_per_sample: Repetitions::new(8).unwrap(),
        total_invocations: Repetitions::new(72).unwrap(),
    }
}

fn sampling(warmup: u64, samples: u64, repetitions: u64) -> Sampling {
    Sampling::admit(
        Quantity::new(warmup),
        Repetitions::new(samples).unwrap(),
        Repetitions::new(repetitions).unwrap(),
        capacity(),
    )
    .unwrap()
}

fn identity(value: &str) -> Digest {
    let mut frame = Frame::new("test-identity");
    frame.text(value);
    frame.finish()
}

fn binding() -> CaptureBinding {
    CaptureBinding {
        run: identity("test-run"),
        case: identity("file-null"),
    }
}

fn fixture() -> (Prepared, Observation) {
    let source: Arc<[u8]> = Arc::from(&b"null"[..]);
    let limits = crate::materialize_tests::limits();
    let document = materialize_file(Arc::clone(&source), limits).unwrap();
    assert!(matches!(
        document.node(document.root()).kind(),
        crate::materialize::NodeKind::Null
    ));
    let expected = observation::document(&document);
    (Prepared::Entry { source, limits }, expected)
}

#[test]
fn warmup_is_excluded_and_raw_sample_order_and_repetitions_are_retained() {
    let (prepared, expected) = fixture();
    // Two warmups (100,800), then two samples of two reps (3+5,7+11).
    let clock = ScriptedClock::nanos([
        0, 100, 200, 1000, 2000, 2003, 4000, 4005, 6000, 6007, 8000, 8011,
    ]);
    let policy = sampling(2, 2, 2);
    let capture = collect(&clock, &prepared, expected, policy, binding()).unwrap();
    assert_eq!(clock.remaining(), 0);
    assert_eq!(capture.warmup_completed, Quantity::new(2));
    assert_eq!(capture.samples.len(), 2);
    assert_eq!(capture.samples[0].aggregate_nanoseconds, Quantity::new(8));
    assert_eq!(capture.samples[1].aggregate_nanoseconds, Quantity::new(18));
    for (ordinal, sample) in capture.samples.iter().enumerate() {
        assert_eq!(sample.ordinal.get(), u64::try_from(ordinal).unwrap());
        assert_eq!(sample.repetition_count.get(), 2);
        assert_eq!(sample.semantic_observation, expected);
        assert_eq!(sample.determinism_proof, DurationProof::NotRequired);
        assert_eq!(sample.acquisition, Acquisition::Unpaired);
        let wire = serde_json::to_string(sample).unwrap();
        assert_eq!(
            serde_json::from_str::<CapturedSample>(&wire).unwrap(),
            *sample
        );
    }
    assert_ne!(
        capture.samples[0].local_identity,
        capture.samples[1].local_identity
    );
    assert_ne!(
        capture.samples[0].execution_identity,
        capture.samples[1].execution_identity
    );
    assert!(validate_capture(&capture, policy, binding(), expected).is_empty());
}

#[test]
fn zero_warmup_and_one_sample_are_valid_observations_not_a_numeric_gate() {
    let (prepared, expected) = fixture();
    let policy = sampling(0, 1, 1);
    let capture = collect(
        &ScriptedClock::nanos([1, 1]),
        &prepared,
        expected,
        policy,
        binding(),
    )
    .unwrap();
    assert_eq!(capture.warmup_completed.get(), 0);
    assert_eq!(capture.samples[0].aggregate_nanoseconds.get(), 0);
    assert!(validate_capture(&capture, policy, binding(), expected).is_empty());
}

#[test]
fn wrong_semantics_during_warmup_withholds_the_entire_case() {
    let (prepared, mut expected) = fixture();
    expected.shared = identity("deliberately-wrong-observation");
    let error = collect(
        &ScriptedClock::nanos([0, 1]),
        &prepared,
        expected,
        sampling(1, 1, 1),
        binding(),
    )
    .unwrap_err();
    assert!(matches!(
        error.cause,
        CaptureFailureCause::SemanticObservationMismatch(_)
    ));
    assert_eq!(error.stage, CaptureStage::Warmup);
    assert!(error.complete_sample_prefix.is_empty());
    assert_eq!(error.warmup_completed.get(), 0);
}

#[test]
fn a_later_failed_repetition_keeps_only_a_diagnostic_prefix_not_successful_evidence() {
    let (_, expected) = fixture();
    let mut calls = 0;
    let cause = CaptureFailureCause::SemanticObservationMismatch(Box::new(ObservationMismatch {
        expected,
        actual: Observation {
            shared: identity("wrong"),
            entry: None,
        },
    }));
    let error = collect_inner(true, sampling(0, 2, 2), binding(), expected, || {
        calls += 1;
        if calls == 4 {
            Err(cause.clone())
        } else {
            Ok(Quantity::new(5))
        }
    })
    .unwrap_err();
    assert_eq!(calls, 4);
    assert_eq!(error.cause, cause);
    assert_eq!(error.stage, CaptureStage::Measured);
    assert_eq!(error.complete_sample_prefix.len(), 1);
    assert_eq!(error.attempted_repetitions_in_sample.get(), 2);
    assert_eq!(error.completed_repetitions_in_sample.get(), 1);
    assert_eq!(error.accumulated_nanoseconds.get(), 5);
}

#[test]
fn clock_failure_is_not_an_empty_or_zero_successful_sample() {
    let (prepared, expected) = fixture();
    let clock = ScriptedClock::new([Err(ClockFailure::InvalidTimestamp)]);
    let error = collect(&clock, &prepared, expected, sampling(0, 1, 1), binding()).unwrap_err();
    assert_eq!(
        error.cause,
        CaptureFailureCause::Measurement(MeasurementFailure::Clock(ClockFailure::InvalidTimestamp))
    );
    assert!(error.complete_sample_prefix.is_empty());
    assert_eq!(error.attempted_repetitions_in_sample.get(), 1);
    assert_eq!(error.completed_repetitions_in_sample.get(), 0);
}

#[test]
fn aggregate_overflow_rejects_the_complete_sample_without_saturation() {
    let (_, expected) = fixture();
    let mut values = [Quantity::new(u64::MAX), Quantity::new(1)].into_iter();
    let error = collect_inner(true, sampling(0, 1, 2), binding(), expected, || {
        Ok(values.next().unwrap())
    })
    .unwrap_err();
    assert_eq!(error.cause, CaptureFailureCause::MeasurementOverflow);
    assert!(error.complete_sample_prefix.is_empty());
    assert_eq!(error.attempted_repetitions_in_sample.get(), 2);
    assert_eq!(error.completed_repetitions_in_sample.get(), 1);
    // This is explicitly the diagnostic sum of the completed valid prefix.
    assert_eq!(error.accumulated_nanoseconds.get(), u64::MAX);
}

#[test]
fn sample_capacity_and_total_arithmetic_are_admitted_before_execution_or_allocation() {
    let exact = Sampling::admit(
        Quantity::new(8),
        Repetitions::new(8).unwrap(),
        Repetitions::new(8).unwrap(),
        capacity(),
    )
    .unwrap();
    assert_eq!(exact.sample_capacity, 8);
    for (warmup, samples, repetitions) in [(9, 8, 8), (8, 9, 8), (8, 8, 9)] {
        assert_eq!(
            Sampling::admit(
                Quantity::new(warmup),
                Repetitions::new(samples).unwrap(),
                Repetitions::new(repetitions).unwrap(),
                capacity()
            )
            .err(),
            Some(SamplingError::CapacityExceeded)
        );
    }
    assert_eq!(
        Sampling::admit(
            Quantity::new(1),
            Repetitions::new(u64::MAX).unwrap(),
            Repetitions::new(1).unwrap(),
            capacity()
        )
        .err(),
        Some(SamplingError::InvocationCountOverflow)
    );
    assert_eq!(
        Sampling::admit(
            Quantity::new(0),
            Repetitions::new(u64::MAX).unwrap(),
            Repetitions::new(2).unwrap(),
            capacity()
        )
        .err(),
        Some(SamplingError::InvocationCountOverflow)
    );
}

#[test]
fn unavailable_preparation_runs_no_warmup_or_measurement() {
    let (_, expected) = fixture();
    let error = collect_inner(false, sampling(1, 1, 1), binding(), expected, || {
        panic!("must not invoke")
    })
    .unwrap_err();
    assert_eq!(error.cause, CaptureFailureCause::PrerequisiteUnavailable);
    assert_eq!(error.stage, CaptureStage::Preparation);
    assert!(error.complete_sample_prefix.is_empty());
}

#[test]
fn decoded_samples_are_revalidated_against_exact_binding_and_sampling_policy() {
    let (prepared, expected) = fixture();
    let policy = sampling(0, 2, 1);
    let mut capture = collect(
        &ScriptedClock::nanos([0, 3, 5, 8]),
        &prepared,
        expected,
        policy,
        binding(),
    )
    .unwrap();
    capture.warmup_completed = Quantity::new(1);
    capture.samples[0].ordinal = Quantity::new(9);
    capture.samples[0].repetition_count = Repetitions::new(2).unwrap();
    capture.samples[0].local_identity = identity("other-sample");
    capture.samples[0].execution_identity = identity("other-execution");
    capture.samples[0].semantic_observation.shared = identity("other-result");
    capture.samples.pop();
    let issues = validate_capture(&capture, policy, binding(), expected);
    assert_eq!(
        issues.iter().map(|issue| issue.kind).collect::<Vec<_>>(),
        [
            SampleIntegrityKind::WarmupCount,
            SampleIntegrityKind::SampleCount,
            SampleIntegrityKind::Ordinal,
            SampleIntegrityKind::Repetitions,
            SampleIntegrityKind::SampleIdentity,
            SampleIntegrityKind::ExecutionIdentity,
            SampleIntegrityKind::SemanticObservation,
        ]
    );
}

#[test]
fn cross_run_cross_case_and_reordered_samples_cannot_pass_revalidation() {
    let (prepared, expected) = fixture();
    let policy = sampling(0, 2, 1);
    let mut capture = collect(
        &ScriptedClock::nanos([0, 1, 2, 3]),
        &prepared,
        expected,
        policy,
        binding(),
    )
    .unwrap();
    for context in [
        CaptureBinding {
            run: identity("other-run"),
            ..binding()
        },
        CaptureBinding {
            case: identity("other-case"),
            ..binding()
        },
    ] {
        let issues = validate_capture(&capture, policy, context, expected);
        assert_eq!(issues.len(), 4);
        assert!(issues.iter().all(|issue| matches!(
            issue.kind,
            SampleIntegrityKind::SampleIdentity | SampleIntegrityKind::ExecutionIdentity
        )));
    }
    capture.samples.swap(0, 1);
    assert!(!validate_capture(&capture, policy, binding(), expected).is_empty());
}
