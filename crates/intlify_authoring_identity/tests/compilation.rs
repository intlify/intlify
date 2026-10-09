// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 028's representative module, from source to compiled artifacts.
//!
//! The JS Producer reads the module, reconciliation plans its first update
//! from a genesis with fixed candidates, and compilation resolves it against
//! the result; a second revision rewords one message inside its literal.
//! Every expectation is an artifact of `fixtures/phase3/compile-vectors.json`,
//! which was written by hand rather than by the code under test, and which a
//! second implementation has checked.

mod support;

use std::collections::BTreeSet;

use intlify_authoring::test_context::{admit_inventory, TestContext};
use intlify_authoring::{
    AdmittedInventory, AnalysisWorkspace, AuthoringArtifact, Completeness, InventoryArtifact,
    MessageIntentId, OwnerIdentity, OwnerKind, SourceBytes, SourceSnapshot, SurfaceVocabulary,
    Token,
};
use intlify_authoring_identity::{
    admit_intent, admit_reference, admit_registry, admit_update, compile, detail, reconcile,
    AdmittedRegistry, Compilation, CompileEvidence, CompiledScope, ContinuationBasis,
    ContinuityInputs, Eligibility, IdentityWorkspace, IntentMismatch, PreviousUpdate,
    ReconcileInputs, Reconciliation, RegistryArtifact, RetainedSources, SourceEdit,
};
use intlify_authoring_js::{
    admit_units, analyze_unit, assemble_inventory, CheckedInventory, DomGlobal, Grammar, Intrinsic,
    IntrinsicBinding, JsAnalysisWorkspace, JsAuthoringLimits, JsAuthoringProfile, SourceUnit,
    UnitMember,
};
use intlify_shared_json::encoding::digest_bytes;
use intlify_shared_json::token::IntegrityDigest;
use serde_json::{json, Value};
use support::{authoring_limits, bytes, limits, reseal};

const VECTORS: &str = include_str!("../fixtures/phase3/compile-vectors.json");
const INTENT_SCHEMA: &str = include_str!("../schema/message-intent-v0.schema.json");
const REFERENCE_SCHEMA: &str = include_str!("../schema/message-reference-v0.schema.json");

const SCOPE: &str = "storefront-web";
const UNIT: &str = "app";

fn document() -> Value {
    serde_json::from_str(VECTORS).expect("committed vectors")
}

/// One committed artifact, by its vector id.
fn vector(id: &str) -> Value {
    document()["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|vector| vector["id"] == id)
        .unwrap_or_else(|| panic!("no vector {id}"))["artifact"]
        .clone()
}

/// Every committed artifact whose id starts with a prefix, in order.
fn vectors(prefix: &str) -> Vec<Value> {
    document()["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|vector| vector["id"].as_str().unwrap().starts_with(prefix))
        .map(|vector| vector["artifact"].clone())
        .collect()
}

/// The retained text of one revision of the module.
fn text(revision: &str) -> String {
    document()["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["revision"] == revision)
        .and_then(|source| source["text"].as_str())
        .unwrap()
        .to_owned()
}

fn owner() -> OwnerIdentity {
    OwnerIdentity::new(OwnerKind::Application, "storefront").expect("checked project id")
}

/// The test context the JS Producer resolves the module against.
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

fn profile() -> JsAuthoringProfile {
    JsAuthoringProfile::new()
        .with_bindings([
            IntrinsicBinding::new("fixture-authoring", "intent", Intrinsic::Intent),
            IntrinsicBinding::new("fixture-authoring", "mf2", Intrinsic::Mf2),
            IntrinsicBinding::new("fixture-authoring", "noIntent", Intrinsic::NoIntent),
        ])
        .expect("a consistent binding set")
        .with_dom_globals([DomGlobal::Document])
}

fn js_limits() -> JsAuthoringLimits {
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
        authoring: authoring_limits(),
    }
    .validate()
    .expect("satisfiable bounds")
}

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

/// Read one revision of the module through the JS Producer.
fn produce(revision: &str, text: &str) -> InventoryArtifact {
    let (context, profile, limits) = (context(), profile(), js_limits());
    let membership = [UnitMember::new(UNIT, revision).unwrap()];
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
    assemble_inventory(
        &context,
        &profile,
        SCOPE,
        Completeness::Complete,
        &membership,
        &analyses,
        &limits,
    )
    .expect("assembled")
    .checked_inventory()
    .and_then(CheckedInventory::complete)
    .expect("one complete checked inventory")
    .clone()
}

/// Admit a committed inventory against the bytes it names.
fn inventory(id: &str, revision: &str) -> AdmittedInventory {
    let text = text(revision);
    admit_inventory(
        &bytes(&vector(id)),
        &context(),
        &[SourceBytes {
            unit: UNIT,
            bytes: text.as_bytes(),
        }],
        &authoring_limits(),
        &mut AnalysisWorkspace::new(),
    )
    .expect("an admissible inventory")
}

fn registry(id: &str) -> AdmittedRegistry {
    admit_registry(&bytes(&vector(id)), &limits()).expect("an admissible registry")
}

/// The Intent ID a committed Intent records.
fn id_of(id: &str) -> MessageIntentId {
    serde_json::from_value(vector(id)["body"]["intentId"].clone()).unwrap()
}

/// The one edit between the two revisions, as the reworded welcome's
/// continuity records it.
fn reword() -> SourceEdit {
    let continuity = &vector("intent-2-welcome")["body"]["continuity"];
    let ContinuationBasis::VerifiedEdit(basis) =
        serde_json::from_value(continuity["basis"].clone()).unwrap()
    else {
        panic!("a verified edit");
    };
    basis.changes()[0].clone()
}

/// The texts of both revisions, held while their bytes are borrowed.
struct Texts {
    first: String,
    second: String,
}

impl Texts {
    fn load() -> Self {
        Self {
            first: text("1"),
            second: text("2"),
        }
    }

    fn retained(&self) -> RetainedSources<'_> {
        RetainedSources::new([
            (snapshot("1", &self.first), self.first.as_bytes()),
            (snapshot("2", &self.second), self.second.as_bytes()),
        ])
        .unwrap()
    }
}

/// Compile with the retained texts and the given edits.
fn compiled(
    registry: &AdmittedRegistry,
    inventory: &AdmittedInventory,
    edits: &[SourceEdit],
    workspace: &mut IdentityWorkspace,
) -> Compilation {
    let texts = Texts::load();
    let sources = texts.retained();
    let evidence = CompileEvidence {
        sources: &sources,
        edits,
        previous: None,
    };
    compile(registry, inventory, &evidence, &limits(), workspace).expect("compilation runs")
}

fn scope(compilation: Compilation) -> CompiledScope {
    match compilation {
        Compilation::Compiled(scope) => *scope,
        Compilation::Unresolved(diagnostics) => panic!("unresolved: {diagnostics:?}"),
    }
}

fn values<A: serde::Serialize>(artifacts: &[A]) -> Vec<Value> {
    artifacts
        .iter()
        .map(|artifact| serde_json::to_value(artifact).unwrap())
        .collect()
}

#[test]
fn the_producer_reads_both_revisions_into_the_committed_inventories() {
    for (revision, id) in [("1", "inventory-1"), ("2", "inventory-2")] {
        let produced = produce(revision, &text(revision));
        assert_eq!(serde_json::to_value(&produced).unwrap(), vector(id), "{id}");
    }
}

#[test]
fn reconciliation_allocates_every_declaration_from_the_genesis() {
    let genesis = registry("registry-0");
    let first = inventory("inventory-1", "1");
    // The host's candidates, in the canonical order of the declarations they
    // go to: the greeting tag, the 'Save' text, the 'Welcome' literal.
    let candidates = [
        id_of("intent-1-greeting"),
        id_of("intent-1-save"),
        id_of("intent-1-welcome"),
    ];
    let texts = Texts::load();
    let sources = texts.retained();
    let membership = [Token::new(UNIT).unwrap()];
    let inputs = ReconcileInputs {
        evidence: ContinuityInputs {
            sources: &sources,
            edits: &[],
            membership: &membership,
            previous: None,
        },
        explicit: &[],
        links: &[],
        candidates: &candidates,
    };
    let Ok(Reconciliation::Planned(plan)) = reconcile(
        &genesis,
        &first,
        &inputs,
        &limits(),
        &mut IdentityWorkspace::new(),
    ) else {
        panic!("a plan");
    };
    assert_eq!(
        serde_json::to_value(plan.update().artifact()).unwrap(),
        vector("update-1")
    );
    let result = RegistryArtifact::seal(plan.result().clone()).unwrap();
    assert_eq!(serde_json::to_value(&result).unwrap(), vector("registry-1"));
    assert!(plan
        .eligibility()
        .iter()
        .all(|(_, eligibility)| *eligibility == Eligibility::Automatic));
}

#[test]
fn compilation_gives_exactly_the_committed_artifacts_and_one_target_per_declaration() {
    let (allocated, first) = (registry("registry-1"), inventory("inventory-1", "1"));
    let scope = scope(compiled(
        &allocated,
        &first,
        &[],
        &mut IdentityWorkspace::new(),
    ));
    assert_eq!(values(scope.intents()), vectors("intent-1-"));
    assert_eq!(values(scope.references()), vectors("reference-1-"));
    assert_eq!(scope.completeness(), Completeness::Complete);

    // Both greetings use the one declaration, so they share its target.
    let [save, welcome, greeting, again] = scope.references() else {
        panic!("four use sites");
    };
    assert_ne!(greeting.body().occurrence(), again.body().occurrence());
    assert_eq!(greeting.body().targets(), again.body().targets());
    // Separate declarations are separate Intents.
    let ids: BTreeSet<&MessageIntentId> = [save, welcome, greeting]
        .iter()
        .map(|reference| reference.body().targets()[0].intent_id())
        .collect();
    assert_eq!(ids.len(), 3);
}

#[test]
fn the_same_source_and_registry_give_the_same_bytes_and_nothing_to_update() {
    let (allocated, first) = (registry("registry-1"), inventory("inventory-1", "1"));
    let encoded = |compilation: Compilation| {
        let scope = scope(compilation);
        (
            serde_json::to_vec(scope.intents()).unwrap(),
            serde_json::to_vec(scope.references()).unwrap(),
        )
    };
    let mut workspace = IdentityWorkspace::new();
    let once = encoded(compiled(&allocated, &first, &[], &mut workspace));
    let again = encoded(compiled(&allocated, &first, &[], &mut workspace));
    let fresh = encoded(compiled(
        &allocated,
        &first,
        &[],
        &mut IdentityWorkspace::new(),
    ));
    assert_eq!(once, again);
    assert_eq!(once, fresh);

    // Reconciling the same inventory decides nothing, so the registry the
    // artifacts name stays the current one.
    let update = admit_update(&bytes(&vector("update-1")), &limits()).unwrap();
    let texts = Texts::load();
    let sources = texts.retained();
    let membership = [Token::new(UNIT).unwrap()];
    let inputs = ReconcileInputs {
        evidence: ContinuityInputs {
            sources: &sources,
            edits: &[],
            membership: &membership,
            previous: Some(PreviousUpdate {
                update: &update,
                inventory: &first,
            }),
        },
        explicit: &[],
        links: &[],
        candidates: &[],
    };
    assert_eq!(
        reconcile(&allocated, &first, &inputs, &limits(), &mut workspace),
        Ok(Reconciliation::Unchanged)
    );
}

#[test]
fn a_reworded_welcome_keeps_its_id_across_its_one_edit() {
    let allocated = registry("registry-1");
    let (first, second) = (inventory("inventory-1", "1"), inventory("inventory-2", "2"));
    // Without the edit, nothing carries an ID into the new revision.
    let Compilation::Unresolved(found) =
        compiled(&allocated, &second, &[], &mut IdentityWorkspace::new())
    else {
        panic!("unresolved");
    };
    let located: Vec<_> = found
        .iter()
        .map(|diagnostic| (diagnostic.detail(), diagnostic.occurrence().cloned()))
        .collect();
    let expected: Vec<_> = second
        .inventory()
        .declarations()
        .iter()
        .map(|facts| {
            (
                Some(detail::association_missing()),
                Some(facts.occurrence().clone()),
            )
        })
        .collect();
    assert_eq!(located, expected);

    // With it, every ID continues, and the artifacts are the committed ones.
    let carried = scope(compiled(
        &allocated,
        &second,
        &[reword()],
        &mut IdentityWorkspace::new(),
    ));
    assert_eq!(values(carried.intents()), vectors("intent-2-"));
    assert_eq!(values(carried.references()), vectors("reference-2-"));
    let held = scope(compiled(
        &allocated,
        &first,
        &[],
        &mut IdentityWorkspace::new(),
    ));
    let changed: Vec<bool> = held
        .intents()
        .iter()
        .zip(carried.intents())
        .map(|(before, after)| {
            assert_eq!(before.body().intent_id(), after.body().intent_id());
            before.body().intent_revision() != after.body().intent_revision()
        })
        .collect();
    // Only the reworded message changes what it means.
    assert_eq!(changed, [false, false, true]);
}

#[test]
fn a_supplied_artifact_is_checked_against_the_compilation_it_came_from() {
    let allocated = registry("registry-1");
    let (first, second) = (inventory("inventory-1", "1"), inventory("inventory-2", "2"));
    let held = scope(compiled(
        &allocated,
        &first,
        &[],
        &mut IdentityWorkspace::new(),
    ));
    let carried = scope(compiled(
        &allocated,
        &second,
        &[reword()],
        &mut IdentityWorkspace::new(),
    ));
    for (scope, other, revision) in [(&held, &carried, "1"), (&carried, &held, "2")] {
        for value in vectors(&format!("intent-{revision}-")) {
            let supplied = admit_intent(&bytes(&value), &limits()).expect("admissible");
            assert_eq!(scope.check_intent(&supplied), Ok(()));
            // The other revision's compilation is of another inventory.
            assert_eq!(
                other.check_intent(&supplied),
                Err(IntentMismatch::Inventory)
            );
        }
        for value in vectors(&format!("reference-{revision}-")) {
            let supplied = admit_reference(&bytes(&value), &limits()).expect("admissible");
            assert_eq!(scope.check_reference(&supplied), Ok(()));
        }
    }
}

#[test]
fn compilation_allocates_nothing_and_leaves_its_inputs_as_they_were() {
    let genesis = registry("registry-0");
    let first = inventory("inventory-1", "1");
    let before = (genesis.clone(), first.clone());
    // A genesis holds no identity, and compilation makes none.
    let Compilation::Unresolved(found) =
        compiled(&genesis, &first, &[], &mut IdentityWorkspace::new())
    else {
        panic!("unresolved");
    };
    assert_eq!(found.len(), first.inventory().declarations().len());
    assert!(found
        .iter()
        .all(|diagnostic| diagnostic.detail() == Some(detail::association_missing())));
    assert_eq!((genesis, first), before);
}

type Damage = (&'static str, fn(&mut Value));

#[test]
fn the_committed_schemas_accept_what_the_reader_admits_and_refuse_the_same_damage() {
    let intent_cases: [Damage; 7] = [
        ("another kind", |v| v["kind"] = json!("message-reference")),
        ("an unknown member in the body", |v| {
            v["body"]["note"] = json!("x");
        }),
        ("a registry member naming an inventory", |v| {
            v["body"]["registry"]["kind"] = json!("authoring-inventory");
        }),
        ("an inventory member under a later schema", |v| {
            v["body"]["inventory"]["schemaRevision"] = json!("1");
        }),
        ("an Intent ID in capitals", |v| {
            v["body"]["intentId"]["value"] = json!("D9E281F4B2486236A2C52B774ABE863D");
        }),
        ("an unregistered declaration role", |v| {
            v["body"]["declaration"]["role"] = json!("comment");
        }),
        ("a continuity basis of an unregistered kind", |v| {
            v["body"]["continuity"]["basis"]["kind"] = json!("guessed");
        }),
    ];
    let reference_cases: [Damage; 4] = [
        ("an unknown member in a target", |v| {
            v["body"]["targets"][0]["note"] = json!("x");
        }),
        ("a target naming a registry", |v| {
            v["body"]["targets"][0]["intentArtifact"]["kind"] = json!("intent-registry");
        }),
        ("a target revision that is not a digest", |v| {
            v["body"]["targets"][0]["intentRevision"] = json!("sha256:xyz");
        }),
        ("an inventory member naming an Intent", |v| {
            v["body"]["inventory"]["kind"] = json!("message-intent");
        }),
    ];

    let intent_schema: Value = serde_json::from_str(INTENT_SCHEMA).unwrap();
    let intents = jsonschema::draft7::new(&intent_schema).expect("a valid schema");
    let reference_schema: Value = serde_json::from_str(REFERENCE_SCHEMA).unwrap();
    let references = jsonschema::draft7::new(&reference_schema).expect("a valid schema");
    for value in vectors("intent-") {
        assert!(intents.is_valid(&value));
        assert!(admit_intent(&bytes(&value), &limits()).is_ok());
    }
    for value in vectors("reference-") {
        assert!(references.is_valid(&value));
        assert!(admit_reference(&bytes(&value), &limits()).is_ok());
    }

    for (label, damage) in intent_cases {
        let mut damaged = vector("intent-2-welcome");
        damage(&mut damaged);
        assert!(!intents.is_valid(&damaged), "the schema admitted {label}");
        assert!(
            admit_intent(&reseal(damaged), &limits()).is_err(),
            "the reader admitted {label}"
        );
    }
    for (label, damage) in reference_cases {
        let mut damaged = vector("reference-2-3");
        damage(&mut damaged);
        assert!(
            !references.is_valid(&damaged),
            "the schema admitted {label}"
        );
        assert!(
            admit_reference(&reseal(damaged), &limits()).is_err(),
            "the reader admitted {label}"
        );
    }
}
