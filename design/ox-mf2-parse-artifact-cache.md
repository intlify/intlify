# ox-mf2 Parse Artifact Cache (Design Note)

## Purpose

Phase 1 ships a parser core that parses one caller-owned `SourceStore` entry identified by `SourceId` into an owned syntax-only `ParseResult` (CST tables + diagnostics). Dictionary-shaped tooling — i18n checkers, LSPs, batch linters, codegen — works at a different granularity: many `(locale, message_id, source_text)` entries, frequently revisited as files change.

The `ox-content` MF2 checker (see `refers/ox-content/crates/ox_content_i18n/src/checker.rs`) re-parses the same dictionary value across `check_type_mismatch`, `check_syntax_errors`, and `check_all`, multiplied across locale/key loops. Anti-pattern reference captured in `.reviews/005-ox-mf2-ox-content-mf2-parser-optimization-notes.md`, Point 5.

This document is the **bridge** between Phase 1's parser API and the Phase 2 dictionary tooling layer: it pins the shape of a per-message parse artifact cache so the parser core does not have to learn about dictionaries, and so the dictionary layer does not have to keep re-parsing.

This is a design note, not a Phase 1 deliverable.

Phase 3C's initial linter remains source-backed: `lintMessage(source, options)` parses and validates the supplied source text directly. Cache-backed or snapshot-backed linting is a future optimization for dictionary tooling, LSP/editor adapters, and batch workflows after the parser owns a snapshot-to-`SemanticModel` path. This cache note therefore describes a layered reuse strategy, not an initial linter API requirement.

## Non-goals

- The parser core stays oblivious of dictionaries, locales, and keys. The cache lives in a higher layer (`ox_mf2_dict`, `ox_mf2_lsp`, an i18n checker, etc.).
- This note does not introduce a public Phase 1 cache API. Phase 1 already exposes the primitives — `parse_source_session`, `parse_batch`, and `ParseWorkspace` — that the cache builds on.
- The cache does not create worker threads, select a concurrency width, or own an async runtime. It is safe to use from a caller-owned parallel or async execution environment, but scheduling remains a consumer responsibility.

## Cache key

```rust
struct CacheKey {
    source_id_namespace: SourceIdNamespace,
    message_id: MessageId,
    parser_version: ParserVersion,
    parse_options: ParseOptionsKey,
    source: SourceFingerprint,
}

struct ParseOptionsKey {
    recovery: bool,
    collect_trivia: bool,
}

struct SourceFingerprint {
    hash: u64,
    bytes: Arc<str>,
}
```

- `source_id_namespace` — usually `(project_root, locale)` so the same message id in different locales / dictionaries does not collide.
- `message_id` — translation key. Stable identifier from the dictionary layer; the parser itself only sees source text and never depends on it.
- `parser_version` — exact parser compatibility identity. A result produced by a different parser version is never a hit.
- `parse_options` — normalized values for every option that changes the syntax-only `ParseResult`: `recovery` and `collect_trivia`. Metadata that does not affect parser output is excluded.
- `source.hash` — seeded XXH3-64 of the message source bytes, used only to accelerate this cache instance's map lookup.
- `source.bytes` — exact source bytes used for equality. Any byte-level edit invalidates the entry even if its hash collides.

`SourceFingerprint` equality compares both `hash` and `bytes`; its `Hash` implementation may feed only the precomputed `hash` into the map hasher. Standard map collision resolution then compares the exact bytes before returning a hit. A hash match with unequal bytes is a cache miss, never a reusable artifact. This makes a non-cryptographic hash acceptable without claiming that hashes never collide.

Each `ParseCache` owns one non-public `u64` fingerprint seed for its entire lifetime and computes `xxh3_64_with_seed(source.as_bytes(), seed)` exactly once while constructing a request key. Production construction derives a fresh seed from process-local randomness; it must not use a fixed public seed, wall-clock time alone, a memory address alone, or caller-controlled project data. Tests may inject a deterministic seed or a test-only fingerprint function. All key construction remains cache-internal, so one cache cannot accidentally receive a fingerprint produced with another cache's seed.

The implementation adds `xxhash-rust` as a direct workspace dependency with only the XXH3 feature when this cache lands; the current transitive `rustc-hash` dependency is not promoted or treated as this design's content-fingerprint contract. The outer concurrent map retains its own randomized key hasher independently of the source fingerprint.

The seed and XXH3 value are ephemeral implementation data. They are never serialized into snapshots, logs, diagnostics, metrics labels, CLI JSON, or persistent cache metadata, and callers cannot use them as source identity. A new process or cache instance may compute a different `hash` for identical bytes without changing cache semantics. Any future persistent representation verifies source identity through its own versioned durable contract and exact source consistency, not this in-memory lookup accelerator.

The cache constructs `CacheKey` internally from the request. Callers must not supply a prebuilt key that can disagree with `source`, `options`, or the cache's parser version. The dictionary layer may widen the namespace with file path or dictionary revision, but it must not remove parser version, result-affecting options, or exact source equality.

## Cache value

```rust
struct CachedParse {
    sources: Arc<SourceStore>, // owns source text, metadata, and line indexes
    result: ParseResult,       // every SourceId resolves through `sources`
}
```

- `SourceStore` is the owner that assigned every `SourceId` referenced by the result, CST token/trivia records, and diagnostics. Keeping it in the same cached value makes those ids valid for the full artifact lifetime.
- `ParseResult` already owns `CstTables`, materialized parser diagnostics, and the `trivia_collected` capability. Keeping the complete syntax result avoids losing fields when the parser API evolves.
- `Arc<SourceStore>` and the owned result make the entry shareable across readers. Source slices and line/column lookup always use `cached.sources` together with ids from `cached.result`; consumers never recreate a store and assume the same numeric ids will be assigned.
- The `Arc<str>` in `CacheKey.source` exists for exact cache-key equality. It is not a replacement owner for parser `SourceId` resolution; that role belongs exclusively to `CachedParse.sources`.
- The cache milestone requires `CachedParse` and every owned parser artifact reachable through it to satisfy `Send + Sync`. Per-parse `ParseWorkspace` scratch state is not stored and does not acquire that requirement. `ParseCache` itself is `Send + Sync`, while a lookup result remains immutable through `Arc<CachedParse>`.

## Semantic Artifact Separation

`ParseCache` never stores a `SemanticModel`, a lazy semantic once-cell, or a semantic-construction failure inside `CachedParse`. Syntax reuse and semantic reuse have different lifecycles: formatting needs the former, while linting or validation may additionally need the latter only after a diagnostic-free parse.

A future cache-backed semantic consumer may layer a separately owned value over the immutable parse artifact:

```rust
struct CachedSemantic {
    parse: Arc<CachedParse>,
    model: SemanticModel,
}
```

This is a consumer-layer shape, not an initial `ParseCache` API or Phase 3C linter requirement. Its key must include the complete parse-artifact identity, the exact semantic-construction compatibility version, and every future option that changes semantic facts. Such options never enter `ParseOptionsKey` merely to reuse the same map. A diagnostic-bearing parse has no semantic artifact. Construction misuse or invariant failure is not cached as a successful semantic value. Any semantic cache owns its own capacity, single-flight, invalidation, and weight accounting rather than silently consuming the parse cache's limits.

## Invariants enforced by the cache layer (not the parser)

1. **One active parse flight per complete `CacheKey` and residency epoch.** Parser version, result-affecting options, source hash, and exact source bytes are already part of the key. The first miss becomes the producer; concurrent equal-key callers wait for that flight and receive the same `Arc<CachedParse>`. A successful value remains one shared ready entry until explicit invalidation or eviction. Re-parsing after eviction, invalidation, or an unsuccessful producer attempt begins a new residency epoch and is allowed.
2. **All downstream syntax consumers read from the cache.** Variable extraction, syntax diagnostics, formatter, future cache-backed linter flows, and LSP requests consume `CachedParse` and never call `parse_source` themselves. A semantic consumer calls parser-owned `build_semantic_model(cached.sources.as_ref(), &cached.result)` after confirming parser diagnostics are empty; the resulting `SemanticModel` is a separate artifact and is never inserted into `ParseResult`. The formatter receives the same pair through its workspace-internal parsed-artifact API; it neither depends on `CachedParse` nor converts the entry to a Binary AST snapshot. For linter rules, this describes a future dictionary/LSP/cache-backed adapter workflow; it is not the initial Phase 3C `lintMessage(source, options)` or rule API input contract.
3. **Cache invalidation is explicit at file/dictionary update boundaries.** Either the dictionary layer removes entries for the changed namespace/message on write, or a lookup naturally misses when parser version, parse options, or exact source bytes differ. In an editor catalog, a host-document version or resource mapping change invalidates extraction and mapped-result state but does not by itself evict an unchanged entry's message-level parse artifact. Formatter- and linter-only configuration changes likewise do not alter this parse key. The selective editor invalidation and generation-check contract is owned by [009-ox-mf2-phase-3d-lsp-editor-design.md](./009-ox-mf2-phase-3d-lsp-editor-design.md#artifact-cache-and-invalidation).
4. **Normal parser diagnostics are cacheable success data.** A diagnostic-bearing `ParseResult` is a completed parse artifact and becomes `Ready` exactly like a clean result. `ParseError`, source-store construction failure, producer cancellation/drop, and panic are unsuccessful attempts: current waiters receive one cache-layer failure, no ready value is retained, and a later lookup may retry.
5. **Single-flight coordination is per key, not one global parse lock.** Different complete keys may parse concurrently. No map shard, entry-table mutex, or flight-state mutex is held while source storage, parsing, or diagnostic materialization runs. Semantic construction is outside this parse flight.

## Concurrent Entry Lifecycle

Each complete key has one conceptual slot with these states:

```rust
enum EntryState {
    InFlight(Arc<ParseFlight>),
    Ready(ReadyEntry),
}

struct ReadyEntry {
    artifact: Arc<CachedParse>,
    estimated_bytes: u64,
    last_access: AccessStamp,
}

enum FlightOutcome {
    Running,
    Complete(Result<Arc<CachedParse>, Arc<CacheLookupError>>),
}
```

A lookup uses the concurrent map's atomic entry operation rather than a separate `get` followed by `insert`:

1. `Ready` refreshes internal recency and returns the stored `Arc<CachedParse>`.
2. The caller that atomically creates `InFlight` is the sole producer for that key and residency epoch.
3. A caller that observes `InFlight` waits on that flight without retaining the map entry guard or shard lock.
4. The producer builds its private `SourceStore` and parses outside all cache locks.
5. On success, the producer transitions the same slot to `Ready`, publishes the same `Arc<CachedParse>` to every waiter, and wakes them.
6. On `ParseError` or other returned construction failure, it publishes the same failure to current waiters, removes the failed slot, and wakes them. The failure is not a negative cache entry.
7. A producer-drop guard covers unwind or cancellation. If the producer exits before publishing, the guard marks the flight `producer_aborted`, removes or retires the slot, and wakes every waiter. Panic payloads and dependency debug text are not copied into the cache-layer error; the producer's panic may continue to the caller-owned worker boundary.

`CacheLookupError` is owned by the cache layer rather than added to the parser API. It distinguishes a returned parser/construction error from `producer_aborted` sufficiently for the consumer to avoid waiting forever; it is not a parser diagnostic and is not retained after the flight ends. The flight shares one immutable error value through `Arc` so every current waiter observes the same normalized outcome without requiring dependency errors to implement `Clone`. A waiter does not silently elect itself producer within the failed flight. A subsequent independent lookup performs the retry through a new atomic entry operation.

Explicit invalidation marks a matching in-flight slot retired before unlinking it. Its existing producer may finish and satisfy callers already attached to that flight, but it must not reinsert a ready value into the cache after retirement. A later lookup starts a new residency epoch. This is the only intentional case in which an old retired producer and a new producer for the same complete key may temporarily overlap; the invalidation boundary, not a lookup race, created the overlap.

## Capacity and Eviction

Every `ParseCache` is constructed with two finite, non-zero limits:

```rust
struct CacheLimits {
    max_entries: NonZeroUsize,
    max_estimated_bytes: NonZeroU64,
}
```

There is no implicit unbounded mode in the reusable cache. A long-lived LSP, one-shot dictionary checker, or other consumer selects limits appropriate to its own process budget when constructing the cache; the initial values are internal integration and benchmark choices, not project configuration, editor settings, or a public CLI option.

Only resident `Ready` entries count toward both limits. After a successful producer completes, the cache computes one immutable `estimated_bytes` weight for the key, coordination entry, and retained artifact. The estimate includes conservatively retained key/namespace/message strings, exact source fingerprint bytes, `SourceStore` text and line indexes, CST/token/trivia table capacities, diagnostics, and fixed map/`Arc` bookkeeping. A separately retained `SemanticModel` is not owned or counted by `ParseCache`. Shared allocations are counted at least once for each cache entry that keeps them alive; the estimator does not reduce the weight merely because another external `Arc` currently exists. Saturating `u64` arithmetic turns an unrepresentable estimate into an over-limit weight rather than wrapping.

The byte limit is deliberately an estimated cache-residency budget, not an allocator-exact whole-process memory promise. Per-call parser scratch memory and externally retained `Arc<CachedParse>` clones are outside the resident-cache total. Bounded caller-owned concurrency and the parser/resource input limits bound active parse work; the cache records in-flight counts and weights for metrics but does not evict or synchronously cancel an `InFlight` producer to enforce the ready-entry LRU.

An artifact whose individual weight exceeds `max_estimated_bytes` is published once to its producer and existing waiters but is not installed as `Ready`. Its flight is removed after publication, so a later lookup may parse it again. Otherwise the new artifact is installed as the most-recently-used ready entry, then the cache evicts least-recently-used ready entries until both the entry-count and estimated-byte limits are satisfied. A hit refreshes recency. Concurrent hit order may follow the cache synchronization order and is not a semantic or serialized-output contract.

Eviction removes only the cache's map ownership. A caller that already cloned the evicted `Arc<CachedParse>` may continue using it safely, and the cache never waits for such external readers before admitting another entry. Explicit invalidation and `clear` update ready-entry counts, estimated bytes, and recency metadata through the same accounting path. In-flight retirement follows the separate lifecycle above and must not subtract a ready weight that was never installed.

Capacity affects only performance. A hit, a miss, an overweight non-resident result, and a result whose entry was evicted immediately after another access must expose identical `CachedParse` content for the same complete key. No reporter, diagnostic, formatter output, or exit behavior may depend on LRU state.

## Suggested implementation skeleton

```rust
// In the dictionary tooling crate, NOT in ox_mf2_parser.
pub struct ParseCache {
    entries: ConcurrentMap<CacheKey, EntryState>,
    parser_version: ParserVersion,
    source_fingerprint: SourceFingerprintHasher, // seeded XXH3-64
    limits: CacheLimits,
    ready_accounting: ReadyAccounting,
}

impl ParseCache {
    pub fn get_or_parse(
        &self,
        source_id_namespace: SourceIdNamespace,
        message_id: MessageId,
        source: &str,
        options: ParseOptions,
    ) -> Result<Arc<CachedParse>, Arc<CacheLookupError>> {
        let key = CacheKey {
            source_id_namespace,
            message_id,
            parser_version: self.parser_version,
            parse_options: ParseOptionsKey::from(&options),
            source: self.source_fingerprint.fingerprint(source),
        };
        match self.entries.reserve_or_observe(key) {
            Reservation::Ready(ready) => {
                self.ready_accounting.record_hit(&ready);
                Ok(ready.artifact.clone())
            }
            Reservation::Wait(flight) => flight.wait(),
            Reservation::Produce(mut producer) => {
                // No map or flight lock is held while this closure runs.
                // The producer guard wakes waiters and retires the slot if
                // parsing returns an error or unwinds before publication.
                producer.run(|| {
                    let mut sources = SourceStore::new();
                    let id = sources
                        .try_add(SourceFileInput {
                            source,
                            ..Default::default()
                        })
                        .map_err(CacheLookupError::from_source_store)?;
                    let result = parse_source(&sources, id, options)
                        .map_err(CacheLookupError::from_parse)?;
                    Ok(Arc::new(CachedParse {
                        sources: Arc::new(sources),
                        result,
                    }))
                })
            }
        }
    }

    pub fn invalidate_message(
        &self,
        source_id_namespace: &SourceIdNamespace,
        message_id: &MessageId,
    ) {
        self.entries.retire_matching(|key| {
            key.source_id_namespace == *source_id_namespace
                && key.message_id == *message_id
        });
    }
}
```

`ConcurrentMap`, `Reservation`, `ParseFlight`, `ReadyAccounting`, and access stamps above are conceptual boundaries, not mandated dependencies or public type names. The producer-publication path owns weight calculation, ready installation, and any required eviction as one accounting transaction before other lookups observe the new resident state. An implementation may use a sharded map plus per-entry mutex/condition variable, a synchronous once-cell-like primitive with explicit failure cleanup, or an equivalent mechanism. A plain concurrent-map `get` followed by parse and `insert` is not conforming because two concurrent misses can both parse. Waiting is synchronous and does not cause the cache to create a runtime; an async consumer may place the call on its own blocking worker boundary.

Consumers keep the owner/result pair intact:

```rust
let cached = cache.get_or_parse(namespace, message_id, source, options)?;
let file = cached
    .sources
    .get(cached.result.source)
    .expect("cache preserves the parse result's SourceStore owner");
let view = CstView::new(
    &cached.sources,
    cached.result.source,
    &cached.result.cst,
);

let formatted = format_parsed(
    cached.sources.as_ref(),
    &cached.result,
    format_options,
);
```

`format_parsed` and `check_parsed` create the formatter-owned validated input view from these two references. They do not reparse `file`, copy the CST, or encode/decode a snapshot. A diagnostic-bearing cached result is passed through unchanged so the formatter can apply its normal strict diagnostic policy. The deferred bounded parsed-artifact API will reuse this same owner-pair contract when implemented.

Moving only `cached.result`, persisting only its numeric `SourceId`, or attaching it to a newly-created `SourceStore` violates both the cache and parsed-formatter contracts. The formatter rejects inconsistencies that the retained data makes observable, but coincidentally equal numeric ids are not proof of ownership. If a future cache combines multiple entries into one shared store, it must explicitly remap every SourceId-bearing record; numeric id reuse is never sufficient.

The cache deliberately does NOT live inside `ParseWorkspace`. `ParseWorkspace` is a per-thread scratchpad whose lifetime is one or a few `parse_*` calls; the cache spans many parses with long-lived owned results.

## Process Lifetime and Persistence

`ParseCache` is an in-memory, process-local component. An LSP/editor integration may retain it for the server process lifetime; a dictionary checker, batch workflow, or CLI command retains it only for that invocation. Dropping or explicitly clearing the cache releases its resident map ownership, and a later process or cache instance reparses on demand. The cache performs no filesystem reads or writes and defines no cache directory, cleanup command, environment variable, project setting, editor setting, or public CLI persistence option.

Binary AST snapshots are the separate versioned representation for a consumer that deliberately needs artifact exchange across processes. Producing or loading such a snapshot is an explicit consumer workflow governed by [003-ox-mf2-phase-2-binary-ast-snapshot-design.md](./003-ox-mf2-phase-2-binary-ast-snapshot-design.md); it is not an automatic backing store, second lookup tier for `ParseCache`, or intermediate formatter input for an in-memory cache hit. Snapshot schema compatibility, source consistency, corruption handling, and future snapshot-to-`SemanticModel` reconstruction remain outside this cache.

In-memory `CacheKey` instances, the seeded XXH3 value and seed, single-flight state, access stamps, LRU order, estimated weights, and namespace invalidation metadata are never serialized. A future request for automatic disk caching requires a new design rather than a storage-backend trait reserved in the initial API. The initial implementation therefore stays free to shape its concurrency and eviction internals around process memory without treating them as a durable compatibility surface.

## Why the parser keeps the same API

Nothing in Phase 1 changes:

- `parse_source` continues to return `Result<ParseResult, ParseError>` with an owned result.
- `parse_source_session` continues to return `Result<ParseSessionResult, ParseError>` with a borrowed result tied to a `ParseWorkspace`.
- `parse_batch` continues to return `Result<BatchParseResult, BatchParseError>` with ordered `BatchParseItem`s.

The cache layer composes these primitives. Parser-core benchmarks (`parse_cst_no_trivia` / `parse_cst`) describe the parse cost the cache pays on a miss. `lower_semantic` separately measures `build_semantic_model` over an already available owner/result pair, and parser-owned `semantic_validation` is a third benchmark phase.

## What the cache layer should also measure

Once a real cache lands, the bench harness should grow scenarios that match how dictionary tooling actually runs:

- many small messages across many locales (cache populated entries)
- same dictionary parsed twice in a row (hit ratio = 1)
- random 10% rewrite (mixed hit / miss)
- recovery-heavy dictionary (malformed messages)
- many concurrent callers for one cold key, reporting producer work once and waiter latency separately
- concurrent misses for distinct keys, proving single-flight coordination does not serialize unrelated parses

Captured as `.reviews/005-ox-mf2-ox-content-mf2-parser-optimization-notes.md` Recommended Follow-up Work #2.

## Validation

- Compile-time assertions require `ParseCache: Send + Sync` and `CachedParse: Send + Sync`.
- A barrier starts many equal-key callers simultaneously; an injected parse counter is exactly one, every caller receives the same `Arc` identity, and both clean and diagnostic-bearing results are covered.
- Distinct keys cross a second barrier inside the parser seam to prove their producers can run concurrently.
- A constant test hasher proves unequal exact source bytes never join one flight even when their hashes collide.
- Production XXH3 fixtures lock neither a public digest nor a cross-version serialized value; they verify equal bytes under one cache seed hash equally, different cache seeds remain semantically equivalent, and no hash or seed appears in observable output.
- Returned `ParseError` and source-store failures wake every waiter with the same flight outcome, retain no entry, and allow the next lookup to become a new producer.
- An injected producer panic/drop wakes every waiter as `producer_aborted`, leaks no slot or lock, and permits a later successful retry without poisoning the cache.
- Invalidation during an in-flight parse prevents the retired producer from restoring a ready entry. Existing waiters may receive its completed artifact, while a post-invalidation lookup belongs to a new residency epoch.
- Stress tests mix hits, misses, failures, invalidation, and forced out-of-order completion under a bounded caller-owned pool and verify that no caller waits indefinitely.
- Capacity tests independently cross the entry and estimated-byte limits, refresh recency on hit, evict the least-recently-used ready entry, and verify accounting after explicit invalidation and `clear`.
- A single overweight artifact is returned to every current flight participant but is not retained; a later lookup parses again. Estimates at the exact byte limit remain resident, while saturating overflow is treated as over-limit.
- An externally held `Arc<CachedParse>` remains usable after eviction, and its continued lifetime does not block new cache admission. Equivalent parse content is observed with large, tiny, and churn-heavy limits.
- A cache miss followed by `format_parsed` increments an injected parser counter once; a cache hit followed by `format_parsed` leaves it unchanged. Parsed-artifact formatting matches `format_message` output, diagnostics, and `changed`, while injected snapshot encoder/decoder counters remain zero.
- Parsed-artifact formatter tests cover clean and diagnostic-bearing results, standard mode without trivia, preserve mode with trivia, and the `parsed_artifact_attachment` failure boundary for detectable owner/source/span inconsistency.

## Open Questions

No parse artifact cache open questions remain at this design level. Consumer-specific capacity values and implementation dependency versions remain benchmark and implementation choices within the contracts above.
