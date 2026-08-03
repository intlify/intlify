// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Closed, bounded failures produced after validated export begins.
//!
//! This module owns exporter capability, generation, common-output limit,
//! candidate-validation, and internal-invariant evidence. It deliberately does
//! not wrap linker, preparation, filesystem, destination, or reporter errors.

use intlify_contract::{DeliveryUnitId, EntryReference, Locale, SourceDocumentIdentity};

/// Complete common failure from one platform-exporter invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportError {
    evidence: ExportErrorEvidence,
}

impl ExportError {
    /// Construct an unsupported-batch failure from checked evidence.
    #[must_use]
    pub const fn unsupported_batch(evidence: UnsupportedBatchEvidence) -> Self {
        Self {
            evidence: ExportErrorEvidence::UnsupportedBatch(evidence),
        }
    }

    /// Construct a representation-generation failure from checked evidence.
    #[must_use]
    pub const fn generation_failed(evidence: GenerationFailedEvidence) -> Self {
        Self {
            evidence: ExportErrorEvidence::GenerationFailed(evidence),
        }
    }

    /// Construct a common output-limit failure from checked evidence.
    #[must_use]
    pub const fn output_limit_exceeded(evidence: OutputLimitExceededEvidence) -> Self {
        Self {
            evidence: ExportErrorEvidence::OutputLimitExceeded(evidence),
        }
    }

    /// Construct a non-limit common output-contract failure.
    #[must_use]
    pub const fn invalid_output(evidence: InvalidOutputEvidence) -> Self {
        Self {
            evidence: ExportErrorEvidence::InvalidOutput(evidence),
        }
    }

    /// Construct an explicitly detected exporter invariant failure.
    #[must_use]
    pub const fn internal_invariant(evidence: InternalInvariantEvidence) -> Self {
        Self {
            evidence: ExportErrorEvidence::InternalInvariant(evidence),
        }
    }

    /// Return the classification derived from the sole stored evidence value.
    #[must_use]
    pub const fn kind(&self) -> ExportErrorKind {
        self.evidence.kind()
    }

    /// Return the exact checked evidence retained by this failure.
    #[must_use]
    pub const fn evidence(&self) -> &ExportErrorEvidence {
        &self.evidence
    }
}

impl core::fmt::Display for ExportError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self.kind() {
            ExportErrorKind::UnsupportedBatch => {
                "validated export batch is unsupported by the selected exporter"
            }
            ExportErrorKind::GenerationFailed => "export representation generation failed",
            ExportErrorKind::OutputLimitExceeded => "common export output limit exceeded",
            ExportErrorKind::InvalidOutput => "candidate export output is invalid",
            ExportErrorKind::InternalInvariant => "exporter internal invariant failed",
        })
    }
}

impl std::error::Error for ExportError {}

/// Closed classification derived from [`ExportErrorEvidence`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExportErrorKind {
    /// A valid prepared batch cannot be represented by the selected exporter.
    UnsupportedBatch,
    /// A supported deterministic generation operation failed.
    GenerationFailed,
    /// A fixed common output resource ceiling was exceeded.
    OutputLimitExceeded,
    /// A non-limit common output contract was violated.
    InvalidOutput,
    /// Exporter or shared-constructor state contradicted its own invariant.
    InternalInvariant,
}

impl ExportErrorKind {
    /// Return the comparison precedence; lower values win.
    #[must_use]
    pub const fn precedence(self) -> u8 {
        match self {
            Self::InternalInvariant => 0,
            Self::UnsupportedBatch => 1,
            Self::GenerationFailed => 2,
            Self::OutputLimitExceeded => 3,
            Self::InvalidOutput => 4,
        }
    }
}

/// Sole stored union for common exporter failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportErrorEvidence {
    /// Evidence for [`ExportErrorKind::UnsupportedBatch`].
    UnsupportedBatch(UnsupportedBatchEvidence),
    /// Evidence for [`ExportErrorKind::GenerationFailed`].
    GenerationFailed(GenerationFailedEvidence),
    /// Evidence for [`ExportErrorKind::OutputLimitExceeded`].
    OutputLimitExceeded(OutputLimitExceededEvidence),
    /// Evidence for [`ExportErrorKind::InvalidOutput`].
    InvalidOutput(InvalidOutputEvidence),
    /// Evidence for [`ExportErrorKind::InternalInvariant`].
    InternalInvariant(InternalInvariantEvidence),
}

impl ExportErrorEvidence {
    const fn kind(&self) -> ExportErrorKind {
        match self {
            Self::UnsupportedBatch(_) => ExportErrorKind::UnsupportedBatch,
            Self::GenerationFailed(_) => ExportErrorKind::GenerationFailed,
            Self::OutputLimitExceeded(_) => ExportErrorKind::OutputLimitExceeded,
            Self::InvalidOutput(_) => ExportErrorKind::InvalidOutput,
            Self::InternalInvariant(_) => ExportErrorKind::InternalInvariant,
        }
    }
}

/// Checked evidence for one unsupported valid batch requirement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsupportedBatchEvidence {
    feature: UnsupportedBatchFeature,
    location: UnsupportedBatchLocation,
}

impl UnsupportedBatchEvidence {
    /// Construct evidence only when the feature and location shapes correspond.
    #[must_use]
    pub fn try_new(
        feature: UnsupportedBatchFeature,
        location: UnsupportedBatchLocation,
    ) -> Option<Self> {
        let valid = matches!(
            (feature, &location),
            (
                UnsupportedBatchFeature::BatchComposition,
                UnsupportedBatchLocation::Batch
            ) | (
                UnsupportedBatchFeature::DeliveryUnitPartitioning
                    | UnsupportedBatchFeature::LocalePartitioning
                    | UnsupportedBatchFeature::FallbackSemantics,
                UnsupportedBatchLocation::Plan { .. }
            ) | (
                UnsupportedBatchFeature::MessageSemantics,
                UnsupportedBatchLocation::Definition { .. }
            )
        );
        valid.then_some(Self { feature, location })
    }

    /// Return the unsupported language-neutral batch feature.
    #[must_use]
    pub const fn feature(&self) -> UnsupportedBatchFeature {
        self.feature
    }

    /// Return the narrowest stable batch location.
    #[must_use]
    pub const fn location(&self) -> &UnsupportedBatchLocation {
        &self.location
    }
}

/// Closed language-neutral capability requirements in deterministic order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnsupportedBatchFeature {
    /// The complete combination of plans cannot be represented together.
    BatchComposition,
    /// A plan's delivery-unit partition or placement cannot be preserved.
    DeliveryUnitPartitioning,
    /// A plan's requested-locale partition cannot be preserved.
    LocalePartitioning,
    /// A plan's already-resolved fallback semantics cannot be preserved.
    FallbackSemantics,
    /// One validated message's MF2 semantics cannot be represented losslessly.
    MessageSemantics,
}

/// Stable coordinate for an unsupported batch requirement.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum UnsupportedBatchLocation {
    /// The requirement belongs to the complete batch.
    Batch,
    /// The requirement belongs to one canonical plan.
    Plan {
        /// Exact delivery-unit identity.
        delivery_unit: DeliveryUnitId,
        /// Exact requested production locale.
        locale: Locale,
    },
    /// The requirement belongs to one definition use within a plan.
    Definition {
        /// Exact delivery-unit identity.
        delivery_unit: DeliveryUnitId,
        /// Exact requested production locale.
        locale: Locale,
        /// Exact source-document identity.
        source: SourceDocumentIdentity,
        /// Stable source entry evidence.
        entry: EntryReference,
    },
}

/// Checked evidence for one deterministic representation-generation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationFailedEvidence {
    stage: GenerationStage,
    location: GenerationLocation,
}

impl GenerationFailedEvidence {
    /// Construct evidence only when the stage admits the supplied location.
    #[must_use]
    pub fn try_new(stage: GenerationStage, location: GenerationLocation) -> Option<Self> {
        let valid = stage != GenerationStage::MessageCompilation
            || matches!(location, GenerationLocation::Definition { .. });
        valid.then_some(Self { stage, location })
    }

    /// Return the failed deterministic generation stage.
    #[must_use]
    pub const fn stage(&self) -> GenerationStage {
        self.stage
    }

    /// Return the narrowest stable input location.
    #[must_use]
    pub const fn location(&self) -> &GenerationLocation {
        &self.location
    }
}

/// Closed deterministic exporter stages in failure-selection order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GenerationStage {
    /// Compilation of one supported syntax-valid message failed.
    MessageCompilation,
    /// Deterministic target payload encoding failed.
    PayloadEncoding,
    /// Deterministic common metadata generation failed.
    MetadataGeneration,
}

/// Stable coordinate for one generation operation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GenerationLocation {
    /// The operation belongs to the complete batch.
    Batch,
    /// The operation belongs to one canonical plan.
    Plan {
        /// Exact delivery-unit identity.
        delivery_unit: DeliveryUnitId,
        /// Exact requested production locale.
        locale: Locale,
    },
    /// The operation belongs to one definition use within a plan.
    Definition {
        /// Exact delivery-unit identity.
        delivery_unit: DeliveryUnitId,
        /// Exact requested production locale.
        locale: Locale,
        /// Exact source-document identity.
        source: SourceDocumentIdentity,
        /// Stable source entry evidence.
        entry: EntryReference,
    },
}

/// Checked evidence for one fixed common output ceiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OutputLimitExceededEvidence {
    counter: OutputLimitCounter,
    observation: OutputLimitObservation,
}

impl OutputLimitExceededEvidence {
    /// Construct an exact observation only when it exceeds the inclusive ceiling.
    #[must_use]
    pub const fn try_exact(counter: OutputLimitCounter, value: u64) -> Option<Self> {
        if value > counter.ceiling() {
            Some(Self {
                counter,
                observation: OutputLimitObservation::Exact(value),
            })
        } else {
            None
        }
    }

    /// Construct evidence for a failed length conversion or checked addition.
    #[must_use]
    pub const fn arithmetic_overflow(counter: OutputLimitCounter) -> Self {
        Self {
            counter,
            observation: OutputLimitObservation::ArithmeticOverflow,
        }
    }

    /// Return the fixed common output counter.
    #[must_use]
    pub const fn counter(self) -> OutputLimitCounter {
        self.counter
    }

    /// Return the exact or arithmetic-overflow observation.
    #[must_use]
    pub const fn observation(self) -> OutputLimitObservation {
        self.observation
    }
}

/// Fixed common output resource counters in normative precedence order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutputLimitCounter {
    /// Submitted artifact records in one set.
    ArtifactRecordsPerSet,
    /// Segments in one logical path.
    ArtifactPathSegmentsPerPath,
    /// Decoded UTF-8 bytes in one logical-path segment.
    ArtifactPathSegmentBytes,
    /// Decoded segment bytes in one logical path.
    ArtifactPathBytesPerPath,
    /// Decoded artifact logical-path bytes in one set.
    ArtifactPathBytesPerSet,
    /// Decoded ASCII bytes in one artifact kind.
    ArtifactKindBytes,
    /// Decoded ASCII bytes in one media-type component.
    MediaTypeComponentBytes,
    /// Decoded ASCII bytes in one complete media type.
    MediaTypeBytes,
    /// Submitted relationship records in one artifact.
    RelationshipRecordsPerArtifact,
    /// Submitted relationship records in one set.
    RelationshipRecordsPerSet,
    /// Decoded target-segment bytes across relationship occurrences.
    RelationshipTargetBytesPerSet,
    /// Stored payload bytes in one artifact.
    PayloadBytesPerArtifact,
    /// Stored payload bytes in one set.
    PayloadBytesPerSet,
}

impl OutputLimitCounter {
    pub(crate) const ALL: [Self; 13] = [
        Self::ArtifactRecordsPerSet,
        Self::ArtifactPathSegmentsPerPath,
        Self::ArtifactPathSegmentBytes,
        Self::ArtifactPathBytesPerPath,
        Self::ArtifactPathBytesPerSet,
        Self::ArtifactKindBytes,
        Self::MediaTypeComponentBytes,
        Self::MediaTypeBytes,
        Self::RelationshipRecordsPerArtifact,
        Self::RelationshipRecordsPerSet,
        Self::RelationshipTargetBytesPerSet,
        Self::PayloadBytesPerArtifact,
        Self::PayloadBytesPerSet,
    ];

    /// Return the fixed inclusive protocol ceiling.
    #[must_use]
    pub const fn ceiling(self) -> u64 {
        match self {
            Self::ArtifactRecordsPerSet | Self::RelationshipRecordsPerSet => 65_536,
            Self::ArtifactPathSegmentsPerPath => 64,
            Self::ArtifactPathSegmentBytes | Self::ArtifactKindBytes | Self::MediaTypeBytes => 255,
            Self::ArtifactPathBytesPerPath | Self::RelationshipRecordsPerArtifact => 4_096,
            Self::ArtifactPathBytesPerSet | Self::RelationshipTargetBytesPerSet => 67_108_864,
            Self::MediaTypeComponentBytes => 127,
            Self::PayloadBytesPerArtifact => 268_435_456,
            Self::PayloadBytesPerSet => 1_073_741_824,
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::ArtifactRecordsPerSet => 0,
            Self::ArtifactPathSegmentsPerPath => 1,
            Self::ArtifactPathSegmentBytes => 2,
            Self::ArtifactPathBytesPerPath => 3,
            Self::ArtifactPathBytesPerSet => 4,
            Self::ArtifactKindBytes => 5,
            Self::MediaTypeComponentBytes => 6,
            Self::MediaTypeBytes => 7,
            Self::RelationshipRecordsPerArtifact => 8,
            Self::RelationshipRecordsPerSet => 9,
            Self::RelationshipTargetBytesPerSet => 10,
            Self::PayloadBytesPerArtifact => 11,
            Self::PayloadBytesPerSet => 12,
        }
    }
}

/// Attempted common output count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutputLimitObservation {
    /// Exact attempted value greater than its counter's ceiling.
    Exact(u64),
    /// A conversion or checked addition could not represent the attempted value.
    ArithmeticOverflow,
}

/// Checked evidence for one non-limit common output violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InvalidOutputEvidence {
    violation: InvalidOutputViolation,
}

impl InvalidOutputEvidence {
    /// Construct evidence for one closed common output violation.
    #[must_use]
    pub const fn new(violation: InvalidOutputViolation) -> Self {
        Self { violation }
    }

    /// Return the selected non-limit common output violation.
    #[must_use]
    pub const fn violation(self) -> InvalidOutputViolation {
        self.violation
    }
}

/// Non-limit common output violations in normative precedence order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InvalidOutputViolation {
    /// The common artifact root has a missing or malformed field.
    ArtifactEnvelope,
    /// A logical artifact path or relationship target is malformed.
    ArtifactPath,
    /// Two artifact records have the same exact logical path.
    DuplicateArtifactPath,
    /// An artifact kind violates its canonical namespace grammar.
    ArtifactKind,
    /// A format version is malformed outside the typed Rust boundary.
    FormatVersion,
    /// The metadata envelope is missing or malformed.
    MetadataEnvelope,
    /// A present media type violates the parameter-free canonical grammar.
    MediaType,
    /// A relationship envelope is missing or malformed.
    RelationshipEnvelope,
    /// A relationship tag is outside the closed portable kind set.
    RelationshipKind,
    /// A relationship target does not resolve within the same artifact set.
    RelationshipTarget,
    /// A source repeats the same exact relationship kind and target.
    DuplicateRelationship,
    /// A source classifies one target as both eager and lazy.
    ConflictingRelationshipClassification,
    /// A relationship resolves from an artifact to itself.
    RelationshipSelfEdge,
    /// A lazy target is already reachable through eager-only edges.
    LazyTargetInEagerClosure,
}

/// Checked evidence for one explicit exporter or constructor contradiction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InternalInvariantEvidence {
    invariant: InternalInvariantViolation,
}

impl InternalInvariantEvidence {
    /// Construct evidence for one closed internal invariant.
    #[must_use]
    pub const fn new(invariant: InternalInvariantViolation) -> Self {
        Self { invariant }
    }

    /// Return the selected internal invariant.
    #[must_use]
    pub const fn invariant(self) -> InternalInvariantViolation {
        self.invariant
    }
}

/// Explicit internal contradictions in exporter-pipeline precedence order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InternalInvariantViolation {
    /// A supposedly validated batch contradicts its checked contract.
    ValidatedBatchContract,
    /// Capability preflight contradicts itself for unchanged input.
    CapabilityPreflightContract,
    /// The deterministic generation state machine takes an impossible transition.
    GenerationState,
    /// Shared bounded-output accounting contradicts its own state.
    OutputBudgetAccounting,
    /// Generated parts cannot be paired into their planned artifact records.
    ArtifactAssemblyState,
    /// The shared constructor's validated indexing or graph state contradicts itself.
    SharedConstructorState,
}

/// Collect all common limit failures before selecting canonical evidence.
#[derive(Debug, Clone)]
pub(crate) struct OutputLimitAccumulator {
    observations: [Option<OutputLimitObservation>; 13],
}

impl OutputLimitAccumulator {
    pub(crate) const fn new() -> Self {
        Self {
            observations: [None; 13],
        }
    }

    pub(crate) fn observe_usize(&mut self, counter: OutputLimitCounter, value: usize) {
        match u64::try_from(value) {
            Ok(value) => self.observe_exact(counter, value),
            Err(_) => self.observe_overflow(counter),
        }
    }

    pub(crate) fn add_usize(&mut self, counter: OutputLimitCounter, total: &mut u64, value: usize) {
        let Ok(value) = u64::try_from(value) else {
            self.observe_overflow(counter);
            return;
        };
        let Some(next) = total.checked_add(value) else {
            self.observe_overflow(counter);
            return;
        };
        *total = next;
        self.observe_exact(counter, next);
    }

    pub(crate) fn observe_exact(&mut self, counter: OutputLimitCounter, value: u64) {
        if value <= counter.ceiling() {
            return;
        }
        let slot = &mut self.observations[counter.index()];
        let replace = match slot {
            Some(OutputLimitObservation::Exact(previous)) => value > *previous,
            None => true,
            Some(OutputLimitObservation::ArithmeticOverflow) => false,
        };
        if replace {
            *slot = Some(OutputLimitObservation::Exact(value));
        }
    }

    pub(crate) fn observe_overflow(&mut self, counter: OutputLimitCounter) {
        self.observations[counter.index()] = Some(OutputLimitObservation::ArithmeticOverflow);
    }

    pub(crate) fn finish(self) -> Option<ExportError> {
        for counter in OutputLimitCounter::ALL {
            let Some(observation) = self.observations[counter.index()] else {
                continue;
            };
            let evidence = match observation {
                OutputLimitObservation::Exact(value) => OutputLimitExceededEvidence {
                    counter,
                    observation: OutputLimitObservation::Exact(value),
                },
                OutputLimitObservation::ArithmeticOverflow => {
                    OutputLimitExceededEvidence::arithmetic_overflow(counter)
                }
            };
            return Some(ExportError::output_limit_exceeded(evidence));
        }
        None
    }
}

pub(crate) const fn invalid_output(violation: InvalidOutputViolation) -> ExportError {
    ExportError::invalid_output(InvalidOutputEvidence::new(violation))
}

#[cfg(test)]
mod tests {
    use super::{
        ExportErrorKind, GenerationFailedEvidence, GenerationLocation, GenerationStage,
        OutputLimitAccumulator, OutputLimitCounter, OutputLimitExceededEvidence,
        OutputLimitObservation, UnsupportedBatchEvidence, UnsupportedBatchFeature,
        UnsupportedBatchLocation,
    };

    #[test]
    fn kind_precedence_is_explicit_and_independent_of_declaration_order() {
        assert_eq!(ExportErrorKind::InternalInvariant.precedence(), 0);
        assert_eq!(ExportErrorKind::UnsupportedBatch.precedence(), 1);
        assert_eq!(ExportErrorKind::GenerationFailed.precedence(), 2);
        assert_eq!(ExportErrorKind::OutputLimitExceeded.precedence(), 3);
        assert_eq!(ExportErrorKind::InvalidOutput.precedence(), 4);
    }

    #[test]
    fn all_thirteen_counter_ceilings_are_fixed_in_precedence_order() {
        assert_eq!(
            OutputLimitCounter::ALL.map(OutputLimitCounter::ceiling),
            [
                65_536,
                64,
                255,
                4_096,
                67_108_864,
                255,
                127,
                255,
                4_096,
                65_536,
                67_108_864,
                268_435_456,
                1_073_741_824,
            ]
        );
    }

    #[test]
    fn counter_precedence_order_matches_observation_slot_indexes() {
        for (slot, counter) in OutputLimitCounter::ALL.into_iter().enumerate() {
            assert_eq!(counter.index(), slot);
        }
    }

    #[test]
    fn every_exact_limit_accepts_its_boundary_and_rejects_the_first_over_value() {
        for counter in OutputLimitCounter::ALL {
            assert!(OutputLimitExceededEvidence::try_exact(counter, counter.ceiling()).is_none());
            let evidence = OutputLimitExceededEvidence::try_exact(counter, counter.ceiling() + 1)
                .expect("first over-limit value must construct evidence");
            assert_eq!(evidence.counter(), counter);
            assert_eq!(
                evidence.observation(),
                OutputLimitObservation::Exact(counter.ceiling() + 1)
            );
        }
    }

    #[test]
    fn accumulator_selects_counter_then_overflow_then_greatest_exact_value() {
        let mut accumulator = OutputLimitAccumulator::new();
        accumulator.observe_exact(OutputLimitCounter::PayloadBytesPerSet, 1_073_741_900);
        accumulator.observe_exact(OutputLimitCounter::ArtifactPathSegmentBytes, 256);
        accumulator.observe_exact(OutputLimitCounter::ArtifactPathSegmentBytes, 300);
        accumulator.observe_exact(OutputLimitCounter::ArtifactPathSegmentBytes, 280);
        accumulator.observe_overflow(OutputLimitCounter::ArtifactPathSegmentBytes);

        let error = accumulator.finish().unwrap();
        let super::ExportErrorEvidence::OutputLimitExceeded(evidence) = error.evidence() else {
            panic!("wrong evidence kind");
        };
        assert_eq!(
            evidence.counter(),
            OutputLimitCounter::ArtifactPathSegmentBytes
        );
        assert_eq!(
            evidence.observation(),
            OutputLimitObservation::ArithmeticOverflow
        );
    }

    #[test]
    fn evidence_construction_rejects_feature_and_stage_location_mismatches() {
        assert!(UnsupportedBatchEvidence::try_new(
            UnsupportedBatchFeature::MessageSemantics,
            UnsupportedBatchLocation::Batch,
        )
        .is_none());
        assert!(GenerationFailedEvidence::try_new(
            GenerationStage::MessageCompilation,
            GenerationLocation::Batch,
        )
        .is_none());
        assert!(GenerationFailedEvidence::try_new(
            GenerationStage::PayloadEncoding,
            GenerationLocation::Batch,
        )
        .is_some());
    }
}
