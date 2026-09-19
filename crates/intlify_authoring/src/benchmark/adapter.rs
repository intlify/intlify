// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! This owner's answers to the common pipeline.
//!
//! The pipeline asks four things: what was planned, whether a submitted
//! document is the one this run issued, how each planned case turned out, and
//! what the measured cases project to. Everything else — the record shapes,
//! the reason vocabulary, admission — belongs to `intlify_measurement`.

use intlify_measurement::acquisition::{ClockFailure, MeasurementFailure};
use intlify_measurement::identity::{
    CaseIdentity, NativeChecksum, OwnerLabel, OwnerRecordIdentity, RecordIdentity, Token,
    VersionedIdentity,
};
use intlify_measurement::measurement::{
    ChecksumMethod, ExpectedOwner, MeasuredCase as CommonCase, OwnerBinding, OwnerResultFacts,
    RunBinding, SampleDomains, SampleFacts, SamplingFacts,
};
use intlify_measurement::owner::{
    CaseOutcome, OwnerEvidenceBody, OwnerFailure, OwnerRun, Rejection,
};
use intlify_measurement::plan::RunPlanRecord;
use intlify_measurement::reason::InvocationFailure;
use intlify_measurement::record::Reference;
use intlify_shared_json::quantity::Quantity;

use super::capture::CaptureFailure;
use super::context::projection;
use super::descriptor::Descriptors;
use super::observation::Observation;
use super::projection::CaseProjection;
use super::run::{
    producing_tool, AttemptResult, CheckedOwnerRecord, OwnerRecord, RecordedRun, RunIssue,
    RESULT_CODEC,
};

/// How this owner computes every checksum it records.
fn method() -> ChecksumMethod {
    ChecksumMethod {
        owner_identity: Token::literal("intlify-authoring"),
        algorithm: OwnerLabel::literal("blake3-256"),
        framing: OwnerLabel::literal("intlify-authoring-minimum-observation/0"),
    }
}

/// The digest domains this owner separates inside one measured case.
fn domains() -> SampleDomains {
    SampleDomains {
        semantic: OwnerLabel::literal("observation"),
        execution: OwnerLabel::literal("sample-execution"),
    }
}

fn result_schema() -> OwnerLabel {
    OwnerLabel::literal(RESULT_CODEC)
}

fn failed_invocation(failure: MeasurementFailure) -> InvocationFailure {
    match failure {
        MeasurementFailure::Clock(ClockFailure::DurationConversionOverflow) => {
            InvocationFailure::DurationConversionOverflow
        }
        MeasurementFailure::Clock(_) => InvocationFailure::ClockFailure,
        MeasurementFailure::InvocationPanicked => InvocationFailure::InvocationPanicked,
        MeasurementFailure::PrerequisiteUnavailable => InvocationFailure::PrerequisiteUnavailable,
    }
}

impl OwnerRun for RecordedRun {
    type Projection = CaseProjection;
    type Descriptors = Descriptors;
    type Observation = Observation;
    type Admitted = CheckedOwnerRecord;

    fn plan(&self) -> &RunPlanRecord {
        self.plan_record()
    }

    fn projections(&self) -> &[CaseProjection] {
        self.common_plan().projections()
    }

    fn expected(&self) -> ExpectedOwner {
        ExpectedOwner::new(
            self.expected_owner_identity().clone(),
            result_schema(),
            self.plan_record().body.measurement_profile.clone(),
        )
    }

    fn producing_tool(&self) -> VersionedIdentity {
        producing_tool()
    }

    fn encode(&self) -> Result<Vec<u8>, OwnerFailure> {
        Self::encode(self).map_err(|_| OwnerFailure)
    }

    fn admit(&self, bytes: &[u8]) -> Result<CheckedOwnerRecord, Rejection> {
        let Ok(value) = intlify_measurement::decode::value(bytes, false) else {
            return Err(Rejection::Unreadable);
        };
        // The exact tuple is selected before the body is strictly typed, so a
        // future owner schema is unsupported rather than declared corrupt.
        match value
            .pointer("/result/codec")
            .and_then(serde_json::Value::as_str)
        {
            Some(RESULT_CODEC) => {}
            Some(_) => return Err(Rejection::UnsupportedCodec),
            None => return Err(Rejection::Unreadable),
        }
        let Ok(document) = intlify_measurement::decode::typed::<OwnerRecord>(value) else {
            return Err(Rejection::Unreadable);
        };
        let submitted = document.result().record_identity.clone();
        if submitted != *self.expected_owner_identity()
            || document.plan_reference() != self.plan_record().identity()
        {
            return Err(Rejection::Binding(Some(submitted)));
        }
        match self.admit(document) {
            Ok(checked) => Ok(checked),
            Err(issues) if issues.contains(&RunIssue::Integrity) => {
                Err(Rejection::Integrity(submitted))
            }
            Err(_) => Err(Rejection::Binding(Some(submitted))),
        }
    }

    fn result_identity<'a>(&self, admitted: &'a CheckedOwnerRecord) -> &'a OwnerRecordIdentity {
        &admitted.document().result().record_identity
    }

    fn outcomes(&self, admitted: &CheckedOwnerRecord) -> Vec<CaseOutcome> {
        admitted
            .document()
            .result()
            .attempts
            .iter()
            .map(|attempt| match &attempt.result {
                AttemptResult::Measured(_) => CaseOutcome::Measured,
                AttemptResult::CaptureFailed(CaptureFailure::Measurement(failure)) => {
                    CaseOutcome::Failed(failed_invocation(*failure))
                }
                AttemptResult::CaptureFailed(CaptureFailure::Overflow) => {
                    CaseOutcome::Failed(InvocationFailure::MeasurementOverflow)
                }
                // A different result is not a lost sample: it invalidates the
                // run, because the expectation it was compared against held.
                AttemptResult::CaptureFailed(CaptureFailure::SemanticMismatch) => {
                    CaseOutcome::SemanticMismatch
                }
                AttemptResult::CaptureFailed(CaptureFailure::Preparation(_))
                | AttemptResult::PreparationFailed(_) => CaseOutcome::Invalid,
            })
            .collect()
    }

    fn evidence(
        &self,
        admitted: &CheckedOwnerRecord,
        parent: &RecordIdentity,
    ) -> Result<Option<OwnerEvidenceBody<Self>>, OwnerFailure> {
        let document = admitted.document();
        let result = document.result();
        let plan = self.plan_record();
        let mut cases = Vec::new();
        for ((attempt, planned), projection) in result
            .attempts
            .iter()
            .zip(&plan.body.case_inventory)
            .zip(self.common_plan().projections())
        {
            let AttemptResult::Measured(measured) = &attempt.result else {
                continue;
            };
            let sampling = &result.context.sampling;
            cases.push(
                intlify_measurement::measurement::CaseEvidence::project(
                    &method(),
                    &domains(),
                    &result.record_identity,
                    &planned.case_identity,
                    projection.clone(),
                    CommonCase {
                        ordinal: attempt.ordinal,
                        descriptors: measured.descriptors.clone(),
                        sampling: SamplingFacts {
                            warmup_strategy: Token::literal(
                                "fixed-invocations-before-measured-samples-per-case",
                            ),
                            warmup_repetitions: sampling.warmup_repetitions,
                            warmup_completed: measured.capture.warmup_completed,
                            measured_samples: sampling.measured_samples,
                            repetitions_per_sample: sampling.repetitions_per_sample,
                        },
                        samples: measured
                            .capture
                            .samples
                            .iter()
                            .map(|sample| SampleFacts {
                                ordinal: sample.ordinal,
                                repetition_count: sample.repetition_count,
                                aggregate_quantity: sample.aggregate_nanoseconds,
                                semantic_observation: sample.semantic_observation,
                                semantic_identity: NativeChecksum::from_bytes(
                                    sample.semantic_observation.identity().bytes(),
                                ),
                                execution_identity: NativeChecksum::from_bytes(
                                    sample.execution_identity.bytes(),
                                ),
                                local_identity: NativeChecksum::from_bytes(
                                    sample.local_identity.bytes(),
                                ),
                            })
                            .collect(),
                    },
                )
                .map_err(|_| OwnerFailure)?,
            );
        }
        if cases.is_empty() {
            return Ok(None);
        }
        let context = self.context();
        Ok(Some(OwnerEvidenceBody::<Self> {
            binding: RunBinding::from_plan(plan),
            projection: projection(),
            owner_result: OwnerBinding::new(OwnerResultFacts {
                method: method(),
                result_schema: result_schema(),
                benchmark_profile: context.profile.clone(),
                result_identity: result.record_identity.clone(),
                domain: OwnerLabel::literal("owner-run-result"),
                checksum: NativeChecksum::from_bytes(document.checksum().bytes()),
            }),
            build: context
                .build
                .build(parent, context.profile.clone())
                .map_err(|_| OwnerFailure)?,
            environment: super::context::environment(context, parent).map_err(|_| OwnerFailure)?,
            cases,
        }))
    }

    fn attempt_reference(
        &self,
        admitted: &CheckedOwnerRecord,
        ordinal: Quantity,
    ) -> Result<Reference, OwnerFailure> {
        intlify_measurement::measurement::owner_attempt_reference(
            &admitted.document().result().record_identity,
            ordinal,
        )
        .map_err(|_| OwnerFailure)
    }
}

// The case identity is the common plan's, so this alias only documents that
// the adapter never mints one of its own.
#[allow(dead_code)]
type PlannedCaseIdentity = CaseIdentity;
