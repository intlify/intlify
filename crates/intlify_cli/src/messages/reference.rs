// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Configuration-authoritative reference inventory and artifact ingestion.
//!
//! This module owns built-in JS/TS source selection, host physical grouping,
//! bounded source snapshots, producer invocation, exact external-artifact
//! grouping and decoding, and the shared in-process producer/decode cache. It
//! retains source-local failures for completeness construction and leaves
//! request admission, semantic linking, reporting, and filesystem output to
//! their owning layers.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use intlify_contract::{
    decode_reference_artifact, encode_reference_artifact, ArtifactContractError, ArtifactNamespace,
    CatalogScopeId, LinkLimitCounter, LinkLimits, MessageReferenceArtifact, PortableRelativePath,
    SourceDocumentIdentity, ValueConstructionError,
};
#[cfg(feature = "benchmark")]
use intlify_producer_js::benchmark::{
    benchmark_produce_reference_artifacts, benchmark_produce_reference_artifacts_with_cache,
    BenchmarkJsStage,
};
use intlify_producer_js::{
    preflight_source_group_count, produce_reference_artifacts,
    produce_reference_artifacts_with_cache, JsGrammarProfile, JsPhysicalSourceGroup,
    JsProducerCache, JsProducerCacheIoError, JsProducerCacheKey, JsProducerError,
    JsProducerFailure, JsRecognizerSet, JsSourceGoal, JsSourceProfile, SOURCE_BYTES_LIMIT,
};

use super::config::{ResolvedJsProducerConfig, ResolvedMessageProducers};
use super::observation::{
    observation_checksum, MessageBenchmarkObserver, MessageBenchmarkStage,
    NoopMessageBenchmarkObserver,
};
use super::physical::{
    acquire_physical_snapshot, compare_optional_paths, discover_project_files,
    group_physical_files, portable_path, PhysicalFileGroup, ProjectFileTarget,
};
use crate::error::OperationalError;

const EXTERNAL_REFERENCE_DECODER_REVISION: u32 = 1;

/// Thread-safe cache shared by repeated in-process project-link invocations.
#[derive(Debug, Default)]
pub(crate) struct MessageLinkCache {
    js_entries: Mutex<HashMap<[u8; 32], Box<[u8]>>>,
    external_entries: Mutex<BTreeMap<ExternalArtifactCacheKey, MessageReferenceArtifact>>,
    js_loads: AtomicU64,
    js_hits: AtomicU64,
    external_loads: AtomicU64,
    external_hits: AtomicU64,
}

impl MessageLinkCache {
    fn load_external(
        &self,
        key: &ExternalArtifactCacheKey,
        limits: &LinkLimits,
    ) -> Option<MessageReferenceArtifact> {
        self.external_loads.fetch_add(1, Ordering::Relaxed);
        let artifact = self.external_entries.lock().ok()?.get(key)?.clone();
        // Effective limits are a compatibility predicate, not exact cache
        // identity. Revalidation permits reuse under different limits only
        // when the immutable decoded artifact still passes all typed counters.
        if artifact.revalidate(limits).is_err() {
            return None;
        }
        self.external_hits.fetch_add(1, Ordering::Relaxed);
        Some(artifact)
    }

    fn store_external(&self, key: ExternalArtifactCacheKey, artifact: MessageReferenceArtifact) {
        if let Ok(mut entries) = self.external_entries.lock() {
            entries.insert(key, artifact);
        }
    }

    #[cfg(test)]
    pub(crate) fn stats(&self) -> MessageLinkCacheStats {
        MessageLinkCacheStats {
            js_loads: self.js_loads.load(Ordering::Relaxed),
            js_hits: self.js_hits.load(Ordering::Relaxed),
            external_loads: self.external_loads.load(Ordering::Relaxed),
            external_hits: self.external_hits.load(Ordering::Relaxed),
        }
    }
}

impl JsProducerCache for MessageLinkCache {
    fn load(&self, key: &JsProducerCacheKey) -> Result<Option<Box<[u8]>>, JsProducerCacheIoError> {
        self.js_loads.fetch_add(1, Ordering::Relaxed);
        let entries = self
            .js_entries
            .lock()
            .map_err(|_| cache_lock_error("JavaScript producer cache"))?;
        let value = entries.get(key.lookup_digest()).cloned();
        if value.is_some() {
            self.js_hits.fetch_add(1, Ordering::Relaxed);
        }
        Ok(value)
    }

    fn store(&self, key: &JsProducerCacheKey, value: &[u8]) -> Result<(), JsProducerCacheIoError> {
        self.js_entries
            .lock()
            .map_err(|_| cache_lock_error("JavaScript producer cache"))?
            .insert(*key.lookup_digest(), value.to_vec().into_boxed_slice());
        Ok(())
    }
}

fn cache_lock_error(cache: &'static str) -> JsProducerCacheIoError {
    Box::new(CacheLockError(cache))
}

#[derive(Debug)]
struct CacheLockError(&'static str);

impl fmt::Display for CacheLockError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} lock is poisoned", self.0)
    }
}

impl Error for CacheLockError {}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MessageLinkCacheStats {
    pub(crate) js_loads: u64,
    pub(crate) js_hits: u64,
    pub(crate) external_loads: u64,
    pub(crate) external_hits: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ExternalArtifactCacheKey {
    primary: PortableRelativePath,
    byte_length: u64,
    digest: [u8; 32],
    decoder_revision: u32,
}

impl ExternalArtifactCacheKey {
    fn for_snapshot(primary: PortableRelativePath, bytes: &[u8]) -> Self {
        Self {
            primary,
            byte_length: u64::try_from(bytes.len())
                .expect("an admitted artifact snapshot length fits into u64"),
            digest: *blake3::hash(bytes).as_bytes(),
            decoder_revision: EXTERNAL_REFERENCE_DECODER_REVISION,
        }
    }
}

/// Complete successful and failed reference-side execution evidence.
#[derive(Debug)]
pub(crate) struct ReferenceInventory {
    artifacts: Vec<MessageReferenceArtifact>,
    failures: Vec<ReferenceSourceFailure>,
}

impl ReferenceInventory {
    /// Return successful reference artifacts in canonical identity order.
    pub(crate) fn artifacts(&self) -> &[MessageReferenceArtifact] {
        &self.artifacts
    }

    /// Return retained source-local failures in deterministic participant order.
    pub(crate) fn failures(&self) -> &[ReferenceSourceFailure] {
        &self.failures
    }

    /// Consume the inventory into its owned artifact and failure collections.
    pub(crate) fn into_parts(self) -> (Vec<MessageReferenceArtifact>, Vec<ReferenceSourceFailure>) {
        (self.artifacts, self.failures)
    }
}

/// Reference participant selected by host inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReferenceParticipant {
    /// Built-in JavaScript and TypeScript source producer.
    BuiltInJs,
    /// Configured external message-reference artifact.
    ExternalArtifact,
}

/// One source-local reference-side failure retained for operational evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReferenceSourceFailure {
    /// Host discovery, metadata, or snapshot acquisition failed.
    Host {
        participant: ReferenceParticipant,
        affected_scopes: Box<[CatalogScopeId]>,
        error: OperationalError,
    },
    /// A selected JS logical identity could not enter the contract boundary.
    Identity {
        affected_scopes: Box<[CatalogScopeId]>,
        path: String,
        error: ValueConstructionError,
    },
    /// The built-in producer returned checked source-attributable evidence.
    Producer {
        affected_scopes: Box<[CatalogScopeId]>,
        failure: JsProducerFailure,
    },
    /// One authoritative external snapshot failed ordinary artifact decoding.
    ExternalArtifact {
        affected_scopes: Box<[CatalogScopeId]>,
        host_primary: PortableRelativePath,
        error: ArtifactContractError,
    },
}

impl ReferenceSourceFailure {
    /// Return scopes whose reference side cannot be proven closed.
    pub(crate) fn affected_scopes(&self) -> &[CatalogScopeId] {
        match self {
            Self::Host {
                affected_scopes, ..
            }
            | Self::Identity {
                affected_scopes, ..
            }
            | Self::Producer {
                affected_scopes, ..
            }
            | Self::ExternalArtifact {
                affected_scopes, ..
            } => affected_scopes,
        }
    }
}

/// Fatal reference-inventory failure that cannot become partial evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReferenceInventoryError {
    /// The producer reported cancellation or an internal invariant failure.
    Producer(JsProducerError),
    /// Host grouping produced an impossible empty logical-source set.
    InternalInvariant,
}

impl fmt::Display for ReferenceInventoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Producer(error) => error.fmt(formatter),
            Self::InternalInvariant => formatter.write_str("reference inventory invariant failed"),
        }
    }
}

impl Error for ReferenceInventoryError {}

#[derive(Debug)]
struct ReferenceTarget {
    relative: String,
    absolute: PathBuf,
}

impl ProjectFileTarget for ReferenceTarget {
    fn project_path(&self) -> &str {
        &self.relative
    }

    fn absolute_path(&self) -> &Path {
        &self.absolute
    }
}

#[derive(Debug)]
struct ExternalArtifactTarget {
    relative: PortableRelativePath,
    label: String,
    absolute: PathBuf,
}

struct ExternalArtifactSnapshot {
    primary: PortableRelativePath,
    primary_label: String,
    logical_paths: Option<Box<[String]>>,
    bytes: Box<[u8]>,
}

impl ProjectFileTarget for ExternalArtifactTarget {
    fn project_path(&self) -> &str {
        &self.label
    }

    fn absolute_path(&self) -> &Path {
        &self.absolute
    }
}

struct PendingReferenceFailure {
    order_path: Option<String>,
    rank: u8,
    failure: ReferenceSourceFailure,
}

/// Produce the complete built-in and configured external reference inventory.
pub(crate) fn produce_reference_inventory(
    project_root: &Path,
    target_scopes: &[CatalogScopeId],
    producers: &ResolvedMessageProducers,
    limits: &LinkLimits,
    cache: Option<&MessageLinkCache>,
) -> Result<ReferenceInventory, ReferenceInventoryError> {
    produce_reference_inventory_with_observer(
        project_root,
        target_scopes,
        producers,
        limits,
        cache,
        &mut NoopMessageBenchmarkObserver,
    )
}

/// Produce references through the ordinary pipeline while reporting opaque
/// benchmark stages to the non-default observer.
pub(crate) fn produce_reference_inventory_with_observer<O>(
    project_root: &Path,
    target_scopes: &[CatalogScopeId],
    producers: &ResolvedMessageProducers,
    limits: &LinkLimits,
    cache: Option<&MessageLinkCache>,
    observer: &mut O,
) -> Result<ReferenceInventory, ReferenceInventoryError>
where
    O: MessageBenchmarkObserver + ?Sized,
{
    let mut artifacts = Vec::new();
    let mut pending_failures = Vec::new();

    if let Some(js) = producers.js() {
        produce_js_inventory(
            project_root,
            js,
            limits,
            cache,
            &mut artifacts,
            &mut pending_failures,
            observer,
        )?;
    }

    produce_external_inventory(
        project_root,
        target_scopes,
        producers.artifacts(),
        limits,
        cache,
        &mut artifacts,
        &mut pending_failures,
        observer,
    );

    artifacts.sort_by(|left, right| left.identity().cmp(right.identity()));
    pending_failures.sort_by(|left, right| {
        compare_optional_paths(left.order_path.as_deref(), right.order_path.as_deref())
            .then_with(|| left.rank.cmp(&right.rank))
    });

    Ok(ReferenceInventory {
        artifacts,
        failures: pending_failures
            .into_iter()
            .map(|pending| pending.failure)
            .collect(),
    })
}

fn produce_js_inventory<O>(
    project_root: &Path,
    js: &ResolvedJsProducerConfig,
    limits: &LinkLimits,
    cache: Option<&MessageLinkCache>,
    artifacts: &mut Vec<MessageReferenceArtifact>,
    pending_failures: &mut Vec<PendingReferenceFailure>,
    observer: &mut O,
) -> Result<(), ReferenceInventoryError>
where
    O: MessageBenchmarkObserver + ?Sized,
{
    let producer_scopes = recognizer_scopes(js.recognizers());
    observer.begin(
        MessageBenchmarkStage::InventoryMetadataIo,
        "reference-js-inventory",
    );
    let discovery = discover_project_files(project_root);
    pending_failures.extend(
        discovery
            .issues
            .into_iter()
            .map(|issue| PendingReferenceFailure {
                order_path: issue.order_path,
                rank: 0,
                failure: ReferenceSourceFailure::Host {
                    participant: ReferenceParticipant::BuiltInJs,
                    affected_scopes: producer_scopes.clone(),
                    error: issue.error,
                },
            }),
    );

    let targets = discovery
        .candidates
        .into_iter()
        .filter(|candidate| {
            js.include()
                .iter()
                .any(|pattern| pattern.is_match(candidate.relative.as_str()))
        })
        .map(|candidate| ReferenceTarget {
            relative: candidate.relative.as_str().to_owned(),
            absolute: candidate.absolute,
        })
        .collect();
    let (groups, metadata_failures) = group_physical_files(targets);
    pending_failures.extend(metadata_failures.into_iter().map(|failure| {
        let path = failure.target.relative.clone();
        PendingReferenceFailure {
            order_path: Some(path),
            rank: 0,
            failure: ReferenceSourceFailure::Host {
                participant: ReferenceParticipant::BuiltInJs,
                affected_scopes: producer_scopes.clone(),
                error: failure.error,
            },
        }
    }));

    let mut admitted_groups = Vec::with_capacity(groups.len());
    for group in groups {
        match source_identities(&group) {
            Ok(identities) => admitted_groups.push((group, identities)),
            Err((path, error)) => pending_failures.push(PendingReferenceFailure {
                order_path: Some(path.clone()),
                rank: 1,
                failure: ReferenceSourceFailure::Identity {
                    affected_scopes: producer_scopes.clone(),
                    path,
                    error,
                },
            }),
        }
    }
    observer.finish(
        MessageBenchmarkStage::InventoryMetadataIo,
        "reference-js-inventory",
    );
    if observer.enabled() {
        let groups = admitted_groups
            .iter()
            .map(|(group, _)| group)
            .collect::<Vec<_>>();
        let observation_fields = inventory_observation(b"reference-js", &groups);
        observer.observe(
            MessageBenchmarkStage::InventoryMetadataIo,
            "reference-js-inventory",
            observation_checksum(
                MessageBenchmarkStage::InventoryMetadataIo.cost().as_bytes(),
                observation_fields.iter().map(Vec::as_slice),
            ),
        );
    }

    if let Err(failure) = preflight_source_group_count(
        admitted_groups
            .iter()
            .map(|(_, identities)| identities[0].clone())
            .collect(),
    ) {
        pending_failures.push(producer_pending_failure(producer_scopes, failure));
        return Ok(());
    }

    observer.begin(
        MessageBenchmarkStage::ReferenceSnapshotRead,
        "reference-snapshots",
    );
    let mut producer_groups = Vec::with_capacity(admitted_groups.len());
    for (group, identities) in admitted_groups {
        match acquire_physical_snapshot(&group, SOURCE_BYTES_LIMIT) {
            Ok(snapshot) => {
                let producer_group = JsPhysicalSourceGroup::try_new(identities, snapshot)
                    .map_err(|_| ReferenceInventoryError::InternalInvariant)?;
                producer_groups.push(producer_group);
            }
            Err(error) => pending_failures.push(PendingReferenceFailure {
                order_path: Some(group.primary_path().to_owned()),
                rank: 0,
                failure: ReferenceSourceFailure::Host {
                    participant: ReferenceParticipant::BuiltInJs,
                    affected_scopes: producer_scopes.clone(),
                    error,
                },
            }),
        }
    }
    observer.finish(
        MessageBenchmarkStage::ReferenceSnapshotRead,
        "reference-snapshots",
    );
    if observer.enabled() {
        let observation_fields = reference_snapshot_observation(&producer_groups);
        observer.observe(
            MessageBenchmarkStage::ReferenceSnapshotRead,
            "reference-snapshots",
            observation_checksum(
                MessageBenchmarkStage::ReferenceSnapshotRead
                    .cost()
                    .as_bytes(),
                observation_fields.iter().map(Vec::as_slice),
            ),
        );
    }

    let outcome = produce_js_groups(producer_groups, js.recognizers(), limits, cache, observer)?;

    artifacts.extend(outcome.artifacts().iter().cloned());
    pending_failures.extend(outcome.source_failures().iter().cloned().map(|failure| {
        producer_pending_failure(
            outcome.producer_scopes().to_vec().into_boxed_slice(),
            failure,
        )
    }));
    if let Some(failure) = outcome.invocation_failure().cloned() {
        pending_failures.push(producer_pending_failure(
            outcome.producer_scopes().to_vec().into_boxed_slice(),
            failure,
        ));
    }
    Ok(())
}

fn produce_js_groups<O>(
    groups: Vec<JsPhysicalSourceGroup>,
    recognizers: &JsRecognizerSet,
    limits: &LinkLimits,
    cache: Option<&MessageLinkCache>,
    observer: &mut O,
) -> Result<intlify_producer_js::JsProducerOutcome, ReferenceInventoryError>
where
    O: MessageBenchmarkObserver + ?Sized,
{
    #[cfg(not(feature = "benchmark"))]
    let _ = &observer;

    #[cfg(feature = "benchmark")]
    if observer.enabled() {
        let execution = match cache {
            Some(cache) => {
                benchmark_produce_reference_artifacts_with_cache(groups, recognizers, limits, cache)
            }
            None => benchmark_produce_reference_artifacts(groups, recognizers, limits),
        }
        .map_err(ReferenceInventoryError::Producer)?;
        observer.exclude_observation_overhead(execution.observation_overhead());
        for measurement in execution.stages() {
            let identity = source_path_label(measurement.source());
            observer.record(
                message_stage_for_js(measurement.stage()),
                &identity,
                measurement.elapsed(),
                measurement.checksum(),
            );
        }
        return Ok(execution.into_outcome());
    }

    match cache {
        Some(cache) => produce_reference_artifacts_with_cache(groups, recognizers, limits, cache),
        None => produce_reference_artifacts(groups, recognizers, limits),
    }
    .map_err(ReferenceInventoryError::Producer)
}

#[cfg(feature = "benchmark")]
const fn message_stage_for_js(stage: BenchmarkJsStage) -> MessageBenchmarkStage {
    match stage {
        BenchmarkJsStage::SourceParse => MessageBenchmarkStage::SourceParse,
        BenchmarkJsStage::RecognizerMatchAndStaticEvaluation => {
            MessageBenchmarkStage::RecognizerMatchAndStaticEvaluation
        }
        BenchmarkJsStage::KeyAndProvenanceConstruction => {
            MessageBenchmarkStage::KeyAndProvenanceConstruction
        }
        BenchmarkJsStage::ReferenceArtifactConstruction => {
            MessageBenchmarkStage::ReferenceArtifactConstruction
        }
        BenchmarkJsStage::CacheMissProduction => MessageBenchmarkStage::CacheMissProduction,
        BenchmarkJsStage::CacheHitValidationAndAccess => {
            MessageBenchmarkStage::CacheHitValidationAndAccess
        }
    }
}

fn inventory_observation<T>(source_kind: &[u8], groups: &[&PhysicalFileGroup<T>]) -> Vec<Vec<u8>>
where
    T: ProjectFileTarget,
{
    let mut fields = Vec::new();
    for group in groups {
        for target in &group.targets {
            fields.push(b"logical-source".to_vec());
            fields.push(source_kind.to_vec());
            fields.push(target.project_path().as_bytes().to_vec());
        }
    }
    for group in groups {
        fields.push(b"physical-group".to_vec());
        fields.push(
            u64::try_from(group.targets.len())
                .expect("supported physical alias counts fit u64")
                .to_be_bytes()
                .to_vec(),
        );
        fields.extend(
            group
                .targets
                .iter()
                .map(|target| target.project_path().as_bytes().to_vec()),
        );
    }
    fields
}

fn reference_snapshot_observation(groups: &[JsPhysicalSourceGroup]) -> Vec<Vec<u8>> {
    let mut fields = Vec::new();
    for group in groups {
        fields.push(b"reference-snapshot".to_vec());
        fields.push(
            u64::try_from(group.aliases().len() + 1)
                .expect("supported physical alias counts fit u64")
                .to_be_bytes()
                .to_vec(),
        );
        fields.push(source_path_label(group.primary()).into_bytes());
        fields.extend(
            group
                .aliases()
                .iter()
                .map(|source| source_path_label(source).into_bytes()),
        );
        let (grammar, goal) = JsSourceProfile::from_source(group.primary()).map_or(
            (b"unsupported".as_slice(), b"unsupported".as_slice()),
            source_profile_observation,
        );
        fields.push(grammar.to_vec());
        fields.push(goal.to_vec());
        fields.push(group.source_bytes().to_vec());
    }
    fields
}

const fn source_profile_observation(profile: JsSourceProfile) -> (&'static [u8], &'static [u8]) {
    let grammar: &'static [u8] = match profile.grammar() {
        JsGrammarProfile::JavaScript => b"javascript",
        JsGrammarProfile::JavaScriptJsx => b"javascript-jsx",
        JsGrammarProfile::TypeScript => b"typescript",
        JsGrammarProfile::TypeScriptJsx => b"typescript-jsx",
        JsGrammarProfile::TypeScriptDeclaration => b"typescript-declaration",
    };
    let goal: &'static [u8] = match profile.goal() {
        JsSourceGoal::BoundedUnambiguous => b"bounded-unambiguous",
        JsSourceGoal::Module => b"module",
        JsSourceGoal::CommonJs => b"commonjs",
    };
    (grammar, goal)
}

fn source_identities(
    group: &PhysicalFileGroup<ReferenceTarget>,
) -> Result<Vec<SourceDocumentIdentity>, (String, ValueConstructionError)> {
    group
        .targets
        .iter()
        .map(|target| {
            portable_path(&target.relative)
                .map(|path| SourceDocumentIdentity::new(ArtifactNamespace::Project, path))
                .map_err(|error| (target.relative.clone(), error))
        })
        .collect()
}

fn producer_pending_failure(
    affected_scopes: Box<[CatalogScopeId]>,
    failure: JsProducerFailure,
) -> PendingReferenceFailure {
    PendingReferenceFailure {
        order_path: Some(source_path_label(failure.source())),
        rank: 2,
        failure: ReferenceSourceFailure::Producer {
            affected_scopes,
            failure,
        },
    }
}

fn recognizer_scopes(recognizers: &JsRecognizerSet) -> Box<[CatalogScopeId]> {
    recognizers
        .bindings()
        .iter()
        .map(|binding| binding.scope().clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .into_boxed_slice()
}

#[allow(clippy::too_many_arguments)]
fn produce_external_inventory<O>(
    project_root: &Path,
    target_scopes: &[CatalogScopeId],
    configured_paths: &[PortableRelativePath],
    limits: &LinkLimits,
    cache: Option<&MessageLinkCache>,
    artifacts: &mut Vec<MessageReferenceArtifact>,
    pending_failures: &mut Vec<PendingReferenceFailure>,
    observer: &mut O,
) where
    O: MessageBenchmarkObserver + ?Sized,
{
    observer.begin(
        MessageBenchmarkStage::InventoryMetadataIo,
        "reference-external-inventory",
    );
    let targets = configured_paths
        .iter()
        .cloned()
        .map(|relative| {
            let label = portable_path_label(&relative);
            let absolute = relative
                .segments()
                .iter()
                .fold(project_root.to_path_buf(), |path, segment| {
                    path.join(segment.as_str())
                });
            ExternalArtifactTarget {
                relative,
                label,
                absolute,
            }
        })
        .collect();
    let (groups, metadata_failures) = group_physical_files(targets);
    pending_failures.extend(metadata_failures.into_iter().map(|failure| {
        let path = failure.target.label.clone();
        PendingReferenceFailure {
            order_path: Some(path),
            rank: 3,
            failure: ReferenceSourceFailure::Host {
                participant: ReferenceParticipant::ExternalArtifact,
                affected_scopes: target_scopes.to_vec().into_boxed_slice(),
                error: failure.error,
            },
        }
    }));
    observer.finish(
        MessageBenchmarkStage::InventoryMetadataIo,
        "reference-external-inventory",
    );
    if observer.enabled() {
        let groups = groups.iter().collect::<Vec<_>>();
        let observation_fields = inventory_observation(b"reference-external", &groups);
        observer.observe(
            MessageBenchmarkStage::InventoryMetadataIo,
            "reference-external-inventory",
            observation_checksum(
                MessageBenchmarkStage::InventoryMetadataIo.cost().as_bytes(),
                observation_fields.iter().map(Vec::as_slice),
            ),
        );
    }

    let mut logical_paths = observer
        .enabled()
        .then(|| {
            groups
                .iter()
                .map(|group| {
                    group
                        .targets
                        .iter()
                        .map(|target| target.label.clone())
                        .collect::<Vec<_>>()
                        .into_boxed_slice()
                })
                .collect::<Vec<_>>()
        })
        .map(Vec::into_iter);
    observer.begin(
        MessageBenchmarkStage::ExternalArtifactRead,
        "external-artifact-snapshots",
    );
    let mut snapshots = Vec::new();
    for group in groups {
        let primary = group.primary().relative.clone();
        let primary_label = group.primary_path().to_owned();
        let observed_paths = logical_paths.as_mut().and_then(Iterator::next);
        let snapshot = match acquire_physical_snapshot(
            &group,
            limits.effective_limit(LinkLimitCounter::ReferenceArtifactWireBytes),
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                pending_failures.push(PendingReferenceFailure {
                    order_path: Some(primary_label),
                    rank: 3,
                    failure: ReferenceSourceFailure::Host {
                        participant: ReferenceParticipant::ExternalArtifact,
                        affected_scopes: target_scopes.to_vec().into_boxed_slice(),
                        error,
                    },
                });
                continue;
            }
        };
        snapshots.push(ExternalArtifactSnapshot {
            primary,
            primary_label,
            logical_paths: observed_paths,
            bytes: snapshot,
        });
    }
    observer.finish(
        MessageBenchmarkStage::ExternalArtifactRead,
        "external-artifact-snapshots",
    );
    if observer.enabled() {
        let observation_fields = external_snapshot_observation(&snapshots);
        observer.observe(
            MessageBenchmarkStage::ExternalArtifactRead,
            "external-artifact-snapshots",
            observation_checksum(
                MessageBenchmarkStage::ExternalArtifactRead
                    .cost()
                    .as_bytes(),
                observation_fields.iter().map(Vec::as_slice),
            ),
        );
    }

    for snapshot in snapshots {
        let cache_key =
            ExternalArtifactCacheKey::for_snapshot(snapshot.primary.clone(), &snapshot.bytes);
        let cached = cache
            // Typed revalidation cannot recover the original wire charge. The
            // authoritative current snapshot must independently pass it before
            // a cached decoded value can participate.
            .filter(|_| {
                cache_key.byte_length
                    <= limits.effective_limit(LinkLimitCounter::ReferenceArtifactWireBytes)
            })
            .and_then(|cache| cache.load_external(&cache_key, limits));
        if let Some(artifact) = cached {
            artifacts.push(artifact);
            continue;
        }

        observer.begin(
            MessageBenchmarkStage::ReferenceArtifactDecode,
            &snapshot.primary_label,
        );
        let decoded = decode_reference_artifact(&snapshot.bytes, limits);
        observer.finish(
            MessageBenchmarkStage::ReferenceArtifactDecode,
            &snapshot.primary_label,
        );
        if let Ok(artifact) = &decoded {
            if observer.enabled() {
                match encode_reference_artifact(artifact, limits) {
                    Ok(bytes) => observer.observe(
                        MessageBenchmarkStage::ReferenceArtifactDecode,
                        &snapshot.primary_label,
                        observation_checksum(
                            MessageBenchmarkStage::ReferenceArtifactDecode
                                .cost()
                                .as_bytes(),
                            [bytes.as_ref()],
                        ),
                    ),
                    Err(_) => observer.invalidate(),
                }
            }
        }
        match decoded {
            Ok(artifact) => {
                if let Some(cache) = cache {
                    cache.store_external(cache_key, artifact.clone());
                }
                artifacts.push(artifact);
            }
            Err(error) => pending_failures.push(PendingReferenceFailure {
                order_path: Some(snapshot.primary_label),
                rank: 4,
                failure: ReferenceSourceFailure::ExternalArtifact {
                    affected_scopes: target_scopes.to_vec().into_boxed_slice(),
                    host_primary: snapshot.primary,
                    error,
                },
            }),
        }
    }
}

fn external_snapshot_observation(snapshots: &[ExternalArtifactSnapshot]) -> Vec<Vec<u8>> {
    let mut fields = Vec::new();
    for snapshot in snapshots {
        fields.push(b"external-artifact-snapshot".to_vec());
        fields.push(
            u64::try_from(
                snapshot
                    .logical_paths
                    .as_ref()
                    .expect("enabled observations retain external logical paths")
                    .len(),
            )
            .expect("supported physical alias counts fit u64")
            .to_be_bytes()
            .to_vec(),
        );
        fields.extend(
            snapshot
                .logical_paths
                .as_ref()
                .expect("enabled observations retain external logical paths")
                .iter()
                .map(|path| path.as_bytes().to_vec()),
        );
        fields.push(snapshot.bytes.to_vec());
    }
    fields
}

fn portable_path_label(path: &PortableRelativePath) -> String {
    path.segments()
        .iter()
        .map(intlify_contract::PortablePathSegment::as_str)
        .collect::<Vec<_>>()
        .join("/")
}

fn source_path_label(source: &SourceDocumentIdentity) -> String {
    portable_path_label(source.path())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::ops::Deref;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use intlify_contract::{
        ArtifactNamespace, CatalogScopeId, CatalogScopeName, LinkLimitCounter, LinkLimits,
        ReferenceArtifactSegment,
    };
    use intlify_resource::ResourcesConfig;
    use serde_json::{json, Value};

    use super::{
        produce_reference_inventory, MessageLinkCache, ReferenceParticipant, ReferenceSourceFailure,
    };
    use crate::messages::validate_messages_config;

    const ZERO_REFERENCES: &[u8] =
        include_bytes!("../../../intlify_contract/fixtures/reference/v0.1/zero-references.json");

    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new(name: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be after the epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "intlify-message-reference-{name}-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("temporary project root should be created");
            Self(path)
        }
    }

    impl Deref for TempRoot {
        type Target = Path;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write(root: &Path, relative: &str, bytes: impl AsRef<[u8]>) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().expect("fixture parent"))
            .expect("fixture parent should be created");
        fs::write(path, bytes).expect("fixture should be written");
    }

    fn app_scope() -> CatalogScopeId {
        CatalogScopeId::new(
            ArtifactNamespace::Project,
            CatalogScopeName::try_new("app").unwrap(),
        )
    }

    fn producers(messages: &Value) -> crate::messages::ResolvedMessageProducers {
        let resources = ResourcesConfig::validate(Some(&json!({
            "catalogs": [{
                "scope": "app",
                "include": ["locales/*.json"],
                "locale": { "from": "fixed", "value": "en" }
            }]
        })))
        .unwrap()
        .resolve();
        validate_messages_config(Some(messages), &resources)
            .unwrap()
            .unwrap()
            .1
            .producers()
            .clone()
    }

    fn js_messages(include: &[&str]) -> Value {
        json!({
            "locales": ["en"],
            "producers": {
                "js": {
                    "include": include,
                    "recognizers": {
                        "t": {
                            "kind": "lookup",
                            "scope": "app",
                            "domain": "json-pointer",
                            "keySyntax": "literal"
                        }
                    }
                }
            }
        })
    }

    fn external_messages(paths: &[&str]) -> Value {
        json!({
            "locales": ["en"],
            "producers": { "artifacts": paths }
        })
    }

    fn artifact_identity(artifact: &intlify_contract::MessageReferenceArtifact) -> Vec<&str> {
        artifact
            .identity()
            .segments()
            .iter()
            .map(ReferenceArtifactSegment::as_str)
            .collect()
    }

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn reference_inventory_and_cache_values_are_thread_transferable() {
        assert_send_sync::<super::MessageLinkCache>();
        assert_send_sync::<super::ReferenceInventory>();
        assert_send_sync::<super::ReferenceInventoryError>();
        assert_send_sync::<super::ReferenceSourceFailure>();
    }

    #[test]
    fn omitted_producers_and_zero_match_js_are_successful_empty_inventories() {
        let root = TempRoot::new("empty");
        let limits = LinkLimits::default();
        let scopes = [app_scope()];

        let omitted = produce_reference_inventory(
            &root,
            &scopes,
            &producers(&json!({ "locales": ["en"] })),
            &limits,
            None,
        )
        .unwrap();
        let zero_match = produce_reference_inventory(
            &root,
            &scopes,
            &producers(&js_messages(&["src/**/*.ts"])),
            &limits,
            None,
        )
        .unwrap();

        assert!(omitted.artifacts().is_empty());
        assert!(omitted.failures().is_empty());
        assert!(zero_match.artifacts().is_empty());
        assert!(zero_match.failures().is_empty());
    }

    #[test]
    fn js_inventory_has_no_hidden_or_dependency_default_filters() {
        let root = TempRoot::new("complete-js");
        write(&root, ".hidden/entry.ts", "t('hidden')");
        write(&root, "node_modules/pkg/entry.ts", "t('dependency')");
        let inventory = produce_reference_inventory(
            &root,
            &[app_scope()],
            &producers(&js_messages(&["**/*.ts"])),
            &LinkLimits::default(),
            None,
        )
        .unwrap();

        assert!(inventory.failures().is_empty());
        assert_eq!(inventory.artifacts().len(), 2);
        assert_eq!(
            inventory
                .artifacts()
                .iter()
                .map(artifact_identity)
                .collect::<Vec<_>>(),
            [
                vec!["js", "module", ".hidden", "entry.ts"],
                vec!["js", "module", "node_modules", "pkg", "entry.ts"],
            ]
        );
    }

    #[test]
    fn js_source_failure_is_retained_for_every_recognizer_scope() {
        let root = TempRoot::new("js-failure");
        write(&root, "src/broken.ts", "const =");
        let inventory = produce_reference_inventory(
            &root,
            &[app_scope()],
            &producers(&js_messages(&["src/**/*.ts"])),
            &LinkLimits::default(),
            None,
        )
        .unwrap();

        assert!(inventory.artifacts().is_empty());
        assert_eq!(inventory.failures().len(), 1);
        assert!(matches!(
            &inventory.failures()[0],
            ReferenceSourceFailure::Producer {
                affected_scopes,
                ..
            } if affected_scopes.as_ref() == [app_scope()]
        ));
    }

    #[test]
    fn js_physical_aliases_are_scanned_once_and_cache_hits_are_equal() {
        let root = TempRoot::new("js-alias-cache");
        write(&root, "src/z.ts", "t('checkout.title')");
        fs::hard_link(root.join("src/z.ts"), root.join("src/a.ts"))
            .expect("hard-link alias should be created");
        let producers = producers(&js_messages(&["src/**/*.ts"]));
        let limits = LinkLimits::default();
        let cache = MessageLinkCache::default();

        let miss =
            produce_reference_inventory(&root, &[app_scope()], &producers, &limits, Some(&cache))
                .unwrap();
        let hit =
            produce_reference_inventory(&root, &[app_scope()], &producers, &limits, Some(&cache))
                .unwrap();

        assert_eq!(miss.artifacts(), hit.artifacts());
        assert_eq!(miss.failures(), hit.failures());
        assert_eq!(miss.artifacts().len(), 1);
        assert_eq!(
            artifact_identity(&miss.artifacts()[0]),
            ["js", "module", "src", "a.ts"]
        );
        let stats = cache.stats();
        assert!(stats.js_loads > stats.js_hits);
        assert!(stats.js_hits > 0);
    }

    #[test]
    fn external_aliases_decode_once_and_alias_only_changes_reuse_cache() {
        let root = TempRoot::new("external-alias-cache");
        write(&root, "refs/z.json", ZERO_REFERENCES);
        fs::hard_link(root.join("refs/z.json"), root.join("refs/a.json"))
            .expect("hard-link alias should be created");
        let limits = LinkLimits::default();
        let cache = MessageLinkCache::default();

        let aliased = produce_reference_inventory(
            &root,
            &[app_scope()],
            &producers(&external_messages(&["refs/z.json", "refs/a.json"])),
            &limits,
            Some(&cache),
        )
        .unwrap();
        let same_primary = produce_reference_inventory(
            &root,
            &[app_scope()],
            &producers(&external_messages(&["refs/a.json"])),
            &limits,
            Some(&cache),
        )
        .unwrap();
        let changed_primary = produce_reference_inventory(
            &root,
            &[app_scope()],
            &producers(&external_messages(&["refs/z.json"])),
            &limits,
            Some(&cache),
        )
        .unwrap();
        let compatible_limits = limits
            .try_with_limit(LinkLimitCounter::ReferenceRecords, 0)
            .unwrap();
        let compatible_hit = produce_reference_inventory(
            &root,
            &[app_scope()],
            &producers(&external_messages(&["refs/z.json"])),
            &compatible_limits,
            Some(&cache),
        )
        .unwrap();

        assert_eq!(aliased.artifacts(), same_primary.artifacts());
        assert_eq!(same_primary.artifacts(), changed_primary.artifacts());
        assert_eq!(changed_primary.artifacts(), compatible_hit.artifacts());
        assert_eq!(aliased.artifacts().len(), 1);
        let stats = cache.stats();
        assert_eq!(stats.external_loads, 4);
        assert_eq!(stats.external_hits, 2);
    }

    #[test]
    fn external_cache_never_bypasses_the_current_wire_byte_limit() {
        let root = TempRoot::new("external-wire-limit");
        write(&root, "refs/artifact.json", ZERO_REFERENCES);
        let producers = producers(&external_messages(&["refs/artifact.json"]));
        let cache = MessageLinkCache::default();
        let admitted = produce_reference_inventory(
            &root,
            &[app_scope()],
            &producers,
            &LinkLimits::default(),
            Some(&cache),
        )
        .unwrap();
        assert_eq!(admitted.artifacts().len(), 1);

        let strict_wire_limit = LinkLimits::default()
            .try_with_limit(
                LinkLimitCounter::ReferenceArtifactWireBytes,
                u64::try_from(ZERO_REFERENCES.len()).unwrap() - 1,
            )
            .unwrap();
        let rejected = produce_reference_inventory(
            &root,
            &[app_scope()],
            &producers,
            &strict_wire_limit,
            Some(&cache),
        )
        .unwrap();

        assert!(rejected.artifacts().is_empty());
        assert_eq!(rejected.failures().len(), 1);
        assert!(matches!(
            &rejected.failures()[0],
            ReferenceSourceFailure::ExternalArtifact { .. }
        ));
        assert_eq!(cache.stats().external_hits, 0);
    }

    #[test]
    fn external_metadata_and_decode_failures_affect_every_target_scope() {
        let root = TempRoot::new("external-failures");
        write(&root, "refs/invalid.json", "{");
        let vendor_scope = CatalogScopeId::new(
            ArtifactNamespace::Project,
            CatalogScopeName::try_new("vendor").unwrap(),
        );
        let scopes = [app_scope(), vendor_scope];
        let inventory = produce_reference_inventory(
            &root,
            &scopes,
            &producers(&external_messages(&[
                "refs/invalid.json",
                "refs/missing.json",
            ])),
            &LinkLimits::default(),
            None,
        )
        .unwrap();

        assert!(inventory.artifacts().is_empty());
        assert_eq!(inventory.failures().len(), 2);
        for failure in inventory.failures() {
            assert_eq!(failure.affected_scopes(), scopes);
        }
        assert!(inventory.failures().iter().any(|failure| matches!(
            failure,
            ReferenceSourceFailure::Host {
                participant: ReferenceParticipant::ExternalArtifact,
                ..
            }
        )));
        assert!(inventory
            .failures()
            .iter()
            .any(|failure| matches!(failure, ReferenceSourceFailure::ExternalArtifact { .. })));
    }
}
