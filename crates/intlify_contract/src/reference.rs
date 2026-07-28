// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Immutable message-reference records and complete reference artifacts.
//!
//! This module owns the checked record composition, derived record identity,
//! complete artifact admission, canonical lower-limit revalidation phases, and
//! observable decoded-byte charge used by artifact codecs and linker admission.
//!
//! It does not parse or emit artifact transport, scan source languages, resolve
//! selectors, inspect catalog definitions, or construct linker findings.

use std::error::Error;
use std::fmt;

use crate::{
    ArtifactContractError, ArtifactVersion, ArtifactVersionEvidence, ArtifactVersionSupport,
    CatalogKeyDomain, CatalogScopeId, DeliveryUnitId, LinkLimitCounter, LinkLimits,
    MessageSelector, ProducerIdentity, ReasonText, ReferenceArtifactIdentity, SourceOrigin,
};

const REFERENCE_VERSION_SUPPORT: ArtifactVersionSupport =
    ArtifactVersionSupport::Exact(ArtifactVersion::DRAFT_V0_1);

/// Cross-field failure returned while constructing one checked reference record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessageReferenceConstructionError {
    /// A selector payload belongs to a different catalog-key domain.
    SelectorDomainMismatch,
}

impl fmt::Display for MessageReferenceConstructionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SelectorDomainMismatch => {
                formatter.write_str("selector payload domain does not match the reference domain")
            }
        }
    }
}

impl Error for MessageReferenceConstructionError {}

/// One immutable reference to a message selector in one exact scope-domain pair.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MessageReference {
    scope: CatalogScopeId,
    domain: CatalogKeyDomain,
    selector: MessageSelector,
    reason: Option<ReasonText>,
    origin: Option<SourceOrigin>,
}

impl MessageReference {
    /// Construct one complete checked reference record.
    pub fn try_new(
        scope: CatalogScopeId,
        domain: CatalogKeyDomain,
        selector: MessageSelector,
        reason: Option<ReasonText>,
        origin: Option<SourceOrigin>,
    ) -> Result<Self, MessageReferenceConstructionError> {
        if !selector.is_for_domain(domain) {
            return Err(MessageReferenceConstructionError::SelectorDomainMismatch);
        }
        Ok(Self {
            scope,
            domain,
            selector,
            reason,
            origin,
        })
    }

    /// Return the exact declared catalog scope.
    #[must_use]
    pub const fn scope(&self) -> &CatalogScopeId {
        &self.scope
    }

    /// Return the exact catalog-key comparison domain.
    #[must_use]
    pub const fn domain(&self) -> CatalogKeyDomain {
        self.domain
    }

    /// Return the checked selector.
    #[must_use]
    pub const fn selector(&self) -> &MessageSelector {
        &self.selector
    }

    /// Return the optional exact widening reason.
    #[must_use]
    pub const fn reason(&self) -> Option<&ReasonText> {
        self.reason.as_ref()
    }

    /// Return the optional diagnostic source evidence.
    #[must_use]
    pub const fn origin(&self) -> Option<&SourceOrigin> {
        self.origin.as_ref()
    }
}

/// Portable identity of one reference record in one exact artifact.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReferenceRecordIdentity {
    artifact: ReferenceArtifactIdentity,
    ordinal: u32,
}

impl ReferenceRecordIdentity {
    fn new(artifact: ReferenceArtifactIdentity, ordinal: u32) -> Self {
        Self { artifact, ordinal }
    }

    /// Return the owning reference-artifact identity.
    #[must_use]
    pub const fn artifact(&self) -> &ReferenceArtifactIdentity {
        &self.artifact
    }

    /// Return the zero-based semantic record position.
    #[must_use]
    pub const fn ordinal(&self) -> u32 {
        self.ordinal
    }
}

/// One complete immutable message-reference artifact.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MessageReferenceArtifact {
    version: ArtifactVersion,
    producer: ProducerIdentity,
    identity: ReferenceArtifactIdentity,
    delivery_unit: DeliveryUnitId,
    references: Box<[MessageReference]>,
    decoded_bytes: u64,
}

impl MessageReferenceArtifact {
    /// Validate and retain one complete reference artifact.
    pub fn try_new(
        version: ArtifactVersion,
        producer: ProducerIdentity,
        identity: ReferenceArtifactIdentity,
        delivery_unit: DeliveryUnitId,
        references: Vec<MessageReference>,
        limits: &LinkLimits,
    ) -> Result<Self, ArtifactContractError> {
        let decoded_bytes = validate_parts(
            version,
            &producer,
            &identity,
            &delivery_unit,
            &references,
            limits,
        )?;
        Ok(Self {
            version,
            producer,
            identity,
            delivery_unit,
            references: references.into_boxed_slice(),
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

    /// Return the portable logical artifact identity.
    #[must_use]
    pub const fn identity(&self) -> &ReferenceArtifactIdentity {
        &self.identity
    }

    /// Return the delivery-graph node identity.
    #[must_use]
    pub const fn delivery_unit(&self) -> &DeliveryUnitId {
        &self.delivery_unit
    }

    /// Return the semantic reference array in submitted order.
    #[must_use]
    pub const fn references(&self) -> &[MessageReference] {
        &self.references
    }

    /// Derive one record identity only for an admitted ordinal.
    #[must_use]
    pub fn record_identity(&self, ordinal: usize) -> Option<ReferenceRecordIdentity> {
        if ordinal >= self.references.len() {
            return None;
        }
        let ordinal = u32::try_from(ordinal)
            .expect("the admitted reference-record ceiling is below u32::MAX");
        Some(ReferenceRecordIdentity::new(self.identity.clone(), ordinal))
    }

    /// Return the exact variable-width decoded payload charge.
    #[must_use]
    pub const fn decoded_bytes(&self) -> u64 {
        self.decoded_bytes
    }

    /// Revalidate this complete artifact under current caller-selected limits.
    pub fn revalidate(&self, limits: &LinkLimits) -> Result<(), ArtifactContractError> {
        validate_parts(
            self.version,
            &self.producer,
            &self.identity,
            &self.delivery_unit,
            &self.references,
            limits,
        )
        .map(|_| ())
    }
}

fn validate_parts(
    version: ArtifactVersion,
    producer: &ProducerIdentity,
    identity: &ReferenceArtifactIdentity,
    delivery_unit: &DeliveryUnitId,
    references: &[MessageReference],
    limits: &LinkLimits,
) -> Result<u64, ArtifactContractError> {
    if !REFERENCE_VERSION_SUPPORT.supports(version) {
        return Err(ArtifactContractError::UnsupportedVersion(
            ArtifactVersionEvidence::for_unsupported(version, REFERENCE_VERSION_SUPPORT),
        ));
    }

    // Revalidation deliberately mirrors the wire decoder's canonical phases.
    // Keep collection checks as separate whole-slice passes: validating each
    // reference from start to finish would change which failure wins.
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

    identity.revalidate_limits(limits)?;
    for segment in identity.segments() {
        charge_decoded(&mut decoded_bytes, segment.as_str().len() as u64, limits)?;
    }

    delivery_unit.revalidate_limits(limits)?;
    for segment in delivery_unit.segments() {
        charge_decoded(&mut decoded_bytes, segment.as_str().len() as u64, limits)?;
    }

    LinkLimitCounter::ReferenceRecords.check_artifact_limit(references.len() as u64, limits)?;

    // Scope-name limits and decoded accounting precede every selector phase.
    for reference in references {
        reference.scope.name().revalidate_limit(limits)?;
        charge_decoded(
            &mut decoded_bytes,
            reference.scope.name().as_str().len() as u64,
            limits,
        )?;
    }

    // Exact and Prefix byte limits share one phase across the collection.
    for reference in references {
        match &reference.selector {
            MessageSelector::Exact(key) => {
                key.revalidate_selector_bytes(limits)?;
                charge_decoded(&mut decoded_bytes, key.as_str().len() as u64, limits)?;
            }
            MessageSelector::Prefix(prefix) => {
                prefix.revalidate_bytes(limits)?;
                charge_decoded(&mut decoded_bytes, prefix.as_str().len() as u64, limits)?;
            }
            MessageSelector::Pattern(_)
            | MessageSelector::AllInScope
            | MessageSelector::UnboundedDynamic => {}
        }
    }

    // Pattern byte and token limits are independent of selector-path limits.
    for reference in references {
        if let MessageSelector::Pattern(pattern) = &reference.selector {
            pattern.revalidate_bytes(limits)?;
            charge_decoded(&mut decoded_bytes, pattern.as_str().len() as u64, limits)?;
        }
    }

    for reference in references {
        if let MessageSelector::Pattern(pattern) = &reference.selector {
            pattern.revalidate_tokens(limits)?;
        }
    }

    // Checked ReasonText values still re-enter their byte phase so a caller-
    // selected lower limit behaves exactly like serialized decoding.
    for reference in references {
        if let Some(reason) = &reference.reason {
            reason.revalidate_limit(limits)?;
            charge_decoded(&mut decoded_bytes, reason.as_str().len() as u64, limits)?;
        }
    }

    // Origin paths retain count, per-segment, and running-total phases instead
    // of collapsing them into one path constructor check.
    for reference in references {
        if let Some(origin) = &reference.origin {
            LinkLimitCounter::PathSegments
                .check_artifact_limit(origin.source().path().segments().len() as u64, limits)?;
        }
    }

    for reference in references {
        if let Some(origin) = &reference.origin {
            for segment in origin.source().path().segments() {
                LinkLimitCounter::PathSegmentBytes
                    .check_artifact_limit(segment.as_str().len() as u64, limits)?;
                charge_decoded(&mut decoded_bytes, segment.as_str().len() as u64, limits)?;
            }
        }
    }

    for reference in references {
        if let Some(origin) = &reference.origin {
            let mut path_bytes = 0_u64;
            for segment in origin.source().path().segments() {
                path_bytes = path_bytes
                    .checked_add(segment.as_str().len() as u64)
                    .expect("checked portable path components cannot overflow u64");
                LinkLimitCounter::PathBytes.check_artifact_limit(path_bytes, limits)?;
            }
        }
    }

    // Exact and Prefix token limits are last among selector checks, matching
    // the decoder phase that follows origin byte admission.
    for reference in references {
        match &reference.selector {
            MessageSelector::Exact(key) => key.revalidate_tokens(limits)?,
            MessageSelector::Prefix(prefix) => prefix.revalidate_tokens(limits)?,
            MessageSelector::Pattern(_)
            | MessageSelector::AllInScope
            | MessageSelector::UnboundedDynamic => {}
        }
    }

    Ok(decoded_bytes)
}

fn charge_decoded(
    total: &mut u64,
    addend: u64,
    limits: &LinkLimits,
) -> Result<(), ArtifactContractError> {
    let attempted = total
        .checked_add(addend)
        .expect("artifact payload ceilings make decoded-byte overflow impossible");
    LinkLimitCounter::ReferenceArtifactDecodedBytes.check_artifact_limit(attempted, limits)?;
    *total = attempted;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        MessageReference, MessageReferenceArtifact, MessageReferenceConstructionError,
        REFERENCE_VERSION_SUPPORT,
    };
    use crate::{
        ArtifactContractError, ArtifactNamespace, ArtifactVersion, CatalogKey, CatalogKeyDomain,
        CatalogScopeId, CatalogScopeName, DeliveryUnitId, LinkLimitCounter, LinkLimits,
        MessageSelector, ProducerId, ProducerIdentity, ProducerRevision, ReferenceArtifactIdentity,
        ReferenceArtifactSegment,
    };

    fn identity() -> ReferenceArtifactIdentity {
        ReferenceArtifactIdentity::try_new(
            ArtifactNamespace::Project,
            vec![ReferenceArtifactSegment::try_new("fixture").unwrap()],
        )
        .unwrap()
    }

    fn producer() -> ProducerIdentity {
        ProducerIdentity::new(
            ProducerId::try_new("dev.intlify/test").unwrap(),
            ProducerRevision::try_new("1").unwrap(),
        )
    }

    fn reference() -> MessageReference {
        MessageReference::try_new(
            CatalogScopeId::new(
                ArtifactNamespace::Project,
                CatalogScopeName::try_new("app").unwrap(),
            ),
            CatalogKeyDomain::JsonPointer,
            MessageSelector::Exact(
                CatalogKey::try_new(CatalogKeyDomain::JsonPointer, "/title").unwrap(),
            ),
            None,
            None,
        )
        .unwrap()
    }

    #[test]
    fn reference_rejects_a_selector_from_another_domain() {
        let result = MessageReference::try_new(
            CatalogScopeId::new(
                ArtifactNamespace::Project,
                CatalogScopeName::try_new("app").unwrap(),
            ),
            CatalogKeyDomain::YamlTypedPath,
            MessageSelector::Exact(
                CatalogKey::try_new(CatalogKeyDomain::JsonPointer, "/title").unwrap(),
            ),
            None,
            None,
        );
        assert_eq!(
            result,
            Err(MessageReferenceConstructionError::SelectorDomainMismatch)
        );
    }

    #[test]
    fn zero_reference_artifact_is_valid_and_record_identity_is_derived() {
        let empty = MessageReferenceArtifact::try_new(
            ArtifactVersion::DRAFT_V0_1,
            producer(),
            identity(),
            DeliveryUnitId::main(),
            Vec::new(),
            &LinkLimits::default(),
        )
        .unwrap();
        assert!(empty.references().is_empty());
        assert!(empty.record_identity(0).is_none());

        let populated = MessageReferenceArtifact::try_new(
            ArtifactVersion::DRAFT_V0_1,
            producer(),
            identity(),
            DeliveryUnitId::main(),
            vec![reference()],
            &LinkLimits::default(),
        )
        .unwrap();
        let record = populated.record_identity(0).unwrap();
        assert_eq!(record.artifact(), populated.identity());
        assert_eq!(record.ordinal(), 0);
        assert!(populated.record_identity(1).is_none());
    }

    #[test]
    fn unsupported_draft_is_rejected_before_artifact_admission() {
        let observed = ArtifactVersion::new(0, 2);
        let error = MessageReferenceArtifact::try_new(
            observed,
            producer(),
            identity(),
            DeliveryUnitId::main(),
            Vec::new(),
            &LinkLimits::default(),
        )
        .unwrap_err();
        let ArtifactContractError::UnsupportedVersion(evidence) = error else {
            panic!("expected unsupported version")
        };
        assert_eq!(evidence.observed(), observed);
        assert_eq!(evidence.supported(), REFERENCE_VERSION_SUPPORT);
    }

    #[test]
    fn lower_record_and_decoded_limits_fail_without_a_partial_artifact() {
        let record_limit = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::ReferenceRecords, 0)
            .unwrap();
        assert!(matches!(
            MessageReferenceArtifact::try_new(
                ArtifactVersion::DRAFT_V0_1,
                producer(),
                identity(),
                DeliveryUnitId::main(),
                vec![reference()],
                &record_limit,
            ),
            Err(ArtifactContractError::Limit(evidence))
                if evidence.counter() == LinkLimitCounter::ReferenceRecords
        ));

        let decoded_limit = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::ReferenceArtifactDecodedBytes, 0)
            .unwrap();
        assert!(matches!(
            MessageReferenceArtifact::try_new(
                ArtifactVersion::DRAFT_V0_1,
                producer(),
                identity(),
                DeliveryUnitId::main(),
                Vec::new(),
                &decoded_limit,
            ),
            Err(ArtifactContractError::Limit(evidence))
                if evidence.counter() == LinkLimitCounter::ReferenceArtifactDecodedBytes
        ));
    }
}
