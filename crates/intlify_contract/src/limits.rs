// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Resource-limit configuration and failure evidence contracts.
//!
//! This module owns the compatibility-stable counter vocabulary, protocol
//! defaults, immutable caller-selected lower limits, and validated artifact- or
//! linker-side evidence describing the first rejected observation.
//!
//! It does not measure work, mutate shared counters, or decide when an operation
//! must stop. Artifact codecs and linker stages observe their own work and use
//! these types to apply and report the shared limits consistently.

use std::error::Error;
use std::fmt;

use crate::{
    DeliveryUnitId, Locale, ReferenceArtifactIdentity, SourceDocumentIdentity,
    ValueConstructionError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CounterState {
    Active,
    Reserved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ObservationContract {
    Exact,
    FirstOver,
    ExactOrArithmeticOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CounterBoundary {
    LinkOnly,
    ArtifactAndLink,
    ArtifactOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
enum LinkLimitSubjectKind {
    Request = 1 << 0,
    DefinitionArtifactEnvelope = 1 << 1,
    ReferenceArtifactGroup = 1 << 2,
    DefinitionArtifactGroup = 1 << 3,
    DeliveryGraph = 1 << 4,
    DeliveryUnitGroup = 1 << 5,
    ResolvedPolicy = 1 << 6,
    FallbackSource = 1 << 7,
    ScopeMappings = 1 << 8,
}

#[derive(Debug, Clone, Copy)]
struct LinkLimitCounterMetadata {
    counter: LinkLimitCounter,
    name: &'static str,
    protocol_ceiling: u64,
    state: CounterState,
    observation: ObservationContract,
    boundary: CounterBoundary,
    subject_mask: u16,
}

macro_rules! counter_metadata {
    (
        $counter:ident,
        $name:literal,
        $ceiling:literal,
        $state:ident,
        $observation:ident,
        $boundary:ident,
        [$($subject:ident),* $(,)?]
    ) => {
        LinkLimitCounterMetadata {
            counter: LinkLimitCounter::$counter,
            name: $name,
            protocol_ceiling: $ceiling,
            state: CounterState::$state,
            observation: ObservationContract::$observation,
            boundary: CounterBoundary::$boundary,
            subject_mask: 0_u16 $(| (LinkLimitSubjectKind::$subject as u16))*,
        }
    };
}

/// Closed linker and artifact resource-counter vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum LinkLimitCounter {
    /// Submitted reference artifact occurrences in one request.
    ReferenceArtifacts = 0,
    /// Aggregate logical identity bytes across reference artifacts.
    ReferenceIdentityBytesTotal,
    /// Aggregate reference records across reference artifacts.
    ReferenceRecordsTotal,
    /// Aggregate decoded bytes across reference artifacts.
    ReferenceArtifactDecodedBytesTotal,
    /// Submitted definition artifact occurrences in one request.
    DefinitionArtifacts,
    /// Aggregate definition records across definition artifacts.
    DefinitionsTotal,
    /// Aggregate decoded bytes across definition artifacts.
    DefinitionArtifactDecodedBytesTotal,
    /// Submitted delivery graph node occurrences.
    DeliveryGraphNodes,
    /// Submitted delivery graph edge occurrences.
    DeliveryGraphEdges,
    /// Aggregate delivery graph node-identity bytes.
    DeliveryGraphIdBytes,
    /// Submitted production-locale occurrences.
    ProductionLocales,
    /// Reserved fallback-source occurrences.
    FallbackSources,
    /// Submitted configured-root occurrences.
    ConfiguredRoots,
    /// Reserved target occurrences for one fallback source.
    FallbackTargetsPerSource,
    /// Decoded bytes in one locale occurrence.
    LocaleBytes,
    /// Decoded bytes in one entry structural path.
    EntryStructuralPathBytes,
    /// Decoded bytes in one definition catalog key.
    CatalogKeyBytes,
    /// Decoded bytes in one message payload.
    MessageBytes,
    /// Aggregate message bytes in one definition artifact.
    TotalMessageBytes,
    /// Decoded bytes in one catalog scope name.
    CatalogScopeNameBytes,
    /// Submitted scope-mapping occurrences.
    ScopeMappingEntries,
    /// Decoded bytes in one exact or prefix selector.
    SelectorPathBytes,
    /// Decoded bytes in one pattern selector.
    SelectorPatternBytes,
    /// Parsed tokens in one pattern selector.
    SelectorPatternTokens,
    /// Aggregate logical pattern-match states in one request.
    PatternMatchStatesTotal,
    /// Decoded bytes in one optional reason.
    ReasonBytes,
    /// Submitted segments in one portable path.
    PathSegments,
    /// Decoded bytes in one portable path segment.
    PathSegmentBytes,
    /// Aggregate segment bytes in one portable path.
    PathBytes,
    /// Submitted logical aliases in one definition artifact.
    LogicalAliases,
    /// Aggregate primary and alias path bytes in one definition artifact.
    SourcePathBytes,
    /// Exact serialized bytes in one reference artifact.
    ReferenceArtifactWireBytes,
    /// Decoded scalar payload bytes in one reference artifact.
    ReferenceArtifactDecodedBytes,
    /// Exact serialized bytes in one definition artifact.
    DefinitionArtifactWireBytes,
    /// Decoded scalar payload bytes in one definition artifact.
    DefinitionArtifactDecodedBytes,
    /// Final retained findings in one outcome.
    FindingsTotal,
    /// Aggregate semantic bytes in final findings.
    FindingBytesTotal,
    /// Canonical delivery-unit and locale plans.
    BundlePlansTotal,
    /// Final resolved-message placements across plans.
    ResolvedMessagesTotal,
    /// Aggregate semantic bytes in plans and placements.
    BundlePlanBytesTotal,
    /// Decoded structural tokens in one catalog key or prefix.
    CatalogKeyTokens,
    /// Decoded bytes in one reference-artifact identity segment.
    ReferenceIdentitySegmentBytes,
    /// Submitted segments in one reference-artifact identity.
    ReferenceIdentitySegments,
    /// Aggregate segment bytes in one reference-artifact identity.
    ReferenceIdentityBytes,
    /// Decoded bytes in one delivery-unit segment.
    DeliveryUnitSegmentBytes,
    /// Submitted segments in one delivery-unit identity.
    DeliveryUnitSegments,
    /// Aggregate segment bytes in one delivery-unit identity.
    DeliveryUnitBytes,
    /// Submitted reference records in one reference artifact.
    ReferenceRecords,
    /// Submitted definition records in one definition artifact.
    Definitions,
    /// Submitted declared-scope coverage-baseline occurrences.
    CoverageBaselines,
    /// Aggregate coverage-baseline scope-name and locale bytes.
    CoverageBaselineBytesTotal,
}

// This registry is the single source of truth for counter names, ceilings,
// activation state, observation semantics, boundary availability, and admitted
// linker subjects. Its order must remain identical to the enum discriminants;
// the registry tests lock that compatibility contract.
const LINK_LIMIT_COUNTER_METADATA: &[LinkLimitCounterMetadata] = &[
    counter_metadata!(
        ReferenceArtifacts,
        "reference_artifacts",
        65_536,
        Active,
        FirstOver,
        LinkOnly,
        [Request]
    ),
    counter_metadata!(
        ReferenceIdentityBytesTotal,
        "reference_identity_bytes_total",
        67_108_864,
        Active,
        ExactOrArithmeticOverflow,
        LinkOnly,
        [ReferenceArtifactGroup]
    ),
    counter_metadata!(
        ReferenceRecordsTotal,
        "reference_records_total",
        4_000_000,
        Active,
        ExactOrArithmeticOverflow,
        LinkOnly,
        [ReferenceArtifactGroup]
    ),
    counter_metadata!(
        ReferenceArtifactDecodedBytesTotal,
        "reference_artifact_decoded_bytes_total",
        1_073_741_824,
        Active,
        ExactOrArithmeticOverflow,
        LinkOnly,
        [ReferenceArtifactGroup]
    ),
    counter_metadata!(
        DefinitionArtifacts,
        "definition_artifacts",
        65_536,
        Active,
        FirstOver,
        LinkOnly,
        [Request]
    ),
    counter_metadata!(
        DefinitionsTotal,
        "definitions_total",
        4_000_000,
        Active,
        ExactOrArithmeticOverflow,
        LinkOnly,
        [DefinitionArtifactGroup]
    ),
    counter_metadata!(
        DefinitionArtifactDecodedBytesTotal,
        "definition_artifact_decoded_bytes_total",
        1_073_741_824,
        Active,
        ExactOrArithmeticOverflow,
        LinkOnly,
        [DefinitionArtifactGroup]
    ),
    counter_metadata!(
        DeliveryGraphNodes,
        "delivery_graph_nodes",
        65_536,
        Active,
        FirstOver,
        LinkOnly,
        [DeliveryGraph]
    ),
    counter_metadata!(
        DeliveryGraphEdges,
        "delivery_graph_edges",
        1_048_576,
        Active,
        FirstOver,
        LinkOnly,
        [DeliveryGraph]
    ),
    counter_metadata!(
        DeliveryGraphIdBytes,
        "delivery_graph_id_bytes",
        67_108_864,
        Active,
        ExactOrArithmeticOverflow,
        LinkOnly,
        [DeliveryUnitGroup]
    ),
    counter_metadata!(
        ProductionLocales,
        "production_locales",
        1_024,
        Active,
        FirstOver,
        LinkOnly,
        [ResolvedPolicy]
    ),
    counter_metadata!(
        FallbackSources,
        "fallback_sources",
        1_024,
        Reserved,
        FirstOver,
        LinkOnly,
        []
    ),
    counter_metadata!(
        ConfiguredRoots,
        "configured_roots",
        4_096,
        Active,
        FirstOver,
        LinkOnly,
        [ResolvedPolicy]
    ),
    counter_metadata!(
        FallbackTargetsPerSource,
        "fallback_targets_per_source",
        64,
        Reserved,
        FirstOver,
        LinkOnly,
        [FallbackSource]
    ),
    counter_metadata!(
        LocaleBytes,
        "locale_bytes",
        255,
        Active,
        FirstOver,
        ArtifactAndLink,
        [DefinitionArtifactGroup, ResolvedPolicy]
    ),
    counter_metadata!(
        EntryStructuralPathBytes,
        "entry_structural_path_bytes",
        67_108_864,
        Active,
        FirstOver,
        ArtifactAndLink,
        [DefinitionArtifactGroup]
    ),
    counter_metadata!(
        CatalogKeyBytes,
        "catalog_key_bytes",
        67_108_864,
        Active,
        FirstOver,
        ArtifactAndLink,
        [DefinitionArtifactGroup]
    ),
    counter_metadata!(
        MessageBytes,
        "message_bytes",
        1_048_576,
        Active,
        FirstOver,
        ArtifactAndLink,
        [DefinitionArtifactGroup]
    ),
    counter_metadata!(
        TotalMessageBytes,
        "total_message_bytes",
        67_108_864,
        Active,
        Exact,
        ArtifactAndLink,
        [DefinitionArtifactGroup]
    ),
    counter_metadata!(
        CatalogScopeNameBytes,
        "catalog_scope_name_bytes",
        255,
        Active,
        FirstOver,
        ArtifactAndLink,
        [
            ReferenceArtifactGroup,
            DefinitionArtifactGroup,
            ResolvedPolicy,
            ScopeMappings
        ]
    ),
    counter_metadata!(
        ScopeMappingEntries,
        "scope_mapping_entries",
        4_096,
        Active,
        FirstOver,
        LinkOnly,
        [ScopeMappings]
    ),
    counter_metadata!(
        SelectorPathBytes,
        "selector_path_bytes",
        67_108_864,
        Active,
        FirstOver,
        ArtifactAndLink,
        [ReferenceArtifactGroup]
    ),
    counter_metadata!(
        SelectorPatternBytes,
        "selector_pattern_bytes",
        134_217_728,
        Active,
        FirstOver,
        ArtifactAndLink,
        [ReferenceArtifactGroup]
    ),
    counter_metadata!(
        SelectorPatternTokens,
        "selector_pattern_tokens",
        513,
        Active,
        FirstOver,
        ArtifactAndLink,
        [ReferenceArtifactGroup]
    ),
    counter_metadata!(
        PatternMatchStatesTotal,
        "pattern_match_states_total",
        100_000_000,
        Active,
        Exact,
        LinkOnly,
        [Request]
    ),
    counter_metadata!(
        ReasonBytes,
        "reason_bytes",
        4_096,
        Active,
        FirstOver,
        ArtifactAndLink,
        [ReferenceArtifactGroup]
    ),
    counter_metadata!(
        PathSegments,
        "path_segments",
        1_024,
        Active,
        FirstOver,
        ArtifactAndLink,
        [
            DefinitionArtifactEnvelope,
            ReferenceArtifactGroup,
            DefinitionArtifactGroup
        ]
    ),
    counter_metadata!(
        PathSegmentBytes,
        "path_segment_bytes",
        4_096,
        Active,
        FirstOver,
        ArtifactAndLink,
        [
            DefinitionArtifactEnvelope,
            ReferenceArtifactGroup,
            DefinitionArtifactGroup
        ]
    ),
    counter_metadata!(
        PathBytes,
        "path_bytes",
        262_144,
        Active,
        Exact,
        ArtifactAndLink,
        [
            DefinitionArtifactEnvelope,
            ReferenceArtifactGroup,
            DefinitionArtifactGroup
        ]
    ),
    counter_metadata!(
        LogicalAliases,
        "logical_aliases",
        4_096,
        Active,
        FirstOver,
        ArtifactAndLink,
        [DefinitionArtifactGroup]
    ),
    counter_metadata!(
        SourcePathBytes,
        "source_path_bytes",
        67_108_864,
        Active,
        Exact,
        ArtifactAndLink,
        [DefinitionArtifactGroup]
    ),
    counter_metadata!(
        ReferenceArtifactWireBytes,
        "reference_artifact_wire_bytes",
        536_870_912,
        Active,
        FirstOver,
        ArtifactOnly,
        []
    ),
    counter_metadata!(
        ReferenceArtifactDecodedBytes,
        "reference_artifact_decoded_bytes",
        268_435_456,
        Active,
        Exact,
        ArtifactAndLink,
        [ReferenceArtifactGroup]
    ),
    counter_metadata!(
        DefinitionArtifactWireBytes,
        "definition_artifact_wire_bytes",
        536_870_912,
        Active,
        FirstOver,
        ArtifactOnly,
        []
    ),
    counter_metadata!(
        DefinitionArtifactDecodedBytes,
        "definition_artifact_decoded_bytes",
        268_435_456,
        Active,
        Exact,
        ArtifactAndLink,
        [DefinitionArtifactGroup]
    ),
    counter_metadata!(
        FindingsTotal,
        "findings_total",
        1_000_000,
        Active,
        FirstOver,
        LinkOnly,
        [Request]
    ),
    counter_metadata!(
        FindingBytesTotal,
        "finding_bytes_total",
        268_435_456,
        Active,
        Exact,
        LinkOnly,
        [Request]
    ),
    counter_metadata!(
        BundlePlansTotal,
        "bundle_plans_total",
        1_048_576,
        Active,
        FirstOver,
        LinkOnly,
        [Request]
    ),
    counter_metadata!(
        ResolvedMessagesTotal,
        "resolved_messages_total",
        4_000_000,
        Active,
        FirstOver,
        LinkOnly,
        [Request]
    ),
    counter_metadata!(
        BundlePlanBytesTotal,
        "bundle_plan_bytes_total",
        1_073_741_824,
        Active,
        Exact,
        LinkOnly,
        [Request]
    ),
    counter_metadata!(
        CatalogKeyTokens,
        "catalog_key_tokens",
        256,
        Active,
        FirstOver,
        ArtifactAndLink,
        [ReferenceArtifactGroup, DefinitionArtifactGroup]
    ),
    counter_metadata!(
        ReferenceIdentitySegmentBytes,
        "reference_identity_segment_bytes",
        255,
        Active,
        FirstOver,
        ArtifactAndLink,
        [ReferenceArtifactGroup]
    ),
    counter_metadata!(
        ReferenceIdentitySegments,
        "reference_identity_segments",
        64,
        Active,
        FirstOver,
        ArtifactAndLink,
        [ReferenceArtifactGroup]
    ),
    counter_metadata!(
        ReferenceIdentityBytes,
        "reference_identity_bytes",
        4_096,
        Active,
        Exact,
        ArtifactAndLink,
        [ReferenceArtifactGroup]
    ),
    counter_metadata!(
        DeliveryUnitSegmentBytes,
        "delivery_unit_segment_bytes",
        255,
        Active,
        FirstOver,
        ArtifactAndLink,
        [ReferenceArtifactGroup, DeliveryUnitGroup]
    ),
    counter_metadata!(
        DeliveryUnitSegments,
        "delivery_unit_segments",
        64,
        Active,
        FirstOver,
        ArtifactAndLink,
        [ReferenceArtifactGroup, DeliveryUnitGroup]
    ),
    counter_metadata!(
        DeliveryUnitBytes,
        "delivery_unit_bytes",
        4_096,
        Active,
        Exact,
        ArtifactAndLink,
        [ReferenceArtifactGroup, DeliveryUnitGroup]
    ),
    counter_metadata!(
        ReferenceRecords,
        "reference_records",
        1_000_000,
        Active,
        FirstOver,
        ArtifactAndLink,
        [ReferenceArtifactGroup]
    ),
    counter_metadata!(
        Definitions,
        "definitions",
        100_000,
        Active,
        FirstOver,
        ArtifactAndLink,
        [DefinitionArtifactGroup]
    ),
    counter_metadata!(
        CoverageBaselines,
        "coverage_baselines",
        4_096,
        Active,
        FirstOver,
        LinkOnly,
        [ResolvedPolicy]
    ),
    counter_metadata!(
        CoverageBaselineBytesTotal,
        "coverage_baseline_bytes_total",
        2_088_960,
        Active,
        Exact,
        LinkOnly,
        [ResolvedPolicy]
    ),
];

const COUNTER_COUNT: usize = LINK_LIMIT_COUNTER_METADATA.len();

impl LinkLimitCounter {
    /// All counters in their compatibility-stable declaration order.
    pub const ALL: [Self; COUNTER_COUNT] = Self::build_all();

    const fn build_all() -> [Self; COUNTER_COUNT] {
        let mut counters = [Self::ReferenceArtifacts; COUNTER_COUNT];
        let mut index = 0;
        while index < COUNTER_COUNT {
            counters[index] = LINK_LIMIT_COUNTER_METADATA[index].counter;
            index += 1;
        }
        counters
    }

    /// Return the one-based compatibility ordinal.
    #[must_use]
    pub const fn ordinal(self) -> u8 {
        self as u8 + 1
    }

    /// Return the exact structured spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.metadata().name
    }

    /// Return the inclusive protocol hard ceiling.
    #[must_use]
    pub const fn protocol_ceiling(self) -> u64 {
        self.metadata().protocol_ceiling
    }

    /// Return whether this counter is reserved by the current protocol.
    #[must_use]
    pub const fn is_reserved(self) -> bool {
        matches!(self.metadata().state, CounterState::Reserved)
    }

    const fn index(self) -> usize {
        self as usize
    }

    const fn metadata(self) -> LinkLimitCounterMetadata {
        LINK_LIMIT_COUNTER_METADATA[self.index()]
    }

    const fn requires_first_over(self) -> bool {
        matches!(self.metadata().observation, ObservationContract::FirstOver)
    }

    const fn allows_arithmetic_overflow(self) -> bool {
        matches!(
            self.metadata().observation,
            ObservationContract::ExactOrArithmeticOverflow
        )
    }

    const fn is_artifact_counter(self) -> bool {
        matches!(
            self.metadata().boundary,
            CounterBoundary::ArtifactAndLink | CounterBoundary::ArtifactOnly
        )
    }

    const fn is_artifact_only(self) -> bool {
        matches!(self.metadata().boundary, CounterBoundary::ArtifactOnly)
    }

    const fn allows_subject(self, subject: LinkLimitSubjectKind) -> bool {
        self.metadata().subject_mask & subject as u16 != 0
    }

    /// Enforce this counter's protocol ceiling while constructing a checked value.
    pub(crate) fn check_construction_limit(
        self,
        observed: u64,
    ) -> Result<(), ValueConstructionError> {
        let limit = self.protocol_ceiling();
        if observed <= limit {
            return Ok(());
        }
        let attempted = if self.requires_first_over() {
            limit + 1
        } else {
            observed
        };
        Err(ValueConstructionError::field_limit(self, limit, attempted))
    }

    /// Revalidate one artifact observation against the caller's effective limit.
    pub(crate) fn check_artifact_limit(
        self,
        observed: u64,
        limits: &LinkLimits,
    ) -> Result<(), ArtifactLimitEvidence> {
        let limit = limits.effective_limit(self);
        if observed <= limit {
            return Ok(());
        }
        let observation = if self.requires_first_over() {
            LinkLimitObservation::Exact(limit + 1)
        } else {
            LinkLimitObservation::Exact(observed)
        };
        Err(ArtifactLimitEvidence::try_new(self, limit, observation)
            .expect("checked artifact limit evidence is valid"))
    }
}

/// Immutable effective limits for one artifact/link invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkLimits {
    values: [u64; COUNTER_COUNT],
}

impl LinkLimits {
    /// Construct the exact protocol-default limit set.
    #[must_use]
    pub fn protocol_defaults() -> Self {
        let mut values = [0; COUNTER_COUNT];
        let mut index = 0;
        while index < COUNTER_COUNT {
            values[index] = LinkLimitCounter::ALL[index].protocol_ceiling();
            index += 1;
        }
        Self { values }
    }

    /// Return a new limit set with one active counter lowered or restored.
    pub fn try_with_limit(
        mut self,
        counter: LinkLimitCounter,
        value: u64,
    ) -> Result<Self, LinkLimitConfigurationError> {
        if counter.is_reserved() || value > counter.protocol_ceiling() {
            return Err(LinkLimitConfigurationError {
                counter,
                submitted: value,
            });
        }
        self.values[counter.index()] = value;
        Ok(self)
    }

    /// Return the effective immutable value for one counter.
    #[must_use]
    pub const fn effective_limit(&self, counter: LinkLimitCounter) -> u64 {
        self.values[counter.index()]
    }
}

impl Default for LinkLimits {
    fn default() -> Self {
        Self::protocol_defaults()
    }
}

/// Invalid caller-selected lower-limit configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkLimitConfigurationError {
    counter: LinkLimitCounter,
    submitted: u64,
}

impl LinkLimitConfigurationError {
    /// Return the rejected closed counter.
    #[must_use]
    pub const fn counter(&self) -> LinkLimitCounter {
        self.counter
    }

    /// Return the exact rejected value.
    #[must_use]
    pub const fn submitted(&self) -> u64 {
        self.submitted
    }

    /// Return the fixed protocol ceiling derived from the counter.
    #[must_use]
    pub const fn protocol_ceiling(&self) -> u64 {
        self.counter.protocol_ceiling()
    }
}

impl fmt::Display for LinkLimitConfigurationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.counter.is_reserved() {
            return write!(
                formatter,
                "{} is reserved by the current protocol and cannot be configured",
                self.counter.as_str()
            );
        }
        write!(
            formatter,
            "{} limit {} exceeds protocol ceiling {}",
            self.counter.as_str(),
            self.submitted,
            self.counter.protocol_ceiling()
        )
    }
}

impl Error for LinkLimitConfigurationError {}

/// Canonical observation attached to one limit failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinkLimitObservation {
    /// Exact first attempted value above the effective limit.
    Exact(u64),
    /// Checked arithmetic could not retain an exact attempted value.
    ArithmeticOverflow,
}

/// Bounded semantic subject attached to linker-side limit evidence.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LinkLimitSubject {
    /// Whole link request.
    Request,
    /// Definition envelope before a checked source identity exists.
    DefinitionArtifactEnvelope,
    /// Canonical equal-identity reference artifact group.
    ReferenceArtifactGroup(ReferenceArtifactIdentity),
    /// Canonical equal-source definition artifact group.
    DefinitionArtifactGroup(SourceDocumentIdentity),
    /// Complete delivery graph.
    DeliveryGraph,
    /// Canonical equal-ID delivery-unit group.
    DeliveryUnitGroup(DeliveryUnitId),
    /// Complete resolved link policy.
    ResolvedPolicy,
    /// Canonical checked fallback source reserved for a future protocol revision.
    FallbackSource(Locale),
    /// Complete scope-mapping table.
    ScopeMappings,
}

impl LinkLimitSubject {
    const fn kind(&self) -> LinkLimitSubjectKind {
        match self {
            Self::Request => LinkLimitSubjectKind::Request,
            Self::DefinitionArtifactEnvelope => LinkLimitSubjectKind::DefinitionArtifactEnvelope,
            Self::ReferenceArtifactGroup(_) => LinkLimitSubjectKind::ReferenceArtifactGroup,
            Self::DefinitionArtifactGroup(_) => LinkLimitSubjectKind::DefinitionArtifactGroup,
            Self::DeliveryGraph => LinkLimitSubjectKind::DeliveryGraph,
            Self::DeliveryUnitGroup(_) => LinkLimitSubjectKind::DeliveryUnitGroup,
            Self::ResolvedPolicy => LinkLimitSubjectKind::ResolvedPolicy,
            Self::FallbackSource(_) => LinkLimitSubjectKind::FallbackSource,
            Self::ScopeMappings => LinkLimitSubjectKind::ScopeMappings,
        }
    }
}

/// Invalid attempted construction of limit evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkLimitEvidenceConstructionError {
    /// The supplied effective limit is above the counter's protocol ceiling.
    EffectiveLimitAboveProtocol,
    /// The counter is reserved by the current protocol.
    ReservedCounter,
    /// The exact observation does not exceed the effective limit.
    NonExceedingObservation,
    /// A first-over counter used a larger non-canonical attempted value.
    NonCanonicalObservation,
    /// Arithmetic overflow is impossible for this bounded counter.
    ArithmeticOverflowUnavailable,
    /// The counter and semantic subject do not form an admitted pair.
    InvalidSubject,
    /// A serialized wire counter cannot occur at the typed linker boundary.
    WireCounterAtLinkBoundary,
    /// The counter does not occur at a single-artifact contract boundary.
    CounterOutsideArtifactBoundary,
}

impl fmt::Display for LinkLimitEvidenceConstructionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid link limit evidence: {self:?}")
    }
}

impl Error for LinkLimitEvidenceConstructionError {}

/// Subject-free limit evidence for one artifact constructor or codec operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ArtifactLimitEvidence {
    counter: LinkLimitCounter,
    effective_limit: u64,
    observation: LinkLimitObservation,
}

impl ArtifactLimitEvidence {
    /// Construct evidence only when the counter, limit, and observation are coherent.
    pub fn try_new(
        counter: LinkLimitCounter,
        effective_limit: u64,
        observation: LinkLimitObservation,
    ) -> Result<Self, LinkLimitEvidenceConstructionError> {
        validate_common_evidence(counter, effective_limit, observation)?;
        if !counter.is_artifact_counter() {
            return Err(LinkLimitEvidenceConstructionError::CounterOutsideArtifactBoundary);
        }
        Ok(Self {
            counter,
            effective_limit,
            observation,
        })
    }

    /// Return the closed counter.
    #[must_use]
    pub const fn counter(&self) -> LinkLimitCounter {
        self.counter
    }

    /// Return the exact effective limit.
    #[must_use]
    pub const fn effective_limit(&self) -> u64 {
        self.effective_limit
    }

    /// Return the canonical overrun observation.
    #[must_use]
    pub const fn observation(&self) -> LinkLimitObservation {
        self.observation
    }
}

/// Checked linker-side limit evidence with a type-safe bounded subject.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LinkLimitEvidence {
    counter: LinkLimitCounter,
    subject: LinkLimitSubject,
    effective_limit: u64,
    observation: LinkLimitObservation,
}

impl LinkLimitEvidence {
    /// Construct evidence only for one design-admitted counter/subject pair.
    pub fn try_new(
        counter: LinkLimitCounter,
        subject: LinkLimitSubject,
        effective_limit: u64,
        observation: LinkLimitObservation,
    ) -> Result<Self, LinkLimitEvidenceConstructionError> {
        validate_common_evidence(counter, effective_limit, observation)?;
        if counter.is_artifact_only() {
            return Err(LinkLimitEvidenceConstructionError::WireCounterAtLinkBoundary);
        }
        if !valid_subject(counter, &subject) {
            return Err(LinkLimitEvidenceConstructionError::InvalidSubject);
        }
        Ok(Self {
            counter,
            subject,
            effective_limit,
            observation,
        })
    }

    /// Return the closed counter.
    #[must_use]
    pub const fn counter(&self) -> LinkLimitCounter {
        self.counter
    }

    /// Return the exact bounded semantic subject.
    #[must_use]
    pub const fn subject(&self) -> &LinkLimitSubject {
        &self.subject
    }

    /// Return the exact effective limit.
    #[must_use]
    pub const fn effective_limit(&self) -> u64 {
        self.effective_limit
    }

    /// Return the canonical overrun observation.
    #[must_use]
    pub const fn observation(&self) -> LinkLimitObservation {
        self.observation
    }
}

fn validate_common_evidence(
    counter: LinkLimitCounter,
    effective_limit: u64,
    observation: LinkLimitObservation,
) -> Result<(), LinkLimitEvidenceConstructionError> {
    if counter.is_reserved() {
        return Err(LinkLimitEvidenceConstructionError::ReservedCounter);
    }
    if effective_limit > counter.protocol_ceiling() {
        return Err(LinkLimitEvidenceConstructionError::EffectiveLimitAboveProtocol);
    }
    match observation {
        LinkLimitObservation::Exact(attempted) if attempted <= effective_limit => {
            Err(LinkLimitEvidenceConstructionError::NonExceedingObservation)
        }
        LinkLimitObservation::Exact(attempted)
            if counter.requires_first_over() && attempted != effective_limit + 1 =>
        {
            Err(LinkLimitEvidenceConstructionError::NonCanonicalObservation)
        }
        LinkLimitObservation::ArithmeticOverflow if !counter.allows_arithmetic_overflow() => {
            Err(LinkLimitEvidenceConstructionError::ArithmeticOverflowUnavailable)
        }
        LinkLimitObservation::Exact(_) | LinkLimitObservation::ArithmeticOverflow => Ok(()),
    }
}

fn valid_subject(counter: LinkLimitCounter, subject: &LinkLimitSubject) -> bool {
    counter.allows_subject(subject.kind())
}

#[cfg(test)]
mod tests {
    use super::{
        ArtifactLimitEvidence, LinkLimitCounter, LinkLimitEvidence,
        LinkLimitEvidenceConstructionError, LinkLimitObservation, LinkLimitSubject,
        LinkLimitSubjectKind, LinkLimits,
    };
    use crate::{
        ArtifactNamespace, DeliveryUnitId, DeliveryUnitSegment, PortablePathSegment,
        PortableRelativePath, ReferenceArtifactIdentity, ReferenceArtifactSegment,
        SourceDocumentIdentity,
    };

    #[test]
    fn counter_registry_has_exact_stable_ordinals_and_spellings() {
        let expected = [
            "reference_artifacts",
            "reference_identity_bytes_total",
            "reference_records_total",
            "reference_artifact_decoded_bytes_total",
            "definition_artifacts",
            "definitions_total",
            "definition_artifact_decoded_bytes_total",
            "delivery_graph_nodes",
            "delivery_graph_edges",
            "delivery_graph_id_bytes",
            "production_locales",
            "fallback_sources",
            "configured_roots",
            "fallback_targets_per_source",
            "locale_bytes",
            "entry_structural_path_bytes",
            "catalog_key_bytes",
            "message_bytes",
            "total_message_bytes",
            "catalog_scope_name_bytes",
            "scope_mapping_entries",
            "selector_path_bytes",
            "selector_pattern_bytes",
            "selector_pattern_tokens",
            "pattern_match_states_total",
            "reason_bytes",
            "path_segments",
            "path_segment_bytes",
            "path_bytes",
            "logical_aliases",
            "source_path_bytes",
            "reference_artifact_wire_bytes",
            "reference_artifact_decoded_bytes",
            "definition_artifact_wire_bytes",
            "definition_artifact_decoded_bytes",
            "findings_total",
            "finding_bytes_total",
            "bundle_plans_total",
            "resolved_messages_total",
            "bundle_plan_bytes_total",
            "catalog_key_tokens",
            "reference_identity_segment_bytes",
            "reference_identity_segments",
            "reference_identity_bytes",
            "delivery_unit_segment_bytes",
            "delivery_unit_segments",
            "delivery_unit_bytes",
            "reference_records",
            "definitions",
            "coverage_baselines",
            "coverage_baseline_bytes_total",
        ];
        let expected_ceilings = [
            65_536,
            67_108_864,
            4_000_000,
            1_073_741_824,
            65_536,
            4_000_000,
            1_073_741_824,
            65_536,
            1_048_576,
            67_108_864,
            1_024,
            1_024,
            4_096,
            64,
            255,
            67_108_864,
            67_108_864,
            1_048_576,
            67_108_864,
            255,
            4_096,
            67_108_864,
            134_217_728,
            513,
            100_000_000,
            4_096,
            1_024,
            4_096,
            262_144,
            4_096,
            67_108_864,
            536_870_912,
            268_435_456,
            536_870_912,
            268_435_456,
            1_000_000,
            268_435_456,
            1_048_576,
            4_000_000,
            1_073_741_824,
            256,
            255,
            64,
            4_096,
            255,
            64,
            4_096,
            1_000_000,
            100_000,
            4_096,
            2_088_960,
        ];
        assert_eq!(LinkLimitCounter::ALL.len(), expected.len());
        assert_eq!(LinkLimitCounter::ALL.len(), expected_ceilings.len());
        for (index, ((counter, spelling), ceiling)) in LinkLimitCounter::ALL
            .into_iter()
            .zip(expected)
            .zip(expected_ceilings)
            .enumerate()
        {
            assert_eq!(counter.ordinal() as usize, index + 1);
            assert_eq!(counter.metadata().counter, counter);
            assert_eq!(counter.as_str(), spelling);
            assert_eq!(counter.protocol_ceiling(), ceiling);
        }
    }

    #[test]
    fn counter_registry_keeps_exact_state_observation_and_boundary_contracts() {
        use LinkLimitCounter as Counter;

        let reserved = [Counter::FallbackSources, Counter::FallbackTargetsPerSource];
        let first_over = [
            Counter::ReferenceArtifacts,
            Counter::DefinitionArtifacts,
            Counter::DeliveryGraphNodes,
            Counter::DeliveryGraphEdges,
            Counter::ProductionLocales,
            Counter::FallbackSources,
            Counter::ConfiguredRoots,
            Counter::FallbackTargetsPerSource,
            Counter::LocaleBytes,
            Counter::EntryStructuralPathBytes,
            Counter::CatalogKeyBytes,
            Counter::MessageBytes,
            Counter::CatalogScopeNameBytes,
            Counter::ScopeMappingEntries,
            Counter::SelectorPathBytes,
            Counter::SelectorPatternBytes,
            Counter::SelectorPatternTokens,
            Counter::ReasonBytes,
            Counter::PathSegments,
            Counter::PathSegmentBytes,
            Counter::LogicalAliases,
            Counter::ReferenceArtifactWireBytes,
            Counter::DefinitionArtifactWireBytes,
            Counter::FindingsTotal,
            Counter::BundlePlansTotal,
            Counter::ResolvedMessagesTotal,
            Counter::CatalogKeyTokens,
            Counter::ReferenceIdentitySegmentBytes,
            Counter::ReferenceIdentitySegments,
            Counter::DeliveryUnitSegmentBytes,
            Counter::DeliveryUnitSegments,
            Counter::ReferenceRecords,
            Counter::Definitions,
            Counter::CoverageBaselines,
        ];
        let arithmetic_overflow = [
            Counter::ReferenceIdentityBytesTotal,
            Counter::ReferenceRecordsTotal,
            Counter::ReferenceArtifactDecodedBytesTotal,
            Counter::DefinitionsTotal,
            Counter::DefinitionArtifactDecodedBytesTotal,
            Counter::DeliveryGraphIdBytes,
        ];
        let artifact = [
            Counter::LocaleBytes,
            Counter::EntryStructuralPathBytes,
            Counter::CatalogKeyBytes,
            Counter::MessageBytes,
            Counter::TotalMessageBytes,
            Counter::CatalogScopeNameBytes,
            Counter::SelectorPathBytes,
            Counter::SelectorPatternBytes,
            Counter::SelectorPatternTokens,
            Counter::ReasonBytes,
            Counter::PathSegments,
            Counter::PathSegmentBytes,
            Counter::PathBytes,
            Counter::LogicalAliases,
            Counter::SourcePathBytes,
            Counter::ReferenceArtifactWireBytes,
            Counter::ReferenceArtifactDecodedBytes,
            Counter::DefinitionArtifactWireBytes,
            Counter::DefinitionArtifactDecodedBytes,
            Counter::CatalogKeyTokens,
            Counter::ReferenceIdentitySegmentBytes,
            Counter::ReferenceIdentitySegments,
            Counter::ReferenceIdentityBytes,
            Counter::DeliveryUnitSegmentBytes,
            Counter::DeliveryUnitSegments,
            Counter::DeliveryUnitBytes,
            Counter::ReferenceRecords,
            Counter::Definitions,
        ];
        let artifact_only = [
            Counter::ReferenceArtifactWireBytes,
            Counter::DefinitionArtifactWireBytes,
        ];

        for counter in Counter::ALL {
            assert_eq!(counter.is_reserved(), reserved.contains(&counter));
            assert_eq!(counter.requires_first_over(), first_over.contains(&counter));
            assert_eq!(
                counter.allows_arithmetic_overflow(),
                arithmetic_overflow.contains(&counter)
            );
            assert_eq!(counter.is_artifact_counter(), artifact.contains(&counter));
            assert_eq!(counter.is_artifact_only(), artifact_only.contains(&counter));
            assert!(
                !(counter.requires_first_over() && counter.allows_arithmetic_overflow()),
                "{} has conflicting observation contracts",
                counter.as_str()
            );
        }
    }

    #[test]
    fn counter_registry_keeps_exact_subject_contracts() {
        use LinkLimitCounter as Counter;
        use LinkLimitSubjectKind as Subject;

        let expected: &[(Subject, &[Counter])] = &[
            (
                Subject::Request,
                &[
                    Counter::ReferenceArtifacts,
                    Counter::DefinitionArtifacts,
                    Counter::PatternMatchStatesTotal,
                    Counter::FindingsTotal,
                    Counter::FindingBytesTotal,
                    Counter::BundlePlansTotal,
                    Counter::ResolvedMessagesTotal,
                    Counter::BundlePlanBytesTotal,
                ],
            ),
            (
                Subject::DefinitionArtifactEnvelope,
                &[
                    Counter::PathSegments,
                    Counter::PathSegmentBytes,
                    Counter::PathBytes,
                ],
            ),
            (
                Subject::ReferenceArtifactGroup,
                &[
                    Counter::ReferenceIdentityBytesTotal,
                    Counter::ReferenceRecordsTotal,
                    Counter::ReferenceArtifactDecodedBytesTotal,
                    Counter::CatalogScopeNameBytes,
                    Counter::SelectorPathBytes,
                    Counter::SelectorPatternBytes,
                    Counter::SelectorPatternTokens,
                    Counter::ReasonBytes,
                    Counter::PathSegments,
                    Counter::PathSegmentBytes,
                    Counter::PathBytes,
                    Counter::ReferenceArtifactDecodedBytes,
                    Counter::CatalogKeyTokens,
                    Counter::ReferenceIdentitySegmentBytes,
                    Counter::ReferenceIdentitySegments,
                    Counter::ReferenceIdentityBytes,
                    Counter::DeliveryUnitSegmentBytes,
                    Counter::DeliveryUnitSegments,
                    Counter::DeliveryUnitBytes,
                    Counter::ReferenceRecords,
                ],
            ),
            (
                Subject::DefinitionArtifactGroup,
                &[
                    Counter::DefinitionsTotal,
                    Counter::DefinitionArtifactDecodedBytesTotal,
                    Counter::LocaleBytes,
                    Counter::EntryStructuralPathBytes,
                    Counter::CatalogKeyBytes,
                    Counter::MessageBytes,
                    Counter::TotalMessageBytes,
                    Counter::CatalogScopeNameBytes,
                    Counter::PathSegments,
                    Counter::PathSegmentBytes,
                    Counter::PathBytes,
                    Counter::LogicalAliases,
                    Counter::SourcePathBytes,
                    Counter::DefinitionArtifactDecodedBytes,
                    Counter::CatalogKeyTokens,
                    Counter::Definitions,
                ],
            ),
            (
                Subject::DeliveryGraph,
                &[Counter::DeliveryGraphNodes, Counter::DeliveryGraphEdges],
            ),
            (
                Subject::DeliveryUnitGroup,
                &[
                    Counter::DeliveryGraphIdBytes,
                    Counter::DeliveryUnitSegmentBytes,
                    Counter::DeliveryUnitSegments,
                    Counter::DeliveryUnitBytes,
                ],
            ),
            (
                Subject::ResolvedPolicy,
                &[
                    Counter::ProductionLocales,
                    Counter::ConfiguredRoots,
                    Counter::LocaleBytes,
                    Counter::CatalogScopeNameBytes,
                    Counter::CoverageBaselines,
                    Counter::CoverageBaselineBytesTotal,
                ],
            ),
            (
                Subject::FallbackSource,
                &[Counter::FallbackTargetsPerSource],
            ),
            (
                Subject::ScopeMappings,
                &[Counter::ScopeMappingEntries, Counter::CatalogScopeNameBytes],
            ),
        ];

        for (subject, expected_counters) in expected {
            for counter in Counter::ALL {
                assert_eq!(
                    counter.allows_subject(*subject),
                    expected_counters.contains(&counter),
                    "{} has an unexpected {subject:?} association",
                    counter.as_str()
                );
            }
        }
    }

    #[test]
    fn default_is_exactly_protocol_defaults() {
        assert_eq!(LinkLimits::default(), LinkLimits::protocol_defaults());
        for counter in LinkLimitCounter::ALL {
            assert_eq!(
                LinkLimits::default().effective_limit(counter),
                counter.protocol_ceiling()
            );
        }
    }

    #[test]
    fn active_lower_limits_accept_zero_exact_and_reject_first_over() {
        for counter in LinkLimitCounter::ALL {
            if counter.is_reserved() {
                continue;
            }
            let ceiling = counter.protocol_ceiling();
            assert!(
                LinkLimits::default().try_with_limit(counter, 0).is_ok(),
                "zero for {}",
                counter.as_str()
            );
            assert!(
                LinkLimits::default()
                    .try_with_limit(counter, ceiling)
                    .is_ok(),
                "exact ceiling for {}",
                counter.as_str()
            );
            let error = LinkLimits::default()
                .try_with_limit(counter, ceiling + 1)
                .unwrap_err();
            assert_eq!(error.counter(), counter);
            assert_eq!(error.submitted(), ceiling + 1);
            assert_eq!(error.protocol_ceiling(), ceiling);
        }
    }

    #[test]
    fn fallback_counters_are_reserved_and_cannot_be_lowered() {
        for counter in [
            LinkLimitCounter::FallbackSources,
            LinkLimitCounter::FallbackTargetsPerSource,
        ] {
            assert!(counter.is_reserved());
            assert_eq!(
                LinkLimits::default()
                    .try_with_limit(counter, 0)
                    .unwrap_err()
                    .counter(),
                counter
            );
        }
    }

    #[test]
    fn limit_updates_are_immutable_and_independent() {
        let defaults = LinkLimits::default();
        let lowered = defaults
            .clone()
            .try_with_limit(LinkLimitCounter::LocaleBytes, 10)
            .unwrap()
            .try_with_limit(LinkLimitCounter::ReasonBytes, 20)
            .unwrap();
        assert_eq!(defaults.effective_limit(LinkLimitCounter::LocaleBytes), 255);
        assert_eq!(lowered.effective_limit(LinkLimitCounter::LocaleBytes), 10);
        assert_eq!(lowered.effective_limit(LinkLimitCounter::ReasonBytes), 20);
    }

    #[test]
    fn evidence_requires_an_actual_overrun_and_valid_subject() {
        let identity = ReferenceArtifactIdentity::try_new(
            ArtifactNamespace::Project,
            vec![ReferenceArtifactSegment::try_new("js").unwrap()],
        )
        .unwrap();
        let evidence = LinkLimitEvidence::try_new(
            LinkLimitCounter::SelectorPathBytes,
            LinkLimitSubject::ReferenceArtifactGroup(identity),
            4,
            LinkLimitObservation::Exact(5),
        )
        .unwrap();
        assert_eq!(evidence.effective_limit(), 4);
        assert_eq!(evidence.observation(), LinkLimitObservation::Exact(5));

        assert_eq!(
            LinkLimitEvidence::try_new(
                LinkLimitCounter::SelectorPathBytes,
                LinkLimitSubject::Request,
                4,
                LinkLimitObservation::Exact(5),
            )
            .unwrap_err(),
            LinkLimitEvidenceConstructionError::InvalidSubject
        );
        assert_eq!(
            LinkLimitEvidence::try_new(
                LinkLimitCounter::FindingsTotal,
                LinkLimitSubject::Request,
                4,
                LinkLimitObservation::Exact(4),
            )
            .unwrap_err(),
            LinkLimitEvidenceConstructionError::NonExceedingObservation
        );
        assert_eq!(
            LinkLimitEvidence::try_new(
                LinkLimitCounter::FindingsTotal,
                LinkLimitSubject::Request,
                4,
                LinkLimitObservation::Exact(6),
            )
            .unwrap_err(),
            LinkLimitEvidenceConstructionError::NonCanonicalObservation
        );
    }

    #[test]
    fn wire_counters_are_artifact_only() {
        assert!(ArtifactLimitEvidence::try_new(
            LinkLimitCounter::ReferenceArtifactWireBytes,
            8,
            LinkLimitObservation::Exact(9),
        )
        .is_ok());
        assert_eq!(
            LinkLimitEvidence::try_new(
                LinkLimitCounter::ReferenceArtifactWireBytes,
                LinkLimitSubject::Request,
                8,
                LinkLimitObservation::Exact(9),
            )
            .unwrap_err(),
            LinkLimitEvidenceConstructionError::WireCounterAtLinkBoundary
        );
    }

    #[test]
    fn arithmetic_overflow_is_rejected_for_bounded_first_over_counters() {
        assert_eq!(
            ArtifactLimitEvidence::try_new(
                LinkLimitCounter::ReasonBytes,
                4,
                LinkLimitObservation::ArithmeticOverflow,
            )
            .unwrap_err(),
            LinkLimitEvidenceConstructionError::ArithmeticOverflowUnavailable
        );

        assert_eq!(
            LinkLimitEvidence::try_new(
                LinkLimitCounter::CoverageBaselineBytesTotal,
                LinkLimitSubject::ResolvedPolicy,
                4,
                LinkLimitObservation::ArithmeticOverflow,
            )
            .unwrap_err(),
            LinkLimitEvidenceConstructionError::ArithmeticOverflowUnavailable
        );
    }

    #[test]
    fn coverage_baseline_counters_are_policy_only() {
        for counter in [
            LinkLimitCounter::CoverageBaselines,
            LinkLimitCounter::CoverageBaselineBytesTotal,
        ] {
            assert_eq!(
                ArtifactLimitEvidence::try_new(counter, 0, LinkLimitObservation::Exact(1))
                    .unwrap_err(),
                LinkLimitEvidenceConstructionError::CounterOutsideArtifactBoundary
            );
            assert!(LinkLimitEvidence::try_new(
                counter,
                LinkLimitSubject::ResolvedPolicy,
                0,
                LinkLimitObservation::Exact(1),
            )
            .is_ok());
        }
    }

    #[test]
    fn appended_identity_and_collection_counters_keep_exact_subject_contracts() {
        let reference_identity = ReferenceArtifactIdentity::try_new(
            ArtifactNamespace::Project,
            vec![ReferenceArtifactSegment::try_new("js").unwrap()],
        )
        .unwrap();
        assert!(LinkLimitEvidence::try_new(
            LinkLimitCounter::ReferenceIdentityBytes,
            LinkLimitSubject::ReferenceArtifactGroup(reference_identity),
            1,
            LinkLimitObservation::Exact(2),
        )
        .is_ok());

        let delivery =
            DeliveryUnitId::try_new(vec![DeliveryUnitSegment::try_new("main").unwrap()]).unwrap();
        assert!(LinkLimitEvidence::try_new(
            LinkLimitCounter::DeliveryUnitBytes,
            LinkLimitSubject::DeliveryUnitGroup(delivery),
            1,
            LinkLimitObservation::Exact(2),
        )
        .is_ok());

        let source = SourceDocumentIdentity::new(
            ArtifactNamespace::Project,
            PortableRelativePath::try_new(vec![
                PortablePathSegment::try_new("messages.json").unwrap()
            ])
            .unwrap(),
        );
        assert!(LinkLimitEvidence::try_new(
            LinkLimitCounter::Definitions,
            LinkLimitSubject::DefinitionArtifactGroup(source),
            0,
            LinkLimitObservation::Exact(1),
        )
        .is_ok());

        assert!(ArtifactLimitEvidence::try_new(
            LinkLimitCounter::ReferenceRecords,
            0,
            LinkLimitObservation::Exact(1),
        )
        .is_ok());
    }
}
