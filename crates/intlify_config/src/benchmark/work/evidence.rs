// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use crate::benchmark::evidence::{require_observable, Resolved};
use crate::construction::registry::check::RecordKind;

use super::{LogicalWork, WorkFailure, WorkKind, WorkStage, WorkValue};

fn count(value: usize) -> Result<u64, WorkFailure> {
    u64::try_from(value).map_err(|_| WorkFailure::UnrepresentableCounter)
}

fn sum(mut values: impl Iterator<Item = u64>) -> Result<u64, WorkFailure> {
    values.try_fold(0_u64, |total, value| {
        total
            .checked_add(value)
            .ok_or(WorkFailure::UnrepresentableCounter)
    })
}

impl LogicalWork {
    /// The workload of the Shared projection stage in the units and at the
    /// stages of the bounds it applies (D15:1851, 1962-1970, 2796): what the
    /// Shared domain committed under its seven `diagnostics.shared` counts,
    /// and the Shared scope under its four `evidenceMetadata` counts.
    pub(super) fn shared_evidence(&mut self, resolved: &Resolved) -> Result<(), WorkFailure> {
        let retained =
            require_observable(resolved).map_err(|_| WorkFailure::InvalidOrdinaryResult)?;
        let slices = &retained.slices;
        let records = || slices.iter().flat_map(|slice| &slice.records);
        let of = |kind: RecordKind| count(records().filter(|record| record.kind == kind).count());
        let scope = resolved.scope;
        for (kind, value) in [
            (
                WorkKind::SharedEvidenceFindingCandidates,
                sum(slices.iter().map(|slice| slice.candidates.findings))?,
            ),
            (
                WorkKind::SharedEvidenceFindings,
                of(RecordKind::SharedFinding)?,
            ),
            (
                WorkKind::SharedEvidenceEvaluationStatuses,
                of(RecordKind::SharedEvaluationStatus)?,
            ),
            (
                WorkKind::SharedEvidenceMaximumCausesPerStatus,
                records().map(|record| record.causes).max().unwrap_or(0),
            ),
            (
                WorkKind::SharedEvidenceCauseOccurrences,
                sum(slices.iter().map(|slice| slice.candidates.causes))?,
            ),
            (WorkKind::SharedEvidenceRecords, count(records().count())?),
            (
                WorkKind::SharedEvidenceCanonicalBytes,
                sum(records().map(|record| record.bytes))?,
            ),
            (
                WorkKind::SharedEvidenceSubjectOccurrences,
                scope.subject_occurrences,
            ),
            (WorkKind::SharedEvidenceTokenInputs, scope.token_inputs),
            (
                WorkKind::SharedEvidenceCollisionComparisons,
                scope.collision_comparisons,
            ),
            (
                WorkKind::SharedEvidenceMetadataWorkUnits,
                scope.metadata_work_units,
            ),
        ] {
            self.set(kind, WorkStage::OperationResult, WorkValue::exact(value));
        }
        Ok(())
    }
}
