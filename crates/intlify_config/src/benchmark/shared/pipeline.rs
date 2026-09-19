// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! This owner's adapter onto the common observational pipeline.
//!
//! The pipeline itself belongs to `intlify_measurement`. What this module
//! answers is what only this owner can: whether a submitted document is the
//! one this run issued, how each planned case turned out, and what the
//! measured cases project to.

use intlify_measurement::owner::{
    CaseOutcome, ObservedDescriptors, OwnerFailure, OwnerRun, Rejection,
};

pub(in crate::benchmark) use intlify_measurement::pipeline::{
    produce, validate_records, Artifacts,
};

// Submitting owner documents other than this run's own, and admitting produced
// artifacts directly, are how the negative fixtures below reach the pipeline.
#[cfg(test)]
pub(in crate::benchmark) use intlify_measurement::pipeline::{produce_with_owner, validate};

use super::identity::{OwnerRecordIdentity, RecordIdentity, VersionedIdentity};
use super::measurement::{self, EvidenceBody, ExpectedOwner};
use super::plan::{CaseProjection, RunPlanRecord};
use super::record::producing_tool;
use crate::benchmark::clock::ClockFailure;
use crate::benchmark::collect::CollectionFailure;
use crate::benchmark::descriptor::Descriptors;
use crate::benchmark::measure::MeasurementFailure;
use crate::benchmark::observation::Observation;
use crate::benchmark::profile::ProfileCollectionFailure;
use crate::benchmark::quantity::Quantity;
use crate::benchmark::run::{
    attempt_outcome, AttemptResult, CheckedOwnerRecord, OwnerOutcome, OwnerRecord, RecordedRun,
    RunIssue,
};
use crate::benchmark::sample::CaptureFailureCause;
use crate::benchmark::work::WorkFailure;
use intlify_measurement::reason::InvocationFailure as Invocation;
use intlify_measurement::record::Reference;

const RESULT_CODEC: &str = "intlify-config-owner-run-result/1";

impl ObservedDescriptors for Descriptors {
    fn execution(&self) -> &intlify_measurement::execution::Execution {
        &self.execution
    }
}

fn failed_invocation(cause: &CaptureFailureCause) -> Invocation {
    match cause {
        CaptureFailureCause::Measurement(MeasurementFailure::Clock(
            ClockFailure::DurationConversionOverflow,
        )) => Invocation::DurationConversionOverflow,
        CaptureFailureCause::Measurement(MeasurementFailure::Clock(_)) => Invocation::ClockFailure,
        CaptureFailureCause::Measurement(MeasurementFailure::InvocationPanicked) => {
            Invocation::InvocationPanicked
        }
        CaptureFailureCause::MeasurementOverflow => Invocation::MeasurementOverflow,
        CaptureFailureCause::LogicalWorkObservation(WorkFailure::UnrepresentableCounter) => {
            Invocation::CounterOverflow
        }
        CaptureFailureCause::CollectorAllocation => Invocation::CollectorAllocation,
        CaptureFailureCause::ObservationPanicked => Invocation::ObservationPanicked,
        CaptureFailureCause::PrerequisiteUnavailable
        | CaptureFailureCause::Measurement(MeasurementFailure::PrerequisiteUnavailable) => {
            Invocation::PrerequisiteUnavailable
        }
        CaptureFailureCause::Output(_) => Invocation::OutputFailure,
        CaptureFailureCause::LogicalWorkObservation(_)
        | CaptureFailureCause::LogicalWorkMismatch(_)
        | CaptureFailureCause::SemanticObservationMismatch(_) => Invocation::WorkObservationFailure,
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
        measurement::expected_from_plan(self.plan_record(), self.expected_owner_identity())
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
        if document.result().record_identity != *self.expected_owner_identity()
            || document.plan_reference() != self.plan_record().identity()
        {
            return Err(Rejection::Binding(Some(submitted)));
        }
        match self.admit_owned(document) {
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
                AttemptResult::CollectionFailed(ProfileCollectionFailure::Collection(
                    CollectionFailure::Capture(failure),
                )) if attempt_outcome(attempt) == OwnerOutcome::Incomplete => {
                    CaseOutcome::Failed(failed_invocation(&failure.cause))
                }
                AttemptResult::CollectionFailed(ProfileCollectionFailure::Collection(
                    CollectionFailure::Capture(failure),
                )) if matches!(
                    failure.cause,
                    CaptureFailureCause::SemanticObservationMismatch(_)
                        | CaptureFailureCause::LogicalWorkMismatch(_)
                ) =>
                {
                    CaseOutcome::SemanticMismatch
                }
                _ => CaseOutcome::Invalid,
            })
            .collect()
    }

    fn evidence(
        &self,
        admitted: &CheckedOwnerRecord,
        parent: &RecordIdentity,
    ) -> Result<Option<EvidenceBody>, OwnerFailure> {
        let source = admitted.projection_source();
        let plan = self.plan_record();
        let mut cases = Vec::new();
        for ((attempt, planned), projection) in source
            .document()
            .result()
            .attempts
            .iter()
            .zip(&plan.body.case_inventory)
            .zip(self.common_plan().projections())
        {
            if let Some(case) =
                measurement::project_case(&source, attempt, &planned.case_identity, projection)
                    .map_err(|_| OwnerFailure)?
            {
                cases.push(case);
            }
        }
        if cases.is_empty() {
            return Ok(None);
        }
        match measurement::evidence_body(&source, plan, parent, cases) {
            Ok(body) => Ok(Some(body)),
            // A valid observation whose identifiers cannot be mapped losslessly
            // is projection-ineligible, not a fabricated value.
            Err(intlify_measurement::identity::IdentityFailure::InvalidToken) => Ok(None),
            Err(_) => Err(OwnerFailure),
        }
    }

    fn attempt_reference(
        &self,
        admitted: &CheckedOwnerRecord,
        ordinal: Quantity,
    ) -> Result<Reference, OwnerFailure> {
        measurement::attempt_reference(admitted.document(), ordinal).map_err(|_| OwnerFailure)
    }
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;
