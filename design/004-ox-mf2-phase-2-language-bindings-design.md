# ox-mf2 Phase 2 Language Bindings Design

## Purpose

This document defines the Phase 2 language binding boundary for ox-mf2.

Binary AST snapshot wire format, snapshot-producing APIs, decoder/accessor APIs, and snapshot ownership are defined in [003-ox-mf2-phase-2-binary-ast-snapshot-design.md](./003-ox-mf2-phase-2-binary-ast-snapshot-design.md). This document focuses on how N-API, WASM, and future bindings expose that snapshot-backed model without reimplementing MF2 semantics outside Rust.

## Basic Policy

ox-mf2 uses the Rust core as the single semantic implementation. Bindings are ergonomic wrappers around the Rust parser, snapshot writer, decoder/accessor view, diagnostics, and later formatter/linter APIs.

Bindings must not expose a nested JSON AST as the standard hot-path output. The standard public CST/AST view is a lazy accessor over the versioned Binary AST snapshot. Debug JSON may exist, but it is not the compatibility boundary.

## Binding Targets

Binding implementation priority:

1. N-API binding: the primary Node.js target for intlify and JavaScript tooling integration
2. WASM binding: portable target for browsers, playgrounds, editor extensions, and edge runtime integration
3. C ABI binding design: foundation for future Go, Swift, C#, Zig, Python FFI, and broader native language integration

Phase 2 does not require a stable C ABI implementation. C ABI remains design preparation. However, snapshot record layout, numeric error codes, handle ids, buffer ownership, `toBytes()` copy semantics, and decode error boundary stay C-ABI-friendly.

## Result Object Boundary

N-API and WASM bindings return result objects with lazy decoder/accessors instead of eagerly materialized JS object trees. The default public API does not return raw snapshot bytes directly. Snapshot bytes are retained as an internal buffer in the result/accessor object.

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

Bindings map `messageId` to Rust/snapshot `message_id`, and `baseOffset` to `base_offset`. `baseOffset` is a UTF-8 byte offset and defaults to `0`. JavaScript string UTF-16 position conversion is a binding/editor boundary responsibility and is not stored in snapshot node fields.

`result.roots[i]` always corresponds to `items[i]`. The sources section may deduplicate by source identity, so multiple roots may point to the same SourceRecord, but batch result root order does not change from input order.

## Snapshot Accessor Boundary

`result.snapshot` is an accessor object, not raw bytes. Root, node, token, trivia, diagnostic, and source metadata are read lazily through accessors. Raw snapshot bytes are not included in the default result shape.

Public Root / Node / Token / Trivia handles are pairs of snapshot owner and section-local id, not object pointers.

```text
RootHandle   { snapshot: SnapshotRef, id: RootId }
NodeHandle   { snapshot: SnapshotRef, id: NodeId }
TokenHandle  { snapshot: SnapshotRef, id: TokenId }
TriviaHandle { snapshot: SnapshotRef, id: TriviaId }
```

N-API and WASM handles keep references to the accessor object that owns the snapshot buffer. Handles do not copy snapshot bytes and do not point directly into Rust memory in a way that can dangle after JS GC, WASM lifetime changes, or worker transfer.

The binding traversal API uses array-like methods and indexed accessors as the compatibility surface.

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

`node.children()` returns a lightweight child handle array in source order. It does not materialize a subtree. Allocation-sensitive consumers use `node.childCount()` and `node.childAt(index)`.

If an out-of-range index is passed to an indexed accessor, N-API converts it to `RangeError`; WASM converts it to an exported API error result or thrown JS error. Invalid accessor usage fails loudly so formatter/linter traversal bugs are found early.

JS iterators may be added as convenience APIs, but they are not the primary compatibility surface.

## Source Text Retention

Bindings may retain original source text on the caller side or in the binding result. Since the default snapshot uses `include_source_text = false`, context-bound `sourceSlice(span)` reads external source text, not snapshot bytes, when provided by the decoder/accessor.

If the binding result keeps external source text, context-bound `sourceSlice(span)` may succeed even with `include_source_text = false`. If the binding result does not keep source text and the snapshot has no source text data, `sourceSlice(span)` returns a source text unavailable error.

`result.source` and `result.sources` are SourceRecord-backed SourceViews, not raw source strings. Use `sourceSlice(span)` or SourceView accessors to read source text. In batch results, `result.sources` order is SourceId order, not input order, because the sources section may be deduplicated.

With `include_source_text = false` in a batch result, each root has a `source_id`, source metadata is in SourceRecord, and input source text references are retained by the binding result. With `include_source_text = true`, source text data is included in the snapshot so the snapshot alone can resolve source slices after worker transfer or persistence.

## Raw Bytes Export

Advanced use cases such as debug, fixtures, persistence, worker transfer, and external process transport can explicitly extract snapshot bytes.

```ts
const bytes = result.snapshot.toBytes()
```

`toBytes()` returns a copy of the internal snapshot buffer. The binding does not expose the internal buffer directly. This prevents consumers from retaining, mutating, or transferring returned bytes in a way that breaks the lifetime or validation invariants of existing accessor objects.

## Diagnostics

`result.diagnostics` is a flat array in root order. In a single-message result, it contains diagnostics for one root. In a batch result, it contains all diagnostics grouped by `result.roots` order. Each root handle can also expose its own diagnostics range through a lazy accessor such as `root.diagnostics()`.

The flat diagnostics array and root-local diagnostics accessor read the same snapshot diagnostics section. Bindings do not duplicate diagnostics into another table. For JS/WASM ergonomics, lightweight diagnostic view objects may be materialized when consumers read `result.diagnostics`.

## Error Mapping

N-API and WASM bindings convert Rust `DecodeError` and parser/snapshot errors into their language boundaries.

- N-API: convert errors into JS exceptions or explicit `Result` objects.
- WASM: convert errors into thrown JS errors or exported API error results.

The binding boundary preserves error code, message, optional section kind, optional offset, and optional record index. Human-readable messages may be returned for developer ergonomics, but compact error codes are the base for programmatic handling.

## Benchmarks

Binding work must not be hidden inside parser or snapshot numbers.

Relevant binding benchmark phases:

- binding_call
- snapshot_to_bytes_copy
- parse_message_binding
- parse_batch_binding

`binding_call` measures the cost to cross N-API / WASM boundary and return result object plus handles. It is measured separately from parse, encode, decode, and snapshot accessor traversal.

`snapshot_to_bytes_copy` is reported both from the snapshot perspective and binding perspective. The binding report includes language boundary allocation and copy cost.
