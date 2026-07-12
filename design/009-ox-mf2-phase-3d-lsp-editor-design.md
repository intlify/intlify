# ox-mf2 Phase 3D LSP and Editor Design

This document tracks the detailed LSP and editor integration design for ox-mf2.

The Phase 3 tooling boundary is defined in [005-ox-mf2-phase-3-tooling-transport-design.md](./005-ox-mf2-phase-3-tooling-transport-design.md). That document fixes the high-level adapter workflow. This document is the implementation-facing place to refine host document mapping, diagnostic publication, formatting edits, configuration reload behavior, and future editor features.

Where the Phase 3 boundary document describes resource and catalog files, the editor workflow is specified against the host-format-agnostic message entry model. Localizable MF2 messages are managed in message catalogs with many different host formats — JSON, YAML, JSON5, XLIFF, framework-specific single-file-component blocks, and other localization interchange formats — so this document specifies editor behavior against extracted message entries rather than against any concrete host format. The message entry model, the host format adapter contract, the host format registry, catalog configuration, and the host format tier roadmap are owned by [013-ox-mf2-resource-catalog-adapter-design.md](./013-ox-mf2-resource-catalog-adapter-design.md); this document owns the editor-facing behavior built on those shared contracts.

## Goals

- Reuse the parser, formatter, linter, `SnapshotView`, and future `SemanticView` without making the core crates depend on LSP protocol types.
- Support diagnostics and formatting for standalone `.mf2` files.
- Support diagnostics and formatting for MF2 messages embedded in message catalog host files through adapter-owned extraction and mapping, starting with JSON catalogs; later host formats follow the tier roadmap owned by [013-ox-mf2-resource-catalog-adapter-design.md](./013-ox-mf2-resource-catalog-adapter-design.md).
- Build the editor workflow on the consumer-neutral message entry contract owned by [013-ox-mf2-resource-catalog-adapter-design.md](./013-ox-mf2-resource-catalog-adapter-design.md), so additional catalog formats extend the shared registry without changing diagnostics publication, formatting edits, caching, or position conversion behavior.
- Convert core UTF-8 byte spans into editor-facing positions, UTF-16 by default, at the adapter boundary.
- Keep LSP/editor artifact caches, document version checks, and host-file edit ownership outside of the parser, formatter, and linter cores.

## Non-Goals

- Making an LSP server or editor extension a direct Phase 3 product.
- Adding LSP protocol types to parser, formatter, or linter result objects.
- Implementing code actions, quick fixes, hover, completion, go-to-definition, or rename in the initial workflow.
- Implementing true range-only formatting or minimal-diff formatting in the formatter core.
- Owning the message entry model, host format adapter contract, registry, catalog configuration, or host format tier roadmap; those are owned by [013-ox-mf2-resource-catalog-adapter-design.md](./013-ox-mf2-resource-catalog-adapter-design.md).
- Owning host-format syntax validation, host-format schema validation, or host-format styling. JSON, YAML, and XML syntax errors and document layout belong to host-format tooling.
- Cross-file and cross-locale catalog features such as missing-translation or key-parity checks. They belong to future catalog-level linting layered per [008-ox-mf2-phase-3c-linter-design.md](./008-ox-mf2-phase-3c-linter-design.md) and [013-ox-mf2-resource-catalog-adapter-design.md](./013-ox-mf2-resource-catalog-adapter-design.md).
- Defining a plugin system for editor integrations or a third-party host format adapter API.

## Initial Workflow

The initial editor workflow focuses on diagnostics and formatting.

Adapters extract MF2 message entries from the host document, call core APIs per entry, and map message-local results back to document-level ranges. The core APIs operate on message-local UTF-8 byte spans. Editor adapters own document URI handling, document version checks, editor position conversion, and protocol-specific result shapes.

## Host Document Model

![Phase 3D LSP and editor architecture](./assets/009-ox-mf2-phase-3d-lsp-editor-architecture.svg)

Editor adapters should treat every supported document as a host document that yields zero or more message entries over the same message-level core.

- A **host document** is one editor document: a standalone `.mf2` file or an opted-in message catalog file, beginning with JSON catalogs.
- A **message entry** is the unit that connects one host document region to one MF2 message: a stable entry key, a raw host value span, MF2 message text, a message-to-raw offset map, and a read-only marker. The canonical entry model definition and its invariants are owned by [013-ox-mf2-resource-catalog-adapter-design.md](./013-ox-mf2-resource-catalog-adapter-design.md).
- A **host format adapter** is the format-specific component that classifies, parses, maps, and re-escapes one host format, per the adapter contract in the same document.

Standalone `.mf2` files are the degenerate host format in the editor workflow. Before producing the single message entry, the standalone adapter applies the Phase 3B [File Framing](./007-ox-mf2-phase-3b-formatter-design.md#file-framing) read contract: it removes at most one leading UTF-8 BOM and then one trailing `LF` or `CRLF`, when present. The entry's message text is that unframed text, its raw value span is the whole document, and its document map retains the removed framing so message-local positions map back to the original bytes. Standalone documents take the same downstream path as catalog entries after extraction; they are not a separate diagnostics or formatting workflow.

All downstream editor behavior in this document — diagnostics publication, formatting edits, artifact caching, and position conversion — is defined against message entries, never against concrete host formats. Supporting a new catalog format means implementing the shared adapter contract and registering it in the shared layer; it must not require changes to parser, formatter, or linter core APIs or to the editor workflow contract.

## Host Format Adapter Layer

Editor adapters consume catalog classification, extraction, offset mapping, and write-back re-escaping from the shared resource adapter layer specified by [013-ox-mf2-resource-catalog-adapter-design.md](./013-ox-mf2-resource-catalog-adapter-design.md). The editor-owned standalone adapter produces the same message entry shape directly and applies the Phase 3B file-framing contract; it does not route `.mf2` files through catalog extraction. The editor workflow relies on these guarantees:

- entries are ordered by raw span start and raw value spans do not overlap
- catalog entry text is byte-exact, with no trimming, normalization, or injected framing; standalone entry text differs from the document bytes only by the explicitly removed file framing
- offset maps are monotonic and mapped ranges never split a host escape sequence
- write-back re-escaping is value-identical, and entries that cannot be safely re-escaped are marked read-only
- project-configured catalog detection is strictly opt-in and resolves identically in editor and CLI resource workflows

Editor-side registry behavior on top of those guarantees:

- documents with the `.mf2` extension or the MF2 language id always resolve to the standalone adapter
- editor-specific settings may add editor-only ad-hoc catalog opt-in on top of the project configuration; this additive overlay does not change CI catalog membership, and the precedence for overlaps is an open question below
- a host document resolves to at most one host format adapter

## Diagnostics Publication

Editor diagnostics should be produced from the shared diagnostic result contract used by parser, semantic, and linter workflows.

The preferred initial path is to use source-backed `lintMessage` per message entry as the diagnostic source for editor publication because it already includes parser, semantic, and lint diagnostics. A future `lintSnapshot` path is an optimization for parse-artifact reuse after the parser owns a snapshot-to-`SemanticModel` path. If an adapter composes diagnostics manually from cached parser, semantic, and linter results, that composition is an internal optimization only and must preserve the same ordering, category/code/severity values, and de-duplication behavior as `lintMessage`.

The editor adapter consumes the linter result contract from [008-ox-mf2-phase-3c-linter-design.md](./008-ox-mf2-phase-3c-linter-design.md). Parser-owned semantic diagnostic catalog and ordering details remain owned by [012-ox-mf2-parser-semantic-validation-design.md](./012-ox-mf2-parser-semantic-validation-design.md). Rule-facing documentation and rule ids are indexed from [linter-rules/index.md](./linter-rules/index.md). Editor integrations should not redefine diagnostic meaning, rule metadata, or semantic validation behavior.

The initial editor workflow follows the same strict pipeline as CLI and bindings for each message entry:

- parser diagnostics are always reported
- semantic diagnostics are reported only when parsing has no parser diagnostics and semantic validation runs
- lint diagnostics are reported only when parser and semantic diagnostics are clean
- parser diagnostics prevent semantic validation and linter rule execution

Multi-entry host documents add document-level publication rules on top of the per-entry contract:

- Entries are independent. Parser diagnostics in one entry never suppress semantic validation, linting, or publication for other entries; the strict pipeline is per entry, not per document.
- Published document diagnostics are the union of all entries' mapped diagnostics. Entries appear in raw span order; within one entry, the core result order is preserved verbatim. Because entry spans do not overlap, this ordering is deterministic.
- Diagnostic identity across document updates should combine the entry key, the stable diagnostic code, and the message-local span. Entry keys survive unrelated edits elsewhere in the catalog, so this identity is more stable than document-level spans alone.
- Catalog-level diagnostics such as duplicate catalog keys or cross-locale checks are future catalog-level linting. When they arrive they flow through the same entry mapping and must not be reimplemented in editor layers.

When host document extraction fails because the host document cannot be parsed, the adapter publishes no new MF2 diagnostics and returns no formatting edits. Previously published diagnostics should be retained rather than cleared, because host syntax breakage is usually a transient typing state; the retained set is replaced on the next successful extraction, and exact retention tuning is an implementation detail. Host syntax errors are never translated into MF2 diagnostics, and extraction failure is an expected editing state, not an operational editor error. The CLI presents the same extraction failure as a target-local operational error instead; that surface behavior is owned by [013-ox-mf2-resource-catalog-adapter-design.md](./013-ox-mf2-resource-catalog-adapter-design.md).

Core `"error"` and `"warn"` severities map to editor/LSP diagnostic severity at the adapter boundary; adapters convert `"warn"` to the editor's warning severity. Editor layers may add protocol-specific advice later, but the core linter does not emit `info` or `hint` diagnostics initially.

## Formatting Edits

Formatter core APIs return formatted message text, not LSP `TextEdit` objects.

Editor adapters should find the containing message entry, call whole-message formatting, and create editor edits at the adapter boundary. The initial adapter replaces the whole containing message range rather than computing a minimal diff. For standalone `.mf2` files, that range is the whole document and the replacement is the formatted unframed message followed by exactly one `LF`, with no BOM, matching the Phase 3B [File Framing](./007-ox-mf2-phase-3b-formatter-design.md#file-framing) write contract. The adapter compares those framed replacement bytes with the original document, so a leading BOM, a missing final newline, or a final `CRLF` produces an edit even when the unframed message text is already formatted. For catalog host documents, the range is the raw value span of the containing entry, replaced with the re-escaped formatted text produced by the shared write-back re-escaping contract in [013-ox-mf2-resource-catalog-adapter-design.md](./013-ox-mf2-resource-catalog-adapter-design.md); catalog entries never receive standalone file framing.

Document formatting for a catalog host document formats every writable message entry independently:

- Entries whose extracted text has parser diagnostics are skipped as per-entry no-ops, matching the strict invalid-syntax contract in [007-ox-mf2-phase-3b-formatter-design.md](./007-ox-mf2-phase-3b-formatter-design.md). Other entries still format.
- Read-only entries are skipped silently.
- Each changed entry produces one edit replacing its raw value span with the re-escaped formatted text. Unchanged entries produce no edit.
- Formatting edits never touch host syntax outside raw value spans: no key reordering, no indentation or quoting changes outside the message value, and no host layout normalization. Host-format styling belongs to host formatters, so keeping MF2 edits inside value spans lets an MF2 formatter and a host-format formatter coexist on one document without conflicting edits.

A range formatting request formats the writable entries whose raw value spans intersect the requested range, using whole-entry formatting.

Adapters should only return edits when the document version and message mapping used to create the edit still match the current document. If the document version or mapping is stale, or the containing message entry can no longer be identified, the adapter silently returns no edits. This expected concurrency outcome is a no-op, not an operational editor error. The exact protocol-specific version comparison remains an implementation-design question.

True range-only formatting and minimal-diff formatting are deferred. A selection inside an MF2 message should initially format the containing message rather than requiring range-local formatting from the formatter core.

## Span and Position Conversion

Core parser, snapshot, semantic, formatter, and linter APIs use message-local UTF-8 byte spans as their canonical location model. Editor adapters convert spans through a fixed pipeline at the adapter boundary:

1. Core APIs return message-local UTF-8 byte spans over the entry message text.
2. The entry offset map, per the mapping contract in [013-ox-mf2-resource-catalog-adapter-design.md](./013-ox-mf2-resource-catalog-adapter-design.md), converts message-local spans to host-document UTF-8 byte spans.
3. The adapter converts host-document UTF-8 spans to the editor-facing position encoding.

For standalone `.mf2` documents, step 2 uses the file-framing-aware document map. A removed leading UTF-8 BOM shifts mapped byte positions by its three-byte length, while a removed trailing newline stays outside the message range; when no framing was removed, the map is the identity over the message body. The editor-facing default is UTF-16 positions; adapters that negotiate a different LSP `positionEncoding`, UTF-8 or UTF-32 in LSP 3.17 and later, still perform the conversion at the same boundary. The parser, formatter, and linter cores never perform position encoding conversion.

## Configuration Sources

Editor adapters should normalize project configuration and editor-specific settings into the same resolved formatter and linter configuration models used by CLI workflows.

Possible editor-specific sources include workspace settings, user settings, and LSP initialization options. The exact source list, precedence, reload behavior, and failure presentation are still open.

Catalog opt-in and extraction scope configuration are owned by [013-ox-mf2-resource-catalog-adapter-design.md](./013-ox-mf2-resource-catalog-adapter-design.md) as the `resources` section of the unified project config. Editor adapters resolve that section into the same project-configured catalog membership as CLI workflows and may layer explicitly editor-only ad-hoc opt-in settings on top. Changing either source re-runs editor document classification and extraction, with the invalidation consequences described below; editor-only membership is not implied to exist in CI.

Configuration loading failures are operational editor errors. They should not be mixed into parser, semantic, formatter, or linter diagnostics.

## Artifact Cache and Invalidation

Editor adapters may cache two artifact classes per document version once semantic APIs are exposed:

- extraction artifacts per host document version: classification results, entry lists, and offset maps
- message-level artifacts per entry: source views, decoded snapshots, diagnostics, and future semantic views

Message-level artifacts should follow the cache-key discipline in [ox-mf2-parse-artifact-cache.md](./ox-mf2-parse-artifact-cache.md), using the entry key as the message id and the document URI as part of the namespace. Because that cache key includes the exact source bytes, entries whose message text is unchanged across host document versions can hit the cache even when unrelated parts of a large catalog changed. Extraction artifacts remain version-specific.

Cached artifacts must be invalidated when the document changes. Configuration changes may also invalidate classification, extraction, formatter, linter, semantic, or diagnostic artifacts depending on which options changed.

Detailed cache ownership, eviction, and invalidation policy belongs to the LSP/editor implementation design and the parse artifact cache design.

## Shared Resource Adapter Ownership

Host format extraction, mapping, and write-back re-escaping are implemented once in the shared resource adapter layer owned by [013-ox-mf2-resource-catalog-adapter-design.md](./013-ox-mf2-resource-catalog-adapter-design.md), and editor integrations must consume that layer rather than reimplementing host-format knowledge per editor. Message-to-raw mapping and re-escaping correctness is subtle and must not drift between the CLI and editor surfaces.

Editor-side glue stays thin. Document lifecycle, URI and version tracking, protocol result shapes, and position encoding remain editor adapter code; host-format knowledge does not.

## Future Editor Features

The initial workflow does not require code actions, quick fixes, hover, completion, go-to-definition, rename, true range-only formatting, or minimal-diff formatting.

Future editor features should build on stable core concepts rather than adding LSP-specific state to parser, formatter, or linter crates. Quick fixes are a future adapter-owned feature. They may use stable diagnostic codes, configurable rule metadata, formatter output, and future rule suggestions, but the initial linter core does not expose a fix API. Style fixes should call formatter APIs rather than reimplementing formatting inside editor or linter adapters. Semantic features should build on future `SemanticView` exposure.

Cross-locale editor features — missing-translation indicators, catalog key completion, and locale parity lenses — additionally build on future catalog-level linting and locale-bound catalog configuration, not on editor-local extraction alone.

## Open Questions

- What precedence applies between the project catalog configuration owned by [013-ox-mf2-resource-catalog-adapter-design.md](./013-ox-mf2-resource-catalog-adapter-design.md) and editor-specific ad-hoc opt-in settings?
- Once snapshot-to-`SemanticModel` exists, when should editor adapters switch from source-backed `lintMessage` to future `lintSnapshot` for parse-artifact reuse?
- Should a future recovery-aware editor mode provide partial semantic or lint diagnostics for incomplete buffers, and how would it avoid conflicting with the strict CLI and binding pipeline?
- What exact document version checks are required before returning formatting `TextEdit` values?
- Which configuration sources should editor adapters support, such as project config, VS Code workspace settings, user settings, and LSP initialization options?
- What precedence should apply when project config and editor-specific settings provide overlapping formatter or linter options?
- How should config reloads invalidate editor-side classification, extraction, formatter, linter, and parse artifacts?
- How should config loading failures be surfaced in editor integrations without mixing them into parser, semantic, formatter, or linter diagnostics?
- Which future editor features should be designed first after diagnostics and formatting: quick fixes, hover, completion, go-to-definition, or rename?
