# ox-mf2 Phase 3 Tooling and Transport Design

## Purpose

This document defines the Phase 3 design boundary for tooling and transport workflows around ox-mf2.

Phase 1 parser design is defined in [002-ox-mf2-phase-1-rust-parser-design.md](./002-ox-mf2-phase-1-rust-parser-design.md). Phase 2 Binary AST snapshot design is defined in [003-ox-mf2-phase-2-binary-ast-snapshot-design.md](./003-ox-mf2-phase-2-binary-ast-snapshot-design.md). Phase 2 language binding design is defined in [004-ox-mf2-phase-2-language-bindings-design.md](./004-ox-mf2-phase-2-language-bindings-design.md).

This document focuses on formatter/linter input, resource and message-linker layering, future SemanticView exposure, LSP/editor workflows, agent coding workflows, transport choices, and long-lived language-service scenarios.

## Basic Policy

The standard public cross-language CST/AST syntax boundary remains the versioned Binary AST snapshot. Tooling may use Rust-internal construction-time tables for fast paths, but a public cross-language input that exposes MF2 syntax-tree structure should converge on the Binary AST decoder/accessor view.

Semantic information can be exposed later as SemanticView or a later compact semantic snapshot. It is not forced into the lossless Binary AST snapshot.

This syntax-boundary policy does not absorb versioned product-domain contracts. The 014 `MessageReferenceArtifact`, `MessageDefinitionArtifact`, selectors, scope/completeness values, linker findings, and bundle plans remain language-neutral message-linker artifacts under `intlify_contract` and `intlify_linker`; they are neither CST/AST inputs nor Binary AST extensions. Binary AST, a future SemanticView, and message-linker artifacts remain separate boundaries with separate ownership and compatibility rules.

MessagePack is not the CST/AST representation of ox-mf2. It is reserved as a future transport for long-lived language-service workflows.

## Implementation Phasing

This document defines the broader tooling and consumer boundary for Phase 3 and later work. It does not require formatter, linter, LSP/editor, agent integration, and long-lived transport features to be implemented in one release or one milestone.

Implementation should be split by consumer-facing product surface:

1. **Phase 3A: Tooling Foundation**
   - `crates/intlify_cli` and the `intlify` command structure
   - unified `fmt` / `lint` project config model and JSON Schema
   - shared machine-readable output conventions
   - package and distribution boundaries for CLI, N-API, and WASM tooling

2. **Phase 3B: Formatter Product**
   - workspace-internal `crates/intlify_format`
   - `intlify fmt`
   - deterministic sequential file processing in `crates/intlify_cli`, reused by later file-oriented commands
   - `fmt --check` and formatter check result contract
   - standard and preserve formatting modes
   - `@intlify/format-napi` and `@intlify/format-wasm`

3. **Phase 3C: Linter Product**
   - `crates/intlify_lint`
   - `intlify lint`
   - reuse of shared CLI discovery, physical-file grouping, and deterministic sequential result aggregation
   - parser, semantic, and lint diagnostic result contract
   - recommended preset, core semantic diagnostics, and initial configurable lint rules
   - `@intlify/lint-napi` and `@intlify/lint-wasm`

**Unnumbered layered milestone completed before Phase 3D: Resource Catalog Support**

- workspace-internal `crates/intlify_resource` and the consumer-neutral extraction/write-back contract
- Tier 1 JSON catalog adapter
- additive `resources` project config, unified schema update, catalog membership, and format resolution
- catalog-aware `intlify fmt` and `intlify lint` composition over the completed message-level products
- resource validation, deterministic reporter, concurrent-use safety, and release-gate coverage required before editor consumption

This milestone deliberately has no Phase 3 number because it is a layered capability rather than another standalone product surface. Its consumer-neutral `intlify_resource` crate, Tier 1 adapter, and resource configuration foundation may start and land without waiting for the Phase 3C linter product; they do not depend on `intlify_lint`. Catalog-aware formatting depends on the completed Phase 3B message-level formatter, while catalog-aware linting separately depends on the Phase 3C message-level linter. The complete milestone must finish before Phase 3D starts its opted-in catalog path, so editor integration consumes one implemented resource layer instead of creating a provisional duplicate. The complete Tier 1 milestone is part of the initial tooling v0.1 feature scope. In Phase 3 documents, that label describes the tooling product feature set; it is independent of the Binary AST snapshot header version and the existing `@intlify/cli` npm package version.

**Parallel track: Message Linker and Distribution**

The [014 message-linker design](./014-ox-mf2-message-linker-design.md) defines a layered track rather than another Phase 3C or Phase 3D subphase:

- M0 establishes `intlify_contract`, the language-neutral `intlify_linker`, a JavaScript/TypeScript source-scan producer, completeness-gated findings, and basic bundle plans.
- M1 adds the coverage-baseline-driven language-neutral key-only typed-key model without parsing MF2 payloads, adding a CLI leaf, or writing platform source, and M2 adds locale/fallback-aware linking.
- M3 adds workspace-internal `crates/intlify_export`, which owns the common parser-backed export-preparation and exporter boundary plus the initial ESM exporter. Export preparation validates the identity-deduplicated union of plan-selected delivery definitions and M1 baseline definitions required for typed signatures, then derives the validated language-neutral signature information. The ESM exporter combines that M3 information with admitted M1 key models and renders scope-bound JS/TS accessor modules in the same transaction as its resource assets. `crates/intlify_cli` owns the `intlify messages emit` orchestration, exporter factory/registry wiring, output registration, and `--check`.
- M4 adds live bundler-graph integration over the M3 exporter transaction.
- M5 adds `intlify messages prune` through a separate 013 structural-deletion contract; it does not broaden formatter value write-back.
- L0 and L1 are later lint-presentation adapters. They wait for the Phase 3C result/rule contracts and the catalog-level lint addendum, but do not gate M0 through M5.
- E0 is the first JS/TS catalog-key completion milestone owned by 009. It depends on the M1 typed-key model and producer reverse projection, but not on L0/L1, M2, or an exporter milestone.
- N0 and N1 are a parallel native-producer track after M0 for Rust and then C/C++/WASM reference production.

M0 may begin when 013 Tier 1 extraction and the coordinated explicit scope, `path` / `fixed` locale binding, and producer-to-scope/domain binding prerequisites are available. It also integrates the M0 `messages` fields into the common Phase 3A configuration envelope, generated schema, and validation pipeline and implements the reusable `crates/intlify_cli` project-inventory and linker orchestration, but it exposes no executable CLI leaf. M0 has no dependency on `intlify_lint` or Phase 3D. M3 additionally admits `messages.delivery`, exposes `intlify messages emit` and its result contract, and depends on the shared parser and SemanticModel export-validation gate. M4 builds on the M3 transaction. M5 waits for the dedicated 013 structural-mutation addendum and format capability rules.

Vue SFC application-reference production is not part of M0. It is the next producer phase tracked by 014's Deferred Follow-Up Notes and is independent of 013's Vue SFC resource-catalog adapter.

Phase 3D may consume M0 linker findings through the additive project-backed editor session defined by 009, and E0 may later consume M1 typed-key models through that same session. Neither the initial editor product nor E0 gates the linker main track. The exact linker, exporter, and artifact contracts remain owned by 014, while E0 activation, cursor behavior, publication, and source edits remain owned by 009.

4. **Phase 3D: LSP/Editor Integration**
   - adapter workflows for diagnostics and formatting
   - `.mf2` and opted-in catalog resource message mapping
   - UTF-8 byte span to editor position conversion
   - editor-specific configuration source handling

5. **Phase 3E: Agent Coding Integration (pending design follow-up)**
   - agent workflows over stable CLI JSON output
   - repo instructions, skills, plugins, hooks, or MCP wrappers as needed
   - no vendor-specific agent protocol as the core contract
   - no implementation or product-shape selection until the deferred design work in 010 resumes

6. **Phase 3F or Later: Long-lived Transport**
   - JSON-RPC baseline measurement
   - MessagePack transport evaluation
   - daemon/session/cache optimization for repeated language-service queries

Earlier phases should keep later consumers in mind when shaping public contracts, but later consumer workflows remain layered integrations until their product phase starts. Consumer-neutral resource and linker work may land in separate PRs while Phase 3C is still in progress; this does not retroactively add resource or linker implementation work to the Phase 3B or Phase 3C product PR sequences. Lint and editor adapters join the linker only at their declared integration milestones.

## CLI File Execution Boundary

Initial `intlify fmt` and `intlify lint` file mode processes physical file groups sequentially in deterministic order on the caller thread. Parser, formatter, linter, and resource crates remain synchronous and safe for concurrent consumer calls so a later optimization does not require moving scheduling into those crates or changing their APIs.

`crates/intlify_cli` owns batch traversal for the native `intlify fmt` and `intlify lint` file workflows, initially as sequential coordination and later as the only owner of any common worker scheduler. Parser, formatter, linter, and resource crates expose synchronous operations that are safe to invoke concurrently with independent per-call state; they do not create worker threads, select a concurrency width, own an async runtime, or aggregate CLI output. The parser's `parse_batch` is a sequential convenience operation rather than an exception to this rule; consumers that need parallel parsing schedule independent synchronous calls at their own boundary. In particular, `crates/intlify_resource` guarantees concurrent use of its registry and immutable extraction artifacts while leaving all decisions about whether and where to run calls concurrently to its consumer.

The same CLI owner uses an extension-neutral, crate-private discovery model before physical grouping. Its conceptual types are:

```rust
struct DiscoveredCandidate {
    logical_path: PathBuf,
    normalized_absolute_path: String,
    origins: Vec<CandidateOrigin>,
}

enum CandidateOrigin {
    DirectFile { operand_index: usize },
    Directory { operand_index: usize },
    CliGlob { operand_index: usize },
}

struct SelectedTarget {
    candidate: DiscoveredCandidate,
    classification: WorkflowClassification,
}
```

These names and concrete containers are implementation guidance rather than a public Rust or JSON API, but the retained information is required. Filesystem enumeration produces concrete paths and origins without filtering by supported extension or the ordinary ignore stack. De-duplication uses the slash-normalized absolute path and merges every origin in stable operand order; the presence of any `DirectFile` origin gives that one candidate direct-input classification semantics. Supported-input classification then gives reserved standalone MF2 precedence, applies resource policy and the narrow config-free direct rule, and produces either `standalone:mf2` or a `catalog:<registry-id>` selected target. Catalog-assignment conflicts are resolved as the classification-wide gate before ordinary ignore matching. The command applies its ordinary ignore stack only to successfully classified targets, then inspects physical identity and groups aliases for execution.

Enumeration errors retain their operand/discovery order separately from path-sorted supported-input classification errors. The coordinator combines those buffers only according to the command's published error precedence; streaming enumeration or classification must not make implementation encounter order observable. All discovery structs remain inside `intlify_cli` and are not exported by parser, formatter, linter, or resource crates.

Concurrent-use safety is an explicit core acceptance boundary, not an incidental consequence of the first implementation. Owned work descriptions and structured results transferred between the CLI coordinator and workers must be `Send`; any resolved configuration, registry, or other immutable core state intentionally shared by multiple workers must be `Send + Sync`. Per-call parser, semantic, rule, layout, and rendering scratch types that never cross or share across the worker boundary do not acquire a blanket `Send + Sync` requirement. Formatter, linter, and resource crates expose no process-wide mutable execution state, and thread-local caches or dependency internals must not change results, ordering, diagnostics, errors, or later-call behavior.

Each owning crate provides compile-time trait assertions for the concrete types that may cross or be shared across this boundary and concurrent-invocation tests that compare complete results with a serial baseline. Fault-isolation tests prove that a returned operational failure in one call cannot affect independent calls. The deferred scheduler adds escaped-panic containment coverage before parallel CLI execution is enabled. These requirements govern Rust core and CLI composition; they do not promise that a JavaScript N-API isolate or a single-threaded WASM instance may itself be called from arbitrary operating-system threads.

After argument validation, configuration, discovery, supported-input classification, and ignore filtering have completed, the CLI inspects physical identities and groups the remaining logical targets. Ignore sources are loaded and validated during setup, but matching them against concrete candidates occurs only after supported-input classification. One execution unit is one physical file group, not one logical path. Initial execution visits groups sequentially, while normalized logical aliases within one group execute their complete read and core-processing pipelines serially. This gives formatter and linter commands one execution model and prepares the same grouping boundary for future parallel execution without allowing aliases to race in formatter write mode.

Physical identity follows the selected target file and groups both symbolic-link and hard-link aliases. POSIX implementations use device ID plus inode; Windows implementations use volume ID plus file ID. Another supported native platform must provide an equivalent stable file identifier before enabling file-mode formatter or linter processing. Canonical or normalized path text is never a fallback identity because it cannot detect hard links reliably. Identity is captured once after ignore filtering and before target processing. It prevents this CLI process from scheduling aliases concurrently but does not add external mutation detection; rename, symlink retargeting, or file replacement after the snapshot follows the formatter's direct-write and no-stale-check contract.

Failure to inspect one selected target's physical identity is a target-local `input_read_failed` with `details.reason: "metadata_failed"`. `details.ioKind` is required and normalizes to exactly `"not_found"`, `"permission_denied"`, `"not_file"`, `"not_directory"`, or `"unknown"`. `details.rawOsError` is included only when the underlying I/O error exposes a raw operating-system code and is serialized unchanged as a signed JSON integer. A broken symbolic link therefore reports its concrete metadata failure rather than receiving a guessed identity. That logical target is not grouped or processed; other successfully identified targets continue. Dependency debug names, operating-system prose, and newly added Rust `ErrorKind` names never become stable detail values implicitly.

Every runnable physical group has one workflow classification. Products that add classifications own their stable classification tokens and the error contract for a group whose logical aliases disagree; the resource catalog workflow defines its `standalone:mf2` and `catalog:<registry-id>` conflict in [013-ox-mf2-resource-catalog-adapter-design.md](./013-ox-mf2-resource-catalog-adapter-design.md#input-selection). A non-conflicting group's logical aliases run serially in normalized logical-path order. Each alias opens and reads the source only after every earlier alias has completed, so a successful formatter write is visible to the next alias through a fresh read.

A target-local failure before a filesystem write attempt does not stop later aliases in its physical group. This includes read, parse, extraction, formatter, linter, candidate-validation, diagnostic-mapping, and result-construction failures. An `output_write_failed` result in formatter write mode is the sole fail-stop case because direct writes provide no rollback guarantee and may leave the physical file indeterminate. The CLI performs no further stat, open, read, parse, format, lint, or write for later aliases in that group; unrelated groups continue. The exact current-target result, synthesized `alias_processing_blocked` results, details, and summary accounting are owned by [007-ox-mf2-phase-3b-formatter-design.md](./007-ox-mf2-phase-3b-formatter-design.md#physical-alias-write-failure). Check modes and lint never cross this write-failure boundary and therefore always continue after a target-local failure.

## CLI Project Inventory Boundary

`intlify messages emit` and `intlify messages prune` use the project-inventory workflow defined by 014 rather than the operand-driven file mode above. They accept no positional file, directory, or glob operands and do not translate an empty operand list into directory `.`. The 006 project root plus the validated `resources` and `messages` sections are their only initial inventory authority.

`crates/intlify_cli` owns filesystem enumeration, physical-file grouping, execution accounting, and construction of the execution-derived completeness evidence. `intlify_resource`, language producers, `intlify_contract`, and `intlify_linker` remain synchronous filesystem-neutral cores. The CLI may reuse the shared path-representability, deterministic native discovery ordering, file-symlink, directory-symlink, metadata, and physical-identity primitives, but it does not reuse direct-operand classification or formatter/linter ignore semantics.

For linker definition inputs, the CLI resolves assignments for every logical target, inspects physical identity, forms one physical-source group, and fixes its canonical primary-plus-alias order before invoking host-owned definition production. That production boundary has two stages: pre-extraction admission validates the already formed group's equal bindings, host-format profile, and portable path/alias limits; after admission, the CLI invokes `intlify_resource` exactly once and post-extraction projection performs checked resource-to-contract conversion into one `MessageDefinitionArtifact`. Neither `intlify_resource` nor `intlify_contract` enumerates paths, groups aliases, or depends on the other crate.

The project-inventory workflow selects definition paths only through linker-participating `resources.catalogs` definitions, reference source paths through resolved producer include patterns, and external reference artifacts through their exact resolved declarations. These explicit sets are authoritative. Root `.gitignore`, `--ignore-path`, `fmt.ignorePatterns`, `lint.ignorePatterns`, the ordinary hidden-file exclusion, and the default dependency/output-directory exclusion cannot silently remove an input from a closed-world link. Configuration-owned include/exclude policy must express any omission.

A zero-match valid pattern is a complete empty selection, not a fallback request or implicit scan. Conversely, an enumeration, metadata, read, extraction, projection, producer, or external-artifact failure remains attached to its configured inventory participant and prevents the affected completeness side from becoming `Closed`. `--target` changes only delivery/export selection and never narrows either evidence inventory. The exact inventory, failure-to-completeness mapping, and no-generation/no-mutation gates remain owned by 014.

## Deferred Follow-Up Notes

The following tooling work is intentionally deferred and is not required by the initial formatter, linter, or resource implementation.

### CLI Parallel Execution Boundary

The common CLI worker scheduler and parallel file execution described in this subsection are a future optimization. They extend the sequential physical-group boundary above without changing observable target ordering, alias behavior, or crate ownership.

The first scheduler implementation creates at most one bounded, command-scoped worker-thread pool after the runnable groups are known. No pool is created when there are no runnable groups. Otherwise, its default width is `max(1, min(runnable_physical_groups, available_parallelism, MAX_CLI_WORKERS))`, with `4` as the provisional implementation value of the internal product constant `MAX_CLI_WORKERS`. This hard ceiling bounds simultaneous source, parser, extraction-artifact, formatted-message, replacement, and candidate-artifact retention when several large catalogs are selected. Parser, formatter, linter, and resource work uses that same pool rather than creating nested or crate-specific pools. Stdin mode is one caller-thread workflow and does not construct a file worker pool. An async runtime is not required solely for these CPU-heavy synchronous pipelines.

`MAX_CLI_WORKERS` is not a public CLI option, config field, environment variable, or machine-readable output field. Before the first release that enables this deferred scheduler, the benchmark gate compares worker width one with the provisional width four over near-limit message-dense, structure-dense, lint, and changed-format/write-back workloads and records peak live allocation or peak RSS under one documented measurement setup. Width four must be explicitly accepted from that measured memory envelope; if the result is not accepted or no reliable measurement is available, the production constant is reduced to two or one and the gate is rerun. This is a release decision over an internal bound, not a new runtime resource error, dynamic memory scheduler, or compatibility surface. Afterward the constant may still be tuned in a later release from representative scheduling, memory, and end-to-end benchmarks without a compatibility change, provided the pool remains bounded and every observable ordering, error, result, and exit contract stays identical. Tests inject widths independently of this production default and include runnable group counts below, at, and above the active ceiling. The concrete pool dependency and scheduling algorithm are implementation details within those constraints.

Every worker-runtime infrastructure failure is command-fatal. Failure to obtain the operating system's available parallelism, construct the pool, spawn a worker, dispatch work, contain a worker panic, or join and tear down the pool does not fall back to caller-thread or partially available serial execution. The command emits exactly one top-level `internal_error` with `details.reason: "cli_worker_runtime_failed"` and `details.phase` equal to `"initialize"`, `"dispatch"`, `"execute"`, or `"join"`. The normal top-level `path` is included only when the scheduler can identify the active normalized logical target exactly; dependency error names, panic payloads, backtraces, and Rust debug text are not exposed.

After a post-initialization failure is detected, the coordinator stops dispatching new physical groups, cancels queued work where the chosen runtime supports cancellation, and waits for already running work only as required for safe teardown. Normal target results from that failed run are discarded: JSON uses `summary.status: "error"`, an empty `results` array, the one top-level error, and omits command-specific target and diagnostic counters; the process exits with `2`. Formatter writes that completed before detection or during unavoidable in-flight teardown are not rolled back, and no atomic whole-command write guarantee is implied. If teardown observes multiple runtime failures, the one public error is selected by phase order `initialize`, `dispatch`, `execute`, then `join`; within one phase a pathless failure sorts first, followed by normalized logical path order. These rules keep the failure envelope deterministic without treating a compromised runtime as a target-local recoverable error.

Workers never write reporter output directly. They return structured target results tagged with their stable logical identities; the coordinating thread orders results by the command's normalized-path and within-file rules, computes summaries, and renders text or JSON only after ordered aggregation. Completion order therefore cannot affect stdout, stderr, JSON arrays, selected errors, or exit status. Formatter writes to different physical groups may complete in any order, but a write failure affects only the remaining aliases in its own group. The scheduler uses backpressure and does not eagerly retain every file's source or parse artifacts merely because every path has already been discovered.

The first scheduler implementation parallelizes only across physical file groups. It does not split one standalone message or catalog into nested worker tasks. Public concurrency controls such as `--threads` remain a later CLI surface; tests and benchmarks may inject a worker width through an internal scheduler construction boundary without exposing it as configuration.

A later explicit scheduler extension may reuse that same command-scoped bounded pool for independent delivery-target transactions. Target work starts only after the 014 project-global configuration, inventory, artifact-production, completeness, linking, and export-preparation prefix has completed successfully. Physical-group work and target-transaction work are separate scheduler stages; they do not overlap, and no target, exporter, or registration path creates a nested or private pool.

That extension may change completion order only. The coordinator still assembles results in canonical target-name order, preserves each target's independent commit, rollback, recovery, and error boundary, and applies the 014 command-summary and exit precedence after every selected transaction finishes. Initial M3 target execution remains sequential until this extension is separately implemented and validated with completion-order permutations, bounded live-work checks, and the existing target-transaction fixtures.

### Non-Rust Project-Backed Editor Bridge

Direct Node and browser access to catalog extraction, project inventory, definition and reference artifacts, linking, typed-key models, and project-backed findings is deferred until a concrete non-Rust editor or language-service consumer requires it. Existing parser, formatter, and linter N-API/WASM packages remain message-local and do not absorb those APIs.

A promoted integration must choose one explicit boundary: a versioned native-process protocol suitable for the target environment, or separately designed bounded read-only bindings for every required resource, contract, producer, linker, and orchestration value. It must define lifecycle, cancellation, resource limits, error mapping, snapshot identity, cache invalidation, and compatibility without serializing private Rust representations or duplicating 013/014 project semantics. Browser support cannot be claimed through a native-process design.

## SnapshotView

Phase 3 does not introduce a second public AST view. The existing Binary AST `SnapshotView` / binding-side snapshot accessor remains the common public serialized syntax foundation for formatter, linter, LSP/editor, and transport consumers; this does not require every initial product entry point to accept a snapshot directly.

`SnapshotView` is already defined by the Phase 2 Binary AST snapshot design as a lazy decoder/accessor over versioned snapshot bytes. Phase 3 extends that contract at the consumer-requirements level rather than replacing it with a recursive object AST.

The Phase 3C initial linter is the explicit source-backed exception: `lintMessage(source)` parses into construction-time `CstView` plus parser-owned `SemanticModel` facts and does not expose `lintSnapshot`. Snapshot-backed linting remains deferred until the parser owns a snapshot-to-`SemanticModel` path that preserves semantic validation behavior. Formatter snapshot APIs and future snapshot-backed linter/editor optimizations continue to use `SnapshotView` as the public serialized syntax boundary.

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

Binary AST handles CST, tokens, trivia, and source spans. SemanticView handles semantic facts and source links derived from `ox_mf2_parser`-owned `SemanticModel` data.

The v0.1 Binary AST does not serialize Phase 1 semantic `StringRef` values. A future public SemanticView that exposes cooked or normalized text must define explicit view-owned or future-snapshot string storage instead of treating the v0.1 metadata/diagnostic string table as semantic storage.

- declarations
- references
- selectors
- variants
- fallback/default information
- links to NodeId and Span
- future duplicate-key or coverage metadata if those facts become part of the shared semantic surface

Bindings, editors, future snapshot-backed linting, compiler-like consumers, and language-service features can combine Binary AST decoder/accessor traversal with SemanticView. Initial Phase 3C built-in lint rules use the Rust-internal `RuleContext` described in the linter input section instead of requiring SemanticView as a public rule API.

Semantic diagnostics are produced separately by the parser-owned `validate_semantics(model)` boundary. SemanticView is not the diagnostic owner; it is the semantic fact/source-link concept that consumers can combine with diagnostics and Binary AST traversal. In the initial Phase 3 scope, it is a tooling-facing semantic concept and future public semantic surface rather than a fixed N-API/WASM public API or built-in lint rule API. Bindings may expose it later, but they must not implement a separate semantic analyzer in JavaScript or another host language.

## Formatter Input

### Product Surface

Phase 3 should provide a dedicated formatter engine and reusable formatter entry points for workspace Rust, N-API, and WASM consumers. The CLI is the primary user-facing workflow, while N-API and WASM bindings allow the same formatter engine to power playgrounds, editor integrations, workers, and Node-based tools.

### Crate Ownership

The Rust formatter engine should live in a separate workspace-internal `crates/intlify_format` crate that depends on `ox_mf2_parser`. This crate is not a crates.io deliverable in Phase 3B; public formatter distribution happens through the `intlify fmt` CLI and formatter N-API/WASM packages. The parser crate remains responsible for CST construction, parser diagnostics, Binary AST snapshots, SemanticModel construction, and parser-owned semantic validation. The formatter crate owns formatting modes, formatter configuration, layout construction, rendering, and formatter result shaping.

### CLI and Package Distribution

The CLI binary should live in `crates/intlify_cli` and expose `intlify fmt` alongside `intlify lint`. Distribution should happen through npm packages that let JavaScript users install and run the Rust CLI through the npm ecosystem: `@intlify/cli` is the JavaScript wrapper package, while `@intlify/cli-native` owns the compiled native CLI binary artifacts.

N-API and WASM formatter bindings should be published as formatter-specific packages rather than added to the existing parser binding packages. Parser bindings stay focused on parsing, snapshots, and parser-level APIs, while `@intlify/format-napi` and `@intlify/format-wasm` expose formatter-specific APIs backed by `crates/intlify_format`.

The npm distribution surface should separate the CLI wrapper/native binary packages from formatter API packages. `@intlify/cli` exposes the command-line wrapper, `@intlify/cli-native` owns the compiled native binary artifacts, `@intlify/format-napi` exposes Node APIs, and `@intlify/format-wasm` supports browser, worker, and playground use cases.

The Phase 3 formatter deliverables are the Rust formatter engine, CLI command, N-API formatter package, WASM formatter package, JSON configuration contract, generated JSON Schema, and formatter result contract. LSP/editor integration, playground usage, and resource/catalog formatting are consumers or layered workflows rather than separate direct products in this phase.

### Public Syntax Input

From Phase 2 onward, public AST input for formatter APIs is the Binary AST decoder/accessor view.

Formatter implementation has a workspace-internal parsed-artifact path for callers that already own a paired `SourceStore` and `ParseResult`. `intlify_format` constructs a private, validated formatter input view over that pair and traverses the existing CST tables directly. This path must not call the parser again, encode a Binary AST snapshot, or decode a snapshot. It also must not depend on the higher-level `CachedParse` type: cache consumers pass references to the owner/result pair that the cache keeps together. Detectable owner, source-id, table-reference, span, UTF-8-boundary, diagnostic, and mode-capability inconsistencies fail before Layout IR construction. The original owner/result pairing remains a caller invariant because coincidentally equal numeric `SourceId` values do not prove store identity.

This parsed-artifact path is a Rust workspace integration boundary, not a crates.io, N-API, WASM, or serialized compatibility surface. Stable public formatter input remains the Binary AST view shared by Rust, N-API, WASM, and later consumers. Source-backed, parsed-artifact-backed, and snapshot-backed calls converge on one formatter syntax-view abstraction before Layout IR construction so formatting behavior does not vary by artifact transport.

The primary public API is `formatMessage(source, options?)`, which parses one MF2 message and returns a formatter result. `formatSnapshot(snapshot, source, options?)` is an advanced parse-artifact reuse path for playgrounds, workers, and language-service caches that already hold a Binary AST snapshot. The complete source string is required in every formatter mode for source slicing, parser diagnostic materialization, output comparison, and exact verification against the formatted root's SourceRecord byte length and SHA-256 digest; preserve mode additionally uses it for source-shape decisions. The Phase 3 formatter does not expose a source-free snapshot mode. Binding packages should expose direct programmatic formatter APIs rather than a CLI callback bridge.

### Formatting Modes

The formatter should support at least two modes. Detailed formatter rules, API shape, fixtures, and implementation requirements are tracked separately in [007-ox-mf2-phase-3b-formatter-design.md](./007-ox-mf2-phase-3b-formatter-design.md). This document only fixes the Phase 3 boundary and high-level formatter direction.

- standard mode: format to the standard ox-mf2 style without using the original layout as a primary decision input
- preserve mode: preserve source shape where it is meaningful while still applying standard local formatting rules

Standard mode is a deterministic pretty-printer over the public syntax view.

Preserve mode is source-shape-sensitive pretty formatting. It may preserve single-line / multi-line choices, blank-line grouping, and whitespace trivia placement when those choices are recoverable from tokens, trivia, delimiter spans, and source slices. It should still normalize local spacing, indentation, and other standard formatting rules. In Phase 3B, both standard and preserve modes preserve translatable pattern text, quoted and unquoted literal spelling, and escape spelling through verified source slices. MF2 defines no line-comment or block-comment syntax, so comment placement is not a formatter mode capability.

### Message-Level Formatting

The formatter core formats one whole MF2 message at a time. Range-only formatting is outside the initial formatter core. LSP/editor adapters should call whole-message formatting, compare the original and formatted message, and produce editor `TextEdit` values at the integration boundary. Minimal-diff edit computation belongs to the editor/resource adapter, not the formatter core.

### Layout Architecture

The formatter should separate syntax traversal from rendering. Internally, it should have a layout model capable of delayed line/group/indent decisions so line width, standard mode, preserve mode, and resource/catalog adapters can reuse one message-level formatter core. The concrete IR/document implementation is a formatter-specific design detail.

### CLI Input Model

The CLI accepts file path and glob inputs so users can format MF2 files directly, for example `intlify fmt "locales/**/*.mf2"` or `intlify fmt messages/en.mf2`. The primary CLI input unit is a single MF2 message file: one file contains one MF2 message that can be parsed and formatted directly. Selected resource files containing multiple messages are an initial layered CLI workflow that extracts message entries and reuses the message-level formatter. Project catalog policy owns bulk selection, while the resource design also permits a narrow config-free exception for individual file operands and stdin whose extension maps to a shipped adapter. Host-format selection, parsing, string escaping, outer-document edits, and rollout tiers are owned by [013-ox-mf2-resource-catalog-adapter-design.md](./013-ox-mf2-resource-catalog-adapter-design.md), not by the Phase 3 core formatter contract.

### CLI Write and Check Workflows

`intlify fmt` should default to write mode and modify files in place. It should also support check workflows such as `--check` and `--list-different` for CI and pre-commit usage. Stdin formatting should be supported through a file-aware option such as `--stdin-filepath`, allowing editors and scripts to pipe source while still giving the formatter enough context for extension checks and configuration.

### Formatter File Execution

The initial CLI formats physical file groups sequentially on the caller thread. File discovery normalizes and de-duplicates logical paths, and physical grouping keeps symbolic-link or hard-link aliases in one serial boundary. Logical aliases within one group re-read the file after each earlier alias completes.

Text output, `--check`, and `--list-different` results should be reported in stable normalized path order. Programmatic APIs such as `formatMessage(source, options?)` remain single-message operations. The common CLI worker scheduler and any decision to parallelize entries within one catalog are deferred; resource/catalog adapters remain scheduler-neutral.

Initial benchmarks report sequential CLI execution separately from parser, syntax traversal, layout construction, rendering, binding, and file I/O costs. The deferred scheduler adds concurrency-specific benchmark dimensions before it is enabled.

### Configuration

The formatter should load JSON project configuration. Formatter and linter configuration are separate responsibility areas, but they should be sections of one ox-mf2 tooling config so the CLI can resolve `fmt` and `lint` settings from the same root project configuration. The initial config discovery model is intentionally simple: only the root config defined by the Phase 3A CLI foundation is loaded. Nearest-config-wins and nested config discovery are out of scope until a concrete multi-workspace need appears.

The project configuration surface should use one unified JSON Schema with `fmt` and `lint` sections. Formatter and linter crates may keep separate resolved config models internally, but editor completion and config validation should point users at the unified ox-mf2 tooling config schema published through `@intlify/cli/schema/config.schema.json`. CLI output schemas are separate from this config schema and may be split by command.

Formatter configuration should support `ignorePatterns` but not file-specific `overrides` in the initial design. The initial formatter target is a narrow MF2 message file/resource workflow, so file-kind-specific overrides are unnecessary. If future resource/catalog or multi-file-kind workflows need per-file options, overrides can be reconsidered then.

### EditorConfig

The Phase 3B initial formatter does not read `.editorconfig` because `mode` is the only supported formatting option. Once formatter options with corresponding EditorConfig properties, such as line width, indent width, line ending, or final newline, are explicitly supported, the formatter should read `.editorconfig` as formatter-only fallback input for those options when they remain unset by higher-precedence sources. The linter should not read `.editorconfig`.

### Invalid Syntax

Formatter behavior for invalid syntax is strict in the initial design. If parsing produces any parser diagnostic, the formatter does not produce public formatted output. CLI write mode must not modify the file. API consumers should receive diagnostics or an error result without formatted output. LSP/editor adapters should treat incomplete or invalid editing buffers as no-op formatting requests. Recovery-aware formatting is future editor-specific scope.

### Formatter Results

Formatter results are distinct from linter diagnostic results, but parser diagnostic locations should share the core SourceId and UTF-8 byte Span location model. CLI, N-API, WASM, and LSP/editor integrations can derive line/column or UTF-16 positions as needed. Formatter-specific diagnostics may be added later under a formatter category, but linter rule ids and severity configuration should not be mixed into formatter results.

### Formatter Ignore Directives

Formatter ignore directives are not in scope for the initial formatter. MF2 does not define line or block comments, and `#` is a markup sigil, so a comment-like formatter directive would be a syntax extension. Initial formatter ignoring is file-level only, through formatter config, root `.gitignore`, and explicit ignore paths. A future syntax-unit suppression mechanism must be spec-compatible and belongs to the formatter-specific design.

### Formatter and Linter Boundary

The Phase 3 responsibility boundary is style in the formatter and correctness in the linter. Formatting checks should use `intlify fmt --check` or formatter check APIs. If a future linter workflow needs style diagnostics or style fixes, it should delegate to formatter APIs rather than reimplement formatter layout rules in lint rules.

### Dedicated Formatter Design Notes

Formatter detail notes to resolve in the dedicated formatter design:

- exact Rust, N-API, and WASM result types
- formatter options and defaults
- formatter config section schema shape
- future spec-compatible formatter suppression mechanism
- line wrapping rules
- matcher layout rules
- formatter fixture and idempotency requirements
- native package lazy-loading and config helper behavior
- required SnapshotView helper priority

### Formatter Benchmarks

Formatter output should measure parser, parsed-artifact attachment/direct CST access, snapshot encode/decode/access, shared syntax traversal, layout construction, rendering, binding calls, and CLI end-to-end cost separately. Parsed-artifact benchmarks must keep snapshot encoder/decoder counters at zero so an accidental serialization detour is visible.

## Linter Input

### Product Surface

Phase 3 should provide a dedicated lint CLI and reusable linter entry points for Rust, N-API, and WASM consumers. The CLI is the primary user-facing workflow, while N-API and WASM bindings allow the same linter engine to power playgrounds, editor integrations, and Node-based tools.

### Crate Ownership

The Rust linter engine should live in a separate workspace-internal `crates/intlify_lint` crate that depends on `ox_mf2_parser`. This crate is not a crates.io deliverable in Phase 3C; public linter distribution happens through the `intlify lint` CLI and linter N-API/WASM packages. The parser crate remains responsible for CST, diagnostics, snapshots, SemanticModel construction, and parser-owned semantic validation, while the lint crate owns rule execution, presets, lint configuration, and lint result shaping.

### CLI and Package Distribution

The CLI binary should live in a separate `crates/intlify_cli` crate. That crate composes parser, formatter, and linter crates into user-facing commands such as `intlify lint`. Distribution should happen through npm packages that let JavaScript users install and run the Rust CLI through the npm ecosystem: `@intlify/cli` is the JavaScript wrapper package, while `@intlify/cli-native` owns the compiled native CLI binary artifacts.

N-API and WASM linter bindings should be published as linter-specific packages rather than added to the existing parser binding packages. Parser bindings stay focused on parsing, snapshots, and parser-level APIs, while `@intlify/lint-napi` and `@intlify/lint-wasm` expose lint-specific APIs backed by `crates/intlify_lint`.

The npm distribution surface should separate the CLI wrapper/native binary packages from linter API packages. `@intlify/cli` exposes the command-line wrapper, `@intlify/cli-native` owns the compiled native binary artifacts, `@intlify/lint-napi` exposes Node APIs, and `@intlify/lint-wasm` supports browser, worker, and playground use cases.

The Phase 3 linter deliverables are the Rust linter engine, CLI, N-API linter package, WASM linter package, and shared diagnostic result schema. LSP/editor integration and playground usage are consumers of those deliverables rather than separate direct products in this phase.

### CLI Input Model

The CLI accepts file path and glob inputs so users can lint MF2 files directly, for example `intlify lint "locales/**/*.mf2"` or `intlify lint messages/en.mf2 messages/ja.mf2`.

The primary CLI input unit is a single MF2 message file: one file contains one MF2 message that can be parsed and linted directly. Resource files containing multiple messages, framework-specific i18n files, and multi-locale catalogs are layered consumers that extract message entries and reuse the message-level linter; their concrete file formats are not fixed by the Phase 3 core linter contract.

### Configuration

The CLI should load JSON project configuration for rule severity, presets, and ignore patterns. Formatter and linter configuration are separate responsibility areas, but they should be sections of one ox-mf2 tooling config so the CLI can resolve `fmt` and `lint` settings from the same root project configuration. The initial config discovery model is intentionally simple: only the root config defined by the Phase 3A CLI foundation is loaded. Nearest-config-wins and nested config discovery are out of scope until a concrete multi-workspace need appears. `crates/intlify_lint` is the source of truth for the resolved lint config model, rule registry, preset expansion, defaults, and validation so the CLI, N-API, and WASM entry points share the same behavior.

The JSON configuration surface should be part of the unified config JSON Schema published through `@intlify/cli/schema/config.schema.json`. This schema is part of the tooling contract for editor completion and config validation, while the Rust config model remains the source of truth. Linter configuration should not support file-specific `overrides` in the initial design; file selection belongs to CLI operands and ignore patterns. Resource/catalog linting can revisit this if per-resource configuration becomes necessary.

### Presets

The initial linter preset should be a `recommended`-style preset focused on broadly useful, low-noise message-level diagnostics. Rule category alone does not determine preset membership: the initial recommended rules are best-practice warnings, while a context-dependent correctness rule may remain opt-in. Parser and semantic correctness diagnostics are independent of configurable presets and remain enabled by their pipeline contract. Stricter, nursery, experimental, and resource/catalog-oriented presets are future or linter-specific design details rather than Phase 3 core requirements.

While the linter remains in 0.x, the `recommended` preset may evolve by adding broadly useful, low-noise configurable diagnostics from suitable categories. Preset stability policy should be finalized before a 1.0 release.

### CLI Output and Exit Behavior

The initial CLI output formats should include human-readable `text` output and machine-readable `json` output. Additional formats can be added later, but `json` should use the same diagnostic schema exposed by Rust, N-API, and WASM entry points.

Machine-readable output schemas are distinct from the unified project config schema. `lint`, `fmt --check`, and a future combined `check` command may use command-specific JSON result schemas, while sharing common grouping and summary conventions where practical.

The CLI exits with a failure status when any diagnostic whose JSON `severity` is `"error"` is reported. Diagnostics whose JSON `severity` is `"warn"` do not fail the process by default. A `--max-warnings <n>` option should allow CI users to fail when the warning count exceeds the configured threshold.

Detailed CLI option semantics, including quiet mode, fix mode, linter config section schema details, ignore/include behavior, unmatched-pattern behavior, and additional output formats, belong to the linter-specific design document rather than this Phase 3 consumer contract.

### Public Syntax and Semantic Input

The initial public linter API is source-backed: `lintMessage(source, options?)` parses, performs semantic validation, and runs enabled rules over one MF2 message.

The initial linter rule API is Rust-internal. Built-in rules receive a `RuleContext` that can expose CST access, parser-owned `SemanticModel` facts, source links, and resolved lint configuration without making public bindings depend on a fixed `SemanticView` API. The Binary AST decoder/accessor view remains the shared syntax foundation. Future SemanticView exposure remains the semantic foundation for bindings, editors, and future snapshot-backed linting, but Phase 3C rules do not require SemanticView to be a public N-API/WASM contract.

For N-API and WASM consumers, the primary public entry point is `lintMessage(source, options?)`. A snapshot-based entry point such as `lintSnapshot(snapshot, source?, options?)` is a future advanced parse-artifact reuse path; both its detailed API design and implementation are deferred until the parser owns a snapshot-to-`SemanticModel` path, as recorded in the detailed linter design. The source text or SourceStore-equivalent context is still needed whenever consumers require line/column, UTF-16 positions, or source-slice-aware diagnostics. Binding packages should expose direct programmatic lint APIs rather than a CLI callback bridge or plugin host.

### Location Model

Core diagnostics use SourceId and UTF-8 byte Span as the canonical location model. CLI, LSP, and editor integrations convert spans to line/column or UTF-16 positions through SourceStore or SourceView.

### Diagnostic Result Contract

CLI JSON output, Rust results, N-API results, WASM results, and LSP bridges should share one diagnostic result contract. The exact serialized schema belongs to the linter-specific design, but the shared contract includes:

- result grouping by file or message entry
- diagnostics with `parser`, `semantic`, or `lint` category
- `"error"` or `"warn"` severity
- a single JSON-visible `code` field across parser, semantic, and lint diagnostics
- UTF-8 byte span as the canonical location
- optional derived line/column or UTF-16 positions for CLI/editor consumers
- surface-specific diagnostic and operational counts as defined below

Count field names are intentionally surface-specific:

| Surface | Diagnostic counts | Operational error count |
| --- | --- | --- |
| CLI JSON `summary` | `diagnosticErrorCount` and `diagnosticWarningCount` | `errorCount`, counting top-level `errors` plus all target-local `results[].errors` |
| Programmatic lint `ok: true` | `errorCount` and `warningCount`, derived from the returned diagnostics | none; this branch has no operational errors |
| Programmatic lint `ok: false` | none; incomplete/partial diagnostics are not returned | represented by `errors[]`, without a numeric count field |

The programmatic success branch can use plain `errorCount` for diagnostic errors because no operational error count coexists on that surface. CLI summaries reserve plain `errorCount` for operational errors and use the `diagnostic*` prefix for diagnostic counts, following the shared [Phase 3A machine-readable output](./006-ox-mf2-phase-3a-tooling-foundation-design.md#machine-readable-output) rule for command-specific counts.

### Stable Identifiers and Rule Metadata

Parser diagnostic codes, semantic diagnostic codes, and configurable lint rule ids share one JSON-visible diagnostic `code` namespace and are public stable identifiers because configs, suppressions, JSON output, editor integrations, and external tooling may depend on them. Human-readable diagnostic message text is not a stable compatibility surface and may change for clarity.

The separation between this diagnostic namespace, numeric parser API errors, and Phase 3 operational string errors is indexed in [appendix-ox-mf2-error-code.md](./appendix-ox-mf2-error-code.md).

The lint crate should own rule metadata used by config validation, JSON Schema generation, generated artifacts, documentation pipelines, and internal runtime behavior. Metadata includes at least rule id, category, default/recommended status, default severity, fix capability, docs slug, and rule option schema when a rule accepts options. The docs slug is internal generated metadata unless the linter-specific design defines a public documentation URL, JSON `help`, or CLI display contract. Exact metadata fields are linter-specific design details. Runtime rule listing or introspection APIs for CLI, N-API, or WASM are deferred from the initial linter product.

### Operational Errors

The linter distinguishes lint diagnostics from operational errors. Parser, semantic, and rule diagnostics belong to lint results. Configuration parse errors, invalid config shape, file system errors, snapshot version mismatches, and internal failures are CLI/API errors and should not be mixed into normal lint diagnostics.

### Parallelism

The initial CLI lints physical file groups sequentially in stable normalized-path order, using the same physical-identity and result-ordering rules as `intlify fmt`. Logical aliases within a group remain serial even though lint does not write. Programmatic `lintMessage` remains a synchronous single-message operation and does not own a worker pool. When the deferred common scheduler is implemented, different physical groups may run concurrently under the retained boundary above without changing observable output.

### Message and Catalog Scope

The linter supports message-level linting as its reusable core. The initial layered resource workflow defined by [013-ox-mf2-resource-catalog-adapter-design.md](./013-ox-mf2-resource-catalog-adapter-design.md) extracts each selected resource entry and applies that same message-level pipeline.

Catalog-level analysis is split by the evidence it requires:

- Catalog-native structural checks over affirmative, present-entry evidence are a future layer owned jointly by the linter product boundary in [008-ox-mf2-phase-3c-linter-design.md](./008-ox-mf2-phase-3c-linter-design.md) and the resource evidence contract in 013.
- Application-level missing, unused, fallback, coverage, and reachability analysis across entries, locales, files, and source references is owned by [014-ox-mf2-message-linker-design.md](./014-ox-mf2-message-linker-design.md).
- L0/L1 lint integrations may present applicable 014 `LinkFinding` values through linter surfaces, but they must not reimplement linker analysis in `intlify_lint`.

Neither layer changes the reusable message-level lint core.

### File Discovery

CLI filesystem enumeration is extension-neutral and must not filter concrete candidates through a supported-extension list. At the later supported-input classification boundary, the CLI assembles an explicit direct-input extension list: standalone `.mf2` plus, after the resource milestone, registry-owned shipped extensions accepted for config-free individual-file and stdin classification. That list supplies direct lookup and `unsupported_input_file.details.supportedExtensions`; directory and CLI-native glob catalog selection instead requires project membership and may resolve an extensionless or otherwise arbitrary filename through an explicit catalog `format`. Unsupported direct files and unmatched patterns are CLI input conditions rather than parser diagnostics.

The standalone-message discovery, ignore, file framing, unmatched-pattern, invalid-glob, and shared input error semantics are fixed by the linter-specific [File Discovery and Shared CLI Contract](./008-ox-mf2-phase-3c-linter-design.md#file-discovery-and-shared-cli-contract). The initial resource workflow extends classification through project-matched bulk targets and the config-free direct-input exception in [013-ox-mf2-resource-catalog-adapter-design.md](./013-ox-mf2-resource-catalog-adapter-design.md) without changing the message-level linter core.

### Lint Pipeline

`lintMessage(source)` should parse the message, perform semantic validation as needed, run enabled rules, and return parser, semantic, and lint diagnostics in one result. Parser diagnostics are always included in the lint result, even when no lint rules run, so CLI and editor users can treat syntax failures as lint failures.

Diagnostics should identify their source category and stable JSON-visible code. Parser diagnostics use `category: "parser"` and a parser diagnostic code. Parser diagnostics are independent of rule configuration and are emitted with `severity: "error"` in the initial design, including recoverable syntax errors. `severity: "warn"` is reserved for future compatibility or deprecation-style parser diagnostics.

If parsing produces any parser diagnostics, the initial linter stops before semantic validation and rule execution. This keeps rule implementations from depending on incomplete recovery AST shapes. A future recovery-aware editor mode may run selected rules on partial syntax, but that is outside the initial linter core.

Semantic diagnostics, when produced by parser-owned semantic validation, are included in `lintMessage(source)` after successful parsing. They use `category: "semantic"` and a semantic diagnostic code. These diagnostics represent MF2 meaning errors rather than configurable lint rules. Initial semantic diagnostics are emitted with `severity: "error"`; `severity: "warn"` is reserved for future best-practice or ambiguous-but-valid semantic diagnostics.

If semantic validation produces any semantic diagnostics, configurable lint rules do not run. The initial linter pipeline is strictly `parser -> semantic -> rules`, and each stage must complete without diagnostics before the next stage runs.

### Severity

Rule configuration uses an ESLint/oxlint-style severity state:

- `off`: disable the rule
- `warn`: report diagnostics as warnings
- `error`: report diagnostics as errors

Emitted linter diagnostics initially use only `"warn"` and `"error"`. In prose, "warning" refers to diagnostics whose JSON `severity` is `"warn"`. `off` is a rule configuration state, not an emitted diagnostic severity. `info` and `hint` are reserved for LSP/editor or advice-style layers, not for the initial linter core.

### Fixes and Formatter Boundary

Initial configurable lint rules should be built into the Rust linter crate. JavaScript custom rules and linter plugins are out of scope. Auto-fix is also out of scope initially; future style fixes should delegate to the formatter API/crate so formatting behavior stays consistent between formatter and linter integrations.

The Phase 3 responsibility boundary is correctness in the linter and style in the formatter. Formatting style diagnostics are not part of the initial linter core. If a future lint workflow needs formatting checks or style fixes, it should call formatter check/format APIs rather than duplicate formatting logic in lint rules. Non-style lint fixes, if added later, should remain semantic-safe and independent from formatter output.

### Initial Core Semantic Diagnostics

The initial core semantic diagnostics are classified by the linter product design and specified by the parser-owned semantic validation design:

- `duplicate-declaration`
- `invalid-declaration-dependency`
- `missing-selector-annotation`
- `variant-key-arity-mismatch`
- `missing-fallback-variant`
- `duplicate-variant`
- `duplicate-option-name`

Reader-facing design-time explanations for these semantic diagnostics and configurable lint rules are indexed in [linter-rules/index.md](./linter-rules/index.md). Canonical semantic diagnostic spans, ordering, duplicate-family partitioning, and cascade behavior remain owned by [012-ox-mf2-parser-semantic-validation-design.md](./012-ox-mf2-parser-semantic-validation-design.md).

The remaining early candidates were classified out of the core semantic set: undeclared-variable checking is the configurable rule `no-undeclared-variable` because undeclared variables are valid external inputs in MF2, `unreachable-variant` is deferred, and SemanticModel construction or semantic validation invariant failures after a clean parse are internal operational errors rather than user-facing diagnostics. The linter product classification is owned by [008-ox-mf2-phase-3c-linter-design.md](./008-ox-mf2-phase-3c-linter-design.md), while parser-owned semantic diagnostic behavior is owned by [012-ox-mf2-parser-semantic-validation-design.md](./012-ox-mf2-parser-semantic-validation-design.md).

### Detailed Linter Design Reference

Detailed linter product behavior, pipeline rules, reporter behavior, binding contracts, and implementation contracts are specified in [008-ox-mf2-phase-3c-linter-design.md](./008-ox-mf2-phase-3c-linter-design.md). The parser-owned semantic validation catalog, spans, ordering, duplicate-family partitioning, and cascade behavior are specified in [012-ox-mf2-parser-semantic-validation-design.md](./012-ox-mf2-parser-semantic-validation-design.md). Reader-facing design-time rule and semantic diagnostic pages live in [linter-rules/index.md](./linter-rules/index.md). This phase document only fixes the consumer-facing pipeline and initial scope.

### Suppression

MF2 does not define line or block comments, so inline comment-style linter disable directives are not part of the initial linter product. Future suppression must be spec-compatible, such as baseline suppression files or resource/container-level metadata owned by a host format adapter. The detailed linter design owns the suppression model notes.

### Phase 3 Linter Scope

Phase 3 linter core scope:

- Rust linter engine in `crates/intlify_lint`
- CLI in `crates/intlify_cli`
- npm-distributed native CLI package
- linter-specific N-API package
- linter-specific WASM package
- JSON project configuration
- generated JSON Schema for configuration
- shared diagnostic result contract
- rule metadata for config, schema, and documentation generation
- message-level linting for single MF2 message files
- `recommended` preset
- parser and semantic diagnostics integrated into lint results
- initial configurable lint rules

### Future or Layered Linter Scope

Future or layered linter scope:

- catalog-native structural rules and presets over affirmative present-entry evidence, layered on 008 and 013
- L0/L1 presentation rules and preset policy for applicable 014 `LinkFinding` values, without reimplementing application-level linker analysis
- nested config discovery
- recovery-aware editor linting
- spec-compatible suppression model
- `lint --fix`
- rule listing/introspection commands
- resolved config printing
- file discovery debugging
- rule timing output
- LSP/editor as a direct product
- output formats beyond `text` and `json`

### Out-of-Scope Linter Features

Out-of-scope linter features:

- JavaScript custom rules
- linter plugin system

## LSP and Editor Workflow

### Product Boundary

Phase 3 does not require a dedicated LSP server or editor extension as a direct product. Instead, LSP and editor integrations are treated as adapter workflows built on top of the parser, formatter, linter, resource layer, binding packages, `SnapshotView` for syntax traversal, future `SemanticView` for message-local MF2 semantic relationships, and the 014 linker artifacts and `LinkOutcome` for application- and project-level message relationships.

The parser, formatter, and linter cores remain LSP-agnostic. They should not return LSP protocol types such as `Diagnostic`, `TextEdit`, `CodeAction`, or UTF-16 positions directly.

### Initial Scope

The initial editor workflow focuses on diagnostics and formatting.

Code actions, quick fixes, hover, completion, go-to-definition, rename, true range-only formatting, and minimal-diff formatting are not required in the initial workflow. Future `SemanticView` exposure should preserve enough stable relationships for message-local MF2 semantic features. Application-source and project-level message features instead consume the 014 checked typed-key model, `LinkOutcome`, and producer projection contracts; they do not reconstruct project semantics from `SemanticView`.

### Document and Message Mapping

Editor adapters should support both standalone `.mf2` documents and MF2 messages embedded in opted-in resource or catalog files. The concrete host format rollout is owned by [013-ox-mf2-resource-catalog-adapter-design.md](./013-ox-mf2-resource-catalog-adapter-design.md), beginning with JSON catalogs.

For standalone `.mf2` files, the adapter applies the formatter's CLI [File Framing](./007-ox-mf2-phase-3b-formatter-design.md#file-framing) contract before treating the document as one MF2 message: remove at most one leading UTF-8 BOM and then exactly one trailing `LF` or `CRLF`. The adapter retains the removed framing in its document mapping so message-local spans can be translated back to the original document. For catalog resources, the adapter does not apply file framing; it extracts each embedded MF2 message from the relevant resource or catalog key/value entry and tracks the relationship between:

- document URI and version
- resource or catalog key
- document-level value range
- extracted message text
- message-local byte offsets

Parser, formatter, and linter core APIs operate on the extracted message text. Adapters map message-local results back to the containing document.

Host document parsing, host-string unescaping and re-escaping, message-to-raw offset mapping, and validated catalog write-back are owned by [013-ox-mf2-resource-catalog-adapter-design.md](./013-ox-mf2-resource-catalog-adapter-design.md). Editor-facing document mapping, protocol position conversion, version and generation freshness checks, and construction of outer-document edits from validated resource replacements are owned by [009-ox-mf2-phase-3d-lsp-editor-design.md](./009-ox-mf2-phase-3d-lsp-editor-design.md). This overview does not define a third adapter contract.

### Span and Position Conversion

Core parser, snapshot, semantic, formatter, and linter APIs use message-local UTF-8 byte spans as their canonical location model.

LSP and editor adapters are responsible for:

- mapping message-local UTF-8 spans to document-level UTF-8 spans
- converting document-level UTF-8 spans to editor-facing UTF-16 positions
- preserving source identity through `SourceStore` / `SourceView` or equivalent adapter state

This keeps host-format parsing, document URI handling, and LSP position encoding outside the core crates and bindings.

### Artifact Reuse

Long-lived language-service workflows may reuse parse artifacts per document version to avoid re-parsing and re-encoding on every request. They can combine:

- `SourceStore` / `SourceView` for source identity and location conversion
- binary AST snapshot or decoded `SnapshotView` for syntax traversal
- future `SemanticView` for semantic queries once semantic APIs are exposed
- diagnostics store for parser, semantic, and linter diagnostics

Document-version changes invalidate extraction, mapping, and mapped-result state tied to that version. Message-level parse artifacts may survive a host-document version change only when their complete cache key, including exact message bytes, remains equal; this permits unchanged catalog entries to reuse parse work after an unrelated host edit. Formatter consumers reuse such an artifact through the workspace-internal paired `SourceStore` / `ParseResult` path rather than reparsing or converting it to a snapshot. Successful editor configuration reloads use the dependency-specific invalidation matrix and root config-generation checks in the dedicated LSP/editor design rather than flushing all artifact classes indiscriminately. Cache ownership and eviction remain adapter concerns, not parser, formatter, or linter core responsibilities. Detailed parse artifact cache policy belongs in `design/ox-mf2-parse-artifact-cache.md`.

### Diagnostics Workflow

Editor diagnostics are produced by combining parser, semantic, and linter diagnostics through the shared diagnostic result contract.

When an adapter uses `lintMessage` (or a future `lintSnapshot`), that result should be treated as the preferred diagnostic source because it already contains parser, semantic, and lint diagnostics. Adapters must avoid publishing parser diagnostics twice when they also keep parser results in a separate cache.

The initial workflow is strict:

- parser diagnostics are always reported
- semantic diagnostics are reported only when parsing has no parser diagnostics and semantic validation runs
- linter diagnostics are reported only when parser and semantic diagnostics are clean
- parser diagnostics prevent semantic validation and linter rule execution

Core `"error"` and `"warn"` severities map to editor/LSP diagnostic severity at the adapter boundary; adapters convert `"warn"` to the editor's warning severity. The core linter does not emit `info` or `hint` diagnostics initially, but editor layers may add advice-style diagnostics on top of the shared results.

Recovery-aware partial semantic or lint diagnostics for incomplete editor buffers are a future editor-mode concern. The initial editor workflow keeps the same strict `parser -> semantic -> rules` pipeline used by CLI and bindings.

### Formatting Workflow

Formatter core APIs format whole MF2 messages and return formatted message text. They do not return LSP `TextEdit` values directly.

Editor adapters should:

1. find the containing MF2 message or resource entry
2. call whole-message formatting
3. compare the original and formatted message text
4. create editor `TextEdit` values at the adapter boundary

The initial adapter replaces the whole containing message range rather than computing a minimal diff. For standalone `.mf2` documents, that range is the whole document and the replacement is the formatted unframed message followed by exactly one `LF`, with no BOM. This intentionally applies the same BOM removal and final-line-ending normalization as the CLI. For catalog resources, the replacement is only the containing message value range after required host-string re-escaping; no file framing is added to the embedded message.

If a format request contains a selected range, the initial workflow formats the containing MF2 message rather than performing true range-only formatting. When the message has parse errors, editor formatting should no-op instead of returning partially formatted output.

Editor adapters should only return `TextEdit` values when the exact protocol document version, non-reused mapping-generation token, and root config-generation token captured at request start still match the current open-document state. If any differs, the document was closed or reopened, or the containing message range can no longer be identified in the captured artifact, the adapter returns an empty edit array rather than an operational editor error or automatic retry. The dedicated LSP/editor design owns the final-check timing, same-bytes version changes, configuration-driven mapping replacement, formatter-only configuration changes, and standard-LSP response limitations.

### Configuration

Editor adapters normalize their settings into the same resolved formatter and linter configuration models used by CLI workflows. The initial editor source set and field-level precedence are fixed by the dedicated LSP/editor design: dynamic effective editor settings override initialization options, which override unified project configuration and then built-in defaults. The client, not the server, resolves vendor-specific user/workspace/workspace-folder scopes before supplying dynamic settings. Project resource membership remains authoritative under its separate fallback-only editor-overlay rule: project `PolicyAbsent` or `Unmatched` permits overlay resolution, `PolicyEmpty` or `Excluded` blocks it, and project `Matched` returns its assignment without evaluating the overlay for that path.

Configuration loading failures are root-scoped operational editor errors, not parser, semantic, formatter, linter, or resource diagnostics. The dedicated LSP/editor design retains the last successful state, suppresses formatting edits while a failure is active, de-duplicates one `window/showMessage` error per failure episode, records safe detail and recovery through `window/logMessage`, and applies selective invalidation only after a successful transactional reload.

### Out-of-Scope Editor Features

The following features are deferred from the initial Phase 3 editor workflow:

- code actions and quick fixes
- hover, completion, go-to-definition, and rename
- true range-only formatting
- minimal-diff formatting
- recovery-aware partial linting for incomplete buffers
- dedicated LSP server CLI, protocol handlers, and extension packaging

Future editor quick fixes are adapter-owned. They may use stable diagnostic codes, configurable rule metadata, formatter output, and future rule suggestions, but the initial linter core does not expose a fix API. Style fixes should call formatter APIs rather than reimplementing formatting inside editor or linter adapters. Message-local MF2 semantic features should build on future `SemanticView` exposure rather than requiring LSP-specific semantic state in the parser core. Application-source and project-level message features should consume the 014 linker model and producer contracts rather than extending `SemanticView` into a second project analyzer.

### Implementation Targets

Implementation targets are split by the workflow inputs they can actually construct.

The message-local workflow covers standalone MF2 messages and already extracted message text. Its implementations are:

- Rust integrations call the parser, formatter, and linter crates directly.
- Node-based language servers and editor extensions use the parser, formatter, and linter N-API packages.
- Browser-based editors and playgrounds use the corresponding WASM packages.

These targets share message-local parser, semantic, formatter, and linter behavior. N-API or WASM availability for those cores does not imply catalog extraction, project inventory, definition production, reference production, linking, typed-key models, or project-backed findings.

The project-backed workflow additionally requires `intlify_resource`, `intlify_contract`, `intlify_linker`, the applicable producer, and the host-owned 013/014 definition-production and project-inventory orchestration. Its initial implementation target is a Rust host, including a Rust language server or another native integration that reuses the exact shared orchestration boundary. No resource, contract, linker, or project-orchestration N-API/WASM surface is implied by this document.

A Node or browser integration cannot reconstruct the project-backed workflow by combining the existing message-level bindings. A future concrete non-Rust consumer must introduce either a separately designed native-process bridge or the required bounded read-only bindings. That decision must preserve 013/014 configuration, grouping, artifact, limit, completeness, and error contracts rather than creating a second project analyzer.

## Agent Coding Workflow

Agent coding tools such as Codex, Claude Code, Grok Build, and similar systems are separate consumers from LSP/editor integrations. They may expose plugins, skills, commands, hooks, MCP servers, ACP clients, headless execution, or other agent-specific extension points, but those extension systems should wrap the same formatter, linter, parser, and snapshot contracts rather than defining new core behavior.

The initial Phase 3 agent-facing surface should be the `intlify` CLI and stable machine-readable output. Agents can call `intlify fmt`, `intlify lint`, and future check commands in repo workflows, CI-style verification, pre-commit automation, and code review tasks. Agent, CI, and editor-adapter integrations should prefer JSON output over human-readable text output when they need to inspect diagnostics or formatting status.

Agent integrations may later provide MCP servers, agent plugins, skills, or commands, but those should remain distribution and workflow wrappers. They should not become the source of truth for formatting rules, lint diagnostics, configuration semantics, AST structure, parser-owned semantic validation, or linter result contracts.

Detailed agent integration choices are tracked in [010-ox-mf2-phase-3e-agent-integration-design.md](./010-ox-mf2-phase-3e-agent-integration-design.md). Phase 3E implementation and its remaining product-shape decisions are pending; the document's Deferred Follow-Up Notes are not current implementation requirements.

## MessagePack Transport

MessagePack is not the CST/AST representation of ox-mf2.

It is a future transport candidate for long-lived language-service workflows such as LSP, editor integration, daemon mode, and repeated semantic queries. The standard CST/AST product boundary remains the versioned Binary AST snapshot.

MessagePack transport is not an initial Phase 3 deliverable. The initial transport baseline is JSON-based CLI/API output and JSON-RPC-style language-service communication. MessagePack remains a future optimization candidate for long-lived sessions after JSON payload costs are measured.

If MessagePack transport is added later, its overhead must be measured separately from parser, SemanticModel construction, semantic validation, snapshot encoding, snapshot decoding, binding cost, and LSP request handling.

MessagePack payloads should transport query/response data or language-service session messages. They should not become a second AST format that competes with the Binary AST snapshot.

Linter results should be transportable over JSON-RPC or a future MessagePack session using the shared diagnostic result contract. Transport payloads may carry source text, Binary AST snapshot bytes, or diagnostic results depending on the consumer, but the transport layer must not redefine lint diagnostics or AST structure. Benchmarks must keep parse, SemanticModel construction, semantic validation, rule execution, snapshot encode/decode, diagnostic serialization, and transport overhead as separate phases.

## Benchmarks

Tooling and transport benchmarks must be phase-separated.

Initial Phase 3 benchmark phases:

- cli_startup_native
- cli_startup_wrapper
- cli_startup_installed
- format_preserve
- format_standard
- format_check_cli_e2e
- format_check_json
- lint_message_core
- lint_cli_e2e
- lint_json
- lint_binding_napi
- lint_binding_wasm
- cache_miss_parse
- e2e_format

The CLI startup benchmarks are Phase 3A foundation baselines. They should measure a direct native binary invocation, the npm wrapper invoking the native binary from the source tree, and an installed `node_modules/.bin/intlify` invocation separately. These measurements isolate Node.js wrapper startup, native package resolution, and native process spawn overhead from formatter, linter, parser, and transport work. They are baseline measurements rather than blocking performance gates.

Deferred product and integration benchmark phases:

- check_cli_e2e
- check_json
- agent_cli_json

`check_cli_e2e` and `check_json` activate only after the post-v0.1 addendum defines and schedules the combined `intlify check` command. They are distinct from `format_check_cli_e2e` and `format_check_json`, which measure the initial `intlify fmt --check` workflow and remain in the initial list. `agent_cli_json` activates only when Phase 3E resumes and selects a concrete agent-facing deliverable; the accepted CLI JSON consumption profile alone does not schedule that benchmark.

Future transport benchmark phases:

- lint_snapshot_core
- lint_lsp_diagnostics
- semantic_query
- jsonrpc_baseline
- messagepack_transport
- lsp_jsonrpc
- lsp_msgpack
- cache_hit_query
- long_lived_session_query

Reports should separate parser, SemanticModel construction, semantic validation, snapshot encode/decode, binding calls, CLI wrapper startup, native package resolution, native process spawn overhead, CLI JSON serialization, JSON-RPC transport, MessagePack transport, cache hit/miss behavior, and actual rule/formatter work.
