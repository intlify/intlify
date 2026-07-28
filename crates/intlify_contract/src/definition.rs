// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Immutable message definitions and one-complete-source definition artifacts.
//!
//! This module owns checked definition composition, structural input-fingerprint
//! values, canonical physical-alias admission, occurrence continuity, complete
//! artifact revalidation, and the observable decoded-byte charge used by
//! codecs and later linker request admission.
//!
//! It does not parse host resources, compute or verify source fingerprints,
//! parse MF2 payloads, perform message linking, or encode artifact transport.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use crate::{
    ArtifactContractError, ArtifactVersion, ArtifactVersionEvidence, ArtifactVersionSupport,
    ArtifactViolation, ArtifactViolationCode, ArtifactViolationLocation, CatalogKey,
    CatalogKeyDomain, CatalogScopeId, DefinitionField, EntryReference, LinkLimitCounter,
    LinkLimits, Locale, MessagePayload, PortableRelativePath, ProducerIdentity,
    SourceDocumentIdentity,
};

const DEFINITION_VERSION_SUPPORT: ArtifactVersionSupport =
    ArtifactVersionSupport::Exact(ArtifactVersion::DRAFT_V0_1);

/// Fingerprint algorithm admitted by the mutable v0.1 definition contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FingerprintAlgorithm {
    /// Standard unkeyed BLAKE3 with its 256-bit output.
    Blake3_256,
}

impl FingerprintAlgorithm {
    /// Return the exact v0.1 wire tag.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Blake3_256 => "blake3-256",
        }
    }
}

/// Opaque 256-bit structural input-fingerprint digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FingerprintDigest([u8; 32]);

impl FingerprintDigest {
    /// Retain one complete digest produced over the canonical fingerprint input.
    #[must_use]
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Return the exact opaque digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Structural freshness evidence for one complete definition source.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InputFingerprint {
    algorithm: FingerprintAlgorithm,
    digest: FingerprintDigest,
}

impl InputFingerprint {
    /// Construct a fingerprint from its explicitly selected algorithm and digest.
    #[must_use]
    pub const fn new(algorithm: FingerprintAlgorithm, digest: FingerprintDigest) -> Self {
        Self { algorithm, digest }
    }

    /// Return the exact fingerprint algorithm.
    #[must_use]
    pub const fn algorithm(&self) -> FingerprintAlgorithm {
        self.algorithm
    }

    /// Return the opaque digest.
    #[must_use]
    pub const fn digest(&self) -> &FingerprintDigest {
        &self.digest
    }
}

/// Cross-field failure returned while constructing one checked definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessageDefinitionConstructionError {
    /// The canonical key was checked under a different catalog-key domain.
    KeyDomainMismatch,
}

impl fmt::Display for MessageDefinitionConstructionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::KeyDomainMismatch => {
                formatter.write_str("catalog key domain does not match the definition domain")
            }
        }
    }
}

impl Error for MessageDefinitionConstructionError {}

/// One immutable catalog message definition and its source-entry evidence.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MessageDefinition {
    scope: CatalogScopeId,
    domain: CatalogKeyDomain,
    key: CatalogKey,
    locale: Locale,
    message: MessagePayload,
    source: EntryReference,
}

impl MessageDefinition {
    /// Construct one complete definition after checking its domain-qualified key.
    pub fn try_new(
        scope: CatalogScopeId,
        domain: CatalogKeyDomain,
        key: CatalogKey,
        locale: Locale,
        message: MessagePayload,
        source: EntryReference,
    ) -> Result<Self, MessageDefinitionConstructionError> {
        if key.domain() != domain {
            return Err(MessageDefinitionConstructionError::KeyDomainMismatch);
        }
        Ok(Self {
            scope,
            domain,
            key,
            locale,
            message,
            source,
        })
    }

    /// Return the exact declared catalog scope.
    #[must_use]
    pub const fn scope(&self) -> &CatalogScopeId {
        &self.scope
    }

    /// Return the catalog-key comparison domain.
    #[must_use]
    pub const fn domain(&self) -> CatalogKeyDomain {
        self.domain
    }

    /// Return the canonical domain-qualified catalog key.
    #[must_use]
    pub const fn key(&self) -> &CatalogKey {
        &self.key
    }

    /// Return the exact required locale.
    #[must_use]
    pub const fn locale(&self) -> &Locale {
        &self.locale
    }

    /// Return the exact opaque MF2 source payload.
    #[must_use]
    pub const fn message(&self) -> &MessagePayload {
        &self.message
    }

    /// Return the portable source-entry evidence.
    #[must_use]
    pub const fn source(&self) -> &EntryReference {
        &self.source
    }
}

/// One complete immutable definition artifact for one physical-source group.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MessageDefinitionArtifact {
    version: ArtifactVersion,
    producer: ProducerIdentity,
    source: SourceDocumentIdentity,
    logical_aliases: Box<[PortableRelativePath]>,
    input_fingerprint: InputFingerprint,
    definitions: Box<[MessageDefinition]>,
    decoded_bytes: u64,
}

impl MessageDefinitionArtifact {
    /// Validate and retain one complete successful source-group projection.
    pub fn try_new(
        version: ArtifactVersion,
        producer: ProducerIdentity,
        source: SourceDocumentIdentity,
        logical_aliases: Vec<PortableRelativePath>,
        input_fingerprint: InputFingerprint,
        definitions: Vec<MessageDefinition>,
        limits: &LinkLimits,
    ) -> Result<Self, ArtifactContractError> {
        let decoded_bytes = validate_parts(
            version,
            &producer,
            &source,
            &logical_aliases,
            &input_fingerprint,
            &definitions,
            limits,
        )?;
        Ok(Self {
            version,
            producer,
            source,
            logical_aliases: logical_aliases.into_boxed_slice(),
            input_fingerprint,
            definitions: definitions.into_boxed_slice(),
            decoded_bytes,
        })
    }

    /// Return the normalized artifact contract version.
    #[must_use]
    pub const fn version(&self) -> &ArtifactVersion {
        &self.version
    }

    /// Return the complete producer provenance.
    #[must_use]
    pub const fn producer(&self) -> &ProducerIdentity {
        &self.producer
    }

    /// Return the canonical primary source identity.
    #[must_use]
    pub const fn source(&self) -> &SourceDocumentIdentity {
        &self.source
    }

    /// Return canonical physical-alias evidence without duplicating definitions.
    #[must_use]
    pub const fn logical_aliases(&self) -> &[PortableRelativePath] {
        &self.logical_aliases
    }

    /// Return structural source-input freshness evidence.
    #[must_use]
    pub const fn input_fingerprint(&self) -> &InputFingerprint {
        &self.input_fingerprint
    }

    /// Return definitions in the preserved resource raw-entry order.
    #[must_use]
    pub const fn definitions(&self) -> &[MessageDefinition] {
        &self.definitions
    }

    /// Return the exact observable variable-width decoded payload charge.
    #[must_use]
    pub const fn decoded_bytes(&self) -> u64 {
        self.decoded_bytes
    }

    /// Revalidate this complete artifact under current caller-selected limits.
    pub fn revalidate(&self, limits: &LinkLimits) -> Result<(), ArtifactContractError> {
        validate_parts(
            self.version,
            &self.producer,
            &self.source,
            &self.logical_aliases,
            &self.input_fingerprint,
            &self.definitions,
            limits,
        )
        .map(|_| ())
    }
}

fn validate_parts(
    version: ArtifactVersion,
    producer: &ProducerIdentity,
    source: &SourceDocumentIdentity,
    logical_aliases: &[PortableRelativePath],
    input_fingerprint: &InputFingerprint,
    definitions: &[MessageDefinition],
    limits: &LinkLimits,
) -> Result<u64, ArtifactContractError> {
    if !DEFINITION_VERSION_SUPPORT.supports(version) {
        return Err(ArtifactContractError::UnsupportedVersion(
            ArtifactVersionEvidence::for_unsupported(version, DEFINITION_VERSION_SUPPORT),
        ));
    }

    // These collection-wide passes mirror the definition wire decoder. Keeping
    // them explicit prevents a record-local validation strategy from changing
    // which failure is observable when several fields are invalid.
    let mut decoded_bytes = 0_u64;
    charge_decoded(
        &mut decoded_bytes,
        producer.id().as_str().len() as u64,
        limits,
    )?;
    charge_decoded(
        &mut decoded_bytes,
        producer.revision().as_str().len() as u64,
        limits,
    )?;

    validate_primary_path(source.path(), &mut decoded_bytes, limits)?;
    validate_aliases(source, logical_aliases, &mut decoded_bytes, limits)?;

    // The fixed algorithm adds no variable-width charge; the opaque digest
    // contributes its decoded 32 bytes exactly once after alias admission.
    charge_decoded(
        &mut decoded_bytes,
        input_fingerprint.digest().as_bytes().len() as u64,
        limits,
    )?;

    LinkLimitCounter::Definitions.check_artifact_limit(definitions.len() as u64, limits)?;

    for definition in definitions {
        definition.locale.revalidate_limit(limits)?;
        charge_decoded(
            &mut decoded_bytes,
            definition.locale.as_str().len() as u64,
            limits,
        )?;
    }
    for definition in definitions {
        definition.scope.name().revalidate_limit(limits)?;
        charge_decoded(
            &mut decoded_bytes,
            definition.scope.name().as_str().len() as u64,
            limits,
        )?;
    }
    for definition in definitions {
        definition
            .source
            .structural_path()
            .revalidate_limit(limits)?;
        charge_decoded(
            &mut decoded_bytes,
            definition.source.structural_path().as_str().len() as u64,
            limits,
        )?;
    }
    for definition in definitions {
        definition.key.revalidate_definition_bytes(limits)?;
        charge_decoded(
            &mut decoded_bytes,
            definition.key.as_str().len() as u64,
            limits,
        )?;
    }

    let mut total_message_bytes = 0_u64;
    for definition in definitions {
        definition.message.revalidate_limit(limits)?;
        total_message_bytes = total_message_bytes
            .checked_add(definition.message.as_str().len() as u64)
            .expect("the admitted definition count and message ceiling cannot overflow u64");
        LinkLimitCounter::TotalMessageBytes.check_artifact_limit(total_message_bytes, limits)?;
        charge_decoded(
            &mut decoded_bytes,
            definition.message.as_str().len() as u64,
            limits,
        )?;
    }

    for definition in definitions {
        definition.key.revalidate_tokens(limits)?;
    }
    validate_occurrence_continuity(definitions)?;
    Ok(decoded_bytes)
}

fn validate_primary_path(
    path: &PortableRelativePath,
    decoded_bytes: &mut u64,
    limits: &LinkLimits,
) -> Result<(), ArtifactContractError> {
    // Primary paths use their complete numeric phases before any alias phase.
    LinkLimitCounter::PathSegments.check_artifact_limit(path.segments().len() as u64, limits)?;
    for segment in path.segments() {
        LinkLimitCounter::PathSegmentBytes
            .check_artifact_limit(segment.as_str().len() as u64, limits)?;
        charge_decoded(decoded_bytes, segment.as_str().len() as u64, limits)?;
    }
    validate_path_running_bytes(path, limits)
}

fn validate_aliases(
    source: &SourceDocumentIdentity,
    aliases: &[PortableRelativePath],
    decoded_bytes: &mut u64,
    limits: &LinkLimits,
) -> Result<(), ArtifactContractError> {
    LinkLimitCounter::LogicalAliases.check_artifact_limit(aliases.len() as u64, limits)?;

    // Alias numeric checks are deliberately non-interleaved across the set.
    for alias in aliases {
        LinkLimitCounter::PathSegments
            .check_artifact_limit(alias.segments().len() as u64, limits)?;
    }
    for alias in aliases {
        for segment in alias.segments() {
            LinkLimitCounter::PathSegmentBytes
                .check_artifact_limit(segment.as_str().len() as u64, limits)?;
            charge_decoded(decoded_bytes, segment.as_str().len() as u64, limits)?;
        }
    }
    for alias in aliases {
        validate_path_running_bytes(alias, limits)?;
    }

    let mut source_path_bytes = path_bytes(source.path());
    LinkLimitCounter::SourcePathBytes.check_artifact_limit(source_path_bytes, limits)?;
    for alias in aliases {
        source_path_bytes = source_path_bytes
            .checked_add(path_bytes(alias))
            .expect("bounded source paths cannot overflow u64");
        LinkLimitCounter::SourcePathBytes.check_artifact_limit(source_path_bytes, limits)?;
    }

    // PortableRelativePath values are already grammar-checked. The remaining
    // semantic pass must compare the primary and aliases without sorting them.
    let mut previous = source.path();
    for (ordinal, alias) in aliases.iter().enumerate() {
        let location = ArtifactViolationLocation::LogicalAlias {
            alias_ordinal: u32::try_from(ordinal)
                .expect("the admitted alias ceiling is below u32::MAX"),
            segment_ordinal: None,
        };
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
    Ok(())
}

fn validate_path_running_bytes(
    path: &PortableRelativePath,
    limits: &LinkLimits,
) -> Result<(), ArtifactContractError> {
    let mut total = 0_u64;
    for segment in path.segments() {
        total = total
            .checked_add(segment.as_str().len() as u64)
            .expect("bounded path segments cannot overflow u64");
        LinkLimitCounter::PathBytes.check_artifact_limit(total, limits)?;
    }
    Ok(())
}

fn path_bytes(path: &PortableRelativePath) -> u64 {
    path.segments()
        .iter()
        .map(|segment| segment.as_str().len() as u64)
        .sum()
}

fn validate_occurrence_continuity(
    definitions: &[MessageDefinition],
) -> Result<(), ArtifactContractError> {
    let mut next_by_path: BTreeMap<&str, u32> = BTreeMap::new();
    for (ordinal, definition) in definitions.iter().enumerate() {
        let path = definition.source.structural_path().as_str();
        let expected = next_by_path.get(path).copied().unwrap_or(0);
        if definition.source.occurrence() != expected {
            return Err(invalid(
                ArtifactViolationCode::DiscontinuousOccurrence,
                ArtifactViolationLocation::Definition {
                    ordinal: u32::try_from(ordinal)
                        .expect("the admitted definition ceiling is below u32::MAX"),
                    field: Some(DefinitionField::SourceOccurrence),
                },
            ));
        }
        let Some(next) = expected.checked_add(1) else {
            return Err(invalid(
                ArtifactViolationCode::DiscontinuousOccurrence,
                ArtifactViolationLocation::Definition {
                    ordinal: u32::try_from(ordinal)
                        .expect("the admitted definition ceiling is below u32::MAX"),
                    field: Some(DefinitionField::SourceOccurrence),
                },
            ));
        };
        next_by_path.insert(path, next);
    }
    Ok(())
}

fn charge_decoded(
    total: &mut u64,
    addend: u64,
    limits: &LinkLimits,
) -> Result<(), ArtifactContractError> {
    let attempted = total
        .checked_add(addend)
        .expect("bounded definition payload additions cannot overflow u64");
    LinkLimitCounter::DefinitionArtifactDecodedBytes.check_artifact_limit(attempted, limits)?;
    *total = attempted;
    Ok(())
}

fn invalid(
    code: ArtifactViolationCode,
    location: ArtifactViolationLocation,
) -> ArtifactContractError {
    ArtifactContractError::InvalidArtifact(ArtifactViolation::new(code, location))
}

#[cfg(test)]
mod tests {
    use super::{
        FingerprintAlgorithm, FingerprintDigest, InputFingerprint, MessageDefinition,
        MessageDefinitionArtifact, MessageDefinitionConstructionError,
    };
    use crate::{
        ArtifactContractError, ArtifactNamespace, ArtifactVersion, CatalogKey, CatalogKeyDomain,
        CatalogScopeId, CatalogScopeName, EntryReference, EntryStructuralPath, LinkLimitCounter,
        LinkLimitObservation, LinkLimits, Locale, MessagePayload, PortablePathSegment,
        PortableRelativePath, ProducerId, ProducerIdentity, ProducerRevision,
        SourceDocumentIdentity,
    };

    fn path(value: &str) -> PortableRelativePath {
        PortableRelativePath::try_new(vec![PortablePathSegment::try_new(value).unwrap()]).unwrap()
    }

    fn producer() -> ProducerIdentity {
        ProducerIdentity::new(
            ProducerId::try_new("dev.intlify/test").unwrap(),
            ProducerRevision::try_new("1").unwrap(),
        )
    }

    fn fingerprint() -> InputFingerprint {
        InputFingerprint::new(
            FingerprintAlgorithm::Blake3_256,
            FingerprintDigest::new([0; 32]),
        )
    }

    fn definition(domain: CatalogKeyDomain, key_domain: CatalogKeyDomain) -> MessageDefinition {
        MessageDefinition::try_new(
            CatalogScopeId::new(
                ArtifactNamespace::Project,
                CatalogScopeName::try_new("app").unwrap(),
            ),
            domain,
            CatalogKey::try_new(key_domain, "").unwrap(),
            Locale::try_new("en").unwrap(),
            MessagePayload::try_new("").unwrap(),
            EntryReference::new(EntryStructuralPath::try_new("").unwrap(), 0),
        )
        .unwrap()
    }

    #[test]
    fn definition_rejects_a_key_from_another_domain() {
        let result = MessageDefinition::try_new(
            CatalogScopeId::new(
                ArtifactNamespace::Project,
                CatalogScopeName::try_new("app").unwrap(),
            ),
            CatalogKeyDomain::YamlTypedPath,
            CatalogKey::try_new(CatalogKeyDomain::JsonPointer, "").unwrap(),
            Locale::try_new("en").unwrap(),
            MessagePayload::try_new("").unwrap(),
            EntryReference::new(EntryStructuralPath::try_new("").unwrap(), 0),
        );
        assert_eq!(
            result.unwrap_err(),
            MessageDefinitionConstructionError::KeyDomainMismatch
        );
    }

    #[test]
    fn zero_definition_artifact_retains_fingerprint_and_charge() {
        let artifact = MessageDefinitionArtifact::try_new(
            ArtifactVersion::DRAFT_V0_1,
            producer(),
            SourceDocumentIdentity::new(ArtifactNamespace::Project, path("messages.json")),
            Vec::new(),
            fingerprint(),
            Vec::new(),
            &LinkLimits::default(),
        )
        .unwrap();
        assert!(artifact.definitions().is_empty());
        assert_eq!(artifact.input_fingerprint().digest().as_bytes(), &[0; 32]);
        assert!(artifact.decoded_bytes() >= 32);
    }

    #[test]
    fn lower_message_total_preserves_running_sum_evidence() {
        let limits = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::TotalMessageBytes, 0)
            .unwrap();
        let definition = MessageDefinition::try_new(
            CatalogScopeId::new(
                ArtifactNamespace::Project,
                CatalogScopeName::try_new("app").unwrap(),
            ),
            CatalogKeyDomain::JsonPointer,
            CatalogKey::try_new(CatalogKeyDomain::JsonPointer, "").unwrap(),
            Locale::try_new("en").unwrap(),
            MessagePayload::try_new("a").unwrap(),
            EntryReference::new(EntryStructuralPath::try_new("").unwrap(), 0),
        )
        .unwrap();
        let error = MessageDefinitionArtifact::try_new(
            ArtifactVersion::DRAFT_V0_1,
            producer(),
            SourceDocumentIdentity::new(ArtifactNamespace::Project, path("messages.json")),
            Vec::new(),
            fingerprint(),
            vec![definition],
            &limits,
        )
        .unwrap_err();
        let ArtifactContractError::Limit(evidence) = error else {
            panic!("expected a total-message limit failure");
        };
        assert_eq!(evidence.counter(), LinkLimitCounter::TotalMessageBytes);
        assert_eq!(evidence.observation(), LinkLimitObservation::Exact(1));
    }

    #[test]
    fn checked_key_and_definition_domain_stay_equal() {
        let definition = definition(CatalogKeyDomain::JsonPointer, CatalogKeyDomain::JsonPointer);
        assert_eq!(definition.domain(), definition.key().domain());
    }
}
