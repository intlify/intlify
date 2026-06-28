# ox-mf2 Phase 3 Tooling and Transport Design

## Purpose

This document defines the Phase 3 design boundary for tooling and transport workflows around ox-mf2.

Phase 1 parser design is defined in [002-ox-mf2-phase-1-rust-parser-design.md](./002-ox-mf2-phase-1-rust-parser-design.md). Phase 2 Binary AST snapshot design is defined in [003-ox-mf2-phase-2-binary-ast-snapshot-design.md](./003-ox-mf2-phase-2-binary-ast-snapshot-design.md). Phase 2 language binding design is defined in [004-ox-mf2-phase-2-language-bindings-design.md](./004-ox-mf2-phase-2-language-bindings-design.md).

This document focuses on formatter/linter input, SemanticView exposure, LSP/editor workflows, agent coding workflows, transport choices, and long-lived language-service scenarios.

## Basic Policy

The standard CST/AST product boundary remains the versioned Binary AST snapshot. Tooling may use Rust-internal construction-time tables for fast paths, but public cross-language tooling input should converge on the Binary AST decoder/accessor view.

Semantic information is exposed separately as SemanticView or a later compact semantic snapshot. It is not forced into the lossless Binary AST snapshot.

MessagePack is not the CST/AST representation of ox-mf2. It is reserved as a future transport for long-lived language-service workflows.

## SnapshotView

Phase 3 does not introduce a second public AST view. The existing Binary AST `SnapshotView` / binding-side snapshot accessor remains the common syntax input for formatter, linter, LSP/editor, and transport consumers.

`SnapshotView` is already defined by the Phase 2 Binary AST snapshot design as a lazy decoder/accessor over versioned snapshot bytes. Phase 3 extends that contract at the consumer-requirements level rather than replacing it with a recursive object AST.

Core `SnapshotView` requirements:

- root, node, token, trivia, source, and diagnostic access by snapshot-local id
- child traversal in source order without materializing a recursive tree
- node, token, trivia, and diagnostic spans as UTF-8 byte ranges
- token leading/trailing trivia access
- source metadata and source text slicing through embedded or externally supplied source text
- stable raw snapshot bytes for persistence, worker transfer, and cross-process transport

Tooling-facing helpers may be layered on top of this core accessor when they can be derived from snapshot records and source text.

- raw token and trivia text helpers
- root/source-aware token stream traversal
- delimiter and keyword token lookup for formatter rules
- single-line / multi-line shape checks
- blank-line and line-break checks between adjacent syntax records
- byte-offset range lookup for editor requests
- UTF-8 byte span to UTF-16 editor position conversion at the editor boundary

These helpers should not become a second AST format. If a helper cannot be derived from the Binary AST snapshot plus source text, that is a signal to either extend the snapshot format deliberately or keep the feature in a Rust-internal fast path until the public product boundary is clarified.

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

### Product Surface

Phase 3 should provide a dedicated formatter engine and reusable formatter entry points for Rust, N-API, and WASM consumers. The CLI is the primary user-facing workflow, while N-API and WASM bindings allow the same formatter engine to power playgrounds, editor integrations, workers, and Node-based tools.

### Crate Ownership

The Rust formatter engine should live in a separate `crates/ox_mf2_format` crate that depends on `ox_mf2_parser`. The parser crate remains responsible for CST construction, parser diagnostics, Binary AST snapshots, and semantic lowering. The formatter crate owns formatting modes, formatter configuration, layout construction, rendering, formatter result shaping, and formatter-specific suppression behavior.

### CLI and Package Distribution

The CLI binary should live in `crates/ox_mf2_cli` and expose `ox-mf2 format` alongside `ox-mf2 lint`. Distribution should happen through npm packages that expose the compiled native CLI binary, so JavaScript users can install and run the Rust CLI through the npm ecosystem.

N-API and WASM formatter bindings should be published as formatter-specific packages rather than added to the existing parser binding packages. Parser bindings stay focused on parsing, snapshots, and parser-level APIs, while formatter packages expose formatter-specific APIs backed by `crates/ox_mf2_format`.

The npm distribution surface should separate the CLI package from formatter API packages. A CLI package owns native binary distribution for command-line use, a formatter N-API package exposes Node APIs, and a formatter WASM package supports browser, worker, and playground use cases. Exact package names are formatter-specific design details.

The Phase 3 formatter deliverables are the Rust formatter engine, CLI command, N-API formatter package, WASM formatter package, JSON configuration contract, generated JSON Schema, and formatter result contract. LSP/editor integration, playground usage, and resource/catalog formatting are consumers or layered workflows rather than separate direct products in this phase.

### Public Syntax Input

From Phase 2 onward, public AST input for formatter APIs is the Binary AST decoder/accessor view.

Formatter implementation may have Rust-internal fast paths over construction-time tables when formatting immediately after parse. However, stable public formatter input is the Binary AST view shared by Rust, N-API, WASM, and later consumers.

The primary public API is `formatMessage(source, options?)`, which parses one MF2 message and returns a formatter result. A snapshot-based entry point such as `formatSnapshot(snapshot, source?, options?)` is an advanced parse-artifact reuse path for playgrounds, workers, and language-service caches that already hold a Binary AST snapshot. Source text is still needed for preserve mode, source slicing, parser diagnostics, and editor position conversion. Binding packages should expose direct programmatic formatter APIs rather than a CLI callback bridge.

### Formatting Modes

The formatter should support at least two modes. Detailed formatter rules, API shape, fixtures, and implementation requirements are tracked separately in [ox-mf2-formatter-design.md](./ox-mf2-formatter-design.md). This document only fixes the Phase 3 boundary and high-level formatter direction.

- standard mode: format to the standard ox-mf2 style without using the original layout as a primary decision input
- preserve mode: preserve source shape where it is meaningful while still applying standard local formatting rules

Standard mode is a deterministic pretty-printer over the public syntax view.

Preserve mode is source-shape-sensitive pretty formatting. It may preserve single-line / multi-line choices, blank-line grouping, quote or literal spelling, and comment/trivia placement when those choices are recoverable from tokens, trivia, delimiter spans, and source slices. It should still normalize local spacing, indentation, and other standard formatting rules.

### Message-Level Formatting

The formatter core formats one whole MF2 message at a time. Range-only formatting is outside the initial formatter core. LSP/editor adapters should call whole-message formatting, compare the original and formatted message, and produce editor `TextEdit` values at the integration boundary. Minimal-diff edit computation belongs to the editor/resource adapter, not the formatter core.

### Layout Architecture

The formatter should separate syntax traversal from rendering. Internally, it should have a layout model capable of delayed line/group/indent decisions so line width, standard mode, preserve mode, and future resource/catalog adapters can reuse one message-level formatter core. The concrete IR/document implementation is a formatter-specific design detail.

### CLI Input Model

The CLI accepts file path and glob inputs so users can format MF2 files directly, for example `ox-mf2 format "locales/**/*.mf2"` or `ox-mf2 format messages/en.mf2`. The primary CLI input unit is a single MF2 message file: one file contains one MF2 message that can be parsed and formatted directly. Resource files containing multiple messages, framework-specific i18n files, and multi-locale catalogs are layered consumers that extract message entries and reuse the message-level formatter; their host-file parsing, string escaping, and outer document edits are not fixed by the Phase 3 core formatter contract.

### CLI Write and Check Workflows

`ox-mf2 format` should default to write mode and modify files in place. It should also support check workflows such as `--check` and `--list-different` for CI and pre-commit usage. Stdin formatting should be supported through a file-aware option such as `--stdin-filepath`, allowing editors and scripts to pipe source while still giving the formatter enough context for extension checks and configuration.

### Formatter Parallelism

The CLI may format multiple files in parallel, but observable behavior must remain deterministic. File discovery should normalize and de-duplicate paths before formatting so write mode never races on the same file when overlapping globs or repeated paths are provided.

Text output, `--check`, and `--list-different` results should be reported in stable normalized path order, independent of worker scheduling. Programmatic APIs such as `formatMessage(source, options?)` remain single-message operations; resource/catalog adapters and CLI workflows decide whether to parallelize multiple message entries.

Benchmarks should report formatter concurrency settings separately from parser, syntax traversal, layout construction, rendering, binding, and file I/O costs.

### Configuration

The formatter should load JSON project configuration. Formatter and linter configuration are separate responsibility areas, but they should be sections of one ox-mf2 tooling config so the CLI can resolve `format` and `lint` settings from the same root project configuration. The initial config discovery model is intentionally simple: only the repository root config is loaded. Nearest-config-wins and nested config discovery are out of scope until a concrete multi-workspace need appears.

The JSON configuration surface should have a generated JSON Schema published with the npm packages. This schema is part of the tooling contract for editor completion and config validation, while the Rust config model remains the source of truth.

Formatter configuration should support `ignorePatterns` but not file-specific `overrides` in the initial design. The initial formatter target is a narrow MF2 message file/resource workflow, so file-kind-specific overrides are unnecessary. If future resource/catalog or multi-file-kind workflows need per-file options, overrides can be reconsidered then.

### EditorConfig

The formatter should read `.editorconfig` as formatter-only fallback input for unset formatting options. The linter should not read `.editorconfig`.

### Invalid Syntax

Formatter behavior for invalid syntax is strict in the initial design. If parsing produces any parser diagnostic, the formatter does not produce public formatted output. CLI write mode must not modify the file. API consumers should receive the original source with diagnostics or an error result. LSP/editor adapters should treat incomplete or invalid editing buffers as no-op formatting requests. Recovery-aware formatting is future editor-specific scope.

### Formatter Results

Formatter results are distinct from linter diagnostic results, but parser diagnostic locations should share the core SourceId and UTF-8 byte Span location model. CLI, N-API, WASM, and LSP/editor integrations can derive line/column or UTF-16 positions as needed. Formatter-specific diagnostics may be added later under a formatter category, but linter rule ids and severity configuration should not be mixed into formatter results.

### Formatter Ignore Directives

Formatter ignore directives are in scope for the initial formatter. A formatter directive such as an `ox-mf2-ignore`-style comment should suppress formatting for a syntax unit and cause that unit's source slice to be emitted verbatim. Exact directive syntax, target range rules, and comment/trivia interactions belong to the formatter-specific design.

### Formatter and Linter Boundary

The Phase 3 responsibility boundary is style in the formatter and correctness in the linter. Formatting checks should use `ox-mf2 format --check` or formatter check APIs. If a future linter workflow needs style diagnostics or style fixes, it should delegate to formatter APIs rather than reimplement formatter layout rules in lint rules.

### Dedicated Formatter Design Notes

Formatter detail notes to resolve in the dedicated formatter design:

- exact Rust, N-API, and WASM result types
- formatter options and defaults
- formatter config file name and schema shape
- formatter ignore directive syntax and target range rules
- line wrapping rules
- matcher layout rules
- formatter fixture and idempotency requirements
- native package lazy-loading and config helper behavior
- required SnapshotView helper priority

### Formatter Benchmarks

Formatter output should measure parser, snapshot encode/decode/access, syntax traversal, layout construction, rendering, binding calls, and CLI end-to-end cost separately.

## Linter Input

### Product Surface

Phase 3 should provide a dedicated lint CLI and reusable linter entry points for Rust, N-API, and WASM consumers. The CLI is the primary user-facing workflow, while N-API and WASM bindings allow the same linter engine to power playgrounds, editor integrations, and Node-based tools.

### Crate Ownership

The Rust linter engine should live in a separate `crates/ox_mf2_lint` crate that depends on `ox_mf2_parser`. The parser crate remains responsible for CST, diagnostics, snapshots, and semantic lowering, while the lint crate owns rule execution, presets, lint configuration, and lint result shaping.

### CLI and Package Distribution

The CLI binary should live in a separate `crates/ox_mf2_cli` crate. That crate composes parser, formatter, and linter crates into user-facing commands such as `ox-mf2 lint`. Distribution should happen through npm packages that expose the compiled native CLI binary, so JavaScript users can install and run the Rust CLI through the npm ecosystem.

N-API and WASM linter bindings should be published as linter-specific packages rather than added to the existing parser binding packages. Parser bindings stay focused on parsing, snapshots, and parser-level APIs, while linter packages expose lint-specific APIs backed by `crates/ox_mf2_lint`.

The npm distribution surface should separate the CLI package from linter API packages. A CLI package owns native binary distribution for command-line use, a linter N-API package exposes Node APIs, and a linter WASM package supports browser, worker, and playground use cases. Exact package names are linter-specific design details.

The Phase 3 linter deliverables are the Rust linter engine, CLI, N-API linter package, WASM linter package, and shared diagnostic result schema. LSP/editor integration and playground usage are consumers of those deliverables rather than separate direct products in this phase.

### CLI Input Model

The CLI accepts file path and glob inputs so users can lint MF2 files directly, for example `ox-mf2 lint "locales/**/*.mf2"` or `ox-mf2 lint messages/en.mf2 messages/ja.mf2`.

The primary CLI input unit is a single MF2 message file: one file contains one MF2 message that can be parsed and linted directly. Resource files containing multiple messages, framework-specific i18n files, and multi-locale catalogs are layered consumers that extract message entries and reuse the message-level linter; their concrete file formats are not fixed by the Phase 3 core linter contract.

### Configuration

The CLI should load JSON project configuration for rule severity, presets, and file include/exclude patterns. Formatter and linter configuration are separate responsibility areas, but they should be sections of one ox-mf2 tooling config so the CLI can resolve `format` and `lint` settings from the same root project configuration. The initial config discovery model is intentionally simple: only the repository root config is loaded. Nearest-config-wins and nested config discovery are out of scope until a concrete multi-workspace need appears. `crates/ox_mf2_lint` is the source of truth for the resolved lint config model, rule registry, preset expansion, defaults, and validation so the CLI, N-API, and WASM entry points share the same behavior.

The JSON configuration surface should have a generated JSON Schema published with the npm packages. This schema is part of the tooling contract for editor completion and config validation, while the Rust config model remains the source of truth. Linter configuration should not support file-specific `overrides` in the initial design; file selection belongs to include/exclude and ignore patterns. Resource/catalog linting can revisit this if per-resource configuration becomes necessary.

### Presets

The initial linter preset should be a `recommended`-style preset focused on broadly useful correctness diagnostics. Stricter, nursery, experimental, and resource/catalog-oriented presets are future or linter-specific design details rather than Phase 3 core requirements.

While the linter remains in 0.x, the `recommended` preset may evolve by adding broadly useful correctness diagnostics as needed. Preset stability policy should be finalized before a 1.0 release.

### CLI Output and Exit Behavior

The initial CLI output formats should include human-readable `text` output and machine-readable `json` output. Additional formats can be added later, but `json` should use the same diagnostic schema exposed by Rust, N-API, and WASM entry points.

The CLI exits with a failure status when any `error` diagnostic is reported. `warning` diagnostics do not fail the process by default. A `--max-warnings <n>` option should allow CI users to fail when the warning count exceeds the configured threshold.

Detailed CLI option semantics, including quiet mode, fix mode, config file name/schema details, ignore/include behavior, unmatched-pattern behavior, and additional output formats, belong to the linter-specific design document rather than this Phase 3 consumer contract.

### Public Syntax and Semantic Input

From Phase 2 onward, public AST input for linter APIs is the Binary AST decoder/accessor view plus optional SemanticView.

Rule implementations may use Rust-internal semantic fast paths, but rule-facing / binding-facing traversal should converge on the same public Binary AST view whenever practical.

For N-API and WASM consumers, the primary public entry point is `lintMessage(source, options?)`. A snapshot-based entry point such as `lintSnapshot(snapshot, source?, options?)` is an advanced parse-artifact reuse path for playgrounds, workers, and language-service caches that already hold a Binary AST snapshot. The source text or SourceStore-equivalent context is still needed whenever consumers require line/column, UTF-16 positions, or source-slice-aware diagnostics. Binding packages should expose direct programmatic lint APIs rather than a CLI callback bridge or plugin host.

### Location Model

Core diagnostics use SourceId and UTF-8 byte Span as the canonical location model. CLI, LSP, and editor integrations convert spans to line/column or UTF-16 positions through SourceStore or SourceView.

### Diagnostic Result Contract

CLI JSON output, Rust results, N-API results, WASM results, and LSP bridges should share one diagnostic result contract. The exact serialized schema belongs to the linter-specific design, but the shared contract includes:

- result grouping by file or message entry
- diagnostics with `parser`, `semantic`, or `lint` category
- `error` or `warning` severity
- UTF-8 byte span as the canonical location
- optional derived line/column or UTF-16 positions for CLI/editor consumers
- stable rule id for configurable lint diagnostics
- stable diagnostic code for parser and semantic diagnostics
- summary counts such as `errorCount` and `warningCount`

### Stable Identifiers and Rule Metadata

Semantic diagnostic codes and configurable lint rule ids are public stable identifiers because configs, suppressions, JSON output, editor integrations, and external tooling may depend on them. Human-readable diagnostic message text is not a stable compatibility surface and may change for clarity.

The lint crate should expose rule metadata for CLI, bindings, generated docs, and JSON Schema generation. Metadata includes at least rule id, category, default/recommended status, default severity, fix capability, documentation link or docs slug, and rule option schema when a rule accepts options. Exact metadata fields are linter-specific design details.

### Operational Errors

The linter distinguishes lint diagnostics from operational errors. Parser, semantic, and rule diagnostics belong to lint results. Configuration parse errors, invalid config shape, file system errors, snapshot version mismatches, and internal failures are CLI/API errors and should not be mixed into normal lint diagnostics.

### Parallelism

The CLI may lint multiple files in parallel, but output must be deterministic. Text and JSON output should order file results by a stable normalized path order, independent of worker scheduling. Benchmarks should report concurrency settings separately from parser, semantic, rule, binding, and serialization costs.

### Message and Catalog Scope

The linter should support message-level linting first and allow resource/catalog-level linting to be layered on top. Catalog-level linting represents i18n resource validation across a locale/message collection, while the message-level core remains reusable by bindings and tools.

### File Discovery

CLI file discovery should use an explicit supported-extension list owned by the linter/CLI crates. Unsupported files and unmatched patterns are CLI input conditions rather than parser diagnostics. Detailed file discovery, ignore, and unmatched-pattern semantics should be resolved as open questions in the linter-specific design document.

### Lint Pipeline

`lintMessage(source)` should parse the message, perform semantic lowering as needed, run enabled rules, and return parser, semantic, and lint diagnostics in one result. Parser diagnostics are always included in the lint result, even when no lint rules run, so CLI and editor users can treat syntax failures as lint failures.

Diagnostics should identify their source category and rule id when applicable. Parser diagnostics use `category: "parser"` and no rule id, or a reserved parser rule id if a specific output format requires one. Parser diagnostics are independent of rule configuration and are emitted as `error` in the initial design, including recoverable syntax errors. `warning` is reserved for future compatibility or deprecation-style parser diagnostics.

If parsing produces any parser diagnostics, the initial linter stops before semantic lowering and rule execution. This keeps rule implementations from depending on incomplete recovery AST shapes. A future recovery-aware editor mode may run selected rules on partial syntax, but that is outside the initial linter core.

Semantic diagnostics are also always included in `lintMessage(source)` after successful parsing. They use `category: "semantic"` and no rule id, or a reserved semantic rule id if an output format requires one. These diagnostics represent MF2 meaning errors rather than configurable lint rules. Initial semantic diagnostics are emitted as `error`; `warning` is reserved for future best-practice or ambiguous-but-valid semantic diagnostics.

If semantic lowering produces any semantic diagnostics, configurable lint rules do not run. The initial linter pipeline is strictly `parser -> semantic -> rules`, and each stage must complete without diagnostics before the next stage runs.

### Severity

Rule configuration uses an ESLint/oxlint-style severity state:

- `off`: disable the rule
- `warn`: report diagnostics as warnings
- `error`: report diagnostics as errors

Emitted linter diagnostics initially use only `warning` and `error`. `off` is a rule configuration state, not an emitted diagnostic severity. `info` and `hint` are reserved for LSP/editor or advice-style layers, not for the initial linter core.

### Fixes and Formatter Boundary

Initial rules should be Rust core built-ins. JavaScript custom rules and linter plugins are out of scope. Auto-fix is also out of scope initially; future style fixes should delegate to the formatter API/crate so formatting behavior stays consistent between formatter and linter integrations.

The Phase 3 responsibility boundary is correctness in the linter and style in the formatter. Formatting style diagnostics are not part of the initial linter core. If a future lint workflow needs formatting checks or style fixes, it should call formatter check/format APIs rather than duplicate formatting logic in lint rules. Non-style lint fixes, if added later, should remain semantic-safe and independent from formatter output.

### Initial Semantic Diagnostic Candidates

The initial semantic diagnostic candidates are:

- `undefined-variable`
- `duplicate-declaration`
- `invalid-local-dependency`
- `variant-key-arity-mismatch`
- `missing-fallback-variant`
- `duplicate-variant`
- `unreachable-variant`
- `semantic-lowering-failed`

### Detailed Linter Design Reference

Detailed rule semantics, examples, and implementation contracts should be specified in [ox-mf2-linter-design.md](./ox-mf2-linter-design.md). This phase document only fixes the consumer-facing pipeline and initial scope.

### Suppression

Suppression and directive comments are diagnostic-layer concerns. This document does not fix a concrete directive comment syntax inside MF2. A future linter design can define the suppression data shape when rule execution enters implementation.

### Phase 3 Linter Scope

Phase 3 linter core scope:

- Rust linter engine in `crates/ox_mf2_lint`
- CLI in `crates/ox_mf2_cli`
- npm-distributed native CLI package
- linter-specific N-API package
- linter-specific WASM package
- JSON project configuration
- generated JSON Schema for configuration
- shared diagnostic result contract
- rule metadata and rule listing/introspection surface
- message-level linting for single MF2 message files
- `recommended` preset
- parser and semantic diagnostics integrated into lint results

### Future or Layered Linter Scope

Future or layered linter scope:

- resource/catalog linting
- nested config discovery
- recovery-aware editor linting
- suppression directive syntax
- `lint --fix`
- LSP/editor as a direct product
- output formats beyond `text` and `json`

### Out-of-Scope Linter Features

Out-of-scope linter features:

- JavaScript custom rules
- linter plugin system

## LSP and Editor Workflow

### Product Boundary

Phase 3 does not require a dedicated LSP server or editor extension as a direct product. Instead, LSP and editor integrations are treated as adapter workflows built on top of the parser, formatter, linter, `SnapshotView`, `SemanticView`, and binding packages.

The parser, formatter, and linter cores remain LSP-agnostic. They should not return LSP protocol types such as `Diagnostic`, `TextEdit`, `CodeAction`, or UTF-16 positions directly.

### Initial Scope

The initial editor workflow focuses on diagnostics and formatting.

Code actions, quick fixes, hover, completion, go-to-definition, rename, true range-only formatting, and minimal-diff formatting are not required in the initial workflow. `SemanticView` should still preserve enough stable semantic relationships to support those future editor features.

### Document and Message Mapping

Editor adapters should support both standalone `.mf2` documents and MF2 messages embedded in JSON/YAML resource or catalog files.

For `.mf2` files, the adapter may treat the whole document as one message or resource unit. For JSON/YAML resources, the adapter extracts each MF2 message from the relevant key/value entry and tracks the relationship between:

- document URI and version
- resource or catalog key
- document-level value range
- extracted message text
- message-local byte offsets

Parser, formatter, and linter core APIs operate on the extracted message text. Adapters map message-local results back to the containing document.

Host document parsing, string escaping, decoded-to-raw offset mapping, and outer document edit ownership are adapter concerns. Their exact contracts should be specified in a dedicated LSP/editor or resource adapter design.

### Span and Position Conversion

Core parser, snapshot, semantic, formatter, and linter APIs use message-local UTF-8 byte spans as their canonical location model.

LSP and editor adapters are responsible for:

- mapping message-local UTF-8 spans to document-level UTF-8 spans
- converting document-level UTF-8 spans to editor-facing UTF-16 positions
- preserving source identity through `SourceStore` / `SourceView` or equivalent adapter state

This keeps JSON/YAML parsing, document URI handling, and LSP position encoding outside of the core crates and bindings.

### Artifact Reuse

Long-lived language-service workflows may reuse parse artifacts per document version to avoid re-parsing and re-encoding on every request. They can combine:

- `SourceStore` / `SourceView` for source identity and location conversion
- binary AST snapshot or decoded `SnapshotView` for syntax traversal
- `SemanticView` for semantic queries
- diagnostics store for parser, semantic, and linter diagnostics

Cached artifacts must be invalidated when the document version changes. Cache ownership and eviction are adapter concerns, not parser, formatter, or linter core responsibilities. Detailed parse artifact cache policy belongs in `design/ox-mf2-parse-artifact-cache.md`.

### Diagnostics Workflow

Editor diagnostics are produced by combining parser, semantic, and linter diagnostics through the shared diagnostic result contract.

When an adapter uses `lintMessage` or `lintSnapshot`, that result should be treated as the preferred diagnostic source because it already contains parser, semantic, and lint diagnostics. Adapters must avoid publishing parser diagnostics twice when they also keep parser results in a separate cache.

The initial workflow is strict:

- parser diagnostics are always reported
- semantic diagnostics are reported when parse recovery provides enough structure for semantic lowering
- linter diagnostics are reported only when parsing and semantic lowering succeed
- parse failure prevents the linter from running

Shared `error` and `warning` severities map to editor/LSP diagnostic severity at the adapter boundary. The core linter does not emit `info` or `hint` diagnostics initially, but editor layers may add advice-style diagnostics on top of the shared results.

Recovery-aware partial linting for incomplete editor buffers is a future editor-mode concern. The initial editor workflow keeps the same strict `parser -> semantic -> rules` pipeline used by CLI and bindings.

### Formatting Workflow

Formatter core APIs format whole MF2 messages and return formatted message text. They do not return LSP `TextEdit` values directly.

Editor adapters should:

1. find the containing MF2 message or resource entry
2. call whole-message formatting
3. compare the original and formatted message text
4. create editor `TextEdit` values at the adapter boundary

For standalone `.mf2` documents, the resulting edit may replace the whole document. For JSON/YAML resources, the edit should replace only the containing message value range.

If a format request contains a selected range, the initial workflow formats the containing MF2 message rather than performing true range-only formatting. When the message has parse errors, editor formatting should no-op instead of returning partially formatted output.

Editor adapters should only return `TextEdit` values when the document version and message mapping used to create the edit still match the current document. If the mapping is stale or the containing message range can no longer be identified, the adapter should no-op. Exact version checks and edit conflict behavior belong in the dedicated LSP/editor design.

### Configuration

Editor adapters should normalize their settings into the same resolved formatter and linter configuration models used by CLI workflows. They may combine project configuration with editor-specific settings such as workspace settings, user settings, or LSP initialization options before passing options to core APIs.

Configuration loading failures are operational editor errors, not parser, semantic, formatter, or linter diagnostics. Exact config sources, precedence, reload behavior, fallback behavior, and editor error presentation belong in dedicated formatter, linter, or LSP/editor design documents.

### Out-of-Scope Editor Features

The following features are deferred from the initial Phase 3 editor workflow:

- code actions and quick fixes
- hover, completion, go-to-definition, and rename
- true range-only formatting
- minimal-diff formatting
- recovery-aware partial linting for incomplete buffers
- dedicated LSP server CLI, protocol handlers, and extension packaging

Future editor adapters may map stable linter rule ids and formatter output into quick fixes. Future semantic editor features should build on `SemanticView` rather than requiring LSP-specific semantic state in the parser core.

### Implementation Targets

Different integration environments can use the same conceptual workflow through different implementation targets:

- Rust LSP servers can call parser, formatter, and linter crates directly
- Node-based language servers or editor extensions can use N-API packages
- browser-based editors and playgrounds can use WASM packages

The transport or binding layer is selected by the integration environment. The core workflow remains the same across these targets.

## Agent Coding Workflow

Agent coding tools such as Codex, Claude Code, Grok Build, and similar systems are separate consumers from LSP/editor integrations. They may expose plugins, skills, commands, hooks, MCP servers, ACP clients, headless execution, or other agent-specific extension points, but those extension systems should wrap the same formatter, linter, parser, and snapshot contracts rather than defining new core behavior.

The initial Phase 3 agent-facing surface should be the `ox-mf2` CLI and stable machine-readable output. Agents can call `ox-mf2 format`, `ox-mf2 lint`, and future check commands in repo workflows, CI-style verification, pre-commit automation, and code review tasks.

Agent integrations may later provide MCP servers, agent plugins, skills, or commands, but those should remain distribution and workflow wrappers. They should not become the source of truth for formatting rules, lint diagnostics, configuration semantics, AST structure, or semantic analysis.

Detailed agent integration choices are tracked in [ox-mf2-agent-integration-design.md](./ox-mf2-agent-integration-design.md).

## MessagePack Transport

MessagePack is not the CST/AST representation of ox-mf2.

It is a future transport candidate for long-lived language-service workflows such as LSP, editor integration, daemon mode, and repeated semantic queries. The standard CST/AST product boundary remains the versioned Binary AST snapshot.

If MessagePack transport is added later, its overhead must be measured separately from parser, semantic lowering, snapshot encoding, snapshot decoding, binding cost, and LSP request handling.

MessagePack payloads should transport query/response data or language-service session messages. They should not become a second AST format that competes with the Binary AST snapshot.

Linter results should be transportable over JSON-RPC or a future MessagePack session using the shared diagnostic result contract. Transport payloads may carry source text, Binary AST snapshot bytes, or diagnostic results depending on the consumer, but the transport layer must not redefine lint diagnostics or AST structure. Benchmarks must keep parse, semantic lowering, rule execution, snapshot encode/decode, diagnostic serialization, and transport overhead as separate phases.

## Benchmarks

Tooling and transport benchmarks must be phase-separated.

Relevant Phase 3 benchmark phases:

- format_preserve
- format_standard
- lint_message_core
- lint_snapshot_core
- lint_cli_e2e
- lint_binding_napi
- lint_binding_wasm
- lint_lsp_diagnostics
- semantic_query
- lsp_jsonrpc
- lsp_msgpack
- cache_hit_query
- cache_miss_parse
- e2e_format

Reports should separate parser, semantic lowering, snapshot encode/decode, binding calls, MessagePack transport, JSON-RPC transport, cache hit/miss behavior, and actual rule/formatter work.
