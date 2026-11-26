// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The document of a canonical Programmatic Entry Snapshot: the value a file
//! of the same JSON value materializes, with the same root-value counts
//! (D15:1890), each node located at itself in the Snapshot rather than at
//! bytes. A tree with a marker has none, since a marker is no JSON-compatible
//! value (D15:862, 2130). Back from a document of either entry, the canonical
//! tree of its value is what a Materialized Resolution Input Set keeps
//! (D15:206).

use crate::snapshot::{Node, SnapshotCounts};

use super::{LocatedNode, Locus, MaterializedDocument, NodeId, NodeKind, ObjectMember};

impl MaterializedDocument {
    /// The document of the canonical tree at `root`, its nodes in canonical
    /// preorder: each node before what it holds, array elements by index,
    /// and object members by key bytes (D15:868). None when the tree holds a
    /// marker. Built without recursion, so a tree as deep as the capability
    /// admits cannot exhaust the stack (plan 025 decision #298).
    pub(crate) fn from_snapshot(root: &Node) -> Option<Self> {
        let nodes = preorder(root)
            .into_iter()
            .enumerate()
            .map(|(index, (node, held))| located(NodeId(index), node, held))
            .collect::<Option<_>>()?;
        Some(Self {
            nodes,
            value_counts: SnapshotCounts::of(root).value(),
            file: None,
        })
    }

    /// The canonical tree of the document's value, without its loci: the
    /// tree of a marker-free Snapshot of the same value, whichever entry
    /// materialized it. Both number a document's nodes in preorder, so built
    /// in reverse, each node is built after what it holds, without recursion
    /// (plan 025 decision #298).
    pub(crate) fn canonical_value(&self) -> Node {
        let mut built: Vec<Option<Node>> = Vec::with_capacity(self.nodes.len());
        built.resize_with(self.nodes.len(), || None);
        for (index, node) in self.nodes.iter().enumerate().rev() {
            let mut take = |id: NodeId| {
                built[id.0]
                    .take()
                    .expect("a node is numbered after the node that holds it")
            };
            let value = match &node.kind {
                NodeKind::Null => Node::Null,
                NodeKind::Boolean(value) => Node::Boolean(*value),
                NodeKind::Number(value) => Node::Number(*value),
                NodeKind::String(text) => Node::String(text.clone()),
                NodeKind::Array(items) => Node::Array(items.iter().map(|&id| take(id)).collect()),
                NodeKind::Object(members) => Node::Object(
                    members
                        .iter()
                        .map(|(name, member)| (name.clone(), take(member.value)))
                        .collect(),
                ),
            };
            built[index] = Some(value);
        }
        built[self.root().0].take().expect("the root is built last")
    }
}

/// The nodes of the tree at `root` in canonical preorder, each beside the
/// IDs of what it holds, in order. A stack keeps the nodes still to visit,
/// each with the index of the node that holds it.
fn preorder(root: &Node) -> Vec<(&Node, Vec<NodeId>)> {
    let mut order: Vec<(&Node, Vec<NodeId>)> = Vec::new();
    let mut pending: Vec<(&Node, Option<usize>)> = vec![(root, None)];
    while let Some((node, holder)) = pending.pop() {
        let id = NodeId(order.len());
        if let Some(holder) = holder {
            order[holder].1.push(id);
        }
        order.push((node, Vec::new()));
        // What the node holds goes on last first, so the first is next.
        if let Node::Array(elements) = node {
            pending.extend(elements.iter().rev().map(|element| (element, Some(id.0))));
        } else if let Node::Object(members) = node {
            pending.extend(members.values().rev().map(|value| (value, Some(id.0))));
        }
    }
    order
}

/// The document's node `id`, which is `node` holding the nodes `held`; none
/// at a marker.
fn located(id: NodeId, node: &Node, held: Vec<NodeId>) -> Option<LocatedNode> {
    let kind = match node {
        Node::Null => NodeKind::Null,
        Node::Boolean(value) => NodeKind::Boolean(*value),
        Node::Number(value) => NodeKind::Number(*value),
        Node::String(text) => NodeKind::String(text.clone()),
        Node::Array(_) => NodeKind::Array(held),
        Node::Object(members) => NodeKind::Object(
            members
                .keys()
                .zip(held)
                .map(|(name, value)| {
                    let key = Locus::snapshot(value, true);
                    (name.clone(), ObjectMember { key, value })
                })
                .collect(),
        ),
        Node::Marker(_) => return None,
    };
    Some(LocatedNode {
        kind,
        locus: Locus::snapshot(id, false),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use serde_json::{json, Value};

    use crate::fixtures::{complete_config, minimal_config};
    use crate::input_limits::Bound;
    use crate::materialize::{
        materialize_file, Locus, MaterializedDocument, NodeId, NodeKind, ObjectMember,
    };
    use crate::materialize_tests::limits;
    use crate::snapshot::{from_json, Marker, MarkerReason, Node, SnapshotBound};
    use crate::structural::admission_tests::{FixtureAnalysis, FixtureSchema};
    use crate::structural::StructuralLimits;

    /// One JSON value as a file writes it, its members out of key order, with
    /// nested and empty containers, a key with `é`, negative zero, and a long
    /// string.
    const TEXT: &str = r#"{"z":{"é":[],"e":{},"d":[[],{"y":-0}]},"a":[-0.0,1.5,"a longer string value"],"é":null,"k":true}"#;

    /// The value `text` spells, through a file and through a Snapshot.
    fn both(text: &str) -> (MaterializedDocument, MaterializedDocument) {
        let file = materialize_file(Arc::from(text.as_bytes()), limits()).unwrap();
        let value: Value = serde_json::from_str(text).unwrap();
        let snapshot = MaterializedDocument::from_snapshot(&from_json(&value)).unwrap();
        (file, snapshot)
    }

    /// The document's value from its root, without its loci or node IDs.
    fn value(doc: &MaterializedDocument) -> Value {
        doc.decode(doc.root()).unwrap()
    }

    /// The IDs of `id` and of what it holds, in canonical preorder.
    fn preorder(doc: &MaterializedDocument, id: NodeId, found: &mut Vec<NodeId>) {
        found.push(id);
        match doc.node(id).kind() {
            NodeKind::Array(items) => {
                for &item in items {
                    preorder(doc, item, found);
                }
            }
            NodeKind::Object(members) => {
                for member in members.values() {
                    preorder(doc, member.value(), found);
                }
            }
            NodeKind::Null | NodeKind::Boolean(_) | NodeKind::Number(_) | NodeKind::String(_) => {}
        }
    }

    /// Every locus of the document: each node's, then each member key's.
    fn loci(doc: &MaterializedDocument) -> Vec<Locus> {
        let mut ids = Vec::new();
        preorder(doc, doc.root(), &mut ids);
        let keys = ids.iter().filter_map(|&id| match doc.node(id).kind() {
            NodeKind::Object(members) => Some(members.values().map(ObjectMember::key)),
            _ => None,
        });
        ids.iter()
            .map(|&id| doc.node(id).locus())
            .chain(keys.flatten())
            .collect()
    }

    /// D15:1890: a Snapshot and a file of the same value materialize the
    /// same value with the same root-value counts, whatever the file's member
    /// order and number spelling.
    #[test]
    fn a_snapshot_and_its_json_file_materialize_the_same_document() {
        let (file, snapshot) = both(TEXT);
        assert_eq!(value(&snapshot), value(&file));
        assert_eq!(snapshot.value_counts(), file.value_counts());
        let reordered = r#"{"k":true,"é":null,"a":[0,1.5,"a longer string value"],"z":{"d":[[],{"y":0}],"e":{},"é":[]}}"#;
        let (_, again) = both(reordered);
        assert_eq!(value(&again), value(&file));
        assert_eq!(again.value_counts(), file.value_counts());
        assert_eq!(value(&snapshot)["z"]["d"][1]["y"], json!(0));
        for scalar in ["null", "true", "-0", "\"\"", "[]", "{}"] {
            let (file, snapshot) = both(scalar);
            assert_eq!(value(&snapshot), value(&file), "{scalar}");
            assert_eq!(snapshot.value_counts(), file.value_counts(), "{scalar}");
        }
    }

    /// A Snapshot document has its nodes in canonical preorder, the root
    /// first, and no file: its loci have no bytes, and tell every node and
    /// every member key apart.
    #[test]
    fn a_snapshot_document_is_in_canonical_preorder_without_file_bytes() {
        let (_, snapshot) = both(TEXT);
        let mut ids = Vec::new();
        preorder(&snapshot, snapshot.root(), &mut ids);
        assert_eq!(ids, (0..ids.len()).map(NodeId).collect::<Vec<_>>());
        assert_eq!(snapshot.counts(), None);
        let loci = loci(&snapshot);
        assert!(loci.iter().all(|locus| locus.bytes().is_none()));
        let mut distinct = loci.clone();
        distinct.sort();
        distinct.dedup();
        assert_eq!(distinct.len(), loci.len());
        // A member's key and its value are told apart.
        let NodeKind::Object(members) = snapshot.node(snapshot.root()).kind() else {
            panic!("an object");
        };
        let member = &members["k"];
        assert_ne!(member.key(), snapshot.node(member.value()).locus());
    }

    /// Only a tree without markers has a document, whichever marker it holds
    /// and wherever.
    #[test]
    fn a_tree_with_a_marker_has_no_document() {
        let ordinary = Node::Marker(Marker::Rejected(MarkerReason::Cycle));
        let terminal = Node::Marker(Marker::BootstrapLimitExceeded {
            bound: SnapshotBound::Nodes,
            witness: 2,
        });
        for marker in [ordinary, terminal] {
            let nested = Node::Array(vec![from_json(&json!({ "a": [1] })), marker.clone()]);
            let mut member = from_json(&json!({ "a": 1 }));
            let Node::Object(members) = &mut member else {
                unreachable!()
            };
            members.insert("b".to_owned(), marker.clone());
            for tree in [marker, nested, member] {
                assert!(MaterializedDocument::from_snapshot(&tree).is_none());
            }
        }
    }

    /// A tree far deeper than a stack holds is materialized without
    /// recursion: each level is the node after the one that holds it, down
    /// to the innermost value (plan 025 decision #298).
    #[test]
    fn a_deep_snapshot_is_materialized_without_recursion() {
        const LEVELS: usize = 100_000;
        let mut array = Node::Null;
        let mut object = Node::Null;
        for _ in 0..LEVELS {
            array = Node::Array(vec![array]);
            object = Node::Object(BTreeMap::from([("a".to_owned(), object)]));
        }
        for tree in [array, object] {
            let doc = MaterializedDocument::from_snapshot(&tree).unwrap();
            let mut id = doc.root();
            for level in 0..LEVELS {
                assert_eq!(id, NodeId(level));
                id = match doc.node(id).kind() {
                    NodeKind::Array(items) => {
                        assert_eq!(items.len(), 1);
                        items[0]
                    }
                    NodeKind::Object(members) => {
                        assert_eq!(members.len(), 1);
                        members["a"].value()
                    }
                    _ => panic!("a container at level {level}"),
                };
            }
            assert_eq!(id, NodeId(LEVELS));
            assert!(matches!(doc.node(id).kind(), NodeKind::Null));
            assert!(doc.canonical_value() == tree);
        }
    }

    /// D15:206, 3233: a document of either entry gives back the canonical
    /// tree of its value, with every element in its place and every member.
    #[test]
    fn a_document_of_either_entry_gives_back_the_canonical_tree_of_its_value() {
        let complete = serde_json::to_string(&complete_config()).unwrap();
        for text in [TEXT, &complete] {
            let value: Value = serde_json::from_str(text).unwrap();
            let (file, snapshot) = both(text);
            assert!(file.canonical_value() == from_json(&value));
            assert!(snapshot.canonical_value() == from_json(&value));
        }
    }

    /// The analysis of `doc` under the model's configuration schema.
    fn analyzed(doc: MaterializedDocument) -> FixtureAnalysis {
        let limits = StructuralLimits {
            max_profiles: Bound::new(100).unwrap(),
            max_profile_id_bytes: Bound::new(256).unwrap(),
            max_structural_analysis_units: Bound::new(1_000_000).unwrap(),
        };
        FixtureSchema::for_model()
            .unwrap()
            .analyze(Arc::new(doc), limits)
            .unwrap()
    }

    /// D15:1890, 3233: the same value through either entry has the same
    /// structural analysis: its units, its version and admission issues, and
    /// its schema issues.
    #[test]
    fn the_same_value_has_the_same_structural_analysis_through_either_entry() {
        let mut invalid = complete_config();
        invalid["profiles"]["app"]["defaultRequestedLocale"] = false.into();
        invalid["unknown"] = json!({ "x": [1, 2] });
        let values = [
            minimal_config(),
            complete_config(),
            invalid,
            json!({ "schemaVersion": 0 }),
            json!([]),
        ];
        for value in values {
            let text = serde_json::to_string(&value).unwrap();
            let (file, snapshot) = both(&text);
            let (file, snapshot) = (analyzed(file), analyzed(snapshot));
            assert_eq!(snapshot.structural_units(), file.structural_units());
            assert_eq!(snapshot.schema_issue_count(), file.schema_issue_count());
            let reasons = |analysis: &FixtureAnalysis| {
                analysis
                    .issues()
                    .iter()
                    .map(|issue| issue.reason)
                    .collect::<Vec<_>>()
            };
            assert_eq!(reasons(&snapshot), reasons(&file));
            assert_eq!(snapshot.is_complete(), file.is_complete());
            assert_eq!(snapshot.value_counts(), file.value_counts());
        }
    }
}
