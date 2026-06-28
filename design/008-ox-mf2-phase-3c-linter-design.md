# ox-mf2 Phase 3C Linter Design

This document captures the detailed design for the ox-mf2 linter. The Phase 3 tooling and transport design fixes the high-level consumer contract; this file tracks the rule-level behavior, examples, and implementation decisions that need more detail before implementation.

## Goals

- Provide a message-level linter core for MF2 messages.
- Provide a dedicated lint CLI backed by the same core.
- Include parser and semantic diagnostics in `lintMessage(source)` results.
- Keep initial rules implemented in Rust core.
- Expose stable linter results through Rust, N-API, and WASM bindings for playgrounds, editor integrations, and Node-based tools.
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

The Rust linter engine lives in `crates/ox_mf2_lint` and depends on `ox_mf2_parser`. The parser crate owns CST construction, parser diagnostics, Binary AST snapshots, and semantic lowering. The lint crate owns rule execution, presets, lint configuration, and lint result shaping.

The user-facing CLI binary lives in `crates/ox_mf2_cli`. It composes the parser, formatter, and linter crates into commands such as `ox-mf2 lint`. npm packages distribute the compiled native CLI binary for JavaScript users.

N-API and WASM linter bindings are distributed as linter-specific packages backed by `crates/ox_mf2_lint`. Existing parser binding packages remain focused on parsing, snapshots, and parser-level APIs.

The npm distribution surface is split by consumer:

- CLI package: distributes compiled native binaries for command-line use.
- N-API linter package: exposes Node APIs.
- WASM linter package: supports browser, worker, and playground use cases.

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

## Pipeline

The initial linter pipeline is strict:

```text
parser -> semantic -> rules
```

Parser diagnostics are always included in lint results. If any parser diagnostic is produced, semantic lowering and configurable lint rules do not run.

Semantic diagnostics, when produced by semantic lowering, are included after successful parsing. If semantic lowering produces any semantic diagnostic, configurable lint rules do not run.

Rules only run when parsing and semantic lowering complete without diagnostics.

## Diagnostic Shape

Every diagnostic should carry:

- category: `parser`, `semantic`, or `lint`
- severity: `error` or `warning`
- code or rule id
- source id
- UTF-8 byte span
- message
- optional labels
- optional help text

Parser diagnostics use `category: "parser"` and no rule id, unless an output format requires a reserved parser rule id.

Semantic diagnostics use `category: "semantic"` and no rule id, unless an output format requires a reserved semantic rule id.

Lint rule diagnostics use `category: "lint"` and a stable rule id.

## Failure Model

Lint diagnostics and operational errors are separate.

Lint diagnostics:

- parser diagnostics
- semantic diagnostics
- configurable lint rule diagnostics

Operational errors:

- JSON config parse errors
- invalid config shape
- file system errors
- snapshot version mismatches
- internal failures

Operational errors are returned as CLI/API errors rather than mixed into normal lint diagnostic results.

Open decisions:

- CLI exit code classification for operational errors
- N-API/WASM error object shape
- whether JSON output can include a top-level operational error envelope

## Stable Identifiers

Semantic diagnostic codes and configurable lint rule ids are public stable identifiers. Config files, suppression directives, JSON output, editor integrations, and external tools may depend on them.

Diagnostic message text is not a stable compatibility surface and may change for clarity.

Open decisions:

- semantic diagnostic code naming scheme
- configurable rule id naming scheme
- whether rule ids use a package namespace
- how aliases or deprecations are represented

## Rule Metadata

The lint crate should expose rule metadata for the CLI, bindings, generated documentation, and JSON Schema generation.

Metadata includes at least:

- rule id
- category
- default/recommended status
- default severity
- fix capability
- documentation link or docs slug
- rule option schema when a rule accepts options

Open decisions:

- exact rule metadata struct
- category names
- whether documentation links are generated from rule ids
- how rule option schemas are generated and published

## Severity

Rule configuration uses an ESLint/oxlint-style state:

- `off`: disable a configurable rule
- `warn`: report configurable rule diagnostics as warnings
- `error`: report configurable rule diagnostics as errors

`off` is not an emitted severity.

Parser and semantic diagnostics are independent from rule configuration. Initial parser and semantic diagnostics are emitted as `error`. Future compatibility, deprecation, or best-practice diagnostics may use `warning`.

`info` and `hint` are reserved for LSP/editor or advice-style layers.

## Presets

The initial preset is `recommended`, focused on broadly useful correctness diagnostics.

While the linter remains in 0.x, `recommended` may evolve by adding useful diagnostics as needed. Preset stability policy should be finalized before a 1.0 release.

Open decisions:

- exact rule set for `recommended`
- future `strict`, `nursery`, `experimental`, or resource/catalog-oriented presets

## Config Contract

Project configuration is JSON, not JavaScript or TypeScript. Lint settings live in the `lint` section of one ox-mf2 tooling config shared with formatter settings. The Rust linter engine is the source of truth for the resolved config model.

Initial config discovery is root-only. The CLI loads only the repository root config. Nearest-config-wins and nested config discovery are not part of the initial design.

Initial linter config does not support file-specific `overrides`. File selection belongs to include/exclude and ignore patterns. Resource/catalog linting can revisit per-resource configuration later if a concrete need appears.

`crates/ox_mf2_lint` owns:

- rule registry
- default rule severity
- preset expansion
- config defaults
- resolved config validation

The CLI loads JSON config files and passes the resolved data through the Rust config model. N-API and WASM entry points should accept equivalent JSON or structured options and normalize them through the same Rust validation path.

The JSON configuration surface should have a generated JSON Schema published with npm packages for editor completion and validation.

Open decisions:

- exact config file name
- JSON schema shape
- package-level JSON schema publication
- include/exclude pattern semantics
- root detection rule for locating the single project config

## File Discovery

The CLI owns file discovery for path and glob inputs. It should use an explicit supported-extension list owned by the linter/CLI crates.

Unsupported files and unmatched patterns are CLI input conditions rather than parser diagnostics.

Open decisions:

- supported extension names
- ignore file support
- include/exclude glob syntax
- unmatched pattern behavior
- directory traversal behavior
- duplicate path handling and deterministic ordering
- symlink traversal
- hidden files, VCS directories, and dependency directories such as `node_modules`

## CLI Detailed Behavior

The Phase 3 tooling and transport design defines the CLI-level contract:

- dedicated command-line lint workflow
- file path and glob inputs
- project configuration loading
- human-readable `text` output
- machine-readable `json` output
- failure when any `error` diagnostic is reported
- warning threshold support through `--max-warnings <n>`

This document owns the detailed CLI option semantics.

Open decisions:

- exact command name and package/bin ownership
- rule listing/introspection command
- resolved config printing command
- config file name and JSON schema details
- include/exclude and ignore behavior
- quiet mode behavior
- whether fix mode exists in the first CLI version
- output format variants beyond `text` and `json`
- JSON schema compatibility with ESLint-style output
- path normalization and source id representation in output
- deterministic output ordering with parallel file linting
- default concurrency and optional concurrency controls

## Core Semantic Diagnostics vs Configurable Rules

This detailed linter design owns the classification between core semantic diagnostics and configurable lint rules. The Phase 3 tooling and transport design only records the consumer-facing pipeline and initial candidate scope.

Each candidate should be classified into one of these groups:

- core semantic diagnostic: always enabled after successful parsing, independent from rule configuration, and initially reported as `error` when the semantic condition is present
- configurable lint rule: runs only after parser and semantic diagnostics are clean, and is controlled by `off`, `warn`, or `error`
- deferred: requires more MF2 selection semantics, resource/catalog context, or editor-specific behavior before implementation

The initial semantic diagnostic candidates below are intentionally not fully classified yet. Their exact category should be decided in this document before implementation.

## Initial Semantic Diagnostics

The initial semantic diagnostic candidates are listed below. Their exact messages, spans, labels, and edge cases are intentionally still open and should be decided before implementation.

### undefined-variable

Reports a variable reference that cannot be resolved to a visible `.input` or `.local` declaration when the selected semantic mode requires explicit declarations.

Example:

```mf2
.input {$count :number}
{{You have {$total} items.}}
```

Open decisions:

- Whether simple messages allow implicit variables by default.
- Whether this diagnostic requires strict mode.
- Whether `.local` right-hand sides share visibility rules with message body references.

### duplicate-declaration

Reports repeated declarations of the same variable in the same message scope.

Example:

```mf2
.input {$count :number}
.input {$count :number}
{{{$count}}}
```

Example:

```mf2
.local $label = {$count}
.local $label = {{items}}
{{{$label}}}
```

Open decisions:

- Whether `.input` and `.local` share one namespace.
- Whether duplicate declarations are always semantic errors or can be relaxed in compatibility modes.

### invalid-local-dependency

Reports invalid `.local` dependency graphs, including self-references and cycles.

Example:

```mf2
.local $label = {$label}
{{{$label}}}
```

Example:

```mf2
.local $a = {$b}
.local $b = {$a}
{{{$a}}}
```

Open decisions:

- Whether forward references are valid when the dependency graph is acyclic.
- How to choose the primary span for a dependency cycle.

### variant-key-arity-mismatch

Reports a matcher variant whose key count does not match the selector count.

Example:

```mf2
.match $gender $count
male {{He has items.}}
* {{Fallback}}
```

Example:

```mf2
.match $count
one few {{Items}}
* {{Fallback}}
```

Open decisions:

- Whether the parser should already reject all malformed variant boundaries.
- How to label the selector list and the offending variant key list.

### missing-fallback-variant

Reports a matcher without a catch-all fallback variant.

Example:

```mf2
.match $count
0 {{No items}}
1 {{One item}}
```

Example:

```mf2
.match $gender $count
male 1 {{He has one item}}
female 1 {{She has one item}}
```

Open decisions:

- Exact fallback requirement for multiple selectors.
- Whether this is always a semantic error or a configurable lint rule.
- How the rule interacts with function-specific selector domains.

### duplicate-variant

Reports duplicate variant key tuples.

Example:

```mf2
.match $count
1 {{One item}}
1 {{Single item}}
* {{Items}}
```

Example:

```mf2
.match $gender $count
male 1 {{He has one item}}
male 1 {{He has a single item}}
* * {{Fallback}}
```

Open decisions:

- Whether key normalization is needed before comparison.
- Whether literal and numeric keys can collide after normalization.

### unreachable-variant

Reports variants that cannot be selected.

This candidate needs more specification work before implementation. It should only report cases that can be proven unreachable from MF2 selection semantics and known selector domains.

Open decisions:

- Whether this belongs in semantic diagnostics or configurable lint rules.
- Which selector domains are known enough for sound reachability checks.
- Whether catch-all ordering can make later variants unreachable.

### semantic-lowering-failed

Reports an internal semantic lowering failure when parsing completed but a valid semantic model cannot be produced.

Open decisions:

- Whether this diagnostic should be exposed as a user-facing semantic diagnostic or treated as an internal error.
- How much context to expose without leaking implementation details.

## Configurable Rule Categories

Initial configurable lint rules should avoid style concerns. Style checks and formatting fixes should be delegated to the formatter API/crate.

Candidate categories:

- `correctness`: correctness checks that can be configured after core diagnostics and run in the configurable rules phase
- `best-practice`: maintainability or translation workflow checks
- `resource`: future resource/catalog-level checks

## Resource and Catalog Linting

Message-level linting is the core. Resource/catalog linting should be layered on top and reuse `lintMessage(source)` for each message entry.

Future resource/catalog examples:

- missing translation keys across locales
- placeholder mismatch across locales
- variant coverage mismatch across locales
- duplicate message ids
- unused messages

## Formatter Interaction

Lint rules should not implement style fixes directly. Future `lint --fix` style behavior should call formatter APIs so style decisions remain consistent across formatter, linter, and editor integrations.

## Suppression Directives

Directive syntax is not fixed yet. The linter result model should be able to represent suppressed diagnostics later, but the first implementation does not need a concrete MF2 suppression comment syntax.

## Open Questions

The following items are detailed linter design questions, not Phase 3 boundary decisions:

- exact config file name and JSON Schema publication path
- exact root detection rule for loading the single project config
- include/exclude glob syntax and precedence
- ignore file support and interaction with config ignore patterns
- supported file extensions for direct message files
- file discovery behavior for paths, globs, directories, duplicate matches, and deterministic ordering
- unmatched pattern behavior
- symlink traversal, hidden files, VCS directories, and dependency directories such as `node_modules`
- CLI exclude flags and their interaction with config
- exact JSON output source path normalization
- default concurrency and deterministic output ordering with parallel linting
- rule registry and introspection surface, including whether the CLI exposes rule listing, resolved metadata, or documentation links
- rule metadata generation workflow for docs, JSON Schema, rule option schemas, and implementation-time rule tables
- suppression model beyond the first implementation, including inline disable directives, unused directive diagnostics, and baseline suppression files
- fix model boundaries, including safe fixes, suggestions, dangerous fixes, and how non-style fixes interact with formatter-owned style changes
- machine reporter roadmap beyond initial `text` and `json`, including whether CI-oriented formats such as GitHub annotations or SARIF need stable compatibility targets
- CLI inspection and debug modes, such as resolved config printing, file discovery debugging, rule timing output, and their exit code behavior
- exact API result schema parity across Rust, N-API, WASM, CLI JSON, and future editor integrations
- rule test harness shape, fixture conventions, snapshot expectations, and validation for rules that claim fix capability
- benchmark scope for linting, including parse/semantic/rule execution split, per-rule timing, file discovery overhead, and output serialization overhead
