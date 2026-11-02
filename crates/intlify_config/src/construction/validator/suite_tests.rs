// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The JSON Schema Test Suite's required Draft 7 tests, vendored in
//! `fixtures/json-schema-test-suite`, against the validator. Every group is
//! read but the thirteen that need another document, and every test of the
//! others has the result the suite expects, in no more pairs than design 015
//! counts beforehand and in exactly the pairs their definition finds.

use std::collections::HashSet;

use serde_json::Value;

use super::schema::{census, Dependency, Items, Node, NodeId, Schema, SchemaFailure};
use super::validation::instance_nodes;

macro_rules! suite {
    ($($name:literal),* $(,)?) => {
        [$((
            $name,
            include_str!(concat!(
                "../../../fixtures/json-schema-test-suite/tests/draft7/",
                $name,
                ".json"
            )),
        )),*]
    };
}

/// The suite's required Draft 7 files, each by its name.
const FILES: [(&str, &str); 37] = suite![
    "additionalItems",
    "additionalProperties",
    "allOf",
    "anyOf",
    "boolean_schema",
    "const",
    "contains",
    "default",
    "definitions",
    "dependencies",
    "enum",
    "exclusiveMaximum",
    "exclusiveMinimum",
    "format",
    "if-then-else",
    "infinite-loop-detection",
    "items",
    "maxItems",
    "maxLength",
    "maxProperties",
    "maximum",
    "minItems",
    "minLength",
    "minProperties",
    "minimum",
    "multipleOf",
    "not",
    "oneOf",
    "pattern",
    "patternProperties",
    "properties",
    "propertyNames",
    "ref",
    "refRemote",
    "required",
    "type",
    "uniqueItems",
];

/// The groups that need a document other than their own schema, which
/// revision `"0"` never retrieves: the meta-schema by its URI, or a remote
/// schema. Each is named by its file and description, in the suite's order.
const ELSEWHERE: [(&str, &str); 13] = [
    ("definitions", "validate definition against metaschema"),
    ("ref", "remote ref, containing refs itself"),
    ("refRemote", "remote ref"),
    ("refRemote", "fragment within remote ref"),
    ("refRemote", "ref within remote ref"),
    ("refRemote", "base URI change"),
    ("refRemote", "base URI change - change folder"),
    ("refRemote", "base URI change - change folder in subschema"),
    ("refRemote", "root ref in remote ref"),
    ("refRemote", "remote ref with ref to definitions"),
    ("refRemote", "Location-independent identifier in remote ref"),
    (
        "refRemote",
        "retrieved nested refs resolve relative to their URI not $id",
    ),
    ("refRemote", "$ref to $ref finds location-independent $id"),
];

/// Every group of the suite, with its file's name and its description.
pub(super) fn groups() -> Vec<(&'static str, String, Value)> {
    FILES
        .iter()
        .flat_map(|(file, text)| {
            let groups: Vec<Value> = serde_json::from_str(text).expect("a file of groups");
            groups.into_iter().map(move |group| {
                let description = group["description"]
                    .as_str()
                    .expect("a description")
                    .to_owned();
                (*file, description, group)
            })
        })
        .collect()
}

/// Whether a group needs another document.
pub(super) fn elsewhere(file: &str, description: &str) -> bool {
    ELSEWHERE.contains(&(file, description))
}

/// The pairs of a validation by their definition, found one at a time. A
/// node applied to a value applies, to the same value, the nodes it applies
/// in place, and to the value's members and items, the nodes it names for
/// them, with its `propertyNames` to each member's name. A node applied to a
/// name, which is a string, applies only the nodes it applies in place. A
/// name's pairs are those of its member's value.
pub(super) fn defined_pairs(schema: &Schema, instance: &Value) -> u64 {
    // Each node met, with the address of the value it meets and whether it
    // meets the value itself or the name of its member.
    let mut met = HashSet::new();
    let mut pending = vec![(0, instance, false)];
    while let Some((id, value, named)) = pending.pop() {
        if !met.insert((id, std::ptr::from_ref(value), named)) {
            continue;
        }
        let keywords = match schema.node(id) {
            Node::Boolean(_) => continue,
            Node::Reference(target) => {
                pending.push((*target, value, named));
                continue;
            }
            Node::Keywords(keywords) => keywords,
        };
        let mut same: Vec<NodeId> = keywords.all_of.clone();
        same.extend(&keywords.any_of);
        same.extend(&keywords.one_of);
        same.extend(keywords.not);
        if let Some(condition) = keywords.condition {
            same.push(condition.test);
            same.extend(condition.then);
            same.extend(condition.otherwise);
        }
        if named {
            pending.extend(same.into_iter().map(|target| (target, value, true)));
            continue;
        }
        match value {
            Value::Object(members) => {
                for (name, dependency) in &keywords.dependencies {
                    if let Dependency::Schema(target) = dependency {
                        if members.contains_key(name) {
                            same.push(*target);
                        }
                    }
                }
                for (name, member) in members {
                    let mut targets: Vec<NodeId> =
                        keywords.properties.get(name).copied().into_iter().collect();
                    for (pattern, target) in &keywords.pattern_properties {
                        if pattern.matches(name) {
                            targets.push(*target);
                        }
                    }
                    if targets.is_empty() {
                        targets.extend(keywords.additional_properties);
                    }
                    pending.extend(targets.into_iter().map(|target| (target, member, false)));
                    if let Some(target) = keywords.property_names {
                        pending.push((target, member, true));
                    }
                }
            }
            Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    let target = match &keywords.items {
                        Some(Items::All(target)) => Some(*target),
                        Some(Items::Each(targets)) => {
                            targets.get(index).copied().or(keywords.additional_items)
                        }
                        None => None,
                    };
                    for target in target.into_iter().chain(keywords.contains) {
                        pending.push((target, item, false));
                    }
                }
            }
            _ => {}
        }
        pending.extend(same.into_iter().map(|target| (target, value, false)));
    }
    let pairs: HashSet<_> = met.into_iter().map(|(id, at, _)| (id, at)).collect();
    pairs.len() as u64
}

#[test]
fn every_group_is_read_but_those_that_need_another_document() {
    let mut refused = Vec::new();
    let mut read = 0;
    for (file, description, group) in groups() {
        let document = &group["schema"];
        match Schema::compile(document) {
            Ok(schema) => {
                assert!(!elsewhere(file, &description), "{file}: {description}");
                assert_eq!(
                    census(document).nodes,
                    schema.node_count() as u64,
                    "{file}: {description}"
                );
                read += 1;
            }
            Err(failure) => {
                assert_eq!(failure, SchemaFailure::Reference, "{file}: {description}");
                refused.push((file, description));
            }
        }
    }
    assert_eq!(
        refused,
        ELSEWHERE.map(|(file, description)| (file, description.to_owned()))
    );
    assert_eq!(read, 245);
}

#[test]
fn every_test_has_the_result_the_suite_expects() {
    let mut tests = 0;
    for (file, description, group) in groups() {
        let document = &group["schema"];
        let Ok(schema) = Schema::compile(document) else {
            assert!(elsewhere(file, &description), "{file}: {description}");
            continue;
        };
        let nodes = census(document).nodes;
        for test in group["tests"].as_array().expect("tests") {
            let data = &test["data"];
            let name = format!("{file}: {description}: {}", test["description"]);
            let validation = schema.validate(data);
            assert_eq!(Some(validation.valid), test["valid"].as_bool(), "{name}");
            assert!(validation.pairs <= nodes * instance_nodes(data), "{name}");
            assert_eq!(validation.pairs, defined_pairs(&schema, data), "{name}");
            tests += 1;
        }
    }
    assert_eq!(tests, 898);
}
