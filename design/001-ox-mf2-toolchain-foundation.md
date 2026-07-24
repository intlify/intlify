# ox-mf2 Toolchain Foundation Design

## Purpose

ox-mf2 is designed not only as a high-performance parser for MessageFormat 2.0 (MF2), but also as an MF2 toolchain foundation that can later support resource extraction, linting, formatting, message linking, compilation, diagnostics, and bindings.

The initial implementation focuses on the parser. However, tokens, trivia, spans, NodeId, diagnostics, and the table boundary are treated as part of the initial design so that later tools can be added without breaking the foundation.

## Design Overview

- Make Rust crates the single source of truth for MF2 behavior across parser, resource, formatter, linter, and message-linker workflows.
- In Phase 1, build a recovering, lossless, snapshot-friendly parser foundation.
- In Phase 2, make a versioned Binary AST snapshot the standard boundary for the public CST/AST view.
- Treat N-API and WASM as the primary language binding targets.
- Keep future SemanticView exposure separate from the lossless Binary AST snapshot and link semantic facts to NodeId / Span.
- Treat the Unicode WG interchange data model as an on-demand compatibility surface, not as a Phase 1 or Phase 2 deliverable.
- Reserve MessagePack for future LSP/editor transport, not as an AST representation.
- The Rust parser / AST / performance details for Phase 1 live in [002-ox-mf2-phase-1-rust-parser-design.md](./002-ox-mf2-phase-1-rust-parser-design.md).
- The implementation-oriented details for Phase 2 Binary AST snapshots live in [003-ox-mf2-phase-2-binary-ast-snapshot-design.md](./003-ox-mf2-phase-2-binary-ast-snapshot-design.md).
- The implementation-oriented details for Phase 2 language bindings live in [004-ox-mf2-phase-2-language-bindings-design.md](./004-ox-mf2-phase-2-language-bindings-design.md).
- The implementation-oriented details for Phase 3 tooling and transport live in [005-ox-mf2-phase-3-tooling-transport-design.md](./005-ox-mf2-phase-3-tooling-transport-design.md).
- The unnumbered resource catalog milestone whose consumer-neutral foundation may start independently of Phase 3C, and whose complete CLI integration precedes Phase 3D, lives in [013-ox-mf2-resource-catalog-adapter-design.md](./013-ox-mf2-resource-catalog-adapter-design.md).
- The message-linker track, which may proceed independently of the linter and editor product sequence once its resource prerequisites are available, lives in [014-ox-mf2-message-linker-design.md](./014-ox-mf2-message-linker-design.md).

## Design Philosophy

### Design as an MF2 Toolchain Foundation

![ox-mf2 toolchain foundation](./assets/001-ox-mf2-toolchain-foundation.svg)

The central design principle of ox-mf2 is **MF2 toolchain foundation**.

The parser is central, but the goal is not merely to build the fastest standalone parser. The goal is to build a foundation where linting, formatting, compilation, runtime validation, editor integration, and benchmarking can all sit on top of the same core model.

### Inherit oxc's High-Performance Design Philosophy

ox-mf2 may reuse some crates provided by oxc. This is not only about crate reuse. ox-mf2 also explicitly inherits oxc's high-performance design philosophy: phase separation, data-oriented tables, allocation control, and benchmark-driven design.

- phase separation: make lexer, parse, SemanticModel construction, diagnostics, formatting, and linting individually measurable.
- data-oriented tables: avoid relying only on pointer traversal, and use NodeId plus flat indexed tables to make downstream work fast.
- stable identifiers: allow AST/CST nodes, tokens, and sources to be referenced by IDs.
- allocation control: avoid unnecessary heap allocation during the parse phase.
- benchmark-driven design: measure internal phases, not only end-to-end performance.

However, ox-mf2 does not adopt the same arena typed AST model as oxc. MF2 has a smaller syntax surface than JavaScript/TypeScript, and formatting / linting matter more for an internationalization message format. Therefore, flat indexed tables are the primary representation.

### Make the Core Extensible Into a Toolchain

Existing dedicated parser toolchains show that it is effective to put parser, CST/AST, parser diagnostics, `SemanticModel` construction, and parser-owned semantic validation in the parser core, then place CLI, LSP, formatter, linter, and external toolchain integrations around it as product crates or adapters.

ox-mf2 follows the same direction. It puts the MF2-specific parser, CST, SemanticModel construction, parser diagnostics, and parser-owned semantic validation in `ox_mf2_parser`, while configurable lint rules, formatter behavior, CLI routing, and external toolchain integrations live in product crates or adapters. This keeps the parser core focused on MF2 while allowing extension to Node bindings, CLI, LSP, and various linter integrations.

### Binary AST Is Not the Initial Internal Representation

The Binary AST-style designs in ox-jsdoc and typescript-go are useful references for bindings, snapshots, persistence, and fast transfer.

However, ox-mf2 does not make Binary AST the first primary internal representation. The Phase 1 tool-facing syntax boundary is centered on NodeId, TokenId, Span, and accessors. This preserves the table boundary, avoids forcing the parser construction path to be Binary AST-first, and allows the public AST view to move to a Binary AST snapshot in Phase 2.

## Agreed Design Decisions

### Initial Responsibilities

ox-mf2 is a `toolchain foundation`.

The parser is implemented first, but token, span, accessor, and table boundary design are also included from the initial stage. This allows linting, formatting, and compilation to be added later.

### Syntax Tree

Adopt `Lossless CST + SemanticModel`.

```text
source
  -> lexer / token stream + trivia
  -> lossless CST
       -> formatter
       -> SemanticModel / SemanticView
            -> linter / compiler
```

The formatter primarily uses CST, tokens, and trivia. The linter and compiler primarily use the semantic model.

### Parser Error Handling

Adopt a `recovering parser`.

Even when syntax errors are found, the parser builds as much CST as possible and returns diagnostics. Downstream tooling that requires SemanticModel construction or parser-owned semantic validation must skip that work when parser diagnostics exist. Fatal gaps where even the root node cannot be built are API errors rather than partial semantic results.

The Phase 1 result shape, recovery behavior, and diagnostic cost model are defined in [002-ox-mf2-phase-1-rust-parser-design.md](./002-ox-mf2-phase-1-rust-parser-design.md).

### Internal Memory Representation

![ox-mf2 internal memory representation](./assets/001-ox-mf2-internal-memory-representation.svg)

Adopt `flat indexed tables`.

Core identifiers use stable `u32` indexes, and spans use UTF-8 byte offsets. The same identifier model is shared by construction-time CST tables, future Binary AST snapshots, diagnostics, formatters, linters, language bindings, and future SemanticView exposure.

Linters, formatters, and compilers do not depend directly on typed node structs. They read through NodeId and accessors.

Internal tables are snapshot-friendly. ox-mf2 avoids building a public typed AST first and then recursively converting it to Binary AST. Instead, the parser and SemanticModel construction paths generate table-oriented records so that SnapshotWriter can emit nodes, edges, tokens, trivia, inline span fields, strings, and diagnostics in a linear pass.

Phase 1 Rust tools may directly use construction-time flat indexed tables. From Phase 2 onward, the Binary AST decoder/accessor view shared by Rust, N-API, WASM, and other consumers becomes the canonical public AST view. This aligns the public AST surface across languages while allowing the parser to use efficient internal construction tables.

The Phase 1 table contract, identifier model, and source/span rules are defined in [002-ox-mf2-phase-1-rust-parser-design.md](./002-ox-mf2-phase-1-rust-parser-design.md). The Phase 2 Binary AST snapshot layout is defined in [003-ox-mf2-phase-2-binary-ast-snapshot-design.md](./003-ox-mf2-phase-2-binary-ast-snapshot-design.md).

### Formatter

Adopt a `preserve-capable formatter foundation`.

The formatter itself does not need to be part of the initial MVP. However, the parser/table layer prioritizes retaining tokens, trivia, original lexemes, delimiter spans, recovery nodes, and source-map-like information so that a preserve mode can be built later. This is a source-fidelity capability requirement for the syntax foundation, not the product's default formatting mode.

From Phase 2 onward, the formatter's public AST input is the Binary AST decoder/accessor view. The Rust implementation may have an internal fast path over construction-time tables when needed, but the stable public formatter surface is aligned with the Binary AST view shared by Rust, N-API, and WASM consumers.

A future formatter should support at least two modes. `standard` is the product default.

- preserve mode: preserve meaningful source shape where recoverable while still applying standard local spacing and normalization rules; it does not promise byte-identical output.
- standard mode: format to the standard ox-mf2 style.

### Linter

Adopt `diagnostics foundation`.

The initial Phase 3C linter is intentionally small, but it is not empty. It integrates parser diagnostics, parser-owned core semantic diagnostics, and the initial built-in configurable lint rules documented in [008-ox-mf2-phase-3c-linter-design.md](./008-ox-mf2-phase-3c-linter-design.md) and [linter-rules/index.md](./linter-rules/index.md).

The initial Phase 3C linter API is source-backed: `lintMessage(source, options)` parses, performs semantic validation, and runs enabled rules over one MF2 message. Initial built-in rules run through a Rust-internal `RuleContext` over CST access and parser-owned `SemanticModel` facts. The Binary AST decoder/accessor view remains the shared syntax foundation, and future SemanticView exposure remains the semantic foundation for bindings, editors, and future snapshot-backed linting. Snapshot-backed linting is deferred until the parser owns a snapshot-to-`SemanticModel` path.

Core diagnostics use SourceId and UTF-8 byte Span as the canonical location model. Labels also have byte spans. CLI, LSP, and editor integration are responsible for converting spans to line/column or UTF-16 positions through SourceStore.

Diagnostic ownership is split by source of truth. Parser diagnostics and parser-owned semantic diagnostics live in `ox_mf2_parser`; configurable lint rule diagnostics live in `intlify_lint`; reporter shaping and JSON envelopes belong to the CLI, binding, or lint result layer. These layers share the same location model and JSON-visible diagnostic code contract, but they do not redefine each other's detection semantics.

Parser diagnostics, SemanticModel foundations, and success-path parser cost constraints are defined in [002-ox-mf2-phase-1-rust-parser-design.md](./002-ox-mf2-phase-1-rust-parser-design.md). Parser-owned semantic diagnostics are defined in [012-ox-mf2-parser-semantic-validation-design.md](./012-ox-mf2-parser-semantic-validation-design.md). Linter result shape, rule diagnostics, reporter behavior, and binding contracts are defined in [008-ox-mf2-phase-3c-linter-design.md](./008-ox-mf2-phase-3c-linter-design.md).

### SemanticModel / SemanticView

![ox-mf2 semantic model and semantic view](./assets/001-ox-mf2-semantic-model-view.svg)

Adopt a `shared semantic model`.

This is not a low-level IR immediately before runtime execution. It is a semantic information model shared by the linter, compiler, and validation.

SemanticView is the planned public semantic surface once semantic APIs are exposed. Phase 2 bindings do not expose SemanticView yet; they expose the Binary AST syntax snapshot and parser diagnostics only. Semantic facts are linked to Binary AST NodeId and Span. Semantic information is not forced into the initial Binary AST snapshot. Binary AST handles the lossless CST surface, while SemanticView handles semantic facts such as declarations, references, selectors, variants, and fallback/default information.

Candidate contents:

- symbol table
- variable declarations
- variable references
- function annotations
- selector list
- variant matrix
- fallback/default variant
- source span mapping
- future duplicate key or coverage metadata if those facts become part of the shared semantic surface

### Unicode WG Interchange Data Model

Adopt `defer until compatibility surface is needed`.

The Unicode WG data model in `refers/message-format-wg/spec/data-model` is an interchange representation for messages. It is useful for cross-implementation compatibility, JSON/YAML exchange, legacy format conversion, and translation-format integration, but it is not required to be the internal representation of an implementation.

ox-mf2 therefore does not make the WG data model a mandatory Phase 1 or Phase 2 output. Phase 1 focuses on `CstTables + CstView + SemanticModel`, and Phase 2 focuses on the Binary AST snapshot and language binding boundary.

When a compatibility surface is needed, ox-mf2 can add a dedicated `DataModelView` / `InterchangeDataModel` API that exports and imports the WG data model from the existing semantic layer. The important constraint is that `SemanticModel` must retain enough information to derive the WG data model later, including message kind, declarations, selectors, variants, patterns, expressions, markup, options, attributes, and cooked literal values.

This keeps the hot parser and binding path compact while preserving a clear path for future compatibility APIs.

### Language Binding

![ox-mf2 language binding architecture](./assets/001-ox-mf2-language-binding.svg)

Adopt `Rust crates as the single MF2 behavior implementation`.

ox-mf2 does not reimplement MF2 behavior per target language. `ox_mf2_parser` owns MF2 parsing, CST construction, parser diagnostics, `SemanticModel` construction, and parser-owned semantic validation. Phase 3 product crates own product behavior: `intlify_format` owns formatter modes, layout, rendering, and formatter configuration; `intlify_lint` owns configurable lint rule execution, presets, lint configuration, and result shaping.

N-API, WASM, C ABI, and other language bindings are not required in the initial MVP. However, the Rust crate APIs are designed to be binding-friendly from the beginning.

Binding implementation priority:

1. N-API binding: the primary Node.js target for intlify and JavaScript tooling integration
2. WASM binding: the portable target for browsers, playgrounds, editor extensions, and edge runtime integration
3. C ABI binding design: the foundation for future Go, Swift, C#, Zig, Python FFI, and broader native language integration

Rust internal types are not directly exposed to other languages. Boundary types such as Binary AST decoder/accessor views, DiagnosticView, formatter results, lint results, and encoded snapshot views are allowed.

The binding layer is an ergonomic surface, not a place to duplicate MF2 behavior. JS, WASM, C ABI, Go, Swift, C#, and other consumers call the owning Rust crate for each product and receive stable views, handles, diagnostics, formatted text, lint results, or encoded snapshots as appropriate for that product boundary.

When returning full CST/AST output across a language boundary, the canonical Phase 2 product boundary is a versioned Binary AST snapshot, not a nested JSON AST. Debug JSON and compatibility JSON may exist, but they are not the standard hot-path representation.

The Phase 2 Binary AST snapshot focuses on the lossless CST surface. Semantic information can be exposed later as SemanticView or a later semantic snapshot; Phase 2 bindings expose syntax snapshots and parser diagnostics only. N-API and WASM bindings return result objects with lazy decoder/accessors rather than eagerly materialized object trees. Raw snapshot bytes are not included in the default result; they are available only through explicit advanced/debug/transport APIs.

MessagePack is not the CST/AST representation of ox-mf2. It is reserved as a future transport for long-lived language-service workflows such as LSP, editor integration, daemon mode, and repeated semantic queries.

The detailed design is split across [003-ox-mf2-phase-2-binary-ast-snapshot-design.md](./003-ox-mf2-phase-2-binary-ast-snapshot-design.md), [004-ox-mf2-phase-2-language-bindings-design.md](./004-ox-mf2-phase-2-language-bindings-design.md), and [005-ox-mf2-phase-3-tooling-transport-design.md](./005-ox-mf2-phase-3-tooling-transport-design.md).

### Parser API

Adopt `parse_source + SourceStore`.

SourceStore is the common source ownership layer for `parse_source`, batch parse, diagnostics, editor boundaries, and future snapshot roots sections. The `parse_message` convenience API creates a private one-entry `SourceStore` and returns it together with the syntax-only `ParseResult` as one `StandaloneParseResult`, so source slices, diagnostics, SemanticModel construction, and later snapshot encoding all retain the original source owner.

MF2 workloads often contain many messages in one file, one locale set, or one project, so batch parsing is a first-class API. Batch metadata such as path, locale, message_id, and base_offset is used for identity, diagnostics, fixtures, benchmarks, and future snapshot root mapping. It must not change parser semantics.

The Phase 1 parser APIs, SourceStore contract, ParseInput metadata, ParseOptions defaults, and result types are defined in [002-ox-mf2-phase-1-rust-parser-design.md](./002-ox-mf2-phase-1-rust-parser-design.md). Snapshot-producing APIs are defined in [003-ox-mf2-phase-2-binary-ast-snapshot-design.md](./003-ox-mf2-phase-2-binary-ast-snapshot-design.md).

### Suppression Model

Adopt `diagnostic suppression boundary only`.

At the initial stage, ox-mf2 does not define inline directive comments inside MF2. MF2 has no line or block comments, so comment-style disable directives would be a syntax extension rather than a parser or linter foundation feature.

Suppression is treated as a diagnostic-layer concern, not a parser syntax policy. Future suppression must be spec-compatible, such as baseline suppression files or resource/container-level metadata owned by a host format adapter. The linter product design tracks the future suppression model in [008-ox-mf2-phase-3c-linter-design.md](./008-ox-mf2-phase-3c-linter-design.md).

### Benchmark

Adopt `phase-separated benchmark`.

In addition to hyperfine measurement for the entire CLI, internal performance is made visible by phase.

Target phases:

- lexer
- parse_cst
- lower_semantic
- semantic_validation
- diagnostics
- encode_snapshot
- decode_snapshot
- snapshot_accessor_traversal
- snapshot_to_bytes_copy
- binding_call
- parse_batch_to_snapshot
- format_preserve
- format_standard
- e2e_parse
- e2e_lint

`lower_semantic` is kept as a compatibility benchmark phase name and means explicit SemanticModel construction over an already produced diagnostic-free source/result pair; it excludes parsing and parser-owned semantic validation. `e2e_lint` is kept as a broad legacy benchmark alias. Canonical Phase 3 linter benchmark names are defined in [005-ox-mf2-phase-3-tooling-transport-design.md](./005-ox-mf2-phase-3-tooling-transport-design.md) and [008-ox-mf2-phase-3c-linter-design.md](./008-ox-mf2-phase-3c-linter-design.md).

`snapshot_accessor_traversal` and `binding_call` are aggregate reporting families, not required standalone result series. The snapshot traversal family maps to the canonical `traverse_nodes`, `traverse_tokens`, and `traverse_diagnostics` series defined by the Binary AST design. The binding call family maps to `parse_message_binding`, `parse_batch_binding`, `decode_snapshot_binding`, and `snapshot_to_bytes_copy_binding` from the language bindings design. Reports use the detailed names for measurements and may group them under the aggregate family labels in summaries.

The Phase 1 parser / AST / performance design is detailed in [002-ox-mf2-phase-1-rust-parser-design.md](./002-ox-mf2-phase-1-rust-parser-design.md).

### Crate Structure

Adopt `parser core plus product and contract crates`.

The parser crate is the MF2 syntax and semantic foundation crate. Formatter, linter, CLI, resource-catalog, public message-artifact, language-specific producer, linker, and export behavior live in separate crates so each layer can own its configuration, result shaping, fixtures, and compatibility boundary without duplicating parser, host-adapter, link-analysis, or export-preparation behavior.

```text
crates/
  ox_mf2_parser     # parser, CST, parser diagnostics, snapshots, SemanticModel,
                    # and parser-owned semantic validation
  intlify_cli       # native intlify command shell and subcommand routing
  intlify_format    # formatter engine, formatter config, layout, and rendering
  intlify_lint      # linter engine, configurable rule execution, presets,
                    # lint config, rule registry, and result shaping
  intlify_resource  # host-format registry, catalog config, extraction, mapping,
                    # resource limits, and validated write-back re-escaping
  intlify_contract  # versioned message reference/definition artifacts, selector
                    # and domain contracts, wire compatibility, conformance
  intlify_producer_js # initial JS/TS reference production;
                      # Vue SFC reference production is a later phase;
                      # no link resolution
  intlify_linker    # language-neutral resolution, reachability, findings,
                    # bundle plans, and the M1 key-only typed-key model
  intlify_export    # shared export preparation, validated batches, exporter
                    # contract, common output artifacts, and built-in exporters
  intlify_producer_bin # later native/WASM tagged-reference scanning
```

`intlify_format` and `intlify_lint` depend on `ox_mf2_parser` instead of reimplementing parser, diagnostic, snapshot, or semantic validation logic. `intlify_resource` remains independent of the parser, formatter, linter, contract, and linker cores. It exposes complete extraction artifacts but neither imports `intlify_contract` nor aggregates files. `intlify_contract` likewise does not import `intlify_resource`; similarly named catalog-key, scope, locale, and entry-reference values remain boundary-specific checked types rather than shared Rust types.

For linker definition production, the `intlify_cli` project inventory enumerates logical catalog targets, inspects physical identity, and forms canonically ordered physical-source groups. A host-owned pre-extraction stage validates each already formed group's assignments and portable path/alias limits, `intlify_resource` extracts that physical source once, and a host-owned post-extraction projection performs checked conversion into one `MessageDefinitionArtifact`. Neither stage performs selector matching or reachability analysis.

`intlify_contract` owns the public, versioned interchange boundary. Language-specific producers emit that boundary, and `intlify_linker` consumes it without parsing source languages, catalog host formats, or depending on `intlify_lint`.

`intlify_export` owns the M3 shared export-preparation and exporter boundary. It depends on `intlify_linker`, `intlify_contract`, and `ox_mf2_parser`, validates the identity-deduplicated union of plan-selected delivery definitions and M1 baseline definitions required for typed signatures, derives any MF2 argument-signature information required by typed platform output, and exposes an opaque validated batch before any exporter invocation. The M1 linker model carries only resolved scope plus domain-qualified message-key identity; it never parses a payload or carries an MF2 argument signature. This keeps `intlify_linker` parser-independent while allowing non-CLI build integrations to reuse the same preparation, common output, error, and built-in exporter contracts.

`intlify_cli` composes the message-level, resource, producer, linker, and exporter workflows required by each command. Public distribution is handled through npm packages and language bindings where appropriate; workspace-internal crates do not imply crates.io publishing.

### Spec Tracking

Adopt `Unicode spec primary + TC39 proposal tracking`.

Primary source:

- `refers/message-format-wg/spec`
- `refers/message-format-wg/spec/data-model`

Tracked source:

- `refers/proposal-intl-messageformat`

MF2 syntax and the message data model primarily follow the Unicode WG spec. The data model is tracked as a future compatibility/export surface, not as the parser's primary internal representation. Intl.MessageFormat API integration and ECMAScript-side behavior track the TC39 proposal.

### Conformance Tests

Adopt `spec fixtures + implementation fixtures`.

```text
fixtures/
  spec/
    unicode-wg/
    tc39/
  implementations/
    formatjs/
    messageformat/
    mf2-tools/
    ox-content/
  recovery/
  formatter/
  diagnostics/
```

Spec fixtures are the basis for conformance checks, and implementation fixtures are used for compatibility and diff detection.

Spec fixtures are based on the Unicode Message Format WG spec and the TC39 proposal. Their purpose is to verify that ox-mf2 accepts MF2 that is valid under the spec and rejects syntax that is invalid under the spec. In other words, spec fixture results represent parser conformance.

Implementation fixtures are based on existing parser implementations and real-world messages. Their purpose is to observe differences between cases accepted/rejected by existing implementations and ox-mf2. They include edge cases, error recovery, MF1 compatibility cases, and real project messages. Implementation fixture results do not define spec conformance. They are treated as compatibility information, diff detection, and input for design decisions.

For example, spec fixtures can be structured as follows.

```text
fixtures/spec/unicode-wg/valid/local-declaration.mf2
fixtures/spec/unicode-wg/valid/matcher-select.mf2
fixtures/spec/unicode-wg/invalid/unclosed-expression.mf2
fixtures/spec/tc39/valid/intl-messageformat-api-options.mf2
```

Implementation fixtures can be structured as follows.

```text
fixtures/implementations/messageformat/accepted-edge-cases.mf2
fixtures/implementations/mf2-tools/error-recovery-cases.mf2
fixtures/implementations/formatjs/mf1-compat-cases.mf1
fixtures/implementations/ox-content/real-world-messages.mf2
```

Implementation fixtures are not a substitute for the specification. Even if another implementation accepts a message, ox-mf2 may reject it when it violates the Unicode WG spec or the TC39 proposal. The difference should still be recorded as compatibility input.

## Initial Architecture

![ox-mf2 initial architecture](./assets/001-ox-mf2-initial-architecture.svg)

## Table Boundary

The table boundary is the boundary between the internal table representation and tool-facing APIs.

![ox-mf2 table boundary](./assets/001-ox-mf2-table-boundary.svg)

This boundary allows linter, formatter, and compiler APIs to remain mostly stable even if the initial implementation uses flat indexed tables and the public AST view later moves to Binary AST-style compact tables.

## Phase Plan

### MVP / Phase 1

The first phase focuses on the parser foundation.

- lexer
- recovering CST parser
- diagnostics model
- conformance fixtures
- implementation fixtures for compatibility observation
- phase-separated benchmark
- parser performance design
- snapshot-friendly flat indexed tables
- Rust facade API with SourceStore / SourceId
- Rust batch parsing API shape

### Phase 2

The second phase adds the cross-language product boundary.

- versioned Binary AST snapshot
- SnapshotWriter
- lossless CST snapshot sections with required core sections for roots, sources, nodes, edges, tokens, string offsets, and string data, plus optional sections for trivia, diagnostics, diagnostic labels, source text data, and extended data
- spans are stored inline in NodeRecord, TokenRecord, TriviaRecord, and DiagnosticRecord, not in a separate section
- Rust Binary AST decoder / accessor API
- N-API binding with lazy decoder/accessor API
- WASM binding with portable decoder/accessor API
- first-class parseBatch API with shared snapshot buffer and shared string table
- semantic model can be exposed later as SemanticView or a future semantic snapshot
- C ABI design preparation without requiring a stable C ABI implementation
- snapshot encoding / decoding / binding benchmarks

### Phase 3

The third phase expands ox-mf2 into broader tooling workflows.

- formatter expansion
- linter expansion
- resource catalog extraction, mapping, and validated write-back
- language service / LSP model
- editor workflow cache and repeated query model
- versioned message reference and definition artifact contracts
- language-specific reference producers and language-neutral message linking
- deterministic linker findings, bundle plans, the M1 key-only typed-key model, export preparation, and platform build integration
- optional MessagePack transport for internal language-service sessions
- editor workflow benchmarks that separate parser, semantic, snapshot, transport, and binding costs

Phase 3 product numbering does not impose one serial implementation chain on every layered capability. The consumer-neutral resource foundation may proceed independently of the linter product as defined by 013. The 014 main linker track may start once its explicit 013 Tier 1 extraction, scope, locale-binding, and producer-binding prerequisites are available; it does not wait for `intlify_lint` or the LSP/editor product. Lint presentation and editor projection remain later consumers of linker findings, while native producers and platform build integrations follow the milestone dependencies defined by 014.

## Non-Goals

The initial stage does not implement the following.

- Binary AST-first internal representation in Phase 1
- Unicode WG data model import/export API unless a compatibility surface is required
- full linter ruleset
- standard formatter
- N-API / WASM binding
- MessagePack transport
- complete Intl.MessageFormat runtime

However, the initial design includes the boundaries required to add these later.
