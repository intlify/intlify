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
    resolve_declarations, AdmittedInventory, AnalysisWorkspace, AuthoringArtifact,
    AuthoringContext, AuthoringLimits, ByteRange, Completeness, DeclarationFacts, DeclarationInput,
    DeclarationMetadata, InputSegment, IntegrityDigest, InventoryArtifact, InventoryBuilder,
    MessageInput, MessageIntentId, Occurrence, OccurrenceRole, OwnerIdentity, OwnerKind,
    ReferenceFacts, SourceSnapshot, SurfaceVocabulary, UnitOutcome, UnitResult, VersionedIdentity,
    ARTIFACT_INTEGRITY_DOMAIN,
};
use intlify_authoring_identity::{
    admit_registry, admit_update, apply, AdmittedRegistry, AdmittedUpdate, IdentityLimits,
    IntentRegistrySnapshot, IntentRegistryUpdate, RegistryArtifact, RegistryIdentity,
    RegistryUpdateArtifact, Transition,
};
use intlify_shared_json::encoding::{digest_bytes, hash, Domain};
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

/// One unit revision and its exact text.
pub struct Unit {
    pub name: &'static str,
    pub revision: &'static str,
    pub text: String,
}

impl Unit {
    pub fn new(name: &'static str, revision: &'static str, text: &str) -> Self {
        Self {
            name,
            revision,
            text: text.to_owned(),
        }
    }

    /// The snapshot that names exactly this text.
    pub fn snapshot(&self) -> SourceSnapshot {
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

/// One `intent(…)` call: the whole call, the quoted literal, and its content.
struct Call {
    whole: ByteRange,
    literal: ByteRange,
    content: ByteRange,
}

fn range(start: usize, end: usize) -> ByteRange {
    ByteRange::new(start as u64, end as u64).expect("ordered")
}

/// Find every `intent('…')` call in a fixture text, in source order.
fn calls(text: &str) -> Vec<Call> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(offset) = text[from..].find("intent(") {
        let start = from + offset;
        let mut quote = start + "intent(".len();
        while bytes[quote].is_ascii_whitespace() {
            quote += 1;
        }
        assert_eq!(bytes[quote], b'\'', "a quoted literal");
        let close = quote + 1 + text[quote + 1..].find('\'').expect("the literal closes");
        let end = close + 1 + text[close + 1..].find(')').expect("the call closes") + 1;
        found.push(Call {
            whole: range(start, end),
            literal: range(quote, close + 1),
            content: range(quote + 1, close),
        });
        from = end;
    }
    found
}

/// Build and admit an inventory of checked units, as a Producer would.
pub fn inventory_of(units: &[&Unit], completeness: Completeness) -> AdmittedInventory {
    inventory_with_role(units, completeness, OccurrenceRole::IntentLiteral)
}

/// Build and admit an inventory whose declarations all have one role.
pub fn inventory_with_role(
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
    inventory(&serde_json::to_value(sealed).expect("serializable"))
}

/// The declaration an inventory records for the `nth` call in a unit.
pub fn declaration(inventory: &AdmittedInventory, unit: &Unit, nth: usize) -> Occurrence {
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
pub fn id(value: &str) -> MessageIntentId {
    MessageIntentId::retained(owner(), value).expect("a fixture value")
}

/// A genesis of the committed owner and scope.
pub fn genesis(identity: &str) -> AdmittedRegistry {
    let snapshot = IntentRegistrySnapshot::genesis(
        owner(),
        "storefront-web",
        RegistryIdentity::retained(identity).expect("a fixture value"),
    )
    .expect("a checked scope");
    let sealed = RegistryArtifact::seal(snapshot).expect("sealable");
    registry(&serde_json::to_value(sealed).expect("serializable"))
}

/// Seal and admit an update.
pub fn sealed_update(plan: IntentRegistryUpdate) -> AdmittedUpdate {
    let sealed = RegistryUpdateArtifact::seal(plan).expect("sealable");
    update(&serde_json::to_value(sealed).expect("serializable"))
}

/// Apply an update that has to apply, then seal and admit its result.
pub fn applied(
    base: &AdmittedRegistry,
    plan: &AdmittedUpdate,
    inventory: &AdmittedInventory,
) -> AdmittedRegistry {
    let Ok(Transition::Applied(result)) = apply(base, plan, inventory) else {
        panic!("the update applies");
    };
    let sealed = RegistryArtifact::seal(*result).expect("sealable");
    registry(&serde_json::to_value(sealed).expect("serializable"))
}
