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

The IR is an internal implementation detail of `intlify_format`.

The public formatter API accepts source text or a `SnapshotView`, then returns formatted text or diagnostics/errors. Callers do not construct or inspect formatter IR nodes.

The intended pipeline is:

```text
source text
  -> parser / SnapshotView
  -> formatter syntax traversal
  -> formatter IR
  -> renderer
  -> formatted source text
```

The IR should carry enough structure for rendering decisions without duplicating the full parser snapshot. Source-sensitive decisions, such as preserve-mode blank-line grouping, should be derived from `SnapshotView` spans and trivia before or during IR construction.

## Initial Concepts

The exact node set is still open, but the IR is expected to include concepts similar to:

- text fragments
- spaces
- hard lines
- optional or mode-driven line breaks
- groups
- indentation
- aligned matcher columns
- raw source slices for whitespace-sensitive pattern text

The formatter should make a clear distinction between semantic text that must be preserved exactly and syntax text that can be normalized.

## Open Questions

- What is the minimal IR node set needed for Phase 3B standard and preserve formatting?
- Does preserve mode need explicit "source shape" annotations in the IR, or should shape decisions happen before IR construction?
- How should matcher table alignment be represented: a dedicated IR node, precomputed column widths, or normal grouped layout primitives?
- Should line wrapping be modeled now as dormant IR capability, or added only when `lineWidth` is introduced?
- How should raw source slices be represented to avoid copying while still keeping rendering deterministic?
- What fixture or snapshot tests should validate IR construction separately from final rendered text?
- Which benchmark stages should measure syntax traversal, IR construction, matcher alignment, and rendering separately?
