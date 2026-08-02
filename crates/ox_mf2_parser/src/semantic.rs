// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Source-backed `SemanticModel` construction from `CstTables`.
//!
//! [`build_semantic_model`] is the checked consumer boundary. It accepts only
//! a parser-diagnostic-free result paired with its source owner, then derives
//! raw CST-linked records and cooked read-only facts. Parsing remains a syntax
//! phase; the returned model is owned separately from [`crate::ParseResult`].
//!
//! The model owns facts only. Parser-owned data-model diagnostics are produced
//! separately by [`crate::validate_semantics`], and locale or runtime fallback
//! behavior remains outside this module.
//!
//! See `design/002-ox-mf2-phase-1-rust-parser-design.md` §"`SemanticModel`
//! Design" for the longer-form rationale.

use crate::source::SourceStore;
use crate::span::{NodeId, SourceId, Span};
use crate::syntax_kind::SyntaxKind;
use crate::tables::{CstEdgeKind, CstNodeRecord, CstTables};
use unicode_normalization::UnicodeNormalization;

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

/// Stable model-local identity of one declaration in source order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeclarationId(u32);

impl DeclarationId {
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    const fn from_index(index: usize) -> Self {
        Self(index as u32)
    }

    const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Stable model-local identity of one variable reference in source order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReferenceId(u32);

impl ReferenceId {
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    const fn from_index(index: usize) -> Self {
        Self(index as u32)
    }

    const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Stable model-local identity of one matcher in source order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MatcherId(u32);

impl MatcherId {
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    const fn from_index(index: usize) -> Self {
        Self(index as u32)
    }
}

/// Stable model-local identity of one matcher variant in source order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VariantId(u32);

impl VariantId {
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    const fn from_index(index: usize) -> Self {
        Self(index as u32)
    }
}

/// Syntactic role of a variable reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReferenceKind {
    LocalRhs,
    Selector,
    MessageBody,
    FunctionOption,
    MarkupOption,
}

/// Read-only cooked declaration fact shared by semantic consumers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticDeclaration {
    id: DeclarationId,
    kind: DeclarationKind,
    name: Box<str>,
    span: Span,
    name_span: Span,
    function_annotation: Option<Box<str>>,
}

impl SemanticDeclaration {
    #[must_use]
    pub const fn id(&self) -> DeclarationId {
        self.id
    }

    #[must_use]
    pub const fn kind(&self) -> DeclarationKind {
        self.kind
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    #[must_use]
    pub const fn name_span(&self) -> Span {
        self.name_span
    }

    /// Return the direct function annotation identifier without its leading colon.
    #[must_use]
    pub fn function_annotation(&self) -> Option<&str> {
        self.function_annotation.as_deref()
    }

    /// Return the direct input annotation; local annotations are intentionally excluded.
    #[must_use]
    pub fn input_annotation(&self) -> Option<&str> {
        (self.kind == DeclarationKind::Input)
            .then(|| self.function_annotation())
            .flatten()
    }
}

/// Read-only cooked variable-reference fact with parser-owned resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticReference {
    id: ReferenceId,
    name: Box<str>,
    kind: ReferenceKind,
    span: Span,
    resolved_declaration: Option<DeclarationId>,
    enclosing_declaration: Option<DeclarationId>,
    is_declaration_dependency: bool,
}

impl SemanticReference {
    #[must_use]
    pub const fn id(&self) -> ReferenceId {
        self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn kind(&self) -> ReferenceKind {
        self.kind
    }

    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    #[must_use]
    pub const fn resolved_declaration(&self) -> Option<DeclarationId> {
        self.resolved_declaration
    }

    #[must_use]
    pub const fn enclosing_declaration(&self) -> Option<DeclarationId> {
        self.enclosing_declaration
    }

    #[must_use]
    pub const fn is_declaration_dependency(&self) -> bool {
        self.is_declaration_dependency
    }
}

/// Owner taxonomy for duplicate-option validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OptionOwnerKind {
    Function,
    Markup,
}

/// One cooked option occurrence in owner-local order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticOption {
    owner_kind: OptionOwnerKind,
    owner_ordinal: u32,
    owner_local_ordinal: u32,
    name: Box<str>,
    span: Span,
}

/// Owner taxonomy for read-only attribute occurrences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttributeOwnerKind {
    Expression,
    MarkupOpen,
    MarkupClose,
    MarkupStandalone,
}

/// One cooked attribute occurrence in owner-local order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticAttribute {
    owner_kind: AttributeOwnerKind,
    owner_ordinal: u32,
    owner_local_ordinal: u32,
    name: Box<str>,
    span: Span,
}

impl SemanticAttribute {
    #[must_use]
    pub const fn owner_kind(&self) -> AttributeOwnerKind {
        self.owner_kind
    }

    #[must_use]
    pub const fn owner_ordinal(&self) -> u32 {
        self.owner_ordinal
    }

    #[must_use]
    pub const fn owner_local_ordinal(&self) -> u32 {
        self.owner_local_ordinal
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }
}

impl SemanticOption {
    #[must_use]
    pub const fn owner_kind(&self) -> OptionOwnerKind {
        self.owner_kind
    }

    #[must_use]
    pub const fn owner_ordinal(&self) -> u32 {
        self.owner_ordinal
    }

    #[must_use]
    pub const fn owner_local_ordinal(&self) -> u32 {
        self.owner_local_ordinal
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }
}

/// Cooked matcher-variant key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SemanticVariantKey {
    CatchAll,
    Literal(Box<str>),
}

/// One matcher variant in source order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticVariant {
    id: VariantId,
    matcher: MatcherId,
    order: u32,
    span: Span,
    key_span: Span,
    body_span: Span,
    keys: Box<[SemanticVariantKey]>,
}

impl SemanticVariant {
    #[must_use]
    pub const fn id(&self) -> VariantId {
        self.id
    }

    #[must_use]
    pub const fn matcher(&self) -> MatcherId {
        self.matcher
    }

    #[must_use]
    pub const fn order(&self) -> u32 {
        self.order
    }

    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    #[must_use]
    pub const fn key_span(&self) -> Span {
        self.key_span
    }

    #[must_use]
    pub const fn body_span(&self) -> Span {
        self.body_span
    }

    #[must_use]
    pub fn keys(&self) -> &[SemanticVariantKey] {
        &self.keys
    }
}

/// Complete parser-owned matcher facts used by semantic validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticMatcher {
    id: MatcherId,
    span: Span,
    keyword_span: Span,
    selector_span: Span,
    selectors: Box<[ReferenceId]>,
    variants: Box<[SemanticVariant]>,
}

impl SemanticMatcher {
    #[must_use]
    pub const fn id(&self) -> MatcherId {
        self.id
    }

    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    #[must_use]
    pub const fn keyword_span(&self) -> Span {
        self.keyword_span
    }

    #[must_use]
    pub const fn selector_span(&self) -> Span {
        self.selector_span
    }

    #[must_use]
    pub fn selectors(&self) -> &[ReferenceId] {
        &self.selectors
    }

    #[must_use]
    pub fn selector_count(&self) -> usize {
        self.selectors.len()
    }

    #[must_use]
    pub fn variants(&self) -> &[SemanticVariant] {
        &self.variants
    }
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

/// Parser-owned semantic facts built explicitly from a syntax result.
#[derive(Debug, Default, Clone)]
pub struct SemanticModel {
    mode: MessageMode,
    kind: SemanticMessageKind,
    declarations: Vec<DeclarationRecord>,
    references: Vec<ReferenceRecord>,
    patterns: Vec<PatternRecord>,
    expressions: Vec<ExpressionRecord>,
    markups: Vec<MarkupRecord>,
    literals: Vec<LiteralRecord>,
    functions: Vec<FunctionRecord>,
    options: Vec<OptionRecord>,
    attributes: Vec<AttributeRecord>,
    selectors: Vec<SelectorRecord>,
    variants: Vec<VariantRecord>,
    source: SourceId,
    source_len: u32,
    semantic_declarations: Vec<SemanticDeclaration>,
    semantic_references: Vec<SemanticReference>,
    semantic_options: Vec<SemanticOption>,
    semantic_attributes: Vec<SemanticAttribute>,
    semantic_matchers: Vec<SemanticMatcher>,
    constructed: bool,
}

impl SemanticModel {
    #[must_use]
    pub const fn mode(&self) -> MessageMode {
        self.mode
    }

    #[must_use]
    pub const fn kind(&self) -> SemanticMessageKind {
        self.kind
    }

    #[must_use]
    pub fn declarations(&self) -> &[DeclarationRecord] {
        &self.declarations
    }

    #[must_use]
    pub fn references(&self) -> &[ReferenceRecord] {
        &self.references
    }

    #[must_use]
    pub fn patterns(&self) -> &[PatternRecord] {
        &self.patterns
    }

    #[must_use]
    pub fn expressions(&self) -> &[ExpressionRecord] {
        &self.expressions
    }

    #[must_use]
    pub fn markups(&self) -> &[MarkupRecord] {
        &self.markups
    }

    #[must_use]
    pub fn literals(&self) -> &[LiteralRecord] {
        &self.literals
    }

    #[must_use]
    pub fn functions(&self) -> &[FunctionRecord] {
        &self.functions
    }

    #[must_use]
    pub fn options(&self) -> &[OptionRecord] {
        &self.options
    }

    #[must_use]
    pub fn attributes(&self) -> &[AttributeRecord] {
        &self.attributes
    }

    #[must_use]
    pub fn selectors(&self) -> &[SelectorRecord] {
        &self.selectors
    }

    #[must_use]
    pub fn variants(&self) -> &[VariantRecord] {
        &self.variants
    }

    /// Return the source identity assigned by the paired source store.
    #[must_use]
    pub const fn source(&self) -> SourceId {
        self.source
    }

    pub(crate) const fn source_len(&self) -> u32 {
        self.source_len
    }

    pub(crate) const fn is_constructed(&self) -> bool {
        self.constructed
    }

    pub(crate) fn fact_counts_are_complete(&self) -> bool {
        self.semantic_declarations.len() == self.declarations.len()
            && self.semantic_references.len() == self.references.len()
            && self.semantic_options.len() == self.options.len()
            && self.semantic_attributes.len() == self.attributes.len()
            && self.semantic_matchers.len() == usize::from(self.kind == SemanticMessageKind::Select)
            && self
                .semantic_matchers
                .iter()
                .map(|matcher| matcher.selectors.len())
                .sum::<usize>()
                == self.selectors.len()
            && self
                .semantic_matchers
                .iter()
                .map(|matcher| matcher.variants.len())
                .sum::<usize>()
                == self.variants.len()
    }

    /// Return cooked declarations in source order.
    #[must_use]
    pub fn semantic_declarations(&self) -> &[SemanticDeclaration] {
        &self.semantic_declarations
    }

    /// Return cooked references in source order.
    #[must_use]
    pub fn semantic_references(&self) -> &[SemanticReference] {
        &self.semantic_references
    }

    /// Return option occurrences in owner/source order.
    #[must_use]
    pub fn semantic_options(&self) -> &[SemanticOption] {
        &self.semantic_options
    }

    /// Return attribute occurrences in owner/source order.
    #[must_use]
    pub fn semantic_attributes(&self) -> &[SemanticAttribute] {
        &self.semantic_attributes
    }

    /// Return matcher facts in source order.
    #[must_use]
    pub fn semantic_matchers(&self) -> &[SemanticMatcher] {
        &self.semantic_matchers
    }

    /// Return a reference by its model-local identity.
    #[must_use]
    pub fn reference(&self, id: ReferenceId) -> Option<&SemanticReference> {
        self.semantic_references.get(id.index())
    }

    /// Return a declaration by its model-local identity.
    #[must_use]
    pub fn declaration(&self, id: DeclarationId) -> Option<&SemanticDeclaration> {
        self.semantic_declarations.get(id.index())
    }

    /// Return references reachable directly from the rendered message body.
    pub fn output_references(&self) -> impl Iterator<Item = &SemanticReference> {
        self.semantic_references.iter().filter(|reference| {
            reference.kind != ReferenceKind::Selector && reference.enclosing_declaration.is_none()
        })
    }

    /// Return selector roots and the declaration references they depend on.
    pub fn selection_references(&self) -> impl Iterator<Item = &SemanticReference> {
        let mut included_references = std::collections::BTreeSet::new();
        let mut pending_declarations = Vec::new();
        for selector in self
            .semantic_references
            .iter()
            .filter(|reference| reference.kind == ReferenceKind::Selector)
        {
            included_references.insert(selector.id);
            if let Some(declaration) = selector.resolved_declaration {
                pending_declarations.push(declaration);
            }
        }

        let mut visited_declarations = std::collections::BTreeSet::new();
        while let Some(declaration) = pending_declarations.pop() {
            if !visited_declarations.insert(declaration) {
                continue;
            }
            for reference in self
                .semantic_references
                .iter()
                .filter(|reference| reference.enclosing_declaration == Some(declaration))
            {
                included_references.insert(reference.id);
                if let Some(dependency) = reference.resolved_declaration {
                    pending_declarations.push(dependency);
                }
            }
        }

        self.semantic_references
            .iter()
            .filter(move |reference| included_references.contains(&reference.id))
    }

    pub(crate) fn clear_preserving_capacity(&mut self) {
        self.mode = MessageMode::default();
        self.kind = SemanticMessageKind::default();
        self.source = SourceId::default();
        self.source_len = 0;
        self.declarations.clear();
        self.references.clear();
        self.patterns.clear();
        self.expressions.clear();
        self.markups.clear();
        self.literals.clear();
        self.functions.clear();
        self.options.clear();
        self.attributes.clear();
        self.selectors.clear();
        self.variants.clear();
        self.semantic_declarations.clear();
        self.semantic_references.clear();
        self.semantic_options.clear();
        self.semantic_attributes.clear();
        self.semantic_matchers.clear();
        self.constructed = false;
    }
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

    pub fn semantic_declarations(&self) -> &[SemanticDeclaration] {
        self.model.semantic_declarations()
    }

    pub fn semantic_references(&self) -> &[SemanticReference] {
        self.model.semantic_references()
    }

    pub fn semantic_options(&self) -> &[SemanticOption] {
        self.model.semantic_options()
    }

    pub fn semantic_attributes(&self) -> &[SemanticAttribute] {
        self.model.semantic_attributes()
    }

    pub fn semantic_matchers(&self) -> &[SemanticMatcher] {
        self.model.semantic_matchers()
    }
}

/// Construct source-backed semantic facts from one diagnostic-free parse result.
pub fn build_semantic_model(
    sources: &SourceStore,
    result: &crate::ParseResult,
) -> Result<SemanticModel, crate::SemanticInvariantError> {
    if !result.diagnostics.is_empty() || !source_attachment_is_consistent(sources, result) {
        return Err(crate::SemanticInvariantError::api_misuse());
    }
    let mut model = SemanticModel::default();
    lower_into(sources, result.source, &result.cst, &mut model);
    if model.source != result.source
        || model.source_len != sources.get(result.source).map_or(0, crate::SourceFile::len)
        || !model.fact_counts_are_complete()
    {
        return Err(crate::SemanticInvariantError::invariant_violation());
    }
    Ok(model)
}

fn source_attachment_is_consistent(sources: &SourceStore, result: &crate::ParseResult) -> bool {
    let Some(source) = sources.get(result.source) else {
        return false;
    };
    let source_len = source.len();
    let nodes_are_valid = result
        .cst
        .nodes
        .iter()
        .all(|node| node.span_start <= node.span_end && node.span_end <= source_len);
    let tokens_are_valid = result.cst.tokens.iter().all(|token| {
        token.source_id == result.source.raw()
            && token.span_start <= token.span_end
            && token.span_end <= source_len
    });
    let trivia_are_valid = result.cst.trivia.iter().all(|trivia| {
        trivia.source_id == result.source.raw()
            && trivia.span_start <= trivia.span_end
            && trivia.span_end <= source_len
    });
    result.cst.root_id().is_some() && nodes_are_valid && tokens_are_valid && trivia_are_valid
}

/// Lower a [`CstView`] into a fresh [`SemanticModel`]. Internal convenience
/// wrapper around [`lower_into`] used by checked construction.
#[allow(dead_code)]
pub fn lower(sources: &SourceStore, source_id: SourceId, tables: &CstTables) -> SemanticModel {
    let mut model = SemanticModel::default();
    lower_into(sources, source_id, tables, &mut model);
    model
}

/// Lower a [`CstTables`] into a parser-owned [`SemanticModel`]. The model is
/// cleared first so internal callers may reuse its vector capacities.
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
    reset_model(model);
    model.source = source_id;
    model.source_len = sources.get(source_id).map_or(0, crate::SourceFile::len);
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
    build_semantic_facts(sources, source_id, tables, model);
}

/// Drain every record vector while preserving capacity. Mirrors
/// `ParseWorkspace::clear` so a `lower_into` call into a reused model
/// behaves identically to lowering into a fresh one.
fn reset_model(model: &mut SemanticModel) {
    model.clear_preserving_capacity();
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
fn first_direct_child_span(tables: &CstTables, rec: &CstNodeRecord, kind: SyntaxKind) -> Span {
    first_direct_node_child(tables, rec, kind)
        .map(|(_, r)| span_of(r))
        .unwrap_or_default()
}

#[inline]
fn variable_name_span(tables: &CstTables, variable: &CstNodeRecord) -> Span {
    first_direct_child_span(tables, variable, SyntaxKind::Name)
}

fn lower_message_children(tables: &CstTables, node_rec: &CstNodeRecord, model: &mut SemanticModel) {
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
            // `.input o { $var [:func] }` — both the declared variable and
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
                    name_span: variable_name_span(tables, tables.node_at(v.node)),
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
                name_span: variable_name_span(tables, n_rec),
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

fn build_semantic_facts(
    sources: &SourceStore,
    source: SourceId,
    tables: &CstTables,
    model: &mut SemanticModel,
) {
    model.semantic_declarations = model
        .declarations
        .iter()
        .enumerate()
        .filter_map(|(index, declaration)| {
            let variable = declaration.variable?;
            let variable_record = tables.node_at(variable.node);
            let name_span = variable_name_span(tables, variable_record);
            let name = cook_name(sources.slice_in(source, name_span));
            let function_annotation = model
                .functions
                .iter()
                .filter(|function| {
                    span_contains(declaration.semantic_ref.span, function.semantic_ref.span)
                })
                .min_by_key(|function| {
                    (
                        function.semantic_ref.span.start,
                        function.semantic_ref.span.end,
                    )
                })
                .map(|function| {
                    cook_identifier(sources.slice_in(source, function.identifier_span))
                        .into_boxed_str()
                });
            Some(SemanticDeclaration {
                id: DeclarationId::from_index(index),
                kind: declaration.kind,
                name: name.into_boxed_str(),
                span: declaration.semantic_ref.span,
                name_span,
                function_annotation,
            })
        })
        .collect();

    let mut reference_order: Vec<usize> = (0..model.references.len()).collect();
    reference_order.sort_by_key(|&index| {
        let reference = &model.references[index];
        (
            reference.semantic_ref.span.start,
            reference.semantic_ref.span.end,
            index,
        )
    });
    model.semantic_references = reference_order
        .into_iter()
        .enumerate()
        .map(|(fact_index, raw_index)| {
            let reference = &model.references[raw_index];
            let name = cook_name(sources.slice_in(source, reference.name_span));
            let enclosing_declaration = model
                .semantic_declarations
                .iter()
                .find(|declaration| span_contains(declaration.span, reference.semantic_ref.span))
                .map(SemanticDeclaration::id);
            let resolved_declaration = model
                .semantic_declarations
                .iter()
                .filter(|declaration| {
                    Some(declaration.id) != enclosing_declaration
                        && declaration.name() == name
                        && declaration.name_span.end <= reference.semantic_ref.span.start
                })
                .map(SemanticDeclaration::id)
                .next();
            let kind = reference_kind(model, reference, enclosing_declaration);
            SemanticReference {
                id: ReferenceId::from_index(fact_index),
                name: name.into_boxed_str(),
                kind,
                span: reference.semantic_ref.span,
                resolved_declaration,
                enclosing_declaration,
                is_declaration_dependency: enclosing_declaration.is_some(),
            }
        })
        .collect();

    model.semantic_options = build_option_facts(sources, source, model);
    model.semantic_attributes = build_attribute_facts(sources, source, model);
    model.semantic_matchers = build_matcher_facts(sources, source, tables, model);
    model.constructed = true;
}

fn reference_kind(
    model: &SemanticModel,
    reference: &ReferenceRecord,
    enclosing_declaration: Option<DeclarationId>,
) -> ReferenceKind {
    if model.selectors.iter().any(|selector| {
        selector
            .variable
            .is_some_and(|variable| variable.node == reference.semantic_ref.node)
    }) {
        return ReferenceKind::Selector;
    }
    if let Some(markup) = model
        .markups
        .iter()
        .find(|markup| span_contains(markup.semantic_ref.span, reference.semantic_ref.span))
    {
        let _ = markup;
        return ReferenceKind::MarkupOption;
    }
    if model
        .options
        .iter()
        .any(|option| span_contains(option.semantic_ref.span, reference.semantic_ref.span))
    {
        return ReferenceKind::FunctionOption;
    }
    if enclosing_declaration
        .and_then(|id| model.semantic_declarations.get(id.index()))
        .is_some_and(|declaration| declaration.kind == DeclarationKind::Local)
    {
        ReferenceKind::LocalRhs
    } else {
        ReferenceKind::MessageBody
    }
}

fn build_option_facts(
    sources: &SourceStore,
    source: SourceId,
    model: &SemanticModel,
) -> Vec<SemanticOption> {
    let mut owners: Vec<(OptionOwnerKind, Span)> = model
        .functions
        .iter()
        .map(|function| (OptionOwnerKind::Function, function.semantic_ref.span))
        .chain(
            model
                .markups
                .iter()
                .map(|markup| (OptionOwnerKind::Markup, markup.semantic_ref.span)),
        )
        .collect();
    owners.sort_by_key(|(kind, span)| {
        (
            span.start,
            span.end,
            match kind {
                OptionOwnerKind::Function => 0u8,
                OptionOwnerKind::Markup => 1u8,
            },
        )
    });

    let mut options: Vec<&OptionRecord> = model.options.iter().collect();
    options.sort_by_key(|option| (option.semantic_ref.span.start, option.semantic_ref.span.end));
    let mut local_counts = vec![0u32; owners.len()];
    options
        .into_iter()
        .filter_map(|option| {
            let (owner_index, (owner_kind, _)) = owners
                .iter()
                .enumerate()
                .filter(|(_, (_, span))| span_contains(*span, option.semantic_ref.span))
                .min_by_key(|(_, (_, span))| span.end.saturating_sub(span.start))?;
            let local_ordinal = local_counts[owner_index];
            local_counts[owner_index] = local_ordinal.saturating_add(1);
            Some(SemanticOption {
                owner_kind: *owner_kind,
                owner_ordinal: owner_index as u32,
                owner_local_ordinal: local_ordinal,
                name: cook_identifier(sources.slice_in(source, option.identifier_span))
                    .into_boxed_str(),
                span: option.identifier_span,
            })
        })
        .collect()
}

fn build_attribute_facts(
    sources: &SourceStore,
    source: SourceId,
    model: &SemanticModel,
) -> Vec<SemanticAttribute> {
    let mut owners: Vec<(AttributeOwnerKind, Span)> = model
        .expressions
        .iter()
        .map(|expression| (AttributeOwnerKind::Expression, expression.semantic_ref.span))
        .chain(model.markups.iter().map(|markup| {
            let kind = match markup.kind {
                MarkupKind::Open => AttributeOwnerKind::MarkupOpen,
                MarkupKind::Close => AttributeOwnerKind::MarkupClose,
                MarkupKind::Standalone => AttributeOwnerKind::MarkupStandalone,
            };
            (kind, markup.semantic_ref.span)
        }))
        .collect();
    owners.sort_by_key(|(kind, span)| {
        (
            span.start,
            span.end,
            match kind {
                AttributeOwnerKind::Expression => 0u8,
                AttributeOwnerKind::MarkupOpen => 1u8,
                AttributeOwnerKind::MarkupClose => 2u8,
                AttributeOwnerKind::MarkupStandalone => 3u8,
            },
        )
    });

    let mut attributes: Vec<&AttributeRecord> = model.attributes.iter().collect();
    attributes.sort_by_key(|attribute| {
        (
            attribute.semantic_ref.span.start,
            attribute.semantic_ref.span.end,
        )
    });
    let mut local_counts = vec![0u32; owners.len()];
    attributes
        .into_iter()
        .filter_map(|attribute| {
            let (owner_index, (owner_kind, _)) = owners
                .iter()
                .enumerate()
                .filter(|(_, (_, span))| span_contains(*span, attribute.semantic_ref.span))
                .min_by_key(|(_, (_, span))| span.end.saturating_sub(span.start))?;
            let local_ordinal = local_counts[owner_index];
            local_counts[owner_index] = local_ordinal.saturating_add(1);
            Some(SemanticAttribute {
                owner_kind: *owner_kind,
                owner_ordinal: owner_index as u32,
                owner_local_ordinal: local_ordinal,
                name: cook_identifier(sources.slice_in(source, attribute.identifier_span))
                    .into_boxed_str(),
                span: attribute.identifier_span,
            })
        })
        .collect()
}

fn build_matcher_facts(
    sources: &SourceStore,
    source: SourceId,
    tables: &CstTables,
    model: &SemanticModel,
) -> Vec<SemanticMatcher> {
    let mut matchers: Vec<(NodeId, &CstNodeRecord)> = tables
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, record)| record.kind == SyntaxKind::Matcher as u16)
        .map(|(index, record)| (NodeId::new(index as u32), record))
        .collect();
    matchers.sort_by_key(|(_, record)| (record.span_start, record.span_end));
    let mut next_variant_id = 0usize;
    matchers
        .into_iter()
        .enumerate()
        .map(|(matcher_index, (_, matcher))| {
            let matcher_id = MatcherId::from_index(matcher_index);
            let keyword_span = direct_token_span(tables, matcher, SyntaxKind::MatchKeyword)
                .unwrap_or_else(|| Span::at(matcher.span_start));
            let selector_nodes: Vec<&CstNodeRecord> = iter_node_children(tables, matcher)
                .filter(|(_, record)| record.kind == SyntaxKind::Selector as u16)
                .map(|(_, record)| record)
                .collect();
            let selectors: Vec<ReferenceId> = selector_nodes
                .iter()
                .filter_map(|selector| {
                    let (_, variable) =
                        first_direct_node_child(tables, selector, SyntaxKind::Variable)?;
                    let span = span_of(variable);
                    model
                        .semantic_references
                        .iter()
                        .find(|reference| {
                            reference.kind == ReferenceKind::Selector && reference.span == span
                        })
                        .map(SemanticReference::id)
                })
                .collect();
            let selector_span = selector_nodes
                .first()
                .zip(selector_nodes.last())
                .map_or(keyword_span, |(first, last)| {
                    Span::new(first.span_start, last.span_end)
                });
            let variants = iter_node_children(tables, matcher)
                .filter(|(_, record)| record.kind == SyntaxKind::Variant as u16)
                .enumerate()
                .map(|(variant_order, (_, variant))| {
                    let variant_id = VariantId::from_index(next_variant_id);
                    next_variant_id = next_variant_id.saturating_add(1);
                    build_variant_fact(
                        sources,
                        source,
                        tables,
                        variant,
                        matcher_id,
                        variant_id,
                        variant_order,
                    )
                })
                .collect::<Vec<_>>()
                .into_boxed_slice();
            SemanticMatcher {
                id: matcher_id,
                span: span_of(matcher),
                keyword_span,
                selector_span,
                selectors: selectors.into_boxed_slice(),
                variants,
            }
        })
        .collect()
}

fn build_variant_fact(
    sources: &SourceStore,
    source: SourceId,
    tables: &CstTables,
    variant: &CstNodeRecord,
    matcher: MatcherId,
    id: VariantId,
    order: usize,
) -> SemanticVariant {
    let key_nodes: Vec<&CstNodeRecord> = iter_node_children(tables, variant)
        .filter(|(_, record)| {
            record.kind == SyntaxKind::CatchAllKey as u16
                || record.kind == SyntaxKind::VariantKey as u16
        })
        .map(|(_, record)| record)
        .collect();
    let key_span = key_nodes.first().zip(key_nodes.last()).map_or_else(
        || span_of(variant),
        |(first, last)| Span::new(first.span_start, last.span_end),
    );
    let keys = key_nodes
        .iter()
        .map(|key| {
            if key.kind == SyntaxKind::CatchAllKey as u16 {
                SemanticVariantKey::CatchAll
            } else {
                let literal = iter_node_children(tables, key)
                    .find(|(_, record)| {
                        record.kind == SyntaxKind::QuotedLiteral as u16
                            || record.kind == SyntaxKind::UnquotedLiteral as u16
                    })
                    .map_or_else(|| span_of(key), |(_, record)| span_of(record));
                SemanticVariantKey::Literal(
                    cook_literal(sources.slice_in(source, literal)).into_boxed_str(),
                )
            }
        })
        .collect::<Vec<_>>()
        .into_boxed_slice();
    let body_span = first_direct_node_child(tables, variant, SyntaxKind::QuotedPattern)
        .map_or_else(|| Span::at(variant.span_end), |(_, body)| span_of(body));
    SemanticVariant {
        id,
        matcher,
        order: order as u32,
        span: span_of(variant),
        key_span,
        body_span,
        keys,
    }
}

fn direct_token_span(tables: &CstTables, node: &CstNodeRecord, kind: SyntaxKind) -> Option<Span> {
    tables.edges_for(node).iter().find_map(|edge| {
        if edge.kind != CstEdgeKind::Token as u16 {
            return None;
        }
        let token = tables.token_at(crate::TokenId::new(edge.ref_id));
        (token.kind == kind as u16).then(|| Span::new(token.span_start, token.span_end))
    })
}

const fn span_contains(container: Span, child: Span) -> bool {
    container.start <= child.start && child.end <= container.end
}

fn cook_name(raw: &str) -> String {
    raw.chars()
        .filter(|character| !is_bidi_wrapper(*character))
        .nfc()
        .collect()
}

fn cook_identifier(raw: &str) -> String {
    raw.split(':').map(cook_name).collect::<Vec<_>>().join(":")
}

fn cook_literal(raw: &str) -> String {
    let value = if raw.starts_with('|') && raw.ends_with('|') && raw.len() >= 2 {
        let mut cooked = String::with_capacity(raw.len().saturating_sub(2));
        let mut characters = raw[1..raw.len() - 1].chars();
        while let Some(character) = characters.next() {
            if character == '\\' {
                if let Some(escaped) = characters.next() {
                    cooked.push(escaped);
                }
            } else {
                cooked.push(character);
            }
        }
        cooked
    } else {
        cook_name(raw)
    };
    value.nfc().collect()
}

const fn is_bidi_wrapper(character: char) -> bool {
    matches!(
        character,
        '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{2066}' | '\u{2067}' | '\u{2068}' | '\u{2069}'
    )
}
