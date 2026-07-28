// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Shared bounded transport and canonical JSON-writing primitives.
//!
//! This module owns behavior that is identical for reference and definition
//! artifact codecs: one-document reader admission, transport error separation,
//! strict JSON document parsing, and all-or-nothing canonical output sinks.
//!
//! It does not select an artifact schema, validation precedence, wire counter,
//! semantic model, or nested member order. Typed codecs supply those choices.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::io::{self, Read};

use super::json::{JsonDocument, JsonMember, JsonNode, JsonParseError, NodeId};
use crate::{
    ArtifactContractError, ArtifactNamespace, ArtifactViolation, ArtifactViolationCode,
    ArtifactViolationLocation, LinkLimitCounter, LinkLimits, ValueConstructionError,
    ValueLimitKind,
};

const READER_BUFFER_BYTES: usize = 8 * 1024;

/// Failure while reading and admitting one serialized artifact.
#[derive(Debug)]
pub enum ArtifactReadError {
    /// The input transport failed before a bounded EOF was observed.
    Transport(io::Error),
    /// The complete bounded byte sequence violated the artifact contract.
    Contract(ArtifactContractError),
}

impl fmt::Display for ArtifactReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(error) => write!(formatter, "artifact transport failed: {error}"),
            Self::Contract(error) => error.fmt(formatter),
        }
    }
}

impl Error for ArtifactReadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Transport(error) => Some(error),
            Self::Contract(error) => Some(error),
        }
    }
}

pub(super) fn decode_from_reader<R, T>(
    reader: &mut R,
    limits: &LinkLimits,
    wire_counter: LinkLimitCounter,
    decode: impl FnOnce(&[u8], &LinkLimits) -> Result<T, ArtifactContractError>,
) -> Result<T, ArtifactReadError>
where
    R: Read,
{
    let wire_limit = effective_wire_limit_usize(limits, wire_counter);
    let mut input = Vec::with_capacity(wire_limit.min(READER_BUFFER_BYTES));
    let mut buffer = [0_u8; READER_BUFFER_BYTES];

    loop {
        // Never request bytes after the canonical first-over observation.
        // Syntax/contract failures remain provisional until bounded EOF so a
        // real transport failure retains the precedence fixed by the contract.
        let remaining_through_first_over = wire_limit.saturating_add(1).saturating_sub(input.len());
        let read_capacity = remaining_through_first_over.min(buffer.len());
        debug_assert!(read_capacity > 0);

        match reader.read(&mut buffer[..read_capacity]) {
            Ok(0) => return decode(&input, limits).map_err(ArtifactReadError::Contract),
            Ok(read) => {
                if input.len() + read > wire_limit {
                    return Err(ArtifactReadError::Contract(wire_limit_error(
                        limits,
                        wire_counter,
                    )));
                }
                input.extend_from_slice(&buffer[..read]);
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(ArtifactReadError::Transport(error)),
        }
    }
}

pub(super) fn admit_known_wire_length(
    length: usize,
    limits: &LinkLimits,
    wire_counter: LinkLimitCounter,
) -> Result<(), ArtifactContractError> {
    if length > effective_wire_limit_usize(limits, wire_counter) {
        return Err(wire_limit_error(limits, wire_counter));
    }
    Ok(())
}

fn effective_wire_limit_usize(limits: &LinkLimits, wire_counter: LinkLimitCounter) -> usize {
    usize::try_from(limits.effective_limit(wire_counter))
        .expect("every artifact wire ceiling fits every supported usize target")
}

fn wire_limit_error(limits: &LinkLimits, wire_counter: LinkLimitCounter) -> ArtifactContractError {
    let limit = limits.effective_limit(wire_counter);
    wire_counter
        .check_artifact_limit(limit + 1, limits)
        .expect_err("the canonical first-over value exceeds the effective wire limit")
        .into()
}

pub(super) fn parse_document(input: &[u8]) -> Result<JsonDocument, ArtifactContractError> {
    let source = std::str::from_utf8(input).map_err(|_| {
        invalid(
            ArtifactViolationCode::InvalidUtf8,
            ArtifactViolationLocation::Root,
        )
    })?;
    JsonDocument::parse(source).map_err(|error| match error {
        JsonParseError::Syntax => invalid(
            ArtifactViolationCode::InvalidJsonSyntax,
            ArtifactViolationLocation::Root,
        ),
        JsonParseError::TrailingData => invalid(
            ArtifactViolationCode::TrailingData,
            ArtifactViolationLocation::Root,
        ),
    })
}

pub(super) fn invalid(
    code: ArtifactViolationCode,
    location: ArtifactViolationLocation,
) -> ArtifactContractError {
    ArtifactContractError::InvalidArtifact(ArtifactViolation::new(code, location))
}

#[derive(Clone)]
pub(super) struct FieldSpec {
    pub(super) name: &'static str,
    pub(super) location: ArtifactViolationLocation,
}

pub(super) struct ObjectFields {
    values: Box<[Option<NodeId>]>,
    has_unknown: bool,
}

impl ObjectFields {
    pub(super) fn required(
        &self,
        index: usize,
        location: ArtifactViolationLocation,
    ) -> Result<NodeId, ArtifactContractError> {
        self.values[index].ok_or_else(|| invalid(ArtifactViolationCode::MissingMember, location))
    }

    pub(super) fn optional(&self, index: usize) -> Option<NodeId> {
        self.values[index]
    }

    pub(super) fn len(&self) -> usize {
        self.values.len()
    }

    pub(super) fn has_unknown(&self) -> bool {
        self.has_unknown
    }

    pub(super) fn reject_unknown(
        &self,
        container: ArtifactViolationLocation,
    ) -> Result<(), ArtifactContractError> {
        if self.has_unknown {
            return Err(invalid(ArtifactViolationCode::UnknownMember, container));
        }
        Ok(())
    }
}

/// Shared strict-schema operations over the duplicate-preserving JSON tree.
///
/// Typed codecs retain ownership of their field vocabulary and validation
/// phases. This trait centralizes only JSON category, integer, duplicate, and
/// checked-value error translation that must be identical for both artifacts.
pub(super) trait SchemaDecoder<'document> {
    fn document(&self) -> &'document JsonDocument;

    fn limits(&self) -> &LinkLimits;

    fn inspect_object(
        &self,
        id: NodeId,
        specs: &[FieldSpec],
        container: ArtifactViolationLocation,
    ) -> Result<ObjectFields, ArtifactContractError> {
        let members = match self.document().node(id) {
            JsonNode::Null => {
                return Err(invalid(ArtifactViolationCode::NullNotAllowed, container));
            }
            JsonNode::Object(members) => members,
            JsonNode::Bool | JsonNode::Number(_) | JsonNode::String(_) | JsonNode::Array(_) => {
                return Err(invalid(ArtifactViolationCode::TypeMismatch, container));
            }
        };
        inspect_members(members, specs, container)
    }

    fn string(
        &self,
        id: NodeId,
        location: ArtifactViolationLocation,
    ) -> Result<&'document str, ArtifactContractError> {
        match self.document().node(id) {
            JsonNode::Null => Err(invalid(ArtifactViolationCode::NullNotAllowed, location)),
            JsonNode::String(value) => Ok(value),
            JsonNode::Bool | JsonNode::Number(_) | JsonNode::Array(_) | JsonNode::Object(_) => {
                Err(invalid(ArtifactViolationCode::TypeMismatch, location))
            }
        }
    }

    fn array(
        &self,
        id: NodeId,
        location: ArtifactViolationLocation,
    ) -> Result<&'document [NodeId], ArtifactContractError> {
        match self.document().node(id) {
            JsonNode::Null => Err(invalid(ArtifactViolationCode::NullNotAllowed, location)),
            JsonNode::Array(values) => Ok(values),
            JsonNode::Bool | JsonNode::Number(_) | JsonNode::String(_) | JsonNode::Object(_) => {
                Err(invalid(ArtifactViolationCode::TypeMismatch, location))
            }
        }
    }

    fn unsigned_u16(
        &self,
        id: NodeId,
        location: ArtifactViolationLocation,
    ) -> Result<u16, ArtifactContractError> {
        let token = self.integer_token(id, location.clone())?;
        token
            .parse::<u16>()
            .map_err(|_| invalid(ArtifactViolationCode::InvalidInteger, location))
    }

    fn unsigned_u32(
        &self,
        id: NodeId,
        location: ArtifactViolationLocation,
    ) -> Result<u32, ArtifactContractError> {
        let token = self.integer_token(id, location.clone())?;
        token
            .parse::<u32>()
            .map_err(|_| invalid(ArtifactViolationCode::InvalidInteger, location))
    }

    fn integer_token(
        &self,
        id: NodeId,
        location: ArtifactViolationLocation,
    ) -> Result<&'document str, ArtifactContractError> {
        match self.document().node(id) {
            JsonNode::Null => Err(invalid(ArtifactViolationCode::NullNotAllowed, location)),
            JsonNode::Number(token)
                if token.as_ref() == "0"
                    || (token
                        .as_bytes()
                        .first()
                        .is_some_and(|byte| matches!(byte, b'1'..=b'9'))
                        && token.as_bytes().iter().all(u8::is_ascii_digit)) =>
            {
                Ok(token)
            }
            JsonNode::Number(_) => Err(invalid(ArtifactViolationCode::InvalidInteger, location)),
            JsonNode::Bool | JsonNode::String(_) | JsonNode::Array(_) | JsonNode::Object(_) => {
                Err(invalid(ArtifactViolationCode::TypeMismatch, location))
            }
        }
    }

    fn value_error(
        &self,
        error: &ValueConstructionError,
        location: ArtifactViolationLocation,
        token_counter: Option<LinkLimitCounter>,
    ) -> ArtifactContractError {
        // Value constructors enforce protocol ceilings. Replay their limit
        // evidence against the current possibly lower invocation budget.
        match error {
            ValueConstructionError::FieldLimit {
                counter, attempted, ..
            } => (*counter)
                .check_artifact_limit(*attempted, self.limits())
                .expect_err("a protocol-ceiling failure exceeds every lower limit")
                .into(),
            ValueConstructionError::StructuralLimit(evidence)
                if evidence.kind() == ValueLimitKind::Tokens && token_counter.is_some() =>
            {
                token_counter
                    .expect("guarded above")
                    .check_artifact_limit(evidence.attempted(), self.limits())
                    .expect_err("a token construction failure exceeds every lower limit")
                    .into()
            }
            ValueConstructionError::Grammar(_)
            | ValueConstructionError::StructuralLimit(_)
            | ValueConstructionError::Range(_) => {
                invalid(ArtifactViolationCode::InvalidValueGrammar, location)
            }
        }
    }
}

fn inspect_members(
    members: &[JsonMember],
    specs: &[FieldSpec],
    container: ArtifactViolationLocation,
) -> Result<ObjectFields, ArtifactContractError> {
    // Preserve the first value while counting all decoded names. Public
    // duplicate selection follows schema order rather than input order.
    let mut occurrences: BTreeMap<&str, (NodeId, u32)> = BTreeMap::new();
    for member in members {
        let occurrence = occurrences
            .entry(member.name.as_ref())
            .or_insert((member.value, 0));
        occurrence.1 += 1;
    }

    for spec in specs {
        if occurrences
            .get(spec.name)
            .is_some_and(|(_, count)| *count > 1)
        {
            return Err(invalid(
                ArtifactViolationCode::DuplicateMember,
                spec.location.clone(),
            ));
        }
    }
    if occurrences
        .iter()
        .any(|(name, (_, count))| *count > 1 && !specs.iter().any(|spec| spec.name == *name))
    {
        return Err(invalid(ArtifactViolationCode::DuplicateMember, container));
    }

    let values = specs
        .iter()
        .map(|spec| occurrences.get(spec.name).map(|(id, _)| *id))
        .collect::<Vec<_>>()
        .into_boxed_slice();
    let has_unknown = occurrences
        .keys()
        .any(|name| !specs.iter().any(|spec| spec.name == *name));
    Ok(ObjectFields {
        values,
        has_unknown,
    })
}

pub(super) trait JsonSink {
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), ArtifactContractError>;
}

pub(super) struct CountingSink<'a> {
    length: u64,
    limits: &'a LinkLimits,
    wire_counter: LinkLimitCounter,
}

impl<'a> CountingSink<'a> {
    pub(super) const fn new(limits: &'a LinkLimits, wire_counter: LinkLimitCounter) -> Self {
        Self {
            length: 0,
            limits,
            wire_counter,
        }
    }

    pub(super) fn length(&self) -> usize {
        usize::try_from(self.length).expect("an admitted artifact wire length fits usize")
    }
}

impl JsonSink for CountingSink<'_> {
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), ArtifactContractError> {
        let attempted = self
            .length
            .checked_add(bytes.len() as u64)
            .expect("the bounded canonical artifact length cannot overflow u64");
        self.wire_counter
            .check_artifact_limit(attempted, self.limits)?;
        self.length = attempted;
        Ok(())
    }
}

pub(super) struct VecSink {
    bytes: Vec<u8>,
}

impl VecSink {
    pub(super) fn with_capacity(capacity: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(capacity),
        }
    }

    pub(super) fn length(&self) -> usize {
        self.bytes.len()
    }

    pub(super) fn into_boxed_bytes(self) -> Box<[u8]> {
        self.bytes.into_boxed_slice()
    }
}

impl JsonSink for VecSink {
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), ArtifactContractError> {
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
}

pub(super) fn write_namespace(
    sink: &mut impl JsonSink,
    namespace: ArtifactNamespace,
) -> Result<(), ArtifactContractError> {
    sink.write_bytes(br#"{"kind":"#)?;
    write_json_string(sink, namespace.as_str())?;
    sink.write_bytes(b"}")
}

pub(super) fn write_string_array<'a>(
    sink: &mut impl JsonSink,
    values: impl Iterator<Item = &'a str>,
) -> Result<(), ArtifactContractError> {
    sink.write_bytes(b"[")?;
    for (index, value) in values.enumerate() {
        if index > 0 {
            sink.write_bytes(b",")?;
        }
        write_json_string(sink, value)?;
    }
    sink.write_bytes(b"]")
}

pub(super) fn write_json_string(
    sink: &mut impl JsonSink,
    value: &str,
) -> Result<(), ArtifactContractError> {
    sink.write_bytes(b"\"")?;
    let mut scalar_buffer = [0_u8; 4];
    for character in value.chars() {
        // Use the one canonical minimal spelling shared by both artifact kinds.
        match character {
            '"' => sink.write_bytes(br#"\""#)?,
            '\\' => sink.write_bytes(br"\\")?,
            '\u{0008}' => sink.write_bytes(br"\b")?,
            '\t' => sink.write_bytes(br"\t")?,
            '\n' => sink.write_bytes(br"\n")?,
            '\u{000c}' => sink.write_bytes(br"\f")?,
            '\r' => sink.write_bytes(br"\r")?,
            '\u{0000}'..='\u{001f}' => {
                const HEX: &[u8; 16] = b"0123456789abcdef";
                let value = character as u8;
                let escape = [
                    b'\\',
                    b'u',
                    b'0',
                    b'0',
                    HEX[usize::from(value >> 4)],
                    HEX[usize::from(value & 0x0f)],
                ];
                sink.write_bytes(&escape)?;
            }
            _ => sink.write_bytes(character.encode_utf8(&mut scalar_buffer).as_bytes())?,
        }
    }
    sink.write_bytes(b"\"")
}

pub(super) fn write_u16(sink: &mut impl JsonSink, value: u16) -> Result<(), ArtifactContractError> {
    write_unsigned(sink, u64::from(value))
}

pub(super) fn write_u32(sink: &mut impl JsonSink, value: u32) -> Result<(), ArtifactContractError> {
    write_unsigned(sink, u64::from(value))
}

fn write_unsigned(sink: &mut impl JsonSink, mut value: u64) -> Result<(), ArtifactContractError> {
    let mut buffer = [0_u8; 20];
    let mut cursor = buffer.len();
    loop {
        cursor -= 1;
        buffer[cursor] = b'0' + u8::try_from(value % 10).expect("one decimal digit fits u8");
        value /= 10;
        if value == 0 {
            break;
        }
    }
    sink.write_bytes(&buffer[cursor..])
}
