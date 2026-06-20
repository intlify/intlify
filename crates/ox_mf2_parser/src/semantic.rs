//! Optional `SemanticModel` lowered from `CstTables`.
//!
//! Full lowering lands in Milestone 8.

use crate::diagnostic::Diagnostic;
use crate::span::{NodeId, Span};
use crate::tables::CstTables;

/// Syntactic message mode (`simple-message` vs `complex-message`).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessageMode {
    #[default]
    Simple,
    Complex,
}

/// Data-model message kind (`PatternMessage` vs `SelectMessage`).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SemanticMessageKind {
    #[default]
    Pattern,
    Select,
}

/// Lazy reference from a semantic record back to its CST origin.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SemanticRef {
    pub node: NodeId,
    pub span: Span,
}

/// Optional semantic lowering result.
///
/// Only built when [`crate::ParseOptions::parse_semantic`] is true.
#[derive(Debug, Default, Clone)]
pub struct SemanticModel {
    pub mode: MessageMode,
    pub kind: SemanticMessageKind,
    pub diagnostics: Vec<Diagnostic>,
    // declarations / references / patterns / expressions / markups / literals
    // / functions / options / attributes / selectors / variants — land in
    // Milestone 8.
}

/// Borrowed view onto a [`SemanticModel`].
#[derive(Debug, Clone, Copy)]
pub struct SemanticView<'a> {
    pub(crate) model: &'a SemanticModel,
    #[allow(dead_code)]
    pub(crate) tables: &'a CstTables,
}

impl<'a> SemanticView<'a> {
    pub fn new(model: &'a SemanticModel, tables: &'a CstTables) -> Self {
        Self { model, tables }
    }

    pub fn mode(&self) -> MessageMode {
        self.model.mode
    }

    pub fn kind(&self) -> SemanticMessageKind {
        self.model.kind
    }
}
