// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Draft 7 schemas, read once for validation.
//!
//! A schema document is read into nodes: the root and every object or boolean
//! in one of Draft 7's subschema positions, in document order. The same
//! positions define the census construction takes before any other check, the
//! number of nodes and of `$ref` occurrences, so it is counted on any JSON.
//!
//! Each node resolves references against a base URI: the nearest `$id` that
//! names a resource, or [`DEFAULT_BASE`] for a root that names none. An `$id`
//! that starts with `#` is a plain-name anchor within its resource. A `$ref`
//! is resolved within the document to a node, through a resource and then its
//! fragment: empty, a JSON Pointer, or an anchor. Nothing is retrieved, not
//! even the meta-schema. An object with `$ref` is that reference alone: its
//! other keywords, `$id` and `$schema` among them, are ignored (core 8.3).
//!
//! Applying a node to an instance may apply other nodes to the same instance:
//! the node a `$ref` names, the members of `allOf`, `anyOf`, and `oneOf`,
//! `not`, `if` with its `then` and `else`, and the schemas of `dependencies`.
//! A cycle of these recurses without end, which Draft 7 leaves undefined, so a
//! schema with one is refused whatever instance it would meet. Every other
//! applicator moves to a member, an item, or a name, so it ends.
//!
//! A keyword the validator reads has the shape the meta-schema gives it: an
//! array of subschemas or of types is not empty, and `type`, `required`, and a
//! dependency's names repeat nothing. Annotations, such as `title`, are not
//! read, and are left to the meta-schema.
//!
//! A document is checked in this order, each step over its nodes in document
//! order: its dialect and identifiers, its nodes (a root that is not a schema,
//! and keywords of the wrong shape, patterns, and divisors), its references,
//! and its cycles. The first failure is reported.

use std::collections::{BTreeSet, HashMap};

use serde_json::{Map, Value};

use super::equal::{key, Key};
use super::number::{Decimal, Divisor, DivisorFailure};
use super::pattern::{Pattern, PatternFailure};
use super::pointer::Pointer;
use super::uri::Reference;
use crate::construction::admission::DRAFT_7;

/// The base URI of a document whose root names no resource.
pub(crate) const DEFAULT_BASE: &str = "json-schema:///";

/// A node's index: its place in document order, the root first.
pub(crate) type NodeId = usize;

/// The totals construction counts on any JSON before reading it as a schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Census {
    /// Schema objects and boolean schemas: design 015's `schema-node`.
    pub(crate) nodes: u64,
    /// Objects among them that have `$ref`: design 015's `schema-reference`.
    pub(crate) references: u64,
}

/// Why a document is not a schema this validator reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SchemaFailure {
    /// The root is neither an object nor a boolean.
    NotASchema,
    /// The root's `$schema` is not Draft 7's URI, or a subschema has one.
    Dialect,
    /// An `$id` is not a URI reference, has a fragment, or follows `#` with
    /// something other than a plain name.
    Identifier,
    /// Two `$id`s name one resource, or one anchor within a resource.
    DuplicateIdentifier,
    /// A keyword's value is not of the shape the meta-schema gives it.
    Keyword,
    /// A `pattern`, or a name of `patternProperties`, does not compile.
    Pattern(PatternFailure),
    /// A `multipleOf` is not a divisor.
    Divisor(DivisorFailure),
    /// A `$ref` is not a URI reference, or names no node of the document.
    Reference,
    /// Nodes apply one another to the same instance in a cycle.
    Cycle,
}

/// One of Draft 7's seven instance types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Type {
    Array,
    Boolean,
    Integer,
    Null,
    Number,
    Object,
    String,
}

impl Type {
    fn named(name: &str) -> Option<Self> {
        Some(match name {
            "array" => Self::Array,
            "boolean" => Self::Boolean,
            "integer" => Self::Integer,
            "null" => Self::Null,
            "number" => Self::Number,
            "object" => Self::Object,
            "string" => Self::String,
            _ => return None,
        })
    }
}

/// The types a `type` keyword allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Types(u8);

impl Types {
    fn with(self, kind: Type) -> Self {
        Self(self.0 | 1 << kind as u8)
    }

    pub(crate) fn contains(self, kind: Type) -> bool {
        self.0 & 1 << kind as u8 != 0
    }
}

/// What `items` applies to an array's items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Items {
    /// One node to every item.
    All(NodeId),
    /// One node to each item at the same index.
    Each(Vec<NodeId>),
}

/// What a member of `dependencies` asks when its name is present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Dependency {
    /// These members are present too.
    Names(Vec<String>),
    /// The object is valid against this node.
    Schema(NodeId),
}

/// `if`, with the `then` and the `else` it chooses between.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Condition {
    pub(crate) test: NodeId,
    pub(crate) then: Option<NodeId>,
    pub(crate) otherwise: Option<NodeId>,
}

/// The keywords of a schema object without `$ref`. Annotations, such as
/// `title` and `format`, and unknown keywords are not kept.
#[derive(Debug, Default)]
pub(crate) struct Keywords {
    pub(crate) types: Option<Types>,
    pub(crate) enumeration: Option<BTreeSet<Key>>,
    pub(crate) constant: Option<Key>,
    pub(crate) multiple_of: Option<Divisor>,
    pub(crate) maximum: Option<Decimal>,
    pub(crate) exclusive_maximum: Option<Decimal>,
    pub(crate) minimum: Option<Decimal>,
    pub(crate) exclusive_minimum: Option<Decimal>,
    pub(crate) max_length: Option<u64>,
    pub(crate) min_length: Option<u64>,
    pub(crate) pattern: Option<Pattern>,
    pub(crate) items: Option<Items>,
    pub(crate) additional_items: Option<NodeId>,
    pub(crate) max_items: Option<u64>,
    pub(crate) min_items: Option<u64>,
    pub(crate) unique_items: bool,
    pub(crate) contains: Option<NodeId>,
    pub(crate) max_properties: Option<u64>,
    pub(crate) min_properties: Option<u64>,
    pub(crate) required: Vec<String>,
    pub(crate) properties: Vec<(String, NodeId)>,
    pub(crate) pattern_properties: Vec<(Pattern, NodeId)>,
    pub(crate) additional_properties: Option<NodeId>,
    pub(crate) dependencies: Vec<(String, Dependency)>,
    pub(crate) property_names: Option<NodeId>,
    pub(crate) condition: Option<Condition>,
    pub(crate) all_of: Vec<NodeId>,
    pub(crate) any_of: Vec<NodeId>,
    pub(crate) one_of: Vec<NodeId>,
    pub(crate) not: Option<NodeId>,
}

/// A node of a schema.
#[derive(Debug)]
pub(crate) enum Node {
    /// `true` or `false`.
    Boolean(bool),
    /// An object with `$ref`, which applies the node it names.
    Reference(NodeId),
    /// Any other object.
    Keywords(Box<Keywords>),
}

/// A schema document, read.
#[derive(Debug)]
pub(crate) struct Schema {
    nodes: Vec<Node>,
    locations: Vec<String>,
    resources: Vec<(Reference, NodeId)>,
    anchors: Vec<(Reference, String, NodeId)>,
    order: Vec<NodeId>,
}

impl Schema {
    /// Read a schema document.
    pub(crate) fn compile(document: &Value) -> Result<Self, SchemaFailure> {
        let walk = Walk::read(document)?;
        let mut nodes = walk
            .values
            .iter()
            .map(|value| walk.node(value))
            .collect::<Result<Vec<_>, _>>()?;
        for (id, value) in walk.values.iter().enumerate() {
            if let Some(reference) = value.get("$ref") {
                nodes[id] = Node::Reference(walk.resolve(id, reference)?);
            }
        }
        let order = in_place_order(&nodes)?;
        Ok(Self {
            nodes,
            locations: walk.locations,
            resources: walk.resources,
            anchors: walk.anchors,
            order,
        })
    }

    /// How many nodes the schema has.
    pub(crate) fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub(crate) fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id]
    }

    /// The JSON Pointer to a node within its document.
    pub(crate) fn location(&self, id: NodeId) -> &str {
        &self.locations[id]
    }

    /// The resources the document names, each with its node, in document
    /// order.
    pub(crate) fn resources(&self) -> &[(Reference, NodeId)] {
        &self.resources
    }

    /// The plain-name anchors, each with its resource and node, in document
    /// order.
    pub(crate) fn anchors(&self) -> &[(Reference, String, NodeId)] {
        &self.anchors
    }

    /// Every node, each after every node it applies to the same instance.
    pub(crate) fn order(&self) -> &[NodeId] {
        &self.order
    }
}

/// Count the schema nodes and `$ref` occurrences of any JSON.
pub(crate) fn census(document: &Value) -> Census {
    let mut census = Census::default();
    let mut pending = vec![document];
    while let Some(value) = pending.pop() {
        match value {
            Value::Bool(_) => census.nodes += 1,
            Value::Object(object) => {
                census.nodes += 1;
                census.references += u64::from(object.contains_key("$ref"));
                pending.extend(children(object).into_iter().map(|(_, child)| child));
            }
            _ => {}
        }
    }
    census
}

fn is_schema(value: &Value) -> bool {
    matches!(value, Value::Object(_) | Value::Bool(_))
}

/// The subschemas a schema object holds, each with the end of its JSON
/// Pointer: every object or boolean in one of Draft 7's subschema positions,
/// in the object's order.
fn children(object: &Map<String, Value>) -> Vec<(String, &Value)> {
    let mut children = Vec::new();
    for (keyword, value) in object {
        let at = format!("/{}", escape(keyword));
        match keyword.as_str() {
            "additionalItems"
            | "additionalProperties"
            | "contains"
            | "else"
            | "if"
            | "not"
            | "propertyNames"
            | "then" => children.push((at, value)),
            "allOf" | "anyOf" | "oneOf" => {
                if let Value::Array(items) = value {
                    for (index, item) in items.iter().enumerate() {
                        children.push((format!("{at}/{index}"), item));
                    }
                }
            }
            "items" => match value {
                Value::Array(items) => {
                    for (index, item) in items.iter().enumerate() {
                        children.push((format!("{at}/{index}"), item));
                    }
                }
                _ => children.push((at, value)),
            },
            // A dependency's array of names is not a schema, so is left out
            // below with every other value that is not one.
            "definitions" | "dependencies" | "patternProperties" | "properties" => {
                if let Value::Object(members) = value {
                    for (name, member) in members {
                        children.push((format!("{at}/{}", escape(name)), member));
                    }
                }
            }
            _ => {}
        }
    }
    children.retain(|(_, value)| is_schema(value));
    children
}

/// A JSON Pointer's reference token for a name.
fn escape(name: &str) -> String {
    name.replace('~', "~0").replace('/', "~1")
}

/// `[A-Za-z][A-Za-z0-9-_:.]*`, the plain name of a Draft 7 anchor (core
/// 8.2.3).
fn is_plain_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && bytes
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':' | b'.'))
}

/// The nodes of a document in document order, with what names them.
struct Walk<'a> {
    values: Vec<&'a Value>,
    locations: Vec<String>,
    /// The base each node resolves its `$ref` against.
    bases: Vec<Reference>,
    addresses: HashMap<*const Value, NodeId>,
    resources: Vec<(Reference, NodeId)>,
    resource_index: HashMap<Reference, NodeId>,
    anchors: Vec<(Reference, String, NodeId)>,
    anchor_index: HashMap<(Reference, String), NodeId>,
}

impl<'a> Walk<'a> {
    /// Visit every node in document order, checking `$schema` and reading
    /// `$id`. A root that is not a schema is the only node visited.
    fn read(document: &'a Value) -> Result<Self, SchemaFailure> {
        let default = Reference::parse(DEFAULT_BASE)
            .expect("the default base is a URI")
            .normalize();
        let mut walk = Self {
            values: Vec::new(),
            locations: Vec::new(),
            bases: Vec::new(),
            addresses: HashMap::new(),
            resources: Vec::new(),
            resource_index: HashMap::new(),
            anchors: Vec::new(),
            anchor_index: HashMap::new(),
        };
        let mut pending = vec![(document, String::new(), default.clone())];
        while let Some((value, location, mut base)) = pending.pop() {
            let id = walk.values.len();
            walk.values.push(value);
            walk.addresses.insert(std::ptr::from_ref(value), id);
            if let Value::Object(object) = value {
                if !object.contains_key("$ref") {
                    if let Some(dialect) = object.get("$schema") {
                        if id != 0 || dialect.as_str() != Some(DRAFT_7) {
                            return Err(SchemaFailure::Dialect);
                        }
                    }
                    if let Some(identifier) = object.get("$id") {
                        base = walk.identify(id, identifier, base)?;
                    }
                }
                for (end, child) in children(object).into_iter().rev() {
                    pending.push((child, format!("{location}{end}"), base.clone()));
                }
            }
            // A root that names no resource is the default base's.
            if id == 0 && walk.resources.is_empty() {
                walk.register(default.clone(), 0)?;
            }
            walk.locations.push(location);
            walk.bases.push(base);
        }
        Ok(walk)
    }

    /// Read node `id`'s `$id` against `base`, and return the base of its
    /// keywords and subschemas.
    fn identify(
        &mut self,
        id: NodeId,
        identifier: &Value,
        base: Reference,
    ) -> Result<Reference, SchemaFailure> {
        let text = identifier.as_str().ok_or(SchemaFailure::Identifier)?;
        if let Some(name) = text.strip_prefix('#') {
            if !is_plain_name(name) {
                return Err(SchemaFailure::Identifier);
            }
            let anchor = (base.clone(), name.to_owned());
            if self.anchor_index.insert(anchor, id).is_some() {
                return Err(SchemaFailure::DuplicateIdentifier);
            }
            self.anchors.push((base.clone(), name.to_owned(), id));
            return Ok(base);
        }
        let target = Reference::parse(text)
            .and_then(|reference| reference.resolve(&base))
            .ok_or(SchemaFailure::Identifier)?
            .normalize();
        if target
            .fragment()
            .is_some_and(|fragment| !fragment.is_empty())
        {
            return Err(SchemaFailure::Identifier);
        }
        let resource = target.without_fragment();
        self.register(resource.clone(), id)?;
        Ok(resource)
    }

    fn register(&mut self, resource: Reference, id: NodeId) -> Result<(), SchemaFailure> {
        if self.resource_index.insert(resource.clone(), id).is_some() {
            return Err(SchemaFailure::DuplicateIdentifier);
        }
        self.resources.push((resource, id));
        Ok(())
    }

    /// The node of a subschema value, which must be a node of the document.
    fn id_of(&self, value: &Value) -> Result<NodeId, SchemaFailure> {
        self.addresses
            .get(&std::ptr::from_ref(value))
            .copied()
            .ok_or(SchemaFailure::Keyword)
    }

    /// Read one node's keywords. A `$ref` is resolved later; until then the
    /// node names itself. Only the root can be neither an object nor a
    /// boolean.
    fn node(&self, value: &Value) -> Result<Node, SchemaFailure> {
        match value {
            Value::Bool(boolean) => Ok(Node::Boolean(*boolean)),
            Value::Object(object) if object.contains_key("$ref") => {
                Ok(Node::Reference(self.id_of(value)?))
            }
            Value::Object(object) => Ok(Node::Keywords(Box::new(self.keywords(object)?))),
            _ => Err(SchemaFailure::NotASchema),
        }
    }

    fn keywords(&self, object: &Map<String, Value>) -> Result<Keywords, SchemaFailure> {
        let mut keywords = Keywords::default();
        let (mut test, mut then, mut otherwise) = (None, None, None);
        for (name, value) in object {
            match name.as_str() {
                "type" => keywords.types = Some(types(value)?),
                "enum" => {
                    let items = value.as_array().ok_or(SchemaFailure::Keyword)?;
                    keywords.enumeration = Some(
                        items
                            .iter()
                            .map(|item| key(item).ok_or(SchemaFailure::Keyword))
                            .collect::<Result<_, _>>()?,
                    );
                }
                "const" => keywords.constant = Some(key(value).ok_or(SchemaFailure::Keyword)?),
                "multipleOf" => {
                    keywords.multiple_of =
                        Some(decimal(value)?.divisor().map_err(SchemaFailure::Divisor)?);
                }
                "maximum" => keywords.maximum = Some(decimal(value)?),
                "exclusiveMaximum" => keywords.exclusive_maximum = Some(decimal(value)?),
                "minimum" => keywords.minimum = Some(decimal(value)?),
                "exclusiveMinimum" => keywords.exclusive_minimum = Some(decimal(value)?),
                "maxLength" => keywords.max_length = Some(count(value)?),
                "minLength" => keywords.min_length = Some(count(value)?),
                "pattern" => keywords.pattern = Some(pattern(value)?),
                "items" => {
                    keywords.items = Some(match value {
                        Value::Array(items) => Items::Each(self.ids_of(items)?),
                        _ => Items::All(self.id_of(value)?),
                    });
                }
                "additionalItems" => keywords.additional_items = Some(self.id_of(value)?),
                "maxItems" => keywords.max_items = Some(count(value)?),
                "minItems" => keywords.min_items = Some(count(value)?),
                "uniqueItems" => {
                    keywords.unique_items = value.as_bool().ok_or(SchemaFailure::Keyword)?;
                }
                "contains" => keywords.contains = Some(self.id_of(value)?),
                "maxProperties" => keywords.max_properties = Some(count(value)?),
                "minProperties" => keywords.min_properties = Some(count(value)?),
                "required" => keywords.required = names(value)?,
                "properties" => {
                    keywords.properties = members(value)?
                        .iter()
                        .map(|(name, member)| Ok(((*name).clone(), self.id_of(member)?)))
                        .collect::<Result<_, _>>()?;
                }
                "patternProperties" => {
                    keywords.pattern_properties = members(value)?
                        .iter()
                        .map(|(name, member)| {
                            let compiled =
                                Pattern::compile(name).map_err(SchemaFailure::Pattern)?;
                            Ok((compiled, self.id_of(member)?))
                        })
                        .collect::<Result<_, _>>()?;
                }
                "additionalProperties" => {
                    keywords.additional_properties = Some(self.id_of(value)?);
                }
                "dependencies" => {
                    keywords.dependencies = members(value)?
                        .iter()
                        .map(|(name, member)| {
                            let dependency = match member {
                                Value::Array(_) => Dependency::Names(names(member)?),
                                _ => Dependency::Schema(self.id_of(member)?),
                            };
                            Ok(((*name).clone(), dependency))
                        })
                        .collect::<Result<_, _>>()?;
                }
                "propertyNames" => keywords.property_names = Some(self.id_of(value)?),
                "if" => test = Some(self.id_of(value)?),
                "then" => then = Some(self.id_of(value)?),
                "else" => otherwise = Some(self.id_of(value)?),
                "allOf" => keywords.all_of = self.ids_of(array(value)?)?,
                "anyOf" => keywords.any_of = self.ids_of(array(value)?)?,
                "oneOf" => keywords.one_of = self.ids_of(array(value)?)?,
                "not" => keywords.not = Some(self.id_of(value)?),
                "definitions" => {
                    for (_, member) in members(value)? {
                        self.id_of(member)?;
                    }
                }
                _ => {}
            }
        }
        // `then` and `else` apply only beside `if`.
        keywords.condition = test.map(|test| Condition {
            test,
            then,
            otherwise,
        });
        Ok(keywords)
    }

    /// The nodes of an array of subschemas, which the meta-schema requires
    /// not to be empty.
    fn ids_of(&self, values: &[Value]) -> Result<Vec<NodeId>, SchemaFailure> {
        if values.is_empty() {
            return Err(SchemaFailure::Keyword);
        }
        values.iter().map(|value| self.id_of(value)).collect()
    }

    /// The node a `$ref` names, resolved against node `id`'s base.
    fn resolve(&self, id: NodeId, reference: &Value) -> Result<NodeId, SchemaFailure> {
        let target = reference
            .as_str()
            .and_then(Reference::parse)
            .and_then(|reference| reference.resolve(&self.bases[id]))
            .ok_or(SchemaFailure::Reference)?
            .normalize();
        let resource = target.without_fragment();
        let root = *self
            .resource_index
            .get(&resource)
            .ok_or(SchemaFailure::Reference)?;
        match target.fragment() {
            None | Some("") => Ok(root),
            Some(fragment) if fragment.starts_with('/') => {
                let value = Pointer::from_fragment(fragment)
                    .and_then(|pointer| pointer.evaluate(self.values[root]))
                    .ok_or(SchemaFailure::Reference)?;
                self.addresses
                    .get(&std::ptr::from_ref(value))
                    .copied()
                    .ok_or(SchemaFailure::Reference)
            }
            Some(name) => self
                .anchor_index
                .get(&(resource, name.to_owned()))
                .copied()
                .ok_or(SchemaFailure::Reference),
        }
    }
}

fn array(value: &Value) -> Result<&[Value], SchemaFailure> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or(SchemaFailure::Keyword)
}

fn members(value: &Value) -> Result<Vec<(&String, &Value)>, SchemaFailure> {
    value
        .as_object()
        .map(|members| members.iter().collect())
        .ok_or(SchemaFailure::Keyword)
}

/// An array of names, none repeated, as `required` and a dependency's
/// names are.
fn names(value: &Value) -> Result<Vec<String>, SchemaFailure> {
    let mut seen = BTreeSet::new();
    array(value)?
        .iter()
        .map(|item| match item.as_str() {
            Some(name) if seen.insert(name) => Ok(name.to_owned()),
            _ => Err(SchemaFailure::Keyword),
        })
        .collect()
}

fn types(value: &Value) -> Result<Types, SchemaFailure> {
    let one = |value: &Value| {
        value
            .as_str()
            .and_then(Type::named)
            .ok_or(SchemaFailure::Keyword)
    };
    match value {
        // An array of types is not empty, and names no type twice.
        Value::Array(items) if !items.is_empty() => {
            items.iter().try_fold(Types::default(), |types, item| {
                let kind = one(item)?;
                if types.contains(kind) {
                    return Err(SchemaFailure::Keyword);
                }
                Ok(types.with(kind))
            })
        }
        Value::Array(_) => Err(SchemaFailure::Keyword),
        _ => Ok(Types::default().with(one(value)?)),
    }
}

fn decimal(value: &Value) -> Result<Decimal, SchemaFailure> {
    match value {
        Value::Number(number) => Decimal::parse(number.as_str()).ok_or(SchemaFailure::Keyword),
        _ => Err(SchemaFailure::Keyword),
    }
}

fn count(value: &Value) -> Result<u64, SchemaFailure> {
    decimal(value)?.to_count().ok_or(SchemaFailure::Keyword)
}

fn pattern(value: &Value) -> Result<Pattern, SchemaFailure> {
    Pattern::compile(value.as_str().ok_or(SchemaFailure::Keyword)?).map_err(SchemaFailure::Pattern)
}

/// The nodes a node applies to the same instance.
fn in_place(node: &Node) -> Vec<NodeId> {
    match node {
        Node::Boolean(_) => Vec::new(),
        Node::Reference(target) => vec![*target],
        Node::Keywords(keywords) => {
            let mut edges = Vec::new();
            edges.extend(&keywords.all_of);
            edges.extend(&keywords.any_of);
            edges.extend(&keywords.one_of);
            edges.extend(keywords.not);
            if let Some(condition) = keywords.condition {
                edges.push(condition.test);
                edges.extend(condition.then);
                edges.extend(condition.otherwise);
            }
            for (_, dependency) in &keywords.dependencies {
                if let Dependency::Schema(id) = dependency {
                    edges.push(*id);
                }
            }
            edges
        }
    }
}

/// Every node, each after every node it applies in place, or a cycle.
fn in_place_order(nodes: &[Node]) -> Result<Vec<NodeId>, SchemaFailure> {
    const UNSEEN: u8 = 0;
    const OPEN: u8 = 1;
    const DONE: u8 = 2;
    let mut state = vec![UNSEEN; nodes.len()];
    let mut order = Vec::with_capacity(nodes.len());
    for start in 0..nodes.len() {
        if state[start] != UNSEEN {
            continue;
        }
        state[start] = OPEN;
        let mut stack = vec![(start, in_place(&nodes[start]), 0_usize)];
        while let Some((node, edges, next)) = stack.last_mut() {
            let Some(&successor) = edges.get(*next) else {
                state[*node] = DONE;
                order.push(*node);
                stack.pop();
                continue;
            };
            *next += 1;
            match state[successor] {
                UNSEEN => {
                    state[successor] = OPEN;
                    let edges = in_place(&nodes[successor]);
                    stack.push((successor, edges, 0));
                }
                OPEN => return Err(SchemaFailure::Cycle),
                _ => {}
            }
        }
    }
    Ok(order)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::construction::meta_schema::DRAFT_7_META_SCHEMA;
    use crate::construction::validator::pattern::Construct;

    fn compiled(document: &Value) -> Schema {
        Schema::compile(document)
            .unwrap_or_else(|failure| panic!("{document} fails with {failure:?}"))
    }

    fn failure(document: &Value) -> SchemaFailure {
        match Schema::compile(document) {
            Ok(_) => panic!("{document} compiles"),
            Err(failure) => failure,
        }
    }

    fn id_at(schema: &Schema, location: &str) -> NodeId {
        (0..schema.node_count())
            .find(|id| schema.location(*id) == location)
            .unwrap_or_else(|| panic!("no node at {location:?}"))
    }

    /// The location of the node the `$ref` at `location` names.
    fn target<'s>(schema: &'s Schema, location: &str) -> &'s str {
        match schema.node(id_at(schema, location)) {
            Node::Reference(target) => schema.location(*target),
            node => panic!("{location:?} is {node:?}"),
        }
    }

    fn keywords_at<'s>(schema: &'s Schema, location: &str) -> &'s Keywords {
        match schema.node(id_at(schema, location)) {
            Node::Keywords(keywords) => keywords,
            node => panic!("{location:?} is {node:?}"),
        }
    }

    #[test]
    fn the_census_counts_every_schema_position_of_any_json() {
        for (document, nodes, references) in [
            (json!(5), 0, 0),
            (json!(null), 0, 0),
            (json!([{}]), 0, 0),
            (json!(true), 1, 0),
            (json!({}), 1, 0),
            (json!({"$ref": "#"}), 1, 1),
            (json!({"$ref": 5}), 1, 1),
            (json!({"properties": {"$ref": {"type": "string"}}}), 2, 0),
            (
                json!({"$ref": "#", "properties": {"a": {}, "b": {"$ref": "#"}}}),
                3,
                2,
            ),
            (json!({"items": [true, {}, 5]}), 3, 0),
            (json!({"items": {"items": false}}), 3, 0),
            (
                json!({"dependencies": {"a": ["b"], "c": {}, "d": true}}),
                3,
                0,
            ),
            (
                json!({"enum": [{}, true], "const": {}, "default": {}, "examples": [{}]}),
                1,
                0,
            ),
            (json!({"unknown": {"$ref": "#"}, "x-items": {}}), 1, 0),
            (json!({"definitions": {"a": {"$ref": "#"}, "b": 1}}), 2, 1),
            (
                json!({"allOf": [{}, {}], "anyOf": [true], "oneOf": [{"not": {}}], "not": "x"}),
                6,
                0,
            ),
            (
                json!({
                    "additionalItems": {}, "additionalProperties": {}, "contains": {},
                    "else": {}, "if": {}, "not": {}, "propertyNames": {}, "then": {}
                }),
                9,
                0,
            ),
            (
                json!({"patternProperties": {"^a": {}}, "allOf": 5, "properties": []}),
                2,
                0,
            ),
        ] {
            assert_eq!(
                census(&document),
                Census { nodes, references },
                "{document}"
            );
        }
    }

    #[test]
    fn the_meta_schema_reads_into_its_seventy_nodes() {
        let document: Value =
            serde_json::from_str(DRAFT_7_META_SCHEMA).expect("the meta-schema is JSON");
        assert_eq!(
            census(&document),
            Census {
                nodes: 70,
                references: 29
            }
        );
        let schema = compiled(&document);
        assert_eq!(schema.node_count(), 70);
        let resources: Vec<(String, NodeId)> = schema
            .resources()
            .iter()
            .map(|(uri, id)| (uri.to_string(), *id))
            .collect();
        assert_eq!(
            resources,
            [("http://json-schema.org/draft-07/schema".to_owned(), 0)]
        );
        assert!(schema.anchors().is_empty());
        let references = (0..schema.node_count())
            .filter(|id| matches!(schema.node(*id), Node::Reference(_)))
            .count();
        assert_eq!(references, 29);
        assert_eq!(target(&schema, "/properties/additionalItems"), "");
        assert_eq!(
            target(&schema, "/properties/allOf"),
            "/definitions/schemaArray"
        );
        assert_eq!(
            target(&schema, "/definitions/nonNegativeIntegerDefault0/allOf/0"),
            "/definitions/nonNegativeInteger"
        );
    }

    #[test]
    fn nodes_take_their_places_in_document_order() {
        let document = json!({
            "properties": {"a/b": {"items": [true, {}]}, "c~d": false},
            "not": {},
            "definitions": {"e": {"if": {}}}
        });
        let schema = compiled(&document);
        let locations: Vec<&str> = (0..schema.node_count())
            .map(|id| schema.location(id))
            .collect();
        assert_eq!(
            locations,
            [
                "",
                "/properties/a~1b",
                "/properties/a~1b/items/0",
                "/properties/a~1b/items/1",
                "/properties/c~0d",
                "/not",
                "/definitions/e",
                "/definitions/e/if",
            ]
        );
        assert!(matches!(
            compiled(&json!(false)).node(0),
            Node::Boolean(false)
        ));
        assert!(matches!(
            compiled(&json!(true)).node(0),
            Node::Boolean(true)
        ));
    }

    /// Draft 7 core 8.2.4's example, with a `$ref` to each URI it lists.
    #[test]
    fn the_draft_7_identification_example_names_each_subschema() {
        let uris = [
            ("http://example.com/root.json", ""),
            ("http://example.com/root.json#", ""),
            ("http://example.com/root.json#foo", "/definitions/A"),
            (
                "http://example.com/root.json#/definitions/A",
                "/definitions/A",
            ),
            ("http://example.com/other.json", "/definitions/B"),
            ("http://example.com/other.json#", "/definitions/B"),
            (
                "http://example.com/root.json#/definitions/B",
                "/definitions/B",
            ),
            (
                "http://example.com/other.json#bar",
                "/definitions/B/definitions/X",
            ),
            (
                "http://example.com/other.json#/definitions/X",
                "/definitions/B/definitions/X",
            ),
            (
                "http://example.com/root.json#/definitions/B/definitions/X",
                "/definitions/B/definitions/X",
            ),
            (
                "http://example.com/t/inner.json",
                "/definitions/B/definitions/Y",
            ),
            (
                "http://example.com/t/inner.json#",
                "/definitions/B/definitions/Y",
            ),
            (
                "http://example.com/other.json#/definitions/Y",
                "/definitions/B/definitions/Y",
            ),
            (
                "http://example.com/root.json#/definitions/B/definitions/Y",
                "/definitions/B/definitions/Y",
            ),
            (
                "urn:uuid:ee564b8a-7a87-4125-8c96-e9f123d6766f",
                "/definitions/C",
            ),
            (
                "urn:uuid:ee564b8a-7a87-4125-8c96-e9f123d6766f#",
                "/definitions/C",
            ),
            (
                "http://example.com/root.json#/definitions/C",
                "/definitions/C",
            ),
        ];
        let references: Vec<Value> = uris.iter().map(|(uri, _)| json!({ "$ref": uri })).collect();
        let document = json!({
            "$id": "http://example.com/root.json",
            "definitions": {
                "A": { "$id": "#foo" },
                "B": {
                    "$id": "other.json",
                    "definitions": {
                        "X": { "$id": "#bar" },
                        "Y": { "$id": "t/inner.json" }
                    }
                },
                "C": { "$id": "urn:uuid:ee564b8a-7a87-4125-8c96-e9f123d6766f" },
                "references": { "allOf": references }
            }
        });
        let schema = compiled(&document);
        for (index, (uri, location)) in uris.iter().enumerate() {
            let at = format!("/definitions/references/allOf/{index}");
            assert_eq!(target(&schema, &at), *location, "{uri}");
        }
        let resources: Vec<(String, &str)> = schema
            .resources()
            .iter()
            .map(|(uri, id)| (uri.to_string(), schema.location(*id)))
            .collect();
        assert_eq!(
            resources,
            [
                ("http://example.com/root.json".to_owned(), ""),
                ("http://example.com/other.json".to_owned(), "/definitions/B"),
                (
                    "http://example.com/t/inner.json".to_owned(),
                    "/definitions/B/definitions/Y"
                ),
                (
                    "urn:uuid:ee564b8a-7a87-4125-8c96-e9f123d6766f".to_owned(),
                    "/definitions/C"
                ),
            ]
        );
        let anchors: Vec<(String, &str, &str)> = schema
            .anchors()
            .iter()
            .map(|(uri, name, id)| (uri.to_string(), name.as_str(), schema.location(*id)))
            .collect();
        assert_eq!(
            anchors,
            [
                (
                    "http://example.com/root.json".to_owned(),
                    "foo",
                    "/definitions/A"
                ),
                (
                    "http://example.com/other.json".to_owned(),
                    "bar",
                    "/definitions/B/definitions/X"
                ),
            ]
        );
    }

    #[test]
    fn a_reference_resolves_within_its_document() {
        let document = json!({
            "$id": "http://example.com/schemas/root.json",
            "definitions": {
                "a/b": {"type": "string"},
                "c%d": {"type": "number"},
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
                "boolean": {"$ref": "#/definitions/yes"},
                "item": {"$ref": "#/definitions/tuple/items/1"},
                "anchor": {"$ref": "nested/item.json#Inner"},
                "spelled": {"$ref": "HTTP://Example.COM/schemas/nested/item.json#%49nner"},
                "root": {"$ref": "#"}
            }
        });
        let schema = compiled(&document);
        for (location, expected) in [
            (
                "/definitions/nested/definitions/sibling",
                "/definitions/nested/definitions/inner",
            ),
            (
                "/definitions/relative",
                "/definitions/nested/definitions/inner",
            ),
            ("/definitions/parent", "/definitions/yes"),
            ("/definitions/escaped", "/definitions/a~1b"),
            ("/definitions/percent", "/definitions/c%d"),
            ("/definitions/boolean", "/definitions/yes"),
            ("/definitions/item", "/definitions/tuple/items/1"),
            (
                "/definitions/anchor",
                "/definitions/nested/definitions/inner",
            ),
            (
                "/definitions/spelled",
                "/definitions/nested/definitions/inner",
            ),
            ("/definitions/root", ""),
        ] {
            assert_eq!(target(&schema, location), expected, "{location}");
        }
    }

    #[test]
    fn an_object_with_a_reference_ignores_its_other_keywords() {
        // `$id`, `$schema`, and a malformed `type` beside `$ref` take no part,
        // and the subschemas beside it are nodes with the parent's base.
        let document = json!({
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
        });
        let schema = compiled(&document);
        assert_eq!(target(&schema, "/definitions/a"), "/definitions/b");
        assert_eq!(
            target(&schema, "/definitions/a/definitions/c"),
            "/definitions/b"
        );
        assert_eq!(schema.resources().len(), 1);
        // An anchor beside `$ref` names nothing.
        let anchored = json!({
            "definitions": {
                "a": {"$ref": "#/definitions/b", "$id": "#x"},
                "b": {},
                "c": {"$ref": "#x"}
            }
        });
        assert_eq!(failure(&anchored), SchemaFailure::Reference);
    }

    #[test]
    fn identifiers_name_resources_and_anchors_once() {
        // A root that names no resource is the default base's.
        let schema = compiled(&json!({
            "$id": "#top",
            "definitions": {
                "a": {"$ref": "#top"},
                "b": {"$ref": "json-schema:///#/definitions/a"}
            }
        }));
        assert_eq!(schema.resources()[0].0.to_string(), DEFAULT_BASE);
        assert_eq!(target(&schema, "/definitions/a"), "");
        assert_eq!(target(&schema, "/definitions/b"), "/definitions/a");
        // An empty fragment names the resource.
        let schema = compiled(&json!({"$id": "http://example.com/a.json#"}));
        assert_eq!(
            schema.resources()[0].0.to_string(),
            "http://example.com/a.json"
        );
        // One name in two resources is two anchors.
        let schema = compiled(&json!({
            "definitions": {
                "a": {"$id": "#x"},
                "b": {"$id": "other.json", "definitions": {"c": {"$id": "#x"}}}
            }
        }));
        assert_eq!(schema.anchors().len(), 2);
        for (document, expected) in [
            (json!({"$id": 5}), SchemaFailure::Identifier),
            (json!({"$id": "a b"}), SchemaFailure::Identifier),
            (json!({"$id": "#"}), SchemaFailure::Identifier),
            (json!({"$id": "#1a"}), SchemaFailure::Identifier),
            (json!({"$id": "#a b"}), SchemaFailure::Identifier),
            (json!({"$id": "#/definitions/a"}), SchemaFailure::Identifier),
            (
                json!({"$id": "http://example.com/a.json#frag"}),
                SchemaFailure::Identifier,
            ),
            (
                json!({"definitions": {"a": {"$id": "#x"}, "b": {"$id": "#x"}}}),
                SchemaFailure::DuplicateIdentifier,
            ),
            (
                json!({"$id": "http://example.com/a.json", "definitions": {"a": {"$id": "a.json"}}}),
                SchemaFailure::DuplicateIdentifier,
            ),
            (
                json!({"definitions": {"a": {"$id": "json-schema:///"}}}),
                SchemaFailure::DuplicateIdentifier,
            ),
            (
                json!({"definitions": {"a": {"$id": "HTTP://Example.com/x"}, "b": {"$id": "http://example.com/%78"}}}),
                SchemaFailure::DuplicateIdentifier,
            ),
        ] {
            assert_eq!(failure(&document), expected, "{document}");
        }
    }

    #[test]
    fn a_plain_name_is_a_letter_then_letters_digits_and_four_marks() {
        for name in ["a", "Z", "a1", "a-b_c:d.e", "x.Y-0"] {
            assert!(is_plain_name(name), "{name:?}");
        }
        for name in [
            "", "1a", "-a", "_a", ":a", ".a", "a b", "a/b", "a%41", "a#b", "a\u{e9}", "\u{e9}",
        ] {
            assert!(!is_plain_name(name), "{name:?}");
        }
    }

    #[test]
    fn only_the_root_names_its_dialect_and_only_draft_7() {
        assert!(Schema::compile(&json!({"$schema": DRAFT_7})).is_ok());
        assert!(Schema::compile(&json!({
            "$schema": DRAFT_7,
            "definitions": {"a": {"$ref": "#", "$schema": "x"}}
        }))
        .is_ok());
        for document in [
            json!({"$schema": "http://json-schema.org/draft-04/schema#"}),
            json!({"$schema": "http://json-schema.org/draft-07/schema"}),
            json!({"$schema": 7}),
            json!({"definitions": {"a": {"$schema": DRAFT_7}}}),
            json!({"$schema": DRAFT_7, "properties": {"a": {"$schema": DRAFT_7}}}),
        ] {
            assert_eq!(failure(&document), SchemaFailure::Dialect, "{document}");
        }
    }

    #[test]
    fn a_root_that_is_neither_an_object_nor_a_boolean_is_not_a_schema() {
        for document in [json!(5), json!("x"), json!(null), json!([]), json!([{}])] {
            assert_eq!(failure(&document), SchemaFailure::NotASchema, "{document}");
        }
    }

    #[test]
    fn a_keyword_of_the_wrong_shape_is_refused() {
        for document in [
            json!({"type": "integer-ish"}),
            json!({"type": 5}),
            json!({"type": ["string", 5]}),
            json!({"enum": {}}),
            json!({"multipleOf": "1"}),
            json!({"maximum": "1"}),
            json!({"exclusiveMaximum": null}),
            json!({"minimum": []}),
            json!({"exclusiveMinimum": {}}),
            json!({"maxLength": -1}),
            json!({"minLength": 1.5}),
            json!({"pattern": 5}),
            json!({"items": 5}),
            json!({"items": [{}, 5]}),
            json!({"additionalItems": 5}),
            json!({"maxItems": "1"}),
            json!({"minItems": -0.5}),
            json!({"uniqueItems": 1}),
            json!({"contains": []}),
            json!({"maxProperties": true}),
            json!({"minProperties": 0.1}),
            json!({"required": "a"}),
            json!({"required": ["a", 1]}),
            json!({"properties": []}),
            json!({"properties": {"a": 1}}),
            json!({"patternProperties": {"a": "b"}}),
            json!({"additionalProperties": "no"}),
            json!({"dependencies": []}),
            json!({"dependencies": {"a": "b"}}),
            json!({"dependencies": {"a": [1]}}),
            json!({"propertyNames": 1}),
            json!({"if": 1}),
            json!({"then": 1}),
            json!({"else": 1}),
            json!({"allOf": {}}),
            json!({"anyOf": [1]}),
            json!({"oneOf": "a"}),
            json!({"not": null}),
            json!({"definitions": []}),
            json!({"definitions": {"a": 1}}),
            // The meta-schema's arrays: not empty, and nothing repeated.
            json!({"type": []}),
            json!({"type": ["string", "null", "string"]}),
            json!({"allOf": []}),
            json!({"anyOf": []}),
            json!({"oneOf": []}),
            json!({"items": []}),
            json!({"required": ["a", "b", "a"]}),
            json!({"dependencies": {"a": ["b", "b"]}}),
        ] {
            assert_eq!(failure(&document), SchemaFailure::Keyword, "{document}");
        }
        // The meta-schema lets these be empty, and `enum` repeat.
        let schema = compiled(&json!({
            "required": [],
            "dependencies": {"a": []},
            "enum": [],
            "items": {"enum": [1, 1.0, 1]}
        }));
        let keywords = keywords_at(&schema, "");
        assert!(keywords.required.is_empty());
        assert_eq!(
            keywords.dependencies,
            [("a".to_owned(), Dependency::Names(Vec::new()))]
        );
        assert_eq!(keywords.enumeration.as_ref().map(BTreeSet::len), Some(0));
        assert_eq!(
            keywords_at(&schema, "/items")
                .enumeration
                .as_ref()
                .map(BTreeSet::len),
            Some(1)
        );
        // Annotations and unknown keywords take any value.
        assert!(Schema::compile(&json!({
            "title": 1, "description": [], "default": {}, "examples": 5, "format": 1,
            "contentMediaType": 1, "contentEncoding": 1, "$comment": 1, "readOnly": 1,
            "writeOnly": 1, "unknown": {"$ref": 5}
        }))
        .is_ok());
    }

    #[test]
    fn a_pattern_or_a_divisor_that_does_not_compile_is_refused() {
        assert_eq!(
            failure(&json!({"pattern": "("})),
            SchemaFailure::Pattern(PatternFailure::Syntax)
        );
        assert_eq!(
            failure(&json!({"patternProperties": {"(?=a)": {}}})),
            SchemaFailure::Pattern(PatternFailure::Unsupported(Construct::Lookaround))
        );
        assert_eq!(
            failure(&json!({"multipleOf": 0})),
            SchemaFailure::Divisor(DivisorFailure::NotPositive)
        );
        let digits: Value =
            serde_json::from_str(r#"{"multipleOf": 1.2345678901234567891}"#).expect("JSON");
        assert_eq!(
            failure(&digits),
            SchemaFailure::Divisor(DivisorFailure::Digits)
        );
    }

    #[test]
    fn a_reference_that_names_no_node_is_refused() {
        for document in [
            json!({"$ref": 5}),
            json!({"definitions": {"a": {"$ref": "a b"}}}),
            json!({"definitions": {"a": {"$ref": "other.json"}}}),
            json!({"definitions": {"a": {"$ref": "#/definitions/missing"}}}),
            json!({"definitions": {"a": {"$ref": "#/definitions"}}}),
            json!({"enum": [{}], "definitions": {"a": {"$ref": "#/enum/0"}}}),
            json!({"x": {}, "definitions": {"a": {"$ref": "#/x"}}}),
            json!({"definitions": {"a": {"$ref": "#nope"}}}),
            json!({"definitions": {"a": {"$ref": "#/definitions/%FF"}}}),
            json!({"definitions": {"a": {"$ref": "#/definitions/a~2"}}}),
            json!({"definitions": {"a": {"$ref": "http://json-schema.org/draft-07/schema#"}}}),
            json!({"items": [{}], "definitions": {"a": {"$ref": "#/items/01"}}}),
        ] {
            assert_eq!(failure(&document), SchemaFailure::Reference, "{document}");
        }
    }

    #[test]
    fn nodes_that_apply_one_another_in_place_in_a_cycle_are_refused() {
        for document in [
            json!({"$ref": "#"}),
            json!({"allOf": [{"$ref": "#"}]}),
            json!({"anyOf": [{"$ref": "#"}]}),
            json!({"oneOf": [{"$ref": "#"}]}),
            json!({"not": {"$ref": "#"}}),
            json!({"if": {"$ref": "#"}}),
            json!({"if": {}, "then": {"$ref": "#"}}),
            json!({"if": {}, "else": {"$ref": "#"}}),
            json!({"dependencies": {"a": {"$ref": "#"}}}),
            json!({"definitions": {"a": {"$ref": "#/definitions/a"}}}),
            json!({
                "definitions": {
                    "a": {"$ref": "#/definitions/b"},
                    "b": {"allOf": [{"$ref": "#/definitions/a"}]}
                }
            }),
        ] {
            assert_eq!(failure(&document), SchemaFailure::Cycle, "{document}");
        }
        // Applicators that move to a member, an item, or a name end.
        for document in [
            json!({"properties": {"a": {"$ref": "#"}}}),
            json!({"items": {"$ref": "#"}}),
            json!({"items": [{"$ref": "#"}], "additionalItems": {"$ref": "#"}}),
            json!({"contains": {"$ref": "#"}}),
            json!({"additionalProperties": {"$ref": "#"}}),
            json!({"patternProperties": {"a": {"$ref": "#"}}}),
            json!({"propertyNames": {"$ref": "#"}}),
            json!({"dependencies": {"a": ["b"]}, "definitions": {"b": {"$ref": "#"}}}),
            // `then` and `else` apply only beside `if`.
            json!({"then": {"$ref": "#"}, "else": {"$ref": "#"}}),
            // Reaching one node twice is not a cycle.
            json!({
                "allOf": [{"$ref": "#/definitions/a"}, {"$ref": "#/definitions/a"}],
                "definitions": {"a": {}}
            }),
        ] {
            assert!(Schema::compile(&document).is_ok(), "{document}");
        }
    }

    #[test]
    fn every_node_comes_after_the_nodes_it_applies_in_place() {
        let document = json!({
            "allOf": [{"$ref": "#/definitions/a"}, {"anyOf": [{"not": {"$ref": "#/definitions/b"}}]}],
            "if": {"$ref": "#/definitions/b"},
            "then": {},
            "else": {"oneOf": [true]},
            "dependencies": {"x": {"$ref": "#/definitions/a"}, "y": ["z"]},
            "definitions": {"a": {"allOf": [{"$ref": "#/definitions/b"}]}, "b": {}}
        });
        let schema = compiled(&document);
        let order = schema.order();
        let mut sorted = order.to_vec();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..schema.node_count()).collect::<Vec<_>>());
        let mut place = vec![0; order.len()];
        for (index, id) in order.iter().enumerate() {
            place[*id] = index;
        }
        let mut edges = 0;
        for id in 0..schema.node_count() {
            for successor in in_place(schema.node(id)) {
                edges += 1;
                assert!(
                    place[successor] < place[id],
                    "{} before {}",
                    schema.location(successor),
                    schema.location(id)
                );
            }
        }
        // The root's six, and one from each of nine more nodes.
        assert_eq!(edges, 15);
    }

    #[test]
    fn a_document_fails_at_the_first_check_in_order() {
        // Structure before keywords.
        assert_eq!(
            failure(&json!({"type": 5, "definitions": {"a": {"$schema": DRAFT_7}}})),
            SchemaFailure::Dialect
        );
        assert_eq!(
            failure(&json!({"type": 5, "$id": 5})),
            SchemaFailure::Identifier
        );
        // Keywords before references.
        assert_eq!(
            failure(&json!({"definitions": {"a": {"$ref": "#/missing"}, "b": {"type": 5}}})),
            SchemaFailure::Keyword
        );
        // References before cycles.
        assert_eq!(
            failure(
                &json!({"allOf": [{"$ref": "#"}], "definitions": {"a": {"$ref": "#/missing"}}})
            ),
            SchemaFailure::Reference
        );
        // Within a check, document order.
        assert_eq!(
            failure(&json!({"definitions": {"a": {"pattern": "("}, "b": {"multipleOf": 0}}})),
            SchemaFailure::Pattern(PatternFailure::Syntax)
        );
        assert_eq!(
            failure(&json!({"definitions": {"a": {"multipleOf": 0}, "b": {"pattern": "("}}})),
            SchemaFailure::Divisor(DivisorFailure::NotPositive)
        );
    }

    #[test]
    fn keywords_read_into_their_values() {
        let document: Value = serde_json::from_str(
            r#"{
                "type": ["integer", "string"],
                "enum": [1, 1.0, "a"],
                "const": {"b": [true]},
                "multipleOf": 0.5,
                "maximum": 10,
                "exclusiveMaximum": 1e1,
                "minimum": -2.50,
                "exclusiveMinimum": -3,
                "maxLength": 5,
                "minLength": 1.0,
                "pattern": "^a",
                "items": [{}, true],
                "additionalItems": false,
                "maxItems": 4,
                "minItems": 2,
                "uniqueItems": true,
                "contains": {},
                "maxProperties": 3,
                "minProperties": 1,
                "required": ["a", "b"],
                "properties": {"a": {}, "b": false},
                "patternProperties": {"^x": {}},
                "additionalProperties": true,
                "dependencies": {"a": ["b"], "c": {}},
                "propertyNames": {"maxLength": 3},
                "if": {},
                "then": {},
                "else": {},
                "allOf": [{}],
                "anyOf": [{}, {}],
                "oneOf": [{}],
                "not": {},
                "format": "date",
                "title": "t"
            }"#,
        )
        .expect("JSON");
        let schema = compiled(&document);
        let at = |location: &str| id_at(&schema, location);
        let keywords = keywords_at(&schema, "");
        let types = keywords.types.expect("a type");
        assert!(types.contains(Type::Integer) && types.contains(Type::String));
        for kind in [
            Type::Array,
            Type::Boolean,
            Type::Null,
            Type::Number,
            Type::Object,
        ] {
            assert!(!types.contains(kind), "{kind:?}");
        }
        assert_eq!(keywords.enumeration.as_ref().map(BTreeSet::len), Some(2));
        assert_eq!(keywords.constant, key(&json!({"b": [true]})));
        assert_eq!(
            keywords.multiple_of,
            Decimal::parse("0.5").and_then(|decimal| decimal.divisor().ok())
        );
        assert_eq!(keywords.maximum, Decimal::parse("10"));
        assert_eq!(keywords.exclusive_maximum, Decimal::parse("10"));
        assert_eq!(keywords.minimum, Decimal::parse("-2.5"));
        assert_eq!(keywords.exclusive_minimum, Decimal::parse("-3"));
        assert_eq!(
            (keywords.max_length, keywords.min_length),
            (Some(5), Some(1))
        );
        assert_eq!(keywords.pattern, Pattern::compile("^a").ok());
        assert_eq!(
            keywords.items,
            Some(Items::Each(vec![at("/items/0"), at("/items/1")]))
        );
        assert_eq!(keywords.additional_items, Some(at("/additionalItems")));
        assert_eq!(
            (
                keywords.max_items,
                keywords.min_items,
                keywords.unique_items
            ),
            (Some(4), Some(2), true)
        );
        assert_eq!(keywords.contains, Some(at("/contains")));
        assert_eq!(
            (keywords.max_properties, keywords.min_properties),
            (Some(3), Some(1))
        );
        assert_eq!(keywords.required, ["a", "b"]);
        assert_eq!(
            keywords.properties,
            [
                ("a".to_owned(), at("/properties/a")),
                ("b".to_owned(), at("/properties/b"))
            ]
        );
        assert_eq!(
            keywords.pattern_properties,
            [(
                Pattern::compile("^x").expect("a pattern"),
                at("/patternProperties/^x")
            )]
        );
        assert_eq!(
            keywords.additional_properties,
            Some(at("/additionalProperties"))
        );
        assert_eq!(
            keywords.dependencies,
            [
                ("a".to_owned(), Dependency::Names(vec!["b".to_owned()])),
                ("c".to_owned(), Dependency::Schema(at("/dependencies/c")))
            ]
        );
        assert_eq!(keywords.property_names, Some(at("/propertyNames")));
        assert_eq!(
            keywords.condition,
            Some(Condition {
                test: at("/if"),
                then: Some(at("/then")),
                otherwise: Some(at("/else"))
            })
        );
        assert_eq!(keywords.all_of, [at("/allOf/0")]);
        assert_eq!(keywords.any_of, [at("/anyOf/0"), at("/anyOf/1")]);
        assert_eq!(keywords.one_of, [at("/oneOf/0")]);
        assert_eq!(keywords.not, Some(at("/not")));
        assert_eq!(keywords_at(&schema, "/propertyNames").max_length, Some(3));
        // Without `if`, `then` and `else` are nodes that nothing applies.
        let schema = compiled(&json!({"then": {}, "else": {}, "items": {}}));
        let keywords = keywords_at(&schema, "");
        assert_eq!(keywords.condition, None);
        assert_eq!(keywords.items, Some(Items::All(id_at(&schema, "/items"))));
        assert_eq!(schema.node_count(), 4);
        // An absent `uniqueItems` holds no constraint.
        assert!(!keywords.unique_items);
    }
}
