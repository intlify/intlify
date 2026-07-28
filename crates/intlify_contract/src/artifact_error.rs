// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Versioned artifact admission failures and semantic locations.
//!
//! This module owns the closed error vocabulary shared by direct artifact
//! construction and serialized artifact decoding. Errors retain bounded,
//! machine-readable evidence without parser-native messages, source excerpts,
//! raw member names, or implementation detail.
//!
//! It does not parse JSON, perform transport I/O, render diagnostics, or map an
//! artifact failure into a linker or CLI error.

use std::error::Error;
use std::fmt;

use crate::{ArtifactLimitEvidence, ArtifactVersion};

/// Closed failure returned while admitting one complete artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactContractError {
    /// The submitted value violates the selected artifact schema or value contract.
    InvalidArtifact(ArtifactViolation),
    /// A structurally valid artifact version is not supported by this consumer.
    UnsupportedVersion(ArtifactVersionEvidence),
    /// One artifact-local resource limit was exceeded.
    Limit(ArtifactLimitEvidence),
}

impl fmt::Display for ArtifactContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidArtifact(violation) => write!(
                formatter,
                "invalid artifact: {:?} at {:?}",
                violation.code, violation.location
            ),
            Self::UnsupportedVersion(evidence) => write!(
                formatter,
                "unsupported artifact version {}.{}",
                evidence.observed.major(),
                evidence.observed.minor()
            ),
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

impl Error for ArtifactContractError {}

impl From<ArtifactLimitEvidence> for ArtifactContractError {
    fn from(evidence: ArtifactLimitEvidence) -> Self {
        Self::Limit(evidence)
    }
}

/// One schema/value violation selected by canonical artifact precedence.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArtifactViolation {
    code: ArtifactViolationCode,
    location: ArtifactViolationLocation,
}

impl ArtifactViolation {
    pub(crate) const fn new(
        code: ArtifactViolationCode,
        location: ArtifactViolationLocation,
    ) -> Self {
        Self { code, location }
    }

    /// Return the closed violation classification.
    #[must_use]
    pub const fn code(&self) -> ArtifactViolationCode {
        self.code
    }

    /// Return the bounded semantic contract location.
    #[must_use]
    pub const fn location(&self) -> &ArtifactViolationLocation {
        &self.location
    }
}

/// Closed invalid-artifact classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ArtifactViolationCode {
    /// Input bytes are not valid UTF-8.
    InvalidUtf8,
    /// Valid UTF-8 bytes violate the strict JSON lexical or grammar contract.
    InvalidJsonSyntax,
    /// A complete root value is followed by non-whitespace data.
    TrailingData,
    /// A required object member is absent.
    MissingMember,
    /// One decoded object-member name occurs more than once.
    DuplicateMember,
    /// An object contains a member outside the selected schema.
    UnknownMember,
    /// An explicitly present null is forbidden.
    NullNotAllowed,
    /// A present value has the wrong JSON category.
    TypeMismatch,
    /// A numeric token is not the required shortest unsigned integer.
    InvalidInteger,
    /// The artifact root kind is not the exact typed-decoder kind.
    KindMismatch,
    /// A closed nested string tag is unknown.
    UnknownTag,
    /// A typed scalar or compound value violates its exact grammar.
    InvalidValueGrammar,
    /// A semantic collection is not in its canonical order.
    NonCanonicalOrder,
    /// A semantic collection contains a forbidden repeated value.
    DuplicateValue,
    /// Individually valid fields violate a cross-field invariant.
    InconsistentValue,
    /// An entry occurrence sequence is not continuous.
    DiscontinuousOccurrence,
}

/// Bounded semantic location for one invalid artifact.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ArtifactViolationLocation {
    /// No selected schema location is available.
    Root,
    /// One reference- or definition-artifact envelope container or leaf.
    Envelope(ArtifactEnvelopeLocation),
    /// One segment inside an envelope-owned path or logical identity.
    EnvelopePathSegment {
        /// The path-bearing envelope role.
        role: ArtifactPathRole,
        /// Zero-based segment position.
        segment_ordinal: u32,
    },
    /// One definition logical alias or one of its segments.
    LogicalAlias {
        /// Zero-based alias position.
        alias_ordinal: u32,
        /// Zero-based segment position when one segment is selected.
        segment_ordinal: Option<u32>,
    },
    /// One definition record container or leaf.
    Definition {
        /// Zero-based definition position.
        ordinal: u32,
        /// Selected known field, when available.
        field: Option<DefinitionField>,
    },
    /// One reference record container or leaf.
    Reference {
        /// Zero-based reference position.
        ordinal: u32,
        /// Selected known field, when available.
        field: Option<ReferenceField>,
    },
    /// One source-path segment inside a reference origin.
    ReferenceOriginSegment {
        /// Zero-based reference position.
        reference_ordinal: u32,
        /// Zero-based source-path segment position.
        segment_ordinal: u32,
    },
}

/// Artifact-kind-tagged envelope location.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ArtifactEnvelopeLocation {
    /// Reference-artifact envelope or one of its fields.
    Reference {
        /// Selected known field, when available.
        field: Option<ReferenceEnvelopeField>,
    },
    /// Definition-artifact envelope or one of its fields.
    Definition {
        /// Selected known field, when available.
        field: Option<DefinitionEnvelopeField>,
    },
}

/// Envelope-owned path or logical-identity role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ArtifactPathRole {
    /// Definition-artifact primary source path.
    DefinitionSource,
    /// Reference-artifact logical identity.
    ReferenceIdentity,
    /// Reference-artifact delivery unit.
    DeliveryUnit,
}

/// Reference-artifact envelope fields in canonical adapter order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ReferenceEnvelopeField {
    Kind,
    Version,
    VersionMajor,
    VersionMinor,
    Producer,
    ProducerId,
    ProducerRevision,
    Identity,
    IdentityNamespace,
    IdentitySegments,
    DeliveryUnit,
    References,
}

/// Definition-artifact envelope fields reserved by the shared error contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DefinitionEnvelopeField {
    Kind,
    Version,
    VersionMajor,
    VersionMinor,
    Producer,
    ProducerId,
    ProducerRevision,
    Source,
    SourceNamespace,
    SourcePath,
    LogicalAliases,
    InputFingerprint,
    FingerprintAlgorithm,
    FingerprintDigest,
    Definitions,
}

/// Definition-record fields reserved by the shared error contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DefinitionField {
    Scope,
    ScopeNamespace,
    ScopeName,
    Domain,
    Key,
    Locale,
    Message,
    Source,
    SourceStructuralPath,
    SourceOccurrence,
}

/// Reference-record fields in canonical adapter order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ReferenceField {
    Scope,
    ScopeNamespace,
    ScopeName,
    Domain,
    Selector,
    SelectorKind,
    SelectorKey,
    SelectorPrefix,
    SelectorPattern,
    Reason,
    Origin,
    OriginSource,
    OriginSourceNamespace,
    OriginSourcePath,
    OriginSpan,
    OriginSpanStart,
    OriginSpanEnd,
}

/// Consumer-supported artifact-version contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArtifactVersionSupport {
    /// One exact mutable draft version.
    Exact(ArtifactVersion),
    /// One stable major and every minor through the inclusive maximum.
    StableRange {
        /// Supported stable major.
        major: u16,
        /// Inclusive maximum supported minor.
        max_minor: u16,
    },
}

impl ArtifactVersionSupport {
    /// Return whether this support value obeys the draft/stable mode contract.
    #[must_use]
    pub const fn is_valid(self) -> bool {
        match self {
            Self::Exact(version) => version.major() == 0,
            Self::StableRange { major, .. } => major >= 1,
        }
    }

    /// Return whether one observed version is admitted.
    #[must_use]
    pub const fn supports(self, observed: ArtifactVersion) -> bool {
        if !self.is_valid() {
            return false;
        }
        match self {
            Self::Exact(expected) => {
                observed.major() == expected.major() && observed.minor() == expected.minor()
            }
            Self::StableRange { major, max_minor } => {
                observed.major() == major && observed.minor() <= max_minor
            }
        }
    }
}

/// Evidence for one structurally valid unsupported artifact version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ArtifactVersionEvidence {
    observed: ArtifactVersion,
    supported: ArtifactVersionSupport,
}

impl ArtifactVersionEvidence {
    pub(crate) fn for_unsupported(
        observed: ArtifactVersion,
        supported: ArtifactVersionSupport,
    ) -> Self {
        debug_assert!(supported.is_valid());
        debug_assert!(!supported.supports(observed));
        Self {
            observed,
            supported,
        }
    }

    /// Return the exact observed version.
    #[must_use]
    pub const fn observed(&self) -> ArtifactVersion {
        self.observed
    }

    /// Return the checked consumer support contract.
    #[must_use]
    pub const fn supported(&self) -> ArtifactVersionSupport {
        self.supported
    }
}

#[cfg(test)]
mod tests {
    use super::ArtifactVersionSupport;
    use crate::ArtifactVersion;

    #[test]
    fn draft_support_requires_one_exact_draft_pair() {
        let support = ArtifactVersionSupport::Exact(ArtifactVersion::new(0, 1));
        assert!(support.is_valid());
        assert!(support.supports(ArtifactVersion::new(0, 1)));
        assert!(!support.supports(ArtifactVersion::new(0, 0)));
        assert!(!support.supports(ArtifactVersion::new(0, 2)));
        assert!(!support.supports(ArtifactVersion::new(1, 1)));
    }

    #[test]
    fn stable_support_admits_only_one_major_through_the_maximum_minor() {
        let support = ArtifactVersionSupport::StableRange {
            major: 2,
            max_minor: 3,
        };
        assert!(support.is_valid());
        assert!(support.supports(ArtifactVersion::new(2, 0)));
        assert!(support.supports(ArtifactVersion::new(2, 3)));
        assert!(!support.supports(ArtifactVersion::new(2, 4)));
        assert!(!support.supports(ArtifactVersion::new(1, 3)));
        assert!(!support.supports(ArtifactVersion::new(3, 0)));
    }

    #[test]
    fn invalid_support_modes_never_admit_an_observed_version() {
        let stable_exact = ArtifactVersionSupport::Exact(ArtifactVersion::new(1, 0));
        assert!(!stable_exact.is_valid());
        assert!(!stable_exact.supports(ArtifactVersion::new(1, 0)));

        let draft_range = ArtifactVersionSupport::StableRange {
            major: 0,
            max_minor: 1,
        };
        assert!(!draft_range.is_valid());
        assert!(!draft_range.supports(ArtifactVersion::new(0, 1)));
    }
}
