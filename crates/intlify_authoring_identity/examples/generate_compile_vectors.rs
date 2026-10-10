// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Write the compilation vectors an independent implementation checks.
//!
//! The vectors carry design 028's representative module from source to
//! compiled artifacts. The JS Producer reads the module into an inventory;
//! a genesis, the update that allocates its three declarations, and the
//! registry that update produces follow; then the Intents and references
//! compiled from that registry. A second revision rewords `'Welcome'` inside
//! its literal, and its Intents keep their IDs across that one edit.
//!
//! Every update, registry, Intent and reference is written out by hand from
//! the inventory's declarations and the fixed IDs, not produced by
//! reconciling or compiling. That makes each one an expectation
//! `tests/compilation.rs` checks those operations against.
//!
//! The Intent ID and registry identity values are fixed fixture values. They
//! stand in for what a host draws from operating-system randomness; nothing
//! here or in this crate generates one.

use std::path::PathBuf;
use std::process::ExitCode;

use intlify_authoring::test_context::TestContext;
use intlify_authoring::{
    intent_revision, AuthoringArtifact, AuthoringLimits, ByteRange, Completeness,
    InventoryArtifact, MessageIntentId, Occurrence, OwnerIdentity, OwnerKind, SourceSnapshot,
    SurfaceVocabulary, VersionedIdentity, ARTIFACT_INTEGRITY_DOMAIN,
};
use intlify_authoring_identity::{
    AllocationBasis, ContinuationBasis, EntryState, IdentityDecision, IntentContinuity,
    IntentRegistrySnapshot, IntentRegistryUpdate, MessageIntentArtifact, MessageIntentBody,
    MessageReferenceArtifact, MessageReferenceBody, ReferenceTarget, RegistryArtifact,
    RegistryEntry, RegistryIdentity, RegistryUpdateArtifact, Replacement, SourceEdit,
    EDIT_REPLAY_PROFILE, EDIT_REPLAY_REVISION,
};
use intlify_authoring_js::{
    admit_units, analyze_unit, assemble_inventory, CheckedInventory, DomGlobal, Grammar, Intrinsic,
    IntrinsicBinding, JsAnalysisWorkspace, JsAuthoringLimits, JsAuthoringProfile, SourceUnit,
    UnitMember,
};
use intlify_shared_json::encoding::digest_bytes;
use intlify_shared_json::token::IntegrityDigest;
use serde_json::{json, Value};

const ARTIFACT: &str = "fixtures/phase3/compile-vectors.json";

/// Design 028's representative application, byte for byte.
const APPLICATION: &str =
    include_str!("../../intlify_authoring_js/fixtures/phase2/representative-application.js");

const SCOPE: &str = "storefront-web";
const UNIT: &str = "app";

/// The module the fixture profile registers its intrinsics under.
const MODULE: &str = "fixture-authoring";

const REGISTRY: &str = "7660c5e984f1816f6c099923d8bcff1b";
const GREETING: &str = "d9e281f4b2486236a2c52b774abe863d";
const SAVE: &str = "db06942ac933f751cc4e870d6325e61b";
const WELCOME: &str = "26cf116dc8bd8ce09e3894c0e41d9ca6";

fn owner() -> OwnerIdentity {
    OwnerIdentity::new(OwnerKind::Application, "storefront").expect("checked project id")
}

fn id(value: &str) -> MessageIntentId {
    MessageIntentId::retained(owner(), value).expect("a fixture value")
}

/// The test context the JS Producer's fixtures resolve against.
fn context() -> TestContext {
    TestContext::builder(
        owner(),
        SurfaceVocabulary::new(["checkout", "nav"]).expect("a vocabulary"),
    )
    .authoring_profile(JsAuthoringProfile::new().identity().clone())
    .usage_profile(JsAuthoringProfile::usage_profile())
    .default_source_locale("en")
    .default_surface_class("checkout")
    .build()
    .expect("checked test context")
}

/// The fixture intrinsics, and the standard `document` for UI text.
fn profile() -> JsAuthoringProfile {
    JsAuthoringProfile::new()
        .with_bindings([
            IntrinsicBinding::new(MODULE, "intent", Intrinsic::Intent),
            IntrinsicBinding::new(MODULE, "mf2", Intrinsic::Mf2),
            IntrinsicBinding::new(MODULE, "noIntent", Intrinsic::NoIntent),
        ])
        .expect("a consistent binding set")
        .with_dom_globals([DomGlobal::Document])
}

fn limits() -> JsAuthoringLimits {
    JsAuthoringLimits {
        units: 16,
        unit_bytes: 64 * 1024,
        total_bytes: 256 * 1024,
        ast_nodes: 64 * 1024,
        input_segments: 1024,
        references: 256,
        exclusions: 256,
        parameter_bindings: 64,
        alias_chain: 16,
        tracked_origins: 64,
        proof_steps: 1 << 20,
        annotation_bytes: 4096,
        authoring: AuthoringLimits {
            declarations: 256,
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
    .validate()
    .expect("satisfiable bounds")
}

/// The snapshot that names exactly this revision of the module.
fn snapshot(revision: &str, text: &str) -> SourceSnapshot {
    SourceSnapshot::new(
        owner(),
        UNIT,
        revision,
        Grammar::JsModule.identity(),
        text.len() as u64,
        IntegrityDigest::from_hash(digest_bytes(text.as_bytes())).as_str(),
    )
    .expect("checked snapshot")
}

/// Read one revision of the module through the JS Producer, as one complete
/// scope.
fn produce(revision: &str, text: &str) -> InventoryArtifact {
    let (context, profile, limits) = (context(), profile(), limits());
    let membership = [UnitMember::new(UNIT, revision).expect("checked member")];
    let supplied = [SourceUnit::new(snapshot(revision, text), text.as_bytes())];
    let units = admit_units(
        &context,
        &profile,
        Completeness::Complete,
        &membership,
        &supplied,
        &limits,
    )
    .expect("admitted units");
    let mut workspace = JsAnalysisWorkspace::new();
    let analyses: Vec<_> = units
        .iter()
        .map(|unit| {
            analyze_unit(&context, &profile, unit, &limits, &mut workspace, &|| false)
                .expect("the analysis runs")
        })
        .collect();
    let assembled = assemble_inventory(
        &context,
        &profile,
        SCOPE,
        Completeness::Complete,
        &membership,
        &analyses,
        &limits,
    )
    .expect("assembled");
    assembled
        .checked_inventory()
        .and_then(CheckedInventory::complete)
        .expect("one complete checked inventory")
        .clone()
}

/// The declarations an inventory records, in canonical order: the greeting
/// tag, the `'Save'` text and the `'Welcome'` literal.
fn declarations(inventory: &InventoryArtifact) -> [Occurrence; 3] {
    let found: Vec<Occurrence> = inventory
        .body()
        .declarations()
        .iter()
        .map(|facts| facts.occurrence().clone())
        .collect();
    found.try_into().expect("three declarations")
}

/// Write one Intent by hand: the ID, the declaration's own revision, and
/// the continuity when one is given.
fn intent(
    inventory: &InventoryArtifact,
    registry: &RegistryArtifact,
    nth: usize,
    value: &str,
    continuity: Option<IntentContinuity>,
) -> MessageIntentArtifact {
    let facts = &inventory.body().declarations()[nth];
    MessageIntentArtifact::seal(
        MessageIntentBody::new(
            id(value),
            intent_revision(facts.projection()).expect("a revision"),
            inventory.reference(),
            facts.occurrence().clone(),
            registry.reference(),
            continuity,
        )
        .expect("a well-formed body"),
    )
    .expect("sealable")
}

/// Write each use site's reference by hand, naming the Intent written for
/// each declaration it may use.
fn references(
    inventory: &InventoryArtifact,
    intents: &[MessageIntentArtifact],
) -> Vec<MessageReferenceArtifact> {
    inventory
        .body()
        .references()
        .iter()
        .map(|reference| {
            let targets = reference
                .declarations()
                .iter()
                .map(|declaration| {
                    let intent = intents
                        .iter()
                        .find(|intent| intent.body().declaration() == declaration)
                        .expect("an Intent for every declaration");
                    ReferenceTarget::new(
                        intent.body().intent_id().clone(),
                        intent.body().intent_revision().clone(),
                        intent.reference(),
                    )
                })
                .collect();
            MessageReferenceArtifact::seal(
                MessageReferenceBody::new(
                    inventory.reference(),
                    reference.occurrence().clone(),
                    targets,
                )
                .expect("a well-formed body"),
            )
            .expect("sealable")
        })
        .collect()
}

fn entry(id: &str, note: &str, artifact: &impl serde::Serialize) -> Value {
    json!({ "id": id, "note": note, "artifact": artifact })
}

fn main() -> ExitCode {
    let write = matches!(std::env::args().nth(1).as_deref(), Some("--write"));

    // Revision 2 rewords the welcome inside its quotes; nothing else moves.
    let literal = APPLICATION.find("'Welcome'").expect("the welcome literal");
    let reworded = format!(
        "{}Welcome back{}",
        &APPLICATION[..=literal],
        &APPLICATION[literal + "'Welcome".len()..]
    );
    let welcome_back = SourceEdit::new(
        Some(snapshot("1", APPLICATION)),
        Some(snapshot("2", &reworded)),
        vec![Replacement::new(
            ByteRange::new(literal as u64 + 1, (literal + "'Welcome".len()) as u64)
                .expect("ordered"),
            "Welcome back",
        )],
    );

    let first = produce("1", APPLICATION);
    let second = produce("2", &reworded);
    let [greeting, save, welcome] = declarations(&first);

    let genesis = RegistryArtifact::seal(
        IntentRegistrySnapshot::genesis(
            owner(),
            SCOPE,
            RegistryIdentity::retained(REGISTRY).expect("a fixture value"),
        )
        .expect("checked scope"),
    )
    .expect("sealable");

    // The base has no history, so every declaration is new.
    let allocate = RegistryUpdateArtifact::seal(
        IntentRegistryUpdate::new(
            owner(),
            genesis.reference(),
            first.reference(),
            [(GREETING, &greeting), (SAVE, &save), (WELCOME, &welcome)]
                .into_iter()
                .map(|(value, declaration)| {
                    IdentityDecision::allocation(
                        id(value),
                        declaration.clone(),
                        AllocationBasis::confirmed_new(),
                    )
                })
                .collect(),
            vec![],
        )
        .expect("a well-formed update"),
    )
    .expect("sealable");
    let allocated = RegistryArtifact::seal(
        IntentRegistrySnapshot::successor(
            &genesis,
            &allocate,
            [(GREETING, &greeting), (SAVE, &save), (WELCOME, &welcome)]
                .into_iter()
                .map(|(value, declaration)| {
                    RegistryEntry::new(id(value), EntryState::Active, declaration.clone())
                })
                .collect(),
        )
        .expect("a well-formed snapshot"),
    )
    .expect("sealable");

    // Revision 1 against the registry: every declaration is held exactly.
    let held: Vec<MessageIntentArtifact> = [GREETING, SAVE, WELCOME]
        .iter()
        .enumerate()
        .map(|(nth, value)| intent(&first, &allocated, nth, value, None))
        .collect();
    let held_uses = references(&first, &held);

    // Revision 2: each ID continues from its revision 1 declaration across
    // the one edit, and only the welcome's revision changes.
    let carried: Vec<MessageIntentArtifact> =
        [(GREETING, &greeting), (SAVE, &save), (WELCOME, &welcome)]
            .iter()
            .enumerate()
            .map(|(nth, (value, from))| {
                let continuity = IntentContinuity::new(
                    (*from).clone(),
                    ContinuationBasis::verified_edit(
                        VersionedIdentity::literal(EDIT_REPLAY_PROFILE, EDIT_REPLAY_REVISION),
                        vec![welcome_back.clone()],
                    ),
                );
                intent(&second, &allocated, nth, value, Some(continuity))
            })
            .collect();
    let carried_uses = references(&second, &carried);

    let named = ["greeting", "save", "welcome"];
    let uses = [
        "save.textContent = 'Save'",
        "intent('Welcome')",
        "the first intent(greeting, { name })",
        "the second intent(greeting, { name })",
    ];
    let mut artifacts = vec![
        entry(
            "inventory-1",
            "Design 028's representative module, read by the JS Producer: the greeting tag, the 'Save' text and the 'Welcome' literal.",
            &first,
        ),
        entry("registry-0", "The genesis: no history and no entries.", &genesis),
        entry(
            "update-1",
            "Three allocations against the genesis. The base has no history, so every declaration is new.",
            &allocate,
        ),
        entry("registry-1", "Three active entries.", &allocated),
    ];
    for (name, artifact) in named.iter().zip(&held) {
        artifacts.push(entry(
            &format!("intent-1-{name}"),
            "Held exactly by its registry-1 entry, so it carries no continuity.",
            artifact,
        ));
    }
    for (nth, (use_site, artifact)) in uses.iter().zip(&held_uses).enumerate() {
        artifacts.push(entry(
            &format!("reference-1-{}", nth + 1),
            &format!("The use site {use_site}."),
            artifact,
        ));
    }
    artifacts.push(entry(
        "inventory-2",
        "Revision 2 of the module: 'Welcome' reworded to 'Welcome back' inside its quotes.",
        &second,
    ));
    for (name, artifact) in named.iter().zip(&carried) {
        artifacts.push(entry(
            &format!("intent-2-{name}"),
            "Carried from its revision 1 declaration by the one edit; registry-1 is unchanged.",
            artifact,
        ));
    }
    for (nth, (use_site, artifact)) in uses.iter().zip(&carried_uses).enumerate() {
        artifacts.push(entry(
            &format!("reference-2-{}", nth + 1),
            &format!("The use site {use_site}, in revision 2."),
            artifact,
        ));
    }

    let document = json!({
        "note": "Design 028's representative module carried from source to compiled Intent and reference artifacts. An independent implementation re-derives each artifact's integrity digest under the domain below and each Intent's revision from its declaration's projection, resolves every reference to the earlier artifact it names, checks each Intent against its registry entry and each continuity's edit against the retained text, and checks each reference's targets against the Intents of the declarations it may use.",
        "domain": ARTIFACT_INTEGRITY_DOMAIN,
        "revisionDomain": "intent-semantic-revision",
        "projectionSpecification": intlify_authoring::projection_specification(),
        "editProfile": VersionedIdentity::literal(EDIT_REPLAY_PROFILE, EDIT_REPLAY_REVISION),
        "sources": [
            { "unit": UNIT, "revision": "1", "text": APPLICATION },
            { "unit": UNIT, "revision": "2", "text": reworded },
        ],
        "artifacts": artifacts,
    });
    let mut rendered = serde_json::to_string_pretty(&document).expect("serializable document");
    rendered.push('\n');

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(ARTIFACT);
    let committed = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());
    if !write {
        if committed.as_ref() == Some(&document) {
            println!("{} is fresh", path.display());
            return ExitCode::SUCCESS;
        }
        eprintln!("{} is stale; rerun with --write", path.display());
        return ExitCode::FAILURE;
    }
    if committed.as_ref() == Some(&document) {
        println!("{} is already fresh", path.display());
        return ExitCode::SUCCESS;
    }
    match std::fs::write(&path, rendered) {
        Ok(()) => {
            println!("wrote {}", path.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("could not write {}: {error}", path.display());
            ExitCode::FAILURE
        }
    }
}
