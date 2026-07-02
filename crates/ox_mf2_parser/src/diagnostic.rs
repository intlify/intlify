// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Compact parser diagnostic model.
//!
//! The parser commits [`DiagnosticRecord`] entries into the workspace; labels
//! live in a side table. On request we materialise public [`Diagnostic`]
//! values that resolve the message through a static catalog and the
//! line/column through [`SourceStore`]. The hot path stays allocation-free.

use crate::source::{SourceLocation, SourceStore};
use crate::span::{SourceId, Span};

/// Severity of a diagnostic. Compact `u8` representation; numeric ordering
/// is used by tooling layers but the parser itself does not rely on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum DiagnosticSeverity {
    Error = 0,
    Warning = 1,
    Information = 2,
    Hint = 3,
}

/// Diagnostic code. The catalog is part of the parser's snapshot
/// compatibility surface — do not reuse a code for a different message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
#[non_exhaustive]
pub enum DiagnosticCode {
    Unspecified = 0,
    /// Required syntax was missing (e.g. `}` after `{` placeholder).
    UnexpectedEndOfInput = 1,
    /// A `{` was opened but its matching `}` was never found.
    UnclosedExpression = 2,
    /// A `|` was opened but its closing `|` was never found.
    UnclosedQuotedLiteral = 3,
    /// A `{{` was opened but its closing `}}` was never found.
    UnclosedQuotedPattern = 4,
    /// Input began with `.` but did not match `.input`, `.local`, or `.match`.
    InvalidDeclarationStart = 5,
    /// Matcher body found but missing required selector or variant.
    InvalidMatcherSyntax = 6,
    /// Variant key sequence was malformed (e.g. missing key before `{{`).
    InvalidVariantBoundary = 7,
    /// Markup open / close / standalone syntax was malformed.
    InvalidMarkupBoundary = 8,
    /// Declarations were present but no complex body followed.
    MissingComplexBody = 9,
    /// Source token did not match any production.
    UnexpectedToken = 10,
    /// Source length or span exceeded `u32::MAX`.
    SpanOverflow = 11,
    /// `\` was followed by a character that is not `\\`, `{`, `|`, or `}`.
    InvalidEscape = 12,
    /// Mode detection failed; recovered as a simple message.
    AmbiguousMessageMode = 13,
    /// A required `s` separator (at least one `ws`) was missing between
    /// adjacent productions (e.g. `.local$x`, `{$x:func}`, `{#tag@attr}`).
    MissingRequiredWhitespace = 14,
    /// A `namespace ":"` was given but the trailing `name` was absent
    /// (e.g. `{:foo:}`, `{#ns:}`).
    MissingIdentifierName = 15,
    /// `.input` declaration value was not a variable expression
    /// (e.g. `.input {|x|}`, `.input {:f}`, `.input {#m}`).
    InvalidInputDeclaration = 16,
}

impl DiagnosticCode {
    /// Severity associated with this code. Single source of truth used by the
    /// catalog so the parser does not embed severity at every emit site.
    pub const fn severity(self) -> DiagnosticSeverity {
        match self {
            Self::AmbiguousMessageMode => DiagnosticSeverity::Warning,
            _ => DiagnosticSeverity::Error,
        }
    }

    /// Human-readable static message. No allocation, no formatting.
    pub const fn static_message(self) -> &'static str {
        match self {
            Self::Unspecified => "unspecified parser diagnostic",
            Self::UnexpectedEndOfInput => "unexpected end of input",
            Self::UnclosedExpression => "unclosed placeholder expression",
            Self::UnclosedQuotedLiteral => "unclosed quoted literal",
            Self::UnclosedQuotedPattern => "unclosed quoted pattern",
            Self::InvalidDeclarationStart => {
                "invalid declaration keyword; expected '.input', '.local', or '.match'"
            }
            Self::InvalidMatcherSyntax => "invalid matcher syntax",
            Self::InvalidVariantBoundary => "invalid variant boundary",
            Self::InvalidMarkupBoundary => "invalid markup boundary",
            Self::MissingComplexBody => "declarations are not followed by a complex body",
            Self::UnexpectedToken => "unexpected token",
            Self::SpanOverflow => "source length exceeds u32::MAX byte offsets",
            Self::InvalidEscape => "invalid escape sequence",
            Self::AmbiguousMessageMode => "ambiguous message mode; recovered as a simple message",
            Self::MissingRequiredWhitespace => "missing required whitespace between productions",
            Self::MissingIdentifierName => "missing identifier name after ':'",
            Self::InvalidInputDeclaration => {
                "'.input' declaration value must be a variable expression"
            }
        }
    }

    /// Numeric value used by snapshot encoding and external tooling.
    #[inline]
    pub const fn as_u16(self) -> u16 {
        self as u16
    }
}

/// Sentinel for `message_ref` meaning "use the catalog static message".
pub(crate) const MESSAGE_REF_CATALOG: u32 = u32::MAX;

#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub(crate) struct DiagnosticRecord {
    pub source_id: u32,
    pub span_start: u32,
    pub span_end: u32,
    pub severity: u8,
    pub _pad: u8,
    pub code: u16,
    pub message_ref: u32,
    pub label_start: u32,
    pub label_count: u32,
}

impl DiagnosticRecord {
    pub fn from_code(source: SourceId, span: Span, code: DiagnosticCode) -> Self {
        Self {
            source_id: source.raw(),
            span_start: span.start,
            span_end: span.end,
            severity: code.severity() as u8,
            _pad: 0,
            code: code.as_u16(),
            message_ref: MESSAGE_REF_CATALOG,
            label_start: 0,
            label_count: 0,
        }
    }

    pub fn code(&self) -> DiagnosticCode {
        diagnostic_code_from_u16(self.code)
    }

    pub fn severity(&self) -> DiagnosticSeverity {
        diagnostic_severity_from_u8(self.severity)
    }

    pub fn span(&self) -> Span {
        Span::new(self.span_start, self.span_end)
    }

    pub fn source(&self) -> SourceId {
        SourceId::new(self.source_id)
    }
}

#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub(crate) struct DiagnosticLabelRecord {
    pub source_id: u32,
    pub span_start: u32,
    pub span_end: u32,
    pub message_ref: u32,
}

/// Owned public diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub source: SourceId,
    pub span: Span,
    pub location: SourceLocation,
    pub severity: DiagnosticSeverity,
    pub code: DiagnosticCode,
    pub message: &'static str,
    pub labels: Vec<DiagnosticLabel>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticLabel {
    pub source: SourceId,
    pub span: Span,
    pub message: &'static str,
}

/// Borrowed diagnostics view tied to a [`crate::ParseWorkspace`].
#[derive(Debug, Clone, Copy)]
pub struct DiagnosticView<'a> {
    pub(crate) sources: &'a SourceStore,
    pub(crate) records: &'a [DiagnosticRecord],
}

impl<'a> DiagnosticView<'a> {
    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn sources(&self) -> &'a SourceStore {
        self.sources
    }

    /// Iterate as owned [`Diagnostic`] values. Each value resolves its
    /// line/column lazily; the records themselves are not allocated.
    pub fn iter(self) -> DiagnosticIter<'a> {
        DiagnosticIter {
            view: self,
            cursor: 0,
        }
    }

    pub fn record(&self, index: usize) -> Option<DiagnosticRef<'a>> {
        let record = self.records.get(index)?;
        Some(DiagnosticRef {
            sources: self.sources,
            record,
        })
    }
}

/// Lightweight reference-style view over a single diagnostic record.
#[derive(Debug, Clone, Copy)]
pub struct DiagnosticRef<'a> {
    pub(crate) sources: &'a SourceStore,
    pub(crate) record: &'a DiagnosticRecord,
}

impl DiagnosticRef<'_> {
    pub fn code(&self) -> DiagnosticCode {
        self.record.code()
    }

    pub fn severity(&self) -> DiagnosticSeverity {
        self.record.severity()
    }

    pub fn span(&self) -> Span {
        self.record.span()
    }

    pub fn source(&self) -> SourceId {
        self.record.source()
    }

    pub fn message(&self) -> &'static str {
        self.code().static_message()
    }

    pub fn location(&self) -> SourceLocation {
        self.sources.location(self.source(), self.span())
    }

    pub fn to_owned_diagnostic(&self) -> Diagnostic {
        let code = self.code();
        Diagnostic {
            source: self.source(),
            span: self.span(),
            location: self.location(),
            severity: self.severity(),
            code,
            message: code.static_message(),
            labels: Vec::new(),
        }
    }
}

pub struct DiagnosticIter<'a> {
    view: DiagnosticView<'a>,
    cursor: usize,
}

impl Iterator for DiagnosticIter<'_> {
    type Item = Diagnostic;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.view.record(self.cursor)?.to_owned_diagnostic();
        self.cursor += 1;
        Some(item)
    }
}

fn diagnostic_severity_from_u8(value: u8) -> DiagnosticSeverity {
    match value {
        0 => DiagnosticSeverity::Error,
        1 => DiagnosticSeverity::Warning,
        2 => DiagnosticSeverity::Information,
        _ => DiagnosticSeverity::Hint,
    }
}

fn diagnostic_code_from_u16(value: u16) -> DiagnosticCode {
    match value {
        v if v == DiagnosticCode::UnexpectedEndOfInput as u16 => {
            DiagnosticCode::UnexpectedEndOfInput
        }
        v if v == DiagnosticCode::UnclosedExpression as u16 => DiagnosticCode::UnclosedExpression,
        v if v == DiagnosticCode::UnclosedQuotedLiteral as u16 => {
            DiagnosticCode::UnclosedQuotedLiteral
        }
        v if v == DiagnosticCode::UnclosedQuotedPattern as u16 => {
            DiagnosticCode::UnclosedQuotedPattern
        }
        v if v == DiagnosticCode::InvalidDeclarationStart as u16 => {
            DiagnosticCode::InvalidDeclarationStart
        }
        v if v == DiagnosticCode::InvalidMatcherSyntax as u16 => {
            DiagnosticCode::InvalidMatcherSyntax
        }
        v if v == DiagnosticCode::InvalidVariantBoundary as u16 => {
            DiagnosticCode::InvalidVariantBoundary
        }
        v if v == DiagnosticCode::InvalidMarkupBoundary as u16 => {
            DiagnosticCode::InvalidMarkupBoundary
        }
        v if v == DiagnosticCode::MissingComplexBody as u16 => DiagnosticCode::MissingComplexBody,
        v if v == DiagnosticCode::UnexpectedToken as u16 => DiagnosticCode::UnexpectedToken,
        v if v == DiagnosticCode::SpanOverflow as u16 => DiagnosticCode::SpanOverflow,
        v if v == DiagnosticCode::InvalidEscape as u16 => DiagnosticCode::InvalidEscape,
        v if v == DiagnosticCode::AmbiguousMessageMode as u16 => {
            DiagnosticCode::AmbiguousMessageMode
        }
        v if v == DiagnosticCode::MissingRequiredWhitespace as u16 => {
            DiagnosticCode::MissingRequiredWhitespace
        }
        v if v == DiagnosticCode::MissingIdentifierName as u16 => {
            DiagnosticCode::MissingIdentifierName
        }
        v if v == DiagnosticCode::InvalidInputDeclaration as u16 => {
            DiagnosticCode::InvalidInputDeclaration
        }
        _ => DiagnosticCode::Unspecified,
    }
}

/// Bridge that lets the parser sink diagnostics into the workspace without
/// allocating message strings on the hot path.
#[derive(Debug)]
pub(crate) struct DiagnosticSink<'a> {
    pub records: &'a mut Vec<DiagnosticRecord>,
    pub labels: &'a mut Vec<DiagnosticLabelRecord>,
}

#[allow(dead_code)] // hot-path consumer wires in with Milestone 6/7.
impl<'a> DiagnosticSink<'a> {
    pub fn new(
        records: &'a mut Vec<DiagnosticRecord>,
        labels: &'a mut Vec<DiagnosticLabelRecord>,
    ) -> Self {
        Self { records, labels }
    }

    pub fn push(&mut self, source: SourceId, span: Span, code: DiagnosticCode) {
        self.records
            .push(DiagnosticRecord::from_code(source, span, code));
    }

    pub fn push_with_label(
        &mut self,
        source: SourceId,
        span: Span,
        code: DiagnosticCode,
        label_span: Span,
        label_message: u32,
    ) {
        let label_start = self.labels.len() as u32;
        self.labels.push(DiagnosticLabelRecord {
            source_id: source.raw(),
            span_start: label_span.start,
            span_end: label_span.end,
            message_ref: label_message,
        });
        let mut record = DiagnosticRecord::from_code(source, span, code);
        record.label_start = label_start;
        record.label_count = 1;
        self.records.push(record);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::size_of;

    #[test]
    fn record_sizes_stay_within_budget() {
        // design/002 §"Record Layout / Size Budget":
        //   DiagnosticRecord <= 32 bytes
        //   DiagnosticLabelRecord <= 16 bytes
        assert!(size_of::<DiagnosticRecord>() <= 32);
        assert!(size_of::<DiagnosticLabelRecord>() <= 16);
    }

    #[test]
    fn catalog_has_one_message_per_code() {
        use DiagnosticCode::*;
        for code in [
            Unspecified,
            UnexpectedEndOfInput,
            UnclosedExpression,
            UnclosedQuotedLiteral,
            UnclosedQuotedPattern,
            InvalidDeclarationStart,
            InvalidMatcherSyntax,
            InvalidVariantBoundary,
            InvalidMarkupBoundary,
            MissingComplexBody,
            UnexpectedToken,
            SpanOverflow,
            InvalidEscape,
            AmbiguousMessageMode,
            MissingRequiredWhitespace,
            MissingIdentifierName,
            InvalidInputDeclaration,
        ] {
            let msg = code.static_message();
            assert!(!msg.is_empty(), "missing message for {code:?}");
        }
    }

    #[test]
    fn diagnostic_sink_resolves_line_and_column() {
        use crate::source::{SourceFileInput, SourceStore};

        let mut store = SourceStore::new();
        let id = store.add(SourceFileInput {
            source: "ab\ncd",
            ..Default::default()
        });

        let mut records: Vec<DiagnosticRecord> = Vec::new();
        let mut labels: Vec<DiagnosticLabelRecord> = Vec::new();
        let mut sink = DiagnosticSink::new(&mut records, &mut labels);
        sink.push(id, Span::new(3, 4), DiagnosticCode::UnclosedExpression);

        let view = DiagnosticView {
            sources: &store,
            records: &records,
        };
        assert_eq!(view.len(), 1);
        let diag = view.iter().next().unwrap();
        assert_eq!(diag.code, DiagnosticCode::UnclosedExpression);
        assert_eq!(diag.location, SourceLocation { line: 2, column: 1 });
        assert_eq!(
            diag.message,
            DiagnosticCode::UnclosedExpression.static_message()
        );
        assert_eq!(diag.severity, DiagnosticSeverity::Error);
    }
}
