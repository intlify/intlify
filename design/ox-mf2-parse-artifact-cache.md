# ox-mf2 Parse Artifact Cache (Design Note)

## Purpose

Phase 1 ships a parser core that parses one caller-owned `SourceStore` entry identified by `SourceId` into an owned `ParseResult` (CST tables + diagnostics + optional `SemanticModel`). Dictionary-shaped tooling — i18n checkers, LSPs, batch linters, codegen — works at a different granularity: many `(locale, message_id, source_text)` entries, frequently revisited as files change.

The `ox-content` MF2 checker (see `refers/ox-content/crates/ox_content_i18n/src/checker.rs`) re-parses the same dictionary value across `check_type_mismatch`, `check_syntax_errors`, and `check_all`, multiplied across locale/key loops. Anti-pattern reference captured in `.reviews/005-ox-mf2-ox-content-mf2-parser-optimization-notes.md`, Point 5.

This document is the **bridge** between Phase 1's parser API and the Phase 2 dictionary tooling layer: it pins the shape of a per-message parse artifact cache so the parser core does not have to learn about dictionaries, and so the dictionary layer does not have to keep re-parsing.

This is a design note, not a Phase 1 deliverable.

Phase 3C's initial linter remains source-backed: `lintMessage(source, options)` parses and validates the supplied source text directly. Cache-backed or snapshot-backed linting is a future optimization for dictionary tooling, LSP/editor adapters, and batch workflows after the parser owns a snapshot-to-`SemanticModel` path. This cache note therefore describes a layered reuse strategy, not an initial linter API requirement.

## Non-goals

- The parser core stays oblivious of dictionaries, locales, and keys. The cache lives in a higher layer (`ox_mf2_dict`, `ox_mf2_lsp`, an i18n checker, etc.).
- This note does not introduce a public Phase 1 cache API. Phase 1 already exposes the primitives — `parse_source_session`, `parse_batch`, and `ParseWorkspace` — that the cache builds on.
- No concurrency story here: parallel batch parsing is a separate follow-up (`.reviews/003` Priority 9).

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
    parse_semantic: bool,
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
- `parse_options` — normalized values for every option that changes `ParseResult`: `recovery`, `parse_semantic`, and `collect_trivia`. Metadata that does not affect parser output is excluded.
- `source.hash` — content hash (e.g. blake3 / xxhash) of the message source bytes, used to accelerate map lookup.
- `source.bytes` — exact source bytes used for equality. Any byte-level edit invalidates the entry even if its hash collides.

`SourceFingerprint` equality compares both `hash` and `bytes`; its `Hash` implementation may feed only the precomputed `hash` into the map hasher. Standard map collision resolution then compares the exact bytes before returning a hit. A hash match with unequal bytes is a cache miss, never a reusable artifact. This makes a non-cryptographic hash acceptable without claiming that hashes never collide.

The cache constructs `CacheKey` internally from the request. Callers must not supply a prebuilt key that can disagree with `source`, `options`, or the cache's parser version. The dictionary layer may widen the namespace with file path or dictionary revision, but it must not remove parser version, result-affecting options, or exact source equality.

## Cache value

```rust
struct CachedParse {
    sources: Arc<SourceStore>, // owns source text, metadata, and line indexes
    result: ParseResult,       // every SourceId resolves through `sources`
}
```

- `SourceStore` is the owner that assigned every `SourceId` referenced by the result, CST token/trivia records, and diagnostics. Keeping it in the same cached value makes those ids valid for the full artifact lifetime.
- `ParseResult` already owns `CstTables`, materialized parser diagnostics, optional `SemanticModel`, and the `trivia_collected` capability. Keeping the complete result avoids losing fields when the parser API evolves.
- `Arc<SourceStore>` and the owned result make the entry shareable across readers. Source slices and line/column lookup always use `cached.sources` together with ids from `cached.result`; consumers never recreate a store and assume the same numeric ids will be assigned.
- The `Arc<str>` in `CacheKey.source` exists for exact cache-key equality. It is not a replacement owner for parser `SourceId` resolution; that role belongs exclusively to `CachedParse.sources`.

## Invariants enforced by the cache layer (not the parser)

1. **One parse per complete `CacheKey` per dictionary update.** Parser version, result-affecting options, source hash, and exact source bytes are already part of the key. Re-parsing an equal key is a bug in the cache, not in the parser.
2. **All downstream checks read from the cache.** Variable extraction, syntax diagnostics, semantic validation, formatter, linter rules, LSP requests — all consume `CachedParse`, never call `parse_source` themselves. For linter rules, this describes a future dictionary/LSP/cache-backed adapter workflow; it is not the initial Phase 3C `lintMessage(source, options)` or rule API input contract.
3. **Cache invalidation is explicit at file/dictionary update boundaries.** Either the dictionary layer removes entries for the changed namespace/message on write, or a lookup naturally misses when parser version, parse options, or exact source bytes differ.

## Suggested implementation skeleton

```rust
// In the dictionary tooling crate, NOT in ox_mf2_parser.
pub struct ParseCache {
    entries: DashMap<CacheKey, Arc<CachedParse>>, // or HashMap behind RwLock
    parser_version: ParserVersion,
}

impl ParseCache {
    pub fn get_or_parse(
        &self,
        source_id_namespace: SourceIdNamespace,
        message_id: MessageId,
        source: &str,
        options: ParseOptions,
    ) -> Result<Arc<CachedParse>, ParseError> {
        let key = CacheKey {
            source_id_namespace,
            message_id,
            parser_version: self.parser_version,
            parse_options: ParseOptionsKey::from(options),
            source: SourceFingerprint::new(source),
        };
        if let Some(hit) = self.entries.get(&key) {
            // CacheKey equality includes exact source bytes. A hash collision
            // with different bytes cannot reach this branch.
            return Ok(hit.clone());
        }
        // Parse with a per-entry SourceStore, then move both owner and result
        // into the same long-lived cached artifact.
        let mut sources = SourceStore::new();
        let id = sources
            .try_add(SourceFileInput {
                source,
                ..Default::default()
            })
            .map_err(|_| ParseError::SourceTooLarge)?;
        let result = parse_source(&sources, id, options)?;
        let cached = Arc::new(CachedParse {
            sources: Arc::new(sources),
            result,
        });
        self.entries.insert(key, cached.clone());
        Ok(cached)
    }

    pub fn invalidate_message(
        &self,
        source_id_namespace: &SourceIdNamespace,
        message_id: &MessageId,
    ) {
        self.entries.retain(|key, _| {
            key.source_id_namespace != *source_id_namespace
                || key.message_id != *message_id
        });
    }
}
```

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
```

Moving only `cached.result`, persisting only its numeric `SourceId`, or attaching it to a newly-created `SourceStore` violates the cache contract. If a future cache combines multiple entries into one shared store, it must explicitly remap every SourceId-bearing record; coincidental numeric id reuse is never sufficient.

The cache deliberately does NOT live inside `ParseWorkspace`. `ParseWorkspace` is a per-thread scratchpad whose lifetime is one or a few `parse_*` calls; the cache spans many parses with long-lived owned results.

## Why the parser keeps the same API

Nothing in Phase 1 changes:

- `parse_source` continues to return `Result<ParseResult, ParseError>` with an owned result.
- `parse_source_session` continues to return `Result<ParseSessionResult, ParseError>` with a borrowed result tied to a `ParseWorkspace`.
- `parse_batch` continues to return `Result<BatchParseResult, BatchParseError>` with ordered `BatchParseItem`s.

The cache layer composes these primitives. Parser-core benchmarks (`parse_cst_no_trivia` / `parse_cst` / `lower_semantic`) still describe the cost the cache pays on a miss. Here `lower_semantic` means SemanticModel construction only; parser-owned `semantic_validation` is a separate benchmark phase.

## What the cache layer should also measure

Once a real cache lands, the bench harness should grow scenarios that match how dictionary tooling actually runs:

- many small messages across many locales (cache populated entries)
- same dictionary parsed twice in a row (hit ratio = 1)
- random 10% rewrite (mixed hit / miss)
- recovery-heavy dictionary (malformed messages)

Captured as `.reviews/005-ox-mf2-ox-content-mf2-parser-optimization-notes.md` Recommended Follow-up Work #2.

## Open questions (defer to implementation)

- Eviction policy: LRU? unbounded? per-dictionary capped? The right answer depends on whether the cache lives in an LSP (bounded by open documents) or a one-shot checker (unbounded for the run).
- Hashing: blake3 (fast, large) vs xxhash (faster, smaller, non-crypto). Non-crypto is fine because hash equality never substitutes for exact source-byte equality; collision behavior should still be covered by a test hasher that deliberately returns a constant.
- Persistence: should cache entries survive across CLI invocations? Binary AST snapshots (see [003-ox-mf2-phase-2-binary-ast-snapshot-design.md](./003-ox-mf2-phase-2-binary-ast-snapshot-design.md)) are the right durable representation; the in-memory cache is for one process lifetime.
