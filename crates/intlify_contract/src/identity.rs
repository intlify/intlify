// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Checked identities shared by artifact producers and linker consumers.
//!
//! This module owns artifact version and namespace values, producer provenance,
//! catalog-scope identities, reference-artifact identities, and delivery-unit
//! identities. Constructors preserve exact spellings, enforce the contract
//! grammar and size ceilings, and provide canonical fingerprint payloads.
//!
//! It does not discover producers, resolve catalog scopes, construct delivery
//! graphs, or decide whether two independently supplied identities refer to the
//! same external entity beyond their defined value equality.

use crate::error::{ValueConstructionError, ValueGrammar};
use crate::fingerprint::{write_sequence, write_tagged_field, FingerprintPayload};
use crate::{ArtifactLimitEvidence, LinkLimitCounter, LinkLimitObservation, LinkLimits};

const PRODUCER_ID_BYTES: usize = 255;
const PRODUCER_REVISION_BYTES: usize = 128;
const CATALOG_SCOPE_NAME_BYTES: usize = 255;
const LOGICAL_SEGMENT_BYTES: usize = 255;
const LOGICAL_SEGMENTS: usize = 64;
const LOGICAL_IDENTITY_BYTES: u64 = 4_096;

/// Artifact wire-contract version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ArtifactVersion {
    major: u16,
    minor: u16,
}

impl ArtifactVersion {
    /// Current mutable draft version emitted by artifact writers.
    pub const DRAFT_V0_1: Self = Self::new(0, 1);

    /// Construct one artifact version from its unsigned components.
    #[must_use]
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }

    /// Return the major component.
    #[must_use]
    pub const fn major(self) -> u16 {
        self.major
    }

    /// Return the minor component.
    #[must_use]
    pub const fn minor(self) -> u16 {
        self.minor
    }
}

impl FingerprintPayload for ArtifactVersion {
    fn write_fingerprint_payload(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(&self.major.to_be_bytes());
        output.extend_from_slice(&self.minor.to_be_bytes());
    }
}

/// Portable namespace for application-owned artifacts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ArtifactNamespace {
    /// Identity is contextual to the consuming project.
    Project,
}

impl ArtifactNamespace {
    /// Return the exact v0.1 wire tag.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
        }
    }

    pub(crate) const fn fingerprint_discriminant(self) -> u8 {
        match self {
            Self::Project => 0,
        }
    }
}

impl FingerprintPayload for ArtifactNamespace {
    fn write_fingerprint_payload(&self, output: &mut Vec<u8>) {
        output.push(self.fingerprint_discriminant());
    }
}

/// Stable producer implementation-family identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProducerId(Box<str>);

impl ProducerId {
    /// Validate and retain an exact `<reverse-dns>/<producer-slug>` identifier.
    pub fn try_new(value: impl Into<Box<str>>) -> Result<Self, ValueConstructionError> {
        let value = value.into();
        if value.is_empty() || value.len() > PRODUCER_ID_BYTES || !value.is_ascii() {
            return Err(ValueConstructionError::Grammar(ValueGrammar::Invalid));
        }

        let Some((domain, slug)) = value.split_once('/') else {
            return Err(ValueConstructionError::Grammar(ValueGrammar::Invalid));
        };
        if slug.contains('/') || !valid_reverse_dns(domain) || !valid_slug(slug) {
            return Err(ValueConstructionError::Grammar(ValueGrammar::Invalid));
        }

        Ok(Self(value))
    }

    /// Return the exact validated identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FingerprintPayload for ProducerId {
    fn write_fingerprint_payload(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(self.0.as_bytes());
    }
}

/// Opaque deterministic revision of one artifact-producing build.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProducerRevision(Box<str>);

impl ProducerRevision {
    /// Validate and retain an exact producer revision.
    pub fn try_new(value: impl Into<Box<str>>) -> Result<Self, ValueConstructionError> {
        let value = value.into();
        let bytes = value.as_bytes();
        if bytes.is_empty()
            || bytes.len() > PRODUCER_REVISION_BYTES
            || !bytes[0].is_ascii_alphanumeric()
            || !bytes.iter().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'+' | b'-')
            })
        {
            return Err(ValueConstructionError::Grammar(ValueGrammar::Invalid));
        }
        Ok(Self(value))
    }

    /// Return the exact validated revision.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FingerprintPayload for ProducerRevision {
    fn write_fingerprint_payload(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(self.0.as_bytes());
    }
}

/// Complete artifact producer provenance.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProducerIdentity {
    id: ProducerId,
    revision: ProducerRevision,
}

impl ProducerIdentity {
    /// Construct provenance from independently checked components.
    #[must_use]
    pub const fn new(id: ProducerId, revision: ProducerRevision) -> Self {
        Self { id, revision }
    }

    /// Return the stable implementation-family identifier.
    #[must_use]
    pub const fn id(&self) -> &ProducerId {
        &self.id
    }

    /// Return the exact implementation/build revision.
    #[must_use]
    pub const fn revision(&self) -> &ProducerRevision {
        &self.revision
    }
}

impl FingerprintPayload for ProducerIdentity {
    fn write_fingerprint_payload(&self, output: &mut Vec<u8>) {
        write_tagged_field(0x01, self.id.as_str().as_bytes(), output);
        write_tagged_field(0x02, self.revision.as_str().as_bytes(), output);
    }
}

/// Exact application-local catalog scope name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CatalogScopeName(Box<str>);

impl CatalogScopeName {
    /// Validate and retain a non-empty scope name of at most 255 UTF-8 bytes.
    pub fn try_new(value: impl Into<Box<str>>) -> Result<Self, ValueConstructionError> {
        let value = value.into();
        if value.is_empty() {
            return Err(ValueConstructionError::Grammar(ValueGrammar::Empty));
        }
        if value.len() > CATALOG_SCOPE_NAME_BYTES {
            return Err(ValueConstructionError::field_limit(
                crate::LinkLimitCounter::CatalogScopeNameBytes,
                CATALOG_SCOPE_NAME_BYTES as u64,
                CATALOG_SCOPE_NAME_BYTES as u64 + 1,
            ));
        }
        Ok(Self(value))
    }

    /// Return the exact scope spelling.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[allow(dead_code)]
    pub(crate) fn revalidate_limit(
        &self,
        limits: &LinkLimits,
    ) -> Result<(), ArtifactLimitEvidence> {
        let counter = crate::LinkLimitCounter::CatalogScopeNameBytes;
        let limit = limits.effective_limit(counter);
        if self.0.len() as u64 > limit {
            return Err(ArtifactLimitEvidence::try_new(
                counter,
                limit,
                LinkLimitObservation::Exact(limit + 1),
            )
            .expect("checked first-over scope-name evidence is valid"));
        }
        Ok(())
    }
}

impl FingerprintPayload for CatalogScopeName {
    fn write_fingerprint_payload(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(self.0.as_bytes());
    }
}

/// Portable project-local catalog scope identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CatalogScopeId {
    namespace: ArtifactNamespace,
    name: CatalogScopeName,
}

impl CatalogScopeId {
    /// Construct a scope from independently checked components.
    #[must_use]
    pub const fn new(namespace: ArtifactNamespace, name: CatalogScopeName) -> Self {
        Self { namespace, name }
    }

    /// Return the explicit artifact namespace.
    #[must_use]
    pub const fn namespace(&self) -> ArtifactNamespace {
        self.namespace
    }

    /// Return the exact scope name.
    #[must_use]
    pub const fn name(&self) -> &CatalogScopeName {
        &self.name
    }
}

impl FingerprintPayload for CatalogScopeId {
    fn write_fingerprint_payload(&self, output: &mut Vec<u8>) {
        let namespace = [self.namespace.fingerprint_discriminant()];
        write_tagged_field(0x01, &namespace, output);
        write_tagged_field(0x02, self.name.as_str().as_bytes(), output);
    }
}

macro_rules! logical_segment {
    ($name:ident, $description:literal, $counter:expr) => {
        #[doc = $description]
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(Box<str>);

        impl $name {
            /// Validate and retain one exact logical segment.
            pub fn try_new(value: impl Into<Box<str>>) -> Result<Self, ValueConstructionError> {
                let value = value.into();
                validate_logical_segment(&value, $counter)?;
                Ok(Self(value))
            }

            /// Return the exact segment spelling.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

logical_segment!(
    ReferenceArtifactSegment,
    "One logical segment in a reference-artifact identity.",
    LinkLimitCounter::ReferenceIdentitySegmentBytes
);
logical_segment!(
    DeliveryUnitSegment,
    "One logical segment in a delivery-unit identity.",
    LinkLimitCounter::DeliveryUnitSegmentBytes
);

/// Non-empty application-local logical reference-artifact identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReferenceArtifactIdentity {
    namespace: ArtifactNamespace,
    segments: Box<[ReferenceArtifactSegment]>,
}

impl ReferenceArtifactIdentity {
    /// Validate and retain an ordered logical segment sequence.
    pub fn try_new(
        namespace: ArtifactNamespace,
        segments: Vec<ReferenceArtifactSegment>,
    ) -> Result<Self, ValueConstructionError> {
        validate_identity_segments(
            &segments,
            LinkLimitCounter::ReferenceIdentitySegments,
            LinkLimitCounter::ReferenceIdentityBytes,
        )?;
        Ok(Self {
            namespace,
            segments: segments.into_boxed_slice(),
        })
    }

    /// Return the explicit artifact namespace.
    #[must_use]
    pub const fn namespace(&self) -> ArtifactNamespace {
        self.namespace
    }

    /// Return the exact ordered logical segments.
    #[must_use]
    pub fn segments(&self) -> &[ReferenceArtifactSegment] {
        &self.segments
    }

    #[allow(dead_code)]
    pub(crate) fn decoded_bytes(&self) -> u64 {
        self.segments
            .iter()
            .map(|segment| segment.as_str().len() as u64)
            .sum()
    }

    #[allow(dead_code)]
    pub(crate) fn revalidate_limits(
        &self,
        limits: &LinkLimits,
    ) -> Result<(), ArtifactLimitEvidence> {
        revalidate_identity_segments(
            &self.segments,
            LinkLimitCounter::ReferenceIdentitySegmentBytes,
            LinkLimitCounter::ReferenceIdentitySegments,
            LinkLimitCounter::ReferenceIdentityBytes,
            limits,
        )
    }
}

impl FingerprintPayload for ReferenceArtifactIdentity {
    fn write_fingerprint_payload(&self, output: &mut Vec<u8>) {
        let namespace = [self.namespace.fingerprint_discriminant()];
        write_tagged_field(0x01, &namespace, output);
        let mut segments = Vec::new();
        write_sequence(
            self.segments
                .iter()
                .map(|segment| segment.as_str().as_bytes()),
            &mut segments,
        );
        write_tagged_field(0x02, &segments, output);
    }
}

/// Non-empty application-local delivery graph node identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeliveryUnitId(Box<[DeliveryUnitSegment]>);

impl DeliveryUnitId {
    /// Validate and retain an ordered delivery-unit segment sequence.
    pub fn try_new(segments: Vec<DeliveryUnitSegment>) -> Result<Self, ValueConstructionError> {
        validate_identity_segments(
            &segments,
            LinkLimitCounter::DeliveryUnitSegments,
            LinkLimitCounter::DeliveryUnitBytes,
        )?;
        Ok(Self(segments.into_boxed_slice()))
    }

    /// Construct the built-in one-node delivery-unit identity `["main"]`.
    pub fn main() -> Self {
        Self::try_new(vec![
            DeliveryUnitSegment::try_new("main").expect("the built-in main segment is valid")
        ])
        .expect("the built-in main delivery unit is valid")
    }

    /// Return the exact ordered logical segments.
    #[must_use]
    pub fn segments(&self) -> &[DeliveryUnitSegment] {
        &self.0
    }

    #[allow(dead_code)]
    pub(crate) fn decoded_bytes(&self) -> u64 {
        self.0
            .iter()
            .map(|segment| segment.as_str().len() as u64)
            .sum()
    }

    #[allow(dead_code)]
    pub(crate) fn revalidate_limits(
        &self,
        limits: &LinkLimits,
    ) -> Result<(), ArtifactLimitEvidence> {
        revalidate_identity_segments(
            &self.0,
            LinkLimitCounter::DeliveryUnitSegmentBytes,
            LinkLimitCounter::DeliveryUnitSegments,
            LinkLimitCounter::DeliveryUnitBytes,
            limits,
        )
    }
}

impl FingerprintPayload for DeliveryUnitId {
    fn write_fingerprint_payload(&self, output: &mut Vec<u8>) {
        write_sequence(
            self.0.iter().map(|segment| segment.as_str().as_bytes()),
            output,
        );
    }
}

trait LogicalSegment {
    fn logical_segment(&self) -> &str;
}

impl LogicalSegment for ReferenceArtifactSegment {
    fn logical_segment(&self) -> &str {
        self.as_str()
    }
}

impl LogicalSegment for DeliveryUnitSegment {
    fn logical_segment(&self) -> &str {
        self.as_str()
    }
}

fn validate_logical_segment(
    value: &str,
    segment_counter: LinkLimitCounter,
) -> Result<(), ValueConstructionError> {
    if value.len() > LOGICAL_SEGMENT_BYTES {
        return Err(ValueConstructionError::field_limit(
            segment_counter,
            LOGICAL_SEGMENT_BYTES as u64,
            LOGICAL_SEGMENT_BYTES as u64 + 1,
        ));
    }
    if value.is_empty() {
        return Err(ValueConstructionError::Grammar(ValueGrammar::Empty));
    }
    if matches!(value, "." | "..") || value.contains('\0') || value.contains('/') {
        return Err(ValueConstructionError::Grammar(ValueGrammar::Invalid));
    }
    Ok(())
}

fn validate_identity_segments<T: LogicalSegment>(
    segments: &[T],
    segment_count_counter: LinkLimitCounter,
    identity_bytes_counter: LinkLimitCounter,
) -> Result<(), ValueConstructionError> {
    if segments.is_empty() {
        return Err(ValueConstructionError::Grammar(ValueGrammar::Empty));
    }
    if segments.len() > LOGICAL_SEGMENTS {
        return Err(ValueConstructionError::field_limit(
            segment_count_counter,
            LOGICAL_SEGMENTS as u64,
            LOGICAL_SEGMENTS as u64 + 1,
        ));
    }

    let mut total = 0_u64;
    for segment in segments {
        let length = segment.logical_segment().len() as u64;
        total = total
            .checked_add(length)
            .unwrap_or(LOGICAL_IDENTITY_BYTES + 1);
        if total > LOGICAL_IDENTITY_BYTES {
            return Err(ValueConstructionError::field_limit(
                identity_bytes_counter,
                LOGICAL_IDENTITY_BYTES,
                total,
            ));
        }
    }
    Ok(())
}

fn revalidate_identity_segments<T: LogicalSegment>(
    segments: &[T],
    segment_bytes_counter: LinkLimitCounter,
    segment_count_counter: LinkLimitCounter,
    identity_bytes_counter: LinkLimitCounter,
    limits: &LinkLimits,
) -> Result<(), ArtifactLimitEvidence> {
    let segment_count_limit = limits.effective_limit(segment_count_counter);
    if segments.len() as u64 > segment_count_limit {
        return Err(ArtifactLimitEvidence::try_new(
            segment_count_counter,
            segment_count_limit,
            LinkLimitObservation::Exact(segment_count_limit + 1),
        )
        .expect("checked first-over identity segment-count evidence is valid"));
    }

    let segment_bytes_limit = limits.effective_limit(segment_bytes_counter);
    let identity_bytes_limit = limits.effective_limit(identity_bytes_counter);
    let mut total = 0_u64;
    for segment in segments {
        let length = segment.logical_segment().len() as u64;
        if length > segment_bytes_limit {
            return Err(ArtifactLimitEvidence::try_new(
                segment_bytes_counter,
                segment_bytes_limit,
                LinkLimitObservation::Exact(segment_bytes_limit + 1),
            )
            .expect("checked first-over identity segment-byte evidence is valid"));
        }
        total = total
            .checked_add(length)
            .expect("checked logical identity components cannot overflow u64");
        if total > identity_bytes_limit {
            return Err(ArtifactLimitEvidence::try_new(
                identity_bytes_counter,
                identity_bytes_limit,
                LinkLimitObservation::Exact(total),
            )
            .expect("checked running identity-byte evidence is valid"));
        }
    }
    Ok(())
}

fn valid_reverse_dns(value: &str) -> bool {
    let mut labels = value.split('.');
    let Some(first) = labels.next() else {
        return false;
    };
    let Some(second) = labels.next() else {
        return false;
    };
    valid_dns_label(first) && valid_dns_label(second) && labels.all(valid_dns_label)
}

fn valid_dns_label(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 63
        && is_ascii_lowercase_or_digit(bytes[0])
        && is_ascii_lowercase_or_digit(bytes[bytes.len() - 1])
        && bytes
            .iter()
            .all(|byte| is_ascii_lowercase_or_digit(*byte) || *byte == b'-')
}

fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value
            .split('-')
            .all(|part| !part.is_empty() && part.bytes().all(is_ascii_lowercase_or_digit))
}

fn is_ascii_lowercase_or_digit(value: u8) -> bool {
    value.is_ascii_lowercase() || value.is_ascii_digit()
}

#[cfg(test)]
mod tests {
    use super::{
        ArtifactNamespace, ArtifactVersion, CatalogScopeId, CatalogScopeName, DeliveryUnitId,
        DeliveryUnitSegment, ProducerId, ProducerIdentity, ProducerRevision,
        ReferenceArtifactIdentity, ReferenceArtifactSegment,
    };
    use crate::fingerprint::FingerprintPayload;
    use crate::{
        LinkLimitCounter, LinkLimitObservation, LinkLimits, ValueConstructionError, ValueGrammar,
    };

    #[test]
    fn artifact_version_fingerprint_is_two_big_endian_u16_values() {
        assert_eq!(
            ArtifactVersion::new(0x0102, 0x0304)
                .fingerprint_payload()
                .as_ref(),
            [0x01, 0x02, 0x03, 0x04]
        );
    }

    #[test]
    fn producer_identity_uses_exact_grammars() {
        assert!(ProducerId::try_new("dev.intlify/js-reference").is_ok());
        assert!(ProducerId::try_new("com.example/custom-json").is_ok());
        assert!(ProducerId::try_new("example/tool").is_err());
        assert!(ProducerId::try_new("Dev.intlify/tool").is_err());
        assert!(ProducerId::try_new("dev.intlify/two/slashes").is_err());

        assert!(ProducerRevision::try_new("0.4.2+sha.0123ABCD").is_ok());
        assert!(ProducerRevision::try_new("").is_err());
        assert!(ProducerRevision::try_new("release/1").is_err());

        let identity = ProducerIdentity::new(
            ProducerId::try_new("dev.intlify/js-reference").unwrap(),
            ProducerRevision::try_new("0.1.0+sha.1234").unwrap(),
        );
        assert_eq!(identity.id().as_str(), "dev.intlify/js-reference");
        assert_eq!(identity.revision().as_str(), "0.1.0+sha.1234");
    }

    #[test]
    fn scope_name_accepts_exact_protocol_boundary() {
        let exact = "é".repeat(127) + "a";
        assert_eq!(exact.len(), 255);
        assert!(CatalogScopeName::try_new(exact).is_ok());
        assert!(matches!(
            CatalogScopeName::try_new("é".repeat(128)),
            Err(ValueConstructionError::FieldLimit {
                limit: 255,
                attempted: 256,
                ..
            })
        ));
    }

    #[test]
    fn reference_identity_preserves_exact_segment_boundaries_and_order() {
        let identity = ReferenceArtifactIdentity::try_new(
            ArtifactNamespace::Project,
            vec![
                ReferenceArtifactSegment::try_new("js").unwrap(),
                ReferenceArtifactSegment::try_new("module").unwrap(),
                ReferenceArtifactSegment::try_new("src\\checkout.ts").unwrap(),
            ],
        )
        .unwrap();
        assert_eq!(identity.namespace(), ArtifactNamespace::Project);
        assert_eq!(identity.segments()[2].as_str(), "src\\checkout.ts");

        let other = ReferenceArtifactIdentity::try_new(
            ArtifactNamespace::Project,
            vec![
                ReferenceArtifactSegment::try_new("js").unwrap(),
                ReferenceArtifactSegment::try_new("module").unwrap(),
                ReferenceArtifactSegment::try_new("src").unwrap(),
                ReferenceArtifactSegment::try_new("checkout.ts").unwrap(),
            ],
        )
        .unwrap();
        assert_ne!(identity, other);
    }

    #[test]
    fn logical_identity_boundaries_are_inclusive() {
        assert!(ReferenceArtifactSegment::try_new("a".repeat(255)).is_ok());
        assert!(matches!(
            ReferenceArtifactSegment::try_new("a".repeat(256)),
            Err(ValueConstructionError::FieldLimit {
                counter: LinkLimitCounter::ReferenceIdentitySegmentBytes,
                limit: 255,
                attempted: 256,
            })
        ));

        let exact_segments = (0..64)
            .map(|_| ReferenceArtifactSegment::try_new("a").unwrap())
            .collect();
        assert!(
            ReferenceArtifactIdentity::try_new(ArtifactNamespace::Project, exact_segments).is_ok()
        );
        let over_segments = (0..65)
            .map(|_| ReferenceArtifactSegment::try_new("a").unwrap())
            .collect();
        assert!(matches!(
            ReferenceArtifactIdentity::try_new(ArtifactNamespace::Project, over_segments),
            Err(ValueConstructionError::FieldLimit {
                counter: LinkLimitCounter::ReferenceIdentitySegments,
                limit: 64,
                attempted: 65,
            })
        ));

        let exact_bytes = (0..16)
            .map(|_| ReferenceArtifactSegment::try_new("a".repeat(255)).unwrap())
            .chain(std::iter::once(
                ReferenceArtifactSegment::try_new("a".repeat(16)).unwrap(),
            ))
            .collect();
        assert!(
            ReferenceArtifactIdentity::try_new(ArtifactNamespace::Project, exact_bytes).is_ok()
        );
        let over_bytes = (0..16)
            .map(|_| ReferenceArtifactSegment::try_new("a".repeat(255)).unwrap())
            .chain(std::iter::once(
                ReferenceArtifactSegment::try_new("a".repeat(17)).unwrap(),
            ))
            .collect();
        assert!(matches!(
            ReferenceArtifactIdentity::try_new(ArtifactNamespace::Project, over_bytes),
            Err(ValueConstructionError::FieldLimit {
                counter: LinkLimitCounter::ReferenceIdentityBytes,
                limit: 4_096,
                attempted: 4_097,
            })
        ));
    }

    #[test]
    fn lower_identity_limits_follow_count_segment_and_running_total_order() {
        let identity = ReferenceArtifactIdentity::try_new(
            ArtifactNamespace::Project,
            vec![
                ReferenceArtifactSegment::try_new("abc").unwrap(),
                ReferenceArtifactSegment::try_new("def").unwrap(),
            ],
        )
        .unwrap();

        let count_zero = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::ReferenceIdentitySegments, 0)
            .unwrap();
        let evidence = identity.revalidate_limits(&count_zero).unwrap_err();
        assert_eq!(
            evidence.counter(),
            LinkLimitCounter::ReferenceIdentitySegments
        );
        assert_eq!(evidence.observation(), LinkLimitObservation::Exact(1));

        let segment_first_over = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::ReferenceIdentitySegmentBytes, 2)
            .unwrap();
        let evidence = identity.revalidate_limits(&segment_first_over).unwrap_err();
        assert_eq!(
            evidence.counter(),
            LinkLimitCounter::ReferenceIdentitySegmentBytes
        );
        assert_eq!(evidence.observation(), LinkLimitObservation::Exact(3));

        let total_exact = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::ReferenceIdentityBytes, 6)
            .unwrap();
        assert!(identity.revalidate_limits(&total_exact).is_ok());

        let total_first_over = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::ReferenceIdentityBytes, 5)
            .unwrap();
        let evidence = identity.revalidate_limits(&total_first_over).unwrap_err();
        assert_eq!(evidence.counter(), LinkLimitCounter::ReferenceIdentityBytes);
        assert_eq!(evidence.observation(), LinkLimitObservation::Exact(6));
    }

    #[test]
    fn invalid_logical_segments_are_not_constructible() {
        for invalid in ["", ".", "..", "a/b", "a\0b"] {
            assert!(matches!(
                ReferenceArtifactSegment::try_new(invalid),
                Err(ValueConstructionError::Grammar(
                    ValueGrammar::Empty | ValueGrammar::Invalid
                ))
            ));
        }
    }

    #[test]
    fn delivery_unit_main_is_the_exact_built_in_identity() {
        let main = DeliveryUnitId::main();
        assert_eq!(main.segments().len(), 1);
        assert_eq!(main.segments()[0].as_str(), "main");

        let explicit =
            DeliveryUnitId::try_new(vec![DeliveryUnitSegment::try_new("main").unwrap()]).unwrap();
        assert_eq!(main, explicit);
    }

    #[test]
    fn lower_delivery_and_scope_limits_accept_exact_and_reject_first_over() {
        let delivery = DeliveryUnitId::try_new(vec![
            DeliveryUnitSegment::try_new("abc").unwrap(),
            DeliveryUnitSegment::try_new("def").unwrap(),
        ])
        .unwrap();

        let count_zero = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::DeliveryUnitSegments, 0)
            .unwrap();
        let evidence = delivery.revalidate_limits(&count_zero).unwrap_err();
        assert_eq!(evidence.counter(), LinkLimitCounter::DeliveryUnitSegments);
        assert_eq!(evidence.observation(), LinkLimitObservation::Exact(1));

        let segment_first_over = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::DeliveryUnitSegmentBytes, 2)
            .unwrap();
        let evidence = delivery.revalidate_limits(&segment_first_over).unwrap_err();
        assert_eq!(
            evidence.counter(),
            LinkLimitCounter::DeliveryUnitSegmentBytes
        );
        assert_eq!(evidence.observation(), LinkLimitObservation::Exact(3));

        let total_exact = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::DeliveryUnitBytes, 6)
            .unwrap();
        assert!(delivery.revalidate_limits(&total_exact).is_ok());

        let total_first_over = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::DeliveryUnitBytes, 5)
            .unwrap();
        let evidence = delivery.revalidate_limits(&total_first_over).unwrap_err();
        assert_eq!(evidence.counter(), LinkLimitCounter::DeliveryUnitBytes);
        assert_eq!(evidence.observation(), LinkLimitObservation::Exact(6));

        let scope = CatalogScopeName::try_new("é").unwrap();
        let scope_zero = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::CatalogScopeNameBytes, 0)
            .unwrap();
        assert_eq!(
            scope
                .revalidate_limit(&scope_zero)
                .unwrap_err()
                .observation(),
            LinkLimitObservation::Exact(1)
        );
        let scope_exact = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::CatalogScopeNameBytes, 2)
            .unwrap();
        assert!(scope.revalidate_limit(&scope_exact).is_ok());
        let scope_first_over = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::CatalogScopeNameBytes, 1)
            .unwrap();
        assert_eq!(
            scope
                .revalidate_limit(&scope_first_over)
                .unwrap_err()
                .observation(),
            LinkLimitObservation::Exact(2)
        );
    }

    #[test]
    fn scope_fingerprint_is_tagged_and_namespace_explicit() {
        let scope = CatalogScopeId::new(
            ArtifactNamespace::Project,
            CatalogScopeName::try_new("app").unwrap(),
        );
        assert_eq!(
            scope.fingerprint_payload().as_ref(),
            [0x01, 0, 0, 0, 0, 0, 0, 0, 1, 0x00, 0x02, 0, 0, 0, 0, 0, 0, 0, 3, b'a', b'p', b'p']
        );
    }
}
