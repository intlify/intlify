// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Strict file entry for the private 015 path, independent of CLI compatibility.
//!
//! Parsing builds an invocation-owned flat token index under raw byte/token
//! limits, decoding only member names needed for duplicate detection. Complete
//! logical counts are checked before constructing any materialized value nodes. Rejected input never yields an authoritative
//! prefix. The index and parser stack use ordinary safe collections; nested
//! values contain indices, so successful and failed cleanup do not recurse.
//!
//! Raw source is immutable shared input. A successful document owns its decoded
//! values and source map, not borrowed scratch. No reusable workspace, global
//! cache, or capacity-retention policy is introduced by this implementation.
//! Spans and typed failures below are internal observations, not formal 015
//! Evidence, Source Identities, Finding Registry records, or Resolver Outcomes.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::input_limits::{CountRelation, InputBound, InputLimits, LimitViolation, ValueCounts};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ByteSpan {
    start: u64,
    end: u64,
}

impl ByteSpan {
    pub(crate) const fn new(start: u64, end: u64) -> Self {
        Self { start, end }
    }
    pub(crate) const fn start_byte(self) -> u64 {
        self.start
    }
    pub(crate) const fn end_byte(self) -> u64 {
        self.end
    }

    fn from_offsets(start: usize, end: usize) -> Self {
        Self::new(byte_count(start), byte_count(end))
    }
}

fn byte_count(length: usize) -> u64 {
    // Checked conversion of an existing slice coordinate, never u64 accounting
    // performed through usize or a host-sized maximum substituted for a bound.
    u64::try_from(length).expect("an addressable Rust slice length fits in u64")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct NodeId(usize);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PortableNumber(f64);

impl PortableNumber {
    fn from_token(token: &str) -> Option<Self> {
        let value: f64 = token.parse().ok()?;
        if !value.is_finite() || value.abs() > 9_007_199_254_740_991.0 {
            return None;
        }
        Some(Self(if value == 0.0 { 0.0 } else { value }))
    }

    pub(crate) const fn get(self) -> f64 {
        self.0
    }
}

// Deliberately no Debug/Serialize on raw configuration-bearing structures.
pub(crate) enum NodeKind {
    Null,
    Boolean(bool),
    Number(PortableNumber),
    String(String),
    Array(Vec<NodeId>),
    Object(BTreeMap<String, ObjectMember>),
}

pub(crate) struct ObjectMember {
    key_span: ByteSpan,
    value: NodeId,
}

impl ObjectMember {
    pub(crate) const fn key_span(&self) -> ByteSpan {
        self.key_span
    }
    pub(crate) const fn value(&self) -> NodeId {
        self.value
    }
}

pub(crate) struct LocatedNode {
    kind: NodeKind,
    span: ByteSpan,
}

impl LocatedNode {
    pub(crate) const fn kind(&self) -> &NodeKind {
        &self.kind
    }
    pub(crate) const fn span(&self) -> ByteSpan {
        self.span
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InputCounts {
    pub(crate) file_bytes: u64,
    pub(crate) parser_tokens: u64,
    pub(crate) value: ValueCounts,
}

pub(crate) struct MaterializedDocument {
    source: Arc<[u8]>,
    nodes: Vec<LocatedNode>,
    counts: InputCounts,
}

impl MaterializedDocument {
    pub(crate) fn root(&self) -> NodeId {
        debug_assert!(!self.nodes.is_empty());
        NodeId(0)
    }
    pub(crate) fn node(&self, id: NodeId) -> &LocatedNode {
        &self.nodes[id.0]
    }
    pub(crate) const fn counts(&self) -> InputCounts {
        self.counts
    }
    pub(crate) fn raw_span(&self, span: ByteSpan) -> &[u8] {
        let start = usize::try_from(span.start).expect("parser-proven source offset");
        let end = usize::try_from(span.end).expect("parser-proven source offset");
        &self.source[start..end]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InputFailure {
    InvalidUtf8,
    Syntax,
    DuplicateMember,
    NonScalarString,
    NonPortableNumber,
    ResourceLimits(Vec<LimitViolation>),
    AccountingOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EntryProgress {
    pub(crate) file_bytes: u64,
    /// Tokens actually visited, not an asserted total over an uninspected suffix.
    pub(crate) parser_tokens_visited: u64,
    /// Complete logical counts exist only when raw parsing finished successfully.
    pub(crate) complete_value: Option<Box<ValueCounts>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MaterializationError {
    pub(crate) reason: InputFailure,
    pub(crate) span: Option<ByteSpan>,
    pub(crate) related_span: Option<ByteSpan>,
    pub(crate) progress: EntryProgress,
}

pub(crate) fn materialize_file(
    source: Arc<[u8]>,
    limits: InputLimits,
) -> Result<MaterializedDocument, MaterializationError> {
    let file_bytes = byte_count(source.len());
    let before_parse = EntryProgress {
        file_bytes,
        parser_tokens_visited: 0,
        complete_value: None,
    };
    if file_bytes > limits.raw.max_file_bytes.get() {
        return Err(MaterializationError {
            reason: InputFailure::ResourceLimits(vec![LimitViolation {
                bound: InputBound::FileBytes,
                limit: limits.raw.max_file_bytes,
                actual: file_bytes,
                relation: CountRelation::Exact,
            }]),
            span: None,
            related_span: None,
            progress: before_parse,
        });
    }
    let text = std::str::from_utf8(&source).map_err(|error| MaterializationError {
        reason: InputFailure::InvalidUtf8,
        span: Some(ByteSpan::from_offsets(
            error.valid_up_to(),
            error
                .error_len()
                .map_or(source.len(), |length| error.valid_up_to() + length),
        )),
        related_span: None,
        progress: before_parse,
    })?;
    let parsed = parse::parse(text, limits.raw)?;
    let violations = limits.value.violations(parsed.counts.value);
    if !violations.is_empty() {
        return Err(MaterializationError {
            reason: InputFailure::ResourceLimits(violations),
            span: Some(ByteSpan::new(0, file_bytes)),
            related_span: None,
            progress: EntryProgress {
                file_bytes,
                parser_tokens_visited: parsed.counts.parser_tokens,
                complete_value: Some(Box::new(parsed.counts.value)),
            },
        });
    }
    let nodes = build_nodes(text, parsed.tokens, parsed.counts.value.nodes);
    Ok(MaterializedDocument {
        source,
        nodes,
        counts: parsed.counts,
    })
}

mod lex;
mod parse;
mod typed;

pub(crate) use typed::DecodeError;

impl MaterializedDocument {
    /// Internal bridge for structurally admitted authoring fragments. A caller
    /// must not interpret successful deserialization as complete root admission.
    pub(crate) fn decode<T: serde::de::DeserializeOwned>(
        &self,
        id: NodeId,
    ) -> Result<T, DecodeError> {
        typed::decode(self, id)
    }
}

struct ParsedIndex {
    tokens: Vec<Token>,
    counts: InputCounts,
}

#[derive(Clone, Copy)]
enum TokenKind {
    OpenArray,
    CloseArray,
    OpenObject,
    CloseObject,
    Colon,
    Comma,
    Null,
    Boolean(bool),
    Number(PortableNumber),
    // Raw string token, with its decoded byte length but no owned string value.
    String(u64),
}

#[derive(Clone, Copy)]
struct Token {
    kind: TokenKind,
    span: ByteSpan,
}

fn decode_string(source: &str, span: ByteSpan) -> String {
    let start = usize::try_from(span.start).expect("parser-proven source offset");
    let end = usize::try_from(span.end).expect("parser-proven source offset");
    let token = &source[start..end];
    if token.as_bytes().contains(&b'\\') {
        // The lexer already validated this bounded token's syntax and Unicode
        // scalar domain; serde is used only to decode strings, never numbers/maps.
        serde_json::from_str(token).expect("strictly validated string token")
    } else {
        token[1..token.len() - 1].to_owned()
    }
}

struct BuildFrame {
    owner: NodeId,
    pending_key: Option<(String, ByteSpan)>,
    expects_key: bool,
}

/// The immutable token stream has passed strict syntax, duplicate, scalar,
/// number, and complete logical-domain admission. This pass only constructs
/// owned values. It does not re-tokenize input or infer a new counting domain.
fn build_nodes(source: &str, tokens: Vec<Token>, node_count: u64) -> Vec<LocatedNode> {
    // Capacity comes from the admitted, measured domain, never a declared max.
    let capacity = usize::try_from(node_count).expect("parsed node count fits its token vector");
    let mut nodes: Vec<LocatedNode> = Vec::with_capacity(capacity);
    let mut stack: Vec<BuildFrame> = Vec::new();
    for token in tokens {
        match token.kind {
            TokenKind::CloseArray | TokenKind::CloseObject => {
                let frame = stack.pop().expect("validated container close");
                nodes[frame.owner.0].span.end = token.span.end;
                continue;
            }
            TokenKind::Colon => continue,
            TokenKind::Comma => {
                let frame = stack.last_mut().expect("validated collection comma");
                frame.expects_key = matches!(nodes[frame.owner.0].kind, NodeKind::Object(_));
                continue;
            }
            TokenKind::String(_) if stack.last().is_some_and(|frame| frame.expects_key) => {
                let frame = stack.last_mut().expect("validated member name");
                frame.pending_key = Some((decode_string(source, token.span), token.span));
                frame.expects_key = false;
                continue;
            }
            _ => {}
        }
        let kind = match token.kind {
            TokenKind::Null => NodeKind::Null,
            TokenKind::Boolean(value) => NodeKind::Boolean(value),
            TokenKind::Number(value) => NodeKind::Number(value),
            TokenKind::String(_) => NodeKind::String(decode_string(source, token.span)),
            TokenKind::OpenArray => NodeKind::Array(Vec::new()),
            TokenKind::OpenObject => NodeKind::Object(BTreeMap::new()),
            _ => unreachable!("validated value token"),
        };
        let container = match &kind {
            NodeKind::Array(_) => Some(false),
            NodeKind::Object(_) => Some(true),
            _ => None,
        };
        let id = NodeId(nodes.len());
        nodes.push(LocatedNode {
            kind,
            span: token.span,
        });
        if let Some(frame) = stack.last_mut() {
            match &mut nodes[frame.owner.0].kind {
                NodeKind::Array(items) => items.push(id),
                NodeKind::Object(members) => {
                    let (name, key_span) =
                        frame.pending_key.take().expect("validated member value");
                    members.insert(
                        name,
                        ObjectMember {
                            key_span,
                            value: id,
                        },
                    );
                }
                _ => unreachable!("validated container owner"),
            }
        }
        if let Some(expects_key) = container {
            stack.push(BuildFrame {
                owner: id,
                pending_key: None,
                expects_key,
            });
        }
    }
    debug_assert!(stack.is_empty());
    debug_assert_eq!(nodes.len(), capacity);
    nodes
}
