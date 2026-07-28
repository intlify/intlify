// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Fail-complete operational errors for checked linker request admission.
//!
//! This module owns bounded, typed evidence for invalid request structure,
//! unsupported artifact contracts, and operational limit overruns. It does not
//! render CLI diagnostics or turn semantic linker findings into errors.

use std::error::Error;
use std::fmt;

use intlify_contract::{
    ArtifactVersionEvidence, ArtifactViolation, CatalogKeyDomain, CatalogScopeId, DeliveryUnitId,
    LinkLimitEvidence, Locale, MessageSelector, ReferenceArtifactIdentity, SourceDocumentIdentity,
};

use crate::ResolvedCatalogScopeId;

/// Artifact kind participating in one link request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ArtifactKind {
    /// Message reference artifact.
    Reference,
    /// Message definition artifact.
    Definition,
}

/// Canonical artifact identity retained by a request-admission failure.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ArtifactContractSubject {
    /// Logical reference artifact identity.
    Reference(ReferenceArtifactIdentity),
    /// Primary definition source identity.
    Definition(SourceDocumentIdentity),
}

/// Structural identity of one configured root, excluding optional reason evidence.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ConfiguredRootIdentity {
    pub(crate) scope: CatalogScopeId,
    pub(crate) domain: CatalogKeyDomain,
    pub(crate) selector: MessageSelector,
}

impl ConfiguredRootIdentity {
    /// Return the catalog scope at the current validation boundary.
    #[must_use]
    pub const fn scope(&self) -> &CatalogScopeId {
        &self.scope
    }

    /// Return the catalog-key comparison domain.
    #[must_use]
    pub const fn domain(&self) -> CatalogKeyDomain {
        self.domain
    }

    /// Return the checked selector.
    #[must_use]
    pub const fn selector(&self) -> &MessageSelector {
        &self.selector
    }
}

/// Role of one scope use rejected during request binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ScopeUse {
    /// Scope declared by a reference record.
    Reference,
    /// Scope declared by a definition record.
    Definition,
    /// Scope declared by a configured root.
    ConfiguredRoot,
}

/// Endpoint role inside one checked scope mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ScopeEndpoint {
    /// Mapping source.
    Source,
    /// Mapping target.
    Target,
}

/// Endpoint role inside one delivery-graph edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DeliveryEdgeEndpoint {
    /// Parent endpoint.
    Parent,
    /// Child endpoint.
    Child,
}

/// One request invariant rejected before semantic linking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidRequestError {
    /// Link policy requires at least one production locale.
    EmptyProductionLocales,
    /// One checked locale occurs more than once in the production set.
    DuplicateProductionLocale(Locale),
    /// One checked configured-root identity occurs more than once.
    DuplicateConfiguredRoot(ConfiguredRootIdentity),
    /// The declared scope inventory contains an equal duplicate.
    DuplicateDeclaredScope(CatalogScopeId),
    /// One mapping endpoint is absent from the declared scope inventory.
    UndeclaredMappingEndpoint {
        /// Endpoint role.
        endpoint: ScopeEndpoint,
        /// Exact rejected scope.
        scope: CatalogScopeId,
    },
    /// A mapping source occurs more than once.
    DuplicateScopeMappingSource(CatalogScopeId),
    /// A mapping redundantly maps one scope to itself.
    ScopeMappingSelfMap(CatalogScopeId),
    /// A mapping target also occurs as a source, forming a chain or cycle.
    ScopeMappingTargetIsSource(CatalogScopeId),
    /// One completeness scope occurs more than once.
    DuplicateCompletenessScope(CatalogScopeId),
    /// Completeness entries are not in canonical scope order.
    NonCanonicalCompletenessOrder {
        /// Earlier submitted scope.
        previous: CatalogScopeId,
        /// Later scope that sorts before the earlier scope.
        current: CatalogScopeId,
    },
    /// One scope required by the inventory has no completeness entry.
    MissingCompletenessScope(CatalogScopeId),
    /// One completeness entry is outside the target inventory.
    ExtraCompletenessScope(CatalogScopeId),
    /// One delivery graph node occurs more than once.
    DuplicateDeliveryUnit(DeliveryUnitId),
    /// One edge endpoint does not name a submitted graph node.
    UnknownDeliveryEdgeEndpoint {
        /// Parent or child endpoint.
        endpoint: DeliveryEdgeEndpoint,
        /// Exact missing node.
        delivery_unit: DeliveryUnitId,
    },
    /// One exact directed edge occurs more than once.
    DuplicateDeliveryEdge {
        /// Exact parent node.
        parent: DeliveryUnitId,
        /// Exact child node.
        child: DeliveryUnitId,
    },
    /// One graph edge points from a node to itself.
    DeliverySelfEdge(DeliveryUnitId),
    /// The submitted directed graph contains a cycle.
    DeliveryGraphCycle,
    /// Two submitted reference artifacts have the same logical identity.
    DuplicateReferenceArtifact(ReferenceArtifactIdentity),
    /// Two submitted definition artifacts have the same source identity.
    DuplicateDefinitionArtifact(SourceDocumentIdentity),
    /// A record or policy scope is outside the complete target inventory.
    UndeclaredScope {
        /// Owning request input.
        usage: ScopeUse,
        /// Exact rejected scope.
        scope: CatalogScopeId,
    },
    /// The mapping table was constructed against a different scope inventory.
    MappingInventoryMismatch,
    /// Distinct declared roots collapse to one resolved identity with unequal reasons.
    ResolvedConfiguredRootConflict(ConfiguredRootIdentity),
    /// A reference artifact names no node in the checked delivery graph.
    MissingReferenceDeliveryUnit {
        /// Logical reference artifact identity.
        artifact: ReferenceArtifactIdentity,
        /// Exact missing delivery unit.
        delivery_unit: DeliveryUnitId,
    },
    /// Configured roots cannot be placed because the checked graph is empty.
    ConfiguredRootRequiresDeliveryUnit,
    /// One resolved scope contains incompatible catalog-key domains.
    ResolvedDomainConflict {
        /// Resolved semantic scope.
        scope: ResolvedCatalogScopeId,
        /// Canonically first conflicting domain.
        first: CatalogKeyDomain,
        /// Canonically second conflicting domain.
        second: CatalogKeyDomain,
    },
    /// A supposedly checked artifact failed defensive structural revalidation.
    ArtifactContract {
        /// Exact artifact subject.
        subject: ArtifactContractSubject,
        /// Canonical contract violation.
        violation: ArtifactViolation,
    },
}

/// Unsupported artifact or semantic contract supplied to the linker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnsupportedContractError {
    /// One artifact version is outside the exact contract supported by this build.
    ArtifactVersion {
        /// Artifact kind.
        kind: ArtifactKind,
        /// Exact artifact subject.
        subject: ArtifactContractSubject,
        /// Checked version-negotiation evidence.
        evidence: ArtifactVersionEvidence,
    },
}

/// Fail-complete error returned before a semantic result exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkOperationalError {
    /// Checked request inputs contradict one another.
    InvalidRequest(InvalidRequestError),
    /// A supplied contract revision is unsupported.
    UnsupportedContract(UnsupportedContractError),
    /// One operational budget was exceeded.
    Limit(LinkLimitEvidence),
}

impl fmt::Display for LinkOperationalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest(error) => write!(formatter, "invalid link request: {error:?}"),
            Self::UnsupportedContract(error) => {
                write!(formatter, "unsupported link contract: {error:?}")
            }
            Self::Limit(evidence) => write!(
                formatter,
                "{} limit {} exceeded by {:?}",
                evidence.counter().as_str(),
                evidence.effective_limit(),
                evidence.observation()
            ),
        }
    }
}

impl Error for LinkOperationalError {}

impl From<InvalidRequestError> for LinkOperationalError {
    fn from(error: InvalidRequestError) -> Self {
        Self::InvalidRequest(error)
    }
}

impl From<LinkLimitEvidence> for LinkOperationalError {
    fn from(evidence: LinkLimitEvidence) -> Self {
        Self::Limit(evidence)
    }
}
