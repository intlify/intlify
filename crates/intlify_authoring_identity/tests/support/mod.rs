// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The committed registry chain, and the inputs the integration tests read it
//! with.
//!
//! Every case starts from an artifact of the committed vectors, which a second
//! implementation has already checked. A changed artifact is resealed the way a
//! forger with the specification would, so a check is reached rather than
//! stopped at the digest.

#![allow(dead_code, reason = "each test file uses its own subset")]

use intlify_authoring::test_context::{admit_inventory, TestContext};
use intlify_authoring::{
    AdmittedInventory, AnalysisWorkspace, AuthoringLimits, IntegrityDigest, OwnerIdentity,
    OwnerKind, SurfaceVocabulary, ARTIFACT_INTEGRITY_DOMAIN,
};
use intlify_authoring_identity::{
    admit_registry, admit_update, AdmittedRegistry, AdmittedUpdate, IdentityLimits,
};
use intlify_shared_json::encoding::{hash, Domain};
use serde_json::{json, Value};

const VECTORS: &str = include_str!("../../fixtures/phase3/registry-vectors.json");

/// The committed vectors document.
pub fn document() -> Value {
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

/// Bounds every case stays well inside.
pub fn limits() -> IdentityLimits {
    IdentityLimits {
        entries: 1024,
        decisions: 1024,
        lineage_links: 64,
        lineage_members: 256,
        source_edits: 256,
        replacements: 4096,
        replacement_bytes: 1024 * 1024,
        history_steps: 64,
    }
}

/// Bounds for admitting the vectors' inventories.
pub fn authoring_limits() -> AuthoringLimits {
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

/// The owner of the committed chain.
pub fn owner() -> OwnerIdentity {
    OwnerIdentity::new(OwnerKind::Application, "storefront").expect("checked project id")
}

/// The context the vectors' inventories were resolved against.
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

/// Encode a value as an artifact's bytes.
pub fn bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).expect("serializable")
}

/// Recompute the digest over an edited value, the way a forger with the
/// specification in hand would.
pub fn sealed(mut value: Value) -> Value {
    let object = value.as_object_mut().expect("an artifact object");
    object.remove("integrityDigest");
    let digest = hash(Domain::literal(ARTIFACT_INTEGRITY_DOMAIN), &value).expect("encodable");
    value["integrityDigest"] = json!(IntegrityDigest::from_hash(digest).as_str());
    value
}

/// Reseal an edited value and encode it.
pub fn reseal(value: Value) -> Vec<u8> {
    bytes(&sealed(value))
}

/// Admit a registry artifact that has to be admissible.
pub fn registry(value: &Value) -> AdmittedRegistry {
    admit_registry(&bytes(value), &limits()).expect("an admissible registry")
}

/// Admit an update artifact that has to be admissible.
pub fn update(value: &Value) -> AdmittedUpdate {
    admit_update(&bytes(value), &limits()).expect("an admissible update")
}

/// Admit an inventory artifact that has to be admissible, without its bytes.
///
/// Its units stay unverified against source, which no case here relies on.
pub fn inventory(value: &Value) -> AdmittedInventory {
    admit_inventory(
        &bytes(value),
        &context(),
        &[],
        &authoring_limits(),
        &mut AnalysisWorkspace::new(),
    )
    .expect("an admissible inventory")
}
