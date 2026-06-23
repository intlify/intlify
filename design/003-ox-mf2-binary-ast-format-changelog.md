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
