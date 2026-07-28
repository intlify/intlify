// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Strict JSON transport for one complete message-reference artifact.
//!
//! This module owns wire-byte admission, transport-versus-contract failure
//! separation, v0.1 schema decoding, canonical complete-document encoding, and
//! translation from untrusted JSON into the immutable checked reference model.
//!
//! It does not perform filesystem discovery, decompression, async I/O, artifact
//! framing, source-language analysis, linking, or publication.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::io::{self, Read};

use super::json::{JsonDocument, JsonMember, JsonNode, JsonParseError, NodeId};
use crate::{
    ArtifactContractError, ArtifactEnvelopeLocation, ArtifactNamespace, ArtifactPathRole,
    ArtifactVersion, ArtifactVersionEvidence, ArtifactVersionSupport, ArtifactViolation,
    ArtifactViolationCode, ArtifactViolationLocation, CatalogKey, CatalogKeyDomain,
    CatalogKeyPattern, CatalogKeyPrefix, CatalogScopeId, CatalogScopeName, DeliveryUnitId,
    DeliveryUnitSegment, LinkLimitCounter, LinkLimits, MessageReference, MessageReferenceArtifact,
    MessageSelector, PortablePathSegment, PortableRelativePath, ProducerId, ProducerIdentity,
    ProducerRevision, ReasonText, ReferenceArtifactIdentity, ReferenceArtifactSegment,
    ReferenceEnvelopeField, ReferenceField, SourceDocumentIdentity, SourceOrigin, SourceUtf8Span,
    ValueConstructionError, ValueLimitKind,
};

const REFERENCE_KIND: &str = "message-reference";
const VERSION_SUPPORT: ArtifactVersionSupport =
    ArtifactVersionSupport::Exact(ArtifactVersion::DRAFT_V0_1);
const READER_BUFFER_BYTES: usize = 8 * 1024;

/// Failure while reading and admitting one serialized reference artifact.
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

/// Decode one complete in-memory reference-artifact JSON document.
pub fn decode_reference_artifact(
    input: &[u8],
    limits: &LinkLimits,
) -> Result<MessageReferenceArtifact, ArtifactContractError> {
    admit_known_wire_length(input.len(), limits)?;
    decode_admitted_bytes(input, limits)
}

/// Read and decode one complete reference-artifact JSON document.
///
/// `Interrupted` reads are retried. Any other transport error before EOF wins
/// over a provisional JSON or contract failure. The reader is never asked for
/// bytes beyond the first byte over the effective wire limit.
pub fn decode_reference_artifact_from_reader<R: Read>(
    reader: &mut R,
    limits: &LinkLimits,
) -> Result<MessageReferenceArtifact, ArtifactReadError> {
    let wire_limit = effective_wire_limit_usize(limits);
    let mut input = Vec::with_capacity(wire_limit.min(READER_BUFFER_BYTES));
    let mut buffer = [0_u8; READER_BUFFER_BYTES];

    loop {
        // Never ask the transport for bytes after the canonical first-over
        // observation. This makes wire-limit evidence independent of the
        // reader's chunk size and leaves the remaining transport unread.
        let remaining_through_first_over = wire_limit.saturating_add(1).saturating_sub(input.len());
        let read_capacity = remaining_through_first_over.min(buffer.len());
        debug_assert!(read_capacity > 0);

        match reader.read(&mut buffer[..read_capacity]) {
            Ok(0) => {
                // Syntax and contract failures remain provisional until EOF:
                // a transport error before this point must win instead.
                return decode_admitted_bytes(&input, limits).map_err(ArtifactReadError::Contract);
            }
            Ok(read) => {
                if input.len() + read > wire_limit {
                    return Err(ArtifactReadError::Contract(wire_limit_error(limits)));
                }
                input.extend_from_slice(&buffer[..read]);
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(ArtifactReadError::Transport(error)),
        }
    }
}

/// Encode one immutable reference artifact as one complete canonical JSON value.
///
/// The writer first selects the supported version and performs a bounded
/// counting pass. It then revalidates decoded limits and allocates exactly one
/// complete returned byte buffer. No external sink observes a partial value.
pub fn encode_reference_artifact(
    artifact: &MessageReferenceArtifact,
    limits: &LinkLimits,
) -> Result<Box<[u8]>, ArtifactContractError> {
    if !VERSION_SUPPORT.supports(*artifact.version()) {
        return Err(ArtifactContractError::UnsupportedVersion(
            ArtifactVersionEvidence::for_unsupported(*artifact.version(), VERSION_SUPPORT),
        ));
    }

    let mut counter = CountingSink::new(limits);
    write_artifact(&mut counter, artifact)?;
    artifact.revalidate(limits)?;

    let mut output = VecSink::with_capacity(counter.length());
    write_artifact(&mut output, artifact)?;
    debug_assert_eq!(output.bytes.len(), counter.length());
    Ok(output.bytes.into_boxed_slice())
}

fn decode_admitted_bytes(
    input: &[u8],
    limits: &LinkLimits,
) -> Result<MessageReferenceArtifact, ArtifactContractError> {
    let source = std::str::from_utf8(input).map_err(|_| {
        invalid(
            ArtifactViolationCode::InvalidUtf8,
            ArtifactViolationLocation::Root,
        )
    })?;
    let document = JsonDocument::parse(source).map_err(|error| match error {
        JsonParseError::Syntax => invalid(
            ArtifactViolationCode::InvalidJsonSyntax,
            ArtifactViolationLocation::Root,
        ),
        JsonParseError::TrailingData => invalid(
            ArtifactViolationCode::TrailingData,
            ArtifactViolationLocation::Root,
        ),
    })?;
    Decoder::new(&document, limits).decode()
}

fn admit_known_wire_length(
    length: usize,
    limits: &LinkLimits,
) -> Result<(), ArtifactContractError> {
    if length > effective_wire_limit_usize(limits) {
        return Err(wire_limit_error(limits));
    }
    Ok(())
}

fn effective_wire_limit_usize(limits: &LinkLimits) -> usize {
    usize::try_from(limits.effective_limit(LinkLimitCounter::ReferenceArtifactWireBytes))
        .expect("the reference wire ceiling fits every supported usize target")
}

fn wire_limit_error(limits: &LinkLimits) -> ArtifactContractError {
    let limit = limits.effective_limit(LinkLimitCounter::ReferenceArtifactWireBytes);
    LinkLimitCounter::ReferenceArtifactWireBytes
        .check_artifact_limit(limit + 1, limits)
        .expect_err("the canonical first-over value exceeds the effective wire limit")
        .into()
}

fn invalid(
    code: ArtifactViolationCode,
    location: ArtifactViolationLocation,
) -> ArtifactContractError {
    ArtifactContractError::InvalidArtifact(ArtifactViolation::new(code, location))
}

fn reference_envelope(field: Option<ReferenceEnvelopeField>) -> ArtifactViolationLocation {
    ArtifactViolationLocation::Envelope(ArtifactEnvelopeLocation::Reference { field })
}

fn reference_location(ordinal: u32, field: Option<ReferenceField>) -> ArtifactViolationLocation {
    ArtifactViolationLocation::Reference { ordinal, field }
}

fn reference_origin_segment(
    reference_ordinal: u32,
    segment_ordinal: u32,
) -> ArtifactViolationLocation {
    ArtifactViolationLocation::ReferenceOriginSegment {
        reference_ordinal,
        segment_ordinal,
    }
}

#[derive(Clone)]
struct FieldSpec {
    name: &'static str,
    location: ArtifactViolationLocation,
}

struct ObjectFields {
    values: Box<[Option<NodeId>]>,
    has_unknown: bool,
}

impl ObjectFields {
    fn required(
        &self,
        index: usize,
        location: ArtifactViolationLocation,
    ) -> Result<NodeId, ArtifactContractError> {
        self.values[index].ok_or_else(|| invalid(ArtifactViolationCode::MissingMember, location))
    }

    fn reject_unknown(
        &self,
        container: ArtifactViolationLocation,
    ) -> Result<(), ArtifactContractError> {
        if self.has_unknown {
            return Err(invalid(ArtifactViolationCode::UnknownMember, container));
        }
        Ok(())
    }
}

struct Decoder<'document> {
    document: &'document JsonDocument,
    limits: &'document LinkLimits,
}

impl<'document> Decoder<'document> {
    const fn new(document: &'document JsonDocument, limits: &'document LinkLimits) -> Self {
        Self { document, limits }
    }

    fn decode(&self) -> Result<MessageReferenceArtifact, ArtifactContractError> {
        let root_specs = [
            FieldSpec {
                name: "kind",
                location: reference_envelope(Some(ReferenceEnvelopeField::Kind)),
            },
            FieldSpec {
                name: "version",
                location: reference_envelope(Some(ReferenceEnvelopeField::Version)),
            },
            FieldSpec {
                name: "producer",
                location: reference_envelope(Some(ReferenceEnvelopeField::Producer)),
            },
            FieldSpec {
                name: "identity",
                location: reference_envelope(Some(ReferenceEnvelopeField::Identity)),
            },
            FieldSpec {
                name: "deliveryUnit",
                location: reference_envelope(Some(ReferenceEnvelopeField::DeliveryUnit)),
            },
            FieldSpec {
                name: "references",
                location: reference_envelope(Some(ReferenceEnvelopeField::References)),
            },
        ];
        let root = self.inspect_object(
            self.document.root(),
            &root_specs,
            ArtifactViolationLocation::Root,
        )?;

        // Kind and version form the bootstrap schema. They are selected before
        // the remaining required-member and unknown-member checks so an
        // unsupported but structurally valid version wins deterministically.
        let kind_id = root.required(0, root_specs[0].location.clone())?;
        let kind = self.string(kind_id, root_specs[0].location.clone())?;
        if kind != REFERENCE_KIND {
            return Err(invalid(
                ArtifactViolationCode::KindMismatch,
                root_specs[0].location.clone(),
            ));
        }

        let version_id = root.required(1, root_specs[1].location.clone())?;
        let version = self.decode_version(version_id)?;
        if !VERSION_SUPPORT.supports(version) {
            return Err(ArtifactContractError::UnsupportedVersion(
                ArtifactVersionEvidence::for_unsupported(version, VERSION_SUPPORT),
            ));
        }

        let producer_id = root.required(2, root_specs[2].location.clone())?;
        let identity_id = root.required(3, root_specs[3].location.clone())?;
        let delivery_unit_id = root.required(4, root_specs[4].location.clone())?;
        let references_id = root.required(5, root_specs[5].location.clone())?;
        root.reject_unknown(ArtifactViolationLocation::Root)?;

        // From here onward the decoder follows the same canonical validation
        // phases as direct typed construction while accumulating the exact
        // decoded-byte charge in that order.
        let mut decoded_bytes = 0_u64;
        let producer = self.decode_producer(producer_id)?;
        self.charge_decoded(&mut decoded_bytes, producer.id().as_str().len() as u64)?;
        self.charge_decoded(
            &mut decoded_bytes,
            producer.revision().as_str().len() as u64,
        )?;

        let identity = self.decode_identity(identity_id)?;
        identity.revalidate_limits(self.limits)?;
        for segment in identity.segments() {
            self.charge_decoded(&mut decoded_bytes, segment.as_str().len() as u64)?;
        }

        let delivery_unit = self.decode_delivery_unit(delivery_unit_id)?;
        delivery_unit.revalidate_limits(self.limits)?;
        for segment in delivery_unit.segments() {
            self.charge_decoded(&mut decoded_bytes, segment.as_str().len() as u64)?;
        }

        let reference_nodes = self.array(
            references_id,
            reference_envelope(Some(ReferenceEnvelopeField::References)),
        )?;
        LinkLimitCounter::ReferenceRecords
            .check_artifact_limit(reference_nodes.len() as u64, self.limits)?;
        let mut raw_references = Vec::with_capacity(reference_nodes.len());
        for (ordinal, node) in reference_nodes.iter().copied().enumerate() {
            let ordinal = u32::try_from(ordinal)
                .expect("the admitted reference-record ceiling is below u32::MAX");
            raw_references.push(self.decode_raw_reference_envelope(node, ordinal)?);
        }

        // Record envelopes are staged first, then admitted collection-wide by
        // field phase. Validating one record completely before the next would
        // make error precedence depend on submitted record order.
        let references = self.admit_references(&raw_references, &mut decoded_bytes)?;
        let artifact = MessageReferenceArtifact::try_new(
            version,
            producer,
            identity,
            delivery_unit,
            references,
            self.limits,
        )?;
        debug_assert_eq!(artifact.decoded_bytes(), decoded_bytes);
        Ok(artifact)
    }

    fn inspect_object(
        &self,
        id: NodeId,
        specs: &[FieldSpec],
        container: ArtifactViolationLocation,
    ) -> Result<ObjectFields, ArtifactContractError> {
        let members = match self.document.node(id) {
            JsonNode::Null => {
                return Err(invalid(ArtifactViolationCode::NullNotAllowed, container));
            }
            JsonNode::Object(members) => members,
            JsonNode::Bool | JsonNode::Number(_) | JsonNode::String(_) | JsonNode::Array(_) => {
                return Err(invalid(ArtifactViolationCode::TypeMismatch, container));
            }
        };
        Self::inspect_members(members, specs, container)
    }

    fn inspect_members(
        members: &[JsonMember],
        specs: &[FieldSpec],
        container: ArtifactViolationLocation,
    ) -> Result<ObjectFields, ArtifactContractError> {
        // Preserve the first value while counting every decoded member name.
        // The following passes select duplicates by schema order, not by the
        // order in which an input object happened to spell its members.
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
        // Unknown duplicate names have no field-specific location. They are
        // considered only after every known field in canonical schema order.
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

    fn string(
        &self,
        id: NodeId,
        location: ArtifactViolationLocation,
    ) -> Result<&'document str, ArtifactContractError> {
        match self.document.node(id) {
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
        match self.document.node(id) {
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
        match self.document.node(id) {
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

    fn charge_decoded(&self, total: &mut u64, addend: u64) -> Result<(), ArtifactContractError> {
        let attempted = total
            .checked_add(addend)
            .expect("bounded artifact payload additions cannot overflow u64");
        LinkLimitCounter::ReferenceArtifactDecodedBytes
            .check_artifact_limit(attempted, self.limits)?;
        *total = attempted;
        Ok(())
    }

    fn value_error(
        &self,
        error: &ValueConstructionError,
        location: ArtifactViolationLocation,
        token_counter: Option<LinkLimitCounter>,
    ) -> ArtifactContractError {
        // Checked-value constructors report against protocol ceilings. Replay
        // their limit evidence through this decoder's possibly lower limits;
        // only non-limit construction failures become schema grammar errors.
        match error {
            ValueConstructionError::FieldLimit {
                counter, attempted, ..
            } => (*counter)
                .check_artifact_limit(*attempted, self.limits)
                .expect_err("a protocol-ceiling construction failure exceeds every lower limit")
                .into(),
            ValueConstructionError::StructuralLimit(evidence)
                if evidence.kind() == ValueLimitKind::Tokens && token_counter.is_some() =>
            {
                let counter = token_counter.expect("guarded above");
                counter
                    .check_artifact_limit(evidence.attempted(), self.limits)
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

// These borrowed staging records preserve schema-admitted fields without
// constructing semantic values too early. `admit_references` consumes them in
// canonical collection-wide phases.
struct RawReferenceEnvelope<'a> {
    ordinal: u32,
    scope: RawScope<'a>,
    domain: NodeId,
    selector: NodeId,
    reason: Option<NodeId>,
    origin: Option<NodeId>,
}

struct RawScope<'a> {
    namespace: NodeId,
    name: &'a str,
}

enum RawSelector<'a> {
    Exact(&'a str),
    Prefix(&'a str),
    Pattern(&'a str),
    AllInScope,
    UnboundedDynamic,
}

struct RawOrigin<'a> {
    source_namespace: NodeId,
    path: &'a [NodeId],
    span: NodeId,
}

impl<'document> Decoder<'document> {
    fn decode_version(&self, id: NodeId) -> Result<ArtifactVersion, ArtifactContractError> {
        let specs = [
            FieldSpec {
                name: "major",
                location: reference_envelope(Some(ReferenceEnvelopeField::VersionMajor)),
            },
            FieldSpec {
                name: "minor",
                location: reference_envelope(Some(ReferenceEnvelopeField::VersionMinor)),
            },
        ];
        let fields = self.inspect_object(
            id,
            &specs,
            reference_envelope(Some(ReferenceEnvelopeField::Version)),
        )?;
        let major_id = fields.required(0, specs[0].location.clone())?;
        let minor_id = fields.required(1, specs[1].location.clone())?;
        fields.reject_unknown(reference_envelope(Some(ReferenceEnvelopeField::Version)))?;
        let major = self.unsigned_u16(major_id, specs[0].location.clone())?;
        let minor = self.unsigned_u16(minor_id, specs[1].location.clone())?;
        Ok(ArtifactVersion::new(major, minor))
    }

    fn decode_producer(&self, id: NodeId) -> Result<ProducerIdentity, ArtifactContractError> {
        let specs = [
            FieldSpec {
                name: "id",
                location: reference_envelope(Some(ReferenceEnvelopeField::ProducerId)),
            },
            FieldSpec {
                name: "revision",
                location: reference_envelope(Some(ReferenceEnvelopeField::ProducerRevision)),
            },
        ];
        let fields = self.inspect_object(
            id,
            &specs,
            reference_envelope(Some(ReferenceEnvelopeField::Producer)),
        )?;
        let producer_id = fields.required(0, specs[0].location.clone())?;
        let revision_id = fields.required(1, specs[1].location.clone())?;
        fields.reject_unknown(reference_envelope(Some(ReferenceEnvelopeField::Producer)))?;

        let producer_id = self.string(producer_id, specs[0].location.clone())?;
        let revision = self.string(revision_id, specs[1].location.clone())?;
        let producer_id = ProducerId::try_new(producer_id)
            .map_err(|error| self.value_error(&error, specs[0].location.clone(), None))?;
        let revision = ProducerRevision::try_new(revision)
            .map_err(|error| self.value_error(&error, specs[1].location.clone(), None))?;
        Ok(ProducerIdentity::new(producer_id, revision))
    }

    fn decode_identity(
        &self,
        id: NodeId,
    ) -> Result<ReferenceArtifactIdentity, ArtifactContractError> {
        let specs = [
            FieldSpec {
                name: "namespace",
                location: reference_envelope(Some(ReferenceEnvelopeField::IdentityNamespace)),
            },
            FieldSpec {
                name: "segments",
                location: reference_envelope(Some(ReferenceEnvelopeField::IdentitySegments)),
            },
        ];
        let fields = self.inspect_object(
            id,
            &specs,
            reference_envelope(Some(ReferenceEnvelopeField::Identity)),
        )?;
        let namespace_id = fields.required(0, specs[0].location.clone())?;
        let segments_id = fields.required(1, specs[1].location.clone())?;
        fields.reject_unknown(reference_envelope(Some(ReferenceEnvelopeField::Identity)))?;
        let namespace = self.decode_namespace(namespace_id, specs[0].location.clone())?;
        let raw_segments = self.array(segments_id, specs[1].location.clone())?;

        if raw_segments.is_empty() {
            return Err(invalid(
                ArtifactViolationCode::InvalidValueGrammar,
                specs[1].location.clone(),
            ));
        }
        LinkLimitCounter::ReferenceIdentitySegments
            .check_artifact_limit(raw_segments.len() as u64, self.limits)?;
        let mut total = 0_u64;
        let mut segments = Vec::with_capacity(raw_segments.len());
        // Check each segment before extending the running path total. The
        // counter order is observable contract behavior and must not be
        // collapsed into one post-construction revalidation.
        for (ordinal, node) in raw_segments.iter().copied().enumerate() {
            let ordinal = u32::try_from(ordinal)
                .expect("the admitted identity-segment ceiling is below u32::MAX");
            let location = ArtifactViolationLocation::EnvelopePathSegment {
                role: ArtifactPathRole::ReferenceIdentity,
                segment_ordinal: ordinal,
            };
            let value = self.string(node, location.clone())?;
            LinkLimitCounter::ReferenceIdentitySegmentBytes
                .check_artifact_limit(value.len() as u64, self.limits)?;
            total = total
                .checked_add(value.len() as u64)
                .expect("bounded identity segments cannot overflow u64");
            LinkLimitCounter::ReferenceIdentityBytes.check_artifact_limit(total, self.limits)?;
            segments.push(
                ReferenceArtifactSegment::try_new(value)
                    .map_err(|error| self.value_error(&error, location, None))?,
            );
        }
        ReferenceArtifactIdentity::try_new(namespace, segments)
            .map_err(|error| self.value_error(&error, specs[1].location.clone(), None))
    }

    fn decode_delivery_unit(&self, id: NodeId) -> Result<DeliveryUnitId, ArtifactContractError> {
        let field_location = reference_envelope(Some(ReferenceEnvelopeField::DeliveryUnit));
        let raw_segments = self.array(id, field_location.clone())?;
        if raw_segments.is_empty() {
            return Err(invalid(
                ArtifactViolationCode::InvalidValueGrammar,
                field_location,
            ));
        }
        LinkLimitCounter::DeliveryUnitSegments
            .check_artifact_limit(raw_segments.len() as u64, self.limits)?;
        let mut total = 0_u64;
        let mut segments = Vec::with_capacity(raw_segments.len());
        // Delivery-unit evidence follows the same per-segment-before-total
        // ordering as identity paths, but uses its own independent counters.
        for (ordinal, node) in raw_segments.iter().copied().enumerate() {
            let ordinal = u32::try_from(ordinal)
                .expect("the admitted delivery-unit segment ceiling is below u32::MAX");
            let location = ArtifactViolationLocation::EnvelopePathSegment {
                role: ArtifactPathRole::DeliveryUnit,
                segment_ordinal: ordinal,
            };
            let value = self.string(node, location.clone())?;
            LinkLimitCounter::DeliveryUnitSegmentBytes
                .check_artifact_limit(value.len() as u64, self.limits)?;
            total = total
                .checked_add(value.len() as u64)
                .expect("bounded delivery-unit segments cannot overflow u64");
            LinkLimitCounter::DeliveryUnitBytes.check_artifact_limit(total, self.limits)?;
            segments.push(
                DeliveryUnitSegment::try_new(value)
                    .map_err(|error| self.value_error(&error, location, None))?,
            );
        }
        DeliveryUnitId::try_new(segments)
            .map_err(|error| self.value_error(&error, field_location, None))
    }

    fn decode_namespace(
        &self,
        id: NodeId,
        location: ArtifactViolationLocation,
    ) -> Result<ArtifactNamespace, ArtifactContractError> {
        let specs = [FieldSpec {
            name: "kind",
            location: location.clone(),
        }];
        let fields = self.inspect_object(id, &specs, location.clone())?;
        let kind_id = fields.required(0, location.clone())?;
        fields.reject_unknown(location.clone())?;
        let kind = self.string(kind_id, location.clone())?;
        match kind {
            "project" => Ok(ArtifactNamespace::Project),
            _ => Err(invalid(ArtifactViolationCode::UnknownTag, location)),
        }
    }

    fn decode_raw_reference_envelope(
        &self,
        id: NodeId,
        ordinal: u32,
    ) -> Result<RawReferenceEnvelope<'document>, ArtifactContractError> {
        let specs = [
            FieldSpec {
                name: "scope",
                location: reference_location(ordinal, Some(ReferenceField::Scope)),
            },
            FieldSpec {
                name: "domain",
                location: reference_location(ordinal, Some(ReferenceField::Domain)),
            },
            FieldSpec {
                name: "selector",
                location: reference_location(ordinal, Some(ReferenceField::Selector)),
            },
            FieldSpec {
                name: "reason",
                location: reference_location(ordinal, Some(ReferenceField::Reason)),
            },
            FieldSpec {
                name: "origin",
                location: reference_location(ordinal, Some(ReferenceField::Origin)),
            },
        ];
        let container = reference_location(ordinal, None);
        let fields = self.inspect_object(id, &specs, container.clone())?;
        let scope_id = fields.required(0, specs[0].location.clone())?;
        let domain_id = fields.required(1, specs[1].location.clone())?;
        let selector_id = fields.required(2, specs[2].location.clone())?;
        fields.reject_unknown(container)?;

        let scope = self.decode_raw_scope(scope_id, ordinal)?;
        Ok(RawReferenceEnvelope {
            ordinal,
            scope,
            domain: domain_id,
            selector: selector_id,
            reason: fields.values[3],
            origin: fields.values[4],
        })
    }

    fn decode_raw_scope(
        &self,
        id: NodeId,
        ordinal: u32,
    ) -> Result<RawScope<'document>, ArtifactContractError> {
        let specs = [
            FieldSpec {
                name: "namespace",
                location: reference_location(ordinal, Some(ReferenceField::ScopeNamespace)),
            },
            FieldSpec {
                name: "name",
                location: reference_location(ordinal, Some(ReferenceField::ScopeName)),
            },
        ];
        let container = reference_location(ordinal, Some(ReferenceField::Scope));
        let fields = self.inspect_object(id, &specs, container.clone())?;
        let namespace_id = fields.required(0, specs[0].location.clone())?;
        let name_id = fields.required(1, specs[1].location.clone())?;
        fields.reject_unknown(container)?;
        let name = self.string(name_id, specs[1].location.clone())?;
        Ok(RawScope {
            namespace: namespace_id,
            name,
        })
    }

    fn decode_raw_selector(
        &self,
        id: NodeId,
        ordinal: u32,
    ) -> Result<RawSelector<'document>, ArtifactContractError> {
        let specs = [
            FieldSpec {
                name: "kind",
                location: reference_location(ordinal, Some(ReferenceField::SelectorKind)),
            },
            FieldSpec {
                name: "key",
                location: reference_location(ordinal, Some(ReferenceField::SelectorKey)),
            },
            FieldSpec {
                name: "prefix",
                location: reference_location(ordinal, Some(ReferenceField::SelectorPrefix)),
            },
            FieldSpec {
                name: "pattern",
                location: reference_location(ordinal, Some(ReferenceField::SelectorPattern)),
            },
        ];
        let container = reference_location(ordinal, Some(ReferenceField::Selector));
        let fields = self.inspect_object(id, &specs, container.clone())?;
        let kind_id = fields.required(0, specs[0].location.clone())?;
        let kind = self.string(kind_id, specs[0].location.clone())?;
        // Resolve the tag before deciding which payload is required. Payloads
        // belonging to another selector variant are rejected as unknown
        // members rather than silently ignored.
        let allowed_payload = match kind {
            "exact" => Some(1),
            "prefix" => Some(2),
            "pattern" => Some(3),
            "all-in-scope" | "unbounded-dynamic" => None,
            _ => {
                return Err(invalid(
                    ArtifactViolationCode::UnknownTag,
                    specs[0].location.clone(),
                ));
            }
        };
        let payload = allowed_payload
            .map(|index| fields.required(index, specs[index].location.clone()))
            .transpose()?;

        let has_variant_unknown = (1..fields.values.len())
            .any(|index| fields.values[index].is_some() && Some(index) != allowed_payload);
        if fields.has_unknown || has_variant_unknown {
            return Err(invalid(ArtifactViolationCode::UnknownMember, container));
        }

        match kind {
            "exact" => Ok(RawSelector::Exact(self.string(
                payload.expect("exact selectors require key"),
                specs[1].location.clone(),
            )?)),
            "prefix" => Ok(RawSelector::Prefix(self.string(
                payload.expect("prefix selectors require prefix"),
                specs[2].location.clone(),
            )?)),
            "pattern" => Ok(RawSelector::Pattern(self.string(
                payload.expect("pattern selectors require pattern"),
                specs[3].location.clone(),
            )?)),
            "all-in-scope" => Ok(RawSelector::AllInScope),
            "unbounded-dynamic" => Ok(RawSelector::UnboundedDynamic),
            _ => unreachable!("the closed selector tag was validated above"),
        }
    }

    fn decode_raw_origin(
        &self,
        id: NodeId,
        ordinal: u32,
    ) -> Result<RawOrigin<'document>, ArtifactContractError> {
        let specs = [
            FieldSpec {
                name: "source",
                location: reference_location(ordinal, Some(ReferenceField::OriginSource)),
            },
            FieldSpec {
                name: "span",
                location: reference_location(ordinal, Some(ReferenceField::OriginSpan)),
            },
        ];
        let container = reference_location(ordinal, Some(ReferenceField::Origin));
        let fields = self.inspect_object(id, &specs, container.clone())?;
        let source_id = fields.required(0, specs[0].location.clone())?;
        let span_id = fields.required(1, specs[1].location.clone())?;
        fields.reject_unknown(container)?;
        let (source_namespace, path) = self.decode_raw_origin_source(source_id, ordinal)?;
        Ok(RawOrigin {
            source_namespace,
            path,
            span: span_id,
        })
    }

    fn decode_raw_origin_source(
        &self,
        id: NodeId,
        ordinal: u32,
    ) -> Result<(NodeId, &'document [NodeId]), ArtifactContractError> {
        let specs = [
            FieldSpec {
                name: "namespace",
                location: reference_location(ordinal, Some(ReferenceField::OriginSourceNamespace)),
            },
            FieldSpec {
                name: "path",
                location: reference_location(ordinal, Some(ReferenceField::OriginSourcePath)),
            },
        ];
        let container = reference_location(ordinal, Some(ReferenceField::OriginSource));
        let fields = self.inspect_object(id, &specs, container.clone())?;
        let namespace_id = fields.required(0, specs[0].location.clone())?;
        let path_id = fields.required(1, specs[1].location.clone())?;
        fields.reject_unknown(container)?;
        let nodes = self.array(path_id, specs[1].location.clone())?;
        Ok((namespace_id, nodes))
    }

    fn decode_raw_origin_span(
        &self,
        id: NodeId,
        ordinal: u32,
    ) -> Result<(u32, u32), ArtifactContractError> {
        let specs = [
            FieldSpec {
                name: "start",
                location: reference_location(ordinal, Some(ReferenceField::OriginSpanStart)),
            },
            FieldSpec {
                name: "end",
                location: reference_location(ordinal, Some(ReferenceField::OriginSpanEnd)),
            },
        ];
        let container = reference_location(ordinal, Some(ReferenceField::OriginSpan));
        let fields = self.inspect_object(id, &specs, container.clone())?;
        let start_id = fields.required(0, specs[0].location.clone())?;
        let end_id = fields.required(1, specs[1].location.clone())?;
        fields.reject_unknown(container)?;
        Ok((
            self.unsigned_u32(start_id, specs[0].location.clone())?,
            self.unsigned_u32(end_id, specs[1].location.clone())?,
        ))
    }

    fn admit_references(
        &self,
        envelopes: &[RawReferenceEnvelope<'document>],
        decoded_bytes: &mut u64,
    ) -> Result<Vec<MessageReference>, ArtifactContractError> {
        // Every pass below covers the complete record collection before the
        // next pass begins. This is intentionally not a record-at-a-time
        // decoder: field-phase precedence must be independent of which record
        // first contains a later failure.

        // Scope-name byte admission precedes scope value construction for all
        // records, including namespace and non-empty-name grammar checks.
        for reference in envelopes {
            LinkLimitCounter::CatalogScopeNameBytes
                .check_artifact_limit(reference.scope.name.len() as u64, self.limits)?;
            self.charge_decoded(decoded_bytes, reference.scope.name.len() as u64)?;
        }

        let mut scopes = Vec::with_capacity(envelopes.len());
        for reference in envelopes {
            let namespace = self.decode_namespace(
                reference.scope.namespace,
                reference_location(reference.ordinal, Some(ReferenceField::ScopeNamespace)),
            )?;
            let location = reference_location(reference.ordinal, Some(ReferenceField::ScopeName));
            let name = CatalogScopeName::try_new(reference.scope.name)
                .map_err(|error| self.value_error(&error, location, None))?;
            scopes.push(CatalogScopeId::new(namespace, name));
        }

        // Selector envelopes establish only the closed variant and raw payload.
        // Domain-qualified key grammar is deferred to its canonical phase.
        let mut raw_selectors = Vec::with_capacity(envelopes.len());
        for envelope in envelopes {
            raw_selectors.push(self.decode_raw_selector(envelope.selector, envelope.ordinal)?);
        }

        let mut selectors: Vec<Option<MessageSelector>> = std::iter::repeat_with(|| None)
            .take(envelopes.len())
            .collect();
        // Exact and Prefix share the selector-path byte counter. Complete this
        // pass before inspecting any Pattern byte budget or catalog domain.
        for selector in &raw_selectors {
            let payload = match selector {
                RawSelector::Exact(payload) | RawSelector::Prefix(payload) => payload,
                RawSelector::Pattern(_)
                | RawSelector::AllInScope
                | RawSelector::UnboundedDynamic => continue,
            };
            LinkLimitCounter::SelectorPathBytes
                .check_artifact_limit(payload.len() as u64, self.limits)?;
            self.charge_decoded(decoded_bytes, payload.len() as u64)?;
        }

        // Pattern has an independent byte budget and decoded-byte contribution.
        for selector in &raw_selectors {
            let RawSelector::Pattern(pattern) = selector else {
                continue;
            };
            LinkLimitCounter::SelectorPatternBytes
                .check_artifact_limit(pattern.len() as u64, self.limits)?;
            self.charge_decoded(decoded_bytes, pattern.len() as u64)?;
        }

        // Payload byte admission intentionally wins over an unknown domain.
        let mut domains = Vec::with_capacity(envelopes.len());
        for envelope in envelopes {
            let domain = self.string(
                envelope.domain,
                reference_location(envelope.ordinal, Some(ReferenceField::Domain)),
            )?;
            let domain = match domain {
                "json-pointer" => CatalogKeyDomain::JsonPointer,
                "yaml-typed-path" => CatalogKeyDomain::YamlTypedPath,
                "xliff-1.2" => CatalogKeyDomain::Xliff12,
                "xliff-2" => CatalogKeyDomain::Xliff2,
                _ => {
                    return Err(invalid(
                        ArtifactViolationCode::UnknownTag,
                        reference_location(envelope.ordinal, Some(ReferenceField::Domain)),
                    ));
                }
            };
            domains.push(domain);
        }

        // Pattern grammar and complexity are admitted before optional reason
        // and origin phases. Exact and Prefix grammar occur later by contract.
        for (index, selector) in raw_selectors.iter().enumerate() {
            let RawSelector::Pattern(pattern) = selector else {
                continue;
            };
            let location = reference_location(
                envelopes[index].ordinal,
                Some(ReferenceField::SelectorPattern),
            );
            let pattern =
                CatalogKeyPattern::try_new(domains[index], *pattern).map_err(|error| {
                    self.value_error(
                        &error,
                        location,
                        Some(LinkLimitCounter::SelectorPatternTokens),
                    )
                })?;
            selectors[index] = Some(MessageSelector::Pattern(pattern));
        }
        for selector in selectors.iter().flatten() {
            if let MessageSelector::Pattern(pattern) = selector {
                pattern.revalidate_tokens(self.limits)?;
            }
        }

        // Decode and charge reason strings now, but defer ReasonText grammar
        // until the earlier selector and origin byte phases have completed.
        let mut raw_reasons = Vec::with_capacity(envelopes.len());
        for envelope in envelopes {
            raw_reasons.push(
                envelope
                    .reason
                    .map(|node| {
                        self.string(
                            node,
                            reference_location(envelope.ordinal, Some(ReferenceField::Reason)),
                        )
                    })
                    .transpose()?,
            );
        }
        for reason in &raw_reasons {
            let Some(reason) = reason else {
                continue;
            };
            LinkLimitCounter::ReasonBytes.check_artifact_limit(reason.len() as u64, self.limits)?;
            self.charge_decoded(decoded_bytes, reason.len() as u64)?;
        }

        // Origin envelopes are staged without constructing path or span values.
        // This lets collection-wide path count and byte limits select failures
        // before per-segment grammar and span consistency.
        let mut raw_origins = Vec::with_capacity(envelopes.len());
        for envelope in envelopes {
            raw_origins.push(
                envelope
                    .origin
                    .map(|node| self.decode_raw_origin(node, envelope.ordinal))
                    .transpose()?,
            );
        }
        for origin in &raw_origins {
            let Some(origin) = origin else {
                continue;
            };
            LinkLimitCounter::PathSegments
                .check_artifact_limit(origin.path.len() as u64, self.limits)?;
        }

        // Decode every path segment and apply its individual byte limit before
        // computing any complete-path running total.
        let mut raw_origin_paths: Vec<Option<Vec<&str>>> = std::iter::repeat_with(|| None)
            .take(envelopes.len())
            .collect();
        for (index, origin) in raw_origins.iter().enumerate() {
            let Some(origin) = origin else {
                continue;
            };
            let mut path = Vec::with_capacity(origin.path.len());
            for (segment_ordinal, node) in origin.path.iter().copied().enumerate() {
                let segment_ordinal = u32::try_from(segment_ordinal)
                    .expect("the admitted path-segment ceiling is below u32::MAX");
                let segment = self.string(
                    node,
                    reference_origin_segment(envelopes[index].ordinal, segment_ordinal),
                )?;
                LinkLimitCounter::PathSegmentBytes
                    .check_artifact_limit(segment.len() as u64, self.limits)?;
                self.charge_decoded(decoded_bytes, segment.len() as u64)?;
                path.push(segment);
            }
            raw_origin_paths[index] = Some(path);
        }

        // PathBytes observes the first crossing running total within each path,
        // after all per-segment byte checks across the collection have passed.
        for path in &raw_origin_paths {
            let Some(path) = path else {
                continue;
            };
            let mut total = 0_u64;
            for segment in path {
                total = total
                    .checked_add(segment.len() as u64)
                    .expect("bounded path segments cannot overflow u64");
                LinkLimitCounter::PathBytes.check_artifact_limit(total, self.limits)?;
            }
        }

        // Exact and Prefix become domain-qualified values only after every
        // earlier origin byte phase. This ordering is easy to break by eagerly
        // constructing a selector while decoding its record envelope.
        for (index, raw_selector) in raw_selectors.iter().enumerate() {
            let (payload, field, is_prefix) = match raw_selector {
                RawSelector::Exact(payload) => (payload, ReferenceField::SelectorKey, false),
                RawSelector::Prefix(payload) => (payload, ReferenceField::SelectorPrefix, true),
                RawSelector::Pattern(_)
                | RawSelector::AllInScope
                | RawSelector::UnboundedDynamic => continue,
            };
            let location = reference_location(envelopes[index].ordinal, Some(field));
            let selector = if is_prefix {
                MessageSelector::Prefix(
                    CatalogKeyPrefix::try_new(domains[index], *payload).map_err(|error| {
                        self.value_error(&error, location, Some(LinkLimitCounter::CatalogKeyTokens))
                    })?,
                )
            } else {
                MessageSelector::Exact(CatalogKey::try_new(domains[index], *payload).map_err(
                    |error| {
                        self.value_error(&error, location, Some(LinkLimitCounter::CatalogKeyTokens))
                    },
                )?)
            };
            selectors[index] = Some(selector);
        }
        // Token limits are separate from byte limits and run only after every
        // Exact/Prefix value has passed its domain grammar.
        for selector in selectors.iter().flatten() {
            match selector {
                MessageSelector::Exact(key) => key.revalidate_tokens(self.limits)?,
                MessageSelector::Prefix(prefix) => prefix.revalidate_tokens(self.limits)?,
                MessageSelector::Pattern(_)
                | MessageSelector::AllInScope
                | MessageSelector::UnboundedDynamic => {}
            }
        }

        // Reason grammar follows selector admission; byte length was already
        // charged above so invalid text cannot reorder an earlier byte failure.
        let mut reasons = Vec::with_capacity(envelopes.len());
        for (index, reason) in raw_reasons.iter().enumerate() {
            let Some(reason) = reason else {
                reasons.push(None);
                continue;
            };
            let location =
                reference_location(envelopes[index].ordinal, Some(ReferenceField::Reason));
            reasons.push(Some(
                ReasonText::try_new(*reason)
                    .map_err(|error| self.value_error(&error, location, None))?,
            ));
        }

        // Origin semantic construction is last: namespace, portable segment
        // grammar, complete relative-path grammar, and span consistency are
        // checked only after all earlier collection phases have passed.
        let mut origins = Vec::with_capacity(envelopes.len());
        for (index, origin) in raw_origins.iter().enumerate() {
            let Some(origin) = origin else {
                origins.push(None);
                continue;
            };
            let ordinal = envelopes[index].ordinal;
            let namespace = self.decode_namespace(
                origin.source_namespace,
                reference_location(ordinal, Some(ReferenceField::OriginSourceNamespace)),
            )?;
            let raw_path = raw_origin_paths[index]
                .as_ref()
                .expect("origin path strings were admitted above");
            let mut segments = Vec::with_capacity(raw_path.len());
            for (segment_ordinal, segment) in raw_path.iter().copied().enumerate() {
                let segment_ordinal = u32::try_from(segment_ordinal)
                    .expect("the admitted path-segment ceiling is below u32::MAX");
                let location = reference_origin_segment(ordinal, segment_ordinal);
                segments.push(
                    PortablePathSegment::try_new(segment)
                        .map_err(|error| self.value_error(&error, location, None))?,
                );
            }
            let path_location = reference_location(ordinal, Some(ReferenceField::OriginSourcePath));
            let path = PortableRelativePath::try_new(segments)
                .map_err(|error| self.value_error(&error, path_location, None))?;
            let span_location = reference_location(ordinal, Some(ReferenceField::OriginSpan));
            let (start, end) = self.decode_raw_origin_span(origin.span, ordinal)?;
            let span = SourceUtf8Span::try_new(start, end)
                .map_err(|error| self.value_error(&error, span_location, None))?;
            origins.push(Some(SourceOrigin::new(
                SourceDocumentIdentity::new(namespace, path),
                span,
            )));
        }

        // All components are now independently admitted. The final constructor
        // enforces only cross-field consistency and cannot expose partial
        // references if one record is inconsistent.
        let mut references = Vec::with_capacity(envelopes.len());
        for (index, raw_selector) in raw_selectors.iter().enumerate() {
            let selector = match raw_selector {
                RawSelector::AllInScope => MessageSelector::AllInScope,
                RawSelector::UnboundedDynamic => MessageSelector::UnboundedDynamic,
                RawSelector::Exact(_) | RawSelector::Prefix(_) | RawSelector::Pattern(_) => {
                    selectors[index]
                        .take()
                        .expect("payload selector was admitted above")
                }
            };
            references.push(
                MessageReference::try_new(
                    scopes[index].clone(),
                    domains[index],
                    selector,
                    reasons[index].take(),
                    origins[index].take(),
                )
                .map_err(|_| {
                    invalid(
                        ArtifactViolationCode::InconsistentValue,
                        reference_location(
                            envelopes[index].ordinal,
                            Some(ReferenceField::Selector),
                        ),
                    )
                })?,
            );
        }
        Ok(references)
    }
}

trait JsonSink {
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), ArtifactContractError>;
}

struct CountingSink<'a> {
    length: u64,
    limits: &'a LinkLimits,
}

impl<'a> CountingSink<'a> {
    const fn new(limits: &'a LinkLimits) -> Self {
        Self { length: 0, limits }
    }

    fn length(&self) -> usize {
        usize::try_from(self.length)
            .expect("the admitted reference-artifact wire length fits usize")
    }
}

impl JsonSink for CountingSink<'_> {
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), ArtifactContractError> {
        let attempted = self
            .length
            .checked_add(bytes.len() as u64)
            .expect("the bounded canonical artifact length cannot overflow u64");
        LinkLimitCounter::ReferenceArtifactWireBytes
            .check_artifact_limit(attempted, self.limits)?;
        self.length = attempted;
        Ok(())
    }
}

struct VecSink {
    bytes: Vec<u8>,
}

impl VecSink {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(capacity),
        }
    }
}

impl JsonSink for VecSink {
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), ArtifactContractError> {
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
}

fn write_artifact(
    sink: &mut impl JsonSink,
    artifact: &MessageReferenceArtifact,
) -> Result<(), ArtifactContractError> {
    // Write the schema's canonical member order directly. Using a map or a
    // general serializer here could make ordering and number spelling depend
    // on an implementation detail rather than the versioned wire contract.
    sink.write_bytes(br#"{"kind":"message-reference","version":{"major":"#)?;
    write_u16(sink, artifact.version().major())?;
    sink.write_bytes(br#","minor":"#)?;
    write_u16(sink, artifact.version().minor())?;
    sink.write_bytes(br#"},"producer":{"id":"#)?;
    write_json_string(sink, artifact.producer().id().as_str())?;
    sink.write_bytes(br#","revision":"#)?;
    write_json_string(sink, artifact.producer().revision().as_str())?;
    sink.write_bytes(br#"},"identity":{"namespace":"#)?;
    write_namespace(sink, artifact.identity().namespace())?;
    sink.write_bytes(br#","segments":"#)?;
    write_string_array(
        sink,
        artifact
            .identity()
            .segments()
            .iter()
            .map(ReferenceArtifactSegment::as_str),
    )?;
    sink.write_bytes(br#"},"deliveryUnit":"#)?;
    write_string_array(
        sink,
        artifact
            .delivery_unit()
            .segments()
            .iter()
            .map(DeliveryUnitSegment::as_str),
    )?;
    sink.write_bytes(br#","references":["#)?;
    for (index, reference) in artifact.references().iter().enumerate() {
        if index > 0 {
            sink.write_bytes(b",")?;
        }
        write_reference(sink, reference)?;
    }
    sink.write_bytes(b"]}")?;
    Ok(())
}

fn write_reference(
    sink: &mut impl JsonSink,
    reference: &MessageReference,
) -> Result<(), ArtifactContractError> {
    sink.write_bytes(br#"{"scope":{"namespace":"#)?;
    write_namespace(sink, reference.scope().namespace())?;
    sink.write_bytes(br#","name":"#)?;
    write_json_string(sink, reference.scope().name().as_str())?;
    sink.write_bytes(br#"},"domain":"#)?;
    write_json_string(sink, reference.domain().as_str())?;
    sink.write_bytes(br#","selector":"#)?;
    write_selector(sink, reference.selector())?;
    if let Some(reason) = reference.reason() {
        sink.write_bytes(br#","reason":"#)?;
        write_json_string(sink, reason.as_str())?;
    }
    if let Some(origin) = reference.origin() {
        sink.write_bytes(br#","origin":"#)?;
        write_origin(sink, origin)?;
    }
    sink.write_bytes(b"}")?;
    Ok(())
}

fn write_selector(
    sink: &mut impl JsonSink,
    selector: &MessageSelector,
) -> Result<(), ArtifactContractError> {
    sink.write_bytes(br#"{"kind":"#)?;
    write_json_string(sink, selector.kind_str())?;
    match selector {
        MessageSelector::Exact(key) => {
            sink.write_bytes(br#","key":"#)?;
            write_json_string(sink, key.as_str())?;
        }
        MessageSelector::Prefix(prefix) => {
            sink.write_bytes(br#","prefix":"#)?;
            write_json_string(sink, prefix.as_str())?;
        }
        MessageSelector::Pattern(pattern) => {
            sink.write_bytes(br#","pattern":"#)?;
            write_json_string(sink, pattern.as_str())?;
        }
        MessageSelector::AllInScope | MessageSelector::UnboundedDynamic => {}
    }
    sink.write_bytes(b"}")?;
    Ok(())
}

fn write_origin(
    sink: &mut impl JsonSink,
    origin: &SourceOrigin,
) -> Result<(), ArtifactContractError> {
    sink.write_bytes(br#"{"source":{"namespace":"#)?;
    write_namespace(sink, origin.source().namespace())?;
    sink.write_bytes(br#","path":"#)?;
    write_string_array(
        sink,
        origin
            .source()
            .path()
            .segments()
            .iter()
            .map(PortablePathSegment::as_str),
    )?;
    sink.write_bytes(br#"},"span":{"start":"#)?;
    write_u32(sink, origin.span().start())?;
    sink.write_bytes(br#","end":"#)?;
    write_u32(sink, origin.span().end())?;
    sink.write_bytes(b"}}")?;
    Ok(())
}

fn write_namespace(
    sink: &mut impl JsonSink,
    namespace: ArtifactNamespace,
) -> Result<(), ArtifactContractError> {
    sink.write_bytes(br#"{"kind":"#)?;
    write_json_string(sink, namespace.as_str())?;
    sink.write_bytes(b"}")
}

fn write_string_array<'a>(
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

fn write_json_string(sink: &mut impl JsonSink, value: &str) -> Result<(), ArtifactContractError> {
    sink.write_bytes(b"\"")?;
    let mut scalar_buffer = [0_u8; 4];
    for character in value.chars() {
        // Use one deterministic minimal spelling: named JSON escapes where
        // available, lowercase \u00xx for remaining controls, and direct UTF-8
        // for every other Unicode scalar.
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

fn write_u16(sink: &mut impl JsonSink, value: u16) -> Result<(), ArtifactContractError> {
    write_unsigned(sink, u64::from(value))
}

fn write_u32(sink: &mut impl JsonSink, value: u32) -> Result<(), ArtifactContractError> {
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

#[cfg(test)]
mod tests {
    use std::io::{self, Read};

    use super::{
        decode_reference_artifact, decode_reference_artifact_from_reader,
        encode_reference_artifact, ArtifactReadError,
    };
    use crate::{
        ArtifactContractError, ArtifactViolationCode, ArtifactViolationLocation, LinkLimitCounter,
        LinkLimitObservation, LinkLimits, MessageReferenceArtifact, ReferenceEnvelopeField,
        ReferenceField,
    };

    const CANONICAL: &str = concat!(
        r#"{"kind":"message-reference","version":{"major":0,"minor":1},"#,
        r#""producer":{"id":"dev.intlify/js-reference","revision":"0.1.0"},"#,
        r#""identity":{"namespace":{"kind":"project"},"segments":["js","module","src","app.ts"]},"#,
        r#""deliveryUnit":["main"],"references":[{"#,
        r#""scope":{"namespace":{"kind":"project"},"name":"app"},"#,
        r#""domain":"json-pointer","selector":{"kind":"exact","key":"/title"},"#,
        r#""origin":{"source":{"namespace":{"kind":"project"},"path":["src","app.ts"]},"#,
        r#""span":{"start":1,"end":7}}},{"#,
        r#""scope":{"namespace":{"kind":"project"},"name":"app"},"#,
        r#""domain":"json-pointer","selector":{"kind":"pattern","pattern":"/items/*"},"#,
        r#""reason":"bounded set\n"}]}"#
    );

    const ZERO_REFERENCE: &str = concat!(
        r#"{"kind":"message-reference","version":{"major":0,"minor":1},"#,
        r#""producer":{"id":"dev.intlify/js-reference","revision":"0.1.0"},"#,
        r#""identity":{"namespace":{"kind":"project"},"segments":["empty"]},"#,
        r#""deliveryUnit":["main"],"references":[]}"#
    );

    fn violation(
        error: &ArtifactContractError,
    ) -> (ArtifactViolationCode, ArtifactViolationLocation) {
        let ArtifactContractError::InvalidArtifact(violation) = error else {
            panic!("expected invalid artifact, got {error:?}");
        };
        (violation.code(), violation.location().clone())
    }

    #[test]
    fn canonical_writer_round_trips_exact_bytes_and_preserves_record_order() {
        let limits = LinkLimits::default();
        let artifact = decode_reference_artifact(CANONICAL.as_bytes(), &limits).unwrap();
        assert_eq!(artifact.references().len(), 2);
        assert_eq!(artifact.record_identity(0).unwrap().ordinal(), 0);
        assert_eq!(artifact.record_identity(1).unwrap().ordinal(), 1);
        assert_eq!(
            encode_reference_artifact(&artifact, &limits)
                .unwrap()
                .as_ref(),
            CANONICAL.as_bytes()
        );

        let round_trip = decode_reference_artifact(
            &encode_reference_artifact(&artifact, &limits).unwrap(),
            &limits,
        )
        .unwrap();
        assert_eq!(round_trip, artifact);
    }

    #[test]
    fn decoder_accepts_nonsemantic_member_order_whitespace_and_escape_spelling() {
        let input = concat!(
            " {\n",
            r#""references":[],"deliveryUnit":["main"],"#,
            r#""identity":{"segments":["empty"],"namespace":{"kind":"project"}},"#,
            r#""producer":{"revision":"0.1.0","id":"dev.intlify/js-reference"},"#,
            r#""version":{"minor":1,"major":0},"kind":"message-\u0072eference"} "#,
            "\r\n"
        );
        let limits = LinkLimits::default();
        let artifact = decode_reference_artifact(input.as_bytes(), &limits).unwrap();
        assert_eq!(
            encode_reference_artifact(&artifact, &limits)
                .unwrap()
                .as_ref(),
            ZERO_REFERENCE.as_bytes()
        );
    }

    #[test]
    fn zero_reference_artifact_remains_a_complete_distinct_value() {
        let limits = LinkLimits::default();
        let artifact = decode_reference_artifact(ZERO_REFERENCE.as_bytes(), &limits).unwrap();
        assert!(artifact.references().is_empty());
        assert!(artifact.record_identity(0).is_none());
        assert_eq!(artifact.identity().segments()[0].as_str(), "empty");
        assert_eq!(artifact.delivery_unit().segments()[0].as_str(), "main");
    }

    #[test]
    fn decoder_selects_closed_root_and_version_failures() {
        let limits = LinkLimits::default();
        let invalid_utf8 = decode_reference_artifact(&[0xff], &limits).unwrap_err();
        assert_eq!(
            violation(&invalid_utf8),
            (
                ArtifactViolationCode::InvalidUtf8,
                ArtifactViolationLocation::Root
            )
        );

        let trailing = decode_reference_artifact(b"{} true", &limits).unwrap_err();
        assert_eq!(violation(&trailing).0, ArtifactViolationCode::TrailingData);

        let duplicate =
            decode_reference_artifact(br#"{"kind":"message-reference","kind":"x"}"#, &limits)
                .unwrap_err();
        assert_eq!(
            violation(&duplicate).0,
            ArtifactViolationCode::DuplicateMember
        );

        let wrong_kind = ZERO_REFERENCE.replace("message-reference", "message-definition");
        let wrong_kind = decode_reference_artifact(wrong_kind.as_bytes(), &limits).unwrap_err();
        assert_eq!(
            violation(&wrong_kind).0,
            ArtifactViolationCode::KindMismatch
        );

        let unsupported = ZERO_REFERENCE.replace(
            r#""version":{"major":0,"minor":1}"#,
            r#""version":{"major":0,"minor":2}"#,
        );
        assert!(matches!(
            decode_reference_artifact(unsupported.as_bytes(), &limits),
            Err(ArtifactContractError::UnsupportedVersion(_))
        ));
    }

    #[test]
    fn decoder_rejects_unknown_null_and_noncanonical_integer_forms() {
        let limits = LinkLimits::default();

        let unknown =
            ZERO_REFERENCE.replace(r#""references":[]}"#, r#""references":[],"extra":0}"#);
        assert_eq!(
            violation(&decode_reference_artifact(unknown.as_bytes(), &limits).unwrap_err()).0,
            ArtifactViolationCode::UnknownMember
        );

        let null = ZERO_REFERENCE.replace(r#""deliveryUnit":["main"]"#, r#""deliveryUnit":null"#);
        assert_eq!(
            violation(&decode_reference_artifact(null.as_bytes(), &limits).unwrap_err()).0,
            ArtifactViolationCode::NullNotAllowed
        );

        let signed = ZERO_REFERENCE.replace(r#""major":0"#, r#""major":-0"#);
        assert_eq!(
            violation(&decode_reference_artifact(signed.as_bytes(), &limits).unwrap_err()).0,
            ArtifactViolationCode::InvalidInteger
        );
    }

    #[test]
    fn canonical_writer_uses_exact_string_escaping() {
        let escaped = ZERO_REFERENCE.replace(
            r#""references":[]}"#,
            concat!(
                r#""references":[{"scope":{"namespace":{"kind":"project"},"#,
                r#""name":"x\u0001\t\"\\/\ud83d\ude00"},"domain":"json-pointer","#,
                r#""selector":{"kind":"all-in-scope"}}]}"#
            ),
        );
        let limits = LinkLimits::default();
        let artifact = decode_reference_artifact(escaped.as_bytes(), &limits).unwrap();
        let encoded = String::from_utf8(
            encode_reference_artifact(&artifact, &limits)
                .unwrap()
                .into_vec(),
        )
        .unwrap();
        assert!(encoded.contains(r#""name":"x\u0001\t\"\\/😀""#));
    }

    #[test]
    fn direct_and_decoded_lower_limit_failures_are_identical() {
        let defaults = LinkLimits::default();
        let artifact = decode_reference_artifact(ZERO_REFERENCE.as_bytes(), &defaults).unwrap();
        let lower = defaults
            .clone()
            .try_with_limit(LinkLimitCounter::ReferenceArtifactDecodedBytes, 1)
            .unwrap();
        let direct = MessageReferenceArtifact::try_new(
            *artifact.version(),
            artifact.producer().clone(),
            artifact.identity().clone(),
            artifact.delivery_unit().clone(),
            artifact.references().to_vec(),
            &lower,
        )
        .unwrap_err();
        let decoded = decode_reference_artifact(ZERO_REFERENCE.as_bytes(), &lower).unwrap_err();
        assert_eq!(decoded, direct);
    }

    #[test]
    fn every_selector_variant_has_one_exact_canonical_wire_shape() {
        let limits = LinkLimits::default();
        for (selector, expected_kind) in [
            (r#"{"kind":"exact","key":""}"#, "exact"),
            (r#"{"kind":"prefix","prefix":"/items"}"#, "prefix"),
            (r#"{"kind":"pattern","pattern":"/items/*"}"#, "pattern"),
            (r#"{"kind":"all-in-scope"}"#, "all-in-scope"),
            (r#"{"kind":"unbounded-dynamic"}"#, "unbounded-dynamic"),
        ] {
            let reference = format!(
                concat!(
                    r#"{{"scope":{{"namespace":{{"kind":"project"}},"name":"app"}},"#,
                    r#""domain":"json-pointer","selector":{}}}"#
                ),
                selector
            );
            let input = ZERO_REFERENCE.replace(
                r#""references":[]}"#,
                &format!(r#""references":[{reference}]}}"#),
            );
            let artifact = decode_reference_artifact(input.as_bytes(), &limits).unwrap();
            assert_eq!(
                artifact.references()[0].selector().kind_str(),
                expected_kind
            );
            assert_eq!(
                encode_reference_artifact(&artifact, &limits)
                    .unwrap()
                    .as_ref(),
                input.as_bytes()
            );
        }
    }

    #[test]
    fn canonical_validation_phases_win_over_later_record_grammar() {
        let input = ZERO_REFERENCE.replace(
            r#""references":[]}"#,
            concat!(
                r#""references":[{"scope":{"namespace":{"kind":"project"},"name":"app"},"#,
                r#""domain":"json-pointer","selector":{"kind":"exact","key":"not-a-pointer"}}]}"#
            ),
        );
        let limits = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::CatalogScopeNameBytes, 0)
            .unwrap();
        let error = decode_reference_artifact(input.as_bytes(), &limits).unwrap_err();
        let ArtifactContractError::Limit(evidence) = error else {
            panic!("scope phase should select its limit before selector grammar");
        };
        assert_eq!(evidence.counter(), LinkLimitCounter::CatalogScopeNameBytes);
    }

    #[test]
    fn complete_scope_pass_wins_over_an_earlier_records_remaining_schema() {
        let input = ZERO_REFERENCE.replace(
            r#""references":[]}"#,
            concat!(
                r#""references":[{"scope":{"namespace":{"kind":"project"},"name":"app"},"#,
                r#""domain":0,"selector":{"kind":"all-in-scope"}},{"#,
                r#""scope":{"namespace":{"kind":"project"},"name":"later"},"#,
                r#""domain":"json-pointer","selector":{"kind":"all-in-scope"}}]}"#
            ),
        );
        let limits = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::CatalogScopeNameBytes, 3)
            .unwrap();
        let error = decode_reference_artifact(input.as_bytes(), &limits).unwrap_err();
        let ArtifactContractError::Limit(evidence) = error else {
            panic!("complete scope pass should precede remaining record schema");
        };
        assert_eq!(evidence.counter(), LinkLimitCounter::CatalogScopeNameBytes);
        assert_eq!(evidence.observation(), LinkLimitObservation::Exact(4));
    }

    #[test]
    fn selector_byte_passes_win_over_domain_and_reason_schema() {
        let domain_input = ZERO_REFERENCE.replace(
            r#""references":[]}"#,
            concat!(
                r#""references":[{"scope":{"namespace":{"kind":"project"},"name":"app"},"#,
                r#""domain":0,"selector":{"kind":"exact","key":"/a"}},{"#,
                r#""scope":{"namespace":{"kind":"project"},"name":"app"},"#,
                r#""domain":"json-pointer","selector":{"kind":"exact","key":"/long"}}]}"#
            ),
        );
        let path_limits = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::SelectorPathBytes, 2)
            .unwrap();
        let error = decode_reference_artifact(domain_input.as_bytes(), &path_limits).unwrap_err();
        let ArtifactContractError::Limit(evidence) = error else {
            panic!("selector bytes should precede domain schema");
        };
        assert_eq!(evidence.counter(), LinkLimitCounter::SelectorPathBytes);
        assert_eq!(evidence.observation(), LinkLimitObservation::Exact(3));

        let reason_input = ZERO_REFERENCE.replace(
            r#""references":[]}"#,
            concat!(
                r#""references":[{"scope":{"namespace":{"kind":"project"},"name":"app"},"#,
                r#""domain":"json-pointer","selector":{"kind":"pattern","pattern":""}},{"#,
                r#""scope":{"namespace":{"kind":"project"},"name":"app"},"#,
                r#""domain":"json-pointer","selector":{"kind":"all-in-scope"},"reason":null}]}"#
            ),
        );
        let error =
            decode_reference_artifact(reason_input.as_bytes(), &LinkLimits::default()).unwrap_err();
        assert_eq!(
            violation(&error),
            (
                ArtifactViolationCode::InvalidValueGrammar,
                ArtifactViolationLocation::Reference {
                    ordinal: 0,
                    field: Some(ReferenceField::SelectorPattern),
                },
            )
        );
    }

    #[test]
    fn reason_grammar_waits_for_the_complete_origin_byte_passes() {
        let input = ZERO_REFERENCE.replace(
            r#""references":[]}"#,
            concat!(
                r#""references":[{"scope":{"namespace":{"kind":"project"},"name":"app"},"#,
                r#""domain":"json-pointer","selector":{"kind":"all-in-scope"},"#,
                r#""reason":"\u0001"},{"scope":{"namespace":{"kind":"project"},"name":"app"},"#,
                r#""domain":"json-pointer","selector":{"kind":"all-in-scope"},"#,
                r#""origin":{"source":{"namespace":{"kind":"project"},"path":["xx"]},"#,
                r#""span":{"start":0,"end":0}}}]}"#
            ),
        );
        let limits = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::PathSegmentBytes, 1)
            .unwrap();
        let error = decode_reference_artifact(input.as_bytes(), &limits).unwrap_err();
        let ArtifactContractError::Limit(evidence) = error else {
            panic!("origin byte admission should precede reason grammar");
        };
        assert_eq!(evidence.counter(), LinkLimitCounter::PathSegmentBytes);
        assert_eq!(evidence.observation(), LinkLimitObservation::Exact(2));
    }

    #[test]
    fn origin_count_and_byte_preflights_win_over_segment_and_span_schema() {
        let over_count = ZERO_REFERENCE.replace(
            r#""references":[]}"#,
            concat!(
                r#""references":[{"scope":{"namespace":{"kind":"project"},"name":"app"},"#,
                r#""domain":"json-pointer","selector":{"kind":"all-in-scope"},"#,
                r#""origin":{"source":{"namespace":{"kind":"project"},"path":[0,"x"]},"#,
                r#""span":null}}]}"#
            ),
        );
        let count_limits = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::PathSegments, 1)
            .unwrap();
        let error = decode_reference_artifact(over_count.as_bytes(), &count_limits).unwrap_err();
        let ArtifactContractError::Limit(evidence) = error else {
            panic!("path count should precede segment and span schema");
        };
        assert_eq!(evidence.counter(), LinkLimitCounter::PathSegments);
        assert_eq!(evidence.observation(), LinkLimitObservation::Exact(2));

        let over_segment = over_count.replace(r#""path":[0,"x"]"#, r#""path":["x"]"#);
        let segment_limits = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::PathSegmentBytes, 0)
            .unwrap();
        let error =
            decode_reference_artifact(over_segment.as_bytes(), &segment_limits).unwrap_err();
        let ArtifactContractError::Limit(evidence) = error else {
            panic!("path segment bytes should precede span schema");
        };
        assert_eq!(evidence.counter(), LinkLimitCounter::PathSegmentBytes);
        assert_eq!(evidence.observation(), LinkLimitObservation::Exact(1));
    }

    #[test]
    fn origin_path_grammar_waits_for_exact_selector_parsing() {
        let input = ZERO_REFERENCE.replace(
            r#""references":[]}"#,
            concat!(
                r#""references":[{"scope":{"namespace":{"kind":"project"},"name":"app"},"#,
                r#""domain":"json-pointer","selector":{"kind":"all-in-scope"},"#,
                r#""origin":{"source":{"namespace":{"kind":"project"},"path":["."]},"#,
                r#""span":{"start":0,"end":0}}},{"#,
                r#""scope":{"namespace":{"kind":"project"},"name":"app"},"#,
                r#""domain":"json-pointer","selector":{"kind":"exact","key":"not-a-pointer"}}]}"#
            ),
        );
        let error =
            decode_reference_artifact(input.as_bytes(), &LinkLimits::default()).unwrap_err();
        assert_eq!(
            violation(&error),
            (
                ArtifactViolationCode::InvalidValueGrammar,
                ArtifactViolationLocation::Reference {
                    ordinal: 1,
                    field: Some(ReferenceField::SelectorKey),
                },
            )
        );
    }

    #[test]
    fn reference_count_preflight_wins_before_record_decoding() {
        let input = ZERO_REFERENCE.replace(r#""references":[]}"#, r#""references":[null]}"#);
        let limits = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::ReferenceRecords, 0)
            .unwrap();
        let error = decode_reference_artifact(input.as_bytes(), &limits).unwrap_err();
        let ArtifactContractError::Limit(evidence) = error else {
            panic!("record count should be preflighted");
        };
        assert_eq!(evidence.counter(), LinkLimitCounter::ReferenceRecords);
        assert_eq!(evidence.observation(), LinkLimitObservation::Exact(1));
    }

    #[test]
    fn known_wire_overrun_wins_before_utf8_or_json_admission() {
        let limits = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::ReferenceArtifactWireBytes, 0)
            .unwrap();
        let error = decode_reference_artifact(&[0xff], &limits).unwrap_err();
        let ArtifactContractError::Limit(evidence) = error else {
            panic!("wire phase must precede UTF-8");
        };
        assert_eq!(
            evidence.counter(),
            LinkLimitCounter::ReferenceArtifactWireBytes
        );
    }

    #[test]
    fn reader_chunking_matches_slice_decoding() {
        let limits = LinkLimits::default();
        for chunk in [1, 2, 7, 64, 4096] {
            let mut reader = ChunkReader::new(CANONICAL.as_bytes(), chunk);
            let from_reader = decode_reference_artifact_from_reader(&mut reader, &limits).unwrap();
            let from_slice = decode_reference_artifact(CANONICAL.as_bytes(), &limits).unwrap();
            assert_eq!(from_reader, from_slice);
        }
    }

    #[test]
    fn reader_chunking_preserves_every_selected_contract_failure() {
        let inputs: [&[u8]; 6] = [
            &[0xff],
            b"{",
            b"{} true",
            br#"{"kind":"message-reference","kind":"message-reference"}"#,
            br#"{"kind":"message-definition"}"#,
            br#"{"kind":"message-reference","version":{"major":0,"minor":2}}"#,
        ];
        let limits = LinkLimits::default();

        for input in inputs {
            let expected = decode_reference_artifact(input, &limits).unwrap_err();
            for chunk in [1, 2, 7, 64] {
                let mut reader = ChunkReader::new(input, chunk);
                let actual = match decode_reference_artifact_from_reader(&mut reader, &limits)
                    .unwrap_err()
                {
                    ArtifactReadError::Contract(error) => error,
                    ArtifactReadError::Transport(error) => {
                        panic!("bounded in-memory fixture cannot fail transport: {error}")
                    }
                };
                assert_eq!(actual, expected, "input={input:?}, chunk={chunk}");
            }
        }
    }

    #[test]
    fn known_and_unknown_length_wire_overruns_select_identical_evidence() {
        let limits = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::ReferenceArtifactWireBytes, 5)
            .unwrap();
        let expected = decode_reference_artifact(CANONICAL.as_bytes(), &limits).unwrap_err();

        for chunk in [1, 2, 7, 64, 4096] {
            let mut reader = ChunkReader::new(CANONICAL.as_bytes(), chunk);
            let actual =
                match decode_reference_artifact_from_reader(&mut reader, &limits).unwrap_err() {
                    ArtifactReadError::Contract(error) => error,
                    ArtifactReadError::Transport(error) => {
                        panic!("bounded in-memory fixture cannot fail transport: {error}")
                    }
                };
            assert_eq!(actual, expected, "chunk={chunk}");
            assert_eq!(reader.offset, 6);
        }
    }

    #[test]
    fn direct_and_wire_admission_share_all_observable_lower_limit_phases() {
        let defaults = LinkLimits::default();
        let artifact = decode_reference_artifact(CANONICAL.as_bytes(), &defaults).unwrap();
        let counters = [
            LinkLimitCounter::ReferenceIdentitySegmentBytes,
            LinkLimitCounter::ReferenceIdentitySegments,
            LinkLimitCounter::ReferenceIdentityBytes,
            LinkLimitCounter::DeliveryUnitSegmentBytes,
            LinkLimitCounter::DeliveryUnitSegments,
            LinkLimitCounter::DeliveryUnitBytes,
            LinkLimitCounter::ReferenceRecords,
            LinkLimitCounter::CatalogScopeNameBytes,
            LinkLimitCounter::SelectorPathBytes,
            LinkLimitCounter::SelectorPatternBytes,
            LinkLimitCounter::SelectorPatternTokens,
            LinkLimitCounter::ReasonBytes,
            LinkLimitCounter::PathSegments,
            LinkLimitCounter::PathSegmentBytes,
            LinkLimitCounter::PathBytes,
            LinkLimitCounter::CatalogKeyTokens,
            LinkLimitCounter::ReferenceArtifactDecodedBytes,
        ];

        for counter in counters {
            let limits = defaults.clone().try_with_limit(counter, 0).unwrap();
            let direct = MessageReferenceArtifact::try_new(
                *artifact.version(),
                artifact.producer().clone(),
                artifact.identity().clone(),
                artifact.delivery_unit().clone(),
                artifact.references().to_vec(),
                &limits,
            )
            .unwrap_err();
            let decoded = decode_reference_artifact(CANONICAL.as_bytes(), &limits).unwrap_err();
            assert_eq!(direct, decoded, "counter={counter:?}");
        }
    }

    #[test]
    fn reader_transport_failure_precedes_a_provisional_contract_result() {
        let limits = LinkLimits::default();
        let mut reader = ErrorBeforeEof {
            bytes: b"{}".as_slice(),
            delivered: false,
        };
        assert!(matches!(
            decode_reference_artifact_from_reader(&mut reader, &limits),
            Err(ArtifactReadError::Transport(error))
                if error.kind() == io::ErrorKind::TimedOut
        ));
    }

    #[test]
    fn interrupted_reads_are_semantically_invisible() {
        let limits = LinkLimits::default();
        let mut reader = InterruptingReader {
            inner: ChunkReader::new(ZERO_REFERENCE.as_bytes(), 3),
            interrupt_next: true,
        };
        let artifact = decode_reference_artifact_from_reader(&mut reader, &limits).unwrap();
        let expected = decode_reference_artifact(ZERO_REFERENCE.as_bytes(), &limits).unwrap();
        assert_eq!(artifact, expected);
    }

    #[test]
    fn reader_stops_at_exactly_the_first_wire_byte_over_limit() {
        let limits = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::ReferenceArtifactWireBytes, 5)
            .unwrap();
        let mut reader = ChunkReader::new(b"0123456789", 10);
        let error = decode_reference_artifact_from_reader(&mut reader, &limits).unwrap_err();
        let ArtifactReadError::Contract(ArtifactContractError::Limit(evidence)) = error else {
            panic!("expected contract wire limit");
        };
        assert_eq!(reader.offset, 6);
        assert_eq!(
            evidence.counter(),
            LinkLimitCounter::ReferenceArtifactWireBytes
        );
        assert_eq!(evidence.observation(), LinkLimitObservation::Exact(6));
    }

    #[test]
    fn encoder_counting_pass_returns_no_partial_value() {
        let defaults = LinkLimits::default();
        let artifact = decode_reference_artifact(ZERO_REFERENCE.as_bytes(), &defaults).unwrap();
        let lower = defaults
            .clone()
            .try_with_limit(
                LinkLimitCounter::ReferenceArtifactWireBytes,
                u64::try_from(ZERO_REFERENCE.len() - 1).unwrap(),
            )
            .unwrap();
        let error = encode_reference_artifact(&artifact, &lower).unwrap_err();
        let ArtifactContractError::Limit(evidence) = error else {
            panic!("expected wire limit");
        };
        assert_eq!(
            evidence.counter(),
            LinkLimitCounter::ReferenceArtifactWireBytes
        );
        assert_eq!(
            evidence.observation(),
            LinkLimitObservation::Exact(u64::try_from(ZERO_REFERENCE.len()).unwrap())
        );
    }

    struct ChunkReader<'a> {
        bytes: &'a [u8],
        offset: usize,
        chunk: usize,
    }

    impl<'a> ChunkReader<'a> {
        const fn new(bytes: &'a [u8], chunk: usize) -> Self {
            Self {
                bytes,
                offset: 0,
                chunk,
            }
        }
    }

    impl Read for ChunkReader<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if self.offset == self.bytes.len() {
                return Ok(0);
            }
            let length = buffer
                .len()
                .min(self.chunk)
                .min(self.bytes.len() - self.offset);
            buffer[..length].copy_from_slice(&self.bytes[self.offset..self.offset + length]);
            self.offset += length;
            Ok(length)
        }
    }

    struct ErrorBeforeEof<'a> {
        bytes: &'a [u8],
        delivered: bool,
    }

    struct InterruptingReader<'a> {
        inner: ChunkReader<'a>,
        interrupt_next: bool,
    }

    impl Read for InterruptingReader<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if self.interrupt_next {
                self.interrupt_next = false;
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "fixture interruption",
                ));
            }
            self.interrupt_next = true;
            self.inner.read(buffer)
        }
    }

    impl Read for ErrorBeforeEof<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if !self.delivered {
                buffer[..self.bytes.len()].copy_from_slice(self.bytes);
                self.delivered = true;
                return Ok(self.bytes.len());
            }
            Err(io::Error::new(io::ErrorKind::TimedOut, "fixture timeout"))
        }
    }

    #[test]
    fn missing_kind_has_typed_envelope_location() {
        let limits = LinkLimits::default();
        let error = decode_reference_artifact(b"{}", &limits).unwrap_err();
        let (code, location) = violation(&error);
        assert_eq!(code, ArtifactViolationCode::MissingMember);
        assert_eq!(
            location,
            super::reference_envelope(Some(ReferenceEnvelopeField::Kind))
        );
    }
}
