// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The committed registry chain, admitted, for this module's unit tests.
//!
//! The integration tests read the same file through `tests/support`. Unit
//! tests cannot share that module, so the little they need is repeated here.

use intlify_authoring::test_context::{admit_inventory, TestContext};
use intlify_authoring::{
    AdmittedInventory, AnalysisWorkspace, AuthoringLimits, OwnerIdentity, OwnerKind,
    SurfaceVocabulary,
};
use serde_json::Value;

use super::admit::{admit_registry, admit_update, AdmittedRegistry, AdmittedUpdate};
use crate::limits::IdentityLimits;

const VECTORS: &str = include_str!("../../fixtures/phase3/registry-vectors.json");

/// One committed artifact, by its vector id.
pub(crate) fn artifact(id: &str) -> Value {
    let document: Value = serde_json::from_str(VECTORS).expect("committed vectors");
    document["artifacts"]
        .as_array()
        .expect("an artifact list")
        .iter()
        .find(|vector| vector["id"] == id)
        .unwrap_or_else(|| panic!("no vector {id}"))["artifact"]
        .clone()
}

/// Every retained source of the committed chain: each unit revision's
/// snapshot, as the inventories record it, with its text.
pub(crate) fn sources() -> Vec<(intlify_authoring::SourceSnapshot, String)> {
    let document: Value = serde_json::from_str(VECTORS).expect("committed vectors");
    let mut sources: Vec<(intlify_authoring::SourceSnapshot, String)> = Vec::new();
    for n in 1..=3 {
        for unit in artifact(&format!("inventory-{n}"))["body"]["units"]
            .as_array()
            .expect("units")
        {
            let snapshot: intlify_authoring::SourceSnapshot =
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

/// Bounds every case stays well inside.
pub(crate) fn limits() -> IdentityLimits {
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

/// The owner of the committed chain.
pub(crate) fn owner() -> OwnerIdentity {
    OwnerIdentity::new(OwnerKind::Application, "storefront").expect("checked project id")
}

fn bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).expect("serializable")
}

/// Admit an inventory of the committed owner, without its bytes.
pub(crate) fn admitted_inventory(value: &Value) -> AdmittedInventory {
    let context = TestContext::builder(
        owner(),
        SurfaceVocabulary::new(["checkout", "nav"]).expect("a vocabulary"),
    )
    .default_source_locale("en")
    .default_surface_class("checkout")
    .build()
    .expect("checked test context");
    let limits = AuthoringLimits {
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
    .expect("satisfiable bounds");
    admit_inventory(
        &bytes(value),
        &context,
        &[],
        &limits,
        &mut AnalysisWorkspace::new(),
    )
    .expect("an admissible inventory")
}

/// The committed chain: registry-0 to registry-3, update-1 to update-3 and
/// inventory-1 to inventory-3, admitted.
pub(crate) struct Chain {
    pub(crate) registries: Vec<AdmittedRegistry>,
    pub(crate) updates: Vec<AdmittedUpdate>,
    pub(crate) inventories: Vec<AdmittedInventory>,
}

impl Chain {
    pub(crate) fn load() -> Self {
        Self {
            registries: (0..=3)
                .map(|n| {
                    admit_registry(&bytes(&artifact(&format!("registry-{n}"))), &limits())
                        .expect("an admissible registry")
                })
                .collect(),
            updates: (1..=3)
                .map(|n| {
                    admit_update(&bytes(&artifact(&format!("update-{n}"))), &limits())
                        .expect("an admissible update")
                })
                .collect(),
            inventories: (1..=3)
                .map(|n| admitted_inventory(&artifact(&format!("inventory-{n}"))))
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
