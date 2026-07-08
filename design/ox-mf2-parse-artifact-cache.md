# ox-mf2 Parse Artifact Cache (Design Note)

## Purpose

Phase 1 ships a parser core that turns a single `(source_id, source_text)` into an owned `ParseResult` (CST tables + diagnostics + optional `SemanticModel`). Dictionary-shaped tooling — i18n checkers, LSPs, batch linters, codegen — works at a different granularity: many `(locale, message_id, source_text)` entries, frequently revisited as files change.

The `ox-content` MF2 checker (see `refers/ox-content/crates/ox_content_i18n/src/checker.rs`) re-parses the same dictionary value across `check_type_mismatch`, `check_syntax_errors`, and `check_all`, multiplied across locale/key loops. Anti-pattern reference captured in `.reviews/005-ox-mf2-ox-content-mf2-parser-optimization-notes.md`, Point 5.

This document is the **bridge** between Phase 1's parser API and the Phase 2 dictionary tooling layer: it pins the shape of a per-message parse artifact cache so the parser core does not have to learn about dictionaries, and so the dictionary layer does not have to keep re-parsing.

This is a design note, not a Phase 1 deliverable.

Phase 3C's initial linter remains source-backed: `lintMessage(source, options)` parses and validates the supplied source text directly. Cache-backed or snapshot-backed linting is a future optimization for dictionary tooling, LSP/editor adapters, and batch workflows after the parser owns a snapshot-to-`SemanticModel` path. This cache note therefore describes a layered reuse strategy, not an initial linter API requirement.

## Non-goals

- The parser core stays oblivious of dictionaries, locales, and keys. The cache lives in a higher layer (`ox_mf2_dict`, `ox_mf2_lsp`, an i18n checker, etc.).
- This note does not introduce a public Phase 1 cache API. Phase 1 already exposes the primitives — `parse_source_session`, `parse_batch`, and `ParseWorkspace` — that the cache builds on.
- No concurrency story here: parallel batch parsing is a separate follow-up (`.reviews/003` Priority 9).

## Cache key

```
(source_id_namespace, message_id, source_hash)
```

- `source_id_namespace` — usually `(project_root, locale)` so the same message id in different locales / dictionaries does not collide.
- `message_id` — translation key. Stable identifier from the dictionary layer; the parser itself only sees source text and never depends on it.
- `source_hash` — content hash (e.g. blake3 / xxhash) of the message source bytes. Defines cache validity: any byte-level edit invalidates the entry.

The dictionary layer is free to widen the key (file path, dictionary revision, parser version) but should never narrow it past content hash — two different source texts must never collide on one cache entry.

## Cache value

```rust
struct CachedParse {
    cst: CstTables,                          // owned, cloned from ParseWorkspace
    diagnostics: Vec<Diagnostic>,             // materialised once
    semantic: Option<SemanticModel>,          // populated iff requested
    source: Arc<str>,                         // pointer-stable source text
    source_id: SourceId,                      // stable within the SourceStore
}
```

- `CstTables` is `Clone + Send + Sync` (no interior mutability). A cached entry is freely shareable across reader threads.
- `Diagnostic` is owned and `Send`; the dictionary layer can hand the list to a reporter without re-parsing.
- `SemanticModel` is optional — only the consumers that need declarations / references / variants pay for it. Producers that only want syntax errors leave it `None`.
- `Arc<str>` makes the cached source text cheap to share with later consumers that need to render a span (e.g. an LSP hover or a CLI diagnostic printer).
- `SourceId` lets cached spans land back in a fresh `SourceStore` for span → line/column resolution without re-parsing.

## Invariants enforced by the cache layer (not the parser)

1. **One parse per `(key, parser_version)` per dictionary update.** Re-parsing the same source under the same parser version is a bug in the cache, not in the parser.
2. **All downstream checks read from the cache.** Variable extraction, syntax diagnostics, semantic validation, formatter, linter rules, LSP requests — all consume `CachedParse`, never call `parse_source` themselves.
3. **Cache invalidation is explicit at file/dictionary update boundaries.** Either the dictionary layer wipes the entry on write, or it stamps each entry with `(source_hash, parser_version)` and lazily invalidates on mismatch.

## Suggested implementation skeleton

```rust
// In the dictionary tooling crate, NOT in ox_mf2_parser.
pub struct ParseCache {
    workspace: ParseWorkspace,
    entries: DashMap<CacheKey, Arc<CachedParse>>, // or HashMap behind RwLock
    parser_version: u32,
}

impl ParseCache {
    pub fn get_or_parse(
        &self,
        key: CacheKey,
        source: &str,
        options: ParseOptions,
    ) -> Arc<CachedParse> {
        if let Some(hit) = self.entries.get(&key) {
            return hit.clone();
        }
        // Parse with a temporary SourceStore, then move the result into
        // a long-lived Arc. The workspace can be reused for the next miss.
        let mut sources = SourceStore::new();
        let id = sources.add(SourceFileInput {
            source,
            ..Default::default()
        });
        let result = parse_source(&sources, id, options);
        let cached = Arc::new(CachedParse {
            cst: result.cst,
            diagnostics: result.diagnostics,
            semantic: result.semantic,
            source: source.into(),
            source_id: id,
        });
        self.entries.insert(key, cached.clone());
        cached
    }

    pub fn invalidate(&self, key: &CacheKey) {
        self.entries.remove(key);
    }
}
```

The cache deliberately does NOT live inside `ParseWorkspace`. `ParseWorkspace` is a per-thread scratchpad whose lifetime is one or a few `parse_*` calls; the cache spans many parses with long-lived owned results.

## Why the parser keeps the same API

Nothing in Phase 1 changes:

- `parse_source` continues to return an owned `ParseResult`.
- `parse_source_session` continues to return a borrowed `ParseSessionResult` tied to a `ParseWorkspace`.
- `parse_batch` continues to return ordered `BatchParseItem`s.

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
- Hashing: blake3 (fast, large) vs xxhash (faster, smaller, non-crypto). Non-crypto is fine — the cache is not a security boundary.
- Persistence: should cache entries survive across CLI invocations? Binary AST snapshots (see [003-ox-mf2-phase-2-binary-ast-snapshot-design.md](./003-ox-mf2-phase-2-binary-ast-snapshot-design.md)) are the right durable representation; the in-memory cache is for one process lifetime.
