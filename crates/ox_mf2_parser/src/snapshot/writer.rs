// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Phase 1 `ParseResult` / `BatchParseResult` → Binary AST snapshot
//! encoder.
//!
//! The writer first builds section-local byte buffers, then hands them
//! to [`crate::snapshot::sections::SnapshotAssembler`] for final layout
//! computation, header emission, padding, and section payload copy.

use crate::api::{
    parse_batch as run_parse_batch, parse_source as run_parse_source, BatchParseOptions,
    BatchParseResult, ParseInput, ParseOptions, ParseResult, ParseSessionResult,
};
use crate::diagnostic::{Diagnostic, DiagnosticCode, MESSAGE_REF_CATALOG};
use crate::snapshot::error::SnapshotWriteError;
use crate::snapshot::format::{
    checked_u32, write_u16_le, write_u32_le, write_u8, RootId, SectionKind, StringId,
    EDGE_KIND_NODE, EDGE_KIND_TOKEN, NONE_REF,
};
use crate::snapshot::sections::{EmittedSection, SnapshotAssembler};
use crate::snapshot::source_map::SourceMap;
use crate::snapshot::string_table::StringTableBuilder;
use crate::source::{SourceFile, SourceStore};
use crate::span::SourceId as PhaseOneSourceId;
use crate::tables::CstTables;

/// Snapshot encoding options.
///
/// These flags only control which already-produced parser data is
/// encoded into the snapshot bytes. They never change MF2 parser
/// semantics, and they do not change the `SnapshotResult.diagnostics`
/// field returned to the caller.
#[derive(Debug, Clone, Copy)]
pub struct SnapshotOptions {
    /// Encode parser diagnostics into the snapshot. Default `true`.
    /// When `false`, the diagnostics and diagnostic labels sections
    /// are omitted, and diagnostic messages are not interned into the
    /// snapshot string table.
    pub include_diagnostics: bool,
    /// Encode source text bytes into the snapshot. Default `false`.
    /// When `false`, `SourceRecord.text` uses the canonical none
    /// sentinel and the source text data section is omitted.
    pub include_source_text: bool,
    /// Encode trivia records into the snapshot. Default `true`. When
    /// `false`, the trivia section is omitted and `TokenRecord`
    /// trivia ranges are written as `0`.
    pub include_trivia: bool,
}

impl Default for SnapshotOptions {
    fn default() -> Self {
        Self {
            include_diagnostics: true,
            include_source_text: false,
            include_trivia: true,
        }
    }
}

/// Owned result of a single-input snapshot encode.
#[derive(Debug, Clone)]
pub struct SnapshotResult {
    /// Snapshot wire bytes.
    pub bytes: Vec<u8>,
    /// Snapshot-local root id of this input. v0.1 single-input
    /// snapshots always have `root.raw() == 0`.
    pub root: RootId,
    /// Parser diagnostics for caller convenience. Always returned,
    /// even when `SnapshotOptions.include_diagnostics = false`.
    pub diagnostics: Vec<Diagnostic>,
}

/// Owned result of a batch snapshot encode.
#[derive(Debug, Clone)]
pub struct BatchSnapshotResult {
    pub bytes: Vec<u8>,
    pub roots: Vec<RootId>,
    pub diagnostics: Vec<Diagnostic>,
    pub execution: crate::api::BatchExecution,
    pub degraded: bool,
}

/// Encode an already-produced [`ParseResult`] into a Binary AST
/// snapshot.
///
/// The `sources` store must contain the Phase 1 SourceId carried by
/// `result.source`. `parse_result_to_snapshot` does not reparse the
/// source.
pub fn parse_result_to_snapshot(
    sources: &SourceStore,
    result: &ParseResult,
    options: SnapshotOptions,
) -> Result<SnapshotResult, SnapshotWriteError> {
    encode_single(
        sources,
        result.source,
        &result.cst,
        &result.diagnostics,
        options,
    )
}

/// Parse `source_id` from `sources` and encode the result into a
/// Binary AST snapshot in a single call.
pub fn parse_source_to_snapshot(
    sources: &SourceStore,
    source_id: PhaseOneSourceId,
    parse_options: ParseOptions,
    snapshot_options: SnapshotOptions,
) -> Result<SnapshotResult, SnapshotWriteError> {
    let result = run_parse_source(sources, source_id, parse_options);
    parse_result_to_snapshot(sources, &result, snapshot_options)
}

/// Encode a borrowed [`ParseSessionResult`] into a Binary AST snapshot.
///
/// The session's CstView, diagnostics, and source identity are read
/// in place without copying the underlying tables.
pub fn parse_session_to_snapshot(
    session: &ParseSessionResult<'_>,
    options: SnapshotOptions,
) -> Result<SnapshotResult, SnapshotWriteError> {
    // Materialise diagnostics into owned values so the encoder shape
    // is identical to the parse_result path. The records themselves
    // stay borrowed; this only copies the catalog static `&str`s and
    // resolved line/column. Cheap relative to encoding.
    let diagnostics: Vec<Diagnostic> = session.diagnostics.iter().collect();
    encode_single(
        session.cst.sources(),
        session.source,
        session.cst.tables(),
        &diagnostics,
        options,
    )
}

/// Parse `inputs` sequentially and encode the result into a single
/// shared Binary AST snapshot.
pub fn parse_batch_to_snapshot(
    inputs: &[ParseInput<'_>],
    batch_options: BatchParseOptions,
    snapshot_options: SnapshotOptions,
) -> Result<BatchSnapshotResult, SnapshotWriteError> {
    let batch = run_parse_batch(inputs, batch_options);
    parse_batch_result_to_snapshot(&batch, snapshot_options)
}

/// Encode an already-produced [`BatchParseResult`] into a shared
/// Binary AST snapshot.
pub fn parse_batch_result_to_snapshot(
    result: &BatchParseResult,
    options: SnapshotOptions,
) -> Result<BatchSnapshotResult, SnapshotWriteError> {
    let mut writer = SnapshotWriter::new(options);
    for item in &result.items {
        writer.add_root(
            &result.sources,
            item.source,
            &item.result.cst,
            &item.result.diagnostics,
        )?;
    }
    let bytes = writer.finish(&result.sources)?;
    let roots = (0..result.items.len() as u32).map(RootId::new).collect();
    let diagnostics = result
        .items
        .iter()
        .flat_map(|item| item.result.diagnostics.clone())
        .collect();
    Ok(BatchSnapshotResult {
        bytes,
        roots,
        diagnostics,
        execution: result.execution,
        degraded: result.degraded,
    })
}

fn encode_single(
    sources: &SourceStore,
    source: PhaseOneSourceId,
    cst: &CstTables,
    diagnostics: &[Diagnostic],
    options: SnapshotOptions,
) -> Result<SnapshotResult, SnapshotWriteError> {
    let mut writer = SnapshotWriter::new(options);
    writer.add_root(sources, source, cst, diagnostics)?;
    let bytes = writer.finish(sources)?;
    Ok(SnapshotResult {
        bytes,
        root: RootId::new(0),
        diagnostics: diagnostics.to_vec(),
    })
}

// ── internal writer state ────────────────────────────────────────────

struct PendingRoot {
    source_local: u32,
    root_node: u32,
    /// Offset into the snapshot diagnostics array.
    diag_start: u32,
    diag_count: u32,
}

struct SnapshotWriter {
    options: SnapshotOptions,
    string_table: StringTableBuilder,
    source_map: SourceMap,
    nodes_bytes: Vec<u8>,
    nodes_count: u32,
    edges_bytes: Vec<u8>,
    edges_count: u32,
    tokens_bytes: Vec<u8>,
    tokens_count: u32,
    trivia_bytes: Vec<u8>,
    trivia_count: u32,
    diagnostics_bytes: Vec<u8>,
    diagnostics_count: u32,
    diagnostic_labels_bytes: Vec<u8>,
    diagnostic_labels_count: u32,
    roots: Vec<PendingRoot>,
    /// Cached snapshot-local source ids of input roots, in input
    /// order. Used when emitting RootRecord wire bytes.
    root_source_locals: Vec<u32>,
}

impl SnapshotWriter {
    fn new(options: SnapshotOptions) -> Self {
        Self {
            options,
            string_table: StringTableBuilder::new(),
            source_map: SourceMap::new(),
            nodes_bytes: Vec::new(),
            nodes_count: 0,
            edges_bytes: Vec::new(),
            edges_count: 0,
            tokens_bytes: Vec::new(),
            tokens_count: 0,
            trivia_bytes: Vec::new(),
            trivia_count: 0,
            diagnostics_bytes: Vec::new(),
            diagnostics_count: 0,
            diagnostic_labels_bytes: Vec::new(),
            diagnostic_labels_count: 0,
            roots: Vec::new(),
            root_source_locals: Vec::new(),
        }
    }

    fn add_root(
        &mut self,
        sources: &SourceStore,
        source: PhaseOneSourceId,
        cst: &CstTables,
        diagnostics: &[Diagnostic],
    ) -> Result<(), SnapshotWriteError> {
        if sources.get(source).is_none() {
            return Err(SnapshotWriteError::InvalidSourceId);
        }
        let source_local = self.source_map.intern(source)?;

        // Trivia first so token records can reference snapshot-local
        // trivia ids without a second pass. With include_trivia=false
        // we still walk parser trivia to fill the remap with NONE_REF
        // so the per-token leading/trailing ranges encode as `0`.
        let trivia_remap = self.emit_trivia(sources, cst)?;
        // Tokens next: token records reference trivia ranges.
        let token_remap = self.emit_tokens(sources, cst, &trivia_remap)?;
        // Nodes / edges share a single post-order pass: every edge
        // refers to either a node id or token id, and node order
        // follows parser post-order so the parser root is the last
        // node.
        let node_remap = self.emit_nodes_and_edges(cst, &token_remap)?;

        let root_node = match cst.root_id() {
            Some(parser_root) => node_remap[parser_root.index()],
            None => return Err(SnapshotWriteError::MissingRoot),
        };

        let diag_start = self.diagnostics_count;
        for diag in diagnostics {
            self.emit_diagnostic(diag)?;
        }
        let diag_count = self
            .diagnostics_count
            .checked_sub(diag_start)
            .expect("diagnostics_count only grows");

        // `node_remap` / `token_remap` / `trivia_remap` are consumed
        // entirely inside this call: tokens and trivia are emitted
        // before nodes, and nodes remap their child edges against the
        // already-populated maps. Drop them at scope end.
        let _ = node_remap;
        let _ = token_remap;
        let _ = trivia_remap;

        self.roots.push(PendingRoot {
            source_local,
            root_node,
            diag_start,
            diag_count,
        });
        self.root_source_locals.push(source_local);
        Ok(())
    }

    fn emit_trivia(
        &mut self,
        sources: &SourceStore,
        cst: &CstTables,
    ) -> Result<Vec<u32>, SnapshotWriteError> {
        let trivia_count = cst.trivia_count();
        let mut remap = Vec::with_capacity(trivia_count);
        if !self.options.include_trivia || trivia_count == 0 {
            remap.resize(trivia_count, NONE_REF);
            return Ok(remap);
        }
        for trivia in &cst.trivia {
            let local = self.next_trivia_id()?;
            let source = PhaseOneSourceId::new(trivia.source_id);
            if sources.get(source).is_none() {
                return Err(SnapshotWriteError::InvalidSourceId);
            }
            let source_local = self.source_map.intern(source)?;
            write_u16_le(&mut self.trivia_bytes, trivia.kind);
            write_u16_le(&mut self.trivia_bytes, 0); // flags
            write_u32_le(&mut self.trivia_bytes, trivia.span_start);
            write_u32_le(&mut self.trivia_bytes, trivia.span_end);
            write_u32_le(&mut self.trivia_bytes, source_local);
            remap.push(local);
        }
        Ok(remap)
    }

    fn emit_tokens(
        &mut self,
        sources: &SourceStore,
        cst: &CstTables,
        trivia_remap: &[u32],
    ) -> Result<Vec<u32>, SnapshotWriteError> {
        let mut remap = Vec::with_capacity(cst.token_count());
        for token in &cst.tokens {
            let local = self.next_token_id()?;
            let source = PhaseOneSourceId::new(token.source_id);
            if sources.get(source).is_none() {
                return Err(SnapshotWriteError::InvalidSourceId);
            }
            let source_local = self.source_map.intern(source)?;

            let (leading_start, leading_count, trailing_start, trailing_count) =
                if self.options.include_trivia
                    && (token.leading_trivia_count != 0 || token.trailing_trivia_count != 0)
                {
                    let lead_start_parser = token.first_trivia;
                    let trail_start_parser = lead_start_parser + token.leading_trivia_count as u32;
                    let lead_start_snap = if token.leading_trivia_count == 0 {
                        0
                    } else {
                        trivia_remap[lead_start_parser as usize]
                    };
                    let trail_start_snap = if token.trailing_trivia_count == 0 {
                        0
                    } else {
                        trivia_remap[trail_start_parser as usize]
                    };
                    (
                        lead_start_snap,
                        token.leading_trivia_count as u32,
                        trail_start_snap,
                        token.trailing_trivia_count as u32,
                    )
                } else {
                    (0, 0, 0, 0)
                };

            write_u16_le(&mut self.tokens_bytes, token.kind);
            write_u16_le(&mut self.tokens_bytes, 0); // flags
            write_u32_le(&mut self.tokens_bytes, token.span_start);
            write_u32_le(&mut self.tokens_bytes, token.span_end);
            write_u32_le(&mut self.tokens_bytes, source_local);
            write_u32_le(&mut self.tokens_bytes, leading_start);
            write_u32_le(&mut self.tokens_bytes, leading_count);
            write_u32_le(&mut self.tokens_bytes, trailing_start);
            write_u32_le(&mut self.tokens_bytes, trailing_count);
            write_u32_le(&mut self.tokens_bytes, 0); // reserved_tail
            remap.push(local);
        }
        Ok(remap)
    }

    fn emit_nodes_and_edges(
        &mut self,
        cst: &CstTables,
        token_remap: &[u32],
    ) -> Result<Vec<u32>, SnapshotWriteError> {
        let mut remap = Vec::with_capacity(cst.node_count());
        for node in &cst.nodes {
            let snapshot_node_id = self.next_node_id()?;
            let child_start = self.edges_count;
            // Edges first — node references in edges always point at
            // ids that are smaller than the current node id (post-order),
            // so `remap` already contains them.
            for edge in cst.edges_for(node) {
                let snapshot_edge_id = self.next_edge_id()?;
                let _ = snapshot_edge_id;
                match edge.kind {
                    k if k == EDGE_KIND_NODE => {
                        let snap_id = remap[edge.ref_id as usize];
                        write_u16_le(&mut self.edges_bytes, EDGE_KIND_NODE);
                        write_u16_le(&mut self.edges_bytes, 0); // flags
                        write_u32_le(&mut self.edges_bytes, snap_id);
                    }
                    k if k == EDGE_KIND_TOKEN => {
                        let snap_id = token_remap[edge.ref_id as usize];
                        write_u16_le(&mut self.edges_bytes, EDGE_KIND_TOKEN);
                        write_u16_le(&mut self.edges_bytes, 0); // flags
                        write_u32_le(&mut self.edges_bytes, snap_id);
                    }
                    _ => {
                        // Phase 1 builder never produces other edge
                        // kinds; treat as an internal invariant
                        // violation by remapping the edge as a token
                        // reference. (Decoder will reject if reached.)
                        return Err(SnapshotWriteError::InvalidSourceId);
                    }
                }
            }
            let child_count = self
                .edges_count
                .checked_sub(child_start)
                .expect("edges_count only grows");

            write_u16_le(&mut self.nodes_bytes, node.kind);
            write_u16_le(&mut self.nodes_bytes, 0); // flags
            write_u32_le(&mut self.nodes_bytes, node.span_start);
            write_u32_le(&mut self.nodes_bytes, node.span_end);
            write_u32_le(&mut self.nodes_bytes, child_start);
            write_u32_le(&mut self.nodes_bytes, child_count);
            write_u32_le(&mut self.nodes_bytes, NONE_REF); // data_ref
            remap.push(snapshot_node_id);
        }
        Ok(remap)
    }

    fn emit_diagnostic(&mut self, diagnostic: &Diagnostic) -> Result<(), SnapshotWriteError> {
        let source_local = self.source_map.intern(diagnostic.source)?;
        let label_start = self.diagnostic_labels_count;
        for label in &diagnostic.labels {
            let label_source = self.source_map.intern(label.source)?;
            let msg_id = if self.options.include_diagnostics {
                self.string_table.intern(label.message)?
            } else {
                StringId::NONE
            };
            self.next_diagnostic_label_id()?;
            write_u32_le(&mut self.diagnostic_labels_bytes, label_source);
            write_u32_le(&mut self.diagnostic_labels_bytes, label.span.start);
            write_u32_le(&mut self.diagnostic_labels_bytes, label.span.end);
            write_u32_le(&mut self.diagnostic_labels_bytes, msg_id.raw());
        }
        let label_count = self
            .diagnostic_labels_count
            .checked_sub(label_start)
            .expect("label count only grows");

        let message_id = if self.options.include_diagnostics {
            self.string_table.intern(diagnostic.message)?
        } else {
            StringId::NONE
        };

        self.next_diagnostic_id()?;
        write_u32_le(&mut self.diagnostics_bytes, source_local);
        write_u32_le(&mut self.diagnostics_bytes, diagnostic.span.start);
        write_u32_le(&mut self.diagnostics_bytes, diagnostic.span.end);
        write_u8(&mut self.diagnostics_bytes, diagnostic.severity as u8);
        write_u8(&mut self.diagnostics_bytes, 0); // reserved
        write_u16_le(&mut self.diagnostics_bytes, diagnostic.code.as_u16());
        write_u32_le(&mut self.diagnostics_bytes, message_id.raw());
        write_u32_le(&mut self.diagnostics_bytes, label_start);
        write_u32_le(&mut self.diagnostics_bytes, label_count);
        Ok(())
    }

    fn finish(self, sources: &SourceStore) -> Result<Vec<u8>, SnapshotWriteError> {
        let Self {
            options,
            mut string_table,
            source_map,
            nodes_bytes,
            nodes_count,
            edges_bytes,
            edges_count,
            tokens_bytes,
            tokens_count,
            trivia_bytes,
            trivia_count,
            diagnostics_bytes,
            diagnostics_count,
            diagnostic_labels_bytes,
            diagnostic_labels_count,
            roots,
            root_source_locals,
        } = self;

        if roots.is_empty() {
            return Err(SnapshotWriteError::MissingRoot);
        }

        // ── Sources section + optional source text data ──────────────
        let mut sources_bytes = Vec::with_capacity(source_map.len() * 32);
        let mut sources_count: u32 = 0;
        let mut source_text_bytes: Vec<u8> = Vec::new();
        let include_source_text = options.include_source_text;
        for (snapshot_local, phase_one) in source_map.iter() {
            let file = sources
                .get(phase_one)
                .ok_or(SnapshotWriteError::InvalidSourceId)?;
            let path_id = string_table.intern_optional(file.path.as_deref())?;
            let locale_id = string_table.intern_optional(file.locale.as_deref())?;
            let message_id = string_table.intern_optional(file.message_id.as_deref())?;
            let (text_source, text_offset, text_len) = if include_source_text {
                let offset = checked_u32(source_text_bytes.len())
                    .ok_or(SnapshotWriteError::SectionTooLarge)?;
                let len = file.len();
                source_text_bytes.extend_from_slice(file.text.as_bytes());
                (snapshot_local, offset, len)
            } else {
                (NONE_REF, 0, 0)
            };
            write_u32_le(&mut sources_bytes, snapshot_local);
            write_u32_le(&mut sources_bytes, path_id.raw());
            write_u32_le(&mut sources_bytes, locale_id.raw());
            write_u32_le(&mut sources_bytes, message_id.raw());
            write_u32_le(&mut sources_bytes, file.base_offset);
            // SourceTextRef { source_id, offset, len }
            write_u32_le(&mut sources_bytes, text_source);
            write_u32_le(&mut sources_bytes, text_offset);
            write_u32_le(&mut sources_bytes, text_len);
            // Force the file binding to stay live; quiets clippy when
            // SnapshotOptions doesn't include source text.
            let _ = file;
            let _: SourceFile;
            sources_count = sources_count
                .checked_add(1)
                .ok_or(SnapshotWriteError::TooManySources)?;
        }
        let _ = root_source_locals; // (debug-only invariant; remap reused via roots.source_local)

        // ── Roots section ────────────────────────────────────────────
        let roots_count = checked_u32(roots.len()).ok_or(SnapshotWriteError::TooManyRoots)?;
        let mut roots_bytes = Vec::with_capacity(roots.len() * 16);
        for root in &roots {
            write_u32_le(&mut roots_bytes, root.root_node);
            write_u32_le(&mut roots_bytes, root.source_local);
            if options.include_diagnostics {
                write_u32_le(&mut roots_bytes, root.diag_start);
                write_u32_le(&mut roots_bytes, root.diag_count);
            } else {
                write_u32_le(&mut roots_bytes, 0);
                write_u32_le(&mut roots_bytes, 0);
            }
        }

        // ── String offsets and string data ───────────────────────────
        let offsets = string_table.offsets();
        let strings_count = checked_u32(offsets.len()).ok_or(SnapshotWriteError::TooManyStrings)?;
        let mut string_offsets_bytes = Vec::with_capacity(offsets.len() * 8);
        for entry in offsets {
            write_u32_le(&mut string_offsets_bytes, entry.offset);
            write_u32_le(&mut string_offsets_bytes, entry.len);
        }
        let string_data = string_table.data().to_vec();

        // ── Assemble ─────────────────────────────────────────────────
        let mut assembler = SnapshotAssembler::new();
        assembler.push(EmittedSection {
            kind: SectionKind::Roots,
            bytes: roots_bytes,
            count: roots_count,
        });
        assembler.push(EmittedSection {
            kind: SectionKind::Sources,
            bytes: sources_bytes,
            count: sources_count,
        });
        assembler.push(EmittedSection {
            kind: SectionKind::Nodes,
            bytes: nodes_bytes,
            count: nodes_count,
        });
        assembler.push(EmittedSection {
            kind: SectionKind::Edges,
            bytes: edges_bytes,
            count: edges_count,
        });
        assembler.push(EmittedSection {
            kind: SectionKind::Tokens,
            bytes: tokens_bytes,
            count: tokens_count,
        });
        // Trivia is omitted when empty (even when include_trivia=true).
        if options.include_trivia && trivia_count > 0 {
            assembler.push(EmittedSection {
                kind: SectionKind::Trivia,
                bytes: trivia_bytes,
                count: trivia_count,
            });
        }
        if options.include_diagnostics && diagnostics_count > 0 {
            assembler.push(EmittedSection {
                kind: SectionKind::Diagnostics,
                bytes: diagnostics_bytes,
                count: diagnostics_count,
            });
        }
        if options.include_diagnostics && diagnostic_labels_count > 0 {
            assembler.push(EmittedSection {
                kind: SectionKind::DiagnosticLabels,
                bytes: diagnostic_labels_bytes,
                count: diagnostic_labels_count,
            });
        }
        assembler.push(EmittedSection {
            kind: SectionKind::StringOffsets,
            bytes: string_offsets_bytes,
            count: strings_count,
        });
        assembler.push(EmittedSection {
            kind: SectionKind::StringData,
            bytes: string_data,
            count: 0,
        });
        if include_source_text && !source_text_bytes.is_empty() {
            assembler.push(EmittedSection {
                kind: SectionKind::SourceTextData,
                bytes: source_text_bytes,
                count: 0,
            });
        }

        assembler.finish()
    }

    // ── id allocation helpers ───────────────────────────────────────

    fn next_node_id(&mut self) -> Result<u32, SnapshotWriteError> {
        let id = self.nodes_count;
        self.nodes_count = self
            .nodes_count
            .checked_add(1)
            .ok_or(SnapshotWriteError::TooManyNodes)?;
        Ok(id)
    }

    fn next_edge_id(&mut self) -> Result<u32, SnapshotWriteError> {
        let id = self.edges_count;
        self.edges_count = self
            .edges_count
            .checked_add(1)
            .ok_or(SnapshotWriteError::TooManyEdges)?;
        Ok(id)
    }

    fn next_token_id(&mut self) -> Result<u32, SnapshotWriteError> {
        let id = self.tokens_count;
        self.tokens_count = self
            .tokens_count
            .checked_add(1)
            .ok_or(SnapshotWriteError::TooManyTokens)?;
        Ok(id)
    }

    fn next_trivia_id(&mut self) -> Result<u32, SnapshotWriteError> {
        let id = self.trivia_count;
        self.trivia_count = self
            .trivia_count
            .checked_add(1)
            .ok_or(SnapshotWriteError::TooManyTrivia)?;
        Ok(id)
    }

    fn next_diagnostic_id(&mut self) -> Result<u32, SnapshotWriteError> {
        let id = self.diagnostics_count;
        self.diagnostics_count = self
            .diagnostics_count
            .checked_add(1)
            .ok_or(SnapshotWriteError::TooManyDiagnostics)?;
        Ok(id)
    }

    fn next_diagnostic_label_id(&mut self) -> Result<u32, SnapshotWriteError> {
        let id = self.diagnostic_labels_count;
        self.diagnostic_labels_count = self
            .diagnostic_labels_count
            .checked_add(1)
            .ok_or(SnapshotWriteError::TooManyDiagnosticLabels)?;
        Ok(id)
    }
}

#[allow(dead_code)] // accept Phase 1 catalog sentinel so the writer can
                    // intern catalog message strings later if needed.
fn diagnostic_catalog_str(code: DiagnosticCode) -> &'static str {
    code.static_message()
}

#[allow(dead_code)]
const _ASSERT_MESSAGE_REF_CATALOG: u32 = MESSAGE_REF_CATALOG;
