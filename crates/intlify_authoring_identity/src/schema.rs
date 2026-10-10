// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Draft 7 generation for the four authoring kinds this crate owns.
//!
//! Like the inventory schema, each is a version-controlled companion to design
//! 017, committed and checked rather than inferred at decode time. The
//! generation itself is shared with `intlify_authoring`, so every authoring
//! kind's schema is made the same way.

use intlify_authoring::schema::{draft7_schema, CommittedSchema};
use intlify_authoring::{
    ArtifactKind, AuthoringArtifactReference, ARTIFACT_SCHEMA_REVISION,
    AUTHORING_SPECIFICATION_IDENTITY, AUTHORING_SPECIFICATION_REVISION,
};
use schemars::{json_schema, Schema, SchemaGenerator};
use serde_json::Value;

use crate::intent::{MessageIntentArtifact, MessageReferenceArtifact};
use crate::registry::{RegistryArtifact, RegistryUpdateArtifact};

/// Every schema this crate commits, in the order they are checked.
pub const COMMITTED_SCHEMAS: [CommittedSchema; 4] = [
    CommittedSchema {
        path: "schema/intent-registry-v0.schema.json",
        generate: intent_registry_schema,
    },
    CommittedSchema {
        path: "schema/intent-registry-update-v0.schema.json",
        generate: intent_registry_update_schema,
    },
    CommittedSchema {
        path: "schema/message-intent-v0.schema.json",
        generate: message_intent_schema,
    },
    CommittedSchema {
        path: "schema/message-reference-v0.schema.json",
        generate: message_reference_schema,
    },
];

/// Generate the complete closed schema of a sealed `intent-registry`.
pub fn intent_registry_schema() -> Result<Value, serde_json::Error> {
    draft7_schema::<RegistryArtifact>()
}

/// Generate the complete closed schema of a sealed `intent-registry-update`.
pub fn intent_registry_update_schema() -> Result<Value, serde_json::Error> {
    draft7_schema::<RegistryUpdateArtifact>()
}

/// Generate the complete closed schema of a sealed `message-intent`.
pub fn message_intent_schema() -> Result<Value, serde_json::Error> {
    draft7_schema::<MessageIntentArtifact>()
}

/// Generate the complete closed schema of a sealed `message-reference`.
pub fn message_reference_schema() -> Result<Value, serde_json::Error> {
    draft7_schema::<MessageReferenceArtifact>()
}

// A body holds every reference as the shared reference type, which names any
// registered kind under any revision, because that is how 017 writes it and how
// a reference is compared with the artifact it names. Each member still has to
// name one kind under the one tuple this reader implements, and the reader
// refuses anything else by name. These narrow the schema to the same set, so
// it is no looser than the reader.

pub(crate) fn registry_reference(generator: &mut SchemaGenerator) -> Schema {
    reference_to(generator, ArtifactKind::IntentRegistry)
}

pub(crate) fn inventory_reference(generator: &mut SchemaGenerator) -> Schema {
    reference_to(generator, ArtifactKind::AuthoringInventory)
}

pub(crate) fn intent_reference(generator: &mut SchemaGenerator) -> Schema {
    reference_to(generator, ArtifactKind::MessageIntent)
}

pub(crate) fn optional_registry_reference(generator: &mut SchemaGenerator) -> Schema {
    optional(&reference_to(generator, ArtifactKind::IntentRegistry))
}

pub(crate) fn optional_update_reference(generator: &mut SchemaGenerator) -> Schema {
    optional(&reference_to(generator, ArtifactKind::IntentRegistryUpdate))
}

fn reference_to(generator: &mut SchemaGenerator, kind: ArtifactKind) -> Schema {
    let reference = generator.subschema_for::<AuthoringArtifactReference>();
    json_schema!({
        "allOf": [
            reference,
            {
                "properties": {
                    "kind": { "enum": [kind.as_str()] },
                    "schemaRevision": { "enum": [ARTIFACT_SCHEMA_REVISION] },
                    "authoringSpecification": {
                        "properties": {
                            "identity": { "enum": [AUTHORING_SPECIFICATION_IDENTITY] },
                            "revision": { "enum": [AUTHORING_SPECIFICATION_REVISION] }
                        }
                    }
                }
            }
        ]
    })
}

// The same reading of an absent member as every other optional member in the
// committed authoring schemas: absent or null.
fn optional(schema: &Schema) -> Schema {
    json_schema!({ "anyOf": [schema, { "type": "null" }] })
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMITTED_REGISTRY: &str = include_str!("../schema/intent-registry-v0.schema.json");
    const COMMITTED_UPDATE: &str = include_str!("../schema/intent-registry-update-v0.schema.json");
    const COMMITTED_INTENT: &str = include_str!("../schema/message-intent-v0.schema.json");
    const COMMITTED_REFERENCE: &str = include_str!("../schema/message-reference-v0.schema.json");

    #[test]
    fn the_committed_schemas_match_the_current_artifacts() {
        for (generated, committed) in [
            (intent_registry_schema(), COMMITTED_REGISTRY),
            (intent_registry_update_schema(), COMMITTED_UPDATE),
            (message_intent_schema(), COMMITTED_INTENT),
            (message_reference_schema(), COMMITTED_REFERENCE),
        ] {
            let committed: Value = serde_json::from_str(committed).unwrap();
            assert_eq!(
                generated.unwrap(),
                committed,
                "regenerate with: cargo run -p intlify_authoring_identity \
                 --example generate_identity_schema -- --write"
            );
        }
    }

    #[test]
    fn each_envelope_pins_its_kind_and_each_reference_member_one_kind() {
        let registry = intent_registry_schema().unwrap();
        let update = intent_registry_update_schema().unwrap();
        assert_eq!(
            registry["definitions"]["RegistryKind"]["enum"],
            serde_json::json!(["intent-registry"])
        );
        assert_eq!(
            update["definitions"]["RegistryUpdateKind"]["enum"],
            serde_json::json!(["intent-registry-update"])
        );
        for schema in [&registry, &update] {
            assert_eq!(schema["additionalProperties"], serde_json::json!(false));
            assert!(schema.get("$id").is_none());
        }

        let named = |member: &Value| member["allOf"][1]["properties"]["kind"]["enum"].clone();
        let snapshot = &registry["definitions"]["IntentRegistrySnapshot"]["properties"];
        assert_eq!(
            named(&snapshot["base"]["anyOf"][0]),
            serde_json::json!(["intent-registry"])
        );
        assert_eq!(
            named(&snapshot["update"]["anyOf"][0]),
            serde_json::json!(["intent-registry-update"])
        );
        let plan = &update["definitions"]["IntentRegistryUpdate"]["properties"];
        assert_eq!(named(&plan["base"]), serde_json::json!(["intent-registry"]));
        assert_eq!(
            named(&plan["inventory"]),
            serde_json::json!(["authoring-inventory"])
        );
    }

    #[test]
    fn the_intent_and_reference_envelopes_pin_their_kinds_and_references() {
        let intent = message_intent_schema().unwrap();
        let reference = message_reference_schema().unwrap();
        assert_eq!(
            intent["definitions"]["MessageIntentKind"]["enum"],
            serde_json::json!(["message-intent"])
        );
        assert_eq!(
            reference["definitions"]["MessageReferenceKind"]["enum"],
            serde_json::json!(["message-reference"])
        );
        for schema in [&intent, &reference] {
            assert_eq!(schema["additionalProperties"], serde_json::json!(false));
            assert!(schema.get("$id").is_none());
        }

        let named = |member: &Value| member["allOf"][1]["properties"]["kind"]["enum"].clone();
        let body = &intent["definitions"]["MessageIntentBody"];
        assert_eq!(
            named(&body["properties"]["inventory"]),
            serde_json::json!(["authoring-inventory"])
        );
        assert_eq!(
            named(&body["properties"]["registry"]),
            serde_json::json!(["intent-registry"])
        );
        // The continuity is optional, and read absent or null alike.
        assert_eq!(
            body["properties"]["continuity"]["anyOf"][1],
            serde_json::json!({ "type": "null" })
        );
        assert!(!body["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("continuity")));
        let uses = &reference["definitions"]["MessageReferenceBody"]["properties"];
        assert_eq!(
            named(&uses["inventory"]),
            serde_json::json!(["authoring-inventory"])
        );
        let target = &reference["definitions"]["ReferenceTarget"]["properties"];
        assert_eq!(
            named(&target["intentArtifact"]),
            serde_json::json!(["message-intent"])
        );
    }
}
