// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Draft 7 generation for the representations this crate owns.
//!
//! The schema is a version-controlled companion to design 017, not a consumer
//! selected plugin schema or a distribution URL. Every implementation admitting
//! the projection specification pins the same complete schema, so the generated
//! artifact is committed and checked rather than inferred at decode time.

use schemars::{generate::SchemaSettings, JsonSchema};
use serde_json::Value;

use crate::inventory::InventoryArtifact;
use crate::projection::IntentProjection;

/// One committed schema: its generator and where it lives in this crate.
pub struct CommittedSchema {
    /// The path of the committed artifact, relative to the crate root.
    pub path: &'static str,
    /// Generate the schema the committed artifact must equal.
    pub generate: fn() -> Result<Value, serde_json::Error>,
}

/// Every schema this crate commits, in the order they are checked.
pub const COMMITTED_SCHEMAS: [CommittedSchema; 2] = [
    CommittedSchema {
        path: "schema/intent-projection-v0.schema.json",
        generate: intent_projection_schema,
    },
    CommittedSchema {
        path: "schema/authoring-inventory-v0.schema.json",
        generate: authoring_inventory_schema,
    },
];

/// Generate the complete closed schema of the semantic projection.
pub fn intent_projection_schema() -> Result<Value, serde_json::Error> {
    draft7_schema::<IntentProjection>()
}

/// Generate the complete closed schema of a sealed `authoring-inventory`.
///
/// This is the whole artifact, envelope included, because the envelope is
/// where the kind, schema revision and specification are pinned.
pub fn authoring_inventory_schema() -> Result<Value, serde_json::Error> {
    draft7_schema::<InventoryArtifact>()
}

/// Generate one model's Draft 7 schema without the generator-only root `$id`.
pub fn draft7_schema<T: JsonSchema>() -> Result<Value, serde_json::Error> {
    let root = SchemaSettings::draft07()
        .into_generator()
        .into_root_schema_for::<T>();
    let mut value = serde_json::to_value(root)?;
    if let Some(object) = value.as_object_mut() {
        object.remove("$id");
    }
    Ok(value)
}

/// Produce stable bytes for the committed artifact.
///
/// Members are sorted by unsigned UTF-8 bytes so regeneration is deterministic.
/// Freshness is checked by comparing decoded values rather than bytes, because
/// the repository formatter also owns the committed file's layout and the two
/// must not fight over it.
pub fn format_schema(mut value: Value) -> Result<String, serde_json::Error> {
    sort_object_members(&mut value);
    let mut output = serde_json::to_string_pretty(&value)?;
    output.push('\n');
    Ok(output)
}

fn sort_object_members(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for member in object.values_mut() {
                sort_object_members(member);
            }
            let mut members = std::mem::take(object).into_iter().collect::<Vec<_>>();
            members.sort_by(|(left, _), (right, _)| left.as_bytes().cmp(right.as_bytes()));
            object.extend(members);
        }
        Value::Array(values) => {
            for member in values {
                sort_object_members(member);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMITTED: &str = include_str!("../schema/intent-projection-v0.schema.json");
    const COMMITTED_INVENTORY: &str = include_str!("../schema/authoring-inventory-v0.schema.json");

    #[test]
    fn the_committed_schema_matches_the_current_projection() {
        let generated = intent_projection_schema().unwrap();
        let committed: Value = serde_json::from_str(COMMITTED).unwrap();
        assert_eq!(
            generated, committed,
            "regenerate with: cargo run -p intlify_authoring \
             --example generate_authoring_schema -- --write"
        );
    }

    #[test]
    fn the_committed_inventory_schema_matches_the_current_artifact() {
        let generated = authoring_inventory_schema().unwrap();
        let committed: Value = serde_json::from_str(COMMITTED_INVENTORY).unwrap();
        assert_eq!(
            generated, committed,
            "regenerate with: cargo run -p intlify_authoring \
             --example generate_authoring_schema -- --write"
        );
    }

    #[test]
    fn the_inventory_envelope_pins_one_kind_revision_and_specification() {
        // A schema that listed all five registered kinds would admit a
        // message-intent envelope around an inventory body, which the reader
        // refuses. Each pin is a single value for that reason.
        let schema = authoring_inventory_schema().unwrap();
        let definitions = &schema["definitions"];
        assert_eq!(
            definitions["InventoryKind"]["enum"],
            serde_json::json!(["authoring-inventory"])
        );
        assert_eq!(
            definitions["RevisionZero"]["enum"],
            serde_json::json!(["0"])
        );
        assert_eq!(
            definitions["Design016"]["enum"],
            serde_json::json!(["intlify-design-016"])
        );
        assert_eq!(schema["additionalProperties"], serde_json::json!(false));
    }

    #[test]
    fn generation_is_deterministic_and_the_root_is_closed() {
        let first = intent_projection_schema().unwrap();
        assert_eq!(
            format_schema(first.clone()).unwrap(),
            format_schema(intent_projection_schema().unwrap()).unwrap()
        );
        assert_eq!(first["additionalProperties"], serde_json::json!(false));
        assert_eq!(first["title"], serde_json::json!("IntentProjection"));
        assert!(
            first.get("$id").is_none(),
            "generator-only root id must be removed"
        );
        // Every member of the projection is part of what a revision means, so
        // the root requires all of the non-optional ones.
        let required = first["required"].as_array().unwrap();
        for member in ["mf2Specification", "message", "sourceLocale", "parameters"] {
            assert!(
                required.iter().any(|value| value == member),
                "{member} must be required"
            );
        }
        for optional in ["usage", "description"] {
            assert!(!required.iter().any(|value| value == optional));
        }
    }

    #[test]
    fn the_schema_admits_a_real_projection_and_rejects_a_damaged_one() {
        let schema = intent_projection_schema().unwrap();
        let validator = jsonschema::draft7::new(&schema).unwrap();

        let projection = crate::projection::intent_projection(
            crate::projection::MessageProjection {
                declarations: Box::new([]),
                body: crate::projection::MessageBody::Pattern(crate::projection::PatternBody {
                    kind: crate::projection::PatternTag::Value,
                    parts: Box::new([crate::projection::PatternPart::Text(
                        crate::projection::TextPart {
                            kind: crate::projection::TextTag::Value,
                            value: "Pay now".into(),
                        },
                    )]),
                }),
            },
            Box::new([]),
            "en",
            None,
            None,
        )
        .unwrap();
        let mut value = serde_json::to_value(&projection).unwrap();
        assert!(
            validator.is_valid(&value),
            "a real projection must validate"
        );

        for damage in 0..4 {
            let mut invalid = value.clone();
            match damage {
                0 => invalid["sourceLocale"] = serde_json::json!(""),
                1 => invalid["extra"] = serde_json::json!(true),
                2 => {
                    invalid.as_object_mut().unwrap().remove("parameters");
                }
                _ => invalid["message"]["body"]["kind"] = serde_json::json!("unknown"),
            }
            assert!(
                !validator.is_valid(&invalid),
                "damage {damage} was admitted"
            );
        }
        value["usage"] = serde_json::json!({"profile": {"identity": "p", "revision": "0"}, "value": "button-label"});
        assert!(validator.is_valid(&value));
    }
}
