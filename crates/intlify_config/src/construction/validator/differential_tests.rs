// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The validator against the `jsonschema` crate as an oracle, with formats
//! left unchecked as revision `"0"` leaves them. The inputs keep to what the
//! two read alike: no number a float cannot hold exactly, no character on
//! which `\s`, `.`, or `\b` part them, no `format` or content keyword, no
//! `$id` beside a `$ref`, no other document, and no cycle of in-place
//! applicators, on which the oracle would not stop. Nor are two objects
//! compared whose members were written in different orders: under this
//! crate's `preserve_order`, the oracle compares them member by member. Every
//! validation's pairs are also checked against their definition and the
//! product design 015 counts beforehand.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use super::schema::{census, Schema, SchemaFailure};
use super::suite_tests::{defined_pairs, elsewhere, groups};
use super::validation::instance_nodes;
use crate::construction::admission::EnvelopeSchema;
use crate::construction::envelope;
use crate::construction::meta_schema::DRAFT_7_META_SCHEMA;
use crate::profile_fixtures::{complete_config, minimal_config};

const CONFIGURATION_SCHEMA: &str =
    include_str!("../../../schema/project-profile-config-v0.schema.json");
const ADMISSION_SCHEMA: &str =
    include_str!("../../../conformance/construction/construction-admission-v0.schema.json");
const ADMISSION: &str =
    include_str!("../../../conformance/construction/construction-admission-v0.json");
const SCHEMA_VECTORS: &str = include_str!("../../../fixtures/validator-schemas/vectors.json");
const SCHEMAS: [&str; 6] = [
    include_str!("../../../schema/intent-surface-class-vocabulary-v0.schema.json"),
    include_str!("../../../schema/measurement-case-identity-v0.schema.json"),
    include_str!("../../../schema/measurement-evidence-set-v0.schema.json"),
    include_str!("../../../schema/measurement-run-evaluation-v0.schema.json"),
    include_str!("../../../schema/measurement-run-plan-v0.schema.json"),
    include_str!("../../../schema/measurement-structured-report-v0.schema.json"),
];

const DIGEST: &str = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

fn parse(text: &str) -> Value {
    serde_json::from_str(text).expect("JSON")
}

fn oracle(document: &Value) -> jsonschema::Validator {
    jsonschema::draft7::options()
        .should_validate_formats(false)
        .build(document)
        .expect("the oracle reads the schema")
}

/// A schema read by both validators.
struct Pair {
    document: Value,
    nodes: u64,
    schema: Schema,
    oracle: jsonschema::Validator,
}

impl Pair {
    fn new(document: Value) -> Self {
        let schema = Schema::compile(&document).expect("a schema");
        Self {
            nodes: census(&document).nodes,
            oracle: oracle(&document),
            schema,
            document,
        }
    }

    /// Check that both validators judge `instance` alike, and that the
    /// validator's pairs are their definition's, within the product.
    fn judge(&self, instance: &Value) -> bool {
        let validation = self.schema.validate(instance);
        assert_eq!(
            validation.valid,
            self.oracle.is_valid(instance),
            "schema: {}\ninstance: {instance}",
            self.document
        );
        assert_eq!(
            validation.pairs,
            defined_pairs(&self.schema, instance),
            "schema: {}\ninstance: {instance}",
            self.document
        );
        assert!(validation.pairs <= self.nodes * instance_nodes(instance));
        validation.valid
    }
}

/// A JSON Pointer's reference token for a name.
fn token(name: &str) -> String {
    name.replace('~', "~0").replace('/', "~1")
}

/// Every node of `value`, each with its JSON Pointer.
fn locations(value: &Value) -> Vec<String> {
    let mut locations = Vec::new();
    let mut pending = vec![(String::new(), value)];
    while let Some((location, value)) = pending.pop() {
        match value {
            Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    pending.push((format!("{location}/{index}"), item));
                }
            }
            Value::Object(members) => {
                for (name, member) in members {
                    pending.push((format!("{location}/{}", token(name)), member));
                }
            }
            _ => {}
        }
        locations.push(location);
    }
    locations
}

/// Values near `value`: each node replaced by each of a few values, each
/// member and item removed, an unknown member added to each object, and the
/// first item of each array repeated.
fn variants(value: &Value) -> Vec<Value> {
    let replacements = [
        json!(null),
        json!(true),
        json!(0),
        json!(1.5),
        json!(""),
        json!("en"),
        json!([]),
        json!({}),
    ];
    let mut variants = Vec::new();
    for location in locations(value) {
        for replacement in &replacements {
            let mut variant = value.clone();
            *variant.pointer_mut(&location).expect("a node") = replacement.clone();
            variants.push(variant);
        }
        let node = value.pointer(&location).expect("a node");
        match node {
            Value::Object(members) => {
                for name in members.keys() {
                    let mut variant = value.clone();
                    if let Some(Value::Object(members)) = variant.pointer_mut(&location) {
                        members.remove(name);
                    }
                    variants.push(variant);
                }
                let mut variant = value.clone();
                variant.pointer_mut(&location).expect("a node")["unknown"] = json!(null);
                variants.push(variant);
            }
            Value::Array(items) => {
                for index in 0..items.len() {
                    let mut variant = value.clone();
                    if let Some(Value::Array(items)) = variant.pointer_mut(&location) {
                        items.remove(index);
                    }
                    variants.push(variant);
                }
                if let Some(first) = items.first() {
                    let mut variant = value.clone();
                    if let Some(Value::Array(items)) = variant.pointer_mut(&location) {
                        items.push(first.clone());
                    }
                    variants.push(variant);
                }
            }
            _ => {}
        }
    }
    variants
}

/// Judge `instance` and its variants, and return how many of each result.
fn judge_near(pair: &Pair, instance: &Value) -> (usize, usize) {
    assert!(pair.judge(instance), "{instance}");
    let mut counts = (1, 0);
    for variant in variants(instance) {
        if pair.judge(&variant) {
            counts.0 += 1;
        } else {
            counts.1 += 1;
        }
    }
    counts
}

/// The suite's tests the oracle fails as this crate builds it: under
/// `serde_json`'s `preserve_order`, it compares two objects member by member
/// in the order each was written.
const ORACLE_MISSES: [(&str, &str, &str); 3] = [
    (
        "const",
        "const with object",
        "same object with different property order is valid",
    ),
    (
        "uniqueItems",
        "uniqueItems validation",
        "property order of array of objects is ignored",
    ),
    (
        "uniqueItems",
        "uniqueItems validation",
        "objects are non-unique despite key order",
    ),
];

#[test]
fn the_oracle_passes_the_suite_but_where_it_compares_objects_in_order() {
    let mut tests = 0;
    let mut misses = Vec::new();
    for (file, description, group) in groups() {
        if elsewhere(file, &description) {
            continue;
        }
        let oracle = oracle(&group["schema"]);
        for test in group["tests"].as_array().expect("tests") {
            if Some(oracle.is_valid(&test["data"])) != test["valid"].as_bool() {
                let name = test["description"].as_str().expect("a description");
                misses.push((file, description.clone(), name.to_owned()));
            }
            tests += 1;
        }
    }
    assert_eq!(tests, 898);
    assert_eq!(
        misses,
        ORACLE_MISSES.map(|(file, group, test)| (file, group.to_owned(), test.to_owned()))
    );
}

#[test]
fn the_configuration_and_envelope_schemas_judge_as_the_oracle_does() {
    let configuration = Pair::new(parse(CONFIGURATION_SCHEMA));
    let mut counts = Vec::new();
    for config in [minimal_config(), complete_config()] {
        counts.push(judge_near(&configuration, &config));
    }
    let admission = Pair::new(parse(ADMISSION_SCHEMA));
    counts.push(judge_near(&admission, &parse(ADMISSION)));
    let tuple = json!({"identity": "intlify-project-profile", "revision": "0", "digest": DIGEST});
    let mut specification = tuple.clone();
    specification["semanticDigest"] = json!(DIGEST);
    let mut registries = Map::new();
    for role in [
        "safe-origin-kind-registry-schema",
        "check-registry-schema",
        "finding-registry-schema",
        "safe-origin-kind-registry",
        "check-registry",
        "finding-registry",
    ] {
        registries.insert(role.to_owned(), tuple.clone());
    }
    for (kind, instance) in [
        (EnvelopeSchema::ComponentTuple, tuple.clone()),
        (
            EnvelopeSchema::ProfileSpecificationTuple,
            specification.clone(),
        ),
        (
            EnvelopeSchema::ConfigurationSchemaAuthoritySet,
            json!({
                "profileSpecification": {
                    "identity": "intlify-project-profile",
                    "revision": "0",
                    "semanticDigest": DIGEST
                },
                "members": {"0": {
                    "identity": "intlify-configuration-schema",
                    "revision": "0",
                    "dialect": "http://json-schema.org/draft-07/schema#",
                    "digest": DIGEST
                }}
            }),
        ),
        (
            EnvelopeSchema::RegistryPackage,
            json!({"members": registries}),
        ),
    ] {
        counts.push(judge_near(&Pair::new(envelope::schema(kind)), &instance));
    }
    // Each instance has variants of both results.
    for (valid, invalid) in &counts {
        assert!(*valid > 1 && *invalid > 0, "{counts:?}");
    }
}

/// Keywords set into a schema object, valid and not under the meta-schema.
fn insertions() -> Vec<(&'static str, Value)> {
    vec![
        ("type", json!("nothing")),
        ("type", json!([])),
        ("type", json!(["string", "string"])),
        ("type", json!(["integer", "null"])),
        ("enum", json!([])),
        ("enum", json!("x")),
        ("const", json!(null)),
        ("multipleOf", json!(0)),
        ("multipleOf", json!(0.5)),
        ("minimum", json!("1")),
        ("minLength", json!(-1)),
        ("minLength", json!(1.5)),
        ("minLength", json!(2)),
        ("maxItems", json!("1")),
        ("pattern", json!(5)),
        ("items", json!(1)),
        ("items", json!([])),
        ("items", json!([{}, true])),
        ("required", json!([1])),
        ("required", json!(["a", "a"])),
        ("required", json!([])),
        ("properties", json!([])),
        ("properties", json!({"a": 1})),
        ("additionalProperties", json!(1)),
        ("dependencies", json!({"a": 1})),
        ("dependencies", json!({"a": ["b", "b"]})),
        ("dependencies", json!({"a": ["b"], "c": {}})),
        ("propertyNames", json!({"type": "string"})),
        ("allOf", json!([])),
        ("anyOf", json!([{}, 5])),
        ("if", json!(5)),
        ("$ref", json!(5)),
        ("$id", json!(5)),
        ("format", json!(5)),
        ("definitions", json!({"a": 5})),
        ("unknown", json!(5)),
    ]
}

/// Each schema object of `document`, with each insertion set into it.
fn inserted(document: &Value) -> Vec<Value> {
    let Ok(schema) = Schema::compile(document) else {
        return Vec::new();
    };
    let mut documents = Vec::new();
    for id in 0..schema.node_count() {
        let location = schema.location(id);
        if !document.pointer(location).is_some_and(Value::is_object) {
            continue;
        }
        for (keyword, value) in insertions() {
            let mut changed = document.clone();
            changed.pointer_mut(location).expect("a node")[keyword] = value;
            documents.push(changed);
        }
    }
    documents
}

#[test]
fn the_meta_schema_judges_schemas_as_the_oracle_does() {
    let meta = Pair::new(parse(DRAFT_7_META_SCHEMA));
    let mut documents: Vec<Value> = groups()
        .into_iter()
        .map(|(_, _, mut group)| group["schema"].take())
        .collect();
    let small = documents.len();
    documents.extend(SCHEMAS.iter().map(|text| parse(text)));
    documents.push(parse(CONFIGURATION_SCHEMA));
    documents.push(parse(ADMISSION_SCHEMA));
    documents.push(parse(DRAFT_7_META_SCHEMA));
    for kind in [
        EnvelopeSchema::ComponentTuple,
        EnvelopeSchema::ProfileSpecificationTuple,
        EnvelopeSchema::ConfigurationSchemaAuthoritySet,
        EnvelopeSchema::RegistryPackage,
    ] {
        documents.push(envelope::schema(kind));
    }
    let vectors = parse(SCHEMA_VECTORS);
    for document in vectors["documents"].as_array().expect("documents") {
        documents.push(parse(document["text"].as_str().expect("a text")));
    }
    let (mut valid, mut invalid) = (0, 0);
    for (index, document) in documents.iter().enumerate() {
        let mut judged = vec![document.clone()];
        // The suite's small schemas take every insertion at every object.
        if index < small {
            judged.extend(inserted(document));
        }
        for document in judged {
            if meta.judge(&document) {
                valid += 1;
            } else {
                invalid += 1;
            }
        }
    }
    assert!(
        valid > 1000 && invalid > 1000,
        "{valid} valid, {invalid} invalid"
    );
}

/// A deterministic source of choices, by the `SplitMix64` steps.
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

const NAMES: [&str; 5] = ["a", "b", "ab", "c", "x1"];
const STRINGS: [&str; 8] = ["", "a", "ab", "ba", "abc", "x1", "12", "\u{e9}t\u{e9}"];
const NUMBERS: [&str; 10] = ["0", "1", "-1", "2", "2.0", "3", "10", "0.5", "1.5", "-2.5"];
const DIVISORS: [&str; 4] = ["1", "2", "0.5", "1.5"];
const PATTERNS: [&str; 6] = ["^a", "b$", "^[a-c]*$", "x|y", "^.b", "[0-9]"];
const TYPES: [&str; 7] = [
    "array", "boolean", "integer", "null", "number", "object", "string",
];
const REFERENCES: [&str; 3] = ["#", "#/definitions/d0", "#/definitions/d1"];

fn instance(random: &mut Random, depth: usize) -> Value {
    match random.below(if depth == 0 { 4 } else { 6 }) {
        0 => Value::Null,
        1 => Value::Bool(random.chance(50)),
        2 => parse(random.pick(&NUMBERS)),
        3 => json!(random.pick(&STRINGS)),
        4 => {
            let count = random.below(4);
            Value::Array((0..count).map(|_| instance(random, depth - 1)).collect())
        }
        _ => {
            // Members in the order of their names, which the oracle needs to
            // compare two objects.
            let count = random.below(4);
            let members: BTreeMap<String, Value> = (0..count)
                .map(|_| {
                    (
                        (*random.pick(&NAMES)).to_owned(),
                        instance(random, depth - 1),
                    )
                })
                .collect();
            Value::Object(members.into_iter().collect())
        }
    }
}

/// A few of `items`, none twice.
fn some(random: &mut Random, items: &[&str]) -> Value {
    let mut chosen: Vec<&str> = Vec::new();
    for _ in 0..=random.below(items.len().min(3)) {
        let item = *random.pick(items);
        if !chosen.contains(&item) {
            chosen.push(item);
        }
    }
    json!(chosen)
}

fn subschemas(random: &mut Random, depth: usize) -> Value {
    let count = 1 + random.below(3);
    Value::Array((0..count).map(|_| subschema(random, depth)).collect())
}

fn subschema(random: &mut Random, depth: usize) -> Value {
    if depth == 0 || random.chance(20) {
        return match random.below(4) {
            0 => json!(true),
            1 => json!(false),
            2 => json!({"$ref": random.pick(&REFERENCES)}),
            _ => json!({}),
        };
    }
    let mut object = Map::new();
    for _ in 0..=random.below(3) {
        let (keyword, value) = keyword(random, depth - 1);
        object.insert(keyword.to_owned(), value);
    }
    // A reference beside other keywords ignores them.
    if random.chance(5) {
        object.insert("$ref".to_owned(), json!(random.pick(&REFERENCES)));
    }
    Value::Object(object)
}

fn keyword(random: &mut Random, depth: usize) -> (&'static str, Value) {
    let count = |random: &mut Random| json!(random.below(4));
    let number = |random: &mut Random| parse(random.pick(&NUMBERS));
    match random.below(31) {
        0 => (
            "type",
            if random.chance(50) {
                json!(random.pick(&TYPES))
            } else {
                some(random, &TYPES)
            },
        ),
        1 => {
            let count = 1 + random.below(3);
            (
                "enum",
                Value::Array((0..count).map(|_| instance(random, 1)).collect()),
            )
        }
        2 => ("const", instance(random, 1)),
        3 => ("multipleOf", parse(random.pick(&DIVISORS))),
        4 => ("maximum", number(random)),
        5 => ("exclusiveMaximum", number(random)),
        6 => ("minimum", number(random)),
        7 => ("exclusiveMinimum", number(random)),
        8 => ("maxLength", count(random)),
        9 => ("minLength", count(random)),
        10 => ("pattern", json!(random.pick(&PATTERNS))),
        11 => (
            "items",
            if random.chance(50) {
                subschema(random, depth)
            } else {
                subschemas(random, depth)
            },
        ),
        12 => ("additionalItems", subschema(random, depth)),
        13 => ("maxItems", count(random)),
        14 => ("minItems", count(random)),
        15 => ("uniqueItems", json!(random.chance(50))),
        16 => ("contains", subschema(random, depth)),
        17 => ("maxProperties", count(random)),
        18 => ("minProperties", count(random)),
        19 => ("required", some(random, &NAMES)),
        20 => {
            let mut properties = Map::new();
            for _ in 0..=random.below(2) {
                properties.insert((*random.pick(&NAMES)).to_owned(), subschema(random, depth));
            }
            ("properties", Value::Object(properties))
        }
        21 => (
            "patternProperties",
            json!({*random.pick(&PATTERNS): subschema(random, depth)}),
        ),
        22 => ("additionalProperties", subschema(random, depth)),
        23 => {
            let dependency = if random.chance(50) {
                some(random, &NAMES)
            } else {
                subschema(random, depth)
            };
            ("dependencies", json!({*random.pick(&NAMES): dependency}))
        }
        24 => ("propertyNames", subschema(random, depth)),
        25 => ("if", subschema(random, depth)),
        26 => ("then", subschema(random, depth)),
        27 => ("else", subschema(random, depth)),
        28 => ("allOf", subschemas(random, depth)),
        29 => ("anyOf", subschemas(random, depth)),
        _ => {
            if random.chance(50) {
                ("oneOf", subschemas(random, depth))
            } else {
                ("not", subschema(random, depth))
            }
        }
    }
}

/// A schema whose references name its root or one of its two definitions.
fn document(random: &mut Random) -> Value {
    let mut root = Map::new();
    for _ in 0..=random.below(3) {
        let (keyword, value) = keyword(random, 3);
        root.insert(keyword.to_owned(), value);
    }
    root.insert(
        "definitions".to_owned(),
        json!({"d0": subschema(random, 2), "d1": subschema(random, 2)}),
    );
    Value::Object(root)
}

#[test]
fn random_schemas_judge_random_instances_as_the_oracle_does() {
    let mut random = Random(0x0015_0003_0b3b);
    let (mut judged, mut cycles, mut valid) = (0, 0, 0);
    while judged < 2000 {
        let document = document(&mut random);
        match Schema::compile(&document) {
            Ok(_) => {}
            // The oracle would not stop on a cycle.
            Err(SchemaFailure::Cycle) => {
                cycles += 1;
                continue;
            }
            Err(failure) => panic!("{failure:?}: {document}"),
        }
        let pair = Pair::new(document);
        for _ in 0..8 {
            let instance = instance(&mut random, 3);
            valid += usize::from(pair.judge(&instance));
        }
        judged += 1;
    }
    // Both results are common, and few schemas are cycles.
    assert!(valid > 2000 && valid < 14000, "{valid} of 16000 valid");
    assert!(cycles < 400, "{cycles} cycles");
}
