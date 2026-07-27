// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::error::Error;
use std::fmt;

use crate::{DeliveryUnitId, Locale, ReferenceArtifactIdentity, SourceDocumentIdentity};

const COUNTER_COUNT: usize = 49;

/// Closed M0/M1 linker and artifact resource-counter vocabulary.
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
    /// Reserved M2 fallback-source occurrences.
    FallbackSources,
    /// Submitted configured-root occurrences.
    ConfiguredRoots,
    /// Reserved M2 target occurrences for one fallback source.
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
}

impl LinkLimitCounter {
    /// All counters in their compatibility-stable declaration order.
    pub const ALL: [Self; COUNTER_COUNT] = [
        Self::ReferenceArtifacts,
        Self::ReferenceIdentityBytesTotal,
        Self::ReferenceRecordsTotal,
        Self::ReferenceArtifactDecodedBytesTotal,
        Self::DefinitionArtifacts,
        Self::DefinitionsTotal,
        Self::DefinitionArtifactDecodedBytesTotal,
        Self::DeliveryGraphNodes,
        Self::DeliveryGraphEdges,
        Self::DeliveryGraphIdBytes,
        Self::ProductionLocales,
        Self::FallbackSources,
        Self::ConfiguredRoots,
        Self::FallbackTargetsPerSource,
        Self::LocaleBytes,
        Self::EntryStructuralPathBytes,
        Self::CatalogKeyBytes,
        Self::MessageBytes,
        Self::TotalMessageBytes,
        Self::CatalogScopeNameBytes,
        Self::ScopeMappingEntries,
        Self::SelectorPathBytes,
        Self::SelectorPatternBytes,
        Self::SelectorPatternTokens,
        Self::PatternMatchStatesTotal,
        Self::ReasonBytes,
        Self::PathSegments,
        Self::PathSegmentBytes,
        Self::PathBytes,
        Self::LogicalAliases,
        Self::SourcePathBytes,
        Self::ReferenceArtifactWireBytes,
        Self::ReferenceArtifactDecodedBytes,
        Self::DefinitionArtifactWireBytes,
        Self::DefinitionArtifactDecodedBytes,
        Self::FindingsTotal,
        Self::FindingBytesTotal,
        Self::BundlePlansTotal,
        Self::ResolvedMessagesTotal,
        Self::BundlePlanBytesTotal,
        Self::CatalogKeyTokens,
        Self::ReferenceIdentitySegmentBytes,
        Self::ReferenceIdentitySegments,
        Self::ReferenceIdentityBytes,
        Self::DeliveryUnitSegmentBytes,
        Self::DeliveryUnitSegments,
        Self::DeliveryUnitBytes,
        Self::ReferenceRecords,
        Self::Definitions,
    ];

    /// Return the one-based compatibility ordinal.
    #[must_use]
    pub const fn ordinal(self) -> u8 {
        self as u8 + 1
    }

    /// Return the exact structured spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReferenceArtifacts => "reference_artifacts",
            Self::ReferenceIdentityBytesTotal => "reference_identity_bytes_total",
            Self::ReferenceRecordsTotal => "reference_records_total",
            Self::ReferenceArtifactDecodedBytesTotal => "reference_artifact_decoded_bytes_total",
            Self::DefinitionArtifacts => "definition_artifacts",
            Self::DefinitionsTotal => "definitions_total",
            Self::DefinitionArtifactDecodedBytesTotal => "definition_artifact_decoded_bytes_total",
            Self::DeliveryGraphNodes => "delivery_graph_nodes",
            Self::DeliveryGraphEdges => "delivery_graph_edges",
            Self::DeliveryGraphIdBytes => "delivery_graph_id_bytes",
            Self::ProductionLocales => "production_locales",
            Self::FallbackSources => "fallback_sources",
            Self::ConfiguredRoots => "configured_roots",
            Self::FallbackTargetsPerSource => "fallback_targets_per_source",
            Self::LocaleBytes => "locale_bytes",
            Self::EntryStructuralPathBytes => "entry_structural_path_bytes",
            Self::CatalogKeyBytes => "catalog_key_bytes",
            Self::MessageBytes => "message_bytes",
            Self::TotalMessageBytes => "total_message_bytes",
            Self::CatalogScopeNameBytes => "catalog_scope_name_bytes",
            Self::ScopeMappingEntries => "scope_mapping_entries",
            Self::SelectorPathBytes => "selector_path_bytes",
            Self::SelectorPatternBytes => "selector_pattern_bytes",
            Self::SelectorPatternTokens => "selector_pattern_tokens",
            Self::PatternMatchStatesTotal => "pattern_match_states_total",
            Self::ReasonBytes => "reason_bytes",
            Self::PathSegments => "path_segments",
            Self::PathSegmentBytes => "path_segment_bytes",
            Self::PathBytes => "path_bytes",
            Self::LogicalAliases => "logical_aliases",
            Self::SourcePathBytes => "source_path_bytes",
            Self::ReferenceArtifactWireBytes => "reference_artifact_wire_bytes",
            Self::ReferenceArtifactDecodedBytes => "reference_artifact_decoded_bytes",
            Self::DefinitionArtifactWireBytes => "definition_artifact_wire_bytes",
            Self::DefinitionArtifactDecodedBytes => "definition_artifact_decoded_bytes",
            Self::FindingsTotal => "findings_total",
            Self::FindingBytesTotal => "finding_bytes_total",
            Self::BundlePlansTotal => "bundle_plans_total",
            Self::ResolvedMessagesTotal => "resolved_messages_total",
            Self::BundlePlanBytesTotal => "bundle_plan_bytes_total",
            Self::CatalogKeyTokens => "catalog_key_tokens",
            Self::ReferenceIdentitySegmentBytes => "reference_identity_segment_bytes",
            Self::ReferenceIdentitySegments => "reference_identity_segments",
            Self::ReferenceIdentityBytes => "reference_identity_bytes",
            Self::DeliveryUnitSegmentBytes => "delivery_unit_segment_bytes",
            Self::DeliveryUnitSegments => "delivery_unit_segments",
            Self::DeliveryUnitBytes => "delivery_unit_bytes",
            Self::ReferenceRecords => "reference_records",
            Self::Definitions => "definitions",
        }
    }

    /// Return the inclusive protocol hard ceiling.
    #[must_use]
    pub const fn protocol_ceiling(self) -> u64 {
        match self {
            Self::ReferenceArtifacts | Self::DefinitionArtifacts | Self::DeliveryGraphNodes => {
                65_536
            }
            Self::ReferenceIdentityBytesTotal
            | Self::EntryStructuralPathBytes
            | Self::CatalogKeyBytes
            | Self::TotalMessageBytes
            | Self::SelectorPathBytes
            | Self::SourcePathBytes
            | Self::DeliveryGraphIdBytes => 67_108_864,
            Self::ReferenceRecordsTotal | Self::DefinitionsTotal | Self::ResolvedMessagesTotal => {
                4_000_000
            }
            Self::ReferenceArtifactDecodedBytesTotal
            | Self::DefinitionArtifactDecodedBytesTotal
            | Self::BundlePlanBytesTotal => 1_073_741_824,
            Self::DeliveryGraphEdges | Self::BundlePlansTotal | Self::MessageBytes => 1_048_576,
            Self::ProductionLocales | Self::FallbackSources | Self::PathSegments => 1_024,
            Self::ConfiguredRoots
            | Self::ScopeMappingEntries
            | Self::LogicalAliases
            | Self::ReasonBytes
            | Self::PathSegmentBytes
            | Self::ReferenceIdentityBytes
            | Self::DeliveryUnitBytes => 4_096,
            Self::FallbackTargetsPerSource
            | Self::ReferenceIdentitySegments
            | Self::DeliveryUnitSegments => 64,
            Self::LocaleBytes
            | Self::CatalogScopeNameBytes
            | Self::ReferenceIdentitySegmentBytes
            | Self::DeliveryUnitSegmentBytes => 255,
            Self::SelectorPatternBytes => 134_217_728,
            Self::SelectorPatternTokens => 513,
            Self::PatternMatchStatesTotal => 100_000_000,
            Self::PathBytes => 262_144,
            Self::ReferenceArtifactWireBytes | Self::DefinitionArtifactWireBytes => 536_870_912,
            Self::ReferenceArtifactDecodedBytes
            | Self::DefinitionArtifactDecodedBytes
            | Self::FindingBytesTotal => 268_435_456,
            Self::FindingsTotal | Self::ReferenceRecords => 1_000_000,
            Self::CatalogKeyTokens => 256,
            Self::Definitions => 100_000,
        }
    }

    /// Return whether this counter is reserved and unreachable before M2.
    #[must_use]
    pub const fn is_reserved_before_m2(self) -> bool {
        matches!(self, Self::FallbackSources | Self::FallbackTargetsPerSource)
    }

    const fn index(self) -> usize {
        self as usize
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
        if counter.is_reserved_before_m2() || value > counter.protocol_ceiling() {
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
        if self.counter.is_reserved_before_m2() {
            return write!(
                formatter,
                "{} is reserved and cannot be configured before M2",
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
    /// Canonical checked fallback source, available beginning with M2.
    FallbackSource(Locale),
    /// Complete scope-mapping table.
    ScopeMappings,
}

/// Invalid attempted construction of limit evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkLimitEvidenceConstructionError {
    /// The supplied effective limit is above the counter's protocol ceiling.
    EffectiveLimitAboveProtocol,
    /// The counter remains reserved and unreachable before M2.
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
        if !is_artifact_counter(counter) {
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
        if matches!(
            counter,
            LinkLimitCounter::ReferenceArtifactWireBytes
                | LinkLimitCounter::DefinitionArtifactWireBytes
        ) {
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
    if counter.is_reserved_before_m2() {
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
            if requires_first_over(counter) && attempted != effective_limit + 1 =>
        {
            Err(LinkLimitEvidenceConstructionError::NonCanonicalObservation)
        }
        LinkLimitObservation::ArithmeticOverflow if !allows_arithmetic_overflow(counter) => {
            Err(LinkLimitEvidenceConstructionError::ArithmeticOverflowUnavailable)
        }
        LinkLimitObservation::Exact(_) | LinkLimitObservation::ArithmeticOverflow => Ok(()),
    }
}

const fn requires_first_over(counter: LinkLimitCounter) -> bool {
    matches!(
        counter,
        LinkLimitCounter::ReferenceArtifacts
            | LinkLimitCounter::DefinitionArtifacts
            | LinkLimitCounter::DeliveryGraphNodes
            | LinkLimitCounter::DeliveryGraphEdges
            | LinkLimitCounter::ProductionLocales
            | LinkLimitCounter::FallbackSources
            | LinkLimitCounter::ConfiguredRoots
            | LinkLimitCounter::FallbackTargetsPerSource
            | LinkLimitCounter::LocaleBytes
            | LinkLimitCounter::EntryStructuralPathBytes
            | LinkLimitCounter::CatalogKeyBytes
            | LinkLimitCounter::MessageBytes
            | LinkLimitCounter::CatalogScopeNameBytes
            | LinkLimitCounter::ScopeMappingEntries
            | LinkLimitCounter::SelectorPathBytes
            | LinkLimitCounter::SelectorPatternBytes
            | LinkLimitCounter::SelectorPatternTokens
            | LinkLimitCounter::ReasonBytes
            | LinkLimitCounter::PathSegments
            | LinkLimitCounter::PathSegmentBytes
            | LinkLimitCounter::LogicalAliases
            | LinkLimitCounter::ReferenceArtifactWireBytes
            | LinkLimitCounter::DefinitionArtifactWireBytes
            | LinkLimitCounter::FindingsTotal
            | LinkLimitCounter::BundlePlansTotal
            | LinkLimitCounter::ResolvedMessagesTotal
            | LinkLimitCounter::CatalogKeyTokens
            | LinkLimitCounter::ReferenceIdentitySegmentBytes
            | LinkLimitCounter::ReferenceIdentitySegments
            | LinkLimitCounter::DeliveryUnitSegmentBytes
            | LinkLimitCounter::DeliveryUnitSegments
            | LinkLimitCounter::ReferenceRecords
            | LinkLimitCounter::Definitions
    )
}

const fn allows_arithmetic_overflow(counter: LinkLimitCounter) -> bool {
    matches!(
        counter,
        LinkLimitCounter::ReferenceIdentityBytesTotal
            | LinkLimitCounter::ReferenceRecordsTotal
            | LinkLimitCounter::ReferenceArtifactDecodedBytesTotal
            | LinkLimitCounter::DefinitionsTotal
            | LinkLimitCounter::DefinitionArtifactDecodedBytesTotal
            | LinkLimitCounter::DeliveryGraphIdBytes
    )
}

const fn is_artifact_counter(counter: LinkLimitCounter) -> bool {
    matches!(
        counter,
        LinkLimitCounter::LocaleBytes
            | LinkLimitCounter::EntryStructuralPathBytes
            | LinkLimitCounter::CatalogKeyBytes
            | LinkLimitCounter::MessageBytes
            | LinkLimitCounter::TotalMessageBytes
            | LinkLimitCounter::CatalogScopeNameBytes
            | LinkLimitCounter::SelectorPathBytes
            | LinkLimitCounter::SelectorPatternBytes
            | LinkLimitCounter::SelectorPatternTokens
            | LinkLimitCounter::ReasonBytes
            | LinkLimitCounter::PathSegments
            | LinkLimitCounter::PathSegmentBytes
            | LinkLimitCounter::PathBytes
            | LinkLimitCounter::LogicalAliases
            | LinkLimitCounter::SourcePathBytes
            | LinkLimitCounter::ReferenceArtifactWireBytes
            | LinkLimitCounter::ReferenceArtifactDecodedBytes
            | LinkLimitCounter::DefinitionArtifactWireBytes
            | LinkLimitCounter::DefinitionArtifactDecodedBytes
            | LinkLimitCounter::CatalogKeyTokens
            | LinkLimitCounter::ReferenceIdentitySegmentBytes
            | LinkLimitCounter::ReferenceIdentitySegments
            | LinkLimitCounter::ReferenceIdentityBytes
            | LinkLimitCounter::DeliveryUnitSegmentBytes
            | LinkLimitCounter::DeliveryUnitSegments
            | LinkLimitCounter::DeliveryUnitBytes
            | LinkLimitCounter::ReferenceRecords
            | LinkLimitCounter::Definitions
    )
}

fn valid_subject(counter: LinkLimitCounter, subject: &LinkLimitSubject) -> bool {
    use LinkLimitCounter as Counter;
    use LinkLimitSubject as Subject;

    match subject {
        Subject::Request => matches!(
            counter,
            Counter::ReferenceArtifacts
                | Counter::DefinitionArtifacts
                | Counter::PatternMatchStatesTotal
                | Counter::FindingsTotal
                | Counter::FindingBytesTotal
                | Counter::BundlePlansTotal
                | Counter::ResolvedMessagesTotal
                | Counter::BundlePlanBytesTotal
        ),
        Subject::DefinitionArtifactEnvelope => matches!(
            counter,
            Counter::PathSegments | Counter::PathSegmentBytes | Counter::PathBytes
        ),
        Subject::ReferenceArtifactGroup(_) => matches!(
            counter,
            Counter::ReferenceIdentityBytesTotal
                | Counter::ReferenceRecordsTotal
                | Counter::ReferenceArtifactDecodedBytesTotal
                | Counter::CatalogScopeNameBytes
                | Counter::SelectorPathBytes
                | Counter::SelectorPatternBytes
                | Counter::SelectorPatternTokens
                | Counter::ReasonBytes
                | Counter::PathSegments
                | Counter::PathSegmentBytes
                | Counter::PathBytes
                | Counter::ReferenceArtifactDecodedBytes
                | Counter::CatalogKeyTokens
                | Counter::ReferenceIdentitySegmentBytes
                | Counter::ReferenceIdentitySegments
                | Counter::ReferenceIdentityBytes
                | Counter::DeliveryUnitSegmentBytes
                | Counter::DeliveryUnitSegments
                | Counter::DeliveryUnitBytes
                | Counter::ReferenceRecords
        ),
        Subject::DefinitionArtifactGroup(_) => matches!(
            counter,
            Counter::DefinitionsTotal
                | Counter::DefinitionArtifactDecodedBytesTotal
                | Counter::LocaleBytes
                | Counter::EntryStructuralPathBytes
                | Counter::CatalogKeyBytes
                | Counter::MessageBytes
                | Counter::TotalMessageBytes
                | Counter::CatalogScopeNameBytes
                | Counter::PathSegments
                | Counter::PathSegmentBytes
                | Counter::PathBytes
                | Counter::LogicalAliases
                | Counter::SourcePathBytes
                | Counter::DefinitionArtifactDecodedBytes
                | Counter::CatalogKeyTokens
                | Counter::Definitions
        ),
        Subject::DeliveryGraph => {
            matches!(
                counter,
                Counter::DeliveryGraphNodes | Counter::DeliveryGraphEdges
            )
        }
        Subject::DeliveryUnitGroup(_) => matches!(
            counter,
            Counter::DeliveryGraphIdBytes
                | Counter::DeliveryUnitSegmentBytes
                | Counter::DeliveryUnitSegments
                | Counter::DeliveryUnitBytes
        ),
        Subject::ResolvedPolicy => matches!(
            counter,
            Counter::ProductionLocales
                | Counter::ConfiguredRoots
                | Counter::LocaleBytes
                | Counter::CatalogScopeNameBytes
        ),
        Subject::FallbackSource(_) => counter == Counter::FallbackTargetsPerSource,
        Subject::ScopeMappings => matches!(
            counter,
            Counter::ScopeMappingEntries | Counter::CatalogScopeNameBytes
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ArtifactLimitEvidence, LinkLimitCounter, LinkLimitEvidence,
        LinkLimitEvidenceConstructionError, LinkLimitObservation, LinkLimitSubject, LinkLimits,
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
            assert_eq!(counter.as_str(), spelling);
            assert_eq!(counter.protocol_ceiling(), ceiling);
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
            if counter.is_reserved_before_m2() {
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
            assert!(counter.is_reserved_before_m2());
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
