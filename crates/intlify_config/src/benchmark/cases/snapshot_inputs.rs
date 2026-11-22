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
use super::{LimitKind, Recipe};

/// A frame's tag and `U64` payload length (D15:873).
const HEADER: u64 = 9;

/// Each `bootstrapOnly.snapshot` bound with edges, in the table's order, and
/// the first recipe whose count of it is at least 2 and its minimum, so that
/// an Admitted Implementation Capability can hold both edges (D15:1773,
/// 1790), and whose first-over Snapshot the boundary admits. One under a
/// recipe's envelope, the first overrun is its last node in traversal order,
/// which in the recipes before `TrailingString` frames shorter than the
/// terminal marker taking its place, so the Snapshot is over the bound it
/// names (D15:869). `maxSingleMarkerCanonicalBytes` has no edge: no marker
/// frames as long as its minimum, and the terminal marker frames longer than
/// any ordinary one it would take the place of (plan 025 decisions #132,
/// #134, #263, and #325).
pub(in crate::benchmark) const EDGES: [(LimitKind, SnapshotRecipe); 8] = [
    (LimitKind::SnapshotNodes, SnapshotRecipe::Minimal),
    (LimitKind::SnapshotDepth, SnapshotRecipe::Minimal),
    (
        LimitKind::SnapshotCollectionEntries,
        SnapshotRecipe::Minimal,
    ),
    (LimitKind::SnapshotTotalStringBytes, SnapshotRecipe::Minimal),
    (
        LimitKind::SnapshotSingleStringBytes,
        SnapshotRecipe::Minimal,
    ),
    (
        LimitKind::SnapshotRejectedMarkers,
        SnapshotRecipe::ManyMarkers,
    ),
    (
        LimitKind::SnapshotEnvelopeCanonicalBytes,
        SnapshotRecipe::TrailingString,
    ),
    (LimitKind::SnapshotTraversalUnits, SnapshotRecipe::Minimal),
];

/// The `bootstrapOnly.snapshot` bound `kind` names, if any.
pub(in crate::benchmark) const fn bound(kind: LimitKind) -> Option<SnapshotBound> {
    Some(match kind {
        LimitKind::SnapshotNodes => SnapshotBound::Nodes,
        LimitKind::SnapshotDepth => SnapshotBound::Depth,
        LimitKind::SnapshotCollectionEntries => SnapshotBound::CollectionEntries,
        LimitKind::SnapshotTotalStringBytes => SnapshotBound::TotalStringBytes,
        LimitKind::SnapshotSingleStringBytes => SnapshotBound::SingleStringBytes,
        LimitKind::SnapshotRejectedMarkers => SnapshotBound::RejectedMarkers,
        LimitKind::SnapshotEnvelopeCanonicalBytes => SnapshotBound::EnvelopeCanonicalBytes,
        LimitKind::SnapshotTraversalUnits => SnapshotBound::TraversalUnits,
        _ => return None,
    })
}

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
    use crate::boundary::snapshot::{read_report, SnapshotRefusal};
    use crate::input_limits::{InputLimits, RawInputLimits, ValueLimits};
    use crate::resolver::capability::BootstrapLimits;
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

    /// The count of `recipe`'s Snapshot under `bound`, which its unbounded
    /// case's work reports
    /// (`the_work_counts_the_admitted_tree_and_its_document`).
    fn count_of(recipe: SnapshotRecipe, bound: SnapshotBound) -> u64 {
        let admitted = read_report(report(recipe).as_ref(), &limits()).unwrap();
        let counts = SnapshotCounts::of(admitted.root()).by_bound();
        counts
            .into_iter()
            .find(|(each, _)| *each == bound)
            .unwrap()
            .1
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

    /// The boundary's verdict on `recipe`'s first-over Snapshot of `bound`.
    fn first_over_of(recipe: SnapshotRecipe, bound: SnapshotBound) -> Result<(), SnapshotRefusal> {
        let limit = Bound::new(count_of(recipe, bound) - 1).unwrap();
        let report = first_over(recipe, bound, limit).unwrap();
        read_report(Some(&report), &bounded(bound, limit)).map(|_| ())
    }

    /// Plan 025 decisions #132, #263, and #325; D15:869, 1773, 1790: every
    /// bound but `maxSingleMarkerCanonicalBytes` has edges, in the table's
    /// order, each on the first recipe whose count of it is at least 2 and
    /// the bound's minimum and whose first-over Snapshot the boundary admits.
    /// Only the envelope's passes recipes over: from `LongString` on, those
    /// that reach its minimum frame their last node in traversal order
    /// shorter than the terminal marker that takes its place, so their
    /// first-over Snapshot is over the bound. A file entry's bound names no
    /// Snapshot bound.
    #[test]
    fn each_edge_is_on_the_first_recipe_that_reaches_it() {
        use SnapshotRecipe as R;

        let minimum = BootstrapLimits::MINIMUM.snapshot;
        let edged: Vec<_> = EDGES.iter().map(|&(kind, _)| bound(kind)).collect();
        let expected: Vec<_> = SnapshotBound::ALL
            .into_iter()
            .filter(|&each| each != SnapshotBound::SingleMarkerCanonicalBytes)
            .map(Some)
            .collect();
        assert_eq!(edged, expected);
        let mut passed = Vec::new();
        for (kind, recipe) in EDGES {
            let bound = bound(kind).unwrap();
            let least = bound.limit(&minimum).get().max(2);
            let reaches = |&each: &SnapshotRecipe| count_of(each, bound) >= least;
            let mut first = None;
            for each in SnapshotRecipe::ALL.into_iter().filter(reaches) {
                match first_over_of(each, bound) {
                    Ok(()) => {
                        first = Some(each);
                        break;
                    }
                    Err(refusal) => passed.push((bound, each, refusal)),
                }
            }
            assert_eq!(first, Some(recipe), "{bound:?}");
        }
        let envelope = SnapshotBound::EnvelopeCanonicalBytes;
        let over = |recipe| (envelope, recipe, SnapshotRefusal::EnvelopeOverBound);
        let refused = [
            R::LongString,
            R::ManyProfiles,
            R::ManyValues,
            R::ManyMarkers,
        ];
        assert_eq!(passed, refused.map(over));
        assert_eq!(bound(LimitKind::Nodes), None);
    }

    /// D15:869, 1773, 1790, 1803: `maxSingleMarkerCanonicalBytes` has neither
    /// edge. Every marker frames shorter than its minimum, below which the
    /// capability boundary refuses a capability first, so no admitted
    /// capability's bound is reached; and the terminal marker naming it
    /// frames longer than any ordinary marker, so where one is its first
    /// overrun the Snapshot is over the bound and the boundary refuses it.
    #[test]
    fn no_snapshot_reaches_the_single_marker_bound() {
        let bound = SnapshotBound::SingleMarkerCanonicalBytes;
        let frame = |marker: Node| count(marker.framed().len());
        let ordinary =
            MarkerReason::ALL.map(|reason| frame(Node::Marker(Marker::Rejected(reason))));
        let terminals = SnapshotBound::ALL.map(|each| frame(terminal(each, 1)));
        let minimum = bound.limit(&BootstrapLimits::MINIMUM.snapshot).get();
        let every = ordinary.iter().chain(&terminals);
        assert!(every.into_iter().all(|&each| each < minimum));
        let named = frame(terminal(bound, 1));
        assert!(ordinary.iter().all(|&each| each < named));
        let largest = count_of(SnapshotRecipe::ManyMarkers, bound);
        assert_eq!(Some(&largest), ordinary.iter().max());
        let limit = Bound::new(largest - 1).unwrap();
        let report = first_over(SnapshotRecipe::ManyMarkers, bound, limit).unwrap();
        let refused = read_report(Some(&report), &bounded(bound, limit)).err();
        assert_eq!(refused, Some(SnapshotRefusal::EnvelopeOverBound));
    }

    /// Each node of the tree at `root` in canonical preorder: its depth, the
    /// key it is held under, and its frame, a container's tag alone.
    fn preorder(root: &Node) -> Vec<(u64, Option<&str>, Vec<u8>)> {
        let mut found = Vec::new();
        let mut pending = vec![(root, 1, None)];
        while let Some((node, depth, key)) = pending.pop() {
            let frame = match node {
                Node::Array(elements) => {
                    pending.extend(elements.iter().rev().map(|each| (each, depth + 1, None)));
                    vec![0x05]
                }
                Node::Object(members) => {
                    let held = members.iter().rev();
                    pending.extend(held.map(|(key, each)| (each, depth + 1, Some(key.as_str()))));
                    vec![0x06]
                }
                leaf => leaf.framed(),
            };
            found.push((depth, key, frame));
        }
        found
    }

    /// The path to the last node of the tree at `root` in canonical order,
    /// and that node.
    fn last(root: &Node) -> (Vec<Slot>, &Node) {
        let mut path = Vec::new();
        let mut node = root;
        loop {
            node = match node {
                Node::Array(elements) if !elements.is_empty() => {
                    path.push(Slot::Element(elements.len() - 1));
                    &elements[elements.len() - 1]
                }
                Node::Object(members) if !members.is_empty() => {
                    let (key, value) = members.iter().next_back().unwrap();
                    path.push(Slot::Member(key.clone()));
                    value
                }
                _ => return (path, node),
            };
        }
    }

    /// D15:867, 869; 015-199; plan 025 decision #263: each edge's case is
    /// admitted under its bound at the count its recipe's unbounded case
    /// reports, or one under it, and every other bound at the room. The
    /// exact edge admits the recipe's Snapshot. The first-over edge's holds
    /// the recipe's nodes in traversal order up to the first one that takes
    /// the count over the limit, in whose place the marker of the bound
    /// stands, its witness one over the limit.
    #[test]
    fn each_edge_is_at_its_count_or_its_first_overrun() {
        use crate::benchmark::cases::prepare::prepare;
        use crate::benchmark::cases::{declarations, LimitEdge};
        use crate::benchmark::operation::{Operation, Output, Prepared};

        let mut first_overs = 0;
        for case in declarations() {
            let (Recipe::Snapshot(recipe), Some((kind, edge))) = (case.fixture, case.limit) else {
                continue;
            };
            assert_eq!(case.operation, Operation::ProgrammaticSnapshotAdmission);
            let bound = bound(kind).unwrap();
            let counted = count_of(recipe, bound);
            let limit = counted - u64::from(edge == LimitEdge::FirstOver);
            let prepared = prepare(&case).unwrap();
            let (Prepared::Snapshot(snapshot), Output::Snapshot(resolved)) =
                (&prepared.prepared, &prepared.output)
            else {
                unreachable!()
            };
            let bounds = bounded(bound, Bound::new(limit).unwrap());
            assert_eq!(snapshot.limits, bounds, "{case:?}");
            let Ok(admitted) = &**resolved else {
                panic!("{case:?} is admitted");
            };
            let tree = admitted.snapshot.root();
            let (_, recipes) = trees(recipe).unwrap();
            if edge == LimitEdge::Exact {
                assert!(*tree == recipes, "{case:?}");
                continue;
            }
            first_overs += 1;
            let (path, marker) = last(tree);
            assert!(*marker == terminal(bound, limit + 1), "{case:?}");
            // The recipe's node where the marker is, without what it holds,
            // ends the recipe's nodes the Snapshot keeps and overruns.
            let original = path.iter().fold(&recipes, |node, slot| match (node, slot) {
                (Node::Array(elements), Slot::Element(index)) => &elements[*index],
                (Node::Object(members), Slot::Member(key)) => &members[key],
                _ => unreachable!(),
            });
            let alone = match original {
                Node::Array(_) => Node::Array(Vec::new()),
                Node::Object(_) => Node::Object(BTreeMap::new()),
                leaf => leaf.clone(),
            };
            let through = place(trees(recipe).unwrap().1, &path, alone).unwrap();
            let kept = preorder(&through);
            assert_eq!(kept, preorder(&recipes)[..kept.len()], "{case:?}");
            let found = preorder(tree);
            assert_eq!(found.len(), kept.len(), "{case:?}");
            assert_eq!(found[..found.len() - 1], kept[..kept.len() - 1], "{case:?}");
            let counts = SnapshotCounts::of(&through).by_bound();
            let over = |(each, count): (SnapshotBound, u64)| each == bound && count > limit;
            assert!(counts.into_iter().any(over), "{case:?}");
        }
        assert_eq!(first_overs, 8);
    }
}
