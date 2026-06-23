// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Borrowed and owned snapshot views with allocation-free accessors.
//!
//! Once [`crate::snapshot::decode_snapshot`] succeeds, validated
//! `SectionIndex` metadata is paired with the snapshot byte slice. The
//! returned view does not materialise a recursive object graph;
//! traversal slices the buffer on demand.

use std::sync::Arc;

use crate::diagnostic::{DiagnosticCode, DiagnosticSeverity};
use crate::snapshot::decoder::{diagnostic_code_from_u16_strict, syntax_kind_from_u16};
use crate::snapshot::format::{
    read_u16_le, read_u32_le, RootId, SectionKind, StringId, DIAGNOSTIC_LABEL_RECORD_SIZE,
    DIAGNOSTIC_RECORD_SIZE, EDGE_RECORD_SIZE, NODE_RECORD_SIZE, NONE_REF, ROOT_RECORD_SIZE,
    SOURCE_RECORD_SIZE, STRING_OFFSET_RECORD_SIZE, TOKEN_RECORD_SIZE, TRIVIA_RECORD_SIZE,
};
use crate::span::{NodeId, SourceId, Span, TokenId, TriviaId};
use crate::syntax_kind::SyntaxKind;

/// Error returned when a snapshot accessor cannot resolve source
/// bytes through [`SourceView::source_slice`].
///
/// `NotIncluded` means the snapshot was produced with
/// `SnapshotOptions.include_source_text = false`, so the writer
/// did not embed any source bytes. `SpanOutOfBounds` means the
/// span extends past the encoded text or lands inside a multibyte
/// UTF-8 scalar. The distinction matters because the former is
/// recoverable by the caller (use external source text) while the
/// latter signals a logic bug or stale span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceTextUnavailable {
    NotIncluded,
    SpanOutOfBounds,
}

impl core::fmt::Display for SourceTextUnavailable {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::NotIncluded => {
                "snapshot was encoded without source text (include_source_text = false)"
            }
            Self::SpanOutOfBounds => {
                "span extends past the encoded source text or splits a UTF-8 scalar"
            }
        })
    }
}

impl std::error::Error for SourceTextUnavailable {}

/// Section payload range. Validated by the decoder so accessors can
/// slice the underlying snapshot buffer without re-checking bounds.
#[derive(Debug, Clone, Copy)]
pub struct SectionSlice {
    pub offset: u32,
    pub byte_len: u32,
    pub count: u32,
}

/// Validated section metadata index. Built by
/// [`crate::snapshot::decode_snapshot`].
#[derive(Debug, Clone, Copy)]
pub struct SectionIndex {
    pub roots: SectionSlice,
    pub sources: SectionSlice,
    pub nodes: SectionSlice,
    pub edges: SectionSlice,
    pub tokens: SectionSlice,
    pub trivia: Option<SectionSlice>,
    pub diagnostics: Option<SectionSlice>,
    pub diagnostic_labels: Option<SectionSlice>,
    pub string_offsets: SectionSlice,
    pub string_data: SectionSlice,
    pub source_text_data: Option<SectionSlice>,
    pub extended_data: Option<SectionSlice>,
}

/// Read-only section metadata returned from
/// [`SnapshotView::section`] / [`SnapshotViewOwned::section`].
#[derive(Debug, Clone, Copy)]
pub struct SectionView {
    pub kind: SectionKind,
    pub offset: u32,
    pub byte_len: u32,
    pub count: u32,
    pub record_size: u16,
    pub alignment: u8,
}

/// Borrowed snapshot view. The byte buffer's lifetime is the caller's
/// responsibility.
#[derive(Debug, Clone, Copy)]
pub struct SnapshotView<'a> {
    bytes: &'a [u8],
    sections: SectionIndex,
}

impl<'a> SnapshotView<'a> {
    pub(crate) fn from_validated(bytes: &'a [u8], sections: SectionIndex) -> Self {
        Self { bytes, sections }
    }

    /// Raw snapshot bytes.
    pub fn as_bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// Validated section metadata.
    pub fn sections(&self) -> &SectionIndex {
        &self.sections
    }

    /// Section view by kind. Returns `None` for sections that are
    /// optional and absent.
    pub fn section(&self, kind: SectionKind) -> Option<SectionView> {
        let slice = match kind {
            SectionKind::Roots => Some(self.sections.roots),
            SectionKind::Sources => Some(self.sections.sources),
            SectionKind::Nodes => Some(self.sections.nodes),
            SectionKind::Edges => Some(self.sections.edges),
            SectionKind::Tokens => Some(self.sections.tokens),
            SectionKind::Trivia => self.sections.trivia,
            SectionKind::Diagnostics => self.sections.diagnostics,
            SectionKind::DiagnosticLabels => self.sections.diagnostic_labels,
            SectionKind::StringOffsets => Some(self.sections.string_offsets),
            SectionKind::StringData => Some(self.sections.string_data),
            SectionKind::SourceTextData => self.sections.source_text_data,
            SectionKind::ExtendedData => self.sections.extended_data,
        }?;
        Some(SectionView {
            kind,
            offset: slice.offset,
            byte_len: slice.byte_len,
            count: slice.count,
            record_size: kind.record_size(),
            alignment: crate::snapshot::format::SECTION_ALIGNMENT as u8,
        })
    }

    /// Number of root records.
    pub fn root_count(&self) -> u32 {
        self.sections.roots.count
    }

    /// Borrowed root view by id. Returns `None` if `id` is out of
    /// range.
    pub fn root(&self, id: RootId) -> Option<RootView<'a>> {
        if id.raw() >= self.sections.roots.count {
            return None;
        }
        Some(RootView {
            bytes: self.bytes,
            sections: self.sections,
            id,
        })
    }

    /// Number of node records.
    pub fn node_count(&self) -> u32 {
        self.sections.nodes.count
    }

    /// Number of token records.
    pub fn token_count(&self) -> u32 {
        self.sections.tokens.count
    }

    /// Number of trivia records (zero when the trivia section is
    /// absent).
    pub fn trivia_count(&self) -> u32 {
        self.sections.trivia.map_or(0, |s| s.count)
    }

    /// Borrowed node view by id.
    pub fn node(&self, id: NodeId) -> Option<NodeView<'a>> {
        if id.is_none() || id.raw() >= self.sections.nodes.count {
            return None;
        }
        Some(self.node_unchecked(id))
    }

    /// Borrowed node view by id after the caller has already ensured
    /// that `id < nodes.count`.
    pub(crate) fn node_unchecked(&self, id: NodeId) -> NodeView<'a> {
        debug_assert!(!id.is_none());
        debug_assert!(id.raw() < self.sections.nodes.count);
        NodeView {
            bytes: self.bytes,
            sections: self.sections,
            id,
        }
    }

    /// Borrowed token view by id.
    pub fn token(&self, id: TokenId) -> Option<TokenView<'a>> {
        if id.is_none() || id.raw() >= self.sections.tokens.count {
            return None;
        }
        Some(TokenView {
            bytes: self.bytes,
            sections: self.sections,
            id,
        })
    }

    /// Borrowed trivia view by id.
    pub fn trivia(&self, id: TriviaId) -> Option<TriviaView<'a>> {
        let trivia = self.sections.trivia?;
        if id.is_none() || id.raw() >= trivia.count {
            return None;
        }
        Some(TriviaView {
            bytes: self.bytes,
            sections: self.sections,
            id,
        })
    }

    /// Number of source records.
    pub fn source_count(&self) -> u32 {
        self.sections.sources.count
    }

    /// Borrowed source view by id.
    pub fn source(&self, id: SourceId) -> Option<SourceView<'a>> {
        if id.is_none() || id.raw() >= self.sections.sources.count {
            return None;
        }
        Some(SourceView {
            bytes: self.bytes,
            sections: self.sections,
            id,
        })
    }

    /// String accessor. Returns `None` for the canonical sentinel
    /// `0xFFFF_FFFF` and for ids beyond the string offsets count.
    pub fn string(&self, id: StringId) -> Option<&'a str> {
        if id.is_none() {
            return None;
        }
        string_slice(self.bytes, &self.sections, id.raw())
    }

    /// Number of diagnostic records (zero when the section is absent).
    pub fn diagnostic_count(&self) -> u32 {
        self.sections.diagnostics.map_or(0, |s| s.count)
    }

    /// Borrowed diagnostic view by ordinal.
    pub fn diagnostic(&self, index: u32) -> Option<DiagnosticRecordView<'a>> {
        let diags = self.sections.diagnostics?;
        if index >= diags.count {
            return None;
        }
        Some(DiagnosticRecordView {
            bytes: self.bytes,
            sections: self.sections,
            index,
        })
    }
}

/// Owned snapshot view. Shares ownership of the buffer via [`Arc<[u8]>`]
/// so accessors can outlive the original byte producer.
#[derive(Debug, Clone)]
pub struct SnapshotViewOwned {
    bytes: Arc<[u8]>,
    sections: SectionIndex,
}

impl SnapshotViewOwned {
    pub(crate) fn from_validated(bytes: Arc<[u8]>, sections: SectionIndex) -> Self {
        Self { bytes, sections }
    }

    /// Raw snapshot bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Borrowed view over the same buffer.
    pub fn view(&self) -> SnapshotView<'_> {
        SnapshotView::from_validated(&self.bytes, self.sections)
    }

    /// Shared ownership of the underlying buffer.
    pub fn bytes_arc(&self) -> Arc<[u8]> {
        Arc::clone(&self.bytes)
    }

    /// See [`SnapshotView::section`].
    pub fn section(&self, kind: SectionKind) -> Option<SectionView> {
        self.view().section(kind)
    }

    /// See [`SnapshotView::sections`].
    pub fn sections(&self) -> SectionIndex {
        self.sections
    }
}

// ── Root / Source / Node / Token / Trivia views ────────────────────

#[derive(Debug, Clone, Copy)]
pub struct RootView<'a> {
    bytes: &'a [u8],
    sections: SectionIndex,
    id: RootId,
}

impl<'a> RootView<'a> {
    pub fn id(&self) -> RootId {
        self.id
    }

    pub fn root_node(&self) -> NodeId {
        let off = self.sections.roots.offset as usize + self.id.index() * ROOT_RECORD_SIZE as usize;
        NodeId::new(read_u32_le(self.bytes, off))
    }

    pub fn source_id(&self) -> SourceId {
        let off = self.sections.roots.offset as usize + self.id.index() * ROOT_RECORD_SIZE as usize;
        SourceId::new(read_u32_le(self.bytes, off + 4))
    }

    pub fn diagnostic_range(&self) -> (u32, u32) {
        let off = self.sections.roots.offset as usize + self.id.index() * ROOT_RECORD_SIZE as usize;
        (
            read_u32_le(self.bytes, off + 8),
            read_u32_le(self.bytes, off + 12),
        )
    }

    pub fn node(&self) -> NodeView<'a> {
        NodeView {
            bytes: self.bytes,
            sections: self.sections,
            id: self.root_node(),
        }
    }

    pub fn diagnostics(&self) -> DiagnosticIter<'a> {
        let (start, count) = self.diagnostic_range();
        DiagnosticIter {
            bytes: self.bytes,
            sections: self.sections,
            cursor: start,
            remaining: count,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SourceView<'a> {
    bytes: &'a [u8],
    sections: SectionIndex,
    id: SourceId,
}

impl<'a> SourceView<'a> {
    pub fn id(&self) -> SourceId {
        self.id
    }

    fn base(&self) -> usize {
        self.sections.sources.offset as usize + self.id.index() * SOURCE_RECORD_SIZE as usize
    }

    pub fn path(&self) -> Option<&'a str> {
        let raw = read_u32_le(self.bytes, self.base() + 4);
        string_slice(self.bytes, &self.sections, raw)
    }

    pub fn locale(&self) -> Option<&'a str> {
        let raw = read_u32_le(self.bytes, self.base() + 8);
        string_slice(self.bytes, &self.sections, raw)
    }

    pub fn message_id(&self) -> Option<&'a str> {
        let raw = read_u32_le(self.bytes, self.base() + 12);
        string_slice(self.bytes, &self.sections, raw)
    }

    pub fn base_offset(&self) -> u32 {
        read_u32_le(self.bytes, self.base() + 16)
    }

    /// Snapshot-embedded source text bytes, if `include_source_text`
    /// was used during encoding.
    pub fn text(&self) -> Option<&'a str> {
        let base = self.base();
        let text_source = read_u32_le(self.bytes, base + 20);
        if text_source == NONE_REF {
            return None;
        }
        let offset = read_u32_le(self.bytes, base + 24);
        let len = read_u32_le(self.bytes, base + 28);
        let ed = self.sections.source_text_data?;
        let start = ed.offset as usize + offset as usize;
        let end = start + len as usize;
        core::str::from_utf8(&self.bytes[start..end]).ok()
    }

    /// Slice the snapshot-embedded source text by `span`, returning
    /// an explicit `SourceTextUnavailable` rather than `None` so
    /// callers can distinguish "source text not encoded" from
    /// "span out of bounds" — see `design/003` §"Source Section".
    ///
    /// `span` is interpreted as a zero-based byte offset into the
    /// snapshot-embedded text, matching the spans returned by
    /// [`NodeView::span`] / [`TokenView::span`] /
    /// [`TriviaView::span`] / [`DiagnosticRecordView::span`] /
    /// [`DiagnosticLabelView::span`]. `SourceView::base_offset` is
    /// metadata (the file offset where the embedded substring starts
    /// in the original source) and is intentionally NOT subtracted
    /// here. Absolute file positions are `base_offset + span_start`
    /// / `base_offset + span_end`; callers that already hold an
    /// absolute span must subtract `base_offset` themselves before
    /// calling `source_slice`.
    ///
    /// Returns `Err(SourceTextUnavailable::NotIncluded)` when the
    /// snapshot was produced with `include_source_text = false`
    /// (text source ref is the canonical `NONE_REF` sentinel).
    /// Returns `Err(SourceTextUnavailable::SpanOutOfBounds)` when
    /// `span` extends past the encoded text, when `span.start >
    /// span.end`, or when either endpoint lands inside a multibyte
    /// UTF-8 scalar.
    pub fn source_slice(&self, span: Span) -> Result<&'a str, SourceTextUnavailable> {
        let text = self.text().ok_or(SourceTextUnavailable::NotIncluded)?;
        slice_source_text(text, span)
    }

    /// Slice source text by `span`, falling back to caller-provided
    /// external text when the snapshot was encoded with
    /// `include_source_text = false`.
    ///
    /// The external text is explicit rather than looked up through a
    /// `SourceStore`: snapshot `SourceId`s are local to the emitted
    /// snapshot and do not have to match Phase 1 `SourceStore` ids.
    /// When snapshot-embedded text exists, it is preferred so this
    /// method has the same behaviour as [`Self::source_slice`].
    pub fn source_slice_with_external_text(
        &self,
        span: Span,
        external_source_text: &'a str,
    ) -> Result<&'a str, SourceTextUnavailable> {
        let text = self.text().unwrap_or(external_source_text);
        slice_source_text(text, span)
    }
}

fn slice_source_text(text: &str, span: Span) -> Result<&str, SourceTextUnavailable> {
    if span.start > span.end {
        return Err(SourceTextUnavailable::SpanOutOfBounds);
    }
    let len_u32 = text.len() as u32;
    if span.end > len_u32 {
        return Err(SourceTextUnavailable::SpanOutOfBounds);
    }
    let start = span.start as usize;
    let end = span.end as usize;
    text.get(start..end)
        .ok_or(SourceTextUnavailable::SpanOutOfBounds)
}

#[derive(Debug, Clone, Copy)]
pub struct NodeView<'a> {
    bytes: &'a [u8],
    sections: SectionIndex,
    id: NodeId,
}

impl<'a> NodeView<'a> {
    pub fn id(&self) -> NodeId {
        self.id
    }

    fn base(&self) -> usize {
        self.sections.nodes.offset as usize + self.id.index() * NODE_RECORD_SIZE as usize
    }

    pub fn kind(&self) -> SyntaxKind {
        syntax_kind_from_u16(read_u16_le(self.bytes, self.base())).unwrap_or(SyntaxKind::Unknown)
    }

    pub fn span(&self) -> Span {
        Span::new(
            read_u32_le(self.bytes, self.base() + 4),
            read_u32_le(self.bytes, self.base() + 8),
        )
    }

    pub fn child_count(&self) -> u32 {
        read_u32_le(self.bytes, self.base() + 16)
    }

    pub fn children(&self) -> ChildIter<'a> {
        let start = read_u32_le(self.bytes, self.base() + 12);
        let count = self.child_count();
        ChildIter {
            bytes: self.bytes,
            sections: self.sections,
            edge_start: start,
            remaining: count,
        }
    }

    pub fn child_at(&self, index: u32) -> Option<ChildView<'a>> {
        if index >= self.child_count() {
            return None;
        }
        let start = read_u32_le(self.bytes, self.base() + 12);
        Some(read_child(self.bytes, &self.sections, start + index))
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ChildView<'a> {
    Node(NodeView<'a>),
    Token(TokenView<'a>),
}

#[derive(Debug, Clone)]
pub struct ChildIter<'a> {
    bytes: &'a [u8],
    sections: SectionIndex,
    edge_start: u32,
    remaining: u32,
}

impl<'a> Iterator for ChildIter<'a> {
    type Item = ChildView<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let child = read_child(self.bytes, &self.sections, self.edge_start);
        self.edge_start += 1;
        self.remaining -= 1;
        Some(child)
    }
}

fn read_child<'a>(bytes: &'a [u8], sections: &SectionIndex, edge_index: u32) -> ChildView<'a> {
    let off = sections.edges.offset as usize + edge_index as usize * EDGE_RECORD_SIZE as usize;
    let kind = read_u16_le(bytes, off);
    let ref_id = read_u32_le(bytes, off + 4);
    if kind == crate::snapshot::format::EDGE_KIND_TOKEN {
        ChildView::Token(TokenView {
            bytes,
            sections: *sections,
            id: TokenId::new(ref_id),
        })
    } else {
        ChildView::Node(NodeView {
            bytes,
            sections: *sections,
            id: NodeId::new(ref_id),
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TokenView<'a> {
    bytes: &'a [u8],
    sections: SectionIndex,
    id: TokenId,
}

impl<'a> TokenView<'a> {
    pub fn id(&self) -> TokenId {
        self.id
    }

    fn base(&self) -> usize {
        self.sections.tokens.offset as usize + self.id.index() * TOKEN_RECORD_SIZE as usize
    }

    pub fn kind(&self) -> SyntaxKind {
        syntax_kind_from_u16(read_u16_le(self.bytes, self.base())).unwrap_or(SyntaxKind::Unknown)
    }

    pub fn span(&self) -> Span {
        Span::new(
            read_u32_le(self.bytes, self.base() + 4),
            read_u32_le(self.bytes, self.base() + 8),
        )
    }

    pub fn source_id(&self) -> SourceId {
        SourceId::new(read_u32_le(self.bytes, self.base() + 12))
    }

    pub fn leading_trivia(&self) -> TriviaIter<'a> {
        let start = read_u32_le(self.bytes, self.base() + 16);
        let count = read_u32_le(self.bytes, self.base() + 20);
        TriviaIter {
            bytes: self.bytes,
            sections: self.sections,
            cursor: start,
            remaining: count,
        }
    }

    pub fn trailing_trivia(&self) -> TriviaIter<'a> {
        let start = read_u32_le(self.bytes, self.base() + 24);
        let count = read_u32_le(self.bytes, self.base() + 28);
        TriviaIter {
            bytes: self.bytes,
            sections: self.sections,
            cursor: start,
            remaining: count,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TriviaView<'a> {
    bytes: &'a [u8],
    sections: SectionIndex,
    id: TriviaId,
}

impl TriviaView<'_> {
    pub fn id(&self) -> TriviaId {
        self.id
    }

    fn base(&self) -> usize {
        let trivia = self.sections.trivia.expect("trivia view requires section");
        trivia.offset as usize + self.id.index() * TRIVIA_RECORD_SIZE as usize
    }

    pub fn kind(&self) -> SyntaxKind {
        syntax_kind_from_u16(read_u16_le(self.bytes, self.base())).unwrap_or(SyntaxKind::Unknown)
    }

    pub fn span(&self) -> Span {
        Span::new(
            read_u32_le(self.bytes, self.base() + 4),
            read_u32_le(self.bytes, self.base() + 8),
        )
    }

    pub fn source_id(&self) -> SourceId {
        SourceId::new(read_u32_le(self.bytes, self.base() + 12))
    }
}

#[derive(Debug, Clone)]
pub struct TriviaIter<'a> {
    bytes: &'a [u8],
    sections: SectionIndex,
    cursor: u32,
    remaining: u32,
}

impl<'a> Iterator for TriviaIter<'a> {
    type Item = TriviaView<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let item = TriviaView {
            bytes: self.bytes,
            sections: self.sections,
            id: TriviaId::new(self.cursor),
        };
        self.cursor += 1;
        self.remaining -= 1;
        Some(item)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DiagnosticRecordView<'a> {
    bytes: &'a [u8],
    sections: SectionIndex,
    index: u32,
}

#[derive(Debug, Clone)]
pub struct DiagnosticIter<'a> {
    bytes: &'a [u8],
    sections: SectionIndex,
    cursor: u32,
    remaining: u32,
}

impl<'a> Iterator for DiagnosticIter<'a> {
    type Item = DiagnosticRecordView<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let item = DiagnosticRecordView {
            bytes: self.bytes,
            sections: self.sections,
            index: self.cursor,
        };
        self.cursor += 1;
        self.remaining -= 1;
        Some(item)
    }
}

impl<'a> DiagnosticRecordView<'a> {
    fn base(&self) -> usize {
        let diags = self
            .sections
            .diagnostics
            .expect("diagnostic view requires section");
        diags.offset as usize + self.index as usize * DIAGNOSTIC_RECORD_SIZE as usize
    }

    pub fn source_id(&self) -> SourceId {
        SourceId::new(read_u32_le(self.bytes, self.base()))
    }

    pub fn span(&self) -> Span {
        Span::new(
            read_u32_le(self.bytes, self.base() + 4),
            read_u32_le(self.bytes, self.base() + 8),
        )
    }

    pub fn severity(&self) -> DiagnosticSeverity {
        match self.bytes[self.base() + 12] {
            0 => DiagnosticSeverity::Error,
            1 => DiagnosticSeverity::Warning,
            2 => DiagnosticSeverity::Information,
            _ => DiagnosticSeverity::Hint,
        }
    }

    pub fn code(&self) -> DiagnosticCode {
        let raw = read_u16_le(self.bytes, self.base() + 14);
        diagnostic_code_from_u16_strict(raw).unwrap_or(DiagnosticCode::Unspecified)
    }

    pub fn message(&self) -> Option<&'a str> {
        let raw = read_u32_le(self.bytes, self.base() + 16);
        string_slice(self.bytes, &self.sections, raw)
    }

    pub fn label_range(&self) -> (u32, u32) {
        (
            read_u32_le(self.bytes, self.base() + 20),
            read_u32_le(self.bytes, self.base() + 24),
        )
    }

    pub fn labels(&self) -> DiagnosticLabelIter<'a> {
        let (start, count) = self.label_range();
        DiagnosticLabelIter {
            bytes: self.bytes,
            sections: self.sections,
            cursor: start,
            remaining: count,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DiagnosticLabelView<'a> {
    bytes: &'a [u8],
    sections: SectionIndex,
    index: u32,
}

impl<'a> DiagnosticLabelView<'a> {
    fn base(&self) -> usize {
        let labels = self
            .sections
            .diagnostic_labels
            .expect("label view requires section");
        labels.offset as usize + self.index as usize * DIAGNOSTIC_LABEL_RECORD_SIZE as usize
    }

    pub fn source_id(&self) -> SourceId {
        SourceId::new(read_u32_le(self.bytes, self.base()))
    }

    pub fn span(&self) -> Span {
        Span::new(
            read_u32_le(self.bytes, self.base() + 4),
            read_u32_le(self.bytes, self.base() + 8),
        )
    }

    pub fn message(&self) -> Option<&'a str> {
        let raw = read_u32_le(self.bytes, self.base() + 12);
        string_slice(self.bytes, &self.sections, raw)
    }
}

#[derive(Debug, Clone)]
pub struct DiagnosticLabelIter<'a> {
    bytes: &'a [u8],
    sections: SectionIndex,
    cursor: u32,
    remaining: u32,
}

impl<'a> Iterator for DiagnosticLabelIter<'a> {
    type Item = DiagnosticLabelView<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let item = DiagnosticLabelView {
            bytes: self.bytes,
            sections: self.sections,
            index: self.cursor,
        };
        self.cursor += 1;
        self.remaining -= 1;
        Some(item)
    }
}

fn string_slice<'a>(bytes: &'a [u8], sections: &SectionIndex, id_raw: u32) -> Option<&'a str> {
    if id_raw == NONE_REF || id_raw >= sections.string_offsets.count {
        return None;
    }
    let rec_off = sections.string_offsets.offset as usize
        + id_raw as usize * STRING_OFFSET_RECORD_SIZE as usize;
    let off = read_u32_le(bytes, rec_off);
    let len = read_u32_le(bytes, rec_off + 4);
    let start = sections.string_data.offset as usize + off as usize;
    let end = start + len as usize;
    core::str::from_utf8(&bytes[start..end]).ok()
}
