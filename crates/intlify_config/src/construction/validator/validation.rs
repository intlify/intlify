// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Draft 7 validation of an instance against a read schema, counted in
//! `(schema node, instance node)` pairs.
//!
//! A validation visits each instance node once. The schema nodes it applies
//! to a node are its seeds, with every node they apply to the same instance:
//! the node a `$ref` names, the members of `allOf`, `anyOf`, and `oneOf`,
//! `not`, `if` with its `then` and `else`, and the schema of each dependency
//! whose name the object has. The root's seed is the schema's root, and any
//! other node's are the nodes that the nodes applied to its parent name for it
//! through `properties`, `patternProperties`, `additionalProperties`,
//! `items`, `additionalItems`, and `contains`. The children are visited
//! first, and then the applied nodes are decided in the schema's order, each
//! after the nodes it applies in place. Every branch is decided whatever the
//! others decide, `then` and `else` alike, so that what the nodes ask of the
//! children is known before the children are visited, once each. A child no
//! node applies to is not visited.
//!
//! `propertyNames` applies its node to the name of each member, which is a
//! string but not a node of the instance, so a node applied to a name is
//! counted with the member's value. A validation's pairs are, for each
//! instance node, the nodes applied to its value or to its name, each once.
//! There are at most the schema's nodes times the instance's nodes of them,
//! the product design 015 counts before the validation, and each pair decides
//! its node at most twice.
//!
//! A validation only answers whether the instance is valid: design 015 lets a
//! failed construction carry no details but the bound it exceeded.

use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use super::equal::{all_unique, key, Key};
use super::number::Decimal;
use super::schema::{in_place, Dependency, Items, Keywords, Node, NodeId, Schema, Type, Types};

/// What one validation found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Validation {
    /// Whether the instance is valid against the schema.
    pub(crate) valid: bool,
    /// The `(schema node, instance node)` pairs it decided: design 015's
    /// `validation-pair`.
    pub(crate) pairs: u64,
}

/// Count the nodes of an instance: every JSON value, object keys aside, as
/// design 015's `json-node` counts them.
pub(crate) fn instance_nodes(instance: &Value) -> u64 {
    let mut nodes = 0;
    let mut pending = vec![instance];
    while let Some(value) = pending.pop() {
        nodes += 1;
        match value {
            Value::Array(items) => pending.extend(items),
            Value::Object(members) => pending.extend(members.values()),
            _ => {}
        }
    }
    nodes
}

impl Schema {
    /// Validate `instance` against this schema.
    pub(crate) fn validate(&self, instance: &Value) -> Validation {
        let mut evaluation = Evaluation {
            schema: self,
            pairs: 0,
        };
        let results = evaluation.visit(instance, BTreeSet::from([0]));
        Validation {
            valid: result(&results, 0),
            pairs: evaluation.pairs,
        }
    }
}

/// The result of each node applied to one value or name.
type Results = BTreeMap<NodeId, bool>;

fn result(results: &Results, id: NodeId) -> bool {
    *results.get(&id).expect("every node applied has a result")
}

/// The results of the nodes applied to a member's value and to its name.
struct Member {
    value: Results,
    name: Results,
}

/// What an instance's children found: the results of each item, or of each
/// member.
enum Children {
    None,
    Items(Vec<Results>),
    Members(Vec<Member>),
}

struct Evaluation<'a> {
    schema: &'a Schema,
    pairs: u64,
}

impl<'a> Evaluation<'a> {
    /// Apply `seeds` to `value`, and return the result of each node applied.
    fn visit(&mut self, value: &Value, seeds: BTreeSet<NodeId>) -> Results {
        if seeds.is_empty() {
            return Results::new();
        }
        let applied = self.applied(seeds, value.as_object());
        self.pairs += applied.len() as u64;
        let children = match value {
            Value::Array(items) => Children::Items(self.items(&applied, items)),
            Value::Object(members) => Children::Members(self.members(&applied, members)),
            _ => Children::None,
        };
        self.decide(&applied, value, &children)
    }

    /// The seeds with every node they apply to the same instance, each after
    /// the nodes it applies. A name, or any other instance that is not an
    /// object, has no `object`.
    fn applied(&self, seeds: BTreeSet<NodeId>, object: Option<&Map<String, Value>>) -> Vec<NodeId> {
        let has = |name: &str| object.is_some_and(|members| members.contains_key(name));
        let mut applied = BTreeSet::new();
        let mut pending: Vec<NodeId> = seeds.into_iter().collect();
        while let Some(id) = pending.pop() {
            if applied.insert(id) {
                pending.extend(in_place(self.schema.node(id), has));
            }
        }
        let mut applied: Vec<NodeId> = applied.into_iter().collect();
        applied.sort_by_key(|&id| self.schema.rank(id));
        applied
    }

    fn keywords(&self, applied: &[NodeId]) -> Vec<&'a Keywords> {
        let schema = self.schema;
        applied
            .iter()
            .filter_map(|&id| match schema.node(id) {
                Node::Keywords(keywords) => Some(&**keywords),
                _ => None,
            })
            .collect()
    }

    fn items(&mut self, applied: &[NodeId], items: &[Value]) -> Vec<Results> {
        let applied = self.keywords(applied);
        items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let seeds = applied
                    .iter()
                    .flat_map(|keywords| {
                        item_node(keywords, index)
                            .into_iter()
                            .chain(keywords.contains)
                    })
                    .collect();
                self.visit(item, seeds)
            })
            .collect()
    }

    /// Visit each member's value, and decide the nodes `propertyNames`
    /// applies to its name, counting those not applied to its value.
    fn members(&mut self, applied: &[NodeId], members: &Map<String, Value>) -> Vec<Member> {
        let applied = self.keywords(applied);
        let naming = self.applied(
            applied
                .iter()
                .filter_map(|keywords| keywords.property_names)
                .collect(),
            None,
        );
        let mut children = Vec::with_capacity(members.len());
        for (name, member) in members {
            let seeds = applied
                .iter()
                .flat_map(|keywords| member_nodes(keywords, name))
                .collect();
            let value = self.visit(member, seeds);
            let name = if naming.is_empty() {
                Results::new()
            } else {
                self.pairs += naming.iter().filter(|id| !value.contains_key(id)).count() as u64;
                self.decide(&naming, &Value::String(name.clone()), &Children::None)
            };
            children.push(Member { value, name });
        }
        children
    }

    /// Decide each applied node in order, from the nodes it applies in place
    /// and what the children found.
    fn decide(&self, applied: &[NodeId], value: &Value, children: &Children) -> Results {
        let facts = Facts::new(value);
        let mut results = Results::new();
        for &id in applied {
            let holds = match self.schema.node(id) {
                Node::Boolean(boolean) => *boolean,
                Node::Reference(target) => result(&results, *target),
                Node::Keywords(keywords) => holds(keywords, &facts, &results, children),
            };
            results.insert(id, holds);
        }
        results
    }
}

/// The node `items` or `additionalItems` applies to the item at `index`.
fn item_node(keywords: &Keywords, index: usize) -> Option<NodeId> {
    match &keywords.items {
        Some(Items::All(node)) => Some(*node),
        Some(Items::Each(nodes)) => nodes.get(index).copied().or(keywords.additional_items),
        None => None,
    }
}

/// The nodes applied to the value of the member `name`: its property and
/// every pattern property whose pattern the name matches, or when there are
/// none, the additional properties.
fn member_nodes(keywords: &Keywords, name: &str) -> Vec<NodeId> {
    let mut nodes: Vec<NodeId> = keywords.properties.get(name).copied().into_iter().collect();
    nodes.extend(
        keywords
            .pattern_properties
            .iter()
            .filter(|(pattern, _)| pattern.matches(name))
            .map(|(_, node)| *node),
    );
    if nodes.is_empty() {
        nodes.extend(keywords.additional_properties);
    }
    nodes
}

/// What the keywords read of one instance, each read once.
struct Facts<'v> {
    value: &'v Value,
    key: OnceCell<Option<Key>>,
    number: OnceCell<Option<Decimal>>,
    length: OnceCell<u64>,
    unique: OnceCell<bool>,
}

impl<'v> Facts<'v> {
    fn new(value: &'v Value) -> Self {
        Self {
            value,
            key: OnceCell::new(),
            number: OnceCell::new(),
            length: OnceCell::new(),
            unique: OnceCell::new(),
        }
    }

    /// The value's key, which `const` and `enum` compare.
    fn key(&self) -> Option<&Key> {
        self.key.get_or_init(|| key(self.value)).as_ref()
    }

    /// A number's exact value.
    fn number(&self) -> Option<&Decimal> {
        self.number
            .get_or_init(|| match self.value {
                Value::Number(number) => Decimal::parse(number.as_str()),
                _ => None,
            })
            .as_ref()
    }

    /// A string's length in code points.
    fn length(&self) -> u64 {
        *self.length.get_or_init(|| {
            self.value
                .as_str()
                .map_or(0, |text| text.chars().count() as u64)
        })
    }

    /// Whether no two of an array's items are equal.
    fn unique(&self) -> bool {
        *self.unique.get_or_init(|| {
            self.value
                .as_array()
                .and_then(|items| all_unique(items))
                .unwrap_or(false)
        })
    }
}

/// Whether every keyword of a node holds of an instance.
fn holds(keywords: &Keywords, facts: &Facts<'_>, results: &Results, children: &Children) -> bool {
    keywords.types.is_none_or(|types| is_of(types, facts))
        && keywords
            .enumeration
            .as_ref()
            .is_none_or(|values| facts.key().is_some_and(|key| values.contains(key)))
        && keywords
            .constant
            .as_ref()
            .is_none_or(|constant| facts.key() == Some(constant))
        && match (facts.value, children) {
            (Value::Number(_), _) => facts
                .number()
                .is_some_and(|number| number_holds(keywords, number)),
            (Value::String(text), _) => string_holds(keywords, text, facts),
            (Value::Array(items), Children::Items(found)) => {
                array_holds(keywords, items.len(), found, facts)
            }
            (Value::Object(members), Children::Members(found)) => {
                object_holds(keywords, members, found, results)
            }
            _ => true,
        }
        && in_place_holds(keywords, results)
}

fn is_of(types: Types, facts: &Facts<'_>) -> bool {
    match facts.value {
        Value::Null => types.contains(Type::Null),
        Value::Bool(_) => types.contains(Type::Boolean),
        Value::Number(_) => {
            types.contains(Type::Number)
                || types.contains(Type::Integer) && facts.number().is_some_and(Decimal::is_integer)
        }
        Value::String(_) => types.contains(Type::String),
        Value::Array(_) => types.contains(Type::Array),
        Value::Object(_) => types.contains(Type::Object),
    }
}

fn number_holds(keywords: &Keywords, number: &Decimal) -> bool {
    keywords
        .multiple_of
        .as_ref()
        .is_none_or(|divisor| number.is_multiple_of(divisor))
        && keywords
            .maximum
            .as_ref()
            .is_none_or(|maximum| number <= maximum)
        && keywords
            .exclusive_maximum
            .as_ref()
            .is_none_or(|maximum| number < maximum)
        && keywords
            .minimum
            .as_ref()
            .is_none_or(|minimum| number >= minimum)
        && keywords
            .exclusive_minimum
            .as_ref()
            .is_none_or(|minimum| number > minimum)
}

fn string_holds(keywords: &Keywords, text: &str, facts: &Facts<'_>) -> bool {
    keywords.max_length.is_none_or(|max| facts.length() <= max)
        && keywords.min_length.is_none_or(|min| facts.length() >= min)
        && keywords
            .pattern
            .as_ref()
            .is_none_or(|pattern| pattern.matches(text))
}

fn array_holds(keywords: &Keywords, count: usize, items: &[Results], facts: &Facts<'_>) -> bool {
    let count = count as u64;
    keywords.max_items.is_none_or(|max| count <= max)
        && keywords.min_items.is_none_or(|min| count >= min)
        && (!keywords.unique_items || facts.unique())
        && keywords
            .contains
            .is_none_or(|node| items.iter().any(|item| result(item, node)))
        && items
            .iter()
            .enumerate()
            .all(|(index, item)| item_node(keywords, index).is_none_or(|node| result(item, node)))
}

fn object_holds(
    keywords: &Keywords,
    members: &Map<String, Value>,
    found: &[Member],
    results: &Results,
) -> bool {
    let count = members.len() as u64;
    keywords.max_properties.is_none_or(|max| count <= max)
        && keywords.min_properties.is_none_or(|min| count >= min)
        && keywords
            .required
            .iter()
            .all(|name| members.contains_key(name))
        && members.keys().zip(found).all(|(name, member)| {
            member_nodes(keywords, name)
                .into_iter()
                .all(|node| result(&member.value, node))
                && keywords
                    .property_names
                    .is_none_or(|node| result(&member.name, node))
        })
        && keywords.dependencies.iter().all(|(name, dependency)| {
            !members.contains_key(name)
                || match dependency {
                    Dependency::Names(names) => {
                        names.iter().all(|other| members.contains_key(other))
                    }
                    Dependency::Schema(node) => result(results, *node),
                }
        })
}

fn in_place_holds(keywords: &Keywords, results: &Results) -> bool {
    let holds = |node: &NodeId| result(results, *node);
    keywords.all_of.iter().all(holds)
        && (keywords.any_of.is_empty() || keywords.any_of.iter().any(holds))
        && (keywords.one_of.is_empty()
            || keywords.one_of.iter().filter(|node| holds(node)).count() == 1)
        && keywords.not.is_none_or(|node| !holds(&node))
        && keywords.condition.is_none_or(|condition| {
            if holds(&condition.test) {
                condition.then.is_none_or(|node| holds(&node))
            } else {
                condition.otherwise.is_none_or(|node| holds(&node))
            }
        })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::construction::validator::schema::census;

    fn compiled(document: &Value) -> Schema {
        Schema::compile(document).expect("a schema")
    }

    /// Validate each instance against `document`, and check the pairs stay
    /// within the product design 015 counts beforehand.
    fn validated(document: &Value, instances: &[Value]) -> Vec<Validation> {
        let schema = compiled(document);
        instances
            .iter()
            .map(|instance| {
                let validation = schema.validate(instance);
                assert!(
                    validation.pairs <= census(document).nodes * instance_nodes(instance),
                    "{instance}"
                );
                validation
            })
            .collect()
    }

    /// Whether each instance is valid against `document`.
    fn valid(document: &Value, instances: &[Value]) -> Vec<bool> {
        validated(document, instances)
            .into_iter()
            .map(|validation| validation.valid)
            .collect()
    }

    #[test]
    fn instance_nodes_are_values_and_not_names() {
        assert_eq!(instance_nodes(&json!(null)), 1);
        assert_eq!(instance_nodes(&json!([])), 1);
        assert_eq!(instance_nodes(&json!({"a": [1, {"b": null}], "c": "x"})), 6);
    }

    #[test]
    fn each_type_holds_of_its_values() {
        let values = [
            json!(null),
            json!(true),
            json!(1),
            json!(1.0),
            json!(1e2),
            json!(1.5),
            json!("1"),
            json!([1]),
            json!({"a": 1}),
        ];
        let of = |kind: &str| valid(&json!({"type": kind}), &values);
        assert_eq!(
            of("null"),
            [true, false, false, false, false, false, false, false, false]
        );
        assert_eq!(
            of("boolean"),
            [false, true, false, false, false, false, false, false, false]
        );
        // A number with no fraction is an integer, however it is written.
        assert_eq!(
            of("integer"),
            [false, false, true, true, true, false, false, false, false]
        );
        assert_eq!(
            of("number"),
            [false, false, true, true, true, true, false, false, false]
        );
        assert_eq!(
            of("string"),
            [false, false, false, false, false, false, true, false, false]
        );
        assert_eq!(
            of("array"),
            [false, false, false, false, false, false, false, true, false]
        );
        assert_eq!(
            of("object"),
            [false, false, false, false, false, false, false, false, true]
        );
        assert_eq!(
            valid(&json!({"type": ["null", "string"]}), &values),
            [true, false, false, false, false, false, true, false, false]
        );
    }

    #[test]
    fn const_and_enum_compare_values_exactly() {
        let values = [
            json!({"a": [1.0], "b": "x"}),
            json!({"b": "x", "a": [1]}),
            json!({"a": [1], "b": "x", "c": null}),
            json!("x"),
            json!(1),
        ];
        assert_eq!(
            valid(&json!({"const": {"a": [1], "b": "x"}}), &values),
            [true, true, false, false, false]
        );
        assert_eq!(
            valid(&json!({"enum": ["x", 1.0]}), &values),
            [false, false, false, true, true]
        );
        // An empty enumeration allows nothing.
        assert_eq!(valid(&json!({"enum": []}), &values), [false; 5]);
    }

    #[test]
    fn the_numeric_keywords_read_numbers_exactly() {
        let values = [
            json!(0.0075),
            json!(0.3),
            json!(10),
            json!(10.5),
            json!("10"),
        ];
        assert_eq!(
            valid(&json!({"multipleOf": 0.0001}), &values),
            [true, true, true, true, true]
        );
        assert_eq!(
            valid(&json!({"multipleOf": 0.1}), &values),
            [false, true, true, true, true]
        );
        assert_eq!(
            valid(&json!({"multipleOf": 2}), &values),
            [false, false, true, false, true]
        );
        assert_eq!(
            valid(&json!({"maximum": 10}), &values),
            [true, true, true, false, true]
        );
        assert_eq!(
            valid(&json!({"exclusiveMaximum": 10}), &values),
            [true, true, false, false, true]
        );
        assert_eq!(
            valid(&json!({"minimum": 10}), &values),
            [false, false, true, true, true]
        );
        assert_eq!(
            valid(&json!({"exclusiveMinimum": 10}), &values),
            [false, false, false, true, true]
        );
        // Exponents past any float's still compare.
        let parse = |text: &str| serde_json::from_str::<Value>(text).expect("JSON");
        assert_eq!(
            valid(
                &parse(r#"{"minimum": 1e400}"#),
                &[json!(1e300), parse("1e401")]
            ),
            [false, true]
        );
    }

    #[test]
    fn the_string_keywords_count_code_points() {
        let face = '\u{1f600}';
        let values = [
            json!(""),
            json!(format!("{face}{face}")),
            json!("abc"),
            json!("expression"),
            json!(3),
        ];
        assert_eq!(
            valid(&json!({"maxLength": 2}), &values),
            [true, true, false, false, true]
        );
        assert_eq!(
            valid(&json!({"minLength": 2}), &values),
            [false, true, true, true, true]
        );
        // A pattern is not anchored.
        assert_eq!(
            valid(&json!({"pattern": "es"}), &values),
            [false, false, false, true, true]
        );
    }

    #[test]
    fn the_array_keywords_apply_to_items() {
        let values = [
            json!([]),
            json!([1]),
            json!([1, "a"]),
            json!([1, "a", null]),
            json!([1, 1.0]),
            json!([{"a": 1, "b": 2}, {"b": 2, "a": 1}]),
            json!({"0": "a"}),
        ];
        assert_eq!(
            valid(&json!({"items": {"type": "integer"}}), &values),
            [true, true, false, false, true, false, true]
        );
        // `additionalItems` applies past an array of `items`.
        assert_eq!(
            valid(
                &json!({"items": [{"type": "integer"}, {"type": "string"}], "additionalItems": false}),
                &values
            ),
            [true, true, true, false, false, false, true]
        );
        // Beside a single `items`, or with none, it applies to nothing.
        for document in [
            json!({"items": {}, "additionalItems": false}),
            json!({"additionalItems": false}),
        ] {
            assert_eq!(valid(&document, &values), [true; 7]);
        }
        assert_eq!(
            valid(&json!({"maxItems": 1, "minItems": 1}), &values),
            [false, true, false, false, false, false, true]
        );
        assert_eq!(
            valid(&json!({"uniqueItems": true}), &values),
            [true, true, true, true, false, false, true]
        );
        assert_eq!(valid(&json!({"uniqueItems": false}), &values), [true; 7]);
        // An empty array contains nothing.
        assert_eq!(
            valid(&json!({"contains": {"type": "string"}}), &values),
            [false, false, true, true, false, false, true]
        );
    }

    #[test]
    fn the_object_keywords_apply_to_members() {
        let values = [
            json!({}),
            json!({"a": 1}),
            json!({"a": "x"}),
            json!({"ab": 1}),
            json!({"ab": "x"}),
            json!({"b": 1}),
            json!({"b": "x", "c": 1}),
            json!(["a"]),
        ];
        let document = json!({
            "properties": {"a": {"type": "integer"}},
            "patternProperties": {"^a": {"minimum": 1}, "b$": {"type": "integer"}},
            "additionalProperties": {"type": "string"}
        });
        // `a` meets its property and the pattern `^a`; `ab` meets both
        // patterns; `b` meets `b$`; and `c` meets the additional properties.
        assert_eq!(
            valid(&document, &values),
            [true, true, false, true, false, true, false, true]
        );
        assert_eq!(
            valid(&json!({"required": ["a"], "maxProperties": 1}), &values),
            [false, true, true, false, false, false, false, true]
        );
        assert_eq!(
            valid(&json!({"minProperties": 2}), &values),
            [false, false, false, false, false, false, true, true]
        );
        // A dependency applies only beside its name.
        assert_eq!(
            valid(
                &json!({"dependencies": {"a": ["c"], "b": {"required": ["c"]}}}),
                &values
            ),
            [true, false, false, true, true, false, true, true]
        );
        // `propertyNames` meets each name as a string.
        assert_eq!(
            valid(&json!({"propertyNames": {"maxLength": 1}}), &values),
            [true, true, true, false, false, true, true, true]
        );
        assert_eq!(
            valid(&json!({"propertyNames": false}), &values),
            [true, false, false, false, false, false, false, true]
        );
    }

    #[test]
    fn the_applicators_combine_their_nodes() {
        let values = [json!(-1), json!(0), json!(5), json!(15)];
        let small = json!({"maximum": 9});
        let positive = json!({"exclusiveMinimum": 0});
        assert_eq!(
            valid(&json!({"allOf": [small, positive]}), &values),
            [false, false, true, false]
        );
        assert_eq!(
            valid(&json!({"anyOf": [small, positive]}), &values),
            [true, true, true, true]
        );
        assert_eq!(
            valid(&json!({"oneOf": [small, positive]}), &values),
            [true, true, false, true]
        );
        assert_eq!(
            valid(&json!({"not": positive}), &values),
            [true, true, false, false]
        );
        assert_eq!(
            valid(
                &json!({"if": positive, "then": small, "else": {"const": 0}}),
                &values
            ),
            [false, true, true, false]
        );
        // Without `then` or `else`, a branch allows anything.
        assert_eq!(
            valid(&json!({"if": positive, "then": small}), &values),
            [true, true, true, false]
        );
        assert_eq!(
            valid(&json!({"if": positive, "else": {"const": 0}}), &values),
            [false, true, true, true]
        );
        assert_eq!(valid(&json!({"if": positive}), &values), [true; 4]);
        assert_eq!(valid(&json!(true), &values), [true; 4]);
        assert_eq!(valid(&json!(false), &values), [false; 4]);
    }

    #[test]
    fn a_reference_applies_its_node_and_nothing_beside_it() {
        let document = json!({
            "definitions": {"small": {"maximum": 9}},
            "properties": {"a": {"$ref": "#/definitions/small", "minimum": 5}}
        });
        assert_eq!(
            valid(&document, &[json!({"a": 1}), json!({"a": 10})]),
            [true, false]
        );
        // A node applies itself through members, which ends.
        let tree = json!({
            "properties": {"leaf": {"type": "integer"}, "child": {"$ref": "#"}},
            "additionalProperties": false
        });
        assert_eq!(
            valid(
                &tree,
                &[
                    json!({"leaf": 1, "child": {"leaf": 2, "child": {}}}),
                    json!({"child": {"child": {"leaf": "x"}}}),
                    json!({"child": {"other": 1}}),
                ]
            ),
            [true, false, false]
        );
    }

    #[test]
    fn each_applied_node_meets_an_instance_node_once() {
        let document = json!({
            "allOf": [{"$ref": "#/definitions/a"}, {"$ref": "#/definitions/a"}],
            "definitions": {"a": {"properties": {"x": {"type": "integer"}}}}
        });
        // The root, the two references, and `a` meet the object; one node
        // meets `x`; and nothing meets `y`.
        let validation = validated(&document, &[json!({"x": 1, "y": [2, 3]})])[0];
        assert_eq!(
            validation,
            Validation {
                valid: true,
                pairs: 5
            }
        );
    }

    #[test]
    fn every_branch_is_decided_whatever_the_others_decide() {
        let document = json!({
            "anyOf": [true, {"type": "string"}],
            "if": true,
            "then": true,
            "else": false,
            "not": false,
            "dependencies": {"a": true, "b": {"$ref": "#/anyOf/1"}}
        });
        // The root, two branches of `anyOf`, `if`, `then`, `else`, `not`, and
        // the dependency of `a`, which the object has; not that of `b`.
        let validation = validated(&document, &[json!({"a": 1})])[0];
        assert_eq!(
            validation,
            Validation {
                valid: true,
                pairs: 8
            }
        );
        // A string has no names, so no dependency applies to it.
        let validation = validated(&document, &[json!("x")])[0];
        assert_eq!(
            validation,
            Validation {
                valid: true,
                pairs: 7
            }
        );
    }

    #[test]
    fn a_name_is_counted_with_its_members_value() {
        // Both the names and the values meet the root through a reference.
        let document = json!({
            "propertyNames": {"$ref": "#"},
            "additionalProperties": {"$ref": "#"}
        });
        let instance = json!({"a": {"a": {"a": {}}}});
        // The outermost object meets the root, and each of the three inner
        // objects meets `additionalProperties` and the root as its value, and
        // `propertyNames` and the root as its name. Counted apart, the names
        // would add six pairs to the values' seven, more than the product of
        // the three schema nodes and the four instance nodes.
        let validation = validated(&document, std::slice::from_ref(&instance))[0];
        assert_eq!(
            validation,
            Validation {
                valid: true,
                pairs: 10
            }
        );
        assert_eq!(census(&document).nodes * instance_nodes(&instance), 12);
    }

    #[test]
    fn no_dependency_applies_to_a_name() {
        // The object has `a`, but the name `a` is a string without members,
        // so the dependency `propertyNames` holds applies to nothing: the root
        // meets the object, and `propertyNames` the name.
        let document = json!({"propertyNames": {"dependencies": {"a": {"maxLength": 1}}}});
        let validation = validated(&document, &[json!({"a": 1})])[0];
        assert_eq!(
            validation,
            Validation {
                valid: true,
                pairs: 2
            }
        );
    }

    #[test]
    fn a_child_no_node_applies_to_is_not_visited() {
        let deep = json!({"a": [{"b": [1, 2, {"c": null}]}]});
        for document in [
            json!(true),
            json!({"type": "object"}),
            json!({"items": false}),
        ] {
            let validation = validated(&document, std::slice::from_ref(&deep))[0];
            assert_eq!(
                validation,
                Validation {
                    valid: true,
                    pairs: 1
                }
            );
        }
        // `items` meets each item, and `false` asks nothing of theirs.
        let validation = validated(&json!({"items": false}), &[json!([1, [2, 3]])])[0];
        assert_eq!(
            validation,
            Validation {
                valid: false,
                pairs: 3
            }
        );
    }
}
