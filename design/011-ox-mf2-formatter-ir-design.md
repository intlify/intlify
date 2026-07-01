# ox-mf2 Formatter IR Design

## Purpose

This document tracks the detailed design for the internal formatter IR used by `intlify_format`.

The Phase 3B formatter product design is defined in [007-ox-mf2-phase-3b-formatter-design.md](./007-ox-mf2-phase-3b-formatter-design.md). That document fixes formatter modes, public APIs, CLI behavior, fixtures, diagnostics policy, and SnapshotView requirements. This document focuses only on the internal layout/document model that turns parsed MF2 syntax into formatted text.

## Goals

The formatter IR should provide a stable implementation boundary between syntax traversal and text rendering.

Primary goals:

- avoid direct string concatenation during `SnapshotView` traversal
- represent formatter output as a structured document or layout tree before rendering
- support standard and preserve formatting modes through one formatter pipeline
- keep line, group, and indent decisions explicit enough for future line wrapping
- preserve semantically significant pattern text and literal spelling
- allow resource/catalog adapters to reuse message-level formatting later
- make formatting behavior testable independently from CLI, N-API, and WASM bindings
- keep benchmark stages separable for traversal, IR construction, and rendering

## Non-Goals

Initial non-goals:

- public AST replacement or a second public syntax format
- exposing the formatter IR through N-API or WASM
- range-only formatting
- minimal-diff edit generation
- JSON/YAML/resource host-file edit modeling
- finalizing line wrapping behavior
- implementing formatter ignore directives
- preserving parser recovery output with diagnostics

Range-only and minimal-diff editing remain LSP/editor integration concerns. Resource/catalog host-file edits remain adapter concerns above the message-level formatter.

## Design Boundary

![Formatter IR design boundary](./assets/011-ox-mf2-formatter-ir-boundary.svg)

The IR is an internal implementation detail of `intlify_format`.

The public formatter API accepts source text or a `SnapshotView`, then returns formatted text or diagnostics/errors. Callers do not construct or inspect formatter IR nodes.

The intended pipeline is:

```text
source text
  -> parser / SnapshotView
  -> formatter syntax traversal
  -> MF2 Layout IR construction
  -> MF2 Layout IR normalize pass
  -> Document IR lowering
  -> Document IR rendering
  -> formatted source text
```

The formatter uses a two-layer IR:

1. **MF2 Layout IR**: an ox-mf2-specific layout model that captures message structure and formatter intent.
2. **Document IR**: a small Prettier-style document model that captures text rendering primitives.

The MF2 Layout IR carries enough structure for MF2-specific rendering decisions without duplicating the full parser snapshot. Source-sensitive decisions, such as preserve-mode source shape and blank-line grouping, are derived from `SnapshotView` spans and trivia during MF2 Layout IR construction.

The Document IR is independent of MF2 semantics. It should not know about matcher tables, declarations, selectors, or pattern semantics. Its job is to render an already-decided document layout deterministically.

## MF2 Layout IR

![MF2 Layout IR structure](./assets/011-ox-mf2-mf2-layout-ir.svg)

The MF2 Layout IR is a mixed model:

- it is syntax-oriented for ordinary message structure, such as messages, declarations, expressions, patterns, markup, options, attributes, and literals
- it uses dedicated layout nodes where formatter decisions are MF2-specific, such as matcher tables, pattern chunks, and preserve-mode grouping

The initial model should include a dedicated matcher table layout node. The matcher table node owns row data and computed column widths. Document IR and the renderer must not know MF2 matcher semantics.

Preserve mode records source-shape metadata on major syntax/layout nodes:

```text
ShapeHint = Flat | Break | Unknown
blank_lines_before = 0 | 1
```

`ShapeHint` is stored on major nodes such as message, declaration block, expression, matcher table, pattern, and markup. Phase 3B does not store shape hints at every token pair or delimiter.

`Flat` means the source shape should be treated as single-line where possible. `Break` means the source shape should be treated as multi-line. `Unknown` is used when standard mode is active, when source shape is not meaningful, or when shape cannot be recovered.

`blank_lines_before` records whether a major node had a blank-line gap before it. Multiple blank lines are normalized to one blank line. This preserves grouping intent without letting arbitrary vertical spacing leak into formatter output.

## Normalize Pass

After MF2 Layout IR construction, a normalize pass prepares data that needs whole-structure knowledge.

Phase 3B normalize work includes:

- matcher table column width calculation
- `blank_lines_before` normalization to `0` or `1`

`Group(flat|break)` decisions are made during MF2 Layout IR construction from formatter mode and shape hints. The normalize pass should not become the place where all formatting decisions are delayed.

## Document IR

![Document IR structure](./assets/011-ox-mf2-document-ir.svg)

The initial Document IR uses a minimal document model with dormant wrapping hooks:

- `Text`
- `SourceSlice`
- `Space`
- `HardLine`
- `SoftLine`
- `Concat`
- `Indent`
- `Group`

`Group` has a fixed mode in Phase 3B:

```text
GroupMode = Flat | Break
```

Phase 3B does not use `lineWidth`, so rendering is deterministic:

- `SoftLine` inside `Group(Flat)` renders as a space
- `SoftLine` inside `Group(Break)` renders as a newline

This keeps line-breaking intent explicit without implementing width-based wrapping in the first formatter.

`Text` and `SourceSlice` are separate:

- `Text` is generated formatter syntax, such as `.input`, spaces, braces, separators, and normalized punctuation.
- `SourceSlice(span)` is verified source text copied from the original input, such as whitespace-sensitive pattern text, literal spelling, or escape spelling.

`SourceSlice(span)` may use token spans or formatter-computed contiguous spans. Formatter-computed spans must be derived from verified token/source ranges. Snapshot/source consistency and span boundaries are checked during IR construction, before rendering.

The renderer returns errors. IR invariant violations, invalid unverified source slices, or source access failures are converted to formatter operational errors such as `internal_error`; they must not leak as public API panics.

## Tests and Dumps

Final formatted output remains the primary formatter fixture assertion.

Selective stable text dumps are also used where intermediate IR behavior is important. These dumps should not use Rust `Debug` output. They should use a stable formatter-owned text representation that includes only reviewable fields such as node kind, `shape_hint`, `blank_lines_before`, spans, matcher rows, and matcher column widths.

Phase 3B stable dumps cover:

- MF2 Layout IR before normalization
- MF2 Layout IR after normalization

Document IR dumps are not required initially. Final output fixtures validate Document IR lowering and rendering unless a future bug or feature makes Document IR dumps necessary.

Initial selective dump fixtures should focus on:

- matcher table normalization
- preserve-mode `shape_hint`
- `blank_lines_before` normalization
- `SourceSlice` construction

## Benchmarks

Formatter IR benchmark stages should align with the pipeline:

- `SnapshotView` traversal
- MF2 Layout IR construction
- MF2 Layout IR normalize pass
- Document IR lowering
- Document IR rendering

These stages supplement the formatter benchmark categories in [007-ox-mf2-phase-3b-formatter-design.md](./007-ox-mf2-phase-3b-formatter-design.md). They should make traversal, IR construction, normalization, lowering, and rendering costs observable separately without adding CI timing thresholds.

## Open Questions

- What is the minimal IR node set needed for Phase 3B standard and preserve formatting?
- What stable dump syntax should be used for MF2 Layout IR fixture files?
- Which exact invariant checks should return `internal_error` versus fixture-authoring failures in tests?
