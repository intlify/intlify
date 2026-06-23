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
    DIAGNOSTIC_LABEL_RECORD_SIZE, DIAGNOSTIC_RECORD_SIZE, EDGE_KIND_NODE, EDGE_KIND_TOKEN,
    EDGE_RECORD_SIZE, NODE_RECORD_SIZE, NONE_REF, TOKEN_RECORD_SIZE, TRIVIA_RECORD_SIZE,
};
use crate::snapshot::sections::{EmittedSection, SnapshotAssembler};
use crate::snapshot::string_table::StringTableBuilder;
use crate::source::{SourceFileInput, SourceStore};
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
/// `sources` must be the same [`SourceStore`] the result was parsed
/// against — i.e. `result` must have been produced by
/// [`parse_source`](crate::parse_source) or [`parse_batch`](crate::parse_batch)
/// using this very store. The encoder reads `SourceRecord` metadata
/// (path / locale / `message_id` / `base_offset` / optional source
/// text) from `sources.get(result.source)` and trusts the caller to
/// have kept them in sync.
///
/// **Do not pass a [`ParseResult`] from
/// [`parse_message`](crate::parse_message) here**:
/// `parse_message` does not register the source in any store and
/// always returns `SourceId::new(0)`, so pairing it with an
/// unrelated store silently encodes the wrong source metadata. Use
/// [`parse_message_to_snapshot`] for the standalone case.
///
/// `parse_result_to_snapshot` does not reparse the source.
pub fn parse_result_to_snapshot(
    sources: &SourceStore,
    result: &ParseResult,
    options: SnapshotOptions,
) -> Result<SnapshotResult, SnapshotWriteError> {
    // The owned-result path clones once, here, so the encoder body
    // can move the `Vec` straight into `SnapshotResult.diagnostics`
    // instead of doing a second `.to_vec()` inside `encode_single`.
    let diagnostics = result.diagnostics.clone();
    encode_single(sources, result.source, &result.cst, diagnostics, options)
}

/// Snapshot source metadata that pairs with
/// [`parse_message_to_snapshot`]'s `source` parameter.
///
/// Deliberately omits the `source` field that
/// [`SourceFileInput`] carries: the snapshot writer must point at
/// the same bytes the parser saw, and a metadata struct that owned
/// its own `source` would invite the caller to set two different
/// strings. Bindings and language wrappers should follow the same
/// shape.
#[derive(Debug, Default, Clone, Copy)]
pub struct SnapshotSourceMetadata<'a> {
    /// Optional filesystem path, used for diagnostics.
    pub path: Option<&'a str>,
    /// Optional BCP-47 locale tag, used for project-aware tooling.
    pub locale: Option<&'a str>,
    /// Optional logical message id (e.g. translation key).
    pub message_id: Option<&'a str>,
    /// Optional base offset, used when the source is a substring of a
    /// larger file (e.g. a single entry inside a locale resource).
    pub base_offset: Option<u32>,
}

/// Parse `source` standalone and encode the result into a Binary
/// AST snapshot. Builds a private one-entry [`SourceStore`] so
/// callers never have to construct one to pair with
/// [`parse_message`](crate::parse_message)'s
/// `SourceId::new(0)` return value.
///
/// `metadata` lets callers attach path / locale / `message_id` /
/// `base_offset` to the resulting `SourceRecord`. When `None`, the
/// snapshot's `SourceRecord` carries no metadata strings and
/// `base_offset = 0`.
pub fn parse_message_to_snapshot(
    source: &str,
    metadata: Option<SnapshotSourceMetadata<'_>>,
    parse_options: ParseOptions,
    snapshot_options: SnapshotOptions,
) -> Result<SnapshotResult, SnapshotWriteError> {
    let mut sources = SourceStore::with_capacity(1);
    let metadata = metadata.unwrap_or_default();
    let input = SourceFileInput {
        source,
        path: metadata.path,
        locale: metadata.locale,
        message_id: metadata.message_id,
        base_offset: metadata.base_offset,
    };
    // The public `parse_message_to_snapshot` signature returns
    // `Result<_, SnapshotWriteError>`, so route oversized sources
    // through `try_add` and convert the `SourceStoreError` into a
    // `SnapshotWriteError::SourceTooLarge` instead of letting
    // `SourceStore::add`'s panic escape the API boundary.
    let id = sources
        .try_add(input)
        .map_err(|_| SnapshotWriteError::SourceTooLarge)?;
    let result = run_parse_source(&sources, id, parse_options);
    parse_result_to_snapshot(&sources, &result, snapshot_options)
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

/// Encode a borrowed [`ParseSessionResult`] into a Binary AST
/// snapshot.
///
/// The session's [`crate::CstView`], diagnostics, and source identity
/// are read in place without copying the underlying tables.
pub fn parse_session_to_snapshot(
    session: &ParseSessionResult<'_>,
    options: SnapshotOptions,
) -> Result<SnapshotResult, SnapshotWriteError> {
    // Materialise diagnostics into owned values exactly once. The
    // owned `Vec` is moved into both the writer (as a borrow) and the
    // returned `SnapshotResult.diagnostics`, so workspace-reuse / LSP
    // callers do not pay for a second `.to_vec()`.
    let diagnostics: Vec<Diagnostic> = session.diagnostics.iter().collect();
    encode_single(
        session.cst.sources(),
        session.source,
        session.cst.tables(),
        diagnostics,
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
    // Phase 1's `parse_batch` registers each input through
    // `SourceStore::add`, which panics on `source.len() > u32::MAX`.
    // The public snapshot API returns `Result<_, SnapshotWriteError>`,
    // so pre-validate every input here and convert oversized inputs
    // to `SnapshotWriteError::SourceTooLarge` before parse runs.
    for input in inputs {
        if u32::try_from(input.source.len()).is_err() {
            return Err(SnapshotWriteError::SourceTooLarge);
        }
    }
    let batch = run_parse_batch(inputs, batch_options);
    parse_batch_result_to_snapshot(&batch, snapshot_options)
}

/// Encode an already-produced [`BatchParseResult`] into a shared
/// Binary AST snapshot.
pub fn parse_batch_result_to_snapshot(
    result: &BatchParseResult,
    options: SnapshotOptions,
) -> Result<BatchSnapshotResult, SnapshotWriteError> {
    // Pre-size the writer for the batch so the string table /
    // roots vectors don't grow during encoding.
    let mut writer = SnapshotWriter::with_root_hint(options, result.items.len());
    // Pre-intern every batch root's source metadata before any
    // `add_root` runs so the string table emits source metadata
    // strings ahead of diagnostic messages — the canonical writer
    // order required by `design/003` §"String Table".
    writer.pre_intern_root_sources(&result.sources, result.items.iter().map(|item| item.source))?;
    for item in &result.items {
        writer.add_root(
            &result.sources,
            item.source,
            &item.result.cst,
            &item.result.diagnostics,
        )?;
    }
    let bytes = writer.finish(&result.sources)?;
    let roots_count = checked_u32(result.items.len()).ok_or(SnapshotWriteError::TooManyRoots)?;
    let roots = (0..roots_count).map(RootId::new).collect();
    // `SnapshotResult.diagnostics` is returned for caller
    // convenience regardless of `SnapshotOptions.include_diagnostics`,
    // so callers can still inspect parser output without re-parsing.
    // Reserve once for the exact total and extend with per-diagnostic
    // clones instead of `flat_map(|item| item.result.diagnostics.clone())`,
    // which would allocate a temporary `Vec` per batch item.
    let diagnostic_total: usize = result
        .items
        .iter()
        .map(|item| item.result.diagnostics.len())
        .sum();
    let mut diagnostics = Vec::with_capacity(diagnostic_total);
    for item in &result.items {
        diagnostics.extend(item.result.diagnostics.iter().cloned());
    }
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
    diagnostics: Vec<Diagnostic>,
    options: SnapshotOptions,
) -> Result<SnapshotResult, SnapshotWriteError> {
    let mut writer = SnapshotWriter::new(options);
    // Match `parse_batch_result_to_snapshot`: register the root
    // source metadata up front so it lands in the string table
    // ahead of any diagnostic messages emitted during `add_root`.
    writer.pre_intern_root_sources(sources, [source])?;
    writer.add_root(sources, source, cst, &diagnostics)?;
    let bytes = writer.finish(sources)?;
    Ok(SnapshotResult {
        bytes,
        root: RootId::new(0),
        diagnostics,
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

/// Per-snapshot-local-source metadata `StringId`s, captured by
/// `intern_source` the first time a Phase 1 source is registered
/// so `finish` can emit `SourceRecord` wire bytes without a second
/// `StringTableBuilder::intern_optional` lookup per field.
#[derive(Debug, Clone, Copy)]
struct SourceMetaIds {
    path: StringId,
    locale: StringId,
    message_id: StringId,
}

struct SnapshotWriter {
    options: SnapshotOptions,
    string_table: StringTableBuilder,
    /// Phase 1 `SourceId` per snapshot-local `SourceId` (the
    /// `Vec` index is the snapshot-local id). Parallel to
    /// `source_meta`. v0.1 writer does NOT deduplicate
    /// `SourceRecord`s — see `design/003` §"Source Section".
    /// Each `add_root` call appends one entry, even when the
    /// Phase 1 source matches an earlier root.
    source_phase_one: Vec<PhaseOneSourceId>,
    /// Parallel to `source_phase_one`: metadata `StringId` triple
    /// captured by `allocate_root_source` so `finish` can copy
    /// `path` / `locale` / `message_id` straight into the
    /// `SourceRecord` without a second string-table lookup.
    source_meta: Vec<SourceMetaIds>,
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
}

impl SnapshotWriter {
    fn new(options: SnapshotOptions) -> Self {
        Self::with_root_hint(options, 1)
    }

    /// Build a writer with capacity hints derived from the expected
    /// root count. Each root contributes up to one source and three
    /// metadata strings (`path` / `locale` / `message_id`), so a
    /// batch caller can hand the writer a tight reservation up
    /// front.
    fn with_root_hint(options: SnapshotOptions, root_hint: usize) -> Self {
        // 3 metadata strings per source plus a per-root fixed
        // overhead for diagnostic catalog strings (~15 entries
        // across the catalog) — being generous on string_hint
        // costs only a small amount of capacity in the lookup
        // vectors and avoids growth during intern.
        let string_hint = root_hint.saturating_mul(3).saturating_add(16);
        let data_hint = root_hint.saturating_mul(64);
        Self {
            options,
            string_table: StringTableBuilder::with_capacity(string_hint, data_hint),
            source_phase_one: Vec::with_capacity(root_hint),
            source_meta: Vec::with_capacity(root_hint),
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
            roots: Vec::with_capacity(root_hint),
        }
    }

    /// Allocate a fresh snapshot-local `SourceId` for `root_source`
    /// and cache its metadata `StringId` triple. v0.1 writer does
    /// NOT deduplicate `SourceRecord`s by Phase 1 source id — see
    /// `design/003` §"Source Section". Every `add_root` call gets
    /// its own slot, even when the Phase 1 source matches an
    /// earlier root, so root identity in the snapshot is 1:1 with
    /// the input root order.
    fn allocate_root_source(
        &mut self,
        sources: &SourceStore,
        root_source: PhaseOneSourceId,
    ) -> Result<u32, SnapshotWriteError> {
        let file = sources
            .get(root_source)
            .ok_or(SnapshotWriteError::InvalidSourceId)?;
        let local =
            checked_u32(self.source_phase_one.len()).ok_or(SnapshotWriteError::TooManySources)?;
        let path = self.string_table.intern_optional(file.path.as_deref())?;
        let locale = self.string_table.intern_optional(file.locale.as_deref())?;
        let message_id = self
            .string_table
            .intern_optional(file.message_id.as_deref())?;
        self.source_phase_one.push(root_source);
        self.source_meta.push(SourceMetaIds {
            path,
            locale,
            message_id,
        });
        debug_assert_eq!(self.source_meta.len(), self.source_phase_one.len());
        Ok(local)
    }

    /// Pre-intern every root's source metadata strings into the
    /// `StringTable` ahead of any `add_root` call. The string
    /// table then emits source metadata strings strictly before
    /// diagnostic messages — the canonical order called out in
    /// `design/003` §"String Table" — even across batch items.
    ///
    /// Source slot allocation happens later inside `add_root` via
    /// `allocate_root_source`; this method only touches the string
    /// table, so repeated Phase 1 source ids are interned
    /// idempotently and never cost extra string table entries.
    fn pre_intern_root_sources<I>(
        &mut self,
        sources: &SourceStore,
        source_ids: I,
    ) -> Result<(), SnapshotWriteError>
    where
        I: IntoIterator<Item = PhaseOneSourceId>,
    {
        for id in source_ids {
            let file = sources.get(id).ok_or(SnapshotWriteError::InvalidSourceId)?;
            self.string_table.intern_optional(file.path.as_deref())?;
            self.string_table.intern_optional(file.locale.as_deref())?;
            self.string_table
                .intern_optional(file.message_id.as_deref())?;
        }
        Ok(())
    }

    fn add_root(
        &mut self,
        sources: &SourceStore,
        source: PhaseOneSourceId,
        cst: &CstTables,
        diagnostics: &[Diagnostic],
    ) -> Result<(), SnapshotWriteError> {
        // Reserve section byte buffers from the Phase 1 CST counts
        // so the per-record `write_*` calls below don't grow the
        // underlying `Vec`s mid-loop. Diagnostics / labels are
        // skipped when `include_diagnostics = false` so the writer
        // doesn't speculate on encode work it won't perform.
        self.nodes_bytes
            .reserve(cst.node_count() * NODE_RECORD_SIZE as usize);
        self.edges_bytes
            .reserve(cst.edge_count() * EDGE_RECORD_SIZE as usize);
        self.tokens_bytes
            .reserve(cst.token_count() * TOKEN_RECORD_SIZE as usize);
        if self.options.include_trivia {
            self.trivia_bytes
                .reserve(cst.trivia_count() * TRIVIA_RECORD_SIZE as usize);
        }
        if self.options.include_diagnostics {
            self.diagnostics_bytes
                .reserve(diagnostics.len() * DIAGNOSTIC_RECORD_SIZE as usize);
            let label_total: usize = diagnostics.iter().map(|d| d.labels.len()).sum();
            self.diagnostic_labels_bytes
                .reserve(label_total * DIAGNOSTIC_LABEL_RECORD_SIZE as usize);
        }

        // Allocate a fresh SourceRecord slot for THIS root (no
        // dedup by Phase 1 SourceId — see `design/003` §"Source
        // Section"). `emit_*` below all reference this slot
        // directly so every token / trivia / diagnostic in this
        // root points at this root's snapshot-local SourceId.
        let source_local = self.allocate_root_source(sources, source)?;

        // Trivia first so token records can reference snapshot-local
        // trivia ids without a second pass. With include_trivia=false
        // we still walk parser trivia to fill the remap with NONE_REF
        // so the per-token leading/trailing ranges encode as `0`.
        let trivia_remap = self.emit_trivia(cst, source_local)?;
        // Tokens next: token records reference trivia ranges.
        let token_remap = self.emit_tokens(cst, &trivia_remap, source_local)?;
        // Nodes / edges share a single post-order pass: every edge
        // refers to either a node id or token id, and node order
        // follows parser post-order so the parser root is the last
        // node.
        let node_remap = self.emit_nodes_and_edges(cst, &token_remap)?;

        let root_node = match cst.root_id() {
            Some(parser_root) => node_remap[parser_root.index()],
            None => return Err(SnapshotWriteError::MissingRoot),
        };

        // Diagnostic encoding is the writer's, not the caller's,
        // choice. `SnapshotResult.diagnostics` is always returned to
        // the caller from `encode_single` / batch encoders (so they
        // can still see what the parser produced), but when
        // `include_diagnostics = false` the writer must skip building
        // any diagnostic section bytes and must not advance the
        // diagnostic / label counters.
        let (diag_start, diag_count) = if self.options.include_diagnostics {
            let start = self.diagnostics_count;
            for diag in diagnostics {
                self.emit_diagnostic(diag, source_local)?;
            }
            let count = self
                .diagnostics_count
                .checked_sub(start)
                .expect("diagnostics_count only grows");
            (start, count)
        } else {
            (0, 0)
        };

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
        Ok(())
    }

    fn emit_trivia(
        &mut self,
        cst: &CstTables,
        root_source_local: u32,
    ) -> Result<Vec<u32>, SnapshotWriteError> {
        let trivia_count = cst.trivia_count();
        let mut remap = Vec::with_capacity(trivia_count);
        if !self.options.include_trivia || trivia_count == 0 {
            remap.resize(trivia_count, NONE_REF);
            return Ok(remap);
        }
        // Each trivia is recorded as belonging to the current
        // root's snapshot-local `SourceRecord` — v0.1 keeps the
        // writer to one SourceRecord per root and references each
        // trivia / token / diagnostic span against that slot,
        // matching the design/003 "no source dedup" contract.
        for trivia in &cst.trivia {
            let local = self.next_trivia_id()?;
            write_u16_le(&mut self.trivia_bytes, trivia.kind);
            write_u16_le(&mut self.trivia_bytes, 0); // flags
            write_u32_le(&mut self.trivia_bytes, trivia.span_start);
            write_u32_le(&mut self.trivia_bytes, trivia.span_end);
            write_u32_le(&mut self.trivia_bytes, root_source_local);
            remap.push(local);
        }
        Ok(remap)
    }

    fn emit_tokens(
        &mut self,
        cst: &CstTables,
        trivia_remap: &[u32],
        root_source_local: u32,
    ) -> Result<Vec<u32>, SnapshotWriteError> {
        let mut remap = Vec::with_capacity(cst.token_count());
        for token in &cst.tokens {
            let local = self.next_token_id()?;
            let source_local = root_source_local;

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

    fn emit_diagnostic(
        &mut self,
        diagnostic: &Diagnostic,
        root_source_local: u32,
    ) -> Result<(), SnapshotWriteError> {
        // Diagnostics within a root all reference the root's
        // snapshot-local `SourceRecord` (v0.1 has one SourceRecord
        // per root). Multi-source diagnostics within a single root
        // are not supported by the v0.1 writer.
        let source_local = root_source_local;
        let label_start = self.diagnostic_labels_count;
        for label in &diagnostic.labels {
            let label_source = root_source_local;
            let _ = label.source; // intentional: v0.1 collapses to root source
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
            string_table,
            source_phase_one,
            source_meta,
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
        } = self;

        if roots.is_empty() {
            return Err(SnapshotWriteError::MissingRoot);
        }

        debug_assert_eq!(source_meta.len(), source_phase_one.len());

        // ── Sources section + optional source text data ──────────────
        let mut sources_bytes = Vec::with_capacity(source_phase_one.len() * 32);
        let mut sources_count: u32 = 0;
        let include_source_text = options.include_source_text;
        // Pre-size the source text data buffer from the actual
        // per-source text lengths so `extend_from_slice` below does
        // not grow the underlying `Vec`. Sum with checked arithmetic
        // so callers see `SectionTooLarge` instead of a panic when
        // the total exceeds the `u32` byte-offset domain.
        let mut source_text_bytes: Vec<u8> = if include_source_text {
            let mut total: u32 = 0;
            for &phase_one in &source_phase_one {
                let file = sources
                    .get(phase_one)
                    .ok_or(SnapshotWriteError::InvalidSourceId)?;
                total = total
                    .checked_add(file.len())
                    .ok_or(SnapshotWriteError::SectionTooLarge)?;
            }
            Vec::with_capacity(total as usize)
        } else {
            Vec::new()
        };
        for (snapshot_local, (&phase_one, meta)) in
            source_phase_one.iter().zip(source_meta.iter()).enumerate()
        {
            let snapshot_local =
                checked_u32(snapshot_local).ok_or(SnapshotWriteError::TooManySources)?;
            let file = sources
                .get(phase_one)
                .ok_or(SnapshotWriteError::InvalidSourceId)?;
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
            write_u32_le(&mut sources_bytes, meta.path.raw());
            write_u32_le(&mut sources_bytes, meta.locale.raw());
            write_u32_le(&mut sources_bytes, meta.message_id.raw());
            write_u32_le(&mut sources_bytes, file.base_offset);
            // SourceTextRef { source_id, offset, len }
            write_u32_le(&mut sources_bytes, text_source);
            write_u32_le(&mut sources_bytes, text_offset);
            write_u32_le(&mut sources_bytes, text_len);
            sources_count = sources_count
                .checked_add(1)
                .ok_or(SnapshotWriteError::TooManySources)?;
        }

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
        // When `include_source_text = true`, every SourceRecord
        // writes a non-`NONE_REF` text_source above, even for empty
        // source text. The section must therefore be emitted (even
        // empty) so the decoder's `text_source != NONE_REF` branch
        // can resolve an in-range `offset + len` and `SourceView::text`
        // can round-trip `Some("")` back instead of returning `None`.
        if include_source_text {
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
