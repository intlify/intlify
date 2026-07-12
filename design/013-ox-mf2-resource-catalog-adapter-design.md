# ox-mf2 Resource Catalog Adapter Design

## Purpose

This document tracks the detailed design of the shared resource catalog adapter layer for ox-mf2: extraction of MF2 message entries from multi-entry host files such as JSON catalogs, mapping between message text and raw host documents, write-back re-escaping, and the CLI resource workflows that formatting and linting build on that layer.

The Phase 3 tooling boundary is defined in [005-ox-mf2-phase-3-tooling-transport-design.md](./005-ox-mf2-phase-3-tooling-transport-design.md). That document treats resource files, framework-specific i18n files, and multi-locale catalogs as layered consumers of the message-level formatter and linter. The formatter-side expectations are recorded in [007-ox-mf2-phase-3b-formatter-design.md](./007-ox-mf2-phase-3b-formatter-design.md#resource-and-catalog-formatting), the linter-side expectations in [008-ox-mf2-phase-3c-linter-design.md](./008-ox-mf2-phase-3c-linter-design.md#resource-and-catalog-linting), and the editor-side consumption in [009-ox-mf2-phase-3d-lsp-editor-design.md](./009-ox-mf2-phase-3d-lsp-editor-design.md). This document is the dedicated resource adapter design those documents defer to.

The message entry model and the host format adapter contract were first drafted from the editor perspective. This document owns them as consumer-neutral contracts; [009-ox-mf2-phase-3d-lsp-editor-design.md](./009-ox-mf2-phase-3d-lsp-editor-design.md) retains only editor-specific behavior over the same contracts. Resource catalog support is a layered milestone on top of the Phase 3 products, not itself a numbered Phase 3 product phase.

## Terminology

ox-mf2 documents reserve encode and decode for Binary AST snapshot encoding and decoding, defined by [003-ox-mf2-phase-2-binary-ast-snapshot-design.md](./003-ox-mf2-phase-2-binary-ast-snapshot-design.md). To keep those words unambiguous, this layer names its string conversions differently:

- **raw text**: bytes as they appear in the host file, addressed by raw host value spans.
- **message text**: the exact MF2 message string that an i18n runtime would receive from the host format; the input to message-level core APIs.
- **unescaping**: the host-format-specific conversion from raw text to message text. It covers string escapes, quoting and scalar styles, XML entities, and CDATA sections.
- **re-escaping**: the write-back conversion from formatted message text to replacement raw text.

LSP position encodings in [009-ox-mf2-phase-3d-lsp-editor-design.md](./009-ox-mf2-phase-3d-lsp-editor-design.md) are protocol terminology, unrelated to either conversion.

## Goals

- Define one consumer-neutral message entry model and host format adapter contract shared by CLI formatting, CLI linting, and editor adapters.
- Fix one shared implementation home for host format classification, span-preserving parsing, extraction, offset mapping, and write-back re-escaping.
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
- MF2 message text
- a message-to-raw offset map
- a read-only marker for values that cannot be safely re-escaped

Consumer-neutral invariants:

- Extraction returns entries ordered by raw span start. Raw value spans of different entries must not overlap.
- Message text must be the exact string value that an i18n runtime would receive from the host format. Adapters must not trim, normalize, or append to message text; per the [Phase 3B file framing contract](./007-ox-mf2-phase-3b-formatter-design.md#file-framing), message-level core APIs never receive an injected final newline or lose message-leading content.
- Host constructs whose unescaped value cannot be represented as well-formed UTF-8, such as JSON escape sequences that denote unpaired surrogates, are excluded from extraction until the parser-level source-text direction noted in [002-ox-mf2-phase-1-rust-parser-design.md](./002-ox-mf2-phase-1-rust-parser-design.md) supports them.

Standalone `.mf2` files can reuse this model's shape as a degenerate single-entry host document, but their map is not unconditionally the identity: the editor adapter applies the Phase 3B [File Framing](./007-ox-mf2-phase-3b-formatter-design.md#file-framing) read contract and retains any removed BOM and trailing newline in a framing-aware document map. That uniformity is an editor-workflow concern owned by [009-ox-mf2-phase-3d-lsp-editor-design.md](./009-ox-mf2-phase-3d-lsp-editor-design.md). CLI formatting and linting keep their existing direct `.mf2` file paths unchanged; resource workflows neither apply standalone file framing to embedded messages nor reroute standalone message files through catalog extraction.

### Rust Types and Mapping API

The workspace-internal Rust contract uses resource-owned UTF-8 byte span and entry identity types. `crates/intlify_resource` does not reuse `ox_mf2_parser::Span`, because the resource crate must not depend on the parser crate even though both span types use the same half-open `u32` representation.

```rust
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Utf8ByteSpan {
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntryKey(String);

#[derive(Debug, Clone)]
pub struct MessageEntry {
    pub key: EntryKey,
    pub display_key: Option<String>,
    pub raw_value_span: Utf8ByteSpan,
    pub message_text: String,
    pub offset_map: MessageOffsetMap,
    pub read_only: bool,
}

#[derive(Debug, Clone)]
pub struct MessageOffsetMap {
    // Private, validated representation.
}
```

- `EntryKey` provides construction from the adapter's serialized structural identity and read-only string access; consumers do not mutate its representation. The empty string remains valid because it is the RFC 6901 identity of a JSON document-root value.
- `message_text` is an owned `String`. Extraction artifacts must be cacheable and movable into per-entry parallel work without borrowing from a self-referential host-document object.
- `raw_value_span` and every raw span inside the map use absolute host-document UTF-8 byte offsets. Message spans are relative to `message_text`.
- The map's crate-private builder uses an immutable segment sequence with `Identity`, `Unescape`, and `RawOnly` segment kinds. It validates ordered, non-overlapping raw spans; complete ordered coverage of message bytes; containment within `raw_value_span`; segment-kind length rules; and UTF-8 boundaries. Invalid adapter-produced maps are internal invariant failures.
- Consumers map core ranges through `MessageOffsetMap::map_span(Utf8ByteSpan) -> Result<Utf8ByteSpan, OffsetMapError>` rather than inspecting segments. Non-empty ranges apply the escape-boundary and raw-only-gap rules above. An empty span maps to the host position before the next message byte, or before trailing raw-only syntax when it is at message end.
- Inputs whose host or message text cannot be addressed by `u32` UTF-8 byte offsets fail through the shared input-size operational-error path before entries are constructed.

No stable binding-facing resource entry or offset-map type ships in the initial milestone, because no resource N-API or WASM package is planned. A future concrete non-Rust consumer must receive a separately designed, camel-cased, read-only DTO derived from these Rust types rather than exposing the internal segment representation as an ABI contract.

## Host Format Adapter Contract

A host format adapter is the format-specific component that classifies one host format, parses it, maps it into message entries, and re-escapes formatted message text back into host raw text.

### Extraction

- Host parsing must be span-preserving. Extraction needs the raw source span of every message value, so value-only deserialization in the plain `serde` style is not sufficient.
- Extraction produces the ordered entry list defined by the message entry model.

### Entry Identity

- The entry key is a serialized, structurally unambiguous identity. JSON adapters use RFC 6901 JSON Pointer, so the literal catalog key `"a.b"` and the nested path `a` → `b` remain distinct even when a runtime flattens both to `a.b`. YAML adapters use a typed structural path that distinguishes mapping keys from sequence indices and preserves each scalar key's resolved tag and value; entry paths containing complex mapping keys are excluded initially. A YAML adapter may use JSON Pointer serialization only for JSON-compatible, string-keyed paths. Thus a string key `"1"`, an integer key `1`, and sequence index `1` cannot collide.
- The entry key is an identity, not a display string. Adapters may additionally expose a runtime-style display key, such as dot-joined nesting, for UI and reporting purposes, but identity comparisons use the structural key.
- When a host document contains duplicate keys, each raw occurrence is a separate entry and the entry key carries an occurrence discriminator. Reporting duplicate catalog keys as a problem is future catalog-level linting, not an extraction failure.

### Message-to-Raw Offset Mapping

Each entry carries a monotonic offset map that aligns message-local UTF-8 byte ranges of the message text with raw host UTF-8 byte ranges inside the raw value span.

- Runs where message text bytes equal raw bytes share one identity segment.
- Each atomic unescaping unit — a single host escape, multiple escapes that jointly unescape to one scalar such as a JSON UTF-16 surrogate pair, or an XML entity — is one segment mapping the message text bytes to the full raw escape range.
- Raw-only syntax such as string quotes and CDATA delimiters remains inside the raw value span used for replacement but consumes no message text bytes. Offset maps retain these boundary gaps so diagnostic ranges do not consume delimiters. When the entry shape is reused by the standalone editor adapter, removed file framing is represented by the same kind of raw-only boundary gap.
- A message-local position that falls inside the output of one escape maps to the raw escape start when it is a range start and to the raw escape end when it is a range end, so mapped ranges never split a host escape sequence.

### Write-Back Re-Escaping

- Re-escaping must be value-identical: unescaping the re-escaped text must produce exactly the formatted message text.
- Re-escaping should preserve the host value style, such as quoting or scalar style, when that style can represent the formatted text. Otherwise the adapter may switch to a style that can, while keeping the host document semantically identical outside the message value.
- When the adapter cannot guarantee value-identical re-escaping for a host construct, extraction marks the entry read-only. Read-only entries still produce diagnostics; formatting skips them.

### Host Parse Failures

When the host document itself cannot be parsed, extraction fails for the whole document. Host syntax errors are never translated into MF2 diagnostics; host-format tooling owns them. Presentation of an extraction failure is surface-specific: editor adapters retain previously published diagnostics as defined in [009-ox-mf2-phase-3d-lsp-editor-design.md](./009-ox-mf2-phase-3d-lsp-editor-design.md), while the CLI reports a target-local operational error as defined in the CLI resource workflow below.

## Host Format Registry and Catalog Detection

Consumers resolve each input file or document to at most one host format adapter through a host format registry.

- Catalog host formats are strictly opt-in. Arbitrary JSON, YAML, or XML files must not be assumed to contain MF2 messages; a false positive reports wrong diagnostics on unrelated files and, worse, exposes them to formatting write-back. That is worse than requiring configuration.
- Project-configured catalog opt-in comes from the `resources` section of the unified project config defined below. Editor adapters may additionally layer explicitly editor-only ad-hoc settings over that result. Filename conventions never imply catalog membership: names such as `*.mf2.json` may be documented as recommended `include` patterns, but they are not automatic opt-in defaults in any tier.
- CLI resource workflows and editor adapters must resolve project-configured membership and host format identically. An editor-only ad-hoc entry is intentionally absent from CLI and CI processing until it is persisted in project configuration; editor integrations should distinguish that state rather than presenting it as CI coverage.
- If overlapping catalog configuration maps one file to multiple host formats, that is a configuration validation failure and therefore an operational error, not silent precedence.

Within an opted-in catalog document, the default extraction scope is every string leaf value, including string elements of arrays, whose path and unescaped value satisfy the message-entry representability invariants above. String leaves excluded by those invariants, such as values below an initially unsupported YAML complex-key path or values that cannot be represented as well-formed UTF-8, are not entries. Non-string scalars are never message entries. Narrowing extraction with key selectors, such as include/exclude paths, is future configuration detail; the entry model does not change when selectors arrive.

Locale identity is not required by per-entry formatting and linting, because parsing, formatting, and linting one MF2 message do not depend on locale. The future locale binding defined in [Locale Binding for Future Catalog Checks](#locale-binding-for-future-catalog-checks) associates locale metadata with entries for catalog-level and cross-locale lint rules without changing message extraction or message-level core APIs.

## Host Format Tiers

Host formats are introduced in tiers behind the same adapter contract. Tiers order implementation effort and ecosystem demand; they do not change the contract.

| Tier | Host formats | Status |
| --- | --- | --- |
| 1 | JSON catalogs (`.json`) | initial resource milestone |
| 2 | Vue SFC `<i18n>` custom blocks through adapter composition | deferred follow-up |
| 3 | YAML catalogs (`.yaml`, `.yml`); JSONC and JSON5 catalogs; XLIFF 1.2 / 2.x; other interchange formats such as ARB, gettext PO, and Java properties | deferred follow-up, demand-driven |

Only Tier 1 is implemented in the initial resource milestone. Tier 2 and Tier 3 formats are deferred and tracked in [Deferred Follow-Up Notes](#deferred-follow-up-notes). Their subsections below record contract-level design notes so the shared contracts stay tier-proof; they are not initial implementation commitments.

### JSON Catalogs

JSON catalogs unescape RFC 8259 string escapes. The JSON Pointer entry identity rules and the unpaired-surrogate exclusion above apply. Formatted multi-line MF2 output, such as a formatted matcher, re-escapes line breaks as `\n` escapes inside the single-line JSON string value.

### Vue SFC `<i18n>` Custom Blocks

Single-file-component `<i18n>` blocks embed a catalog region inside a `.vue` host document. This is adapter composition: an outer adapter locates the block region and its declared language, an inner catalog adapter runs over the region text, and the offset maps compose into document coordinates. The entry model is closed under this composition; no new consumer-facing behavior is required.

Composition consumes only the inner adapters that have shipped when this tier lands: blocks whose language is JSON, the `<i18n>` block default, compose the Tier 1 JSON adapter, while `lang="yaml"` or `lang="json5"` blocks require their Tier 3 adapters and stay out of extraction until those land.

`crates/intlify_resource` owns the built-in Vue SFC outer adapter and the composition with inner catalog adapters. Consumers pass the complete `.vue` host document to the shared registry and must not locate `<i18n>` blocks independently. The outer adapter preserves the block content spans and declared languages, selects a shipped inner adapter, and lifts the inner entries, offset maps, and write-back replacements into absolute `.vue` document coordinates. This keeps classification, extraction, and write-back identical across CLI and editor consumers.

Ownership of the outer adapter does not require the resource crate to implement Vue syntax parsing itself. It may use a dedicated parser dependency or a private workspace helper, provided that the helper exposes the span-preserving SFC syntax information required by the adapter and does not become a second consumer-facing extraction path.

### YAML Catalogs

YAML catalogs resolve entries from tag-resolved string scalars. Plain, single-quoted, and double-quoted scalars are read-write when this tier lands. Literal and folded block scalars produce correct offset maps for diagnostics but remain read-only for the entire initial YAML adapter milestone. Write support for block scalars is a separate follow-up milestone rather than a condition for shipping the Tier 3 YAML adapter. Anchored values produce one entry at the anchor definition site; alias nodes and merge-key expansions do not produce additional entries.

Block scalar entries graduate to writable only after the adapter supports value-identical re-escaping for both literal and folded styles; indentation indicators `1` through `9`; strip, clip, and keep chomping; empty and leading-empty content; trailing line breaks and trailing empty lines; and more-indented lines. Fixtures must reparse every generated candidate and compare its resolved string value with the formatted message text, and must prove formatting idempotency. Until those requirements land, linting reports mapped diagnostics normally, while fmt write mode skips block scalar entries and `--check` does not count them as different.

Once that follow-up milestone makes block scalars writable, re-escaping uses a fixed fallback order. The adapter first preserves the original literal or folded style, adjusting indentation and chomping indicators as necessary, when reparsing produces the exact formatted message text. If folded style cannot represent that value identically, it switches to literal style. If literal style also cannot represent the value safely, it switches to double-quoted style. Every candidate is reparsed and accepted only on exact value equality; failure of all candidates is a `resource_write_back_failed` internal error and produces no file write.

### JSONC and JSON5 Catalogs

JSONC adds comments and trailing commas over the JSON adapter. JSON5 adds further string syntax, such as single-quoted strings and line continuations, that changes unescaping and re-escaping but not the entry model. These are ecosystem-relevant catalog syntaxes and should reuse the JSON adapter structure.

### XLIFF

XLIFF is an XML host format: XLIFF 1.2 stores message text in `<trans-unit>` `<source>`/`<target>` elements and XLIFF 2.x in `<unit>`/`<segment>` `<source>`/`<target>` elements. Entry keys serialize the file/group/unit and, where present, segment identity path. Unescaping covers XML entities and CDATA sections and must respect `xml:space` handling.

The initial XLIFF adapter extracts only elements whose content is plain XML character data, including character data represented through entity references or CDATA sections. A candidate `<source>` or `<target>` that contains any inline child element, including character, standalone, paired, spanning, marker, or annotation elements, is excluded from extraction. The generic resource adapter does not replace inline elements with sentinel placeholders or translate them into MF2 markup, because doing so would require format- and project-specific rules for identity, pairing, nesting, movement, deletion, and round-trip preservation.

Inline-element support, if a concrete project or TMS workflow requires it later, is introduced as a separate explicit XLIFF profile. That profile must define the XLIFF-to-MF2 representation, protected-edit semantics, validation rules, and lossless round-trip behavior before entries containing inline elements become extractable. Whether MF2 message text is carried inside XLIFF at all remains a project or TMS convention; the adapter processes only catalogs that configuration explicitly opts in.

## Catalog Configuration

Catalog opt-in is a new `resources` section of the unified project config, alongside the existing `fmt` and `lint` sections, because catalog membership is consumed by formatting, linting, and editor surfaces alike. The section follows the Phase 3A configuration contract in [006-ox-mf2-phase-3a-tooling-foundation-design.md](./006-ox-mf2-phase-3a-tooling-foundation-design.md): root-only discovery, strict unknown-field validation, Rust config model as the source of truth, and schema generation into the committed `config.schema.json` artifact.

The initial section shape is:

```jsonc
{
  "resources": {
    "catalogs": [
      {
        // Required. Files covered by this catalog definition.
        "include": ["locales/**/*.json"],
        // Optional. Excluded files within include.
        "exclude": ["locales/**/generated.*.json"],
        // Optional. Single per-file classification override; when omitted,
        // each matched file's format defaults from its own extension.
        "format": "json"
      }
    ]
  }
}
```

The normalized Rust configuration types are:

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResourcesConfig {
    pub catalogs: Vec<CatalogConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogConfig {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub format: Option<HostFormat>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostFormat {
    Json,
}
```

The root `resources` section and its `catalogs` field are optional and normalize to `ResourcesConfig { catalogs: [] }` when absent. Each catalog definition requires a non-empty `include` array. `exclude` is optional and defaults to an empty array; `format` is optional and defaults to extension-based classification. The initial `HostFormat` schema accepts only `"json"` and expands as deferred adapters ship.

A project that manages MF2 messages in several host formats opts them all in at once; classification is per file, so no `format` field is needed for mixed sets:

```jsonc
{
  "resources": {
    "catalogs": [
      // Mixed-format catalogs: format omitted, classified per file extension.
      { "include": ["locales/**/*.json", "locales/**/*.yaml"] },
      // SFC catalogs: the file classifies as the SFC host format; embedded
      // block languages come from each <i18n> block's lang attribute.
      { "include": ["src/**/*.vue"] }
    ]
  }
}
```

Opted-in files whose format tier has not shipped yet follow the `resource_format_unsupported` rule below, so a project widens its include globs as deferred tiers land.

- `include` globs are required per catalog definition and must contain at least one valid string pattern; `exclude` is optional. Glob semantics follow the shared CLI discovery contract in [008-ox-mf2-phase-3c-linter-design.md](./008-ox-mf2-phase-3c-linter-design.md#file-discovery-and-shared-cli-contract). Every pattern is resolved relative to `projectRoot`, and `exclude` removes matches only from the `include` set of the same catalog definition.
- `format` is a single-valued, per-file classification override, because the registry resolves every file to exactly one host format adapter. It is not a list of enabled formats: list-like values such as `"json,yaml"` or arrays are invalid.
- One catalog definition may cover files of multiple host formats. When `format` is omitted, each matched file's format defaults from its own extension, so a mixed include set needs no `format` field at all. An explicit `format` override applies to every file its definition matches and exists for extension-ambiguous cases; a heterogeneous file set that needs different overrides is expressed as multiple `catalogs` definitions. Content sniffing is never used for classification.
- For Vue SFC catalogs, `format` identifies only the outer SFC host format. The embedded block language comes from each `<i18n>` block's `lang` attribute inside the file, not from configuration.
- A file matched by multiple definitions that resolve to the same host format is de-duplicated and processed once. A file assigned to different formats by overlapping definitions is a `config_validation_failed` error with a JSON pointer to the later conflicting catalog definition; no definition silently takes precedence.
- Explicit `format` values are schema-validated against the host formats shipped in the current release. Opted-in files whose derived format has no shipped adapter, such as `.yaml` catalogs before their tier lands, are reported by the CLI as target-local operational errors with `kind: "input"` and `code: "resource_format_unsupported"` rather than being silently skipped; editor adapters do not process such documents.
- Key selectors are not accepted as reserved, nullable, or placeholder fields in the initial schema; their future names and semantics are designed with their workflow. The future locale binding has the shape fixed below, but the `locale` field likewise remains invalid until catalog-level linting that consumes it is implemented.
- Per-catalog formatter and linter overrides are likewise absent rather than reserved in the initial schema. Uniform project-level `fmt` and `lint` options apply to every catalog entry.
- An explicit `--config <path>` follows the Phase 3A contract: it replaces root config discovery without merging with a discovered config and does not change `projectRoot`. Relative `--config` paths resolve from the process working directory, while resource `include` and `exclude` globs remain relative to `projectRoot`, including when the explicit config file lives elsewhere.
- The field names and initial semantics in this section are fixed by this document and must be represented directly in the Rust config model and generated unified config schema.

An empty or absent `resources` section means no catalog files are processed. Standalone `.mf2` workflows are unaffected by the `resources` section.

## Implementation Home and Package Boundaries

The shared layer lives in a workspace-internal `crates/intlify_resource` crate, following the crate conventions of the formatter and linter products: not a crates.io deliverable, `publish = false`, consumed by `crates/intlify_cli`.

- `crates/intlify_resource` owns the host format registry, span-preserving host parsers, extraction, entry identity, offset mapping, write-back re-escaping, and the resolved catalog configuration model for the `resources` section. This includes the built-in Vue SFC outer adapter and its composition with inner catalog adapters; a private parser helper may supply SFC syntax spans without owning the extraction workflow.
- The crate does not depend on `ox_mf2_parser`, `intlify_format`, or `intlify_lint`. Consumers compose extraction with message-level core calls. This keeps the dependency direction acyclic and the layer reusable by any consumer.
- Concrete host parser dependencies are implementation choices, constrained by the contract: raw value spans, byte-exact unescaping, and value-identical re-escaping, all locked by fixtures.
- The crate is the only planned deliverable for this layer. No resource N-API or WASM binding packages are planned at this time; reconsidering package distribution is a deferred follow-up that requires a concrete non-Rust consumer. Parser, formatter, and linter binding packages do not absorb resource APIs.

## CLI Resource Workflow

### Input Selection

A file enters resource processing only when it is both selected by CLI operands or globs and opted-in by the resolved `resources` configuration.

- The supported CLI input set extends from direct `.mf2` files to opted-in catalog files. The extension alone does not make a file a catalog; membership requires configuration. This extends the supported-input membership promised by the shared discovery contract in [008-ox-mf2-phase-3c-linter-design.md](./008-ox-mf2-phase-3c-linter-design.md#file-discovery-and-shared-cli-contract) without changing that contract's shape.
- Glob expansion intersects the supported input set: `.mf2` files plus opted-in catalog files. Non-opted-in JSON/YAML files matched by a glob are not resource inputs and are skipped by discovery.
- An explicit file operand that is neither a `.mf2` file nor an opted-in catalog follows the existing unsupported-input handling of the shared discovery contract.

### Host Parse Failures

When a catalog file cannot be parsed as its host format, the CLI reports a target-local operational error in `results[].errors` using the shared operational error shape: `kind: "input"`, `code: "resource_parse_failed"`, with `details.line` and `details.column` when the host parser reports them. The file contributes no diagnostics, is never written by fmt, and other selected files are still processed. Operational errors drive `summary.status: "error"` and exit code `2` per the Phase 3A output contract.

### Determinism and Parallelism

File results are reported in stable normalized path order; entries within a file are reported in raw span order. The initial resource implementation does not introduce intra-file parallelism and follows the consumer CLI's existing file execution policy. Future concurrency first parallelizes across files. The resource crate itself does not create a thread pool; scheduling belongs to the CLI consumer so all work shares one bounded global pool.

Intra-file execution is reconsidered only after file-level parallelism exists and benchmarks show that a single very large catalog leaves that pool materially underutilized. The future unit is a contiguous chunk of entries in raw span order, not one task per entry and not a host-format syntax subtree. Chunking amortizes scheduling overhead for small messages, stays host-format-neutral, and lets a single catalog share the same worker pool without nested parallelism or oversubscription.

Chunk results carry their original entry indices and are reassembled in raw span order before reporting or write-back. Formatter replacement composition remains a single deterministic file-level step after all chunks succeed. If concurrent entry work produces more than one operational error, the error for the lowest raw-order entry is selected regardless of task completion order; the existing no-partial-result and no-partial-write rules still apply. Exact chunk size, minimum entry count or measured-work threshold, and worker scheduling strategy are benchmark-tuned internal details rather than config fields or CLI compatibility surfaces.

Benchmarks report extraction, re-escaping, core calls, scheduling overhead, and file I/O as separate phases. Intra-file parallelism may land only when representative large-catalog benchmarks demonstrate a repeatable improvement after scheduling and ordered aggregation costs are included.

### Catalog JSON Result Layout

The JSON reporter keeps one `results[]` item per selected host file and nests complete entry results under catalog targets. A standalone message result requires `diagnostics` and omits `entries`, preserving the Phase 3B and Phase 3C shape. A catalog result requires `entries` and omits the file-level `diagnostics` field; the committed JSON schemas express these as mutually exclusive result variants. Diagnostic objects are not duplicated between file and entry levels and do not gain resource-specific optional fields.

On successful extraction and complete per-entry processing, `entries` contains every extracted message entry in raw span order, including clean, unchanged, diagnostic-bearing, and read-only entries. `displayKey` is omitted when an adapter has no display identity. The linter variant is:

```json
{
  "path": "locales/en.json",
  "status": "problems",
  "entries": [
    {
      "key": "/greeting",
      "displayKey": "greeting",
      "status": "problems",
      "diagnostics": []
    }
  ],
  "errors": []
}
```

Lint entry `status` is `"clean"` or `"problems"` and is computed from the full entry diagnostic set before reporter filtering, following the file-level `--quiet` rule. The formatter variant adds `changed` to both the file result and every entry result and uses the formatter status vocabulary at entry scope:

```json
{
  "path": "locales/en.json",
  "status": "formatted",
  "changed": true,
  "entries": [
    {
      "key": "/greeting",
      "displayKey": "greeting",
      "status": "formatted",
      "changed": true,
      "readOnly": false,
      "diagnostics": []
    }
  ],
  "errors": []
}
```

Every formatter entry result has a `readOnly` boolean. For a completed writable entry, `status` is `"formatted"`, `"would_format"`, `"unchanged"`, or `"diagnostic"` with the same mode-dependent meaning as the corresponding file status. A read-only entry with no parser diagnostics uses `status: "skipped"`, `changed: false`, and `readOnly: true`; `"unchanged"` is reserved for an entry whose formatting was actually evaluated. If a read-only entry has parser diagnostics, `"diagnostic"` takes precedence while `readOnly` remains `true`, so its diagnostics are not hidden.

The complete JSON entry array is the only informational reporting for a read-only skip. Human write, check, and list-different output does not mention it on stdout or stderr; no read-only counter is added to the summary; and the skip does not affect file status or the process exit code. In particular, a catalog containing only valid read-only entries is `"unchanged"` and does not appear in the path-only `fmt --check` output.

Entry results do not contain an `errors` array. A host-level operational error stays in the file result's `errors`. A per-entry operational error also makes the catalog result incomplete under the failure rules below: the file has `status: "error"`, its `entries` array is empty, and the file-level error carries `details.entryKey` when known. This avoids presenting partial entry results as a complete catalog result.

Every diagnostic `span`, `location`, and `labels[].span` in a catalog entry result uses mapped host-document coordinates. `span` and label spans are absolute UTF-8 byte ranges in the complete host document, and `location` is derived from that document. The JSON result does not expose a parallel `messageSpan` or message-local location. Message-local spans remain available internally in the core diagnostic result before `MessageOffsetMap` mapping; keeping one public coordinate space prevents consumers from accidentally applying a message-local range to the host file. A future concrete consumer that needs both coordinate spaces requires an explicit schema addition rather than overloading `span`.

## Catalog Linting

Catalog linting runs the message-level linter per entry and aggregates per file.

- Each entry's message text goes through `lintMessage(source, options)` with the same resolved lint configuration used for `.mf2` files. The strict `parser -> semantic -> rules` pipeline applies per entry.
- Entries are independent: parser diagnostics in one entry never suppress semantic validation, linting, or reporting for other entries.
- Entry diagnostics are reported exclusively with host-file coordinates in JSON: the mapped host UTF-8 byte span and derived line/column, produced through the entry offset map. Each entry-level result carries its entry key; a display key may accompany it for human-readable output. Message-local spans remain internal and are not serialized alongside the mapped spans.
- If any per-entry linter call returns an operational error instead of a complete diagnostic result, the catalog target follows the existing target-level linter error contract: it reports `status: "error"`, an empty `entries` array, and the error in `results[].errors`, with `details.entryKey` when the entry is known. Diagnostics already collected from other entries in that catalog are discarded because the target result is incomplete; other selected files still continue.
- Entry-level results use the nested catalog JSON result layout above as an extension of the linter JSON schema, inside the existing envelope, summary, and count conventions of [008-ox-mf2-phase-3c-linter-design.md](./008-ox-mf2-phase-3c-linter-design.md). Diagnostic counts and `--max-warnings` include `entries[].diagnostics`.
- Lint options apply uniformly; there are no per-catalog rule overrides initially. The evidence-gated reconsideration policy below applies to both lint and formatter options.

## Catalog Formatting

Catalog formatting runs the message-level formatter per entry and composes write-back edits per file.

- Each writable entry's message text goes through `formatMessage(source, options?)` with the same resolved fmt configuration used for `.mf2` files.
- Write mode: for every writable, syntactically valid entry whose formatted output differs, the adapter re-escapes the formatted text and replaces the entry's raw value span. Edits are applied in descending raw-span offset order and the file is written once, using the Phase 3B CLI's file-I/O and operational-error conventions but not its standalone `.mf2` file framing. All bytes outside replaced value spans remain byte-identical, including any host-file BOM and trailing line ending: no key reordering, no indentation or quoting changes outside message values, and no host layout normalization.
- Entries whose extracted text has parser diagnostics are skipped per entry and reported with the same strict invalid-syntax semantics that [007-ox-mf2-phase-3b-formatter-design.md](./007-ox-mf2-phase-3b-formatter-design.md) fixes for invalid standalone files, scoped to the entry. Other entries still format. One broken message must not block formatting of a large catalog; this mirrors the editor behavior in [009-ox-mf2-phase-3d-lsp-editor-design.md](./009-ox-mf2-phase-3d-lsp-editor-design.md).
- For catalog targets, the formatting unit affected by parser diagnostics is the entry, not the whole host file. A diagnostic-bearing entry is never replaced, but the same catalog file may still be written once when other entries produce valid changes. This is the resource-layer specialization of Phase 3B's rule that parser diagnostics do not modify the affected formatting unit.
- Read-only entries are excluded from formatting by definition: write mode skips them and `--check` does not count them as changed. Complete JSON output records each valid read-only entry as `status: "skipped"`, `changed: false`, and `readOnly: true`, but human output, summary counts, file status, and the process exit code do not report or change because of the skip.
- `--check` and `--list-different` report a catalog file as different when at least one writable, syntactically valid entry differs after formatting.
- If a per-entry formatter call or writable-entry re-escaping step returns an operational error, the catalog produces no write and no partial formatting result. Its file result uses `status: "error"`, `changed: false`, an empty `entries` array, and the operational error in `results[].errors`, with `details.entryKey` when known. A re-escaping failure for an entry previously classified as writable is an `internal_error` with `details.reason: "resource_write_back_failed"`. Other selected files still continue.
- Idempotency: the write-back round-trip law plus message-level formatter idempotency imply that formatting an already formatted catalog produces no writes. Fixtures lock this.

A catalog may contain both valid changed entries and parser-diagnostic entries. The file-level JSON result keeps the Phase 3B status enum and applies this precedence after successful, non-operational processing:

1. In write mode, any composed byte change uses `status: "formatted"` and `changed: true`, even when `diagnostics` is non-empty; the file is written once and the command still exits with `1` because diagnostics remain.
2. In check mode, any composed byte difference uses `status: "would_format"` and `changed: true`, even when `diagnostics` is non-empty; no file is written.
3. When no writable entry differs but one or more entries have parser diagnostics, the result uses `status: "diagnostic"` and `changed: false`.
4. When no writable entry differs and no diagnostics or operational errors exist, the result uses `status: "unchanged"` and `changed: false`.

Accordingly, `formattedFiles` or `differentFiles` and `diagnosticFiles` may count the same catalog file; these summary counts are intentionally not a partition of `matchedFiles`. `diagnosticCount` includes all mapped entry diagnostics, and diagnostics or check differences keep `summary.status: "failure"` and exit code `1` unless an operational error raises the result to exit code `2`.

## Editor Consumption

For project-configured catalog documents, editor adapters consume the same registry, extraction, offset maps, and write-back re-escaping through this shared layer, so membership, host format classification, and mapping are identical in CI and in the editor. An editor-only ad-hoc opt-in still uses the same host adapter contract, but its membership is intentionally editor-local until persisted in project configuration. Standalone `.mf2` documents continue through the editor-owned adapter defined by [009-ox-mf2-phase-3d-lsp-editor-design.md](./009-ox-mf2-phase-3d-lsp-editor-design.md), not catalog extraction. Surface behavior stays intentionally different: editors convert mapped spans to editor position encodings, publish per document, retain previous diagnostics across transient host parse failures, and no-op on stale document versions, while the CLI reports host parse failures as target-local operational errors and writes files directly.

## Catalog-Level Checks

Catalog-level and cross-locale checks — duplicate catalog keys, key parity across locales, missing or unused translations — operate across entries and files above this layer. They require locale binding in catalog configuration and their rule identities, presets, and severities belong to the linter product design track. They are future work, and they must flow through the same entry model rather than introducing a second extraction path.

## Locale Binding for Future Catalog Checks

When catalog-level linting first consumes locale identity, `CatalogConfig` gains an optional, discriminated `locale` object. It does not accept a string shorthand, and locale capture is not embedded into `include` glob syntax. The two initial binding variants are a path capture:

```jsonc
{
  "include": ["locales/*.json"],
  "locale": {
    "from": "path",
    "pattern": "locales/{locale}.json"
  }
}
```

and a fixed locale:

```jsonc
{
  "include": ["vendor/english/*.json"],
  "locale": {
    "from": "fixed",
    "value": "en"
  }
}
```

The normalized Rust shape is a tagged enum rather than optional fields whose valid combinations must be inferred:

```rust
#[serde(tag = "from", rename_all = "snake_case")]
pub enum LocaleBindingConfig {
    Path { pattern: String },
    Fixed { value: String },
}
```

`locale` remains absent from the initial resource schema and Rust `CatalogConfig`; the field and enum land together with their first catalog-level consumer. Once present, strict unknown-field validation rejects unknown binding variants, a string `locale` value, and variant-inappropriate fields. A future host format with intrinsic locale metadata may add a `from: "host"` variant after its extraction and conflict semantics are designed; that value is not pre-accepted by the first locale-aware schema.

For `from: "path"`, `pattern` is matched against the slash-normalized, `projectRoot`-relative file path and must contain exactly one literal `{locale}` capture token. The token captures one or more characters without crossing a `/` boundary; it may have literal prefix or suffix text in the same segment, as in `{locale}.json`. To keep capture identity unambiguous, the segment containing `{locale}` cannot contain another variable-width glob construct. Other path segments may use the shared glob syntax. Invalid capture-pattern structure is a configuration validation failure, and a selected catalog path must have exactly one full-pattern match when a locale-aware workflow resolves it.

Both `path` captures and `fixed.value` must resolve to a non-empty string. The resource layer preserves that string byte-for-byte: it does not trim whitespace, change case, replace `_` with `-`, validate BCP 47 or Unicode locale syntax, or canonicalize deprecated subtags. Locale identity comparison is exact and case-sensitive. Standards conformance and canonical-spelling advice belong to future configurable linter rules, so project- or runtime-specific locale identifiers remain representable without changing resource extraction.

The resource layer resolves locale identity as consumer-neutral entry metadata. The future catalog linter groups that metadata with entry identity and a comparison scope, then owns rule definitions, severity, and reporting. Locale binding does not select a runtime locale, define locale fallback, change message text, or enable rules by itself.

Each locale-bound `CatalogConfig` is one comparison scope by default. All files matched by that definition are therefore treated as parts of the same logical message collection, and cross-locale identity is the tuple of that implicit scope, the exact resolved locale string, and `EntryKey`. Projects with independent message namespaces split them into separate catalog definitions so identical entry keys do not create false cross-file relationships.

When multiple catalog definitions must form one comparison scope, such as separate fixed-locale definitions, they opt into the same non-empty `group` string:

```jsonc
{
  "group": "vendor-messages",
  "include": ["vendor/en/*.json"],
  "locale": { "from": "fixed", "value": "en" }
}
```

```jsonc
{
  "group": "vendor-messages",
  "include": ["vendor/ja/*.json"],
  "locale": { "from": "fixed", "value": "ja" }
}
```

`group` is an exact, case-sensitive identifier and is not trimmed or normalized. Definitions with the same group share one comparison scope; an ungrouped definition never shares its implicit scope with another definition. The field lands together with locale-aware catalog linting and remains invalid in the initial resource schema. A catalog definition cannot specify `group` without `locale`, because no locale-indexed comparison scope would be constructible.

If one file is matched by multiple locale-aware definitions, de-duplication is valid only when they resolve to the same host format, exact locale identity, and comparison scope. Different locale or scope assignments are a configuration conflict rather than placing one physical entry into multiple comparison groups. This extends the existing overlapping-definition conflict rule once locale-aware configuration lands.

## Per-Catalog Option Override Reconsideration

Per-catalog formatter or linter option overrides have no scheduled tier and are reconsidered only when a concrete workflow cannot be expressed reasonably with uniform project options, catalog `include` and `exclude`, or separate CLI invocations using explicit config files. Generated or externally managed catalogs with demonstrably different policy are examples of evidence, not automatic justification for adding overrides.

No `fmt`, `lint`, `options`, or other placeholder field is reserved inside `CatalogConfig` before that evidence exists. A later additive design must define all of the following before changing the schema:

- the inheritance and merge rules from project-level formatter and linter options
- conflict handling when one file matches multiple catalog definitions with different resolved overrides
- identical option resolution for CLI and editor consumers
- configuration identity in formatter, linter, extraction, and editor cache keys and invalidation
- whether overrides apply to entry-level rules only or also to future catalog-level and cross-locale checks
- deterministic JSON and human reporting of the effective policy when a target fails

Until those requirements are designed together, catalog processing always uses the single resolved project-level formatter and linter configurations.

## Benchmarks

Resource benchmarks are phase-separated, following the Phase 3 benchmark conventions:

- `resource_extract`: host parse and entry extraction per format
- `resource_write_back`: write-back re-escaping and edit composition
- `fmt_catalog_check_e2e` and `fmt_catalog_write_e2e`
- `lint_catalog_e2e`
- large single-catalog sequential and candidate chunked execution, including scheduling and ordered-aggregation overhead before intra-file parallelism may graduate
- catalog-shaped cache scenarios from [ox-mf2-parse-artifact-cache.md](./ox-mf2-parse-artifact-cache.md), such as entries with unchanged message text across host file edits

Extraction and re-escaping costs must be reported separately from parser, semantic, rule, formatter, and file I/O costs.

## Validation

- Extraction fixtures per host format, landing with each format's tier. The Tier 1 JSON set covers string unescaping, JSON Pointer identity including `"a.b"` versus nested `a` → `b`, duplicate-key occurrence discriminators, array elements, and unpaired-surrogate exclusion. Deferred tiers add their own sets when they land, such as typed YAML path identity including string-key/integer-key/sequence-index collisions, YAML scalar styles, anchors and aliases, block scalar read-only marking, and complex-key exclusion.
- Round-trip tests: for writable constructs, extracting the re-escaped output yields exactly the formatted message text.
- Offset map fixtures: message-local spans map to expected host spans across single escapes, compound surrogate-pair escapes, and raw-only delimiters, including the escape-boundary rule.
- CLI end-to-end fixtures: lint and fmt write/check over catalog fixtures, including broken-entry and broken-host-file cases, mixed changed-plus-diagnostic formatter results and overlapping summary counts, per-entry operational failures with no partial file write, deterministic ordering, complete nested entry arrays, host-coordinate-only diagnostic JSON shapes, and JSON-only read-only skip reporting.
- Configuration fixtures: `resources` section validation, rejection of list-like `format` values, unshipped `format` values and `resource_format_unsupported` targets, overlapping-definition conflicts, and empty-section behavior. When locale-aware catalog linting lands, its fixtures additionally cover the tagged binding variants, capture-pattern validation, path matches, exact locale identity, rejection of string shorthand, implicit definition scopes, explicit group joining, and overlap conflicts across locale or scope assignments.
- Determinism tests for parallel runs, including stable entry ordering and lowest-raw-order operational-error selection when future chunked execution is enabled.

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

## Deferred Follow-Up Notes

The following items are intentionally not part of the initial resource milestone, but should remain visible for later work:

- Tier 2 Vue SFC `<i18n>` custom block support: `crates/intlify_resource` owns outer block extraction and adapter composition over the block region, per the tier notes above. Blocks whose declared language has no shipped inner adapter stay out of extraction.
- Tier 3 YAML catalog support (`.yaml`, `.yml`): typed structural path identity, scalar-style handling, anchors and aliases, and diagnostic mapping with block scalars kept read-only per the tier notes above. Writable block scalar re-escaping is a separate follow-up milestone gated by the listed round-trip and idempotency requirements.
- Tier 3 JSONC and JSON5 catalog support, reusing the JSON adapter structure.
- Tier 3 XLIFF 1.2 / 2.x support: XML entity and CDATA unescaping, `xml:space` handling, and plain-character-data extraction only. Inline-element support requires a separately designed, explicit XLIFF profile with protected-edit and lossless round-trip semantics.
- Other Tier 3 interchange formats such as ARB, gettext PO, and Java properties remain demand-driven candidates.
- Resource N-API and WASM binding packages are not planned at this time. Distribution stays the workspace-internal `crates/intlify_resource` crate; package distribution is reconsidered only if a concrete non-Rust consumer needs direct access to this layer.
- Key selectors, locale binding, and catalog-level checks remain future configuration and linting work tracked in the sections of this document. Locale binding uses the fixed discriminated object and comparison-scope rules above when that milestone lands; `locale` and `group` are not reserved in the initial schema.
- Per-catalog formatter and linter option overrides remain unscheduled and evidence-gated; no placeholder fields are accepted before the reconsideration requirements above are met.
- Intra-file parallelism remains disabled until file-level parallelism and representative benchmarks justify shared-pool, contiguous-entry chunking under the determinism rules above.

## Open Questions

No resource catalog adapter open questions remain at this design level. Deferred implementation work and evidence-gated reconsiderations are tracked in the sections above.
