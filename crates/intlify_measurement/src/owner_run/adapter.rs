// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! An owner run's answers to the common pipeline.
//!
//! The pipeline asks four things: what was planned, whether a submitted
//! document is the one this run issued, how each planned case turned out, and
//! what the measured cases project to. A recorded run answers all four from
//! what it retained; the owner contributes only its labels, its projections,
//! its descriptors, and its environment.

use intlify_shared_json::quantity::Quantity;

use super::capture::CaptureFailure;
use super::run::{
    producing_tool, AttemptResult, CheckedOwnerRecord, RecordOf, RecordedRun, RunIssue,
};
use super::Owner;
use crate::acquisition::{ClockFailure, MeasurementFailure};
use crate::identity::{
    NativeChecksum, OwnerLabel, OwnerRecordIdentity, RecordIdentity, Token, VersionedIdentity,
};
use crate::measurement::{
    owner_attempt_reference, CaseEvidence, ChecksumMethod, ExpectedOwner, MeasuredCase,
    OwnerBinding, OwnerResultFacts, RunBinding, SampleDomains, SampleFacts, SamplingFacts,
};
use crate::owner::{CaseOutcome, OwnerEvidenceBody, OwnerFailure, OwnerRun, Rejection};
use crate::plan::RunPlanRecord;
use crate::reason::InvocationFailure;
use crate::record::Reference;

/// How an owner computes every checksum it records.
fn method<O: Owner>() -> ChecksumMethod {
    ChecksumMethod {
        owner_identity: Token::literal(O::LABELS.owner),
        algorithm: OwnerLabel::literal("blake3-256"),
        framing: OwnerLabel::literal(O::LABELS.framing.label()),
    }
}

/// The digest domains the capture separates inside one measured case.
fn domains() -> SampleDomains {
    SampleDomains {
        semantic: OwnerLabel::literal("observation"),
        execution: OwnerLabel::literal("sample-execution"),
    }
}

fn result_schema<O: Owner>() -> OwnerLabel {
    OwnerLabel::literal(O::LABELS.result_codec)
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

impl<O: Owner> OwnerRun for RecordedRun<O> {
    type Projection = O::Projection;
    type Descriptors = O::Descriptors;
    type Observation = super::observation::Observation;
    type Admitted = CheckedOwnerRecord<O>;

    fn plan(&self) -> &RunPlanRecord {
        self.plan_record()
    }

    fn projections(&self) -> &[O::Projection] {
        self.common_plan().projections()
    }

    fn expected(&self) -> ExpectedOwner {
        ExpectedOwner::new(
            self.expected_owner_identity().clone(),
            result_schema::<O>(),
            self.plan_record().body.measurement_profile.clone(),
        )
    }

    fn producing_tool(&self) -> VersionedIdentity {
        producing_tool::<O>()
    }

    fn encode(&self) -> Result<Vec<u8>, OwnerFailure> {
        Self::encode(self).map_err(|_| OwnerFailure)
    }

    fn admit(&self, bytes: &[u8]) -> Result<CheckedOwnerRecord<O>, Rejection> {
        let Ok(value) = crate::decode::value(bytes, false) else {
            return Err(Rejection::Unreadable);
        };
        // The exact tuple is selected before the body is strictly typed, so a
        // future owner schema is unsupported rather than declared corrupt.
        match value
            .pointer("/result/codec")
            .and_then(serde_json::Value::as_str)
        {
            Some(codec) if codec == O::LABELS.result_codec => {}
            Some(_) => return Err(Rejection::UnsupportedCodec),
            None => return Err(Rejection::Unreadable),
        }
        let Ok(document) = crate::decode::typed::<RecordOf<O>>(value) else {
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

    fn result_identity<'a>(&self, admitted: &'a CheckedOwnerRecord<O>) -> &'a OwnerRecordIdentity {
        &admitted.document().result().record_identity
    }

    fn outcomes(&self, admitted: &CheckedOwnerRecord<O>) -> Vec<CaseOutcome> {
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
        admitted: &CheckedOwnerRecord<O>,
        parent: &RecordIdentity,
    ) -> Result<Option<OwnerEvidenceBody<Self>>, OwnerFailure> {
        let labels = O::LABELS;
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
                CaseEvidence::project(
                    &method::<O>(),
                    &domains(),
                    &result.record_identity,
                    &planned.case_identity,
                    projection.clone(),
                    MeasuredCase {
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
                                    sample.semantic_observation.identity(labels.framing).bytes(),
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
            projection: labels.projection.versioned(),
            owner_result: OwnerBinding::new(OwnerResultFacts {
                method: method::<O>(),
                result_schema: result_schema::<O>(),
                benchmark_profile: context.profile.clone(),
                result_identity: result.record_identity.clone(),
                domain: OwnerLabel::literal("owner-run-result"),
                checksum: NativeChecksum::from_bytes(document.checksum().bytes()),
            }),
            build: context
                .build
                .build(parent, context.profile.clone())
                .map_err(|_| OwnerFailure)?,
            environment: O::environment(context, parent).map_err(|_| OwnerFailure)?,
            cases,
        }))
    }

    fn attempt_reference(
        &self,
        admitted: &CheckedOwnerRecord<O>,
        ordinal: Quantity,
    ) -> Result<Reference, OwnerFailure> {
        owner_attempt_reference(&admitted.document().result().record_identity, ordinal)
            .map_err(|_| OwnerFailure)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_measurement_keeps_its_common_cause() {
        for (failure, cause) in [
            (
                MeasurementFailure::Clock(ClockFailure::DurationConversionOverflow),
                InvocationFailure::DurationConversionOverflow,
            ),
            (
                MeasurementFailure::Clock(ClockFailure::ReversedClock),
                InvocationFailure::ClockFailure,
            ),
            (
                MeasurementFailure::Clock(ClockFailure::UnsupportedPlatform),
                InvocationFailure::ClockFailure,
            ),
            (
                MeasurementFailure::InvocationPanicked,
                InvocationFailure::InvocationPanicked,
            ),
            (
                MeasurementFailure::PrerequisiteUnavailable,
                InvocationFailure::PrerequisiteUnavailable,
            ),
        ] {
            assert_eq!(failed_invocation(failure), cause, "{failure:?}");
        }
    }
}
