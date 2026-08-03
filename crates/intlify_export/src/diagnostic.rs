// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Portable message-local diagnostics anchored by definition identity.

use intlify_linker::DefinitionLocation;
use ox_mf2_parser::{
    Diagnostic, DiagnosticCode, DiagnosticLabel, DiagnosticSeverity, SemanticDiagnostic,
    SemanticDiagnosticCode, SourceId, Span,
};

use crate::{DiagnosticMappingInvariant, ExportPreparationInvariant};

const MAX_LABELS: usize = 32;

/// Complete bounded message-validation failure for one preparation call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportMessageValidationFailure {
    diagnostics: Vec<MappedMessageDiagnostic>,
    total_diagnostics: u64,
    truncated: bool,
}

impl ExportMessageValidationFailure {
    pub(crate) fn checked(
        diagnostics: Vec<MappedMessageDiagnostic>,
        total_diagnostics: u64,
    ) -> Result<Self, ExportPreparationInvariant> {
        let retained = u64::try_from(diagnostics.len())
            .map_err(|_| ExportPreparationInvariant::DiagnosticCountOverflow)?;
        if total_diagnostics == 0 || total_diagnostics < retained {
            return Err(ExportPreparationInvariant::DiagnosticCountOverflow);
        }
        Ok(Self {
            diagnostics,
            total_diagnostics,
            truncated: total_diagnostics > retained,
        })
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[MappedMessageDiagnostic] {
        &self.diagnostics
    }

    #[must_use]
    pub const fn total_diagnostics(&self) -> u64 {
        self.total_diagnostics
    }

    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

/// Portable diagnostic with message-local UTF-8 spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappedMessageDiagnostic {
    definition: DefinitionLocation,
    kind: MappedMessageDiagnosticKind,
    severity: DiagnosticSeverity,
    message: &'static str,
    span: MessageUtf8Span,
    labels: Vec<MappedMessageLabel>,
}

impl MappedMessageDiagnostic {
    #[must_use]
    pub const fn definition(&self) -> &DefinitionLocation {
        &self.definition
    }

    #[must_use]
    pub const fn kind(&self) -> &MappedMessageDiagnosticKind {
        &self.kind
    }

    #[must_use]
    pub const fn severity(&self) -> DiagnosticSeverity {
        self.severity
    }

    #[must_use]
    pub const fn message(&self) -> &'static str {
        self.message
    }

    #[must_use]
    pub const fn span(&self) -> &MessageUtf8Span {
        &self.span
    }

    #[must_use]
    pub fn labels(&self) -> &[MappedMessageLabel] {
        &self.labels
    }
}

/// Parser-owned category and code retained as one typed identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MappedMessageDiagnosticKind {
    Parser(DiagnosticCode),
    Semantic(SemanticDiagnosticCode),
}

impl MappedMessageDiagnosticKind {
    #[must_use]
    pub const fn category(&self) -> &'static str {
        match self {
            Self::Parser(_) => "parser",
            Self::Semantic(_) => "semantic",
        }
    }

    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Parser(code) => code.json_code(),
            Self::Semantic(code) => code.json_code(),
        }
    }
}

/// One diagnostic label in parser-owned order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappedMessageLabel {
    span: MessageUtf8Span,
    message: &'static str,
}

impl MappedMessageLabel {
    #[must_use]
    pub const fn span(&self) -> &MessageUtf8Span {
        &self.span
    }

    #[must_use]
    pub const fn message(&self) -> &'static str {
        self.message
    }
}

/// Half-open UTF-8 byte offsets into one exact message payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageUtf8Span {
    start: u32,
    end: u32,
}

impl MessageUtf8Span {
    #[must_use]
    pub const fn start(&self) -> u32 {
        self.start
    }

    #[must_use]
    pub const fn end(&self) -> u32 {
        self.end
    }

    fn checked(span: Span, payload_len: u32) -> Option<Self> {
        (span.start <= span.end && span.end <= payload_len).then_some(Self {
            start: span.start,
            end: span.end,
        })
    }
}

pub(crate) fn map_parser_diagnostic(
    definition: &DefinitionLocation,
    payload_len: u32,
    diagnostic: &Diagnostic,
) -> Result<MappedMessageDiagnostic, DiagnosticMappingInvariant> {
    map_diagnostic(
        definition,
        payload_len,
        diagnostic.source,
        diagnostic.span,
        diagnostic.severity,
        diagnostic.message,
        MappedMessageDiagnosticKind::Parser(diagnostic.code),
        &diagnostic.labels,
    )
}

pub(crate) fn map_semantic_diagnostic(
    definition: &DefinitionLocation,
    payload_len: u32,
    diagnostic: &SemanticDiagnostic,
) -> Result<MappedMessageDiagnostic, DiagnosticMappingInvariant> {
    map_diagnostic(
        definition,
        payload_len,
        diagnostic.source(),
        diagnostic.span(),
        diagnostic.severity(),
        diagnostic.message(),
        MappedMessageDiagnosticKind::Semantic(diagnostic.code()),
        diagnostic.labels(),
    )
}

#[allow(clippy::too_many_arguments)]
fn map_diagnostic(
    definition: &DefinitionLocation,
    payload_len: u32,
    source: SourceId,
    span: Span,
    severity: DiagnosticSeverity,
    message: &'static str,
    kind: MappedMessageDiagnosticKind,
    labels: &[DiagnosticLabel],
) -> Result<MappedMessageDiagnostic, DiagnosticMappingInvariant> {
    // Preserve the documented invariant precedence: the complete label count
    // is checked before the primary span. `map_labels` repeats the bound so it
    // remains a self-contained mapping boundary.
    if labels.len() > MAX_LABELS {
        return Err(DiagnosticMappingInvariant::LabelCountExceeded);
    }
    let span = MessageUtf8Span::checked(span, payload_len)
        .ok_or(DiagnosticMappingInvariant::InvalidPrimarySpan)?;
    let mapped_labels = map_labels(source, payload_len, labels)?;
    Ok(MappedMessageDiagnostic {
        definition: definition.clone(),
        kind,
        severity,
        message,
        span,
        labels: mapped_labels,
    })
}

fn map_labels(
    source: SourceId,
    payload_len: u32,
    labels: &[DiagnosticLabel],
) -> Result<Vec<MappedMessageLabel>, DiagnosticMappingInvariant> {
    if labels.len() > MAX_LABELS {
        return Err(DiagnosticMappingInvariant::LabelCountExceeded);
    }
    let mut mapped = Vec::with_capacity(labels.len());
    for label in labels {
        let span = MessageUtf8Span::checked(label.span, payload_len)
            .ok_or(DiagnosticMappingInvariant::InvalidLabelSpan)?;
        if label.source != source {
            return Err(DiagnosticMappingInvariant::ForeignLabelSource);
        }
        mapped.push(MappedMessageLabel {
            span,
            message: label.message,
        });
    }
    Ok(mapped)
}

#[cfg(test)]
mod tests {
    use ox_mf2_parser::{DiagnosticLabel, SourceId, Span};

    use super::{map_labels, MessageUtf8Span, MAX_LABELS};
    use crate::DiagnosticMappingInvariant;

    fn label(source: SourceId, span: Span) -> DiagnosticLabel {
        DiagnosticLabel {
            source,
            span,
            message: "label",
        }
    }

    #[test]
    fn message_spans_are_half_open_and_payload_bounded() {
        assert_eq!(
            MessageUtf8Span::checked(Span::new(0, 4), 4).unwrap().end(),
            4
        );
        assert!(MessageUtf8Span::checked(Span::new(3, 2), 4).is_none());
        assert!(MessageUtf8Span::checked(Span::new(0, 5), 4).is_none());
    }

    #[test]
    fn label_mapping_checks_count_span_then_source() {
        let source = SourceId::new(0);
        let too_many = vec![label(source, Span::new(0, 0)); MAX_LABELS + 1];
        assert_eq!(
            map_labels(source, 0, &too_many).unwrap_err(),
            DiagnosticMappingInvariant::LabelCountExceeded
        );
        assert_eq!(
            map_labels(source, 1, &[label(source, Span::new(0, 2))]).unwrap_err(),
            DiagnosticMappingInvariant::InvalidLabelSpan
        );
        assert_eq!(
            map_labels(source, 1, &[label(SourceId::new(1), Span::new(0, 1))]).unwrap_err(),
            DiagnosticMappingInvariant::ForeignLabelSource
        );
    }
}
