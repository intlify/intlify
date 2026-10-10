// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The identity crate's committed registry chain, and the authorities and
//! plans the conformance tests read it with.
//!
//! Every plan is made by the real reconciliation from the chain's own
//! evidence, and every authority by the explicitly test-owned entry, so a
//! case exercises the actual evaluator rather than an expected answer.

#![allow(dead_code, reason = "each test file uses its own subset")]

use intlify_authoring::test_context::{admit_inventory, TestContext};
use intlify_authoring::{
    AdmittedInventory, AnalysisWorkspace, AuthoringBasis, AuthoringContext, AuthoringLimits,
    MessageIntentId, OwnerIdentity, OwnerKind, SourceSnapshot, SurfaceVocabulary, Token,
    UnitResult,
};
use intlify_authoring_identity::{
    admit_registry, admit_update, reconcile, AdmittedRegistry, AdmittedUpdate, ContinuationBasis,
    ContinuityInputs, ExplicitDecision, IdentityDecision, IdentityLimits, IdentityWorkspace,
    PreviousUpdate, ReconcileFailure, ReconcileInputs, Reconciliation, RegistryIdentity,
    RetainedSources, SourceEdit,
};
use intlify_local_host::test_authority::TestAuthority;
use intlify_local_host::{Action, AuthorityLimits, Destination, LocalAuthority, Principal};
use serde_json::Value;

const VECTORS: &str =
    include_str!("../../../intlify_authoring_identity/fixtures/phase3/registry-vectors.json");

/// The registry identity of the committed chain.
pub const REGISTRY: &str = "f55ca0b3224c28776d729daf805177d7";

fn document() -> Value {
    serde_json::from_str(VECTORS).expect("committed vectors")
}

/// One committed artifact, by its vector id.
pub fn artifact(id: &str) -> Value {
    document()["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|vector| vector["id"] == id)
        .unwrap_or_else(|| panic!("no vector {id}"))["artifact"]
        .clone()
}

pub fn bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).expect("serializable")
}

pub fn owner() -> OwnerIdentity {
    OwnerIdentity::new(OwnerKind::Application, "storefront").expect("checked project id")
}

/// The context the committed inventories were resolved against.
pub fn context() -> TestContext {
    TestContext::builder(
        owner(),
        SurfaceVocabulary::new(["checkout", "nav"]).unwrap(),
    )
    .default_source_locale("en")
    .default_surface_class("checkout")
    .build()
    .expect("checked test context")
}

pub fn basis() -> AuthoringBasis {
    context().basis().clone()
}

pub fn identity_limits() -> IdentityLimits {
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

pub fn limits() -> AuthorityLimits {
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

/// The committed chain, admitted.
pub struct Chain {
    registries: Vec<AdmittedRegistry>,
    updates: Vec<AdmittedUpdate>,
    inventories: Vec<AdmittedInventory>,
}

impl Chain {
    pub fn load() -> Self {
        Self {
            registries: (0..=3)
                .map(|n| {
                    admit_registry(
                        &bytes(&artifact(&format!("registry-{n}"))),
                        &identity_limits(),
                    )
                    .unwrap()
                })
                .collect(),
            updates: (1..=3)
                .map(|n| {
                    admit_update(
                        &bytes(&artifact(&format!("update-{n}"))),
                        &identity_limits(),
                    )
                    .unwrap()
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
                    .unwrap()
                })
                .collect(),
        }
    }

    pub fn registry(&self, n: usize) -> &AdmittedRegistry {
        &self.registries[n]
    }

    pub fn update(&self, n: usize) -> &AdmittedUpdate {
        &self.updates[n - 1]
    }

    pub fn inventory(&self, n: usize) -> &AdmittedInventory {
        &self.inventories[n - 1]
    }
}

/// The k-th decision of a committed update.
pub fn committed(n: usize, k: usize) -> IdentityDecision {
    serde_json::from_value(artifact(&format!("update-{n}"))["body"]["decisions"][k].clone())
        .unwrap()
}

pub fn id_of(n: usize, k: usize) -> MessageIntentId {
    committed(n, k).intent_id().clone()
}

/// The one edit a committed update's verified continuations carry.
pub fn the_edit(n: usize) -> SourceEdit {
    let IdentityDecision::Continue(continuation) = committed(n, 0) else {
        panic!("the first decision of update-{n} continues");
    };
    let ContinuationBasis::VerifiedEdit(edit) = continuation.basis() else {
        panic!("a verified edit");
    };
    edit.changes()[0].clone()
}

/// Every retained source of the committed chain, with its text.
pub fn sources() -> Vec<(SourceSnapshot, String)> {
    let document = document();
    let mut sources: Vec<(SourceSnapshot, String)> = Vec::new();
    for n in 1..=3 {
        for unit in artifact(&format!("inventory-{n}"))["body"]["units"]
            .as_array()
            .unwrap()
        {
            let snapshot: SourceSnapshot = serde_json::from_value(unit["source"].clone()).unwrap();
            if sources.iter().any(|(known, _)| *known == snapshot) {
                continue;
            }
            let text = document["sources"]
                .as_array()
                .unwrap()
                .iter()
                .find(|source| {
                    source["unit"].as_str() == Some(snapshot.unit().as_str())
                        && source["revision"].as_str() == Some(snapshot.revision().as_str())
                })
                .and_then(|source| source["text"].as_str())
                .unwrap()
                .to_owned();
            sources.push((snapshot, text));
        }
    }
    sources
}

/// What a case hands reconciliation besides the chain's own evidence.
#[derive(Default)]
pub struct Case<'c> {
    pub edits: &'c [SourceEdit],
    pub explicit: &'c [ExplicitDecision],
    pub candidates: &'c [MessageIntentId],
}

/// Reconcile inventory `current` against registry `base` with the chain's
/// sources, both units as members, and the update that produced the base.
pub fn reconciled(
    chain: &Chain,
    (base, current): (usize, usize),
    case: &Case<'_>,
) -> Result<Reconciliation, ReconcileFailure> {
    let owned = sources();
    let retained = RetainedSources::new(
        owned
            .iter()
            .map(|(snapshot, text)| (snapshot.clone(), text.as_bytes())),
    )
    .unwrap();
    let membership = [Token::new("checkout").unwrap(), Token::new("nav").unwrap()];
    let inputs = ReconcileInputs {
        evidence: ContinuityInputs {
            sources: &retained,
            edits: case.edits,
            membership: &membership,
            previous: (base > 0).then(|| PreviousUpdate {
                update: chain.update(base),
                inventory: chain.inventory(base),
            }),
        },
        explicit: case.explicit,
        links: &[],
        candidates: case.candidates,
    };
    reconcile(
        chain.registry(base),
        chain.inventory(current),
        &inputs,
        &identity_limits(),
        &mut IdentityWorkspace::new(),
    )
}

/// The explicit restore of update 3, bound to registry-2 and inventory-3.
pub fn restore(chain: &Chain) -> ExplicitDecision {
    ExplicitDecision::new(
        chain.registry(2).reference(),
        chain.inventory(3).reference(),
        committed(3, 1),
    )
    .unwrap()
}

pub fn principal(id: &str) -> Principal {
    TestAuthority::principal(id).unwrap()
}

pub fn chain_destination() -> Destination {
    TestAuthority::destination(
        "storefront-registry",
        owner(),
        "storefront-web",
        Some(RegistryIdentity::retained(REGISTRY).unwrap()),
    )
    .unwrap()
}

/// A test-owned authority over the chain for updating to inventory `n`:
/// its units acquired and the registry before it accepted as the anchor,
/// with each named principal holding its actions.
pub fn authority_over(
    chain: &Chain,
    n: usize,
    grants: &[(&str, &[Action])],
    development: bool,
) -> LocalAuthority {
    let mut setup = TestAuthority::new(chain_destination(), basis());
    for (name, actions) in grants {
        setup = setup.grant(&principal(name), actions.iter().copied());
    }
    for unit in chain.inventory(n).inventory().units() {
        setup = setup.acquired(UnitResult::source(unit).clone());
    }
    setup = setup.anchor(chain.registry(n - 1).reference());
    if development {
        setup = setup.development_session();
    }
    setup
        .establish(&limits())
        .expect("an established authority")
}
