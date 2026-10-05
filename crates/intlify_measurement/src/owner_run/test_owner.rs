// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! A minimal owner, so the shared run can be driven end to end here.
//!
//! It measures counting the characters of a fixed text. That is not a useful
//! operation, which is the point: everything these tests show belongs to the
//! scaffolding rather than to an owner's work.

use intlify_shared_json::quantity::Quantity;
use serde::{Deserialize, Serialize};

use super::capture::{Capture, CaptureFailure, Capturing, Observed};
use super::observation::{Framing, Observation};
use super::run::{Expected, PreparationFailure};
use super::{Case, Labels, Owner, Package, Versioned};
use crate::acquisition::Clock;
use crate::environment::ClockObservation;
use crate::execution::{Execution, OutputBuffer};
use crate::owner::ObservedDescriptors;
use crate::plan::CaseProjection;

pub(super) const LABELS: Labels = Labels {
    owner: "intlify-measurement-test",
    framing: Framing::new("intlify-measurement-test-observation/0"),
    plan_codec: "intlify-measurement-test-owner-run-plan/1",
    result_codec: "intlify-measurement-test-owner-run-result/1",
    result_domain: "intlify-measurement-test-result-v1",
    runner_domain: "intlify-measurement-test-runner-instance-v0",
    build_schema: "intlify-measurement-test-build-observation/0",
    subject: "intlify-measurement-test-subject",
    profile: Versioned::new("intlify-measurement-test-smoke", "0"),
    harness: Versioned::new("intlify-measurement-test-owner-run-harness", "1"),
    projection: Versioned::new("intlify-measurement-test-to-026", "0"),
    native_rule: Versioned::new("intlify-measurement-test-native-component-context", "0"),
    memory_rule: Versioned::new("intlify-measurement-test-duration-only", "0"),
    instrumentation: Versioned::new("intlify-measurement-test-instrumentation", "0"),
    concurrency: Versioned::new("intlify-measurement-test-concurrency", "0"),
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Fixture {
    name: &'static str,
    text: &'static str,
    path: Expected,
}

impl Case for Fixture {
    fn name(&self) -> &'static str {
        self.name
    }
    fn path(&self) -> Expected {
        self.path
    }
}

const FIXTURES: [Fixture; 3] = [
    Fixture {
        name: "ascii",
        text: "Pay now",
        path: Expected::Complete,
    },
    Fixture {
        name: "multibyte",
        text: "日本語",
        path: Expected::Complete,
    },
    Fixture {
        name: "empty",
        text: "",
        path: Expected::Blocked,
    },
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Work {
    bytes: Quantity,
    characters: Quantity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Projection {
    owner: String,
    phase: String,
    cost: String,
    fixture: String,
    path: Expected,
    workload: Work,
}

impl CaseProjection for Projection {
    fn phase(&self) -> &str {
        &self.phase
    }
    fn cost(&self) -> &str {
        &self.cost
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Descriptors {
    clock: ClockObservation,
    execution: Execution,
}

impl ObservedDescriptors for Descriptors {
    fn execution(&self) -> &Execution {
        &self.execution
    }
}

pub(super) struct Prepared {
    fixture: Fixture,
    expected: Observed<Work>,
}

fn invoke(text: &'static str) -> Option<usize> {
    (!text.is_empty()).then(|| text.chars().count())
}

thread_local! {
    static DRIFT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The same operation, except that each call counts one more character, even
/// in an empty text.
fn drifting(text: &'static str) -> Option<usize> {
    DRIFT.with(|drift| {
        drift.set(drift.get() + 1);
        Some(text.chars().count() + drift.get())
    })
}

#[allow(
    clippy::ref_option,
    reason = "the capture contract fixes this signature"
)]
fn observe(output: &Option<usize>, text: &'static str) -> Observed<Work> {
    let mut semantic = LABELS.framing.frame("character-count");
    semantic.flag(output.is_some());
    semantic.uint(output.unwrap_or(0) as u64);
    Observed {
        observation: Observation {
            semantic: semantic.finish(),
            positions: None,
        },
        work: Work {
            bytes: Quantity::new(text.len() as u64),
            characters: Quantity::new(output.unwrap_or(0) as u64),
        },
        complete: output.is_some(),
    }
}

/// The same operation, failing every time it is measured.
fn failing(text: &'static str) -> Option<usize> {
    panic!("a measured invocation of {} bytes failed", text.len())
}

/// How the test owner misbehaves, if at all.
pub(super) const SOUND: u8 = 0;
/// The blocked fixture cannot be prepared.
pub(super) const UNPREPARED: u8 = 1;
/// Every measured invocation disagrees with the expectation.
pub(super) const DRIFTING: u8 = 2;
/// Every measured invocation fails, so no case has a sample.
pub(super) const FAILING: u8 = 3;

pub(super) struct TestOwner<const MODE: u8>;

impl<const MODE: u8> Owner for TestOwner<MODE> {
    const LABELS: Labels = LABELS;
    const PACKAGE: Package = Package {
        name: env!("CARGO_PKG_NAME"),
        version: env!("CARGO_PKG_VERSION"),
        assertions: cfg!(debug_assertions),
    };

    type Fixture = Fixture;
    type Prepared = Prepared;
    type Projection = Projection;
    type Descriptors = Descriptors;
    type Work = Work;

    fn fixtures() -> &'static [Fixture] {
        &FIXTURES
    }

    fn prepare(fixture: Fixture) -> Result<Prepared, PreparationFailure> {
        if MODE == UNPREPARED && fixture.path == Expected::Blocked {
            return Err(PreparationFailure::Fixture);
        }
        Ok(Prepared {
            fixture,
            expected: observe(&invoke(fixture.text), fixture.text),
        })
    }

    fn expected(prepared: &Prepared) -> &Observed<Work> {
        &prepared.expected
    }

    fn project(prepared: &Prepared) -> Projection {
        Projection {
            owner: LABELS.owner.into(),
            phase: "test_phase".into(),
            cost: "character_count".into(),
            fixture: prepared.fixture.name.into(),
            path: prepared.fixture.path,
            workload: prepared.expected.work.clone(),
        }
    }

    fn descriptors(_: &Prepared, clock: ClockObservation) -> Descriptors {
        Descriptors {
            clock,
            execution: Execution {
                process_state: "reused-process".into(),
                engine_state: "reused".into(),
                initial_preparation_state: "resident".into(),
                cache_state: "disabled".into(),
                runtime_compilation_state: "ahead-of-time".into(),
                managed_heap_state: "not-applicable".into(),
                scratch_reuse_state: "not-applicable".into(),
                output_buffer_state: OutputBuffer::NotApplicable {},
            },
        }
    }

    fn capture<C: Clock>(
        capturing: &Capturing<'_, C>,
        prepared: &Prepared,
    ) -> Result<Capture<Work>, CaptureFailure> {
        let operation = match MODE {
            DRIFTING => drifting,
            FAILING => failing,
            _ => invoke,
        };
        capturing.capture(
            &prepared.expected,
            prepared.fixture.text,
            operation,
            observe,
        )
    }
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests {
    use super::*;
    use crate::owner::{CaseOutcome, OwnerRun};
    use crate::owner_run::run::{
        OwnerOutcome, PreparedRun, RecordOf, RecordedRun, RunFailure, RunIssue,
    };
    use crate::owner_run::smoke::SmokeFailure;
    use crate::owner_run::smoke::{observe as observe_smoke, Observation, ObservationOutcome};
    use crate::pipeline::ValidationFailure;
    use crate::plan::PlanFailure;

    type Sound = TestOwner<SOUND>;

    fn recorded<O: Owner>() -> RecordedRun<O> {
        PreparedRun::<O>::acquire().unwrap().collect().unwrap()
    }

    fn saved<O: Owner>(observation: &Observation<O>) -> Vec<(&'static str, &[u8])> {
        observation.records()
    }

    /// The common records this observation wrote, without the owner document.
    fn common<O: Owner>(observation: &Observation<O>) -> Vec<&[u8]> {
        observation
            .records()
            .into_iter()
            .filter(|(name, _)| *name != "owner-result.json")
            .map(|(_, bytes)| bytes)
            .collect()
    }

    #[test]
    fn a_run_produces_records_that_re_admit_from_their_own_bytes() {
        let observation = observe_smoke::<Sound>().unwrap();
        let summary = observation.verify_saved(&saved(&observation)).unwrap();
        assert_eq!(summary.outcome, ObservationOutcome::Complete);
        assert_eq!(summary.planned_cases, FIXTURES.len());
        // A fixture declared blocked is measured too: reporting why it cannot
        // produce a result is the operation it measures.
        assert_eq!(summary.measured_cases, summary.planned_cases);
        assert_eq!(summary.non_measured_cases, 0);
    }

    #[test]
    fn a_withheld_record_is_not_a_successful_run() {
        let observation = observe_smoke::<Sound>().unwrap();
        let records = observation.records();
        for withheld in 0..records.len() {
            let partial = records
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != withheld)
                .map(|(_, record)| *record)
                .collect::<Vec<_>>();
            assert_eq!(
                observation.verify_saved(&partial),
                Err(SmokeFailure::MissingSavedRecord),
                "withholding {} was admitted",
                records[withheld].0
            );
        }
    }

    #[test]
    fn two_owner_documents_for_one_run_are_an_ambiguous_binding() {
        // Two documents for one run is not twice the evidence. Neither one is
        // selected, and the records that claim one resolved are refused.
        let observation = observe_smoke::<Sound>().unwrap();
        let owner = observation.owner_document();
        assert!(observation
            .validate(&[owner, owner], &common(&observation))
            .is_err());
    }

    #[test]
    fn a_document_from_another_run_does_not_bind_to_this_one() {
        // Both runs measured the same fixtures and agree on every case
        // identity. What they do not share is which run was captured, and
        // that is what admission checks.
        let first = observe_smoke::<Sound>().unwrap();
        let second = observe_smoke::<Sound>().unwrap();
        assert_eq!(
            first.run().plan_record().body.case_inventory,
            second.run().plan_record().body.case_inventory,
            "the two runs plan the same cases"
        );
        assert!(first
            .validate(&[second.owner_document()], &common(&first))
            .is_err());
        assert!(matches!(
            OwnerRun::admit(first.run(), second.owner_document()),
            Err(crate::owner::Rejection::Binding(Some(_)))
        ));

        // Changed as well, it is still another run's document first: which
        // run it names is checked before whether its content matches its
        // checksum, so it is not reported as a corrupt document of this run.
        let mut value: serde_json::Value = serde_json::from_slice(second.owner_document()).unwrap();
        value["result"]["outcome"] = serde_json::json!("invalid");
        let changed = serde_json::to_vec(&value).unwrap();
        let identity = serde_json::from_value(value["result"]["recordIdentity"].clone()).unwrap();
        assert_eq!(
            OwnerRun::admit(first.run(), &changed).err(),
            Some(crate::owner::Rejection::Binding(Some(identity)))
        );
        // The same change to this run's own document is a corrupt document.
        let mut value: serde_json::Value = serde_json::from_slice(first.owner_document()).unwrap();
        value["result"]["outcome"] = serde_json::json!("invalid");
        let changed = serde_json::to_vec(&value).unwrap();
        assert!(matches!(
            OwnerRun::admit(first.run(), &changed),
            Err(crate::owner::Rejection::Integrity(_))
        ));
    }

    #[test]
    fn no_owner_document_at_all_is_not_a_complete_run() {
        let observation = observe_smoke::<Sound>().unwrap();
        assert!(observation.validate(&[], &common(&observation)).is_err());
    }

    #[test]
    fn a_rehashed_change_to_a_saved_record_is_not_admitted() {
        let observation = observe_smoke::<Sound>().unwrap();
        let records = observation.records();

        // Drop one measured row from the report, then recompute the record's
        // own integrity digest. That is what a submitter can actually do, and
        // the result is a perfectly self-consistent document.
        let (_, report) = records
            .iter()
            .find(|(name, _)| *name == "report.json")
            .unwrap();
        let mut report: serde_json::Value = serde_json::from_slice(report).unwrap();
        report["body"]["sections"][0]["rows"]
            .as_array_mut()
            .unwrap()
            .pop();
        let digest = crate::identity::IntegrityDigest::from_hash(
            crate::encoding::record_hash(&report).unwrap(),
        );
        report["envelope"]["integrityDigest"] = serde_json::to_value(digest).unwrap();
        let changed = serde_json::to_vec(&report).unwrap();

        let saved = records
            .iter()
            .map(|(name, bytes)| {
                if *name == "report.json" {
                    (*name, changed.as_slice())
                } else {
                    (*name, *bytes)
                }
            })
            .collect::<Vec<_>>();
        // Self-consistency is not admission: the report must be the one this
        // run's evaluation produced.
        assert_eq!(
            observation.verify_saved(&saved),
            Err(SmokeFailure::Admission(ValidationFailure::Report))
        );
    }

    #[test]
    fn a_resealed_owner_document_is_not_what_was_captured() {
        // Changing a sample and recomputing the owner checksum gives a document
        // whose checksum matches its content. It still is not the run that was
        // captured, and admission names both facts apart.
        let run = recorded::<Sound>();
        let mut value: serde_json::Value = serde_json::from_slice(&run.encode().unwrap()).unwrap();
        let sample = &mut value["result"]["attempts"][0]["result"]["detail"]["capture"]["samples"]
            [0]["aggregateNanoseconds"];
        let captured: u64 = sample.as_str().unwrap().parse().unwrap();
        *sample = serde_json::json!((captured + 1).to_string());
        let changed: RecordOf<Sound> = serde_json::from_value(value.clone()).unwrap();
        // Unchanged checksum: the content no longer matches it.
        let issues = run.validate(&changed);
        assert!(issues.contains(&RunIssue::Integrity));
        assert!(issues.contains(&RunIssue::RecordedObservation));
        assert!(issues.contains(&RunIssue::Attempt));

        // Resealed: the checksum matches, but the content is still not what
        // this run recorded.
        let mut frame = LABELS.framing.frame("owner-run-result");
        frame.json(&value["result"]);
        value["checksum"] = serde_json::to_value(frame.finish()).unwrap();
        let resealed: RecordOf<Sound> = serde_json::from_value(value).unwrap();
        let issues = run.validate(&resealed);
        assert!(!issues.contains(&RunIssue::Integrity));
        assert!(issues.contains(&RunIssue::RecordedObservation));
        assert!(issues.contains(&RunIssue::Attempt));
    }

    #[test]
    fn another_owners_document_is_an_unsupported_codec_not_a_corrupt_one() {
        let run = recorded::<Sound>();
        let mut value: serde_json::Value = serde_json::from_slice(&run.encode().unwrap()).unwrap();
        value["result"]["codec"] =
            serde_json::json!("intlify-measurement-other-owner-run-result/1");
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(matches!(
            OwnerRun::admit(&run, &bytes),
            Err(crate::owner::Rejection::UnsupportedCodec)
        ));
        assert!(matches!(
            OwnerRun::admit(&run, b"not json"),
            Err(crate::owner::Rejection::Unreadable)
        ));
    }

    #[test]
    fn a_fixture_that_will_not_prepare_fails_the_run_before_any_capture() {
        assert_eq!(
            PreparedRun::<TestOwner<UNPREPARED>>::acquire().err(),
            Some(RunFailure::Plan(PlanFailure::InvalidRecord))
        );
    }

    #[test]
    fn a_result_that_drifts_from_its_expectation_invalidates_the_run() {
        let run = recorded::<TestOwner<DRIFTING>>();
        let admitted = OwnerRun::admit(&run, &run.encode().unwrap()).unwrap();
        // Every case disagreed with its expectation, which is not a lost
        // sample: no case is measured, and the run is invalid.
        assert_eq!(
            run.outcomes(&admitted),
            vec![CaseOutcome::SemanticMismatch; FIXTURES.len()]
        );
        assert_eq!(admitted.document().result().outcome, OwnerOutcome::Invalid);
        let observation = observe_smoke::<TestOwner<DRIFTING>>().unwrap();
        let summary = observation.verify_saved(&saved(&observation)).unwrap();
        assert_eq!(summary.outcome, ObservationOutcome::Invalid);
        assert_eq!(summary.measured_cases, 0);
        // Without a measured case there is no Evidence Set at all, rather than
        // one that claims nothing.
        assert!(observation
            .records()
            .iter()
            .all(|(name, _)| *name != "measurement-evidence.json"));
    }

    #[test]
    fn a_measurement_that_fails_leaves_the_run_incomplete_rather_than_invalid() {
        // A failed invocation is a missing measurement, not a wrong result: the
        // run is incomplete, and each case names the failure.
        let run = recorded::<TestOwner<FAILING>>();
        let admitted = OwnerRun::admit(&run, &run.encode().unwrap()).unwrap();
        assert_eq!(
            run.outcomes(&admitted),
            vec![
                CaseOutcome::Failed(crate::reason::InvocationFailure::InvocationPanicked);
                FIXTURES.len()
            ]
        );
        assert_eq!(
            admitted.document().result().outcome,
            OwnerOutcome::Incomplete
        );
        let observation = observe_smoke::<TestOwner<FAILING>>().unwrap();
        let summary = observation.verify_saved(&saved(&observation)).unwrap();
        assert_eq!(summary.outcome, ObservationOutcome::Incomplete);
        assert_eq!(summary.measured_cases, 0);
        assert_eq!(summary.non_measured_cases, FIXTURES.len());
    }

    #[test]
    fn the_planned_case_binding_is_the_same_for_two_runs() {
        // The case binding has no nonce, clock, or build in it, so two runs of
        // the same fixtures plan the same cases and differ in the run alone.
        let first = recorded::<Sound>();
        let second = recorded::<Sound>();
        let cases = |run: &RecordedRun<Sound>| -> Vec<serde_json::Value> {
            let value: serde_json::Value = serde_json::from_slice(&run.encode().unwrap()).unwrap();
            value["result"]["plan"]["cases"].as_array().unwrap().clone()
        };
        assert_eq!(cases(&first), cases(&second));
        let runs = |run: &RecordedRun<Sound>| -> serde_json::Value {
            serde_json::from_slice::<serde_json::Value>(&run.encode().unwrap()).unwrap()["result"]
                ["plan"]["run"]
                .clone()
        };
        assert_ne!(runs(&first), runs(&second));
    }
}
