# ox-mf2 Phase 3D LSP and Editor Design

This document tracks the detailed LSP and editor integration design for ox-mf2.

The Phase 3 tooling boundary is defined in [005-ox-mf2-phase-3-tooling-transport-design.md](./005-ox-mf2-phase-3-tooling-transport-design.md). That document fixes the high-level adapter workflow. This document is the implementation-facing place to refine document mapping, diagnostic publication, formatting edits, configuration reload behavior, and future editor features.

## Goals

- Reuse the parser, formatter, linter, `SnapshotView`, and `SemanticView` without making the core crates depend on LSP protocol types.
- Support diagnostics and formatting for standalone `.mf2` files.
- Support diagnostics and formatting for MF2 messages embedded in JSON/YAML resource or catalog files through adapter-owned extraction and mapping.
- Convert core UTF-8 byte spans into editor-facing UTF-16 positions at the adapter boundary.
- Keep LSP/editor artifact caches, document version checks, and host-file edit ownership outside of the parser, formatter, and linter cores.

## Non-Goals

- Making an LSP server or editor extension a direct Phase 3 product.
- Adding LSP protocol types to parser, formatter, or linter result objects.
- Implementing code actions, quick fixes, hover, completion, go-to-definition, or rename in the initial workflow.
- Implementing true range-only formatting or minimal-diff formatting in the formatter core.
- Defining a plugin system for editor integrations.

## Initial Workflow

The initial editor workflow focuses on diagnostics and formatting.

Adapters extract MF2 message text from the host document, call core APIs, and map message-local results back to document-level ranges. The core APIs operate on message-local UTF-8 byte spans. Editor adapters own document URI handling, document version checks, UTF-16 position conversion, and protocol-specific result shapes.

## Document Mapping

Editor adapters should treat standalone `.mf2` files and JSON/YAML resource files as different host document shapes over the same message-level core.

For standalone `.mf2` files, the document maps directly to one MF2 message. For JSON/YAML resource files, the adapter extracts embedded MF2 message text from the relevant resource or catalog entry and keeps enough mapping data to translate message-local results back to the host document.

Host document parsing, string escaping, decoded-to-raw offset mapping, and outer document edits remain adapter responsibilities. The parser, formatter, and linter cores receive extracted message text and return message-local results.

## Diagnostics Publication

Editor diagnostics should be produced from the shared diagnostic result contract used by parser, semantic, and linter workflows.

The preferred initial path is to use source-backed `lintMessage` as the diagnostic source for editor publication because it already includes parser, semantic, and lint diagnostics. A future `lintSnapshot` path is an optimization for parse-artifact reuse after the parser owns a snapshot-to-`SemanticModel` path. If an adapter composes diagnostics manually from cached parser, semantic, and linter results, it must avoid publishing duplicate parser diagnostics.

The initial editor workflow follows the same strict pipeline as CLI and bindings:

- parser diagnostics are always reported
- semantic diagnostics are reported only when parsing has no parser diagnostics and semantic validation runs
- lint diagnostics are reported only when parser and semantic diagnostics are clean
- parser diagnostics prevent semantic validation and linter rule execution

Core `"error"` and `"warn"` severities map to editor/LSP diagnostic severity at the adapter boundary; adapters convert `"warn"` to the editor's warning severity. Editor layers may add protocol-specific advice later, but the core linter does not emit `info` or `hint` diagnostics initially.

## Formatting Edits

Formatter core APIs return formatted message text, not LSP `TextEdit` objects.

Editor adapters should find the containing MF2 message or resource entry, call whole-message formatting, and create editor edits at the adapter boundary. For standalone `.mf2` files, the edit may replace the whole document. For JSON/YAML resources, the edit should target the containing message value range.

Adapters should only return edits when the document version and message mapping used to create the edit still match the current document. If mapping is stale or the containing message can no longer be identified, the adapter should no-op.

True range-only formatting and minimal-diff formatting are deferred. A selection inside an MF2 message should initially format the containing message rather than requiring range-local formatting from the formatter core.

## Configuration Sources

Editor adapters should normalize project configuration and editor-specific settings into the same resolved formatter and linter configuration models used by CLI workflows.

Possible editor-specific sources include workspace settings, user settings, and LSP initialization options. The exact source list, precedence, reload behavior, and failure presentation are still open.

Configuration loading failures are operational editor errors. They should not be mixed into parser, semantic, formatter, or linter diagnostics.

## Artifact Cache and Invalidation

Editor adapters may cache source views, decoded snapshots, semantic views, and diagnostics per document version.

Cached artifacts must be invalidated when the document changes. Configuration changes may also invalidate formatter, linter, semantic, or diagnostic artifacts depending on which options changed.

Detailed cache ownership, eviction, and invalidation policy belongs to the LSP/editor implementation design and the parse artifact cache design.

## Future Editor Features

The initial workflow does not require code actions, quick fixes, hover, completion, go-to-definition, rename, true range-only formatting, or minimal-diff formatting.

Future editor features should build on stable core concepts rather than adding LSP-specific state to parser, formatter, or linter crates. Quick fixes can use stable linter rule ids and formatter output. Semantic features should build on `SemanticView`.

## Open Questions

- What exact mapping data structure should represent a standalone `.mf2` document versus a JSON/YAML resource entry?
- For JSON/YAML resources, how should adapters model raw host value ranges, decoded MF2 message text, decoded-to-raw offset mapping, and string re-escaping?
- Should a shared resource adapter crate/package own JSON/YAML parsing and mapping, or should each editor integration implement that layer independently?
- Should editor diagnostics always use `lintMessage` as the initial single diagnostic source, or should adapters be allowed to compose parser, semantic, and linter diagnostics manually when they can guarantee de-duplication?
- Once snapshot-to-`SemanticModel` exists, when should editor adapters switch from source-backed `lintMessage` to future `lintSnapshot` for parse-artifact reuse?
- Should a future recovery-aware editor mode provide partial semantic or lint diagnostics for incomplete buffers, and how would it avoid conflicting with the strict CLI and binding pipeline?
- What stable key should identify diagnostics across document updates: source span, diagnostic code/rule id, resource key, or a combined identity?
- What exact document version checks are required before returning formatting `TextEdit` values?
- When a format request uses stale parse artifacts or stale message mapping, should the adapter silently no-op or report an operational editor error?
- Should editor formatting initially replace the whole containing message range, or should the adapter compute the smallest practical `TextEdit` even though minimal-diff formatting is outside the formatter core?
- Which configuration sources should editor adapters support, such as project config, VS Code workspace settings, user settings, and LSP initialization options?
- What precedence should apply when project config and editor-specific settings provide overlapping formatter or linter options?
- How should config reloads invalidate editor-side formatter, linter, and parse artifacts?
- How should config loading failures be surfaced in editor integrations without mixing them into parser, semantic, formatter, or linter diagnostics?
- Which future editor features should be designed first after diagnostics and formatting: quick fixes, hover, completion, go-to-definition, or rename?
