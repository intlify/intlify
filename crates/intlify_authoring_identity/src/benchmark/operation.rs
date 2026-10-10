// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The operations this owner measures, and what counts as the same result.
//!
//! Each operation is the ordinary public one. The harness prepares its input,
//! measures exactly one invocation, and afterwards observes the output.
//! Nothing inside a measured interval encodes an observation, validates a
//! result, or counts work beyond what the operation itself does.
//!
//! Wherever an observation frames a result itself, it keeps what the result
//! says apart from where it points: a unit, a role, a class and an ID are
//! semantic, and a byte range is a position. The update a plan proposes is
//! observed by its integrity digest instead, which covers its positions too.

use std::cell::RefCell;

use intlify_authoring::{
    AdmittedInventory, ByteRange, Diagnostic, Location, MessageIntentId, Occurrence, Token,
    VersionedIdentity,
};
use intlify_measurement::owner_run::observation::{Frame, Observation};
use intlify_measurement::owner_run::run::Expected;
use intlify_shared_json::quantity::Quantity;
use serde::{Deserialize, Serialize};

use super::owner::LABELS;
use crate::{
    reconcile, verify_history, AdmittedRegistry, Anchor, Classification, DeclarationClass,
    Eligibility, EntryClass, HistoryFailure, HistoryOutcome, IdentityDecision, IdentityLimits,
    IdentityWorkspace, ReconcileFailure, ReconcileInputs, Reconciliation, RetainedHistory,
};

/// The two operations design 016 names for this phase's reconciliation.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Operation {
    /// An admitted base and current inventory to association decisions and
    /// a proposed plan, with fixed allocation candidates.
    AssociationPlanning,
    /// A retained chain verified from its head back to an anchor and
    /// replayed forward.
    RegistryReplay,
}

impl Operation {
    /// The complete set this phase measures.
    ///
    /// Allocation is not one of them: drawing an ID is the host's, outside
    /// deterministic reconciliation, and the candidates a fixture supplies are
    /// fixed. A case here never reports allocation work as zero.
    #[cfg(test)]
    pub(super) const ALL: [Self; 2] = [Self::AssociationPlanning, Self::RegistryReplay];

    pub(super) const fn phase(self) -> &'static str {
        match self {
            Self::AssociationPlanning | Self::RegistryReplay => "identity_reconciliation",
        }
    }

    pub(super) const fn cost(self) -> &'static str {
        match self {
            Self::AssociationPlanning => "association_planning",
            Self::RegistryReplay => "registry_replay",
        }
    }

    pub(super) const fn boundary(self) -> &'static str {
        match self {
            Self::AssociationPlanning => "intlify-authoring-identity-association-planning",
            Self::RegistryReplay => "intlify-authoring-identity-registry-replay",
        }
    }
}

/// How one counted fact is known.
///
/// A count this harness does not take is said to be unavailable, and work a
/// path never does is said not to apply. Neither is written as zero, because
/// zero is a count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub(super) enum Count {
    /// Counted in full.
    Exact { value: Quantity },
    /// This revision of the harness does not count it.
    Unavailable {},
    /// The path the operation took does no such work.
    NotApplicable {},
}

impl Count {
    const fn exact(value: u64) -> Self {
        Self::Exact {
            value: Quantity::new(value),
        }
    }
}

/// One counted fact about what an operation processed.
///
/// These are counts, never durations, and never a rate derived from one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkFact {
    name: Token,
    count: Count,
}

/// The complete logical work one case performed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LogicalWork {
    specification: VersionedIdentity,
    facts: Vec<WorkFact>,
}

impl LogicalWork {
    pub(super) fn new(facts: impl IntoIterator<Item = (&'static str, Count)>) -> Self {
        Self {
            specification: VersionedIdentity::literal(
                "intlify-authoring-identity-logical-work",
                "0",
            ),
            facts: facts
                .into_iter()
                .map(|(name, count)| WorkFact {
                    name: Token::literal(name),
                    count,
                })
                .collect(),
        }
    }
}

/// What one measured invocation produced, observed after the interval closed.
pub(super) type Observed = intlify_measurement::owner_run::capture::Observed<LogicalWork>;

fn range(frame: &mut Frame, range: ByteRange) {
    frame.uint(range.start());
    frame.uint(range.end());
}

/// Frame the unit an occurrence is in and the role it plays there, and,
/// apart from that, where in the unit it is.
fn located(semantic: &mut Frame, positions: &mut Frame, occurrence: &Occurrence) {
    semantic.text(occurrence.source().unit().as_str());
    semantic.text(occurrence.role().as_str());
    range(positions, occurrence.range());
}

/// Frame each record's meaning, and where it points apart from that.
fn records(semantic: &mut Frame, positions: &mut Frame, records: &[Diagnostic]) {
    semantic.uint(records.len() as u64);
    for record in records {
        semantic.text(record.origin().code());
        semantic.flag(record.detail().is_some());
        if let Some(detail) = record.detail() {
            semantic.text(detail.as_str());
        }
        match record.location() {
            Location::Unit(snapshot) => {
                semantic.uint(0);
                semantic.text(snapshot.unit().as_str());
            }
            Location::Region(region) => {
                semantic.uint(1);
                semantic.text(region.source().unit().as_str());
                range(positions, region.range());
            }
            Location::Occurrence(occurrence) => {
                semantic.uint(2);
                located(semantic, positions, occurrence);
            }
        }
        semantic.uint(record.related().len() as u64);
        for related in record.related() {
            located(semantic, positions, related);
        }
    }
}

fn identity(frame: &mut Frame, id: &MessageIntentId) {
    frame.text(id.value().as_str());
}

// --- association planning --------------------------------------------------

/// Everything one planning invocation reads.
pub(super) type PlanInput<'a> = (
    &'a AdmittedRegistry,
    &'a AdmittedInventory,
    &'a ReconcileInputs<'a>,
    &'a IdentityLimits,
    &'a RefCell<IdentityWorkspace>,
);

pub(super) type PlanOutput = Result<Reconciliation, ReconcileFailure>;

pub(super) fn invoke_plan(input: PlanInput<'_>) -> PlanOutput {
    let (base, inventory, inputs, limits, workspace) = input;
    reconcile(base, inventory, inputs, limits, &mut workspace.borrow_mut())
}

/// The counts every planning case has, read from what it was given.
fn supplied(input: PlanInput<'_>) -> [(&'static str, Count); 7] {
    let (base, inventory, inputs, _, _) = input;
    let snapshot = base.snapshot();
    let current = inventory.inventory();
    [
        (
            "base_entries",
            Count::exact(snapshot.entries().len() as u64),
        ),
        ("units", Count::exact(current.units().len() as u64)),
        (
            "declarations",
            Count::exact(current.declarations().len() as u64),
        ),
        ("edits", Count::exact(inputs.evidence.edits.len() as u64)),
        (
            "explicit_decisions",
            Count::exact(inputs.explicit.len() as u64),
        ),
        ("lineage_links", Count::exact(inputs.links.len() as u64)),
        ("candidates", Count::exact(inputs.candidates.len() as u64)),
    ]
}

/// What each current declaration and each gone entry was classified as.
const CLASSES: [&str; 10] = [
    "retained",
    "continued",
    "explicit",
    "new",
    "unresolved",
    "conflicting",
    "carried_entries",
    "absent_entries",
    "kept_entries",
    "unsettled_entries",
];

/// Which of [`CLASSES`] a current declaration's class is counted under,
/// which is also the tag its class is framed with.
const fn declaration_slot(class: &DeclarationClass) -> usize {
    match class {
        DeclarationClass::Retained(_) => 0,
        DeclarationClass::Continued(_) => 1,
        DeclarationClass::Explicit(_) => 2,
        DeclarationClass::New => 3,
        DeclarationClass::Unresolved(_) => 4,
        DeclarationClass::Conflict(_) => 5,
    }
}

/// Which of [`CLASSES`] a gone entry's class is counted under.
///
/// An entry is carried onto a current declaration whether evidence or an
/// explicit decision carries it, and left unsettled whether evidence is
/// missing or inputs conflict.
const fn entry_slot(class: &EntryClass) -> usize {
    match class {
        EntryClass::Continued(_) | EntryClass::Explicit => 6,
        EntryClass::Absent => 7,
        EntryClass::Kept => 8,
        EntryClass::Unresolved(_) | EntryClass::Conflict(_) => 9,
    }
}

fn classes(classification: &Classification) -> [(&'static str, Count); 10] {
    let mut counts = [0_u64; 10];
    for (_, class) in classification.declarations() {
        counts[declaration_slot(class)] += 1;
    }
    for (_, class) in classification.entries() {
        counts[entry_slot(class)] += 1;
    }
    std::array::from_fn(|slot| (CLASSES[slot], Count::exact(counts[slot])))
}

/// Frame one current declaration's class: its tag, then what it names.
fn declaration_class(semantic: &mut Frame, class: &DeclarationClass) {
    semantic.uint(declaration_slot(class) as u64);
    match class {
        DeclarationClass::Retained(id)
        | DeclarationClass::Continued(id)
        | DeclarationClass::Explicit(id) => identity(semantic, id),
        DeclarationClass::New => {}
        DeclarationClass::Unresolved(gap) => semantic.text(&format!("{gap:?}")),
        DeclarationClass::Conflict(conflict) => semantic.text(&format!("{conflict:?}")),
    }
}

/// Frame one gone entry's class: its tag, then what it names.
fn entry_class(semantic: &mut Frame, positions: &mut Frame, class: &EntryClass) {
    match class {
        EntryClass::Continued(occurrence) => {
            semantic.uint(0);
            located(semantic, positions, occurrence);
        }
        EntryClass::Explicit => semantic.uint(1),
        EntryClass::Absent => semantic.uint(2),
        EntryClass::Kept => semantic.uint(3),
        EntryClass::Unresolved(gap) => {
            semantic.uint(4);
            semantic.text(&format!("{gap:?}"));
        }
        EntryClass::Conflict(conflict) => {
            semantic.uint(5);
            semantic.text(&format!("{conflict:?}"));
        }
    }
}

/// Frame what the classification says, in its own canonical orders.
fn classification(semantic: &mut Frame, positions: &mut Frame, classification: &Classification) {
    semantic.uint(classification.declarations().len() as u64);
    for (occurrence, class) in classification.declarations() {
        located(semantic, positions, occurrence);
        declaration_class(semantic, class);
    }
    semantic.uint(classification.entries().len() as u64);
    for (id, class) in classification.entries() {
        identity(semantic, id);
        entry_class(semantic, positions, class);
    }
}

/// Frame which decisions need the host's confirmation, in decision order.
fn eligibility(semantic: &mut Frame, eligibility: &[(MessageIntentId, Eligibility)]) {
    semantic.uint(eligibility.len() as u64);
    for (id, eligibility) in eligibility {
        identity(semantic, id);
        semantic.flag(*eligibility == Eligibility::RequiresConfirmation);
    }
}

/// Counts only a planned update has.
const PLANNED: [&str; 5] = [
    "decisions",
    "allocations",
    "continuations",
    "retirements",
    "restorations",
];

fn decisions(decisions: &[IdentityDecision]) -> [(&'static str, Count); 5] {
    let kind = |wanted: fn(&IdentityDecision) -> bool| {
        decisions.iter().filter(|decision| wanted(decision)).count() as u64
    };
    [
        ("decisions", Count::exact(decisions.len() as u64)),
        (
            "allocations",
            Count::exact(kind(|decision| {
                matches!(decision, IdentityDecision::Allocate(_))
            })),
        ),
        (
            "continuations",
            Count::exact(kind(|decision| {
                matches!(decision, IdentityDecision::Continue(_))
            })),
        ),
        (
            "retirements",
            Count::exact(kind(|decision| {
                matches!(decision, IdentityDecision::Retire(_))
            })),
        ),
        (
            "restorations",
            Count::exact(kind(|decision| {
                matches!(decision, IdentityDecision::Restore(_))
            })),
        ),
    ]
}

/// Counts nothing in this revision of the harness takes, on any path.
const UNCOUNTED: [&str; 1] = ["candidate_comparisons"];

fn planning_work(
    input: PlanInput<'_>,
    classified: [(&'static str, Count); 10],
    planned: [(&'static str, Count); 5],
    diagnostics: Count,
) -> LogicalWork {
    LogicalWork::new(
        supplied(input)
            .into_iter()
            .chain(UNCOUNTED.map(|name| (name, Count::Unavailable {})))
            .chain(classified)
            .chain(planned)
            .chain([("diagnostics", diagnostics)]),
    )
}

// The capture measures an operation through `fn(I) -> O` and observes it
// through `fn(&O, I)`, so the observer takes whatever the operation returned.
pub(super) fn observe_plan(output: &PlanOutput, input: PlanInput<'_>) -> Observed {
    let mut semantic = LABELS.framing.frame("association-planning");
    let mut positions = LABELS.framing.frame("association-planning-positions");
    let unknown = |names: &[&'static str]| -> Vec<(&'static str, Count)> {
        names
            .iter()
            .map(|name| (*name, Count::Unavailable {}))
            .collect()
    };
    let not_done = |names: &[&'static str]| -> Vec<(&'static str, Count)> {
        names
            .iter()
            .map(|name| (*name, Count::NotApplicable {}))
            .collect()
    };
    let (observation, work, path) = match output {
        Err(failure) => {
            semantic.uint(0);
            semantic.text(&format!("{failure:?}"));
            (
                Observation {
                    semantic: semantic.finish(),
                    positions: None,
                },
                planning_work(
                    input,
                    unknown(&CLASSES).try_into().expect("one per class"),
                    unknown(&PLANNED).try_into().expect("one per count"),
                    Count::Unavailable {},
                ),
                Expected::OperationalFailure,
            )
        }
        Ok(Reconciliation::Unchanged) => {
            semantic.uint(1);
            // Nothing is classified or planned when nothing changed; what is
            // not observed is said to be unavailable, not counted as zero.
            (
                Observation {
                    semantic: semantic.finish(),
                    positions: None,
                },
                planning_work(
                    input,
                    unknown(&CLASSES).try_into().expect("one per class"),
                    not_done(&PLANNED).try_into().expect("one per count"),
                    Count::NotApplicable {},
                ),
                Expected::Complete,
            )
        }
        Ok(Reconciliation::Planned(plan)) => {
            semantic.uint(2);
            // The update's integrity digest covers every decision and link,
            // so it is the exact plan observation.
            semantic.text(plan.update().reference().integrity_digest().as_str());
            eligibility(&mut semantic, plan.eligibility());
            classification(&mut semantic, &mut positions, plan.classification());
            (
                Observation {
                    semantic: semantic.finish(),
                    positions: Some(positions.finish()),
                },
                planning_work(
                    input,
                    classes(plan.classification()),
                    decisions(plan.update().update().decisions()),
                    Count::exact(0),
                ),
                Expected::Complete,
            )
        }
        Ok(Reconciliation::Unresolved(report)) => {
            semantic.uint(3);
            records(&mut semantic, &mut positions, report.diagnostics());
            classification(&mut semantic, &mut positions, report.classification());
            (
                Observation {
                    semantic: semantic.finish(),
                    positions: Some(positions.finish()),
                },
                planning_work(
                    input,
                    classes(report.classification()),
                    not_done(&PLANNED).try_into().expect("one per count"),
                    Count::exact(report.diagnostics().len() as u64),
                ),
                Expected::Blocked,
            )
        }
    };
    Observed {
        observation,
        work,
        path,
    }
}

// --- registry replay --------------------------------------------------------

/// How many retained artifacts of each kind a replay case was given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Retained {
    pub(super) registries: u64,
    pub(super) updates: u64,
    pub(super) inventories: u64,
}

/// Everything one replay invocation reads.
pub(super) type ReplayInput<'a> = (
    &'a AdmittedRegistry,
    &'a Anchor<'a>,
    &'a RetainedHistory<'a>,
    &'a IdentityLimits,
    Retained,
);

pub(super) type ReplayOutput = Result<HistoryOutcome, HistoryFailure>;

pub(super) fn invoke_replay(input: ReplayInput<'_>) -> ReplayOutput {
    let (head, anchor, retained, limits, _) = input;
    verify_history(head, anchor, retained, limits)
}

pub(super) fn observe_replay(output: &ReplayOutput, input: ReplayInput<'_>) -> Observed {
    let (head, _, _, _, retained) = input;
    let mut semantic = LABELS.framing.frame("registry-replay");
    let supplied = [
        (
            "head_entries",
            Count::exact(head.snapshot().entries().len() as u64),
        ),
        ("retained_registries", Count::exact(retained.registries)),
        ("retained_updates", Count::exact(retained.updates)),
        ("retained_inventories", Count::exact(retained.inventories)),
    ];
    let (replayed, path) = match output {
        Err(failure) => {
            semantic.uint(0);
            semantic.text(&format!("{failure:?}"));
            // A chain that fails stops wherever it stopped; how many steps
            // replayed before that is not observed.
            (Count::Unavailable {}, Expected::OperationalFailure)
        }
        Ok(HistoryOutcome::Verified { steps }) => {
            semantic.uint(1);
            semantic.uint(*steps);
            (Count::exact(*steps), Expected::Complete)
        }
        Ok(HistoryOutcome::Blocked { missing }) => {
            semantic.uint(2);
            semantic.text(&format!("{:?}", missing.kind()));
            semantic.text(missing.integrity_digest().as_str());
            // Every step is resolved before any is replayed, so a blocked
            // chain replays none.
            (Count::exact(0), Expected::Blocked)
        }
    };
    Observed {
        observation: Observation {
            semantic: semantic.finish(),
            positions: None,
        },
        work: LogicalWork::new(supplied.into_iter().chain([("replayed_steps", replayed)])),
        path,
    }
}

#[cfg(test)]
mod tests {
    use intlify_authoring::{
        DiagnosticOrigin, IntegrityDigest, OccurrenceRole, Opaque128, OwnerIdentity, OwnerKind,
        ReasonFamily, Region, Severity, SourceSnapshot, Stage,
    };
    use intlify_measurement::owner_run::observation::Digest;
    use intlify_shared_json::encoding::digest_bytes;

    use super::super::cases::testing;
    use super::*;
    use crate::{
        detail, AllocationBasis, BasisGap, Conflict, ContinuationBasis, ExplicitBasis,
        IdentityCapacities,
    };

    /// One framed item, as this revision of the observation writes it.
    #[derive(Debug, Clone, Copy)]
    enum Item<'a> {
        Uint(u64),
        Flag(bool),
        Text(&'a str),
    }

    use Item::{Flag, Text, Uint};

    /// The digest a frame of exactly these items closes to.
    fn transcript(domain: &str, items: &[Item<'_>]) -> Digest {
        let mut frame = LABELS.framing.frame(domain);
        for item in items {
            match *item {
                Uint(value) => frame.uint(value),
                Flag(value) => frame.flag(value),
                Text(value) => frame.text(value),
            }
        }
        frame.finish()
    }

    /// What one framing step writes into a fresh pair of frames.
    fn framed(write: impl FnOnce(&mut Frame, &mut Frame)) -> (Digest, Digest) {
        let mut semantic = LABELS.framing.frame("semantic");
        let mut positions = LABELS.framing.frame("positions");
        write(&mut semantic, &mut positions);
        (semantic.finish(), positions.finish())
    }

    /// The pair of frames holding exactly these items.
    fn expected(semantic: &[Item<'_>], positions: &[Item<'_>]) -> (Digest, Digest) {
        (
            transcript("semantic", semantic),
            transcript("positions", positions),
        )
    }

    fn owner() -> OwnerIdentity {
        OwnerIdentity::new(OwnerKind::Application, "storefront").unwrap()
    }

    fn snapshot(unit: &str) -> SourceSnapshot {
        SourceSnapshot::new(
            owner(),
            unit,
            "1",
            VersionedIdentity::literal("fixture-grammar", "0"),
            64,
            IntegrityDigest::from_hash(digest_bytes(unit.as_bytes())).as_str(),
        )
        .unwrap()
    }

    fn occurrence(unit: &str, start: u64, end: u64, role: OccurrenceRole) -> Occurrence {
        Occurrence::new(snapshot(unit), ByteRange::new(start, end).unwrap(), role).unwrap()
    }

    fn id(ordinal: u8) -> MessageIntentId {
        MessageIntentId::retained(owner(), Opaque128::from_bytes([ordinal; 16]).as_str()).unwrap()
    }

    #[test]
    fn the_operations_and_their_intervals_are_spelt_as_registered() {
        let spelt = Operation::ALL
            .map(|operation| (operation.phase(), operation.cost(), operation.boundary()));
        assert_eq!(
            spelt,
            [
                (
                    "identity_reconciliation",
                    "association_planning",
                    "intlify-authoring-identity-association-planning"
                ),
                (
                    "identity_reconciliation",
                    "registry_replay",
                    "intlify-authoring-identity-registry-replay"
                ),
            ]
        );
        assert_eq!(
            serde_json::to_value(LogicalWork::new([("units", Count::exact(2))])).unwrap(),
            serde_json::json!({
                "specification": {
                    "identity": "intlify-authoring-identity-logical-work",
                    "revision": "0"
                },
                "facts": [{ "name": "units", "count": { "kind": "exact", "value": "2" } }]
            })
        );
    }

    #[test]
    fn a_record_is_framed_as_this_revision_of_the_observation_writes_it() {
        let required = DiagnosticOrigin::Authoring(ReasonFamily::AuthoringIdentityUpdateRequired);
        let conflict = DiagnosticOrigin::Authoring(ReasonFamily::AuthoringIdentityConflict);
        let record = |origin, location: Location| {
            Diagnostic::new(Stage::IdentityResolution, origin, Severity::Error, location)
        };
        let region = Region::new(snapshot("nav"), ByteRange::new(3, 9).unwrap()).unwrap();
        let reported = [
            // A whole unit, with no detail and nothing related.
            record(required, Location::Unit(snapshot("checkout"))),
            // A region, which plays no role.
            record(conflict, Location::Region(region)).with_detail(detail::competing_claim()),
            // An occurrence, and the declarations related to it.
            record(
                required,
                Location::Occurrence(occurrence(
                    "checkout",
                    10,
                    20,
                    OccurrenceRole::IntentLiteral,
                )),
            )
            .with_detail(detail::continuity_ambiguous())
            .with_related(vec![
                occurrence("nav", 1, 4, OccurrenceRole::UiLiteral),
                occurrence("checkout", 30, 40, OccurrenceRole::IntentLiteral),
            ]),
        ];
        assert_eq!(
            framed(|semantic, positions| records(semantic, positions, &reported)),
            expected(
                &[
                    Uint(3),
                    // The unit.
                    Text("authoring-identity-update-required"),
                    Flag(false),
                    Uint(0),
                    Text("checkout"),
                    Uint(0),
                    // The region.
                    Text("authoring-identity-conflict"),
                    Flag(true),
                    Text("identity-competing-claim"),
                    Uint(1),
                    Text("nav"),
                    Uint(0),
                    // The occurrence.
                    Text("authoring-identity-update-required"),
                    Flag(true),
                    Text("identity-continuity-ambiguous"),
                    Uint(2),
                    Text("checkout"),
                    Text("intent-literal"),
                    Uint(2),
                    Text("nav"),
                    Text("ui-literal"),
                    Text("checkout"),
                    Text("intent-literal"),
                ],
                &[
                    Uint(3),
                    Uint(9),
                    Uint(10),
                    Uint(20),
                    Uint(1),
                    Uint(4),
                    Uint(30),
                    Uint(40),
                ],
            )
        );
    }

    #[test]
    fn each_class_is_counted_and_framed_under_its_own_name_and_tag() {
        let held = id(1);
        let name = held.value().as_str();
        let declarations = [
            (
                DeclarationClass::Retained(held.clone()),
                "retained",
                vec![Uint(0), Text(name)],
            ),
            (
                DeclarationClass::Continued(held.clone()),
                "continued",
                vec![Uint(1), Text(name)],
            ),
            (
                DeclarationClass::Explicit(held.clone()),
                "explicit",
                vec![Uint(2), Text(name)],
            ),
            (DeclarationClass::New, "new", vec![Uint(3)]),
            (
                DeclarationClass::Unresolved(BasisGap::Ambiguous),
                "unresolved",
                vec![Uint(4), Text("Ambiguous")],
            ),
            (
                DeclarationClass::Conflict(Conflict::CompetingClaim),
                "conflicting",
                vec![Uint(5), Text("CompetingClaim")],
            ),
        ];
        for (class, counted, items) in &declarations {
            assert_eq!(CLASSES[declaration_slot(class)], *counted, "{class:?}");
            assert_eq!(
                framed(|semantic, _| declaration_class(semantic, class)),
                expected(items, &[]),
                "{class:?}"
            );
        }
        let carried =
            EntryClass::Continued(occurrence("checkout", 10, 20, OccurrenceRole::UiLiteral));
        let entries = [
            (
                carried,
                "carried_entries",
                vec![Uint(0), Text("checkout"), Text("ui-literal")],
                vec![Uint(10), Uint(20)],
            ),
            (
                EntryClass::Explicit,
                "carried_entries",
                vec![Uint(1)],
                vec![],
            ),
            (EntryClass::Absent, "absent_entries", vec![Uint(2)], vec![]),
            (EntryClass::Kept, "kept_entries", vec![Uint(3)], vec![]),
            (
                EntryClass::Unresolved(BasisGap::EditMissing),
                "unsettled_entries",
                vec![Uint(4), Text("EditMissing")],
                vec![],
            ),
            (
                EntryClass::Conflict(Conflict::Collision),
                "unsettled_entries",
                vec![Uint(5), Text("Collision")],
                vec![],
            ),
        ];
        for (class, counted, semantic, positions) in &entries {
            assert_eq!(CLASSES[entry_slot(class)], *counted, "{class:?}");
            assert_eq!(
                framed(|frame, at| entry_class(frame, at, class)),
                expected(semantic, positions),
                "{class:?}"
            );
        }
    }

    #[test]
    fn a_classification_is_framed_declaration_by_declaration_then_entry_by_entry() {
        // Pay continues across the deletion and home stays; pay's old
        // declaration is carried onto its new one, and cancel's is gone.
        let deleted = testing::prepared("complete-deletion");
        let Ok(Reconciliation::Planned(plan)) = testing::reconciled(&deleted) else {
            panic!("the deletion is planned");
        };
        let classified = plan.classification();
        let (pay, home) = (
            &classified.declarations()[0].0,
            &classified.declarations()[1].0,
        );
        let ids = testing::candidates(0, 3);
        let at = |occurrence: &Occurrence| [occurrence.range().start(), occurrence.range().end()];
        let [pay_start, pay_end] = at(pay);
        let [home_start, home_end] = at(home);
        assert_eq!(
            framed(|semantic, positions| classification(semantic, positions, classified)),
            expected(
                &[
                    Uint(2),
                    Text("checkout"),
                    Text("intent-literal"),
                    Uint(1),
                    Text(ids[0].value().as_str()),
                    Text("nav"),
                    Text("intent-literal"),
                    Uint(0),
                    Text(ids[2].value().as_str()),
                    Uint(2),
                    Text(ids[0].value().as_str()),
                    Uint(0),
                    Text("checkout"),
                    Text("intent-literal"),
                    Text(ids[1].value().as_str()),
                    Uint(2),
                ],
                &[
                    Uint(pay_start),
                    Uint(pay_end),
                    Uint(home_start),
                    Uint(home_end),
                    Uint(pay_start),
                    Uint(pay_end),
                ],
            )
        );
    }

    #[test]
    fn eligibility_is_framed_in_decision_order_with_what_needs_confirmation() {
        let (automatic, confirmed) = (id(1), id(2));
        assert_eq!(
            framed(|semantic, _| eligibility(
                semantic,
                &[
                    (automatic.clone(), Eligibility::Automatic),
                    (confirmed.clone(), Eligibility::RequiresConfirmation),
                ]
            )),
            expected(
                &[
                    Uint(2),
                    Text(automatic.value().as_str()),
                    Flag(false),
                    Text(confirmed.value().as_str()),
                    Flag(true),
                ],
                &[]
            )
        );
    }

    #[test]
    fn each_decision_is_counted_under_its_own_kind() {
        let from = occurrence("checkout", 0, 10, OccurrenceRole::IntentLiteral);
        let to = occurrence("checkout", 20, 30, OccurrenceRole::IntentLiteral);
        // A different number of each kind, so no two counts can trade places.
        let mut planned = vec![IdentityDecision::allocation(
            id(1),
            to.clone(),
            AllocationBasis::confirmed_new(),
        )];
        planned.extend((0..2).map(|_| {
            IdentityDecision::continuation(
                id(2),
                from.clone(),
                to.clone(),
                ContinuationBasis::unchanged_snapshot(),
            )
        }));
        planned.extend((0..3).map(|_| IdentityDecision::retirement(id(3), from.clone())));
        planned.extend((0..4).map(|_| {
            IdentityDecision::restoration(
                id(4),
                from.clone(),
                to.clone(),
                ExplicitBasis::new("restored").unwrap(),
            )
        }));
        assert_eq!(
            decisions(&planned),
            [
                ("decisions", Count::exact(10)),
                ("allocations", Count::exact(1)),
                ("continuations", Count::exact(2)),
                ("retirements", Count::exact(3)),
                ("restorations", Count::exact(4)),
            ]
        );
    }

    #[test]
    fn each_planning_path_is_framed_under_its_own_tag() {
        let semantic = |items: &[Item<'_>]| transcript("association-planning", items);
        // A refusal is its failure alone.
        let refused = testing::prepared("candidates-first-over");
        let Err(failure) = testing::reconciled(&refused) else {
            panic!("the candidates are refused");
        };
        assert_eq!(
            refused.expected().observation,
            Observation {
                semantic: semantic(&[Uint(0), Text(&format!("{failure:?}"))]),
                positions: None,
            }
        );
        // Nothing changed is its tag alone.
        assert_eq!(
            testing::prepared("catalog-retained").expected().observation,
            Observation {
                semantic: semantic(&[Uint(1)]),
                positions: None,
            }
        );
        // A plan is its update, which decisions need confirmation, and the
        // classification it was made from.
        let copied = testing::prepared("equal-text-copy");
        let Ok(Reconciliation::Planned(plan)) = testing::reconciled(&copied) else {
            panic!("the copy is planned");
        };
        let mut planned = LABELS.framing.frame("association-planning");
        let mut positions = LABELS.framing.frame("association-planning-positions");
        planned.uint(2);
        planned.text(plan.update().reference().integrity_digest().as_str());
        eligibility(&mut planned, plan.eligibility());
        classification(&mut planned, &mut positions, plan.classification());
        assert_eq!(
            copied.expected().observation,
            Observation {
                semantic: planned.finish(),
                positions: Some(positions.finish()),
            }
        );
        // An unresolved plan is its records and its classification.
        let ambiguous = testing::prepared("ambiguous-copy");
        let Ok(Reconciliation::Unresolved(report)) = testing::reconciled(&ambiguous) else {
            panic!("the copy is unresolved");
        };
        let mut unresolved = LABELS.framing.frame("association-planning");
        let mut positions = LABELS.framing.frame("association-planning-positions");
        unresolved.uint(3);
        records(&mut unresolved, &mut positions, report.diagnostics());
        classification(&mut unresolved, &mut positions, report.classification());
        assert_eq!(
            ambiguous.expected().observation,
            Observation {
                semantic: unresolved.finish(),
                positions: Some(positions.finish()),
            }
        );
    }

    #[test]
    fn each_replay_path_is_framed_under_its_own_tag() {
        let semantic = |items: &[Item<'_>]| Observation {
            semantic: transcript("registry-replay", items),
            positions: None,
        };
        assert_eq!(
            testing::prepared("small-chain").expected().observation,
            semantic(&[Uint(1), Uint(2)])
        );
        // The small chain retained without its first update is blocked on
        // exactly that update.
        let small = testing::prepared("small-chain");
        let first = testing::replay_case(&small).updates[0].reference();
        assert_eq!(
            testing::prepared("missing-update").expected().observation,
            semantic(&[
                Uint(2),
                Text(&format!("{:?}", first.kind())),
                Text(first.integrity_digest().as_str()),
            ])
        );
        let refused = testing::prepared("history-steps-first-over");
        let Err(failure) = testing::replayed(&refused) else {
            panic!("the history is refused");
        };
        assert_eq!(
            refused.expected().observation,
            semantic(&[Uint(0), Text(&format!("{failure:?}"))])
        );
    }

    #[test]
    fn the_measured_invocation_reconciles_in_the_case_workspace() {
        let prepared = testing::prepared("catalog-allocation");
        let case = testing::plan_case(&prepared);
        let workspace = RefCell::new(IdentityWorkspace::new());
        let empty = IdentityCapacities {
            classes: 0,
            entries: 0,
            diagnostics: 0,
        };
        assert_eq!(workspace.borrow().capacities(), empty);
        let planned = case
            .evidence
            .inputs(|inputs| {
                invoke_plan((
                    &case.base,
                    &case.inventory,
                    inputs,
                    &case.limits,
                    &workspace,
                ))
            })
            .unwrap();
        assert!(matches!(planned, Ok(Reconciliation::Planned(_))));
        // The scratch the plan was made in is the one the case lent.
        assert_ne!(workspace.borrow().capacities(), empty);
    }
}
