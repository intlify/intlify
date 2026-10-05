// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The fixed fixtures this owner measures against.
//!
//! Every fixture, its expectation, and the order they run in are fixed before
//! any capture begins. Nothing here is generated from a duration, a machine, or
//! an environment variable. A bound a fixture is measured at is taken from the
//! fixture itself — its own node count, proof steps, alias links, annotation,
//! parameters, bytes or units — so the edge it sits on is exact by
//! construction rather than a guessed number.

use std::cell::RefCell;
use std::fmt::Write as _;

use intlify_authoring::test_context::TestContext;
use intlify_authoring::{
    AuthoringLimits, Completeness, IntegrityDigest, OwnerIdentity, OwnerKind, SourceSnapshot,
    SurfaceVocabulary,
};
use intlify_measurement::owner_run::run::{Expected, PreparationFailure};
use intlify_measurement::owner_run::Case;
use intlify_shared_json::encoding::digest_bytes;

use super::operation::{self, Observed, Operation};
use crate::analysis::{Classification, UnitAnalysis, UnitWork};
use crate::{
    admit_units, analyze_unit, AdmittedUnit, DomGlobal, Grammar, Intrinsic, IntrinsicBinding,
    JsAnalysisWorkspace, JsAuthoringLimits, JsAuthoringProfile, JsLimitKind, SourceUnit,
    UnitMember,
};

/// The module the fixtures register their intrinsics under.
///
/// It names no published package. Design 028's application imports it, so the
/// representative module is read byte for byte.
const MODULE: &str = "fixture-authoring";

macro_rules! prelude {
    () => {
        "import { intent, mf2, noIntent } from 'fixture-authoring'\n"
    };
}

macro_rules! note {
    () => {
        r#"/* @intlify { "description": "Pay" } */"#
    };
}

/// Design 028's representative application, byte for byte.
const REPRESENTATIVE: &str = include_str!("../../fixtures/phase2/representative-application.js");

const NO_CANDIDATES: &str = "export function total(items) {\n  let sum = 0\n  for (const item of items) {\n    sum += item.price * item.quantity\n  }\n  return sum\n}\n";

const EQUAL_TEXT: &str = concat!(
    prelude!(),
    "export const pay = intent('Pay now')\nexport const confirm = intent('Pay now')\n"
);

const MF2_COMPLEX: &str = concat!(
    prelude!(),
    "const summary = mf2`.input {$count :number}\n",
    ".local $shown = {$count :number minimumFractionDigits=0}\n",
    ".match $count\n",
    "one {{You have {$shown} item}}\n",
    "* {{You have {$shown} items}}`\n",
    "export function render(count) {\n  return intent(summary, { count })\n}\n"
);

const CONDITIONAL: &str = concat!(
    prelude!(),
    "const loading = mf2`Loading`\nconst done = mf2`Done`\n",
    "export function status(pending) {\n  return intent(pending ? loading : done)\n}\n"
);

const RECEIVER_ESCAPE: &str = "export function render() {\n  const pay = document.querySelector('#pay')\n  customize(pay)\n  pay.textContent = 'Pay now'\n}\n";

const HOST_SYNTAX_INVALID: &str = "const = 1\n";

const SMALL: &str = concat!(prelude!(), "export const pay = intent('Pay now')\n");

const PROOF: &str = "export function render(ready) {\n  const pay = document.querySelector('#pay')\n  while (ready()) { log(pay.id) }\n  pay.textContent = 'Pay now'\n}\n";

const ANNOTATED: &str = concat!(
    prelude!(),
    note!(),
    "\nexport const pay = intent('Pay now')\n"
);

/// What one unit fixture's bytes are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Source {
    /// Fixed text.
    Text(&'static str),
    /// One short literal assigned to each of this many proven sinks.
    UiLiterals(u32),
    /// One shared declaration, used this many times.
    References(u32),
    /// One use site supplying this many parameters.
    Parameters(u32),
    /// This many `const` aliases of one DOM origin, the last one assigned to.
    AliasChain(u32),
    /// Design 028's representative application.
    Representative,
}

impl Source {
    fn text(self) -> String {
        match self {
            Self::Text(text) => text.to_owned(),
            Self::UiLiterals(count) => {
                let mut text = String::new();
                for index in 0..count {
                    writeln!(
                        text,
                        "document.querySelector('#item-{index}').textContent = 'Item {index}'"
                    )
                    .expect("writing to a string");
                }
                text
            }
            Self::References(count) => {
                let mut text = String::from(prelude!());
                text.push_str("const greeting = mf2`Hello {$name}!`\n");
                text.push_str("export function render(name) {\n");
                for _ in 0..count {
                    text.push_str("  show(intent(greeting, { name }))\n");
                }
                text.push_str("}\n");
                text
            }
            Self::Parameters(count) => {
                let names = (0..count)
                    .map(|index| format!("p{index}"))
                    .collect::<Vec<_>>();
                let message = names
                    .iter()
                    .map(|name| format!("{{${name}}}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                format!(
                    "{}export function render({names}) {{\n  return intent('{message}', {{ {names} }})\n}}\n",
                    prelude!(),
                    names = names.join(", "),
                )
            }
            Self::AliasChain(length) => {
                let mut text = String::from("const a0 = document.createElement('p')\n");
                for index in 1..=length {
                    writeln!(text, "const a{index} = a{}", index - 1).expect("writing to a string");
                }
                writeln!(text, "a{length}.textContent = 'Deep'").expect("writing to a string");
                text
            }
            Self::Representative => REPRESENTATIVE.to_owned(),
        }
    }
}

/// Which profile a unit is read under.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Profile {
    /// The three fixture intrinsics, and no DOM global.
    Explicit,
    /// The three fixture intrinsics and the standard `document`.
    Dom,
}

impl Profile {
    fn build(self) -> Result<JsAuthoringProfile, PreparationFailure> {
        let explicit = JsAuthoringProfile::new()
            .with_bindings([
                IntrinsicBinding::new(MODULE, "intent", Intrinsic::Intent),
                IntrinsicBinding::new(MODULE, "mf2", Intrinsic::Mf2),
                IntrinsicBinding::new(MODULE, "noIntent", Intrinsic::NoIntent),
            ])
            .map_err(|_| PreparationFailure::Fixture)?;
        Ok(match self {
            Self::Explicit => explicit,
            Self::Dom => explicit.with_dom_globals([DomGlobal::Document]),
        })
    }
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

/// The bound a fixture is measured at, when it is measured at one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Bound {
    /// Every bound has room to spare.
    Roomy,
    /// One bound sits at the fixture's own count, or one below it.
    At(JsLimitKind, Edge),
}

/// What one fixture reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Input {
    /// One unit, read by source discovery.
    Unit {
        source: Source,
        profile: Profile,
        bound: Bound,
    },
    /// The fixed unit set, assembled into one inventory.
    Inventory {
        completeness: Completeness,
        bound: Bound,
    },
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

const fn unit(
    name: &'static str,
    source: Source,
    profile: Profile,
    bound: Bound,
    expected: Expected,
) -> Fixture {
    Fixture {
        operation: Operation::ParseAndClassify,
        name,
        input: Input::Unit {
            source,
            profile,
            bound,
        },
        expected,
    }
}

const fn inventory(
    name: &'static str,
    completeness: Completeness,
    bound: Bound,
    expected: Expected,
) -> Fixture {
    Fixture {
        operation: Operation::InventoryAssembly,
        name,
        input: Input::Inventory {
            completeness,
            bound,
        },
        expected,
    }
}

const COMPLETE: Expected = Expected::Complete;
const BLOCKED: Expected = Expected::Blocked;
const REFUSED: Expected = Expected::OperationalFailure;
const ROOMY: Bound = Bound::Roomy;
const EXACT: Edge = Edge::Exact;
const OVER: Edge = Edge::FirstOver;

/// The complete inventory, in the order it is attempted.
///
/// A bound on one declaration or one use site blocks it with a diagnostic;
/// a bound on a unit or on the invocation refuses the invocation. The
/// first-over side of each bound takes the path its scope gives it. A unit
/// past its byte bound is refused at admission, before either measured
/// interval, so only its exact side is measured here.
pub(super) const FIXTURES: [Fixture; 30] = [
    unit(
        "no-candidates",
        Source::Text(NO_CANDIDATES),
        Profile::Dom,
        ROOMY,
        COMPLETE,
    ),
    unit(
        "ui-literals-1",
        Source::UiLiterals(1),
        Profile::Dom,
        ROOMY,
        COMPLETE,
    ),
    unit(
        "ui-literals-64",
        Source::UiLiterals(64),
        Profile::Dom,
        ROOMY,
        COMPLETE,
    ),
    unit(
        "ui-literals-1024",
        Source::UiLiterals(1024),
        Profile::Dom,
        ROOMY,
        COMPLETE,
    ),
    unit(
        "equal-text-distinct-declarations",
        Source::Text(EQUAL_TEXT),
        Profile::Explicit,
        ROOMY,
        COMPLETE,
    ),
    unit(
        "mf2-complex",
        Source::Text(MF2_COMPLEX),
        Profile::Explicit,
        ROOMY,
        COMPLETE,
    ),
    unit(
        "parameters-sparse",
        Source::Parameters(1),
        Profile::Explicit,
        ROOMY,
        COMPLETE,
    ),
    unit(
        "parameters-dense",
        Source::Parameters(32),
        Profile::Explicit,
        ROOMY,
        COMPLETE,
    ),
    unit(
        "references-to-one-declaration-1",
        Source::References(1),
        Profile::Explicit,
        ROOMY,
        COMPLETE,
    ),
    unit(
        "references-to-one-declaration-64",
        Source::References(64),
        Profile::Explicit,
        ROOMY,
        COMPLETE,
    ),
    unit(
        "references-to-one-declaration-1024",
        Source::References(1024),
        Profile::Explicit,
        ROOMY,
        COMPLETE,
    ),
    unit(
        "alias-chain-at-limit",
        Source::AliasChain(16),
        Profile::Dom,
        Bound::At(JsLimitKind::AliasChain, EXACT),
        COMPLETE,
    ),
    unit(
        "vertical-slice-module",
        Source::Representative,
        Profile::Dom,
        ROOMY,
        COMPLETE,
    ),
    unit(
        "conditional-selection-unsupported",
        Source::Text(CONDITIONAL),
        Profile::Explicit,
        ROOMY,
        BLOCKED,
    ),
    unit(
        "receiver-escape-blocked",
        Source::Text(RECEIVER_ESCAPE),
        Profile::Dom,
        ROOMY,
        BLOCKED,
    ),
    unit(
        "host-syntax-invalid",
        Source::Text(HOST_SYNTAX_INVALID),
        Profile::Explicit,
        ROOMY,
        BLOCKED,
    ),
    unit(
        "ast-nodes-exact",
        Source::Text(SMALL),
        Profile::Explicit,
        Bound::At(JsLimitKind::AstNodes, EXACT),
        COMPLETE,
    ),
    unit(
        "ast-nodes-first-over",
        Source::Text(SMALL),
        Profile::Explicit,
        Bound::At(JsLimitKind::AstNodes, OVER),
        REFUSED,
    ),
    unit(
        "alias-chain-first-over",
        Source::AliasChain(16),
        Profile::Dom,
        Bound::At(JsLimitKind::AliasChain, OVER),
        BLOCKED,
    ),
    unit(
        "proof-steps-exact",
        Source::Text(PROOF),
        Profile::Dom,
        Bound::At(JsLimitKind::ProofSteps, EXACT),
        COMPLETE,
    ),
    unit(
        "proof-steps-first-over",
        Source::Text(PROOF),
        Profile::Dom,
        Bound::At(JsLimitKind::ProofSteps, OVER),
        BLOCKED,
    ),
    unit(
        "annotation-bytes-exact",
        Source::Text(ANNOTATED),
        Profile::Explicit,
        Bound::At(JsLimitKind::AnnotationBytes, EXACT),
        COMPLETE,
    ),
    unit(
        "annotation-bytes-first-over",
        Source::Text(ANNOTATED),
        Profile::Explicit,
        Bound::At(JsLimitKind::AnnotationBytes, OVER),
        BLOCKED,
    ),
    unit(
        "parameter-bindings-exact",
        Source::Parameters(2),
        Profile::Explicit,
        Bound::At(JsLimitKind::ParameterBindings, EXACT),
        COMPLETE,
    ),
    unit(
        "parameter-bindings-first-over",
        Source::Parameters(2),
        Profile::Explicit,
        Bound::At(JsLimitKind::ParameterBindings, OVER),
        BLOCKED,
    ),
    unit(
        "unit-bytes-exact",
        Source::Text(SMALL),
        Profile::Explicit,
        Bound::At(JsLimitKind::UnitBytes, EXACT),
        COMPLETE,
    ),
    inventory(
        "inventory-complete",
        Completeness::Complete,
        ROOMY,
        COMPLETE,
    ),
    inventory("inventory-partial", Completeness::Partial, ROOMY, COMPLETE),
    inventory(
        "units-exact",
        Completeness::Complete,
        Bound::At(JsLimitKind::Units, EXACT),
        COMPLETE,
    ),
    inventory(
        "units-first-over",
        Completeness::Complete,
        Bound::At(JsLimitKind::Units, OVER),
        REFUSED,
    ),
];

/// The units every inventory fixture assembles, each read under the DOM profile.
const INVENTORY_UNITS: [(&str, Source); 3] = [
    ("checkout", Source::Representative),
    ("nav", Source::UiLiterals(64)),
    ("help", Source::Text(NO_CANDIDATES)),
];

/// How many units every inventory fixture assembles.
pub(super) const INVENTORY_UNIT_COUNT: u64 = INVENTORY_UNITS.len() as u64;

/// The scope the inventory fixtures declare.
pub(super) const SCOPE: &str = "storefront";

/// Bounds with room for every fixture, so only the one a fixture names binds.
pub(super) fn roomy() -> JsAuthoringLimits {
    JsAuthoringLimits {
        units: 16,
        unit_bytes: 256 * 1024,
        total_bytes: 1024 * 1024,
        ast_nodes: 256 * 1024,
        input_segments: 4096,
        references: 2048,
        exclusions: 256,
        parameter_bindings: 64,
        alias_chain: 16,
        tracked_origins: 64,
        proof_steps: 1 << 20,
        annotation_bytes: 4096,
        authoring: AuthoringLimits {
            declarations: 2048,
            message_text_bytes: 64 * 1024,
            emitted_mf2_bytes: 128 * 1024,
            extraction_segments: 4096,
            parameter_names: 64,
            parameter_name_bytes: 256,
            metadata_value_bytes: 4096,
            vocabulary_members: 256,
            projection_nodes: 4096,
            projection_depth: 32,
            diagnostics: 256,
        },
    }
}

fn owner() -> Result<OwnerIdentity, PreparationFailure> {
    OwnerIdentity::new(OwnerKind::Application, "intlify-authoring-js-smoke")
        .map_err(|_| PreparationFailure::Fixture)
}

/// The context every measured unit is read against, pinned to this profile.
pub(super) fn context() -> Result<TestContext, PreparationFailure> {
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

fn snapshot(unit: &str, bytes: &[u8]) -> Result<SourceSnapshot, PreparationFailure> {
    SourceSnapshot::new(
        owner()?,
        unit,
        "1",
        Grammar::JsModule.identity(),
        bytes.len() as u64,
        IntegrityDigest::from_hash(digest_bytes(bytes)).as_str(),
    )
    .map_err(|_| PreparationFailure::Fixture)
}

fn member(unit: &str) -> Result<UnitMember, PreparationFailure> {
    UnitMember::new(unit, "1").map_err(|_| PreparationFailure::Fixture)
}

/// Admit one unit's bytes, as a caller would, outside any interval.
pub(super) fn admit<'b>(
    context: &TestContext,
    profile: &JsAuthoringProfile,
    snapshot: &SourceSnapshot,
    bytes: &'b [u8],
    limits: &JsAuthoringLimits,
) -> Result<Vec<AdmittedUnit<'b>>, PreparationFailure> {
    admit_units(
        context,
        profile,
        Completeness::Complete,
        &[member(snapshot.unit().as_str())?],
        &[SourceUnit::new(snapshot.clone(), bytes)],
        limits,
    )
    .map_err(|_| PreparationFailure::Fixture)
}

fn bounded(
    mut limits: JsAuthoringLimits,
    kind: JsLimitKind,
    value: u64,
) -> Result<JsAuthoringLimits, PreparationFailure> {
    match kind {
        JsLimitKind::AstNodes => limits.ast_nodes = value,
        JsLimitKind::ProofSteps => limits.proof_steps = value,
        JsLimitKind::AliasChain => limits.alias_chain = value,
        JsLimitKind::AnnotationBytes => limits.annotation_bytes = value,
        JsLimitKind::ParameterBindings => limits.parameter_bindings = value,
        JsLimitKind::UnitBytes => limits.unit_bytes = value,
        JsLimitKind::Units => limits.units = value,
        _ => return Err(PreparationFailure::Fixture),
    }
    limits.validate().map_err(|_| PreparationFailure::Fixture)
}

/// A bound and the value it was set to, as the case projection names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SetBound {
    pub(super) kind: JsLimitKind,
    pub(super) edge: Edge,
    pub(super) value: u64,
}

/// One unit fixture, prepared.
pub(super) struct UnitCase {
    pub(super) profile: JsAuthoringProfile,
    pub(super) snapshot: SourceSnapshot,
    pub(super) bytes: Vec<u8>,
    pub(super) limits: JsAuthoringLimits,
    /// The scratch every measured invocation of this case reuses.
    pub(super) workspace: RefCell<JsAnalysisWorkspace>,
}

/// One inventory fixture, prepared.
pub(super) struct InventoryCase {
    pub(super) profile: JsAuthoringProfile,
    pub(super) completeness: Completeness,
    pub(super) membership: Vec<UnitMember>,
    /// The analyses assembly starts from, established outside the interval.
    pub(super) analyses: Vec<UnitAnalysis>,
    pub(super) limits: JsAuthoringLimits,
}

/// What a prepared fixture reads.
#[allow(
    clippy::large_enum_variant,
    reason = "one value per case, prepared once before any capture"
)]
pub(super) enum Reads {
    Unit(UnitCase),
    Inventory(InventoryCase),
}

/// Everything one case needs, prepared before any interval opens.
pub struct Prepared {
    pub(super) fixture: Fixture,
    pub(super) context: TestContext,
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

/// Classify one unit once, outside measurement, on the given workspace.
pub(super) fn observe_unit(
    context: &TestContext,
    case: &UnitCase,
    workspace: &RefCell<JsAnalysisWorkspace>,
) -> Result<Observed, PreparationFailure> {
    let units = admit(
        context,
        &case.profile,
        &case.snapshot,
        &case.bytes,
        &case.limits,
    )?;
    let input = (context, &case.profile, &units[0], &case.limits, workspace);
    Ok(operation::observe_classify(
        &operation::invoke_classify(input),
        input,
    ))
}

/// The fixture's own count of the bound it is measured at.
fn own_count(
    context: &TestContext,
    source: Source,
    profile: &JsAuthoringProfile,
    snapshot: &SourceSnapshot,
    bytes: &[u8],
    kind: JsLimitKind,
) -> Result<u64, PreparationFailure> {
    // Counts the analysis takes are read from a roomy run of the same
    // operation; counts written into the fixture are read from it.
    let counted = |pick: fn(UnitWork) -> u64| -> Result<u64, PreparationFailure> {
        let units = admit(context, profile, snapshot, bytes, &roomy())?;
        match crate::analysis::classify(
            context,
            profile,
            &units[0],
            &roomy(),
            &mut JsAnalysisWorkspace::new(),
            &|| false,
        ) {
            Ok(Classification::Read(read)) => Ok(pick(read.work)),
            _ => Err(PreparationFailure::Fixture),
        }
    };
    match (kind, source) {
        (JsLimitKind::AstNodes, _) => counted(|work| work.ast_nodes),
        (JsLimitKind::ProofSteps, _) => counted(|work| work.proof_steps),
        (JsLimitKind::AliasChain, Source::AliasChain(length)) => Ok(u64::from(length)),
        (JsLimitKind::AnnotationBytes, _) => Ok(note!().len() as u64),
        (JsLimitKind::ParameterBindings, Source::Parameters(count)) => Ok(u64::from(count)),
        (JsLimitKind::UnitBytes, _) => Ok(bytes.len() as u64),
        _ => Err(PreparationFailure::Fixture),
    }
}

fn set(own: u64, kind: JsLimitKind, edge: Edge) -> Result<SetBound, PreparationFailure> {
    let value = match edge {
        Edge::Exact => own,
        Edge::FirstOver => own.checked_sub(1).ok_or(PreparationFailure::Fixture)?,
    };
    Ok(SetBound { kind, edge, value })
}

fn prepare_unit(
    context: &TestContext,
    source: Source,
    profile: Profile,
    bound: Bound,
) -> Result<(UnitCase, Option<SetBound>), PreparationFailure> {
    let profile = profile.build()?;
    let bytes = source.text().into_bytes();
    let snapshot = snapshot("checkout", &bytes)?;
    let (limits, set_bound) = match bound {
        Bound::Roomy => (roomy(), None),
        Bound::At(kind, edge) => {
            let own = own_count(context, source, &profile, &snapshot, &bytes, kind)?;
            let set_bound = set(own, kind, edge)?;
            (bounded(roomy(), kind, set_bound.value)?, Some(set_bound))
        }
    };
    Ok((
        UnitCase {
            profile,
            snapshot,
            bytes,
            limits,
            workspace: RefCell::new(JsAnalysisWorkspace::new()),
        },
        set_bound,
    ))
}

fn prepare_inventory(
    context: &TestContext,
    completeness: Completeness,
    bound: Bound,
) -> Result<(InventoryCase, Option<SetBound>), PreparationFailure> {
    let profile = Profile::Dom.build()?;
    let mut membership = Vec::with_capacity(INVENTORY_UNITS.len());
    let mut analyses = Vec::with_capacity(INVENTORY_UNITS.len());
    // Each unit is analyzed on its own fresh workspace, outside the interval:
    // assembly starts from analyses, not from bytes.
    for (unit, source) in INVENTORY_UNITS {
        let bytes = source.text().into_bytes();
        let snapshot = snapshot(unit, &bytes)?;
        let units = admit(context, &profile, &snapshot, &bytes, &roomy())?;
        let analysis = analyze_unit(
            context,
            &profile,
            &units[0],
            &roomy(),
            &mut JsAnalysisWorkspace::new(),
            &|| false,
        )
        .map_err(|_| PreparationFailure::PriorStage)?;
        membership.push(member(unit)?);
        analyses.push(analysis);
    }
    let (limits, set_bound) = match bound {
        Bound::Roomy => (roomy(), None),
        Bound::At(JsLimitKind::Units, edge) => {
            let set_bound = set(INVENTORY_UNITS.len() as u64, JsLimitKind::Units, edge)?;
            (
                bounded(roomy(), JsLimitKind::Units, set_bound.value)?,
                Some(set_bound),
            )
        }
        Bound::At(..) => return Err(PreparationFailure::Fixture),
    };
    Ok((
        InventoryCase {
            profile,
            completeness,
            membership,
            analyses,
            limits,
        },
        set_bound,
    ))
}

/// Prepare one case completely, before any measurement begins.
pub(super) fn prepare(fixture: Fixture) -> Result<Prepared, PreparationFailure> {
    let context = context()?;
    let (reads, bound) = match fixture.input {
        Input::Unit {
            source,
            profile,
            bound,
        } => {
            let (case, bound) = prepare_unit(&context, source, profile, bound)?;
            (Reads::Unit(case), bound)
        }
        Input::Inventory {
            completeness,
            bound,
        } => {
            let (case, bound) = prepare_inventory(&context, completeness, bound)?;
            (Reads::Inventory(case), bound)
        }
    };
    // The expectation is established here, by one invocation that is not
    // measured and never becomes a sample. A unit is read on a fresh
    // workspace, so every measured invocation, which reuses the case's own
    // workspace, is compared with a fresh reading.
    let expected = match &reads {
        Reads::Unit(case) => {
            observe_unit(&context, case, &RefCell::new(JsAnalysisWorkspace::new()))?
        }
        Reads::Inventory(case) => {
            let input = (
                &context,
                &case.profile,
                SCOPE,
                case.completeness,
                case.membership.as_slice(),
                case.analyses.as_slice(),
                &case.limits,
            );
            operation::observe_assemble(&operation::invoke_assemble(input), input)
        }
    };
    Ok(Prepared {
        fixture,
        context,
        reads,
        bound,
        expected,
    })
}

#[cfg(test)]
mod tests {
    use intlify_authoring::UnitOutcome;

    use super::*;
    use crate::benchmark::operation::Count;
    use crate::ProducerFailure;

    fn prepared(name: &str) -> Prepared {
        let fixture = FIXTURES
            .into_iter()
            .find(|fixture| fixture.name == name)
            .unwrap_or_else(|| panic!("{name} is a fixture"));
        prepare(fixture).unwrap()
    }

    fn unit_case(prepared: &Prepared) -> &UnitCase {
        let Reads::Unit(case) = &prepared.reads else {
            panic!("{} reads one unit", prepared.fixture.name);
        };
        case
    }

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
        // Every path is exercised, the refusing one included.
        for path in [COMPLETE, BLOCKED, REFUSED] {
            assert!(FIXTURES.iter().any(|fixture| fixture.expected == path));
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

    #[test]
    fn the_whole_analysis_reaches_the_same_conclusion_as_classifying() {
        // Classifying stops before messages are analyzed. A fixture claiming
        // to measure valid work has to be valid all the way through, so each
        // unit is also analyzed in full.
        let context = context().unwrap();
        for fixture in FIXTURES {
            let Input::Unit { .. } = fixture.input else {
                continue;
            };
            let prepared = prepare(fixture).unwrap();
            let case = unit_case(&prepared);
            let units = admit(
                &context,
                &case.profile,
                &case.snapshot,
                &case.bytes,
                &case.limits,
            )
            .unwrap();
            let analysis = analyze_unit(
                &context,
                &case.profile,
                &units[0],
                &case.limits,
                &mut JsAnalysisWorkspace::new(),
                &|| false,
            );
            let outcome = analysis.as_ref().map(UnitAnalysis::outcome);
            match fixture.expected {
                Expected::Complete => {
                    assert_eq!(outcome, Ok(UnitOutcome::Checked), "{}", fixture.name);
                }
                Expected::Blocked => assert!(
                    matches!(outcome, Ok(UnitOutcome::Blocked | UnitOutcome::Failed)),
                    "{}: {outcome:?}",
                    fixture.name
                ),
                Expected::OperationalFailure => {
                    assert!(analysis.is_err(), "{}", fixture.name);
                }
            }
        }
    }

    #[test]
    fn each_first_over_bound_is_one_below_its_exact_partner() {
        let bound_of = |name: &str| prepared(name).bound.unwrap();
        for (exact, over) in [
            ("ast-nodes-exact", "ast-nodes-first-over"),
            ("alias-chain-at-limit", "alias-chain-first-over"),
            ("proof-steps-exact", "proof-steps-first-over"),
            ("annotation-bytes-exact", "annotation-bytes-first-over"),
            ("parameter-bindings-exact", "parameter-bindings-first-over"),
            ("units-exact", "units-first-over"),
        ] {
            let (exact, over) = (bound_of(exact), bound_of(over));
            assert_eq!(exact.kind, over.kind);
            assert_eq!((exact.edge, over.edge), (Edge::Exact, Edge::FirstOver));
            assert_eq!(over.value + 1, exact.value);
        }
        // The counts written into a fixture are the bounds it sits at.
        assert_eq!(bound_of("alias-chain-at-limit").value, 16);
        assert_eq!(
            bound_of("annotation-bytes-exact").value,
            note!().len() as u64
        );
        assert_eq!(bound_of("parameter-bindings-exact").value, 2);
        assert_eq!(bound_of("unit-bytes-exact").value, SMALL.len() as u64);
        assert_eq!(bound_of("units-exact").value, INVENTORY_UNIT_COUNT);
    }

    #[test]
    fn a_reused_workspace_agrees_with_a_fresh_one_after_success_failure_and_cancellation() {
        let context = context().unwrap();
        let large = prepared("ui-literals-1024");
        let refused = prepared("ast-nodes-first-over");
        let dirty = |previous: &str| -> RefCell<JsAnalysisWorkspace> {
            let workspace = RefCell::new(JsAnalysisWorkspace::new());
            match previous {
                "success" => {
                    observe_unit(&context, unit_case(&large), &workspace).unwrap();
                }
                "failure" => {
                    let observed = observe_unit(&context, unit_case(&refused), &workspace).unwrap();
                    assert_eq!(observed.path, REFUSED);
                }
                _ => {
                    let case = unit_case(&large);
                    let units = admit(
                        &context,
                        &case.profile,
                        &case.snapshot,
                        &case.bytes,
                        &case.limits,
                    )
                    .unwrap();
                    let stopped = crate::analysis::classify(
                        &context,
                        &case.profile,
                        &units[0],
                        &case.limits,
                        &mut workspace.borrow_mut(),
                        &|| true,
                    );
                    assert!(matches!(stopped, Err(ProducerFailure::Cancelled)));
                }
            }
            workspace
        };
        for fixture in FIXTURES {
            let Input::Unit { .. } = fixture.input else {
                continue;
            };
            let prepared = prepare(fixture).unwrap();
            for previous in ["success", "failure", "cancellation"] {
                let workspace = dirty(previous);
                assert_eq!(
                    &observe_unit(&context, unit_case(&prepared), &workspace).unwrap(),
                    prepared.expected(),
                    "{} after a {previous}",
                    fixture.name
                );
            }
        }
    }

    #[test]
    fn the_work_vector_says_how_each_count_is_known() {
        let count = |name: &str, fact: &str| prepared(name).expected().work.count(fact).cloned();
        let exact = |value| {
            Some(Count::Exact {
                value: intlify_shared_json::quantity::Quantity::new(value),
            })
        };
        // A refused tree is known only to be past its bound.
        let over = prepared("ast-nodes-first-over");
        let limit = over.bound.unwrap().value;
        assert_eq!(
            count("ast-nodes-first-over", "ast_nodes"),
            Some(Count::AtLeast {
                value: intlify_shared_json::quantity::Quantity::new(limit + 1)
            })
        );
        assert_eq!(count("ast-nodes-first-over", "parse_attempts"), exact(1));
        assert_eq!(
            count("ast-nodes-first-over", "declaration_candidates"),
            Some(Count::Unavailable {})
        );
        // A unit the host rejected was parsed once and left no tree to count;
        // nothing in it was recognized.
        assert_eq!(count("host-syntax-invalid", "parse_attempts"), exact(1));
        assert_eq!(
            count("host-syntax-invalid", "ast_nodes"),
            Some(Count::Unavailable {})
        );
        assert_eq!(
            count("host-syntax-invalid", "declaration_candidates"),
            Some(Count::NotApplicable {})
        );
        assert_eq!(count("host-syntax-invalid", "host_diagnostics"), exact(1));
        // Nothing is proven without a DOM global, and no path builds a graph.
        assert_eq!(
            count("equal-text-distinct-declarations", "proof_steps"),
            Some(Count::NotApplicable {})
        );
        assert_eq!(
            count("proof-steps-exact", "proof_steps"),
            exact(prepared("proof-steps-exact").bound.unwrap().value)
        );
        for fixture in FIXTURES {
            if fixture.operation == Operation::ParseAndClassify {
                assert_eq!(
                    count(fixture.name, "cfg_blocks"),
                    Some(Count::NotApplicable {}),
                    "{}",
                    fixture.name
                );
            }
        }
        // The scaled fixtures do the work their scale names.
        assert_eq!(
            count("ui-literals-1024", "declaration_candidates"),
            exact(1024)
        );
        assert_eq!(
            count("references-to-one-declaration-1024", "use_candidates"),
            exact(1024)
        );
        assert_eq!(count("parameters-dense", "parameter_bindings"), exact(32));
        // A reported annotation is still one the unit carries.
        assert_eq!(count("annotation-bytes-exact", "annotations"), exact(1));
        assert_eq!(
            count("annotation-bytes-first-over", "annotations"),
            exact(1)
        );
        assert_eq!(count("no-candidates", "declaration_candidates"), exact(0));
    }

    /// Assemble a unit set the fixtures do not, outside measurement.
    fn assembled(units: &[(&str, Source)]) -> Observed {
        let context = context().unwrap();
        let profile = Profile::Dom.build().unwrap();
        let mut membership = Vec::new();
        let mut analyses = Vec::new();
        for (unit, source) in units {
            let bytes = source.text().into_bytes();
            let snapshot = snapshot(unit, &bytes).unwrap();
            let admitted = admit(&context, &profile, &snapshot, &bytes, &roomy()).unwrap();
            analyses.push(
                analyze_unit(
                    &context,
                    &profile,
                    &admitted[0],
                    &roomy(),
                    &mut JsAnalysisWorkspace::new(),
                    &|| false,
                )
                .unwrap(),
            );
            membership.push(member(unit).unwrap());
        }
        let limits = roomy();
        let input = (
            &context,
            &profile,
            SCOPE,
            Completeness::Complete,
            membership.as_slice(),
            analyses.as_slice(),
            &limits,
        );
        operation::observe_assemble(&operation::invoke_assemble(input), input)
    }

    #[test]
    fn an_inventory_holding_a_blocked_unit_reports_why_rather_than_completing() {
        let observed = assembled(&[("checkout", Source::Text(RECEIVER_ESCAPE))]);
        assert_eq!(observed.path, BLOCKED);
        assert_eq!(
            observed.work.count("blocked_units"),
            Some(&Count::Exact {
                value: intlify_shared_json::quantity::Quantity::new(1)
            })
        );
    }

    #[test]
    fn an_assembly_is_observed_through_the_artifact_it_produced() {
        // Two inventories that make the same claim about the same scope differ
        // in what they hold, and the observation has to tell them apart.
        let none = assembled(&[("help", Source::Text(NO_CANDIDATES))]);
        let one = assembled(&[("help", Source::UiLiterals(1))]);
        assert_eq!((none.path, one.path), (COMPLETE, COMPLETE));
        assert_ne!(none.observation, one.observation);
        let Some(Count::Exact { value }) = one.work.count("artifact_bytes") else {
            panic!("the artifact was written");
        };
        assert!(value.get() > 0);
    }

    /// Classify a unit the fixtures do not, outside measurement.
    fn observed(text: &str, profile: Profile) -> Observed {
        let case = UnitCase {
            profile: profile.build().unwrap(),
            snapshot: snapshot("checkout", text.as_bytes()).unwrap(),
            bytes: text.as_bytes().to_vec(),
            limits: roomy(),
            workspace: RefCell::new(JsAnalysisWorkspace::new()),
        };
        observe_unit(
            &context().unwrap(),
            &case,
            &RefCell::new(JsAnalysisWorkspace::new()),
        )
        .unwrap()
    }

    #[test]
    fn a_classification_is_observed_through_what_each_use_and_record_says() {
        // Both units declare the same messages at the same places and use one
        // of them at the same place. Only which one they use differs.
        let using = |name: &str| {
            format!(
                "{}const a = mf2`A`\nconst b = mf2`B`\nintent({name})\n",
                prelude!()
            )
        };
        assert_ne!(
            observed(&using("a"), Profile::Explicit).observation,
            observed(&using("b"), Profile::Explicit).observation
        );
        // Neither unit declares anything, and each is blocked by one record.
        // Only what the record says, and where, differs.
        let rebound = "export function render() {\n  let pay = document.querySelector('#pay')\n  pay.textContent = 'Pay now'\n}\n";
        let escaped = observed(RECEIVER_ESCAPE, Profile::Dom);
        let unsupported = observed(rebound, Profile::Dom);
        assert_eq!((escaped.path, unsupported.path), (BLOCKED, BLOCKED));
        assert_ne!(escaped.observation, unsupported.observation);
    }

    #[test]
    fn complete_and_partial_assemble_the_same_facts_under_different_claims() {
        let complete = prepared("inventory-complete");
        let partial = prepared("inventory-partial");
        for fact in ["declarations", "references", "exclusions", "checked_units"] {
            assert_eq!(
                complete.expected().work.count(fact),
                partial.expected().work.count(fact),
                "{fact}"
            );
        }
        // What differs is what the inventory claims to cover, so the two are
        // different observations.
        assert_ne!(
            complete.expected().observation,
            partial.expected().observation
        );
        assert_eq!(prepared("units-first-over").expected().path, REFUSED);
    }
}
