// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's Phase 1, `entry-admission`, over the Programmatic Entry
//! Snapshot the boundary admitted. As for a file, the phase's first kind owns
//! the entry dependency records and waits on nothing. Of the complete input's
//! checks, a Snapshot records those it has something to examine, in
//! check-rank order (D15:2129; Decision 015-202): the nine Snapshot-bound
//! checks, which read the admitted Snapshot alone, the JSON-compatible domain
//! on them, and the five root-value bounds on the domain (D15:1876).
//!
//! The boundary admitted at most one bootstrap marker, the terminal one: the
//! last node in canonical traversal order, its witness its bound's limit plus
//! one, and the tree without it and the edge that holds it within every
//! Snapshot bound (D15:867, 869). Phase 1 counts neither for any bound, so
//! only the check of the bound the marker names fails, reporting that bound,
//! its limit, and the witness as an at-least total, and the other eight hold
//! (D15:2006, 2008, 2356). Each ordinary marker is a candidate of the
//! JSON-compatible domain, its reason the marker's (D15:861, 2289). The
//! domain reports them even when a terminal marker follows, since the values
//! that prove them stay admitted, and without them passes only once every
//! Snapshot bound holds, never on a suffix no inspection reached (D15:2006,
//! 2365; plan 025 decision #266). The root-value bounds count the marker-free
//! tree as they count a file of the same value (D15:1889) and report alike,
//! and a document exists only when every check holds.
//!
//! Every candidate is observed through the Programmatic Entry's Entry Source
//! Evidence: its identity and locator, no span, and the Logical input path the
//! resolver projects from the position's Snapshot Structural Path, each
//! member classified as for a file, so no raw key is retained (D15:792-798,
//! 802, 895, 897). A terminal marker for a key past a string bound stands at
//! the object that owns the key, and is observed there (D15:867). A bound is
//! observed beside the capability's bound it exceeds.

use crate::artifacts::count;
use crate::boundary::snapshot::AdmittedSnapshot;
use crate::input_limits::{Bound, ValueLimits};
use crate::materialize::MaterializedDocument;
use crate::snapshot::{Marker, MarkerReason, Node, SnapshotBound, SnapshotCounts};
use crate::structural::Edge;

use super::capability::{CapabilityBound, SnapshotLimits};
use super::entry::{
    capability, input, root_value_bounds, Classify, EntryInvariant, OWNER, ROOT_VALUE_BOUNDS,
};
use super::evaluation::{
    and, Actual, Admitted, Blocking, BootstrapLimitExceeded, Check, Code, Evaluation, Found,
    Observation, Reason,
};
use super::evidence::{EvidenceProjection, Source};
use super::origin::SafeOriginKind;

/// The check of each `bootstrapOnly.snapshot` bound, in the bounds' table
/// order, which is check-rank order.
pub(crate) const SNAPSHOT_BOUNDS: [(Check, SnapshotBound); 9] = [
    (Check::WithinSnapshotNodes, SnapshotBound::Nodes),
    (Check::WithinSnapshotDepth, SnapshotBound::Depth),
    (
        Check::WithinSnapshotCollectionEntries,
        SnapshotBound::CollectionEntries,
    ),
    (
        Check::WithinSnapshotTotalStringBytes,
        SnapshotBound::TotalStringBytes,
    ),
    (
        Check::WithinSnapshotSingleStringBytes,
        SnapshotBound::SingleStringBytes,
    ),
    (
        Check::WithinSnapshotRejectedMarkers,
        SnapshotBound::RejectedMarkers,
    ),
    (
        Check::WithinSnapshotEnvelopeCanonicalBytes,
        SnapshotBound::EnvelopeCanonicalBytes,
    ),
    (
        Check::WithinSnapshotSingleMarkerCanonicalBytes,
        SnapshotBound::SingleMarkerCanonicalBytes,
    ),
    (
        Check::WithinSnapshotTraversalUnits,
        SnapshotBound::TraversalUnits,
    ),
];

/// The Snapshot Phase 1 reads, and what its Evidence names: the source the
/// Programmatic Entry was admitted under, and the classification of the
/// members on a position's path, as a file's (D15:776, 792-798).
pub(crate) struct EntrySnapshot<'a> {
    pub(crate) snapshot: &'a AdmittedSnapshot,
    pub(crate) source: Source<'a>,
    pub(crate) classify: &'a Classify,
}

/// The bounds Phase 1 reads a Snapshot under: those the Admitted
/// Implementation Capability gives `bootstrapOnly.snapshot`, and the
/// root-value bounds of `policyComparable.configurationInput`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SnapshotEntryLimits {
    pub(crate) snapshot: SnapshotLimits,
    pub(crate) value: ValueLimits,
}

impl EntrySnapshot<'_> {
    /// Entry Source Evidence at the position the edges `path` lead to from
    /// the root, each member classified, without a span (D15:792-798, 897).
    fn evidence(&self, path: &[Edge<'_>]) -> Result<EvidenceProjection, EntryInvariant> {
        let steps = (self.classify)(path, None);
        EvidenceProjection::entry_source(
            SafeOriginKind::ConfigurationProgrammatic,
            self.source,
            Some(&steps),
            None,
        )
        .map_err(EntryInvariant::Evidence)
    }
}

/// Phase 1 over `entry` under `limits`: the materialized document, or the
/// causes of its absence, which are the blocking Findings of Phase 1 the
/// Shared projection is unavailable for (D15:2369).
///
/// # Errors
///
/// Returns the resolver bug Phase 1 found, a marker the boundary refuses
/// among them.
pub(crate) fn admit_snapshot_entry(
    evaluation: &mut Evaluation,
    entry: &EntrySnapshot<'_>,
    limits: SnapshotEntryLimits,
) -> Result<Admitted<MaterializedDocument>, EntryInvariant> {
    // The owner records the entry dependencies, and nothing waits on it.
    let _: Admitted<()> = evaluation.check(OWNER, Ok(()), |()| Ok::<(), Blocking>(()))?;
    let Findings {
        mut overrun,
        compatible,
        bounds,
    } = Findings::of(entry, limits)?;
    let mut within = Ok(());
    for (check, bound) in SNAPSHOT_BOUNDS {
        let named = overrun.take_if(|(named, _)| *named == bound);
        let held = evaluation.check_found(input(check), Ok(()), |()| {
            named.map_or(Ok(()), |(_, found)| Err(found))
        })?;
        within = and(within, held);
    }
    let compatible = evaluation.check_seen(input(Check::JsonCompatible), within, compatible)?;
    let mut bounded = Ok(());
    for ((check, _, _), exceeded) in ROOT_VALUE_BOUNDS.into_iter().zip(bounds) {
        let held = evaluation.check_found(input(check), compatible.clone(), |()| {
            exceeded.map_or(Ok(()), Err)
        })?;
        bounded = and(bounded, held);
    }
    match bounded {
        Err(causes) => Ok(Err(causes)),
        Ok(()) => MaterializedDocument::from_snapshot(entry.snapshot.root())
            .map(Ok)
            .ok_or(EntryInvariant::UnreportedFailure),
    }
}

/// What the checks find in the Snapshot, each candidate observed, before
/// they are evaluated: nothing for a check with nothing to report.
#[derive(Default)]
struct Findings {
    /// The terminal marker's, under the bound it names.
    overrun: Option<(SnapshotBound, Found)>,
    /// The JSON-compatible domain's: a candidate for each ordinary marker.
    compatible: Option<Found>,
    /// The root-value bounds', in check-rank order, which no check
    /// evaluates while a marker remains, as for a file's facts.
    bounds: [Option<Found>; 5],
}

impl Findings {
    /// What the checks find in `entry`'s Snapshot under `limits`.
    fn of(entry: &EntrySnapshot<'_>, limits: SnapshotEntryLimits) -> Result<Self, EntryInvariant> {
        let root = entry.snapshot.root();
        let scan = Scan::of(root, limits.value.max_single_string_bytes)?;
        let mut found = Self::default();
        if let Some((bound, witness, path)) = &scan.terminal {
            let overrun = overrun(entry, *bound, *witness, path, &limits.snapshot)?;
            found.overrun = Some((*bound, overrun));
        }
        let mut candidates = Vec::new();
        for (reason, path) in &scan.ordinary {
            let observation = Observation {
                primary: entry.evidence(path)?,
                related: Vec::new(),
            };
            candidates.push(Found::observed(incompatible(*reason), observation));
        }
        found.compatible = candidates.into_iter().reduce(Found::and);
        let violations = limits.value.violations(SnapshotCounts::of(root).value());
        found.bounds = root_value_bounds(&violations, &scan.long, |path| {
            entry.evidence(path.map_or(&[][..], Vec::as_slice))
        })?;
        Ok(found)
    }
}

/// The terminal marker's Finding under `bound`, which it names: the bound,
/// its limit, and `witness` as an at-least total, observed at the marker's
/// position `path` beside the capability's bound (D15:2006, 2356).
fn overrun(
    entry: &EntrySnapshot<'_>,
    bound: SnapshotBound,
    witness: u64,
    path: &[Edge<'_>],
    limits: &SnapshotLimits,
) -> Result<Found, EntryInvariant> {
    let limit = bound.limit(limits);
    // `Bound` excludes `u64::MAX`, so the witness the boundary admits is
    // representable.
    if witness != limit.get() + 1 {
        return Err(EntryInvariant::UnadmittedMarker);
    }
    let observation = Observation {
        primary: entry.evidence(path)?,
        related: vec![capability(CapabilityBound::BootstrapOnly(bound.id()))?],
    };
    let exceeded = Blocking::bootstrap_limit(BootstrapLimitExceeded {
        bound: bound.id(),
        limit,
        actual: Actual::AtLeast(witness),
    });
    Ok(Found::observed(exceeded, observation))
}

/// The JSON-compatible domain's result for an ordinary marker of `reason`,
/// which keeps its reason (D15:861, 2289).
const fn incompatible(reason: MarkerReason) -> Blocking {
    let reason = match reason {
        MarkerReason::UnsupportedType => Reason::UnsupportedType,
        MarkerReason::NonFiniteNumber => Reason::NonFiniteNumber,
        MarkerReason::NonPortableNumber => Reason::NonPortableNumber,
        MarkerReason::InvalidUnicode => Reason::InvalidUnicode,
        MarkerReason::NonStringKey => Reason::NonStringKey,
        MarkerReason::DynamicContainer => Reason::DynamicContainer,
        MarkerReason::DynamicProperty => Reason::DynamicProperty,
        MarkerReason::SparseArray => Reason::SparseArray,
        MarkerReason::Cycle => Reason::Cycle,
    };
    Blocking {
        code: Code::InputNotJsonCompatible,
        reason: Some(reason),
        details: None,
    }
}

/// The edges from the root to a position in the Snapshot, the actual keys
/// among them, which only the classification reads (D15:895).
type Edges<'s> = Vec<Edge<'s>>;

/// What a canonical traversal of a Snapshot finds, each at its position:
/// array elements in index order, object members in key-byte order, each
/// node before what it holds and a key before its value (D15:867).
struct Scan<'s> {
    /// The single-string bound the root-value check applies.
    limit: Bound,
    /// Each ordinary marker, with its reason.
    ordinary: Vec<(MarkerReason, Edges<'s>)>,
    /// The terminal marker, with the bound it names and its witness.
    terminal: Option<(SnapshotBound, u64, Edges<'s>)>,
    /// Each string and key longer than `limit`, a key at its member.
    long: Vec<Edges<'s>>,
}

/// One step of a traversal below the root: entering the edge to a node, or
/// leaving the last edge entered.
enum Step<'s> {
    Enter(Edge<'s>, &'s Node),
    Leave,
}

impl<'s> Scan<'s> {
    /// The traversal of the tree at `root` under the single-string bound
    /// `limit`, without recursion, so a Snapshot as deep as the capability
    /// admits cannot exhaust the stack (plan 025 decision #298).
    fn of(root: &'s Node, limit: Bound) -> Result<Self, EntryInvariant> {
        let mut scan = Self {
            limit,
            ordinary: Vec::new(),
            terminal: None,
            long: Vec::new(),
        };
        let mut path: Edges<'s> = Vec::new();
        let mut pending: Vec<Step<'s>> = Vec::new();
        scan.visit(root, &path, &mut pending)?;
        while let Some(step) = pending.pop() {
            match step {
                Step::Enter(edge, node) => {
                    path.push(edge);
                    if let Edge::Member(key) = edge {
                        scan.string(key, &path);
                    }
                    pending.push(Step::Leave);
                    scan.visit(node, &path, &mut pending)?;
                }
                Step::Leave => {
                    path.pop();
                }
            }
        }
        Ok(scan)
    }

    /// Visits `node` at `path`, and puts the edges to what it holds on
    /// `pending`, the first on top.
    fn visit(
        &mut self,
        node: &'s Node,
        path: &[Edge<'s>],
        pending: &mut Vec<Step<'s>>,
    ) -> Result<(), EntryInvariant> {
        // The boundary admits no node after a bootstrap marker (D15:867).
        if self.terminal.is_some() {
            return Err(EntryInvariant::UnadmittedMarker);
        }
        match node {
            Node::Null | Node::Boolean(_) | Node::Number(_) => {}
            Node::String(text) => self.string(text, path),
            Node::Array(elements) => pending.extend(
                elements
                    .iter()
                    .enumerate()
                    .rev()
                    .map(|(index, element)| Step::Enter(Edge::Index(count(index)), element)),
            ),
            Node::Object(members) => pending.extend(
                members
                    .iter()
                    .rev()
                    .map(|(key, value)| Step::Enter(Edge::Member(key), value)),
            ),
            Node::Marker(Marker::Rejected(reason)) => self.ordinary.push((*reason, path.to_vec())),
            Node::Marker(Marker::BootstrapLimitExceeded { bound, witness }) => {
                self.terminal = Some((*bound, *witness, path.to_vec()));
            }
        }
        Ok(())
    }

    /// Notes the string or key `text` at `path` when it is past the bound.
    fn string(&mut self, text: &str, path: &[Edge<'s>]) {
        if count(text.len()) > self.limit.get() {
            self.long.push(path.to_vec());
        }
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use crate::boundary::snapshot::AdmittedSnapshot;
    use crate::boundary::source::{read_entry_pair, ExactIdentity, SourceLimits};
    use crate::input_limits::Bound;
    use crate::resolver::entry::fixtures::classify;
    use crate::resolver::entry::Classify;
    use crate::resolver::evidence::Source;
    use crate::resolver::limits::ResolutionLimits;
    use crate::snapshot::Node;

    use super::EntrySnapshot;

    /// The Programmatic Entry identity the test Snapshots are admitted under.
    pub(crate) const ENTRY: &str = "intlify-programmatic-entry.v0:storefront";

    /// What a test Snapshot is admitted under: the identity [`ENTRY`], which
    /// its frontend marks non-secret, and the classification a file's facts
    /// have.
    pub(crate) struct SnapshotFixture {
        identity: ExactIdentity,
        classify: Box<Classify>,
    }

    impl SnapshotFixture {
        pub(crate) fn new(comparable: &ResolutionLimits) -> Self {
            let largest = Bound::new(u64::MAX - 1).unwrap();
            let limits = SourceLimits {
                identity_bytes: largest,
                revision_bytes: largest,
            };
            let digest = Node::Null.digest();
            let identity = read_entry_pair(Some(ENTRY), Some(&digest), &digest, limits)
                .unwrap()
                .identity()
                .clone();
            Self {
                identity,
                classify: classify(comparable),
            }
        }

        /// The entry of `snapshot`, with no locator.
        pub(crate) fn entry<'a>(&'a self, snapshot: &'a AdmittedSnapshot) -> EntrySnapshot<'a> {
            EntrySnapshot {
                snapshot,
                source: Source {
                    identity: &self.identity,
                    marked: true,
                    locator: None,
                },
                classify: &*self.classify,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Arc;

    use serde_json::{json, Value};

    use crate::boundary::locator::{read_programmatic_locator, LocatorCandidate, LocatorVariant};
    use crate::boundary::snapshot::{read_report, submitted, AdmittedSnapshot, SnapshotReport};
    use crate::construction::registry::check::testing::{check_id, requirements};
    use crate::construction::registry::check::{CheckRegistry, RecordKind};
    use crate::fixtures::complete_config;
    use crate::input_limits::{Bound, InputLimits, ValueLimits};
    use crate::materialize::MaterializedDocument;
    use crate::materialize_tests::limits as file_limits;
    use crate::resolver::capability::{CapabilityBound, SnapshotLimits};
    use crate::resolver::entry::fixtures::FileFixture;
    use crate::resolver::entry::{
        admit_entry, capability, input, EntryInvariant, OWNER, ROOT_VALUE_BOUNDS,
    };
    use crate::resolver::evaluation::{
        Actual, Admitted, BootstrapLimitExceeded, Check, Code, Evaluation, IssueKey, Reason,
        Status, Unit,
    };
    use crate::resolver::evidence::Evidence;
    use crate::resolver::evidence_token::EvidenceTokens;
    use crate::resolver::limits::fixture_limits;
    use crate::resolver::occurrence::{occurrences, redacted, FindingOccurrence};
    use crate::resolver::record_view::RecordViews;
    use crate::resolver::subject::{EntryPath, SubjectKind};
    use crate::resolver::token::{Sha256, Specification};
    use crate::snapshot::{from_json, Marker, MarkerReason, Node, SnapshotBound};
    use crate::structural::Edge;

    use super::fixtures::SnapshotFixture;
    use super::{admit_snapshot_entry, incompatible, Scan, SnapshotEntryLimits, SNAPSHOT_BOUNDS};

    const PROFILE: Specification<'static> = Specification {
        identity: "intlify-project-profile",
        revision: "0",
    };

    /// Limits under which the boundary admits every test tree, each Snapshot
    /// bound with a value of its own, and the file tests' root-value bounds.
    fn limits() -> SnapshotEntryLimits {
        let bound = |value| Bound::new(value).unwrap();
        SnapshotEntryLimits {
            snapshot: SnapshotLimits {
                max_nodes: bound(4101),
                max_depth: bound(4102),
                max_collection_entries: bound(4103),
                max_total_string_bytes: bound(4104),
                max_single_string_bytes: bound(4105),
                max_rejected_markers: bound(4106),
                max_envelope_canonical_bytes: bound(4107),
                max_single_marker_canonical_bytes: bound(4108),
                max_traversal_units: bound(4109),
            },
            value: file_limits().value,
        }
    }

    /// The terminal marker of a first overrun of `bound` under [`limits`].
    fn overrun(bound: SnapshotBound) -> Node {
        let witness = bound.limit(&limits().snapshot).get() + 1;
        Node::Marker(Marker::BootstrapLimitExceeded { bound, witness })
    }

    /// The canonical tree of `value`, where a string `"!id"` stands for the
    /// ordinary marker whose reason is `id`, or for the terminal marker of
    /// the `bootstrapOnly.snapshot` bound `id`, such as `"!maxNodes"`.
    fn tree(value: &Value) -> Node {
        let marker = |id: &str| {
            MarkerReason::from_id(id).map_or_else(
                || {
                    let path = format!("bootstrapOnly.snapshot.{id}");
                    overrun(SnapshotBound::from_path(&path).unwrap())
                },
                |reason| Node::Marker(Marker::Rejected(reason)),
            )
        };
        match value {
            Value::String(text) => text
                .strip_prefix('!')
                .map_or_else(|| from_json(value), marker),
            Value::Array(elements) => Node::Array(elements.iter().map(tree).collect()),
            Value::Object(members) => Node::Object(
                members
                    .iter()
                    .map(|(key, value)| (key.clone(), tree(value)))
                    .collect(),
            ),
            _ => from_json(value),
        }
    }

    /// `root` as the boundary admits it under `limits`.
    fn admitted(root: &Node, limits: &SnapshotLimits) -> AdmittedSnapshot {
        let report = SnapshotReport::Formal {
            root: submitted(root),
            digest: root.digest(),
        };
        read_report(Some(report), limits).unwrap()
    }

    /// Phase 1 over `snapshot` under `limits`: the evaluation, and the
    /// document or the causes of its absence.
    fn run(
        snapshot: &AdmittedSnapshot,
        limits: SnapshotEntryLimits,
    ) -> Result<(Evaluation, Admitted<MaterializedDocument>), EntryInvariant> {
        let fixture = SnapshotFixture::new(&fixture_limits(100));
        let mut evaluation = Evaluation::default();
        let admitted = admit_snapshot_entry(&mut evaluation, &fixture.entry(snapshot), limits)?;
        Ok((evaluation, admitted))
    }

    /// Phase 1 over `root` as the boundary admits it under `limits`.
    fn entered(
        root: &Node,
        limits: SnapshotEntryLimits,
    ) -> (Evaluation, Admitted<MaterializedDocument>) {
        run(&admitted(root, &limits.snapshot), limits).unwrap()
    }

    /// Phase 1 over the file of `value` under `limits`.
    fn filed(value: &Value, limits: InputLimits) -> (Evaluation, Admitted<MaterializedDocument>) {
        let fixture = FileFixture::new(&fixture_limits(100));
        let file = fixture.file(Arc::from(serde_json::to_vec(value).unwrap()));
        let mut evaluation = Evaluation::default();
        let admitted = admit_entry(&mut evaluation, &file, limits).unwrap();
        (evaluation, admitted)
    }

    /// The units an entry through `path` records, in canonical order: the
    /// owner, then the complete input's checks it examines.
    fn recorded(path: EntryPath) -> Vec<Unit> {
        let checks = SubjectKind::CompleteInput.checks().iter();
        std::iter::once(OWNER)
            .chain(
                checks
                    .filter(|check| check.recorded_for(path))
                    .map(|&check| input(check)),
            )
            .collect()
    }

    fn units(evaluation: &Evaluation) -> Vec<Unit> {
        evaluation
            .statuses()
            .map(|(unit, _)| unit.clone())
            .collect()
    }

    /// Every Finding, as its check, code, and reason.
    fn found(evaluation: &Evaluation) -> Vec<(Check, Code, Option<Reason>)> {
        evaluation
            .findings()
            .iter()
            .map(|finding| {
                (
                    finding.key.unit().check,
                    finding.key.code(),
                    finding.key.reason(),
                )
            })
            .collect()
    }

    /// Each check's status, with the checks of its causes when it was not
    /// evaluated.
    fn statuses(evaluation: &Evaluation) -> BTreeMap<Check, Option<Vec<Check>>> {
        evaluation
            .statuses()
            .map(|(unit, status)| {
                let causes = match status {
                    Status::Evaluated => None,
                    Status::NotEvaluated(causes) => {
                        Some(causes.keys().map(|key| key.unit().check).collect())
                    }
                };
                (unit.check, causes)
            })
            .collect()
    }

    /// The Finding Occurrences of what Phase 1 found, in 015's order, each
    /// checked against its Finding Registry row.
    fn observed(evaluation: &Evaluation) -> Vec<FindingOccurrence> {
        let candidates = evaluation.occurrence_candidates();
        let tokens = EvidenceTokens::assign(PROFILE, redacted(candidates), &Sha256);
        occurrences(candidates, PROFILE, &tokens).unwrap()
    }

    /// The view of `evidence` without the source identity every test entry
    /// shares.
    fn located(evidence: &Evidence) -> Value {
        let mut view = evidence.view();
        view.as_object_mut().unwrap().remove("source");
        view
    }

    /// The view of a Snapshot's Entry Source Evidence at the path `path`.
    fn at(path: &Value) -> Value {
        json!({ "kind": "entry-source", "origin": "configuration-programmatic", "path": path })
    }

    /// The Evidence of the capability's `bound`, which carries no token.
    fn beside(bound: CapabilityBound) -> Evidence {
        let tokens = EvidenceTokens::assign(PROFILE, redacted(&[]), &Sha256);
        capability(bound)
            .unwrap()
            .retained(PROFILE, &tokens)
            .unwrap()
    }

    /// The check of each Snapshot bound is one only a Snapshot records, in
    /// check-rank order, and the bounds follow their table.
    #[test]
    fn each_snapshot_bound_has_the_check_only_a_snapshot_records() {
        let checks: Vec<Check> = SubjectKind::CompleteInput
            .checks()
            .iter()
            .copied()
            .filter(|check| !check.recorded_for(EntryPath::File))
            .collect();
        assert_eq!(SNAPSHOT_BOUNDS.map(|(check, _)| check).to_vec(), checks);
        assert_eq!(SNAPSHOT_BOUNDS.map(|(_, bound)| bound), SnapshotBound::ALL);
    }

    #[test]
    fn a_canonical_snapshot_passes_every_check_and_admits_its_document() {
        let value = complete_config();
        let (evaluation, admitted) = entered(&from_json(&value), limits());
        assert_eq!(found(&evaluation), []);
        assert_eq!(units(&evaluation), recorded(EntryPath::Programmatic));
        assert_eq!(units(&evaluation).len(), 16);
        assert!(statuses(&evaluation).values().all(Option::is_none));
        let document = admitted.unwrap();
        assert_eq!(document.decode::<Value>(document.root()).unwrap(), value);
        assert_eq!(document.counts(), None);
    }

    /// D15:2129, 3354: a Snapshot records no raw-byte, parser-token,
    /// strict-syntax, or duplicate-member unit, a file no Snapshot-bound
    /// unit, and both the owner, the JSON-compatible domain, and the five
    /// root-value bounds (Decision 015-202).
    #[test]
    fn each_entry_records_only_the_checks_it_examines() {
        let value = json!({ "a": [1, "b"] });
        let (snapshot, admitted) = entered(&from_json(&value), limits());
        assert!(admitted.is_ok());
        let (file, admitted) = filed(&value, file_limits());
        assert!(admitted.is_ok());
        assert_eq!(units(&snapshot), recorded(EntryPath::Programmatic));
        assert_eq!(units(&file), recorded(EntryPath::File));
        let filed = units(&file);
        let shared: Vec<Check> = units(&snapshot)
            .into_iter()
            .filter(|unit| filed.contains(unit))
            .map(|unit| unit.check)
            .collect();
        assert_eq!(
            shared,
            [
                Check::EstablishedSlots,
                Check::JsonCompatible,
                Check::WithinNodes,
                Check::WithinDepth,
                Check::WithinCollectionEntries,
                Check::WithinTotalStringBytes,
                Check::WithinSingleStringBytes,
            ]
        );
    }

    /// D15:867, 869, 2006, 2008, 2356: a terminal marker fails only the check
    /// of the bound it names, with its limit and the witness as an at-least
    /// total, observed where it stands beside the capability's bound: at the
    /// root, as an array's last element, or in place of the object that
    /// owns a key past a string bound. The other bounds hold, and what needs
    /// the complete value waits on the overrun, as the Shared projection
    /// does.
    #[test]
    fn the_terminal_marker_reports_the_bound_it_names() {
        let limits = limits();
        let declaration = |member: &str| {
            json!([
                { "safe-member": "profiles" },
                { "safe-member": "app" },
                { "safe-member": member },
            ])
        };
        for (check, bound) in SNAPSHOT_BOUNDS {
            let name = format!("!{}", bound.id().name);
            let mut element = declaration("requestedLocales");
            element
                .as_array_mut()
                .unwrap()
                .push(json!({ "array-index": "1" }));
            let places = [
                (json!(name), json!([])),
                (
                    json!({ "profiles": { "app": { "requestedLocales": ["en", name] } } }),
                    element,
                ),
                (
                    json!({ "profiles": { "app": { "projectId": "storefront", "targetProfiles": name } } }),
                    declaration("targetProfiles"),
                ),
            ];
            for (value, path) in places {
                let (evaluation, admitted) = entered(&tree(&value), limits);
                assert_eq!(
                    found(&evaluation),
                    [(check, Code::BootstrapLimitExceeded, None)]
                );
                let key = evaluation.issues().next().unwrap().clone();
                let limit = bound.limit(&limits.snapshot);
                assert_eq!(
                    evaluation.bootstrap_limit(&key),
                    Some(BootstrapLimitExceeded {
                        bound: bound.id(),
                        limit,
                        actual: Actual::AtLeast(limit.get() + 1),
                    })
                );
                let occurrences = observed(&evaluation);
                assert_eq!(occurrences.len(), 1);
                assert_eq!(located(occurrences[0].primary()), at(&path));
                assert_eq!(
                    occurrences[0].related(),
                    [beside(CapabilityBound::BootstrapOnly(bound.id()))]
                );
                let held: BTreeMap<Check, Option<Vec<Check>>> = recorded(EntryPath::Programmatic)
                    .iter()
                    .map(|unit| {
                        let waits = unit.check >= Check::JsonCompatible;
                        (unit.check, waits.then(|| vec![check]))
                    })
                    .collect();
                assert_eq!(statuses(&evaluation), held);
                let causes: Vec<IssueKey> = admitted.err().unwrap().keys().cloned().collect();
                assert_eq!(causes, [key]);
            }
        }
    }

    /// The ordinary markers of a Snapshot that ends at a terminal marker,
    /// each at its position, among them one under a key the classification
    /// redacts: each with its reason.
    const MARKED: &str = r#"{
        "$schema": "!unsupported-type",
        "profiles": {
            "app": {
                "projectId": "!cycle",
                "requestedLocales": ["en", "!unsupported-type"],
                "x-secret": "!dynamic-property"
            }
        },
        "schemaVersion": "!maxRejectedMarkers"
    }"#;

    /// Plan 025 decision #266 (D15:2151, 2365, 2369): ordinary markers before
    /// the terminal marker are reported, one Finding for each reason and an
    /// occurrence for each marker, and what needs the complete value waits
    /// on both their Findings and the overrun, as the Shared projection does.
    #[test]
    fn ordinary_markers_before_the_terminal_marker_are_reported() {
        let value: Value = serde_json::from_str(MARKED).unwrap();
        let (evaluation, admitted) = entered(&tree(&value), limits());
        assert_eq!(
            found(&evaluation),
            [
                (
                    Check::WithinSnapshotRejectedMarkers,
                    Code::BootstrapLimitExceeded,
                    None
                ),
                (
                    Check::JsonCompatible,
                    Code::InputNotJsonCompatible,
                    Some(Reason::Cycle)
                ),
                (
                    Check::JsonCompatible,
                    Code::InputNotJsonCompatible,
                    Some(Reason::DynamicProperty)
                ),
                (
                    Check::JsonCompatible,
                    Code::InputNotJsonCompatible,
                    Some(Reason::UnsupportedType)
                ),
            ]
        );
        let statuses = statuses(&evaluation);
        assert_eq!(statuses[&Check::JsonCompatible], None);
        for (check, _, _) in ROOT_VALUE_BOUNDS {
            assert_eq!(
                statuses[&check],
                Some(vec![
                    Check::WithinSnapshotRejectedMarkers,
                    Check::JsonCompatible,
                    Check::JsonCompatible,
                    Check::JsonCompatible,
                ])
            );
        }
        let causes: Vec<IssueKey> = admitted.err().unwrap().keys().cloned().collect();
        assert_eq!(causes, evaluation.issues().cloned().collect::<Vec<_>>());
        let views = RecordViews::assign(PROFILE, &evaluation, &Sha256);
        for key in &causes {
            let view = views.shared_unavailability_cause(key).unwrap();
            assert_eq!(view, json!({ "cause": views.finding_key(key).unwrap() }));
            views
                .accounting(key.unit(), RecordKind::SharedUnavailabilityCause, view)
                .unwrap();
        }
        let safe = |name: &str| json!({ "safe-member": name });
        let declared = |member: Value| json!([safe("profiles"), safe("app"), member]);
        let occurrences = observed(&evaluation);
        let seen: Vec<(Option<Reason>, Value)> = occurrences
            .iter()
            .map(|occurrence| {
                let path = located(occurrence.primary())["path"].clone();
                (occurrence.finding().reason(), path)
            })
            .collect();
        let redacted = seen[2].1[2].clone();
        assert!(redacted.get("redacted-member").is_some());
        assert_eq!(
            seen,
            [
                (None, json!([safe("schemaVersion")])),
                (Some(Reason::Cycle), declared(safe("projectId"))),
                (Some(Reason::DynamicProperty), declared(redacted)),
                (Some(Reason::UnsupportedType), json!([safe("$schema")])),
                (
                    Some(Reason::UnsupportedType),
                    json!([
                        safe("profiles"),
                        safe("app"),
                        safe("requestedLocales"),
                        { "array-index": "1" }
                    ])
                ),
            ]
        );
        // No raw key enters Evidence (D15:895, 897).
        for occurrence in &occurrences {
            assert!(!occurrence.primary().view().to_string().contains("x-secret"));
        }
    }

    /// Without a terminal marker, every Snapshot bound holds, the ordinary
    /// markers are reported, and the root-value bounds wait on their
    /// Findings alone.
    #[test]
    fn ordinary_markers_alone_hold_back_the_root_value_bounds() {
        let mut value: Value = serde_json::from_str(MARKED).unwrap();
        value.as_object_mut().unwrap().remove("schemaVersion");
        let (evaluation, admitted) = entered(&tree(&value), limits());
        let checks: Vec<Check> = found(&evaluation)
            .into_iter()
            .map(|(check, _, _)| check)
            .collect();
        assert_eq!(checks, [Check::JsonCompatible; 3]);
        let statuses = statuses(&evaluation);
        for (check, _) in SNAPSHOT_BOUNDS {
            assert_eq!(statuses[&check], None);
        }
        for (check, _, _) in ROOT_VALUE_BOUNDS {
            assert_eq!(statuses[&check], Some(vec![Check::JsonCompatible; 3]));
        }
        assert_eq!(admitted.err().unwrap().keys().count(), 3);
        assert_eq!(observed(&evaluation).len(), 4);
    }

    /// D15:861, 2289: each ordinary marker reports its own reason, the one
    /// the marker carries.
    #[test]
    fn each_ordinary_marker_reports_its_reason() {
        let markers = MarkerReason::ALL.map(|reason| Node::Marker(Marker::Rejected(reason)));
        let (evaluation, _) = entered(&Node::Array(markers.to_vec()), limits());
        let mut ids: Vec<&str> = MarkerReason::ALL.iter().map(|reason| reason.id()).collect();
        ids.sort_unstable();
        let reasons: Vec<&str> = found(&evaluation)
            .into_iter()
            .map(|(_, _, reason)| reason.unwrap().as_str())
            .collect();
        assert_eq!(reasons, ids);
        for reason in MarkerReason::ALL {
            assert_eq!(incompatible(reason).reason.unwrap().as_str(), reason.id());
        }
    }

    /// A value whose key and first string are equally long and whose last
    /// string is one byte shorter, each member of which the schema defines,
    /// so either entry's path names it.
    const BOUNDED: &str = r#"{"profiles":["abcdefgh",true,"abcdefg"]}"#;

    /// The root-value bounds at [`BOUNDED`]'s counts minus `under`: 5 nodes,
    /// depth 3, 4 entries, 23 string bytes, and 8 in the longest.
    fn tight(under: u64) -> ValueLimits {
        let bound = |count: u64| Bound::new(count - under).unwrap();
        ValueLimits {
            max_nodes: bound(5),
            max_depth: bound(3),
            max_collection_entries: bound(4),
            max_total_string_bytes: bound(23),
            max_single_string_bytes: bound(8),
        }
    }

    /// D15:1876, 1889, 3228: the root-value bounds give the same Findings
    /// through either entry, at their exact counts and first over them, and
    /// observe them at the same paths beside the same bounds; only the
    /// origin, the source, and the span differ.
    #[test]
    fn the_root_value_bounds_report_the_same_findings_through_either_entry() {
        let value: Value = serde_json::from_str(BOUNDED).unwrap();
        for (under, reported, occurrences) in [(0, 0, 0), (1, 5, 6)] {
            let mut input = file_limits();
            input.value = tight(under);
            let (file, _) = filed(&value, input);
            let snapshot_limits = SnapshotEntryLimits {
                value: input.value,
                ..limits()
            };
            let (snapshot, admitted) = entered(&from_json(&value), snapshot_limits);
            assert_eq!(admitted.is_ok(), reported == 0);
            assert_eq!(found(&snapshot), found(&file));
            assert_eq!(found(&snapshot).len(), reported);
            let details = |evaluation: &Evaluation| {
                evaluation
                    .issues()
                    .map(|key| evaluation.limit(key))
                    .collect::<Vec<_>>()
            };
            assert_eq!(details(&snapshot), details(&file));
            let (file, snapshot) = (observed(&file), observed(&snapshot));
            // The single-string bound at the key and at the first string,
            // not at the last, which it allows.
            assert_eq!(snapshot.len(), occurrences);
            assert_eq!(snapshot.len(), file.len());
            for (file, snapshot) in file.iter().zip(&snapshot) {
                assert_eq!(snapshot.finding(), file.finding());
                assert_eq!(snapshot.related(), file.related());
                let mut expected = located(file.primary());
                let view = expected.as_object_mut().unwrap();
                // A file observes a string at its token, and a total at none.
                let single = file.finding().unit().check == Check::WithinSingleStringBytes;
                assert_eq!(view.remove("span").is_some(), single);
                view.insert("origin".to_owned(), json!("configuration-programmatic"));
                assert_eq!(located(snapshot.primary()), expected);
            }
        }
    }

    /// The units a Snapshot records, each made to block in turn, hold back
    /// exactly what the Check Registry says waits on them among those units:
    /// each wait is on a row the dependent requires, and each prerequisite a
    /// row lists is waited on (Decision 015-202; plan 025 decision #279).
    #[test]
    fn each_programmatic_stage_holds_back_what_waits_on_it() {
        let registry = CheckRegistry::revision_zero();
        let requirements = requirements(&registry);
        let row = |unit: &Unit| check_id(unit.subject.kind(), unit.check);
        let limits = limits();
        let snapshot = admitted(&from_json(&json!({ "a": [1, "b"] })), &limits.snapshot);
        let fixture = SnapshotFixture::new(&fixture_limits(100));
        let units = recorded(EntryPath::Programmatic);
        let mut waits = BTreeSet::new();
        for blocked in &units {
            let mut evaluation = Evaluation::blocking(blocked.clone());
            let admitted =
                admit_snapshot_entry(&mut evaluation, &fixture.entry(&snapshot), limits).unwrap();
            assert_eq!(admitted.is_ok(), *blocked == OWNER, "{blocked:?}");
            for (unit, status) in evaluation.statuses() {
                if let Status::NotEvaluated(causes) = status {
                    if causes.keys().any(|cause| cause.unit() == blocked) {
                        waits.insert((row(unit), row(blocked)));
                    }
                }
            }
        }
        for &(dependent, prerequisite) in &waits {
            assert!(
                requirements[dependent].contains(prerequisite),
                "{dependent} waits on {prerequisite}"
            );
        }
        let programmatic: BTreeSet<&str> = units.iter().map(row).collect();
        for listing in &registry.rows {
            let dependent = listing.check_id.as_str();
            if !programmatic.contains(dependent) {
                continue;
            }
            for prerequisite in &listing.prerequisite_check_ids {
                if programmatic.contains(prerequisite.as_str()) {
                    assert!(
                        waits.contains(&(dependent, prerequisite.as_str())),
                        "{dependent} lists {prerequisite}"
                    );
                }
            }
        }
        // The domain waits on the nine bounds, and each root-value bound on
        // them and on the domain; nothing waits on the owner.
        assert_eq!(waits.len(), 9 + 5 * (9 + 1));
    }

    /// D15:2369: a document exists only when every check a Snapshot records
    /// admits it (Decision 015-202).
    #[test]
    fn a_document_needs_every_programmatic_check_to_admit_it() {
        let limits = limits();
        let snapshot = admitted(&from_json(&json!({ "a": 1 })), &limits.snapshot);
        let fixture = SnapshotFixture::new(&fixture_limits(100));
        for &check in SubjectKind::CompleteInput
            .checks()
            .iter()
            .filter(|check| check.recorded_for(EntryPath::Programmatic))
        {
            let mut evaluation = Evaluation::blocking(input(check));
            let admitted =
                admit_snapshot_entry(&mut evaluation, &fixture.entry(&snapshot), limits).unwrap();
            assert!(admitted.is_err(), "{check:?}");
        }
    }

    /// D15:867, 869: a bootstrap marker the boundary refuses is a resolver
    /// bug: a witness other than its bound's limit plus one under the bounds
    /// Phase 1 reads, a node after the marker, or a second marker.
    #[test]
    fn a_marker_the_boundary_refuses_is_a_resolver_bug() {
        let limits = limits();
        let nodes = |witness| {
            Node::Array(vec![Node::Marker(Marker::BootstrapLimitExceeded {
                bound: SnapshotBound::Nodes,
                witness,
            })])
        };
        let terminal = || overrun(SnapshotBound::Nodes);
        let one = || from_json(&json!(1));
        let refused = |root: Node, limits| run(&AdmittedSnapshot::of(root), limits).err();
        for root in [
            nodes(4101),
            nodes(4103),
            Node::Array(vec![terminal(), one()]),
            Node::Array(vec![terminal(), terminal()]),
            Node::Array(vec![Node::Array(vec![terminal()]), one()]),
            tree(&json!({ "a": "!maxNodes", "b": 1 })),
        ] {
            assert_eq!(
                refused(root, limits),
                Some(EntryInvariant::UnadmittedMarker)
            );
        }
        let mut other = limits;
        other.snapshot.max_nodes = Bound::new(4100).unwrap();
        assert_eq!(
            refused(nodes(4102), other),
            Some(EntryInvariant::UnadmittedMarker)
        );
        assert_eq!(refused(nodes(4102), limits), None);
    }

    /// A tree far deeper than a stack holds is scanned without recursion,
    /// each candidate at its full path: a long string and an ordinary marker
    /// below arrays, and a long key holding the terminal marker below objects
    /// (plan 025 decision #298).
    #[test]
    fn a_deep_snapshot_is_scanned_without_recursion() {
        const LEVELS: usize = 100_000;
        let long = "x".repeat(5);
        let terminal = Node::Marker(Marker::BootstrapLimitExceeded {
            bound: SnapshotBound::Nodes,
            witness: 2,
        });
        let mut array = Node::Array(vec![
            Node::String(long.clone()),
            Node::Marker(Marker::Rejected(MarkerReason::Cycle)),
        ]);
        let mut object = Node::Object(BTreeMap::from([(long.clone(), terminal)]));
        for _ in 0..LEVELS {
            array = Node::Array(vec![array]);
            object = Node::Object(BTreeMap::from([("a".to_owned(), object)]));
        }
        let limit = Bound::new(4).unwrap();
        let scan = Scan::of(&array, limit).unwrap();
        let below = |last| {
            let mut path = vec![Edge::Index(0); LEVELS];
            path.push(Edge::Index(last));
            path
        };
        assert_eq!(scan.long, [below(0)]);
        assert_eq!(scan.ordinary, [(MarkerReason::Cycle, below(1))]);
        assert_eq!(scan.terminal, None);
        let scan = Scan::of(&object, limit).unwrap();
        let mut path = vec![Edge::Member("a"); LEVELS];
        path.push(Edge::Member(&long));
        assert_eq!(scan.long, [path.clone()]);
        assert!(scan.ordinary.is_empty());
        assert_eq!(scan.terminal, Some((SnapshotBound::Nodes, 2, path)));
    }

    /// D15:792-798: a Snapshot's Entry Source Evidence names the Programmatic
    /// Entry's identity and the locator admitted with it, and has no span.
    #[test]
    fn a_snapshot_is_observed_through_its_programmatic_entry() {
        let limits = limits();
        let snapshot = admitted(&tree(&json!(["!cycle"])), &limits.snapshot);
        let fixture = SnapshotFixture::new(&fixture_limits(100));
        let label = LocatorCandidate {
            variant: LocatorVariant::SafeLabel,
            payload: "storefront entry",
            non_secret: true,
        };
        let locator = read_programmatic_locator(Some(label), Bound::new(64).unwrap()).unwrap();
        let mut entry = fixture.entry(&snapshot);
        entry.source.locator = Some(&locator);
        let mut evaluation = Evaluation::default();
        let admitted = admit_snapshot_entry(&mut evaluation, &entry, limits).unwrap();
        assert!(admitted.is_err());
        // The fixture identity's bytes in hexadecimal, computed outside this
        // crate.
        let identity =
            "696e746c6966792d70726f6772616d6d617469632d656e7472792e76303a73746f726566726f6e74";
        assert_eq!(
            observed(&evaluation)[0].primary().view(),
            json!({
                "kind": "entry-source",
                "origin": "configuration-programmatic",
                "source": { "safe-opaque": identity },
                "locator": { "safe-label": "storefront entry" },
                "path": [{ "array-index": "0" }],
            })
        );
    }
}
