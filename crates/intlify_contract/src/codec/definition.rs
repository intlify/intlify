// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Strict JSON transport for one complete message-definition artifact.
//!
//! This module owns definition wire-byte admission, v0.1 schema decoding,
//! collection-wide validation precedence, canonical complete-document
//! encoding, and conversion from untrusted JSON into immutable contract values.
//!
//! It does not read catalog files, extract host entries, recompute freshness
//! fingerprints, parse MF2 payloads, perform linking, or publish artifacts.

use std::io::Read;

use super::common::{
    admit_known_wire_length, decode_from_reader, invalid, parse_document, write_json_string,
    write_namespace, write_string_array, write_u16, write_u32, ArtifactReadError, CountingSink,
    FieldSpec, JsonSink, SchemaDecoder, VecSink,
};
use super::json::{JsonDocument, NodeId};
use crate::{
    ArtifactContractError, ArtifactEnvelopeLocation, ArtifactNamespace, ArtifactPathRole,
    ArtifactVersion, ArtifactVersionEvidence, ArtifactVersionSupport, ArtifactViolationCode,
    ArtifactViolationLocation, CatalogKey, CatalogKeyDomain, CatalogScopeId, CatalogScopeName,
    DefinitionEnvelopeField, DefinitionField, EntryReference, EntryStructuralPath,
    FingerprintAlgorithm, FingerprintDigest, InputFingerprint, LinkLimitCounter, LinkLimits,
    Locale, MessageDefinition, MessageDefinitionArtifact, MessagePayload, PortablePathSegment,
    PortableRelativePath, ProducerId, ProducerIdentity, ProducerRevision, SourceDocumentIdentity,
};

const DEFINITION_KIND: &str = "message-definition";
const VERSION_SUPPORT: ArtifactVersionSupport =
    ArtifactVersionSupport::Exact(ArtifactVersion::DRAFT_V0_1);

/// Decode one complete in-memory definition-artifact JSON document.
pub fn decode_definition_artifact(
    input: &[u8],
    limits: &LinkLimits,
) -> Result<MessageDefinitionArtifact, ArtifactContractError> {
    admit_known_wire_length(
        input.len(),
        limits,
        LinkLimitCounter::DefinitionArtifactWireBytes,
    )?;
    decode_admitted_bytes(input, limits)
}

/// Read and decode one complete definition-artifact JSON document.
///
/// `Interrupted` reads are retried. Any other transport error before EOF wins
/// over a provisional contract failure, and reading stops at the first wire
/// byte over the current effective limit.
pub fn decode_definition_artifact_from_reader<R: Read>(
    reader: &mut R,
    limits: &LinkLimits,
) -> Result<MessageDefinitionArtifact, ArtifactReadError> {
    decode_from_reader(
        reader,
        limits,
        LinkLimitCounter::DefinitionArtifactWireBytes,
        decode_admitted_bytes,
    )
}

/// Encode one immutable definition artifact as canonical v0.1 JSON.
///
/// A bounded counting pass completes before decoded revalidation and exact
/// allocation, so no caller-visible sink can observe a partial document.
pub fn encode_definition_artifact(
    artifact: &MessageDefinitionArtifact,
    limits: &LinkLimits,
) -> Result<Box<[u8]>, ArtifactContractError> {
    if !VERSION_SUPPORT.supports(*artifact.version()) {
        return Err(ArtifactContractError::UnsupportedVersion(
            ArtifactVersionEvidence::for_unsupported(*artifact.version(), VERSION_SUPPORT),
        ));
    }

    let mut counter = CountingSink::new(limits, LinkLimitCounter::DefinitionArtifactWireBytes);
    write_artifact(&mut counter, artifact)?;
    artifact.revalidate(limits)?;

    let mut output = VecSink::with_capacity(counter.length());
    write_artifact(&mut output, artifact)?;
    debug_assert_eq!(output.length(), counter.length());
    Ok(output.into_boxed_bytes())
}

fn decode_admitted_bytes(
    input: &[u8],
    limits: &LinkLimits,
) -> Result<MessageDefinitionArtifact, ArtifactContractError> {
    let document = parse_document(input)?;
    Decoder::new(&document, limits).decode()
}

fn definition_envelope(field: Option<DefinitionEnvelopeField>) -> ArtifactViolationLocation {
    ArtifactViolationLocation::Envelope(ArtifactEnvelopeLocation::Definition { field })
}

fn definition_location(ordinal: u32, field: Option<DefinitionField>) -> ArtifactViolationLocation {
    ArtifactViolationLocation::Definition { ordinal, field }
}

fn definition_source_segment(segment_ordinal: u32) -> ArtifactViolationLocation {
    ArtifactViolationLocation::EnvelopePathSegment {
        role: ArtifactPathRole::DefinitionSource,
        segment_ordinal,
    }
}

fn logical_alias_location(
    alias_ordinal: u32,
    segment_ordinal: Option<u32>,
) -> ArtifactViolationLocation {
    ArtifactViolationLocation::LogicalAlias {
        alias_ordinal,
        segment_ordinal,
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

    fn decode(&self) -> Result<MessageDefinitionArtifact, ArtifactContractError> {
        let root_specs = [
            FieldSpec {
                name: "kind",
                location: definition_envelope(Some(DefinitionEnvelopeField::Kind)),
            },
            FieldSpec {
                name: "version",
                location: definition_envelope(Some(DefinitionEnvelopeField::Version)),
            },
            FieldSpec {
                name: "producer",
                location: definition_envelope(Some(DefinitionEnvelopeField::Producer)),
            },
            FieldSpec {
                name: "source",
                location: definition_envelope(Some(DefinitionEnvelopeField::Source)),
            },
            FieldSpec {
                name: "logicalAliases",
                location: definition_envelope(Some(DefinitionEnvelopeField::LogicalAliases)),
            },
            FieldSpec {
                name: "inputFingerprint",
                location: definition_envelope(Some(DefinitionEnvelopeField::InputFingerprint)),
            },
            FieldSpec {
                name: "definitions",
                location: definition_envelope(Some(DefinitionEnvelopeField::Definitions)),
            },
        ];
        let root = self.inspect_object(
            self.document.root(),
            &root_specs,
            ArtifactViolationLocation::Root,
        )?;

        // Kind and version select the schema before the supported-version
        // presence preflight. This lets a valid newer version win over fields
        // that are unknown only because its schema was not selected.
        let kind_id = root.required(0, root_specs[0].location.clone())?;
        let kind = self.string(kind_id, root_specs[0].location.clone())?;
        if kind != DEFINITION_KIND {
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
        let source_id = root.required(3, root_specs[3].location.clone())?;
        let aliases_id = root.required(4, root_specs[4].location.clone())?;
        let fingerprint_id = root.required(5, root_specs[5].location.clone())?;
        let definitions_id = root.required(6, root_specs[6].location.clone())?;
        root.reject_unknown(ArtifactViolationLocation::Root)?;

        // The remaining work follows the canonical envelope phases rather than
        // the input object's member order.
        let mut decoded_bytes = 0_u64;
        let producer = self.decode_producer(producer_id)?;
        self.charge_decoded(&mut decoded_bytes, producer.id().as_str().len() as u64)?;
        self.charge_decoded(
            &mut decoded_bytes,
            producer.revision().as_str().len() as u64,
        )?;

        let source = self.decode_primary_source(source_id, &mut decoded_bytes)?;
        let logical_aliases =
            self.decode_logical_aliases(aliases_id, &source, &mut decoded_bytes)?;
        let input_fingerprint =
            self.decode_input_fingerprint(fingerprint_id, &mut decoded_bytes)?;

        let definition_nodes = self.array(
            definitions_id,
            definition_envelope(Some(DefinitionEnvelopeField::Definitions)),
        )?;
        LinkLimitCounter::Definitions
            .check_artifact_limit(definition_nodes.len() as u64, self.limits)?;

        // Stage every record envelope before collection-wide field admission.
        // Fully validating record zero before inspecting record one would
        // violate the fixed cross-record precedence.
        let mut raw_definitions = Vec::with_capacity(definition_nodes.len());
        for (ordinal, node) in definition_nodes.iter().copied().enumerate() {
            let ordinal =
                u32::try_from(ordinal).expect("the admitted definition ceiling is below u32::MAX");
            raw_definitions.push(self.decode_raw_definition_envelope(node, ordinal)?);
        }
        let definitions = self.admit_definitions(&raw_definitions, &mut decoded_bytes)?;

        let artifact = MessageDefinitionArtifact::try_new(
            version,
            producer,
            source,
            logical_aliases,
            input_fingerprint,
            definitions,
            self.limits,
        )?;
        debug_assert_eq!(artifact.decoded_bytes(), decoded_bytes);
        Ok(artifact)
    }

    fn charge_decoded(&self, total: &mut u64, addend: u64) -> Result<(), ArtifactContractError> {
        let attempted = total
            .checked_add(addend)
            .expect("bounded definition payload additions cannot overflow u64");
        LinkLimitCounter::DefinitionArtifactDecodedBytes
            .check_artifact_limit(attempted, self.limits)?;
        *total = attempted;
        Ok(())
    }

    fn decode_version(&self, id: NodeId) -> Result<ArtifactVersion, ArtifactContractError> {
        let specs = [
            FieldSpec {
                name: "major",
                location: definition_envelope(Some(DefinitionEnvelopeField::VersionMajor)),
            },
            FieldSpec {
                name: "minor",
                location: definition_envelope(Some(DefinitionEnvelopeField::VersionMinor)),
            },
        ];
        let container = definition_envelope(Some(DefinitionEnvelopeField::Version));
        let fields = self.inspect_object(id, &specs, container.clone())?;
        let major_id = fields.required(0, specs[0].location.clone())?;
        let minor_id = fields.required(1, specs[1].location.clone())?;
        fields.reject_unknown(container)?;
        let major = self.unsigned_u16(major_id, specs[0].location.clone())?;
        let minor = self.unsigned_u16(minor_id, specs[1].location.clone())?;
        Ok(ArtifactVersion::new(major, minor))
    }

    fn decode_producer(&self, id: NodeId) -> Result<ProducerIdentity, ArtifactContractError> {
        let specs = [
            FieldSpec {
                name: "id",
                location: definition_envelope(Some(DefinitionEnvelopeField::ProducerId)),
            },
            FieldSpec {
                name: "revision",
                location: definition_envelope(Some(DefinitionEnvelopeField::ProducerRevision)),
            },
        ];
        let container = definition_envelope(Some(DefinitionEnvelopeField::Producer));
        let fields = self.inspect_object(id, &specs, container.clone())?;
        let producer_id = fields.required(0, specs[0].location.clone())?;
        let revision_id = fields.required(1, specs[1].location.clone())?;
        fields.reject_unknown(container)?;

        let producer_id = self.string(producer_id, specs[0].location.clone())?;
        let revision = self.string(revision_id, specs[1].location.clone())?;
        let producer_id = ProducerId::try_new(producer_id)
            .map_err(|error| self.value_error(&error, specs[0].location.clone(), None))?;
        let revision = ProducerRevision::try_new(revision)
            .map_err(|error| self.value_error(&error, specs[1].location.clone(), None))?;
        Ok(ProducerIdentity::new(producer_id, revision))
    }

    fn decode_primary_source(
        &self,
        id: NodeId,
        decoded_bytes: &mut u64,
    ) -> Result<SourceDocumentIdentity, ArtifactContractError> {
        let specs = [
            FieldSpec {
                name: "namespace",
                location: definition_envelope(Some(DefinitionEnvelopeField::SourceNamespace)),
            },
            FieldSpec {
                name: "path",
                location: definition_envelope(Some(DefinitionEnvelopeField::SourcePath)),
            },
        ];
        let container = definition_envelope(Some(DefinitionEnvelopeField::Source));
        let fields = self.inspect_object(id, &specs, container.clone())?;
        let namespace_id = fields.required(0, specs[0].location.clone())?;
        let path_id = fields.required(1, specs[1].location.clone())?;
        fields.reject_unknown(container)?;

        let namespace = self.decode_namespace(namespace_id, specs[0].location.clone())?;
        let path_nodes = self.array(path_id, specs[1].location.clone())?;

        // Primary path numeric phases finish before grammar and before any
        // logical-alias work. Segment payloads are charged only after their
        // field-specific byte limit succeeds.
        LinkLimitCounter::PathSegments
            .check_artifact_limit(path_nodes.len() as u64, self.limits)?;
        let mut raw_segments = Vec::with_capacity(path_nodes.len());
        for (ordinal, node) in path_nodes.iter().copied().enumerate() {
            let ordinal =
                u32::try_from(ordinal).expect("the admitted path ceiling is below u32::MAX");
            let location = definition_source_segment(ordinal);
            let segment = self.string(node, location.clone())?;
            LinkLimitCounter::PathSegmentBytes
                .check_artifact_limit(segment.len() as u64, self.limits)?;
            self.charge_decoded(decoded_bytes, segment.len() as u64)?;
            raw_segments.push((segment, location));
        }
        self.check_path_bytes(raw_segments.iter().map(|(segment, _)| *segment))?;

        if raw_segments.is_empty() {
            return Err(invalid(
                ArtifactViolationCode::InvalidValueGrammar,
                specs[1].location.clone(),
            ));
        }
        let segments = raw_segments
            .into_iter()
            .map(|(segment, location)| {
                PortablePathSegment::try_new(segment)
                    .map_err(|error| self.value_error(&error, location, None))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let path = PortableRelativePath::try_new(segments)
            .map_err(|error| self.value_error(&error, specs[1].location.clone(), None))?;
        Ok(SourceDocumentIdentity::new(namespace, path))
    }

    fn decode_logical_aliases(
        &self,
        id: NodeId,
        source: &SourceDocumentIdentity,
        decoded_bytes: &mut u64,
    ) -> Result<Vec<PortableRelativePath>, ArtifactContractError> {
        let field_location = definition_envelope(Some(DefinitionEnvelopeField::LogicalAliases));
        let alias_nodes = self.array(id, field_location)?;
        LinkLimitCounter::LogicalAliases
            .check_artifact_limit(alias_nodes.len() as u64, self.limits)?;

        // Pass one admits every alias container and segment count.
        let mut raw_paths = Vec::with_capacity(alias_nodes.len());
        for (alias_ordinal, node) in alias_nodes.iter().copied().enumerate() {
            let alias_ordinal =
                u32::try_from(alias_ordinal).expect("the alias ceiling is below u32::MAX");
            let location = logical_alias_location(alias_ordinal, None);
            let segment_nodes = self.array(node, location)?;
            LinkLimitCounter::PathSegments
                .check_artifact_limit(segment_nodes.len() as u64, self.limits)?;
            raw_paths.push((alias_ordinal, segment_nodes));
        }

        // Pass two admits and charges every segment before any per-path total.
        let mut raw_strings = Vec::with_capacity(raw_paths.len());
        for (alias_ordinal, segment_nodes) in &raw_paths {
            let mut segments = Vec::with_capacity(segment_nodes.len());
            for (segment_ordinal, node) in segment_nodes.iter().copied().enumerate() {
                let segment_ordinal = u32::try_from(segment_ordinal)
                    .expect("the admitted path ceiling is below u32::MAX");
                let location = logical_alias_location(*alias_ordinal, Some(segment_ordinal));
                let segment = self.string(node, location.clone())?;
                LinkLimitCounter::PathSegmentBytes
                    .check_artifact_limit(segment.len() as u64, self.limits)?;
                self.charge_decoded(decoded_bytes, segment.len() as u64)?;
                segments.push((segment, location));
            }
            raw_strings.push((*alias_ordinal, segments));
        }

        // Complete per-path totals precede the one primary-plus-alias total.
        for (_, segments) in &raw_strings {
            self.check_path_bytes(segments.iter().map(|(segment, _)| *segment))?;
        }
        let mut source_path_bytes = source.path().decoded_bytes();
        LinkLimitCounter::SourcePathBytes.check_artifact_limit(source_path_bytes, self.limits)?;
        for (_, segments) in &raw_strings {
            let alias_bytes = segments
                .iter()
                .map(|(segment, _)| segment.len() as u64)
                .sum::<u64>();
            source_path_bytes = source_path_bytes
                .checked_add(alias_bytes)
                .expect("bounded alias paths cannot overflow u64");
            LinkLimitCounter::SourcePathBytes
                .check_artifact_limit(source_path_bytes, self.limits)?;
        }

        // Only after every numeric phase do grammar and canonical-set checks
        // run. This prevents an early duplicate from hiding a later bad path.
        let mut aliases = Vec::with_capacity(raw_strings.len());
        for (alias_ordinal, segments) in raw_strings {
            let alias_location = logical_alias_location(alias_ordinal, None);
            let segments = segments
                .into_iter()
                .map(|(segment, location)| {
                    PortablePathSegment::try_new(segment)
                        .map_err(|error| self.value_error(&error, location, None))
                })
                .collect::<Result<Vec<_>, _>>()?;
            aliases.push(
                PortableRelativePath::try_new(segments)
                    .map_err(|error| self.value_error(&error, alias_location, None))?,
            );
        }

        let mut previous = source.path();
        for (ordinal, alias) in aliases.iter().enumerate() {
            let location = logical_alias_location(
                u32::try_from(ordinal).expect("the alias ceiling is below u32::MAX"),
                None,
            );
            match alias.cmp(previous) {
                std::cmp::Ordering::Equal => {
                    return Err(invalid(ArtifactViolationCode::DuplicateValue, location));
                }
                std::cmp::Ordering::Less => {
                    return Err(invalid(ArtifactViolationCode::NonCanonicalOrder, location));
                }
                std::cmp::Ordering::Greater => previous = alias,
            }
        }
        Ok(aliases)
    }

    fn decode_input_fingerprint(
        &self,
        id: NodeId,
        decoded_bytes: &mut u64,
    ) -> Result<InputFingerprint, ArtifactContractError> {
        let specs = [
            FieldSpec {
                name: "algorithm",
                location: definition_envelope(Some(DefinitionEnvelopeField::FingerprintAlgorithm)),
            },
            FieldSpec {
                name: "digest",
                location: definition_envelope(Some(DefinitionEnvelopeField::FingerprintDigest)),
            },
        ];
        let container = definition_envelope(Some(DefinitionEnvelopeField::InputFingerprint));
        let fields = self.inspect_object(id, &specs, container.clone())?;
        let algorithm_id = fields.required(0, specs[0].location.clone())?;
        let digest_id = fields.required(1, specs[1].location.clone())?;
        fields.reject_unknown(container)?;

        let algorithm = self.string(algorithm_id, specs[0].location.clone())?;
        let algorithm = match algorithm {
            "blake3-256" => FingerprintAlgorithm::Blake3_256,
            _ => {
                return Err(invalid(
                    ArtifactViolationCode::UnknownTag,
                    specs[0].location.clone(),
                ));
            }
        };
        let digest = self.string(digest_id, specs[1].location.clone())?;
        let digest = decode_fingerprint_digest(digest).ok_or_else(|| {
            invalid(
                ArtifactViolationCode::InvalidValueGrammar,
                specs[1].location.clone(),
            )
        })?;
        self.charge_decoded(decoded_bytes, digest.as_bytes().len() as u64)?;
        Ok(InputFingerprint::new(algorithm, digest))
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

    fn decode_raw_definition_envelope(
        &self,
        id: NodeId,
        ordinal: u32,
    ) -> Result<RawDefinitionEnvelope, ArtifactContractError> {
        let specs = [
            FieldSpec {
                name: "scope",
                location: definition_location(ordinal, Some(DefinitionField::Scope)),
            },
            FieldSpec {
                name: "domain",
                location: definition_location(ordinal, Some(DefinitionField::Domain)),
            },
            FieldSpec {
                name: "key",
                location: definition_location(ordinal, Some(DefinitionField::Key)),
            },
            FieldSpec {
                name: "locale",
                location: definition_location(ordinal, Some(DefinitionField::Locale)),
            },
            FieldSpec {
                name: "message",
                location: definition_location(ordinal, Some(DefinitionField::Message)),
            },
            FieldSpec {
                name: "source",
                location: definition_location(ordinal, Some(DefinitionField::Source)),
            },
        ];
        let container = definition_location(ordinal, None);
        let fields = self.inspect_object(id, &specs, container.clone())?;
        let scope_id = fields.required(0, specs[0].location.clone())?;
        let domain_id = fields.required(1, specs[1].location.clone())?;
        let key_id = fields.required(2, specs[2].location.clone())?;
        let locale_id = fields.required(3, specs[3].location.clone())?;
        let message_id = fields.required(4, specs[4].location.clone())?;
        let source_id = fields.required(5, specs[5].location.clone())?;
        fields.reject_unknown(container)?;

        Ok(RawDefinitionEnvelope {
            ordinal,
            scope: self.decode_raw_scope(scope_id, ordinal)?,
            domain: domain_id,
            key: key_id,
            locale: locale_id,
            message: message_id,
            source: self.decode_raw_entry_source(source_id, ordinal)?,
        })
    }

    fn decode_raw_scope(
        &self,
        id: NodeId,
        ordinal: u32,
    ) -> Result<RawScope, ArtifactContractError> {
        let specs = [
            FieldSpec {
                name: "namespace",
                location: definition_location(ordinal, Some(DefinitionField::ScopeNamespace)),
            },
            FieldSpec {
                name: "name",
                location: definition_location(ordinal, Some(DefinitionField::ScopeName)),
            },
        ];
        let container = definition_location(ordinal, Some(DefinitionField::Scope));
        let fields = self.inspect_object(id, &specs, container.clone())?;
        let namespace = fields.required(0, specs[0].location.clone())?;
        let name = fields.required(1, specs[1].location.clone())?;
        fields.reject_unknown(container)?;
        Ok(RawScope { namespace, name })
    }

    fn decode_raw_entry_source(
        &self,
        id: NodeId,
        ordinal: u32,
    ) -> Result<RawEntrySource, ArtifactContractError> {
        let specs = [
            FieldSpec {
                name: "structuralPath",
                location: definition_location(ordinal, Some(DefinitionField::SourceStructuralPath)),
            },
            FieldSpec {
                name: "occurrence",
                location: definition_location(ordinal, Some(DefinitionField::SourceOccurrence)),
            },
        ];
        let container = definition_location(ordinal, Some(DefinitionField::Source));
        let fields = self.inspect_object(id, &specs, container.clone())?;
        let structural_path = fields.required(0, specs[0].location.clone())?;
        let occurrence = fields.required(1, specs[1].location.clone())?;
        fields.reject_unknown(container)?;
        Ok(RawEntrySource {
            structural_path,
            occurrence,
        })
    }

    fn admit_definitions(
        &self,
        raw: &[RawDefinitionEnvelope],
        decoded_bytes: &mut u64,
    ) -> Result<Vec<MessageDefinition>, ArtifactContractError> {
        // The five byte phases are complete, non-interleaved passes over
        // resource raw-entry order. Every admitted variable-width scalar is
        // charged once immediately after its field-specific limit succeeds.
        let mut locales = Vec::with_capacity(raw.len());
        for definition in raw {
            let location = definition_location(definition.ordinal, Some(DefinitionField::Locale));
            let locale = self.string(definition.locale, location)?;
            LinkLimitCounter::LocaleBytes.check_artifact_limit(locale.len() as u64, self.limits)?;
            self.charge_decoded(decoded_bytes, locale.len() as u64)?;
            locales.push(locale);
        }

        let mut scope_names = Vec::with_capacity(raw.len());
        for definition in raw {
            let location =
                definition_location(definition.ordinal, Some(DefinitionField::ScopeName));
            let name = self.string(definition.scope.name, location)?;
            LinkLimitCounter::CatalogScopeNameBytes
                .check_artifact_limit(name.len() as u64, self.limits)?;
            self.charge_decoded(decoded_bytes, name.len() as u64)?;
            scope_names.push(name);
        }

        let mut structural_paths = Vec::with_capacity(raw.len());
        for definition in raw {
            let location = definition_location(
                definition.ordinal,
                Some(DefinitionField::SourceStructuralPath),
            );
            let path = self.string(definition.source.structural_path, location)?;
            LinkLimitCounter::EntryStructuralPathBytes
                .check_artifact_limit(path.len() as u64, self.limits)?;
            self.charge_decoded(decoded_bytes, path.len() as u64)?;
            structural_paths.push(path);
        }

        let mut keys = Vec::with_capacity(raw.len());
        for definition in raw {
            let location = definition_location(definition.ordinal, Some(DefinitionField::Key));
            let key = self.string(definition.key, location)?;
            LinkLimitCounter::CatalogKeyBytes
                .check_artifact_limit(key.len() as u64, self.limits)?;
            self.charge_decoded(decoded_bytes, key.len() as u64)?;
            keys.push(key);
        }

        let mut messages = Vec::with_capacity(raw.len());
        let mut total_message_bytes = 0_u64;
        for definition in raw {
            let location = definition_location(definition.ordinal, Some(DefinitionField::Message));
            let message = self.string(definition.message, location)?;
            LinkLimitCounter::MessageBytes
                .check_artifact_limit(message.len() as u64, self.limits)?;
            total_message_bytes = total_message_bytes
                .checked_add(message.len() as u64)
                .expect("the definition and message ceilings cannot overflow u64");
            LinkLimitCounter::TotalMessageBytes
                .check_artifact_limit(total_message_bytes, self.limits)?;
            self.charge_decoded(decoded_bytes, message.len() as u64)?;
            messages.push(message);
        }

        // Scalar grammar is intentionally later than every byte pass.
        let locales = raw
            .iter()
            .zip(locales)
            .map(|(definition, locale)| {
                Locale::try_new(locale).map_err(|error| {
                    self.value_error(
                        &error,
                        definition_location(definition.ordinal, Some(DefinitionField::Locale)),
                        None,
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let scope_names = raw
            .iter()
            .zip(scope_names)
            .map(|(definition, name)| {
                CatalogScopeName::try_new(name).map_err(|error| {
                    self.value_error(
                        &error,
                        definition_location(definition.ordinal, Some(DefinitionField::ScopeName)),
                        None,
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Scope namespaces are checked before domains and key-bearing identity.
        let namespaces = raw
            .iter()
            .map(|definition| {
                self.decode_namespace(
                    definition.scope.namespace,
                    definition_location(definition.ordinal, Some(DefinitionField::ScopeNamespace)),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let domains = raw
            .iter()
            .map(|definition| self.decode_domain(definition.domain, definition.ordinal))
            .collect::<Result<Vec<_>, _>>()?;

        // Catalog keys parse structurally only after every earlier field pass.
        // Token limits are rechecked separately under the current lower budget.
        let checked_keys = raw
            .iter()
            .zip(domains.iter().copied())
            .zip(keys)
            .map(|((definition, domain), key)| {
                let location = definition_location(definition.ordinal, Some(DefinitionField::Key));
                let key = CatalogKey::try_new(domain, key).map_err(|error| {
                    self.value_error(&error, location, Some(LinkLimitCounter::CatalogKeyTokens))
                })?;
                key.revalidate_tokens(self.limits)?;
                Ok(key)
            })
            .collect::<Result<Vec<_>, ArtifactContractError>>()?;

        let entry_sources = raw
            .iter()
            .zip(structural_paths)
            .map(|(definition, structural_path)| {
                let path_location = definition_location(
                    definition.ordinal,
                    Some(DefinitionField::SourceStructuralPath),
                );
                let path = EntryStructuralPath::try_new(structural_path)
                    .map_err(|error| self.value_error(&error, path_location, None))?;
                let occurrence = self.unsigned_u32(
                    definition.source.occurrence,
                    definition_location(
                        definition.ordinal,
                        Some(DefinitionField::SourceOccurrence),
                    ),
                )?;
                Ok(EntryReference::new(path, occurrence))
            })
            .collect::<Result<Vec<_>, ArtifactContractError>>()?;
        let messages = raw
            .iter()
            .zip(messages)
            .map(|(definition, message)| {
                MessagePayload::try_new(message).map_err(|error| {
                    self.value_error(
                        &error,
                        definition_location(definition.ordinal, Some(DefinitionField::Message)),
                        None,
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut definitions = Vec::with_capacity(raw.len());
        for (index, raw_definition) in raw.iter().enumerate() {
            let scope = CatalogScopeId::new(namespaces[index], scope_names[index].clone());
            definitions.push(
                MessageDefinition::try_new(
                    scope,
                    domains[index],
                    checked_keys[index].clone(),
                    locales[index].clone(),
                    messages[index].clone(),
                    entry_sources[index].clone(),
                )
                .map_err(|_| {
                    invalid(
                        ArtifactViolationCode::InconsistentValue,
                        definition_location(raw_definition.ordinal, Some(DefinitionField::Key)),
                    )
                })?,
            );
        }
        Ok(definitions)
    }

    fn decode_domain(
        &self,
        id: NodeId,
        ordinal: u32,
    ) -> Result<CatalogKeyDomain, ArtifactContractError> {
        let location = definition_location(ordinal, Some(DefinitionField::Domain));
        match self.string(id, location.clone())? {
            "json-pointer" => Ok(CatalogKeyDomain::JsonPointer),
            "yaml-typed-path" => Ok(CatalogKeyDomain::YamlTypedPath),
            "xliff-1.2" => Ok(CatalogKeyDomain::Xliff12),
            "xliff-2" => Ok(CatalogKeyDomain::Xliff2),
            _ => Err(invalid(ArtifactViolationCode::UnknownTag, location)),
        }
    }

    fn check_path_bytes<'a>(
        &self,
        segments: impl Iterator<Item = &'a str>,
    ) -> Result<(), ArtifactContractError> {
        let mut total = 0_u64;
        for segment in segments {
            total = total
                .checked_add(segment.len() as u64)
                .expect("bounded path segments cannot overflow u64");
            LinkLimitCounter::PathBytes.check_artifact_limit(total, self.limits)?;
        }
        Ok(())
    }
}

impl<'document> SchemaDecoder<'document> for Decoder<'document> {
    fn document(&self) -> &'document JsonDocument {
        self.document
    }

    fn limits(&self) -> &LinkLimits {
        self.limits
    }
}

// Raw records retain only schema-admitted node identities. Their values are
// consumed later in fixed collection-wide passes.
struct RawDefinitionEnvelope {
    ordinal: u32,
    scope: RawScope,
    domain: NodeId,
    key: NodeId,
    locale: NodeId,
    message: NodeId,
    source: RawEntrySource,
}

struct RawScope {
    namespace: NodeId,
    name: NodeId,
}

struct RawEntrySource {
    structural_path: NodeId,
    occurrence: NodeId,
}

fn decode_fingerprint_digest(value: &str) -> Option<FingerprintDigest> {
    if value.len() != 64 {
        return None;
    }
    let mut bytes = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = lowercase_hex_nibble(pair[0])?;
        let low = lowercase_hex_nibble(pair[1])?;
        bytes[index] = (high << 4) | low;
    }
    Some(FingerprintDigest::new(bytes))
}

const fn lowercase_hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

fn write_artifact(
    sink: &mut impl JsonSink,
    artifact: &MessageDefinitionArtifact,
) -> Result<(), ArtifactContractError> {
    // Emit every member directly in the versioned canonical order.
    sink.write_bytes(br#"{"kind":"message-definition","version":{"major":"#)?;
    write_u16(sink, artifact.version().major())?;
    sink.write_bytes(br#","minor":"#)?;
    write_u16(sink, artifact.version().minor())?;
    sink.write_bytes(br#"},"producer":{"id":"#)?;
    write_json_string(sink, artifact.producer().id().as_str())?;
    sink.write_bytes(br#","revision":"#)?;
    write_json_string(sink, artifact.producer().revision().as_str())?;
    sink.write_bytes(br#"},"source":{"namespace":"#)?;
    write_namespace(sink, artifact.source().namespace())?;
    sink.write_bytes(br#","path":"#)?;
    write_path(sink, artifact.source().path())?;
    sink.write_bytes(br#"},"logicalAliases":["#)?;
    for (index, alias) in artifact.logical_aliases().iter().enumerate() {
        if index > 0 {
            sink.write_bytes(b",")?;
        }
        write_path(sink, alias)?;
    }
    sink.write_bytes(br#"],"inputFingerprint":{"algorithm":"#)?;
    write_json_string(sink, artifact.input_fingerprint().algorithm().as_str())?;
    sink.write_bytes(br#","digest":"#)?;
    write_digest(sink, artifact.input_fingerprint().digest())?;
    sink.write_bytes(br#"},"definitions":["#)?;
    for (index, definition) in artifact.definitions().iter().enumerate() {
        if index > 0 {
            sink.write_bytes(b",")?;
        }
        write_definition(sink, definition)?;
    }
    sink.write_bytes(b"]}")?;
    Ok(())
}

fn write_path(
    sink: &mut impl JsonSink,
    path: &PortableRelativePath,
) -> Result<(), ArtifactContractError> {
    write_string_array(
        sink,
        path.segments().iter().map(PortablePathSegment::as_str),
    )
}

fn write_digest(
    sink: &mut impl JsonSink,
    digest: &FingerprintDigest,
) -> Result<(), ArtifactContractError> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    sink.write_bytes(b"\"")?;
    for byte in digest.as_bytes() {
        sink.write_bytes(&[HEX[usize::from(byte >> 4)], HEX[usize::from(byte & 0x0f)]])?;
    }
    sink.write_bytes(b"\"")
}

fn write_definition(
    sink: &mut impl JsonSink,
    definition: &MessageDefinition,
) -> Result<(), ArtifactContractError> {
    sink.write_bytes(br#"{"scope":{"namespace":"#)?;
    write_namespace(sink, definition.scope().namespace())?;
    sink.write_bytes(br#","name":"#)?;
    write_json_string(sink, definition.scope().name().as_str())?;
    sink.write_bytes(br#"},"domain":"#)?;
    write_json_string(sink, definition.domain().as_str())?;
    sink.write_bytes(br#","key":"#)?;
    write_json_string(sink, definition.key().as_str())?;
    sink.write_bytes(br#","locale":"#)?;
    write_json_string(sink, definition.locale().as_str())?;
    sink.write_bytes(br#","message":"#)?;
    write_json_string(sink, definition.message().as_str())?;
    sink.write_bytes(br#","source":{"structuralPath":"#)?;
    write_json_string(sink, definition.source().structural_path().as_str())?;
    sink.write_bytes(br#","occurrence":"#)?;
    write_u32(sink, definition.source().occurrence())?;
    sink.write_bytes(b"}}")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::{self, Read};

    use super::{
        decode_definition_artifact, decode_definition_artifact_from_reader,
        encode_definition_artifact, ArtifactReadError,
    };
    use crate::{
        ArtifactContractError, ArtifactViolationCode, ArtifactViolationLocation,
        DefinitionEnvelopeField, DefinitionField, LinkLimitCounter, LinkLimitObservation,
        LinkLimits,
    };

    const ZERO: &[u8] = br#"{"kind":"message-definition","version":{"major":0,"minor":1},"producer":{"id":"dev.intlify/test","revision":"1"},"source":{"namespace":{"kind":"project"},"path":["messages.json"]},"logicalAliases":[],"inputFingerprint":{"algorithm":"blake3-256","digest":"0000000000000000000000000000000000000000000000000000000000000000"},"definitions":[]}"#;

    fn violation(
        error: ArtifactContractError,
    ) -> (ArtifactViolationCode, ArtifactViolationLocation) {
        let ArtifactContractError::InvalidArtifact(violation) = error else {
            panic!("expected invalid artifact");
        };
        (violation.code(), violation.location().clone())
    }

    #[test]
    fn zero_definition_artifact_round_trips_canonical_bytes() {
        let limits = LinkLimits::default();
        let artifact = decode_definition_artifact(ZERO, &limits).unwrap();
        assert!(artifact.definitions().is_empty());
        assert_eq!(
            encode_definition_artifact(&artifact, &limits)
                .unwrap()
                .as_ref(),
            ZERO
        );
    }

    #[test]
    fn malformed_fingerprint_precedes_definition_count_and_record_failures() {
        let input = ZERO.to_vec();
        let input = String::from_utf8(input)
            .unwrap()
            .replace("\"blake3-256\"", "\"BLAKE3-256\"")
            .replace("\"definitions\":[]", "\"definitions\":[null]");
        let (code, location) = violation(
            decode_definition_artifact(input.as_bytes(), &LinkLimits::default()).unwrap_err(),
        );
        assert_eq!(code, ArtifactViolationCode::UnknownTag);
        assert_eq!(
            location,
            ArtifactViolationLocation::Envelope(crate::ArtifactEnvelopeLocation::Definition {
                field: Some(DefinitionEnvelopeField::FingerprintAlgorithm),
            })
        );
    }

    #[test]
    fn complete_alias_grammar_pass_precedes_canonical_set_failures() {
        let input = String::from_utf8(ZERO.to_vec())
            .unwrap()
            .replace(
                "\"source\":{\"namespace\":{\"kind\":\"project\"},\"path\":[\"messages.json\"]}",
                "\"source\":{\"namespace\":{\"kind\":\"project\"},\"path\":[\"b\"]}",
            )
            .replace("\"logicalAliases\":[]", "\"logicalAliases\":[[\"b\"],[]]");
        let (code, location) = violation(
            decode_definition_artifact(input.as_bytes(), &LinkLimits::default()).unwrap_err(),
        );
        assert_eq!(code, ArtifactViolationCode::InvalidValueGrammar);
        assert_eq!(
            location,
            ArtifactViolationLocation::LogicalAlias {
                alias_ordinal: 1,
                segment_ordinal: None,
            }
        );
    }

    #[test]
    fn locale_byte_phase_precedes_later_record_key_grammar() {
        let input = ZERO.to_vec();
        let definition = r#"{"scope":{"namespace":{"kind":"project"},"name":"app"},"domain":"json-pointer","key":"not-a-pointer","locale":"ab","message":"","source":{"structuralPath":"","occurrence":0}}"#;
        let input = String::from_utf8(input).unwrap().replace(
            "\"definitions\":[]",
            &format!("\"definitions\":[{definition}]"),
        );
        let limits = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::LocaleBytes, 1)
            .unwrap();
        let error = decode_definition_artifact(input.as_bytes(), &limits).unwrap_err();
        let ArtifactContractError::Limit(evidence) = error else {
            panic!("expected locale limit failure");
        };
        assert_eq!(evidence.counter(), LinkLimitCounter::LocaleBytes);
        assert_eq!(evidence.observation(), LinkLimitObservation::Exact(2));
    }

    #[test]
    fn occurrence_integer_has_the_exact_definition_location() {
        let definition = r#"{"scope":{"namespace":{"kind":"project"},"name":"app"},"domain":"json-pointer","key":"","locale":"en","message":"","source":{"structuralPath":"","occurrence":-1}}"#;
        let input = String::from_utf8(ZERO.to_vec()).unwrap().replace(
            "\"definitions\":[]",
            &format!("\"definitions\":[{definition}]"),
        );
        let (code, location) = violation(
            decode_definition_artifact(input.as_bytes(), &LinkLimits::default()).unwrap_err(),
        );
        assert_eq!(code, ArtifactViolationCode::InvalidInteger);
        assert_eq!(
            location,
            ArtifactViolationLocation::Definition {
                ordinal: 0,
                field: Some(DefinitionField::SourceOccurrence),
            }
        );
    }

    struct OneByteReader<'a> {
        bytes: &'a [u8],
        offset: usize,
    }

    impl Read for OneByteReader<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if self.offset == self.bytes.len() {
                return Ok(0);
            }
            buffer[0] = self.bytes[self.offset];
            self.offset += 1;
            Ok(1)
        }
    }

    #[test]
    fn reader_chunking_matches_slice_and_stops_at_wire_limit() {
        let limits = LinkLimits::default();
        let expected = decode_definition_artifact(ZERO, &limits).unwrap();
        let mut reader = OneByteReader {
            bytes: ZERO,
            offset: 0,
        };
        assert_eq!(
            decode_definition_artifact_from_reader(&mut reader, &limits).unwrap(),
            expected
        );

        let limits = LinkLimits::default()
            .try_with_limit(
                LinkLimitCounter::DefinitionArtifactWireBytes,
                (ZERO.len() - 1) as u64,
            )
            .unwrap();
        let mut reader = OneByteReader {
            bytes: ZERO,
            offset: 0,
        };
        let error = decode_definition_artifact_from_reader(&mut reader, &limits).unwrap_err();
        let ArtifactReadError::Contract(ArtifactContractError::Limit(evidence)) = error else {
            panic!("expected a wire contract limit");
        };
        assert_eq!(
            evidence.counter(),
            LinkLimitCounter::DefinitionArtifactWireBytes
        );
        assert_eq!(reader.offset, ZERO.len());
    }
}
