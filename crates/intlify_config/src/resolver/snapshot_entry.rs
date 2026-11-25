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
//! Every candidate is observed through the programmatic entry's Entry Source
//! Evidence, at the Logical input path the resolver projects from the
//! position's Snapshot Structural Path, each member classified as for a file,
//! so no raw key is retained (D15:792-798, 802, 895, 897). As for a file, the
//! path is projected only within the path bounds path admission applies, past
//! which it would be absent, while the position is kept, since the sidecar
//! maps a span by it. Positions share their prefixes, as a file's facts'
//! paths do, so the edges Phase 1 holds grow with the nodes on the
//! candidates' paths, never with the candidates times their depth (D15:776,
//! 1834, 1851; plan 025 decision #299). A marker and a
//! string are the values at their positions, a key past the single-string
//! bound is its member's key, and a total is the root's value. The Evidence
//! names the call-site source and the span the admitted sidecar maps for the
//! role that location selects, and without such a span the Programmatic
//! Entry's identity and locator (D15:747, 796, 798, 802). A terminal marker
//! for a key past a string bound stands at the object that owns the key, and
//! is observed there (D15:867). A bound is observed beside the capability's
//! bound it exceeds.
//!
//! The phases after Phase 1 observe the entry through [`ProgrammaticEntry`],
//! which names a value or a key of the document by its locus, under the
//! programmatic slots' Safe Origin Kinds (D15:204, 800, 804).

use crate::artifacts::count;
use crate::boundary::call_site::AdmittedCallSite;
use crate::boundary::snapshot::AdmittedSnapshot;
use crate::input_limits::{Bound, ValueLimits};
use crate::materialize::{Locus, MaterializedDocument};
use crate::snapshot::{Marker, MarkerReason, Node, SnapshotBound, SnapshotCounts};
use crate::structural::Edge;

use super::call_site_span::{mapped, named, CallSiteSpans, Located};
use super::capability::{CapabilityBound, EvidenceMetadataLimits, SnapshotLimits};
use super::entry::{
    capability, input, root_value_bounds, Classify, ConfigurationEntry, EntryInvariant, OWNER,
    ROOT_VALUE_BOUNDS,
};
use super::evaluation::{
    and, Actual, Admitted, Blocking, BootstrapLimitExceeded, Check, Code, Evaluation, Found,
    Observation, Reason,
};
use super::evidence::{EvidenceProjection, Source};
use super::evidence_source::SourceSpan;
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
/// Programmatic Entry was admitted under, the call-site sidecar admitted
/// beside it, if one was submitted, and the classification of the members on
/// a position's path, as a file's (D15:776, 792-798).
pub(crate) struct EntrySnapshot<'a> {
    pub(crate) snapshot: &'a AdmittedSnapshot,
    pub(crate) source: Source<'a>,
    pub(crate) call_site: Option<&'a AdmittedCallSite>,
    pub(crate) classify: &'a Classify<'a>,
}

/// The bounds Phase 1 reads a Snapshot under: those the Admitted
/// Implementation Capability gives `bootstrapOnly.snapshot`, and the
/// root-value bounds of `policyComparable.configurationInput`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SnapshotEntryLimits {
    pub(crate) snapshot: SnapshotLimits,
    pub(crate) value: ValueLimits,
}

/// What Phase 1 observes a Snapshot's candidates through: `entry` and the
/// path bounds `paths` path admission applies (plan 025 decision #299).
struct Observer<'e, 'a> {
    entry: &'e EntrySnapshot<'a>,
    paths: &'e EvidenceMetadataLimits,
}

impl Observer<'_, '_> {
    /// Entry Source Evidence of `located` at the position the edges `path`
    /// lead to from the root, each member classified, its Logical input path
    /// projected only within the path bounds: in the call-site source at the
    /// span the sidecar maps for the role there, or without one in the
    /// Programmatic Entry (D15:747, 792-798, 802, 897).
    fn evidence(
        &self,
        path: &[Edge<'_>],
        located: Located,
    ) -> Result<EvidenceProjection, EntryInvariant> {
        let steps = (self.entry.classify)(path, None);
        let mapped = self
            .entry
            .call_site
            .and_then(|call_site| mapped(call_site, path, located));
        let (source, span) = named(self.entry.source, mapped);
        EvidenceProjection::entry_source_under(
            SafeOriginKind::ConfigurationProgrammatic,
            source,
            Some(&steps),
            span,
            self.paths,
        )
        .map_err(EntryInvariant::Evidence)
    }
}

/// A programmatic entry as the phases after `entry-admission` observe it: the
/// Programmatic Entry's source, the classification of the members on a
/// value's path, and the spans its admitted sidecar, if any, gives the
/// document Phase 1 materialized (D15:776, 792-800).
pub(crate) struct ProgrammaticEntry<'a> {
    source: Source<'a>,
    classify: &'a Classify<'a>,
    spans: Option<CallSiteSpans<'a>>,
}

impl<'a> ProgrammaticEntry<'a> {
    /// `entry` over `document`, the document Phase 1 materialized from its
    /// Snapshot.
    pub(crate) fn new(entry: &EntrySnapshot<'a>, document: &MaterializedDocument) -> Self {
        Self {
            source: entry.source,
            classify: entry.classify,
            spans: entry
                .call_site
                .map(|call_site| CallSiteSpans::new(call_site, document)),
        }
    }
}

/// A programmatic entry's slots are `configuration-programmatic` and
/// `profile-selector-programmatic`, and a value or a key is named in the
/// call-site source at the span the sidecar maps for it, and otherwise in the
/// Programmatic Entry without a span (D15:204, 800).
impl ConfigurationEntry for ProgrammaticEntry<'_> {
    fn origin(&self) -> SafeOriginKind {
        SafeOriginKind::ConfigurationProgrammatic
    }

    fn selector_origin(&self) -> SafeOriginKind {
        SafeOriginKind::ProfileSelectorProgrammatic
    }

    fn classify(&self) -> &Classify<'_> {
        self.classify
    }

    fn source_at(&self, locus: Locus) -> (Source<'_>, Option<SourceSpan>) {
        let mapped = self.spans.as_ref().and_then(|spans| spans.at(locus));
        named(self.source, mapped)
    }
}

/// Phase 1 over `entry` under `limits`: the materialized document, or the
/// causes of its absence, which are the blocking Findings of Phase 1 the
/// Shared projection is unavailable for (D15:2369). A candidate's Logical
/// input path is projected only within the path bounds of `paths`, the
/// capability's `bootstrapOnly.evidenceMetadata`, under which path admission
/// would leave it absent (D15:776, 1834, 1851; plan 025 decision #299).
///
/// # Errors
///
/// Returns the resolver bug Phase 1 found, a marker the boundary refuses
/// among them.
pub(crate) fn admit_snapshot_entry(
    evaluation: &mut Evaluation,
    entry: &EntrySnapshot<'_>,
    limits: SnapshotEntryLimits,
    paths: &EvidenceMetadataLimits,
) -> Result<Admitted<MaterializedDocument>, EntryInvariant> {
    // The owner records the entry dependencies, and nothing waits on it.
    let _: Admitted<()> = evaluation.check(OWNER, Ok(()), |()| Ok::<(), Blocking>(()))?;
    let Findings {
        mut overrun,
        compatible,
        bounds,
    } = Findings::of(&Observer { entry, paths }, limits)?;
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
    /// The root-value bounds', in check-rank order, found only without a
    /// marker: a marker holds back every root-value bound, as a file's facts
    /// do, so none of their candidates would be evaluated (plan 025 decision
    /// #299).
    bounds: [Option<Found>; 5],
}

impl Findings {
    /// What the checks find in the Snapshot `observer` observes, under
    /// `limits`.
    fn of(
        observer: &Observer<'_, '_>,
        limits: SnapshotEntryLimits,
    ) -> Result<Self, EntryInvariant> {
        let root = observer.entry.snapshot.root();
        let scan = Scan::of(root, limits.value.max_single_string_bytes)?;
        let at = |position: Option<Position>, located| {
            observer.evidence(&scan.positions.edges(position), located)
        };
        let mut found = Self::default();
        if let Some((bound, witness, position)) = scan.terminal {
            let overrun = overrun(bound, witness, &limits.snapshot, || {
                at(position, Located::Value)
            })?;
            found.overrun = Some((bound, overrun));
        }
        let mut candidates = Vec::new();
        for &(reason, position) in &scan.ordinary {
            let observation = Observation {
                primary: at(position, Located::Value)?,
                related: Vec::new(),
            };
            candidates.push(Found::observed(incompatible(reason), observation));
        }
        found.compatible = candidates.into_iter().reduce(Found::and);
        if !scan.marked() {
            let violations = limits.value.violations(SnapshotCounts::of(root).value());
            // A total is observed at the root's value (D15:768).
            found.bounds = root_value_bounds(&violations, &scan.long, |string| match string {
                None => at(None, Located::Value),
                Some(&(position, located)) => at(position, located),
            })?;
        }
        Ok(found)
    }
}

/// The terminal marker's Finding under `bound`, which it names: the bound,
/// its limit, and `witness` as an at-least total, observed at the marker's
/// position, whose Evidence `at` gives, beside the capability's bound
/// (D15:2006, 2356).
fn overrun(
    bound: SnapshotBound,
    witness: u64,
    limits: &SnapshotLimits,
    at: impl FnOnce() -> Result<EvidenceProjection, EntryInvariant>,
) -> Result<Found, EntryInvariant> {
    let limit = bound.limit(limits);
    // `Bound` excludes `u64::MAX`, so the witness the boundary admits is
    // representable.
    if witness != limit.get() + 1 {
        return Err(EntryInvariant::UnadmittedMarker);
    }
    let observation = Observation {
        primary: at()?,
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

/// A position below the root of a Snapshot, in its [`Positions`]. The root,
/// which no edge reaches, is the absence of one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Position(usize);

/// The positions a Snapshot's candidates stand at, and those that lead to
/// them, each entered once after the position that holds it, so candidates
/// share their common prefixes as a file's facts' paths do (plan 025 decision
/// #299). An edge holds an actual key, which only the classification and the
/// sidecar lookup read (D15:895).
#[derive(Debug, Default)]
struct Positions<'s>(Vec<(Option<Position>, Edge<'s>)>);

impl<'s> Positions<'s> {
    /// The edges from the root to `position`, none for the root.
    fn edges(&self, position: Option<Position>) -> Vec<Edge<'s>> {
        let mut edges = Vec::new();
        let mut next = position;
        while let Some(Position(index)) = next {
            let (parent, edge) = self.0[index];
            edges.push(edge);
            next = parent;
        }
        edges.reverse();
        edges
    }

    /// Enters `edge` after `parent`.
    fn push(&mut self, parent: Option<Position>, edge: Edge<'s>) -> Position {
        self.0.push((parent, edge));
        Position(self.0.len() - 1)
    }
}

/// What a canonical traversal of a Snapshot finds, each at its position:
/// array elements in index order, object members in key-byte order, each
/// node before what it holds and a key before its value (D15:867).
struct Scan<'s> {
    /// The single-string bound the root-value check applies.
    limit: Bound,
    /// The edges from the root to the node being visited, each with its
    /// position once a candidate needed it.
    path: Vec<(Edge<'s>, Option<Position>)>,
    /// The positions of the candidates and of what leads to them.
    positions: Positions<'s>,
    /// Each ordinary marker, with its reason.
    ordinary: Vec<(MarkerReason, Option<Position>)>,
    /// The terminal marker, with the bound it names and its witness.
    terminal: Option<(SnapshotBound, u64, Option<Position>)>,
    /// Each string and key longer than `limit`, a key at its member, until a
    /// marker is found: a marker holds back every root-value bound, so none
    /// of them would be observed (plan 025 decision #299).
    long: Vec<(Option<Position>, Located)>,
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
            path: Vec::new(),
            positions: Positions::default(),
            ordinary: Vec::new(),
            terminal: None,
            long: Vec::new(),
        };
        let mut pending: Vec<Step<'s>> = Vec::new();
        scan.visit(root, &mut pending)?;
        while let Some(step) = pending.pop() {
            match step {
                Step::Enter(edge, node) => {
                    scan.path.push((edge, None));
                    if let Edge::Member(key) = edge {
                        scan.string(key, Located::Key);
                    }
                    pending.push(Step::Leave);
                    scan.visit(node, &mut pending)?;
                }
                Step::Leave => {
                    scan.path.pop();
                }
            }
        }
        Ok(scan)
    }

    /// Visits `node` at the current path, and puts the edges to what it
    /// holds on `pending`, the first on top.
    fn visit(&mut self, node: &'s Node, pending: &mut Vec<Step<'s>>) -> Result<(), EntryInvariant> {
        // The boundary admits no node after a bootstrap marker (D15:867).
        if self.terminal.is_some() {
            return Err(EntryInvariant::UnadmittedMarker);
        }
        match node {
            Node::Null | Node::Boolean(_) | Node::Number(_) => {}
            Node::String(text) => self.string(text, Located::Value),
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
            Node::Marker(Marker::Rejected(reason)) => {
                let position = self.marker();
                self.ordinary.push((*reason, position));
            }
            Node::Marker(Marker::BootstrapLimitExceeded { bound, witness }) => {
                self.terminal = Some((*bound, *witness, self.marker()));
            }
        }
        Ok(())
    }

    /// Notes the string or key `text` at the current path, which Evidence of
    /// it locates as `located`, when it is past the bound and no marker was
    /// found.
    fn string(&mut self, text: &str, located: Located) {
        if !self.marked() && count(text.len()) > self.limit.get() {
            let position = self.place();
            self.long.push((position, located));
        }
    }

    /// The position of a marker at the current path. A marker holds back
    /// every root-value bound, so the strings past the bound found before it
    /// are dropped.
    fn marker(&mut self) -> Option<Position> {
        self.long = Vec::new();
        self.place()
    }

    /// Whether a marker was found.
    fn marked(&self) -> bool {
        self.terminal.is_some() || !self.ordinary.is_empty()
    }

    /// The position the current path leads to, entered into `positions` with
    /// each of its prefixes not entered yet.
    fn place(&mut self) -> Option<Position> {
        let mut parent = None;
        for (edge, position) in &mut self.path {
            parent = Some(*position.get_or_insert_with(|| self.positions.push(parent, *edge)));
        }
        parent
    }
}

#[cfg(any(test, feature = "benchmark"))]
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
        classify: Box<Classify<'static>>,
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

        /// The entry of `snapshot`, with no locator and no call-site sidecar.
        pub(crate) fn entry<'a>(&'a self, snapshot: &'a AdmittedSnapshot) -> EntrySnapshot<'a> {
            EntrySnapshot {
                snapshot,
                source: Source {
                    identity: &self.identity,
                    marked: true,
                    locator: None,
                },
                call_site: None,
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

    use crate::artifacts::count;
    use crate::boundary::call_site::{AdmittedCallSite, SubmittedMapping};
    use crate::boundary::locator::{
        read_programmatic_locator, Locator, LocatorCandidate, LocatorVariant,
    };
    use crate::boundary::snapshot::{read_report, submitted, AdmittedSnapshot, SnapshotReport};
    use crate::construction::registry::check::testing::{check_id, requirements};
    use crate::construction::registry::check::{CheckRegistry, RecordKind};
    use crate::fixtures::{
        complete_config, minimal_config, FixturePolicyReference, FixtureTargetReference,
    };
    use crate::input_limits::{Bound, InputLimits, ValueLimits};
    use crate::materialize::{materialize_file, MaterializedDocument};
    use crate::materialize_tests::limits as file_limits;
    use crate::model::IntlifyConfig;
    use crate::resolver::call_site_span::fixtures::{
        mapping, mirrored, sidecar, sidecar_at, CALL_SITE,
    };
    use crate::resolver::call_site_span::Located;
    use crate::resolver::capability::{CapabilityBound, EvidenceMetadataLimits, SnapshotLimits};
    use crate::resolver::entry::fixtures::FileFixture;
    use crate::resolver::entry::{
        admit_entry, capability, input, ConfigurationEntry, EntryInvariant, OWNER,
        ROOT_VALUE_BOUNDS,
    };
    use crate::resolver::evaluation::{
        Actual, Admitted, BootstrapLimitExceeded, Causes, Check, Code, Evaluation, IssueKey,
        Reason, Status, Unit,
    };
    use crate::resolver::evidence::Evidence;
    use crate::resolver::evidence_metadata::fixtures::{unbounded, with};
    use crate::resolver::evidence_metadata::MetadataBound;
    use crate::resolver::evidence_path::PathSize;
    use crate::resolver::evidence_source::hex;
    use crate::resolver::evidence_token::EvidenceTokens;
    use crate::resolver::limits::{fixture_limits, ResolutionLimits};
    use crate::resolver::occurrence::{
        occurrences, redacted, FindingOccurrence, OccurrenceCandidate,
    };
    use crate::resolver::origin::SafeOriginKind;
    use crate::resolver::profile_selection::select_profile;
    use crate::resolver::record_view::RecordViews;
    use crate::resolver::structural_admission::{admit_structure, whole};
    use crate::resolver::subject::{EntryPath, SubjectKind};
    use crate::resolver::token::{Sha256, Specification};
    use crate::snapshot::{from_json, Marker, MarkerReason, Node, SnapshotBound};
    use crate::structural::selection::SelectorInput;
    use crate::structural::{AuthoringSchema, Edge, StructuralAnalysis, StructuralLimits};

    use super::fixtures::{SnapshotFixture, ENTRY};
    use super::{
        admit_snapshot_entry, incompatible, ProgrammaticEntry, Scan, SnapshotEntryLimits,
        SNAPSHOT_BOUNDS,
    };

    type Schema = AuthoringSchema<FixturePolicyReference, FixtureTargetReference>;
    type Analysis = StructuralAnalysis<FixturePolicyReference, FixtureTargetReference>;
    type Typed = IntlifyConfig<FixturePolicyReference, FixtureTargetReference>;

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
        read_report(Some(&report), limits).unwrap()
    }

    /// Phase 1 over `snapshot` under `limits`: the evaluation, and the
    /// document or the causes of its absence.
    fn run(
        snapshot: &AdmittedSnapshot,
        limits: SnapshotEntryLimits,
    ) -> Result<(Evaluation, Admitted<MaterializedDocument>), EntryInvariant> {
        let fixture = SnapshotFixture::new(&fixture_limits(100));
        let mut evaluation = Evaluation::default();
        let admitted = admit_snapshot_entry(
            &mut evaluation,
            &fixture.entry(snapshot),
            limits,
            &unbounded(),
        )?;
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
        let admitted = admit_entry(&mut evaluation, &file, limits, &unbounded()).unwrap();
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

    /// Under the largest bound the terminal marker's Finding reports the
    /// witness `u64::MAX`, the bound's limit plus one (D15:889, 2356, 3323).
    #[test]
    fn the_terminal_marker_reports_u64_max_under_the_largest_bound() {
        let mut limits = limits();
        let largest = Bound::new(u64::MAX - 1).unwrap();
        limits.snapshot.max_nodes = largest;
        let root = Node::Array(vec![Node::Marker(Marker::BootstrapLimitExceeded {
            bound: SnapshotBound::Nodes,
            witness: u64::MAX,
        })]);
        let (evaluation, document) = entered(&root, limits);
        assert!(document.is_err());
        let key = evaluation.issues().next().unwrap().clone();
        assert_eq!(key.code(), Code::BootstrapLimitExceeded);
        assert_eq!(
            evaluation.bootstrap_limit(&key),
            Some(BootstrapLimitExceeded {
                bound: SnapshotBound::Nodes.id(),
                limit: largest,
                actual: Actual::AtLeast(u64::MAX),
            })
        );
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
            let admitted = admit_snapshot_entry(
                &mut evaluation,
                &fixture.entry(&snapshot),
                limits,
                &unbounded(),
            )
            .unwrap();
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
            let admitted = admit_snapshot_entry(
                &mut evaluation,
                &fixture.entry(&snapshot),
                limits,
                &unbounded(),
            )
            .unwrap();
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

    /// What `scan` found past the single-string bound, each at the edges
    /// from the root to its position.
    fn long_at<'s>(scan: &Scan<'s>) -> Vec<(Vec<Edge<'s>>, Located)> {
        scan.long
            .iter()
            .map(|&(position, located)| (scan.positions.edges(position), located))
            .collect()
    }

    /// A tree far deeper than a stack holds is scanned without recursion,
    /// each candidate at its full path: a long string, and an ordinary marker
    /// after one, below arrays, and a long key holding the terminal marker
    /// below objects (plan 025 decision #298). A marker drops the long
    /// strings found before it (plan 025 decision #299).
    #[test]
    fn a_deep_snapshot_is_scanned_without_recursion() {
        const LEVELS: usize = 100_000;
        let long = "x".repeat(5);
        let terminal = Node::Marker(Marker::BootstrapLimitExceeded {
            bound: SnapshotBound::Nodes,
            witness: 2,
        });
        let mut strings = Node::Array(vec![Node::String(long.clone())]);
        let mut array = Node::Array(vec![
            Node::String(long.clone()),
            Node::Marker(Marker::Rejected(MarkerReason::Cycle)),
        ]);
        let mut object = Node::Object(BTreeMap::from([(long.clone(), terminal)]));
        for _ in 0..LEVELS {
            strings = Node::Array(vec![strings]);
            array = Node::Array(vec![array]);
            object = Node::Object(BTreeMap::from([("a".to_owned(), object)]));
        }
        let limit = Bound::new(4).unwrap();
        let below = |last| {
            let mut path = vec![Edge::Index(0); LEVELS];
            path.push(Edge::Index(last));
            path
        };
        let scan = Scan::of(&strings, limit).unwrap();
        assert_eq!(long_at(&scan), [(below(0), Located::Value)]);
        assert!(scan.ordinary.is_empty());
        let scan = Scan::of(&array, limit).unwrap();
        assert!(scan.long.is_empty());
        let [(MarkerReason::Cycle, position)] = scan.ordinary[..] else {
            panic!("one cycle marker");
        };
        assert_eq!(scan.positions.edges(position), below(1));
        assert_eq!(scan.terminal, None);
        let scan = Scan::of(&object, limit).unwrap();
        let mut path = vec![Edge::Member("a"); LEVELS];
        path.push(Edge::Member(&long));
        assert!(scan.long.is_empty());
        assert!(scan.ordinary.is_empty());
        let Some((SnapshotBound::Nodes, 2, position)) = scan.terminal else {
            panic!("the terminal marker");
        };
        assert_eq!(scan.positions.edges(position), path);
    }

    /// Many candidates under a deep shared prefix hold each edge once: the
    /// positions grow with the nodes on the candidates' paths, never with
    /// the candidates times their depth, and each candidate still stands at
    /// its full path (plan 025 decision #299).
    #[test]
    fn candidates_under_a_deep_shared_prefix_hold_each_edge_once() {
        const DEPTH: usize = 1_000;
        const CANDIDATES: usize = 1_000;
        let marker = Node::Marker(Marker::Rejected(MarkerReason::Cycle));
        let mut marked = Node::Array(vec![marker; CANDIDATES]);
        let mut long = Node::Array(vec![Node::String("x".repeat(5)); CANDIDATES]);
        for _ in 0..DEPTH {
            marked = Node::Array(vec![marked]);
            long = Node::Object(BTreeMap::from([("a".to_owned(), long)]));
        }
        let limit = Bound::new(4).unwrap();
        let scan = Scan::of(&marked, limit).unwrap();
        assert_eq!(scan.ordinary.len(), CANDIDATES);
        assert_eq!(scan.positions.0.len(), DEPTH + CANDIDATES);
        for (index, &(_, position)) in scan.ordinary.iter().enumerate() {
            let mut path = vec![Edge::Index(0); DEPTH];
            path.push(Edge::Index(count(index)));
            assert_eq!(scan.positions.edges(position), path);
        }
        let scan = Scan::of(&long, limit).unwrap();
        assert_eq!(scan.positions.0.len(), DEPTH + CANDIDATES);
        let at = long_at(&scan);
        assert_eq!(at.len(), CANDIDATES);
        for (index, (path, located)) in at.into_iter().enumerate() {
            assert_eq!(located, Located::Value);
            assert_eq!(path.len(), DEPTH + 1);
            assert_eq!(
                path[DEPTH - 1..],
                [Edge::Member("a"), Edge::Index(count(index))]
            );
        }
    }

    /// A marker holds back every root-value bound, so once the scan finds
    /// one it keeps no string past the bound, those found before it dropped,
    /// and Phase 1 builds no root-value candidate: only the markers' Findings
    /// are reported, an ordinary marker's or the terminal one's (plan 025
    /// decision #299).
    #[test]
    fn a_marker_leaves_no_root_value_candidate() {
        let long = "x".repeat(5);
        let limit = Bound::new(4).unwrap();
        let (unmarked, marked) = (
            tree(&json!([long, long])),
            tree(&json!([long, "!cycle", long])),
        );
        assert_eq!(Scan::of(&unmarked, limit).unwrap().long.len(), 2);
        let scan = Scan::of(&marked, limit).unwrap();
        assert!(scan.long.is_empty());
        assert_eq!(scan.ordinary.len(), 1);
        let mut narrow = limits();
        narrow.value.max_single_string_bytes = limit;
        for (value, check) in [
            (json!([long, "!cycle", long]), Check::JsonCompatible),
            (
                json!({ "a": long, "b": "!maxNodes" }),
                Check::WithinSnapshotNodes,
            ),
        ] {
            let (evaluation, _) = entered(&tree(&value), narrow);
            let checks: Vec<Check> = found(&evaluation)
                .into_iter()
                .map(|(check, _, _)| check)
                .collect();
            assert_eq!(checks, [check]);
            let statuses = statuses(&evaluation);
            for (check, _, _) in ROOT_VALUE_BOUNDS {
                assert!(statuses[&check].is_some(), "{check:?}");
            }
        }
    }

    /// Phase 1 over `root` as the boundary admits it under [`limits`] beside
    /// `call_site`, if any, each Logical input path projected only within the
    /// path bounds of `paths`: the evaluation, and the document or the causes
    /// of its absence.
    fn entered_under(
        root: &Node,
        call_site: Option<&AdmittedCallSite>,
        paths: &EvidenceMetadataLimits,
    ) -> (Evaluation, Admitted<MaterializedDocument>) {
        let limits = limits();
        let snapshot = admitted(root, &limits.snapshot);
        let fixture = SnapshotFixture::new(&fixture_limits(100));
        let mut entry = fixture.entry(&snapshot);
        entry.call_site = call_site;
        let mut evaluation = Evaluation::default();
        let admitted = admit_snapshot_entry(&mut evaluation, &entry, limits, paths).unwrap();
        (evaluation, admitted)
    }

    /// The size of the Logical input path of each candidate's primary
    /// Evidence, projected or only measured.
    fn sizes(evaluation: &Evaluation) -> Vec<Option<PathSize>> {
        evaluation
            .occurrence_candidates()
            .iter()
            .map(|candidate| candidate.primary.entry_path())
            .collect()
    }

    /// Each candidate as path admission leaves it under `paths`.
    fn admitted_under(
        evaluation: &Evaluation,
        paths: &EvidenceMetadataLimits,
    ) -> Vec<OccurrenceCandidate> {
        evaluation
            .occurrence_candidates()
            .iter()
            .map(|candidate| candidate.admitted(paths).unwrap())
            .collect()
    }

    /// D15:776, 1851: Phase 1 projects a marker's Logical input path only
    /// within the path bounds. Past them only its size is kept, measured as
    /// the projected path would be, and path admission leaves each candidate
    /// as it would have left the projected one, while a path within them is
    /// kept (plan 025 decision #299).
    #[test]
    fn a_logical_input_path_past_a_path_bound_is_never_projected() {
        let value = json!({
            "$schema": "!cycle",
            "profiles": { "app": { "requestedLocales": ["en", "!cycle"] } },
        });
        let root = tree(&value);
        let bound = with(MetadataBound::LogicalPathSegments, 3);
        let (bounded, _) = entered_under(&root, None, &bound);
        let (whole, _) = entered_under(&root, None, &unbounded());
        assert_ne!(
            bounded.occurrence_candidates(),
            whole.occurrence_candidates()
        );
        assert_eq!(sizes(&bounded), sizes(&whole));
        assert_eq!(
            admitted_under(&bounded, &bound),
            admitted_under(&whole, &bound)
        );
        let mut seen: Vec<Value> = observed(&bounded)
            .iter()
            .map(|occurrence| occurrence.primary().view()["path"].clone())
            .collect();
        seen.sort_by_key(Value::to_string);
        // An array's text sorts before `null`'s.
        assert_eq!(seen, [json!([{ "safe-member": "$schema" }]), Value::Null]);
    }

    /// D15:747, 802: a marker whose Logical input path Phase 1 leaves
    /// unprojected still takes the span the sidecar maps at its position,
    /// which Phase 1 keeps (plan 025 decision #299).
    #[test]
    fn a_span_is_mapped_at_a_position_whose_path_is_not_projected() {
        let text =
            r#"{"$schema":"!cycle","profiles":{"app":{"requestedLocales":["en","!cycle"]}}}"#;
        let value: Value = serde_json::from_str(text).unwrap();
        let deep = spot(text, r#""!cycle""#, 1);
        let call_site = sidecar(
            text,
            None,
            vec![
                spanning(
                    &json!(["$schema"]),
                    "member-value",
                    spot(text, r#""!cycle""#, 0),
                ),
                spanning(
                    &json!(["profiles", "app", "requestedLocales", 1]),
                    "node",
                    deep,
                ),
            ],
        );
        let bound = with(MetadataBound::LogicalPathSegments, 3);
        let (evaluation, _) = entered_under(&tree(&value), Some(&call_site), &bound);
        let deepest: Vec<Value> = observed(&evaluation)
            .iter()
            .map(|occurrence| occurrence.primary().view())
            .filter(|view| view.get("path").is_none())
            .collect();
        assert_eq!(deepest.len(), 1);
        assert_eq!(deepest[0]["source"], call_site_source());
        assert_eq!(deepest[0]["span"], span_view(deep));
    }

    /// D15:1889, 3228: under a path bound either entry leaves the same
    /// Logical input paths: the root's and the long key's within it are
    /// projected, the long string's past it only measured, and each measures
    /// as the other entry's does (plan 025 decision #299).
    #[test]
    fn either_entry_projects_the_same_logical_input_paths_under_a_path_bound() {
        let value: Value = serde_json::from_str(BOUNDED).unwrap();
        let mut input = file_limits();
        input.value = tight(1);
        let paths = with(MetadataBound::LogicalPathSegments, 1);
        let fixture = FileFixture::new(&fixture_limits(100));
        let file = fixture.file(Arc::from(serde_json::to_vec(&value).unwrap()));
        let mut filed = Evaluation::default();
        let admitted_file = admit_entry(&mut filed, &file, input, &paths).unwrap();
        let snapshot_limits = SnapshotEntryLimits {
            value: input.value,
            ..limits()
        };
        let snapshot = admitted(&from_json(&value), &snapshot_limits.snapshot);
        let fixture = SnapshotFixture::new(&fixture_limits(100));
        let mut entered = Evaluation::default();
        let admitted_snapshot = admit_snapshot_entry(
            &mut entered,
            &fixture.entry(&snapshot),
            snapshot_limits,
            &paths,
        )
        .unwrap();
        assert!(admitted_file.is_err() && admitted_snapshot.is_err());
        assert_eq!(sizes(&entered), sizes(&filed));
        let (filed, entered) = (observed(&filed), observed(&entered));
        assert_eq!(entered.len(), 6);
        let path = |occurrence: &FindingOccurrence| occurrence.primary().view()["path"].clone();
        let projected = entered
            .iter()
            .filter(|occurrence| !path(occurrence).is_null());
        assert_eq!(projected.count(), 5);
        for (file, snapshot) in filed.iter().zip(&entered) {
            assert_eq!(snapshot.finding(), file.finding());
            assert_eq!(path(snapshot), path(file));
        }
    }

    /// D15:792-798: without a call-site sidecar, a Snapshot's Entry Source
    /// Evidence names the Programmatic Entry's identity and the locator
    /// admitted with it, and has no span.
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
        let admitted = admit_snapshot_entry(&mut evaluation, &entry, limits, &unbounded()).unwrap();
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

    /// D15:776, 792-798, 3416: programmatic Entry Source Evidence holds the
    /// present empty path for a marker at the root, the complete path to a
    /// marker inside the value, the longest safely established prefix when a
    /// further edge would need dynamic behavior, an unsafe member of that
    /// prefix redacted, and no path past a path bound.
    #[test]
    fn programmatic_entry_source_evidence_takes_each_form_of_logical_input_path() {
        let path = |value: &Value, bound: &EvidenceMetadataLimits| {
            let (evaluation, _) = entered_under(&tree(value), None, bound);
            let found = observed(&evaluation);
            let [occurrence] = &found[..] else {
                panic!("one marker: {value}");
            };
            let view = occurrence.primary().view();
            assert_eq!(view["kind"], "entry-source");
            assert_eq!(view["origin"], "configuration-programmatic");
            view.get("path").cloned()
        };
        let every = unbounded();
        assert_eq!(path(&json!("!cycle"), &every), Some(json!([])));
        assert_eq!(
            path(
                &json!({ "profiles": { "app": { "requestedLocales": ["en", "!cycle"] } } }),
                &every
            ),
            Some(json!([
                { "safe-member": "profiles" },
                { "safe-member": "app" },
                { "safe-member": "requestedLocales" },
                { "array-index": "1" },
            ]))
        );
        assert_eq!(
            path(
                &json!({ "profiles": { "app": "!dynamic-container" } }),
                &every
            ),
            Some(json!([
                { "safe-member": "profiles" },
                { "safe-member": "app" },
            ]))
        );
        let unsafe_prefix = path(
            &json!({ "profiles": { "Bad Key": "!dynamic-container" } }),
            &every,
        )
        .unwrap();
        assert_eq!(unsafe_prefix[0], json!({ "safe-member": "profiles" }));
        assert!(unsafe_prefix[1].get("redacted-member").is_some());
        assert_eq!(unsafe_prefix.as_array().unwrap().len(), 2);
        let bound = with(MetadataBound::LogicalPathSegments, 1);
        assert_eq!(
            path(&json!({ "profiles": { "app": "!cycle" } }), &bound),
            None
        );
    }

    /// D15:796, 800, 3416: Materialized Value Evidence names one source with
    /// that source's own admitted locator, `safe-label`, `safe-uri`, or none,
    /// and its required Portable Value Locator, exact or redacted, with the
    /// span where the sidecar maps one and without it where it does not.
    #[test]
    fn materialized_value_evidence_keeps_the_locator_of_the_source_it_names() {
        fn candidate(variant: Option<LocatorVariant>) -> Option<LocatorCandidate<'static>> {
            variant.map(|variant| LocatorCandidate {
                variant,
                payload: match variant {
                    LocatorVariant::SafeUri => "https://example.com/storefront",
                    _ => "storefront label",
                },
                non_secret: true,
            })
        }
        let view_of = |candidate: Option<LocatorCandidate<'static>>| {
            candidate.map_or(Value::Null, |at| json!({ at.variant.id(): at.payload }))
        };
        let mut value = minimal_config();
        value["extra"] = json!(true);
        value["profiles"]["app"]["requestedLocales"] = json!("only");
        let text = serde_json::to_string(&value).unwrap();
        let key = spot(&text, r#""extra""#, 0);
        let locales = spot(&text, r#""only""#, 0);
        let exact = json!({ "exact-json-pointer": "/profiles/app/requestedLocales" });
        let mappings = [
            (
                "exact",
                spanning(
                    &json!(["profiles", "app", "requestedLocales"]),
                    "member-value",
                    locales,
                ),
            ),
            ("redacted", spanning(&json!(["extra"]), "member-key", key)),
        ];
        let variants = [
            None,
            Some(LocatorVariant::SafeLabel),
            Some(LocatorVariant::SafeUri),
        ];
        let mut seen = BTreeSet::new();
        for entry_variant in variants {
            let entry_locator = candidate(entry_variant)
                .map(|at| read_programmatic_locator(Some(at), Bound::new(64).unwrap()).unwrap());
            for site_variant in variants {
                for (mapped, mapping) in &mappings {
                    let call_site =
                        sidecar_at(&text, candidate(site_variant), vec![mapping.clone()]);
                    let found =
                        later_in_snapshot(&value, Some(&call_site), entry_locator.as_ref(), "app");
                    assert_eq!(found.len(), 2, "{text}");
                    for occurrence in &found {
                        let view = occurrence.primary().view();
                        assert_eq!(view["kind"], "configuration-value");
                        let variant = if view["value"] == exact {
                            "exact"
                        } else {
                            let first = &view["value"]["redacted-logical-path"][0];
                            assert!(first.get("redacted-member").is_some(), "{view}");
                            "redacted"
                        };
                        let (source, locator, span) = if variant == *mapped {
                            let at = if variant == "exact" { locales } else { key };
                            (
                                call_site_source(),
                                view_of(candidate(site_variant)),
                                span_view(at),
                            )
                        } else {
                            (
                                entry_source(),
                                view_of(candidate(entry_variant)),
                                Value::Null,
                            )
                        };
                        assert_eq!(view["source"], source, "{view}");
                        assert_eq!(view["locator"], locator, "{view}");
                        assert_eq!(view["span"], span, "{view}");
                        seen.insert((variant, variant == *mapped));
                    }
                }
            }
        }
        assert_eq!(
            seen,
            BTreeSet::from([
                ("exact", false),
                ("exact", true),
                ("redacted", false),
                ("redacted", true),
            ])
        );
    }

    /// The byte offsets of the `nth` occurrence of `part` in `text`.
    fn spot(text: &str, part: &str, nth: usize) -> (usize, usize) {
        let (start, _) = text.match_indices(part).nth(nth).unwrap();
        (start, start + part.len())
    }

    /// The mapping from `path` and `role` to the bytes `[start, end)`.
    fn spanning(path: &Value, role: &str, (start, end): (usize, usize)) -> SubmittedMapping {
        let coordinate = |byte: usize| f64::from(u32::try_from(byte).unwrap());
        mapping(path, role, coordinate(start), coordinate(end))
    }

    /// The view of the span of the bytes `[start, end)`.
    fn span_view((start, end): (usize, usize)) -> Value {
        json!({ "start": start.to_string(), "end": end.to_string() })
    }

    /// The view of the call-site source identity, which its frontend marks
    /// non-secret.
    fn call_site_source() -> Value {
        json!({ "safe-opaque": hex(CALL_SITE.as_bytes()) })
    }

    /// The view of the Programmatic Entry identity, which its frontend marks
    /// non-secret.
    fn entry_source() -> Value {
        json!({ "safe-opaque": hex(ENTRY.as_bytes()) })
    }

    /// Phase 1 over `root` as the boundary admits it under `limits`, beside
    /// `call_site`.
    fn entered_beside(
        root: &Node,
        limits: SnapshotEntryLimits,
        call_site: &AdmittedCallSite,
    ) -> (Evaluation, Admitted<MaterializedDocument>) {
        let snapshot = admitted(root, &limits.snapshot);
        let fixture = SnapshotFixture::new(&fixture_limits(100));
        let mut entry = fixture.entry(&snapshot);
        entry.call_site = Some(call_site);
        let mut evaluation = Evaluation::default();
        let admitted = admit_snapshot_entry(&mut evaluation, &entry, limits, &unbounded()).unwrap();
        (evaluation, admitted)
    }

    /// D15:747, 802 (Decision 015-203): a marker is observed at the span the
    /// sidecar maps for the value at its position, `member-value` at a member
    /// and `node` at an element, in the call-site source. A marker whose
    /// position has only another role mapped, or none, is observed in the
    /// Programmatic Entry without a span.
    #[test]
    fn a_marker_takes_the_span_mapped_for_the_value_at_its_position() {
        let value: Value = serde_json::from_str(MARKED).unwrap();
        let unsupported = r#""!unsupported-type""#;
        let terminal = r#""!maxRejectedMarkers""#;
        let at = |part, nth| spot(MARKED, part, nth);
        let locales = json!(["profiles", "app", "requestedLocales", 1]);
        let call_site = sidecar(
            MARKED,
            None,
            vec![
                spanning(&json!(["$schema"]), "member-value", at(unsupported, 0)),
                spanning(&json!(["$schema"]), "member-key", at(r#""$schema""#, 0)),
                spanning(&json!(["$schema"]), "node", at(r#""$schema""#, 0)),
                spanning(&locales, "node", at(unsupported, 1)),
                spanning(&locales, "member-value", at(r#""en""#, 0)),
                spanning(
                    &json!(["profiles", "app", "projectId"]),
                    "member-key",
                    at(r#""projectId""#, 0),
                ),
                spanning(&json!(["schemaVersion"]), "member-value", at(terminal, 0)),
            ],
        );
        let (evaluation, _) = entered_beside(&tree(&value), limits(), &call_site);
        let seen: Vec<(Option<Reason>, Value, Value)> = observed(&evaluation)
            .iter()
            .map(|occurrence| {
                let view = occurrence.primary().view();
                let reason = occurrence.finding().reason();
                (reason, view["source"].clone(), view["span"].clone())
            })
            .collect();
        let (mapped, entry) = (call_site_source(), entry_source());
        assert_eq!(
            seen,
            [
                (None, mapped.clone(), span_view(at(terminal, 0))),
                (Some(Reason::Cycle), entry.clone(), Value::Null),
                (Some(Reason::DynamicProperty), entry, Value::Null),
                (
                    Some(Reason::UnsupportedType),
                    mapped.clone(),
                    span_view(at(unsupported, 0))
                ),
                (
                    Some(Reason::UnsupportedType),
                    mapped,
                    span_view(at(unsupported, 1))
                ),
            ]
        );
    }

    /// D15:747, 768, 3228 (Decision 015-203): beside a sidecar that maps every
    /// value and key of the file of the same text at its token, the root-value
    /// bounds are observed at the file's spans in the call-site source, a key
    /// past the single-string bound at its key's rather than its value's; a
    /// total, observed at the root's value, takes the root's span, where a
    /// file names none.
    #[test]
    fn a_key_a_string_and_a_total_take_the_spans_mapped_for_their_roles() {
        let value: Value = serde_json::from_str(BOUNDED).unwrap();
        assert_eq!(serde_json::to_string(&value).unwrap(), BOUNDED);
        let mut input = file_limits();
        input.value = tight(1);
        let (file, _) = filed(&value, input);
        let call_site = sidecar(BOUNDED, None, mirrored(BOUNDED));
        let snapshot_limits = SnapshotEntryLimits {
            value: input.value,
            ..limits()
        };
        let (snapshot, admitted) = entered_beside(&from_json(&value), snapshot_limits, &call_site);
        assert!(admitted.is_err());
        let (file, snapshot) = (observed(&file), observed(&snapshot));
        assert_eq!(snapshot.len(), 6);
        assert_eq!(snapshot.len(), file.len());
        let root = span_view((0, BOUNDED.len()));
        for (file, snapshot) in file.iter().zip(&snapshot) {
            assert_eq!(snapshot.finding(), file.finding());
            let mut expected = located(file.primary());
            let view = expected.as_object_mut().unwrap();
            view.insert("origin".to_owned(), json!("configuration-programmatic"));
            if file.finding().unit().check != Check::WithinSingleStringBytes {
                assert_eq!(view.insert("span".to_owned(), root.clone()), None);
            }
            assert_eq!(located(snapshot.primary()), expected);
            assert_eq!(snapshot.primary().view()["source"], call_site_source());
        }
    }

    /// The bounds Phases 2 and 3 run under, which the classification of the
    /// members on a path reads too.
    fn structural() -> StructuralLimits {
        StructuralLimits {
            max_profiles: Bound::new(100).unwrap(),
            max_profile_id_bytes: Bound::new(64).unwrap(),
            max_structural_analysis_units: Bound::new(1_000_000).unwrap(),
        }
    }

    fn comparable() -> ResolutionLimits {
        let mut comparable = fixture_limits(100);
        comparable.configuration_input.structural = structural();
        comparable
    }

    /// Phases 2 and 3 over `document` through `entry`, the selector naming
    /// `selector`: the Finding Occurrences they found.
    fn later(
        entry: &dyn ConfigurationEntry,
        document: MaterializedDocument,
        selector: &str,
    ) -> Vec<FindingOccurrence> {
        let limits = structural();
        let mut evaluation = Evaluation::default();
        let schema = Schema::for_model().unwrap();
        let analysis = admit_structure(&mut evaluation, entry, document, &schema, limits, Ok(()))
            .unwrap()
            .analysis;
        let input = SelectorInput::string(selector, limits.max_profile_id_bytes);
        // What Phase 3 selects is beside the point: only its Findings are.
        let _ = select_profile(
            &mut evaluation,
            entry,
            analysis.as_ref().ok(),
            &input,
            limits,
        )
        .unwrap();
        observed(&evaluation)
    }

    /// Phases 2 and 3 over the file of `value`.
    fn later_in_file(value: &Value, selector: &str) -> Vec<FindingOccurrence> {
        let bytes: Arc<[u8]> = Arc::from(serde_json::to_vec(value).unwrap());
        let document = materialize_file(Arc::clone(&bytes), file_limits()).unwrap();
        let fixture = FileFixture::new(&comparable());
        later(&fixture.file(bytes), document, selector)
    }

    /// Phases 2 and 3 over the document Phase 1 admits from the Snapshot of
    /// `value`, through its programmatic entry beside `call_site`, the
    /// Programmatic Entry's locator `locator`.
    fn later_in_snapshot(
        value: &Value,
        call_site: Option<&AdmittedCallSite>,
        locator: Option<&Locator>,
        selector: &str,
    ) -> Vec<FindingOccurrence> {
        let limits = limits();
        let snapshot = admitted(&from_json(value), &limits.snapshot);
        let fixture = SnapshotFixture::new(&comparable());
        let mut entry = fixture.entry(&snapshot);
        entry.source.locator = locator;
        entry.call_site = call_site;
        let mut evaluation = Evaluation::default();
        let Ok(document) =
            admit_snapshot_entry(&mut evaluation, &entry, limits, &unbounded()).unwrap()
        else {
            panic!("Phase 1 admits the document");
        };
        let programmatic = ProgrammaticEntry::new(&entry, &document);
        later(&programmatic, document, selector)
    }

    /// What Phase 2 left: the typed root its analysis constructs, `None`
    /// without a complete root, beside whether its checks admitted the whole
    /// root.
    fn typed_root(evaluation: &Evaluation, analysis: Admitted<&Analysis>) -> (Option<Typed>, bool) {
        let typed = analysis
            .ok()
            .and_then(|analysis| analysis.construct().unwrap());
        (typed, whole(evaluation).is_ok())
    }

    /// What Phase 2 leaves of `value` through the file entry.
    fn structural_of_file(value: &Value) -> (Option<Typed>, bool) {
        let bytes: Arc<[u8]> = Arc::from(serde_json::to_vec(value).unwrap());
        let document = materialize_file(Arc::clone(&bytes), file_limits()).unwrap();
        let fixture = FileFixture::new(&comparable());
        let mut evaluation = Evaluation::default();
        let schema = Schema::for_model().unwrap();
        let admission = admit_structure(
            &mut evaluation,
            &fixture.file(bytes),
            document,
            &schema,
            structural(),
            Ok(()),
        )
        .unwrap();
        typed_root(
            &evaluation,
            admission.analysis.as_ref().map_err(Causes::clone),
        )
    }

    /// What Phase 2 leaves of `value` through the programmatic entry.
    fn structural_of_snapshot(value: &Value) -> (Option<Typed>, bool) {
        let limits = limits();
        let snapshot = admitted(&from_json(value), &limits.snapshot);
        let fixture = SnapshotFixture::new(&comparable());
        let entry = fixture.entry(&snapshot);
        let mut evaluation = Evaluation::default();
        let Ok(document) =
            admit_snapshot_entry(&mut evaluation, &entry, limits, &unbounded()).unwrap()
        else {
            panic!("Phase 1 admits the document");
        };
        let programmatic = ProgrammaticEntry::new(&entry, &document);
        let schema = Schema::for_model().unwrap();
        let admission = admit_structure(
            &mut evaluation,
            &programmatic,
            document,
            &schema,
            structural(),
            Ok(()),
        )
        .unwrap();
        typed_root(
            &evaluation,
            admission.analysis.as_ref().map_err(Causes::clone),
        )
    }

    /// `value` with every object's members in reverse order, at every depth.
    fn reversed(value: &Value) -> Value {
        match value {
            Value::Object(members) => Value::Object(
                members
                    .iter()
                    .rev()
                    .map(|(key, member)| (key.clone(), reversed(member)))
                    .collect(),
            ),
            Value::Array(items) => Value::Array(items.iter().map(reversed).collect()),
            other => other.clone(),
        }
    }

    /// The file and the Snapshot of one value, whatever the file's member
    /// order, construct one `IntlifyConfig`, and exactly when the checks that
    /// gate profile projection admitted the whole root; a value with a
    /// structural failure constructs none through either entry (D15:395,
    /// 2130, 2142, 3329, 3339).
    #[test]
    fn both_entries_converge_on_one_intlify_config_only_after_structural_admission() {
        for value in [minimal_config(), complete_config()] {
            let (file, whole_file) = structural_of_file(&value);
            let typed = file.expect("the file's root is complete");
            assert!(whole_file);
            assert_eq!(serde_json::to_value(&typed).unwrap(), value);
            let (snapshot, whole_snapshot) = structural_of_snapshot(&value);
            assert_eq!(snapshot.as_ref(), Some(&typed));
            assert!(whole_snapshot);
            let (permuted, whole_permuted) = structural_of_file(&reversed(&value));
            assert_eq!(permuted.as_ref(), Some(&typed));
            assert!(whole_permuted);
        }
        let mut invalid = complete_config();
        invalid["profiles"]["app"]["defaultRequestedLocale"] = json!(false);
        let mut unknown = minimal_config();
        unknown["extra"] = json!(true);
        for value in [invalid, unknown, json!({ "schemaVersion": 0 }), json!([])] {
            assert_eq!(structural_of_file(&value), (None, false), "{value}");
            assert_eq!(structural_of_snapshot(&value), (None, false), "{value}");
        }
    }

    /// Configurations whose Findings in Phases 2 and 3 are observed at each
    /// kind of location, each with the selector it is resolved under: the
    /// root of another type, a missing member of the root and of a
    /// declaration, a member's value, an array element, an undeclared
    /// member's key at the root and nested in a declaration, a target, and a
    /// group (D15:3414), a malformed Profile ID's key, and a selector no
    /// declaration has, beside the declarations.
    fn locations() -> Vec<(Value, &'static str)> {
        let mut unversioned = minimal_config();
        unversioned.as_object_mut().unwrap().remove("schemaVersion");
        let mut versioned = minimal_config();
        versioned["schemaVersion"] = json!(10);
        let declaration = minimal_config()["profiles"]["app"].clone();
        let mut mixed = minimal_config();
        mixed["extra"] = json!(true);
        let app = &mut mixed["profiles"]["app"];
        app.as_object_mut().unwrap().remove("projectId");
        app["requestedLocales"] = json!(["en", 7]);
        app["extra"] = json!({ "schemaVersion": "0" });
        app["targetProfiles"]["browser"]["extra"] = json!(true);
        app["deploymentGroups"]["web"]["extra"] = json!(true);
        let mut malformed = declaration.clone();
        malformed["requestedLocales"] = json!("only");
        mixed["profiles"]["app key"] = malformed;
        let mut declared = minimal_config();
        declared["profiles"]["web"] = declaration;
        vec![
            (json!([]), "app"),
            (unversioned, "app"),
            (versioned, "app"),
            (mixed, "app"),
            (declared, "other"),
        ]
    }

    /// `view` with each token blanked.
    fn blank(view: &mut Value) {
        match view {
            Value::Object(members) => {
                for (name, member) in members.iter_mut() {
                    if name == "token" {
                        *member = Value::Null;
                    } else {
                        blank(member);
                    }
                }
            }
            Value::Array(items) => items.iter_mut().for_each(blank),
            _ => {}
        }
    }

    /// The view of `evidence` as either entry would name it: the slot of its
    /// origin rather than the Safe Origin Kind, and neither its source nor a
    /// token, since a redacted member's token is its origin's.
    fn entry_free(evidence: &Evidence) -> Value {
        let mut view = evidence.view();
        let origin = match evidence.origin() {
            SafeOriginKind::ConfigurationFile | SafeOriginKind::ConfigurationProgrammatic => {
                "configuration"
            }
            SafeOriginKind::ProfileSelectorCli | SafeOriginKind::ProfileSelectorProgrammatic => {
                "profile-selector"
            }
            other => other.id(),
        };
        let members = view.as_object_mut().unwrap();
        members.remove("source");
        members.insert("origin".to_owned(), json!(origin));
        blank(&mut view);
        view
    }

    /// Each occurrence of `found`, as its Finding and its Evidence as either
    /// entry would name it, in an order no token decides.
    fn entry_free_occurrences(found: &[FindingOccurrence]) -> Vec<(IssueKey, String)> {
        let mut free: Vec<(IssueKey, String)> = found
            .iter()
            .map(|occurrence| {
                let mut related: Vec<String> = occurrence
                    .related()
                    .iter()
                    .map(|evidence| entry_free(evidence).to_string())
                    .collect();
                related.sort();
                let primary = entry_free(occurrence.primary());
                (
                    occurrence.finding().clone(),
                    json!([primary, related]).to_string(),
                )
            })
            .collect();
        free.sort();
        free
    }

    /// D15:747, 790, 800, 804, 3228 (Decision 015-203): beside a sidecar that
    /// maps every value and key of the file of the same text at its token,
    /// Phases 2 and 3 observe the programmatic entry at the file's spans, a
    /// missing member at its owning object's, in the call-site source and
    /// under the programmatic origins; nothing else differs from the file's
    /// Evidence but the tokens of the origins.
    #[test]
    fn later_phases_take_a_files_spans_from_a_mirroring_sidecar() {
        let mut origins = BTreeSet::new();
        for (value, selector) in locations() {
            let text = serde_json::to_string(&value).unwrap();
            let call_site = sidecar(&text, None, mirrored(&text));
            let file = later_in_file(&value, selector);
            let snapshot = later_in_snapshot(&value, Some(&call_site), None, selector);
            assert!(!file.is_empty(), "{text}");
            assert_eq!(
                entry_free_occurrences(&snapshot),
                entry_free_occurrences(&file),
                "{text}"
            );
            for occurrence in &snapshot {
                let primary = std::iter::once(occurrence.primary());
                for evidence in primary.chain(occurrence.related()) {
                    origins.insert(evidence.origin());
                    if evidence.origin() == SafeOriginKind::ConfigurationProgrammatic {
                        assert_eq!(evidence.view()["source"], call_site_source());
                    }
                }
            }
        }
        assert!(origins.contains(&SafeOriginKind::ConfigurationProgrammatic));
        assert!(origins.contains(&SafeOriginKind::ProfileSelectorProgrammatic));
        assert!(!origins.contains(&SafeOriginKind::ConfigurationFile));
        assert!(!origins.contains(&SafeOriginKind::ProfileSelectorCli));
    }

    /// D15:796, 800: Materialized Value Evidence names the call-site source,
    /// with its own locator or none, only where the sidecar maps a span, and
    /// otherwise the Programmatic Entry's identity and locator, with no span.
    #[test]
    fn later_phases_name_the_programmatic_entry_where_no_span_is_mapped() {
        let mut value = minimal_config();
        value["extra"] = json!(true);
        value["more"] = json!(false);
        let text = serde_json::to_string(&value).unwrap();
        let extra = spot(&text, r#""extra""#, 0);
        let mapping = || vec![spanning(&json!(["extra"]), "member-key", extra)];
        let (unlabeled, labeled) = (
            sidecar(&text, None, mapping()),
            sidecar(&text, Some("app.ts"), mapping()),
        );
        let label = LocatorCandidate {
            variant: LocatorVariant::SafeLabel,
            payload: "storefront entry",
            non_secret: true,
        };
        let locator = read_programmatic_locator(Some(label), Bound::new(64).unwrap()).unwrap();
        let entry = json!([
            entry_source(),
            { "safe-label": "storefront entry" },
            Value::Null
        ]);
        let mapped = |locator: Value| json!([call_site_source(), locator, span_view(extra)]);
        for (call_site, expected) in [
            (None, [entry.clone(), entry.clone()]),
            (Some(&unlabeled), [mapped(Value::Null), entry.clone()]),
            (
                Some(&labeled),
                [mapped(json!({ "safe-label": "app.ts" })), entry],
            ),
        ] {
            let found = later_in_snapshot(&value, call_site, Some(&locator), "app");
            let mut seen: Vec<Value> = found
                .iter()
                .map(|occurrence| {
                    let view = occurrence.primary().view();
                    assert_eq!(view["origin"], "configuration-programmatic");
                    json!([
                        view["source"].clone(),
                        view["locator"].clone(),
                        view["span"].clone()
                    ])
                })
                .collect();
            seen.sort_by_key(Value::to_string);
            let mut expected = expected.to_vec();
            expected.sort_by_key(Value::to_string);
            assert_eq!(seen, expected);
        }
    }
}
