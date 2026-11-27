// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use crate::benchmark::evidence::{entry, outcome, require_observable, Resolved};
use crate::construction::registry::check::RecordKind;

use super::{LogicalWork, UnavailableWork, WorkFailure, WorkKind, WorkStage, WorkValue};

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
    /// stages of the bounds it applies (D15:1852, 1963-1971, 2801): what the
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
    /// stages of the bounds it applies (D15:1835-1840, 1852, 1947-1957,
    /// 2801): what the Entry domain committed under its eleven
    /// `diagnostics.entry` counts, and the complete invocation under its six
    /// `evidenceMetadata` counts.
    pub(super) fn entry_evidence(&mut self, resolved: &entry::Resolved) -> Result<(), WorkFailure> {
        let retained =
            entry::require_observable(resolved).map_err(|_| WorkFailure::InvalidOrdinaryResult)?;
        let invocation = resolved.invocation;
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
        ] {
            self.set(kind, WorkStage::OperationResult, WorkValue::exact(value));
        }
        // A path past its bound ends the count before what path admission
        // leaves, as a file past its bytes ends it before its value (plan 025
        // decisions #232 and #265).
        let admitted = invocation.admitted;
        for (kind, value) in [
            (
                WorkKind::EntryEvidenceSubjectOccurrences,
                admitted.map(|work| work.subject_occurrences),
            ),
            (
                WorkKind::EntryEvidenceTokenInputs,
                admitted.map(|work| work.token_inputs),
            ),
            (
                WorkKind::EntryEvidenceCollisionComparisons,
                admitted.map(|work| work.collision_comparisons),
            ),
            (
                WorkKind::EntryEvidenceMetadataWorkUnits,
                admitted.map(|work| work.metadata_work_units),
            ),
        ] {
            self.set(
                kind,
                WorkStage::OperationResult,
                value.map_or(
                    WorkValue::Unavailable {
                        reason: UnavailableWork::EvidencePathsNotAdmitted,
                    },
                    WorkValue::exact,
                ),
            );
        }
        Ok(())
    }

    /// The workload of outcome finalization: the records each domain's
    /// projection orders, and the mandatory reserve in `maxCanonicalBytes`'
    /// unit (D15:1848, 2144, 2801).
    pub(super) fn outcome_finalization(
        &mut self,
        resolved: &outcome::Resolved,
    ) -> Result<(), WorkFailure> {
        let retained = outcome::require_observable(resolved)
            .map_err(|_| WorkFailure::InvalidOrdinaryResult)?;
        for (kind, value) in [
            (
                WorkKind::OutcomeFinalizationEntryRecords,
                count(retained.entry.records.len())?,
            ),
            (
                WorkKind::OutcomeFinalizationSharedRecords,
                count(
                    retained
                        .shared
                        .as_ref()
                        .map_or(0, |projection| projection.records.len()),
                )?,
            ),
            (
                WorkKind::OutcomeFinalizationReserveCanonicalBytes,
                retained.reserve,
            ),
        ] {
            self.set(kind, WorkStage::OperationResult, WorkValue::exact(value));
        }
        Ok(())
    }
}
