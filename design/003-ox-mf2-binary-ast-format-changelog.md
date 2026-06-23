# ox-mf2 Binary AST Format Changelog

## Purpose

This document records intentional changes to the ox-mf2 Binary AST snapshot format.

The current snapshot format design is defined in [003-ox-mf2-phase-2-binary-ast-snapshot-design.md](./003-ox-mf2-phase-2-binary-ast-snapshot-design.md).

## v0.1 Draft

- Initial Binary AST snapshot format.
- Snapshot magic prefix is the ASCII byte sequence `OXMF2AST`.
- Fixed-size header and section table.
- SnapshotHeader is explicitly 32 bytes, including reserved fields.
- TokenRecord is explicitly 36 bytes, including reserved tail bytes.
- Header version: `major_version = 0`, `minor_version = 1`.
- v0.x decoders use exact version matching.
- v0.1 decoder accepts only `major_version = 0` and `minor_version = 1`.
- v0.1 decoder rejects unknown section kinds, even when the section is optional.
- v0.1 section flags are strict: core sections are required, known optional sections are not required, and reserved flags are zero.
- v0.1 decoder rejects non-zero reserved fields.
- Minor-version backward compatibility starts only after a future v1.0 format freeze.
- 8-byte section alignment.
- Canonical zero padding between sections.
- No trailing padding after the last emitted section.
- Explicit little-endian field encoding.
- Fixed v0.1 record sizes are defined by the snapshot design and are not derived from Rust struct layout.
- SectionKind order for deterministic writer output.
- Core sections: roots, sources, nodes, edges, tokens, string offsets, string data.
- Optional sections: trivia, diagnostics, diagnostic labels, source text data, extended data.
- Snapshot SourceId values are snapshot-local indexes into the sources section; SnapshotWriter remaps Phase 1 SourceId values during encoding.
- Required core sections are emitted even when the format permits an empty count or byte length.
- Empty optional sections are omitted by the v0.1 default writer.
- v0.1 writer does not emit extended data.
- v0.1 writer does not deduplicate SourceRecord entries or source text bytes.
- String table deduplicates metadata and diagnostics strings only.
- Source-derived text is represented by source id plus UTF-8 byte spans, not by string table entries.
- Source text data is one concatenated UTF-8 byte buffer without NUL terminators or per-entry padding.
- SourceTextRef none sentinel uses `source_id = 0xFFFF_FFFF`, `offset = 0`, and `len = 0`.
- Writer output is canonical for the same parser output, input order, SourceStore metadata, and snapshot options, but not guaranteed stable across parser/package versions before a stable format freeze.
- SyntaxKind numeric values are snapshot-visible draft data and require fixture/changelog updates when changed.
- v0.1 decoder validates SyntaxKind numeric values in node, token, and trivia records.
- Diagnostic code numeric values are snapshot-visible draft data and require fixture/changelog updates when changed.
- v0.1 decoder validates diagnostic severity and diagnostic code numeric values.
- v0.1 decoder rejects inverted spans (`span_start > span_end`) in node, token, trivia, diagnostic, and diagnostic-label records as `DecodeErrorCode::InvalidSpan` (1035).
- v0.1 decoder validates each interned string slice against UTF-8 boundaries (catches offsets that split a multibyte scalar even when the concatenated string-data buffer is valid UTF-8).
- v0.1 decoder normalises section-table read order so an empty section that shares an aligned offset with a non-empty section is never falsely rejected as overlapping.
- `DecodeErrorCode` uses `#[repr(u32)]` with explicit discriminants in the `1000..1999` API error range; the numeric values are stable across the v0.1 surface for tests, fixture validators, and language bindings.
- v0.1 writer emits the (possibly empty) `SourceTextData` section whenever `SnapshotOptions.include_source_text = true`, so empty source text round-trips back as `Some("")` instead of being lost.
- v0.1 writer skips diagnostic / diagnostic-label record encoding entirely when `SnapshotOptions.include_diagnostics = false`. `SnapshotResult.diagnostics` is still populated for caller convenience, but the snapshot bytes no longer carry diagnostic data and the source map is not polluted with diagnostic-only sources.
- v0.1 adds `parse_message_to_snapshot(source, metadata, parse_options, snapshot_options)` for the standalone (no caller `SourceStore`) path; `parse_result_to_snapshot` requires the caller to supply the same `SourceStore` the `ParseResult` was parsed against, so pairing it with a `parse_message`-derived result is no longer the documented pattern.
- v0.1 adds `SnapshotSourceMetadata` as the metadata carrier for `parse_message_to_snapshot`. It carries `path` / `locale` / `message_id` / `base_offset` only — the `source` field that `SourceFileInput` exposes is omitted so the parsed bytes and the encoded `SourceRecord.text` can never disagree.
- v0.1 routes oversized inputs through `SnapshotWriteError::SourceTooLarge` instead of letting `SourceStore::add`'s panic escape the snapshot API boundary. `parse_message_to_snapshot` uses `SourceStore::try_add`; `parse_batch_to_snapshot` pre-validates every `ParseInput.source.len()` before invoking the Phase 1 parser.
- v0.1 adds `SourceView::source_slice(span) -> Result<&str, SourceTextUnavailable>` and the `SourceTextUnavailable { NotIncluded, SpanOutOfBounds }` error so accessors distinguish "snapshot encoded without source text" from "span is out of bounds / splits a UTF-8 scalar" — see the design's source slice accessor contract.
- v0.1 writer pre-interns each batch root's source metadata before any `add_root`, so the string table emits `path` / `locale` / `message_id` strings strictly before diagnostic messages (matching the canonical writer order called out in `design/003` §"String Table").
- v0.1 writer reserves the node / edge / token / trivia / diagnostic / diagnostic-label section byte buffers from the Phase 1 `CstTables` counts at the top of `add_root`, so the hot encode path does not grow the underlying `Vec`s mid-loop on large CSTs or recovery-heavy batches.
- v0.1 writer emits exactly one `SourceRecord` per input root, even when two batch items share the same Phase 1 `SourceId`. The "no source dedup" rule from §"Source Section" is honoured at the writer boundary (previously `SourceMap::intern` deduplicated by Phase 1 id, causing multiple roots to share a single `SourceRecord`). Tokens / trivia / diagnostics inside one root continue to reference that root's snapshot-local `SourceRecord`.
- v0.1 writer collapses `Diagnostic.source` and `DiagnosticLabel.source` to the root's snapshot-local `SourceRecord`. Even when a caller-supplied `ParseResult` / `BatchParseResult` carries a diagnostic that names a different Phase 1 `SourceId`, the encoded record references the root source. The format keeps explicit `source_id` fields on `DiagnosticRecord` / `DiagnosticLabelRecord` so a future writer policy can opt into multi-source diagnostics within a root, but v0.1 writer output is strictly single-source per root.
- v0.1 writer rejects hand-crafted `BatchParseResult` items whose `BatchParseItem.source` does not match `BatchParseItem.result.source` with `SnapshotWriteError::InconsistentSourceId`. The Phase 1 `parse_batch` contract preserves the equality; the validation only fires when a caller bypasses `parse_batch` and constructs a mismatched item directly, preventing a snapshot whose `SourceRecord` metadata describes one source while spans came from another.

## Changelog Update Rule

Any change to the following surface MUST add or amend a section in this changelog **in the same commit** that introduces the change.

- snapshot magic, `major_version`, `minor_version`, or `feature_flags`
- header layout or initial header values
- `SectionKind` numeric values
- section required / optional status
- section alignment or padding rules
- record field layout or `record_size`
- `SyntaxKind` numeric values that the parser emits into snapshot records
- `DiagnosticCode` numeric values that the parser emits into diagnostic records
- decoder validation rules
- canonical writer output rules
- source text data layout
- string table interning or deduplication policy
- SourceRecord deduplication policy
- extended data payload policy

The compatibility guard tests under `crates/ox_mf2_parser/tests/snapshot_compat.rs` lock the v0.1 record sizes, section kind numeric order, default `SnapshotOptions`, edge kind numeric values, and assert that this changelog still documents the v0.1 magic, major, and minor version. A failure in those tests is the signal to update this file.

## Version Bump Checklist

Use this checklist when intentionally changing the snapshot wire format:

1. Decide whether the change is a minor (additive) or major (incompatible) bump. While `major_version = 0`, every change is treated as draft and decoders use exact version matching, so any change inside v0.x is effectively a draft bump that requires updating both the writer and the v0.x decoder.
2. Update the v0.1 section above (or open a new `v0.N` / `vM.0` heading) with one bullet per intentional change.
3. Update `design/003-ox-mf2-phase-2-binary-ast-snapshot-design.md` to describe the new format.
4. Update the Rust constants in `crates/ox_mf2_parser/src/snapshot/format.rs` and any matching decoder validation in `crates/ox_mf2_parser/src/snapshot/decoder.rs`.
5. Regenerate the binary golden fixtures with `UPDATE_SNAPSHOTS=1 cargo test -p ox_mf2_parser --test snapshot_fixtures`.
6. Re-run `vpr check` and `vpr test` to confirm the compatibility guard, fixture round-trips, and invalid fixture coverage all match the new format.
