// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's `programmatic_snapshot_admission`, the programmatic half of
//! `profile_entry_materialize`: from an already constructed inert Snapshot
//! through the document of its value (D15:2765; plan 025 decision #316).
//!
//! A case holds the formal Snapshot a binding submits, built before any
//! interval as live-host inspection is (D15:2824). The interval admits it at
//! the `programmatic-entry-snapshot` boundary under the case's
//! `bootstrapOnly.snapshot` bounds and builds the document of the admitted
//! tree, so it ends with a document, as `file_decode_and_source_map` does.
//! The boundary borrows the report, so the interval releases nothing built
//! before it (D15:2805; plan 025 decision #323). A tree with a marker is
//! admitted with no document, which Phase 1 blocks (D15:140, 2129); a
//! refused report is no case's result (D15:2826).
//!
//! The Shared domain frames the document's value as a file document's is
//! framed, so a file and a Snapshot of one value observe equal Shared
//! domains (D15:2788, 2794); a tree with a marker has no value. The Entry
//! domain frames the admitted Snapshot: its digest, its counts under the
//! nine bounds, and its markers in canonical order.

use crate::boundary::snapshot::{read_report, AdmittedSnapshot, SnapshotRefusal, SnapshotReport};
use crate::materialize::MaterializedDocument;
use crate::resolver::capability::SnapshotLimits;
use crate::snapshot::{Marker, Node, SnapshotCounts};

use super::observation::{self, Frame, Observation};
use super::operation::OutputFailure;

/// What a case submits, built before any interval: the formal Snapshot and
/// the bounds it is admitted under.
pub(super) struct PreparedSnapshot {
    pub(super) report: SnapshotReport,
    pub(super) limits: SnapshotLimits,
}

/// The admitted Snapshot and the document of its value, none when the tree
/// holds a marker. Both own their values, so nothing is copied out.
pub(super) struct Admitted {
    pub(super) snapshot: AdmittedSnapshot,
    pub(super) document: Option<MaterializedDocument>,
}

pub(super) type Resolved = Result<Admitted, SnapshotRefusal>;

/// The interval: the boundary's admission of `report` under `limits`, and
/// the document of the admitted tree (D15:2765).
pub(super) fn admit((report, limits): (&SnapshotReport, SnapshotLimits)) -> Resolved {
    let snapshot = read_report(Some(report), &limits)?;
    let document = MaterializedDocument::from_snapshot(snapshot.root());
    Ok(Admitted { snapshot, document })
}

/// The markers of the tree at `root` in canonical preorder, found without
/// recursion (plan 025 decision #298).
fn markers(root: &Node) -> Vec<Marker> {
    let mut found = Vec::new();
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        match node {
            Node::Array(elements) => pending.extend(elements.iter().rev()),
            Node::Object(members) => pending.extend(members.values().rev()),
            Node::Marker(marker) => found.push(*marker),
            Node::Null | Node::Boolean(_) | Node::Number(_) | Node::String(_) => {}
        }
    }
    found
}

/// A refused report is a benchmark failure (D15:2826).
pub(super) fn observe(resolved: &Resolved) -> Result<Observation, OutputFailure> {
    let admitted = resolved
        .as_ref()
        .map_err(|_| OutputFailure::SnapshotAdmissionRefused)?;
    let shared = admitted.document.as_ref().map_or_else(
        || Frame::new("unmaterialized-value").finish(),
        |document| observation::value(document, |_| {}),
    );
    let root = admitted.snapshot.root();
    let mut entry = Frame::new("programmatic-snapshot");
    entry.text(admitted.snapshot.digest());
    for (_, count) in SnapshotCounts::of(root).by_bound() {
        entry.uint(count);
    }
    let markers = markers(root);
    entry.uint(u64::try_from(markers.len()).expect("bounded markers"));
    for marker in markers {
        entry.bytes(&marker.record());
    }
    Ok(Observation {
        shared,
        entry: Some(entry.finish()),
    })
}

#[cfg(test)]
pub(in crate::benchmark) mod tests {
    use std::any::type_name;
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    use intlify_measurement::measurement::Aggregation;
    use serde_json::Value;

    use super::*;
    use crate::benchmark::cases::prepare::{prepare, Candidate};
    use crate::benchmark::cases::snapshot_inputs::{limits, report, SnapshotRecipe as R};
    use crate::benchmark::cases::{declarations, Recipe};
    use crate::benchmark::clock::tests::ScriptedClock;
    use crate::benchmark::instrument::{Instrument, Interval, Timed};
    use crate::benchmark::operation::Operation;
    use crate::benchmark::quantity::Quantity;
    use crate::snapshot::MarkerReason;

    /// What a binding submits for `recipe`, admitted untimed.
    fn admitted(recipe: R) -> Admitted {
        let report = report(recipe).unwrap();
        admit((&report, limits())).ok().unwrap()
    }

    fn observed(recipe: R) -> Observation {
        observe(&Ok(admitted(recipe))).unwrap()
    }

    /// `recipe`'s declared case, as preparation makes it.
    pub(in crate::benchmark) fn candidate(recipe: R) -> Candidate {
        let declaration = declarations()
            .into_iter()
            .find(|case| case.fixture == Recipe::Snapshot(recipe));
        prepare(&declaration.unwrap()).unwrap()
    }

    /// The `result` the expectations pin for the case of `operation` over
    /// `fixture` with no bound edge.
    pub(in crate::benchmark) fn expected(operation: Operation, fixture: Recipe) -> Value {
        let document: Value =
            serde_json::from_str(include_str!("cases/expectations-v0.json")).unwrap();
        let declaration = |row: &&Value| {
            row["declaration"]["operation"] == serde_json::to_value(operation).unwrap()
                && row["declaration"]["fixture"] == serde_json::to_value(fixture).unwrap()
                && row["declaration"]["limit"].is_null()
        };
        let rows = document["rows"].as_array().unwrap();
        let found: Vec<_> = rows.iter().filter(declaration).collect();
        assert_eq!(found.len(), 1, "{operation:?} {fixture:?}");
        found[0]["result"].clone()
    }

    /// D15:2765, 2824: each recipe's timed admission ends with the document
    /// of its value, built inside the interval, exactly when the tree holds
    /// no marker, and is observed as an untimed one is.
    #[test]
    fn each_recipe_is_admitted_in_its_interval_through_its_document() {
        for recipe in R::ALL {
            let report = report(recipe).unwrap();
            let clock = ScriptedClock::nanos([0, 1]);
            let timed = Timed(&clock).interval((&report, limits()), admit).unwrap();
            let timed = timed.output.ok().unwrap();
            assert_eq!(
                timed.document.is_some(),
                recipe.materializes(),
                "{recipe:?}"
            );
            assert_eq!(observe(&Ok(timed)), Ok(observed(recipe)), "{recipe:?}");
        }
    }

    /// D15:2788, 2794: a Snapshot of a file's value frames the Shared domain
    /// the expectations pin for that file's `file_decode_and_source_map`
    /// case, and its own Entry domain.
    #[test]
    fn a_snapshot_frames_the_shared_domain_its_files_case_pins() {
        for recipe in R::ALL.into_iter().filter(|recipe| recipe.materializes()) {
            let observation = serde_json::to_value(observed(recipe)).unwrap();
            let file = expected(Operation::FileMaterialization, recipe.file());
            assert_eq!(observation["shared"], file["shared"], "{recipe:?}");
            assert_ne!(observation["entry"], file["entry"], "{recipe:?}");
        }
    }

    /// D15:871, 889: members submitted in the reverse order and the same
    /// value submitted in order are one canonical tree, so both domains are
    /// observed alike.
    #[test]
    fn reversed_members_are_observed_as_the_minimal_snapshot() {
        let minimal = observed(R::Minimal);
        assert_eq!(observed(R::ReversedMembers), minimal);
        assert_ne!(observed(R::LongString).entry, minimal.entry);
    }

    /// D15:140, 861: a tree with a marker is admitted with no document, so
    /// its Shared domain frames no value, and its Entry domain frames its
    /// markers' records in canonical order.
    #[test]
    fn a_marked_snapshot_is_admitted_without_a_document() {
        let unmaterialized = Frame::new("unmaterialized-value").finish();
        let mut entries = Vec::new();
        for (recipe, count) in [(R::UnsupportedType, 1), (R::Cycle, 1), (R::ManyMarkers, 64)] {
            let admitted = admitted(recipe);
            assert!(admitted.document.is_none(), "{recipe:?}");
            let observation = observe(&Ok(admitted)).unwrap();
            assert_eq!(observation.shared, unmaterialized);
            let found = markers(admitted_root(recipe).root());
            let reasons = MarkerReason::ALL.into_iter().cycle();
            let expected: Vec<_> = match recipe {
                R::UnsupportedType => vec![MarkerReason::UnsupportedType],
                R::Cycle => vec![MarkerReason::Cycle],
                _ => reasons.take(count).collect(),
            };
            let expected: Vec<_> = expected.into_iter().map(Marker::Rejected).collect();
            assert_eq!(found, expected, "{recipe:?}");
            entries.push(observation.entry);
        }
        entries.sort();
        entries.dedup();
        assert_eq!(entries.len(), 3);
    }

    /// The admitted Snapshot of `recipe`.
    fn admitted_root(recipe: R) -> AdmittedSnapshot {
        admitted(recipe).snapshot
    }

    /// Markers are found in canonical preorder: an array's elements by
    /// index and an object's members by key, each before what follows it.
    #[test]
    fn markers_are_found_in_canonical_order() {
        let marker = |reason| Node::Marker(Marker::Rejected(reason));
        let tree = Node::Object(BTreeMap::from([
            ("a".to_owned(), marker(MarkerReason::Cycle)),
            (
                "b".to_owned(),
                Node::Array(vec![
                    marker(MarkerReason::UnsupportedType),
                    Node::Null,
                    marker(MarkerReason::SparseArray),
                ]),
            ),
            (
                "c".to_owned(),
                Node::Object(BTreeMap::from([(
                    "d".to_owned(),
                    marker(MarkerReason::NonStringKey),
                )])),
            ),
        ]));
        let expected = [
            MarkerReason::Cycle,
            MarkerReason::UnsupportedType,
            MarkerReason::SparseArray,
            MarkerReason::NonStringKey,
        ];
        assert_eq!(markers(&tree), expected.map(Marker::Rejected));
    }

    /// The Entry domain frames the admitted digest, then each of the nine
    /// counts in the bounds' order, then the markers' count and each
    /// marker's record; a refused report is no observation (D15:2826).
    #[test]
    fn the_entry_domain_is_framed_as_the_codec_spells_it() {
        let snapshot = admitted_root(R::Cycle);
        let counts = SnapshotCounts::of(snapshot.root());
        let mut entry = Frame::new("programmatic-snapshot");
        entry.text(snapshot.digest());
        for count in [
            counts.nodes,
            counts.depth,
            counts.collection_entries,
            counts.total_string_bytes,
            counts.single_string_bytes,
            counts.rejected_markers,
            counts.envelope_canonical_bytes,
            counts.single_marker_canonical_bytes,
            counts.traversal_units,
        ] {
            entry.uint(count);
        }
        entry.uint(1);
        entry.bytes(&Marker::Rejected(MarkerReason::Cycle).record());
        assert_eq!(observed(R::Cycle).entry, Some(entry.finish()));
        let refused = admit((&SnapshotReport::Unsafe, limits()));
        let failure = Err(OutputFailure::SnapshotAdmissionRefused);
        assert_eq!(observe(&refused), failure);
    }

    /// D15:2765: each case is the cost 015 names over its recipe's file,
    /// and its interval observes what the case's preparation does and its
    /// expectation pins.
    #[test]
    fn each_case_observes_what_its_expectation_pins() {
        let operation = Operation::ProgrammaticSnapshotAdmission;
        let named = (operation.phase(), operation.cost(), operation.boundary());
        let pair = (
            "profile_entry_materialize",
            "programmatic_snapshot_admission",
        );
        assert_eq!(named, (pair.0, pair.1, "programmatic-snapshot-admission/0"));
        for recipe in R::ALL {
            let candidate = candidate(recipe);
            let source = candidate.declaration.fixture.source();
            assert_eq!(source, recipe.file().source(), "{recipe:?}");
            assert_eq!(candidate.observation, observed(recipe), "{recipe:?}");
            let clock = ScriptedClock::nanos([0, 1]);
            let timed = candidate.prepared.interval(&Timed(&clock)).unwrap();
            assert_eq!(timed.output.observe(), Ok(candidate.observation));
            let pinned = expected(operation, Recipe::Snapshot(recipe));
            let observation = serde_json::to_value(candidate.observation).unwrap();
            assert_eq!(pinned, observation, "{recipe:?}");
        }
    }

    /// D15:2765, 2824: a case's interval is handed the submitted report and
    /// its bounds, never an admitted tree, and hands back the admitted tree
    /// beside its document.
    #[test]
    fn the_interval_is_handed_the_report_and_hands_back_the_document() {
        struct Recording(RefCell<Vec<(&'static str, &'static str)>>);
        impl Instrument for Recording {
            type Failure = ();
            const AGGREGATION: Aggregation = Aggregation::BatchTotal;
            fn interval<Input, Output>(
                &self,
                input: Input,
                invoke: fn(Input) -> Output,
            ) -> Result<Interval<Output>, ()> {
                let types = (type_name::<Input>(), type_name::<Output>());
                self.0.borrow_mut().push(types);
                let output = invoke(input);
                Ok(Interval {
                    quantity: Quantity::new(0),
                    output,
                })
            }
            fn unavailable() {}
        }
        let candidate = candidate(R::Minimal);
        let recording = Recording(RefCell::new(Vec::new()));
        let measured = candidate.prepared.interval(&recording).unwrap();
        assert_eq!(measured.output.observe(), Ok(candidate.observation));
        let handed = recording.0.into_inner();
        let input = type_name::<(&SnapshotReport, SnapshotLimits)>();
        assert_eq!(handed, [(input, type_name::<Resolved>())]);
    }
}
