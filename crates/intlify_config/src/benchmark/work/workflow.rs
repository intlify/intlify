// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use crate::benchmark::evidence::outcome::RetainedProjection;
use crate::benchmark::workflow::{require_observable, EntryCounts, Resolved};

use super::{LogicalWork, UnavailableWork, WorkFailure, WorkKind, WorkStage, WorkValue};

/// A projection's records and the sum of their accounting bytes; none
/// without the projection.
fn records(projection: Option<&RetainedProjection>) -> Result<(u64, u64), WorkFailure> {
    let records = projection.map_or(&[][..], |projection| &projection.records[..]);
    let bytes = records.iter().try_fold(0_u64, |total, (_, record)| {
        total
            .checked_add(record.bytes)
            .ok_or(WorkFailure::UnrepresentableCounter)
    })?;
    let count = u64::try_from(records.len()).map_err(|_| WorkFailure::UnrepresentableCounter)?;
    Ok((count, bytes))
}

impl LogicalWork {
    /// The workload of one entry's complete invocation in the units of the
    /// bounds that count it (D15:2796): what strict file entry counts of the
    /// file and its value, a value's counts unavailable when none is
    /// complete, or the Snapshot's envelope bytes and traversal units and its
    /// value's counts, unavailable when it holds a marker, and the submitted
    /// collection's members and their `C(member)` bytes, counted over what the
    /// interval admitted; then the records of each projection with their
    /// accounting bytes and the mandatory reserve, counted over the outcome
    /// (D15:1847, 1889, 2129).
    pub(super) fn workflow(&mut self, resolved: &Resolved) -> Result<(), WorkFailure> {
        let retained =
            require_observable(resolved).map_err(|_| WorkFailure::InvalidOrdinaryResult)?;
        let admitted = resolved
            .admitted
            .ok_or(WorkFailure::InvalidOrdinaryResult)?;
        let input = WorkStage::PreparedInput;
        let output = WorkStage::OperationResult;
        let (shared, shared_bytes) = records(retained.shared.as_ref())?;
        let (entry, entry_bytes) = records(Some(&retained.entry))?;
        let (counted, reason) = match admitted.entry {
            EntryCounts::File { bytes, tokens } => (
                [
                    (WorkKind::RawFileBytes, bytes),
                    (WorkKind::ParserTokensVisited, tokens),
                ],
                UnavailableWork::RawInputNotComplete,
            ),
            EntryCounts::Snapshot {
                envelope,
                traversal,
            } => (
                [
                    (WorkKind::SnapshotEnvelopeCanonicalBytes, envelope),
                    (WorkKind::SnapshotTraversalUnits, traversal),
                ],
                UnavailableWork::SnapshotHoldsMarkers,
            ),
        };
        for (kind, value) in counted {
            self.set(kind, input, WorkValue::exact(value));
        }
        for (kind, stage, value) in [
            (WorkKind::WorkflowSubmittedMembers, input, admitted.members),
            (
                WorkKind::WorkflowSubmittedCanonicalBytes,
                input,
                admitted.member_bytes,
            ),
            (WorkKind::WorkflowSharedRecords, output, shared),
            (WorkKind::WorkflowSharedCanonicalBytes, output, shared_bytes),
            (WorkKind::WorkflowEntryRecords, output, entry),
            (WorkKind::WorkflowEntryCanonicalBytes, output, entry_bytes),
            (
                WorkKind::WorkflowReserveCanonicalBytes,
                output,
                retained.reserve,
            ),
        ] {
            self.set(kind, stage, WorkValue::exact(value));
        }
        self.values_or(admitted.value, input, reason);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmark::cases::EvidenceRecipe as R;
    use crate::benchmark::operation::{Output, Prepared};
    use crate::benchmark::workflow::tests::candidate;

    /// D15:2796: the file's bytes and the submitted members are counted over
    /// what the interval admitted, a value's counts only when its raw parse
    /// completed, and the records and the reserve over the outcome, at the
    /// stage each is counted at.
    #[test]
    fn the_work_counts_what_the_interval_admitted_and_the_outcome_holds() {
        let outcome = [
            WorkKind::WorkflowSharedRecords,
            WorkKind::WorkflowSharedCanonicalBytes,
            WorkKind::WorkflowEntryRecords,
            WorkKind::WorkflowEntryCanonicalBytes,
            WorkKind::WorkflowReserveCanonicalBytes,
        ];
        for recipe in R::ALL {
            let candidate = candidate(recipe);
            let (Prepared::Workflow(prepared), Output::Workflow(resolved)) =
                (&candidate.prepared, &candidate.output)
            else {
                unreachable!()
            };
            let (raw, retained) = (&prepared.raw, resolved.result.as_ref().unwrap());
            let work = &candidate.logical_work;
            let exact = |kind| {
                work.exact(kind)
                    .and_then(|value| usize::try_from(value).ok())
            };
            assert_eq!(exact(WorkKind::RawFileBytes), Some(raw.file().len()));
            assert_eq!(
                exact(WorkKind::WorkflowSubmittedMembers),
                Some(raw.members.len())
            );
            // An inadmissible file's raw parse completes, so its value is
            // counted, though Phase 1 admits no value of it.
            let parsed = recipe != R::Malformed;
            assert_eq!(exact(WorkKind::LogicalValueNodes).is_some(), parsed);
            let shared = retained
                .shared
                .as_ref()
                .map_or(0, |shared| shared.records.len());
            assert_eq!(exact(WorkKind::WorkflowSharedRecords), Some(shared));
            let entry = retained.entry.records.len();
            assert_eq!(exact(WorkKind::WorkflowEntryRecords), Some(entry));
            assert_eq!(
                work.exact(WorkKind::WorkflowReserveCanonicalBytes),
                Some(retained.reserve)
            );
            for fact in &work.facts {
                let stage = if outcome.contains(&fact.kind) {
                    WorkStage::OperationResult
                } else {
                    WorkStage::PreparedInput
                };
                assert_eq!(fact.stage, stage, "{recipe:?} {:?}", fact.kind);
            }
        }
    }

    /// D15:1802, 1804, 1889, 2796: a programmatic entry's invocation counts
    /// the admitted Snapshot's envelope bytes and traversal units in place of
    /// the file's bytes and parser tokens, and the rest as its file's case
    /// counts it in the same state, but a Snapshot holding a marker has no
    /// value to count.
    #[test]
    fn a_programmatic_entry_counts_its_snapshot_in_place_of_the_file() {
        use crate::benchmark::cases::{ProgrammaticRecipe as P, ResolverState};
        use crate::benchmark::workflow::tests::{candidate_in, programmatic_in};

        let state = ResolverState::Fresh;
        for recipe in P::ALL {
            let candidate = programmatic_in(recipe, state);
            let Output::Workflow(resolved) = &candidate.output else {
                unreachable!()
            };
            let Some(EntryCounts::Snapshot {
                envelope,
                traversal,
            }) = resolved.admitted.map(|admitted| admitted.entry)
            else {
                panic!("{recipe:?} counts a Snapshot");
            };
            let work = &candidate.logical_work;
            let kinds: Vec<_> = work.facts.iter().map(|fact| fact.kind).collect();
            assert_eq!(kinds, WorkKind::PROGRAMMATIC_WORKFLOW, "{recipe:?}");
            let entry = [
                WorkKind::SnapshotEnvelopeCanonicalBytes,
                WorkKind::SnapshotTraversalUnits,
            ];
            assert_eq!(
                entry.map(|kind| work.exact(kind)),
                [envelope, traversal].map(Some)
            );
            let file = candidate_in(recipe.file(), state).logical_work;
            let values = &WorkKind::PROGRAMMATIC_WORKFLOW[2..7];
            let entry = [
                WorkKind::WorkflowEntryRecords,
                WorkKind::WorkflowEntryCanonicalBytes,
            ];
            let marked = recipe == P::RejectedMarker;
            for (fact, paired) in work.facts.iter().zip(&file.facts).skip(2) {
                assert_eq!((fact.kind, fact.stage), (paired.kind, paired.stage));
                if marked && values.contains(&fact.kind) {
                    let reason = UnavailableWork::SnapshotHoldsMarkers;
                    assert_eq!(fact.observation, WorkValue::Unavailable { reason });
                } else if !marked && !entry.contains(&fact.kind) {
                    let kind = fact.kind;
                    assert_eq!(fact.observation, paired.observation, "{recipe:?} {kind:?}");
                }
            }
        }
    }
}
