// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Unit-test fixtures: the committed registry chain, admitted, and
//! inventories built from source text the way a Producer builds them.
//!
//! The integration tests read the same file and build the same inventories
//! through `tests/support`. Unit tests cannot share that module, so what they
//! need is repeated here.

use intlify_authoring::test_context::{admit_inventory, TestContext};
use intlify_authoring::{
    resolve_declarations, AdmittedInventory, AnalysisWorkspace, AuthoringArtifact,
    AuthoringContext, AuthoringLimits, ByteRange, Completeness, DeclarationFacts, DeclarationInput,
    DeclarationMetadata, InputSegment, IntegrityDigest, InventoryArtifact, InventoryBuilder,
    MessageInput, MessageIntentId, Occurrence, OccurrenceRole, OwnerIdentity, OwnerKind,
    ReferenceFacts, SourceSnapshot, SurfaceVocabulary, UnitOutcome, UnitResult, VersionedIdentity,
};
use intlify_shared_json::encoding::digest_bytes;
use serde_json::Value;

use super::admit::{admit_registry, admit_update, AdmittedRegistry, AdmittedUpdate};
use super::apply::{apply, Transition};
use super::artifact::{RegistryArtifact, RegistryUpdateArtifact};
use super::snapshot::IntentRegistrySnapshot;
use super::update::{AllocationBasis, IdentityDecision, IntentRegistryUpdate};
use crate::continuity::PreviousUpdate;
use crate::id::RegistryIdentity;
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
        candidates: 256,
        diagnostics: 256,
    }
}

/// The owner of the committed chain.
pub(crate) fn owner() -> OwnerIdentity {
    OwnerIdentity::new(OwnerKind::Application, "storefront").expect("checked project id")
}

fn bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).expect("serializable")
}

/// The context the committed inventories were resolved against.
fn context() -> TestContext {
    TestContext::builder(
        owner(),
        SurfaceVocabulary::new(["checkout", "nav"]).expect("a vocabulary"),
    )
    .default_source_locale("en")
    .default_surface_class("checkout")
    .build()
    .expect("checked test context")
}

/// Bounds for resolving and admitting inventories.
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

/// Admit an inventory of the committed owner, without its bytes.
pub(crate) fn admitted_inventory(value: &Value) -> AdmittedInventory {
    admit_inventory(
        &bytes(value),
        &context(),
        &[],
        &authoring_limits(),
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

/// One source unit at one revision, as text.
pub(crate) struct Unit {
    pub(crate) name: &'static str,
    pub(crate) revision: &'static str,
    pub(crate) text: String,
}

impl Unit {
    pub(crate) fn new(name: &'static str, revision: &'static str, text: &str) -> Self {
        Self {
            name,
            revision,
            text: text.to_owned(),
        }
    }

    /// The snapshot that names exactly this text.
    pub(crate) fn snapshot(&self) -> SourceSnapshot {
        SourceSnapshot::new(
            owner(),
            self.name,
            self.revision,
            VersionedIdentity::literal("intlify-grammar-js-module", "0"),
            self.text.len() as u64,
            IntegrityDigest::from_hash(digest_bytes(self.text.as_bytes())).as_str(),
        )
        .expect("a checked snapshot")
    }
}

/// One `intent('…')` call: the whole call, the quoted literal, and its
/// content.
struct Call {
    whole: ByteRange,
    literal: ByteRange,
    content: ByteRange,
}

fn span(start: usize, end: usize) -> ByteRange {
    ByteRange::new(start as u64, end as u64).expect("ordered")
}

/// Find every `intent('…')` call in a fixture text, in source order.
fn calls(text: &str) -> Vec<Call> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(offset) = text[from..].find("intent('") {
        let start = from + offset;
        let quote = start + "intent(".len();
        let close = quote + 1 + text[quote + 1..].find('\'').expect("the literal closes");
        let end = close + 1 + text[close + 1..].find(')').expect("the call closes") + 1;
        found.push(Call {
            whole: span(start, end),
            literal: span(quote, close + 1),
            content: span(quote + 1, close),
        });
        from = end;
    }
    found
}

/// Build and admit an inventory of checked units, as a Producer would.
pub(crate) fn inventory_of(units: &[&Unit], completeness: Completeness) -> AdmittedInventory {
    inventory_with_role(units, completeness, OccurrenceRole::IntentLiteral)
}

/// Build and admit an inventory whose declarations all have one role.
pub(crate) fn inventory_with_role(
    units: &[&Unit],
    completeness: Completeness,
    role: OccurrenceRole,
) -> AdmittedInventory {
    let context = context();
    let mut occurrences = Vec::new();
    let mut maps = Vec::new();
    let mut messages = Vec::new();
    let mut references = Vec::new();
    for unit in units {
        let snapshot = unit.snapshot();
        for call in calls(&unit.text) {
            let literal =
                Occurrence::new(snapshot.clone(), call.literal, role).expect("inside the unit");
            let content = &unit.text[call.content.start() as usize..call.content.end() as usize];
            maps.push([InputSegment::new(
                ByteRange::new(0, content.len() as u64).expect("ordered"),
                call.content,
            )]);
            messages.push(content.to_owned());
            references.push(ReferenceFacts::new(
                Occurrence::new(snapshot.clone(), call.whole, OccurrenceRole::Reference)
                    .expect("inside the unit"),
                vec![literal.clone()],
                vec![],
            ));
            occurrences.push(literal);
        }
    }
    let inputs: Vec<DeclarationInput<'_>> = occurrences
        .iter()
        .zip(&maps)
        .zip(&messages)
        .map(|((occurrence, map), message)| DeclarationInput {
            occurrence: occurrence.clone(),
            message: MessageInput::Mf2(message),
            input_map: Some(map),
            metadata: DeclarationMetadata::default(),
            usage: None,
            parameters: Some(&[]),
        })
        .collect();
    let result = resolve_declarations(
        &context,
        &inputs,
        &authoring_limits(),
        &mut AnalysisWorkspace::new(),
    )
    .expect("a complete invocation");
    let mut builder = InventoryBuilder::new(
        owner(),
        "storefront-web",
        context.basis().clone(),
        completeness,
    )
    .expect("a checked scope");
    for unit in units {
        builder.unit(UnitResult::new(unit.snapshot(), UnitOutcome::Checked));
    }
    builder.declarations(result.checked().expect("checked").to_vec());
    for reference in references {
        builder.reference(reference);
    }
    let sealed = InventoryArtifact::seal(builder.finish().expect("well formed")).expect("sealable");
    admitted_inventory(&serde_json::to_value(sealed).expect("serializable"))
}

/// The declaration an inventory records for the `nth` call in a unit.
pub(crate) fn declaration(inventory: &AdmittedInventory, unit: &Unit, nth: usize) -> Occurrence {
    let literal = calls(&unit.text)[nth].literal;
    let snapshot = unit.snapshot();
    inventory
        .inventory()
        .declarations()
        .iter()
        .map(DeclarationFacts::occurrence)
        .find(|occurrence| *occurrence.source() == snapshot && occurrence.range() == literal)
        .expect("the inventory records the call")
        .clone()
}

/// An Intent ID of the committed owner.
pub(crate) fn id(value: &str) -> MessageIntentId {
    MessageIntentId::retained(owner(), value).expect("a fixture value")
}

/// A genesis of the committed owner and scope.
pub(crate) fn genesis(identity: &str) -> AdmittedRegistry {
    seal_genesis(identity, "storefront-web")
}

/// A genesis of the committed owner and another scope.
pub(crate) fn genesis_of_scope(scope: &str) -> AdmittedRegistry {
    seal_genesis("fedcba9876543210fedcba9876543210", scope)
}

fn seal_genesis(identity: &str, scope: &str) -> AdmittedRegistry {
    let snapshot = IntentRegistrySnapshot::genesis(
        owner(),
        scope,
        RegistryIdentity::retained(identity).expect("a fixture value"),
    )
    .expect("a checked scope");
    let sealed = RegistryArtifact::seal(snapshot).expect("sealable");
    admit_registry(
        &bytes(&serde_json::to_value(sealed).expect("serializable")),
        &limits(),
    )
    .expect("an admissible registry")
}

/// Seal and admit an update.
pub(crate) fn sealed(plan: IntentRegistryUpdate) -> AdmittedUpdate {
    let sealed = RegistryUpdateArtifact::seal(plan).expect("sealable");
    admit_update(
        &bytes(&serde_json::to_value(sealed).expect("serializable")),
        &limits(),
    )
    .expect("an admissible update")
}

/// Apply an update that has to apply, then seal and admit its result.
pub(crate) fn applied(
    base: &AdmittedRegistry,
    plan: &AdmittedUpdate,
    inventory: &AdmittedInventory,
) -> AdmittedRegistry {
    let Ok(Transition::Applied(result)) = apply(base, plan, inventory) else {
        panic!("the update applies");
    };
    let sealed = RegistryArtifact::seal(*result).expect("sealable");
    admit_registry(
        &bytes(&serde_json::to_value(sealed).expect("serializable")),
        &limits(),
    )
    .expect("an admissible registry")
}

/// A base with history: every declaration of some units allocated from a
/// genesis, with the update and inventory that produced it.
pub(crate) struct Base {
    pub(crate) registry: AdmittedRegistry,
    pub(crate) update: AdmittedUpdate,
    pub(crate) inventory: AdmittedInventory,
}

impl Base {
    /// Allocate the given IDs to the units' declarations, in canonical order.
    pub(crate) fn of(units: &[&Unit], ids: &[&str]) -> Self {
        let root = genesis("0123456789abcdef0123456789abcdef");
        let inventory = inventory_of(units, Completeness::Complete);
        let allocations = inventory
            .inventory()
            .declarations()
            .iter()
            .zip(ids)
            .map(|(facts, value)| {
                IdentityDecision::allocation(
                    id(value),
                    facts.occurrence().clone(),
                    AllocationBasis::confirmed_new(),
                )
            })
            .collect();
        let update = sealed(
            IntentRegistryUpdate::new(
                owner(),
                root.reference(),
                inventory.reference(),
                allocations,
                vec![],
            )
            .expect("a well-formed update"),
        );
        let registry = applied(&root, &update, &inventory);
        Self {
            registry,
            update,
            inventory,
        }
    }

    /// The update that produced this base, for the pins.
    pub(crate) fn previous(&self) -> PreviousUpdate<'_> {
        PreviousUpdate {
            update: &self.update,
            inventory: &self.inventory,
        }
    }
}
