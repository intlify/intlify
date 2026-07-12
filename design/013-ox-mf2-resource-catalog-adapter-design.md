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

- a stable concrete entry key
- a logical catalog key for future catalog-level grouping
- a raw host value span, as a host-document UTF-8 byte range
- MF2 message text
- a message-to-raw offset map
- a read-only marker for values that cannot be safely re-escaped

Consumer-neutral invariants:

- Extraction returns entries ordered by raw span start. Raw value spans of different entries must not overlap.
- Message text must be the exact string value that an i18n runtime would receive from the host format. Adapters must not trim, normalize, or append to message text; per the [Phase 3B file framing contract](./007-ox-mf2-phase-3b-formatter-design.md#file-framing), message-level core APIs never receive an injected final newline or lose message-leading content.
- A candidate host string whose unescaped value or structural identity cannot satisfy these invariants fails complete extraction with `resource_entry_unsupported`; it is never silently omitted. This includes JSON escape sequences that denote unpaired surrogates until the parser-level source-text direction noted in [002-ox-mf2-phase-1-rust-parser-design.md](./002-ox-mf2-phase-1-rust-parser-design.md) supports them.

Standalone `.mf2` files can reuse this model's shape as a degenerate single-entry host document. That entry uses the empty `StructuralPathKey`, `occurrence: 0`, and the empty `CatalogKey`; its document URI remains the external namespace for cache and diagnostic identity, and it never participates in catalog-level grouping. Its map is not unconditionally the identity: the editor adapter applies the Phase 3B [File Framing](./007-ox-mf2-phase-3b-formatter-design.md#file-framing) read contract and retains any removed BOM and trailing newline in a framing-aware document map. That uniformity is an editor-workflow concern owned by [009-ox-mf2-phase-3d-lsp-editor-design.md](./009-ox-mf2-phase-3d-lsp-editor-design.md). CLI formatting and linting keep their existing direct `.mf2` file paths unchanged; resource workflows neither apply standalone file framing to embedded messages nor reroute standalone message files through catalog extraction.

### Rust Types and Mapping API

The workspace-internal Rust contract uses resource-owned UTF-8 byte span and entry identity types. `crates/intlify_resource` does not reuse `ox_mf2_parser::Span`, because the resource crate must not depend on the parser crate even though both span types use the same half-open `u32` representation.

```rust
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Utf8ByteSpan {
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StructuralPathKey(String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CatalogKey(String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntryKey {
    structural_path: StructuralPathKey,
    occurrence: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntryHandle {
    // Private artifact identity token and u32 entry index.
}

#[derive(Debug, Clone)]
pub struct MessageEntry {
    handle: EntryHandle,
    key: EntryKey,
    catalog_key: CatalogKey,
    display_key: Option<String>,
    raw_value_span: Utf8ByteSpan,
    message_text: String,
    offset_map: MessageOffsetMap,
    read_only: bool,
}

#[derive(Debug, Clone)]
pub struct MessageOffsetMap {
    // Private, validated representation.
}
```

- `StructuralPathKey` provides construction from the adapter's serialized structural path and read-only string access; consumers do not mutate its representation. The empty string remains valid because it is the RFC 6901 identity of a JSON document-root value.
- `CatalogKey` is the logical message identity used for future cross-locale grouping. It has read-only string access and is distinct from concrete host identity even when its serialized value is equal. JSON and YAML copy the structural path value; composed Vue entries copy the inner catalog key; XLIFF removes the final source/target side segment so both locale variants share one logical key.
- `EntryKey` combines that structural path with a zero-based occurrence number. Its read-only accessors expose `structural_path()` and `occurrence()`. Equality, hashing, and ordering use both fields; no concatenated suffix representation is constructed internally or exposed publicly.
- `EntryHandle` is an opaque, artifact-local reference minted during extraction. Its private representation includes both an artifact identity token and a `u32` entry index, so a same-index handle from another artifact is rejected rather than addressing the wrong entry. It is not serialized, persisted, or used as catalog identity; consumers use it only with the `ExtractedCatalog` that returned the corresponding entry. The concrete token-generation mechanism is an internal implementation detail, but it must not reuse a live artifact identity.
- `message_text` is an owned `String`. Extraction artifacts must be cacheable and movable into per-entry parallel work without borrowing from a self-referential host-document object.
- `MessageEntry` fields are private and exposed through read-only accessors for its handle, concrete entry key, catalog key, optional display key, raw value span, message text, offset map, and read-only state. Consumers cannot mutate adapter-produced invariants or construct entries independently of an extraction artifact.
- `raw_value_span` and every raw span inside the map use absolute host-document UTF-8 byte offsets. Message spans are relative to `message_text`.
- The map's crate-private builder uses an immutable segment sequence with `Identity`, `Unescape`, and `RawOnly` segment kinds. It validates ordered, non-overlapping raw spans; complete ordered coverage of message bytes; containment within `raw_value_span`; segment-kind length rules; and UTF-8 boundaries. Invalid adapter-produced maps are internal invariant failures.
- Consumers map core ranges through `MessageOffsetMap::map_span(Utf8ByteSpan) -> Result<Utf8ByteSpan, OffsetMapError>` rather than inspecting segments. Non-empty ranges apply the escape-boundary and raw-only-gap rules above. An empty span maps to the host position before the next message byte, or before trailing raw-only syntax when it is at message end.
- Inputs whose host or message text cannot be addressed by `u32` UTF-8 byte offsets fail with `resource_limit_exceeded` before entries are constructed.

No stable binding-facing resource entry or offset-map type ships in the initial milestone, because no resource N-API or WASM package is planned. A future concrete non-Rust consumer must receive a separately designed, camel-cased, read-only DTO derived from these Rust types rather than exposing the internal segment representation as an ABI contract.

## Host Format Adapter Contract

A host format adapter is the format-specific component that classifies one host format, parses it, maps it into message entries, and re-escapes formatted message text back into host raw text.

### Rust Registry and Extraction Artifact API

The consumer-facing Rust boundary is an opaque extracted-document artifact rather than a public adapter plugin trait or a stateless pair of extraction and re-escaping functions. Its conceptual signatures are:

```rust
HostFormatRegistry::extract(
    &self,
    format: HostFormat,
    source: Arc<str>,
) -> Result<ExtractedCatalog, ResourceError>

ExtractedCatalog::source(&self) -> &str
ExtractedCatalog::entries(&self) -> &[MessageEntry]
ExtractedCatalog::reescape(
    &self,
    entry: EntryHandle,
    formatted_message: &str,
) -> Result<RawReplacement, ResourceError>
ExtractedCatalog::apply_and_validate(
    &self,
    replacements: &[RawReplacement],
) -> Result<String, ResourceError>

pub struct RawReplacement {
    entry: EntryHandle,
    span: Utf8ByteSpan,
    raw_text: String,
    expected_message: String,
}
```

`HostFormatRegistry` contains the built-in, per-release adapter set. Catalog membership and explicit-format resolution produce the `HostFormat` passed to `extract`; the registry then dispatches to the corresponding built-in implementation. Because third-party adapter plugins are a non-goal, concrete adapters and their dispatch trait or enum remain crate-private rather than becoming a public extensibility contract.

`ExtractedCatalog` owns the exact original host source as `Arc<str>`, the ordered `MessageEntry` values, and immutable crate-private adapter state. That state retains any parsed structure, raw spelling, scalar style, outer/inner adapter composition, or other format-specific information needed for mapping and write-back without exposing it through `MessageEntry`. The artifact contains no parser, formatter, or linter core result.

Artifact construction validates all entry spans before `extract` returns: every `raw_value_span` must be on UTF-8 boundaries, contained in the original source, ordered by increasing start offset, and non-overlapping with every other entry span. A violation aborts the complete extraction before any artifact or partial entry list reaches a consumer and maps to `internal_error` with `details.reason: "resource_adapter_invariant_failed"` and `details.phase: "extract"`. Host syntax errors remain `resource_parse_failed`; this invariant error indicates a built-in adapter defect, not invalid user input.

`reescape` accepts only a handle minted by that artifact and validates that it identifies one of its entries. It rejects read-only entries and returns no replacement on failure. A successful `RawReplacement` privately retains that handle and the expected formatted message. Its read-only `span()` is exactly the entry's `raw_value_span`, and `raw_text()` is the complete re-escaped host value for that span. Consumers cannot construct or mutate replacements independently of an artifact.

`apply_and_validate` accepts only replacements minted by the same artifact. As a defense-in-depth check, it rejects an invalid or foreign handle, a span that no longer equals its entry's raw value span, duplicate changes for one entry, and overlapping spans even though artifact construction and the private replacement constructors make overlap unreachable through the public API. It applies the validated replacements to the artifact's original source in descending raw-span order, then uses the same built-in adapter composition to parse and extract the complete candidate host document again.

Candidate validation requires both the re-extracted `EntryKey` sequence and `CatalogKey` sequence to equal their original sequences exactly. Each replaced entry's message text must equal its replacement's private expected message, while every unreplaced entry's message text must remain byte-identical to the original. Any parse, extraction, identity, or value mismatch fails the whole operation; no candidate source or partial replacement set is returned. A successful call returns the complete validated host source. CLI consumers write only that returned string. Editor consumers validate the full candidate for their one or more replacements, discard the candidate string, and then convert the already validated replacements into protocol edits.

The artifact and every adapter state reachable from it must be `Send + Sync`. Extraction completes all mutable host parsing before publishing the artifact; concurrent read access, offset mapping, and `reescape` calls do not mutate it. The source ownership and owned message strings make the artifact cacheable without self-referential borrows. `EntryHandle` has meaning only within its originating artifact, while stable cache and reporting identity continues to use `EntryKey`.

The exact `ResourceError` variants and their CLI operational-error mapping follow the resource error model below; this API does not expose CLI JSON error objects from the resource crate.

### Extraction

- Host parsing must be span-preserving. Extraction needs the raw source span of every message value, so value-only deserialization in the plain `serde` style is not sufficient.
- Extraction produces the ordered entry list defined by the message entry model.

### Entry Identity

- `StructuralPathKey` is a serialized, structurally unambiguous host path. JSON adapters use RFC 6901 JSON Pointer, so the literal catalog key `"a.b"` and the nested path `a` → `b` remain distinct even when a runtime flattens both to `a.b`. YAML adapters use a typed structural path that distinguishes mapping keys from sequence indices and preserves each scalar key's resolved tag and value; a candidate string below an initially unsupported complex mapping key fails extraction with `resource_entry_unsupported` and `details.reason: "structural_path_unsupported"`. A YAML adapter may use JSON Pointer serialization only for JSON-compatible, string-keyed paths. Thus a string key `"1"`, an integer key `1`, and sequence index `1` cannot collide.
- `EntryKey` is the unique occurrence identity, not a display string. For every set of extracted entries with the same structural path, adapters assign `occurrence` in raw source order starting at `0`. An entry whose path appears only once still carries `occurrence: 0`; the field is never omitted or inferred from duplication discovered later.
- When a host document contains duplicate keys or duplicate ancestor paths, each raw leaf occurrence is a separate entry. Numbering is over entries with the same complete structural path, so nested duplicates remain unambiguous without including byte offsets in identity. Reporting duplicate catalog keys as a problem is future catalog-level linting, not an extraction failure.
- `CatalogKey` represents the runtime-style logical message independently of a concrete host role. JSON and YAML set it to the same serialized value as `StructuralPathKey`; formats whose host identity contains locale-role syntax may derive a different value under an explicit format contract. Future cross-locale grouping uses `CatalogKey` together with comparison scope and locale. A host duplicate-key rule instead groups by `StructuralPathKey` without occurrence, so distinct XLIFF source and target roles are never duplicates merely because they share a catalog key. Occurrence and concrete evidence always use `EntryKey`.
- Adapters may additionally expose a human-oriented display key, such as dot-joined nesting, for UI and reporting purposes. Reporting, editor diagnostic identity, and message cache identity use the complete `EntryKey`; display identity never replaces either stable key type.

YAML serializes its typed structural path as a pointer whose root is the empty string. Each path step appends `/` followed by one typed segment, with `~` encoded as `~0` and `/` as `~1` inside the complete segment, matching RFC 6901 escaping. No percent encoding, Unicode normalization, or display-key flattening is applied. Initial segment forms are:

| YAML path step      | Unescaped typed segment         | Example serialized path |
| ------------------- | ------------------------------- | ----------------------- |
| string mapping key  | `k:str:<resolved string>`       | `/k:str:greeting`       |
| null mapping key    | `k:null`                        | `/k:null`               |
| boolean mapping key | `k:bool:true` or `k:bool:false` | `/k:bool:true`          |
| integer mapping key | `k:int:<canonical integer>`     | `/k:int:1`              |
| float mapping key   | `k:float:<canonical float>`     | `/k:float:15e-1`        |
| sequence index      | `i:<zero-based decimal index>`  | `/i:1`                  |

String payload is the exact resolved Unicode scalar sequence; for example, key `a/b~c` produces `/k:str:a~1b~0c`. Integer canonical form is arbitrary-precision base-10 with no leading plus sign or redundant leading zeroes, and every zero spelling becomes `0`. A finite float is normalized as an exact base-10 coefficient and exponent: the coefficient has no leading or trailing zeroes, the exponent has no leading plus sign or redundant zeroes, and zero, including negative zero, is `0e0`; for example `1.0` becomes `1e0`, `1.50` becomes `15e-1`, and `1e3` remains `1e3`. Positive infinity, negative infinity, and NaN are `.inf`, `-.inf`, and `.nan`. This normalizer is resource-owned and must not use dependency debug or display formatting.

Different source spellings with the same resolved Core tag and value therefore share one structural path and receive distinct occurrence numbers, while a string key `"1"`, integer key `1`, and sequence index `1` remain `/k:str:1`, `/k:int:1`, and `/i:1`. Explicit supported Core tags use the same resolved-type serialization. A complex mapping key has no initial segment form and causes the previously defined `resource_entry_unsupported` failure instead of receiving an opaque or lossy path.

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

### Extraction Failure Presentation

When the host document itself cannot be parsed, extraction fails for the whole document. Host syntax errors are never translated into MF2 diagnostics; host-format tooling owns them. The CLI reports `resource_parse_failed` as a target-local operational error, while editor adapters treat that code as a transient host-editing state and retain the last successful MF2 diagnostic publication.

Other actionable resource input failures are not transient parse state. The CLI reports their stable operational codes below. An editor clears stale MF2 diagnostics and publishes one separate `ox-mf2-resource` diagnostic for format-, entry-, document-, or limit-related input failure. Configuration and internal failures instead use the editor operational error channel and transactionally retain the last successful state. Exact editor publication and recovery behavior is owned by [009-ox-mf2-phase-3d-lsp-editor-design.md](./009-ox-mf2-phase-3d-lsp-editor-design.md); every extraction failure returns no formatting edits and no partial artifact.

## Host Format Registry and Catalog Detection

Consumers resolve each input file or document to at most one host format adapter through a host format registry.

- Standalone MF2 classification runs before catalog membership. A `.mf2` file, or an editor document explicitly classified by the MF2 language id, always uses the standalone workflow even when a broad project or ad-hoc catalog pattern also matches its path. That catalog match is inapplicable to the reserved standalone target rather than a format conflict; it never reroutes the file through resource extraction.
- Catalog host formats are strictly opt-in. Arbitrary JSON, YAML, or XML files must not be assumed to contain MF2 messages; a false positive reports wrong diagnostics on unrelated files and, worse, exposes them to formatting write-back. That is worse than requiring configuration.
- Project-configured catalog opt-in comes from the `resources` section of the unified project config defined below. Editor adapters may additionally layer explicitly editor-only ad-hoc settings over that result under the fixed precedence below. Filename conventions never imply catalog membership: names such as `*.mf2.json` may be documented as recommended `include` patterns, but they are not automatic opt-in defaults in any tier.
- CLI resource workflows and editor adapters first resolve project-configured membership and host format identically. A project match is authoritative and cannot be reclassified by an editor setting. An overlapping ad-hoc match resolving to the same format is de-duplicated as the project-owned target; one resolving to a different format is an editor configuration error and never overrides or masks the project result.
- The ad-hoc layer may opt in only a document left unmatched by the resolved project configuration, including a path removed by a project catalog's `exclude`. Overlapping ad-hoc entries use the same rule: same-format matches de-duplicate and different-format matches are an editor configuration error. An invalid project configuration is reported before applying the overlay rather than being bypassed by it.
- An editor-only ad-hoc target is intentionally absent from CLI and CI processing until persisted in project configuration; editor integrations distinguish that state rather than presenting it as CI coverage.
- If overlapping catalog configuration maps one file to multiple host formats, that is a configuration validation failure and therefore an operational error, not silent precedence.

### Format IDs and Extension Classification

Registry ids are canonical lowercase ASCII strings. Extension-derived classification uses the final filename extension and compares it under ASCII case folding on every operating system, independent of filesystem case sensitivity. The original extension spelling remains available for operational-error details. Initial and deferred mappings fixed by this design are:

| Registry id | Derived filename extensions | Tier |
| ----------- | --------------------------- | ---- |
| `json`      | `.json`                     | 1    |
| `vue`       | `.vue`                      | 2    |
| `yaml`      | `.yaml`, `.yml`             | 3    |
| `jsonc`     | `.jsonc`                    | 3    |
| `json5`     | `.json5`                    | 3    |
| `xliff`     | `.xlf`, `.xliff`            | 3    |

For example, `.YML` derives canonical `yaml` on Linux, macOS, and Windows alike, while error details still preserve `.YML`. An explicit catalog `format` is schema-validated as an exact canonical id: aliases such as `yml` and `xlf`, uppercase values such as `JSON`, surrounding whitespace, and comma-joined values are invalid rather than normalized. `HostFormat`, the generated config schema, and `supportedFormats` expose only adapters shipped in that release, while extension classification may recognize a deferred id and report `resource_format_unsupported` until its adapter ships.

These filename rules also classify a catalog stdin virtual path. They do not apply to an embedded Vue `lang` declaration, whose exact ids and explicit `yml` alias are defined separately above, and they do not perform content sniffing.

Within an opted-in catalog document, the default extraction scope begins with every host-typed string leaf value, including string elements of arrays. The adapter then validates the candidate's path and unescaped value against the message-entry representability invariants above. If any candidate fails, extraction fails for the complete host document with `resource_entry_unsupported`; no partial artifact or entry list is returned, and linting, formatting, and write-back do not process that file. When multiple candidates fail, the error for the lowest raw source offset is selected deterministically.

Non-string scalars are outside the message-entry candidate set and do not produce an error. Future explicit key selectors intentionally remove matching strings from the configured extraction scope and likewise do not produce an error; they express a user-selected scope rather than an adapter coverage gap.

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

JSON catalogs unescape RFC 8259 string escapes. The JSON Pointer entry identity rules above apply. A string escape sequence that denotes an unpaired surrogate fails extraction with `resource_entry_unsupported` and `details.reason: "message_text_unrepresentable"`. Formatted multi-line MF2 output, such as a formatted matcher, re-escapes line breaks as `\n` escapes inside the single-line JSON string value.

A JSON catalog may begin with exactly one UTF-8 BOM. The adapter retains its bytes at host span `0..3`, passes only the source after that BOM to the JSON syntax parser, and adds the three-byte base offset back to every parser span and error offset. The BOM is outside every entry and offset map, and write-back preserves it byte-for-byte. A second leading BOM or a BOM code point outside a JSON string is ordinary invalid JSON and produces `resource_parse_failed`; `U+FEFF` inside a string remains message data. A JSON catalog does not use standalone `.mf2` file framing: trailing `LF` or `CRLF`, missing final newline, and all other bytes outside replaced value spans remain unchanged.

For a JSON entry, `raw_value_span` includes both surrounding double quotes. The quotes are `RawOnly` offset-map segments, while the content between them maps through identity and unescape segments. JSON has no alternate writable quote style in this adapter.

Re-escaping uses two exact paths:

- If the supplied formatted message is byte-identical to the entry's original `message_text`, `reescape` returns the original raw value slice byte-for-byte. Existing optional escape spellings are therefore never normalized for an unchanged message.
- If the message changed, `reescape` serializes the complete formatted message into one canonical JSON string value. It does not attempt to align unchanged message characters with their original host escape spellings.

The canonical changed-value serializer emits the opening and closing `"` and applies these scalar rules in order:

- `U+0022` quotation mark becomes `\"`, and `U+005C` reverse solidus becomes `\\`.
- `U+0008`, `U+0009`, `U+000A`, `U+000C`, and `U+000D` use `\b`, `\t`, `\n`, `\f`, and `\r` respectively.
- Every other scalar from `U+0000` through `U+001F` uses `\u` followed by exactly four lowercase hexadecimal digits.
- `U+002F` solidus is emitted as `/`, not `\/`.
- Every other Unicode scalar value is emitted directly as UTF-8, including `U+2028` and `U+2029`; changed values are not converted to ASCII-only JSON.

Rust `str` input cannot contain an unpaired surrogate, so a JSON host value that unescapes to one produces `resource_entry_unsupported` before this serializer is reached. The fixed serializer makes changed-value output independent of the host parser dependency and deterministic across CLI and editor consumers.

### Vue SFC `<i18n>` Custom Blocks

Single-file-component `<i18n>` blocks embed a catalog region inside a `.vue` host document. This is adapter composition: an outer adapter locates the block region and its declared language, an inner catalog adapter runs over the region text, and the offset maps compose into document coordinates. The entry model is closed under this composition; no new consumer-facing behavior is required.

The initial Vue adapter extracts inline `<i18n>` blocks only. A block with a `src` attribute is an external-resource reference and contributes no entries to the `.vue` extraction artifact; the resource layer does not resolve its path, read it transitively, validate that it exists, or write through the SFC. The referenced file must independently match `resources.catalogs` and is then processed as its own catalog target by CLI and editor consumers. This explicit external-resource boundary prevents duplicate diagnostics and gives reads, errors, cache identity, and edits to the file that actually owns the message bytes. A `src` block is therefore intentionally outside the inline candidate set and is not `resource_entry_unsupported`.

Composition consumes only the inner adapters that have shipped when this tier lands: blocks whose language is JSON, the `<i18n>` block default, compose the Tier 1 JSON adapter, while `lang="yaml"` or `lang="json5"` blocks require their Tier 3 adapters. If an inline block declares an inner format whose adapter has not shipped, extraction of the complete `.vue` target fails with `resource_format_unsupported`; supported blocks in the same SFC are not returned as a partial artifact. When more than one inline block is unsupported, the error for the lowest block source offset is selected. A `src` block is filtered as an external-resource reference before inner-format dispatch, so its `lang` does not trigger this error in the `.vue` target.

An inline block with no `lang` attribute selects `json`. A present attribute is matched exactly without trimming or case folding: the recognized declarations are `json`, `json5`, `yaml`, and `yml`, with only `yml` normalized to the `yaml` registry id. A valueless or empty attribute, surrounding whitespace, values such as `JSON`, and unknown values do not fall back to JSON and do not trigger content sniffing; they produce `resource_format_unsupported`. Whether a recognized id is usable still depends on its inner adapter having shipped. These rules consume the static attribute value returned by the SFC syntax parser and never interpret Vue expressions as language declarations.

`jsonc` is intentionally not a Vue embedded-language declaration, even after the top-level JSONC adapter ships. An inline `lang="jsonc"` block therefore produces embedded `resource_format_unsupported`; a `.jsonc` file referenced through `src` may still be processed independently when that external path is opted into `resources.catalogs`. The Vue adapter never treats `lang="json"` as permissive JSONC.

`crates/intlify_resource` owns the built-in Vue SFC outer adapter and the composition with inner catalog adapters. Consumers pass the complete `.vue` host document to the shared registry and must not locate `<i18n>` blocks independently. The outer adapter preserves the block content spans and declared languages, selects a shipped inner adapter, and lifts the inner entries, offset maps, and write-back replacements into absolute `.vue` document coordinates. This keeps classification, extraction, and write-back identical across CLI and editor consumers.

For a lifted entry, `StructuralPathKey` remains the inner adapter's structural path and does not include a Vue block index or source offset. After collecting entries from all supported inline blocks, the outer adapter orders them by absolute raw span and reassigns `occurrence` across the complete SFC for each identical structural path. Thus the same `/greeting` path in two blocks becomes occurrences `0` and `1`. The outer artifact mints its own handles and privately retains each entry's block and inner-artifact identity for mapping and write-back; consumers never use an inner handle directly. `CatalogKey` and `displayKey` likewise remain the inner adapter's logical and display identities. This keeps the logical key free of block positions; the complete `EntryKey` intentionally reflects source order among duplicate occurrences.

Ownership of the outer adapter does not require the resource crate to implement Vue syntax parsing itself. It may use a dedicated parser dependency or a private workspace helper, provided that the helper exposes the span-preserving SFC syntax information required by the adapter and does not become a second consumer-facing extraction path.

Parse errors identify the adapter layer that rejected its source. Invalid SFC syntax produces `resource_parse_failed` with `details.format: "vue"` and no `outerFormat`. If the SFC is valid but an inline catalog is invalid, `format` is the normalized inner registry id and `outerFormat` is `"vue"`. In both cases, `offset`, `line`, and `column` are absolute coordinates in the complete `.vue` source after the outer adapter lifts an inner error position; no supported block is returned when another block fails to parse.

The same failing-layer attribution applies when an inner adapter returns `resource_entry_unsupported` or `resource_document_unsupported`: `format` names the inner adapter, `outerFormat` names `vue`, and the outer artifact lifts the primary unsupported position into absolute SFC coordinates. A failure in SFC structure itself instead uses `format: "vue"` and omits `outerFormat`.

### YAML Catalogs

YAML catalogs resolve entries from tag-resolved string scalars under the YAML 1.2 Core Schema. The adapter configures that schema explicitly and does not inherit a parser dependency's default or YAML 1.1 compatibility mode. Consequently plain `true`, `false`, `null`, and recognized numeric forms are non-string scalars, while values such as `yes`, `on`, and timestamp-shaped text remain strings unless an explicit supported tag changes their type. Fixtures lock these boundaries across dependency upgrades.

An absent `%YAML` directive selects that fixed YAML 1.2 behavior, and a syntactically valid `%YAML 1.2` directive is accepted without changing it. Any other syntactically valid declared version, including `1.1` or a future version, fails complete extraction with `resource_document_unsupported` and `details.feature: "yaml_version"` at the directive token. The adapter neither switches schemas per document nor silently ignores the declaration. A malformed directive remains invalid host syntax and uses `resource_parse_failed` instead.

A YAML catalog may begin with exactly one UTF-8 BOM under the same source-preservation rule as JSON. The adapter retains bytes `0..3`, parses the source after them, and lifts every span and error offset by three bytes; write-back preserves the BOM byte-for-byte. Any later `U+FEFF`, including a second leading code point, is not framing and is passed to the YAML parser as ordinary source or scalar data. A parser rejection is `resource_parse_failed`, while a `U+FEFF` that the fixed YAML rules admit inside a string remains message text. YAML catalog processing does not apply standalone `.mf2` trailing-newline framing, so all bytes outside replaced scalar spans retain their exact line-ending spelling.

Explicit tags are restricted to the YAML 1.2 Core tag set: `tag:yaml.org,2002:str`, `null`, `bool`, `int`, `float`, `seq`, and `map`, including their standard `!!` shorthand. An explicit string tag therefore makes `!!str true` a string entry, while supported explicit non-string tags retain their declared types. A node carrying any local or global tag outside that set fails complete extraction with `resource_document_unsupported` and `details.feature: "custom_tags"` at the tag token; the adapter never ignores the tag and guesses from the underlying scalar or collection. A `%TAG` directive that is declared but never used does not fail extraction.

The initial YAML adapter accepts one YAML document only. An empty stream and one empty document are valid catalogs with zero entries. Encountering a second document fails complete extraction with `resource_document_unsupported` at that document's start; entries from the first document are not returned or processed. A document-end marker does not itself create another document, and an optional first document-start marker remains valid.

Duplicate mapping keys are preserved rather than rejected or collapsed. The adapter consumes a span-preserving node or event representation, traverses concrete values in raw source order, and compares mapping keys by the canonical resolved tag-and-value path serialization above. Every string leaf under an identical complete structural path becomes a separate entry with document-wide occurrences `0`, `1`, and so on, including leaves below duplicate ancestor mappings. A dependency mode that deserializes into a map, rejects duplicates before nodes can be inspected, or keeps only a runtime winner is unsuitable for this adapter. Extraction does not decide first-wins or last-wins runtime semantics; future catalog-level duplicate-key linting reports the ambiguity while each concrete raw value remains independently diagnosable and writable.

Plain, single-quoted, and double-quoted string scalars are read-write when this tier lands. Literal and folded block scalars produce correct offset maps for diagnostics but remain read-only for the entire initial YAML adapter milestone. Write support for block scalars is a separate follow-up milestone rather than a condition for shipping the Tier 3 YAML adapter.

For a block scalar, `raw_value_span` starts at the `|` or `>` indicator, includes the complete header and owned body lines, and ends before the first following token or line that is outside the scalar; a preceding explicit tag or anchor is outside that span. The header, its optional indentation and chomping indicators or comment, and each stripped body indentation prefix are `RawOnly`. Content bytes that survive resolution unchanged use `Identity`. Each physical line-break unit whose resolved form is a folded space or a retained line feed uses one `Unescape` segment, so a diagnostic never splits that transformation. Breaks removed by strip or clip chomping are `RawOnly`, while the one clip-retained break and every keep-retained break map to their corresponding message line feeds. Empty lines and more-indented lines follow the same YAML 1.2 folding rules rather than a simplified newline substitution.

The adapter must derive these segments from span-preserving block tokens, not by searching for decoded substrings after parsing. Failure to build a complete validated map for a supported block scalar is `internal_error` with `details.reason: "resource_offset_map_invariant_failed"`; it does not downgrade the diagnostic to the whole scalar. A valid block scalar is still returned as `read_only: true`, participates in normal per-entry linting, and is skipped only by formatting.

Multi-line plain, single-quoted, and double-quoted scalars remain read-write and require equally exact maps. Their `raw_value_span` covers the complete scalar presentation and owned continuation lines, including surrounding quotes but excluding a preceding tag or anchor. Quote delimiters, stripped continuation indentation, and non-value syntax are `RawOnly`; unchanged content runs are `Identity`; doubled single quotes, double-quoted escapes, and physical line breaks that resolve to a folded space or retained line feed are atomic `Unescape` segments. Each `LF`, `CRLF`, or `CR` physical break is one raw unit so a mapped range never splits `CRLF`. A double-quoted escaped line continuation, including its following indentation, produces no message bytes and is `RawOnly`. These maps are derived from parser tokens under the YAML 1.2 folding rules, never by substring search.

Formatting such a scalar follows the same original-style attempt and exact reparse check below. If a changed multi-line value cannot retain its original presentation exactly, the adapter replaces the complete raw scalar span with the canonical one-physical-line double-quoted form; tags, anchors, surrounding comments, and all bytes outside that span remain unchanged.

YAML re-escaping has the same unchanged-value fast path as JSON: when formatted message text is byte-identical to `message_text`, the original raw scalar slice is returned byte-for-byte. For a changed plain, single-quoted, or double-quoted scalar, a resource-owned style serializer first produces a candidate in the original style only when that style can represent the new value unambiguously in the scalar's existing syntactic context. The adapter reparses the candidate and accepts it only when the resolved tag remains string and the resolved value equals the formatted message exactly. If original-style construction or validation fails, it emits the resource-owned canonical double-quoted form instead. It never delegates spelling or style selection to a dependency emitter. Final full-document `apply_and_validate` remains mandatory after these per-scalar checks.

The plain-style candidate is the formatted message verbatim and is attempted only for a single physical line with no C0 or C1 control character and no `U+2028` or `U+2029`; context-sensitive indicators, surrounding whitespace, comments, and implicit Core typing are left to the mandatory reparse/tag/value check. The single-quoted candidate has the same single-line character precondition, surrounds the value with `'`, and replaces each embedded `'` with `''`; backslashes and double quotes remain literal. Multi-line changed values therefore fall back to double-quoted form rather than relying on YAML folding.

The canonical double-quoted serializer emits one physical line with opening and closing `"` and applies these rules in order:

- `U+0022` quotation mark becomes `\"`, and `U+005C` reverse solidus becomes `\\`.
- `U+0008`, `U+0009`, `U+000A`, `U+000C`, and `U+000D` use `\b`, `\t`, `\n`, `\f`, and `\r` respectively.
- Every other scalar in `U+0000..U+001F` or `U+007F..U+009F` uses `\x` followed by exactly two lowercase hexadecimal digits.
- `U+2028` and `U+2029` use `\u2028` and `\u2029` so YAML does not interpret them as physical line breaks.
- `U+002F` solidus is emitted as `/`. Every other Unicode scalar value is emitted directly as UTF-8; changed values are not converted to ASCII-only YAML and do not use optional YAML named escapes such as `\0`, `\a`, `\e`, `\N`, `\_`, `\L`, or `\P`.

The serializer then reparses this scalar under the fixed Core Schema and requires an exact string value match. An unexpected mismatch is `internal_error` with `details.reason: "resource_write_back_failed"`; it never falls through to another dependency-chosen spelling.

An anchor annotation on a directly defined node does not change that node's extraction: the scalar at the definition site produces its normal entry, and an anchor that is never referenced is accepted. Any alias node that is actually used fails complete extraction with `resource_document_unsupported` and `details.feature: "aliases"`, because silently omitting the alias path would diverge from the runtime catalog and expanding it would make multiple logical entries share one raw source value. YAML merge keys likewise fail with `details.feature: "merge_keys"`; for `<<: *defaults`, the merge-key token is the primary unsupported position rather than the nested alias token. No alias or merge expansion produces a partial or read-only entry in the initial adapter.

Block scalar entries graduate to writable only after the adapter supports value-identical re-escaping for both literal and folded styles; indentation indicators `1` through `9`; strip, clip, and keep chomping; empty and leading-empty content; trailing line breaks and trailing empty lines; and more-indented lines. Fixtures must reparse every generated candidate and compare its resolved string value with the formatted message text, and must prove formatting idempotency. Until those requirements land, linting reports mapped diagnostics normally, while fmt write mode skips block scalar entries and `--check` does not count them as different.

Once that follow-up milestone makes block scalars writable, re-escaping uses a fixed fallback order. The adapter first preserves the original literal or folded style, adjusting indentation and chomping indicators as necessary, when reparsing produces the exact formatted message text. If folded style cannot represent that value identically, it switches to literal style. If literal style also cannot represent the value safely, it switches to double-quoted style. Every candidate is reparsed and accepted only on exact value equality; failure of all candidates is a `resource_write_back_failed` internal error and produces no file write.

### JSONC and JSON5 Catalogs

The `jsonc` adapter implements a fixed ox-mf2 profile over RFC 8259 JSON. It adds JavaScript-style `//` line comments and non-nesting `/* ... */` block comments wherever JSON whitespace is permitted, plus one optional trailing comma after the last member of a non-empty object or the last element of a non-empty array. It does not admit array elisions, `#` comments, single-quoted strings, unquoted member names, hexadecimal or non-finite numbers, or any other JSON5 syntax. This deliberately enables the trailing-comma option that the [JSONC specification](https://jsonc.org/) leaves profile-selectable rather than inheriting a parser default.

JSONC string values, member-name resolution, duplicate occurrence identity, optional single leading BOM, offset mapping, unchanged raw-spelling preservation, canonical changed-value serialization, and full candidate validation are exactly the Tier 1 JSON rules. Comments and trailing commas are host syntax outside string value spans and remain byte-identical during MF2 formatting. A parser dependency must retain their spans and duplicate nodes; stripping comments into a temporary JSON buffer without a complete source map is insufficient.

JSONC is a top-level catalog format only. It is not added to the Vue `<i18n>` inner-language allowlist; external `.jsonc` resources remain separate targets under the `src` boundary above.

The `json5` adapter implements the complete [Standard JSON5 1.0.0 grammar](https://spec.json5.org/), not an arbitrary JavaScript object-literal parser or a dependency-specific permissive mode. JSON5 comments, trailing commas, identifier member names, expanded number syntax, single- and double-quoted strings, additional escapes, and line continuations are accepted only as that grammar defines them. Non-string values remain outside the message candidate set. String and identifier member names resolve to their exact string value for JSON Pointer structural identity, so alternative spellings of one resolved name share a path and use occurrence numbering.

JSON5 follows its grammar's `U+FEFF` rule rather than the JSON-family single-leading-BOM rule. Any number of `U+FEFF` code points outside strings are ordinary token-separating whitespace at any permitted position and remain byte-identical in the host document; the adapter does not strip a leading instance before parsing. Inside a string, `U+FEFF` is message data. All absolute spans and errors therefore count every raw UTF-8 byte directly, and write-back leaves whitespace instances outside replaced value spans untouched.

For a JSON5 string entry, `raw_value_span` includes its exact single or double quote delimiters. Delimiters and a backslash plus its complete physical line-terminator sequence in a line continuation are `RawOnly`; unchanged content is `Identity`; every escape is one atomic `Unescape` unit, including compound UTF-16 surrogate escapes that resolve to one scalar. `CRLF` is never split. A string escape that resolves to an unpaired surrogate fails complete extraction with `resource_entry_unsupported` and `details.reason: "message_text_unrepresentable"`, matching JSON's UTF-8 representability rule.

JSON5 re-escaping preserves the complete original raw string byte-for-byte when the formatted message is unchanged. For a changed value, it retains the original quote delimiter and emits one physical line with these rules:

- The active delimiter becomes `\'` for a single-quoted string or `\"` for a double-quoted string. The other quote remains literal, and reverse solidus becomes `\\`.
- `U+0008`, `U+0009`, `U+000A`, `U+000C`, and `U+000D` use `\b`, `\t`, `\n`, `\f`, and `\r` respectively.
- Every other scalar in `U+0000..U+001F` or `U+007F..U+009F` uses `\x` followed by exactly two lowercase hexadecimal digits. In particular, changed output does not use context-sensitive `\0` or optional `\v`.
- `U+2028` and `U+2029` use `\u2028` and `\u2029`. Solidus is emitted as `/`.
- Every other Unicode scalar value is emitted directly as UTF-8; output is not ASCII-only and never uses a line continuation.

The resource-owned serializer does not modify string or identifier member-name spelling because those tokens lie outside message value spans. The complete candidate is reparsed as Standard JSON5 1.0.0 and must preserve both key sequences and exact message values under the normal `apply_and_validate` contract; a dependency generator is never used.

### XLIFF

XLIFF is an XML host format: XLIFF 1.2 stores message text in `<trans-unit>` `<source>`/`<target>` elements and XLIFF 2.x in `<unit>`/`<segment>` `<source>`/`<target>` elements. Entry keys serialize the file/group/unit and, where present, segment identity path. Unescaping covers XML entities, CDATA sections, and XML line-ending normalization; `xml:space` is retained as host metadata but does not cause adapter-defined text normalization.

The adapter selects one explicit profile from the root element's expanded namespace name and exact `version` attribute. The initial supported pairs are:

| Profile   | Root namespace                          | `version` |
| --------- | --------------------------------------- | --------- |
| XLIFF 1.2 | `urn:oasis:names:tc:xliff:document:1.2` | `1.2`     |
| XLIFF 2.0 | `urn:oasis:names:tc:xliff:document:2.0` | `2.0`     |
| XLIFF 2.1 | `urn:oasis:names:tc:xliff:document:2.0` | `2.1`     |
| XLIFF 2.2 | `urn:oasis:names:tc:xliff:document:2.2` | `2.2`     |

Namespace prefixes are arbitrary and do not participate in matching. A missing namespace or version, a mismatched pair, and an unknown future version are valid XML but unsupported XLIFF profiles and fail complete extraction with `resource_document_unsupported` and `details.feature: "xliff_version"`. Malformed XML remains `resource_parse_failed`. Adding another version requires an explicit tested profile; the adapter never assumes that an arbitrary `2.x` document has compatible structure. Profile selection recognizes the XLIFF structural vocabulary needed for extraction but does not turn this resource layer into a complete XSD or XLIFF constraint validator.

For an explicitly opted-in XLIFF catalog, every supported primary plain-text `<source>` and `<target>` element is an independent read-write MF2 entry. XLIFF 1.2 candidates are direct children of `<trans-unit>` in the 1.2 namespace; XLIFF 2.x candidates are direct children of `<segment>` in the selected 2.x namespace. Matching uses expanded names and exact parent relationships, not namespace prefixes or local names alone. The structural path contains an explicit `source` or `target` role segment after its unit or segment identity, so the two sides never rely on occurrence numbering merely to distinguish their roles. A missing side produces no synthetic entry. Formatting either side is permitted by the catalog opt-in and uses the same value-identical XML write-back contract; format-specific source/target selection is not added to the initial configuration.

`<source>` or `<target>` elements inside XLIFF 1.2 `<alt-trans>`, XLIFF 2.x `<ignorable>`, metadata, extension elements, or any other non-primary context are intentionally outside the candidate set. They produce neither entries nor unsupported-entry errors, even when their content is plain text. This is a semantic scope boundary rather than an adapter coverage gap: the initial adapter processes only the primary translation values that represent the catalog's active messages.

XLIFF also defines the first concrete intrinsic-locale source for the future `locale: { "from": "host" }` binding. For XLIFF 1.2, a primary source entry uses its effective `xml:lang` when present, otherwise its enclosing `<file source-language>`; a target uses effective `xml:lang`, otherwise `<file target-language>`. For XLIFF 2.x, the corresponding fallbacks are the root `<xliff srcLang>` and `<xliff trgLang>`. Effective `xml:lang` follows normal XML ancestor inheritance. The resolved string is preserved exactly without BCP 47 validation, case folding, or canonicalization. This metadata does not affect entry-level extraction, linting, or formatting and is consumed only when the future catalog linter explicitly enables host locale binding.

XLIFF workflow metadata such as `translate`, `state`, approval, or review attributes does not alter extraction or writeability. The resource layer is not a TMS state machine: it neither interprets version-specific inheritance as an edit lock nor changes status, timestamps, provenance, or approval after formatting. Those attributes remain outside the raw message span and are preserved byte-for-byte. An entry is read-only only when its host representation cannot satisfy value-identical re-escaping; explicitly opting the XLIFF file into formatter processing authorizes syntax-only formatting of otherwise writable primary source and target values.

XLIFF structural paths use the same slash-separated `~0`/`~1` escaping rule as the typed YAML pointer, with format-specific typed segments. XLIFF 1.2 uses a non-empty `<file original>` value for `file:original:<value>` and `<group id>` or `<trans-unit id>` for `group:id:<value>` and `unit:id:<value>`. XLIFF 2.x uses non-empty `id` values on `<file>`, `<group>`, `<unit>`, and `<segment>` as `file:id:<value>`, `group:id:<value>`, `unit:id:<value>`, and `segment:id:<value>`. Nested groups append one segment per level. The final segment is always `side:source` or `side:target`.

An absent or empty identity attribute uses `<kind>:i:<zero-based same-kind sibling index>`, such as `segment:i:0`; the adapter does not fail the document merely to enforce an XLIFF schema-required identity. Attribute payload is the XML-resolved Unicode value without trimming, case folding, or Unicode normalization before pointer escaping. Duplicate attribute identities deliberately produce the same structural path, and concrete entries receive document-wide occurrence numbers in raw source order. For example, representative paths are `/file:original:app.json/group:id:menu/unit:id:welcome/side:source` and `/file:id:f1/group:i:0/unit:id:u1/segment:id:s1/side:target`. A runtime-oriented `displayKey` may shorten this hierarchy, but reporting and cache identity use the complete path.

The XLIFF `CatalogKey` is that same serialized hierarchy with the final `side:source` or `side:target` segment omitted. Thus both entries under one unit or segment share `/file:original:app.json/group:id:menu/unit:id:welcome` or `/file:id:f1/group:i:0/unit:id:u1/segment:id:s1` as their logical message identity while retaining distinct concrete `EntryKey` values. Host locale binding can therefore compare source and target as locale variants without erasing their physical roles from diagnostics, caches, or write-back.

DTD processing is disabled before any entity resolution or external access. A well-formed `DOCTYPE` declaration fails complete extraction with `resource_document_unsupported` and `details.feature: "xml_dtd"` at `<!DOCTYPE`; the adapter never reads a system identifier, performs a network request, or expands an internal entity declaration. Malformed XML, including an incomplete declaration, remains `resource_parse_failed`. Text and identity attributes may use only the five predefined XML entities and decimal or hexadecimal numeric character references. Each reference is one `Unescape` offset-map segment; an undeclared named entity is invalid XML and produces `resource_parse_failed`. The byte sequence `<!DOCTYPE` inside a comment, CDATA section, or ordinary escaped character data is data rather than a declaration.

XLIFF input remains subject to the shared UTF-8 host-source contract and XML 1.0 processing rules. An absent XML declaration selects XML 1.0. A declaration must contain exact `version="1.0"`; any other syntactically valid version fails complete extraction with `resource_document_unsupported` and `details.feature: "xml_version"` at the version value. The adapter does not apply XML 1.1 character or line-ending rules.

The declaration may omit `encoding`; when present, the encoding name must equal `UTF-8` under XML's ASCII case-insensitive comparison. Any other syntactically valid encoding declaration fails complete extraction with `resource_document_unsupported` and `details.feature: "xml_encoding"` at the encoding value; the adapter does not transcode or ignore it. A malformed declaration, including one without its XML-required version, is `resource_parse_failed`.

Exactly one leading UTF-8 BOM is accepted, retained at bytes `0..3`, removed only for XML parsing, and added back to every span and error offset. Write-back preserves it byte-for-byte. A second leading `U+FEFF` is ordinary XML source and fails if the XML grammar rejects it, while an admitted `U+FEFF` inside `<source>` or `<target>` character data remains message text. XLIFF catalog processing does not apply standalone `.mf2` trailing-newline framing.

The message value is the exact XML character-data sequence after predefined or numeric entity resolution and XML line-ending normalization. The adapter never trims leading or trailing whitespace, collapses internal runs, removes formatting indentation, or appends a newline based on an effective inherited `xml:space` value. Both `default` and `preserve` therefore keep the same character sequence at this layer; applications may interpret that metadata later, but resource extraction cannot guess an application-specific whitespace policy while satisfying the exact-value contract.

Raw `CRLF` and `CR` units that XML normalizes to `LF` are atomic `Unescape` map segments, while a raw `LF` that survives unchanged is `Identity`. The same rule applies inside ordinary character data and CDATA. Attribute spelling, namespace declarations, and `xml:space` values remain outside message spans and are preserved byte-for-byte by write-back.

For a non-self-closing `<source>` or `<target>`, `raw_value_span` is the complete byte range between the end of the start tag and the start of the end tag. Entity references are atomic `Unescape` segments; CDATA opening and closing delimiters are `RawOnly`; CDATA and ordinary text bytes that survive XML processing are `Identity` or the line-ending segments above. Adjacent ordinary text, references, and CDATA chunks form one message and one offset map.

A self-closing `<source/>` or `<target/>` is a writable empty-string entry rather than an omission or read-only special case. Its `raw_value_span` is the complete self-closing element. The map splits the tag into leading and trailing `RawOnly` syntax with the sole empty message position immediately before the closing `/>`. An unchanged empty value returns the original element byte-for-byte. If a non-empty replacement is ever requested, re-escaping removes only the terminal `/`, retains the exact raw qualified name, prefix, attributes, quoting, and whitespace in the resulting start tag, inserts canonical XML character data, and appends an end tag using that same qualified name. The full-document reparse must confirm the same entry identity and expected value before the expanded element can be written.

An explicitly paired empty element such as `<target></target>` instead has a zero-length content span at the insertion point and remains writable under the normal content replacement path. Replacement validation permits such zero-length spans but still rejects duplicate replacements for the same entry; distinct empty elements have distinct absolute insertion offsets.

XLIFF re-escaping preserves the complete original content slice byte-for-byte when the formatted message is unchanged. For a changed value, it replaces that entire content span with one resource-owned canonical XML character-data sequence; it does not align the new value to old chunks or retain CDATA boundaries and optional entity spellings. Canonical text applies these rules in order:

- `&` becomes `&amp;` and `<` becomes `&lt;`.
- `>` remains literal except when it would complete the forbidden character-data sequence `]]>`, in which case that `>` becomes `&gt;`.
- `U+000D` becomes the hexadecimal numeric reference `&#xd;` so reparsing does not normalize it to `LF`. Tab and `LF` remain literal.
- Quotes and apostrophes remain literal because the replacement is element content, not an attribute. Every other XML 1.0-permitted Unicode scalar is emitted directly as UTF-8.

The candidate content is reparsed in its complete XLIFF document and must resolve to the formatted message exactly. A formatted value containing a scalar that XML 1.0 cannot represent, or any unexpected value mismatch, is `internal_error` with `details.reason: "resource_write_back_failed"` and produces no candidate or partial write.

The initial XLIFF adapter extracts only primary candidates whose content is plain XML character data, including character data represented through entity references or CDATA sections. A candidate `<source>` or `<target>` that contains any inline child element, including character, standalone, paired, spanning, marker, or annotation elements, fails extraction with `resource_entry_unsupported` and `details.reason: "inline_content_unsupported"`. The generic resource adapter does not replace inline elements with sentinel placeholders or translate them into MF2 markup, because doing so would require format- and project-specific rules for identity, pairing, nesting, movement, deletion, and round-trip preservation.

XML comments and processing instructions inside an otherwise plain primary candidate are not inline elements and do not fail extraction. They contribute no message bytes and are `RawOnly`; character data before and after them is concatenated in document order into the exact message value. Because a changed canonical content replacement would delete or relocate that host metadata, the complete entry is marked read-only. It still produces normally mapped lint diagnostics, including ranges on either side of a raw-only gap, while formatter write and check modes skip it under the shared read-only contract. CDATA delimiters alone do not make an entry read-only because changed writable content may safely replace them with equivalent canonical character data.

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
- Glob expansion intersects the supported input set: `.mf2` files plus opted-in catalog files. Non-opted-in JSON, YAML, Vue, XLIFF, or other host-format files matched by a glob are not resource inputs and are skipped by discovery.
- An explicit file operand that is neither a `.mf2` file nor an opted-in catalog follows the existing unsupported-input handling of the shared discovery contract.

### Stdin Selection

Explicit stdin mode supports catalogs through the existing `--stdin-filepath <path>` option. The virtual path does not need to exist. A relative value resolves from the process working directory and is then slash-normalized relative to `projectRoot` when representable, using the same path identity as file-mode configuration matching and reporting.

After config loading, the CLI classifies the virtual path in this order:

1. A `.mf2` extension or MF2 direct-file classification selects the existing standalone stdin workflow.
2. Otherwise the path must match a resolved `resources.catalogs` include, survive that definition's exclude patterns, and resolve to exactly one host format under the normal override, extension, and overlap rules.
3. A non-`.mf2` path that is not opted into a catalog is `unsupported_input_file`; a catalog-like extension never opts stdin in by itself.
4. Supported-input classification happens before ignore matching, preserving the existing rule that an ignored unsupported virtual path is still an input error. After classification, normal formatter or linter ignore sources may skip the target.

Catalog stdin reads the complete UTF-8 host document from stdin and never checks or writes the virtual filesystem path. It does not apply standalone `.mf2` BOM or final-newline framing: extraction sees the exact host bytes, and formatter output is the complete validated host document. Normal formatter mode uses stdout for that host source, while `--reporter json` reserves stdout for the JSON envelope as in the existing stdin contract. Lint JSON uses the nested catalog result shape with the virtual path as `results[0].path`.

Ignored stdin still undergoes UTF-8 validation, matching the existing CLI contract. For valid UTF-8, normal formatter mode passes the original host bytes through to stdout exactly, formatter check and lint produce no human output, JSON uses the existing zero-target stdin summary, and no extraction runs. Catalog stdin keeps the existing prohibition on file, directory, or glob operands alongside `--stdin-filepath`.

For non-ignored normal formatter stdin, successful host extraction always produces one complete host document on stdout, even when one or more message entries have parser diagnostics. Writable diagnostic-free entries contribute validated replacements; parser-diagnostic and read-only entries retain their original raw values. If no entry changes, the original host source is emitted byte-for-byte. Entry diagnostics are rendered to stderr and retain exit code `1`; emitting the safely processed host source does not convert that result to success.

This is an intentional catalog specialization of standalone stdin behavior. A host extraction failure, including invalid host syntax or an unsupported candidate entry, a formatter operational error, a re-escaping failure, or a final candidate-validation failure emits no host source and exits with `2`. With `--reporter json`, stdout contains only the JSON envelope and never the host source. In stdin check mode no host source is emitted: a valid-entry difference prints the virtual path in human mode even when other entries have diagnostics, while diagnostics render to stderr and the combined result exits with `1`; diagnostics without a difference do not print the path.

### Host Parse Failures

When a catalog file cannot be parsed as its host format, the CLI reports a target-local operational error in `results[].errors` using the shared operational error shape: `kind: "input"`, `code: "resource_parse_failed"`, and the mandatory host location details defined below. The file contributes no diagnostics, is never written by fmt, and other selected files are still processed. Operational errors drive `summary.status: "error"` and exit code `2` per the Phase 3A output contract.

### Resource Error Model

`crates/intlify_resource` returns a typed `ResourceError` whose variants may remain more precise than the CLI surface. The CLI consumer performs one centralized conversion into the shared Phase 3A operational-error type; resource APIs do not construct CLI JSON values and resource failures are never parser, semantic, or lint diagnostics.

Only user-actionable resource failures receive dedicated public codes:

| Code | Kind | Exit | When |
| --- | --- | --- | --- |
| `resource_format_unsupported` | `input` | `2` | An opted-in target resolves to a known or extension-derived host format whose adapter has not shipped, or no supported adapter can be resolved. |
| `resource_parse_failed` | `input` | `2` | The selected host adapter rejects the complete host document as invalid host syntax. |
| `resource_entry_unsupported` | `input` | `2` | A candidate host string or message container cannot be represented as one MF2 message entry by the selected adapter. |
| `resource_document_unsupported` | `input` | `2` | A valid host document uses a document-level structure that the selected adapter intentionally does not support. |
| `resource_limit_exceeded` | `input` | `2` | Host bytes, extracted message bytes, entry count, or another documented resource representation exceeds its fixed addressable limit. |

The resource workflow reuses existing shared codes instead of adding aliases:

| Code | Kind | Exit | Resource workflow use |
| --- | --- | --- | --- |
| `config_validation_failed` | `config` | `2` | Invalid `resources` shape, unknown fields, invalid glob or format values, and conflicting catalog assignments. |
| `input_read_failed` | `io` | `2` | Input file read failure or non-UTF-8 host bytes before adapter extraction. |
| `output_write_failed` | `io` | `2` | A validated formatted host source cannot be written. |
| `internal_error` | `internal` | `2` | A supposedly valid adapter artifact, core span, replacement, or write-back result violates an implementation invariant. |

Initial stable `internal_error.details.reason` values owned by this layer are:

- `resource_invalid_entry_handle`: a missing, foreign, or otherwise invalid artifact-local handle reaches an internal consumer boundary.
- `resource_offset_map_invariant_failed`: an adapter attempts to publish an invalid map during extraction.
- `resource_offset_map_failed`: a supposedly valid core span cannot be mapped through a published map.
- `resource_write_back_failed`: re-escaping, replacement validation, candidate host parsing, re-extraction, entry identity preservation, or value equality fails after the entry was classified as writable.
- `resource_adapter_invariant_failed`: any other built-in adapter state or registry invariant fails.

Expected host syntax failures never use `internal_error`, while adapter-generated invalid state never uses `resource_parse_failed` merely because validation reparses host text. The CLI attaches the selected target path and, when known, the structured `{ path, occurrence }` entry key. Any non-empty resource operational-error array produces `summary.status: "error"` and exit code `2`, following the shared `2 > 1 > 0` priority.

Stable reason-specific `details` are part of the resource CLI JSON contract.

`resource_format_unsupported` uses:

```json
{
  "classificationSource": "extension",
  "format": "yaml",
  "extension": ".yaml",
  "supportedFormats": ["json"]
}
```

- `classificationSource` is required and is `"extension"`, `"config"`, or `"embedded"`.
- `format` is the normalized lowercase registry id and is omitted only when the extension, configured input, or embedded declaration cannot be associated with a known format id.
- `extension` is required, includes its leading `.`, preserves the target path's spelling, and is `""` when no extension exists.
- `outerFormat` is omitted for extension- and config-classified targets. It is required for `"embedded"` and contains the normalized registry id of the already selected outer adapter.
- `supportedFormats` is required and contains registry ids in ASCII ascending order. For extension and config classification it lists shipped top-level adapters; for embedded classification it lists the shipped inner adapters accepted by that outer adapter.
- The selected target path is the operational error's top-level `path`, not duplicated in `details`.

For example, a Vue SFC with an inline `lang="yaml"` block before the YAML adapter ships uses:

```json
{
  "classificationSource": "embedded",
  "format": "yaml",
  "extension": ".vue",
  "outerFormat": "vue",
  "supportedFormats": ["json"]
}
```

The complete `.vue` file remains the selected target and top-level error path. The error position is not added to this code's stable details; when several unsupported inline blocks exist, the outer adapter deterministically selects the lowest block source offset for its human message.

`resource_parse_failed` uses:

```json
{
  "format": "json",
  "offset": 18,
  "line": 2,
  "column": 4
}
```

- All four fields are required. A host parser dependency must provide a primary syntax-error position that its adapter can convert to an absolute UTF-8 byte offset; a parser without that capability does not satisfy the span-preserving adapter contract.
- `format` identifies the adapter layer that rejected its input. `outerFormat` is omitted for a direct host parse or an outer-layer failure; it is required for a composed inner-adapter failure and identifies the normalized outer registry id.
- `offset` is the zero-based byte offset from the beginning of the exact host document and may equal the source byte length for an unexpected end of input.
- `line` is one-based and `column` is a zero-based UTF-8 byte column, matching the shared diagnostic `SourceLocation` convention. Both are derived from the exact host source and `offset`, not copied from dependency-specific line/column semantics.
- Every raw byte before the error contributes to `offset` and the first line's byte column. An adapter that removes one leading BOM for parsing adds its three bytes back; JSON5 instead passes every `U+FEFF` whitespace instance through and counts it directly. Parser-specific error codes, expected-token sets, and prose are not stable `details`; the operational error's human `message` may summarize them without becoming a compatibility surface.

For example, invalid JSON inside a valid Vue SFC uses:

```json
{
  "format": "json",
  "outerFormat": "vue",
  "offset": 118,
  "line": 8,
  "column": 6
}
```

The position is derived after lifting the inner parser offset into the complete outer source. An invalid Vue SFC instead uses `format: "vue"` and omits `outerFormat`.

`resource_entry_unsupported` uses:

```json
{
  "format": "json",
  "reason": "message_text_unrepresentable",
  "offset": 18,
  "line": 2,
  "column": 4
}
```

- All five fields are required. Stable initial `reason` values are `"message_text_unrepresentable"`, `"structural_path_unsupported"`, and `"inline_content_unsupported"`; an adapter adds another value only with the deferred format contract that needs it.
- `format` identifies the adapter layer that rejected the candidate. `outerFormat` is omitted for a direct target or outer-layer failure and required for a composed inner-adapter failure, using the same normalized registry-id rule as `resource_parse_failed`.
- The position identifies the first host token that makes the lowest-offset candidate unsupported, such as the first unpaired-surrogate escape, the unsupported complex mapping key, or the first inline child element. If the unsupported property has no narrower token, it identifies the candidate value's raw span start.
- `offset`, `line`, and `column` use the same exact-host UTF-8 coordinate rules as `resource_parse_failed`, including retained BOM bytes and lifting through adapter composition. The operational error's top-level `path` identifies the complete outer host file.
- No `entryKey` is attached: failure occurs before a valid `MessageEntry` identity has been constructed. No extracted entries, diagnostics, formatted candidate, or partial file write are returned for that target.

`resource_document_unsupported` uses:

```json
{
  "format": "yaml",
  "feature": "multiple_documents",
  "offset": 42,
  "line": 5,
  "column": 0
}
```

- All five fields are required. Initial stable `feature` values are `"multiple_documents"`, `"yaml_version"`, `"custom_tags"`, `"aliases"`, `"merge_keys"`, `"xliff_version"`, `"xml_dtd"`, `"xml_version"`, and `"xml_encoding"`; future adapters add values only for document-wide structures that cannot be represented safely by that adapter.
- `format` identifies the adapter layer whose supported document model rejected the construct. `outerFormat` is omitted for a direct target or outer-layer failure and required for a composed inner-adapter failure.
- For a multi-document YAML stream, the position identifies the start of the second document, including its `---` marker when present. For an unsupported YAML version it identifies the `%YAML` directive. For an unsupported YAML tag, it identifies the first used non-Core tag token. For an alias it identifies `*`, and for a merge key it identifies the `<<` key token even when the merge value is itself an alias. For an unsupported XLIFF profile it identifies the `version` attribute value when present, otherwise the root `<xliff>` start. For a DTD it identifies `<!DOCTYPE`; for unsupported XML version or encoding it identifies the corresponding declaration value. When more than one unsupported construct exists, the error with the lowest primary raw source offset wins. `offset`, `line`, and `column` use the same exact-host UTF-8 coordinate rules as `resource_parse_failed`, including lifting through adapter composition.
- The operational error's top-level `path` identifies the complete outer target. No `entryKey`, partial artifact, diagnostics, formatted candidate, or partial write is returned.

`resource_limit_exceeded` uses:

```json
{
  "resource": "host_bytes",
  "limit": 4294967295,
  "actual": 4294967296
}
```

- All fields are required non-negative JSON integers.
- Initial `resource` values are `"host_bytes"`, `"message_bytes"`, and `"entries"`.
- `limit` is the maximum accepted value for that representation and `actual` is the observed value that exceeded it. The initial byte-span and entry-handle domains use `u32`, so their maximum accepted value is `u32::MAX` unless a narrower documented implementation limit is introduced later.

Resource-owned `internal_error` values require both `reason` and `phase`:

```json
{
  "reason": "resource_write_back_failed",
  "phase": "validate_write_back",
  "entryKey": {
    "path": "/greeting",
    "occurrence": 0
  }
}
```

Stable initial `phase` values are `"registry"`, `"extract"`, `"map"`, `"reescape"`, and `"validate_write_back"`. `entryKey` uses the normal structured identity and is included when one entry is known; it is omitted for document-wide failures or failures before entry identity exists. Source text, raw replacement text, dependency-specific errors, and internal adapter type names are never included. Additional structured fields require a separately documented stable need rather than leaking arbitrary Rust debug data.

### Determinism and Parallelism

File results are reported in stable normalized path order; entries within a file are reported in raw span order. The initial resource implementation does not introduce intra-file parallelism and follows the consumer CLI's existing file execution policy. Future concurrency first parallelizes across files. The resource crate itself does not create a thread pool; scheduling belongs to the CLI consumer so all work shares one bounded global pool.

Intra-file execution is reconsidered only after file-level parallelism exists and benchmarks show that a single very large catalog leaves that pool materially underutilized. The future unit is a contiguous chunk of entries in raw span order, not one task per entry and not a host-format syntax subtree. Chunking amortizes scheduling overhead for small messages, stays host-format-neutral, and lets a single catalog share the same worker pool without nested parallelism or oversubscription.

Chunk results carry their original entry indices and are reassembled in raw span order before reporting or write-back. Formatter replacement composition remains a single deterministic file-level step after all chunks succeed. If concurrent entry work produces more than one operational error, the error for the lowest raw-order entry is selected regardless of task completion order; the existing no-partial-result and no-partial-write rules still apply. Exact chunk size, minimum entry count or measured-work threshold, and worker scheduling strategy are benchmark-tuned internal details rather than config fields or CLI compatibility surfaces.

Benchmarks report extraction, re-escaping, core calls, scheduling overhead, and file I/O as separate phases. Intra-file parallelism may land only when representative large-catalog benchmarks demonstrate a repeatable improvement after scheduling and ordered aggregation costs are included.

### Catalog JSON Result Layout

The JSON reporter keeps one `results[]` item per selected host file and nests complete entry results under catalog targets. A standalone message result requires `diagnostics` and omits `entries`, preserving the Phase 3B and Phase 3C shape. A catalog result requires `entries` and omits the file-level `diagnostics` field; a typed Rust result DTO represents these as mutually exclusive enum variants. Diagnostic objects are not duplicated between file and entry levels and do not gain resource-specific optional fields.

While `schemaVersion` remains `"0"`, the project does not generate, commit, or publish command-output JSON Schemas, including internal test-only schemas. The configuration JSON Schema is a separate compatibility surface and remains committed and published. The typed Rust enum/DTO and exact JSON fixtures define and verify the output variants, required and omitted fields, field order, and summary aggregation. Publishing an output JSON Schema is reconsidered only after `schemaVersion` becomes stable.

On successful extraction and complete per-entry processing, `entries` contains every extracted message entry in raw span order, including clean, unchanged, diagnostic-bearing, and read-only entries. `displayKey` is omitted when an adapter has no display identity. The linter variant is:

```json
{
  "path": "locales/en.json",
  "status": "problems",
  "entries": [
    {
      "key": {
        "path": "/greeting",
        "occurrence": 0
      },
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
      "key": {
        "path": "/greeting",
        "occurrence": 0
      },
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

Entry results do not contain an `errors` array. A host-level operational error stays in the file result's `errors`. A per-entry operational error also makes the catalog result incomplete under the failure rules below: the file has `status: "error"`, its `entries` array is empty, and the file-level error carries `details.entryKey` with the same `{ path, occurrence }` object when known. This avoids presenting partial entry results as a complete catalog result.

Every diagnostic `span`, `location`, and `labels[].span` in a catalog entry result uses mapped host-document coordinates. `span` and label spans are absolute UTF-8 byte ranges in the complete host document, and `location` is derived from that document. The JSON result does not expose a parallel `messageSpan` or message-local location. Message-local spans remain available internally in the core diagnostic result before `MessageOffsetMap` mapping; keeping one public coordinate space prevents consumers from accidentally applying a message-local range to the host file. A future concrete consumer that needs both coordinate spaces requires an explicit schema addition rather than overloading `span`.

## Catalog Linting

Catalog linting runs the message-level linter per entry and aggregates per file.

- Each entry's message text goes through `lintMessage(source, options)` with the same resolved lint configuration used for `.mf2` files. The strict `parser -> semantic -> rules` pipeline applies per entry.
- Entries are independent: parser diagnostics in one entry never suppress semantic validation, linting, or reporting for other entries.
- Entry diagnostics are reported exclusively with host-file coordinates in JSON: the mapped host UTF-8 byte span and derived line/column, produced through the entry offset map. Each entry-level result carries its entry key; a display key may accompany it for human-readable output. Message-local spans remain internal and are not serialized alongside the mapped spans.
- If any per-entry linter call returns an operational error instead of a complete diagnostic result, the catalog target follows the existing target-level linter error contract: it reports `status: "error"`, an empty `entries` array, and the error in `results[].errors`, with `details.entryKey` when the entry is known. Diagnostics already collected from other entries in that catalog are discarded because the target result is incomplete; other selected files still continue.
- Entry-level results use the nested catalog JSON result DTO above as an extension of the linter JSON result contract, inside the existing envelope, summary, and count conventions of [008-ox-mf2-phase-3c-linter-design.md](./008-ox-mf2-phase-3c-linter-design.md). Diagnostic counts and `--max-warnings` include `entries[].diagnostics`.
- Lint options apply uniformly; there are no per-catalog rule overrides initially. The evidence-gated reconsideration policy below applies to both lint and formatter options.

## Catalog Formatting

Catalog formatting runs the message-level formatter per entry and composes write-back edits per file.

- Each writable entry's message text goes through `formatMessage(source, options?)` with the same resolved fmt configuration used for `.mf2` files.
- Write mode: for every writable, syntactically valid entry whose formatted output differs, the artifact re-escapes the formatted text into a `RawReplacement`. The complete set is passed once to `apply_and_validate`, which applies edits in descending raw-span offset order and returns a reparsed, re-extracted, value-verified host source. The CLI writes that returned string once, using the Phase 3B CLI's file-I/O and operational-error conventions but not its standalone `.mf2` file framing. All bytes outside replaced value spans remain byte-identical, including any host-file BOM and trailing line ending: no key reordering, no indentation or quoting changes outside message values, and no host layout normalization.
- Entries whose extracted text has parser diagnostics are skipped per entry and reported with the same strict invalid-syntax semantics that [007-ox-mf2-phase-3b-formatter-design.md](./007-ox-mf2-phase-3b-formatter-design.md) fixes for invalid standalone files, scoped to the entry. Other entries still format. One broken message must not block formatting of a large catalog; this mirrors the editor behavior in [009-ox-mf2-phase-3d-lsp-editor-design.md](./009-ox-mf2-phase-3d-lsp-editor-design.md).
- For catalog targets, the formatting unit affected by parser diagnostics is the entry, not the whole host file. A diagnostic-bearing entry is never replaced, but the same catalog file may still be written once when other entries produce valid changes. This is the resource-layer specialization of Phase 3B's rule that parser diagnostics do not modify the affected formatting unit.
- Read-only entries are excluded from formatting by definition: write mode skips them and `--check` does not count them as changed. Complete JSON output records each valid read-only entry as `status: "skipped"`, `changed: false`, and `readOnly: true`, but human output, summary counts, file status, and the process exit code do not report or change because of the skip.
- `--check` and `--list-different` report a catalog file as different when at least one writable, syntactically valid entry differs after formatting.
- If a per-entry formatter call, writable-entry re-escaping step, or final `apply_and_validate` call returns an operational error, the catalog produces no write and no partial formatting result. Its file result uses `status: "error"`, `changed: false`, an empty `entries` array, and the operational error in `results[].errors`, with `details.entryKey` when known. A re-escaping or candidate-validation failure after an entry was classified as writable is an `internal_error` with `details.reason: "resource_write_back_failed"`. Other selected files still continue.
- Idempotency: the validated write-back round-trip law plus message-level formatter idempotency imply that formatting an already formatted catalog produces no writes. Fixtures lock this.

A catalog may contain both valid changed entries and parser-diagnostic entries. The catalog has entry diagnostics when at least one `entries[].diagnostics` array is non-empty; this aggregate condition is used for file status, summary counts, and exit status without adding or duplicating a file-level `diagnostics` field. The file-level JSON result keeps the Phase 3B status enum and applies this precedence after successful, non-operational processing:

1. In write mode, any composed byte change uses `status: "formatted"` and `changed: true`, even when one or more entries have diagnostics; the file is written once and the command still exits with `1` because diagnostics remain.
2. In check mode, any composed byte difference uses `status: "would_format"` and `changed: true`, even when one or more entries have diagnostics; no file is written.
3. When no writable entry differs but one or more entries have diagnostics, the result uses `status: "diagnostic"` and `changed: false`.
4. When no writable entry differs, every entry diagnostic array is empty, and no operational error exists, the result uses `status: "unchanged"` and `changed: false`.

Accordingly, `formattedFiles` or `differentFiles` and `diagnosticFiles` may count the same catalog file; these summary counts are intentionally not a partition of `matchedFiles`. `diagnosticFiles` counts a catalog once when any entry diagnostic array is non-empty, and `diagnosticCount` is the sum of all mapped diagnostics in those arrays. Entry diagnostics or check differences keep `summary.status: "failure"` and exit code `1` unless an operational error raises the result to exit code `2`.

## Editor Consumption

For project-configured catalog documents, editor adapters consume the same registry, extraction, offset maps, and write-back re-escaping through this shared layer, so membership, host format classification, and mapping are identical in CI and in the editor. Project resolution runs first and is authoritative; editor-only ad-hoc opt-in is additive only for unmatched documents under the overlap rules above. An ad-hoc target still uses the same host adapter contract, but its membership is intentionally editor-local until persisted in project configuration. Standalone `.mf2` documents continue through the editor-owned adapter defined by [009-ox-mf2-phase-3d-lsp-editor-design.md](./009-ox-mf2-phase-3d-lsp-editor-design.md), not catalog extraction. Surface behavior stays intentionally different: editors convert mapped spans to editor position encodings, publish per document, retain previous diagnostics across transient host parse failures, and no-op on stale document versions, while the CLI reports host parse failures as target-local operational errors and writes files directly.

## Catalog-Level Checks

Catalog-level and cross-locale checks — duplicate catalog keys, key parity across locales, missing or unused translations — operate across entries and files above this layer. A within-file duplicate-key rule groups complete concrete structural paths without occurrence and therefore operates without locale metadata. Cross-locale rules group `CatalogKey` values and require the locale binding and comparison scope below, while an unused-translation rule may require additional product-specific reference input. Their rule identities, inputs, presets, and severities belong to the linter product design track. They are future work, and they must flow through the same entry model rather than introducing a second extraction path.

The logical subject of a catalog finding is a typed variant under one comparison scope. A concrete-path subject carries `StructuralPathKey` for a host duplicate rule; a catalog-message subject carries `CatalogKey` for cross-locale rules. Occurrence is absent from both subject variants. Any concrete evidence references the normalized host path and complete `{ path, occurrence }` `EntryKey`. A duplicate-key finding can therefore reference every duplicate occurrence of one concrete role without conflating XLIFF source and target, while a missing-locale finding identifies the absent locale and entries that establish the side-free catalog key elsewhere.

A catalog finding has no owning host file. The linter must not attach a missing-locale finding to an arbitrary existing entry, synthesize a nonexistent entry or zero-length host span, or duplicate one logical finding across all related entries. The current `results[]` file variants and `entries[].diagnostics` remain the contract for diagnostics that belong to concrete message entries only.

When the first catalog-level rule ships, its linter design adds a dedicated catalog-level result and finding collection beside the file results. That design must define the serialized comparison-scope identity, typed subject variant and key, affected locale or locales, concrete related-entry references, optional real source spans, deterministic ordering, human and editor presentation, `--quiet` and `--max-warnings` behavior, and summary counting. One logical finding is counted once regardless of its number of related entries. No placeholder JSON fields, Rust reporter variants, or output schemas are added before those rules and their complete output contract land together.

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

`locale` remains absent from the initial resource schema and Rust `CatalogConfig`; the field and enum land together with their first catalog-level consumer. Once present, strict unknown-field validation rejects unknown binding variants, a string `locale` value, and variant-inappropriate fields.

When XLIFF host-locale catalog checks land, the schema and enum add a fieldless `Host` variant in that same milestone:

```jsonc
{
  "include": ["locales/**/*.xlf"],
  "format": "xliff",
  "locale": { "from": "host" }
}
```

```rust
pub enum LocaleBindingConfig {
    // Existing Path and Fixed variants.
    Host,
}
```

`from: "host"` resolves locale per entry rather than once per file, using the XLIFF source/target rules above. It is not accepted as a placeholder before both the XLIFF metadata provider and its first catalog-level consumer ship. A catalog definition using it may match only adapters with a documented intrinsic-locale provider; matching another format is `config_validation_failed` at that definition rather than a fallback to path inference. If an XLIFF entry has no non-empty locale after its profile-specific inheritance, the future catalog workflow reports an explicit target-local operational error and does not silently exclude that entry; the linter design must add that error contract when the consumer lands.

For `from: "path"`, `pattern` is matched against the slash-normalized, `projectRoot`-relative file path and must contain exactly one literal `{locale}` capture token. The token captures one or more characters without crossing a `/` boundary; it may have literal prefix or suffix text in the same segment, as in `{locale}.json`. To keep capture identity unambiguous, the segment containing `{locale}` cannot contain another variable-width glob construct. Other path segments may use the shared glob syntax. Invalid capture-pattern structure is a configuration validation failure, and a selected catalog path must have exactly one full-pattern match when a locale-aware workflow resolves it.

Path captures, `fixed.value`, and host-provided entry locales must resolve to a non-empty string. The resource layer preserves that string byte-for-byte: it does not trim whitespace, change case, replace `_` with `-`, validate BCP 47 or Unicode locale syntax, or canonicalize deprecated subtags. Locale identity comparison is exact and case-sensitive. Standards conformance and canonical-spelling advice belong to future configurable linter rules, so project- or runtime-specific locale identifiers remain representable without changing resource extraction.

The resource layer resolves locale identity as consumer-neutral entry metadata. The future catalog linter groups that metadata with entry identity and a comparison scope, then owns rule definitions, severity, and reporting. Locale binding does not select a runtime locale, define locale fallback, change message text, or enable rules by itself.

Each locale-bound `CatalogConfig` is one comparison scope by default. All files matched by that definition are therefore treated as parts of the same logical message collection, and cross-locale grouping uses the tuple of that implicit scope, the exact resolved locale string, and `CatalogKey`. The complete `EntryKey` continues to identify each reported occurrence inside that group. Projects with independent message namespaces split them into separate catalog definitions so identical catalog keys do not create false cross-file relationships.

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

If one file is matched by multiple locale-aware definitions, de-duplication is valid only when they resolve to the same host format, comparison scope, and exact locale assignment for every extracted entry. For path and fixed bindings that assignment is one repeated file locale; for host binding it is the ordered per-entry locale sequence. Different assignments or scopes are a configuration conflict rather than placing one physical entry into multiple comparison groups. This extends the existing overlapping-definition conflict rule once locale-aware configuration lands.

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
- `resource_write_back`: write-back re-escaping, edit composition, and full candidate reparse/re-extraction validation
- `fmt_catalog_check_e2e` and `fmt_catalog_write_e2e`
- `lint_catalog_e2e`
- large single-catalog sequential and candidate chunked execution, including scheduling and ordered-aggregation overhead before intra-file parallelism may graduate
- catalog-shaped cache scenarios from [ox-mf2-parse-artifact-cache.md](./ox-mf2-parse-artifact-cache.md), such as entries with unchanged message text across host file edits

Extraction and re-escaping costs must be reported separately from parser, semantic, rule, formatter, and file I/O costs.

## Validation

- Extraction fixtures per host format, landing with each format's tier. The Tier 1 JSON set covers string unescaping, JSON Pointer identity including `"a.b"` versus nested `a` → `b`, `{ path, occurrence }` identity for unique and duplicate keys including duplicate ancestors, array elements, optional single-BOM acceptance and absolute span adjustment, repeated/out-of-place BOM rejection, trailing-line-ending preservation, quote-inclusive raw value spans, unchanged raw-spelling preservation, every canonical changed-value escape branch, direct non-ASCII plus `U+2028`/`U+2029` output, and deterministic `resource_entry_unsupported` reporting for an unpaired surrogate. Deferred tiers add their own sets when they land, such as typed YAML path identity including string-key/integer-key/sequence-index collisions, YAML scalar styles, anchors and aliases, block scalar read-only marking, complex-key unsupported-entry reporting, and XLIFF inline-content unsupported-entry reporting.
- Tier 2 Vue composition fixtures cover inline JSON with a missing `lang`, exact language-id and `yml` alias handling, permanent `jsonc` inner-language rejection even after the top-level adapter ships, external `src` blocks producing no SFC entries, unsupported inner formats failing without partial extraction, multiple-block document-wide occurrence numbering, outer and inner parse, entry-unsupported, and document-unsupported attribution with absolute SFC coordinates, composed offset maps, and validated write-back that changes only inner value spans.
- Tier 3 JSONC and JSON5 fixtures cover the exact accepted and rejected grammar differences, JSONC comments and trailing commas with otherwise strict JSON syntax, preservation of all comment and comma bytes, Standard JSON5 1.0.0 identifiers and resolved key collisions, comments, trailing commas, expanded numbers as non-entries, arbitrary-position `U+FEFF` whitespace, single- and double-quoted values, every escape and line-terminator continuation map, unpaired-surrogate unsupported-entry reporting, unchanged raw spelling, both quote-preserving canonical serializer branches, and full candidate reparse/value verification.
- Tier 3 YAML fixtures cover fixed 1.2 Core resolution and explicit tags, accepted and unsupported version directives, optional single-BOM lifting and preservation, empty and single-document streams, multi-document rejection, custom tags, aliases and merge keys, typed-pointer escaping and numeric canonicalization, duplicate paths and ancestors, every scalar style, `LF`/`CRLF`/`CR` folding and continuation maps, block indentation and chomping maps, read-only block reporting, original-style changed-value attempts, every canonical double-quoted escape branch, and full candidate reparse/value verification.
- Tier 3 XLIFF fixtures cover every exact version/namespace profile and mismatch, namespace-prefix independence, primary-context selection, source/target concrete role identity with a shared side-free catalog key, attribute and ordinal path segments, duplicate identities, per-entry host locale inheritance, XML 1.0 and UTF-8 declarations, one retained BOM, DTD rejection without I/O or expansion, predefined and numeric references, invalid named entities, exact whitespace and line-ending maps, mixed text and CDATA, canonical changed XML text including `]]>` and `CR`, self-closing expansion, comments and processing instructions as read-only raw gaps, inline-element unsupported-entry errors, workflow-metadata preservation, and full-document reparse/value verification.
- Round-trip tests: for writable constructs, extracting the re-escaped output yields exactly the formatted message text.
- Extraction-artifact tests: source ownership, read-only entry access, structural and catalog key access and invariants, artifact-local handle validation, immutable `Send + Sync` state, concurrent deterministic re-escaping, rejection of read-only re-escape calls, foreign/duplicate/overlapping replacement rejection, complete candidate reparse and re-extraction, exact entry-key and catalog-key sequence preservation, replaced-value equality, and unreplaced-value preservation.
- Offset map fixtures: message-local spans map to expected host spans across single escapes, compound surrogate-pair escapes, and raw-only delimiters, including the escape-boundary rule.
- CLI end-to-end fixtures: lint and fmt write/check over catalog fixtures, including broken-entry and broken-host-file cases, every resource-specific public error code and internal reason, shared I/O/config code reuse, mixed changed-plus-diagnostic formatter results and overlapping summary counts, per-entry operational failures with no partial file write, deterministic ordering, complete nested entry arrays, host-coordinate-only diagnostic JSON shapes, and JSON-only read-only skip reporting.
- Configuration fixtures: `resources` section validation, exact canonical format ids, rejection of alias, uppercase, whitespace-padded, and list-like `format` values, OS-independent case-insensitive extension mapping and alias pairs, preserved extension spelling in errors, unshipped `format` values and `resource_format_unsupported` targets, overlapping-definition conflicts, and empty-section behavior. When locale-aware catalog linting lands, its fixtures additionally cover the tagged binding variants, capture-pattern validation, path matches, exact locale identity, rejection of string shorthand, implicit definition scopes, explicit group joining, overlap conflicts across locale or scope assignments, concrete-path duplicate subjects, catalog-key cross-locale subjects, and XLIFF source/target non-duplication with shared cross-locale identity.
- Editor classification, range, and failure fixtures: standalone `.mf2` or MF2-language-id precedence over catalog matches, project-authoritative matches, ad-hoc opt-in for unmatched and project-excluded paths, same-format de-duplication, different-format project/ad-hoc and ad-hoc/ad-hoc errors, invalid project configuration preventing overlay fallback, editor-local coverage labeling, transition to project ownership without duplicate extraction, half-open non-empty range intersection, zero-length entry point and caret selection, parse-failure diagnostic retention, actionable resource diagnostic replacement, transactional config/internal failure retention, and recovery without formatting edits from a failed state.
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

- Tier 2 Vue SFC `<i18n>` custom block support: `crates/intlify_resource` owns outer block extraction and adapter composition over the block region, per the tier notes above. An inline block whose declared language has no shipped inner adapter fails complete SFC extraction with the embedded `resource_format_unsupported` contract; external `src` blocks remain separate catalog targets.
- Tier 3 YAML catalog support (`.yaml`, `.yml`): fixed Core Schema resolution, typed structural path identity, scalar-style handling, unused anchor annotations, alias and merge-key rejection, and exact diagnostic mapping with block scalars kept read-only per the tier notes above. Writable block scalar re-escaping is a separate follow-up milestone gated by the listed round-trip and idempotency requirements.
- Tier 3 JSONC and JSON5 catalog support: the fixed comments-plus-trailing-comma JSONC profile reuses JSON string behavior, while Standard JSON5 1.0.0 adds its exact whitespace, identifier, string, mapping, and quote-preserving write-back rules above.
- Tier 3 XLIFF 1.2 / 2.x support: exact versioned profiles, secure XML parsing, entity and CDATA mapping, exact character-data whitespace, primary source/target extraction, and read-only handling for comments or processing instructions. Inline-element support requires a separately designed, explicit XLIFF profile with protected-edit and lossless round-trip semantics.
- Other Tier 3 interchange formats such as ARB, gettext PO, and Java properties remain demand-driven candidates.
- Resource N-API and WASM binding packages are not planned at this time. Distribution stays the workspace-internal `crates/intlify_resource` crate; package distribution is reconsidered only if a concrete non-Rust consumer needs direct access to this layer.
- Key selectors, locale binding, and catalog-level checks remain future configuration and linting work tracked in the sections of this document. Locale binding uses the fixed discriminated object and comparison-scope rules above when that milestone lands; `locale` and `group` are not reserved in the initial schema.
- Per-catalog formatter and linter option overrides remain unscheduled and evidence-gated; no placeholder fields are accepted before the reconsideration requirements above are met.
- Intra-file parallelism remains disabled until file-level parallelism and representative benchmarks justify shared-pool, contiguous-entry chunking under the determinism rules above.

## Open Questions

No resource catalog adapter open questions remain at this design level. Deferred implementation work and evidence-gated reconsiderations are tracked in the sections above.
