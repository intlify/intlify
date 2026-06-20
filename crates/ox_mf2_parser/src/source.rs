//! Source ownership: `SourceStore`, `SourceFile`, `SourceFileInput`.
//!
//! Full implementation lands in Milestone 1.

use crate::span::{SourceId, Span};

/// Public input used to register a source file with [`SourceStore`].
#[derive(Debug, Default, Clone)]
pub struct SourceFileInput<'a> {
    pub source: &'a str,
    pub path: Option<&'a str>,
    pub locale: Option<&'a str>,
    pub message_id: Option<&'a str>,
    pub base_offset: Option<u32>,
}

/// Owned source file registered in [`SourceStore`].
#[derive(Debug, Default, Clone)]
pub struct SourceFile {
    pub id: SourceId,
    pub path: Option<String>,
    pub locale: Option<String>,
    pub message_id: Option<String>,
    pub base_offset: u32,
    pub text: String,
    pub line_starts: Vec<u32>,
}

/// Resolved line/column pair derived from [`SourceStore`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SourceLocation {
    pub line: u32,
    pub column: u32,
}

/// Source ownership for single parse, batch parse, diagnostics, and Phase 2
/// snapshot roots.
#[derive(Debug, Default)]
pub struct SourceStore {
    files: Vec<SourceFile>,
}

impl SourceStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, _input: SourceFileInput<'_>) -> SourceId {
        SourceId::new(0)
    }

    pub fn get(&self, _id: SourceId) -> Option<&SourceFile> {
        self.files.first()
    }

    pub fn slice(&self, _span: Span) -> &str {
        ""
    }

    pub fn location(&self, _id: SourceId, _span: Span) -> SourceLocation {
        SourceLocation::default()
    }

    pub fn files(&self) -> &[SourceFile] {
        &self.files
    }
}
