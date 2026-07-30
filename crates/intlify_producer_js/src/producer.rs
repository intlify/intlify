// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Physical-group admission and complete reference-artifact production.
//!
//! This module owns invocation-wide source ceilings, alias-profile agreement,
//! producer and artifact identity, cache reuse, checked artifact construction,
//! and canonical result order. Filesystem discovery, physical identity,
//! snapshot acquisition, scheduling, completeness mapping, and linker policy
//! remain caller responsibilities.

use std::collections::BTreeSet;

use intlify_contract::{
    decode_reference_artifact, encode_reference_artifact, ArtifactContractError, ArtifactNamespace,
    ArtifactVersion, CatalogScopeId, DeliveryUnitId, LinkLimitObservation, LinkLimits,
    MessageReference, MessageReferenceArtifact, MessageSelector, ProducerIdentity,
    ReferenceArtifactIdentity, ReferenceArtifactSegment, SourceDocumentIdentity,
    ValueConstructionError,
};

use crate::cache::{
    decode_cache_entry, encode_cache_entry, key_for_group, CacheKeyContext, JsProducerCache,
    JsProducerCacheIoError, JsProducerCacheKey, SourceFingerprint,
};
use crate::error::{JsProducerError, JsProducerFailure, JsProducerFailureReason};
use crate::frontend::{scan_source_with_cancellation, BOUNDED_SET_REASON, DYNAMIC_LOOKUP_REASON};
use crate::group::JsPhysicalSourceGroup;
use crate::profile::{JsSelectedSourceGoal, JsSourceGoal, JsSourceProfile};
use crate::provenance::js_producer_identity;
use crate::recognizer::JsRecognizerSet;

/// Fixed inclusive physical-source group ceiling for one producer invocation.
pub const SOURCE_GROUPS_LIMIT: u64 = 65_536;

/// Fixed inclusive aggregate admitted-source byte ceiling for one invocation.
pub const SOURCE_BYTES_TOTAL_LIMIT: u64 = 1024 * 1024 * 1024;

/// Complete deterministic result of one built-in JS producer invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsProducerOutcome {
    artifacts: Box<[MessageReferenceArtifact]>,
    source_failures: Box<[JsProducerFailure]>,
    invocation_failure: Option<JsProducerFailure>,
    producer_scopes: Box<[CatalogScopeId]>,
}

impl JsProducerOutcome {
    /// Return successful artifacts in canonical artifact-identity order.
    #[must_use]
    pub const fn artifacts(&self) -> &[MessageReferenceArtifact] {
        &self.artifacts
    }

    /// Return source-local failures in canonical primary-source order.
    #[must_use]
    pub const fn source_failures(&self) -> &[JsProducerFailure] {
        &self.source_failures
    }

    /// Return an invocation-wide group-count or aggregate-byte failure.
    ///
    /// When present, `artifacts()` is empty and no pending cache value from the
    /// invocation has been published.
    #[must_use]
    pub const fn invocation_failure(&self) -> Option<&JsProducerFailure> {
        self.invocation_failure.as_ref()
    }

    /// Return every scope bound by the enabled recognizers in canonical order.
    ///
    /// Orchestration uses this set when a source or invocation failure weakens
    /// the reference side of scope completeness.
    #[must_use]
    pub const fn producer_scopes(&self) -> &[CatalogScopeId] {
        &self.producer_scopes
    }

    /// Report whether every selected source group produced a complete artifact.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.invocation_failure.is_none() && self.source_failures.is_empty()
    }
}

/// Produce checked artifacts without a producer cache or cancellation.
pub fn produce_reference_artifacts(
    groups: Vec<JsPhysicalSourceGroup>,
    recognizers: &JsRecognizerSet,
    limits: &LinkLimits,
) -> Result<JsProducerOutcome, JsProducerError> {
    produce_impl::<DisabledCache, _>(groups, recognizers, limits, None, &|| false)
}

/// Produce checked artifacts without a cache and with caller-owned cancellation.
pub fn produce_reference_artifacts_with_cancellation<C>(
    groups: Vec<JsPhysicalSourceGroup>,
    recognizers: &JsRecognizerSet,
    limits: &LinkLimits,
    cancelled: &C,
) -> Result<JsProducerOutcome, JsProducerError>
where
    C: Fn() -> bool + ?Sized,
{
    produce_impl::<DisabledCache, _>(groups, recognizers, limits, None, cancelled)
}

/// Produce checked artifacts with an optional-performance cache.
pub fn produce_reference_artifacts_with_cache<S>(
    groups: Vec<JsPhysicalSourceGroup>,
    recognizers: &JsRecognizerSet,
    limits: &LinkLimits,
    cache: &S,
) -> Result<JsProducerOutcome, JsProducerError>
where
    S: JsProducerCache + ?Sized,
{
    produce_impl(groups, recognizers, limits, Some(cache), &|| false)
}

/// Produce checked artifacts with cache reuse and caller-owned cancellation.
pub fn produce_reference_artifacts_with_cache_and_cancellation<S, C>(
    groups: Vec<JsPhysicalSourceGroup>,
    recognizers: &JsRecognizerSet,
    limits: &LinkLimits,
    cache: &S,
    cancelled: &C,
) -> Result<JsProducerOutcome, JsProducerError>
where
    S: JsProducerCache + ?Sized,
    C: Fn() -> bool + ?Sized,
{
    produce_impl(groups, recognizers, limits, Some(cache), cancelled)
}

/// Admit the invocation-wide physical-group count before source snapshots.
///
/// Filesystem-owning integrations call this after canonical physical grouping
/// and before reading source contents. The producer retains ownership of the
/// fixed ceiling, canonical first-over source selection, and failure evidence.
pub fn preflight_source_group_count(
    mut primary_sources: Vec<SourceDocumentIdentity>,
) -> Result<(), JsProducerFailure> {
    primary_sources.sort();
    let source_group_count =
        u64::try_from(primary_sources.len()).unwrap_or(SOURCE_GROUPS_LIMIT.saturating_add(1));
    if source_group_count <= SOURCE_GROUPS_LIMIT {
        return Ok(());
    }
    Err(JsProducerFailure::with_limit(
        JsProducerFailureReason::SourceCountLimit,
        primary_sources[SOURCE_GROUPS_LIMIT as usize].clone(),
        SOURCE_GROUPS_LIMIT,
        SOURCE_GROUPS_LIMIT + 1,
    ))
}

fn produce_impl<S, C>(
    mut groups: Vec<JsPhysicalSourceGroup>,
    recognizers: &JsRecognizerSet,
    limits: &LinkLimits,
    cache: Option<&S>,
    cancelled: &C,
) -> Result<JsProducerOutcome, JsProducerError>
where
    S: JsProducerCache + ?Sized,
    C: Fn() -> bool + ?Sized,
{
    if cancelled() {
        return Err(JsProducerError::Cancelled);
    }

    // Canonical primary order is established before any limit selects a
    // first-over source. The group-count gate deliberately runs before source
    // bytes, profiles, cache access, or parser work.
    groups.sort_by(|left, right| left.primary().cmp(right.primary()));
    let producer_scopes = producer_scopes(recognizers);

    if let Err(failure) =
        preflight_source_group_count(groups.iter().map(|group| group.primary().clone()).collect())
    {
        return Ok(global_failure_outcome(failure, producer_scopes));
    }
    reject_repeated_logical_sources(&groups)?;

    // Source-local failure selection follows the closed stage order. Identity
    // and profile admission therefore complete before snapshot failures are
    // selected, while the invocation-wide group-count gate above remains the
    // explicitly earlier inventory boundary.
    let mut group_contracts = Vec::with_capacity(groups.len());
    for group in &groups {
        if cancelled() {
            return Err(JsProducerError::Cancelled);
        }
        group_contracts.push(admit_group_contract(group)?);
    }

    // Per-source failures are retained, but only fully admitted snapshots enter
    // the aggregate running sum. An aggregate failure remains invocation-wide
    // and emits no source-local prefix.
    let admission = preflight_source_bytes(
        &groups,
        SourceAdmissionLimits {
            per_source: crate::SOURCE_BYTES_LIMIT,
            aggregate: SOURCE_BYTES_TOTAL_LIMIT,
        },
    );
    if let Some(global_failure) = admission.invocation_failure {
        return Ok(global_failure_outcome(global_failure, producer_scopes));
    }
    debug_assert_eq!(group_contracts.len(), groups.len());
    debug_assert_eq!(admission.per_group_failures.len(), groups.len());

    // Group execution remains source-local. Cache candidates stay private
    // until every group has reached a deterministic terminal outcome.
    let producer = js_producer_identity();
    let delivery_unit = DeliveryUnitId::main();
    let mut artifacts = Vec::new();
    let mut source_failures = Vec::new();
    let mut pending_cache = Vec::new();

    for ((group, contract), snapshot_failure) in groups
        .iter()
        .zip(group_contracts)
        .zip(admission.per_group_failures)
    {
        let (identity, profile) = match contract {
            GroupContractAdmission::Admitted { identity, profile } => (identity, profile),
            GroupContractAdmission::Failed(failure) => {
                source_failures.push(failure);
                continue;
            }
        };
        if let Some(snapshot_failure) = snapshot_failure {
            source_failures.push(snapshot_failure);
            continue;
        }
        if cancelled() {
            return Err(JsProducerError::Cancelled);
        }
        match produce_group(
            group,
            profile,
            identity,
            recognizers,
            limits,
            &producer,
            &delivery_unit,
            cache,
            cancelled,
        )? {
            GroupOutcome::Artifact {
                artifact,
                cache_publication,
            } => {
                artifacts.push(artifact);
                if let Some(cache_publication) = cache_publication {
                    pending_cache.push(cache_publication);
                }
            }
            GroupOutcome::Failed(failure) => source_failures.push(failure),
        }
    }

    // Canonicalize independently of worker-ready input order before allowing
    // any best-effort cache publication to become externally observable.
    if cancelled() {
        return Err(JsProducerError::Cancelled);
    }
    artifacts.sort_by(|left, right| left.identity().cmp(right.identity()));
    if artifacts
        .windows(2)
        .any(|pair| pair[0].identity() == pair[1].identity())
    {
        return Err(JsProducerError::InternalInvariant);
    }
    source_failures.sort_by(|left, right| {
        left.source()
            .cmp(right.source())
            .then_with(|| left.cmp_within_source(right))
    });

    if let Some(cache) = cache {
        for publication in pending_cache {
            let _ = cache.store(&publication.key, &publication.value);
        }
    }

    Ok(JsProducerOutcome {
        artifacts: artifacts.into_boxed_slice(),
        source_failures: source_failures.into_boxed_slice(),
        invocation_failure: None,
        producer_scopes,
    })
}

fn producer_scopes(recognizers: &JsRecognizerSet) -> Box<[CatalogScopeId]> {
    recognizers
        .bindings()
        .iter()
        .map(|binding| binding.scope().clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .into_boxed_slice()
}

fn global_failure_outcome(
    failure: JsProducerFailure,
    producer_scopes: Box<[CatalogScopeId]>,
) -> JsProducerOutcome {
    JsProducerOutcome {
        artifacts: Box::new([]),
        source_failures: Box::new([]),
        invocation_failure: Some(failure),
        producer_scopes,
    }
}

#[derive(Debug, Clone, Copy)]
struct SourceAdmissionLimits {
    per_source: u64,
    aggregate: u64,
}

struct SourceAdmission {
    per_group_failures: Vec<Option<JsProducerFailure>>,
    invocation_failure: Option<JsProducerFailure>,
}

fn preflight_source_bytes(
    groups: &[JsPhysicalSourceGroup],
    limits: SourceAdmissionLimits,
) -> SourceAdmission {
    let mut per_group_failures = Vec::with_capacity(groups.len());
    let mut aggregate = 0_u64;

    for group in groups {
        let observed = u64::try_from(group.source_bytes().len())
            .unwrap_or(limits.per_source.saturating_add(1));
        if observed > limits.per_source {
            per_group_failures.push(Some(JsProducerFailure::with_limit(
                JsProducerFailureReason::SourceBytesLimit,
                group.primary().clone(),
                limits.per_source,
                limits.per_source + 1,
            )));
            continue;
        }

        let prospective = aggregate
            .checked_add(observed)
            .expect("fixed source count and byte ceilings cannot overflow u64");
        if prospective > limits.aggregate {
            return SourceAdmission {
                per_group_failures,
                invocation_failure: Some(JsProducerFailure::with_limit(
                    JsProducerFailureReason::SourceBytesTotalLimit,
                    group.primary().clone(),
                    limits.aggregate,
                    prospective,
                )),
            };
        }
        aggregate = prospective;
        per_group_failures.push(None);
    }

    SourceAdmission {
        per_group_failures,
        invocation_failure: None,
    }
}

fn reject_repeated_logical_sources(
    groups: &[JsPhysicalSourceGroup],
) -> Result<(), JsProducerError> {
    let mut seen = BTreeSet::new();
    for source in groups.iter().flat_map(JsPhysicalSourceGroup::all_sources) {
        if !seen.insert(source) {
            return Err(JsProducerError::InternalInvariant);
        }
    }
    Ok(())
}

enum GroupOutcome {
    Artifact {
        artifact: MessageReferenceArtifact,
        cache_publication: Option<CachePublication>,
    },
    Failed(JsProducerFailure),
}

enum GroupContractAdmission {
    Admitted {
        identity: ReferenceArtifactIdentity,
        profile: JsSourceProfile,
    },
    Failed(JsProducerFailure),
}

struct CachePublication {
    key: JsProducerCacheKey,
    value: Box<[u8]>,
}

struct ProducedCacheMiss {
    artifact: MessageReferenceArtifact,
    artifact_bytes: Box<[u8]>,
    selected_goal: JsSelectedSourceGoal,
}

enum CacheMissOutcome {
    Produced(ProducedCacheMiss),
    Failed(JsProducerFailure),
}

#[allow(clippy::too_many_arguments)]
fn produce_group<S, C>(
    group: &JsPhysicalSourceGroup,
    profile: JsSourceProfile,
    identity: ReferenceArtifactIdentity,
    recognizers: &JsRecognizerSet,
    limits: &LinkLimits,
    producer: &ProducerIdentity,
    delivery_unit: &DeliveryUnitId,
    cache: Option<&S>,
    cancelled: &C,
) -> Result<GroupOutcome, JsProducerError>
where
    S: JsProducerCache + ?Sized,
    C: Fn() -> bool + ?Sized,
{
    let Ok(source_text) = std::str::from_utf8(group.source_bytes()) else {
        return Ok(GroupOutcome::Failed(JsProducerFailure::without_optional(
            JsProducerFailureReason::InvalidUtf8,
            group.primary().clone(),
        )));
    };
    let fingerprint = cache.map(|_| SourceFingerprint::for_bytes(group.source_bytes()));

    if let (Some(cache), Some(fingerprint)) = (cache, fingerprint) {
        if let Some(artifact) = try_load_cached_artifact(
            cache,
            group,
            profile,
            fingerprint,
            recognizers,
            limits,
            producer,
            &identity,
            delivery_unit,
            source_text,
            cancelled,
        )? {
            return Ok(GroupOutcome::Artifact {
                artifact,
                cache_publication: None,
            });
        }
    }

    let produced = match produce_cache_miss(
        group,
        profile,
        identity,
        recognizers,
        limits,
        producer,
        delivery_unit,
        cancelled,
    )? {
        CacheMissOutcome::Produced(produced) => produced,
        CacheMissOutcome::Failed(failure) => return Ok(GroupOutcome::Failed(failure)),
    };

    let cache_publication = fingerprint.map(|fingerprint| {
        let key = key_for_group(CacheKeyContext {
            version: ArtifactVersion::DRAFT_V0_1,
            producer,
            group,
            profile,
            selected_goal: produced.selected_goal,
            recognizers,
            delivery_unit,
            fingerprint,
        });
        CachePublication {
            value: encode_cache_entry(&key, &produced.artifact_bytes),
            key,
        }
    });
    Ok(GroupOutcome::Artifact {
        artifact: produced.artifact,
        cache_publication,
    })
}

fn admit_group_contract(
    group: &JsPhysicalSourceGroup,
) -> Result<GroupContractAdmission, JsProducerError> {
    let identity = match build_artifact_identity(group.primary()) {
        Ok(identity) => identity,
        Err(JsProducerError::Failed(failure)) => {
            return Ok(GroupContractAdmission::Failed(failure));
        }
        Err(other) => return Err(other),
    };
    let profile = match admit_group_profile(group) {
        Ok(profile) => profile,
        Err(failure) => return Ok(GroupContractAdmission::Failed(failure)),
    };
    Ok(GroupContractAdmission::Admitted { identity, profile })
}

#[allow(clippy::too_many_arguments)]
fn try_load_cached_artifact<S, C>(
    cache: &S,
    group: &JsPhysicalSourceGroup,
    profile: JsSourceProfile,
    fingerprint: SourceFingerprint,
    recognizers: &JsRecognizerSet,
    limits: &LinkLimits,
    producer: &ProducerIdentity,
    identity: &ReferenceArtifactIdentity,
    delivery_unit: &DeliveryUnitId,
    source_text: &str,
    cancelled: &C,
) -> Result<Option<MessageReferenceArtifact>, JsProducerError>
where
    S: JsProducerCache + ?Sized,
    C: Fn() -> bool + ?Sized,
{
    for selected_goal in cache_goal_candidates(profile.goal()) {
        if cancelled() {
            return Err(JsProducerError::Cancelled);
        }
        let key = key_for_group(CacheKeyContext {
            version: ArtifactVersion::DRAFT_V0_1,
            producer,
            group,
            profile,
            selected_goal: *selected_goal,
            recognizers,
            delivery_unit,
            fingerprint,
        });
        let Ok(Some(entry)) = cache.load(&key) else {
            continue;
        };
        let Some(artifact_bytes) = decode_cache_entry(&key, &entry) else {
            continue;
        };
        if let Some(artifact) = admit_cached_artifact(
            artifact_bytes,
            producer,
            identity,
            delivery_unit,
            group.primary(),
            source_text,
            recognizers,
            limits,
        ) {
            return Ok(Some(artifact));
        }
    }
    Ok(None)
}

#[allow(clippy::too_many_arguments)]
fn produce_cache_miss<C>(
    group: &JsPhysicalSourceGroup,
    profile: JsSourceProfile,
    identity: ReferenceArtifactIdentity,
    recognizers: &JsRecognizerSet,
    limits: &LinkLimits,
    producer: &ProducerIdentity,
    delivery_unit: &DeliveryUnitId,
    cancelled: &C,
) -> Result<CacheMissOutcome, JsProducerError>
where
    C: Fn() -> bool + ?Sized,
{
    let scan = match scan_source_with_cancellation(
        group.primary(),
        group.source_bytes(),
        recognizers,
        cancelled,
    ) {
        Ok(scan) => scan,
        Err(JsProducerError::Failed(failure)) => return Ok(CacheMissOutcome::Failed(failure)),
        Err(other) => return Err(other),
    };
    debug_assert_eq!(scan.profile(), profile);
    let selected_goal = scan.selected_goal();
    let artifact = match MessageReferenceArtifact::try_new(
        ArtifactVersion::DRAFT_V0_1,
        producer.clone(),
        identity,
        delivery_unit.clone(),
        scan.into_references(),
        limits,
    ) {
        Ok(artifact) => artifact,
        Err(error) => {
            return Ok(CacheMissOutcome::Failed(artifact_error(
                group.primary(),
                &error,
                ArtifactFailureContext::Construction,
            )?));
        }
    };
    let artifact_bytes = match encode_reference_artifact(&artifact, limits) {
        Ok(artifact_bytes) => artifact_bytes,
        Err(error) => {
            return Ok(CacheMissOutcome::Failed(artifact_error(
                group.primary(),
                &error,
                ArtifactFailureContext::Encoding,
            )?));
        }
    };

    Ok(CacheMissOutcome::Produced(ProducedCacheMiss {
        artifact,
        artifact_bytes,
        selected_goal,
    }))
}

fn admit_group_profile(
    group: &JsPhysicalSourceGroup,
) -> Result<JsSourceProfile, JsProducerFailure> {
    let profile = JsSourceProfile::from_source(group.primary()).map_err(|_| {
        JsProducerFailure::without_optional(
            JsProducerFailureReason::UnsupportedSourceSuffix,
            group.primary().clone(),
        )
    })?;
    for alias in group.aliases() {
        let alias_profile = JsSourceProfile::from_source(alias).map_err(|_| {
            JsProducerFailure::without_optional(
                JsProducerFailureReason::UnsupportedSourceSuffix,
                group.primary().clone(),
            )
        })?;
        if alias_profile != profile {
            return Err(JsProducerFailure::without_optional(
                JsProducerFailureReason::ConflictingAliasProfile,
                group.primary().clone(),
            ));
        }
    }
    Ok(profile)
}

fn build_artifact_identity(
    source: &SourceDocumentIdentity,
) -> Result<ReferenceArtifactIdentity, JsProducerError> {
    let mut segments = Vec::with_capacity(source.path().segments().len() + 2);
    for value in ["js", "module"] {
        segments.push(
            ReferenceArtifactSegment::try_new(value)
                .expect("the built-in artifact identity prefix is valid"),
        );
    }
    for path_segment in source.path().segments() {
        match ReferenceArtifactSegment::try_new(path_segment.as_str()) {
            Ok(segment) => segments.push(segment),
            Err(error) => return Err(identity_error(source, &error)),
        }
    }
    ReferenceArtifactIdentity::try_new(ArtifactNamespace::Project, segments)
        .map_err(|error| identity_error(source, &error))
}

fn identity_error(
    source: &SourceDocumentIdentity,
    error: &ValueConstructionError,
) -> JsProducerError {
    let evidence = match error {
        ValueConstructionError::StructuralLimit(evidence) => {
            Some((evidence.limit(), evidence.attempted()))
        }
        ValueConstructionError::FieldLimit {
            limit, attempted, ..
        } => Some((*limit, *attempted)),
        ValueConstructionError::Grammar(_) | ValueConstructionError::Range(_) => None,
    };
    match evidence {
        Some((limit, observed)) => JsProducerFailure::with_limit(
            JsProducerFailureReason::ArtifactIdentityLimit,
            source.clone(),
            limit,
            observed,
        )
        .into(),
        None => JsProducerError::InternalInvariant,
    }
}

#[derive(Clone, Copy)]
enum ArtifactFailureContext {
    Construction,
    Encoding,
}

fn artifact_error(
    source: &SourceDocumentIdentity,
    error: &ArtifactContractError,
    context: ArtifactFailureContext,
) -> Result<JsProducerFailure, JsProducerError> {
    match error {
        ArtifactContractError::Limit(evidence) => match evidence.observation() {
            LinkLimitObservation::Exact(observed) => Ok(JsProducerFailure::with_limit(
                JsProducerFailureReason::OutputLimit,
                source.clone(),
                evidence.effective_limit(),
                observed,
            )),
            LinkLimitObservation::ArithmeticOverflow => Err(JsProducerError::InternalInvariant),
        },
        ArtifactContractError::InvalidArtifact(_)
        | ArtifactContractError::UnsupportedVersion(_) => Ok(JsProducerFailure::without_optional(
            match context {
                ArtifactFailureContext::Construction => {
                    JsProducerFailureReason::OutputContractInvalid
                }
                ArtifactFailureContext::Encoding => {
                    JsProducerFailureReason::CanonicalEncodingFailed
                }
            },
            source.clone(),
        )),
    }
}

fn cache_goal_candidates(goal: JsSourceGoal) -> &'static [JsSelectedSourceGoal] {
    const MODULE: &[JsSelectedSourceGoal] = &[JsSelectedSourceGoal::Module];
    const SCRIPT: &[JsSelectedSourceGoal] = &[JsSelectedSourceGoal::Script];
    const BOUNDED: &[JsSelectedSourceGoal] =
        &[JsSelectedSourceGoal::Module, JsSelectedSourceGoal::Script];
    match goal {
        JsSourceGoal::BoundedUnambiguous => BOUNDED,
        JsSourceGoal::Module => MODULE,
        JsSourceGoal::CommonJs => SCRIPT,
    }
}

#[allow(clippy::too_many_arguments)]
fn admit_cached_artifact(
    artifact_bytes: &[u8],
    producer: &ProducerIdentity,
    expected_identity: &ReferenceArtifactIdentity,
    delivery_unit: &DeliveryUnitId,
    primary: &SourceDocumentIdentity,
    source_text: &str,
    recognizers: &JsRecognizerSet,
    limits: &LinkLimits,
) -> Option<MessageReferenceArtifact> {
    let artifact = decode_reference_artifact(artifact_bytes, limits).ok()?;
    if artifact.version() != &ArtifactVersion::DRAFT_V0_1
        || artifact.producer() != producer
        || artifact.identity() != expected_identity
        || artifact.delivery_unit() != delivery_unit
        || !cached_records_are_current(&artifact, primary, source_text, recognizers)
    {
        return None;
    }
    let canonical = encode_reference_artifact(&artifact, limits).ok()?;
    (canonical.as_ref() == artifact_bytes).then_some(artifact)
}

fn cached_records_are_current(
    artifact: &MessageReferenceArtifact,
    primary: &SourceDocumentIdentity,
    source_text: &str,
    recognizers: &JsRecognizerSet,
) -> bool {
    let mut previous = None;
    for reference in artifact.references() {
        let Some(origin) = reference.origin() else {
            return false;
        };
        if origin.source() != primary {
            return false;
        }
        if !cached_reference_shape_is_admitted(reference, recognizers) {
            return false;
        }
        let span = origin.span();
        let start = span.start() as usize;
        let end = span.end() as usize;
        if start == end
            || end > source_text.len()
            || !source_text.is_char_boundary(start)
            || !source_text.is_char_boundary(end)
        {
            return false;
        }
        let current = (span.start(), span.end());
        if previous.is_some_and(|previous| previous >= current) {
            return false;
        }
        previous = Some(current);
    }
    true
}

fn cached_reference_shape_is_admitted(
    reference: &MessageReference,
    recognizers: &JsRecognizerSet,
) -> bool {
    recognizers.bindings().iter().any(|binding| {
        if binding.scope() != reference.scope() || binding.domain() != reference.domain() {
            return false;
        }
        match (binding.kind(), reference.selector(), reference.reason()) {
            (crate::JsRecognizerCallKind::Lookup, MessageSelector::Exact(_), None) => true,
            (
                crate::JsRecognizerCallKind::Lookup,
                MessageSelector::UnboundedDynamic,
                Some(reason),
            ) => reason.as_str() == DYNAMIC_LOOKUP_REASON,
            (crate::JsRecognizerCallKind::Set, MessageSelector::Pattern(_), Some(reason)) => {
                reason.as_str() == BOUNDED_SET_REASON
            }
            (
                crate::JsRecognizerCallKind::Lookup | crate::JsRecognizerCallKind::Set,
                MessageSelector::Exact(_)
                | MessageSelector::Prefix(_)
                | MessageSelector::Pattern(_)
                | MessageSelector::AllInScope
                | MessageSelector::UnboundedDynamic,
                _,
            ) => false,
        }
    })
}

struct DisabledCache;

impl JsProducerCache for DisabledCache {
    fn load(&self, _key: &JsProducerCacheKey) -> Result<Option<Box<[u8]>>, JsProducerCacheIoError> {
        Ok(None)
    }

    fn store(
        &self,
        _key: &JsProducerCacheKey,
        _value: &[u8],
    ) -> Result<(), JsProducerCacheIoError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use intlify_contract::{
        encode_reference_artifact, ArtifactNamespace, ArtifactVersion, CatalogKeyDomain,
        CatalogScopeId, CatalogScopeName, DeliveryUnitId, LinkLimits, MessageReference,
        MessageReferenceArtifact, MessageSelector, PortablePathSegment, PortableRelativePath,
        SourceDocumentIdentity, SourceOrigin, SourceUtf8Span,
    };

    use super::{
        build_artifact_identity, preflight_source_bytes, produce_reference_artifacts,
        produce_reference_artifacts_with_cache, SourceAdmissionLimits,
    };
    use crate::cache::{
        encode_cache_entry, key_for_group, CacheKeyContext, JsProducerCache,
        JsProducerCacheIoError, JsProducerCacheKey, SourceFingerprint,
    };
    use crate::{
        js_producer_identity, JsKeySyntax, JsPhysicalSourceGroup, JsProducerFailureReason,
        JsRecognizerBinding, JsRecognizerCallKind, JsRecognizerSet, JsSelectedSourceGoal,
        JsSourceProfile,
    };

    fn group(name: &str, bytes: usize) -> JsPhysicalSourceGroup {
        let source = SourceDocumentIdentity::new(
            ArtifactNamespace::Project,
            PortableRelativePath::try_new(vec![
                PortablePathSegment::try_new("src").unwrap(),
                PortablePathSegment::try_new(name).unwrap(),
            ])
            .unwrap(),
        );
        JsPhysicalSourceGroup::try_new(vec![source], vec![0; bytes]).unwrap()
    }

    #[test]
    fn source_preflight_excludes_local_overruns_from_the_aggregate() {
        let groups = vec![group("a.ts", 5), group("b.ts", 11), group("c.ts", 5)];
        let admission = preflight_source_bytes(
            &groups,
            SourceAdmissionLimits {
                per_source: 10,
                aggregate: 10,
            },
        );
        assert_eq!(admission.per_group_failures.len(), 3);
        assert!(admission.per_group_failures[0].is_none());
        assert_eq!(
            admission.per_group_failures[1].as_ref().unwrap().reason(),
            JsProducerFailureReason::SourceBytesLimit
        );
        assert!(admission.per_group_failures[2].is_none());
        assert!(admission.invocation_failure.is_none());
    }

    #[test]
    fn aggregate_preflight_reports_the_exact_canonical_prospective_sum() {
        let groups = vec![group("a.ts", 6), group("b.ts", 5)];
        let admission = preflight_source_bytes(
            &groups,
            SourceAdmissionLimits {
                per_source: 10,
                aggregate: 10,
            },
        );
        let failure = admission.invocation_failure.unwrap();
        assert_eq!(
            failure.reason(),
            JsProducerFailureReason::SourceBytesTotalLimit
        );
        assert_eq!(failure.limit(), Some(10));
        assert_eq!(failure.observed(), Some(11));
        assert_eq!(failure.source(), groups[1].primary());
    }

    struct SingleEntryCache {
        key: JsProducerCacheKey,
        entry: Box<[u8]>,
        stores: AtomicUsize,
    }

    impl JsProducerCache for SingleEntryCache {
        fn load(
            &self,
            key: &JsProducerCacheKey,
        ) -> Result<Option<Box<[u8]>>, JsProducerCacheIoError> {
            Ok((key == &self.key).then(|| self.entry.clone()))
        }

        fn store(
            &self,
            _key: &JsProducerCacheKey,
            _value: &[u8],
        ) -> Result<(), JsProducerCacheIoError> {
            self.stores.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn structurally_valid_but_non_producer_cache_records_regenerate() {
        let source = SourceDocumentIdentity::new(
            ArtifactNamespace::Project,
            PortableRelativePath::try_new(vec![
                PortablePathSegment::try_new("src").unwrap(),
                PortablePathSegment::try_new("app.ts").unwrap(),
            ])
            .unwrap(),
        );
        let group =
            JsPhysicalSourceGroup::try_new(vec![source.clone()], b"t('a')".to_vec()).unwrap();
        let scope = CatalogScopeId::new(
            ArtifactNamespace::Project,
            CatalogScopeName::try_new("app").unwrap(),
        );
        let recognizers = JsRecognizerSet::try_new(vec![JsRecognizerBinding::try_new(
            "t",
            JsRecognizerCallKind::Lookup,
            scope.clone(),
            CatalogKeyDomain::JsonPointer,
            JsKeySyntax::Literal,
        )
        .unwrap()])
        .unwrap();
        let limits = LinkLimits::default();
        let producer = js_producer_identity();
        let delivery_unit = DeliveryUnitId::main();
        let identity = build_artifact_identity(&source).unwrap();
        let wrong_reference = MessageReference::try_new(
            scope,
            CatalogKeyDomain::JsonPointer,
            MessageSelector::AllInScope,
            None,
            Some(SourceOrigin::new(
                source,
                SourceUtf8Span::try_new(2, 5).unwrap(),
            )),
        )
        .unwrap();
        let wrong_artifact = MessageReferenceArtifact::try_new(
            ArtifactVersion::DRAFT_V0_1,
            producer.clone(),
            identity,
            delivery_unit.clone(),
            vec![wrong_reference],
            &limits,
        )
        .unwrap();
        let wrong_bytes = encode_reference_artifact(&wrong_artifact, &limits).unwrap();
        let profile = JsSourceProfile::from_source(group.primary()).unwrap();
        let key = key_for_group(CacheKeyContext {
            version: ArtifactVersion::DRAFT_V0_1,
            producer: &producer,
            group: &group,
            profile,
            selected_goal: JsSelectedSourceGoal::Module,
            recognizers: &recognizers,
            delivery_unit: &delivery_unit,
            fingerprint: SourceFingerprint::for_bytes(group.source_bytes()),
        });
        let cache = SingleEntryCache {
            entry: encode_cache_entry(&key, &wrong_bytes),
            key,
            stores: AtomicUsize::new(0),
        };

        let outcome =
            produce_reference_artifacts_with_cache(vec![group], &recognizers, &limits, &cache)
                .unwrap();

        assert_eq!(outcome.artifacts().len(), 1);
        assert!(matches!(
            outcome.artifacts()[0].references()[0].selector(),
            MessageSelector::Exact(_)
        ));
        assert_eq!(cache.stores.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn unsupported_cached_artifact_version_regenerates_from_source() {
        let source = SourceDocumentIdentity::new(
            ArtifactNamespace::Project,
            PortableRelativePath::try_new(vec![
                PortablePathSegment::try_new("src").unwrap(),
                PortablePathSegment::try_new("app.ts").unwrap(),
            ])
            .unwrap(),
        );
        let group = JsPhysicalSourceGroup::try_new(vec![source], b"t('a')".to_vec()).unwrap();
        let scope = CatalogScopeId::new(
            ArtifactNamespace::Project,
            CatalogScopeName::try_new("app").unwrap(),
        );
        let recognizers = JsRecognizerSet::try_new(vec![JsRecognizerBinding::try_new(
            "t",
            JsRecognizerCallKind::Lookup,
            scope,
            CatalogKeyDomain::JsonPointer,
            JsKeySyntax::Literal,
        )
        .unwrap()])
        .unwrap();
        let limits = LinkLimits::default();
        let authoritative =
            produce_reference_artifacts(vec![group.clone()], &recognizers, &limits).unwrap();
        let mut unsupported = encode_reference_artifact(&authoritative.artifacts()[0], &limits)
            .unwrap()
            .into_vec();
        let version = b"\"minor\":1";
        let offset = unsupported
            .windows(version.len())
            .position(|window| window == version)
            .expect("canonical v0.1 artifact contains its minor version");
        unsupported[offset + version.len() - 1] = b'2';

        let producer = js_producer_identity();
        let delivery_unit = DeliveryUnitId::main();
        let profile = JsSourceProfile::from_source(group.primary()).unwrap();
        let key = key_for_group(CacheKeyContext {
            version: ArtifactVersion::DRAFT_V0_1,
            producer: &producer,
            group: &group,
            profile,
            selected_goal: JsSelectedSourceGoal::Module,
            recognizers: &recognizers,
            delivery_unit: &delivery_unit,
            fingerprint: SourceFingerprint::for_bytes(group.source_bytes()),
        });
        let cache = SingleEntryCache {
            entry: encode_cache_entry(&key, &unsupported),
            key,
            stores: AtomicUsize::new(0),
        };

        let regenerated =
            produce_reference_artifacts_with_cache(vec![group], &recognizers, &limits, &cache)
                .unwrap();

        assert_eq!(regenerated, authoritative);
        assert_eq!(cache.stores.load(Ordering::SeqCst), 1);
    }
}
