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

### Mode Examples

The examples in this section are illustrative. Fixture expected files remain the exact source of truth for formatted output.

Standard mode shows canonical spacing, indentation, LF line endings, exactly one final LF, and matcher table spacing:

```mf2
.input   {$count   :number}
.local $label={$count:number}
.match $count
0 {{No items}}
1 {{One item}}
*   {{$label items}}
```

formats toward:

```mf2
.input {$count :number}
.local $label = {$count :number}
.match $count
0  {{No items}}
1  {{One item}}
*  {{$label items}}
```

Preserve mode may use the original single-line or multi-line source shape where the shape is meaningful, while still normalizing local spacing:

```mf2
.input   {$name   :string} {{Hello {$name}}}
```

may remain a single-line message shape:

```mf2
.input {$name :string} {{Hello {$name}}}
```

Preserve mode may also keep blank-line grouping as a source-shape hint:

```mf2
.input {$count :number}

.local $label = {$count :number}
{{{$label} items}}
```

Standard mode may collapse that grouping when canonical layout does not require it, while preserve mode may keep the blank line.

## Public API Shape

The primary Rust API parses and formats one MF2 message:

```rust
format_message(source: &str, options: FormatOptions) -> FormatResult
check_format(source: &str, options: FormatOptions) -> FormatCheckResult
format_snapshot(snapshot: SnapshotView<'_>, source: &str, options: FormatOptions) -> FormatResult
check_snapshot(snapshot: SnapshotView<'_>, source: &str, options: FormatOptions) -> FormatCheckResult
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
function formatSnapshot(snapshot: Uint8Array, source: string, options?: FormatOptions): FormatResult
function checkSnapshot(
  snapshot: Uint8Array,
  source: string,
  options?: FormatOptions
): FormatCheckResult
```

`checkFormat` does not return formatted output. It only reports whether the source would change. Callers that need formatted code should use `formatMessage`.

Parser diagnostics and operational errors are separated:

- `diagnostics` contains parser diagnostics only.
- `errors` uses the Phase 3A operational error shape: `{ kind, code, message, path?, details? }`.
- Formatter-specific diagnostics are not emitted in the initial design.
- Formatter execution failures, invalid options, source/snapshot mismatches, unsupported inputs, and internal errors use `errors`.

If parsing produces any parser diagnostic, all formatter APIs return `ok: false` and no formatted output. Invalid N-API/WASM options also return `ok: false` with `errors`; the public formatter APIs should not throw for normal validation failures.

Programmatic formatter API results do not use the CLI JSON envelope. They do not include `schemaVersion`, `version`, `projectRoot`, `summary`, or `results`. N-API and WASM reuse the same parser diagnostic JavaScript shape as the parser packages, but they do not define a new diagnostic object format.

The advanced snapshot APIs accept an already-created Binary AST snapshot. These APIs are for playgrounds, workers, and language-service caches that already hold parse artifacts.

Rust uses `SnapshotView<'_>` internally. N-API and WASM bindings accept serialized snapshot bytes as `Uint8Array`; formatter binding packages do not exchange parser-package native objects or WASM objects across package boundaries.

`source` is required for snapshot-backed formatting because preserve mode, source slicing, parser diagnostics, and editor position conversion depend on source text. Snapshot-backed formatting is parse-artifact reuse, not a source-free formatting mode.

`formatSnapshot` and `checkSnapshot` follow the same strict diagnostics policy as `formatMessage`: if the snapshot contains parser diagnostics, they return `ok: false`. The implementation must also verify snapshot/source consistency where the snapshot format makes that possible. Phase 3B keeps source identity validation best-effort and does not require snapshots to carry a source hash or source length. A detected mismatch returns `ok: false` with an operational error.

`changed` is computed by byte equality between the formatter output and the supplied source string. This applies to `formatMessage`, `checkFormat`, `formatSnapshot`, and `checkSnapshot`. Inputs with CRLF line endings, missing final LF, or multiple final newlines therefore report `changed: true` when the formatter normalizes them to LF and exactly one final LF. Snapshot-backed APIs compare against the supplied `source`; if embedded source identity exists and differs from the supplied source, the API returns a mismatch failure instead of a changed result.

## Operational Error Codes

Formatter operational errors use the shared Phase 3A error code namespace. Formatter APIs, CLI JSON output, N-API, and WASM should use the same code strings.

Parser diagnostics are not represented as operational errors. If parsing produces diagnostics, formatter APIs return `ok: false` with `diagnostics` populated and `errors` empty unless an independent operational error also occurred.

Formatter-specific codes:

| Code | Kind | Exit | When |
| --- | --- | --- | --- |
| `source_snapshot_mismatch` | `input` | `2` | `formatSnapshot` or `checkSnapshot` receives source text that does not match the snapshot where consistency can be verified. |
| `unsupported_input_file` | `input` | `2` | CLI explicit file input or `--stdin-filepath` uses an unsupported extension or unsupported direct file form. |
| `invalid_ignore_pattern` | `input` | `2` | A `--ignore-path` file contains a pattern outside the formatter gitignore-compatible subset. Invalid `fmt.ignorePatterns` entries use `config_validation_failed`. |
| `ignore_file_read_failed` | `io` | `2` | A `--ignore-path` file cannot be read. |
| `unmatched_input` | `input` | `2` | A CLI input path does not exist or a glob matches no filesystem entries. |
| `invalid_options` | `input` | `2` | N-API, WASM, or other raw external formatter option input fails validation before typed formatter options are constructed. |
| `invalid_snapshot` | `input` | `2` | Snapshot input is corrupt, unsupported, or missing required formatter capabilities, excluding source/snapshot mismatch. |
| `input_read_failed` | `io` | `2` | An input file or discovered directory entry cannot be read. |
| `output_write_failed` | `io` | `2` | Write mode cannot write formatted output. |
| `internal_error` | `internal` | `2` | The formatter hits an implementation invariant or unexpected internal failure. |

Formatter also reuses Phase 3A common codes:

| Code | Kind | Exit | When |
| --- | --- | --- | --- |
| `invalid_cli_argument` | `input` | `2` | Invalid CLI values or combinations, such as `--mode compact`, `--list-different --reporter json`, or an invalid input glob. |
| `missing_cli_option_value` | `input` | `2` | A value-taking CLI option such as `--mode`, `--stdin-filepath`, `--ignore-path`, or `--reporter` is missing its value. |
| `duplicate_cli_option` | `input` | `2` | A non-repeatable CLI option is provided more than once. |
| `reporter_not_supported` | `reporter` | `2` | `--reporter` is provided with a value outside `text` or `json`. |
| `config_read_failed` | `config` | `2` | The shared tooling config file cannot be read. |
| `config_parse_failed` | `config` | `2` | The shared tooling config file is not valid JSON or JSONC. |
| `config_validation_failed` | `config` | `2` | The shared tooling config fails schema validation, including invalid `fmt.mode` or invalid `fmt.ignorePatterns` entries. |

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
    "source": "ignore-path",
    "index": 0,
    "reason": "unterminated_character_class"
  }
  ```

  `source` is `"ignore-path"` and the top-level `path` is the ignore file path. `index` is the zero-based pattern index after blank lines and unescaped leading `#` comments are skipped. Invalid `fmt.ignorePatterns` entries are config validation failures and use JSON pointers such as `/fmt/ignorePatterns/0`.

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

  `kind` is `"path"` or `"glob"`. Top-level `path` is not used because the input does not resolve to an existing file or matched entry. A glob that matches filesystem entries but selects no `.mf2` formatter targets is not `unmatched_input`.

- `invalid_options`:

  ```json
  {
    "pointer": "/mode",
    "value": "compact",
    "allowedValues": ["standard", "preserve"]
  }
  ```

  JavaScript binding calls use `invalid_options` for invalid argument shape or option values, including `null` options, unknown option fields, non-string `source`, non-`Uint8Array` snapshot input, and invalid `mode`. The typed Rust `FormatOptions` API should not allow invalid option states; raw external input is validated before constructing typed options.

- `invalid_cli_argument` for `--list-different --reporter json`:

  ```json
  {
    "option": "--list-different",
    "reason": "text_only_mode",
    "conflictsWith": ["--reporter json"]
  }
  ```

- `invalid_cli_argument` for an invalid input glob:

  ```json
  {
    "input": "[",
    "kind": "glob",
    "reason": "invalid_glob"
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

  `invalid_snapshot` is used after the public binding argument shape has been accepted as snapshot bytes. Non-`Uint8Array` snapshot arguments use `invalid_options`.

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
    "phase": "document_ir_render"
  }
  ```

  Initial phase values are `snapshot_traversal`, `layout_ir_construction`, `layout_ir_normalize`, `document_ir_lowering`, and `document_ir_render`. Formatter internals such as IR node kinds, source text, and source spans are not exposed in public `internal_error.details`.

## CLI Workflow

The CLI command is `intlify fmt`.

Initial CLI flags:

- `--mode standard|preserve`
- `--check`
- `--list-different`
- `--stdin-filepath <path>`
- `--ignore-path <path>`; may be provided multiple times
- `--reporter <text|json>`

Write mode is the default. `--check` reports whether files differ without writing. `--list-different` is a no-write check mode that prints path-only output for files that do not pass formatting. `--check` and `--list-different` may be used together, in which case `--list-different` controls the human-readable output.

The default reporter is `text`. Machine-readable output is selected with `--reporter json`, matching the Phase 3A reporter names.

`--list-different` is a text-only mode. Combining it with `--reporter json` is an invalid CLI argument. Combining it with stdin mode is also invalid, including `--check --list-different --stdin-filepath <path>`. `--check --reporter json` is allowed.

Stdin formatting is supported only through explicit stdin mode. `--stdin-filepath <path>` selects stdin mode, reads all source text from stdin, uses `<path>` as the virtual input path, writes formatted code to stdout, and never writes to `<path>`. This follows the oxfmt-style explicit stdin contract and avoids making `intlify fmt` change behavior based on whether stdin is piped.

Stdin mode cannot be combined with file, directory, or glob operands. Any operand together with `--stdin-filepath` is an `invalid_cli_argument` error.

Stdin with `--check` is allowed. It exits with `1` when the stdin source would change. If stdin has parser diagnostics, human-readable mode writes no formatted code to stdout, writes diagnostics to stderr, and exits with `1`. JSON reporter mode writes the JSON envelope to stdout.

When no file, directory, or glob operands are provided and stdin mode is not selected, `intlify fmt` behaves as `intlify fmt .`. Help and version options keep the Phase 3A precedence and do not trigger config loading, input discovery, or formatting.

`intlify fmt` supports `--` as an end-of-options marker. Tokens after `--` are treated as input path or glob operands even if they start with `-` or `--`. If `--` is present with no operands after it and stdin mode is not selected, the command follows the no-operand rule and formats `.`. Global options such as `--reporter json` and formatter options such as `--mode preserve` are recognized only before `--`.

`--ignore-path` may be provided multiple times. `--mode`, `--stdin-filepath`, `--check`, `--list-different`, and `--reporter` are not repeatable; duplicates are `duplicate_cli_option` errors.

After help/version precedence and command argument shape validation, `intlify fmt` loads and validates config before file discovery or formatting. If `--reporter json` can be parsed before a config error, the config error is emitted as a JSON envelope. Missing root config uses defaults.

### Input Discovery

The primary input unit is `1 file = 1 MF2 message`. Phase 3B initially supports only direct `.mf2` message files.

Input rules:

- in file mode, no operands are equivalent to an explicit directory operand `.`
- explicit `.mf2` file paths are accepted
- explicit non-`.mf2` file paths are unsupported input errors and exit with `2`
- directory inputs are searched recursively for `.mf2` files
- glob inputs may be broad, but only matched `.mf2` files are selected
- unmatched paths or globs that match no filesystem entries are input errors and exit with `2`
- glob inputs that match filesystem entries but select no `.mf2` formatter targets are zero-target successes and exit with `0`
- invalid glob syntax is an `invalid_cli_argument` error
- if the final selected target set is empty, the command exits with `0`
- duplicate matches are de-duplicated by absolute path
- processing and output use stable slash-normalized path order

Directory and glob discovery excludes hidden files and hidden directories by default. Explicit file paths can still refer to hidden files, subject to ignore rules. An explicit hidden path that does not exist is `unmatched_input`; an explicit hidden path with an unsupported extension is `unsupported_input_file`.

Directory and glob discovery also excludes common VCS, dependency, and output directories by default, including `.git`, `.hg`, `.svn`, `node_modules`, `vendor`, `target`, `dist`, and `coverage`. Explicit file paths can still target files under those directories, subject to ignore rules.

File symlinks are followed. Directory symlinks are not followed. Duplicate detection uses slash-normalized absolute paths and does not canonicalize symlink targets, so a file symlink and its target path are treated as separate targets when both paths are provided.

Directory inputs such as `.` that contain no selected `.mf2` files after discovery and filtering exit with `0`. Explicit unmatched inputs such as `intlify fmt missing/**/*.mf2` remain `unmatched_input` errors.

Phase 3B initially processes selected files sequentially after discovery, de-duplication, and sorting. Future parallel execution may be added without changing observable stdout, JSON result ordering, or write target selection.

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

`fmt.ignorePatterns` is a `string[]` config field. Non-string entries and invalid patterns are `config_validation_failed` errors. Empty strings are treated like blank ignore-file lines and ignored. Unsupported gitignore constructs in `fmt.ignorePatterns` are config validation errors, while unsupported or unrecognized constructs in the root `.gitignore` are ignored as non-fatal compatibility behavior.

Ignore rules apply to all target files, including explicit file input. For example, `intlify fmt ignored/file.mf2` skips the file when the ordered ignore list resolves that path as ignored. If all requested inputs are ignored and the final selected target set is empty, the command exits with `0`.

The initial `.gitignore` behavior reads only the Phase 3A discovered project root `.gitignore`, matching the root-only config discovery model. Nested `.gitignore` files are deferred.

All ignore patterns are evaluated relative to the Phase 3A discovered project root, including patterns loaded from `--ignore-path`.

`--ignore-path <path>` itself is resolved after config loading. Absolute ignore file paths are used as-is. Relative ignore file paths are resolved from the Phase 3A discovered project root. This differs from `--config <path>`, which is resolved from the process `cwd` because it is part of the config loading boundary.

Invalid `fmt.ignorePatterns` entries are config validation errors and exit with `2` using `config_validation_failed`. Invalid patterns in `--ignore-path` files are operational errors and exit with `2`. Unsupported or unrecognized patterns in root `.gitignore` are ignored as non-fatal compatibility behavior.

Missing `--ignore-path` files are operational errors and exit with `2`.

Stdin mode applies ignore rules to the `--stdin-filepath` virtual path. If the stdin filepath is ignored, normal stdin formatting writes the original stdin source to stdout and exits with `0`; stdin check mode writes nothing and exits with `0`; JSON reporter output uses a zero-target success summary with no results. Unsupported `--stdin-filepath` extensions are checked before ignore rules, so `--stdin-filepath ignored/file.json` is still `unsupported_input_file`.

### Exit Codes

Exit code classification:

- `0`: all selected files are formatted, or no files are selected after filtering
- `1`: format mismatch, parser diagnostics in selected inputs, or another formatting failure caused by input content
- `2`: operational error, including config errors, IO errors, invalid CLI arguments, unsupported explicit input files, unmatched input patterns, unsupported reporters, or internal errors

When multiple outcomes occur, final exit code priority is `2 > 1 > 0`.

Parser diagnostics never cause write mode to modify the affected file.

In write mode, successfully written files are not failures. If files are formatted and no diagnostics or operational errors occur, the command exits with `0`. If parser diagnostics occur without operational errors, the command exits with `1`. If any operational error occurs, the command exits with `2`.

### Human and JSON Output

Human-readable output keeps stdout machine-friendly and sends problems to stderr.

All human path-only stdout entries use project-root-relative slash-normalized paths in stable slash-normalized path order. On Windows, stdout paths still use `/`.

Write mode:

- changed files are printed to stdout, one path per line
- already formatted files produce no stdout
- parser diagnostics and operational errors are rendered to stderr
- files with parser diagnostics are not modified
- if valid targets are changed before or alongside diagnostics/errors, those changed paths are still printed to stdout

Check mode:

- files that would be formatted are printed to stdout, one path per line
- already formatted files produce no stdout
- parser diagnostics and operational errors are rendered to stderr
- files with parser diagnostics are not included in stdout because stdout means "would-format targets"

`--list-different` is allowed together with `--check` and uses the same path-only human output. In Phase 3B, it is a path-only output guarantee rather than a separate result model.

Stdin human output:

- normal stdin formatting writes formatted code to stdout, writes nothing to stderr on success, and exits with `0`
- stdin `--check` with a difference prints the `--stdin-filepath` virtual path, writes nothing to stderr, and exits with `1`
- stdin `--check` without a difference writes nothing to stdout or stderr and exits with `0`
- stdin parser diagnostics write no formatted code to stdout, render diagnostics to stderr, and exit with `1`

When the final selected target set is empty, human output writes nothing to stdout or stderr and exits with `0`.

For invalid or mixed input, valid selected `.mf2` targets are processed where possible. Write mode prints changed valid targets to stdout, check/list-different prints would-format valid targets to stdout, and operational errors are rendered to stderr. The final exit code still follows `2 > 1 > 0`.

Formatter-specific human stderr wording is not fixed in this document. Parser diagnostics and operational errors use the Phase 3A diagnostic/error renderer contract.

Human stderr fixtures should avoid over-constraining full prose. CLI fixture tests should primarily lock stream selection, exit code, stable error code/path prefixes where needed, and JSON reporter shape. JSON reporter fixtures are the strict machine-readable contract.

JSON reporter output uses the shared Phase 3A envelope. Formatter-specific JSON output has this top-level shape:

```json
{
  "schemaVersion": "0",
  "command": "fmt",
  "version": "0.14.0",
  "projectRoot": "/repo",
  "summary": {},
  "results": [],
  "errors": []
}
```

`schemaVersion`, `version`, and `projectRoot` follow the Phase 3A shared envelope contract. `command` is always `"fmt"`. `projectRoot` is an absolute slash-normalized path. File result paths and error paths are project-root-relative slash-normalized paths when representable, including on Windows.

The top-level `errors` array contains global operational errors only, such as invalid CLI arguments, config errors, input selection errors, ignore file read failures, invalid ignore patterns from setup, and pathless internal errors. File-specific operational errors live in `results[].errors`. Parser diagnostics live only in `results[].diagnostics`; there is no top-level `diagnostics` field.

`summary.status` follows the Phase 3A status contract:

- `"success"` for exit `0`
- `"failure"` for exit `1`, such as check differences or parser diagnostics without operational errors
- `"error"` for exit `2`, including any operational error

Write mode that formats files successfully remains `"success"` even when files changed. In mixed outcomes, any operational error makes the final status `"error"`; otherwise diagnostics or check differences make the final status `"failure"`.

Common `summary` fields:

- `status`
- `operation`
- `mode`, omitted when the mode cannot be resolved because of invalid CLI or config input
- `matchedFiles`, counting only final selected formatter targets
- `unchangedFiles`
- `diagnosticFiles`
- `diagnosticCount`
- `errorCount`, counting top-level `errors` plus all `results[].errors`

`operation` is one of:

- `"write"`
- `"check"`
- `"stdin"`
- `"stdin-check"`

Write mode adds `formattedFiles`, counting files actually written. Check mode adds `differentFiles`, counting targets that would change. Stdin operations do not add `formattedFiles`; changed state is represented by the single result entry.

`--list-different` is not a JSON operation. It is a text-only output mode and conflicts with `--reporter json`. JSON users should use `--check --reporter json`.

When no files are selected after filtering in file mode, JSON output uses a zero-target summary:

```json
{
  "status": "success",
  "operation": "write",
  "mode": "standard",
  "matchedFiles": 0,
  "unchangedFiles": 0,
  "diagnosticFiles": 0,
  "diagnosticCount": 0,
  "errorCount": 0
}
```

`operation` and `mode` are still emitted when they can be resolved. `formattedFiles` and `differentFiles` are omitted for zero-target output. `results` and `errors` are empty arrays. Ignored stdin mode also uses zero-target output, but its `operation` remains `"stdin"` or `"stdin-check"` because stdin mode was selected explicitly.

Ignored stdin JSON output in normal stdin mode uses:

```json
{
  "status": "success",
  "operation": "stdin",
  "mode": "standard",
  "matchedFiles": 0,
  "unchangedFiles": 0,
  "diagnosticFiles": 0,
  "diagnosticCount": 0,
  "errorCount": 0
}
```

Ignored stdin JSON output in stdin check mode uses the same zero-target counters with `"operation": "stdin-check"`. It does not add `differentFiles`, because the ignored stdin source is not checked as a formatter target.

Each `results[]` entry uses this shape:

```json
{
  "path": "messages/foo.mf2",
  "status": "formatted",
  "changed": true,
  "diagnostics": [],
  "errors": []
}
```

`status` is one of:

- `"formatted"`: write mode or stdin produced formatter output that differs from the input
- `"unchanged"`: the target already matched formatter output
- `"would_format"`: check mode or stdin check found a difference without writing formatted output
- `"diagnostic"`: parser diagnostics prevented formatting for that target
- `"error"`: a file-specific operational error occurred

`changed` is always present. It is `true` for `"formatted"` and `"would_format"`, and `false` for `"unchanged"`, `"diagnostic"`, and `"error"`.

Ignored files are not included in `results[]`; `results[]` represents only the final selected formatter target set. Invalid input and unmatched input errors, including `unsupported_input_file` and `unmatched_input`, are top-level operational errors and do not create result entries.

Mixed outcomes continue processing valid selected `.mf2` targets where possible. For example, `intlify fmt valid.mf2 messages.txt --reporter json` reports `messages.txt` as a top-level `unsupported_input_file` error, still processes `valid.mf2`, sets `summary.status` to `"error"`, and exits with `2`.

For stdin JSON output, `matchedFiles` is `1` unless stdin is skipped by ignore rules through `--stdin-filepath`. `results[0].path` is the `--stdin-filepath` virtual path. Normal stdin formatting uses `"formatted"` when the output differs and `"unchanged"` when it does not. Stdin with `--check` uses `"would_format"` when the input would change. Stdin parser diagnostics use `"diagnostic"` with `changed: false`.

If `--reporter json` can be parsed, invalid CLI combinations such as `--list-different --reporter json` still return the JSON envelope on stdout with `summary.status: "error"` and a top-level `invalid_cli_argument` error.

Per-file input read failures and output write failures create `results[]` entries with `status: "error"` and continue processing other selected targets where possible. The final exit code is `2` and `summary.status` is `"error"`. Human text output may still include changed or would-format paths for successful targets, while operational errors are rendered to stderr.

Write mode generates the full formatted output in memory before writing. Phase 3B writes directly to the target file and does not guarantee rollback or atomic replacement if the filesystem write fails.

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

Schema-level formatter config rules:

- `fmt` is optional
- if present, `fmt` must be an object
- `fmt: null` is invalid
- unknown fields inside `fmt` are invalid
- `fmt.mode` is optional and defaults to `"standard"`
- `fmt.mode` accepts only `"standard"` or `"preserve"`
- `fmt.ignorePatterns` is optional and defaults to `[]`
- `fmt.ignorePatterns` must be an array of strings
- each `fmt.ignorePatterns` entry is validated during config validation against the formatter ignore pattern subset

`fmt.mode` is an enum with `"standard"` and `"preserve"`. The default is `"standard"`.

`fmt.ignorePatterns` participates in CLI file discovery only. It is not part of `FormatOptions`, and message-level APIs do not perform file selection.

Formatter configuration does not support file-specific `overrides` in the initial design. The first formatter target is a narrow direct `.mf2` message-file workflow, so per-file option overrides are unnecessary until resource/catalog requirements prove otherwise.

The formatter does not read `.editorconfig` in the initial implementation because `mode` is the only supported formatting option. `.editorconfig` fallback becomes active only when formatter options such as line width, indent width, line ending, or final newline are explicitly supported.

Configuration precedence for formatting mode is:

1. CLI `--mode`
2. `fmt.mode`
3. default `"standard"`

Config files are validated as a whole before CLI overrides are applied. For example, an invalid `fmt.mode` in config still produces `config_validation_failed` even when `--mode standard` is provided. CLI argument shape and value validation happens before config loading when the invalid value can be detected from argv alone, so `--mode compact` is `invalid_cli_argument`.

Formatter-specific config schema definitions live under the unified project config schema, for example `definitions.fmt`, and are published through `@intlify/cli/schema/config.schema.json`. Phase 3B does not publish generated TypeScript config types.

Config validation errors use the Phase 3A `config_validation_failed` operational error, with JSON pointers such as `/fmt/mode` or `/fmt/ignorePatterns`. Invalid CLI values such as `--mode compact` use `invalid_cli_argument` with details such as the option name, provided value, and allowed values.

`fmt.ignorePatterns` uses the same gitignore-compatible subset described in [Ignore Sources](#ignore-sources). Invalid entries are rejected during config validation with pointers such as `/fmt/ignorePatterns/0`.

The resolved message-level `FormatOptions` receives only `mode`. CLI-only file selection settings such as `fmt.ignorePatterns`, root `.gitignore`, and `--ignore-path` remain outside `FormatOptions`.

## Options

Initial options stay intentionally small:

```text
FormatOptions {
  mode: standard | preserve
}
```

Default `mode` is `standard`.

For N-API and WASM, omitted `options` uses the default options. `null` options are invalid. Unknown option fields are invalid to catch typos; `details.pointer` points at the unknown field, `details.reason` is `"unknown_field"`, and `details.allowedFields` may include `["mode"]`.

The Rust API accepts typed `FormatOptions` and should not expose invalid runtime option states. Conversion from raw external inputs, such as N-API values, WASM values, CLI strings, or config data, validates before constructing typed formatter options. Invalid CLI and config input use their CLI/config error codes; invalid programmatic binding options use `invalid_options`.

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

The message-level core must not format by directly concatenating strings during SnapshotView traversal. It should build an internal layout/document representation before rendering text. The layout model should support delayed line, group, and indent decisions so standard mode, preserve mode, future line width support, and future resource/catalog adapters can reuse one formatter core.

The exact IR node shape, printing algorithm, and line-breaking strategy are intentionally left to implementation design. The public contract is that callers format whole MF2 messages and receive either formatted source/check information or diagnostics/errors.

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

`@intlify/format-wasm` is browser-first for playground, worker, and browser tooling use cases. Node users should prefer `@intlify/format-napi`. After `await init()`, the WASM package exposes synchronous `formatMessage`, `checkFormat`, `formatSnapshot`, and `checkSnapshot` APIs. Calling these APIs before `init()` is a usage error and may throw. Repeated `init()` calls are idempotent. The exact `init(input?)` loading parameter shape is implementation-defined in Phase 3B.

New `@intlify/format-*` npm packages may require token-based bootstrap publishing for the first release. After the packages exist on npm and trusted publisher settings are configured, normal releases should use npm trusted publishing.

Parser binding packages remain focused on parser-level APIs. Formatter APIs are not added to existing `@intlify/ox-mf2-napi` or `@intlify/ox-mf2-wasm` packages.

Formatter binding packages do not have runtime dependencies on parser binding packages. `@intlify/format-napi` does not depend on `@intlify/ox-mf2-napi`, and `@intlify/format-wasm` does not depend on `@intlify/ox-mf2-wasm`. Snapshot reuse crosses package boundaries through serialized Binary AST snapshot bytes (`Uint8Array`) plus source text. Formatter packages perform their own snapshot version and capability checks.

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

A future formatter suppression mechanism must be spec-compatible. Possible directions include an attribute-based mechanism for expression/markup units or a future resource/container-level convention such as namespaced resource metadata tags like `@intlify:*`, but no syntax-unit formatter directive is part of the initial formatter product.

## Matcher Layout

Matcher layout uses a table-like layout in both standard and preserve mode.

Rules:

- multi-selector matchers use table-like variant rows
- single-selector matchers also align variant rows
- variant keys align by each key column's maximum width
- `.match` selector variables are not aligned to variant key columns
- key columns have at least 2 spaces between them
- the final key column and the variant value pattern start have at least 2 spaces between them
- preserve mode may preserve existing single-line/multi-line shape and blank-line grouping, but still normalizes matcher rows to the table-like spacing rules

Matcher column width is measured using Unicode display width, not UTF-8 byte length. Literal key width is measured from the raw source spelling that the formatter will emit, not from a decoded literal value. For example, escaped characters and quoted-literal delimiters count according to the emitted source slice. Row alignment applies only to variant key columns and the value pattern start. The value pattern's internal formatting is handled by normal pattern formatting, regardless of whether the pattern starts with quoted text, an expression, or markup. Phase 3B does not wrap long value patterns.

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

The formatter first consumes the existing Binary AST `SnapshotView` / binding-side accessor model. Rust, N-API, and WASM SnapshotView surfaces should provide the same logical accessor contract even if their concrete implementations differ.

Initial required helper semantics:

- node children are traversed in source order
- token traversal is available in source order
- public node and token kind accessors expose stable symbolic names; numeric discriminants are internal
- node and token spans are UTF-8 byte spans using half-open ranges `[start, end)`
- source slicing is available through a `slice(span)`-style helper after source/snapshot consistency has been established
- delimiter spans are exposed through delimiter token kind and span access; dedicated delimiter-specific accessors are not required
- token raw text is not duplicated in the snapshot; consumers use token spans with `slice(span)`
- token-level leading and trailing whitespace trivia spans are available for preserve mode
- derived trivia information such as line-break counts or blank-line counts is computed by the formatter from trivia spans and source slices
- parser diagnostic access
- recovered or missing node/token flags are not formatter requirements because snapshots with parser diagnostics do not produce formatted output
- source/snapshot consistency checks are required when the snapshot carries a source hash or equivalent source identity; otherwise checks are best-effort

`formatMessage(source)` parses the same source it formats, so a public source/snapshot mismatch cannot occur. Span and UTF-8 boundary validation still happens as internal formatter validation.

`formatSnapshot(snapshot, source)` and `checkSnapshot(snapshot, source)` verify source identity when the snapshot carries a source hash, source length, or equivalent source identity. A mismatch returns `source_snapshot_mismatch`. Phase 3B does not require snapshots to carry source identity data. If the snapshot has no source identity, validation is best-effort: corrupt bytes, unsupported versions, missing formatter capabilities, invalid spans, or non-UTF-8 boundaries detected before IR construction return `invalid_snapshot`. Source/span contradictions discovered after IR construction has accepted supposedly consistent input become `internal_error`.

If formatter implementation needs additional public snapshot accessors, those accessors may be added in the formatter PR that needs them. Additions should be limited to the minimum formatter-required surface.

If the formatter requires a Binary AST snapshot format change, the formatter PR may include it, but it must also update snapshot versioning, compatibility policy, parser snapshot round-trip tests, parser snapshot compatibility tests, N-API/WASM exposure, and any affected fixtures. Snapshot format changes should remain narrowly scoped to formatter requirements. If the required snapshot change becomes large enough to obscure the formatter review, split it into a separate prerequisite PR.

## Fixture Strategy

Formatter fixtures should be reviewable and stable.

Core formatter fixtures and CLI fixtures are separate. Core fixtures test the `intlify_format` API, parser diagnostics policy, idempotency, reparsing, and future SemanticView preservation. CLI fixtures test stdout, stderr, exit codes, JSON reporter output, discovery, ignore behavior, stdin, and file mutation behavior.

### Minimum Coverage Matrix

Coverage is scoped by implementation PR. A row is `MUST` for the PR that implements the corresponding behavior; before that PR, the row is either not applicable or `SHOULD` only when it can be covered without pulling future work forward.

Syntax coverage:

| Coverage area | Required by | Requirement |
| --- | --- | --- |
| `.input` and `.local` declarations | Core formatter rules PR | `MUST` |
| expressions with operands and functions | Core formatter rules PR | `MUST` |
| options and attributes | Core formatter rules PR | `MUST` |
| markup syntax | Core formatter rules PR | `MUST` |
| matcher table layout | Core formatter rules PR | `MUST` |
| quoted patterns and whitespace-sensitive pattern text | Core formatter rules PR | `MUST` |
| quoted literal spelling, unquoted literal spelling, and escape spelling preservation | Core formatter rules PR | `MUST` |
| parser diagnostics that produce no formatted output | Fixture harness / core formatter PRs | `MUST` |
| preserve-mode single-line / multi-line source shape | Preserve formatter rules PR | `MUST` |
| preserve-mode blank-line grouping and leading/trailing trivia | Preserve formatter rules PR | `MUST` |

Behavior coverage:

| Coverage area | Required by | Requirement |
| --- | --- | --- |
| `format_message` output and `check_format` changed/unchanged behavior | Core formatter PRs | `MUST` |
| idempotency of formatted output | Core formatter PRs | `MUST` |
| formatted output reparses with zero parser diagnostics | Core formatter PRs | `MUST` |
| parser diagnostics return no public formatted output | Core formatter PRs | `MUST` |
| `intlify fmt` write mode | CLI formatter PR | `MUST` |
| `intlify fmt --check` and `--list-different` | CLI formatter PR | `MUST` |
| stdin formatting and stdin check mode | CLI formatter PR | `MUST` |
| JSON reporter success, difference, diagnostic, and operational-error output | CLI formatter PR | `MUST` |
| explicit file, directory, glob, duplicate, and stable path ordering behavior | CLI formatter PR | `MUST` |
| unsupported file, unmatched input, all ignored, and no selected file behavior | CLI formatter PR | `MUST` |
| ignore source precedence across root `.gitignore`, `--ignore-path`, and `fmt.ignorePatterns` | CLI formatter PR | `MUST` |
| N-API `formatMessage`, `checkFormat`, `formatSnapshot`, and `checkSnapshot` contract | N-API binding PR | `MUST` |
| WASM `formatMessage`, `checkFormat`, `formatSnapshot`, and `checkSnapshot` contract | WASM binding PR | `MUST` |
| local benchmark command execution and result schema validation | Benchmark PR | `MUST` |

Core fixtures live under `crates/intlify_format/fixtures`.

Valid core fixtures use directory fixtures:

```text
crates/intlify_format/fixtures/
  matcher_table/
    input.mf2
    standard.mf2
    preserve.mf2
    options.json
```

`options.json` is required and uses a strict `cases[]` array. Unknown fields are fixture authoring errors.

```json
{
  "cases": [
    {
      "name": "standard",
      "options": { "mode": "standard" },
      "expected": "standard.mf2"
    },
    {
      "name": "preserve",
      "options": { "mode": "preserve" },
      "expected": "preserve.mf2"
    }
  ]
}
```

Each valid case must provide `expected`. There is no implicit fallback from preserve mode to `standard.mf2`; expected output is always explicit.

Invalid core fixtures do not include formatted output:

```text
crates/intlify_format/fixtures/
  invalid_unclosed/
    input.mf2
    diagnostics.json
    options.json
```

Invalid `diagnostics.json` stores a diagnostics summary rather than full parser diagnostic text:

```json
{
  "diagnosticCount": 1
}
```

Invalid cases use `expectedDiagnostics`:

```json
{
  "cases": [
    {
      "name": "standard",
      "options": { "mode": "standard" },
      "expectedDiagnostics": "diagnostics.json"
    }
  ]
}
```

A core fixture directory is either valid or invalid. Mixed `expected` and `expectedDiagnostics` cases in one fixture directory are fixture authoring errors.

Core fixture assertions:

- each valid case formats `input.mf2` to its `expected` file
- each valid case is idempotent when formatting its own expected output with the same options
- each valid expected output reparses with zero parser diagnostics
- valid input has no parser diagnostics
- each invalid case returns `ok: false`, produces no public formatted output, and matches its diagnostics summary
- SemanticView preservation is checked for every valid case once SemanticView is available; until then this assertion is skipped

Core fixture updates use:

```sh
INTLIFY_UPDATE_FORMAT_FIXTURES=1 cargo test -p intlify_format
```

The update mode updates expected `.mf2` files, invalid `diagnostics.json` summaries, and declared layout dump pairs (`*.layout.before.txt` and `*.layout.after.txt`). It does not rewrite fixture `options.json`. Layout dump files are selective and are not auto-created; if only one file in a dump pair exists, update mode still treats the fixture as an authoring error.

CLI fixtures live under `packages/cli/fixtures/fmt` and use one directory per fixture case:

```text
packages/cli/fixtures/fmt/
  write-changed/
    input/
    expected/
      write/
    write.stdout
    options.json
```

For each CLI scenario, the runner copies the contents of `input/` into a temporary directory root and runs the intlify CLI with that temporary directory as cwd. Scenario `args` contain subcommand arguments only; the runner supplies the intlify binary path.

CLI `options.json` uses a strict `scenarios[]` array:

```json
{
  "scenarios": [
    {
      "name": "write",
      "args": ["fmt", "."],
      "exitCode": 0,
      "stdout": "write.stdout",
      "stderr": "write.stderr",
      "expectedTree": "expected/write"
    }
  ]
}
```

Scenario path fields are relative to the fixture case directory. These fields include `stdin`, `stdout`, `stderr`, `stdoutJson`, and `expectedTree`.

CLI scenario rules:

- `stdout` and `stderr` are optional; omitted fields mean empty expected output
- `stdoutJson` parses stdout as JSON and compares it structurally against the expected JSON file
- `stderrJson` is not supported
- `stdin` points to a file used as process stdin
- `expectedTree` is optional; when omitted, the runner asserts that the input tree remains unchanged
- if `expectedTree` is omitted and the tree changes, the test fails even in update mode
- scenario-level `env` is not supported initially

CLI fixture updates use the same `INTLIFY_UPDATE_FORMAT_FIXTURES=1` environment variable. The update mode updates declared `stdout`, `stderr`, `stdoutJson`, and `expectedTree` artifacts. It does not add missing output fields or rewrite `options.json`; undeclared stdout/stderr output remains a test failure.

Fixture authoring errors are hard test failures. Examples include malformed `options.json`, missing required files, mixed valid/invalid core cases, unknown fields, unsupported scenario fields, and expected files that do not exist.

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

WASM bundle size reporting is not a Phase 3B core implementation gate. The initial formatter design does not set a numeric size budget for `@intlify/format-wasm`, and size changes should not fail CI. After `@intlify/format-wasm` exists, follow-up reporting should keep WASM artifact size and JavaScript glue size observable so later releases can compare browser-facing cost over time.

GitHub Actions benchmark jobs and issue-comment reporting are deferred follow-up work. The Phase 3B implementation should not add benchmark runtime to normal `vpr check`, `vpr test`, or default CI gates.

Phase 3B does not publish public command-specific output JSON Schemas while `schemaVersion` is `"0"`. It also does not create internal test-only output schemas. Formatter JSON reporter shape is verified through typed Rust structs and JSON fixtures.

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
- Future formatter IR changes beyond the Phase 3B design in [011-ox-mf2-formatter-ir-design.md](./011-ox-mf2-formatter-ir-design.md), such as width-aware wrapping or additional document primitives.
- `.editorconfig` loading once formatter options exist that can consume it.
- Line wrapping and style options such as `lineWidth`, `indentWidth`, `lineEnding`, `finalNewline`, and quote/literal spelling policy.
- Future `fmt` config option expansion beyond `mode` and `ignorePatterns`, including whether line width, indentation, line endings, final newline, matcher layout, literal spelling, include/exclude, or formatter-specific check behavior should become user-configurable.
- A root-level `projectRoot` config field should be reconsidered only when multi-workspace or resource/catalog requirements need a config-defined root. Phase 3B uses the Phase 3A discovered project root and does not define a project root override.
- Generated TypeScript config type distribution.
- Nested config discovery, nearest-config-wins behavior, file-specific overrides, `--cwd`, and `--root`.
- `--no-error-on-unmatched-pattern` if users need a relaxed unmatched-input mode.
- Runtime controls such as `--threads`.
- `intlify init` config scaffolding once formatter and linter config fields are stable enough to write.
- GitHub Actions benchmark jobs and issue-comment benchmark reporting for parser and formatter trends.
- WASM artifact size and JavaScript glue size reporting for both `@intlify/format-wasm` and `@intlify/ox-mf2-wasm`; these measurements should remain observational until a future design sets an explicit budget.
- Publishing public command-specific output JSON Schemas after `schemaVersion` is stable enough.
- LSP/editor configuration behavior, including explicit formatter config paths and config-load failure fallback/error policy, in the LSP/editor detailed design.

## Open Questions

No formatter-specific open questions remain at this design level. Deferred items are tracked in [Deferred Follow-Up Notes](#deferred-follow-up-notes) or in later product-specific design documents.
