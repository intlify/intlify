//! Compact diagnostic model.
//!
//! Full diagnostic catalog and label table land in Milestone 4.

use crate::source::SourceStore;
use crate::span::{SourceId, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum DiagnosticSeverity {
    Error = 0,
    Warning = 1,
    Information = 2,
    Hint = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
#[non_exhaustive]
pub enum DiagnosticCode {
    // Reserved — concrete codes land with Milestone 4.
    Unspecified = 0,
}

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

#[allow(dead_code)] // populated in Milestone 4.
#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub(crate) struct DiagnosticLabelRecord {
    pub source_id: u32,
    pub span_start: u32,
    pub span_end: u32,
    pub message_ref: u32,
}

/// Owned public diagnostic value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub source: SourceId,
    pub span: Span,
    pub severity: DiagnosticSeverity,
    pub code: DiagnosticCode,
    pub message: String,
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
}
