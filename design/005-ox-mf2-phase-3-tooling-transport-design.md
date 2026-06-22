# ox-mf2 Phase 3 Tooling and Transport Design

## Purpose

This document defines the Phase 3 design boundary for tooling and transport workflows around ox-mf2.

Phase 1 parser design is defined in [002-ox-mf2-phase-1-rust-parser-design.md](./002-ox-mf2-phase-1-rust-parser-design.md). Phase 2 Binary AST snapshot design is defined in [003-ox-mf2-phase-2-binary-ast-snapshot-design.md](./003-ox-mf2-phase-2-binary-ast-snapshot-design.md). Phase 2 language binding design is defined in [004-ox-mf2-phase-2-language-bindings-design.md](./004-ox-mf2-phase-2-language-bindings-design.md).

This document focuses on formatter/linter input, SemanticView exposure, LSP/editor workflows, transport choices, and long-lived language-service scenarios.

## Basic Policy

The standard CST/AST product boundary remains the versioned Binary AST snapshot. Tooling may use Rust-internal construction-time tables for fast paths, but public cross-language tooling input should converge on the Binary AST decoder/accessor view.

Semantic information is exposed separately as SemanticView or a later compact semantic snapshot. It is not forced into the lossless Binary AST snapshot.

MessagePack is not the CST/AST representation of ox-mf2. It is reserved as a future transport for long-lived language-service workflows.

## SemanticView

![ox-mf2 SemanticView](./assets/003-ox-mf2-semantic-view.svg)

SemanticView is separate from the lossless Binary AST snapshot.

Binary AST handles CST, tokens, trivia, and source spans. SemanticView handles semantic facts.

- declarations
- references
- selectors
- variants
- fallback/default information
- duplicate keys
- coverage metadata
- links to NodeId and Span

Linters, compilers, validators, and language-service features can combine Binary AST decoder/accessor traversal with SemanticView.

SemanticView should remain derived from the Rust core semantic model. Bindings may expose it later, but they must not implement a separate semantic analyzer in JavaScript or another host language.

## Formatter Input

From Phase 2 onward, public AST input for formatter APIs is the Binary AST decoder/accessor view.

Formatter implementation may have Rust-internal fast paths over construction-time tables when formatting immediately after parse. However, stable public formatter input is the Binary AST view shared by Rust, N-API, WASM, and later consumers.

A future formatter should support at least two modes.

- preserve mode: preserve original representation as much as possible using tokens, trivia, delimiter spans, and source slices
- canonical mode: format to the standard ox-mf2 style

Formatter output should measure parser, snapshot decode/access, semantic lookup, and formatting cost separately.

## Linter Input

From Phase 2 onward, public AST input for linter APIs is the Binary AST decoder/accessor view plus optional SemanticView.

Rule implementations may use Rust-internal semantic fast paths, but rule-facing / binding-facing traversal should converge on the same public Binary AST view whenever practical.

Core diagnostics use SourceId and UTF-8 byte Span as the canonical location model. CLI, LSP, and editor integrations convert spans to line/column or UTF-16 positions through SourceStore or SourceView.

Suppression and directive comments are diagnostic-layer concerns. This document does not fix a concrete directive comment syntax inside MF2. A future linter design can define the suppression data shape when rule execution enters implementation.

## Parse Artifact Cache

Dictionary-shaped tooling frequently revisits the same `(locale, message_id, source_text)` entries. The parser core should not learn about dictionaries, but higher layers may cache owned parse artifacts.

The parse artifact cache design note is [ox-mf2-parse-artifact-cache.md](./ox-mf2-parse-artifact-cache.md). It is intentionally unnumbered because it is a bridge note, not a numbered phase design document.

## LSP and Editor Workflow

Long-lived language-service workflows should avoid re-parsing and re-encoding on every request. They can combine:

- SourceStore / SourceView for source identity and location conversion
- parse artifact cache for repeated dictionary entries
- Binary AST snapshot or decoded SnapshotView for syntax traversal
- SemanticView for semantic queries
- diagnostics store for parser and linter diagnostics

LSP-facing UTF-16 positions are converted at the editor boundary. Core parser and snapshot spans remain UTF-8 byte offsets.

## MessagePack Transport

MessagePack is not the CST/AST representation of ox-mf2.

It is a future transport candidate for long-lived language-service workflows such as LSP, editor integration, daemon mode, and repeated semantic queries. The standard CST/AST product boundary remains the versioned Binary AST snapshot.

If MessagePack transport is added later, its overhead must be measured separately from parser, semantic lowering, snapshot encoding, snapshot decoding, binding cost, and LSP request handling.

MessagePack payloads should transport query/response data or language-service session messages. They should not become a second AST format that competes with the Binary AST snapshot.

## Benchmarks

Tooling and transport benchmarks must be phase-separated.

Relevant Phase 3 benchmark phases:

- format_preserve
- format_canonical
- lint_rules
- semantic_query
- lsp_jsonrpc
- lsp_msgpack
- cache_hit_query
- cache_miss_parse
- e2e_lint
- e2e_format

Reports should separate parser, semantic lowering, snapshot encode/decode, binding calls, MessagePack transport, JSON-RPC transport, cache hit/miss behavior, and actual rule/formatter work.
