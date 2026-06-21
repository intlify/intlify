// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

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
//! See `design/002-ox-mf2-phase-1-rust-parser-design.md` §"`SemanticModel`
//! Design" for the longer-form rationale.

use crate::diagnostic::Diagnostic;
use crate::source::SourceStore;
use crate::span::{NodeId, SourceId, Span};
use crate::syntax_kind::SyntaxKind;
use crate::tables::{CstEdgeKind, CstNodeRecord, CstTables};

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

/// Lower a [`CstView`] into a fresh [`SemanticModel`]. Convenience wrapper
/// around [`lower_into`] that allocates a new model — prefer the latter
/// when you already own a model whose capacity can be reused (LSP, batch
/// loops, the borrowed-session workspace).
pub fn lower(sources: &SourceStore, source_id: SourceId, tables: &CstTables) -> SemanticModel {
    let mut model = SemanticModel::default();
    lower_into(sources, source_id, tables, &mut model);
    model
}

/// Lower a [`CstTables`] into a caller-owned [`SemanticModel`]. The model
/// is cleared first so all `Vec` capacities are reused. Used by
/// [`crate::ParseWorkspace`] so repeated semantic lowering does not
/// re-allocate the record tables.
///
/// Walks the CST through `CstTables::node_at` / `CstTables::edges_for`
/// directly instead of the public `CstView` layer; spans / kinds come
/// straight off the raw `CstNodeRecord` so the hot path avoids the
/// iterator dispatch, repeated `kind()` lookups, and `find_first_node`
/// recursion that the view-based traversal incurred. The public `CstView`
/// surface stays unchanged for external tooling.
pub fn lower_into(
    sources: &SourceStore,
    source_id: SourceId,
    tables: &CstTables,
    model: &mut SemanticModel,
) {
    // Lowering only needs the topology recorded in `tables`. The source
    // text and id are retained on the public signature for binary
    // stability and for future cooked-value derivations that may need
    // them, but the raw walker itself is span-only.
    let _ = (sources, source_id);
    reset_model(model);
    let Some(root_id) = tables.root_id() else {
        return;
    };
    let root_rec = tables.node_at(root_id);
    // Top-level child should be SimpleMessage or ComplexMessage.
    for (_, message_rec) in iter_node_children(tables, root_rec) {
        let kind = message_rec.kind;
        if kind == SyntaxKind::SimpleMessage as u16 {
            model.mode = MessageMode::Simple;
            model.kind = SemanticMessageKind::Pattern;
            lower_message_children(tables, message_rec, model);
        } else if kind == SyntaxKind::ComplexMessage as u16 {
            model.mode = MessageMode::Complex;
            model.kind = SemanticMessageKind::Pattern; // refined below
            lower_message_children(tables, message_rec, model);
        }
    }
}

/// Drain every record vector while preserving capacity. Mirrors
/// `ParseWorkspace::clear` so a `lower_into` call into a reused model
/// behaves identically to lowering into a fresh one.
fn reset_model(model: &mut SemanticModel) {
    model.mode = MessageMode::default();
    model.kind = SemanticMessageKind::default();
    model.declarations.clear();
    model.references.clear();
    model.patterns.clear();
    model.expressions.clear();
    model.markups.clear();
    model.literals.clear();
    model.functions.clear();
    model.options.clear();
    model.attributes.clear();
    model.selectors.clear();
    model.variants.clear();
    model.diagnostics.clear();
}

// ─────────────────────────── raw table walkers ───────────────────────────
//
// The walkers below operate on `&CstTables` + `&CstNodeRecord` instead of
// `CstNodeView`. They read each record once (so `kind` / `span` come off
// the cache-friendly `CstNodeRecord` slice) and compare kinds with the
// raw `u16` numeric value to skip the `SyntaxKind` enum reconstruction
// that `CstNodeView::kind()` performed for every node visited.

#[inline]
fn span_of(rec: &CstNodeRecord) -> Span {
    Span::new(rec.span_start, rec.span_end)
}

#[inline]
fn semantic_ref_for(id: NodeId, rec: &CstNodeRecord) -> SemanticRef {
    SemanticRef {
        node: id,
        span: span_of(rec),
    }
}

/// Iterator over the direct *node* children of `rec`, paired with the
/// underlying record so the caller does not have to re-fetch it.
#[inline]
fn iter_node_children<'a>(
    tables: &'a CstTables,
    rec: &'a CstNodeRecord,
) -> impl Iterator<Item = (NodeId, &'a CstNodeRecord)> + 'a {
    tables.edges_for(rec).iter().filter_map(move |edge| {
        if edge.kind == CstEdgeKind::Node as u16 {
            let id = NodeId::new(edge.ref_id);
            Some((id, tables.node_at(id)))
        } else {
            None
        }
    })
}

/// Direct-child lookup. The MF2 grammar pins the relevant children to a
/// shallow position (e.g. `Function -> Identifier`) so this avoids the
/// O(subtree) recursion the previous view-based `find_first_node` would
/// pay when walking declarations.
#[inline]
fn first_direct_node_child<'a>(
    tables: &'a CstTables,
    rec: &'a CstNodeRecord,
    kind: SyntaxKind,
) -> Option<(NodeId, &'a CstNodeRecord)> {
    let needle = kind as u16;
    for edge in tables.edges_for(rec) {
        if edge.kind == CstEdgeKind::Node as u16 {
            let id = NodeId::new(edge.ref_id);
            let r = tables.node_at(id);
            if r.kind == needle {
                return Some((id, r));
            }
        }
    }
    None
}

#[inline]
fn first_direct_child_span(
    tables: &CstTables,
    rec: &CstNodeRecord,
    kind: SyntaxKind,
) -> Span {
    first_direct_node_child(tables, rec, kind)
        .map(|(_, r)| span_of(r))
        .unwrap_or_default()
}

fn lower_message_children(
    tables: &CstTables,
    node_rec: &CstNodeRecord,
    model: &mut SemanticModel,
) {
    for (n_id, n_rec) in iter_node_children(tables, node_rec) {
        let kind = n_rec.kind;
        if kind == SyntaxKind::Pattern as u16 {
            collect_pattern(tables, n_id, n_rec, model, /*is_quoted=*/ false);
        } else if kind == SyntaxKind::QuotedPattern as u16 {
            model.patterns.push(PatternRecord {
                semantic_ref: semantic_ref_for(n_id, n_rec),
                is_quoted: true,
            });
            for (inner_id, inner_rec) in iter_node_children(tables, n_rec) {
                if inner_rec.kind == SyntaxKind::Pattern as u16 {
                    collect_pattern(tables, inner_id, inner_rec, model, true);
                }
            }
        } else if kind == SyntaxKind::InputDeclaration as u16 {
            // `.input s { $var [:func] }` — both the declared variable and
            // (in valid input) the placeholder body live as direct children.
            // The placeholder subtree contains exactly one VariableExpression
            // whose Variable child is the declaration's LHS; capture it
            // first so `walk_expression_subtree` can skip it when collecting
            // references.
            let placeholder = first_direct_node_child(tables, n_rec, SyntaxKind::Placeholder);
            let declared_var: Option<(NodeId, SemanticRef)> = placeholder.and_then(|(_, p_rec)| {
                let (_, var_expr_rec) =
                    first_direct_node_child(tables, p_rec, SyntaxKind::VariableExpression)?;
                let (var_id, var_rec) =
                    first_direct_node_child(tables, var_expr_rec, SyntaxKind::Variable)?;
                Some((var_id, semantic_ref_for(var_id, var_rec)))
            });
            model.declarations.push(DeclarationRecord {
                semantic_ref: semantic_ref_for(n_id, n_rec),
                kind: DeclarationKind::Input,
                variable: declared_var.map(|(_, sref)| sref),
            });
            if let Some((_, p_rec)) = placeholder {
                // Walk the placeholder body once; the declared variable is
                // skipped at the source instead of being pushed-then-removed
                // from `model.references`.
                let skip = declared_var.map(|(id, _)| id);
                collect_placeholder(tables, p_rec, model, skip);
            }
        } else if kind == SyntaxKind::LocalDeclaration as u16 {
            // `.local s $var = placeholder` — the declared variable is the
            // first direct Variable child; everything else (the RHS) needs a
            // unified expression walk.
            let declared_var: Option<(NodeId, SemanticRef)> =
                first_direct_node_child(tables, n_rec, SyntaxKind::Variable)
                    .map(|(id, r)| (id, semantic_ref_for(id, r)));
            model.declarations.push(DeclarationRecord {
                semantic_ref: semantic_ref_for(n_id, n_rec),
                kind: DeclarationKind::Local,
                variable: declared_var.map(|(_, sref)| sref),
            });
            let skip = declared_var.map(|(id, _)| id);
            walk_expression_subtree(tables, n_rec, model, skip);
        } else if kind == SyntaxKind::ComplexBody as u16 {
            for (b_id, b_rec) in iter_node_children(tables, n_rec) {
                let b_kind = b_rec.kind;
                if b_kind == SyntaxKind::QuotedPattern as u16 {
                    model.kind = SemanticMessageKind::Pattern;
                    model.patterns.push(PatternRecord {
                        semantic_ref: semantic_ref_for(b_id, b_rec),
                        is_quoted: true,
                    });
                    for (inner_id, inner_rec) in iter_node_children(tables, b_rec) {
                        if inner_rec.kind == SyntaxKind::Pattern as u16 {
                            collect_pattern(tables, inner_id, inner_rec, model, true);
                        }
                    }
                } else if b_kind == SyntaxKind::Matcher as u16 {
                    model.kind = SemanticMessageKind::Select;
                    collect_matcher(tables, b_rec, model);
                }
            }
        }
    }
}

fn collect_pattern(
    tables: &CstTables,
    node_id: NodeId,
    node_rec: &CstNodeRecord,
    model: &mut SemanticModel,
    is_quoted: bool,
) {
    model.patterns.push(PatternRecord {
        semantic_ref: semantic_ref_for(node_id, node_rec),
        is_quoted,
    });
    for (_, n_rec) in iter_node_children(tables, node_rec) {
        if n_rec.kind == SyntaxKind::Placeholder as u16 {
            // Pattern placeholders never declare a variable, so no skip id.
            collect_placeholder(tables, n_rec, model, None);
        }
    }
}

fn collect_placeholder(
    tables: &CstTables,
    node_rec: &CstNodeRecord,
    model: &mut SemanticModel,
    skip_var: Option<NodeId>,
) {
    for (n_id, n_rec) in iter_node_children(tables, node_rec) {
        let kind = n_rec.kind;
        let expr_kind = if kind == SyntaxKind::VariableExpression as u16 {
            Some(ExpressionKind::Variable)
        } else if kind == SyntaxKind::LiteralExpression as u16 {
            Some(ExpressionKind::Literal)
        } else if kind == SyntaxKind::FunctionExpression as u16 {
            Some(ExpressionKind::Function)
        } else {
            None
        };
        if let Some(ek) = expr_kind {
            model.expressions.push(ExpressionRecord {
                semantic_ref: semantic_ref_for(n_id, n_rec),
                kind: ek,
            });
            walk_expression_subtree(tables, n_rec, model, skip_var);
        } else if kind == SyntaxKind::Markup as u16 {
            let markup_kind = detect_markup_kind(tables, n_rec);
            model.markups.push(MarkupRecord {
                semantic_ref: semantic_ref_for(n_id, n_rec),
                kind: markup_kind,
            });
            // Markup carries options and attributes too — walk into the
            // node so duplicate-option-name / `u:dir` markup linting can
            // see them from the semantic model alone.
            walk_expression_subtree(tables, n_rec, model, skip_var);
        }
    }
}

fn detect_markup_kind(tables: &CstTables, node_rec: &CstNodeRecord) -> MarkupKind {
    let mut saw_hash = false;
    let mut saw_slash = false;
    let hash = SyntaxKind::HashToken as u16;
    let slash = SyntaxKind::SlashToken as u16;
    for edge in tables.edges_for(node_rec) {
        if edge.kind == CstEdgeKind::Token as u16 {
            let tok_kind = tables.token_at(crate::span::TokenId::new(edge.ref_id)).kind;
            if tok_kind == hash {
                saw_hash = true;
            } else if tok_kind == slash {
                saw_slash = true;
            }
        }
    }
    match (saw_hash, saw_slash) {
        (true, true) => MarkupKind::Standalone,
        (false, true) => MarkupKind::Close,
        // (true, false) and the (false, false) fallback both mean "open" —
        // the parser only emits a sigil-less markup node when recovery
        // synthesises one, in which case Open is the safest default.
        _ => MarkupKind::Open,
    }
}

fn collect_matcher(tables: &CstTables, node_rec: &CstNodeRecord, model: &mut SemanticModel) {
    for (n_id, n_rec) in iter_node_children(tables, node_rec) {
        let kind = n_rec.kind;
        if kind == SyntaxKind::Selector as u16 {
            let var = first_direct_node_child(tables, n_rec, SyntaxKind::Variable)
                .map(|(id, r)| semantic_ref_for(id, r));
            model.selectors.push(SelectorRecord {
                semantic_ref: semantic_ref_for(n_id, n_rec),
                variable: var,
            });
            if let Some(v) = var {
                model.references.push(ReferenceRecord {
                    semantic_ref: v,
                    name_span: v.span,
                });
            }
        } else if kind == SyntaxKind::Variant as u16 {
            let mut key_count = 0u32;
            let mut has_catch_all = false;
            for (k_id, k_rec) in iter_node_children(tables, n_rec) {
                let k_kind = k_rec.kind;
                if k_kind == SyntaxKind::CatchAllKey as u16 {
                    has_catch_all = true;
                    key_count += 1;
                } else if k_kind == SyntaxKind::VariantKey as u16 {
                    key_count += 1;
                    for (l_id, l_rec) in iter_node_children(tables, k_rec) {
                        let l_kind = l_rec.kind;
                        if l_kind == SyntaxKind::QuotedLiteral as u16
                            || l_kind == SyntaxKind::UnquotedLiteral as u16
                        {
                            model.literals.push(LiteralRecord {
                                semantic_ref: semantic_ref_for(l_id, l_rec),
                                is_quoted: l_kind == SyntaxKind::QuotedLiteral as u16,
                            });
                        }
                    }
                } else if k_kind == SyntaxKind::QuotedPattern as u16 {
                    model.patterns.push(PatternRecord {
                        semantic_ref: semantic_ref_for(k_id, k_rec),
                        is_quoted: true,
                    });
                    for (inner_id, inner_rec) in iter_node_children(tables, k_rec) {
                        if inner_rec.kind == SyntaxKind::Pattern as u16 {
                            collect_pattern(tables, inner_id, inner_rec, model, true);
                        }
                    }
                }
            }
            model.variants.push(VariantRecord {
                semantic_ref: semantic_ref_for(n_id, n_rec),
                key_count,
                has_catch_all,
            });
        }
    }
}

/// Single-pass walk over an expression subtree that collects references,
/// functions, options, attributes, and literals at the same time. The
/// optional `skip_var` lets declaration handling avoid pushing the
/// declared variable into `model.references` — replacing the previous
/// push-then-`Vec::remove` dance with an O(1) check during collection.
fn walk_expression_subtree(
    tables: &CstTables,
    node_rec: &CstNodeRecord,
    model: &mut SemanticModel,
    skip_var: Option<NodeId>,
) {
    for (n_id, n_rec) in iter_node_children(tables, node_rec) {
        let kind = n_rec.kind;
        if kind == SyntaxKind::Variable as u16 {
            if skip_var == Some(n_id) {
                continue;
            }
            let sref = semantic_ref_for(n_id, n_rec);
            model.references.push(ReferenceRecord {
                semantic_ref: sref,
                name_span: sref.span,
            });
            // Variables have no expression-bearing children, so no
            // recursion is needed.
        } else if kind == SyntaxKind::Function as u16 {
            let identifier_span = first_direct_child_span(tables, n_rec, SyntaxKind::Identifier);
            model.functions.push(FunctionRecord {
                semantic_ref: semantic_ref_for(n_id, n_rec),
                identifier_span,
            });
            walk_expression_subtree(tables, n_rec, model, skip_var);
        } else if kind == SyntaxKind::Option as u16 {
            let identifier_span = first_direct_child_span(tables, n_rec, SyntaxKind::Identifier);
            model.options.push(OptionRecord {
                semantic_ref: semantic_ref_for(n_id, n_rec),
                identifier_span,
            });
            walk_expression_subtree(tables, n_rec, model, skip_var);
        } else if kind == SyntaxKind::Attribute as u16 {
            let identifier_span = first_direct_child_span(tables, n_rec, SyntaxKind::Identifier);
            model.attributes.push(AttributeRecord {
                semantic_ref: semantic_ref_for(n_id, n_rec),
                identifier_span,
            });
            walk_expression_subtree(tables, n_rec, model, skip_var);
        } else if kind == SyntaxKind::QuotedLiteral as u16
            || kind == SyntaxKind::UnquotedLiteral as u16
        {
            model.literals.push(LiteralRecord {
                semantic_ref: semantic_ref_for(n_id, n_rec),
                is_quoted: kind == SyntaxKind::QuotedLiteral as u16,
            });
        } else {
            walk_expression_subtree(tables, n_rec, model, skip_var);
        }
    }
}
