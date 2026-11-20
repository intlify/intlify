// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's `programmatic-entry-snapshot` boundary: the formal Snapshot a
//! binding built from a host value, its tree and the digest it supplied,
//! admitted as the canonical tree or refused with one of the five reasons of
//! its row (D15:869, 871, 891, 3148; 015-199, 015-200).
//!
//! The binding reports the Snapshot, or that it could not build one without
//! running host behavior, which includes not being able to construct the
//! reserved marker (D15:865, 869). The submitted tree can hold every form the
//! boundary refuses, so tests and conformance supply each without a live
//! host: a number that is not finite or not portable, a string or key that is
//! not a sequence of Unicode scalar values, keys in any order or repeated, and
//! a marker outside the closed model.
//!
//! The checks follow 015's failure positions (D15:3155), and the first that
//! fails decides:
//!
//! 1. a Snapshot is present, then built safely;
//! 2. the bounds: `S(root)` and every marker frame fit their byte bounds, and
//!    the tree without its terminal marker and the edge that holds it fits all
//!    nine `bootstrapOnly.snapshot` bounds, or the tree is
//!    `snapshot-envelope-over-bound`. The terminal marker is the last node in
//!    canonical traversal order when its reason is `bootstrap-limit-exceeded`,
//!    whatever its other fields, which the closed model checks next;
//! 3. the closed model: each marker has an ordinary reason and no other field,
//!    or `bootstrap-limit-exceeded`, a `bootstrapOnly.snapshot` bound's path,
//!    and a witness, and the digest is `sha256:` and 64 lowercase hexadecimal
//!    digits, or the Snapshot is `malformed-snapshot`;
//! 4. the remaining value rules: no key repeats, every number is portable,
//!    every string and key is a sequence of Unicode scalar values, and the
//!    only bootstrap marker is the terminal one, whose witness is its bound's
//!    limit plus one, or the tree is `snapshot-envelope-over-bound`. Negative
//!    zero and member order are normalized, not refused;
//! 5. the canonical tree's digest is the one supplied, or the Snapshot is
//!    `snapshot-content-digest-mismatch`.

use std::collections::BTreeMap;
use std::ptr;

use crate::artifacts::count;
use crate::materialize::PortableNumber;
use crate::references::valid_semantic_digest;
use crate::resolver::capability::SnapshotLimits;
use crate::snapshot::{
    Marker, MarkerReason, Node, SnapshotBound, SnapshotCounts, BOOTSTRAP_LIMIT_EXCEEDED,
};

use super::capability::ImplementationCapability;

/// A frame's tag and `U64` length (D15:873).
const FRAME: u64 = 9;
/// A container's `U64` count, and a number's binary64 bytes.
const EIGHT: u64 = 8;

/// The binding's report about the host value. No `Debug`: a Snapshot carries
/// configuration.
pub(crate) enum SnapshotReport {
    /// The binding could not inspect the host value without running its
    /// behavior, or could not construct the reserved marker (D15:865, 869).
    Unsafe,
    /// The formal Snapshot: the tree the binding built and the digest it
    /// supplied (D15:891).
    Formal { root: SubmittedNode, digest: String },
}

/// One node of a submitted tree. No `Debug`.
#[derive(Clone)]
pub(crate) enum SubmittedNode {
    Null,
    Boolean(bool),
    Number(f64),
    /// The string's bytes: its UTF-8 when it is a sequence of Unicode scalar
    /// values, and other bytes, such as the generalized UTF-8 of an isolated
    /// surrogate, when it is not.
    String(Vec<u8>),
    Array(Vec<SubmittedNode>),
    /// The members in the order submitted, each key's bytes as a string's.
    Object(Vec<(Vec<u8>, SubmittedNode)>),
    Marker(SubmittedMarker),
}

/// A marker as submitted: its reason ID, and the bound path and witness a
/// `bootstrap-limit-exceeded` marker carries, each `None` when absent.
#[derive(Clone)]
pub(crate) struct SubmittedMarker {
    pub(crate) reason: String,
    pub(crate) bound: Option<String>,
    pub(crate) witness: Option<u64>,
}

/// Why the boundary refuses a report: one of the five reasons of its row, in
/// that order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SnapshotRefusal {
    Missing,
    UnsafeHostInspection,
    /// A bound is exceeded, or a model value is not finite and canonical.
    EnvelopeOverBound,
    /// A value outside the closed Snapshot model.
    Malformed,
    /// The supplied digest is not the canonical tree's.
    ContentDigestMismatch,
}

impl SnapshotRefusal {
    /// The reason as 015's boundary table spells it.
    pub(crate) const fn reason(self) -> &'static str {
        match self {
            Self::Missing => "missing-snapshot",
            Self::UnsafeHostInspection => "unsafe-host-inspection",
            Self::EnvelopeOverBound => "snapshot-envelope-over-bound",
            Self::Malformed => "malformed-snapshot",
            Self::ContentDigestMismatch => "snapshot-content-digest-mismatch",
        }
    }
}

/// An admitted Snapshot: its canonical tree and the verified digest, which is
/// also the Programmatic Entry revision (D15:743, 889). No `Debug`.
pub(crate) struct AdmittedSnapshot {
    root: Node,
    digest: String,
}

impl AdmittedSnapshot {
    pub(crate) const fn root(&self) -> &Node {
        &self.root
    }

    /// The digest as presented: `sha256:` and 64 lowercase hexadecimal digits.
    pub(crate) fn digest(&self) -> &str {
        &self.digest
    }

    /// A canonical tree as if admitted, for tests of what reads one.
    #[cfg(test)]
    pub(crate) fn of(root: Node) -> Self {
        let digest = root.digest();
        Self { root, digest }
    }
}

/// Admit a report, `None` when no Snapshot was supplied, under the
/// capability's `bootstrapOnly.snapshot` bounds.
pub(crate) fn admit(
    report: Option<SnapshotReport>,
    capability: &ImplementationCapability,
) -> Result<AdmittedSnapshot, SnapshotRefusal> {
    read_report(report, &capability.bootstrap_only().snapshot)
}

/// The boundary's checks under `limits`, which [`admit`] takes from the
/// capability.
pub(crate) fn read_report(
    report: Option<SnapshotReport>,
    limits: &SnapshotLimits,
) -> Result<AdmittedSnapshot, SnapshotRefusal> {
    use SnapshotRefusal::{
        ContentDigestMismatch, EnvelopeOverBound, Malformed, Missing, UnsafeHostInspection,
    };

    let (root, digest) = match report.ok_or(Missing)? {
        SnapshotReport::Unsafe => return Err(UnsafeHostInspection),
        SnapshotReport::Formal { root, digest } => (root, digest),
    };
    let terminal = terminal_marker(&root);
    if !within_bounds(&root, terminal, limits) {
        return Err(EnvelopeOverBound);
    }
    if !closed(&root) || !valid_semantic_digest(&digest) {
        return Err(Malformed);
    }
    let tree = canonical(&root, terminal, limits).ok_or(EnvelopeOverBound)?;
    if tree.digest() != digest {
        return Err(ContentDigestMismatch);
    }
    Ok(AdmittedSnapshot { root: tree, digest })
}

/// The last node in canonical traversal order, the last element of each array
/// and the member with the greatest key bytes of each object, when its reason
/// is `bootstrap-limit-exceeded` (D15:867). Of repeated keys, the one
/// submitted later comes later.
fn terminal_marker(root: &SubmittedNode) -> Option<&SubmittedNode> {
    let mut node = root;
    loop {
        node = match node {
            SubmittedNode::Array(elements) => elements.last()?,
            SubmittedNode::Object(members) => {
                &members.iter().max_by(|left, right| left.0.cmp(&right.0))?.1
            }
            SubmittedNode::Marker(marker) if marker.reason == BOOTSTRAP_LIMIT_EXCEEDED => {
                return Some(node);
            }
            _ => return None,
        };
    }
}

/// Whether `node` is the terminal marker itself.
fn is_terminal(node: &SubmittedNode, terminal: Option<&SubmittedNode>) -> bool {
    terminal.is_some_and(|terminal| ptr::eq(node, terminal))
}

/// Whether `S(root)` and every marker frame fit their byte bounds and the
/// tree without its terminal marker fits every bound (D15:869, 889; 015-199).
fn within_bounds(
    root: &SubmittedNode,
    terminal: Option<&SubmittedNode>,
    limits: &SnapshotLimits,
) -> bool {
    let (whole, rest) = counts(root, terminal);
    whole.envelope_canonical_bytes <= limits.max_envelope_canonical_bytes.get()
        && whole.single_marker_canonical_bytes <= limits.max_single_marker_canonical_bytes.get()
        && rest
            .by_bound()
            .into_iter()
            .all(|(bound, count)| count <= bound.limit(limits).get())
}

/// The counts of the whole tree and of the tree without `terminal` and the
/// edge that holds it, in the units of [`SnapshotCounts`]. One pass counts
/// both without recursion, so a report of any depth is compared with the
/// bounds rather than followed down the stack (plan 025 decision #298).
/// `S(root)` is as long as every node's frame header and own payload
/// together, an object's key frames included. A terminal root is counted in
/// both: every bound is at least one and its frame is checked whole, so
/// leaving it out would change nothing.
fn counts(
    root: &SubmittedNode,
    terminal: Option<&SubmittedNode>,
) -> (SnapshotCounts, SnapshotCounts) {
    let mut whole = SnapshotCounts::default();
    let mut rest = SnapshotCounts::default();
    // Each node to count, its depth, and whether the rest counts it: all but
    // the terminal marker, a leaf, below the root.
    let mut pending = vec![(root, 1, true)];
    while let Some((node, depth, kept)) = pending.pop() {
        for counts in [Some(&mut whole), kept.then_some(&mut rest)]
            .into_iter()
            .flatten()
        {
            own(counts, node, depth);
        }
        match node {
            SubmittedNode::Array(elements) => {
                for element in elements {
                    let kept = !is_terminal(element, terminal);
                    whole.collection_entries += 1;
                    rest.collection_entries += u64::from(kept);
                    pending.push((element, depth + 1, kept));
                }
            }
            SubmittedNode::Object(members) => {
                for (key, value) in members {
                    let kept = !is_terminal(value, terminal);
                    for counts in [Some(&mut whole), kept.then_some(&mut rest)]
                        .into_iter()
                        .flatten()
                    {
                        counts.collection_entries += 1;
                        counts.envelope_canonical_bytes += FRAME + string(counts, key);
                    }
                    pending.push((value, depth + 1, kept));
                }
            }
            _ => {}
        }
    }
    for counts in [&mut whole, &mut rest] {
        counts.traversal_units = counts.nodes + counts.collection_entries;
    }
    (whole, rest)
}

/// Count `node` at `depth`: the node, its level, its own bytes, and its frame
/// header and own payload in the envelope. Its members count on their own.
fn own(counts: &mut SnapshotCounts, node: &SubmittedNode, depth: u64) {
    counts.nodes += 1;
    counts.depth = counts.depth.max(depth);
    let payload = match node {
        SubmittedNode::Null | SubmittedNode::Boolean(_) => 0,
        SubmittedNode::Number(_) | SubmittedNode::Array(_) | SubmittedNode::Object(_) => EIGHT,
        SubmittedNode::String(bytes) => string(counts, bytes),
        SubmittedNode::Marker(marker) => {
            counts.rejected_markers += 1;
            let record = marker.record_length();
            counts.single_marker_canonical_bytes =
                counts.single_marker_canonical_bytes.max(FRAME + record);
            record
        }
    };
    counts.envelope_canonical_bytes += FRAME + payload;
}

/// Count a string or key's bytes and return their number.
fn string(counts: &mut SnapshotCounts, bytes: &[u8]) -> u64 {
    let length = count(bytes.len());
    counts.total_string_bytes += length;
    counts.single_string_bytes = counts.single_string_bytes.max(length);
    length
}

impl SubmittedMarker {
    /// The length of its record, one field frame for each field it carries
    /// (D15:889).
    fn record_length(&self) -> u64 {
        let field = |length: usize| FRAME + count(length);
        field(self.reason.len())
            + self.bound.as_ref().map_or(0, |bound| field(bound.len()))
            + self.witness.map_or(0, |_| field(8))
    }

    /// The marker in the closed model: an ordinary reason with no other field,
    /// or `bootstrap-limit-exceeded` with a `bootstrapOnly.snapshot` bound's
    /// path and a witness (D15:861, 889; 015-200).
    fn model(&self) -> Option<Marker> {
        match (&self.bound, self.witness) {
            (None, None) => MarkerReason::from_id(&self.reason).map(Marker::Rejected),
            (Some(path), Some(witness)) if self.reason == BOOTSTRAP_LIMIT_EXCEEDED => {
                SnapshotBound::from_path(path)
                    .map(|bound| Marker::BootstrapLimitExceeded { bound, witness })
            }
            _ => None,
        }
    }
}

/// Whether every marker is in the closed model, checked without recursion.
fn closed(root: &SubmittedNode) -> bool {
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        match node {
            SubmittedNode::Array(elements) => pending.extend(elements),
            SubmittedNode::Object(members) => {
                pending.extend(members.iter().map(|(_, value)| value));
            }
            SubmittedNode::Marker(marker) if marker.model().is_none() => return false,
            _ => {}
        }
    }
    true
}

/// A container the canonical tree is being built in: the members built so
/// far and those to come, and for an object the key of the member being
/// built.
enum Open<'a> {
    Array(Vec<Node>, std::slice::Iter<'a, SubmittedNode>),
    Object(
        BTreeMap<String, Node>,
        std::slice::Iter<'a, (Vec<u8>, SubmittedNode)>,
        String,
    ),
}

/// The canonical tree, or `None` when a model value is not finite and
/// canonical: a repeated key, a number that is not portable, a string or key
/// that is not a sequence of Unicode scalar values, a bootstrap marker other
/// than `terminal`, or a terminal one whose witness is not its bound's limit
/// plus one (015-199, 015-200). Every marker is in the closed model by now.
/// Built without recursion: a container opens on a stack, and each finished
/// node goes into the container on top (plan 025 decision #298).
fn canonical(
    root: &SubmittedNode,
    terminal: Option<&SubmittedNode>,
    limits: &SnapshotLimits,
) -> Option<Node> {
    let mut open: Vec<Open<'_>> = Vec::new();
    let mut node = root;
    loop {
        let mut done = match node {
            SubmittedNode::Null => Node::Null,
            SubmittedNode::Boolean(value) => Node::Boolean(*value),
            SubmittedNode::Number(value) => Node::Number(PortableNumber::new(*value)?),
            SubmittedNode::String(bytes) => Node::String(scalars(bytes)?),
            SubmittedNode::Marker(marker) => match marker.model()? {
                Marker::BootstrapLimitExceeded { bound, witness }
                    if !is_terminal(node, terminal) || witness != bound.limit(limits).get() + 1 =>
                {
                    return None;
                }
                marker => Node::Marker(marker),
            },
            SubmittedNode::Array(elements) => {
                let mut members = elements.iter();
                if let Some(first) = members.next() {
                    open.push(Open::Array(Vec::with_capacity(elements.len()), members));
                    node = first;
                    continue;
                }
                Node::Array(Vec::new())
            }
            SubmittedNode::Object(members) => {
                let mut members = members.iter();
                if let Some((key, first)) = members.next() {
                    open.push(Open::Object(BTreeMap::new(), members, scalars(key)?));
                    node = first;
                    continue;
                }
                Node::Object(BTreeMap::new())
            }
        };
        // Put the finished node in the container on top, closing each
        // container that has no member left, until one has a next member.
        loop {
            let Some(mut top) = open.pop() else {
                return Some(done);
            };
            let next = match &mut top {
                Open::Array(built, members) => {
                    built.push(done);
                    members.next()
                }
                Open::Object(built, members, key) => {
                    if built.insert(std::mem::take(key), done).is_some() {
                        return None;
                    }
                    match members.next() {
                        Some((next_key, next)) => {
                            *key = scalars(next_key)?;
                            Some(next)
                        }
                        None => None,
                    }
                }
            };
            if let Some(next) = next {
                open.push(top);
                node = next;
                break;
            }
            done = match top {
                Open::Array(built, _) => Node::Array(built),
                Open::Object(built, _, _) => Node::Object(built),
            };
        }
    }
}

impl SubmittedNode {
    /// Move this node's members into `pending`, leaving it with none.
    fn release(&mut self, pending: &mut Vec<Self>) {
        match self {
            Self::Array(elements) => pending.append(elements),
            Self::Object(members) => pending.extend(members.drain(..).map(|(_, value)| value)),
            Self::Null | Self::Boolean(_) | Self::Number(_) | Self::String(_) | Self::Marker(_) => {
            }
        }
    }
}

/// A report's tree is released without recursion, so dropping a deep one,
/// refused or not, cannot exhaust the stack (plan 025 decision #298).
impl Drop for SubmittedNode {
    fn drop(&mut self) {
        let mut pending = Vec::new();
        self.release(&mut pending);
        while let Some(mut node) = pending.pop() {
            node.release(&mut pending);
        }
    }
}

/// The string the bytes spell, if they are a sequence of Unicode scalar
/// values.
fn scalars(bytes: &[u8]) -> Option<String> {
    String::from_utf8(bytes.to_vec()).ok()
}

/// The submitted form of a canonical node, its members in key order, for
/// tests that run a tree through the boundary.
#[cfg(test)]
pub(crate) fn submitted(node: &Node) -> SubmittedNode {
    match node {
        Node::Null => SubmittedNode::Null,
        Node::Boolean(value) => SubmittedNode::Boolean(*value),
        Node::Number(number) => SubmittedNode::Number(number.get()),
        Node::String(text) => SubmittedNode::String(text.as_bytes().to_vec()),
        Node::Array(elements) => SubmittedNode::Array(elements.iter().map(submitted).collect()),
        Node::Object(members) => SubmittedNode::Object(
            members
                .iter()
                .map(|(key, value)| (key.as_bytes().to_vec(), submitted(value)))
                .collect(),
        ),
        Node::Marker(Marker::Rejected(reason)) => SubmittedNode::Marker(SubmittedMarker {
            reason: reason.id().to_owned(),
            bound: None,
            witness: None,
        }),
        Node::Marker(Marker::BootstrapLimitExceeded { bound, witness }) => {
            SubmittedNode::Marker(SubmittedMarker {
                reason: BOOTSTRAP_LIMIT_EXCEEDED.to_owned(),
                bound: Some(bound.path()),
                witness: Some(*witness),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::construction::capability::{IDENTITY, REVISION};
    use crate::input_limits::Bound;
    use crate::locale::VersionedIdentity;
    use crate::resolver::limits::fixture_limits;
    use crate::snapshot::from_json;
    use SnapshotRefusal::{ContentDigestMismatch, EnvelopeOverBound, Malformed};

    /// A digest in the presentation that is no tree's here.
    const OTHER: &str = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

    /// The digest of the tree in `a_canonical_snapshot_is_admitted_with_its_digest`,
    /// which `snapshot.rs`'s tests computed outside this crate.
    const DIGEST: &str = "sha256:6a85cb2bd154bea87ba2c6e0d488e10e9c61bb5849f46bd58bc4e710001e6281";

    /// Limits no tree here is over.
    fn roomy() -> SnapshotLimits {
        let largest = Bound::new(u64::MAX - 1).unwrap();
        SnapshotLimits {
            max_nodes: largest,
            max_depth: largest,
            max_collection_entries: largest,
            max_total_string_bytes: largest,
            max_single_string_bytes: largest,
            max_rejected_markers: largest,
            max_envelope_canonical_bytes: largest,
            max_single_marker_canonical_bytes: largest,
            max_traversal_units: largest,
        }
    }

    /// Roomy limits with `bound` at `value`.
    fn under(bound: SnapshotBound, value: u64) -> SnapshotLimits {
        let mut limits = roomy();
        let field = match bound {
            SnapshotBound::Nodes => &mut limits.max_nodes,
            SnapshotBound::Depth => &mut limits.max_depth,
            SnapshotBound::CollectionEntries => &mut limits.max_collection_entries,
            SnapshotBound::TotalStringBytes => &mut limits.max_total_string_bytes,
            SnapshotBound::SingleStringBytes => &mut limits.max_single_string_bytes,
            SnapshotBound::RejectedMarkers => &mut limits.max_rejected_markers,
            SnapshotBound::EnvelopeCanonicalBytes => &mut limits.max_envelope_canonical_bytes,
            SnapshotBound::SingleMarkerCanonicalBytes => {
                &mut limits.max_single_marker_canonical_bytes
            }
            SnapshotBound::TraversalUnits => &mut limits.max_traversal_units,
        };
        *field = Bound::new(value).unwrap();
        limits
    }

    fn marker(reason: &str, bound: Option<&str>, witness: Option<u64>) -> SubmittedNode {
        SubmittedNode::Marker(SubmittedMarker {
            reason: reason.to_owned(),
            bound: bound.map(str::to_owned),
            witness,
        })
    }

    fn object(members: Vec<(&str, SubmittedNode)>) -> SubmittedNode {
        SubmittedNode::Object(
            members
                .into_iter()
                .map(|(key, value)| (key.as_bytes().to_vec(), value))
                .collect(),
        )
    }

    fn rejected(reason: MarkerReason) -> Node {
        Node::Marker(Marker::Rejected(reason))
    }

    fn overrun(bound: SnapshotBound, witness: u64) -> Node {
        Node::Marker(Marker::BootstrapLimitExceeded { bound, witness })
    }

    /// The object `node` with `key` set to `value`.
    fn with_member(node: &Node, key: &str, value: Node) -> Node {
        let mut node = node.clone();
        let Node::Object(members) = &mut node else {
            unreachable!()
        };
        members.insert(key.to_owned(), value);
        node
    }

    /// A tree whose every count is at least two: an array of four scalars,
    /// two ordinary markers, one of them in a nested object.
    fn rich() -> Node {
        let inner = with_member(
            &from_json(&json!({})),
            "d",
            rejected(MarkerReason::SparseArray),
        );
        let root = with_member(
            &from_json(&json!({ "a": [null, true, 1.5, "xy"] })),
            "b",
            rejected(MarkerReason::Cycle),
        );
        with_member(&root, "c", inner)
    }

    /// The count of `bound` in `counts`.
    fn count_of(counts: SnapshotCounts, bound: SnapshotBound) -> u64 {
        counts
            .by_bound()
            .into_iter()
            .find(|(each, _)| *each == bound)
            .unwrap()
            .1
    }

    fn formal(root: SubmittedNode, digest: &str) -> SnapshotReport {
        SnapshotReport::Formal {
            root,
            digest: digest.to_owned(),
        }
    }

    /// Admit `root` with the digest of `expected`, which must be its
    /// canonical tree.
    fn admits(root: SubmittedNode, expected: &Node, limits: &SnapshotLimits) {
        let admitted = read_report(Some(formal(root, &expected.digest())), limits).unwrap();
        assert_eq!(admitted.root().framed(), expected.framed());
        assert_eq!(admitted.digest(), expected.digest());
    }

    fn refused(report: Option<SnapshotReport>, limits: &SnapshotLimits) -> SnapshotRefusal {
        read_report(report, limits)
            .err()
            .expect("the Snapshot is refused")
    }

    /// The refusal of `root` with a digest that is not its own, so a tree
    /// that passes every other check is a mismatch.
    fn refusal(root: SubmittedNode, limits: &SnapshotLimits) -> SnapshotRefusal {
        refused(Some(formal(root, OTHER)), limits)
    }

    #[test]
    fn a_canonical_snapshot_is_admitted_with_its_digest() {
        let tree = with_member(
            &from_json(&json!({ "a": [null, true, 1.5, "x"] })),
            "b",
            rejected(MarkerReason::Cycle),
        );
        let admitted = read_report(Some(formal(submitted(&tree), DIGEST)), &roomy()).unwrap();
        assert_eq!(admitted.root().framed(), tree.framed());
        assert_eq!(admitted.digest(), DIGEST);
        admits(submitted(&rich()), &rich(), &roomy());
    }

    /// `S` frames negative zero as positive zero and members in key order, so
    /// neither is refused (D15:869, 889; 015-200).
    #[test]
    fn negative_zero_and_member_order_are_normalized() {
        let root = object(vec![
            ("é", SubmittedNode::Number(-0.0)),
            ("z", SubmittedNode::Array(vec![SubmittedNode::Number(-0.0)])),
            ("a", SubmittedNode::Null),
        ]);
        admits(
            root,
            &from_json(&json!({ "a": null, "z": [0], "é": 0 })),
            &roomy(),
        );
    }

    #[test]
    fn a_missing_or_unsafe_report_is_refused_first() {
        assert_eq!(refused(None, &roomy()), SnapshotRefusal::Missing);
        assert_eq!(
            refused(Some(SnapshotReport::Unsafe), &roomy()),
            SnapshotRefusal::UnsafeHostInspection
        );
    }

    /// Each bound admits a tree that reaches it and refuses the tree once it
    /// is one less, in the canonical counts' units (D15:1796-1804).
    #[test]
    fn each_bound_admits_its_exact_count_and_refuses_one_more() {
        let tree = rich();
        for (bound, count) in SnapshotCounts::of(&tree).by_bound() {
            assert!(count >= 2, "{bound:?}");
            admits(submitted(&tree), &tree, &under(bound, count));
            assert_eq!(
                refusal(submitted(&tree), &under(bound, count - 1)),
                EnvelopeOverBound,
                "{bound:?}"
            );
        }
    }

    /// The tree without its terminal marker and the member holding it must
    /// fit every bound, while `S(root)` and the marker's frame count it
    /// (015-199).
    #[test]
    fn the_terminal_marker_is_left_out_of_the_bounds_but_not_its_frames() {
        let rest = rich();
        for (bound, count) in SnapshotCounts::of(&rest).by_bound() {
            let tree = |witness| with_member(&rest, "z", overrun(bound, witness));
            // The two byte bounds hold the whole tree, marker included.
            let limit = match bound {
                SnapshotBound::EnvelopeCanonicalBytes
                | SnapshotBound::SingleMarkerCanonicalBytes => {
                    count_of(SnapshotCounts::of(&tree(1)), bound)
                }
                _ => count,
            };
            let reached = tree(limit + 1);
            admits(submitted(&reached), &reached, &under(bound, limit));
            assert_eq!(
                refusal(submitted(&tree(limit)), &under(bound, limit - 1)),
                EnvelopeOverBound,
                "{bound:?}"
            );
        }
    }

    /// The terminal marker stands where the first overrun would be, so the
    /// tree with it may pass the bound it names: the fourth of three nodes,
    /// the root in place of a string that is too long, or a level too deep.
    #[test]
    fn a_terminal_marker_sits_at_the_first_overrun() {
        let fourth = Node::Array(vec![
            from_json(&json!(1)),
            from_json(&json!(2)),
            overrun(SnapshotBound::Nodes, 4),
        ]);
        admits(submitted(&fourth), &fourth, &under(SnapshotBound::Nodes, 3));
        let root = overrun(SnapshotBound::SingleStringBytes, 3);
        admits(
            submitted(&root),
            &root,
            &under(SnapshotBound::SingleStringBytes, 2),
        );
        let deep = Node::Array(vec![Node::Array(vec![overrun(SnapshotBound::Depth, 3)])]);
        admits(submitted(&deep), &deep, &under(SnapshotBound::Depth, 2));
    }

    /// Ordinary markers and the terminal marker together are a valid Snapshot
    /// (D15:3165).
    #[test]
    fn ordinary_markers_and_the_terminal_marker_are_admitted() {
        let tree = Node::Array(vec![
            rejected(MarkerReason::UnsupportedType),
            from_json(&json!("x")),
            rejected(MarkerReason::Cycle),
            overrun(SnapshotBound::RejectedMarkers, 3),
        ]);
        admits(
            submitted(&tree),
            &tree,
            &under(SnapshotBound::RejectedMarkers, 2),
        );
    }

    /// Only the last node in key order may be a bootstrap marker, whatever
    /// order the members came in (015-199).
    #[test]
    fn only_the_last_node_may_be_a_bootstrap_marker() {
        let limits = under(SnapshotBound::Nodes, 5);
        let terminal = || submitted(&overrun(SnapshotBound::Nodes, 6));
        let one = || SubmittedNode::Number(1.0);
        let tree = with_member(
            &from_json(&json!({ "a": 1 })),
            "z",
            overrun(SnapshotBound::Nodes, 6),
        );
        admits(
            object(vec![("z", terminal()), ("a", one())]),
            &tree,
            &limits,
        );
        for root in [
            object(vec![("a", terminal()), ("z", one())]),
            SubmittedNode::Array(vec![terminal(), one()]),
            SubmittedNode::Array(vec![terminal(), terminal()]),
            SubmittedNode::Array(vec![SubmittedNode::Array(vec![terminal()]), one()]),
        ] {
            assert_eq!(refusal(root, &limits), EnvelopeOverBound);
        }
    }

    /// The terminal marker's witness is the limit plus one of the bound it
    /// names (015-199).
    #[test]
    fn the_witness_is_the_named_bound_plus_one() {
        let limits = under(SnapshotBound::Nodes, 5);
        let array = |witness| Node::Array(vec![overrun(SnapshotBound::Nodes, witness)]);
        admits(submitted(&array(6)), &array(6), &limits);
        for witness in [5, 7, u64::MAX] {
            assert_eq!(
                refusal(submitted(&array(witness)), &limits),
                EnvelopeOverBound,
                "{witness}"
            );
        }
        // Six is another bound's limit plus one, not this one's.
        assert_eq!(
            refusal(submitted(&array(6)), &under(SnapshotBound::Depth, 5)),
            EnvelopeOverBound
        );
    }

    /// A repeated key, a number that is not portable, and a string or key
    /// that is not a sequence of Unicode scalar values are model values that
    /// are not finite and canonical (015-200).
    #[test]
    fn a_model_value_that_is_not_canonical_is_over_bound() {
        // The generalized UTF-8 of the isolated surrogate U+D800.
        let isolated = vec![0xed, 0xa0, 0x80];
        for root in [
            object(vec![("k", SubmittedNode::Null), ("k", SubmittedNode::Null)]),
            SubmittedNode::Number(f64::NAN),
            SubmittedNode::Number(f64::INFINITY),
            SubmittedNode::Number(f64::NEG_INFINITY),
            SubmittedNode::Number(9_007_199_254_740_992.0),
            SubmittedNode::Number(-9_007_199_254_740_992.0),
            SubmittedNode::String(isolated.clone()),
            SubmittedNode::String(vec![0xff]),
            SubmittedNode::Object(vec![(isolated, SubmittedNode::Null)]),
            SubmittedNode::Object(vec![
                (b"a".to_vec(), SubmittedNode::Null),
                (vec![0xff], SubmittedNode::Null),
            ]),
            SubmittedNode::Array(vec![SubmittedNode::Null, SubmittedNode::Number(f64::NAN)]),
        ] {
            assert_eq!(refusal(root, &roomy()), EnvelopeOverBound);
        }
        for value in [9_007_199_254_740_991.0, -9_007_199_254_740_991.0] {
            admits(
                SubmittedNode::Number(value),
                &from_json(&json!(value)),
                &roomy(),
            );
        }
    }

    /// A marker outside the closed vocabulary or with fields its reason does
    /// not take, a bound path that names no `bootstrapOnly.snapshot` bound,
    /// and a digest outside its presentation are outside the closed model
    /// (015-200).
    #[test]
    fn a_value_outside_the_closed_model_is_malformed() {
        let path = SnapshotBound::Nodes.path();
        let nodes = path.as_str();
        let markers = [
            marker("unknown", None, None),
            marker("Cycle", None, None),
            marker("cycle", Some(nodes), None),
            marker("cycle", None, Some(1)),
            marker("cycle", Some(nodes), Some(u64::MAX)),
            marker(BOOTSTRAP_LIMIT_EXCEEDED, None, None),
            marker(BOOTSTRAP_LIMIT_EXCEEDED, Some(nodes), None),
            marker(BOOTSTRAP_LIMIT_EXCEEDED, None, Some(u64::MAX)),
            marker(
                BOOTSTRAP_LIMIT_EXCEEDED,
                Some("bootstrapOnly.rawInput.maxFileBytes"),
                Some(u64::MAX),
            ),
            marker(BOOTSTRAP_LIMIT_EXCEEDED, Some("maxNodes"), Some(u64::MAX)),
        ];
        for marker in markers {
            for root in [
                marker.clone(),
                SubmittedNode::Array(vec![marker.clone(), SubmittedNode::Null]),
                object(vec![("k", marker.clone()), ("l", SubmittedNode::Null)]),
            ] {
                assert_eq!(refusal(root, &roomy()), Malformed);
            }
        }
        let zeros = "0".repeat(64);
        for digest in [
            String::new(),
            "sha256:".to_owned(),
            format!("sha256:{}", &zeros[1..]),
            format!("sha256:{zeros}0"),
            format!("sha256:{}", "A".repeat(64)),
            format!("sha512:{zeros}"),
            zeros.clone(),
        ] {
            assert_eq!(
                refused(Some(formal(SubmittedNode::Null, &digest)), &roomy()),
                Malformed,
                "{digest}"
            );
        }
    }

    /// A digest in its presentation that is not the canonical tree's is a
    /// mismatch (D15:871).
    #[test]
    fn a_digest_other_than_the_trees_is_a_mismatch() {
        assert_eq!(refusal(submitted(&rich()), &roomy()), ContentDigestMismatch);
        assert_eq!(
            refused(
                Some(formal(submitted(&rich()), &Node::Null.digest())),
                &roomy()
            ),
            ContentDigestMismatch
        );
    }

    /// The first failing position decides: the bounds, the closed model, the
    /// remaining value rules, and then the digest (D15:3155; 015-200).
    #[test]
    fn the_first_failing_position_decides() {
        let unknown = || marker("unknown", None, None);
        let nan = || SubmittedNode::Number(f64::NAN);
        let root = || SubmittedNode::Array(vec![unknown(), nan(), SubmittedNode::Null]);
        assert_eq!(
            refusal(root(), &under(SnapshotBound::Nodes, 3)),
            EnvelopeOverBound
        );
        assert_eq!(refusal(root(), &roomy()), Malformed);
        assert_eq!(
            refusal(object(vec![("k", nan()), ("k", unknown())]), &roomy()),
            Malformed
        );
        assert_eq!(refused(Some(formal(nan(), "sha256:")), &roomy()), Malformed);
        assert_eq!(refusal(nan(), &roomy()), EnvelopeOverBound);
    }

    /// The last node with the bootstrap reason is left out of the bounds
    /// whatever its other fields, which the closed model then checks; of
    /// repeated greatest keys, the later one is the last node.
    #[test]
    fn the_terminal_marker_is_found_by_its_place_and_reason() {
        let limits = under(SnapshotBound::Nodes, 3);
        let one = || SubmittedNode::Number(1.0);
        let unknown = || marker("unknown", None, None);
        let no_witness = marker(
            BOOTSTRAP_LIMIT_EXCEEDED,
            Some(SnapshotBound::Nodes.path().as_str()),
            None,
        );
        assert_eq!(
            refusal(
                SubmittedNode::Array(vec![one(), one(), no_witness]),
                &limits
            ),
            Malformed
        );
        assert_eq!(
            refusal(SubmittedNode::Array(vec![one(), one(), unknown()]), &limits),
            EnvelopeOverBound
        );
        let terminal = || submitted(&overrun(SnapshotBound::Nodes, 4));
        assert_eq!(
            refusal(
                object(vec![("a", unknown()), ("z", one()), ("z", terminal())]),
                &limits
            ),
            Malformed
        );
        assert_eq!(
            refusal(
                object(vec![("a", unknown()), ("z", terminal()), ("z", one())]),
                &limits
            ),
            EnvelopeOverBound
        );
    }

    /// The boundary counts a submitted tree in the canonical counts' units,
    /// whatever its member order, and leaves out a terminal member or element
    /// with its edge (D15:1796-1804).
    #[test]
    fn the_counts_are_the_canonical_counts() {
        let terminal = with_member(
            &rich(),
            "z",
            overrun(SnapshotBound::SingleMarkerCanonicalBytes, 2),
        );
        for tree in [
            rich(),
            terminal.clone(),
            Node::Null,
            from_json(&json!({ "é": ["x", {}], "k": -2.5 })),
            overrun(SnapshotBound::Nodes, 2),
        ] {
            let (whole, rest) = counts(&submitted(&tree), None);
            assert_eq!(whole, SnapshotCounts::of(&tree));
            assert_eq!(rest, whole);
        }
        let mut reversed = submitted(&rich());
        let SubmittedNode::Object(members) = &mut reversed else {
            unreachable!()
        };
        members.reverse();
        assert_eq!(counts(&reversed, None).0, SnapshotCounts::of(&rich()));
        for (tree, rest) in [
            (terminal, rich()),
            (
                Node::Array(vec![from_json(&json!(1)), overrun(SnapshotBound::Nodes, 3)]),
                from_json(&json!([1])),
            ),
        ] {
            let report = submitted(&tree);
            let (whole, kept) = counts(&report, terminal_marker(&report));
            assert_eq!(whole, SnapshotCounts::of(&tree));
            assert_eq!(kept, SnapshotCounts::of(&rest));
        }
    }

    /// An empty array or object is admitted as itself, at the root and as a
    /// member or element (plan 025 decision #298).
    #[test]
    fn empty_containers_are_admitted_as_themselves() {
        for tree in [
            Node::Array(Vec::new()),
            Node::Object(BTreeMap::new()),
            from_json(&json!({ "a": [], "b": {}, "c": [[], {}] })),
        ] {
            admits(submitted(&tree), &tree, &roomy());
        }
    }

    /// A report nested far deeper than its depth bound is refused rather than
    /// followed down the stack, and one within roomy bounds is admitted with
    /// the digest of `S` written level by level here. Both are released
    /// without recursion (plan 025 decision #298).
    #[test]
    fn a_deep_report_is_refused_or_admitted_without_recursion() {
        use crate::construction::profile::SNAPSHOT_DOMAIN;
        use intlify_shared_json::encoding::digest_bytes;
        use intlify_shared_json::token::hex;

        const LEVELS: u64 = 100_000;
        let chain = || {
            let mut node = SubmittedNode::Null;
            for _ in 0..LEVELS {
                node = SubmittedNode::Array(vec![node]);
            }
            node
        };
        assert_eq!(
            refusal(chain(), &under(SnapshotBound::Depth, 64)),
            EnvelopeOverBound
        );
        let mut members = SubmittedNode::Null;
        for _ in 0..LEVELS {
            members = SubmittedNode::Object(vec![(b"a".to_vec(), members)]);
        }
        assert_eq!(
            refusal(members, &under(SnapshotBound::Depth, 64)),
            EnvelopeOverBound
        );
        // Each level's array tag, payload length, and count, then null.
        let mut preimage = SNAPSHOT_DOMAIN.as_bytes().to_vec();
        for level in 0..LEVELS {
            preimage.push(0x05);
            preimage.extend_from_slice(&(17 * (LEVELS - level)).to_be_bytes());
            preimage.extend_from_slice(&1_u64.to_be_bytes());
        }
        preimage.extend_from_slice(&[0x01, 0, 0, 0, 0, 0, 0, 0, 0]);
        let digest = format!("sha256:{}", hex(digest_bytes(&preimage)));
        let admitted = read_report(Some(formal(chain(), &digest)), &roomy())
            .ok()
            .unwrap();
        assert_eq!(admitted.digest, digest);
    }

    /// `admit` reads the capability's `bootstrapOnly.snapshot` bounds.
    #[test]
    fn admit_reads_the_capability_snapshot_bounds() {
        let capability = |max_nodes: &str| {
            let mut value = ImplementationCapability::fixture_owned(fixture_limits(1)).normalized();
            let bounds = &mut value["bootstrapOnly"]["snapshot"];
            for (_, bound) in bounds.as_object_mut().unwrap() {
                *bound = json!((u64::MAX - 1).to_string());
            }
            bounds["maxNodes"] = json!(max_nodes);
            let constructed = VersionedIdentity {
                identity: IDENTITY,
                revision: REVISION,
            };
            crate::boundary::capability::admit(Some(&value), &constructed).unwrap()
        };
        let tree = from_json(&json!([1, 2]));
        let read = |max_nodes| {
            admit(
                Some(formal(submitted(&tree), &tree.digest())),
                &capability(max_nodes),
            )
            .err()
        };
        assert_eq!(read("3"), None);
        assert_eq!(read("2"), Some(EnvelopeOverBound));
    }

    #[test]
    fn each_refusal_has_its_reason() {
        use SnapshotRefusal as R;
        assert_eq!(
            [
                R::Missing,
                R::UnsafeHostInspection,
                R::EnvelopeOverBound,
                R::Malformed,
                R::ContentDigestMismatch,
            ]
            .map(R::reason),
            [
                "missing-snapshot",
                "unsafe-host-inspection",
                "snapshot-envelope-over-bound",
                "malformed-snapshot",
                "snapshot-content-digest-mismatch",
            ]
        );
    }
}
