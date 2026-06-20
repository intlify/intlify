//! Optional `SemanticModel` lowered from `CstTables`.
//!
//! Only built when [`crate::ParseOptions::parse_semantic`] is `true`. The
//! lowering walks the CST through [`crate::CstView`] and emits per-record
//! [`SemanticRef`]s back to source `NodeId` + `Span` so consumers can jump
//! between the semantic model and the CST without copying source text.
//!
//! Phase 1 deliberately keeps the model thin:
//!
//! - Raw spans only — cooked values / NFC comparison keys belong to the
//!   semantic validation path, not parse hot paths.
//! - No selector coverage analysis, no duplicate-name policy, no runtime
//!   fallback resolution.
//!
//! See `design/002-ox-mf2-phase-1-rust-parser-design.md` §"SemanticModel
//! Design" for the longer-form rationale.

use crate::diagnostic::Diagnostic;
use crate::source::SourceStore;
use crate::span::{NodeId, SourceId, Span};
use crate::syntax_kind::SyntaxKind;
use crate::tables::CstTables;
use crate::view::{CstChild, CstNodeView, CstView};

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

/// Reference from a semantic record back to its CST origin.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SemanticRef {
    pub node: NodeId,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeclarationKind {
    Input,
    Local,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExpressionKind {
    Literal,
    Variable,
    Function,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MarkupKind {
    Open,
    Standalone,
    Close,
}

#[derive(Debug, Clone, Copy)]
pub struct DeclarationRecord {
    pub semantic_ref: SemanticRef,
    pub kind: DeclarationKind,
    pub variable: Option<SemanticRef>,
}

#[derive(Debug, Clone, Copy)]
pub struct ReferenceRecord {
    pub semantic_ref: SemanticRef,
    /// Source span covering just the `name` (without the `$` sigil).
    pub name_span: Span,
}

#[derive(Debug, Clone, Copy)]
pub struct PatternRecord {
    pub semantic_ref: SemanticRef,
    pub is_quoted: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct ExpressionRecord {
    pub semantic_ref: SemanticRef,
    pub kind: ExpressionKind,
}

#[derive(Debug, Clone, Copy)]
pub struct MarkupRecord {
    pub semantic_ref: SemanticRef,
    pub kind: MarkupKind,
}

#[derive(Debug, Clone, Copy)]
pub struct LiteralRecord {
    pub semantic_ref: SemanticRef,
    pub is_quoted: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct FunctionRecord {
    pub semantic_ref: SemanticRef,
    /// Span of the function identifier (without the leading `:`).
    pub identifier_span: Span,
}

#[derive(Debug, Clone, Copy)]
pub struct OptionRecord {
    pub semantic_ref: SemanticRef,
    pub identifier_span: Span,
}

#[derive(Debug, Clone, Copy)]
pub struct AttributeRecord {
    pub semantic_ref: SemanticRef,
    pub identifier_span: Span,
}

#[derive(Debug, Clone, Copy)]
pub struct SelectorRecord {
    pub semantic_ref: SemanticRef,
    pub variable: Option<SemanticRef>,
}

#[derive(Debug, Clone, Copy)]
pub struct VariantRecord {
    pub semantic_ref: SemanticRef,
    pub key_count: u32,
    pub has_catch_all: bool,
}

/// Optional semantic lowering result.
#[derive(Debug, Default, Clone)]
pub struct SemanticModel {
    pub mode: MessageMode,
    pub kind: SemanticMessageKind,
    pub declarations: Vec<DeclarationRecord>,
    pub references: Vec<ReferenceRecord>,
    pub patterns: Vec<PatternRecord>,
    pub expressions: Vec<ExpressionRecord>,
    pub markups: Vec<MarkupRecord>,
    pub literals: Vec<LiteralRecord>,
    pub functions: Vec<FunctionRecord>,
    pub options: Vec<OptionRecord>,
    pub attributes: Vec<AttributeRecord>,
    pub selectors: Vec<SelectorRecord>,
    pub variants: Vec<VariantRecord>,
    pub diagnostics: Vec<Diagnostic>,
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

    pub fn declarations(&self) -> &[DeclarationRecord] {
        &self.model.declarations
    }

    pub fn references(&self) -> &[ReferenceRecord] {
        &self.model.references
    }

    pub fn patterns(&self) -> &[PatternRecord] {
        &self.model.patterns
    }

    pub fn expressions(&self) -> &[ExpressionRecord] {
        &self.model.expressions
    }

    pub fn variants(&self) -> &[VariantRecord] {
        &self.model.variants
    }
}

/// Lower a [`CstView`] into a [`SemanticModel`]. The caller decides whether
/// to invoke this — controlled by [`crate::ParseOptions::parse_semantic`].
pub fn lower(sources: &SourceStore, source_id: SourceId, tables: &CstTables) -> SemanticModel {
    let view = CstView::new(sources, source_id, tables);
    let mut model = SemanticModel::default();
    let Some(root) = view.root() else {
        return model;
    };

    // Top-level child should be SimpleMessage or ComplexMessage.
    for child in root.children() {
        let CstChild::Node(message_node) = child else { continue };
        match message_node.kind() {
            SyntaxKind::SimpleMessage => {
                model.mode = MessageMode::Simple;
                model.kind = SemanticMessageKind::Pattern;
                lower_message_children(&message_node, &mut model);
            }
            SyntaxKind::ComplexMessage => {
                model.mode = MessageMode::Complex;
                model.kind = SemanticMessageKind::Pattern; // refined below
                lower_message_children(&message_node, &mut model);
            }
            _ => {}
        }
    }
    model
}

fn lower_message_children(node: &CstNodeView<'_>, model: &mut SemanticModel) {
    for child in node.children() {
        let CstChild::Node(n) = child else { continue };
        match n.kind() {
            SyntaxKind::Pattern => collect_pattern(&n, model, /*is_quoted=*/ false),
            SyntaxKind::QuotedPattern => {
                model.patterns.push(PatternRecord {
                    semantic_ref: semantic_ref(&n),
                    is_quoted: true,
                });
                for inner in n.children() {
                    if let CstChild::Node(inner_node) = inner {
                        if inner_node.kind() == SyntaxKind::Pattern {
                            collect_pattern(&inner_node, model, true);
                        }
                    }
                }
            }
            SyntaxKind::InputDeclaration => {
                let variable = find_first_node(&n, SyntaxKind::Variable).map(semantic_ref_of);
                let placeholder_var =
                    find_first_node(&n, SyntaxKind::Placeholder).and_then(|p| {
                        find_first_node(&p, SyntaxKind::VariableExpression).and_then(|ve| {
                            find_first_node(&ve, SyntaxKind::Variable).map(semantic_ref_of)
                        })
                    });
                model.declarations.push(DeclarationRecord {
                    semantic_ref: semantic_ref(&n),
                    kind: DeclarationKind::Input,
                    variable: variable.or(placeholder_var),
                });
            }
            SyntaxKind::LocalDeclaration => {
                let variable = find_first_node(&n, SyntaxKind::Variable).map(semantic_ref_of);
                model.declarations.push(DeclarationRecord {
                    semantic_ref: semantic_ref(&n),
                    kind: DeclarationKind::Local,
                    variable,
                });
                // Walk the right-hand-side expression for references.
                walk_for_references(&n, model);
                walk_for_expressions(&n, model);
            }
            SyntaxKind::ComplexBody => {
                for body_child in n.children() {
                    let CstChild::Node(b) = body_child else { continue };
                    match b.kind() {
                        SyntaxKind::QuotedPattern => {
                            model.kind = SemanticMessageKind::Pattern;
                            model.patterns.push(PatternRecord {
                                semantic_ref: semantic_ref(&b),
                                is_quoted: true,
                            });
                            for inner in b.children() {
                                if let CstChild::Node(inner_node) = inner {
                                    if inner_node.kind() == SyntaxKind::Pattern {
                                        collect_pattern(&inner_node, model, true);
                                    }
                                }
                            }
                        }
                        SyntaxKind::Matcher => {
                            model.kind = SemanticMessageKind::Select;
                            collect_matcher(&b, model);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

fn collect_pattern(node: &CstNodeView<'_>, model: &mut SemanticModel, is_quoted: bool) {
    model.patterns.push(PatternRecord {
        semantic_ref: semantic_ref(node),
        is_quoted,
    });
    for child in node.children() {
        let CstChild::Node(n) = child else { continue };
        if n.kind() == SyntaxKind::Placeholder {
            collect_placeholder(&n, model);
        }
    }
}

fn collect_placeholder(node: &CstNodeView<'_>, model: &mut SemanticModel) {
    for child in node.children() {
        let CstChild::Node(n) = child else { continue };
        match n.kind() {
            SyntaxKind::VariableExpression => {
                model.expressions.push(ExpressionRecord {
                    semantic_ref: semantic_ref(&n),
                    kind: ExpressionKind::Variable,
                });
                walk_for_references(&n, model);
                walk_for_expressions(&n, model);
            }
            SyntaxKind::LiteralExpression => {
                model.expressions.push(ExpressionRecord {
                    semantic_ref: semantic_ref(&n),
                    kind: ExpressionKind::Literal,
                });
                walk_for_expressions(&n, model);
            }
            SyntaxKind::FunctionExpression => {
                model.expressions.push(ExpressionRecord {
                    semantic_ref: semantic_ref(&n),
                    kind: ExpressionKind::Function,
                });
                walk_for_expressions(&n, model);
            }
            SyntaxKind::Markup => {
                let kind = detect_markup_kind(&n);
                model.markups.push(MarkupRecord {
                    semantic_ref: semantic_ref(&n),
                    kind,
                });
            }
            _ => {}
        }
    }
}

fn detect_markup_kind(node: &CstNodeView<'_>) -> MarkupKind {
    let mut saw_hash = false;
    let mut saw_slash = false;
    for child in node.children() {
        if let CstChild::Token(t) = child {
            match t.kind() {
                SyntaxKind::HashToken => saw_hash = true,
                SyntaxKind::SlashToken => saw_slash = true,
                _ => {}
            }
        }
    }
    match (saw_hash, saw_slash) {
        (true, true) => MarkupKind::Standalone,
        (true, false) => MarkupKind::Open,
        (false, true) => MarkupKind::Close,
        _ => MarkupKind::Open,
    }
}

fn collect_matcher(node: &CstNodeView<'_>, model: &mut SemanticModel) {
    for child in node.children() {
        let CstChild::Node(n) = child else { continue };
        match n.kind() {
            SyntaxKind::Selector => {
                let variable = find_first_node(&n, SyntaxKind::Variable).map(semantic_ref_of);
                model.selectors.push(SelectorRecord {
                    semantic_ref: semantic_ref(&n),
                    variable,
                });
                if let Some(var) = variable {
                    model.references.push(ReferenceRecord {
                        semantic_ref: var,
                        name_span: var.span,
                    });
                }
            }
            SyntaxKind::Variant => {
                let mut key_count = 0u32;
                let mut has_catch_all = false;
                for key_child in n.children() {
                    if let CstChild::Node(k) = key_child {
                        match k.kind() {
                            SyntaxKind::CatchAllKey => {
                                has_catch_all = true;
                                key_count += 1;
                            }
                            SyntaxKind::VariantKey => {
                                key_count += 1;
                                for lit_child in k.children() {
                                    if let CstChild::Node(l) = lit_child {
                                        if l.kind() == SyntaxKind::QuotedLiteral
                                            || l.kind() == SyntaxKind::UnquotedLiteral
                                        {
                                            model.literals.push(LiteralRecord {
                                                semantic_ref: semantic_ref(&l),
                                                is_quoted: l.kind() == SyntaxKind::QuotedLiteral,
                                            });
                                        }
                                    }
                                }
                            }
                            SyntaxKind::QuotedPattern => {
                                model.patterns.push(PatternRecord {
                                    semantic_ref: semantic_ref(&k),
                                    is_quoted: true,
                                });
                                for inner in k.children() {
                                    if let CstChild::Node(inner_node) = inner {
                                        if inner_node.kind() == SyntaxKind::Pattern {
                                            collect_pattern(&inner_node, model, true);
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                model.variants.push(VariantRecord {
                    semantic_ref: semantic_ref(&n),
                    key_count,
                    has_catch_all,
                });
            }
            _ => {}
        }
    }
}

fn walk_for_references(node: &CstNodeView<'_>, model: &mut SemanticModel) {
    if node.kind() == SyntaxKind::Variable {
        let semantic_ref = semantic_ref_of(*node);
        model.references.push(ReferenceRecord {
            semantic_ref,
            name_span: semantic_ref.span,
        });
        return;
    }
    for child in node.children() {
        if let CstChild::Node(n) = child {
            walk_for_references(&n, model);
        }
    }
}

fn walk_for_expressions(node: &CstNodeView<'_>, model: &mut SemanticModel) {
    for child in node.children() {
        let CstChild::Node(n) = child else { continue };
        match n.kind() {
            SyntaxKind::Function => {
                let identifier_span = find_first_node(&n, SyntaxKind::Identifier)
                    .map(|id| id.span())
                    .unwrap_or_default();
                model.functions.push(FunctionRecord {
                    semantic_ref: semantic_ref(&n),
                    identifier_span,
                });
                walk_for_expressions(&n, model);
            }
            SyntaxKind::Option => {
                let identifier_span = find_first_node(&n, SyntaxKind::Identifier)
                    .map(|id| id.span())
                    .unwrap_or_default();
                model.options.push(OptionRecord {
                    semantic_ref: semantic_ref(&n),
                    identifier_span,
                });
                walk_for_expressions(&n, model);
            }
            SyntaxKind::Attribute => {
                let identifier_span = find_first_node(&n, SyntaxKind::Identifier)
                    .map(|id| id.span())
                    .unwrap_or_default();
                model.attributes.push(AttributeRecord {
                    semantic_ref: semantic_ref(&n),
                    identifier_span,
                });
            }
            SyntaxKind::QuotedLiteral | SyntaxKind::UnquotedLiteral => {
                model.literals.push(LiteralRecord {
                    semantic_ref: semantic_ref(&n),
                    is_quoted: n.kind() == SyntaxKind::QuotedLiteral,
                });
                walk_for_expressions(&n, model);
            }
            _ => walk_for_expressions(&n, model),
        }
    }
}

fn find_first_node<'a>(node: &CstNodeView<'a>, kind: SyntaxKind) -> Option<CstNodeView<'a>> {
    for child in node.children() {
        if let CstChild::Node(n) = child {
            if n.kind() == kind {
                return Some(n);
            }
            if let Some(found) = find_first_node(&n, kind) {
                return Some(found);
            }
        }
    }
    None
}

#[inline]
fn semantic_ref(node: &CstNodeView<'_>) -> SemanticRef {
    SemanticRef {
        node: node.id(),
        span: node.span(),
    }
}

#[inline]
fn semantic_ref_of(node: CstNodeView<'_>) -> SemanticRef {
    SemanticRef {
        node: node.id(),
        span: node.span(),
    }
}
