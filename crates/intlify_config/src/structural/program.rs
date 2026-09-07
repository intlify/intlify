// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Compile the generated authoring schema's explicit Draft 7 subset into a
//! finite immutable graph. Unsupported keywords/patterns/references fail closed;
//! there is no network lookup, runtime $schema selection, or permissive fallback.
//! This is not a general JSON Schema implementation or formal authority loader.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use crate::model::ID_PATTERN;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct SchemaId(usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ValueType {
    Null,
    Boolean,
    Number,
    Integer,
    String,
    Array,
    Object,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Additional {
    Forbidden,
    Schema(SchemaId),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct SchemaNode {
    pub(super) reference: Option<SchemaId>,
    pub(super) value_type: Option<ValueType>,
    pub(super) enum_strings: Option<BTreeSet<String>>,
    pub(super) identity_pattern: bool,
    pub(super) min_items: Option<u64>,
    pub(super) min_properties: Option<u64>,
    pub(super) required: Option<BTreeSet<String>>,
    pub(super) properties: Option<BTreeMap<String, SchemaId>>,
    pub(super) identity_values: Option<SchemaId>,
    pub(super) additional: Option<Additional>,
    pub(super) items: Option<SchemaId>,
    pub(super) any_of: Option<Vec<SchemaId>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProgramError {
    UnsupportedDialect,
    UnsupportedKeyword,
    UnsupportedPattern,
    UnsupportedReference,
    CyclicReference,
    MalformedSchema,
}

pub(super) struct SchemaProgram {
    nodes: Vec<SchemaNode>,
    root: SchemaId,
    // Canonical schema locations, never paths copied from a configuration value.
    locations: BTreeMap<String, SchemaId>,
}

impl SchemaProgram {
    pub(super) fn compile(body: &Value) -> Result<Self, ProgramError> {
        if body.get("$schema").and_then(Value::as_str)
            != Some("http://json-schema.org/draft-07/schema#")
        {
            return Err(ProgramError::UnsupportedDialect);
        }
        let mut compiler = Compiler {
            body,
            nodes: Vec::new(),
            locations: BTreeMap::new(),
            visiting: BTreeSet::new(),
        };
        let root = compiler.node("")?;
        // Unused definitions must not conceal unsupported behavior for a future
        // field. This also makes a changed generated schema fail visibly in tests.
        if let Some(definitions) = body.get("definitions") {
            for name in definitions
                .as_object()
                .ok_or(ProgramError::MalformedSchema)?
                .keys()
                .collect::<BTreeSet<_>>()
            {
                compiler.node(&child_path("/definitions", name))?;
            }
        }
        Ok(Self {
            nodes: compiler.nodes,
            root,
            locations: compiler.locations,
        })
    }

    pub(super) const fn root(&self) -> SchemaId {
        self.root
    }
    pub(super) fn node(&self, id: SchemaId) -> &SchemaNode {
        &self.nodes[id.0]
    }
    pub(super) fn location(&self, pointer: &str) -> Option<SchemaId> {
        self.locations.get(pointer).copied()
    }
}

struct Compiler<'schema> {
    body: &'schema Value,
    nodes: Vec<SchemaNode>,
    locations: BTreeMap<String, SchemaId>,
    visiting: BTreeSet<String>,
}

fn child_path(parent: &str, name: &str) -> String {
    let escaped = name.replace('~', "~0").replace('/', "~1");
    format!("{parent}/{escaped}")
}

fn strings(value: &Value) -> Result<BTreeSet<String>, ProgramError> {
    let values = value.as_array().ok_or(ProgramError::MalformedSchema)?;
    let strings: BTreeSet<_> = values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or(ProgramError::MalformedSchema)
        })
        .collect::<Result<_, _>>()?;
    if strings.len() != values.len() {
        return Err(ProgramError::MalformedSchema);
    }
    Ok(strings)
}

impl Compiler<'_> {
    fn node(&mut self, path: &str) -> Result<SchemaId, ProgramError> {
        if self.visiting.contains(path) {
            return Err(ProgramError::CyclicReference);
        }
        if let Some(&id) = self.locations.get(path) {
            return Ok(id);
        }
        let source = self.body;
        let object = source
            .pointer(path)
            .and_then(Value::as_object)
            .ok_or(ProgramError::MalformedSchema)?;
        Self::admit_keywords(object, path)?;
        let id = SchemaId(self.nodes.len());
        self.nodes.push(SchemaNode::default());
        self.locations.insert(path.to_owned(), id);
        self.visiting.insert(path.to_owned());
        let mut node = SchemaNode::default();
        if let Some(reference) = object.get("$ref") {
            let reference = reference.as_str().ok_or(ProgramError::MalformedSchema)?;
            let pointer = reference
                .strip_prefix('#')
                .ok_or(ProgramError::UnsupportedReference)?;
            if !pointer.starts_with("/definitions/") || source.pointer(pointer).is_none() {
                return Err(ProgramError::UnsupportedReference);
            }
            node.reference = Some(self.node(pointer)?);
        } else {
            node.value_type = object
                .get("type")
                .map(|value| match value.as_str() {
                    Some("null") => Ok(ValueType::Null),
                    Some("boolean") => Ok(ValueType::Boolean),
                    Some("number") => Ok(ValueType::Number),
                    Some("integer") => Ok(ValueType::Integer),
                    Some("string") => Ok(ValueType::String),
                    Some("array") => Ok(ValueType::Array),
                    Some("object") => Ok(ValueType::Object),
                    _ => Err(ProgramError::MalformedSchema),
                })
                .transpose()?;
            node.enum_strings = object.get("enum").map(strings).transpose()?;
            if node.enum_strings.as_ref().is_some_and(BTreeSet::is_empty) {
                return Err(ProgramError::MalformedSchema);
            }
            node.identity_pattern = match object.get("pattern") {
                None => false,
                Some(Value::String(pattern)) if pattern == ID_PATTERN => true,
                Some(_) => return Err(ProgramError::UnsupportedPattern),
            };
            node.min_items = object
                .get("minItems")
                .map(|value| value.as_u64().ok_or(ProgramError::MalformedSchema))
                .transpose()?;
            node.min_properties = object
                .get("minProperties")
                .map(|value| value.as_u64().ok_or(ProgramError::MalformedSchema))
                .transpose()?;
            node.required = object.get("required").map(strings).transpose()?;
            if let Some(properties) = object.get("properties") {
                let mut children = BTreeMap::new();
                let parent = child_path(path, "properties");
                for name in properties
                    .as_object()
                    .ok_or(ProgramError::MalformedSchema)?
                    .keys()
                    .collect::<BTreeSet<_>>()
                {
                    children.insert(name.clone(), self.node(&child_path(&parent, name))?);
                }
                node.properties = Some(children);
            }
            if let Some(properties) = object.get("patternProperties") {
                let patterns = properties
                    .as_object()
                    .ok_or(ProgramError::MalformedSchema)?;
                if patterns.len() != 1 || !patterns.contains_key(ID_PATTERN) {
                    return Err(ProgramError::UnsupportedPattern);
                }
                node.identity_values = Some(self.node(&child_path(
                    &child_path(path, "patternProperties"),
                    ID_PATTERN,
                ))?);
            }
            node.additional = match object.get("additionalProperties") {
                None => None,
                Some(Value::Bool(false)) => Some(Additional::Forbidden),
                Some(Value::Object(_)) => Some(Additional::Schema(
                    self.node(&child_path(path, "additionalProperties"))?,
                )),
                Some(_) => return Err(ProgramError::MalformedSchema),
            };
            node.items = object
                .get("items")
                .map(|_| self.node(&child_path(path, "items")))
                .transpose()?;
            if let Some(branches) = object.get("anyOf") {
                let branches = branches.as_array().ok_or(ProgramError::MalformedSchema)?;
                if branches.is_empty() {
                    return Err(ProgramError::MalformedSchema);
                }
                let parent = child_path(path, "anyOf");
                node.any_of = Some(
                    (0..branches.len())
                        .map(|index| self.node(&child_path(&parent, &index.to_string())))
                        .collect::<Result<_, _>>()?,
                );
            }
        }
        self.nodes[id.0] = node;
        self.visiting.remove(path);
        Ok(id)
    }

    fn admit_keywords(object: &Map<String, Value>, path: &str) -> Result<(), ProgramError> {
        for (name, value) in object {
            match name.as_str() {
                "$schema" if path.is_empty() => {}
                "definitions" if path.is_empty() && value.is_object() => {}
                "title" | "description" if value.is_string() => {}
                "$ref"
                | "type"
                | "enum"
                | "pattern"
                | "minItems"
                | "minProperties"
                | "required"
                | "properties"
                | "patternProperties"
                | "additionalProperties"
                | "items"
                | "anyOf" => {}
                _ => return Err(ProgramError::UnsupportedKeyword),
            }
        }
        // Draft 7 ignores assertion siblings of $ref. The generated owned model
        // does not emit them; reject instead of implying that we checked them.
        if object.contains_key("$ref")
            && object
                .keys()
                .any(|key| !matches!(key.as_str(), "$ref" | "title" | "description"))
        {
            return Err(ProgramError::UnsupportedKeyword);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{ProgramError, SchemaProgram};
    use crate::fixtures::FixtureConfig;
    use serde_json::json;

    fn schema() -> serde_json::Value {
        crate::schema::draft7_schema::<FixtureConfig>().unwrap()
    }

    fn rejected(value: &serde_json::Value, expected: ProgramError) {
        assert_eq!(SchemaProgram::compile(value).err().unwrap(), expected);
    }

    #[test]
    fn generated_schema_compiles_every_definition_without_runtime_fetching() {
        let source = schema();
        let program = SchemaProgram::compile(&source).unwrap();
        assert!(program
            .node(program.root())
            .properties
            .as_ref()
            .unwrap()
            .contains_key("profiles"));
        for name in source["definitions"].as_object().unwrap().keys() {
            assert!(program.location(&format!("/definitions/{name}")).is_some());
        }
        assert!(program
            .location("/definitions/FixturePolicyReference")
            .is_some());
    }

    #[test]
    fn schema_member_order_does_not_change_compiled_addresses_or_constraints() {
        fn reverse_objects(value: &serde_json::Value) -> serde_json::Value {
            match value {
                serde_json::Value::Object(object) => serde_json::Value::Object(
                    object
                        .iter()
                        .rev()
                        .map(|(name, value)| (name.clone(), reverse_objects(value)))
                        .collect(),
                ),
                serde_json::Value::Array(items) => {
                    serde_json::Value::Array(items.iter().map(reverse_objects).collect())
                }
                other => other.clone(),
            }
        }
        let source = schema();
        let original = SchemaProgram::compile(&source).unwrap();
        let reordered = SchemaProgram::compile(&reverse_objects(&source)).unwrap();
        assert_eq!(original.locations, reordered.locations);
        assert_eq!(original.nodes, reordered.nodes);
        assert_eq!(original.root, reordered.root);
    }

    #[test]
    fn unsupported_schema_behavior_is_not_silently_ignored() {
        let mut value = schema();
        value["unevaluatedProperties"] = false.into();
        rejected(&value, ProgramError::UnsupportedKeyword);
        let mut value = schema();
        value["$schema"] = "https://json-schema.org/draft/2020-12/schema".into();
        rejected(&value, ProgramError::UnsupportedDialect);
        let mut value = schema();
        value["definitions"]["ProjectId"]["pattern"] = ".*".into();
        rejected(&value, ProgramError::UnsupportedPattern);
        let mut value = schema();
        value["definitions"]["Present_string"]["$ref"] =
            "https://invalid.example/string.json".into();
        value["definitions"]["Present_string"]
            .as_object_mut()
            .unwrap()
            .remove("type");
        rejected(&value, ProgramError::UnsupportedReference);
    }

    #[test]
    fn cycles_dangling_references_and_unused_invalid_definitions_fail_closed() {
        let mut value = schema();
        value["definitions"]["Unused"] = json!({"$ref":"#/definitions/Unused"});
        rejected(&value, ProgramError::CyclicReference);
        let mut value = schema();
        value["definitions"]["Unused"] = json!({"$ref":"#/definitions/Missing"});
        rejected(&value, ProgramError::UnsupportedReference);
        let mut value = schema();
        value["definitions"]["Unused"] = json!({"format":"custom"});
        rejected(&value, ProgramError::UnsupportedKeyword);
    }

    #[test]
    fn ref_siblings_and_malformed_constraints_cannot_weaken_the_owned_schema() {
        let mut value = schema();
        value["definitions"]["Present_string"] =
            json!({"$ref":"#/definitions/ProjectId", "minLength":1});
        rejected(&value, ProgramError::UnsupportedKeyword);
        let mut value = schema();
        value["definitions"]["ProjectId"]["enum"] = json!(["a", "a"]);
        rejected(&value, ProgramError::MalformedSchema);
        let mut value = schema();
        value["definitions"]["CoverageRule"]["anyOf"] = json!([]);
        rejected(&value, ProgramError::MalformedSchema);
    }
}
