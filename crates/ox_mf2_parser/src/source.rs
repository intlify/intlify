// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Source ownership: `SourceStore`, `SourceFile`, `SourceFileInput`.
//!
//! `SourceStore` owns source text, optional metadata, and per-file line-start
//! indexes. Spans use UTF-8 byte offsets. Line / column conversion goes
//! through [`SourceStore::location`].
//!
//! Line-start indexes are computed lazily — building the index walks the
//! entire source text, but a parse that emits no diagnostics never asks for
//! a line/column resolution and therefore never pays that cost. The
//! `OnceLock` keeps initialisation single-shot and `Send + Sync`-safe.

use std::sync::OnceLock;

use crate::error::{OxMf2ErrorCode, SourceTextErrorCode};
use crate::span::{usize_to_id_u32, SourceId, Span};

/// Public input used to register a source file with [`SourceStore`].
#[derive(Debug, Default, Clone, Copy)]
pub struct SourceFileInput<'a> {
    /// Source text. Stored as `&str`, so internal text is UTF-8.
    pub source: &'a str,
    /// Optional filesystem path, used for diagnostics.
    pub path: Option<&'a str>,
    /// Optional BCP-47 locale tag, used for project-aware tooling.
    pub locale: Option<&'a str>,
    /// Optional logical message id (e.g. translation key).
    pub message_id: Option<&'a str>,
    /// Optional base offset, used when the source is a substring of a larger
    /// file (e.g. a single entry inside a locale resource).
    pub base_offset: Option<u32>,
}

/// Owned source file registered in [`SourceStore`].
///
/// The line-start index is computed lazily on the first call to
/// [`Self::line_starts`] (and therefore on the first
/// [`SourceStore::location`] resolution). Parses that succeed without
/// diagnostics never trigger the computation.
#[derive(Debug, Clone, Default)]
pub struct SourceFile {
    pub id: SourceId,
    pub path: Option<String>,
    pub locale: Option<String>,
    pub message_id: Option<String>,
    pub base_offset: u32,
    pub text: String,
    /// Lazily-computed byte offsets of each line start. Use
    /// [`Self::line_starts`] to read — never touch this field directly so
    /// the lazy contract stays enforced.
    pub(crate) line_starts: OnceLock<Vec<u32>>,
}

impl SourceFile {
    /// UTF-8 byte length of the source text.
    #[inline]
    pub fn len(&self) -> u32 {
        self.text.len() as u32
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Line-start byte offsets, computed on demand and cached for the life
    /// of the source file. The first call walks the text; subsequent calls
    /// return the cached slice.
    pub fn line_starts(&self) -> &[u32] {
        self.line_starts
            .get_or_init(|| compute_line_starts(&self.text))
            .as_slice()
    }

    /// Line index (0-based) for a byte `offset` clamped to the source length.
    fn line_index_for_offset(&self, offset: u32) -> u32 {
        let len = self.len();
        let offset = offset.min(len);
        let line_starts = self.line_starts();
        // `line_starts` is sorted; partition_point gives the next line.
        let index = line_starts.partition_point(|&start| start <= offset);
        index.saturating_sub(1) as u32
    }
}

/// Resolved 1-based line/column pair derived from [`SourceStore`].
///
/// Columns are 1-based UTF-8 byte offsets from the line start. UTF-16 / grapheme
/// boundaries are handled by editors and bindings, not by the Rust core.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SourceLocation {
    pub line: u32,
    pub column: u32,
}

/// Errors returned by [`SourceStore::try_add`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceStoreError {
    /// Source length exceeds `u32::MAX` and cannot be represented as a span.
    SourceTooLarge,
}

impl core::fmt::Display for SourceStoreError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::SourceTooLarge => f.write_str("source length exceeds u32::MAX byte offsets"),
        }
    }
}

impl std::error::Error for SourceStoreError {}

impl SourceStoreError {
    #[inline]
    pub const fn code(self) -> SourceTextErrorCode {
        match self {
            Self::SourceTooLarge => SourceTextErrorCode::SourceTextTooLarge,
        }
    }

    #[inline]
    pub const fn as_ox_mf2_error_code(self) -> OxMf2ErrorCode {
        self.code().as_ox_mf2_error_code()
    }
}

/// Source ownership for single parse, batch parse, diagnostics, and Phase 2
/// snapshot roots.
#[derive(Debug, Default, Clone)]
pub struct SourceStore {
    files: Vec<SourceFile>,
}

impl SourceStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            files: Vec::with_capacity(cap),
        }
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn files(&self) -> &[SourceFile] {
        &self.files
    }

    /// Register a source file. Panics if the source is larger than
    /// `u32::MAX` bytes — use [`SourceStore::try_add`] for the fallible variant.
    pub fn add(&mut self, input: SourceFileInput<'_>) -> SourceId {
        self.try_add(input).expect("source length fits in u32")
    }

    pub fn try_add(&mut self, input: SourceFileInput<'_>) -> Result<SourceId, SourceStoreError> {
        if input.source.len() > u32::MAX as usize {
            return Err(SourceStoreError::SourceTooLarge);
        }
        let id = SourceId::new(usize_to_id_u32(self.files.len(), "source id"));
        self.files.push(SourceFile {
            id,
            path: input.path.map(str::to_owned),
            locale: input.locale.map(str::to_owned),
            message_id: input.message_id.map(str::to_owned),
            base_offset: input.base_offset.unwrap_or(0),
            text: input.source.to_owned(),
            // Line index stays unevaluated until `SourceFile::line_starts`
            // is first called — the success path of a parse never touches
            // it, so the per-source registration cost stays O(source.len)
            // for the text copy only.
            line_starts: OnceLock::new(),
        });
        Ok(id)
    }

    pub fn get(&self, id: SourceId) -> Option<&SourceFile> {
        if id.is_none() {
            return None;
        }
        self.files.get(id.index())
    }

    /// Resolve a byte span in the most recently relevant file. Callers that
    /// know which source the span belongs to should use [`Self::slice_in`].
    pub fn slice(&self, span: Span) -> &str {
        let Some(file) = self.files.first() else {
            return "";
        };
        Self::slice_str(&file.text, span)
    }

    pub fn slice_in(&self, source: SourceId, span: Span) -> &str {
        let Some(file) = self.get(source) else {
            return "";
        };
        Self::slice_str(&file.text, span)
    }

    fn slice_str(text: &str, span: Span) -> &str {
        let len = text.len() as u32;
        let start = span.start.min(len) as usize;
        let end = span.end.min(len) as usize;
        if end < start {
            return "";
        }
        // Span must always land on UTF-8 boundaries because the scanner steps
        // by char_indices; defensive guard for fuzzing.
        let Some(slice) = text.get(start..end) else {
            return "";
        };
        slice
    }

    /// Resolve a 1-based line/column for the start of `span` inside `source`.
    /// Triggers lazy line-index initialisation on the first call per file.
    pub fn location(&self, source: SourceId, span: Span) -> SourceLocation {
        let Some(file) = self.get(source) else {
            return SourceLocation::default();
        };
        let line0 = file.line_index_for_offset(span.start);
        let line_start = file.line_starts().get(line0 as usize).copied().unwrap_or(0);
        let column0 = span.start.saturating_sub(line_start);
        SourceLocation {
            line: line0 + 1,
            column: column0 + 1,
        }
    }
}

/// Compute byte offsets of every line start. The first entry is always `0`.
fn compute_line_starts(text: &str) -> Vec<u32> {
    let mut starts = Vec::with_capacity(text.len() / 32 + 1);
    starts.push(0);
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\n' {
            starts.push((i + 1) as u32);
            i += 1;
        } else if b == b'\r' {
            // CR or CRLF — count exactly one line break.
            let next_index = i + 1;
            let skip = if bytes.get(next_index) == Some(&b'\n') {
                2
            } else {
                1
            };
            starts.push((i + skip) as u32);
            i += skip;
        } else {
            i += 1;
        }
    }
    starts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_id_zero_is_valid() {
        let mut store = SourceStore::new();
        let id = store.add(SourceFileInput {
            source: "hello",
            ..Default::default()
        });
        assert_eq!(id, SourceId::new(0));
        assert_eq!(store.get(id).unwrap().text, "hello");
    }

    #[test]
    fn span_keeps_utf8_byte_offsets() {
        let text = "あいう"; // 3 chars, 9 bytes
        let mut store = SourceStore::new();
        let id = store.add(SourceFileInput {
            source: text,
            ..Default::default()
        });
        let span = Span::new(3, 6);
        assert_eq!(store.slice_in(id, span), "い");
    }

    #[test]
    fn line_column_conversion_handles_lf_cr_crlf() {
        let text = "ab\ncd\r\nef\rgh";
        let mut store = SourceStore::new();
        let id = store.add(SourceFileInput {
            source: text,
            ..Default::default()
        });

        // line 1 "ab" (offsets 0..2)
        assert_eq!(
            store.location(id, Span::at(0)),
            SourceLocation { line: 1, column: 1 }
        );
        assert_eq!(
            store.location(id, Span::at(1)),
            SourceLocation { line: 1, column: 2 }
        );
        // line 2 "cd" starts at offset 3
        assert_eq!(
            store.location(id, Span::at(3)),
            SourceLocation { line: 2, column: 1 }
        );
        // line 3 "ef" starts after CRLF at offset 7
        assert_eq!(
            store.location(id, Span::at(7)),
            SourceLocation { line: 3, column: 1 }
        );
        // line 4 "gh" starts after CR at offset 10
        assert_eq!(
            store.location(id, Span::at(10)),
            SourceLocation { line: 4, column: 1 }
        );
    }

    #[test]
    fn out_of_range_source_id_returns_none() {
        let store = SourceStore::new();
        assert!(store.get(SourceId::new(5)).is_none());
        assert!(store.get(SourceId::NONE).is_none());
    }

    #[test]
    fn line_starts_are_lazy_until_location_is_called() {
        let mut store = SourceStore::new();
        let id = store.add(SourceFileInput {
            source: "a\nb\nc\nd",
            ..Default::default()
        });
        // Fresh registration must not have populated the line index yet.
        let file = store.get(id).unwrap();
        assert!(
            file.line_starts.get().is_none(),
            "line_starts should stay unevaluated until first read"
        );
        // First location() resolution materialises and caches it.
        let _ = store.location(id, Span::at(3));
        let file = store.get(id).unwrap();
        assert!(
            file.line_starts.get().is_some(),
            "line_starts should be cached after first location()"
        );
        // Subsequent reads return the same slice (no reset).
        let starts = file.line_starts();
        assert_eq!(starts, &[0, 2, 4, 6]);
    }

    #[test]
    fn slice_handles_clamping() {
        let mut store = SourceStore::new();
        let id = store.add(SourceFileInput {
            source: "abc",
            ..Default::default()
        });
        assert_eq!(store.slice_in(id, Span::new(0, 3)), "abc");
        assert_eq!(store.slice_in(id, Span::new(0, 99)), "abc");
        assert_eq!(store.slice_in(id, Span::new(99, 100)), "");
    }
}
