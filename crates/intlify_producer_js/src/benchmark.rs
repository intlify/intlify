// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Non-product observation surface for the dedicated message benchmark.
//!
//! This module exists only with the non-default `benchmark` Cargo feature. It
//! invokes the ordinary producer machinery and exposes completed immutable
//! observations; it cannot construct a partial artifact or caller-authored
//! frontend state.

use std::time::Duration;

use intlify_contract::{LinkLimits, SourceDocumentIdentity};

use crate::{
    JsPhysicalSourceGroup, JsProducerCache, JsProducerError, JsProducerOutcome, JsRecognizerSet,
};

/// One producer-owned benchmark stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BenchmarkJsStage {
    /// Parser work and conversion into the producer-owned syntax view.
    SourceParse,
    /// Configured call recognition and bounded static evaluation.
    RecognizerMatchAndStaticEvaluation,
    /// Selector, checked key, provenance, and reference-record construction.
    KeyAndProvenanceConstruction,
    /// Immutable checked reference-artifact construction.
    ReferenceArtifactConstruction,
    /// Miss lookup, production, cache-value construction, and publication.
    CacheMissProduction,
    /// Matching cache lookup, validation, and immutable artifact access.
    CacheHitValidationAndAccess,
}

impl BenchmarkJsStage {
    /// Return the exact design-owned benchmark phase.
    #[must_use]
    pub const fn phase(self) -> &'static str {
        match self {
            Self::SourceParse
            | Self::RecognizerMatchAndStaticEvaluation
            | Self::KeyAndProvenanceConstruction
            | Self::ReferenceArtifactConstruction => "message_reference_produce_js",
            Self::CacheMissProduction | Self::CacheHitValidationAndAccess => {
                "message_reference_produce_js_cache"
            }
        }
    }

    /// Return the exact design-owned benchmark cost.
    #[must_use]
    pub const fn cost(self) -> &'static str {
        match self {
            Self::SourceParse => "source_parse",
            Self::RecognizerMatchAndStaticEvaluation => "recognizer_match_and_static_evaluation",
            Self::KeyAndProvenanceConstruction => "key_and_provenance_construction",
            Self::ReferenceArtifactConstruction => "reference_artifact_construction",
            Self::CacheMissProduction => "cache_miss_production",
            Self::CacheHitValidationAndAccess => "cache_hit_validation_and_access",
        }
    }

    /// Return the exact stable measurement-boundary identifier.
    #[must_use]
    pub const fn boundary_id(self) -> &'static str {
        match self {
            Self::SourceParse => "js_source_parse",
            Self::RecognizerMatchAndStaticEvaluation => "js_recognizer_static_evaluation",
            Self::KeyAndProvenanceConstruction => "js_key_provenance_construction",
            Self::ReferenceArtifactConstruction => "js_reference_artifact_construction",
            Self::CacheMissProduction => "js_cache_miss_production",
            Self::CacheHitValidationAndAccess => "js_cache_hit_access",
        }
    }
}

/// One completed producer-stage occurrence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkJsStageMeasurement {
    source: SourceDocumentIdentity,
    stage: BenchmarkJsStage,
    elapsed: Duration,
    checksum: u32,
}

impl BenchmarkJsStageMeasurement {
    pub(crate) const fn new(
        source: SourceDocumentIdentity,
        stage: BenchmarkJsStage,
        elapsed: Duration,
        checksum: u32,
    ) -> Self {
        Self {
            source,
            stage,
            elapsed,
            checksum,
        }
    }

    /// Return the canonical physical-group primary source.
    #[must_use]
    pub const fn source(&self) -> &SourceDocumentIdentity {
        &self.source
    }

    /// Return the observed stage.
    #[must_use]
    pub const fn stage(&self) -> BenchmarkJsStage {
        self.stage
    }

    /// Return the unadjusted monotonic-clock duration.
    #[must_use]
    pub const fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// Return the deterministic semantic checksum.
    #[must_use]
    pub const fn checksum(&self) -> u32 {
        self.checksum
    }
}

/// Complete ordinary producer result plus benchmark-only observations.
#[derive(Debug)]
pub struct BenchmarkJsProducerExecution {
    outcome: JsProducerOutcome,
    stages: Box<[BenchmarkJsStageMeasurement]>,
    observation_overhead: Duration,
}

impl BenchmarkJsProducerExecution {
    pub(crate) fn new(
        outcome: JsProducerOutcome,
        stages: Vec<BenchmarkJsStageMeasurement>,
        observation_overhead: Duration,
    ) -> Self {
        Self {
            outcome,
            stages: stages.into_boxed_slice(),
            observation_overhead,
        }
    }

    /// Return the ordinary producer outcome.
    #[must_use]
    pub const fn outcome(&self) -> &JsProducerOutcome {
        &self.outcome
    }

    /// Return completed stage occurrences in execution order.
    #[must_use]
    pub const fn stages(&self) -> &[BenchmarkJsStageMeasurement] {
        &self.stages
    }

    /// Return benchmark-only checksum and recorder time outside stage intervals.
    #[must_use]
    pub const fn observation_overhead(&self) -> Duration {
        self.observation_overhead
    }

    /// Consume the observation and return the ordinary producer outcome.
    #[must_use]
    pub fn into_outcome(self) -> JsProducerOutcome {
        self.outcome
    }
}

/// Observe ordinary production without a cache.
#[doc(hidden)]
pub fn benchmark_produce_reference_artifacts(
    groups: Vec<JsPhysicalSourceGroup>,
    recognizers: &JsRecognizerSet,
    limits: &LinkLimits,
) -> Result<BenchmarkJsProducerExecution, JsProducerError> {
    crate::producer::benchmark_produce(groups, recognizers, limits, None::<&NoCache>)
}

/// Observe ordinary production through a caller-owned cache.
#[doc(hidden)]
pub fn benchmark_produce_reference_artifacts_with_cache<S>(
    groups: Vec<JsPhysicalSourceGroup>,
    recognizers: &JsRecognizerSet,
    limits: &LinkLimits,
    cache: &S,
) -> Result<BenchmarkJsProducerExecution, JsProducerError>
where
    S: JsProducerCache + ?Sized,
{
    crate::producer::benchmark_produce(groups, recognizers, limits, Some(cache))
}

struct NoCache;

impl JsProducerCache for NoCache {
    fn load(
        &self,
        _key: &crate::JsProducerCacheKey,
    ) -> Result<Option<Box<[u8]>>, crate::JsProducerCacheIoError> {
        Ok(None)
    }

    fn store(
        &self,
        _key: &crate::JsProducerCacheKey,
        _value: &[u8],
    ) -> Result<(), crate::JsProducerCacheIoError> {
        Ok(())
    }
}
