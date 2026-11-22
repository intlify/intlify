// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The formal Snapshots a binding submits for `programmatic_snapshot_admission`
//! (D15:2765): the value of each recipe a `file_decode_and_source_map` case
//! materializes, other than the padded one, whose value is Minimal's, so a
//! file and a Snapshot of one value pair (D15:2788); and Minimal's value
//! holding rejected-node markers, which the boundary admits and no document
//! holds (D15:140, 861).
//!
//! A Snapshot's tree is what the file's bytes spell: each object's members in
//! the order the file writes them, so the reversed recipe submits them
//! reversed, and each number as its token reads, so negative zero reaches
//! the boundary, which normalizes both (D15:871, 889). Its digest is the one
//! the canonical tree has. Everything is built before any interval, as
//! live-host inspection is (D15:2824).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::boundary::snapshot::{SnapshotReport, SubmittedMarker, SubmittedNode};
use crate::input_limits::Bound;
use crate::materialize::{materialize_file, Locus, MaterializedDocument, NodeKind, ObjectMember};
use crate::resolver::capability::SnapshotLimits;
use crate::snapshot::{Marker, MarkerReason, Node};

use super::prepare::fixture_capacity;
use super::workflow_inputs::ROOM;
use super::Recipe;

/// How many markers the recipe with many of them holds.
const MARKERS: usize = 64;

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
}

impl SnapshotRecipe {
    pub(in crate::benchmark) const ALL: [Self; 12] = [
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
    ];

    /// The file recipe whose value the Snapshot submits: its own for a
    /// value's copy, and Minimal's, beside which a marked recipe holds its
    /// markers.
    pub(in crate::benchmark) const fn file(self) -> Recipe {
        match self {
            Self::Minimal | Self::UnsupportedType | Self::Cycle | Self::ManyMarkers => {
                Recipe::Minimal
            }
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

    /// Where a marked recipe holds its markers, a root member's key, and
    /// their reasons: one marker is the member's value, and many are the
    /// elements of an array, cycling through every ordinary reason. `None`
    /// for a value's copy.
    fn marked(self) -> Option<(&'static str, Vec<MarkerReason>)> {
        match self {
            Self::UnsupportedType => Some(("onLoad", vec![MarkerReason::UnsupportedType])),
            Self::Cycle => Some(("self", vec![MarkerReason::Cycle])),
            Self::ManyMarkers => Some((
                "hooks",
                MarkerReason::ALL
                    .into_iter()
                    .cycle()
                    .take(MARKERS)
                    .collect(),
            )),
            _ => None,
        }
    }
}

/// The `bootstrapOnly.snapshot` bounds a case's Snapshot is admitted under:
/// each at the room the complete invocation's capability leaves a
/// programmatic entry's Snapshot, which no recipe's tree reaches.
pub(in crate::benchmark) fn limits() -> SnapshotLimits {
    let room = Bound::new(ROOM).expect("a positive bound");
    SnapshotLimits {
        max_nodes: room,
        max_depth: room,
        max_collection_entries: room,
        max_total_string_bytes: room,
        max_single_string_bytes: room,
        max_rejected_markers: room,
        max_envelope_canonical_bytes: room,
        max_single_marker_canonical_bytes: room,
        max_traversal_units: room,
    }
}

/// The formal Snapshot a binding submits for `recipe`: the tree its file
/// recipe's bytes spell, with a marked recipe's markers, and the digest of
/// the canonical tree. `None` when the file does not materialize under the
/// fixture capacity.
pub(in crate::benchmark) fn report(recipe: SnapshotRecipe) -> Option<SnapshotReport> {
    let doc = materialize_file(recipe.file().source(), fixture_capacity().0).ok()?;
    let mut root = submitted(&doc)?;
    let mut canonical = doc.canonical_value();
    if let Some((key, reasons)) = recipe.marked() {
        let (SubmittedNode::Object(members), Node::Object(tree)) = (&mut root, &mut canonical)
        else {
            return None;
        };
        let marker = |reason: &MarkerReason| {
            SubmittedNode::Marker(SubmittedMarker {
                reason: reason.id().to_owned(),
                bound: None,
                witness: None,
            })
        };
        let rejected = |reason: &MarkerReason| Node::Marker(Marker::Rejected(*reason));
        let (value, node) = match &reasons[..] {
            [reason] => (marker(reason), rejected(reason)),
            _ => (
                SubmittedNode::Array(reasons.iter().map(marker).collect()),
                Node::Array(reasons.iter().map(rejected).collect()),
            ),
        };
        members.push((key.as_bytes().to_vec(), value));
        tree.insert(key.to_owned(), node);
    }
    Some(SnapshotReport::Formal {
        digest: canonical.digest(),
        root,
    })
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::boundary::snapshot::read_report;
    use crate::input_limits::{InputLimits, RawInputLimits, ValueLimits};
    use crate::snapshot::SnapshotCounts;

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
    /// negative, and a marked recipe's markers beside Minimal's value.
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
            if recipe.materializes() {
                assert!(*admitted.root() == doc.canonical_value(), "{recipe:?}");
            }
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
                _ => panic!("{recipe:?} holds its markers as declared"),
            }
        }
    }

    /// A value's copy submits the value of the file recipe of its own name,
    /// and a marked recipe Minimal's.
    #[test]
    fn each_snapshot_copies_the_file_recipe_it_names() {
        for recipe in SnapshotRecipe::ALL {
            let named = if recipe.materializes() {
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
}
