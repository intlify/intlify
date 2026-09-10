// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use super::*;
use crate::benchmark::clock::ClockFailure;
use crate::benchmark::collect::CollectionFailure;
use crate::benchmark::measure::MeasurementFailure;
use crate::benchmark::sample::{CaptureFailure, CaptureStage};
use serde_json::{json, Value};

fn recalculate(record: &mut OwnerRecord) {
    record.checksum = checksum("owner-run-result", &record.result).unwrap();
}

fn failed_clock_capture() -> ProfileCollectionFailure {
    ProfileCollectionFailure::Collection(CollectionFailure::Capture(Box::new(CaptureFailure {
        cause: CaptureFailureCause::Measurement(MeasurementFailure::Clock(
            ClockFailure::ReversedClock,
        )),
        stage: CaptureStage::Measured,
        warmup_completed: Quantity::new(1),
        complete_sample_prefix: Vec::new(),
        attempted_repetitions_in_sample: Quantity::new(1),
        completed_repetitions_in_sample: Quantity::new(0),
        accumulated_nanoseconds: Quantity::new(0),
    })))
}

#[test]
fn native_run_collects_all_planned_cases_and_roundtrips_as_one_bound_result() {
    let prepared = PreparedRun::acquire().unwrap();
    assert_eq!(prepared.plan.cases.len(), 127);
    assert_eq!(prepared.fixtures.len(), 127);
    assert!(prepared.fixtures.iter().all(Result::is_ok));
    let plan = prepared.plan.clone();
    let recorded = prepared.collect().unwrap();
    assert_eq!(recorded.record.result.plan, plan);
    assert_eq!(recorded.record.result.outcome, OwnerOutcome::Complete);
    assert_eq!(recorded.record.result.attempts.len(), 127);
    assert!(recorded.validate(&recorded.record).is_empty());
    let bytes = recorded.encode().unwrap();
    let storage = tempfile::tempdir().unwrap();
    let path = storage.path().join("owner-result.json");
    std::fs::write(&path, &bytes).unwrap();
    let decoded = recorded
        .decode_checked(&std::fs::read(path).unwrap())
        .unwrap();
    assert_eq!(decoded.document(), &recorded.record);
    assert!(bytes.len() < MAX_RECORD_BYTES);
    assert!(recorded
        .decode_checked(&serde_json::to_vec_pretty(&recorded.record).unwrap())
        .is_ok());
    drop(recorded);
    // Retained results do not borrow the fixture operations, context, or clock.
    assert_eq!(decoded.document().result.attempts.len(), 127);
    assert_eq!(serde_json::to_vec(decoded.document()).unwrap(), bytes);
}

#[test]
fn each_invocation_has_a_fresh_binding_without_changing_case_identity() {
    let first = PreparedRun::acquire().unwrap();
    let second = PreparedRun::acquire().unwrap();
    assert_ne!(first.plan.run, second.plan.run);
    assert_eq!(first.plan.cases, second.plan.cases);
    let first = first.collect().unwrap();
    let second = second.collect().unwrap();
    assert!(second.validate(&first.record).contains(&RunIssue::Plan));
    assert!(second
        .validate(&first.record)
        .contains(&RunIssue::RecordedObservation));
}

#[test]
fn inconsistent_prepared_inventory_stops_before_any_capture() {
    let mut prepared = PreparedRun::acquire().unwrap();
    prepared.fixtures.pop();
    let mut invoked = false;
    let result = prepared.collect_with(|context, ordinal, fixture, binding| {
        invoked = true;
        context.collect(ordinal, fixture, binding)
    });
    assert!(matches!(result, Err(RunFailure::PreparedInventory)));
    assert!(!invoked);
}

#[test]
fn missing_duplicate_reordered_unknown_or_rebound_attempts_are_not_complete_runs() {
    let recorded = PreparedRun::acquire().unwrap().collect().unwrap();
    for mutate in [
        |r: &mut OwnerRecord| {
            r.result.attempts.pop();
        },
        |r: &mut OwnerRecord| {
            r.result.attempts[1] = r.result.attempts[0].clone();
        },
        |r: &mut OwnerRecord| {
            r.result.attempts.swap(0, 1);
        },
        |r: &mut OwnerRecord| {
            r.result.attempts[0].ordinal = Quantity::new(127);
        },
        |r: &mut OwnerRecord| {
            r.result.attempts[0].binding.case = r.result.plan.run;
        },
    ] {
        let mut changed = recorded.record.clone();
        mutate(&mut changed);
        recalculate(&mut changed);
        let issues = recorded.validate(&changed);
        assert!(issues.contains(&RunIssue::RecordedObservation));
        assert!(issues
            .iter()
            .any(|issue| matches!(issue, RunIssue::AttemptCount | RunIssue::CaseBinding(_))));
    }
    for mutate in [
        |r: &mut OwnerRecord| {
            r.result.plan.cases.pop();
        },
        |r: &mut OwnerRecord| {
            r.result.plan.cases.swap(0, 1);
        },
    ] {
        let mut changed = recorded.record.clone();
        mutate(&mut changed);
        recalculate(&mut changed);
        assert!(recorded.validate(&changed).contains(&RunIssue::Plan));
    }
}

#[test]
fn self_rehashed_metadata_samples_and_status_do_not_replace_acquired_observations() {
    let recorded = PreparedRun::acquire().unwrap().collect().unwrap();
    let raw = serde_json::to_value(&recorded.record).unwrap();
    for (pointer, replacement) in [
        ("/result/recordIdentity/value", json!("0".repeat(64))),
        ("/result/recordIdentity/domain", json!("intlify-measurement-run-v0")),
        ("/result/plan/resultIdentity/value", json!("0".repeat(64))),
        ("/result/plan/commonRunPlan/value", json!("0".repeat(64))),
        ("/result/context/environment/clock/resolutionNanoseconds", json!("0")),
        ("/result/context/profile/sampling/measuredSamples", json!("2")),
        ("/result/context/build/package/revision", json!("changed")),
        ("/result/attempts/0/result/detail/operation/operation/capture/samples/0/aggregateNanoseconds", json!("18446744073709551615")),
        ("/result/outcome", json!("incomplete")),
    ] {
        let mut changed = raw.clone();
        *changed.pointer_mut(pointer).expect(pointer) = replacement;
        let mut changed: OwnerRecord = serde_json::from_value(changed).unwrap();
        recalculate(&mut changed);
        assert!(recorded.validate(&changed).contains(&RunIssue::RecordedObservation));
        assert!(recorded.decode_checked(&serde_json::to_vec(&changed).unwrap()).is_err());
    }
}

#[test]
fn collection_failure_retains_the_case_and_all_later_attempts_without_success_promotion() {
    let prepared = PreparedRun::acquire().unwrap();
    let mut visited = Vec::new();
    let recorded = prepared
        .collect_with(|context, ordinal, fixture, binding| {
            visited.push(ordinal);
            if ordinal.get() == 3 {
                return Err(failed_clock_capture());
            }
            context.collect(ordinal, fixture, binding)
        })
        .unwrap();
    assert_eq!(visited.len(), 127);
    assert_eq!(recorded.record.result.attempts.len(), 127);
    assert_eq!(recorded.record.result.outcome, OwnerOutcome::Incomplete);
    assert!(matches!(
        recorded.record.result.attempts[3].result,
        AttemptResult::CollectionFailed(_)
    ));
    assert!(matches!(
        recorded.record.result.attempts[126].result,
        AttemptResult::Measured(_)
    ));
    let decoded = recorded
        .decode_checked(&recorded.encode().unwrap())
        .unwrap();
    assert_eq!(decoded.document(), &recorded.record);
    let mut forged = recorded.record.clone();
    forged.result.outcome = OwnerOutcome::Complete;
    recalculate(&mut forged);
    assert!(recorded.validate(&forged).contains(&RunIssue::Outcome));
}

#[test]
fn preparation_failure_is_a_harness_failure_not_an_expected_configuration_failure() {
    let mut prepared = PreparedRun::acquire().unwrap();
    prepared.fixtures[2] = Err(FixtureFailure::InputContextMismatch);
    let mut visited = Vec::new();
    let recorded = prepared
        .collect_with(|context, ordinal, fixture, binding| {
            visited.push(ordinal);
            context.collect(ordinal, fixture, binding)
        })
        .unwrap();
    assert_eq!(visited.len(), 126);
    assert!(!visited.contains(&Quantity::new(2)));
    assert_eq!(recorded.record.result.attempts.len(), 127);
    assert_eq!(recorded.record.result.outcome, OwnerOutcome::Invalid);
    assert!(recorded.validate(&recorded.record).is_empty());
    let decoded = recorded
        .decode_checked(&recorded.encode().unwrap())
        .unwrap();
    assert!(matches!(
        decoded.document().result.attempts[2].result,
        AttemptResult::PreparationFailed(FixtureFailure::InputContextMismatch)
    ));
}

#[test]
fn invalid_benchmark_takes_precedence_over_operational_failure_in_either_order() {
    for reverse in [false, true] {
        let recorded = PreparedRun::acquire()
            .unwrap()
            .collect_with(|context, ordinal, fixture, binding| {
                let invalid = if reverse { 4 } else { 3 };
                let failed = if reverse { 3 } else { 4 };
                if ordinal.get() == invalid {
                    return Err(ProfileCollectionFailure::CaseSelection);
                }
                if ordinal.get() == failed {
                    return Err(failed_clock_capture());
                }
                context.collect(ordinal, fixture, binding)
            })
            .unwrap();
        assert_eq!(recorded.record.result.outcome, OwnerOutcome::Invalid);
        assert!(recorded.validate(&recorded.record).is_empty());
    }
}

#[test]
fn common_projection_keeps_failed_cases_diagnostic_and_invalid_precedence() {
    use crate::benchmark::shared::measurement::Outcome;
    use crate::benchmark::shared::pipeline;
    for invalid in [false, true] {
        let recorded = PreparedRun::acquire()
            .unwrap()
            .collect_with(|context, ordinal, fixture, binding| {
                if ordinal.get() == 3 {
                    return Err(failed_clock_capture());
                }
                if invalid && ordinal.get() == 4 {
                    return Err(ProfileCollectionFailure::CaseSelection);
                }
                context.collect(ordinal, fixture, binding)
            })
            .unwrap();
        let artifacts = pipeline::produce(&recorded).unwrap();
        let admitted = pipeline::validate(&recorded, &artifacts).unwrap();
        assert_eq!(
            admitted.outcome,
            if invalid {
                Outcome::Invalid
            } else {
                Outcome::Incomplete
            }
        );
        assert_eq!(admitted.planned_cases, 127);
        assert_eq!(admitted.non_measured_cases, if invalid { 2 } else { 1 });
        assert_eq!(admitted.measured_cases + admitted.non_measured_cases, 127);
        let evaluation: Value = serde_json::from_slice(&artifacts.evaluation).unwrap();
        let failed = &evaluation["body"]["cases"][3]["result"];
        assert_eq!(failed["kind"], "unavailable");
        assert_eq!(failed["unavailableKind"], "failed");
        assert_eq!(failed["reasons"][0]["code"]["code"], "failed-invocation");
        assert_eq!(failed["reasons"][0]["detail"]["subtype"], "clock-failure");
        assert_eq!(
            failed["diagnosticPartialObservations"][0]["reference"]["localRecordIdentity"],
            "attempt-3"
        );
        let evidence: Value = serde_json::from_slice(artifacts.evidence.as_ref().unwrap()).unwrap();
        assert!(evidence["body"]["cases"]
            .as_array()
            .unwrap()
            .iter()
            .all(|case| case["ownerAttempt"]["reference"]["localRecordIdentity"] != "attempt-3"));
    }
}

#[test]
fn closed_bounded_decode_rejects_extra_missing_duplicate_and_malformed_data() {
    let recorded = PreparedRun::acquire().unwrap().collect().unwrap();
    let raw = serde_json::to_value(&recorded.record).unwrap();
    for change in [
        |v: &mut Value| {
            v["extra"] = json!(true);
        },
        |v: &mut Value| {
            v["result"].as_object_mut().unwrap().remove("context");
        },
        |v: &mut Value| {
            v["result"]["attempts"][0]["result"]["kind"] = json!("skipped");
        },
    ] {
        let mut changed = raw.clone();
        change(&mut changed);
        assert!(matches!(
            recorded.decode_checked(&serde_json::to_vec(&changed).unwrap()),
            Err(DecodeFailure::Malformed)
        ));
    }
    for bytes in [
        b"{\"checksum\":0,\"checksum\":1}".as_slice(),
        b"\xff",
        b"{} trailing",
    ] {
        assert!(matches!(
            recorded.decode_checked(bytes),
            Err(DecodeFailure::Malformed)
        ));
    }
    let encoded = String::from_utf8(recorded.encode().unwrap()).unwrap();
    let duplicate = format!(
        "{{\"checksum\":{},{}",
        serde_json::to_string(&recorded.record.checksum).unwrap(),
        &encoded[1..]
    );
    assert!(matches!(
        recorded.decode_checked(duplicate.as_bytes()),
        Err(DecodeFailure::Malformed)
    ));
    assert!(matches!(
        recorded.decode_checked(&vec![b' '; MAX_RECORD_BYTES + 1]),
        Err(DecodeFailure::SizeLimit)
    ));
    // Exact bound reaches JSON validation; it is not rejected as oversized.
    assert!(matches!(
        recorded.decode_checked(&vec![b' '; MAX_RECORD_BYTES]),
        Err(DecodeFailure::Malformed)
    ));
}

#[test]
fn failure_reasons_and_progress_are_validated_against_the_selected_profile() {
    for failure in [
        ProfileCollectionFailure::Collection(CollectionFailure::Descriptor(Vec::new())),
        ProfileCollectionFailure::Collection(CollectionFailure::Integrity(Vec::new())),
        {
            let mut failure = failed_clock_capture();
            if let ProfileCollectionFailure::Collection(CollectionFailure::Capture(capture)) =
                &mut failure
            {
                capture.attempted_repetitions_in_sample = Quantity::new(2);
            }
            failure
        },
    ] {
        let recorded = PreparedRun::acquire()
            .unwrap()
            .collect_with(|context, ordinal, fixture, binding| {
                if ordinal.get() == 3 {
                    return Err(failure.clone());
                }
                context.collect(ordinal, fixture, binding)
            })
            .unwrap();
        assert!(recorded
            .validate(&recorded.record)
            .iter()
            .any(|issue| matches!(
                issue, RunIssue::Operation { ordinal, .. } if ordinal.get() == 3
            )));
        assert!(recorded
            .decode_checked(&recorded.encode().unwrap())
            .is_err());
    }
}

#[test]
fn semantic_mismatch_retains_both_observations_and_is_never_an_operational_success() {
    use crate::benchmark::sample::ObservationMismatch;
    for valid_reason in [true, false] {
        let recorded = PreparedRun::acquire()
            .unwrap()
            .collect_with(|context, ordinal, fixture, binding| {
                if ordinal.get() == 3 {
                    let expected = fixture.expected();
                    let mut actual = expected;
                    if valid_reason {
                        actual.shared = Frame::new("different-test-result").finish();
                    }
                    let mut failure = failed_clock_capture();
                    if let ProfileCollectionFailure::Collection(CollectionFailure::Capture(
                        capture,
                    )) = &mut failure
                    {
                        capture.cause = CaptureFailureCause::SemanticObservationMismatch(Box::new(
                            ObservationMismatch { expected, actual },
                        ));
                    }
                    return Err(failure);
                }
                context.collect(ordinal, fixture, binding)
            })
            .unwrap();
        assert_eq!(recorded.record.result.outcome, OwnerOutcome::Invalid);
        assert_eq!(recorded.validate(&recorded.record).is_empty(), valid_reason);
        assert_eq!(
            recorded.decode_checked(&recorded.encode().unwrap()).is_ok(),
            valid_reason
        );
    }
}

#[test]
fn a_failed_multi_sample_prefix_roundtrips_but_is_not_a_successful_capture() {
    use crate::benchmark::clock::{Clock, Tick};
    use crate::benchmark::quantity::Repetitions;
    use crate::benchmark::sample::{self, Capture, CaptureCapacity, Sampling};
    use std::cell::Cell;

    struct FailSecondSample(Cell<u64>);
    impl Clock for FailSecondSample {
        fn read(&self) -> Result<Tick, ClockFailure> {
            let ordinal = self.0.get();
            self.0.set(ordinal + 1);
            if ordinal >= 2 {
                return Err(ClockFailure::ReversedClock);
            }
            Tick::new(0, i64::try_from(ordinal).unwrap())
        }
    }
    let registry = Registry::load().unwrap();
    let fixture = registry
        .prepare(registry.declarations().next().unwrap())
        .unwrap();
    let one = Repetitions::new(1).unwrap();
    let two = Repetitions::new(2).unwrap();
    let sampling = Sampling::admit(
        Quantity::new(0),
        two,
        one,
        CaptureCapacity {
            warmup_repetitions: Quantity::new(0),
            samples: two,
            repetitions_per_sample: one,
            total_invocations: two,
        },
    )
    .unwrap();
    let binding = CaptureBinding {
        run: Frame::new("diagnostic-prefix-test-run").finish(),
        case: Frame::new("diagnostic-prefix-test-case").finish(),
    };
    let failure = sample::collect_with_work(
        &FailSecondSample(Cell::new(0)),
        fixture.prepared(),
        fixture.expected(),
        fixture.expected_work(),
        sampling,
        binding,
    )
    .unwrap_err();
    assert_eq!(failure.complete_sample_prefix.len(), 1);
    assert!(sample::validate_failure(
        &failure,
        sampling,
        binding,
        fixture.expected(),
        fixture.expected_work()
    )
    .is_empty());
    let decoded: CaptureFailure =
        serde_json::from_slice(&serde_json::to_vec(&failure).unwrap()).unwrap();
    assert_eq!(*failure, decoded);
    let fabricated_success = Capture {
        warmup_completed: decoded.warmup_completed,
        samples: decoded.complete_sample_prefix,
    };
    assert!(
        !sample::validate_capture(&fabricated_success, sampling, binding, fixture.expected())
            .is_empty()
    );
}
