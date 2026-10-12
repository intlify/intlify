// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Draft 7 generation for the representations this crate owns.
//!
//! Design 017 asks the adopting implementation to materialize closed schemas
//! for the conformance value and the manifest. They are committed and checked,
//! not inferred at decode time, and nothing here fetches or executes a schema.
//! A schema states the closed shape; the canonical order 017 fixes is checked
//! by the reader, because Draft 7 cannot express it.

use schemars::{generate::SchemaSettings, JsonSchema};
use serde_json::Value;

use crate::conformance::LocaleConformanceData;

/// One committed schema: its generator and where it lives in this crate.
pub struct CommittedSchema {
    /// The path of the committed artifact, relative to the crate root.
    pub path: &'static str,
    /// Generate the schema the committed artifact must equal.
    pub generate: fn() -> Result<Value, serde_json::Error>,
}

/// Every schema this crate commits, in the order they are checked.
pub const COMMITTED_SCHEMAS: [CommittedSchema; 1] = [CommittedSchema {
    path: "schema/locale-conformance-data-v0.schema.json",
    generate: conformance_schema,
}];

/// Generate the complete closed schema of the conformance value.
pub fn conformance_schema() -> Result<Value, serde_json::Error> {
    draft7_schema::<LocaleConformanceData>()
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
/// the repository formatter also owns the committed file's layout.
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
    use serde_json::{json, Value};

    use super::*;
    use crate::conformance::tests::{sample, shapes};

    const COMMITTED_CONFORMANCE: &str =
        include_str!("../schema/locale-conformance-data-v0.schema.json");

    /// Every object schema in `schema`, wherever it is defined.
    fn objects(schema: &Value) -> Vec<&Value> {
        let mut found = Vec::new();
        let mut pending = vec![schema];
        while let Some(value) = pending.pop() {
            match value {
                Value::Object(members) => {
                    if members.get("type") == Some(&json!("object")) {
                        found.push(value);
                    }
                    pending.extend(members.values());
                }
                Value::Array(items) => pending.extend(items),
                _ => {}
            }
        }
        found
    }

    #[test]
    fn the_committed_schemas_match_the_current_types() {
        for schema in &COMMITTED_SCHEMAS {
            let committed: Value = match schema.path {
                "schema/locale-conformance-data-v0.schema.json" => {
                    serde_json::from_str(COMMITTED_CONFORMANCE).unwrap()
                }
                other => panic!("{other} is not read by this test"),
            };
            assert_eq!(
                (schema.generate)().unwrap(),
                committed,
                "regenerate with: cargo run -p intlify_locale_icu4x \
                 --example generate_locale_schema -- --write"
            );
        }
    }

    #[test]
    fn every_object_is_closed_and_requires_every_member() {
        let schema = conformance_schema().unwrap();
        let objects = objects(&schema);
        // The root, the group, the key, the correction, and both kinds of type.
        assert_eq!(objects.len(), 6);
        for object in objects {
            assert_eq!(object["additionalProperties"], json!(false), "{object}");
            let mut properties: Vec<_> = object["properties"].as_object().unwrap().keys().collect();
            let mut required: Vec<_> = object["required"]
                .as_array()
                .unwrap()
                .iter()
                .map(|member| member.as_str().unwrap())
                .collect();
            properties.sort_unstable();
            required.sort_unstable();
            assert_eq!(properties, required, "{object}");
        }
    }

    #[test]
    fn the_schema_refuses_every_shape_the_reader_refuses_but_not_an_order() {
        let schema = conformance_schema().unwrap();
        let validator = jsonschema::draft7::new(&schema).unwrap();
        assert!(validator.is_valid(&sample()));
        for (case, value) in shapes() {
            assert!(!validator.is_valid(&value), "{case}");
        }
        // Draft 7 cannot say "sorted", so order is the reader's alone.
        let mut unsorted = sample();
        unsorted["validity"].as_array_mut().unwrap().swap(0, 1);
        assert!(validator.is_valid(&unsorted));
    }

    #[test]
    fn generation_is_deterministic_and_the_root_has_no_generator_id() {
        let first = conformance_schema().unwrap();
        assert_eq!(
            format_schema(first.clone()).unwrap(),
            format_schema(conformance_schema().unwrap()).unwrap()
        );
        assert_eq!(first["title"], json!("LocaleConformanceData"));
        assert_eq!(
            first["$schema"],
            json!("http://json-schema.org/draft-07/schema#")
        );
        assert!(first.get("$id").is_none());
        let rendered =
            format_schema(json!({"b": {"d": 1, "c": [{"f": 2, "e": 3}]}, "a": 4})).unwrap();
        assert_eq!(
            rendered,
            "{\n  \"a\": 4,\n  \"b\": {\n    \"c\": [\n      {\n        \"e\": 3,\n        \"f\": 2\n      }\n    ],\n    \"d\": 1\n  }\n}\n"
        );
    }
}
