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
use crate::manifest::LocaleCanonicalizationDataArtifact;

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
        path: "schema/locale-canonicalization-data-v0.schema.json",
        generate: manifest_schema,
    },
    CommittedSchema {
        path: "schema/locale-conformance-data-v0.schema.json",
        generate: conformance_schema,
    },
];

/// Generate the complete closed schema of the artifact manifest.
pub fn manifest_schema() -> Result<Value, serde_json::Error> {
    draft7_schema::<LocaleCanonicalizationDataArtifact>()
}

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
    const COMMITTED_MANIFEST: &str =
        include_str!("../schema/locale-canonicalization-data-v0.schema.json");

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
                "schema/locale-canonicalization-data-v0.schema.json" => {
                    serde_json::from_str(COMMITTED_MANIFEST).unwrap()
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
        // The conformance value: the root, the group, the key, the
        // correction, and both kinds of type. The manifest: the root, the
        // dataset, the generation, the marker data, CLDR, a source, a
        // payload, and a versioned identity.
        for (schema, count) in [
            (conformance_schema().unwrap(), 6),
            (manifest_schema().unwrap(), 8),
        ] {
            let objects = objects(&schema);
            assert_eq!(objects.len(), count, "{}", schema["title"]);
            closed(objects);
        }
    }

    fn closed(objects: Vec<&Value>) {
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

    #[test]
    fn the_manifest_schema_admits_the_fixtures_and_pins_one_kind_and_revision() {
        let schema = manifest_schema().unwrap();
        let validator = jsonschema::draft7::new(&schema).unwrap();
        for bytes in [
            include_bytes!("../fixtures/artifact/manifest-narrow.json").as_slice(),
            include_bytes!("../fixtures/artifact/manifest-wide.json"),
            include_bytes!("../fixtures/artifact/manifest-vendored.json"),
        ] {
            assert!(validator.is_valid(&serde_json::from_slice(bytes).unwrap()));
        }
        let definitions = &schema["definitions"];
        assert_eq!(
            definitions["ArtifactKind"]["enum"],
            json!(["locale-canonicalization-data"])
        );
        assert_eq!(definitions["RevisionZero"]["enum"], json!(["0"]));

        let sample = crate::manifest::tests::sample();
        for (pointer, value) in [
            ("/kind", json!("locale-canonicalization-dataset")),
            ("/schemaRevision", json!("1")),
            ("/payloads/0/byteLength", json!(1521)),
            ("/payloads/0/byteLength", json!("01521")),
            ("/payloads/0/address", json!("../conformance.json")),
            ("/payloads/0/role", json!("dataset")),
            ("/specification/identity", json!("Intlify")),
            ("/dataset/semanticDigest", json!("sha256:00")),
            ("/generation/markers/0", json!("")),
        ] {
            let mut edited = sample.clone();
            *edited.pointer_mut(pointer).unwrap() = value;
            assert!(!validator.is_valid(&edited), "{pointer}");
        }
        let mut unknown = sample.clone();
        unknown["generation"]["cldr"]["comment"] = json!("x");
        assert!(!validator.is_valid(&unknown));
        // The roles' order and the sorted lists are the reader's alone.
        let mut reversed = sample;
        reversed["payloads"].as_array_mut().unwrap().reverse();
        assert!(validator.is_valid(&reversed));
    }
}
