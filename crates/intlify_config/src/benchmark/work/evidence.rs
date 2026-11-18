// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use crate::benchmark::evidence::{entry, require_observable, Resolved};
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

    /// The workload of the Entry projection stage in the units and at the
    /// stages of the bounds it applies (D15:1834-1839, 1851, 1946-1956,
    /// 2796): what the Entry domain committed under its eleven
    /// `diagnostics.entry` counts, and the complete invocation under its six
    /// `evidenceMetadata` counts.
    pub(super) fn entry_evidence(&mut self, resolved: &entry::Resolved) -> Result<(), WorkFailure> {
        let retained =
            entry::require_observable(resolved).map_err(|_| WorkFailure::InvalidOrdinaryResult)?;
        let invocation = resolved
            .invocation
            .ok_or(WorkFailure::InvalidOrdinaryResult)?;
        let slices = &retained.slices;
        let records = || slices.iter().flat_map(|slice| &slice.records);
        let of = |kind: RecordKind| count(records().filter(|record| record.kind == kind).count());
        let evidence = || {
            slices
                .iter()
                .flat_map(|slice| slice.candidates.evidence.iter().copied())
        };
        for (kind, value) in [
            (
                WorkKind::EntryEvidenceFindingCandidates,
                sum(slices.iter().map(|slice| slice.candidates.findings))?,
            ),
            (
                WorkKind::EntryEvidenceFindings,
                of(RecordKind::EntryFinding)?,
            ),
            (
                WorkKind::EntryEvidenceMaximumRelatedEvidencePerOccurrenceGroup,
                records().map(|record| record.related).max().unwrap_or(0),
            ),
            (
                WorkKind::EntryEvidenceRelatedEvidenceOccurrences,
                sum(slices.iter().map(|slice| slice.candidates.related))?,
            ),
            (
                WorkKind::EntryEvidenceMaximumSingleEvidenceBytes,
                evidence().max().unwrap_or(0),
            ),
            (WorkKind::EntryEvidenceTotalEvidenceBytes, sum(evidence())?),
            (
                WorkKind::EntryEvidenceEvaluationStatuses,
                of(RecordKind::EntryEvaluationStatus)?,
            ),
            (
                WorkKind::EntryEvidenceMaximumCausesPerStatus,
                records().map(|record| record.causes).max().unwrap_or(0),
            ),
            (
                WorkKind::EntryEvidenceCauseOccurrences,
                sum(slices.iter().map(|slice| slice.candidates.causes))?,
            ),
            (WorkKind::EntryEvidenceRecords, count(records().count())?),
            (
                WorkKind::EntryEvidenceCanonicalBytes,
                sum(records().map(|record| record.bytes))?,
            ),
            (
                WorkKind::EntryEvidenceMaximumLogicalPathSegments,
                invocation.path_segments,
            ),
            (
                WorkKind::EntryEvidenceMaximumLogicalPathCanonicalBytes,
                invocation.path_bytes,
            ),
            (
                WorkKind::EntryEvidenceSubjectOccurrences,
                invocation.subject_occurrences,
            ),
            (WorkKind::EntryEvidenceTokenInputs, invocation.token_inputs),
            (
                WorkKind::EntryEvidenceCollisionComparisons,
                invocation.collision_comparisons,
            ),
            (
                WorkKind::EntryEvidenceMetadataWorkUnits,
                invocation.metadata_work_units,
            ),
        ] {
            self.set(kind, WorkStage::OperationResult, WorkValue::exact(value));
        }
        Ok(())
    }
}
