// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use crate::benchmark::snapshot_admission::Resolved;
use crate::snapshot::SnapshotCounts;

use super::{LogicalWork, UnavailableWork, WorkFailure, WorkKind, WorkStage, WorkValue};

impl LogicalWork {
    /// The workload of one Snapshot's admission in the units of the bounds
    /// that count it (D15:1796-1804, 2796): the admitted tree's count under
    /// each `bootstrapOnly.snapshot` bound, then its document's root-value
    /// counts, unavailable when the tree holds a marker and so no value
    /// (D15:1889, 2129).
    pub(super) fn snapshot(&mut self, resolved: &Resolved) -> Result<(), WorkFailure> {
        let admitted = resolved
            .as_ref()
            .map_err(|_| WorkFailure::InvalidOrdinaryResult)?;
        let stage = WorkStage::OperationResult;
        let counts = SnapshotCounts::of(admitted.snapshot.root()).by_bound();
        for (kind, (_, count)) in WorkKind::SNAPSHOT.into_iter().zip(counts) {
            self.set(kind, stage, WorkValue::exact(count));
        }
        if let Some(document) = &admitted.document {
            self.values(Some(document.value_counts()), stage);
            return Ok(());
        }
        for &kind in &WorkKind::SNAPSHOT[counts.len()..] {
            let reason = UnavailableWork::SnapshotHoldsMarkers;
            self.set(kind, stage, WorkValue::Unavailable { reason });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmark::cases::SnapshotRecipe as R;
    use crate::benchmark::operation::Output;
    use crate::benchmark::snapshot_admission::tests::candidate;
    use crate::materialize::MaterializedDocument;

    /// D15:1796-1804, 1889: each of the nine counts is the admitted tree's
    /// count under its bound, the five value counts are the document's, or
    /// unavailable for a tree that holds a marker, and every fact is counted
    /// over the interval's result.
    #[test]
    fn the_work_counts_the_admitted_tree_and_its_document() {
        for recipe in R::ALL {
            let candidate = candidate(recipe);
            let Output::Snapshot(resolved) = &candidate.output else {
                unreachable!()
            };
            let Ok(admitted) = &**resolved else {
                panic!("{recipe:?} is admitted");
            };
            let work = &candidate.logical_work;
            let counts = SnapshotCounts::of(admitted.snapshot.root());
            for (kind, count) in [
                (WorkKind::SnapshotNodes, counts.nodes),
                (WorkKind::SnapshotMaximumDepth, counts.depth),
                (
                    WorkKind::SnapshotCollectionEntries,
                    counts.collection_entries,
                ),
                (
                    WorkKind::SnapshotTotalStringBytes,
                    counts.total_string_bytes,
                ),
                (
                    WorkKind::SnapshotMaximumStringBytes,
                    counts.single_string_bytes,
                ),
                (WorkKind::SnapshotRejectedMarkers, counts.rejected_markers),
                (
                    WorkKind::SnapshotEnvelopeCanonicalBytes,
                    counts.envelope_canonical_bytes,
                ),
                (
                    WorkKind::SnapshotMaximumMarkerCanonicalBytes,
                    counts.single_marker_canonical_bytes,
                ),
                (WorkKind::SnapshotTraversalUnits, counts.traversal_units),
            ] {
                assert_eq!(work.exact(kind), Some(count), "{recipe:?} {kind:?}");
            }
            let value = admitted
                .document
                .as_ref()
                .map(MaterializedDocument::value_counts);
            for (kind, count) in [
                (WorkKind::LogicalValueNodes, value.map(|value| value.nodes)),
                (WorkKind::MaximumValueDepth, value.map(|value| value.depth)),
                (
                    WorkKind::CollectionEntries,
                    value.map(|value| value.collection_entries),
                ),
                (
                    WorkKind::TotalDecodedStringBytes,
                    value.map(|value| value.total_string_bytes),
                ),
                (
                    WorkKind::MaximumDecodedStringBytes,
                    value.map(|value| value.single_string_bytes),
                ),
            ] {
                let fact = work.facts.iter().find(|fact| fact.kind == kind).unwrap();
                let reason = UnavailableWork::SnapshotHoldsMarkers;
                let expected = count.map_or(WorkValue::Unavailable { reason }, WorkValue::exact);
                assert_eq!(fact.observation, expected, "{recipe:?} {kind:?}");
            }
            assert_eq!(value.is_some(), recipe.materializes(), "{recipe:?}");
            let kinds: Vec<_> = work.facts.iter().map(|fact| fact.kind).collect();
            assert_eq!(kinds, WorkKind::SNAPSHOT, "{recipe:?}");
            for fact in &work.facts {
                assert_eq!(fact.stage, WorkStage::OperationResult, "{recipe:?}");
            }
        }
    }
}
