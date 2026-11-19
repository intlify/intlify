// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's canonical Programmatic Entry Snapshot (D15:850-895): the closed tree
//! a binding builds from a host value without running host behavior, its
//! framing `S`, the content digest that is also the Programmatic Entry
//! revision, and what the tree counts under the nine `bootstrapOnly.snapshot`
//! bounds.
//!
//! A canonical tree is already normalized: a number is portable, with
//! negative zero as positive zero; a string or key is a Unicode-scalar
//! string; and an object's keys are unique and ordered by their unsigned
//! UTF-8 bytes, which `BTreeMap<String, _>` keeps by construction
//! (D15:854-859, 871, 889). Admitting a submitted Snapshot into this form, or
//! refusing it, is the `programmatic-entry-snapshot` boundary's work
//! ([`crate::boundary::snapshot`]). Like the file entry's value, the tree
//! carries configuration and has no `Debug`.

use std::collections::BTreeMap;

use intlify_shared_json::encoding::digest_bytes;
use intlify_shared_json::token::hex;

use crate::construction::profile::{MARKER_REASONS, SNAPSHOT_DOMAIN};
use crate::framing::{frame, Record};
use crate::input_limits::Bound;
use crate::materialize::PortableNumber;
use crate::resolver::capability::{BootstrapBoundId, BootstrapGroup, SnapshotLimits};

/// The tags of `S`: 017's value tags, then a number and a marker (D15:873-886).
const NULL: u8 = 0x01;
const FALSE: u8 = 0x02;
const TRUE: u8 = 0x03;
const STRING: u8 = 0x04;
const ARRAY: u8 = 0x05;
const OBJECT: u8 = 0x06;
const NUMBER: u8 = 0x07;
const MARKER: u8 = 0x08;

/// One node of a canonical Snapshot (D15:871).
#[derive(Clone, PartialEq)]
pub(crate) enum Node {
    Null,
    Boolean(bool),
    Number(PortableNumber),
    String(String),
    Array(Vec<Node>),
    Object(BTreeMap<String, Node>),
    Marker(Marker),
}

/// A rejected-node marker: an ordinary data violation, or the terminal
/// marker of a bootstrap overrun naming the exceeded `bootstrapOnly.snapshot`
/// bound and its rejecting witness (D15:861, 867, 889).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Marker {
    Rejected(MarkerReason),
    BootstrapLimitExceeded { bound: SnapshotBound, witness: u64 },
}

/// The closed reasons of an ordinary marker (D15:861).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MarkerReason {
    UnsupportedType,
    NonFiniteNumber,
    NonPortableNumber,
    InvalidUnicode,
    NonStringKey,
    DynamicContainer,
    DynamicProperty,
    SparseArray,
    Cycle,
}

impl MarkerReason {
    /// Every ordinary reason, in 015's order.
    pub(crate) const ALL: [Self; 9] = [
        Self::UnsupportedType,
        Self::NonFiniteNumber,
        Self::NonPortableNumber,
        Self::InvalidUnicode,
        Self::NonStringKey,
        Self::DynamicContainer,
        Self::DynamicProperty,
        Self::SparseArray,
        Self::Cycle,
    ];

    /// The reason ID, as the Profile Specification lists it.
    pub(crate) const fn id(self) -> &'static str {
        MARKER_REASONS[self as usize]
    }

    /// The ordinary reason whose ID is `id`, if any.
    pub(crate) fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|reason| reason.id() == id)
    }
}

/// The reason ID of a bootstrap overrun's marker.
pub(crate) const BOOTSTRAP_LIMIT_EXCEEDED: &str = MARKER_REASONS[9];

/// A `bootstrapOnly.snapshot` bound, in the table's order (D15:1796-1804).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SnapshotBound {
    Nodes,
    Depth,
    CollectionEntries,
    TotalStringBytes,
    SingleStringBytes,
    RejectedMarkers,
    EnvelopeCanonicalBytes,
    SingleMarkerCanonicalBytes,
    TraversalUnits,
}

impl SnapshotBound {
    /// Every bound, in the table's order.
    pub(crate) const ALL: [Self; 9] = [
        Self::Nodes,
        Self::Depth,
        Self::CollectionEntries,
        Self::TotalStringBytes,
        Self::SingleStringBytes,
        Self::RejectedMarkers,
        Self::EnvelopeCanonicalBytes,
        Self::SingleMarkerCanonicalBytes,
        Self::TraversalUnits,
    ];

    /// Where the bound sits in `bootstrapOnly`.
    pub(crate) const fn id(self) -> BootstrapBoundId {
        let name = match self {
            Self::Nodes => "maxNodes",
            Self::Depth => "maxDepth",
            Self::CollectionEntries => "maxCollectionEntries",
            Self::TotalStringBytes => "maxTotalStringBytes",
            Self::SingleStringBytes => "maxSingleStringBytes",
            Self::RejectedMarkers => "maxRejectedMarkers",
            Self::EnvelopeCanonicalBytes => "maxEnvelopeCanonicalBytes",
            Self::SingleMarkerCanonicalBytes => "maxSingleMarkerCanonicalBytes",
            Self::TraversalUnits => "maxTraversalUnits",
        };
        BootstrapBoundId {
            group: BootstrapGroup::Snapshot,
            name,
        }
    }

    /// The bound's capability path, such as `bootstrapOnly.snapshot.maxNodes`,
    /// which a bootstrap marker's record carries (D15:889).
    pub(crate) fn path(self) -> String {
        let id = self.id();
        format!("bootstrapOnly.{}.{}", id.group.path(), id.name)
    }

    /// The bound whose capability path is `path`, if any.
    pub(crate) fn from_path(path: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|bound| bound.path() == path)
    }

    /// The bound's value in `limits`.
    pub(crate) const fn limit(self, limits: &SnapshotLimits) -> Bound {
        match self {
            Self::Nodes => limits.max_nodes,
            Self::Depth => limits.max_depth,
            Self::CollectionEntries => limits.max_collection_entries,
            Self::TotalStringBytes => limits.max_total_string_bytes,
            Self::SingleStringBytes => limits.max_single_string_bytes,
            Self::RejectedMarkers => limits.max_rejected_markers,
            Self::EnvelopeCanonicalBytes => limits.max_envelope_canonical_bytes,
            Self::SingleMarkerCanonicalBytes => limits.max_single_marker_canonical_bytes,
            Self::TraversalUnits => limits.max_traversal_units,
        }
    }
}

impl Marker {
    /// The marker record: field `0x01` the reason ID, and for a bootstrap
    /// overrun `0x02` the bound's path and `0x03` the witness as `U64`
    /// (D15:889).
    pub(crate) fn record(self) -> Vec<u8> {
        match self {
            Self::Rejected(reason) => Record::new().field(reason.id().as_bytes()).finish(),
            Self::BootstrapLimitExceeded { bound, witness } => Record::new()
                .field(BOOTSTRAP_LIMIT_EXCEEDED.as_bytes())
                .field(bound.path().as_bytes())
                .field(&witness.to_be_bytes())
                .finish(),
        }
    }
}

/// A frame's tag and its `U64` payload length (D15:873).
const HEADER: usize = 1 + 8;

/// `U64(count)` of a container's elements or members.
fn count(length: usize) -> [u8; 8] {
    u64::try_from(length)
        .expect("a collection length fits in u64 on every supported target")
        .to_be_bytes()
}

/// What framing a tree has left to write, last first.
enum Step<'a> {
    Node(&'a Node),
    Key(&'a str),
    /// Fill in the payload length of the container whose frame starts here.
    Close(usize),
}

/// Open a container's frame in `output`: its tag, a payload length to fill
/// in when it closes, and its count. Returns where the frame starts.
fn open(tag: u8, entries: usize, output: &mut Vec<u8>) -> usize {
    let start = output.len();
    output.push(tag);
    output.extend_from_slice(&[0; 8]);
    output.extend_from_slice(&count(entries));
    start
}

impl Node {
    /// `S` of this node appended to `output` (D15:875-889), without
    /// recursion: a container's frame opens with its payload length left to
    /// fill in, its members follow in place, and the length is written when
    /// it closes, so neither a deep tree nor a long payload is copied once
    /// per level (plan 025 decision #298).
    fn frame_into(&self, output: &mut Vec<u8>) {
        let mut steps = vec![Step::Node(self)];
        while let Some(step) = steps.pop() {
            match step {
                Step::Close(start) => {
                    let length = wide(output.len() - start - HEADER);
                    output[start + 1..start + HEADER].copy_from_slice(&length.to_be_bytes());
                }
                Step::Key(key) => frame(STRING, key.as_bytes(), output),
                Step::Node(Self::Null) => frame(NULL, &[], output),
                Step::Node(Self::Boolean(false)) => frame(FALSE, &[], output),
                Step::Node(Self::Boolean(true)) => frame(TRUE, &[], output),
                Step::Node(Self::String(text)) => frame(STRING, text.as_bytes(), output),
                Step::Node(Self::Number(number)) => {
                    frame(NUMBER, &number.get().to_bits().to_be_bytes(), output);
                }
                Step::Node(Self::Array(elements)) => {
                    steps.push(Step::Close(open(ARRAY, elements.len(), output)));
                    steps.extend(elements.iter().rev().map(Step::Node));
                }
                Step::Node(Self::Object(members)) => {
                    steps.push(Step::Close(open(OBJECT, members.len(), output)));
                    for (key, value) in members.iter().rev() {
                        steps.push(Step::Node(value));
                        steps.push(Step::Key(key));
                    }
                }
                Step::Node(Self::Marker(marker)) => frame(MARKER, &marker.record(), output),
            }
        }
    }

    /// Move this node's members into `pending`, leaving it with none.
    fn release(&mut self, pending: &mut Vec<Self>) {
        match self {
            Self::Array(elements) => pending.append(elements),
            Self::Object(members) => pending.extend(std::mem::take(members).into_values()),
            Self::Null | Self::Boolean(_) | Self::Number(_) | Self::String(_) | Self::Marker(_) => {
            }
        }
    }

    /// `S(self)`, the canonical bytes two equal Snapshots share (D15:889).
    pub(crate) fn framed(&self) -> Vec<u8> {
        let mut output = Vec::new();
        self.frame_into(&mut output);
        output
    }

    /// The snapshot-content digest, which is also the Programmatic Entry
    /// revision: SHA-256 over `intlify.programmatic-entry-snapshot.v0`
    /// followed by `S(root)`, as `sha256:` and 64 lowercase hexadecimal
    /// digits (D15:889).
    pub(crate) fn digest(&self) -> String {
        let mut preimage = SNAPSHOT_DOMAIN.as_bytes().to_vec();
        self.frame_into(&mut preimage);
        format!("sha256:{}", hex(digest_bytes(&preimage)))
    }
}

/// A tree is released without recursion, so dropping a deep one cannot
/// exhaust the stack (plan 025 decision #298).
impl Drop for Node {
    fn drop(&mut self) {
        let mut pending = Vec::new();
        self.release(&mut pending);
        while let Some(mut node) = pending.pop() {
            node.release(&mut pending);
        }
    }
}

/// What a canonical tree counts under the nine `bootstrapOnly.snapshot`
/// bounds, each in its unit (D15:1796-1804): every node, a marker included;
/// tree levels with the root at one; object members and array elements;
/// the UTF-8 bytes of every string and key, and the longest one; markers;
/// the bytes of `S(root)` and of the longest marker frame; and the nodes and
/// edges a traversal visits, an edge being one member or element.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SnapshotCounts {
    pub(crate) nodes: u64,
    pub(crate) depth: u64,
    pub(crate) collection_entries: u64,
    pub(crate) total_string_bytes: u64,
    pub(crate) single_string_bytes: u64,
    pub(crate) rejected_markers: u64,
    pub(crate) envelope_canonical_bytes: u64,
    pub(crate) single_marker_canonical_bytes: u64,
    pub(crate) traversal_units: u64,
}

fn wide(value: usize) -> u64 {
    u64::try_from(value).expect("a length fits in u64 on every supported target")
}

impl SnapshotCounts {
    /// The counts of the tree rooted at `root`, without recursion and without
    /// framing it: `S(root)` is as long as every node's frame header and own
    /// payload together, an object's key frames included (plan 025 decision
    /// #298).
    pub(crate) fn of(root: &Node) -> Self {
        let mut counts = Self::default();
        let mut pending = vec![(root, 1)];
        while let Some((node, depth)) = pending.pop() {
            counts.nodes += 1;
            counts.depth = counts.depth.max(depth);
            let payload = match node {
                Node::Null | Node::Boolean(_) => 0,
                Node::Number(_) => 8,
                Node::String(text) => counts.string(text),
                Node::Array(elements) => {
                    counts.collection_entries += wide(elements.len());
                    pending.extend(elements.iter().map(|element| (element, depth + 1)));
                    8
                }
                Node::Object(members) => {
                    counts.collection_entries += wide(members.len());
                    let mut keys = 0;
                    for (key, value) in members {
                        keys += wide(HEADER) + counts.string(key);
                        pending.push((value, depth + 1));
                    }
                    8 + keys
                }
                Node::Marker(marker) => {
                    counts.rejected_markers += 1;
                    let record = wide(marker.record().len());
                    counts.single_marker_canonical_bytes = counts
                        .single_marker_canonical_bytes
                        .max(wide(HEADER) + record);
                    record
                }
            };
            counts.envelope_canonical_bytes += wide(HEADER) + payload;
        }
        counts.traversal_units = counts.nodes + counts.collection_entries;
        counts
    }

    /// Count a string or key's UTF-8 bytes and return their number.
    fn string(&mut self, text: &str) -> u64 {
        let bytes = wide(text.len());
        self.total_string_bytes += bytes;
        self.single_string_bytes = self.single_string_bytes.max(bytes);
        bytes
    }

    /// Each count beside the bound that limits it, in the table's order.
    pub(crate) const fn by_bound(&self) -> [(SnapshotBound, u64); 9] {
        [
            (SnapshotBound::Nodes, self.nodes),
            (SnapshotBound::Depth, self.depth),
            (SnapshotBound::CollectionEntries, self.collection_entries),
            (SnapshotBound::TotalStringBytes, self.total_string_bytes),
            (SnapshotBound::SingleStringBytes, self.single_string_bytes),
            (SnapshotBound::RejectedMarkers, self.rejected_markers),
            (
                SnapshotBound::EnvelopeCanonicalBytes,
                self.envelope_canonical_bytes,
            ),
            (
                SnapshotBound::SingleMarkerCanonicalBytes,
                self.single_marker_canonical_bytes,
            ),
            (SnapshotBound::TraversalUnits, self.traversal_units),
        ]
    }
}

/// A canonical tree built from a JSON value, for tests: every number must be
/// portable.
#[cfg(test)]
pub(crate) fn from_json(value: &serde_json::Value) -> Node {
    use serde_json::Value;
    match value {
        Value::Null => Node::Null,
        Value::Bool(value) => Node::Boolean(*value),
        Value::Number(number) => Node::Number(
            number
                .as_f64()
                .and_then(PortableNumber::new)
                .expect("a portable number"),
        ),
        Value::String(text) => Node::String(text.clone()),
        Value::Array(elements) => Node::Array(elements.iter().map(from_json).collect()),
        Value::Object(members) => Node::Object(
            members
                .iter()
                .map(|(key, value)| (key.clone(), from_json(value)))
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use super::*;
    use crate::materialize::materialize_file;
    use crate::materialize_tests::limits;

    fn bytes(hex: &str) -> Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap())
            .collect()
    }

    fn number(value: f64) -> Node {
        Node::Number(PortableNumber::new(value).unwrap())
    }

    /// A tree with every variant: an object holding an array of a null, a
    /// true, 1.5, and `"x"`, and a cycle marker.
    fn tree() -> Node {
        let mut members = from_json(&json!({ "a": [null, true, 1.5, "x"] }));
        let Node::Object(map) = &mut members else {
            unreachable!()
        };
        map.insert(
            "b".to_owned(),
            Node::Marker(Marker::Rejected(MarkerReason::Cycle)),
        );
        members
    }

    /// `S` of the tree and its digest, computed outside this crate with
    /// Python's `struct` and `hashlib` from 015's framing.
    const TREE: &str = "06000000000000007100000000000000020400000000000000016105000000000000003500000000000000040100000000000000000300000000000000000700000000000000083ff8000000000000040000000000000001780400000000000000016208000000000000000e0100000000000000056379636c65";
    const DIGEST: &str = "sha256:6a85cb2bd154bea87ba2c6e0d488e10e9c61bb5849f46bd58bc4e710001e6281";

    /// Each variant frames with its tag, an object its members in key order
    /// after their count, and a marker its record, and the digest is
    /// SHA-256 over the domain separator and `S(root)` (D15:875-889).
    #[test]
    fn the_tree_frames_and_digests_as_015_defines() {
        assert_eq!(tree().framed(), bytes(TREE));
        assert_eq!(tree().digest(), DIGEST);
    }

    /// The scalars frame with their tags over an empty payload, a string
    /// over its UTF-8 bytes, and a number over its eight big-endian binary64
    /// bytes, negative zero as positive zero.
    #[test]
    fn each_scalar_frames_with_its_tag() {
        let frame = |tag: u8, payload: &[u8]| {
            let mut output = vec![tag];
            output.extend_from_slice(&u64::try_from(payload.len()).unwrap().to_be_bytes());
            output.extend_from_slice(payload);
            output
        };
        assert_eq!(Node::Null.framed(), frame(0x01, &[]));
        assert_eq!(Node::Boolean(false).framed(), frame(0x02, &[]));
        assert_eq!(Node::Boolean(true).framed(), frame(0x03, &[]));
        assert_eq!(
            Node::String("é".to_owned()).framed(),
            frame(0x04, "é".as_bytes())
        );
        assert_eq!(
            number(-2.5).framed(),
            frame(0x07, &(-2.5_f64).to_bits().to_be_bytes())
        );
        assert_eq!(number(-0.0).framed(), number(0.0).framed());
        assert_eq!(number(-0.0).framed(), frame(0x07, &[0; 8]));
    }

    /// Object members frame in the unsigned UTF-8 order of their keys,
    /// whatever order they were inserted in: `é` (0xc3 0xa9) after `z`.
    #[test]
    fn object_members_frame_in_unsigned_utf8_key_order() {
        let object = from_json(&json!({ "é": 1, "z": 2, "a": 3 }));
        let Node::Object(members) = &object else {
            unreachable!()
        };
        assert_eq!(
            members.keys().map(String::as_str).collect::<Vec<_>>(),
            ["a", "z", "é"]
        );
        let mut expected = 3_u64.to_be_bytes().to_vec();
        for (key, value) in [("a", 3.0), ("z", 2.0), ("é", 1.0)] {
            frame(0x04, key.as_bytes(), &mut expected);
            expected.extend(number(value).framed());
        }
        let mut framed = Vec::new();
        frame(0x06, &expected, &mut framed);
        assert_eq!(object.framed(), framed);
    }

    /// An ordinary marker's record has only its reason; a bootstrap
    /// marker's has the reason, the bound's capability path, and the witness
    /// as `U64`, each reason being one of the Profile Specification's.
    #[test]
    fn a_marker_record_holds_its_reason_and_any_bound_and_witness() {
        assert_eq!(
            Node::Marker(Marker::Rejected(MarkerReason::Cycle)).framed(),
            bytes("08000000000000000e0100000000000000056379636c65")
        );
        let overrun = Marker::BootstrapLimitExceeded {
            bound: SnapshotBound::Nodes,
            witness: 5,
        };
        assert_eq!(
            Node::Marker(overrun).framed(),
            bytes("08000000000000005a010000000000000018626f6f7473747261702d6c696d69742d657863656564656402000000000000001f626f6f7473747261704f6e6c792e736e617073686f742e6d61784e6f6465730300000000000000080000000000000005")
        );
        let ids: Vec<&str> = MarkerReason::ALL.iter().map(|reason| reason.id()).collect();
        assert_eq!(ids, MARKER_REASONS[..9]);
        assert_eq!(BOOTSTRAP_LIMIT_EXCEEDED, "bootstrap-limit-exceeded");
    }

    /// Each bound's path names the capability's `bootstrapOnly.snapshot`
    /// bound, in the capability's own order.
    #[test]
    fn each_bound_names_its_capability_path() {
        use crate::resolver::capability::BootstrapLimits;

        let snapshot: Vec<BootstrapBoundId> = BootstrapLimits::MINIMUM
            .bounds()
            .into_iter()
            .map(|(id, _)| id)
            .filter(|id| id.group == BootstrapGroup::Snapshot)
            .collect();
        assert_eq!(SnapshotBound::ALL.map(SnapshotBound::id).to_vec(), snapshot);
        assert_eq!(
            SnapshotBound::TraversalUnits.path(),
            "bootstrapOnly.snapshot.maxTraversalUnits"
        );
    }

    /// A reason reads back from its ID and a bound from its path, nothing
    /// else does, and each bound's limit is its own field of the limits.
    #[test]
    fn reasons_and_bounds_read_back_from_their_spellings() {
        use crate::resolver::capability::BootstrapLimits;

        for reason in MarkerReason::ALL {
            assert_eq!(MarkerReason::from_id(reason.id()), Some(reason));
        }
        for id in [BOOTSTRAP_LIMIT_EXCEEDED, "cycles", "Cycle", ""] {
            assert_eq!(MarkerReason::from_id(id), None, "{id}");
        }
        for bound in SnapshotBound::ALL {
            assert_eq!(SnapshotBound::from_path(&bound.path()), Some(bound));
        }
        for path in [
            "bootstrapOnly.rawInput.maxFileBytes",
            "bootstrapOnly.snapshot.maxnodes",
            "snapshot.maxNodes",
            "maxNodes",
        ] {
            assert_eq!(SnapshotBound::from_path(path), None, "{path}");
        }
        let mut limits = BootstrapLimits::MINIMUM;
        let distinct = |value| Bound::new(value).unwrap();
        limits.snapshot = SnapshotLimits {
            max_nodes: distinct(11),
            max_depth: distinct(12),
            max_collection_entries: distinct(13),
            max_total_string_bytes: distinct(14),
            max_single_string_bytes: distinct(15),
            max_rejected_markers: distinct(16),
            max_envelope_canonical_bytes: distinct(17),
            max_single_marker_canonical_bytes: distinct(18),
            max_traversal_units: distinct(19),
        };
        let snapshot: Vec<(BootstrapBoundId, Bound)> = limits
            .bounds()
            .into_iter()
            .filter(|(id, _)| id.group == BootstrapGroup::Snapshot)
            .collect();
        assert_eq!(
            SnapshotBound::ALL
                .map(|bound| (bound.id(), bound.limit(&limits.snapshot)))
                .to_vec(),
            snapshot
        );
    }

    /// The tree counts every node and marker, its levels from the root at
    /// one, its members and elements, its string and key bytes, its
    /// markers, its frame and longest marker frame, and its nodes and edges.
    #[test]
    fn the_counts_follow_each_bound_unit() {
        let counts = SnapshotCounts::of(&tree());
        assert_eq!(
            counts,
            SnapshotCounts {
                nodes: 7,
                depth: 3,
                collection_entries: 6,
                total_string_bytes: 3,
                single_string_bytes: 1,
                rejected_markers: 1,
                envelope_canonical_bytes: 122,
                single_marker_canonical_bytes: 23,
                traversal_units: 13,
            }
        );
        assert_eq!(counts.envelope_canonical_bytes, wide(bytes(TREE).len()));
        assert_eq!(
            counts.by_bound().map(|(bound, _)| bound),
            SnapshotBound::ALL
        );
        let overrun = Node::Marker(Marker::BootstrapLimitExceeded {
            bound: SnapshotBound::SingleMarkerCanonicalBytes,
            witness: u64::MAX - 1,
        });
        // The longest marker frame: the longest path, `maxSingleMarkerCanonicalBytes`.
        assert_eq!(
            SnapshotCounts::of(&overrun).single_marker_canonical_bytes,
            120
        );
    }

    /// A tree far deeper than a stack holds frames, digests, counts, and drops
    /// without recursion. Each level is written once, a frame header and a
    /// count, so `S` grows by 17 bytes a level (plan 025 decision #298).
    #[test]
    fn a_deep_tree_frames_counts_and_drops_without_recursion() {
        const LEVELS: u64 = 100_000;
        let mut tree = Node::Null;
        for _ in 0..LEVELS {
            tree = Node::Array(vec![tree]);
        }
        let framed = tree.framed();
        assert_eq!(wide(framed.len()), 17 * LEVELS + 9);
        let mut head = vec![ARRAY];
        head.extend_from_slice(&(17 * LEVELS).to_be_bytes());
        head.extend_from_slice(&1_u64.to_be_bytes());
        assert_eq!(&framed[..17], head.as_slice());
        assert_eq!(&framed[framed.len() - 9..], &[NULL, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert!(tree.digest().starts_with("sha256:"));
        let counts = SnapshotCounts::of(&tree);
        assert_eq!(
            (
                counts.nodes,
                counts.depth,
                counts.collection_entries,
                counts.envelope_canonical_bytes
            ),
            (LEVELS + 1, LEVELS + 1, LEVELS, wide(framed.len()))
        );
        let mut members = Node::Null;
        for _ in 0..LEVELS {
            members = Node::Object(BTreeMap::from([("a".to_owned(), members)]));
        }
        // Each level adds a frame header, a count, and the key's frame.
        assert_eq!(wide(members.framed().len()), 27 * LEVELS + 9);
    }

    /// Without a marker, the five counts the root-value bounds share are
    /// those the file entry counts for the same JSON value (D15:1889).
    #[test]
    fn a_marker_free_tree_counts_as_the_same_json_value() {
        let value = json!({
            "schemaVersion": "0",
            "locales": ["ja", "en", "日本"],
            "nested": { "deep": [[1, 2.5], { "k": null }], "flag": false },
        });
        let source: Arc<[u8]> = serde_json::to_vec(&value).unwrap().into();
        let document = materialize_file(source, limits()).ok().unwrap();
        let file = document.value_counts();
        let snapshot = SnapshotCounts::of(&from_json(&value));
        assert_eq!(
            (
                snapshot.nodes,
                snapshot.depth,
                snapshot.collection_entries,
                snapshot.total_string_bytes,
                snapshot.single_string_bytes,
            ),
            (
                file.nodes,
                file.depth,
                file.collection_entries,
                file.total_string_bytes,
                file.single_string_bytes,
            )
        );
        assert_eq!(snapshot.rejected_markers, 0);
        assert_eq!(snapshot.traversal_units, 2 * snapshot.nodes - 1);
    }
}
