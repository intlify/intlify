# ox-mf2 Phase 2 Binary AST and Language Binding Detailed Design

## Purpose

This document defines implementation-oriented design details for the Phase 2 cross-language boundary of ox-mf2.

The foundation document is [001-ox-mf2-toolchain-foundation.md](./001-ox-mf2-toolchain-foundation.md). It defines the high-level philosophy and phase plan. This document defines the lower-level shape of Binary AST snapshots, language bindings, snapshot APIs, binding result boundaries, and transport boundaries.

## Basic Policy

ox-mf2 uses the Rust core as the single semantic implementation. MF2 parsing, CST construction, semantic analysis, diagnostics, formatting, and linting are not reimplemented in other languages.

Phase 1 builds a recovering parser and snapshot-friendly construction-time tables. Phase 2 introduces a versioned Binary AST snapshot as the product boundary for N-API, WASM, later language bindings, persistence, and transport.

The Rust core hot path keeps `CstTables` / `CstView` / `SemanticModel`. Binary AST snapshot is not the normal Rust core parse output. It is an encoded representation derived from `CstTables` for language boundaries, persistence, worker transfer, and batch transfer.

This design avoids the following path.

```text
public typed AST
  -> recursive conversion
  -> Binary AST snapshot
```

The intended path is as follows.

```text
parser / lowering
  -> snapshot-friendly construction tables
  -> SnapshotWriter
  -> versioned Binary AST snapshot
  -> N-API / WASM / persistence decoder-accessor view
```

![ox-mf2 Binary AST and binding architecture](./assets/003-ox-mf2-binary-ast-binding-architecture.svg)

## Identifier Model

Binary AST snapshot inherits the Phase 1 identifier model defined in [002-ox-mf2-phase-1-rust-parser-design.md](./002-ox-mf2-phase-1-rust-parser-design.md), and adds RootId for snapshot entry points.

Inside a snapshot, RootId, NodeId, TokenId, TriviaId, and SourceId remain `u32` indexes into the corresponding section or source table. These ids are not optional. `RootId = 0`, `NodeId = 0`, `TokenId = 0`, `TriviaId = 0`, and `SourceId = 0` are all valid indexes, and no none sentinel is defined for them. Spans remain UTF-8 byte offsets and do not include source_id. Source identity is obtained from record `source_id` or root/source context. Line/column and UTF-16 editor positions belong to display/editor boundaries and are not stored in snapshot node fields.

SourceStore and ParseInput are also defined in the Phase 1 parser design. Phase 2 uses the same SourceId and ParseInput metadata to build snapshot roots section entries and binding result mappings.

## Parser and Snapshot API

Phase 2 snapshot APIs:

```rust
parse_source_to_snapshot(
  sources: &SourceStore,
  source_id: SourceId,
  parse_options: ParseOptions,
  snapshot_options: SnapshotOptions,
) -> SnapshotResult

parse_session_to_snapshot(
  session: &ParseSessionResult<'_>,
  snapshot_options: SnapshotOptions,
) -> SnapshotResult

parse_batch_to_snapshot(
  inputs: &[ParseInput],
  parse_options: ParseOptions,
  snapshot_options: SnapshotOptions,
) -> BatchSnapshotResult
```

`SourceStore`, `ParseInput`, `ParseOptions`, `ParseResult`, and `ParseSessionResult` are defined in [002-ox-mf2-phase-1-rust-parser-design.md](./002-ox-mf2-phase-1-rust-parser-design.md). Snapshot generation is a separate Phase 2 responsibility so parse cost and snapshot encoding cost can be measured independently.

`parse_source_to_snapshot` is a convenience API that builds a snapshot from the normal owned parse path. `parse_session_to_snapshot` encodes a snapshot from a borrowed parse result produced with `ParseWorkspace`; it is used by paths that want to reduce allocation variance, such as workspace reuse, benchmarks, and LSP. In both cases, the snapshot builds SourceRecord and RootRecord from input `SourceStore` / `ParseInput` metadata.

## Options

Parse behavior and snapshot output use separate option types. Parse behavior is defined by Phase 1 `ParseOptions`; this document defines only snapshot-specific options.

```rust
SnapshotOptions {
  include_diagnostics: bool,
  include_source_text: bool,
  include_trivia: bool,
  preserve_whitespace: bool,
}
```

Snapshot options must not change MF2 parser semantics. They only decide which already-produced parser data is encoded into the snapshot.

Defaults:

- `include_diagnostics = true`
- `include_source_text = false`
- `include_trivia = true`
- `preserve_whitespace = true`

`include_source_text = false` is the default. In normal binding parse results, source text is already retained by the caller, binding layer, or SourceStore, so duplicating it in the snapshot increases size and transfer cost.

`include_source_text = true` is used when the snapshot alone must resolve `source_slice(source_id, span)`. Main uses are debug dump, persistence, worker transfer, fixture snapshots, and transport to external processes.

## Result Types

The result types in this section are Rust snapshot-producing API shapes. The default public API for N-API / WASM bindings does not return raw bytes directly; it wraps them in the result object and snapshot accessor described later.

```rust
SnapshotResult {
  bytes: Vec<u8>,
  root: RootId,
  diagnostics: Vec<Diagnostic>,
}

BatchSnapshotResult {
  bytes: Vec<u8>,
  roots: Vec<RootId>,
  diagnostics: Vec<Diagnostic>,
}
```

`SnapshotResult.root` is the RootId for a single input. `BatchSnapshotResult.bytes` is a shared snapshot buffer. `roots` is a RootId array corresponding to input order. Each root has only root node, source_id, and diagnostic range through RootRecord. Path, locale, message_id, base_offset, and optional source text live in SourceRecord. This lets batch parsing share string tables and snapshot sections across many messages.

Rust snapshot-producing APIs return `bytes + RootId` and do not store a self-referential `RootHandle { snapshot, id }` directly in the result struct. RootHandle is created after a decoded `SnapshotView` or binding result object owns the snapshot.

## Binary AST Snapshot

Binary AST snapshot is the canonical Phase 2 cross-language CST/AST product boundary and persistence format. It does not replace the normal Rust core parse output, and it is not a second semantic implementation.

![ox-mf2 Binary AST snapshot format layout](./assets/003-ox-mf2-binary-ast-format-layout.svg)

Phase 2 snapshot focuses on the lossless CST surface.

- optional source text
- string table
- nodes
- edges
- tokens
- trivia
- inline span fields in records
- diagnostics
- roots section / RootRecord entry points

The semantic model remains available inside Rust and is exposed separately as SemanticView or a later compact semantic snapshot.

Source metadata is a core section. Source text bytes are an optional `source text data` section. The snapshot does not have a separate spans section; NodeRecord, TokenRecord, TriviaRecord, and DiagnosticRecord hold `span_start` / `span_end` inline. With `include_source_text = false`, the decoder cannot resolve `source_slice(source_id, span)` from the snapshot alone. In that case, the decoder/accessor uses external source text retained by the binding layer or reports source text unavailable.

### Wire Layout

![ox-mf2 Binary AST wire layout](./assets/003-ox-mf2-wire-layout.svg)

The snapshot format is based on a fixed-size header, section table, and typed fixed-record sections.

```text
SnapshotHeader {
  magic: [u8; 8],
  major_version: u16,
  minor_version: u16,
  feature_flags: u32,
  header_len: u32,
  section_table_offset: u32,
  section_count: u16,
  reserved: u16,
}

SectionRecord {
  kind: u16,
  flags: u16,
  offset: u32,
  byte_len: u32,
  count: u32,
  record_size: u16,
  alignment: u8,
  reserved: u8,
}
```

`SectionRecord.kind` is a stable numeric enum. Once assigned, a SectionKind number is not reused. Changing the meaning of a section incompatibly requires a major version bump.

The snapshot header has only `major_version` and `minor_version` for wire format compatibility. Patch version is not stored in the snapshot header; it is managed by the crate / npm package / WASM package release version. `major_version` represents incompatible format changes, while `minor_version` represents backward-compatible additions to sections, flags, or metadata.

Adding a new optional section is allowed in a minor version. Existing decoders can skip unknown sections with `SectionFlags.required = false` after validation, so optional metadata, debug data, and future semantic data that do not change existing semantics can be minor additions. A new required section that is necessary to interpret the snapshot correctly requires a major version bump.

`feature_flags` is fully reserved in v1 and only `0` is allowed. v1 extension detection uses the section table and `SectionFlags.required`; header-level feature flags are not used until a clear purpose exists.

```text
SectionFlags {
  required: bit0,
}
```

Unknown sections are handled using `SectionFlags.required`. An unknown section kind can be skipped if `required = false` and offset/size/alignment are valid. An unknown section with `required = true` is rejected because a decoder that cannot read it may be unable to interpret the snapshot correctly.

All multi-byte numeric fields are little-endian. `offset` and `byte_len` are byte offsets / byte lengths from the start of the snapshot buffer. In Phase 2, buffer offsets, section lengths, record counts, NodeId, TokenId, TriviaId, and SourceId are all in the `u32` domain.

Each section's count uses `SectionRecord.count` as the only source of truth. root count, node count, edge count, token count, and trivia count are read from the corresponding `roots`, `nodes`, `edges`, `tokens`, and `trivia` section counts. The header does not duplicate counts.

Section starts are 8-byte aligned by default. Sections with `record_size > 0` are typed fixed-record arrays, and decoders lazy-access `offset + index * record_size`. Sections with `record_size = 0` are raw byte sections, such as string data, source text data, and extended variable data.

The section table has stable IDs per `SectionKind`. v1 section kinds:

```text
0  = invalid/reserved
1  = roots
2  = sources
3  = nodes
4  = edges
5  = tokens
6  = trivia
7  = diagnostics
8  = diagnostic_labels
9  = string_offsets
10 = string_data
11 = source_text_data
12 = extended_data
```

`SectionKind = 0` is not a valid section. Decoders reject a section with `kind = 0`.

nodes, edges, tokens, roots, sources, string offsets, and string data are core sections. Core sections must exist in the section table with `SectionFlags.required = true`. A missing core section, or a core section with `required = false`, makes the snapshot invalid.

Minimum counts for core sections are `roots.count >= 1`, `sources.count >= 1`, and `nodes.count >= 1`. `edges.count` and `tokens.count` may be `0`. string offsets may have `count = 0` when there are no strings, and string data may have `byte_len = 0`.

source text data, trivia, diagnostics, diagnostic labels, and extended data are optional sections. Optional sections normally use `SectionFlags.required = false`. They may be empty depending on options or content. A missing optional section is equivalent to `count = 0`.

Decoder rules:

- reject incompatible major versions
- accept minor version differences only when backward compatible
- do not use the snapshot header to distinguish patch-level implementation differences
- reject `feature_flags != 0` in v1
- reject unknown required features
- reject unknown required sections when `SectionFlags.required = true`
- skip unknown optional sections when `SectionFlags.required = false` and `offset`, `byte_len`, and `alignment` are valid
- reject sections where `offset + byte_len` exceeds buffer length
- reject sections where `record_size > 0` and `byte_len != count * record_size`

### Roots Section

![ox-mf2 roots section](./assets/003-ox-mf2-roots-section.svg)

The roots section is a core section. RootRecord is the batch input entry point and stays compact without metadata payload.

RootRecord array order is fixed to `parse_batch` input order. `roots[i]` corresponds to `inputs[i]`. A single-input snapshot uses the same layout with `roots.count = 1`.

```text
RootRecord {
  root_node: u32,
  source_id: u32,
  diagnostic_start: u32,
  diagnostic_count: u32,
}
```

`root_node` is a valid NodeId into the nodes section and must satisfy `root_node < nodes.count`. There is no root none sentinel. The Phase 1 recovering parser returns diagnostics and partial trees for normal parse failure whenever possible. Fatal errors that prevent snapshot construction are API errors, not snapshot results.

`source_id` is a valid SourceId pointing to SourceRecord and must satisfy `source_id < sources.count`. There is no root-level source none sentinel. Path, locale, message_id, base_offset, and optional source text are read from SourceRecord. This keeps RootRecord as a fixed 16-byte entry point and separates source metadata expansion from roots-section random access.

`diagnostic_start` and `diagnostic_count` point to a contiguous range in the diagnostics section. Diagnostics are grouped by root order. This allows decoders to slice diagnostics from `roots[i]` in O(1).

When `include_diagnostics = false`, RootRecord layout does not change. `diagnostic_count = 0` and `diagnostic_start = 0`. The diagnostics section and diagnostic labels section may be empty or absent optional sections. Decoders must not vary RootRecord record_size by option.

### String Table

![ox-mf2 string table](./assets/003-ox-mf2-string-table.svg)

The string table stores snapshot metadata, diagnostic messages, semantic-independent small strings, normalized strings, and other strings that cannot be represented by source spans alone. Original source text is not mixed into string data; it is stored in the dedicated `source text data` section only when `include_source_text = true`.

The string offsets section and string data section are core sections and always exist, even with zero strings. In that case, string offsets can have `count = 0`, and string data can have `byte_len = 0`.

```text
StringRef {
  id: u32,
}

StringOffsetRecord {
  offset: u32,
  len: u32,
}
```

`StringRef.id` is a StringId into the string offsets section. The decoder reads `string_offsets[id]` and slices `offset..offset+len` from the string data section. `StringId` is the canonical interned string identity inside the snapshot.

`StringId = 0` is a valid string id and points to `string_offsets[0]`. If the string offsets section has `count = 0`, no valid StringId exists. Optional strings use `StringId = 0xFFFF_FFFF` as the none sentinel. Decoders must not look up the none sentinel in string offsets. A non-sentinel StringId greater than or equal to string offsets count is invalid.

Decoders materialize UTF-8 strings lazily, only when a consumer reads them.

### Source Text Data Section

![ox-mf2 source text data section](./assets/003-ox-mf2-source-text-data-section.svg)

The source text data section is an optional raw byte section. When `include_source_text = true`, each input's original MF2 source text is stored in this section as UTF-8 bytes. When `include_source_text = false`, this section is absent or empty.

v1 snapshot source text data covers only UTF-8-valid source text. If an input with unpaired surrogates must be handled for ECMAScript String compatibility, the binding/source boundary keeps it as external source text and combines it with a snapshot using `include_source_text = false`. If storing WTF-8 or UTF-16 source text in snapshots becomes necessary, it should be designed as a future optional section or format change.

```text
SourceTextRef {
  source_id: u32,
  offset: u32,
  len: u32,
}
```

`offset` and `len` are byte ranges inside the source text data section. `SourceTextRef.source_id = 0xFFFF_FFFF` is the none sentinel and means source text bytes are not included in the snapshot. In the sentinel case, decoders must not use `offset` or `len`.

Normal SourceId has no none sentinel, but SourceTextRef is an optional field and therefore has one. Source text data is separated from the string table so metadata string deduplication, diagnostic strings, normalized strings, and original source text lifetime / transfer policy can evolve independently.

### Source Section

![ox-mf2 source section](./assets/003-ox-mf2-source-section.svg)

The source section is a core section. Source records contain source identity and metadata, but not source text bytes.

SourceRecord array order is not fixed to `parse_batch` input order. The sources section may deduplicate by source identity. Multiple RootRecords may point to the same `source_id`. Root-to-source mapping is always resolved through `RootRecord.source_id`.

v1 snapshot format does not define a source dedup key. Identity rules such as `path + base_offset + source text` are not baked into the wire format. SourceId assigned by SourceStore / binding is the canonical source identity, and the snapshot encodes that mapping.

```text
SourceRecord {
  source_id: u32,
  path: StringRef,
  locale: StringRef,
  message_id: StringRef,
  base_offset: u32,
  text: SourceTextRef,
}
```

SourceId is a required index into the sources section. `SourceRecord.source_id` must match its own index in the sources section. RootRecord, TokenRecord, TriviaRecord, and DiagnosticRecord `source_id` fields have no none sentinel and must satisfy `source_id < sources.count`. `SourceId = 0` is valid.

`path`, `locale`, and `message_id` are optional metadata. They always exist as `StringRef` fields in SourceRecord, and use `StringId = 0xFFFF_FFFF` as the none sentinel when absent. SourceRecord layout does not vary by metadata presence.

`base_offset` is a UTF-8 byte offset. It is not optional; `0` is stored when unspecified. Absolute byte positions are computed as `base_offset + span_start/end`. UTF-16 code unit positions, line/column, and LSP positions are converted at the binding/editor boundary and are not stored in snapshot node fields.

When `include_source_text = false`, `text.source_id = 0xFFFF_FFFF`. SourceRecord layout does not change. Roots and diagnostics retain SourceId and Span, so the binding layer can use external source text to resolve locations or source slices.

When `include_source_text = true`, the snapshot stores source text in the dedicated source text data section. `SourceRecord.text.source_id` must equal the same record's `source_id`. In large batches, each SourceRecord has its own text range, and the roots section links source metadata to root nodes.

Source slices are resolved with SourceId plus Span. `sourceSlice(span)` refers to APIs with source context, such as `SourceView.sourceSlice(span)` or convenience accessors on node/token handles. It succeeds only when source text can be resolved from snapshot source text data or from external source text retained by the binding result. If neither is available, the decoder/accessor returns a source text unavailable error instead of silent `undefined`.

### Node Section

![ox-mf2 node section](./assets/003-ox-mf2-node-section.svg)

Snapshot node records are fixed-size as much as possible. This keeps NodeId as a direct `u32` index into the node section.

```text
NodeRecord {
  kind: u16,
  flags: u16,
  span_start: u32,
  span_end: u32,
  first_child: u32,
  child_count: u32,
  data_ref: u32,
}
```

`kind` stores the numeric value of the Phase 1 parser `SyntaxKind` directly. SnapshotWriter does not remap NodeRecord.kind through a snapshot-specific kind table. `SyntaxKind` numeric values are part of the snapshot compatibility contract: once published, values are not reordered, reused, or changed incompatibly. A decoder rejects unknown `SyntaxKind` numeric values. Emitting a new kind in a core NodeRecord / TokenRecord / TriviaRecord, or changing a kind's meaning incompatibly, requires a snapshot major version bump.

`first_child` and `child_count` represent a range in the edges section. Children are stored as an EdgeRecord array in source order, where each edge references either a node or a token.

Variable-length data or node-kind-specific data lives in the extended data section referenced by `data_ref`. The extended data section is a raw byte section with `record_size = 0`. `data_ref = 0xFFFF_FFFF` is the none sentinel and means the node has no extended data. Any non-sentinel `data_ref` must be a valid byte offset into the extended data section. NodeId / TokenId / TriviaId / SourceId have no sentinel, but `data_ref` is optional and therefore has one.

Extended data payloads always have a header.

```text
ExtendedDataHeader {
  kind: u16,
  flags: u16,
  byte_len: u32,
}
```

`data_ref` points to the first byte of ExtendedDataHeader. `byte_len` is the total payload length including the header. Decoders verify `data_ref + byte_len <= extended_data.byte_len`. `kind` is a node-kind-specific payload kind and must be compatible with NodeRecord.kind. If not, the snapshot is invalid.

### Edge Section

![ox-mf2 edge section](./assets/003-ox-mf2-edge-section.svg)

The edge section is a core section. EdgeRecord is a compact typed reference representing CST parent-child relationships.

```text
EdgeRecord {
  kind: u16,
  flags: u16,
  ref_id: u32,
}
```

`kind` is a numeric enum: `node = 0` or `token = 1`. Other values are invalid. If `kind = node`, `ref_id` is a NodeId; if `kind = token`, `ref_id` is a TokenId. The decoder/accessor reads the EdgeRecord range from NodeRecord `first_child` / `child_count` and lazily returns node views or token views according to edge kind. For `kind = node`, `ref_id < nodes.count` must hold; for `kind = token`, `ref_id < tokens.count` must hold.

Trivia is not mixed into child edges. Trivia is read from TokenRecord leading/trailing trivia ranges. This keeps syntax traversal focused on node/token children, while formatters reconstruct source-preserving output from token order and trivia ranges.

### Token and Trivia Sections

![ox-mf2 token and trivia sections](./assets/003-ox-mf2-token-trivia-sections.svg)

Tokens and trivia have dedicated snapshot sections. TokenId and TriviaId are `u32` indexes into their respective sections.

```text
TokenRecord {
  kind: u16,
  flags: u16,
  span_start: u32,
  span_end: u32,
  source_id: u32,
  leading_trivia_start: u32,
  leading_trivia_count: u32,
  trailing_trivia_start: u32,
  trailing_trivia_count: u32,
}

TriviaRecord {
  kind: u16,
  flags: u16,
  span_start: u32,
  span_end: u32,
  source_id: u32,
}
```

TokenRecord.kind and TriviaRecord.kind also store the Phase 1 `SyntaxKind` numeric value directly. Nodes, tokens, and trivia share the same `SyntaxKind` family, while decoders/accessors interpret node kind, token kind, and trivia kind using section context and helper predicates. Decoders reject unknown `SyntaxKind` numeric values for TokenRecord.kind and TriviaRecord.kind too. This keeps kind identity identical across parser tables, snapshot records, and binding accessors, and removes the need for a kind conversion table during snapshot encoding.

The internal Phase 1 `TokenRecord` may use a compact layout with `first_trivia` and leading/trailing counts to reduce record size. SnapshotWriter expands it linearly into snapshot `leading_trivia_start` / `leading_trivia_count` / `trailing_trivia_start` / `trailing_trivia_count`. The snapshot format does not need to be byte-identical to construction-time layout, but the conversion should require only per-token arithmetic.

Formatters, especially preserve mode, use token and trivia sections to reconstruct source faithfully without relying on a nested object tree.

When `include_trivia = false`, TokenRecord layout does not change. `leading_trivia_count` and `trailing_trivia_count` are `0`, and `leading_trivia_start` and `trailing_trivia_start` are also `0`. The trivia section may be empty or absent as an optional section. Decoders must not vary TokenRecord record_size by option.

v1 TokenRecord / TriviaRecord has no `text_ref`. Original token / trivia text is referenced by `source_id + span_start/span_end`. With `include_source_text = false`, external source text is used. With `include_source_text = true`, `SourceRecord.text` points to the source text data section. If normalized text, cooked text, or debug text becomes necessary and cannot be represented only by source spans, add extended data or an optional token-text section instead of growing TokenRecord / TriviaRecord.

### Diagnostics Section

![ox-mf2 diagnostics section](./assets/003-ox-mf2-diagnostics-section.svg)

When `include_diagnostics = true`, the snapshot includes a diagnostics section. This allows snapshots to be inspected or transported with parse diagnostics attached.

```text
DiagnosticRecord {
  source_id: u32,
  span_start: u32,
  span_end: u32,
  severity: u8,
  code: u16,
  message: StringRef,
  label_start: u32,
  label_count: u32,
}
```

`severity` and `code` are compact numeric enums. `message` is an indexed StringRef. If extension/custom diagnostics need human-readable string codes, add a future optional diagnostic-code string section rather than growing DiagnosticRecord.

Diagnostic records are grouped by root order so RootRecord `diagnostic_start` / `diagnostic_count` can reference them. Each DiagnosticRecord has SourceId and Span, so it can still represent diagnostics from multiple sources associated with the same root. The RootRecord range is the source of truth for root-local diagnostics in the snapshot.

Diagnostic labels live in a separate diagnostic labels section. DiagnosticRecord `label_start` / `label_count` points to a contiguous range in the DiagnosticLabelRecord array.

```text
DiagnosticLabelRecord {
  source_id: u32,
  span_start: u32,
  span_end: u32,
  message: StringRef,
}
```

Diagnostics without labels use `label_count = 0`. Decoders verify `label_start + label_count <= diagnostic_labels.count`. Help text is not included in the v1 snapshot record; add it later as an optional diagnostic-help section when needed.

Public APIs may return diagnostics separately for convenience, but the snapshot format must be able to keep diagnostics as part of the encoded result. A flat diagnostics array in a binding result is a view over snapshot diagnostic records in root order, not a separate diagnostic table.

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

Linters, compilers, and validators can combine Binary AST decoder/accessor traversal with SemanticView.

## Decoder Error Boundary

Snapshot decode fails with typed errors, not panics.

Rust decoder API:

```rust
decode_snapshot(bytes: &[u8]) -> Result<SnapshotView<'_>, DecodeError>
decode_snapshot_owned(bytes: Arc<[u8]>) -> Result<SnapshotViewOwned, DecodeError>
```

`DecodeError` covers invalid snapshot, unsupported version, missing required section, invalid section layout, invalid index, unknown required section, unknown `SyntaxKind`, invalid UTF-8 string, out-of-range source text, invalid extended data, diagnostic range mismatch, and similar failures.

Rust APIs return invalid snapshots as `Result::Err(DecodeError)`. Decoder/accessors may handle untrusted bytes, so validation failures must not panic. Internal invariant violations may use debug assertions, but public decode boundaries return recoverable errors.

N-API and WASM bindings convert Rust `DecodeError` into their language boundaries.

- N-API: convert `DecodeError` into a JS exception or explicit `Result` object.
- WASM: convert `DecodeError` into a thrown JS error or exported API error result.

The binding boundary should preserve error code, message, optional section kind, optional offset, and optional record index. Human-readable messages may be returned for developer ergonomics, but compact error codes are the base for programmatic handling.

## Snapshot Buffer Ownership

The Rust decoder provides both borrowed and owned views.

```rust
pub struct SnapshotView<'a> {
  bytes: &'a [u8],
  sections: SectionIndex,
}

pub struct SnapshotViewOwned {
  bytes: Arc<[u8]>,
  sections: SectionIndex,
}
```

`SnapshotView<'a>` requires the caller to manage the lifetime of snapshot bytes. Decode builds only the section table and validation metadata; it does not eagerly materialize nodes, tokens, or strings. This is used by Rust internal tests, benchmarks, temporary decode, and zero-copy inspection.

`SnapshotViewOwned` owns snapshot bytes as `Arc<[u8]>`. Long-lived caches, daemons, LSP, binding objects, and worker handoff use owned views when accessors may outlive the original buffer owner.

N-API and WASM bindings generally hold owned/shared buffers. Node / Token / Trivia / Root handles keep references to the snapshot view object or shared buffer owner, not raw pointers. This avoids dangling views after JS GC, WASM object lifetime changes, or worker transfer.

Bindings do not expand snapshot bytes into a JS object tree. Accessors slice the snapshot buffer and return values only when JS/WASM consumers read nodes, tokens, strings, or diagnostics.

## Handle Model

Public Root / Node / Token / Trivia handles are pairs of snapshot owner and section-local id, not object pointers.

```text
RootHandle   { snapshot: SnapshotRef, id: RootId }
NodeHandle   { snapshot: SnapshotRef, id: NodeId }
TokenHandle  { snapshot: SnapshotRef, id: TokenId }
TriviaHandle { snapshot: SnapshotRef, id: TriviaId }
```

In Rust, `SnapshotRef` is a reference to `SnapshotView` / `SnapshotViewOwned` or an owned view. In N-API / WASM, it is a reference to an accessor object that owns the snapshot buffer. The handle itself does not copy snapshot bytes.

RootId, NodeId, TokenId, and TriviaId are snapshot-local identities. Equal id values from different snapshots do not mean the same node/token/trivia. Handle equality is based on both snapshot identity and id.

Handle construction validates `id < section.count`. Children traversal, root lookup, and token trivia lookup return lightweight handles with the same `SnapshotRef`. This lets lazy accessors avoid materializing object trees and avoids dangling pointers in GC-managed languages.

Rust low-level APIs may also provide accessors that take `SnapshotView` and raw ids separately for performance-sensitive paths. However, the public binding API standardizes the `{ snapshot, id }` handle model.

## Accessor Traversal API

The N-API / WASM public traversal API uses array-like snapshot views as the primary representation, not iterators.

```ts
type ChildHandle = NodeHandle | TokenHandle

root.node(): NodeHandle
root.diagnostics(): DiagnosticView[]

node.kind(): SyntaxKind
node.span(): Span
node.childCount(): number
node.childAt(index: number): ChildHandle
node.children(): ChildHandle[]

token.kind(): SyntaxKind
token.span(): Span
token.leadingTrivia(): TriviaHandle[]
token.trailingTrivia(): TriviaHandle[]

trivia.kind(): SyntaxKind
trivia.span(): Span
```

`node.children()` returns a lightweight child handle array in source order. It does not materialize a subtree; each element is a `{ snapshot, id }` handle. MF2 messages are smaller than general source files, so the default API favors ergonomics.

Allocation-sensitive paths use `node.childCount()` and `node.childAt(index)` to avoid allocating child handle arrays. Rust low-level APIs may provide allocation-free accessors that take `SnapshotView` plus raw id/index.

If an out-of-range index is passed to an indexed accessor, the binding returns an explicit error instead of silent `undefined`. N-API converts this to `RangeError`; WASM converts it to an exported API error result or thrown JS error. Invalid accessor usage fails loudly so formatter/linter traversal bugs are found early.

JS iterators may be added as convenience APIs, but they are not the primary compatibility surface. The standard binding traversal contract is defined by array-like methods and indexed accessors.

## Bindings

![ox-mf2 language bindings](./assets/003-ox-mf2-language-bindings.svg)

Binding implementation priority:

1. N-API binding: the primary Node.js target for intlify and JavaScript tooling integration
2. WASM binding: portable target for browsers, playgrounds, editor extensions, and edge runtime integration
3. C ABI binding design: foundation for future Go, Swift, C#, Zig, Python FFI, and broader native language integration

Phase 2 does not require a stable C ABI implementation. C ABI remains design preparation. However, snapshot record layout, numeric error codes, handle ids, buffer ownership, `toBytes()` copy semantics, and decode error boundary stay C-ABI-friendly. Future C ABI can share the same snapshot decoder/accessor model so N-API / WASM do not carry different semantic implementations from the Rust core.

N-API and WASM bindings return result objects with lazy decoder/accessors instead of eagerly materialized JS object trees. The default public API does not return raw snapshot bytes directly. Snapshot bytes are retained as an internal buffer in the result/accessor object.

Bindings may retain original source text on the caller side or in the binding result. Since the default snapshot uses `include_source_text = false`, context-bound `sourceSlice(span)` reads external source text, not snapshot bytes, when provided by the decoder/accessor.

If the binding result keeps external source text, context-bound `sourceSlice(span)` may succeed even with `include_source_text = false`. If the binding result does not keep source text and the snapshot has no source text data, `sourceSlice(span)` returns a source text unavailable error.

Single-message binding shape:

```ts
type ParseInputObject = {
  source: string
  path?: string
  locale?: string
  messageId?: string
  baseOffset?: number
}

const result = parseMessage(source)
const input: ParseInputObject = { source, locale: 'en', messageId: 'hello' }
const resultWithMetadata = parseMessage(input)

result.diagnostics
result.source
result.root
result.snapshot
```

`parseMessage(source)` is a simple convenience overload. `parseMessage({ source, path?, locale?, messageId?, baseOffset? })` is the standard object form for SourceRecord metadata and uses the same metadata handling as batch input even for single input.

Batch binding shape:

```ts
const result = parseBatch(items: ParseInputObject[])

result.sources
result.roots
result.diagnostics
result.snapshot
```

`parseBatch(items)` takes `{ source, path?, locale?, messageId?, baseOffset? }[]` as standard input. Only `source` is required and determines parser semantics. `path`, `locale`, `messageId`, and `baseOffset` are optional metadata for SourceRecord metadata, diagnostics mapping, LSP/editor mapping, benchmarking/reporting, and root mapping.

Bindings map `messageId` to snapshot/Rust `message_id`, and `baseOffset` to `base_offset`. `baseOffset` is a UTF-8 byte offset and defaults to `0`. JavaScript string UTF-16 position conversion is a binding/editor boundary responsibility and is not stored in snapshot node fields.

`result.roots[i]` always corresponds to `items[i]`. The sources section may deduplicate by source identity, so multiple roots may point to the same SourceRecord, but batch result root order does not change from input order.

`result.source` and `result.sources` are SourceRecord-backed SourceViews, not raw source strings. Use `sourceSlice(span)` or SourceView accessors to read source text. In batch results, `result.sources` order is SourceId order, not input order, because the sources section may be deduplicated.

`result.snapshot` is an accessor object, not raw bytes. Root, node, token, trivia, diagnostic, and source metadata are read lazily through accessors. Raw snapshot bytes are not included in the default result shape.

Advanced use cases such as debug, fixtures, persistence, worker transfer, and external process transport can explicitly extract snapshot bytes.

```ts
const bytes = result.snapshot.toBytes()
```

`toBytes()` returns a copy of the internal snapshot buffer. The binding does not expose the internal buffer directly. This prevents consumers from retaining, mutating, or transferring returned bytes in a way that breaks the lifetime or validation invariants of existing accessor objects.

Batch parsing keeps one shared snapshot buffer internally and returns root handles, diagnostics, and a snapshot accessor for each input item. Nodes and strings are materialized only when consumers read them.

`result.diagnostics` is a flat array in root order. In a single-message result, it contains diagnostics for one root. In a batch result, it contains all diagnostics grouped by `result.roots` order. Each root handle can also expose its own diagnostics range through a lazy accessor such as `root.diagnostics()`.

The flat diagnostics array and root-local diagnostics accessor read the same snapshot diagnostics section. Bindings do not duplicate diagnostics into another table. For JS/WASM ergonomics, lightweight diagnostic view objects may be materialized when consumers read `result.diagnostics`.

With `include_source_text = false` in a batch result, each root has a `source_id`, source metadata is in SourceRecord, and input source text references are retained by the binding result. With `include_source_text = true`, source text data is included in the snapshot so the snapshot alone can resolve source slices after worker transfer or persistence.

## Formatter and Linter Input

From Phase 2 onward, public AST input for formatter and linter is the Binary AST decoder/accessor view. Rust implementations may use construction-time tables or semantic model internal fast paths when needed, but the stable public traversal model is aligned with the Binary AST view shared by Rust, N-API, and WASM consumers.

## MessagePack Transport

MessagePack is not the CST/AST representation of ox-mf2.

It is a future transport candidate for long-lived language-service workflows such as LSP, editor integration, daemon mode, and repeated semantic queries. The standard CST/AST product boundary remains the versioned Binary AST snapshot.

If MessagePack transport is added later, its overhead must be measured separately from parser, semantic lowering, snapshot encoding, and binding costs.

## Snapshot Test Strategy

Phase 2 snapshot tests use both binary golden fixtures and decoded structural fixtures.

Binary golden fixtures:

- validate snapshot header, section table, record layout, alignment, endianness, and required/optional section flags
- detect accidental changes to the versioned wire format
- include option combinations for `include_source_text`, `include_trivia`, and `include_diagnostics`
- include invalid snapshot fixtures and verify decoders reject them with `DecodeError`, not panic

Decoded structural fixtures:

- keep decoded roots, sources, nodes, edges, tokens, trivia, diagnostics, labels, and strings as reviewable text or JSON snapshots
- separate byte-level binary layout changes from semantic structure changes during review
- check root order, source deduplication, diagnostic grouping, token/trivia spans, and `SyntaxKind` numeric values
- detect CST/diagnostics regressions that are hard to review from exact binary bytes alone

Binary golden fixtures are contract tests for wire compatibility. Decoded structural fixtures are review aids that keep implementation behavior explainable. When intentionally changing the snapshot format version, update binary fixtures, decoded fixtures, and the format changelog in the same change.

## Benchmarks

Snapshot and binding work must not be hidden inside one parser number.

Relevant benchmark phases:

- lexer
- parse_cst
- lower_semantic
- diagnostics
- encode_snapshot
- decode_snapshot
- snapshot_to_bytes_copy
- binding_call
- parse_batch
- parse_batch_to_snapshot
- lsp_jsonrpc
- lsp_msgpack

Phase 2 benchmarks measure parser hot path, snapshot encoding, snapshot decoding, and binding/export cost separately.

- `parse_cst`: cost for the Rust parser to build CstTables.
- `encode_snapshot`: cost to build Binary AST snapshot bytes from existing CstTables / diagnostics / source metadata.
- `decode_snapshot`: cost to validate snapshot bytes and build lazy SnapshotView / section index. It does not include node/token/string traversal.
- `snapshot_accessor_traversal`: cost to read roots, nodes, tokens, trivia, and diagnostics lazily from a decoded SnapshotView.
- `snapshot_to_bytes_copy`: cost for `result.snapshot.toBytes()` to copy the internal buffer and return external bytes.
- `binding_call`: cost to cross N-API / WASM boundary and return result object plus handles. This is measured separately from parse / encode / decode.
- `parse_batch_to_snapshot`: combined product path for batch parse plus shared snapshot encoding. Do not mix it with the single-message parser baseline.

Benchmark reports include at least separate series for `parse_cst`, `parse_cst + encode_snapshot`, `decode_snapshot`, `snapshot_accessor_traversal`, and `snapshot_to_bytes_copy`. For single-message comparison with external parsers, the primary baseline is `parse_message` equivalent, meaning `parse_cst` without snapshot encoding or binding materialization. Snapshot/binding numbers are reported separately as ox-mf2 product-boundary cost.
