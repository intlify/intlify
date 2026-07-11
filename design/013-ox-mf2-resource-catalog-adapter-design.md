# ox-mf2 Resource Catalog Adapter Design

## Purpose

This document tracks the detailed design of the shared resource catalog adapter layer for ox-mf2: extraction of MF2 message entries from multi-entry host files such as JSON and YAML catalogs, mapping between decoded message text and raw host documents, write-back encoding, and the CLI resource workflows that formatting and linting build on that layer.

The Phase 3 tooling boundary is defined in [005-ox-mf2-phase-3-tooling-transport-design.md](./005-ox-mf2-phase-3-tooling-transport-design.md). That document treats resource files, framework-specific i18n files, and multi-locale catalogs as layered consumers of the message-level formatter and linter. The formatter-side expectations are recorded in [007-ox-mf2-phase-3b-formatter-design.md](./007-ox-mf2-phase-3b-formatter-design.md#resource-and-catalog-formatting), the linter-side expectations in [008-ox-mf2-phase-3c-linter-design.md](./008-ox-mf2-phase-3c-linter-design.md#resource-and-catalog-linting), and the editor-side consumption in [009-ox-mf2-phase-3d-lsp-editor-design.md](./009-ox-mf2-phase-3d-lsp-editor-design.md). This document is the dedicated resource adapter design those documents defer to.

The message entry model and the host format adapter contract were first drafted from the editor perspective. This document owns them as consumer-neutral contracts; [009-ox-mf2-phase-3d-lsp-editor-design.md](./009-ox-mf2-phase-3d-lsp-editor-design.md) retains only editor-specific behavior over the same contracts. Resource catalog support is a layered milestone on top of the Phase 3 products, not itself a numbered Phase 3 product phase.

## Goals

- Define one consumer-neutral message entry model and host format adapter contract shared by CLI formatting, CLI linting, and editor adapters.
- Fix one shared implementation home for host format classification, span-preserving parsing, extraction, offset mapping, and write-back encoding.
- Define catalog opt-in configuration as part of the unified project config so CLI and editor surfaces resolve identical project-configured catalog membership, while allowing an editor to add an explicitly editor-only ad-hoc overlay.
- Define the CLI resource workflows: input selection, per-entry formatting and linting, result reporting, write-back composition, and check semantics.
- Keep the message-level parser, formatter, and linter cores unchanged; the resource layer and its consumers compose them.
- Keep observable CLI output deterministic, following the shared Phase 3A output conventions.

## Non-Goals

- Implementing every candidate host format at once; host formats arrive in tiers behind one adapter contract.
- Owning host-format syntax validation, host-format schema validation, or host-format styling. JSON, YAML, and XML syntax errors and document layout belong to host-format tooling.
- Catalog-level and cross-locale rules such as duplicate catalog keys, key parity, or missing translations in the initial milestone. The entry model must support them later, but their rule design is future work.
- Extracting MF2 messages from executable JS/TS catalog modules. Extraction from code requires host-language semantic analysis that this layer does not own; bundler and build tooling own that surface.
- A third-party host format adapter plugin API. The host format registry is a built-in, per-release set.
- Editor-specific behavior. Document lifecycle, versions, position encodings, publish timing, staleness handling, and editor edits remain owned by [009-ox-mf2-phase-3d-lsp-editor-design.md](./009-ox-mf2-phase-3d-lsp-editor-design.md).
- Nested config discovery, nearest-config-wins behavior, and per-catalog formatter or linter option overrides.

## Architecture

![ox-mf2 resource catalog adapter architecture](./assets/013-ox-mf2-resource-catalog-adapter-architecture.svg)

The resource layer sits beside the message-level cores, not on top of them. It depends only on host-format parsing and produces message entries; it does not call the parser, formatter, or linter itself. Consumers — the CLI fmt workflow, the CLI lint workflow, and editor adapters — compose extraction with per-entry message-level core calls and own their surface-specific presentation.

## Message Entry Model

A message entry is the unit that connects one host document region to one MF2 message. An entry consists of:

- a stable entry key
- a raw host value span, as a host-document UTF-8 byte range
- decoded MF2 message text
- a decoded-to-raw offset map
- a read-only marker for values that cannot be safely re-encoded

Consumer-neutral invariants:

- Extraction returns entries ordered by raw span start. Raw value spans of different entries must not overlap.
- Decoded message text must be the exact string value that an i18n runtime would receive from the host format. Adapters must not trim, normalize, or append to decoded text; per the [Phase 3B file framing contract](./007-ox-mf2-phase-3b-formatter-design.md#file-framing), message-level core APIs never receive an injected final newline or lose message-leading content.
- Host constructs whose decoded value cannot be represented as well-formed UTF-8, such as JSON escape sequences encoding unpaired surrogates, are excluded from extraction until the parser-level source-text direction noted in [002-ox-mf2-phase-1-rust-parser-design.md](./002-ox-mf2-phase-1-rust-parser-design.md) supports them.

Standalone `.mf2` files can reuse this model's shape as a degenerate single-entry host document, but their map is not unconditionally the identity: the editor adapter applies the Phase 3B [File Framing](./007-ox-mf2-phase-3b-formatter-design.md#file-framing) read contract and retains any removed BOM and trailing newline in a framing-aware document map. That uniformity is an editor-workflow concern owned by [009-ox-mf2-phase-3d-lsp-editor-design.md](./009-ox-mf2-phase-3d-lsp-editor-design.md). CLI formatting and linting keep their existing direct `.mf2` file paths unchanged; resource workflows neither apply standalone file framing to embedded messages nor reroute standalone message files through catalog extraction.

## Host Format Adapter Contract

A host format adapter is the format-specific component that classifies one host format, parses it, maps it into message entries, and re-encodes formatted message text back into host raw text.

### Extraction

- Host parsing must be span-preserving. Extraction needs the raw source span of every message value, so value-only deserialization such as plain `serde`-style decoding is not sufficient.
- Extraction produces the ordered entry list defined by the message entry model.

### Entry Identity

- The entry key is a serialized, structurally unambiguous identity. JSON adapters use RFC 6901 JSON Pointer, so the literal catalog key `"a.b"` and the nested path `a` → `b` remain distinct even when a runtime flattens both to `a.b`. YAML adapters use a typed structural path that distinguishes mapping keys from sequence indices and preserves each scalar key's resolved tag and value; entry paths containing complex mapping keys are excluded initially. A YAML adapter may use JSON Pointer serialization only for JSON-compatible, string-keyed paths. Thus a string key `"1"`, an integer key `1`, and sequence index `1` cannot collide.
- The entry key is an identity, not a display string. Adapters may additionally expose a runtime-style display key, such as dot-joined nesting, for UI and reporting purposes, but identity comparisons use the structural key.
- When a host document contains duplicate keys, each raw occurrence is a separate entry and the entry key carries an occurrence discriminator. Reporting duplicate catalog keys as a problem is future catalog-level linting, not an extraction failure.

### Decoded-to-Raw Offset Mapping

Each entry carries a monotonic offset map that aligns decoded message-local UTF-8 byte ranges with raw host UTF-8 byte ranges inside the raw value span.

- Runs where decoded bytes equal raw bytes share one identity segment.
- Each atomic decoding unit — a single host escape, multiple escapes that jointly decode one scalar such as a JSON UTF-16 surrogate pair, or an XML entity — is one segment mapping the decoded bytes to the full raw encoded range.
- Raw-only syntax such as string quotes and CDATA delimiters remains inside the raw value span used for replacement but consumes no decoded bytes. Offset maps retain these boundary gaps so diagnostic ranges do not consume delimiters. When the entry shape is reused by the standalone editor adapter, removed file framing is represented by the same kind of raw-only boundary gap.
- A message-local position inside a decoded escape maps to the raw escape start when it is a range start and to the raw escape end when it is a range end, so mapped ranges never split a host escape sequence.

### Write-Back Encoding

- Re-encoding must be value-identical: decoding the re-encoded text must produce exactly the formatted message text.
- Re-encoding should preserve the host value style, such as quoting or scalar style, when that style can represent the formatted text. Otherwise the adapter may switch to a style that can, while keeping the host document semantically identical outside the message value.
- When the adapter cannot guarantee value-identical re-encoding for a host construct, extraction marks the entry read-only. Read-only entries still produce diagnostics; formatting skips them.

### Host Parse Failures

When the host document itself cannot be parsed, extraction fails for the whole document. Host syntax errors are never translated into MF2 diagnostics; host-format tooling owns them. Presentation of an extraction failure is surface-specific: editor adapters retain previously published diagnostics as defined in [009-ox-mf2-phase-3d-lsp-editor-design.md](./009-ox-mf2-phase-3d-lsp-editor-design.md), while the CLI reports a target-local operational error as defined in the CLI resource workflow below.

## Host Format Registry and Catalog Detection

Consumers resolve each input file or document to at most one host format adapter through a host format registry.

- Catalog host formats are strictly opt-in. Arbitrary JSON, YAML, or XML files must not be assumed to contain MF2 messages; a false positive reports wrong diagnostics on unrelated files and, worse, exposes them to formatting write-back. That is worse than requiring configuration.
- Project-configured catalog opt-in comes from the `resources` section of the unified project config defined below. Editor adapters may additionally layer explicitly editor-only ad-hoc settings over that result. Filename conventions such as `*.mf2.json` may become future convenience defaults but are not part of the initial contract.
- CLI resource workflows and editor adapters must resolve project-configured membership and host format identically. An editor-only ad-hoc entry is intentionally absent from CLI and CI processing until it is persisted in project configuration; editor integrations should distinguish that state rather than presenting it as CI coverage.
- If overlapping catalog configuration maps one file to multiple host formats, that is a configuration validation failure and therefore an operational error, not silent precedence.

Within an opted-in catalog document, the default extraction scope is every string leaf value, including string elements of arrays, whose path and decoded value satisfy the message-entry representability invariants above. String leaves excluded by those invariants, such as values below an initially unsupported YAML complex-key path or values that cannot be represented as well-formed UTF-8, are not entries. Non-string scalars are never message entries. Narrowing extraction with key selectors, such as include/exclude paths, is future configuration detail; the entry model does not change when selectors arrive.

Locale identity is not required by per-entry formatting and linting, because parsing, formatting, and linting one MF2 message do not depend on locale. Catalog configuration may later bind locale metadata, for example through `locales/{locale}.json` path patterns, to entries for future catalog-level and cross-locale checks.

## Host Format Tiers

Host formats are introduced in tiers behind the same adapter contract. Tiers order implementation effort and ecosystem demand; they do not change the contract.

| Tier | Host formats | Status |
| --- | --- | --- |
| 1 | JSON catalogs (`.json`); YAML catalogs (`.yaml`, `.yml`) | initial resource milestone |
| 2 | JSONC and JSON5 catalogs | planned extension |
| 3 | Vue SFC `<i18n>` custom blocks through adapter composition; XLIFF 1.2 / 2.x; other interchange formats such as ARB, gettext PO, and Java properties | candidates, demand-driven |

### JSON Catalogs

JSON catalogs decode RFC 8259 string escapes. The JSON Pointer entry identity rules and the unpaired-surrogate exclusion above apply. Formatted multi-line MF2 output, such as a formatted matcher, re-encodes line breaks as `\n` escapes inside the single-line JSON string value.

### YAML Catalogs

YAML catalogs resolve entries from tag-resolved string scalars. Plain, single-quoted, and double-quoted scalars are read-write in the initial tier. Literal and folded block scalars must still produce correct offset maps for diagnostics but are read-only for formatting initially, because value-identical re-encoding across block scalar indentation and folding rules needs its own design. Anchored values produce one entry at the anchor definition site; alias nodes and merge-key expansions do not produce additional entries.

### JSONC and JSON5 Catalogs

JSONC adds comments and trailing commas over the JSON adapter. JSON5 adds further string syntax, such as single-quoted strings and line continuations, that changes decoding and re-encoding but not the entry model. These are ecosystem-relevant catalog syntaxes and should reuse the JSON adapter structure.

### Vue SFC `<i18n>` Custom Blocks

Single-file-component `<i18n>` blocks embed a JSON/JSON5/YAML catalog region inside a `.vue` host document. This is adapter composition: an outer adapter locates the block region and its declared language, an inner catalog adapter runs over the region text, and the offset maps compose into document coordinates. The entry model is closed under this composition; no new consumer-facing behavior is required.

### XLIFF

XLIFF is an XML host format: XLIFF 1.2 stores message text in `<trans-unit>` `<source>`/`<target>` elements and XLIFF 2.x in `<unit>`/`<segment>` `<source>`/`<target>` elements. Entry keys serialize the file/group/unit and, where present, segment identity path. Decoding covers XML entities and CDATA sections and must respect `xml:space` handling.

Segments that contain inline elements such as `<ph>`, `<pc>`, `<g>`, or `<x>` are not extracted as MF2 entries initially, because their decoded text interleaves markup and would require a placeholder-protection design. Whether MF2 message text is carried inside XLIFF at all is a project or TMS convention; the adapter processes only catalogs that configuration explicitly opts in.

## Catalog Configuration

Catalog opt-in is a new `resources` section of the unified project config, alongside the existing `fmt` and `lint` sections, because catalog membership is consumed by formatting, linting, and editor surfaces alike. The section follows the Phase 3A configuration contract in [006-ox-mf2-phase-3a-tooling-foundation-design.md](./006-ox-mf2-phase-3a-tooling-foundation-design.md): root-only discovery, strict unknown-field validation, Rust config model as the source of truth, and schema generation into the committed `config.schema.json` artifact.

The initial section shape direction:

```jsonc
{
  "resources": {
    "catalogs": [
      {
        // Required. Files covered by this catalog definition.
        "include": ["locales/**/*.json"],
        // Optional. Excluded files within include.
        "exclude": ["locales/**/generated.*.json"],
        // Optional. Host format override; defaults from the file extension.
        "format": "json"
      }
    ]
  }
}
```

- `include` globs are required per catalog definition; `exclude` is optional. Glob semantics follow the shared CLI discovery contract in [008-ox-mf2-phase-3c-linter-design.md](./008-ox-mf2-phase-3c-linter-design.md#file-discovery-and-shared-cli-contract).
- `format` defaults from the file extension and exists for extension-ambiguous cases. A file assigned to different formats by overlapping definitions is a `config_validation_failed` error with a JSON pointer to the conflicting entry.
- Key selectors and locale binding placeholders are future fields of the catalog definition; they must extend this shape without breaking existing configs.
- Exact field naming is fixed together with the config-model and schema change when implementation starts; this document fixes the semantics.

An empty or absent `resources` section means no catalog files are processed. Standalone `.mf2` workflows are unaffected by the `resources` section.

## Implementation Home and Package Boundaries

The shared layer lives in a workspace-internal `crates/intlify_resource` crate, following the crate conventions of the formatter and linter products: not a crates.io deliverable, `publish = false`, consumed by `crates/intlify_cli`.

- `crates/intlify_resource` owns the host format registry, span-preserving host parsers, extraction, entry identity, offset mapping, write-back encoding, and the resolved catalog configuration model for the `resources` section.
- The crate does not depend on `ox_mf2_parser`, `intlify_format`, or `intlify_lint`. Consumers compose extraction with message-level core calls. This keeps the dependency direction acyclic and the layer reusable by any consumer.
- Concrete host parser dependencies are implementation choices, constrained by the contract: raw value spans, byte-exact decoding, and value-identical re-encoding, all locked by fixtures.
- Binding packages such as `@intlify/resource-napi` and `@intlify/resource-wasm` mirror the formatter and linter packaging structure and are published only when editor or binding consumers materialize. Parser, formatter, and linter binding packages do not absorb resource APIs.

## CLI Resource Workflow

### Input Selection

A file enters resource processing only when it is both selected by CLI operands or globs and opted-in by the resolved `resources` configuration.

- The supported CLI input set extends from direct `.mf2` files to opted-in catalog files. The extension alone does not make a file a catalog; membership requires configuration. This extends the supported-input membership promised by the shared discovery contract in [008-ox-mf2-phase-3c-linter-design.md](./008-ox-mf2-phase-3c-linter-design.md#file-discovery-and-shared-cli-contract) without changing that contract's shape.
- Glob expansion intersects the supported input set: `.mf2` files plus opted-in catalog files. Non-opted-in JSON/YAML files matched by a glob are not resource inputs and are skipped by discovery.
- An explicit file operand that is neither a `.mf2` file nor an opted-in catalog follows the existing unsupported-input handling of the shared discovery contract.

### Host Parse Failures

When a catalog file cannot be parsed as its host format, the CLI reports a target-local operational error in `results[].errors` using the shared operational error shape: `kind: "input"`, `code: "resource_parse_failed"`, with `details.line` and `details.column` when the host parser reports them. The file contributes no diagnostics, is never written by fmt, and other selected files are still processed. Operational errors drive `summary.status: "error"` and exit code `2` per the Phase 3A output contract.

### Determinism and Parallelism

File results are reported in stable normalized path order; entries within a file are reported in raw span order. The CLI may parallelize across files and may parallelize entries within a file, but observable output order must remain deterministic. Benchmarks report extraction, encoding, core calls, and file I/O as separate phases.

## Catalog Linting

Catalog linting runs the message-level linter per entry and aggregates per file.

- Each entry's decoded text goes through `lintMessage(source, options)` with the same resolved lint configuration used for `.mf2` files. The strict `parser -> semantic -> rules` pipeline applies per entry.
- Entries are independent: parser diagnostics in one entry never suppress semantic validation, linting, or reporting for other entries.
- Entry diagnostics are reported with host-file coordinates as the primary location: the mapped host UTF-8 byte span and derived line/column, produced through the entry offset map. Each entry-level result carries its entry key; a display key may accompany it for human-readable output.
- If any per-entry linter call returns an operational error instead of a complete diagnostic result, the catalog target follows the existing target-level linter error contract: it reports `status: "error"`, an empty `diagnostics` array, and the error in `results[].errors`, with `details.entryKey` when the entry is known. Diagnostics already collected from other entries in that catalog are discarded because the target result is incomplete; other selected files still continue.
- The exact serialized layout of entry-level results is fixed as an extension of the linter JSON schema when implemented, inside the existing envelope, summary, and count conventions of [008-ox-mf2-phase-3c-linter-design.md](./008-ox-mf2-phase-3c-linter-design.md). Diagnostic counts and `--max-warnings` include entry diagnostics.
- Lint options apply uniformly; there are no per-catalog rule overrides initially.

## Catalog Formatting

Catalog formatting runs the message-level formatter per entry and composes write-back edits per file.

- Each writable entry's decoded text goes through `formatMessage(source, options?)` with the same resolved fmt configuration used for `.mf2` files.
- Write mode: for every writable, syntactically valid entry whose formatted output differs, the adapter re-encodes the formatted text and replaces the entry's raw value span. Edits are applied in descending raw-span offset order and the file is written once, using the Phase 3B CLI's file-I/O and operational-error conventions but not its standalone `.mf2` file framing. All bytes outside replaced value spans remain byte-identical, including any host-file BOM and trailing line ending: no key reordering, no indentation or quoting changes outside message values, and no host layout normalization.
- Entries whose extracted text has parser diagnostics are skipped per entry and reported with the same strict invalid-syntax semantics that [007-ox-mf2-phase-3b-formatter-design.md](./007-ox-mf2-phase-3b-formatter-design.md) fixes for invalid standalone files, scoped to the entry. Other entries still format. One broken message must not block formatting of a large catalog; this mirrors the editor behavior in [009-ox-mf2-phase-3d-lsp-editor-design.md](./009-ox-mf2-phase-3d-lsp-editor-design.md).
- For catalog targets, the formatting unit affected by parser diagnostics is the entry, not the whole host file. A diagnostic-bearing entry is never replaced, but the same catalog file may still be written once when other entries produce valid changes. This is the resource-layer specialization of Phase 3B's rule that parser diagnostics do not modify the affected formatting unit.
- Read-only entries are excluded from formatting by definition: write mode skips them and `--check` does not count them as changed. Whether check output should mention skipped read-only entries informationally is an open question.
- `--check` and `--list-different` report a catalog file as different when at least one writable, syntactically valid entry differs after formatting.
- If a per-entry formatter call or writable-entry encoding step returns an operational error, the catalog produces no write and no partial formatting result. Its file result uses `status: "error"`, `changed: false`, an empty `diagnostics` array, and the operational error in `results[].errors`, with `details.entryKey` when known. An encoding failure for an entry previously classified as writable is an `internal_error` with `details.reason: "resource_encode_failed"`. Other selected files still continue.
- Idempotency: the write-back round-trip law plus message-level formatter idempotency imply that formatting an already formatted catalog produces no writes. Fixtures lock this.

A catalog may contain both valid changed entries and parser-diagnostic entries. The file-level JSON result keeps the Phase 3B status enum and applies this precedence after successful, non-operational processing:

1. In write mode, any composed byte change uses `status: "formatted"` and `changed: true`, even when `diagnostics` is non-empty; the file is written once and the command still exits with `1` because diagnostics remain.
2. In check mode, any composed byte difference uses `status: "would_format"` and `changed: true`, even when `diagnostics` is non-empty; no file is written.
3. When no writable entry differs but one or more entries have parser diagnostics, the result uses `status: "diagnostic"` and `changed: false`.
4. When no writable entry differs and no diagnostics or operational errors exist, the result uses `status: "unchanged"` and `changed: false`.

Accordingly, `formattedFiles` or `differentFiles` and `diagnosticFiles` may count the same catalog file; these summary counts are intentionally not a partition of `matchedFiles`. `diagnosticCount` includes all mapped entry diagnostics, and diagnostics or check differences keep `summary.status: "failure"` and exit code `1` unless an operational error raises the result to exit code `2`.

## Editor Consumption

For project-configured catalog documents, editor adapters consume the same registry, extraction, offset maps, and write-back encoding through this shared layer, so membership, host format classification, and mapping are identical in CI and in the editor. An editor-only ad-hoc opt-in still uses the same host adapter contract, but its membership is intentionally editor-local until persisted in project configuration. Standalone `.mf2` documents continue through the editor-owned adapter defined by [009-ox-mf2-phase-3d-lsp-editor-design.md](./009-ox-mf2-phase-3d-lsp-editor-design.md), not catalog extraction. Surface behavior stays intentionally different: editors convert mapped spans to editor position encodings, publish per document, retain previous diagnostics across transient host parse failures, and no-op on stale document versions, while the CLI reports host parse failures as target-local operational errors and writes files directly.

## Catalog-Level Checks

Catalog-level and cross-locale checks — duplicate catalog keys, key parity across locales, missing or unused translations — operate across entries and files above this layer. They require locale binding in catalog configuration and their rule identities, presets, and severities belong to the linter product design track. They are future work, and they must flow through the same entry model rather than introducing a second extraction path.

## Benchmarks

Resource benchmarks are phase-separated, following the Phase 3 benchmark conventions:

- `resource_extract`: host parse and entry extraction per format
- `resource_encode`: write-back encoding and edit composition
- `fmt_catalog_check_e2e` and `fmt_catalog_write_e2e`
- `lint_catalog_e2e`
- catalog-shaped cache scenarios from [ox-mf2-parse-artifact-cache.md](./ox-mf2-parse-artifact-cache.md), such as unchanged decoded entries across host file edits

Extraction and encoding costs must be reported separately from parser, semantic, rule, formatter, and file I/O costs.

## Validation

- Extraction fixtures per host format: escape decoding, JSON Pointer identity including `"a.b"` versus nested `a` → `b`, typed YAML path identity including string-key/integer-key/sequence-index collisions, duplicate-key occurrence discriminators, array elements, YAML scalar styles, anchors and aliases, block scalar read-only marking, complex-key exclusion, and unpaired-surrogate exclusion.
- Round-trip tests: for writable constructs, extracting the re-encoded output yields exactly the formatted message text.
- Offset map fixtures: message-local spans map to expected host spans across single escapes, compound surrogate-pair escapes, and raw-only delimiters, including the escape-boundary rule.
- CLI end-to-end fixtures: lint and fmt write/check over catalog fixtures, including broken-entry and broken-host-file cases, mixed changed-plus-diagnostic formatter results and overlapping summary counts, per-entry operational failures with no partial file write, deterministic ordering, and JSON envelope shapes.
- Configuration fixtures: `resources` section validation, overlapping-definition conflicts, and empty-section behavior.
- Determinism tests for parallel runs.

## Relationship to Other Documents

| Document | Owns |
| --- | --- |
| [005-ox-mf2-phase-3-tooling-transport-design.md](./005-ox-mf2-phase-3-tooling-transport-design.md) | Phase 3 boundary; resource/catalog work as layered consumers |
| [006-ox-mf2-phase-3a-tooling-foundation-design.md](./006-ox-mf2-phase-3a-tooling-foundation-design.md) | CLI foundation: config envelope, operational error shape, exit codes, schema pipeline |
| [007-ox-mf2-phase-3b-formatter-design.md](./007-ox-mf2-phase-3b-formatter-design.md) | message-level formatter contract, file framing, check contract |
| [008-ox-mf2-phase-3c-linter-design.md](./008-ox-mf2-phase-3c-linter-design.md) | message-level linter contract, shared file discovery, diagnostic schema and envelope |
| [009-ox-mf2-phase-3d-lsp-editor-design.md](./009-ox-mf2-phase-3d-lsp-editor-design.md) | editor-facing behavior over this layer |
| [ox-mf2-parse-artifact-cache.md](./ox-mf2-parse-artifact-cache.md) | per-entry parse artifact reuse |
| this document | message entry model, host format adapter contract, registry and catalog configuration, `crates/intlify_resource` boundary, CLI resource workflows |

## Open Questions

- What exact Rust and binding-facing type shapes should represent the message entry model and offset map?
- What exact `resources` config field names and shapes should ship: key selectors, locale binding placeholders, per-catalog `format` defaulting rules, and interaction with `--config`?
- Should filename conventions such as `*.mf2.json` become default opt-in in a later tier, and if so with what override semantics?
- When should YAML block scalars graduate from read-only to writable entries, and what style-switch policy applies when a formatted message no longer fits the original scalar style?
- How should XLIFF inline elements be represented if XLIFF entries need more than plain-text segments: placeholder protection, or continued exclusion?
- Which layer owns outer block extraction for Vue SFC `<i18n>` custom blocks: this shared layer or SFC-specific tooling composing it?
- What exact serialized layout should entry-level results use inside the linter and formatter JSON schemas, and should message-local spans be exposed alongside mapped host spans?
- Should `fmt --check` report skipped read-only entries informationally?
- When, if ever, should per-catalog formatter or linter option overrides be reconsidered?
- What locale binding syntax should catalog configuration use for future catalog-level and cross-locale checks?
- What intra-file parallelism granularity is worthwhile for very large catalogs?
