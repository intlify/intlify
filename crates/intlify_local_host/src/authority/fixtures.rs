// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Unit-test fixtures: the identity crate's committed registry chain,
//! admitted, and plans reconciled from it with its own evidence.
//!
//! The integration tests read the same chain through `tests/support`. Unit
//! tests cannot share that module, so what they need is repeated here.

use intlify_authoring::test_context::{admit_inventory, TestContext};
use intlify_authoring::{
    AdmittedInventory, AnalysisWorkspace, AuthoringBasis, AuthoringContext, AuthoringLimits,
    InventoryArtifact, MessageIntentId, OwnerIdentity, OwnerKind, SourceSnapshot,
    SurfaceVocabulary, Token, UnitResult,
};
use intlify_authoring_identity::{
    admit_registry, admit_update, reconcile, AdmittedRegistry, AdmittedUpdate, ContinuationBasis,
    ContinuityInputs, ExplicitDecision, IdentityDecision, IdentityLimits, IdentityWorkspace, Plan,
    PreviousUpdate, ReconcileInputs, Reconciliation, RegistryIdentity, RetainedSources, SourceEdit,
};
use serde_json::Value;

use super::action::Action;
use super::context::{AuthorityLimits, Destination, Establishment, Principal};

const VECTORS: &str =
    include_str!("../../../intlify_authoring_identity/fixtures/phase3/registry-vectors.json");

/// The registry identity of the committed chain.
pub(crate) const REGISTRY: &str = "f55ca0b3224c28776d729daf805177d7";

fn document() -> Value {
    serde_json::from_str(VECTORS).expect("committed vectors")
}

/// One committed artifact, by its vector id.
pub(crate) fn artifact(id: &str) -> Value {
    document()["artifacts"]
        .as_array()
        .expect("an artifact list")
        .iter()
        .find(|vector| vector["id"] == id)
        .unwrap_or_else(|| panic!("no vector {id}"))["artifact"]
        .clone()
}

fn bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).expect("serializable")
}

/// The owner of the committed chain.
pub(crate) fn owner() -> OwnerIdentity {
    OwnerIdentity::new(OwnerKind::Application, "storefront").expect("checked project id")
}

/// The context the committed inventories were resolved against.
pub(crate) fn context() -> TestContext {
    TestContext::builder(
        owner(),
        SurfaceVocabulary::new(["checkout", "nav"]).expect("a vocabulary"),
    )
    .default_source_locale("en")
    .default_surface_class("checkout")
    .build()
    .expect("checked test context")
}

/// The exact pins of that context, which an authority binds.
pub(crate) fn basis() -> AuthoringBasis {
    context().basis().clone()
}

/// Bounds every case stays well inside.
pub(crate) fn identity_limits() -> IdentityLimits {
    IdentityLimits {
        entries: 1024,
        decisions: 1024,
        lineage_links: 64,
        lineage_members: 256,
        source_edits: 256,
        replacements: 4096,
        replacement_bytes: 1024 * 1024,
        history_steps: 64,
        candidates: 256,
        diagnostics: 256,
        targets: 256,
    }
}

/// Bounds every authority stays well inside.
pub(crate) fn limits() -> AuthorityLimits {
    AuthorityLimits {
        principals: 16,
        sources: 64,
        anchors: 16,
        confirmations: 16,
    }
}

fn authoring_limits() -> AuthoringLimits {
    AuthoringLimits {
        declarations: 1024,
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
    }
    .validate()
    .expect("satisfiable bounds")
}

/// The committed chain: registry-0 to registry-3, update-1 to update-3 and
/// inventory-1 to inventory-3, admitted.
pub(crate) struct Chain {
    registries: Vec<AdmittedRegistry>,
    updates: Vec<AdmittedUpdate>,
    inventories: Vec<AdmittedInventory>,
}

impl Chain {
    pub(crate) fn load() -> Self {
        Self {
            registries: (0..=3)
                .map(|n| {
                    admit_registry(
                        &bytes(&artifact(&format!("registry-{n}"))),
                        &identity_limits(),
                    )
                    .expect("an admissible registry")
                })
                .collect(),
            updates: (1..=3)
                .map(|n| {
                    admit_update(
                        &bytes(&artifact(&format!("update-{n}"))),
                        &identity_limits(),
                    )
                    .expect("an admissible update")
                })
                .collect(),
            inventories: (1..=3)
                .map(|n| {
                    admit_inventory(
                        &bytes(&artifact(&format!("inventory-{n}"))),
                        &context(),
                        &[],
                        &authoring_limits(),
                        &mut AnalysisWorkspace::new(),
                    )
                    .expect("an admissible inventory")
                })
                .collect(),
        }
    }

    pub(crate) fn registry(&self, n: usize) -> &AdmittedRegistry {
        &self.registries[n]
    }

    pub(crate) fn update(&self, n: usize) -> &AdmittedUpdate {
        &self.updates[n - 1]
    }

    pub(crate) fn inventory(&self, n: usize) -> &AdmittedInventory {
        &self.inventories[n - 1]
    }
}

/// Committed inventory `n` declaring itself partial, sealed again: the same
/// acquired sources, but another inventory than the committed one.
pub(crate) fn partial(n: usize) -> AdmittedInventory {
    let mut body = artifact(&format!("inventory-{n}"))["body"].clone();
    body["completeness"] = Value::from("partial");
    let sealed = InventoryArtifact::seal(serde_json::from_value(body).expect("an inventory body"))
        .expect("a sealable inventory");
    admit_inventory(
        &bytes(&serde_json::to_value(sealed).expect("serializable")),
        &context(),
        &[],
        &authoring_limits(),
        &mut AnalysisWorkspace::new(),
    )
    .expect("an admissible inventory")
}

/// The k-th decision of a committed update.
pub(crate) fn committed(n: usize, k: usize) -> IdentityDecision {
    serde_json::from_value(artifact(&format!("update-{n}"))["body"]["decisions"][k].clone())
        .expect("a decision")
}

/// The Intent ID of the k-th decision of a committed update.
pub(crate) fn id_of(n: usize, k: usize) -> MessageIntentId {
    committed(n, k).intent_id().clone()
}

/// The one edit a committed update's verified continuations carry.
pub(crate) fn the_edit(n: usize) -> SourceEdit {
    let IdentityDecision::Continue(continuation) = committed(n, 0) else {
        panic!("the first decision of update-{n} continues");
    };
    let ContinuationBasis::VerifiedEdit(edit) = continuation.basis() else {
        panic!("a verified edit");
    };
    edit.changes()[0].clone()
}

/// Every retained source of the committed chain, with its text.
pub(crate) fn sources() -> Vec<(SourceSnapshot, String)> {
    let document = document();
    let mut sources: Vec<(SourceSnapshot, String)> = Vec::new();
    for n in 1..=3 {
        for unit in artifact(&format!("inventory-{n}"))["body"]["units"]
            .as_array()
            .expect("units")
        {
            let snapshot: SourceSnapshot =
                serde_json::from_value(unit["source"].clone()).expect("a snapshot");
            if sources.iter().any(|(known, _)| *known == snapshot) {
                continue;
            }
            let text = document["sources"]
                .as_array()
                .expect("sources")
                .iter()
                .find(|source| {
                    source["unit"].as_str() == Some(snapshot.unit().as_str())
                        && source["revision"].as_str() == Some(snapshot.revision().as_str())
                })
                .and_then(|source| source["text"].as_str())
                .expect("retained text for every unit")
                .to_owned();
            sources.push((snapshot, text));
        }
    }
    sources
}

/// Reconcile inventory `current` against registry `base` with the committed
/// evidence, and require a plan.
pub(crate) fn plan(
    chain: &Chain,
    (base, current): (usize, usize),
    edits: &[SourceEdit],
    explicit: &[ExplicitDecision],
    candidates: &[MessageIntentId],
) -> Plan {
    let owned = sources();
    let retained = RetainedSources::new(
        owned
            .iter()
            .map(|(snapshot, text)| (snapshot.clone(), text.as_bytes())),
    )
    .expect("retained sources");
    let membership = [
        Token::new("checkout").expect("a unit"),
        Token::new("nav").expect("a unit"),
    ];
    let inputs = ReconcileInputs {
        evidence: ContinuityInputs {
            sources: &retained,
            edits,
            membership: &membership,
            previous: (base > 0).then(|| PreviousUpdate {
                update: chain.update(base),
                inventory: chain.inventory(base),
            }),
        },
        explicit,
        links: &[],
        candidates,
    };
    match reconcile(
        chain.registry(base),
        chain.inventory(current),
        &inputs,
        &identity_limits(),
        &mut IdentityWorkspace::new(),
    ) {
        Ok(Reconciliation::Planned(plan)) => *plan,
        other => panic!("not planned: {other:?}"),
    }
}

/// Update 1: three allocations against the genesis, all automatic.
pub(crate) fn allocation_plan(chain: &Chain) -> Plan {
    // Candidates go to the declarations in canonical order: pay, cancel,
    // then home.
    plan(
        chain,
        (0, 1),
        &[],
        &[],
        &[id_of(1, 0), id_of(1, 2), id_of(1, 1)],
    )
}

/// The explicit restore of update 3, bound to registry-2 and inventory-3.
pub(crate) fn restore(chain: &Chain) -> ExplicitDecision {
    ExplicitDecision::new(
        chain.registry(2).reference(),
        chain.inventory(3).reference(),
        committed(3, 1),
    )
    .expect("an explicit decision")
}

/// Update 3: two verified continuations and the explicit restore.
pub(crate) fn restoration_plan(chain: &Chain) -> Plan {
    plan(chain, (2, 3), &[the_edit(3)], &[restore(chain)], &[])
}

/// The destination of the committed chain.
pub(crate) fn chain_destination() -> Destination {
    Destination::new(
        "storefront-registry",
        owner(),
        "storefront-web",
        Some(RegistryIdentity::retained(REGISTRY).expect("a fixture value")),
    )
    .expect("a destination")
}

/// An authority over the committed chain for updating to inventory `n`: its
/// units acquired, and the registry before it accepted as the base.
pub(crate) fn establishment(
    chain: &Chain,
    n: usize,
    grants: &[(&str, &[Action])],
) -> Establishment {
    Establishment {
        destination: chain_destination(),
        context: basis(),
        grants: grants
            .iter()
            .map(|(name, actions)| (Principal::new(name).expect("a principal"), actions.to_vec()))
            .collect(),
        acquired: chain
            .inventory(n)
            .inventory()
            .units()
            .iter()
            .map(UnitResult::source)
            .cloned()
            .collect(),
        anchors: vec![chain.registry(n - 1).reference()],
        development: false,
    }
}
