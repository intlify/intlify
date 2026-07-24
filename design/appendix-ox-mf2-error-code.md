# ox-mf2 Error Code Design Appendix

## Purpose

This appendix defines the namespace, allocation, ownership, and compatibility policy for machine-readable ox-mf2 error and diagnostic identifiers.

The numeric range policy applies only to errors that represent parser, snapshot, source-text, initialization, or binding API failure boundaries. The Phase 3 registry later in this appendix indexes stable operational string codes without allocating numeric values to them. Detailed `details` schemas, placement, ordering, and recovery behavior remain owned by the linked product design documents.

This appendix also records how compact parser classifications, JSON-visible diagnostic codes, operational error codes, subordinate reason values, host-language exceptions, protocol failures, and process exit codes remain separate. It does not turn those identifiers into one interchangeable enum.

## Namespace Separation

ox-mf2 uses separate namespaces because the same word "code" appears at several different boundaries:

| Namespace | Public representation | Naming | Contract |
| --- | --- | --- | --- |
| Numeric API errors | `OxMf2ErrorCode` integer | enum-like PascalCase names mapped to explicit integers | A parser, snapshot, source-text, initialization, or binding API call failed and could not return its normal result. |
| Compact parser classifications | integer enums such as `DiagnosticCode` | Rust enum variants and binding constants | Lossless parser and Binary AST classification. These values may be stored in compact records and are governed by parser or snapshot compatibility. |
| JSON-visible diagnostics | `diagnostics[].code` string | kebab-case | One collision-free namespace shared by parser, semantic, and configurable lint diagnostics. A diagnostic describes source content and is not an operational API failure. |
| Phase 3 operational errors | `errors[].code` string or the equivalent programmatic result field | snake_case | A tooling operation failed because of input, configuration, I/O, unsupported behavior, or an internal invariant. |
| Operational reason discriminants | `errors[].details.reason` string | snake_case | A subordinate discriminator within one operational code and owning boundary. It is never a substitute for the top-level code. |
| External host/protocol status | JavaScript exceptions, LSP/protocol error values, and process exit codes | defined by the host boundary | Kept in the host or protocol namespace rather than allocated as an ox-mf2 code unless an owning design explicitly maps it. |

The representation is part of the contract. Numeric `OxMf2ErrorCode` value `11000` and string operational code `invalid_options`, for example, are distinct identifiers even though both concern option validation. A wrapper must perform an explicitly designed conversion rather than infer one from similar names.

ox-mf2 keeps parser and snapshot enum values separate from numeric API error codes.

The following values are not API error codes and must not be managed by the API error code range policy:

- `SyntaxKind`
- `DiagnosticCode`
- `DiagnosticSeverity`
- `SectionKind`

These enums are compact parser / snapshot values. They are allowed to use small numeric values because they are stored in CST tables or Binary AST snapshot records as classification values.

`DiagnosticCode` is intentionally not an API error code. It identifies a recoverable parser diagnostic emitted into a parse result or diagnostic snapshot section. Parser diagnostics do not throw and do not represent API call failure. At a Phase 3 JSON boundary, its enum value is converted through the parser-owned catalog to a stable kebab-case string such as `missing-required-whitespace`; the compact numeric value is not serialized as the JSON-visible diagnostic code.

`OxMf2ErrorCode` is the API error code namespace. Only values in this namespace use the range policy below.

CLI JSON operational error codes, such as `config_not_found` or `command_not_ready`, are stable string identifiers in the CLI output contract. They are not numeric `OxMf2ErrorCode` values and are not allocated from the ranges in this appendix. A CLI operational error may include lower-level API error information in structured details when needed, but the CLI `errors[].code` field remains a string namespace.

Product-level formatter, linter, resource, linker, and export operational errors exposed through CLI, N-API, WASM, or an equivalent structured integration, such as `invalid_options`, `invalid_input`, `internal_error`, `source_snapshot_mismatch`, or `message_link_failed`, are also stable string identifiers. They are not allocated from the numeric `OxMf2ErrorCode` ranges unless a lower-level binding, runtime initialization, snapshot, or source-text failure needs a numeric API error code.

An operational `details.reason` value is interpreted together with its top-level `code` and owning product boundary. Reason strings do not have an independent global allocation table, and reuse under different codes does not make their remaining detail fields equivalent. A reason or other detail field is a stable compatibility surface only when its owning design says so. Human-readable `message` text is not a stable discriminator.

## Range Allocation

The public API error code namespace starts at `1000`. Values below `1000` are reserved and should not be used for ox-mf2 API errors.

| Range | Domain | Purpose |
| --- | --- | --- |
| `0..999` | Reserved | Reserved. Do not use for ox-mf2 API errors. |
| `1000..1999` | `DecodeErrorCode` | Snapshot byte validation and decode failures. |
| `2000..2999` | `SnapshotWriteErrorCode` | Snapshot encode failures from parser output, source metadata, or snapshot options. |
| `3000..3999` | `SourceTextErrorCode` | Source text attachment, lookup, and source slicing failures. |
| `4000..4999` | `ParseErrorCode` | Fatal parser API failures that cannot return a trustworthy parse result. |
| `5000..9999` | Reserved Rust crate API range | Reserved for future Rust crate API error domains. |
| `10000..10999` | `InitializationErrorCode` | Binding runtime initialization failures, especially WASM init failures. |
| `11000..11999` | `BindingValidationErrorCode` | Binding input validation failures that are not better represented by built-in `TypeError` or `RangeError`. |
| `12000+` | Reserved binding range | Reserved for future binding and host-runtime error domains. |

The gap between Rust crate API ranges and binding ranges is intentional. It leaves space for future Rust crate error domains without forcing binding-owned errors to move.

### Current Defined Allocations

The range table reserves domains; it does not imply that every integer in a domain is already assigned. The current v0.1 contract assigns these contiguous allocations, which the owning implementation enums must match before their public boundary ships:

| Domain | Currently defined values | Status note |
| --- | --- | --- |
| `DecodeErrorCode` | `1000..1036` | Emitted by snapshot validation and decode. |
| `SnapshotWriteErrorCode` | `2000..2014` | Emitted by snapshot encoding. |
| `SourceTextErrorCode` | `3000..3007` | `3000..3003` and `3005..3007` have public source-access or attachment paths; `3004` remains reserved and is not emitted. |
| `ParseErrorCode` | `4000..4008` | Emitted by fatal parser API failures. |
| `InitializationErrorCode` | `10000..10003` | Emitted by WASM initialization guards/attempts and native binding loading. |
| `BindingValidationErrorCode` | `11000` | Defined ahead of binding use for validation that is not better represented by a built-in exception. |

Once implemented, the explicit Rust enum discriminants and their compatibility guard tests are the executable source of truth. Adding a value updates the owning enum, its name mapping, binding exposure where applicable, guard tests, and this status table in the same change.

## Rust Crate API Error Domains

Rust crate API error domains are errors produced by Rust crates such as the parser crate, snapshot layer, or future product crates, and then optionally mapped through bindings.

### DecodeErrorCode

`DecodeErrorCode` uses `1000..1999`.

It covers invalid or unsupported Binary AST snapshot bytes, including invalid magic, unsupported version, malformed section table, invalid record sizes, invalid references, invalid UTF-8, unknown required sections, unknown syntax kind values, invalid diagnostic ranges, invalid source text ranges or identity, invalid extended data, invalid edge kinds, and invalid spans. `DecodeInvalidSourceIdentity = 1036` means embedded source text does not match its SourceRecord byte length or full SHA-256 digest.

### SnapshotWriteErrorCode

`SnapshotWriteErrorCode` uses `2000..2999`.

It covers failures where trusted parser output, source metadata, parse capabilities, or snapshot options cannot be encoded into a valid v0.1 Binary AST snapshot. Examples include source size overflow, too many records, missing root, invalid source id, inconsistent batch source id, requested-but-uncollected trivia, and section size overflow.

Recoverable parser diagnostics are not snapshot write errors. A snapshot may encode diagnostics and still be successfully written.

### SourceTextErrorCode

`SourceTextErrorCode` uses `3000..3999`.

It covers failures related to external source text access after parsing or decoding, such as unavailable source text, source count or keyed-id conflicts during source attachment, source identity mismatch, and out-of-bounds source slicing.

The current allocation is:

| Value | Name | Meaning |
| --: | --- | --- |
| `3000` | `SourceTextNotIncluded` | A source slice requires text that is neither embedded nor attached. |
| `3001` | `SourceTextSpanOutOfBounds` | A requested source slice is outside the verified text or splits a UTF-8 scalar boundary. |
| `3002` | `SourceTextTooLarge` | Source text cannot be represented by the `u32`-bounded source model. |
| `3003` | `SourceTextCountMismatch` | A keyed attachment array does not contain exactly `snapshot.sourceCount()` entries. |
| `3004` | `SourceTextUnpairedSurrogate` | Reserved; Phase 2 JavaScript bindings use built-in `TypeError` instead. |
| `3005` | `SourceTextAlreadyIncluded` | `withSources()` was called for a snapshot that already embeds source text. |
| `3006` | `SourceTextDuplicateSourceId` | A keyed attachment array repeats a valid snapshot-local SourceId. |
| `3007` | `SourceTextIdentityMismatch` | Attached UTF-8 bytes do not match the SourceRecord byte length or full SHA-256 digest. |

`SourceTextDuplicateSourceId` and `SourceTextIdentityMismatch` expose the offending snapshot-local id as `OxMf2ErrorShape.sourceId`. Invalid non-integer or out-of-range attachment ids use built-in `RangeError` and do not consume a numeric code.

`SourceTextUnpairedSurrogate = 3004` is reserved and must not be reused, but Phase 2 bindings do not emit it. They reject unpaired surrogates during raw JavaScript input validation with built-in `TypeError`, before parsing, source attachment, or source slicing. A future input model that accepts WTF-8 or UTF-16 may assign semantics to the reserved code only through an explicit contract revision.

### ParseErrorCode

`ParseErrorCode` uses `4000..4999`.

It covers fatal parser API failures: an oversized source, an invalid source id, exhaustion of a `u32`-indexed parser resource, or a missing CST root. Recoverable MF2 syntax errors remain successful parse results with diagnostics and never use this range. `parse_batch` reports the zero-based input index together with the underlying `ParseError` and fails the whole call rather than returning a partial batch. N-API and WASM `parseBatch` preserve the underlying `ParseErrorCode` and expose that position as optional `OxMf2ErrorShape.inputIndex`; other error paths omit the property.

### Message Linker Typed Error Domains

[The message-linker design](./014-ox-mf2-message-linker-design.md) defines `ArtifactContractError`, `ArtifactReadError`, `LinkOperationalError`, `ExportPreparationError`, and `ExportError` as typed Rust or cross-process contract boundaries. They are not numeric `OxMf2ErrorCode` domains and do not consume the reserved `5000..9999` range merely because they originate in Rust crates.

Their variants and checked structured evidence remain authoritative inside the owning API. A CLI or platform adapter performs the explicit boundary mapping registered below; it does not serialize a Rust discriminant as the CLI code, allocate one top-level code per nested variant, or collapse the evidence into a display string.

`LinkFinding` is successful analysis data in the JSON-visible finding/diagnostic family, not a typed operational error. `ExportMessageValidationFailure` similarly carries existing parser and semantic diagnostic identities as result data. Neither is admitted into `errors[].code` merely because it can block bundle generation.

## Binding Error Domains

Binding error domains are produced by N-API or WASM wrapper code rather than the Rust crate API or snapshot format itself.

### InitializationErrorCode

`InitializationErrorCode` uses `10000..10999`.

It covers binding runtime setup failures. The initial allocation is:

| Value | Name | Meaning |
| --- | --- | --- |
| `10000` | `InitializationWasmNotInitialized` | A synchronous WASM API was called before any initialization attempt completed successfully. |
| `10001` | `InitializationNativeBindingUnavailable` | The N-API host package could not load a supported native binding. |
| `10002` | `InitializationWasmInputConflict` | Explicit WASM input was supplied after an initialization attempt had started or after initialization had succeeded. |
| `10003` | `InitializationWasmInitializationFailed` | An accepted WASM initialization attempt failed while importing, resolving/reading/fetching input, compiling, instantiating, or installing the generated binding. |

`InitializationWasmInputConflict` does not cancel or replace the active/installed runtime. `InitializationWasmInitializationFailed` installs no usable binding, clears the in-flight attempt, and permits a later default-input or explicit-input retry. Its optional host `cause` and human-readable message are diagnostic aids rather than stable discriminators. Values outside the declared `WasmInitInput` runtime boundary use built-in `TypeError` when rejected during binding input validation; they do not consume another initialization code.

### BindingValidationErrorCode

`BindingValidationErrorCode` uses `11000..11999`.

Most wrong input types, invalid numeric ranges, and indexed accessor misuse should use built-in `TypeError` or `RangeError` when possible. `BindingValidationErrorCode` is reserved for binding-specific validation failures that need a stable ox-mf2-specific code.

## Phase 3 Operational String Code Registry

Phase 3 operational codes form one snake_case string namespace across CLI JSON and equivalent product-programmatic failure results. The registry below prevents accidental spelling collisions and points to the document that owns each detailed contract. It is not a second copy of each code's `details` schema.

Intentional reuse keeps the same top-level code rather than adding a product-prefixed alias. The owner of the concrete boundary still defines required details, top-level versus target-local placement, ordering, recovery, and cross-surface conversion. Callers therefore interpret an error using the product API or CLI `command` context as well as `code`.

### Shared Tooling Foundation

[Phase 3A](./006-ox-mf2-phase-3a-tooling-foundation-design.md) owns the shared CLI envelope and the initial common codes:

| Domain | Stable codes |
| --- | --- |
| CLI input and routing | `invalid_cli_argument`, `unknown_cli_option`, `missing_cli_option_value`, `duplicate_cli_option`, `input_path_unrepresentable`, `reporter_not_supported`, `unknown_command`, `command_not_ready` |
| Raw binding or programmatic input | `invalid_input` |
| Project configuration | `config_not_found`, `config_conflict`, `config_extension_unsupported`, `config_read_failed`, `config_parse_failed`, `config_validation_failed`, `config_schema_generation_failed` |
| Node CLI wrapper | `native_platform_unsupported`, `native_package_not_found`, `native_binary_not_found`, `native_binary_failed` |

`invalid_cli_argument` is a reserved generic fallback with concrete Phase 3B and Phase 3C emit sites. `config_schema_generation_failed` is reserved for schema build or validation workflows and is not currently emitted through the CLI JSON envelope. A defined-ahead code must remain documented as reserved until its first public emit path is designed.

### Formatter and Shared File Processing

[Phase 3B](./007-ox-mf2-phase-3b-formatter-design.md#operational-error-codes) owns formatter-specific failures and the file discovery, ignore, read, and write contracts later reused by lint and resource workflows:

| Domain | Stable codes |
| --- | --- |
| Formatter input and snapshot | `source_snapshot_mismatch`, `unsupported_input_file`, `invalid_options`, `invalid_snapshot` |
| Discovery and ignore processing | `invalid_ignore_pattern`, `ignore_file_read_failed`, `unmatched_input` |
| Target I/O and physical-alias write containment | `input_read_failed`, `output_write_failed`, `alias_processing_blocked` |
| Formatter invariant | `internal_error` |

The formatter also reuses the applicable shared Phase 3A codes. [Formatter IR](./011-ox-mf2-formatter-ir-design.md#invariant-and-error-boundaries) owns the pipeline phases and invariant boundary that become formatter `internal_error` failures.

The deferred shared CLI scheduler will reuse `internal_error` for a command-fatal worker-runtime failure when that follow-up is implemented. [Phase 3 transport](./005-ox-mf2-phase-3-tooling-transport-design.md#cli-parallel-execution-boundary) retains its initialization, dispatch, execution, join, placement, deterministic selection, cancellation, result-discard, write-side-effect, and exit contracts. Initial sequential CLI execution cannot emit this reason.

### Linter and Semantic Validation

[Phase 3C](./008-ox-mf2-phase-3c-linter-design.md#failure-model) introduces no additional top-level operational code. It reuses `invalid_input`, `invalid_options`, `internal_error`, the shared configuration codes, and the shared file-processing codes. Phase 3C owns the linter-specific reason and detail variants for source validation, lint options, semantic invariant failures, semantic API misuse, and lint-rule invariant failures. [Parser semantic validation](./012-ox-mf2-parser-semantic-validation-design.md) owns the underlying semantic invariant boundary and semantic diagnostic catalog.

Parser, semantic, and lint findings remain in the separate JSON-visible diagnostic namespace. Their collision and catalog rules are defined by the [Phase 3 transport](./005-ox-mf2-phase-3-tooling-transport-design.md#stable-identifiers-and-rule-metadata), [Phase 3C diagnostic shape](./008-ox-mf2-phase-3c-linter-design.md#diagnostic-shape), and [semantic diagnostic catalog API](./012-ox-mf2-parser-semantic-validation-design.md#diagnostic-code-catalog-api).

### Resource Catalog Workflow

[The resource catalog adapter design](./013-ox-mf2-resource-catalog-adapter-design.md#resource-error-model) owns these user-actionable resource codes:

- `resource_format_unsupported`
- `resource_parse_failed`
- `resource_entry_unsupported`
- `resource_document_unsupported`
- `resource_limit_exceeded`

It reuses `config_validation_failed`, `input_path_unrepresentable`, `input_read_failed`, `output_write_failed`, and `internal_error`. The formatter/shared CLI contract owns `alias_processing_blocked` for physical-alias write-failure containment. The resource workflow owns `input_target_conflict` for conflicting standalone/catalog classifications of one physical input.

The editor design intentionally projects original-extraction `resource_format_unsupported`, `resource_entry_unsupported`, `resource_document_unsupported`, and `resource_limit_exceeded` into error-severity editor diagnostics with source `ox-mf2-resource`. A `resource_limit_exceeded` with `phase: "validate_write_back"` is instead a request-scoped formatting failure: it returns no edits and does not change document diagnostics or installed state. This is a defined cross-surface projection, not admission into the parser/semantic/lint diagnostic namespace. `resource_parse_failed` remains an operational extraction outcome and does not produce a new ox-mf2 editor diagnostic; the exact publication and retention behavior is owned by [Diagnostics Publication](./009-ox-mf2-phase-3d-lsp-editor-design.md#diagnostics-publication).

### Message Linker and Export Workflow

[The message-linker design](./014-ox-mf2-message-linker-design.md#cli-operational-error-mapping) reserves one top-level operational code per user-visible boundary rather than one code per typed nested variant:

| Domain | Stable code | Initial status |
| --- | --- | --- |
| Artifact production and contract ingestion | `message_artifact_failed` | Defined for M0 contract ingestion and the closed built-in producer stage/reason evidence owned by 014. |
| Stateless link request execution | `message_link_failed` | Defined for M0 stateless link execution and host mapping; user-visible emission begins only when a consuming integration ships. |
| Exporter invocation and checked common output | `message_export_failed` | Reserved for M3. |
| Platform destination mapping and registration | `message_output_registration_failed` | Reserved for M3 emission; its closed evidence and target-continuation contract are defined by 014. |

Each code requires the closed `details.kind` vocabulary and canonical structured evidence defined by 014. Artifact transport and ordinary configured-source I/O intentionally reuse `input_read_failed`; config admission reuses `config_validation_failed`; and internal invariants reuse `internal_error`.

The initial M3 CLI exposes no raw export-diagnostic retention option and therefore has no `invalid_options` mapping for `ExportValidationLimitConfigurationError`. A future CLI control or custom raw-input adapter must define its own exact decoding and structured error mapping before reusing `invalid_options`; this appendix does not reserve one implicitly.

`resource_limit_exceeded` remains specific to 013 host-resource processing and is not an alias for an artifact, linker, or exporter limit.

`LinkFinding` values and `ExportMessageValidationFailure` parser/semantic diagnostics remain result data under their own namespaces. A blocking finding or ordinary source diagnostic can prevent output without creating one of the four operational codes. Conversely, an artifact, link, export, or registration operational error never becomes a source diagnostic merely to acquire severity or location.

The four codes are defined ahead of their first CLI emission. `message_artifact_failed` and `message_link_failed` are project-transaction top-level errors. `message_artifact_failed` has its M0 artifact and built-in producer evidence fixed by 014; later additions remain append-only and milestone-owned. `message_export_failed` and `message_output_registration_failed` are selected-target result errors beginning with M3. The 014 M3 text fixes the emit target DTO, counters, output-state field, registration's bounded evidence and deterministic failure selection, independent-target continuation, and command exit precedence. The two codes do not emit before M3, and an implementation does not publish a placeholder or reduced shape ahead of that milestone.

### Detail Schema Ownership

The registry assigns top-level code ownership. Stable subordinate schemas remain in these documents:

| Detail family | Owning design |
| --- | --- |
| CLI routing, config loading, config parsing, native wrapper, and shared envelope | [Phase 3A](./006-ox-mf2-phase-3a-tooling-foundation-design.md) |
| Physical identity/grouping, metadata inspection, alias ordering, and common fail-stop boundary | [Phase 3B](./007-ox-mf2-phase-3b-formatter-design.md#cli-workflow) and [Phase 3 transport](./005-ox-mf2-phase-3-tooling-transport-design.md) |
| Deferred shared CLI worker scheduling | [Phase 3 transport deferred follow-up](./005-ox-mf2-phase-3-tooling-transport-design.md#cli-parallel-execution-boundary) |
| Formatter options, snapshots, discovery, ignore files, target I/O, exact alias-blocked results, and formatter invariant phases | [Phase 3B](./007-ox-mf2-phase-3b-formatter-design.md) and [Formatter IR](./011-ox-mf2-formatter-ir-design.md) |
| Linter binding input/options and semantic or rule invariant reasons | [Phase 3C](./008-ox-mf2-phase-3c-linter-design.md) and [Parser semantic validation](./012-ox-mf2-parser-semantic-validation-design.md) |
| Resource config, catalog classification conflicts, parsing, representability, limits, catalog result specializations, and adapter invariant reasons | [Resource catalog adapter](./013-ox-mf2-resource-catalog-adapter-design.md) |
| Message artifact, linker, export-preparation, exporter, and platform-registration mappings | [Message linker](./014-ox-mf2-message-linker-design.md#cli-operational-error-mapping) and its M0/M3/M5 addenda |

When two products reuse a top-level code, their detail variants are a union only after the owning documents define how a consumer distinguishes and validates every variant. This appendix indexes the current ownership but does not silently normalize incompatible `details` fields.

`internal_error` has one cross-product requirement: `details.reason` is always present and is the first variant discriminator. Consumers select the reason variant before reading owner-specific fields. The registry includes active reasons plus explicitly reserved deferred reasons:

| Owner | Registered `internal_error` reasons | Reason-specific fields |
| --- | --- | --- |
| Shared CLI scheduler (deferred; not emitted by initial sequential execution) | `cli_worker_runtime_failed` | required `phase`: `initialize`, `dispatch`, `execute`, or `join`; optional top-level `path` when the active logical target is known exactly |
| Formatter | `formatter_invariant_failed` | required formatter `phase` |
| Linter and parser semantic validation | `semantic_invariant_failed`, `semantic_api_misuse`, `lint_rule_invariant_failed` | `semantic_invariant_failed` requires `stage`; `semantic_api_misuse` has no additional required field; `lint_rule_invariant_failed` requires `ruleId` |
| Resource catalog adapter | `resource_invalid_entry_handle`, `resource_artifact_identity_exhausted`, `resource_offset_map_invariant_failed`, `resource_offset_map_failed`, `resource_write_back_failed`, `resource_adapter_invariant_failed` | required resource `phase`; optional `entryKey` when one entry is known |
| Message linker and export workflow | `message_artifact_invariant_failed`, `message_producer_invariant_failed`, `message_linker_invariant_failed`, `message_export_preparation_invariant_failed`, `message_exporter_invariant_failed`, `message_output_registration_invariant_failed` | 014 typed evidence where defined; `message_export_preparation_invariant_failed` and `message_exporter_invariant_failed` require `invariant`; remaining evidence fields must be fixed by the owning milestone before first emission |

Reasons in this `internal_error` registry are globally unique across products. A new product-owned reason must not reuse an existing spelling for another condition. Deliberate reuse is allowed only when every owner is reporting the same shared invariant and adopts one identical reason-specific details contract. Each product keeps a compile-time reason catalog available to tests; the Phase 3 shared CLI integration test combines those catalogs and fails on a duplicate. This catalog is test-facing metadata and does not require a public runtime introspection API.

## Compatibility Policy

Once a numeric API error code is released, its meaning is stable within the relevant compatibility line. Names may be documented more clearly over time, but a numeric value must not be reused for a different error.

Adding a new error to an existing range is allowed when it belongs to that domain. If an error belongs to a new domain, allocate it from the reserved Rust crate API range or the reserved binding range rather than overloading an existing domain.

Changing `SyntaxKind`, compact `DiagnosticCode`, `DiagnosticSeverity`, or `SectionKind` values is a Binary AST / parser compatibility concern, not an `OxMf2ErrorCode` range concern.

Once released, a JSON-visible parser, semantic, or lint diagnostic code must not be renamed or reassigned without the breaking-change process defined by its owner. The global collision test covers all three diagnostic categories.

Once released, a Phase 3 operational string code must not be reused for another condition. Renaming or removing a code, changing its `kind`, moving it between global and target-local placement, or changing a documented required detail field is a compatibility change. Documented reason values and other enumerated detail values follow the same rule. Human-readable messages, undocumented debug context, and platform-specific `rawOsError` numbers are not portable discriminators.

## Registration Checklist

Any new machine-readable failure or diagnostic identifier must answer these questions in its owning design before implementation:

1. Which namespace and representation does it use: numeric API error, JSON-visible diagnostic, Phase 3 operational error, subordinate reason, or an external host/protocol status?
2. Does an existing identifier already describe the condition? Reuse is intentional and documented; aliases that differ only by product prefix are not added casually.
3. Which document and implementation type own the identifier, and is it emitted now or reserved for a future boundary?
4. For an operational error, what are its `kind`, global or target-local placement, exit/result/exception behavior, deterministic precedence, and recovery behavior?
5. Which `details` fields and reason values are required, optional, stable, portable, and safe to expose? Dependency error names, Rust `Debug` output, source text, and other accidental internals are not public codes.
6. How is the failure mapped across Rust, CLI JSON, N-API, WASM, editor/LSP, and agent-facing consumers without conflating separate namespaces?
7. Which enum guards, catalog collision tests, binding parity tests, reporter fixtures, or editor mapping tests lock the contract?

For a numeric API error, allocate the next unused value in the correct domain unless the value was explicitly reserved. For a JSON-visible diagnostic or Phase 3 operational error, add the stable spelling to the owning catalog or registry and verify global collision rules before release.

## TypeScript Exposure

Bindings expose `OxMf2ErrorCode` as an enum-like numeric const object and expose helper functions such as `oxMf2ErrorCodeName(code)`.

Bindings may expose domain-specific names internally, but the public error shape uses the unified `OxMf2ErrorCode` namespace:

```ts
type OxMf2ErrorShape = {
  code: OxMf2ErrorCode
  message: string
  cause?: unknown
  sectionKind?: SectionKind
  offset?: number
  recordIndex?: number
  inputIndex?: number
  sourceId?: number
}
```

`sourceId` is a snapshot-local SourceId and is present only when a source-text attachment error identifies one source. It is omitted for source count mismatch, slice errors, parser batch errors, and unrelated domains.

`DiagnosticCode` remains a separate numeric const object for parser diagnostics.
