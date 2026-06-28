# ox-mf2 Phase 3B Formatter Design

## Purpose

This document tracks the detailed formatter design for ox-mf2.

The Phase 3 tooling boundary is defined in [005-ox-mf2-phase-3-tooling-transport-design.md](./005-ox-mf2-phase-3-tooling-transport-design.md). That document fixes the high-level product boundary. This document is the implementation-facing place to refine formatter modes, public APIs, options, fixtures, diagnostics policy, and required SnapshotView helpers.

## Goals

The formatter should provide a deterministic ox-mf2 style while keeping the public syntax input aligned with the Binary AST snapshot accessor model.

Primary goals:

- format MF2 messages through a Rust core implementation
- expose a dedicated `ox-mf2 format` CLI backed by the same core
- expose the formatter through Rust, N-API, and WASM without duplicating formatting logic
- use Binary AST `SnapshotView` / binding-side snapshot accessors as the stable public syntax view
- support both standard and preserve formatting modes
- provide a JSON configuration contract and generated JSON Schema
- support formatter ignore directives for selected syntax units
- keep parser, snapshot decode/access, formatting, and binding costs measurable separately
- preserve parse semantics and produce stable output when formatting succeeds

Non-goals for the first formatter design:

- range-only formatting
- minimal-diff editor formatting
- a second recursive public AST format
- nested config discovery
- file-specific config overrides
- resource/catalog host-file parsing, escaping, and outer edit ownership
- semantic rewriting, variable renaming, variant reordering, or fallback normalization

Range-only and minimal-diff formatting remain LSP/editor workflow concerns until editor requirements are defined.

## Formatter Modes

### Standard Mode

Standard mode is a deterministic pretty-printer over the public syntax view. It formats to the standard ox-mf2 style without using the original source layout as a primary decision input.

Standard mode should normalize:

- declaration spacing
- expression spacing
- function, markup, option, and attribute spacing
- indentation
- matcher layout
- final newline behavior

Style decisions that are independent of original source layout belong to standard mode. For example, matcher selectors and variant keys may use a table-like layout when that improves readability for multi-selector messages. Such decisions should be specified as ox-mf2 style rules, not as preservation behavior.

### Preserve Mode

Preserve mode is source-shape-sensitive pretty formatting. It still applies standard local formatting rules, but it may use original source shape to choose layout where that shape is meaningful.

Preserve mode may preserve:

- single-line / multi-line choices
- blank-line grouping
- quote or literal spelling
- comment/trivia placement
- delimiter-driven source shape when recoverable

Preserve mode should still normalize:

- local spacing around declarations and operators
- indentation
- required spaces between syntax elements
- final newline behavior, unless a later option explicitly controls it

Preserve mode is not a minimal-diff formatter. It may rewrite larger regions when the formatted shape follows ox-mf2 style rules.

## Public API Shape

The primary public API parses and formats one MF2 message:

```rust
format_message(source: &str, options: FormatOptions) -> FormatResult
check_format(source: &str, options: FormatOptions) -> FormatCheckResult
```

Bindings should expose the same shape with host naming conventions:

```ts
formatMessage(source: string, options?: FormatOptions): FormatResult
checkFormat(source: string, options?: FormatOptions): FormatCheckResult
```

The advanced API accepts an already-created Binary AST snapshot. This is for playgrounds, workers, and language-service caches that already hold parse artifacts:

```rust
format_snapshot(snapshot: SnapshotView<'_>, source: Option<&str>, options: FormatOptions) -> FormatResult
```

`source` is still needed for preserve mode, source slicing, parser diagnostics, and editor position conversion. Snapshot-backed formatting should therefore be treated as parse-artifact reuse, not as a source-free formatting mode.

Formatter results are separate from linter diagnostics. Parser diagnostic locations use the shared SourceId plus UTF-8 byte Span model, but formatter result objects should not contain lint severities or lint rule ids.

Open decisions for the detailed result contract:

- exact Rust result types
- exact N-API and WASM result object shape
- whether `checkFormat` returns a boolean only or also normalized formatted output
- whether formatter-specific diagnostics need a separate category in the first implementation

## CLI Workflow

The CLI command is `ox-mf2 format`.

Initial CLI behavior:

- write mode is the default
- `--check` reports whether files differ without writing
- `--list-different` prints paths that would change
- stdin is supported with a file-aware option such as `--stdin-filepath`
- path and glob inputs are accepted
- the primary input unit is `1 file = 1 MF2 message`

Resource files and catalogs that contain multiple messages are layered workflows. A resource/catalog adapter should parse the host file, extract message entries, call the message-level formatter core, and own host-file string escaping and outer document edits.

## Configuration

Formatter configuration lives in the `format` section of one ox-mf2 tooling config shared with lint configuration. The config format is JSON, and the Rust config model is the source of truth for generated JSON Schema.

Initial config discovery is root-only. The CLI loads only the repository root config. Nearest-config-wins and nested config discovery are not part of the initial design.

Initial formatter config supports `ignorePatterns` but not file-specific `overrides`. The first formatter target is a narrow MF2 message/resource workflow, so per-file option overrides are unnecessary until resource/catalog requirements prove otherwise.

The formatter reads `.editorconfig` as fallback input for unset formatting options. The linter does not read `.editorconfig`.

Open decisions for detailed configuration:

- exact config file name
- exact JSON Schema package path
- exact option names, defaults, and validation errors
- how CLI flags override config and `.editorconfig`

## Options

Initial options should stay small.

Candidate minimum:

```text
FormatOptions {
  mode: standard | preserve
}
```

With only `mode` in the minimum option set, `.editorconfig` has no active formatter effect yet because common `.editorconfig` fields map to later options such as line width, indent width, line ending, and final newline. `.editorconfig` fallback becomes active only for formatter options that are explicitly supported.

Options to decide later:

- line width
- indent width
- line ending
- final newline
- matcher table layout enablement
- quote/literal spelling policy in standard mode

Avoid adding style options until fixtures prove that the formatter needs them.

## Diagnostics Policy

Formatter behavior for invalid or partially recovered syntax is strict in the initial design.

If parsing produces any parser diagnostic:

- the formatter does not produce public formatted output
- CLI write mode does not modify the file
- API consumers receive diagnostics or an error result without formatted output
- LSP/editor adapters treat the request as a no-op

Recovery-aware formatting is future editor-specific scope.

Formatter fixtures must cover:

- no diagnostics
- parser diagnostics that must not produce public formatted output
- CLI write-mode no-op behavior on invalid syntax
- API result shape for invalid syntax

## Architecture

The formatter separates syntax traversal from rendering.

The message-level core should build an internal layout representation before rendering text. The layout model should support delayed line, group, and indent decisions so standard mode, preserve mode, line width, and future resource/catalog adapters can reuse one formatter core.

The exact IR/document implementation is intentionally left to implementation design. The public contract is that callers format whole MF2 messages and receive either formatted source/check information or diagnostics.

## Resource and Catalog Formatting

The formatter core formats one MF2 message. Resource/catalog formatting is layered above it.

A resource/catalog adapter is responsible for:

- parsing JSON, YAML, or framework-specific resource files
- locating message entries
- converting entry spans and text into message-level formatter input
- preserving or re-emitting host-file string escaping
- creating outer document edits

This keeps the first formatter core focused while still allowing the same formatter engine to support i18n resources later.

## LSP and Editor Workflow

The formatter core does not implement range-only or minimal-diff formatting initially.

Editor integrations should:

1. identify the message range to format
2. call whole-message formatting
3. compare original and formatted message text
4. produce the smallest practical editor `TextEdit` at the integration boundary

This keeps minimal edit calculation out of the formatter core and lets editors handle UTF-8 byte spans to UTF-16 positions at the boundary.

## Formatter Ignore Directives

The formatter should support an `ox-mf2-ignore`-style directive in the first implementation.

The directive suppresses formatting for a syntax unit and emits the original source slice for that unit. Exact directive syntax, target range selection, and comment/trivia interactions are open detailed-design items.

## Matcher Layout

Matcher layout needs a dedicated rule set because it affects readability and introduces alignment behavior.

Open decisions:

- whether multi-selector matchers always use a table-like layout
- whether single-selector matchers use one-line or multi-line variants
- whether variant keys align by column width
- whether selector names align with variant key columns
- how preserve mode handles existing matcher table shape
- how line wrapping interacts with long keys and long patterns

## Line Wrapping

Line wrapping is not yet specified.

Open decisions:

- default line width
- wrapping behavior for long expressions
- wrapping behavior for long quoted patterns
- wrapping behavior for long matcher variants
- whether preserve mode keeps original line breaks if all lines fit
- how to avoid changing semantic text in quoted patterns while wrapping

## SnapshotView Requirements

The formatter should first consume the existing Binary AST `SnapshotView` / binding-side accessor model.

Likely required helpers:

- raw token text
- raw trivia text
- root/source-aware token stream traversal
- delimiter and keyword token lookup
- single-line / multi-line checks
- blank-line and line-break checks between adjacent syntax records
- source slicing for node, token, and trivia spans

These helpers should be derived from snapshot records and source text when possible. If a formatter rule needs information that cannot be derived, the implementation should either use a Rust-internal fast path temporarily or propose an explicit snapshot format extension.

## Fixture Strategy

Formatter fixtures should be reviewable and stable.

Each fixture should include:

- input
- formatted standard output
- formatted preserve output when different
- parser diagnostics before formatting
- parser diagnostics after formatting
- idempotency assertion

Required assertions:

- formatting output is stable after a second format pass
- formatting preserves the parse tree shape for valid input
- formatting preserves semantic facts once SemanticView participates in formatter tests
- parser diagnostics do not produce public formatted output
- invalid syntax is not modified by CLI write mode

## Benchmarks

Formatter benchmarks should separate:

- parse cost
- snapshot encode cost
- snapshot decode/access cost
- syntax traversal cost
- layout construction cost
- rendering cost
- binding call cost
- CLI end-to-end format cost

Phase 3 benchmark names:

- `format_standard`
- `format_preserve`
- `e2e_format`

## Open Questions

The following items are detailed formatter design questions, not Phase 3 boundary decisions:

- exact `FormatResult`, `FormatCheckResult`, and error envelope shape for Rust, N-API, and WASM
- exact config file name, JSON Schema path, option names, and default values
- exact CLI flag precedence over config and `.editorconfig`
- whether the CLI supports an explicit `--config <path>` in addition to root-only discovery
- CLI exit code classification for format mismatch, formatter errors, config errors, and no matched files
- stdout/stderr behavior for write, check, list-different, stdin, and error-reporting modes
- whether the CLI exposes runtime controls such as `--threads`
- whether the CLI supports `--no-error-on-unmatched-pattern`
- supported file extensions for direct message files
- file discovery behavior for paths, globs, directories, duplicate matches, and deterministic ordering
- ignore file support, CLI exclude flags, and interaction with config `ignorePatterns`
- whether ignore files include `.gitignore`, a formatter-specific ignore file, explicit `--ignore-path`, or only config `ignorePatterns`
- unmatched pattern behavior
- symlink traversal, hidden files, VCS directories, and dependency directories such as `node_modules`
- whether `ox-mf2 init` should generate config and schema comments/examples
- exact formatter ignore directive syntax, target range rules, and trivia handling
- exact matcher layout and line wrapping rules
- LSP/editor configuration shape, including whether an explicit formatter config path is supported
- LSP/editor behavior when config loading fails, including whether it falls back to defaults or reports an operational error
- generated JSON Schema and generated TypeScript type distribution in npm packages
- fixture harness structure, including `options.json` matrices, snapshot format, and idempotency checks
- source text, token, trivia, and comment cursor responsibilities needed by preserve mode and ignore directives
- whether `formatMessage` returns `code + errors`, a Rust-style result envelope, or separate success/error variants
- native package lazy-loading and config helper behavior
- WASM bundle-size constraints and tree-shaking expectations
- whether the WASM API should expose synchronous formatting after initialization or use an asynchronous API shape
- how preserve mode should apply surrounding layout heuristics around syntax units emitted from original source slices by formatter ignore directives
