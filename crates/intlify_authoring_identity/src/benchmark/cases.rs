// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The fixed cases this owner measures, and how each is prepared.
//!
//! Every inventory is read by the real JS Producer from source text written
//! here, under the explicitly test-owned context, and every base registry is
//! built by planning and applying the updates that lead to it. All of that
//! happens before any interval opens. Allocation candidates are fixed
//! values: drawing an ID belongs to the host, and is not measured here.

use std::cell::RefCell;
use std::fmt::Write as _;

use intlify_authoring::test_context::{admit_inventory, TestContext};
use intlify_authoring::{
    AdmittedInventory, AnalysisWorkspace, AuthoringArtifact, AuthoringLimits, ByteRange,
    Completeness, IntegrityDigest, MessageIntentId, Opaque128, OwnerIdentity, OwnerKind,
    SourceSnapshot, SurfaceVocabulary, Token,
};
use intlify_authoring_js::{
    admit_units, analyze_unit, assemble_inventory, DomGlobal, Grammar, Intrinsic, IntrinsicBinding,
    JsAnalysisWorkspace, JsAuthoringLimits, JsAuthoringProfile, SourceUnit, UnitMember,
};
use intlify_measurement::owner_run::run::{Expected, PreparationFailure};
use intlify_measurement::owner_run::Case;
use intlify_shared_json::encoding::digest_bytes;

use super::operation::{self, Observed, Operation, Retained};
use crate::{
    admit_registry, reconcile, verify_history, AdmittedRegistry, AdmittedUpdate, Anchor,
    ContinuityInputs, HistoryOutcome, IdentityLimits, IdentityWorkspace, IntentRegistrySnapshot,
    LineageKind, LineageLink, PreviousUpdate, ReconcileInputs, Reconciliation, RegistryArtifact,
    RegistryIdentity, Replacement, RetainedHistory, RetainedSources, SourceEdit,
};

/// The module the fixture profile registers its intrinsics under.
const MODULE: &str = "fixture-authoring";

const HEADER: &str = "import { intent } from 'fixture-authoring'\n";
const PAY: &str = "export const pay = intent('Pay now')\n";
const AGAIN: &str = "export const again = intent('Pay now')\n";
const CANCEL: &str = "export const cancel = intent('Cancel')\n";
const HOME: &str = "export const home = intent('Home')\n";

/// The owning scope every inventory declares.
pub(super) const SCOPE: &str = "storefront-web";

/// How many short UI literals the large catalog declares.
pub(super) const CATALOG_DECLARATIONS: u64 = 128;

/// The registry identities the two chains are rooted at. Fixture values,
/// standing in for what a host draws.
const SMALL_CHAIN: &str = "c0ffee00000000000000000000000001";
const CATALOG_CHAIN: &str = "c0ffee00000000000000000000000002";

/// A bound a fixture can be measured at, as its case names it.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Limit {
    /// The allocation candidates one reconciliation is offered.
    Candidates,
    /// The diagnostics one reconciliation reports.
    Diagnostics,
    /// The updates one replay verifies.
    HistorySteps,
}

/// Which side of a bound a fixture is measured on.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Edge {
    /// The bound is the fixture's own count, which it may reach.
    Exact,
    /// The bound is one below the fixture's own count.
    FirstOver,
}

/// The bound a planning fixture is measured at, when it is measured at one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PlanBound {
    /// Every bound has room to spare.
    Roomy,
    /// The candidates bound, at the candidates the fixture offers.
    Candidates(Edge),
    /// The diagnostics bound, at the diagnostics the fixture reports.
    Diagnostics(Edge),
}

/// The bound a replay fixture is measured at, when it is measured at one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReplayBound {
    /// Every bound has room to spare.
    Roomy,
    /// The history steps bound, at the steps the fixture's chain replays.
    HistorySteps(Edge),
}

/// Which planning a fixture measures.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Planning {
    /// The checkout and nav units, allocated from the genesis.
    FirstAllocation,
    /// The catalog's short literals, allocated from its genesis.
    CatalogAllocation,
    /// The catalog read again against the registry it produced.
    CatalogRetained,
    /// A second `'Pay now'` inserted beside the first, with the edit that
    /// shows it: one continues, the copy is new.
    EqualTextCopy,
    /// The same copy without the edit: nothing shows which is which.
    AmbiguousCopy,
    /// `'Cancel'` deleted from a complete view, with its edit.
    CompleteDeletion,
    /// The same deletion in a partial view of checkout alone.
    PartialDeletion,
}

/// Which chain a replay fixture verifies.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Chain {
    /// The genesis, the first allocation and the copy: two steps.
    Small,
    /// The catalog's genesis and its allocation: one step of many entries.
    Catalog,
    /// The small chain retained without its first update.
    SmallMissingUpdate,
}

/// What one fixture reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Input {
    /// One reconciliation.
    Plan {
        planning: Planning,
        bound: PlanBound,
    },
    /// One chain verified from its head to its genesis.
    Replay { chain: Chain, bound: ReplayBound },
}

/// One fixed case this owner measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fixture {
    pub(super) operation: Operation,
    pub(super) name: &'static str,
    pub(super) input: Input,
    /// The path this fixture is declared to take.
    pub(super) expected: Expected,
}

impl Case for Fixture {
    fn name(&self) -> &'static str {
        self.name
    }
    fn path(&self) -> Expected {
        self.expected
    }
}

const fn plan(
    name: &'static str,
    planning: Planning,
    bound: PlanBound,
    expected: Expected,
) -> Fixture {
    Fixture {
        operation: Operation::AssociationPlanning,
        name,
        input: Input::Plan { planning, bound },
        expected,
    }
}

const fn replay(
    name: &'static str,
    chain: Chain,
    bound: ReplayBound,
    expected: Expected,
) -> Fixture {
    Fixture {
        operation: Operation::RegistryReplay,
        name,
        input: Input::Replay { chain, bound },
        expected,
    }
}

const COMPLETE: Expected = Expected::Complete;
const BLOCKED: Expected = Expected::Blocked;
const REFUSED: Expected = Expected::OperationalFailure;
const EXACT: Edge = Edge::Exact;
const OVER: Edge = Edge::FirstOver;

/// The complete inventory, in the order it is attempted.
///
/// An unresolved plan reports why, which is measured as a blocked path; a
/// reconciliation or replay past one of its bounds refuses the invocation.
pub(super) const FIXTURES: [Fixture; 16] = [
    plan(
        "first-allocation",
        Planning::FirstAllocation,
        PlanBound::Roomy,
        COMPLETE,
    ),
    plan(
        "catalog-allocation",
        Planning::CatalogAllocation,
        PlanBound::Roomy,
        COMPLETE,
    ),
    plan(
        "catalog-retained",
        Planning::CatalogRetained,
        PlanBound::Roomy,
        COMPLETE,
    ),
    plan(
        "equal-text-copy",
        Planning::EqualTextCopy,
        PlanBound::Roomy,
        COMPLETE,
    ),
    plan(
        "ambiguous-copy",
        Planning::AmbiguousCopy,
        PlanBound::Roomy,
        BLOCKED,
    ),
    plan(
        "complete-deletion",
        Planning::CompleteDeletion,
        PlanBound::Roomy,
        COMPLETE,
    ),
    plan(
        "partial-deletion",
        Planning::PartialDeletion,
        PlanBound::Roomy,
        COMPLETE,
    ),
    plan(
        "candidates-exact",
        Planning::FirstAllocation,
        PlanBound::Candidates(EXACT),
        COMPLETE,
    ),
    plan(
        "candidates-first-over",
        Planning::FirstAllocation,
        PlanBound::Candidates(OVER),
        REFUSED,
    ),
    plan(
        "diagnostics-exact",
        Planning::AmbiguousCopy,
        PlanBound::Diagnostics(EXACT),
        BLOCKED,
    ),
    plan(
        "diagnostics-first-over",
        Planning::AmbiguousCopy,
        PlanBound::Diagnostics(OVER),
        REFUSED,
    ),
    replay("small-chain", Chain::Small, ReplayBound::Roomy, COMPLETE),
    replay(
        "catalog-chain",
        Chain::Catalog,
        ReplayBound::Roomy,
        COMPLETE,
    ),
    replay(
        "missing-update",
        Chain::SmallMissingUpdate,
        ReplayBound::Roomy,
        BLOCKED,
    ),
    replay(
        "history-steps-exact",
        Chain::Small,
        ReplayBound::HistorySteps(EXACT),
        COMPLETE,
    ),
    replay(
        "history-steps-first-over",
        Chain::Small,
        ReplayBound::HistorySteps(OVER),
        REFUSED,
    ),
];

fn owner() -> Result<OwnerIdentity, PreparationFailure> {
    OwnerIdentity::new(OwnerKind::Application, "storefront")
        .map_err(|_| PreparationFailure::Fixture)
}

/// The test context the JS Producer's fixtures resolve against.
fn context() -> Result<TestContext, PreparationFailure> {
    TestContext::builder(
        owner()?,
        SurfaceVocabulary::new(["checkout", "nav"]).map_err(|_| PreparationFailure::Fixture)?,
    )
    .authoring_profile(JsAuthoringProfile::new().identity().clone())
    .usage_profile(JsAuthoringProfile::usage_profile())
    .default_source_locale("en")
    .default_surface_class("checkout")
    .build()
    .map_err(|_| PreparationFailure::Fixture)
}

fn profile() -> Result<JsAuthoringProfile, PreparationFailure> {
    Ok(JsAuthoringProfile::new()
        .with_bindings([
            IntrinsicBinding::new(MODULE, "intent", Intrinsic::Intent),
            IntrinsicBinding::new(MODULE, "mf2", Intrinsic::Mf2),
            IntrinsicBinding::new(MODULE, "noIntent", Intrinsic::NoIntent),
        ])
        .map_err(|_| PreparationFailure::Fixture)?
        .with_dom_globals([DomGlobal::Document]))
}

fn authoring_limits() -> Result<AuthoringLimits, PreparationFailure> {
    AuthoringLimits {
        declarations: 1024,
        message_text_bytes: 64 * 1024,
        emitted_mf2_bytes: 128 * 1024,
        extraction_segments: 8192,
        parameter_names: 64,
        parameter_name_bytes: 256,
        metadata_value_bytes: 4096,
        vocabulary_members: 256,
        projection_nodes: 8192,
        projection_depth: 32,
        diagnostics: 1024,
    }
    .validate()
    .map_err(|_| PreparationFailure::Fixture)
}

fn js_limits() -> Result<JsAuthoringLimits, PreparationFailure> {
    JsAuthoringLimits {
        units: 16,
        unit_bytes: 256 * 1024,
        total_bytes: 1024 * 1024,
        ast_nodes: 1 << 20,
        input_segments: 8192,
        references: 1024,
        exclusions: 1024,
        parameter_bindings: 64,
        alias_chain: 16,
        tracked_origins: 1024,
        proof_steps: 1 << 20,
        annotation_bytes: 4096,
        authoring: authoring_limits()?,
    }
    .validate()
    .map_err(|_| PreparationFailure::Fixture)
}

/// Bounds every roomy case stays well inside.
pub(super) fn roomy() -> IdentityLimits {
    IdentityLimits {
        entries: 4096,
        decisions: 4096,
        lineage_links: 64,
        lineage_members: 256,
        source_edits: 256,
        replacements: 4096,
        replacement_bytes: 1024 * 1024,
        history_steps: 64,
        candidates: 4096,
        diagnostics: 4096,
        targets: 256,
    }
}

/// One revision of one unit: its snapshot and its text.
struct Source {
    snapshot: SourceSnapshot,
    text: String,
}

impl Source {
    fn new(unit: &str, revision: &str, text: String) -> Result<Self, PreparationFailure> {
        Ok(Self {
            snapshot: SourceSnapshot::new(
                owner()?,
                unit,
                revision,
                Grammar::JsModule.identity(),
                text.len() as u64,
                IntegrityDigest::from_hash(digest_bytes(text.as_bytes())).as_str(),
            )
            .map_err(|_| PreparationFailure::Fixture)?,
            text,
        })
    }

    fn member(&self) -> Result<UnitMember, PreparationFailure> {
        UnitMember::new(
            self.snapshot.unit().as_str(),
            self.snapshot.revision().as_str(),
        )
        .map_err(|_| PreparationFailure::Fixture)
    }
}

/// Every source revision the fixtures read.
struct Sources {
    checkout: [Source; 3],
    nav: Source,
    catalog: Source,
}

impl Sources {
    fn new() -> Result<Self, PreparationFailure> {
        // Each line assigns one short literal to a proven DOM sink, which the
        // Producer declares as a UI literal.
        let mut catalog = String::new();
        for item in 0..CATALOG_DECLARATIONS {
            writeln!(
                catalog,
                "document.querySelector('#item-{item}').textContent = 'Item {item}'"
            )
            .map_err(|_| PreparationFailure::Fixture)?;
        }
        Ok(Self {
            checkout: [
                Source::new("checkout", "1", [HEADER, PAY, CANCEL].concat())?,
                Source::new("checkout", "2", [HEADER, PAY, AGAIN, CANCEL].concat())?,
                Source::new("checkout", "3", [HEADER, PAY].concat())?,
            ],
            nav: Source::new("nav", "1", [HEADER, HOME].concat())?,
            catalog: Source::new("catalog", "1", catalog)?,
        })
    }

    /// The edit that inserts the copy, from checkout 1 to checkout 2.
    fn copy(&self) -> Result<SourceEdit, PreparationFailure> {
        let at = (HEADER.len() + PAY.len()) as u64;
        Ok(SourceEdit::new(
            Some(self.checkout[0].snapshot.clone()),
            Some(self.checkout[1].snapshot.clone()),
            vec![Replacement::new(
                ByteRange::new(at, at).map_err(|_| PreparationFailure::Fixture)?,
                AGAIN,
            )],
        ))
    }

    /// The edit that deletes `'Cancel'`, from checkout 1 to checkout 3.
    fn deletion(&self) -> Result<SourceEdit, PreparationFailure> {
        let at = (HEADER.len() + PAY.len()) as u64;
        Ok(SourceEdit::new(
            Some(self.checkout[0].snapshot.clone()),
            Some(self.checkout[2].snapshot.clone()),
            vec![Replacement::new(
                ByteRange::new(at, at + CANCEL.len() as u64)
                    .map_err(|_| PreparationFailure::Fixture)?,
                "",
            )],
        ))
    }
}

/// Read some units through the JS Producer into one admitted inventory.
fn produce(
    context: &TestContext,
    completeness: Completeness,
    read: &[&Source],
    members: &[&Source],
) -> Result<AdmittedInventory, PreparationFailure> {
    let profile = profile()?;
    let limits = js_limits()?;
    let membership = members
        .iter()
        .map(|source| source.member())
        .collect::<Result<Vec<_>, _>>()?;
    let supplied: Vec<SourceUnit<'_>> = read
        .iter()
        .map(|source| SourceUnit::new(source.snapshot.clone(), source.text.as_bytes()))
        .collect();
    let units = admit_units(
        context,
        &profile,
        completeness,
        &membership,
        &supplied,
        &limits,
    )
    .map_err(|_| PreparationFailure::Fixture)?;
    let mut workspace = JsAnalysisWorkspace::new();
    let analyses = units
        .iter()
        .map(|unit| analyze_unit(context, &profile, unit, &limits, &mut workspace, &|| false))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| PreparationFailure::PriorStage)?;
    let assembled = assemble_inventory(
        context,
        &profile,
        SCOPE,
        completeness,
        &membership,
        &analyses,
        &limits,
    )
    .map_err(|_| PreparationFailure::PriorStage)?;
    let artifact = assembled
        .checked_inventory()
        .ok_or(PreparationFailure::PriorStage)?
        .artifact();
    let bytes = serde_json::to_vec(artifact).map_err(|_| PreparationFailure::Fixture)?;
    admit_inventory(
        &bytes,
        context,
        &[],
        &authoring_limits()?,
        &mut AnalysisWorkspace::new(),
    )
    .map_err(|_| PreparationFailure::PriorStage)
}

/// Seal a snapshot and admit its bytes, as a host publishing it would.
fn sealed(snapshot: IntentRegistrySnapshot) -> Result<AdmittedRegistry, PreparationFailure> {
    let artifact = RegistryArtifact::seal(snapshot).map_err(|_| PreparationFailure::Fixture)?;
    let bytes = serde_json::to_vec(&artifact).map_err(|_| PreparationFailure::Fixture)?;
    admit_registry(&bytes, &roomy()).map_err(|_| PreparationFailure::PriorStage)
}

fn genesis(identity: &str) -> Result<AdmittedRegistry, PreparationFailure> {
    sealed(
        IntentRegistrySnapshot::genesis(
            owner()?,
            SCOPE,
            RegistryIdentity::retained(identity).map_err(|_| PreparationFailure::Fixture)?,
        )
        .map_err(|_| PreparationFailure::Fixture)?,
    )
}

/// `count` fixed candidates, distinct within one chain.
fn candidates(first: u64, count: u64) -> Result<Vec<MessageIntentId>, PreparationFailure> {
    (first..first + count)
        .map(|ordinal| {
            let mut bytes = [0xca; 16];
            bytes[8..].copy_from_slice(&ordinal.to_be_bytes());
            MessageIntentId::retained(owner()?, Opaque128::from_bytes(bytes).as_str())
                .map_err(|_| PreparationFailure::Fixture)
        })
        .collect()
}

/// Everything one reconciliation reads besides its base and inventory.
pub(super) struct Evidence {
    pub(super) sources: Vec<(SourceSnapshot, Vec<u8>)>,
    pub(super) edits: Vec<SourceEdit>,
    pub(super) membership: Vec<Token>,
    pub(super) previous: Option<(AdmittedUpdate, AdmittedInventory)>,
    pub(super) links: Vec<LineageLink>,
    pub(super) candidates: Vec<MessageIntentId>,
}

impl Evidence {
    /// Run `with` on the reconciliation inputs this evidence stands for.
    ///
    /// The retained bytes are checked against their snapshots here, which is
    /// input preparation, not reconciliation.
    pub(super) fn inputs<T>(
        &self,
        with: impl FnOnce(&ReconcileInputs<'_>) -> T,
    ) -> Result<T, PreparationFailure> {
        let retained = RetainedSources::new(
            self.sources
                .iter()
                .map(|(snapshot, bytes)| (snapshot.clone(), bytes.as_slice())),
        )
        .map_err(|_| PreparationFailure::Fixture)?;
        Ok(with(&ReconcileInputs {
            evidence: ContinuityInputs {
                sources: &retained,
                edits: &self.edits,
                membership: &self.membership,
                previous: self
                    .previous
                    .as_ref()
                    .map(|(update, inventory)| PreviousUpdate { update, inventory }),
            },
            explicit: &[],
            links: &self.links,
            candidates: &self.candidates,
        }))
    }
}

/// Plan one update with roomy bounds and a fresh workspace, and require a
/// plan: how a base registry for a later fixture is established.
fn planned(
    base: &AdmittedRegistry,
    inventory: &AdmittedInventory,
    evidence: &Evidence,
) -> Result<(AdmittedUpdate, AdmittedRegistry), PreparationFailure> {
    let result = evidence.inputs(|inputs| {
        reconcile(
            base,
            inventory,
            inputs,
            &roomy(),
            &mut IdentityWorkspace::new(),
        )
    })?;
    match result {
        Ok(Reconciliation::Planned(plan)) => {
            let registry = sealed(plan.result().clone())?;
            Ok((plan.update().clone(), registry))
        }
        _ => Err(PreparationFailure::PriorStage),
    }
}

/// The retained bytes of every source revision.
fn retained_sources(sources: &Sources) -> Vec<(SourceSnapshot, Vec<u8>)> {
    sources
        .checkout
        .iter()
        .chain([&sources.nav, &sources.catalog])
        .map(|source| (source.snapshot.clone(), source.text.as_bytes().to_vec()))
        .collect()
}

fn scope(units: &[&str]) -> Result<Vec<Token>, PreparationFailure> {
    units
        .iter()
        .map(|unit| Token::new(unit).map_err(|_| PreparationFailure::Fixture))
        .collect()
}

/// The two chains the fixtures read, established once per preparation.
struct Chains {
    small: [AdmittedRegistry; 3],
    small_updates: [AdmittedUpdate; 2],
    small_inventories: [AdmittedInventory; 2],
    catalog: [AdmittedRegistry; 2],
    catalog_update: AdmittedUpdate,
    catalog_inventory: AdmittedInventory,
}

/// The ID update 2 allocates to the copy.
fn copy_candidate() -> Result<Vec<MessageIntentId>, PreparationFailure> {
    candidates(100, 1)
}

/// Update 2's copy link: the copy is the host's, naming the ID it allocates.
///
/// Candidates go to new declarations in canonical order, checkout before nav
/// and pay first within checkout, so pay holds the first one update 1 used.
fn copy_link() -> Result<Vec<LineageLink>, PreparationFailure> {
    Ok(vec![LineageLink::new(
        LineageKind::Copy,
        candidates(0, 1)?,
        copy_candidate()?,
    )])
}

impl Chains {
    fn new(context: &TestContext, sources: &Sources) -> Result<Self, PreparationFailure> {
        let small_scope = scope(&["checkout", "nav"])?;
        let first_inventory = produce(
            context,
            Completeness::Complete,
            &[&sources.checkout[0], &sources.nav],
            &[&sources.checkout[0], &sources.nav],
        )?;
        let small_genesis = genesis(SMALL_CHAIN)?;
        let (first_update, first) = planned(
            &small_genesis,
            &first_inventory,
            &Evidence {
                sources: retained_sources(sources),
                edits: vec![],
                membership: small_scope.clone(),
                previous: None,
                links: vec![],
                candidates: candidates(0, 3)?,
            },
        )?;
        let copy_inventory = produce(
            context,
            Completeness::Complete,
            &[&sources.checkout[1], &sources.nav],
            &[&sources.checkout[1], &sources.nav],
        )?;
        let (second_update, second) = planned(
            &first,
            &copy_inventory,
            &Evidence {
                sources: retained_sources(sources),
                edits: vec![sources.copy()?],
                membership: small_scope,
                previous: Some((first_update.clone(), first_inventory.clone())),
                links: copy_link()?,
                candidates: copy_candidate()?,
            },
        )?;

        let catalog_inventory = produce(
            context,
            Completeness::Complete,
            &[&sources.catalog],
            &[&sources.catalog],
        )?;
        let catalog_genesis = genesis(CATALOG_CHAIN)?;
        let (catalog_update, catalog) = planned(
            &catalog_genesis,
            &catalog_inventory,
            &Evidence {
                sources: retained_sources(sources),
                edits: vec![],
                membership: scope(&["catalog"])?,
                previous: None,
                links: vec![],
                candidates: candidates(1000, CATALOG_DECLARATIONS)?,
            },
        )?;
        Ok(Self {
            small: [small_genesis, first, second],
            small_updates: [first_update, second_update],
            small_inventories: [first_inventory, copy_inventory],
            catalog: [catalog_genesis, catalog],
            catalog_update,
            catalog_inventory,
        })
    }
}

/// A bound and the value it was set to, as the case projection names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SetBound {
    pub(super) limit: Limit,
    pub(super) edge: Edge,
    pub(super) value: u64,
}

/// Set a bound at a fixture's own count, or one below it.
///
/// A count of zero has nothing below it, so it is no first-over fixture.
fn set(limit: Limit, edge: Edge, own: u64) -> Result<SetBound, PreparationFailure> {
    let value = match edge {
        Edge::Exact => own,
        Edge::FirstOver => own.checked_sub(1).ok_or(PreparationFailure::Fixture)?,
    };
    Ok(SetBound { limit, edge, value })
}

/// One planning fixture, prepared.
pub(super) struct PlanCase {
    pub(super) base: AdmittedRegistry,
    pub(super) inventory: AdmittedInventory,
    pub(super) evidence: Evidence,
    pub(super) limits: IdentityLimits,
    /// The scratch every measured invocation of this case reuses.
    pub(super) workspace: RefCell<IdentityWorkspace>,
}

/// One replay fixture, prepared.
pub(super) struct ReplayCase {
    pub(super) head: AdmittedRegistry,
    pub(super) genesis: AdmittedRegistry,
    pub(super) registries: Vec<AdmittedRegistry>,
    pub(super) updates: Vec<AdmittedUpdate>,
    pub(super) inventories: Vec<AdmittedInventory>,
    pub(super) limits: IdentityLimits,
}

impl ReplayCase {
    /// How many retained artifacts of each kind the case holds.
    pub(super) fn retained(&self) -> Retained {
        Retained {
            registries: self.registries.len() as u64,
            updates: self.updates.len() as u64,
            inventories: self.inventories.len() as u64,
        }
    }

    /// Index the retained history, as a host does before it replays one.
    pub(super) fn history(&self) -> Result<RetainedHistory<'_>, PreparationFailure> {
        RetainedHistory::new(&self.registries, &self.updates, &self.inventories)
            .map_err(|_| PreparationFailure::Fixture)
    }

    /// Accept the chain's genesis as its anchor, as a host does.
    pub(super) fn anchor(&self) -> Result<Anchor<'_>, PreparationFailure> {
        Anchor::genesis(&self.genesis).map_err(|_| PreparationFailure::Fixture)
    }
}

/// What a prepared fixture reads.
#[allow(
    clippy::large_enum_variant,
    reason = "one value per case, prepared once before any capture"
)]
pub(super) enum Reads {
    Plan(PlanCase),
    Replay(ReplayCase),
}

/// Everything one case needs, prepared before any interval opens.
pub struct Prepared {
    pub(super) fixture: Fixture,
    pub(super) reads: Reads,
    pub(super) bound: Option<SetBound>,
    /// What this case is expected to produce, established outside measurement.
    expected: Observed,
}

impl Prepared {
    /// Borrow what this case is expected to produce.
    ///
    /// Every measured invocation is compared against this, so it is
    /// established by one unmeasured invocation, on a fresh workspace, rather
    /// than by a sample.
    pub(super) const fn expected(&self) -> &Observed {
        &self.expected
    }
}

/// Plan once, outside measurement, on the given workspace.
pub(super) fn observe_plan(
    case: &PlanCase,
    workspace: &RefCell<IdentityWorkspace>,
) -> Result<Observed, PreparationFailure> {
    case.evidence.inputs(|inputs| {
        let input = (&case.base, &case.inventory, inputs, &case.limits, workspace);
        operation::observe_plan(&operation::invoke_plan(input), input)
    })
}

/// Replay once, outside measurement.
pub(super) fn observe_replay(case: &ReplayCase) -> Result<Observed, PreparationFailure> {
    let retained = case.history()?;
    let anchor = case.anchor()?;
    let input = (
        &case.head,
        &anchor,
        &retained,
        &case.limits,
        case.retained(),
    );
    Ok(operation::observe_replay(
        &operation::invoke_replay(input),
        input,
    ))
}

/// How many diagnostics a case reports when every bound is roomy.
fn reported(case: &PlanCase) -> Result<u64, PreparationFailure> {
    let result = case.evidence.inputs(|inputs| {
        reconcile(
            &case.base,
            &case.inventory,
            inputs,
            &roomy(),
            &mut IdentityWorkspace::new(),
        )
    })?;
    match result {
        Ok(Reconciliation::Unresolved(report)) => Ok(report.diagnostics().len() as u64),
        _ => Err(PreparationFailure::Fixture),
    }
}

/// How many updates a case's chain replays when every bound is roomy.
fn steps(case: &ReplayCase) -> Result<u64, PreparationFailure> {
    match verify_history(&case.head, &case.anchor()?, &case.history()?, &roomy()) {
        Ok(HistoryOutcome::Verified { steps }) => Ok(steps),
        _ => Err(PreparationFailure::Fixture),
    }
}

fn prepare_plan(
    context: &TestContext,
    sources: &Sources,
    chains: &Chains,
    planning: Planning,
    bound: PlanBound,
) -> Result<(PlanCase, Option<SetBound>), PreparationFailure> {
    let small_scope = scope(&["checkout", "nav"])?;
    let previous = Some((
        chains.small_updates[0].clone(),
        chains.small_inventories[0].clone(),
    ));
    let evidence = |edits, membership, previous, links, candidates| Evidence {
        sources: retained_sources(sources),
        edits,
        membership,
        previous,
        links,
        candidates,
    };
    let (base, inventory, evidence) = match planning {
        Planning::FirstAllocation => (
            chains.small[0].clone(),
            chains.small_inventories[0].clone(),
            evidence(vec![], small_scope, None, vec![], candidates(0, 3)?),
        ),
        Planning::CatalogAllocation => (
            chains.catalog[0].clone(),
            chains.catalog_inventory.clone(),
            evidence(
                vec![],
                scope(&["catalog"])?,
                None,
                vec![],
                candidates(1000, CATALOG_DECLARATIONS)?,
            ),
        ),
        Planning::CatalogRetained => (
            chains.catalog[1].clone(),
            chains.catalog_inventory.clone(),
            evidence(
                vec![],
                scope(&["catalog"])?,
                Some((
                    chains.catalog_update.clone(),
                    chains.catalog_inventory.clone(),
                )),
                vec![],
                vec![],
            ),
        ),
        Planning::EqualTextCopy => (
            chains.small[1].clone(),
            chains.small_inventories[1].clone(),
            evidence(
                vec![sources.copy()?],
                small_scope,
                previous,
                copy_link()?,
                copy_candidate()?,
            ),
        ),
        Planning::AmbiguousCopy => (
            chains.small[1].clone(),
            chains.small_inventories[1].clone(),
            evidence(vec![], small_scope, previous, vec![], copy_candidate()?),
        ),
        Planning::CompleteDeletion | Planning::PartialDeletion => {
            let complete = planning == Planning::CompleteDeletion;
            let read = [&sources.checkout[2]];
            let all = [&sources.checkout[2], &sources.nav];
            let inventory = if complete {
                produce(context, Completeness::Complete, &all, &all)?
            } else {
                produce(context, Completeness::Partial, &read, &all)?
            };
            (
                chains.small[1].clone(),
                inventory,
                evidence(
                    vec![sources.deletion()?],
                    small_scope,
                    previous,
                    vec![],
                    vec![],
                ),
            )
        }
    };
    let mut case = PlanCase {
        base,
        inventory,
        evidence,
        limits: roomy(),
        workspace: RefCell::new(IdentityWorkspace::new()),
    };
    let set_bound = match bound {
        PlanBound::Roomy => None,
        PlanBound::Candidates(edge) => {
            let offered = case.evidence.candidates.len() as u64;
            let set_bound = set(Limit::Candidates, edge, offered)?;
            case.limits.candidates = set_bound.value;
            Some(set_bound)
        }
        PlanBound::Diagnostics(edge) => {
            let set_bound = set(Limit::Diagnostics, edge, reported(&case)?)?;
            case.limits.diagnostics = set_bound.value;
            Some(set_bound)
        }
    };
    Ok((case, set_bound))
}

fn prepare_replay(
    chains: &Chains,
    chain: Chain,
    bound: ReplayBound,
) -> Result<(ReplayCase, Option<SetBound>), PreparationFailure> {
    let mut case = match chain {
        Chain::Small | Chain::SmallMissingUpdate => ReplayCase {
            head: chains.small[2].clone(),
            genesis: chains.small[0].clone(),
            registries: vec![chains.small[1].clone()],
            updates: if chain == Chain::Small {
                chains.small_updates.to_vec()
            } else {
                vec![chains.small_updates[1].clone()]
            },
            inventories: chains.small_inventories.to_vec(),
            limits: roomy(),
        },
        Chain::Catalog => ReplayCase {
            head: chains.catalog[1].clone(),
            genesis: chains.catalog[0].clone(),
            registries: vec![],
            updates: vec![chains.catalog_update.clone()],
            inventories: vec![chains.catalog_inventory.clone()],
            limits: roomy(),
        },
    };
    let set_bound = match bound {
        ReplayBound::Roomy => None,
        ReplayBound::HistorySteps(edge) => {
            let set_bound = set(Limit::HistorySteps, edge, steps(&case)?)?;
            case.limits.history_steps = set_bound.value;
            Some(set_bound)
        }
    };
    Ok((case, set_bound))
}

/// Prepare one case completely, before any measurement begins.
pub(super) fn prepare(fixture: Fixture) -> Result<Prepared, PreparationFailure> {
    let context = context()?;
    let sources = Sources::new()?;
    let chains = Chains::new(&context, &sources)?;
    let (reads, bound) = match fixture.input {
        Input::Plan { planning, bound } => {
            let (case, bound) = prepare_plan(&context, &sources, &chains, planning, bound)?;
            (Reads::Plan(case), bound)
        }
        Input::Replay { chain, bound } => {
            let (case, bound) = prepare_replay(&chains, chain, bound)?;
            (Reads::Replay(case), bound)
        }
    };
    // The expectation is established here, by one invocation that is not
    // measured and never becomes a sample. A plan is made on a fresh
    // workspace, so every measured invocation, which reuses the case's own
    // workspace, is compared with a fresh one.
    let expected = match &reads {
        Reads::Plan(case) => observe_plan(case, &RefCell::new(IdentityWorkspace::new()))?,
        Reads::Replay(case) => observe_replay(case)?,
    };
    Ok(Prepared {
        fixture,
        reads,
        bound,
        expected,
    })
}

/// Prepared fixtures, for this module's tests and its siblings'.
#[cfg(test)]
pub(super) mod testing {
    use super::*;
    use crate::{HistoryFailure, ReconcileFailure};

    /// Prepare the fixture of this name.
    pub(in crate::benchmark) fn prepared(name: &str) -> Prepared {
        let fixture = FIXTURES
            .into_iter()
            .find(|fixture| fixture.name == name)
            .unwrap_or_else(|| panic!("{name} is a fixture"));
        prepare(fixture).unwrap()
    }

    pub(in crate::benchmark) fn plan_case(prepared: &Prepared) -> &PlanCase {
        let Reads::Plan(case) = &prepared.reads else {
            panic!("{} plans", prepared.fixture.name);
        };
        case
    }

    pub(in crate::benchmark) fn replay_case(prepared: &Prepared) -> &ReplayCase {
        let Reads::Replay(case) = &prepared.reads else {
            panic!("{} replays", prepared.fixture.name);
        };
        case
    }

    /// Plan a prepared case again under its own bounds, on a fresh workspace.
    pub(in crate::benchmark) fn reconciled(
        prepared: &Prepared,
    ) -> Result<Reconciliation, ReconcileFailure> {
        let case = plan_case(prepared);
        case.evidence
            .inputs(|inputs| {
                reconcile(
                    &case.base,
                    &case.inventory,
                    inputs,
                    &case.limits,
                    &mut IdentityWorkspace::new(),
                )
            })
            .unwrap()
    }

    /// Replay a prepared case again under its own bounds.
    pub(in crate::benchmark) fn replayed(
        prepared: &Prepared,
    ) -> Result<HistoryOutcome, HistoryFailure> {
        let case = replay_case(prepared);
        verify_history(
            &case.head,
            &case.anchor().unwrap(),
            &case.history().unwrap(),
            &case.limits,
        )
    }

    /// The fixed candidates the chains are given, from `first` on.
    pub(in crate::benchmark) fn candidates(first: u64, count: u64) -> Vec<MessageIntentId> {
        super::candidates(first, count).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use intlify_authoring::OccurrenceRole;
    use intlify_shared_json::quantity::Quantity;

    use super::testing::{plan_case, prepared, reconciled, replay_case};
    use super::*;
    use crate::benchmark::operation::{Count, LogicalWork};
    use crate::{DeclarationClass, IdentityDecision, ReconcileFailure};

    /// What the ambiguous copy reports: its three unresolved declarations
    /// and the two entries it leaves unsettled.
    const DIAGNOSTICS: u64 = 5;

    #[test]
    fn every_operation_is_covered_and_no_fixture_name_repeats() {
        for operation in Operation::ALL {
            assert!(
                FIXTURES
                    .iter()
                    .any(|fixture| fixture.operation == operation),
                "{operation:?} has no fixture"
            );
        }
        let mut names = FIXTURES.map(|fixture| fixture.name).to_vec();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), FIXTURES.len());
        // Every path is exercised, the refusing one included, by both
        // operations.
        for operation in Operation::ALL {
            for path in [COMPLETE, BLOCKED, REFUSED] {
                assert!(
                    FIXTURES
                        .iter()
                        .any(|fixture| fixture.operation == operation && fixture.expected == path),
                    "{operation:?} never takes {path:?}"
                );
            }
        }
    }

    #[test]
    fn every_fixture_takes_the_path_it_declares() {
        // A fixture that quietly moves onto another path measures that path
        // instead of the work it names, and its samples still agree with each
        // other. Only its declared expectation catches that.
        for fixture in FIXTURES {
            let prepared = prepare(fixture).unwrap();
            assert_eq!(
                prepared.expected().path,
                fixture.expected,
                "{} took another path",
                fixture.name
            );
        }
    }

    /// One fixture's expected work under `names`, as a row of counts: a
    /// number is exact, `?` is unavailable and `-` is not applicable, and `|`
    /// only separates groups.
    fn row(names: &[&'static str], counts: &str) -> LogicalWork {
        let counts: Vec<Count> = counts
            .split_whitespace()
            .filter(|count| *count != "|")
            .map(|count| match count {
                "?" => Count::Unavailable {},
                "-" => Count::NotApplicable {},
                exact => Count::Exact {
                    value: Quantity::new(exact.parse().unwrap()),
                },
            })
            .collect();
        assert_eq!(counts.len(), names.len(), "{counts:?}");
        LogicalWork::new(names.iter().copied().zip(counts))
    }

    #[test]
    fn every_fixture_reports_the_work_it_did_and_how_each_count_is_known() {
        const PLANNING: [&str; 24] = [
            // What the case was given.
            "base_entries",
            "units",
            "declarations",
            "edits",
            "explicit_decisions",
            "lineage_links",
            "candidates",
            // What this revision of the harness does not count.
            "candidate_comparisons",
            // What each current declaration was classified as.
            "retained",
            "continued",
            "explicit",
            "new",
            "unresolved",
            "conflicting",
            // What each gone entry was classified as.
            "carried_entries",
            "absent_entries",
            "kept_entries",
            "unsettled_entries",
            // What a planned update decides.
            "decisions",
            "allocations",
            "continuations",
            "retirements",
            "restorations",
            // What an unresolved plan reports.
            "diagnostics",
        ];
        const REPLAY: [&str; 5] = [
            "head_entries",
            "retained_registries",
            "retained_updates",
            "retained_inventories",
            "replayed_steps",
        ];
        // Nothing classified when nothing changed, nothing planned when the
        // plan is unresolved, and nothing after the inputs when refused.
        let planning = [
            (
                "first-allocation",
                "0 2 3 0 0 0 3 | ? | 0 0 0 3 0 0 | 0 0 0 0 | 3 3 0 0 0 | 0",
            ),
            (
                "catalog-allocation",
                "0 1 128 0 0 0 128 | ? | 0 0 0 128 0 0 | 0 0 0 0 | 128 128 0 0 0 | 0",
            ),
            (
                "catalog-retained",
                "128 1 128 0 0 0 0 | ? | ? ? ? ? ? ? | ? ? ? ? | - - - - - | -",
            ),
            (
                "equal-text-copy",
                "3 2 4 1 0 1 1 | ? | 1 2 0 1 0 0 | 2 0 0 0 | 3 1 2 0 0 | 0",
            ),
            (
                "ambiguous-copy",
                "3 2 4 0 0 0 1 | ? | 1 0 0 0 3 0 | 0 0 0 2 | - - - - - | 5",
            ),
            (
                "complete-deletion",
                "3 2 2 1 0 0 0 | ? | 1 1 0 0 0 0 | 1 1 0 0 | 2 0 1 1 0 | 0",
            ),
            // Cancel is kept rather than retired, and so is home, whose unit
            // the view does not read.
            (
                "partial-deletion",
                "3 1 1 1 0 0 0 | ? | 0 1 0 0 0 0 | 1 0 2 0 | 1 0 1 0 0 | 0",
            ),
            (
                "candidates-exact",
                "0 2 3 0 0 0 3 | ? | 0 0 0 3 0 0 | 0 0 0 0 | 3 3 0 0 0 | 0",
            ),
            (
                "candidates-first-over",
                "0 2 3 0 0 0 3 | ? | ? ? ? ? ? ? | ? ? ? ? | ? ? ? ? ? | ?",
            ),
            (
                "diagnostics-exact",
                "3 2 4 0 0 0 1 | ? | 1 0 0 0 3 0 | 0 0 0 2 | - - - - - | 5",
            ),
            (
                "diagnostics-first-over",
                "3 2 4 0 0 0 1 | ? | ? ? ? ? ? ? | ? ? ? ? | ? ? ? ? ? | ?",
            ),
        ];
        // A replay counts the steps it replayed, none when blocked, and does
        // not say how far a refused one got.
        let replay = [
            ("small-chain", "4 1 2 2 | 2"),
            ("catalog-chain", "128 0 1 1 | 1"),
            ("missing-update", "4 1 1 2 | 0"),
            ("history-steps-exact", "4 1 2 2 | 2"),
            ("history-steps-first-over", "4 1 2 2 | ?"),
        ];
        let rows = planning
            .map(|(name, counts)| (name, row(&PLANNING, counts)))
            .into_iter()
            .chain(replay.map(|(name, counts)| (name, row(&REPLAY, counts))))
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), FIXTURES.len());
        for (name, work) in rows {
            assert_eq!(prepared(name).expected().work, work, "{name}");
        }
    }

    #[test]
    fn each_bound_is_set_at_the_fixture_count_or_one_below_it() {
        let bound_of = |name: &str| prepared(name).bound.unwrap();
        for (exact, over, limit) in [
            (
                "candidates-exact",
                "candidates-first-over",
                Limit::Candidates,
            ),
            (
                "diagnostics-exact",
                "diagnostics-first-over",
                Limit::Diagnostics,
            ),
            (
                "history-steps-exact",
                "history-steps-first-over",
                Limit::HistorySteps,
            ),
        ] {
            let (exact, over) = (bound_of(exact), bound_of(over));
            assert_eq!((exact.limit, over.limit), (limit, limit));
            assert_eq!((exact.edge, over.edge), (Edge::Exact, Edge::FirstOver));
            assert_eq!(over.value + 1, exact.value);
        }
        // The counts the fixtures sit at: three candidates for three new
        // declarations, what the ambiguous copy reports, and the two steps of
        // the small chain.
        assert_eq!(bound_of("candidates-exact").value, 3);
        assert_eq!(bound_of("diagnostics-exact").value, DIAGNOSTICS);
        assert_eq!(bound_of("history-steps-exact").value, 2);
        // Each fixture sets its own bound and no other; a roomy one sets none.
        let planned = |name: &str| plan_case(&prepared(name)).limits;
        assert_eq!(
            planned("candidates-first-over"),
            IdentityLimits {
                candidates: 2,
                ..roomy()
            }
        );
        assert_eq!(
            planned("diagnostics-first-over"),
            IdentityLimits {
                diagnostics: DIAGNOSTICS - 1,
                ..roomy()
            }
        );
        assert_eq!(
            replay_case(&prepared("history-steps-first-over")).limits,
            IdentityLimits {
                history_steps: 1,
                ..roomy()
            }
        );
        assert_eq!(prepared("first-allocation").bound, None);
        assert_eq!(planned("first-allocation"), roomy());
        // Zero has nothing below it.
        assert_eq!(
            set(Limit::Candidates, Edge::FirstOver, 0),
            Err(PreparationFailure::Fixture)
        );
        assert_eq!(
            set(Limit::Candidates, Edge::Exact, 0).map(|bound| bound.value),
            Ok(0)
        );
    }

    #[test]
    fn a_count_only_another_path_reports_is_no_fixture() {
        // A plan reports no diagnostics, and a blocked chain replays no steps.
        assert_eq!(
            reported(plan_case(&prepared("first-allocation"))),
            Err(PreparationFailure::Fixture)
        );
        assert_eq!(
            steps(replay_case(&prepared("missing-update"))),
            Err(PreparationFailure::Fixture)
        );
    }

    #[test]
    fn a_reused_workspace_agrees_with_a_fresh_one_after_success_failure_and_cancellation() {
        let catalog = prepared("catalog-allocation");
        let refused = prepared("candidates-first-over");
        let dirty = |previous: &str| -> RefCell<IdentityWorkspace> {
            let workspace = RefCell::new(IdentityWorkspace::new());
            match previous {
                "success" => {
                    observe_plan(plan_case(&catalog), &workspace).unwrap();
                    // The plan was made in the workspace it was lent.
                    assert_ne!(workspace.borrow().capacities().classes, 0);
                }
                "failure" => {
                    let observed = observe_plan(plan_case(&refused), &workspace).unwrap();
                    assert_eq!(observed.path, REFUSED);
                }
                _ => {
                    let case = plan_case(&catalog);
                    let stopped = case
                        .evidence
                        .inputs(|inputs| {
                            crate::reconcile_with_cancellation(
                                &case.base,
                                &case.inventory,
                                inputs,
                                &case.limits,
                                &mut workspace.borrow_mut(),
                                &|| true,
                            )
                        })
                        .unwrap();
                    assert_eq!(stopped, Err(ReconcileFailure::Cancelled));
                }
            }
            workspace
        };
        for fixture in FIXTURES {
            let Input::Plan { .. } = fixture.input else {
                continue;
            };
            let prepared = prepare(fixture).unwrap();
            for previous in ["success", "failure", "cancellation"] {
                let workspace = dirty(previous);
                assert_eq!(
                    &observe_plan(plan_case(&prepared), &workspace).unwrap(),
                    prepared.expected(),
                    "{} after a {previous}",
                    fixture.name
                );
            }
        }
    }

    #[test]
    fn equal_text_keeps_one_identity_and_gives_the_copy_its_own() {
        let Ok(Reconciliation::Planned(plan)) = reconciled(&prepared("equal-text-copy")) else {
            panic!("the copy is planned");
        };
        let classes: Vec<&DeclarationClass> = plan
            .classification()
            .declarations()
            .iter()
            .map(|(_, class)| class)
            .collect();
        // Pay continues across the edit, the copy of its text is new, cancel
        // moves with it, and home stays.
        assert!(matches!(classes[0], DeclarationClass::Continued(_)));
        assert!(matches!(classes[1], DeclarationClass::New));
        assert!(matches!(classes[2], DeclarationClass::Continued(_)));
        assert!(matches!(classes[3], DeclarationClass::Retained(_)));
        let allocated: Vec<&MessageIntentId> = plan
            .update()
            .update()
            .decisions()
            .iter()
            .filter(|decision| matches!(decision, IdentityDecision::Allocate(_)))
            .map(IdentityDecision::intent_id)
            .collect();
        assert_eq!(allocated, [&copy_candidate().unwrap()[0]]);
        let DeclarationClass::Continued(pay) = classes[0] else {
            unreachable!()
        };
        assert_ne!(pay, allocated[0]);
        // The host's link records which identity the copy was made from.
        let [link] = plan.update().update().lineage_links() else {
            panic!("the copy is linked");
        };
        assert_eq!(link.kind(), LineageKind::Copy);
        assert_eq!(link.predecessors(), std::slice::from_ref(pay));
        assert_eq!(link.successors(), std::slice::from_ref(allocated[0]));
        // Without the edit, nothing shows which of the two equal texts is
        // the old one.
        assert!(matches!(
            reconciled(&prepared("ambiguous-copy")),
            Ok(Reconciliation::Unresolved(_))
        ));
    }

    #[test]
    fn the_catalog_is_many_short_ui_literals() {
        let catalog = prepared("catalog-allocation");
        let declarations = plan_case(&catalog).inventory.inventory().declarations();
        assert_eq!(declarations.len() as u64, CATALOG_DECLARATIONS);
        assert!(declarations
            .iter()
            .all(|facts| facts.occurrence().role() == OccurrenceRole::UiLiteral));
        assert_eq!(declarations[0].mf2_source(), "{{Item 0}}");
    }

    #[test]
    fn a_complete_view_retires_what_a_partial_view_keeps() {
        let decisions = |name: &str| {
            let Ok(Reconciliation::Planned(plan)) = reconciled(&prepared(name)) else {
                panic!("{name} is planned");
            };
            plan.update().update().decisions().to_vec()
        };
        let retirements = |name: &str| {
            decisions(name)
                .iter()
                .filter(|decision| matches!(decision, IdentityDecision::Retire(_)))
                .count()
        };
        assert_eq!(retirements("complete-deletion"), 1);
        assert_eq!(retirements("partial-deletion"), 0);
        let partial = prepared("partial-deletion");
        assert_eq!(
            plan_case(&partial).inventory.inventory().completeness(),
            Completeness::Partial
        );
        // Home is in a unit the partial view does not read.
        assert_eq!(plan_case(&partial).inventory.inventory().units().len(), 1);
    }
}
