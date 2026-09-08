// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Benchmark-owned observation codec, revision 0. These hashes are not 017
//! artifact digests, Profile identities, signatures, or evidence-disclosure tokens.
//! Only feature-isolated owner observations and bindings use this module; it is
//! not a shared artifact or file export API.

use std::collections::BTreeMap;
use std::fmt;

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::input_limits::{CountRelation, InputBound, ValueCounts};
use crate::materialize::{
    ByteSpan, InputFailure, MaterializationError, MaterializedDocument, NodeId, NodeKind,
    ObjectMember,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Digest([u8; 32]);

impl schemars::JsonSchema for Digest {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "NativeObservationChecksum".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({"type": "string", "pattern": "^[0-9a-f]{64}$"})
    }
}

impl Serialize for Digest {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&blake3::Hash::from_bytes(self.0).to_hex())
    }
}

impl<'de> Deserialize<'de> for Digest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl de::Visitor<'_> for Visitor {
            type Value = Digest;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a lowercase 64-digit benchmark checksum")
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Digest, E> {
                fn digit(byte: u8) -> Option<u8> {
                    match byte {
                        b'0'..=b'9' => Some(byte - b'0'),
                        b'a'..=b'f' => Some(byte - b'a' + 10),
                        _ => None,
                    }
                }
                let bytes = value.as_bytes();
                if bytes.len() != 64 {
                    return Err(E::custom("invalid benchmark checksum"));
                }
                let mut result = [0; 32];
                for (index, [high, low]) in bytes.as_chunks::<2>().0.iter().enumerate() {
                    result[index] = digit(*high)
                        .zip(digit(*low))
                        .map(|(h, l)| h * 16 + l)
                        .ok_or_else(|| E::custom("invalid benchmark checksum"))?;
                }
                Ok(Digest(result))
            }
        }
        deserializer.deserialize_str(Visitor)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Observation {
    pub(crate) shared: Digest,
    #[serde(deserialize_with = "Option::deserialize")]
    pub(crate) entry: Option<Digest>,
}

impl Observation {
    pub(crate) fn identity(self) -> Digest {
        let mut frame = Frame::new("observation");
        frame.digest(self.shared);
        frame.flag(self.entry.is_some());
        if let Some(entry) = self.entry {
            frame.digest(entry);
        }
        frame.finish()
    }
}

pub(crate) struct Frame(blake3::Hasher);

impl Frame {
    pub(crate) fn new(domain: &str) -> Self {
        let mut frame = Self(blake3::Hasher::new());
        frame.bytes(b"intlify-config-minimum-observation/0");
        frame.text(domain);
        frame
    }
    pub(crate) fn uint(&mut self, value: u64) {
        self.0.update(&value.to_le_bytes());
    }
    pub(crate) fn flag(&mut self, value: bool) {
        self.0.update(&[u8::from(value)]);
    }
    pub(crate) fn bytes(&mut self, bytes: &[u8]) {
        self.uint(u64::try_from(bytes.len()).expect("addressable fixture length"));
        self.0.update(bytes);
    }
    pub(crate) fn text(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }
    pub(crate) fn digest(&mut self, value: Digest) {
        self.0.update(&value.0);
    }
    pub(crate) fn finish(self) -> Digest {
        Digest(*self.0.finalize().as_bytes())
    }
    pub(crate) fn span(&mut self, span: ByteSpan) {
        self.uint(span.start_byte());
        self.uint(span.end_byte());
    }
    pub(crate) fn optional_span(&mut self, span: Option<ByteSpan>) {
        self.flag(span.is_some());
        if let Some(span) = span {
            self.span(span);
        }
    }
    pub(crate) fn value_counts(&mut self, counts: ValueCounts) {
        self.uint(counts.nodes);
        self.uint(counts.depth);
        self.uint(counts.collection_entries);
        self.uint(counts.total_string_bytes);
        self.uint(counts.single_string_bytes);
    }

    /// Generated schema and admitted fixed-depth authoring values only. Arbitrary
    /// deep materialized input uses the iterative document traversal below.
    pub(crate) fn json(&mut self, value: &Value) {
        match value {
            Value::Null => self.uint(0),
            Value::Bool(value) => {
                self.uint(1);
                self.flag(*value);
            }
            Value::Number(value) => {
                self.uint(2);
                self.text(&value.to_string());
            }
            Value::String(value) => {
                self.uint(3);
                self.text(value);
            }
            Value::Array(values) => {
                self.uint(4);
                self.uint(u64::try_from(values.len()).expect("bounded fixture array"));
                for value in values {
                    self.json(value);
                }
            }
            Value::Object(values) => {
                self.uint(5);
                self.uint(u64::try_from(values.len()).expect("bounded fixture map"));
                for (key, value) in values.iter().collect::<BTreeMap<_, _>>() {
                    self.text(key);
                    self.json(value);
                }
            }
        }
    }
}

/// Complete value preorder, independent of source member order and physical IDs.
pub(crate) fn document_order(doc: &MaterializedDocument) -> BTreeMap<NodeId, u64> {
    let mut order = BTreeMap::new();
    let mut pending = vec![doc.root()];
    while let Some(id) = pending.pop() {
        order.insert(id, u64::try_from(order.len()).expect("bounded node count"));
        match doc.node(id).kind() {
            NodeKind::Array(items) => pending.extend(items.iter().rev().copied()),
            NodeKind::Object(items) => {
                pending.extend(items.values().rev().map(ObjectMember::value));
            }
            _ => {}
        }
    }
    order
}

pub(crate) fn document(doc: &MaterializedDocument) -> Observation {
    let mut shared = Frame::new("materialized-value");
    let mut entry = Frame::new("materialized-source-map");
    let mut pending = vec![doc.root()];
    while let Some(id) = pending.pop() {
        let node = doc.node(id);
        entry.span(node.span());
        match node.kind() {
            NodeKind::Null => shared.uint(0),
            NodeKind::Boolean(value) => {
                shared.uint(1);
                shared.flag(*value);
            }
            NodeKind::Number(value) => {
                shared.uint(2);
                shared.uint(value.get().to_bits());
            }
            NodeKind::String(value) => {
                shared.uint(3);
                shared.text(value);
            }
            NodeKind::Array(items) => {
                shared.uint(4);
                shared.uint(u64::try_from(items.len()).expect("bounded array"));
                pending.extend(items.iter().rev().copied());
            }
            NodeKind::Object(items) => {
                shared.uint(5);
                shared.uint(u64::try_from(items.len()).expect("bounded map"));
                for (name, member) in items {
                    shared.text(name);
                    entry.span(member.key_span());
                }
                pending.extend(items.values().rev().map(ObjectMember::value));
            }
        }
    }
    let counts = doc.counts();
    shared.value_counts(counts.value);
    entry.uint(counts.file_bytes);
    entry.uint(counts.parser_tokens);
    // The successful materializer retains unchanged source. Fixture-only entry
    // observation covers that output too, separately from normalized value identity.
    entry.bytes(doc.raw_span(ByteSpan::new(0, counts.file_bytes)));
    Observation {
        shared: shared.finish(),
        entry: Some(entry.finish()),
    }
}

pub(crate) fn materialization_failure(error: &MaterializationError) -> Observation {
    let mut shared = Frame::new("materialization-failure");
    match &error.reason {
        InputFailure::InvalidUtf8 => shared.uint(0),
        InputFailure::Syntax => shared.uint(1),
        InputFailure::DuplicateMember => shared.uint(2),
        InputFailure::NonScalarString => shared.uint(3),
        InputFailure::NonPortableNumber => shared.uint(4),
        InputFailure::AccountingOverflow => shared.uint(5),
        InputFailure::ResourceLimits(violations) => {
            shared.uint(6);
            shared.uint(u64::try_from(violations.len()).expect("finite bounds"));
            for violation in violations {
                shared.uint(match violation.bound {
                    InputBound::FileBytes => 0,
                    InputBound::ParserTokens => 1,
                    InputBound::Nodes => 2,
                    InputBound::Depth => 3,
                    InputBound::CollectionEntries => 4,
                    InputBound::TotalStringBytes => 5,
                    InputBound::SingleStringBytes => 6,
                });
                shared.uint(violation.limit.get());
                shared.uint(violation.actual);
                shared.uint(match violation.relation {
                    CountRelation::Exact => 0,
                    CountRelation::AtLeast => 1,
                });
            }
        }
    }
    let mut entry = Frame::new("materialization-failure-entry");
    entry.optional_span(error.span);
    entry.optional_span(error.related_span);
    entry.uint(error.progress.file_bytes);
    entry.uint(error.progress.parser_tokens_visited);
    entry.flag(error.progress.complete_value.is_some());
    if let Some(counts) = &error.progress.complete_value {
        entry.value_counts(**counts);
    }
    Observation {
        shared: shared.finish(),
        entry: Some(entry.finish()),
    }
}
