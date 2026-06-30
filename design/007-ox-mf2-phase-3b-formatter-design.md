# ox-mf2 Phase 3B Formatter Design

## Purpose

This document tracks the detailed formatter design for ox-mf2.

The Phase 3 tooling boundary is defined in [005-ox-mf2-phase-3-tooling-transport-design.md](./005-ox-mf2-phase-3-tooling-transport-design.md). That document fixes the high-level product boundary. This document is the implementation-facing place to refine formatter modes, public APIs, options, fixtures, diagnostics policy, and required SnapshotView helpers.

## Goals

The formatter should provide a deterministic ox-mf2 style while keeping the public syntax input aligned with the Binary AST snapshot accessor model.

Primary goals:

- format MF2 messages through a workspace-internal Rust core crate named `intlify_format`
- expose a dedicated `intlify fmt` CLI backed by the same core
- expose the formatter through `@intlify/format-napi` and `@intlify/format-wasm` without duplicating formatting logic
- use Binary AST `SnapshotView` / binding-side snapshot accessors as the stable public syntax view
- support both standard and preserve formatting modes
- provide a JSON configuration contract through the unified `@intlify/cli` config schema
- keep parser, snapshot decode/access, formatting, binding, and CLI costs measurable separately
- preserve parse semantics and produce stable output when formatting succeeds
- provide local-first formatter benchmark tooling under `tools/`

Non-goals for the first formatter design:

- range-only formatting
- minimal-diff editor formatting
- a second recursive public AST format
- nested config discovery
- file-specific config overrides
- resource/catalog host-file parsing, escaping, and outer edit ownership
- semantic rewriting, variable renaming, variant reordering, or fallback normalization
- formatter ignore directives or MF2 syntax extensions
- line wrapping and style options beyond `mode`
- `.editorconfig` loading before formatter options exist that can consume it
- generated TypeScript config type distribution
- crates.io publishing for `intlify_format`

Range-only and minimal-diff formatting remain LSP/editor workflow concerns until editor requirements are defined.

## Formatter Modes

### Standard Mode

Standard mode is a deterministic pretty-printer over the public syntax view. It formats to the standard ox-mf2 style without using the original source layout as a primary decision input.

Standard mode normalizes:

- declaration spacing
- expression spacing
- function, markup, option, and attribute spacing
- indentation to 2 spaces
- matcher table layout
- line endings to LF
- final newline behavior to exactly one final LF

Standard mode does not rewrite translatable pattern text, quoted literal spelling, unquoted literal spelling, or escape spelling. Those choices can change runtime content or require heavier validity rules, so quote/literal spelling policy remains future scope.

### Preserve Mode

Preserve mode is source-shape-sensitive pretty formatting. It still applies standard local formatting rules, but it may use original source shape to choose layout where that shape is meaningful.

Preserve mode may preserve:

- single-line / multi-line choices
- blank-line grouping
- delimiter-driven source shape when recoverable

Preserve mode still normalizes:

- local spacing around declarations and operators
- expression, function, option, and attribute spacing
- indentation to 2 spaces
- matcher table layout
- line endings to LF
- final newline behavior to exactly one final LF

Preserve mode is not a minimal-diff formatter. It may rewrite larger regions when the formatted shape follows ox-mf2 style rules.

## Public API Shape

The primary Rust API parses and formats one MF2 message:

```rust
format_message(source: &str, options: FormatOptions) -> FormatResult
check_format(source: &str, options: FormatOptions) -> FormatCheckResult
```

The conceptual Rust result shape is a success/failure split:

```rust
enum FormatResult {
    Ok(FormatSuccess),
    Err(FormatFailure),
}

struct FormatSuccess {
    code: String,
    changed: bool,
}

enum FormatCheckResult {
    Ok(FormatCheckSuccess),
    Err(FormatFailure),
}

struct FormatCheckSuccess {
    changed: bool,
}

struct FormatFailure {
    diagnostics: Vec<ParserDiagnostic>,
    errors: Vec<OperationalError>,
}
```

N-API and WASM expose the same contract as a discriminated union:

```ts
type FormatResult =
  | { ok: true; code: string; changed: boolean }
  | { ok: false; diagnostics: ParserDiagnostic[]; errors: OperationalError[] }

type FormatCheckResult =
  | { ok: true; changed: boolean }
  | { ok: false; diagnostics: ParserDiagnostic[]; errors: OperationalError[] }

function formatMessage(source: string, options?: FormatOptions): FormatResult
function checkFormat(source: string, options?: FormatOptions): FormatCheckResult
```

`checkFormat` does not return formatted output. It only reports whether the source would change. Callers that need formatted code should use `formatMessage`.

Parser diagnostics and operational errors are separated:

- `diagnostics` contains parser diagnostics only.
- `errors` uses the Phase 3A operational error shape: `{ kind, code, message, path?, details? }`.
- Formatter-specific diagnostics are not emitted in the initial design.
- Formatter execution failures, invalid options, source/snapshot mismatches, unsupported inputs, and internal errors use `errors`.

If parsing produces any parser diagnostic, all formatter APIs return `ok: false` and no formatted output. Invalid N-API/WASM options also return `ok: false` with `errors`; the public formatter APIs should not throw for normal validation failures.

The advanced API accepts an already-created Binary AST snapshot. This is for playgrounds, workers, and language-service caches that already hold parse artifacts:

```rust
format_snapshot(snapshot: SnapshotView<'_>, source: &str, options: FormatOptions) -> FormatResult
```

Bindings expose this as:

```ts
function formatSnapshot(
  snapshot: SnapshotView,
  source: string,
  options?: FormatOptions
): FormatResult
```

`source` is required for snapshot-backed formatting because preserve mode, source slicing, parser diagnostics, and editor position conversion depend on source text. Snapshot-backed formatting is parse-artifact reuse, not a source-free formatting mode.

`formatSnapshot` follows the same strict diagnostics policy as `formatMessage`: if the snapshot contains parser diagnostics, it returns `ok: false`. The implementation must also verify snapshot/source consistency where the snapshot format makes that possible. A mismatch returns `ok: false` with an operational error.

## Operational Error Codes

Formatter operational errors use the shared Phase 3A error code namespace. Formatter APIs, CLI JSON output, N-API, and WASM should use the same code strings.

Parser diagnostics are not represented as operational errors. If parsing produces diagnostics, formatter APIs return `ok: false` with `diagnostics` populated and `errors` empty unless an independent operational error also occurred.

Formatter-specific codes:

| Code | Exit | When |
| --- | --- | --- |
| `source_snapshot_mismatch` | `2` | `formatSnapshot` receives source text that does not match the snapshot where consistency can be verified. |
| `unsupported_input_file` | `2` | CLI explicit file input or `--stdin-filepath` uses an unsupported extension or unsupported direct file form. |
| `invalid_ignore_pattern` | `2` | `fmt.ignorePatterns` or `--ignore-path` contains a pattern outside the formatter gitignore-compatible subset. |
| `ignore_file_read_failed` | `2` | A `--ignore-path` file cannot be read. |
| `unmatched_input` | `2` | A CLI input path or glob resolves to no target. |
| `invalid_options` | `2` | Rust, N-API, or WASM formatter APIs receive invalid options. |
| `invalid_snapshot` | `2` | Snapshot input is corrupt, unsupported, or missing required formatter capabilities, excluding source/snapshot mismatch. |
| `input_read_failed` | `2` | An input file or discovered directory entry cannot be read. |
| `output_write_failed` | `2` | Write mode cannot write formatted output. |
| `internal_error` | `2` | The formatter hits an implementation invariant or unexpected internal failure. |

Formatter also reuses Phase 3A common codes:

| Code | Exit | When |
| --- | --- | --- |
| `invalid_cli_argument` | `2` | Invalid CLI values or combinations, such as `--mode compact` or `--list-different --reporter json`. |
| `config_read_failed` | `2` | The shared tooling config file cannot be read. |
| `config_parse_failed` | `2` | The shared tooling config file is not valid JSON or JSONC. |
| `config_validation_failed` | `2` | The shared tooling config fails schema validation, including invalid `fmt.mode` or invalid `fmt.ignorePatterns` entries. |

Standardized `details` fields:

- `source_snapshot_mismatch`: implementation-defined. The code is stable; the exact details schema is not fixed in Phase 3B.
- `unsupported_input_file`:

  ```json
  {
    "extension": ".json",
    "supportedExtensions": [".mf2"]
  }
  ```

  `path` is carried by the top-level error. Files without an extension use `""`.

- `invalid_ignore_pattern`:

  ```json
  {
    "pattern": "[",
    "source": "fmt.ignorePatterns",
    "index": 0,
    "reason": "unterminated_character_class"
  }
  ```

  For `--ignore-path`, `source` is `"ignore-path"` and the top-level `path` is the ignore file path.

- `ignore_file_read_failed`:

  ```json
  {
    "reason": "not_found"
  }
  ```

  The top-level `path` is the ignore file path. Initial reason values are `not_found`, `permission_denied`, `not_file`, and `unknown`.

- `unmatched_input`:

  ```json
  {
    "input": "missing/**/*.mf2",
    "kind": "glob"
  }
  ```

  `kind` is `"path"` or `"glob"`. Top-level `path` is not used because the input does not resolve to an existing file.

- `invalid_options`:

  ```json
  {
    "pointer": "/mode",
    "value": "compact",
    "allowedValues": ["standard", "preserve"]
  }
  ```

- `invalid_snapshot`:

  ```json
  {
    "reason": "unsupported_version",
    "version": 3,
    "supportedVersions": [1, 2]
  }
  ```

  Initial reason values are `corrupt`, `unsupported_version`, `missing_capability`, and `unknown`.

- `input_read_failed` and `output_write_failed`:

  ```json
  {
    "reason": "permission_denied"
  }
  ```

  The top-level `path` is the input or output file path. Initial reason values are `not_found`, `permission_denied`, `not_file`, `not_directory`, and `unknown`.

- `internal_error`:

  ```json
  {
    "reason": "layout_invariant_violation"
  }
  ```

  The `reason` value is an implementation-defined string.

## CLI Workflow

The CLI command is `intlify fmt`.

Initial CLI flags:

- `--mode standard|preserve`
- `--check`
- `--list-different`
- `--stdin-filepath <path>`
- `--ignore-path <path>`; may be provided multiple times

Write mode is the default. `--check` reports whether files differ without writing. `--list-different` is a no-write check mode that prints path-only output for files that do not pass formatting. `--check` and `--list-different` may be used together, in which case `--list-different` controls the human-readable output.

`--list-different` is a text-only mode. Combining it with `--reporter json` is an invalid CLI argument. Combining it with stdin is also invalid.

Stdin formatting is supported. Stdin always writes formatted code to stdout and never writes to `--stdin-filepath`. `--stdin-filepath` is optional and only provides path context for extension checks, result paths, and future adapters. Without `--stdin-filepath`, stdin is treated as a direct MF2 message input named `<stdin>`.

Stdin with `--check` is allowed. It exits with `1` when the stdin source would change. If stdin has parser diagnostics, human-readable mode writes no formatted code to stdout, writes diagnostics to stderr, and exits with `1`. JSON reporter mode writes the JSON envelope to stdout.

### Input Discovery

The primary input unit is `1 file = 1 MF2 message`. Phase 3B initially supports only direct `.mf2` message files.

Input rules:

- explicit `.mf2` file paths are accepted
- explicit non-`.mf2` file paths are unsupported input errors and exit with `2`
- directory inputs are searched recursively for `.mf2` files
- glob inputs may be broad, but only matched `.mf2` files are selected
- unmatched paths or globs are input errors and exit with `2`
- if the final selected target set is empty, the command exits with `0`
- duplicate matches are de-duplicated by absolute path
- processing and output use stable slash-normalized path order

Directory and glob discovery excludes hidden files and hidden directories by default. Explicit file paths can still refer to hidden files, subject to ignore rules.

Directory and glob discovery also excludes common VCS, dependency, and output directories by default, including `.git`, `.hg`, `.svn`, `node_modules`, `vendor`, `target`, `dist`, and `coverage`. Explicit file paths can still target files under those directories, subject to ignore rules.

File symlinks are followed. Directory symlinks are not followed.

### Ignore Sources

Phase 3B supports file-level ignore through:

- root `.gitignore`
- one or more `--ignore-path <path>` files
- `fmt.ignorePatterns`

Ignore sources are evaluated as one ordered pattern list, not as independent additive filters. The source order is:

1. root `.gitignore`
2. `--ignore-path <path>` files in CLI argument order
3. `fmt.ignorePatterns`

Later patterns override earlier patterns. This means negated patterns can re-include files ignored by earlier patterns, and `fmt.ignorePatterns` is the final project-level formatter override.

The ignore grammar is a gitignore-compatible subset:

- `!pattern` negation is supported
- project-root-relative slash-normalized paths are used for matching
- `/pattern` is anchored to the project root
- unanchored patterns match at any depth
- `pattern/` is directory-only
- `**` globstar is supported
- escaped leading `\!` and `\#` are supported
- blank lines and unescaped leading `#` comments are ignored

The same blank-line and unescaped leading `#` behavior applies to `fmt.ignorePatterns` entries.

Ignore rules apply to all target files, including explicit file input. For example, `intlify fmt ignored/file.mf2` skips the file when the ordered ignore list resolves that path as ignored. If all requested inputs are ignored and the final selected target set is empty, the command exits with `0`.

The initial `.gitignore` behavior reads only the project root `.gitignore`, matching the root-only config discovery model. Nested `.gitignore` files are deferred.

All ignore patterns are evaluated relative to the project root, including patterns loaded from `--ignore-path`.

Invalid `fmt.ignorePatterns` entries are config validation errors and exit with `2` using `config_validation_failed`. Invalid patterns in `--ignore-path` files are operational errors and exit with `2`. Unsupported or unrecognized patterns in root `.gitignore` are ignored as non-fatal compatibility behavior.

Missing `--ignore-path` files are operational errors and exit with `2`.

### Exit Codes

Exit code classification:

- `0`: all selected files are formatted, or no files are selected after filtering
- `1`: format mismatch, parser diagnostics in selected inputs, or another formatting failure caused by input content
- `2`: operational error, including config errors, IO errors, invalid CLI arguments, unsupported explicit input files, unmatched input patterns, unsupported reporters, or internal errors

When multiple outcomes occur, final exit code priority is `2 > 1 > 0`.

Parser diagnostics never cause write mode to modify the affected file.

### Human and JSON Output

Human-readable write mode prints only files that changed. Human-readable `--check` prints files that differ and files with parser diagnostics. `--list-different` prints path-only output for files that differ or have parser diagnostics.

JSON reporter output for write and check mode uses the shared Phase 3A envelope and adds command-specific `summary` fields and `results[]` entries. Each result should include:

- `path`
- `changed`
- `diagnostics`
- `errors`

When no files are selected after filtering, JSON output uses `summary.status: "success"`, `summary.matchedFiles: 0`, and `results: []`.

Resource files and catalogs that contain multiple messages are layered workflows. A resource/catalog adapter should parse the host file, extract message entries, call the message-level formatter core, and own host-file string escaping and outer document edits.

## Configuration

Formatter configuration lives in the `fmt` section of one ox-mf2 tooling config shared with lint configuration. The config format is JSON or JSONC as defined by the Phase 3A CLI foundation, and the Rust config model remains the source of truth for generated JSON Schema.

Initial config discovery is root-only and follows the Phase 3A CLI foundation contract. Nearest-config-wins and nested config discovery are not part of the initial design.

Initial formatter config supports:

```json
{
  "fmt": {
    "mode": "standard",
    "ignorePatterns": []
  }
}
```

`fmt.mode` is an enum with `"standard"` and `"preserve"`. The default is `"standard"`.

`fmt.ignorePatterns` participates in CLI file discovery only. It is not part of `FormatOptions`, and message-level APIs do not perform file selection.

Formatter configuration does not support file-specific `overrides` in the initial design. The first formatter target is a narrow direct `.mf2` message-file workflow, so per-file option overrides are unnecessary until resource/catalog requirements prove otherwise.

The formatter does not read `.editorconfig` in the initial implementation because `mode` is the only supported formatting option. `.editorconfig` fallback becomes active only when formatter options such as line width, indent width, line ending, or final newline are explicitly supported.

Configuration precedence for formatting mode is:

1. CLI `--mode`
2. `fmt.mode`
3. default `"standard"`

Formatter-specific schema definitions live under the unified config schema, for example `definitions.fmt`, and are published through `@intlify/cli/schema/config.schema.json`. Phase 3B does not publish generated TypeScript config types.

Config validation errors use the Phase 3A `config_validation_failed` operational error, with JSON pointers such as `/fmt/mode` or `/fmt/ignorePatterns`. Invalid CLI values such as `--mode compact` use `invalid_cli_argument` with details such as the option name, provided value, and allowed values.

`fmt.ignorePatterns` uses the same gitignore-compatible subset described in [Ignore Sources](#ignore-sources). Invalid entries are rejected during config validation.

## Options

Initial options stay intentionally small:

```text
FormatOptions {
  mode: standard | preserve
}
```

Default `mode` is `standard`.

Options deferred until fixtures prove a need:

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
- API consumers receive `ok: false` with parser diagnostics and no formatted output
- CLI write mode does not modify the file
- CLI check/list-different modes treat the file as a formatting failure and exit with `1`
- LSP/editor adapters treat the request as a no-op

Recovery-aware formatting is future editor-specific scope.

Formatter fixtures must cover:

- no diagnostics
- parser diagnostics that must not produce public formatted output
- CLI write-mode no-op behavior on invalid syntax
- API result shape for invalid syntax

## Architecture

The formatter separates syntax traversal from rendering.

The message-level core lives in a workspace-internal Rust crate named `intlify_format`. This crate is not published to crates.io in Phase 3B. It should still expose a clear workspace API for the CLI, N-API binding, WASM binding, and tests.

The message-level core should build an internal layout representation before rendering text. The layout model should support delayed line, group, and indent decisions so standard mode, preserve mode, future line width support, and future resource/catalog adapters can reuse one formatter core.

The exact IR/document implementation is intentionally left to implementation design. The public contract is that callers format whole MF2 messages and receive either formatted source/check information or diagnostics/errors.

`@intlify/cli` owns the `intlify fmt` command and links the `intlify_format` crate through the native CLI binary. Programmatic formatter APIs are distributed separately:

- `@intlify/format-napi`
- `@intlify/format-wasm`

`@intlify/format-napi` is a wrapper package with platform-specific native packages, using the existing label style:

- `@intlify/format-napi-darwin-arm64`
- `@intlify/format-napi-darwin-x64`
- `@intlify/format-napi-linux-x64-gnu`
- `@intlify/format-napi-linux-x64-musl`
- `@intlify/format-napi-linux-arm64-gnu`
- `@intlify/format-napi-win32-x64-msvc`

The N-API package uses lazy native loading. Importing the package should not eagerly load the native binary; API calls load the binding as needed.

`@intlify/format-wasm` is browser-first for playground, worker, and browser tooling use cases. Node users should prefer `@intlify/format-napi`. After `await init()`, the WASM package exposes synchronous `formatMessage`, `checkFormat`, and `formatSnapshot` APIs.

New `@intlify/format-*` npm packages may require token-based bootstrap publishing for the first release. After the packages exist on npm and trusted publisher settings are configured, normal releases should use npm trusted publishing.

Parser binding packages remain focused on parser-level APIs. Formatter APIs are not added to existing `@intlify/ox-mf2-napi` or `@intlify/ox-mf2-wasm` packages.

## Resource and Catalog Formatting

The formatter core formats one MF2 message. Resource/catalog formatting is layered above it.

Phase 3B does not implement JSON, YAML, framework-specific resource files, multi-message catalogs, host-file parsing, host-file escaping, or outer document edits.

A future resource/catalog adapter is responsible for:

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

Formatter ignore directives are not included in Phase 3B.

The MF2 grammar does not define line comments or block comments, and `#` is a markup sigil for `{#tag}`. Introducing a comment-like formatter directive would require a non-standard syntax extension. Phase 3B therefore uses only file-level ignore sources: `fmt.ignorePatterns`, root `.gitignore`, and `--ignore-path`.

A future formatter suppression mechanism must be spec-compatible. Possible directions include an attribute-based mechanism for expression/markup units or a future resource/container-level convention, but no syntax-unit formatter directive is part of the initial formatter product.

## Matcher Layout

Matcher layout uses a table-like layout in both standard and preserve mode.

Rules:

- multi-selector matchers use table-like variant rows
- single-selector matchers also align variant rows
- variant keys align by each key column's maximum width
- `.match` selector expressions are not aligned to variant key columns
- key columns have at least 2 spaces between them
- the final key column and quoted pattern have at least 2 spaces between them
- preserve mode may preserve existing single-line/multi-line shape and blank-line grouping, but still normalizes matcher rows to the table-like spacing rules

Example:

```mf2
.match $count $gender
one  masculine  {{He has one item}}
one  feminine   {{She has one item}}
*    *          {{They have items}}
```

## Line Wrapping

Line wrapping is not implemented in Phase 3B.

The initial formatter does not support `lineWidth`, and it does not wrap long expressions, long quoted patterns, or long matcher variants. Long syntax units are emitted as whole units. Wrapping behavior remains future scope because pattern text and quoted literal whitespace are semantically significant.

## Core Style Rules

Initial core style rules:

- output line endings are LF
- output ends with exactly one final LF
- indentation is 2 spaces
- complex messages put each declaration on its own line and the body on the following line
- simple message pattern text is emitted as-is; the formatter does not insert line breaks around placeholders
- translatable pattern text whitespace is preserved
- quoted pattern text whitespace is preserved
- single-line quoted patterns remain `{{text}}`; delimiters are not split onto separate lines just for formatting
- expression braces have no inner padding
- operand/function/options/attributes are separated by 1 space
- option `=` and attribute `=` have no surrounding spaces
- declaration `=` has 1 space on both sides
- quoted literal spelling, unquoted literal spelling, and escape spelling are preserved

Examples:

```mf2
.input {$count :number}
.local $label = {$count :number}
.match $count
one  {{One item}}
*    {{$label items}}
```

```mf2
{$value :number minimumFractionDigits=2 maximumFractionDigits=4 @foo=|bar|}
```

## SnapshotView Requirements

The formatter first consumes the existing Binary AST `SnapshotView` / binding-side accessor model.

Initial required helpers are the minimum needed by the formatter:

- node and token kind traversal
- node and token span access
- source slicing for node and token spans
- leading/trailing trivia span access where already represented
- parser diagnostic access
- source/snapshot consistency checks where the snapshot format supports them

If formatter implementation needs additional public snapshot accessors, those accessors may be added in the formatter PR that needs them. Additions should be limited to the minimum formatter-required surface.

If the formatter requires a Binary AST snapshot format change, the formatter PR may include it, but it must also update snapshot versioning, compatibility policy, parser snapshot round-trip tests, parser snapshot compatibility tests, N-API/WASM exposure, and any affected fixtures. Snapshot format changes should remain narrowly scoped to formatter requirements.

## Fixture Strategy

Formatter fixtures should be reviewable and stable.

Valid fixtures use directory fixtures:

```text
crates/intlify_format/fixtures/
  matcher_table/
    input.mf2
    standard.mf2
    preserve.mf2
    options.json
```

`preserve.mf2` is optional. When it is absent, preserve mode is expected to match `standard.mf2`.

Invalid fixtures do not include formatted output:

```text
crates/intlify_format/fixtures/
  invalid_unclosed/
    input.mf2
    diagnostics.json
```

Required assertions:

- `format(input, standard)` matches `standard.mf2`
- `format(input, preserve)` matches `preserve.mf2` when present, otherwise `standard.mf2`
- formatting standard output again in standard mode is idempotent
- formatting preserve output again in preserve mode is idempotent
- valid input has no parser diagnostics
- invalid input produces no public formatted output
- formatted output reparses without diagnostics
- CLI write mode does not modify invalid syntax
- formatting preserves semantic facts once SemanticView participates in formatter tests

Formatter benchmarks may reuse parser fixtures for syntax coverage and add formatter-specific fixtures for spacing, layout, matcher, preserve mode, and direct `.mf2` workflows.

## Benchmarks

Formatter benchmarks are local-first tooling under `tools/`, following the parser benchmark pattern.

Formatter benchmarks should separate:

- parse cost
- snapshot encode cost
- snapshot decode/access cost
- syntax traversal cost
- layout construction cost
- rendering cost
- N-API binding call cost
- WASM binding call cost
- CLI end-to-end format cost
- CLI JSON reporter cost

Phase 3 benchmark names:

- `format_standard`
- `format_preserve`
- `format_check_cli_e2e`
- `format_check_json`
- `e2e_format`

Benchmark commands and result schemas must be executable and testable, but timing thresholds are not CI pass/fail gates in Phase 3B. A benchmark command that cannot build, cannot execute, cannot read fixtures, or emits malformed results is an implementation failure. A slow timing value is an observation, not a failing threshold.

GitHub Actions benchmark jobs and issue-comment reporting are deferred follow-up work. The Phase 3B implementation should not add benchmark runtime to normal `vpr check`, `vpr test`, or default CI gates.

## Implementation Phasing

Phase 3B formatter implementation should be split into reviewable PRs:

1. `intlify_format` crate scaffold, result/options/config model, and fixture harness
2. standard/preserve core formatter rules for direct `.mf2` messages
3. `intlify fmt` CLI integration, file discovery, check/write mode, and JSON reporter
4. `@intlify/format-napi` wrapper and platform native packages
5. `@intlify/format-wasm`
6. local-first formatter benchmarks under `tools/`

Each PR should be cut from `main`, keep formatter work separated from Phase 3C linter work, and maintain the existing Phase 3A CLI contract unless the PR explicitly extends it for `intlify fmt`.

## Deferred Follow-Up Notes

- Resource/catalog adapters for JSON, YAML, framework-specific resource files, string escaping, decoded-to-raw mapping, and outer document edits.
- Formatter ignore or suppression mechanisms that are compatible with MF2 syntax.
- `.editorconfig` loading once formatter options exist that can consume it.
- Line wrapping and style options such as `lineWidth`, `indentWidth`, `lineEnding`, `finalNewline`, and quote/literal spelling policy.
- Generated TypeScript config type distribution.
- Nested config discovery, nearest-config-wins behavior, file-specific overrides, `--cwd`, and `--root`.
- `--no-error-on-unmatched-pattern` if users need a relaxed unmatched-input mode.
- Runtime controls such as `--threads`.
- `intlify init` config scaffolding once formatter and linter config fields are stable enough to write.
- GitHub Actions benchmark jobs and issue-comment benchmark reporting for parser and formatter trends.
- Publishing public command-specific output JSON Schemas after `schemaVersion` is stable enough.

## Open Questions

The following items remain detailed formatter design questions, not Phase 3 boundary decisions:

- exact JSON reporter `summary` fields for write, check, list-different-equivalent failures, stdin, and no selected files
- exact text reporter wording beyond stdout/stderr and path-list behavior
- exact internal layout IR/document representation
- exact SnapshotView accessors or binary format extensions needed by the formatter implementation
- exact fixture harness runner format for `options.json` matrices and diagnostics snapshots
- LSP/editor configuration shape, including whether an explicit formatter config path is supported
- LSP/editor behavior when config loading fails, including whether it falls back to defaults or reports an operational error
- WASM bundle-size constraints and tree-shaking expectations
