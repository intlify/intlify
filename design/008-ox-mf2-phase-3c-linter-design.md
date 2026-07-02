# ox-mf2 Phase 3C Linter Design

This document captures the detailed design for the ox-mf2 linter. The Phase 3 tooling and transport design fixes the high-level consumer contract; this file tracks the rule-level behavior, examples, and implementation decisions.

## Goals

- Provide a message-level linter core for MF2 messages.
- Provide a dedicated lint CLI backed by the same core.
- Include parser and semantic diagnostics in `lintMessage(source)` results.
- Keep initial rules implemented in Rust core.
- Expose stable linter results through Rust, N-API, and WASM bindings for playgrounds, editor integrations, and Node-based tools.
- Share the file discovery, ignore, file framing, exit code, and JSON envelope contracts with `intlify fmt`.
- Leave resource/catalog linting as a layer above message-level linting.

## Deliverables

Phase 3 linter deliverables:

- Rust linter engine
- CLI
- N-API linter package
- WASM linter package
- shared diagnostic result schema

LSP/editor integration and playground usage are consumers of these deliverables, not separate direct products in this phase.

## Ownership

The Rust linter engine lives in a workspace-internal crate named `crates/intlify_lint` and depends on `ox_mf2_parser`. Like `intlify_format`, this crate is not published to crates.io in Phase 3C (`publish = false`); public linter distribution happens through the `intlify lint` CLI and the linter N-API/WASM packages. The parser crate owns CST construction, parser diagnostics, Binary AST snapshots, and semantic lowering. The lint crate owns rule execution, presets, lint configuration, and lint result shaping.

The user-facing CLI binary lives in `crates/intlify_cli`. It composes the parser, formatter, and linter crates into commands such as `intlify lint`. npm packages distribute the compiled native CLI binary for JavaScript users.

N-API and WASM linter bindings are distributed as linter-specific packages backed by `crates/intlify_lint`:

- `@intlify/lint-napi`
- `@intlify/lint-wasm`

These names are symmetric with `@intlify/format-napi` and `@intlify/format-wasm`. `@intlify/lint-napi` follows the same wrapper-plus-platform-native-package model and lazy native loading as the formatter N-API package. `@intlify/lint-wasm` follows the same explicit `init()` contract as `@intlify/ox-mf2-wasm` and `@intlify/format-wasm`. Existing parser binding packages remain focused on parsing, snapshots, and parser-level APIs, and linter binding packages do not have runtime dependencies on parser or formatter binding packages.

Binding packages expose direct programmatic lint APIs. They do not host plugins and do not need a CLI callback bridge.

## Non-Goals

- JavaScript custom rules.
- A linter plugin system.
- Style or formatting fixes in lint rules.
- Recovery-aware rule execution on incomplete parser or semantic output.
- Resource/catalog rule implementation details.
- Suppression directive syntax in the first linter design.
- LSP/editor as a direct product.
- Nested config discovery.
- File-specific config overrides.
- Output formats beyond `text` and `json` in the first CLI contract.
- `lint --fix`, rule listing/introspection commands, and resolved-config printing in the first CLI version.
- Per-rule CLI severity flags such as oxlint-style `-A` / `-W` / `-D`; rule severity is controlled through `lint.rules` config.

## Pipeline

The initial linter pipeline is strict:

```text
parser -> semantic -> rules
```

Parser diagnostics are always included in lint results. If any parser diagnostic is produced, semantic lowering and configurable lint rules do not run.

Core semantic diagnostics, when produced by semantic analysis, are included after successful parsing. If semantic analysis produces any semantic diagnostic, configurable lint rules do not run.

Configurable rules only run when parsing and semantic analysis complete without diagnostics.

The zero-diagnostic guarantee in [002-ox-mf2-phase-1-rust-parser-design.md](./002-ox-mf2-phase-1-rust-parser-design.md) applies: a parse result with zero parser diagnostics is syntactically valid per the MF2 ABNF, so semantic analysis and rules never see grammar-invalid CST shapes.

## Diagnostic Classification

Diagnostic candidates are classified into four groups. This classification is fixed by this document:

- **core semantic diagnostic**: always enabled after successful parsing, independent from rule configuration, and reported as `error`. These correspond to MF2 Data Model Errors: messages that carry them are not valid MF2 and will fail or misbehave at runtime, so they must not be configurable.
- **configurable lint rule**: runs only after parser and semantic diagnostics are clean, and is controlled by `off`, `warn`, or `error`.
- **deferred**: requires more MF2 selection semantics, resource/catalog context, or editor-specific behavior before implementation.
- **internal**: not a user-facing diagnostic; reported as an operational error.

Classification result:

| Candidate | Classification | Notes |
| --- | --- | --- |
| `duplicate-declaration` | core semantic | MF2 Duplicate Declaration data model error |
| `invalid-local-dependency` | core semantic | self-reference and forward-binding cases of the Duplicate Declaration family, kept as a separate code for better messages |
| `missing-selector-annotation` | core semantic | MF2 Missing Selector Annotation data model error |
| `variant-key-arity-mismatch` | core semantic | MF2 Variant Key Mismatch data model error |
| `missing-fallback-variant` | core semantic | MF2 Missing Fallback Variant data model error |
| `duplicate-variant` | core semantic | MF2 Duplicate Variant data model error |
| `duplicate-option-name` | core semantic | MF2 Duplicate Option Name data model error |
| `no-undeclared-variable` | configurable rule, default `off` | undeclared variables are valid external inputs in MF2, so this is a strict-workflow opt-in, not an error |
| `no-unused-declaration` | configurable rule, default `warn` | declared variable never referenced |
| `no-duplicate-attribute` | configurable rule, default `warn` | the MF2 spec says attribute identifiers SHOULD be unique; duplicates are ignored with last-one-wins semantics |
| `unreachable-variant` | deferred | needs sound selection-semantics and selector-domain modeling |
| `semantic-lowering-failed` | internal | see below |

`semantic-lowering-failed` is not a user-facing diagnostic. Under the zero-diagnostic guarantee, semantic lowering must succeed for any parse result with zero parser diagnostics; a lowering failure therefore indicates an implementation bug and is reported as an `internal_error` operational error, mirroring the formatter's invariant-violation boundary.

## Diagnostic Shape

Every diagnostic carries:

- category: `parser`, `semantic`, or `lint`
- severity: `error` or `warning`
- code or rule id
- source id
- UTF-8 byte span
- message
- optional labels
- optional help text

Parser diagnostics use `category: "parser"` and the parser `DiagnosticCode` name; they have no rule id.

Semantic diagnostics use `category: "semantic"` and a stable semantic diagnostic code; they have no rule id.

Lint rule diagnostics use `category: "lint"` and a stable rule id.

## Failure Model

Lint diagnostics and operational errors are separate.

Lint diagnostics:

- parser diagnostics
- core semantic diagnostics
- configurable lint rule diagnostics

Operational errors:

- config errors (parse, validation, conflict)
- file system and encoding errors
- invalid CLI arguments
- invalid binding options
- invalid or capability-missing snapshots
- internal failures, including semantic lowering failures after a clean parse

Operational errors use the Phase 3A operational error shape `{ kind, code, message, path?, details? }` and the shared string code namespace. The CLI exit code classification follows Phase 3A: `0` success, `1` lint failure (any `error` diagnostic, or warnings over `--max-warnings`), `2` operational error, with `2 > 1 > 0` priority for mixed outcomes. JSON output uses the Phase 3A top-level envelope, including its top-level `errors` array for global operational errors and `results[].errors` for file-specific operational errors.

## Stable Identifiers

Semantic diagnostic codes and configurable lint rule ids are public stable identifiers. Config files, future suppression directives, JSON output, editor integrations, and external tools may depend on them.

Naming rules:

- Semantic diagnostic codes are noun-phrase kebab-case: `duplicate-declaration`, `missing-fallback-variant`.
- Configurable rule ids are kebab-case and use a `no-` prefix when the rule forbids something: `no-undeclared-variable`, `no-unused-declaration`, `no-duplicate-attribute`.
- There is no namespace prefix. Plugins are a non-goal, and future resource/catalog rules can add a category-style prefix later if needed.
- There is no alias or deprecation mechanism before 1.0. Renaming an identifier is a breaking change and requires a normal breaking-change release process.

Diagnostic message text is not a stable compatibility surface and may change for clarity.

## Rule Metadata

The lint crate exposes rule metadata for the CLI, bindings, generated documentation, and JSON Schema generation.

Metadata includes at least:

- rule id
- category
- default/recommended status
- default severity
- fix capability (always `false` in the initial rules)
- docs slug, generated from the rule id
- rule option schema when a rule accepts options

No initial rule accepts options, so rule option schemas are an empty surface in Phase 3C. The exact Rust metadata struct is an implementation detail; the JSON-visible metadata fields above are the compatibility surface.

## Severity

Rule configuration uses an ESLint/oxlint-style state:

- `off`: disable a configurable rule
- `warn`: report configurable rule diagnostics as warnings
- `error`: report configurable rule diagnostics as errors

`off` is not an emitted severity.

Parser and semantic diagnostics are independent from rule configuration and are emitted as `error`. Future compatibility, deprecation, or best-practice diagnostics may use `warning`.

`info` and `hint` are reserved for LSP/editor or advice-style layers.

## Presets

The initial preset is `recommended`. It is applied implicitly as the default rule configuration; there is no `preset` config field until additional presets actually exist.

`recommended` enables:

- `no-unused-declaration`: `warn`
- `no-duplicate-attribute`: `warn`

`no-undeclared-variable` defaults to `off` and is not part of `recommended`, because undeclared variables are valid external inputs in MF2.

While the linter remains in 0.x, `recommended` may evolve by adding useful diagnostics as needed. Preset stability policy should be finalized before a 1.0 release. `strict`, `nursery`, `experimental`, and resource/catalog-oriented presets are future design work.

## Config Contract

Project configuration is JSON, not JavaScript or TypeScript. Lint settings live in the `lint` section of one ox-mf2 tooling config shared with formatter settings. The Rust linter engine is the source of truth for the resolved config model.

Initial config discovery is root-only and follows the Phase 3A CLI foundation contract. Nearest-config-wins and nested config discovery are not part of the initial design. Initial linter config does not support file-specific `overrides`.

Initial lint config supports:

```json
{
  "lint": {
    "rules": {
      "no-undeclared-variable": "warn"
    },
    "ignorePatterns": []
  }
}
```

Schema-level lint config rules:

- `lint` must be an object; the Phase 3A required-section and unknown-field rules apply
- `lint.rules` is optional and defaults to `{}`
- `lint.rules` keys must be known configurable rule ids; unknown rule ids are `config_validation_failed` errors
- `lint.rules` values accept only the strings `"off"`, `"warn"`, or `"error"`; the ESLint-style `["warn", { ... }]` tuple form is reserved for future rules with options and is invalid in Phase 3C
- semantic diagnostic codes are not accepted as `lint.rules` keys; core semantic diagnostics are not configurable
- `lint.ignorePatterns` is optional, defaults to `[]`, and uses the same gitignore-compatible subset and validation rules as `fmt.ignorePatterns`

The resolved configuration is the implicit `recommended` defaults overlaid with `lint.rules`. `crates/intlify_lint` owns the rule registry, default severities, preset expansion, config defaults, and resolved config validation. The CLI loads JSON config files and passes the resolved data through the Rust config model. N-API and WASM entry points accept equivalent structured options and normalize them through the same Rust validation path; invalid binding options use `invalid_options`.

The lint schema definitions live under the unified project config schema published through `@intlify/cli/schema/config.schema.json`.

## File Discovery and Shared CLI Contract

`intlify lint` shares the `intlify fmt` contract for everything that is not lint-specific:

- the primary input unit is `1 file = 1 MF2 message`; the supported-extension list is initially `.mf2` only
- input rules, hidden-file and VCS/dependency-directory exclusion, symlink behavior, duplicate de-duplication, stable slash-normalized ordering, unmatched-input errors, zero-target success, and invalid-glob handling follow the `intlify fmt` Input Discovery contract
- ignore sources are one ordered pattern list: root `.gitignore`, then `--ignore-path` files in CLI argument order, then `lint.ignorePatterns`, with later patterns overriding earlier ones
- read framing follows the `intlify fmt` File Framing contract: one leading UTF-8 BOM and then one trailing `LF` or `CRLF` are removed before parsing, so lint spans match fmt spans for the same file; lint never writes files, so write framing does not apply
- non-UTF-8 input reports `input_read_failed` with `details.reason: "invalid_utf8"`
- Phase 3C processes selected files sequentially; future parallel execution must not change observable output ordering
- exit codes and the JSON envelope follow Phase 3A

Resource/catalog input such as JSON and YAML i18n files is planned as a layered adapter workflow. When resource/catalog adapters arrive, they extend the supported-extension list and own host-file parsing, message extraction, and span mapping; the message-level linter core and this shared discovery contract do not change.

## CLI Detailed Behavior

The CLI command is `intlify lint`.

Initial CLI flags:

- `--max-warnings <n>`
- `--quiet`
- `--stdin-filepath <path>`
- `--ignore-path <path>`; may be provided multiple times
- `--reporter <text|json>`

The flag surface intentionally mirrors oxlint's basic flags plus the oxfmt-style explicit stdin mode already adopted by `intlify fmt`. Per-rule CLI severity flags (`-A` / `-W` / `-D`) are intentionally not provided; rule severity lives in `lint.rules`.

Flag semantics:

- `--max-warnings <n>`: the CLI exits with `1` when the total warning count exceeds `n`, even when no `error` diagnostics are reported. The default is unlimited.
- `--quiet`: `warning` diagnostics are not reported in text or JSON output. Exit code behavior does not change: `--max-warnings` still counts suppressed warnings, and summary counts still include them.
- `--stdin-filepath <path>`: explicit stdin mode with the same semantics as `intlify fmt`: reads all source text from stdin, applies read framing, uses `<path>` as the virtual input path for extension checks, ignore rules, and output, and cannot be combined with file, directory, or glob operands.
- `--ignore-path <path>`: same resolution and pattern rules as `intlify fmt`.
- `--reporter <text|json>`: Phase 3A reporter selection.
- `--max-warnings`, `--quiet`, `--stdin-filepath`, and `--reporter` are not repeatable; duplicates are `duplicate_cli_option` errors.
- `--` end-of-options handling follows `intlify fmt`.

When no operands are provided and stdin mode is not selected, `intlify lint` behaves as `intlify lint .`.

Human-readable output renders diagnostics to stderr-style problem output and keeps stdout machine-friendly, following the Phase 3A text reporter conventions; exact human wording is not a fixture-locked contract. JSON output uses the Phase 3A envelope with `command: "lint"`. Result entries carry the project-root-relative slash-normalized path and that file's diagnostics; summary counts include at least `status`, `matchedFiles`, `errorCount` for operational errors, and lint `error` / `warning` diagnostic counts. The exact JSON field set is locked by fixtures in the CLI implementation PR, following the same conventions as the fmt JSON reporter.

Deferred CLI features: `lint --fix`, rule listing/introspection commands, resolved-config printing, file discovery debugging, rule timing output, additional reporters (including GitHub annotations and SARIF), and concurrency controls such as `--threads`.

## Programmatic API Shape

The primary Rust APIs mirror the formatter split between source-backed and snapshot-backed entry points:

```rust
lint_message(source: &str, options: LintOptions) -> LintResult
lint_snapshot(snapshot: SnapshotView<'_>, source: &str, options: LintOptions) -> LintResult
```

N-API and WASM expose the same contract as a discriminated union, using the Phase 3A operational error shape:

```ts
type LintResult =
  | { ok: true; diagnostics: LintDiagnostic[]; errorCount: number; warningCount: number }
  | { ok: false; errors: OperationalError[] }

function lintMessage(source: string, options?: LintOptions): LintResult
function lintSnapshot(snapshot: Uint8Array, source: string, options?: LintOptions): LintResult
```

`ok: true` results always include parser, semantic, and rule diagnostics in one flat array with category markers; a message with parser diagnostics is still an `ok: true` lint result. `ok: false` is reserved for operational errors such as invalid options, invalid snapshots, or internal failures.

`LintOptions` carries the resolved rule severities. Message-level APIs do not perform file selection; `lint.ignorePatterns` is CLI-only, matching the formatter's `FormatOptions` boundary. Programmatic API sources are treated as whole messages: no file framing is applied, matching `formatMessage`.

Snapshot-backed linting follows the formatter's snapshot input constraints: the snapshot must carry verifiable diagnostic capability (a snapshot that may have omitted diagnostics returns `invalid_snapshot` with `details.reason: "missing_capability"`), source consistency checks are best-effort, and corrupt or unsupported snapshots return `invalid_snapshot`. Linting does not require trivia, so trivia-less snapshots are accepted.

`@intlify/lint-wasm` follows the `@intlify/ox-mf2-wasm` initialization contract as specified for `@intlify/format-wasm` in [007-ox-mf2-phase-3b-formatter-design.md](./007-ox-mf2-phase-3b-formatter-design.md).

## Core Semantic Diagnostics

Core semantic diagnostics are always enabled after successful parsing, are reported as `error`, and are not configurable. They correspond to MF2 Data Model Errors.

### duplicate-declaration

Reports a declaration that binds a variable that already appeared in a previous declaration. `.input` and `.local` share one variable namespace, per the MF2 declaration rules.

```mf2
.input {$count :number}
.input {$count :number}
{{{$count}}}
```

```mf2
.local $label = {$count}
.local $label = {|items|}
{{{$label}}}
```

Duplicate declarations are always semantic errors; there is no compatibility relaxation. The primary span is the later declaration's bound variable, with a label on the earlier declaration.

### invalid-local-dependency

Reports `.local` declarations that violate MF2 declaration dependency rules: a declaration must not bind a variable that appears in its own expression, and must not bind a variable that already appeared in a previous declaration's expression. Self-references, forward references that are later bound, and therefore all dependency cycles are invalid — including acyclic-looking forward references, which the MF2 Duplicate Declaration rules still prohibit.

```mf2
.local $label = {$label}
{{{$label}}}
```

```mf2
.local $a = {$b}
.local $b = {$a}
{{{$a}}}
```

The primary span is the bound variable of the declaration that completes the violation, with labels on the earlier appearances.

### missing-selector-annotation

Reports a selector variable that does not directly or indirectly (through `.local` chains) reference a declaration with a function.

```mf2
.input {$count}
.match $count
one {{One item}}
* {{Items}}
```

### variant-key-arity-mismatch

Reports a matcher variant whose key count does not match the selector count. The parser accepts arbitrary key counts syntactically, so this stays a semantic diagnostic. The primary span is the offending variant's key list, with a label on the selector list.

```mf2
.match $gender $count
male {{He has items.}}
* * {{Fallback}}
```

```mf2
.match $count
one few {{Items}}
* {{Fallback}}
```

### missing-fallback-variant

Reports a matcher without a fallback variant. Per the MF2 rule, at least one variant must have all keys equal to the catch-all key `*`, regardless of selector functions or selector domains.

```mf2
.match $count
0 {{No items}}
1 {{One item}}
```

```mf2
.match $gender $count
male 1 {{He has one item}}
female 1 {{She has one item}}
```

### duplicate-variant

Reports duplicate variant key tuples. Literal keys are compared by their cooked string values after the NFC normalization rule defined in the Phase 1 parser design, not by syntactical appearance, so `1` and `|1|` collide.

```mf2
.match $count
1 {{One item}}
|1| {{Single item}}
* {{Items}}
```

### duplicate-option-name

Reports duplicate option identifiers within one function. Per the MF2 rule, option identifiers must be unique within a function; duplicates are a Duplicate Option Name data model error.

```mf2
{$count :number minimumFractionDigits=2 minimumFractionDigits=3}
```

## Configurable Rules

Initial configurable rules avoid style concerns. Style checks and formatting fixes are delegated to the formatter API/crate.

### no-unused-declaration

Category: `best-practice`. Default: `warn`, enabled in `recommended`.

Reports a declared variable that is never referenced by a later declaration expression, a selector, or the message body.

```mf2
.input {$count :number}
.local $unused = {$count}
{{You have {$count} items.}}
```

### no-duplicate-attribute

Category: `best-practice`. Default: `warn`, enabled in `recommended`.

Reports repeated attribute identifiers on one expression or markup placeholder. The MF2 spec says attribute identifiers SHOULD be unique and defines last-one-wins semantics for duplicates, so this is a warning-level rule rather than a semantic error.

```mf2
{$name :string @note=|a| @note=|b|}
```

### no-undeclared-variable

Category: `correctness`. Default: `off`, not in `recommended`.

Reports a variable reference that cannot be resolved to a visible `.input` or `.local` declaration. Undeclared variables are valid external input variables in MF2, so this rule is an opt-in for teams that adopt a declare-all-inputs workflow. Enabling it also catches declaration typos such as declaring `$count` and referencing `$conut`.

```mf2
.input {$count :number}
{{You have {$total} items.}}
```

Simple messages with no declarations reference external inputs by design; teams that enable this rule accept that such messages must move to `.input` declarations.

## Deferred Diagnostics

### unreachable-variant

Reports variants that cannot be selected. Deferred: it must only report cases that are provably unreachable from MF2 selection semantics and known selector domains, which requires selector-function domain modeling that is out of scope for Phase 3C.

## Rule Categories

- `correctness`: checks that catch likely-broken messages beyond core semantic diagnostics
- `best-practice`: maintainability or translation workflow checks
- `resource`: future resource/catalog-level checks

Categories are rule metadata, not part of rule ids, so a rule can be recategorized without a breaking change.

## Resource and Catalog Linting

Message-level linting is the core. Resource/catalog linting for host formats such as JSON and YAML i18n files is planned as a layer above message-level linting that extracts message entries and reuses `lintMessage(source)` per entry.

Future resource/catalog examples:

- missing translation keys across locales
- placeholder mismatch across locales
- variant coverage mismatch across locales
- duplicate message ids
- unused messages

## Formatter Interaction

Lint rules do not implement style fixes. Future `lint --fix` behavior should call formatter APIs for style-related fixes so style decisions remain consistent across formatter, linter, and editor integrations.

## Suppression Directives

Directive syntax is not fixed yet. The linter result model should be able to represent suppressed diagnostics later, but the first implementation does not need a concrete MF2 suppression comment syntax.

## Fixtures and Validation

Core linter fixtures live under `crates/intlify_lint/fixtures` and follow the same directory-fixture and explicit-update-flag conventions as the formatter fixture harness, with diagnostics expectations instead of formatted output. CLI fixtures for `intlify lint` follow the `packages/cli` fixture conventions established by the fmt CLI fixtures. Exact harness details are locked in the implementation PRs.

Minimum coverage includes: every core semantic diagnostic with positive and negative cases, every configurable rule in `off` / `warn` / `error` states, preset default behavior, parser-diagnostic short-circuiting, `--max-warnings` and `--quiet` behavior, stdin mode, ignore precedence, JSON reporter output, and binding parity for `lintMessage` / `lintSnapshot`.

## Benchmarks

Linter benchmarks are local-first tooling under `tools/`, following the parser and formatter benchmark patterns. They should separate parse cost, semantic analysis cost, rule execution cost (per rule where practical), binding call cost, and CLI end-to-end cost, matching the Phase 3 benchmark names `lint_message_core`, `lint_snapshot_core`, `lint_cli_e2e`, `lint_json`, `lint_binding_napi`, and `lint_binding_wasm`. Benchmark commands must be executable and testable, but timings are not CI gates.

## Implementation Phasing

Phase 3C linter implementation should be split into reviewable PRs:

1. `intlify_lint` crate scaffold, result/options/config model, rule registry, and fixture harness
2. core semantic diagnostics
3. configurable rules and the `recommended` preset
4. `intlify lint` CLI integration, shared discovery/ignore/framing reuse, and JSON reporter
5. `@intlify/lint-napi` wrapper and platform native packages
6. `@intlify/lint-wasm`
7. local-first linter benchmarks under `tools/`

Each PR should be cut from `main` and keep linter work separated from formatter work. Shared CLI infrastructure (discovery, ignore, framing, envelope) introduced by the fmt CLI PR should be reused, not duplicated.

## Deferred Follow-Up Notes

- Suppression model: inline disable directives, unused directive diagnostics, and baseline suppression files.
- Fix model: safe fixes, suggestions, dangerous fixes, and how non-style fixes interact with formatter-owned style changes.
- Machine reporter roadmap beyond `text` and `json`, including GitHub annotations and SARIF.
- CLI inspection and debug modes: rule listing, resolved config printing, file discovery debugging, and rule timing output.
- Rule options and the `["severity", { options }]` config tuple form, once a rule needs options.
- `preset` config field or preset composition, once presets beyond `recommended` exist.
- `unreachable-variant` selection-semantics modeling.
- Resource/catalog adapters for JSON/YAML host files and resource-level rules.
- Parallel file linting with deterministic output, and concurrency controls.

## Open Questions

No linter-specific open questions remain at this design level. Deferred items are tracked in [Deferred Follow-Up Notes](#deferred-follow-up-notes) or in later product-specific design documents.
