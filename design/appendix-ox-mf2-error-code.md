# ox-mf2 Error Code Design Appendix

## Purpose

This appendix defines the numeric range policy for ox-mf2 API error codes.

The range policy applies only to errors that represent API failure boundaries, such as snapshot decode failures, snapshot write failures, source text failures, initialization failures, and binding validation failures.

The range policy does not apply to parser or snapshot classification enums, or to CLI JSON operational error string codes.

## Namespace Separation

ox-mf2 keeps parser and snapshot enum values separate from API error codes.

The following values are not API error codes and must not be managed by the API error code range policy:

- `SyntaxKind`
- `DiagnosticCode`
- `DiagnosticSeverity`
- `SectionKind`

These enums are compact parser / snapshot values. They are allowed to use small numeric values because they are stored in CST tables or Binary AST snapshot records as classification values.

`DiagnosticCode` is intentionally not an API error code. It identifies a recoverable parser diagnostic emitted into a parse result or diagnostic snapshot section. Parser diagnostics do not throw and do not represent API call failure.

`OxMf2ErrorCode` is the API error code namespace. Only values in this namespace use the range policy below.

CLI JSON operational error codes, such as `config_not_found` or `command_not_ready`, are stable string identifiers in the CLI output contract. They are not numeric `OxMf2ErrorCode` values and are not allocated from the ranges in this appendix. A CLI operational error may include lower-level API error information in structured details when needed, but the CLI `errors[].code` field remains a string namespace.

## Range Allocation

The public API error code namespace starts at `1000`. Values below `1000` are reserved and should not be used for ox-mf2 API errors.

| Range | Domain | Purpose |
| --- | --- | --- |
| `0..999` | Reserved | Reserved. Do not use for ox-mf2 API errors. |
| `1000..1999` | `DecodeErrorCode` | Snapshot byte validation and decode failures. |
| `2000..2999` | `SnapshotWriteErrorCode` | Snapshot encode failures from parser output, source metadata, or snapshot options. |
| `3000..3999` | `SourceTextErrorCode` | Source text attachment, lookup, and source slicing failures. |
| `4000..9999` | Reserved Rust crate API range | Reserved for future Rust crate API error domains. |
| `10000..10999` | `InitializationErrorCode` | Binding runtime initialization failures, especially WASM init failures. |
| `11000..11999` | `BindingValidationErrorCode` | Binding input validation failures that are not better represented by built-in `TypeError` or `RangeError`. |
| `12000+` | Reserved binding range | Reserved for future binding and host-runtime error domains. |

The gap between Rust crate API ranges and binding ranges is intentional. It leaves space for future Rust crate error domains without forcing binding-owned errors to move.

## Rust Crate API Error Domains

Rust crate API error domains are errors produced by Rust crates such as the parser crate, snapshot layer, or future product crates, and then optionally mapped through bindings.

### DecodeErrorCode

`DecodeErrorCode` uses `1000..1999`.

It covers invalid or unsupported Binary AST snapshot bytes, including invalid magic, unsupported version, malformed section table, invalid record sizes, invalid references, invalid UTF-8, unknown required sections, unknown syntax kind values, invalid diagnostic ranges, invalid source text ranges, invalid extended data, invalid edge kinds, and invalid spans.

### SnapshotWriteErrorCode

`SnapshotWriteErrorCode` uses `2000..2999`.

It covers failures where trusted parser output, source metadata, or snapshot options cannot be encoded into a valid v0.1 Binary AST snapshot. Examples include source size overflow, too many records, missing root, invalid source id, inconsistent batch source id, and section size overflow.

Recoverable parser diagnostics are not snapshot write errors. A snapshot may encode diagnostics and still be successfully written.

### SourceTextErrorCode

`SourceTextErrorCode` uses `3000..3999`.

It covers failures related to external source text access after parsing or decoding, such as unavailable source text, source count mismatch during source attachment, unpaired surrogate rejection at the binding boundary, and out-of-bounds source slicing.

## Binding Error Domains

Binding error domains are produced by N-API or WASM wrapper code rather than the Rust crate API or snapshot format itself.

### InitializationErrorCode

`InitializationErrorCode` uses `10000..10999`.

It covers runtime setup failures, primarily WASM initialization before using sync APIs or invalid WASM initialization input.

### BindingValidationErrorCode

`BindingValidationErrorCode` uses `11000..11999`.

Most wrong input types, invalid numeric ranges, and indexed accessor misuse should use built-in `TypeError` or `RangeError` when possible. `BindingValidationErrorCode` is reserved for binding-specific validation failures that need a stable ox-mf2-specific code.

## Compatibility Policy

Once a numeric API error code is released, its meaning is stable within the relevant compatibility line. Names may be documented more clearly over time, but a numeric value must not be reused for a different error.

Adding a new error to an existing range is allowed when it belongs to that domain. If an error belongs to a new domain, allocate it from the reserved Rust crate API range or the reserved binding range rather than overloading an existing domain.

Changing `SyntaxKind`, `DiagnosticCode`, `DiagnosticSeverity`, or `SectionKind` values is a Binary AST / parser compatibility concern, not an `OxMf2ErrorCode` range concern.

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
}
```

`DiagnosticCode` remains a separate numeric const object for parser diagnostics.
