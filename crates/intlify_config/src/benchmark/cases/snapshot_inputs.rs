// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The formal Snapshots a binding submits for `programmatic_snapshot_admission`
//! (D15:2765): the value of each recipe a `file_decode_and_source_map` case
//! materializes, other than the padded one, whose value is Minimal's, so a
//! file and a Snapshot of one value pair (D15:2788); Minimal's value
//! holding rejected-node markers, which the boundary admits and no document
//! holds (D15:140, 861); and Minimal's value with a long string as its last
//! node in traversal order, whose envelope can be represented one over its
//! bound (plan 025 decision #325).
//!
//! A Snapshot's tree is what the file's bytes spell: each object's members in
//! the order the file writes them, so the reversed recipe submits them
//! reversed, and each number as its token reads, so negative zero reaches
//! the boundary, which normalizes both (D15:871, 889). Its digest is the one
//! the canonical tree has. Everything is built before any interval, as
//! live-host inspection is (D15:2824).
//!
//! At an edge of a `bootstrapOnly.snapshot` bound, a recipe's Snapshot is
//! admitted under that bound at its count, or at one under it, where the
//! binding inspects the value in traversal order and puts the terminal
//! marker at the first overrun (D15:867, 2786; 015-199).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::artifacts::count;
use crate::boundary::snapshot::{SnapshotReport, SubmittedMarker, SubmittedNode};
use crate::input_limits::Bound;
use crate::materialize::{materialize_file, Locus, MaterializedDocument, NodeKind, ObjectMember};
use crate::resolver::capability::SnapshotLimits;
use crate::snapshot::{
    Marker, MarkerReason, Node, SnapshotBound, SnapshotCounts, BOOTSTRAP_LIMIT_EXCEEDED,
};

use super::prepare::fixture_capacity;
use super::workflow_inputs::ROOM;
use super::Recipe;

/// A frame's tag and `U64` payload length (D15:873).
const HEADER: u64 = 9;

/// How many markers the recipe with many of them holds.
const MARKERS: usize = 64;

/// The bytes of the string `TrailingString` holds last: its envelope less
/// one reaches the bound's minimum, and the string frames longer than the
/// terminal marker of that bound.
const TRAILING: usize = 2048;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(in crate::benchmark) enum SnapshotRecipe {
    Minimal,
    ReversedMembers,
    LongString,
    ManyProfiles,
    ManyLocaleOccurrences,
    NestedArray,
    ManyValues,
    PortableMaximum,
    NegativeZero,
    UnsupportedType,
    Cycle,
    ManyMarkers,
    TrailingString,
}

impl SnapshotRecipe {
    pub(in crate::benchmark) const ALL: [Self; 13] = [
        Self::Minimal,
        Self::ReversedMembers,
        Self::LongString,
        Self::ManyProfiles,
        Self::ManyLocaleOccurrences,
        Self::NestedArray,
        Self::ManyValues,
        Self::PortableMaximum,
        Self::NegativeZero,
        Self::UnsupportedType,
        Self::Cycle,
        Self::ManyMarkers,
        Self::TrailingString,
    ];

    /// The file recipe whose value the Snapshot submits: its own for a
    /// value's copy, and Minimal's, beside which the others hold a member.
    pub(in crate::benchmark) const fn file(self) -> Recipe {
        match self {
            Self::Minimal
            | Self::UnsupportedType
            | Self::Cycle
            | Self::ManyMarkers
            | Self::TrailingString => Recipe::Minimal,
            Self::ReversedMembers => Recipe::ReversedMembers,
            Self::LongString => Recipe::LongString,
            Self::ManyProfiles => Recipe::ManyProfiles,
            Self::ManyLocaleOccurrences => Recipe::ManyLocaleOccurrences,
            Self::NestedArray => Recipe::NestedArray,
            Self::ManyValues => Recipe::ManyValues,
            Self::PortableMaximum => Recipe::PortableMaximum,
            Self::NegativeZero => Recipe::NegativeZero,
        }
    }

    /// Whether the Snapshot holds no marker, so its document is its file
    /// recipe's value (D15:140).
    pub(in crate::benchmark) const fn materializes(self) -> bool {
        !matches!(
            self,
            Self::UnsupportedType | Self::Cycle | Self::ManyMarkers
        )
    }

    /// Whether the Snapshot is its file recipe's value, which that file's
    /// case pairs with (D15:2788).
    #[cfg(test)]
    pub(in crate::benchmark) fn copies(self) -> bool {
        self.member().is_none()
    }

    /// The root member a recipe adds to Minimal's value, its key, and its
    /// value as submitted and as canonical: one marker, an array of markers
    /// cycling through every ordinary reason, or a long string, whose key
    /// comes after every other. `None` for a value's copy.
    fn member(self) -> Option<(&'static str, SubmittedNode, Node)> {
        let marker = |reason: MarkerReason| {
            let submitted = SubmittedMarker {
                reason: reason.id().to_owned(),
                bound: None,
                witness: None,
            };
            let node = Node::Marker(Marker::Rejected(reason));
            (SubmittedNode::Marker(submitted), node)
        };
        let (key, (value, node)) = match self {
            Self::UnsupportedType => ("onLoad", marker(MarkerReason::UnsupportedType)),
            Self::Cycle => ("self", marker(MarkerReason::Cycle)),
            Self::ManyMarkers => {
                let reasons = MarkerReason::ALL.into_iter().cycle().take(MARKERS);
                let (values, nodes): (Vec<_>, Vec<_>) = reasons.map(marker).unzip();
                ("hooks", (SubmittedNode::Array(values), Node::Array(nodes)))
            }
            Self::TrailingString => {
                let text = "a".repeat(TRAILING);
                let value = SubmittedNode::String(text.as_bytes().to_vec());
                ("summary", (value, Node::String(text)))
            }
            _ => return None,
        };
        Some((key, value, node))
    }
}

/// The `bootstrapOnly.snapshot` bounds a case's Snapshot is admitted under:
/// each at the room the complete invocation's capability leaves a
/// programmatic entry's Snapshot, which no recipe's tree reaches.
pub(in crate::benchmark) fn limits() -> SnapshotLimits {
    room_but(None)
}

/// [`limits`] with `bound` at `limit`, as an edge's case is admitted.
pub(in crate::benchmark) fn bounded(bound: SnapshotBound, limit: Bound) -> SnapshotLimits {
    room_but(Some((bound, limit)))
}

fn room_but(edge: Option<(SnapshotBound, Bound)>) -> SnapshotLimits {
    let room = Bound::new(ROOM).expect("a positive bound");
    let at = |bound| match edge {
        Some((edged, limit)) if edged == bound => limit,
        _ => room,
    };
    SnapshotLimits {
        max_nodes: at(SnapshotBound::Nodes),
        max_depth: at(SnapshotBound::Depth),
        max_collection_entries: at(SnapshotBound::CollectionEntries),
        max_total_string_bytes: at(SnapshotBound::TotalStringBytes),
        max_single_string_bytes: at(SnapshotBound::SingleStringBytes),
        max_rejected_markers: at(SnapshotBound::RejectedMarkers),
        max_envelope_canonical_bytes: at(SnapshotBound::EnvelopeCanonicalBytes),
        max_single_marker_canonical_bytes: at(SnapshotBound::SingleMarkerCanonicalBytes),
        max_traversal_units: at(SnapshotBound::TraversalUnits),
    }
}

/// The formal Snapshot a binding submits for `recipe`: the tree its file
/// recipe's bytes spell, with the member the recipe adds, and the digest of
/// the canonical tree. `None` when the file does not materialize under the
/// fixture capacity.
pub(in crate::benchmark) fn report(recipe: SnapshotRecipe) -> Option<SnapshotReport> {
    let (root, canonical) = trees(recipe)?;
    Some(SnapshotReport::Formal {
        digest: canonical.digest(),
        root,
    })
}

/// The formal Snapshot a binding submits for `recipe` when `bound` is at
/// `limit`: the canonical tree of the recipe's value with the terminal
/// marker of `bound` in place of its first overrun, each object's members in
/// key order. `None` when the value fits.
pub(in crate::benchmark) fn first_over(
    recipe: SnapshotRecipe,
    bound: SnapshotBound,
    limit: Bound,
) -> Option<SnapshotReport> {
    let (_, canonical) = trees(recipe)?;
    let tree = overrun(canonical, bound, limit)?;
    Some(SnapshotReport::Formal {
        digest: tree.digest(),
        root: submit(&tree)?,
    })
}

/// The tree a binding submits for `recipe` and its canonical tree.
fn trees(recipe: SnapshotRecipe) -> Option<(SubmittedNode, Node)> {
    let doc = materialize_file(recipe.file().source(), fixture_capacity().0).ok()?;
    let mut root = submitted(&doc)?;
    let mut canonical = doc.canonical_value();
    if let Some((key, value, node)) = recipe.member() {
        let (SubmittedNode::Object(members), Node::Object(tree)) = (&mut root, &mut canonical)
        else {
            return None;
        };
        members.push((key.as_bytes().to_vec(), value));
        tree.insert(key.to_owned(), node);
    }
    Some((root, canonical))
}

/// A file document's value as a binding submits it: each object's members in
/// the order their keys' tokens have in the file, and each number as its
/// token reads. Built without recursion, each node after the nodes it holds,
/// which a preorder walk lists after it, so a tree as deep as a file can be
/// cannot exhaust the stack (plan 025 decision #298).
fn submitted(doc: &MaterializedDocument) -> Option<SubmittedNode> {
    let mut preorder = Vec::new();
    let mut pending = vec![doc.root()];
    while let Some(id) = pending.pop() {
        preorder.push(id);
        match doc.node(id).kind() {
            NodeKind::Array(items) => pending.extend(items),
            NodeKind::Object(members) => {
                pending.extend(members.values().map(ObjectMember::value));
            }
            _ => {}
        }
    }
    let mut built = BTreeMap::new();
    for id in preorder.into_iter().rev() {
        let node = doc.node(id);
        let mut take = |held| built.remove(&held);
        let value = match node.kind() {
            NodeKind::Null => SubmittedNode::Null,
            NodeKind::Boolean(value) => SubmittedNode::Boolean(*value),
            NodeKind::Number(_) => SubmittedNode::Number(number(doc, node.locus())?),
            NodeKind::String(text) => SubmittedNode::String(text.as_bytes().to_vec()),
            NodeKind::Array(items) => SubmittedNode::Array(
                items
                    .iter()
                    .map(|&item| take(item))
                    .collect::<Option<_>>()?,
            ),
            NodeKind::Object(members) => {
                let mut spelled: Vec<_> = members.iter().collect();
                spelled.sort_by_key(|(_, member)| member.key());
                SubmittedNode::Object(
                    spelled
                        .into_iter()
                        .map(|(key, member)| Some((key.as_bytes().to_vec(), take(member.value())?)))
                        .collect::<Option<_>>()?,
                )
            }
        };
        built.insert(id, value);
    }
    built.remove(&doc.root())
}

/// The binary64 value the file's number token at `at` reads as.
fn number(doc: &MaterializedDocument, at: Locus) -> Option<f64> {
    std::str::from_utf8(doc.raw_span(at.bytes()?)?)
        .ok()?
        .parse()
        .ok()
}

/// Where a node sits in the container that holds it.
#[derive(Clone)]
enum Slot {
    Element(usize),
    Member(String),
}

/// The tree at `root` with the terminal marker of `bound`, its witness
/// `limit + 1`, at the first overrun and no node after it in traversal
/// order (D15:867; 015-199). `None` when the tree fits.
fn overrun(root: Node, bound: SnapshotBound, limit: Bound) -> Option<Node> {
    let path = first_overrun(&root, bound, limit.get())?;
    let witness = limit.get() + 1;
    let marker = Node::Marker(Marker::BootstrapLimitExceeded { bound, witness });
    place(root, &path, marker)
}

/// The tree at `root` with `node` at `path` and no node after it in
/// traversal order: of each container on the way, the members after the one
/// on the way are dropped, and so is all the node it replaces held. Followed
/// down in a loop, and what is dropped is released without recursion (plan
/// 025 decision #298).
fn place(mut root: Node, path: &[Slot], node: Node) -> Option<Node> {
    let mut at = &mut root;
    for slot in path {
        at = match (at, slot) {
            (Node::Array(elements), Slot::Element(index)) => {
                elements.truncate(index + 1);
                elements.get_mut(*index)?
            }
            (Node::Object(members), Slot::Member(key)) => {
                let mut after = members.split_off(key.as_str());
                let value = after.remove(key)?;
                members.entry(key.clone()).or_insert(value)
            }
            _ => return None,
        };
    }
    *at = node;
    Some(root)
}

/// The path to where a binding inspecting the tree at `root` in traversal
/// order, each edge before the node it holds, finds the first overrun of
/// `bound` at `limit`, each unit counted as the bound's row counts it
/// (D15:867, 1796-1804): the node that overruns it, the edge for a
/// collection entry or traversal unit, and for a key the object that owns
/// it, as no Snapshot key can be a marker (D15:3901). `None` when the tree
/// fits. Walked without recursion, each visited node's place kept to read a
/// path back (plan 025 decision #298).
fn first_overrun(root: &Node, bound: SnapshotBound, limit: u64) -> Option<Vec<Slot>> {
    let over = |counts: &SnapshotCounts| {
        counts
            .by_bound()
            .into_iter()
            .any(|(each, count)| each == bound && count > limit)
    };
    let mut counts = SnapshotCounts::default();
    // Each visited node's parent and place in it; the root has none.
    let mut places: Vec<Option<(usize, Slot)>> = Vec::new();
    let mut pending = vec![(root, 1, None::<(usize, Slot)>)];
    while let Some((node, depth, held)) = pending.pop() {
        if let Some((parent, slot)) = &held {
            counts.collection_entries += 1;
            counts.traversal_units += 1;
            if over(&counts) {
                let mut path = path(&places, *parent);
                path.push(slot.clone());
                return Some(path);
            }
            if let Slot::Member(key) = slot {
                counts.envelope_canonical_bytes += HEADER + string(&mut counts, key);
                if over(&counts) {
                    return Some(path(&places, *parent));
                }
            }
        }
        places.push(held);
        let at = places.len() - 1;
        counts.nodes += 1;
        counts.traversal_units += 1;
        counts.depth = counts.depth.max(depth);
        let payload = match node {
            Node::Null | Node::Boolean(_) => 0,
            Node::Number(_) | Node::Array(_) | Node::Object(_) => 8,
            Node::String(text) => string(&mut counts, text),
            Node::Marker(marker) => {
                let record = count(marker.record().len());
                counts.rejected_markers += 1;
                counts.single_marker_canonical_bytes =
                    counts.single_marker_canonical_bytes.max(HEADER + record);
                record
            }
        };
        counts.envelope_canonical_bytes += HEADER + payload;
        if over(&counts) {
            return Some(path(&places, at));
        }
        let place = |slot| Some((at, slot));
        match node {
            Node::Array(elements) => {
                let held = elements.iter().enumerate().rev();
                pending.extend(
                    held.map(|(index, each)| (each, depth + 1, place(Slot::Element(index)))),
                );
            }
            Node::Object(members) => {
                let held = members.iter().rev();
                pending.extend(
                    held.map(|(key, each)| (each, depth + 1, place(Slot::Member(key.clone())))),
                );
            }
            _ => {}
        }
    }
    None
}

/// The path to the node visited at `at`, read back through each parent.
fn path(places: &[Option<(usize, Slot)>], mut at: usize) -> Vec<Slot> {
    let mut path = Vec::new();
    while let Some((parent, slot)) = &places[at] {
        path.push(slot.clone());
        at = *parent;
    }
    path.reverse();
    path
}

/// Count a string or key's UTF-8 bytes and return their number.
fn string(counts: &mut SnapshotCounts, text: &str) -> u64 {
    let bytes = count(text.len());
    counts.total_string_bytes += bytes;
    counts.single_string_bytes = counts.single_string_bytes.max(bytes);
    bytes
}

/// The submitted form of a canonical tree, each object's members in key
/// order. Built without recursion, each node after the nodes it holds,
/// which a preorder walk that takes the last member first lists after it
/// (plan 025 decision #298).
fn submit(root: &Node) -> Option<SubmittedNode> {
    let mut preorder = Vec::new();
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        preorder.push(node);
        match node {
            Node::Array(elements) => pending.extend(elements),
            Node::Object(members) => pending.extend(members.values()),
            _ => {}
        }
    }
    let mut built = Vec::new();
    for node in preorder.into_iter().rev() {
        let value = match node {
            Node::Null => SubmittedNode::Null,
            Node::Boolean(value) => SubmittedNode::Boolean(*value),
            Node::Number(number) => SubmittedNode::Number(number.get()),
            Node::String(text) => SubmittedNode::String(text.as_bytes().to_vec()),
            Node::Array(elements) => {
                SubmittedNode::Array(built.split_off(built.len() - elements.len()))
            }
            Node::Object(members) => {
                let values = built.split_off(built.len() - members.len());
                let keys = members.keys().map(|key| key.as_bytes().to_vec());
                SubmittedNode::Object(keys.zip(values).collect())
            }
            Node::Marker(marker) => {
                let (reason, bound, witness) = match *marker {
                    Marker::Rejected(reason) => (reason.id(), None, None),
                    Marker::BootstrapLimitExceeded { bound, witness } => {
                        (BOOTSTRAP_LIMIT_EXCEEDED, Some(bound.path()), Some(witness))
                    }
                };
                SubmittedNode::Marker(SubmittedMarker {
                    reason: reason.to_owned(),
                    bound,
                    witness,
                })
            }
        };
        built.push(value);
    }
    built.pop()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use super::*;
    use crate::boundary::snapshot::read_report;
    use crate::input_limits::{InputLimits, RawInputLimits, ValueLimits};
    use crate::snapshot::from_json;

    fn formal(recipe: SnapshotRecipe) -> (SubmittedNode, String) {
        let Some(SnapshotReport::Formal { root, digest }) = report(recipe) else {
            panic!("{recipe:?} is a formal Snapshot");
        };
        (root, digest)
    }

    /// The root object's keys in the order they are submitted.
    fn keys(root: &SubmittedNode) -> Vec<&[u8]> {
        let SubmittedNode::Object(members) = root else {
            panic!("an object");
        };
        members.iter().map(|(key, _)| key.as_slice()).collect()
    }

    /// D15:871, 889: each Snapshot is admitted with its supplied digest, a
    /// value's copy as the canonical tree of its file's value, the reversed
    /// recipe's members submitted in the reverse order and negative zero as
    /// negative, and the member each other recipe adds beside Minimal's value.
    #[test]
    fn each_snapshot_submits_what_its_files_bytes_spell() {
        for recipe in SnapshotRecipe::ALL {
            let (root, digest) = formal(recipe);
            let report = SnapshotReport::Formal {
                root: root.clone(),
                digest: digest.clone(),
            };
            let admitted = read_report(Some(&report), &limits()).unwrap();
            assert_eq!(admitted.digest(), digest, "{recipe:?}");
            let doc = materialize_file(recipe.file().source(), fixture_capacity().0).unwrap();
            let markers = SnapshotCounts::of(admitted.root()).rejected_markers;
            let expected = match recipe {
                SnapshotRecipe::UnsupportedType | SnapshotRecipe::Cycle => 1,
                SnapshotRecipe::ManyMarkers => 64,
                _ => 0,
            };
            assert_eq!(markers, expected, "{recipe:?}");
            assert_eq!(recipe.materializes(), markers == 0, "{recipe:?}");
            let copy = *admitted.root() == doc.canonical_value();
            assert_eq!(copy, recipe.copies(), "{recipe:?}");
        }
        let (minimal, _) = formal(SnapshotRecipe::Minimal);
        let (reversed, _) = formal(SnapshotRecipe::ReversedMembers);
        let mut order = keys(&minimal);
        assert_eq!(order, [&b"schemaVersion"[..], b"profiles"]);
        order.reverse();
        assert_eq!(keys(&reversed), order);
        let (SubmittedNode::Number(zero), _) = formal(SnapshotRecipe::NegativeZero) else {
            panic!("a number");
        };
        assert!(zero == 0.0 && zero.is_sign_negative());
        for (recipe, key, elements) in [
            (SnapshotRecipe::UnsupportedType, "onLoad", None),
            (SnapshotRecipe::Cycle, "self", None),
            (SnapshotRecipe::ManyMarkers, "hooks", Some(64)),
            (SnapshotRecipe::TrailingString, "summary", Some(TRAILING)),
        ] {
            let (marked, _) = formal(recipe);
            let expected = [&b"schemaVersion"[..], b"profiles", key.as_bytes()];
            assert_eq!(keys(&marked), expected, "{recipe:?}");
            let SubmittedNode::Object(members) = &marked else {
                unreachable!()
            };
            match (&members[2].1, elements) {
                (SubmittedNode::Marker(_), None) => {}
                (SubmittedNode::Array(markers), Some(count)) => {
                    assert_eq!(markers.len(), count);
                }
                (SubmittedNode::String(text), Some(count)) => assert_eq!(text.len(), count),
                _ => panic!("{recipe:?} holds its member as declared"),
            }
        }
    }

    /// A value's copy submits the value of the file recipe of its own name,
    /// and each other recipe Minimal's.
    #[test]
    fn each_snapshot_copies_the_file_recipe_it_names() {
        for recipe in SnapshotRecipe::ALL {
            let named = if recipe.copies() {
                serde_json::to_value(recipe).unwrap()
            } else {
                serde_json::json!("minimal")
            };
            assert_eq!(serde_json::to_value(recipe.file()).unwrap(), named);
        }
    }

    /// D15:871, 889: at every depth, an array keeps its elements' order, an
    /// object its members' written order, and a number its token's value, so
    /// the tree admitted is the canonical tree of the file's value.
    #[test]
    fn a_files_value_is_submitted_as_written_at_every_depth() {
        let source = br#"{"b":[1,"x",true,null,[2,{"z":0,"y":-0.0}]],"a":{}}"#;
        let doc = materialize_file(Arc::from(&source[..]), fixture_capacity().0).unwrap();
        let root = submitted(&doc).unwrap();
        assert_eq!(keys(&root), [&b"b"[..], b"a"]);
        let SubmittedNode::Object(members) = &root else {
            unreachable!()
        };
        let SubmittedNode::Array(elements) = &members[0].1 else {
            panic!("an array");
        };
        let SubmittedNode::Array(inner) = &elements[4] else {
            panic!("the last element is an array");
        };
        assert_eq!(keys(&inner[1]), [&b"z"[..], b"y"]);
        let SubmittedNode::Object(nested) = &inner[1] else {
            unreachable!()
        };
        let SubmittedNode::Number(zero) = nested[1].1 else {
            panic!("a number");
        };
        assert!(zero == 0.0 && zero.is_sign_negative());
        let canonical = doc.canonical_value();
        let report = SnapshotReport::Formal {
            root,
            digest: canonical.digest(),
        };
        let admitted = read_report(Some(&report), &limits()).unwrap();
        assert!(*admitted.root() == canonical);
    }

    /// Plan 025 decision #298: a file far deeper than any recipe's is
    /// submitted and admitted with no recursion over its depth, which on a
    /// test thread's stack would overflow.
    #[test]
    fn a_deep_value_is_submitted_without_recursion() {
        const DEPTH: usize = 100_000;
        let large = Bound::new(10_000_000).unwrap();
        let limits_of_the_file = InputLimits {
            raw: RawInputLimits {
                max_file_bytes: large,
                max_parser_tokens: large,
            },
            value: ValueLimits {
                max_nodes: large,
                max_depth: large,
                max_collection_entries: large,
                max_total_string_bytes: large,
                max_single_string_bytes: large,
            },
        };
        let source = format!("{}-0{}", "[".repeat(DEPTH), "]".repeat(DEPTH));
        let doc = materialize_file(Arc::from(source.into_bytes()), limits_of_the_file).unwrap();
        let canonical = doc.canonical_value();
        let report = SnapshotReport::Formal {
            root: submitted(&doc).unwrap(),
            digest: canonical.digest(),
        };
        let admitted = read_report(Some(&report), &limits()).unwrap();
        assert!(*admitted.root() == canonical);
        let depth = u64::try_from(DEPTH).unwrap() + 1;
        assert_eq!(SnapshotCounts::of(admitted.root()).depth, depth);
    }

    fn object(members: Vec<(&str, Node)>) -> Node {
        Node::Object(
            members
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
        )
    }

    fn terminal(bound: SnapshotBound, witness: u64) -> Node {
        Node::Marker(Marker::BootstrapLimitExceeded { bound, witness })
    }

    /// `host`'s Snapshot with `bound` at `limit` is `expected`, which is
    /// submitted and admitted under that bound.
    fn builds(host: Node, bound: SnapshotBound, limit: u64, expected: &Node) {
        let limit = Bound::new(limit).unwrap();
        let built = overrun(host, bound, limit).expect("the value overruns the bound");
        assert!(built == *expected, "{bound:?}");
        let report = SnapshotReport::Formal {
            root: submit(&built).unwrap(),
            digest: built.digest(),
        };
        let admitted = read_report(Some(&report), &bounded(bound, limit)).unwrap();
        assert!(*admitted.root() == *expected, "{bound:?}");
    }

    /// D15:867, 1796-1804, 3901: inspecting in traversal order, each edge
    /// before the node it holds, the binding puts the terminal marker where
    /// the first node overruns a bound, where the first edge overruns a
    /// collection entry or traversal unit, and, for a key that overruns a
    /// string bound or the envelope, where the object that owns the key is,
    /// and keeps nothing after it; the boundary admits each. Where the marker
    /// frames too long for the bound, it still takes the place of the first
    /// overrun. A tree within the bound has none.
    #[test]
    fn the_terminal_marker_takes_the_place_of_the_first_overrun() {
        use SnapshotBound as B;

        let cycle = || Node::Marker(Marker::Rejected(MarkerReason::Cycle));
        let host = || from_json(&json!({ "a": [1, [2, 3]], "b": 4 }));
        let held = |marker| object(vec![("a", Node::Array(vec![from_json(&json!(1)), marker]))]);
        // The fourth node, the third edge, and the sixth and seventh units:
        // the edge to the inner array, then the array.
        builds(host(), B::Nodes, 3, &held(terminal(B::Nodes, 4)));
        let entries = terminal(B::CollectionEntries, 3);
        builds(host(), B::CollectionEntries, 2, &held(entries));
        for (limit, witness) in [(5, 6), (6, 7)] {
            let units = terminal(B::TraversalUnits, witness);
            builds(host(), B::TraversalUnits, limit, &held(units));
        }
        assert!(overrun(host(), B::Nodes, Bound::new(7).unwrap()).is_none());
        let deep = from_json(&json!({ "a": { "b": { "c": null } }, "d": 1 }));
        let level = object(vec![("b", terminal(B::Depth, 3))]);
        builds(deep, B::Depth, 2, &object(vec![("a", level)]));
        let keys = from_json(&json!({ "a": { "bc": 1, "de": 2 }, "f": "g" }));
        let total = terminal(B::TotalStringBytes, 5);
        builds(keys, B::TotalStringBytes, 4, &object(vec![("a", total)]));
        let single = || terminal(B::SingleStringBytes, 3);
        builds(
            from_json(&json!({ "abc": 1 })),
            B::SingleStringBytes,
            2,
            &single(),
        );
        let strings = from_json(&json!(["ab", "cde", "f"]));
        let expected = Node::Array(vec![from_json(&json!("ab")), single()]);
        builds(strings, B::SingleStringBytes, 2, &expected);
        let markers = Node::Array(vec![cycle(), Node::Null, cycle(), cycle()]);
        let third = terminal(B::RejectedMarkers, 3);
        let expected = Node::Array(vec![cycle(), Node::Null, cycle(), third]);
        builds(markers, B::RejectedMarkers, 2, &expected);
        // A string that frames longer than the marker in its place.
        let long = || Node::Array(vec![Node::String("a".repeat(200))]);
        let bytes = SnapshotCounts::of(&long()).envelope_canonical_bytes;
        let envelope = Node::Array(vec![terminal(B::EnvelopeCanonicalBytes, bytes)]);
        builds(long(), B::EnvelopeCanonicalBytes, bytes - 1, &envelope);
        // A key that takes the envelope past 240 bytes with its frame, at 253.
        let key = "x".repeat(200);
        let keyed = object(vec![(
            "a",
            object(vec![(key.as_str(), from_json(&json!(1)))]),
        )]);
        let owner = object(vec![("a", terminal(B::EnvelopeCanonicalBytes, 241))]);
        builds(keyed, B::EnvelopeCanonicalBytes, 240, &owner);
        // The second member's edge, with the key it holds.
        let pair = || {
            object(vec![
                ("a", from_json(&json!(1))),
                ("b", from_json(&json!(2))),
            ])
        };
        let second = terminal(B::CollectionEntries, 2);
        let expected = object(vec![("a", from_json(&json!(1))), ("b", second)]);
        builds(pair(), B::CollectionEntries, 1, &expected);
        // A boolean frames nine bytes past the null's, at 35 of 52, and the
        // second marker frames 34 bytes; the marker of either bound frames
        // longer, so the boundary would refuse each.
        let at = |host, bound, limit| overrun(host, bound, Bound::new(limit).unwrap());
        let scalars = from_json(&json!([null, true, 1.5]));
        let envelope = Node::Array(vec![Node::Null, terminal(B::EnvelopeCanonicalBytes, 31)]);
        assert!(at(scalars, B::EnvelopeCanonicalBytes, 30).is_some_and(|built| built == envelope));
        let unsupported = Node::Marker(Marker::Rejected(MarkerReason::UnsupportedType));
        let frames = Node::Array(vec![cycle(), unsupported]);
        let single = Node::Array(vec![cycle(), terminal(B::SingleMarkerCanonicalBytes, 31)]);
        let built = at(frames, B::SingleMarkerCanonicalBytes, 30);
        assert!(built.is_some_and(|built| built == single));
    }

    /// Plan 025 decision #298: a value far deeper than any recipe's has its
    /// first overrun found, the marker put in its place, and its Snapshot
    /// submitted with no recursion over its depth.
    #[test]
    fn a_deep_values_first_over_is_built_without_recursion() {
        const LEVELS: u64 = 100_000;
        let mut host = Node::Null;
        for _ in 0..LEVELS {
            host = Node::Array(vec![host]);
        }
        // The null at the bottom is the last node.
        let limit = Bound::new(LEVELS).unwrap();
        let built = overrun(host, SnapshotBound::Nodes, limit).unwrap();
        let report = SnapshotReport::Formal {
            root: submit(&built).unwrap(),
            digest: built.digest(),
        };
        let bounds = bounded(SnapshotBound::Nodes, limit);
        let admitted = read_report(Some(&report), &bounds).unwrap();
        let counts = SnapshotCounts::of(admitted.root());
        let found = (counts.nodes, counts.depth, counts.rejected_markers);
        assert_eq!(found, (LEVELS + 1, LEVELS + 1, 1));
    }
}
