// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use serde_json::{json, Value};

use super::*;
use crate::conformance::tests::{sample, shapes};

const COMMITTED_CONFORMANCE: &str =
    include_str!("../../schema/locale-conformance-data-v0.schema.json");

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
    let rendered = format_schema(json!({"b": {"d": 1, "c": [{"f": 2, "e": 3}]}, "a": 4})).unwrap();
    assert_eq!(
        rendered,
        "{\n  \"a\": 4,\n  \"b\": {\n    \"c\": [\n      {\n        \"e\": 3,\n        \"f\": 2\n      }\n    ],\n    \"d\": 1\n  }\n}\n"
    );
}
