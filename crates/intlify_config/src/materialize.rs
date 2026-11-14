// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Strict file entry for the private 015 path, independent of CLI compatibility.
//!
//! Parsing builds an invocation-owned flat token index under raw byte/token
//! limits, decoding only member names needed for duplicate detection. It stops
//! only at the token bound or the first byte that is not UTF-8 or not strict
//! JSON; past a duplicate member or a value outside the JSON-compatible domain
//! it continues, and reports every one in the complete file with its internal
//! structural path (D15:802, Decision 015-150). Complete logical counts are
//! checked before constructing any materialized value nodes. Rejected input never yields an authoritative
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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

/// Why strict file entry produced no value. Each failure holds spans and
/// counts only, never a member name or a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InputFailure {
    /// The file is longer than `maxFileBytes`, exactly: nothing was parsed.
    FileBytes(LimitViolation),
    /// Tokenization reached `maxParserTokens`: `span` starts the first token
    /// past it, the first-over witness.
    ParserTokens {
        violation: LimitViolation,
        span: ByteSpan,
    },
    /// The first bytes that are not UTF-8: nothing after them was parsed.
    InvalidUtf8 { span: ByteSpan },
    /// The first byte or token that is not strict JSON, or the end of an
    /// incomplete input: nothing after it was parsed.
    Syntax { span: ByteSpan },
    /// A count past `u64`, which the file's length bounds, so never reached.
    AccountingOverflow { span: ByteSpan },
    /// The complete file parsed, but its value is not admissible.
    Inadmissible(Box<Inadmissible>),
}

/// What keeps a completely parsed file from being materialized, each in source
/// order: every duplicate member, every value outside the JSON-compatible
/// domain, and every root-value bound the complete value exceeds, with the
/// internal structural paths the facts were found at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Inadmissible {
    pub(crate) duplicates: Vec<DuplicateMember>,
    pub(crate) incompatible: Vec<IncompatibleValue>,
    /// The root-value bounds exceeded, in 015's order: a total exactly, and
    /// the single-string bound as its first-over witness.
    pub(crate) violations: Vec<LimitViolation>,
    /// Each string value and member name longer than `maxSingleStringBytes`,
    /// which 015 checks one occurrence at a time.
    pub(crate) long_strings: Vec<LongString>,
    pub(crate) paths: Paths,
}

/// A member of an object after the first member with its name: its key, the
/// keys of every other member with the name, the first included (Decision
/// 015-194), and its own path, which ends at its key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DuplicateMember {
    pub(crate) key: ByteSpan,
    pub(crate) others: Vec<ByteSpan>,
    pub(crate) path: PathId,
}

/// A string value or member name outside the Unicode scalar domain, or a
/// number outside the Portable JSON Number domain, at its token and at the
/// path of the value or member (D15:2289).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IncompatibleValue {
    pub(crate) reason: Incompatibility,
    pub(crate) span: ByteSpan,
    pub(crate) path: Option<PathId>,
}

/// A string value or member name longer than `maxSingleStringBytes`, at its
/// token and at the path of the value or member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LongString {
    pub(crate) span: ByteSpan,
    pub(crate) path: Option<PathId>,
}

/// One step of an internal structural path from the root: a member, by the
/// span of its key, or an array element, by its index from 0. A step holds no
/// member name, so no path exposes one (D15:776).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PathStep {
    Member(ByteSpan),
    Index(u64),
}

/// A member or array element of a completely parsed file, in its [`Paths`].
/// The root, which no step reaches, is the absence of one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PathId(usize);

/// The internal structural paths a completely parsed file's facts were found
/// at (D15:802): each member and array element on one of them, after the one
/// that contains it, so facts share their common prefixes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Paths(Vec<(Option<PathId>, PathStep)>);

impl Paths {
    /// The steps from the root to `place`, none for the root.
    pub(crate) fn steps(&self, place: Option<PathId>) -> Vec<PathStep> {
        let mut steps = Vec::new();
        let mut next = place;
        while let Some(PathId(index)) = next {
            let (parent, step) = self.0[index];
            steps.push(step);
            next = parent;
        }
        steps.reverse();
        steps
    }

    /// How many members and elements the paths hold.
    #[cfg(test)]
    pub(crate) fn entered(&self) -> usize {
        self.0.len()
    }

    /// Enters `step` after `parent`.
    fn push(&mut self, parent: Option<PathId>, step: PathStep) -> PathId {
        self.0.push((parent, step));
        PathId(self.0.len() - 1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Incompatibility {
    InvalidUnicode,
    NonPortableNumber,
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
    pub(crate) progress: EntryProgress,
}

pub(crate) fn materialize_file(
    source: Arc<[u8]>,
    limits: InputLimits,
) -> Result<MaterializedDocument, MaterializationError> {
    let file_bytes = byte_count(source.len());
    let progress = |parser_tokens_visited, complete_value| EntryProgress {
        file_bytes,
        parser_tokens_visited,
        complete_value,
    };
    if file_bytes > limits.raw.max_file_bytes.get() {
        return Err(MaterializationError {
            reason: InputFailure::FileBytes(LimitViolation {
                bound: InputBound::FileBytes,
                limit: limits.raw.max_file_bytes,
                actual: file_bytes,
                relation: CountRelation::Exact,
            }),
            progress: progress(0, None),
        });
    }
    let parsed = parse::parse(&source, limits.raw, limits.value.max_single_string_bytes).map_err(
        |stopped| MaterializationError {
            reason: stopped.failure,
            progress: progress(stopped.tokens, None),
        },
    )?;
    let violations = limits.value.violations(parsed.counts.value);
    if !(parsed.duplicates.is_empty() && parsed.incompatible.is_empty() && violations.is_empty()) {
        return Err(MaterializationError {
            reason: InputFailure::Inadmissible(Box::new(Inadmissible {
                duplicates: parsed.duplicates,
                incompatible: parsed.incompatible,
                violations,
                long_strings: parsed.long_strings,
                paths: parsed.paths,
            })),
            progress: progress(
                parsed.counts.parser_tokens,
                Some(Box::new(parsed.counts.value)),
            ),
        });
    }
    let text =
        std::str::from_utf8(&source).expect("strict tokenization validated every byte as UTF-8");
    let nodes = build_nodes(text, parsed.tokens, parsed.counts.value.nodes);
    Ok(MaterializedDocument {
        source,
        nodes,
        counts: parsed.counts,
    })
}

/// The name of the member whose key is at `key` in the parsed `source`, for
/// classification only: a lone surrogate becomes U+FFFD, so a name outside
/// the Unicode scalar domain is never schema-defined or a valid identifier,
/// and its member stays unsafe (D15:776).
pub(crate) fn member_name(source: &[u8], key: ByteSpan) -> String {
    String::from_utf16_lossy(&parse::name_units(source, key))
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
    /// `None` outside the Portable JSON Number domain.
    Number(Option<PortableNumber>),
    /// Raw string token, with its decoded byte length and whether its value
    /// stays in the Unicode scalar domain, but no owned string value.
    String {
        decoded: u64,
        scalar: bool,
    },
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
        // The lexer already validated this bounded token's syntax, and an
        // admitted value has only Unicode scalars; serde is used only to
        // decode strings, never numbers/maps.
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
            TokenKind::String { .. } if stack.last().is_some_and(|frame| frame.expects_key) => {
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
            TokenKind::Number(value) => {
                NodeKind::Number(value.expect("an admitted value has only portable numbers"))
            }
            TokenKind::String { .. } => NodeKind::String(decode_string(source, token.span)),
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
