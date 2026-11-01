// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The validator schema vectors an independent implementation checks.
//!
//! They hold Draft 7 schema documents with what revision `"0"` reads from
//! them: the census of schema nodes and `$ref` occurrences, the nodes in
//! document order, the resources and anchors the identifiers name, and the
//! node each `$ref` names, or, for a document that is refused, its failure.
//! Each refused document has one problem. The Node checker reads every
//! document with its own walker, its own reading of RFC 3986, and
//! `decodeURIComponent`, finds its cycles on its own, and requires each
//! refused document to have exactly the problem listed, so a disagreement
//! says which of the two is wrong.

use serde_json::{json, Value};

use crate::construction::meta_schema::DRAFT_7_META_SCHEMA;
use crate::construction::validator::schema::{census, Node, Schema, SchemaFailure, DEFAULT_BASE};

/// Schema documents, each with a name: first those that are read, then those
/// refused. No member name looks like an array index, since JavaScript
/// objects do not keep the order of such names.
const DOCUMENTS: &[(&str, &str)] = &[
    (
        "identification-example",
        r##"{
            "$id": "http://example.com/root.json",
            "definitions": {
                "A": {"$id": "#foo"},
                "B": {
                    "$id": "other.json",
                    "definitions": {"X": {"$id": "#bar"}, "Y": {"$id": "t/inner.json"}}
                },
                "C": {"$id": "urn:uuid:ee564b8a-7a87-4125-8c96-e9f123d6766f"},
                "references": {"allOf": [
                    {"$ref": "http://example.com/root.json"},
                    {"$ref": "http://example.com/root.json#"},
                    {"$ref": "http://example.com/root.json#foo"},
                    {"$ref": "http://example.com/root.json#/definitions/A"},
                    {"$ref": "http://example.com/other.json"},
                    {"$ref": "http://example.com/other.json#"},
                    {"$ref": "http://example.com/root.json#/definitions/B"},
                    {"$ref": "http://example.com/other.json#bar"},
                    {"$ref": "http://example.com/other.json#/definitions/X"},
                    {"$ref": "http://example.com/root.json#/definitions/B/definitions/X"},
                    {"$ref": "http://example.com/t/inner.json"},
                    {"$ref": "http://example.com/t/inner.json#"},
                    {"$ref": "http://example.com/other.json#/definitions/Y"},
                    {"$ref": "http://example.com/root.json#/definitions/B/definitions/Y"},
                    {"$ref": "urn:uuid:ee564b8a-7a87-4125-8c96-e9f123d6766f"},
                    {"$ref": "urn:uuid:ee564b8a-7a87-4125-8c96-e9f123d6766f#"},
                    {"$ref": "http://example.com/root.json#/definitions/C"}
                ]}
            }
        }"##,
    ),
    (
        "references",
        r##"{
            "$id": "http://example.com/schemas/root.json",
            "definitions": {
                "a/b": {"type": "string"},
                "c%d": {"type": "number"},
                "e~f": {"type": "null"},
                "yes": true,
                "tuple": {"items": [{"type": "null"}, false]},
                "nested": {
                    "$id": "nested/item.json",
                    "definitions": {"inner": {"$id": "#Inner"}, "sibling": {"$ref": "#Inner"}}
                },
                "relative": {"$ref": "nested/item.json#/definitions/inner"},
                "parent": {"$ref": "../schemas/root.json#/definitions/yes"},
                "escaped": {"$ref": "#/definitions/a~1b"},
                "percent": {"$ref": "#/definitions/c%25d"},
                "tilde": {"$ref": "#/definitions/e~0f"},
                "boolean": {"$ref": "#/definitions/yes"},
                "item": {"$ref": "#/definitions/tuple/items/1"},
                "anchor": {"$ref": "nested/item.json#Inner"},
                "spelled": {"$ref": "HTTP://Example.COM/schemas/nested/item.json#%49nner"},
                "dots": {"$ref": "./nested/../root.json#/definitions/yes"},
                "root": {"$ref": "#"}
            }
        }"##,
    ),
    (
        "reference-siblings",
        r##"{
            "$id": "http://example.com/root.json",
            "definitions": {
                "a": {
                    "$ref": "#/definitions/b",
                    "$id": "other.json",
                    "$schema": "http://json-schema.org/draft-04/schema#",
                    "type": 5,
                    "definitions": {"c": {"$ref": "#/definitions/b"}}
                },
                "b": {}
            }
        }"##,
    ),
    (
        "default-base",
        r##"{
            "$id": "#top",
            "definitions": {
                "a": {"$ref": "#top"},
                "b": {"$ref": "json-schema:///#/definitions/a"},
                "c": {"$id": "item.json", "definitions": {"d": {"$ref": "json-schema:///item.json"}}}
            }
        }"##,
    ),
    (
        "document-order",
        r#"{
            "properties": {"a/b": {"items": [true, {}]}, "c~d": false},
            "not": {},
            "definitions": {"e": {"if": {}}}
        }"#,
    ),
    (
        "every-position",
        r##"{
            "$schema": "http://json-schema.org/draft-07/schema#",
            "additionalItems": {}, "additionalProperties": {}, "contains": {},
            "else": {}, "if": {}, "not": {}, "propertyNames": {}, "then": {},
            "allOf": [{}, true], "anyOf": [{}], "oneOf": [false],
            "definitions": {"a": {}}, "patternProperties": {"^b": {}}, "properties": {"c": {}},
            "items": [{"items": {}}],
            "dependencies": {"d": ["e"], "f": {}, "g": true},
            "enum": [{}], "const": {}, "default": {}, "examples": [{}],
            "x-unknown": {"$ref": "#"}, "format": "uri"
        }"##,
    ),
    (
        "in-place",
        r##"{
            "allOf": [{"$ref": "#/definitions/a"}, {"anyOf": [{"not": {"$ref": "#/definitions/b"}}]}],
            "if": {"$ref": "#/definitions/b"},
            "then": {},
            "else": {"oneOf": [true]},
            "dependencies": {"x": {"$ref": "#/definitions/a"}, "y": ["z"]},
            "definitions": {"a": {"allOf": [{"$ref": "#/definitions/b"}]}, "b": {}}
        }"##,
    ),
    (
        "moving-applicators",
        r##"{
            "properties": {"a": {"$ref": "#"}},
            "items": [{"$ref": "#"}],
            "additionalItems": {"$ref": "#"},
            "contains": {"$ref": "#"},
            "additionalProperties": {"$ref": "#"},
            "patternProperties": {"^p": {"$ref": "#"}},
            "propertyNames": {"$ref": "#"},
            "then": {"$ref": "#"},
            "else": {"$ref": "#"}
        }"##,
    ),
    ("true", "true"),
    ("false", "false"),
    ("not-an-object", "5"),
    ("an-array", "[{}]"),
    (
        "other-dialect",
        r#"{"$schema": "http://json-schema.org/draft-04/schema#"}"#,
    ),
    (
        "subschema-dialect",
        r#"{"definitions": {"a": {"$schema": "http://json-schema.org/draft-07/schema#"}}}"#,
    ),
    (
        "anchor-grammar",
        r##"{"definitions": {"a": {"$id": "#1a"}}}"##,
    ),
    (
        "identifier-fragment",
        r#"{"$id": "http://example.com/a.json#frag"}"#,
    ),
    ("identifier-grammar", r#"{"$id": "a b"}"#),
    (
        "duplicate-anchor",
        r##"{"definitions": {"a": {"$id": "#x"}, "b": {"$id": "#x"}}}"##,
    ),
    (
        "duplicate-resource",
        r#"{"definitions": {"a": {"$id": "HTTP://Example.com/x"}, "b": {"$id": "http://example.com/%78"}}}"#,
    ),
    ("type-name", r#"{"type": "integer-ish"}"#),
    ("required-names", r#"{"required": ["a", 1]}"#),
    ("length-fraction", r#"{"minLength": 1.5}"#),
    ("property-schema", r#"{"properties": {"a": 1}}"#),
    ("pattern-syntax", r#"{"pattern": "("}"#),
    ("divisor-zero", r#"{"multipleOf": 0}"#),
    ("divisor-digits", r#"{"multipleOf": 1.2345678901234567891}"#),
    (
        "missing-pointer",
        r##"{"definitions": {"a": {"$ref": "#/definitions/missing"}}}"##,
    ),
    (
        "not-a-node",
        r##"{"enum": [{}], "definitions": {"a": {"$ref": "#/enum/0"}}}"##,
    ),
    (
        "missing-resource",
        r#"{"definitions": {"a": {"$ref": "other.json"}}}"#,
    ),
    (
        "missing-anchor",
        r##"{"definitions": {"a": {"$ref": "#nope"}}}"##,
    ),
    (
        "not-utf-8",
        r##"{"definitions": {"a": {"$ref": "#/definitions/%FF"}}}"##,
    ),
    (
        "meta-schema-uri",
        r#"{"definitions": {"a": {"$ref": "http://json-schema.org/draft-07/schema#"}}}"#,
    ),
    (
        "reference-syntax",
        r#"{"definitions": {"a": {"$ref": "a b"}}}"#,
    ),
    ("self-reference", r##"{"$ref": "#"}"##),
    (
        "dependency-cycle",
        r##"{"dependencies": {"a": {"$ref": "#"}}}"##,
    ),
    (
        "two-node-cycle",
        r##"{"definitions": {"a": {"$ref": "#/definitions/b"}, "b": {"allOf": [{"$ref": "#/definitions/a"}]}}}"##,
    ),
    ("condition-cycle", r##"{"if": {}, "then": {"$ref": "#"}}"##),
];

fn failure_name(failure: SchemaFailure) -> &'static str {
    match failure {
        SchemaFailure::NotASchema => "notASchema",
        SchemaFailure::Dialect => "dialect",
        SchemaFailure::Identifier => "identifier",
        SchemaFailure::DuplicateIdentifier => "duplicateIdentifier",
        SchemaFailure::Keyword => "keyword",
        SchemaFailure::Pattern(_) => "pattern",
        SchemaFailure::Divisor(_) => "divisor",
        SchemaFailure::Reference => "reference",
        SchemaFailure::Cycle => "cycle",
    }
}

/// What a schema reads to: its nodes, the names of its resources and
/// anchors, and the node each `$ref` names, by their JSON Pointers.
fn reading(schema: &Schema) -> Value {
    let nodes: Vec<&str> = (0..schema.node_count())
        .map(|id| schema.location(id))
        .collect();
    let resources: Vec<Value> = schema
        .resources()
        .iter()
        .map(|(uri, id)| json!({ "uri": uri.to_string(), "node": schema.location(*id) }))
        .collect();
    let anchors: Vec<Value> = schema
        .anchors()
        .iter()
        .map(|(uri, name, id)| {
            json!({ "resource": uri.to_string(), "name": name, "node": schema.location(*id) })
        })
        .collect();
    let references: Vec<Value> = (0..schema.node_count())
        .filter_map(|id| match schema.node(id) {
            Node::Reference(target) => Some(json!({
                "from": schema.location(id),
                "to": schema.location(*target),
            })),
            _ => None,
        })
        .collect();
    json!({
        "nodes": nodes,
        "resources": resources,
        "anchors": anchors,
        "references": references,
    })
}

/// The vectors document.
///
/// # Errors
///
/// A listed document that is not JSON.
pub fn document() -> Result<Value, String> {
    let mut documents = Vec::with_capacity(DOCUMENTS.len() + 1);
    let named =
        std::iter::once(("meta-schema", DRAFT_7_META_SCHEMA)).chain(DOCUMENTS.iter().copied());
    for (name, text) in named {
        let value: Value =
            serde_json::from_str(text).map_err(|error| format!("{name} is not JSON: {error}"))?;
        let counted = census(&value);
        let mut vector = json!({
            "name": name,
            "text": serde_json::to_string(&value).map_err(|error| error.to_string())?,
            "census": { "nodes": counted.nodes, "references": counted.references },
        });
        match Schema::compile(&value) {
            Ok(schema) => {
                vector["failure"] = Value::Null;
                vector["reading"] = reading(&schema);
            }
            Err(failure) => vector["failure"] = json!(failure_name(failure)),
        }
        documents.push(vector);
    }
    Ok(json!({ "defaultBase": DEFAULT_BASE, "documents": documents }))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::construction::validator::number::DivisorFailure;
    use crate::construction::validator::pattern::PatternFailure;

    const COMMITTED: &str = include_str!("../../../fixtures/validator-schemas/vectors.json");

    #[test]
    fn the_committed_vectors_are_the_ones_this_crate_writes() {
        let committed: Value = serde_json::from_str(COMMITTED).expect("the vectors are JSON");
        assert_eq!(committed, document().expect("the vectors build"));
    }

    #[test]
    fn the_documents_reach_every_reading_and_every_failure() {
        let document = document().expect("the vectors build");
        let documents = document["documents"].as_array().expect("documents");
        let names: BTreeSet<&str> = documents
            .iter()
            .map(|vector| vector["name"].as_str().expect("a name"))
            .collect();
        assert_eq!(names.len(), documents.len());
        let failures: BTreeSet<&str> = documents
            .iter()
            .filter_map(|vector| vector["failure"].as_str())
            .collect();
        assert_eq!(
            failures,
            BTreeSet::from([
                "notASchema",
                "dialect",
                "identifier",
                "duplicateIdentifier",
                "keyword",
                "pattern",
                "divisor",
                "reference",
                "cycle",
            ])
        );
        let meta = &documents[0];
        assert_eq!(meta["name"], "meta-schema");
        assert_eq!(meta["census"], json!({ "nodes": 70, "references": 29 }));
        assert_eq!(meta["reading"]["nodes"].as_array().map(Vec::len), Some(70));
        assert_eq!(
            meta["reading"]["references"].as_array().map(Vec::len),
            Some(29)
        );
        let read = documents
            .iter()
            .filter(|vector| vector["failure"].is_null())
            .count();
        assert!(read > 5 && read < documents.len());
        for vector in documents
            .iter()
            .filter(|vector| vector["failure"].is_null())
        {
            let references = vector["reading"]["references"]
                .as_array()
                .expect("references");
            assert_eq!(
                vector["census"]["references"].as_u64(),
                Some(references.len() as u64),
                "{}",
                vector["name"]
            );
        }
    }

    #[test]
    fn every_failure_has_its_name() {
        for (failure, name) in [
            (SchemaFailure::NotASchema, "notASchema"),
            (SchemaFailure::Dialect, "dialect"),
            (SchemaFailure::Identifier, "identifier"),
            (SchemaFailure::DuplicateIdentifier, "duplicateIdentifier"),
            (SchemaFailure::Keyword, "keyword"),
            (SchemaFailure::Pattern(PatternFailure::Syntax), "pattern"),
            (SchemaFailure::Divisor(DivisorFailure::Digits), "divisor"),
            (SchemaFailure::Reference, "reference"),
            (SchemaFailure::Cycle, "cycle"),
        ] {
            assert_eq!(failure_name(failure), name);
        }
    }
}
