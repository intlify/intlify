// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use intlify_contract::{
    encode_reference_artifact, ArtifactNamespace, CatalogKeyDomain, CatalogScopeId,
    CatalogScopeName, DeliveryUnitSegment, LinkLimitCounter, LinkLimits, PortablePathSegment,
    PortableRelativePath, ReferenceArtifactSegment, SourceDocumentIdentity,
};
#[cfg(feature = "benchmark")]
use intlify_producer_js::benchmark::{
    benchmark_produce_reference_artifacts, benchmark_produce_reference_artifacts_with_cache,
    BenchmarkJsStage,
};
use intlify_producer_js::{
    preflight_source_group_count, produce_reference_artifacts,
    produce_reference_artifacts_with_cache, JsKeySyntax, JsPhysicalSourceGroup, JsProducerCache,
    JsProducerCacheIoError, JsProducerCacheKey, JsProducerFailureReason, JsProducerOutcome,
    JsRecognizerBinding, JsRecognizerCallKind, JsRecognizerSet, SOURCE_GROUPS_LIMIT,
};

fn source(path: &[&str]) -> SourceDocumentIdentity {
    SourceDocumentIdentity::new(
        ArtifactNamespace::Project,
        PortableRelativePath::try_new(
            path.iter()
                .map(|segment| PortablePathSegment::try_new(*segment).unwrap())
                .collect(),
        )
        .unwrap(),
    )
}

fn group(paths: &[&[&str]], source_bytes: &[u8]) -> JsPhysicalSourceGroup {
    JsPhysicalSourceGroup::try_new(
        paths.iter().map(|path| source(path)).collect(),
        source_bytes.to_vec(),
    )
    .unwrap()
}

fn app_scope() -> CatalogScopeId {
    CatalogScopeId::new(
        ArtifactNamespace::Project,
        CatalogScopeName::try_new("app").unwrap(),
    )
}

fn lookup(callee: &str) -> JsRecognizerSet {
    JsRecognizerSet::try_new(vec![JsRecognizerBinding::try_new(
        callee,
        JsRecognizerCallKind::Lookup,
        app_scope(),
        CatalogKeyDomain::JsonPointer,
        JsKeySyntax::Literal,
    )
    .unwrap()])
    .unwrap()
}

fn encoded_artifacts(outcome: &JsProducerOutcome, limits: &LinkLimits) -> Vec<Box<[u8]>> {
    outcome
        .artifacts()
        .iter()
        .map(|artifact| encode_reference_artifact(artifact, limits).unwrap())
        .collect()
}

#[derive(Default)]
struct MemoryCache {
    entries: Mutex<HashMap<JsProducerCacheKey, Box<[u8]>>>,
    loads: AtomicUsize,
    stores: AtomicUsize,
}

impl MemoryCache {
    fn load_count(&self) -> usize {
        self.loads.load(Ordering::SeqCst)
    }

    fn store_count(&self) -> usize {
        self.stores.load(Ordering::SeqCst)
    }

    fn corrupt_entries(&self) {
        for entry in self.entries.lock().unwrap().values_mut() {
            if let Some(byte) = entry.last_mut() {
                *byte ^= 1;
            }
        }
    }
}

impl JsProducerCache for MemoryCache {
    fn load(&self, key: &JsProducerCacheKey) -> Result<Option<Box<[u8]>>, JsProducerCacheIoError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(self.entries.lock().unwrap().get(key).cloned())
    }

    fn store(&self, key: &JsProducerCacheKey, value: &[u8]) -> Result<(), JsProducerCacheIoError> {
        self.stores.fetch_add(1, Ordering::SeqCst);
        self.entries
            .lock()
            .unwrap()
            .insert(key.clone(), value.into());
        Ok(())
    }
}

struct FailingCache;

impl JsProducerCache for FailingCache {
    fn load(&self, _key: &JsProducerCacheKey) -> Result<Option<Box<[u8]>>, JsProducerCacheIoError> {
        Err(io::Error::other("cache read unavailable").into())
    }

    fn store(
        &self,
        _key: &JsProducerCacheKey,
        _value: &[u8],
    ) -> Result<(), JsProducerCacheIoError> {
        Err(io::Error::other("cache write unavailable").into())
    }
}

#[cfg(feature = "benchmark")]
#[test]
fn benchmark_observes_the_ordinary_producer_pipeline_without_changing_its_result() {
    let limits = LinkLimits::default();
    let recognizers = lookup("t");
    let selected = || {
        vec![group(
            &[&["src", "app.mts"]],
            b"export const title = t('checkout.title')",
        )]
    };

    let ordinary = produce_reference_artifacts(selected(), &recognizers, &limits).unwrap();
    let benchmark =
        benchmark_produce_reference_artifacts(selected(), &recognizers, &limits).unwrap();
    let repeated =
        benchmark_produce_reference_artifacts(selected(), &recognizers, &limits).unwrap();

    assert_eq!(benchmark.outcome(), &ordinary);
    assert_eq!(repeated.outcome(), &ordinary);
    assert_eq!(
        benchmark
            .stages()
            .iter()
            .map(intlify_producer_js::benchmark::BenchmarkJsStageMeasurement::stage)
            .collect::<Vec<_>>(),
        [
            BenchmarkJsStage::SourceParse,
            BenchmarkJsStage::RecognizerMatchAndStaticEvaluation,
            BenchmarkJsStage::KeyAndProvenanceConstruction,
            BenchmarkJsStage::ReferenceArtifactConstruction,
        ]
    );
    assert!(benchmark.stages().iter().all(|measurement| {
        measurement.source() == &source(&["src", "app.mts"])
            && !measurement.stage().boundary_id().contains("_v")
    }));
    assert_eq!(
        benchmark
            .stages()
            .iter()
            .map(|measurement| {
                (
                    measurement.source(),
                    measurement.stage(),
                    measurement.checksum(),
                )
            })
            .collect::<Vec<_>>(),
        repeated
            .stages()
            .iter()
            .map(|measurement| {
                (
                    measurement.source(),
                    measurement.stage(),
                    measurement.checksum(),
                )
            })
            .collect::<Vec<_>>()
    );
}

#[cfg(feature = "benchmark")]
#[test]
fn benchmark_distinguishes_cache_miss_production_from_cache_hit_access() {
    let limits = LinkLimits::default();
    let recognizers = lookup("t");
    let selected = || {
        vec![group(
            &[&["src", "app.mts"]],
            b"export const title = t('checkout.title')",
        )]
    };
    let cache = MemoryCache::default();

    let miss =
        benchmark_produce_reference_artifacts_with_cache(selected(), &recognizers, &limits, &cache)
            .unwrap();
    let hit =
        benchmark_produce_reference_artifacts_with_cache(selected(), &recognizers, &limits, &cache)
            .unwrap();

    assert_eq!(miss.outcome(), hit.outcome());
    assert_eq!(
        miss.stages()
            .iter()
            .map(intlify_producer_js::benchmark::BenchmarkJsStageMeasurement::stage)
            .collect::<Vec<_>>(),
        [
            BenchmarkJsStage::SourceParse,
            BenchmarkJsStage::RecognizerMatchAndStaticEvaluation,
            BenchmarkJsStage::KeyAndProvenanceConstruction,
            BenchmarkJsStage::ReferenceArtifactConstruction,
            BenchmarkJsStage::CacheMissProduction,
        ]
    );
    assert_eq!(
        hit.stages()
            .iter()
            .map(intlify_producer_js::benchmark::BenchmarkJsStageMeasurement::stage)
            .collect::<Vec<_>>(),
        [BenchmarkJsStage::CacheHitValidationAndAccess]
    );
    assert!(miss
        .stages()
        .iter()
        .chain(hit.stages())
        .all(|measurement| !measurement.stage().boundary_id().contains("_v")));
}

#[test]
fn physical_alias_group_emits_one_primary_owned_artifact() {
    let outcome = produce_reference_artifacts(
        vec![group(
            &[&["src", "linked.ts"], &["src", "app.ts"]],
            b"t('checkout.title')",
        )],
        &lookup("t"),
        &LinkLimits::default(),
    )
    .unwrap();

    assert!(outcome.is_complete());
    assert_eq!(outcome.artifacts().len(), 1);
    let artifact = &outcome.artifacts()[0];
    assert_eq!(
        artifact
            .identity()
            .segments()
            .iter()
            .map(ReferenceArtifactSegment::as_str)
            .collect::<Vec<_>>(),
        ["js", "module", "src", "app.ts"]
    );
    assert_eq!(
        artifact
            .delivery_unit()
            .segments()
            .iter()
            .map(DeliveryUnitSegment::as_str)
            .collect::<Vec<_>>(),
        ["main"]
    );
    assert_eq!(artifact.references().len(), 1);
    assert_eq!(
        artifact.references()[0].origin().unwrap().source(),
        &source(&["src", "app.ts"])
    );
    assert_eq!(outcome.producer_scopes(), &[app_scope()]);
}

#[test]
fn zero_reference_cache_miss_hit_and_disabled_paths_are_identical() {
    let limits = LinkLimits::default();
    let recognizers = lookup("t");
    let selected = || vec![group(&[&["src", "empty.ts"]], b"export const answer = 42")];
    let cache = MemoryCache::default();

    let disabled = produce_reference_artifacts(selected(), &recognizers, &limits).unwrap();
    let miss =
        produce_reference_artifacts_with_cache(selected(), &recognizers, &limits, &cache).unwrap();
    let hit =
        produce_reference_artifacts_with_cache(selected(), &recognizers, &limits, &cache).unwrap();

    assert!(hit.artifacts()[0].references().is_empty());
    assert_eq!(disabled, miss);
    assert_eq!(miss, hit);
    assert_eq!(
        encoded_artifacts(&disabled, &limits),
        encoded_artifacts(&hit, &limits)
    );
    assert_eq!(cache.store_count(), 1);
    // A bounded-unambiguous miss probes module and script identities; the
    // subsequent hit selects the authoritative module identity immediately.
    assert_eq!(cache.load_count(), 3);
}

#[test]
fn reference_cache_miss_hit_and_disabled_paths_are_identical() {
    let limits = LinkLimits::default();
    let recognizers = lookup("t");
    let selected = || vec![group(&[&["src", "app.ts"]], b"t('checkout.title')")];
    let cache = MemoryCache::default();

    let disabled = produce_reference_artifacts(selected(), &recognizers, &limits).unwrap();
    let miss =
        produce_reference_artifacts_with_cache(selected(), &recognizers, &limits, &cache).unwrap();
    let hit =
        produce_reference_artifacts_with_cache(selected(), &recognizers, &limits, &cache).unwrap();

    assert_eq!(disabled.artifacts()[0].references().len(), 1);
    assert_eq!(disabled, miss);
    assert_eq!(miss, hit);
    assert_eq!(
        encoded_artifacts(&disabled, &limits),
        encoded_artifacts(&hit, &limits)
    );
    assert_eq!(cache.store_count(), 1);
    assert_eq!(cache.load_count(), 3);
}

#[test]
fn bounded_script_goal_uses_its_distinct_cache_identity() {
    let limits = LinkLimits::default();
    let recognizers = lookup("t");
    let selected = || vec![group(&[&["src", "legacy.js"]], b"var await = t('legacy')")];
    let cache = MemoryCache::default();

    let miss =
        produce_reference_artifacts_with_cache(selected(), &recognizers, &limits, &cache).unwrap();
    let hit =
        produce_reference_artifacts_with_cache(selected(), &recognizers, &limits, &cache).unwrap();

    assert_eq!(miss, hit);
    assert_eq!(cache.store_count(), 1);
    // Both runs probe module first; the miss then produces script and the hit
    // selects the separately keyed script artifact.
    assert_eq!(cache.load_count(), 4);
}

#[test]
fn corrupt_cache_entry_is_a_miss_and_never_a_partial_artifact() {
    let limits = LinkLimits::default();
    let recognizers = lookup("t");
    let selected = || vec![group(&[&["src", "app.ts"]], b"t('a')")];
    let cache = MemoryCache::default();

    let first =
        produce_reference_artifacts_with_cache(selected(), &recognizers, &limits, &cache).unwrap();
    cache.corrupt_entries();
    let regenerated =
        produce_reference_artifacts_with_cache(selected(), &recognizers, &limits, &cache).unwrap();

    assert_eq!(first, regenerated);
    assert_eq!(regenerated.artifacts().len(), 1);
    assert!(regenerated.source_failures().is_empty());
    assert_eq!(cache.store_count(), 2);
}

#[test]
fn cache_io_failures_do_not_discard_authoritative_success() {
    let outcome = produce_reference_artifacts_with_cache(
        vec![group(&[&["src", "app.ts"]], b"t('a')")],
        &lookup("t"),
        &LinkLimits::default(),
        &FailingCache,
    )
    .unwrap();

    assert!(outcome.is_complete());
    assert_eq!(outcome.artifacts().len(), 1);
}

#[test]
fn cache_hit_is_revalidated_under_current_lower_limits() {
    let cache = MemoryCache::default();
    let recognizers = lookup("t");
    let selected = || vec![group(&[&["src", "app.ts"]], b"t('a')")];
    produce_reference_artifacts_with_cache(
        selected(),
        &recognizers,
        &LinkLimits::default(),
        &cache,
    )
    .unwrap();

    let lower = LinkLimits::default()
        .try_with_limit(LinkLimitCounter::ReferenceRecords, 0)
        .unwrap();
    let outcome =
        produce_reference_artifacts_with_cache(selected(), &recognizers, &lower, &cache).unwrap();

    assert!(outcome.artifacts().is_empty());
    assert_eq!(outcome.source_failures().len(), 1);
    assert_eq!(
        outcome.source_failures()[0].reason(),
        JsProducerFailureReason::OutputLimit
    );
    assert_eq!(outcome.source_failures()[0].limit(), Some(0));
    assert_eq!(outcome.source_failures()[0].observed(), Some(1));
    assert_eq!(cache.store_count(), 1);
}

#[test]
fn source_or_recognizer_changes_do_not_reuse_stale_semantics() {
    let cache = MemoryCache::default();
    let limits = LinkLimits::default();
    let source_bytes = b"t('a'); translate('b')";

    let first = produce_reference_artifacts_with_cache(
        vec![group(&[&["src", "app.ts"]], source_bytes)],
        &lookup("t"),
        &limits,
        &cache,
    )
    .unwrap();
    let changed_recognizer = produce_reference_artifacts_with_cache(
        vec![group(&[&["src", "app.ts"]], source_bytes)],
        &lookup("translate"),
        &limits,
        &cache,
    )
    .unwrap();
    let invalid_source = produce_reference_artifacts_with_cache(
        vec![group(&[&["src", "app.ts"]], b"t(")],
        &lookup("t"),
        &limits,
        &cache,
    )
    .unwrap();

    assert_ne!(first.artifacts(), changed_recognizer.artifacts());
    assert_eq!(cache.store_count(), 2);
    assert!(invalid_source.artifacts().is_empty());
    assert_eq!(
        invalid_source.source_failures()[0].reason(),
        JsProducerFailureReason::SyntaxInvalid
    );
}

#[test]
fn conflicting_alias_profiles_fail_only_the_group() {
    let outcome = produce_reference_artifacts(
        vec![
            group(&[&["src", "app.ts"], &["src", "linked.js"]], b"t('a')"),
            group(&[&["src", "ok.ts"]], b"t('b')"),
        ],
        &lookup("t"),
        &LinkLimits::default(),
    )
    .unwrap();

    assert_eq!(outcome.artifacts().len(), 1);
    assert_eq!(outcome.source_failures().len(), 1);
    assert_eq!(
        outcome.source_failures()[0].reason(),
        JsProducerFailureReason::ConflictingAliasProfile
    );
    assert!(outcome.invocation_failure().is_none());
}

#[test]
fn artifact_identity_limits_fail_without_path_hashing_or_truncation() {
    let oversized_segment = "a".repeat(256);
    let outcome = produce_reference_artifacts(
        vec![group(
            &[&["src", oversized_segment.as_str(), "app.ts"]],
            b"t('a')",
        )],
        &lookup("t"),
        &LinkLimits::default(),
    )
    .unwrap();

    assert!(outcome.artifacts().is_empty());
    assert_eq!(
        outcome.source_failures()[0].reason(),
        JsProducerFailureReason::ArtifactIdentityLimit
    );
    assert_eq!(outcome.source_failures()[0].limit(), Some(255));
    assert_eq!(outcome.source_failures()[0].observed(), Some(256));
}

#[test]
fn source_group_failure_selection_follows_identity_profile_snapshot_order() {
    let oversized_segment = "a".repeat(256);
    let identity_failure = produce_reference_artifacts(
        vec![group(
            &[&["src", oversized_segment.as_str(), "app.unsupported"]],
            &[0xff],
        )],
        &lookup("t"),
        &LinkLimits::default(),
    )
    .unwrap();
    assert_eq!(
        identity_failure.source_failures()[0].reason(),
        JsProducerFailureReason::ArtifactIdentityLimit
    );

    let profile_failure = produce_reference_artifacts(
        vec![group(&[&["src", "app.unsupported"]], &[0xff])],
        &lookup("t"),
        &LinkLimits::default(),
    )
    .unwrap();
    assert_eq!(
        profile_failure.source_failures()[0].reason(),
        JsProducerFailureReason::UnsupportedSourceSuffix
    );

    let snapshot_failure = produce_reference_artifacts(
        vec![group(&[&["src", "app.ts"]], &[0xff])],
        &lookup("t"),
        &LinkLimits::default(),
    )
    .unwrap();
    assert_eq!(
        snapshot_failure.source_failures()[0].reason(),
        JsProducerFailureReason::InvalidUtf8
    );
}

#[test]
fn input_and_completion_order_cannot_change_artifact_order() {
    let limits = LinkLimits::default();
    let recognizers = lookup("t");
    let forward = produce_reference_artifacts(
        vec![
            group(&[&["src", "a.ts"]], b"t('a')"),
            group(&[&["src", "z.ts"]], b"t('z')"),
        ],
        &recognizers,
        &limits,
    )
    .unwrap();
    let reversed = produce_reference_artifacts(
        vec![
            group(&[&["src", "z.ts"]], b"t('z')"),
            group(&[&["src", "a.ts"]], b"t('a')"),
        ],
        &recognizers,
        &limits,
    )
    .unwrap();

    assert_eq!(forward, reversed);
    assert_eq!(
        forward
            .artifacts()
            .iter()
            .map(|artifact| artifact.identity().segments()[3].as_str())
            .collect::<Vec<_>>(),
        ["a.ts", "z.ts"]
    );
}

#[test]
fn source_group_ceiling_discards_all_artifacts_and_cache_publication() {
    let cache = MemoryCache::default();
    let groups = (0..=SOURCE_GROUPS_LIMIT)
        .map(|ordinal| {
            let name = format!("{ordinal:05}.ts");
            group(&[&["src", name.as_str()]], b"")
        })
        .collect::<Vec<_>>();
    let preflight_failure =
        preflight_source_group_count(groups.iter().map(|group| group.primary().clone()).collect())
            .unwrap_err();
    assert_eq!(
        preflight_failure.reason(),
        JsProducerFailureReason::SourceCountLimit
    );
    assert_eq!(preflight_failure.limit(), Some(SOURCE_GROUPS_LIMIT));
    assert_eq!(preflight_failure.observed(), Some(SOURCE_GROUPS_LIMIT + 1));

    let outcome = produce_reference_artifacts_with_cache(
        groups,
        &lookup("t"),
        &LinkLimits::default(),
        &cache,
    )
    .unwrap();

    assert!(outcome.artifacts().is_empty());
    let failure = outcome.invocation_failure().unwrap();
    assert_eq!(failure.reason(), JsProducerFailureReason::SourceCountLimit);
    assert_eq!(failure.limit(), Some(SOURCE_GROUPS_LIMIT));
    assert_eq!(failure.observed(), Some(SOURCE_GROUPS_LIMIT + 1));
    assert_eq!(cache.load_count(), 0);
    assert_eq!(cache.store_count(), 0);
    assert_eq!(outcome.producer_scopes(), &[app_scope()]);
}

#[test]
fn producer_values_are_send_sync_and_reentrant_with_shared_cache() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<JsPhysicalSourceGroup>();
    assert_send_sync::<JsProducerCacheKey>();
    assert_send_sync::<JsProducerOutcome>();

    let cache = Arc::new(MemoryCache::default());
    let handles = (0..4)
        .map(|_| {
            let cache = Arc::clone(&cache);
            std::thread::spawn(move || {
                produce_reference_artifacts_with_cache(
                    vec![group(&[&["src", "app.ts"]], b"t('a')")],
                    &lookup("t"),
                    &LinkLimits::default(),
                    cache.as_ref(),
                )
                .unwrap()
            })
        })
        .collect::<Vec<_>>();
    let outcomes = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();

    assert!(outcomes.windows(2).all(|pair| pair[0] == pair[1]));
}
