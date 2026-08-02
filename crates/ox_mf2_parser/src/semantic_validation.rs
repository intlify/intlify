// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Parser-owned `MessageFormat` data-model validation.
//!
//! This module validates one immutable [`crate::SemanticModel`], emits the
//! closed semantic diagnostic catalog in deterministic report order, and keeps
//! API misuse or impossible model states separate from ordinary diagnostics.

use std::collections::BTreeSet;

use crate::{
    DeclarationId, DeclarationKind, DiagnosticLabel, DiagnosticSeverity, ReferenceKind,
    SemanticMatcher, SemanticModel, SemanticReference, SemanticVariantKey, SourceId, Span,
};

const MAX_DIAGNOSTIC_LABELS: usize = 32;

/// Closed classification for semantic construction and validation failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SemanticInvariantErrorKind {
    ApiMisuse,
    InvariantViolation,
}

/// Operational semantic failure with parser-owned classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticInvariantError {
    kind: SemanticInvariantErrorKind,
}

impl SemanticInvariantError {
    #[must_use]
    pub const fn kind(&self) -> SemanticInvariantErrorKind {
        self.kind
    }

    pub(crate) const fn api_misuse() -> Self {
        Self {
            kind: SemanticInvariantErrorKind::ApiMisuse,
        }
    }

    pub(crate) const fn invariant_violation() -> Self {
        Self {
            kind: SemanticInvariantErrorKind::InvariantViolation,
        }
    }
}

impl core::fmt::Display for SemanticInvariantError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self.kind {
            SemanticInvariantErrorKind::ApiMisuse => "semantic API input is inconsistent",
            SemanticInvariantErrorKind::InvariantViolation => {
                "semantic model violates parser-owned invariants"
            }
        })
    }
}

impl std::error::Error for SemanticInvariantError {}

/// Stable semantic diagnostic catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SemanticDiagnosticCode {
    DuplicateDeclaration,
    InvalidDeclarationDependency,
    MissingSelectorAnnotation,
    VariantKeyArityMismatch,
    MissingFallbackVariant,
    DuplicateVariant,
    DuplicateOptionName,
}

impl SemanticDiagnosticCode {
    /// Complete catalog in declaration order.
    pub const fn all() -> &'static [Self] {
        &[
            Self::DuplicateDeclaration,
            Self::InvalidDeclarationDependency,
            Self::MissingSelectorAnnotation,
            Self::VariantKeyArityMismatch,
            Self::MissingFallbackVariant,
            Self::DuplicateVariant,
            Self::DuplicateOptionName,
        ]
    }

    /// Stable kebab-case code used by structured tooling output.
    pub const fn json_code(self) -> &'static str {
        match self {
            Self::DuplicateDeclaration => "duplicate-declaration",
            Self::InvalidDeclarationDependency => "invalid-declaration-dependency",
            Self::MissingSelectorAnnotation => "missing-selector-annotation",
            Self::VariantKeyArityMismatch => "variant-key-arity-mismatch",
            Self::MissingFallbackVariant => "missing-fallback-variant",
            Self::DuplicateVariant => "duplicate-variant",
            Self::DuplicateOptionName => "duplicate-option-name",
        }
    }

    #[must_use]
    pub const fn severity(self) -> DiagnosticSeverity {
        DiagnosticSeverity::Error
    }

    #[must_use]
    pub const fn static_message(self) -> &'static str {
        match self {
            Self::DuplicateDeclaration => "variable is declared more than once",
            Self::InvalidDeclarationDependency => "declaration dependency is invalid",
            Self::MissingSelectorAnnotation => "selector has no function annotation",
            Self::VariantKeyArityMismatch => "variant key count does not match selector count",
            Self::MissingFallbackVariant => "matcher has no fallback variant",
            Self::DuplicateVariant => "variant key tuple is duplicated",
            Self::DuplicateOptionName => "option name is duplicated within its owner",
        }
    }
}

/// One parser-owned semantic diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticDiagnostic {
    source: SourceId,
    code: SemanticDiagnosticCode,
    severity: DiagnosticSeverity,
    message: &'static str,
    span: Span,
    labels: Vec<DiagnosticLabel>,
}

impl SemanticDiagnostic {
    #[must_use]
    pub const fn source(&self) -> SourceId {
        self.source
    }

    #[must_use]
    pub const fn code(&self) -> SemanticDiagnosticCode {
        self.code
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
    pub const fn span(&self) -> Span {
        self.span
    }

    #[must_use]
    pub fn labels(&self) -> &[DiagnosticLabel] {
        &self.labels
    }
}

struct OrderedDiagnostic {
    diagnostic: SemanticDiagnostic,
    occurrence: u64,
}

/// Validate one parser-owned semantic model and return every independent error.
pub fn validate_semantics(
    model: &SemanticModel,
) -> Result<Vec<SemanticDiagnostic>, SemanticInvariantError> {
    validate_model_contract(model)?;
    let source = model.source();
    let mut diagnostics = Vec::new();
    let mut occurrence = 0u64;
    let invalid_declarations =
        validate_declarations(model, source, &mut diagnostics, &mut occurrence);
    validate_selectors(
        model,
        source,
        &invalid_declarations,
        &mut diagnostics,
        &mut occurrence,
    );
    validate_matchers(model, source, &mut diagnostics, &mut occurrence);
    validate_options(model, source, &mut diagnostics, &mut occurrence);
    diagnostics.sort_by(|left, right| {
        (
            left.diagnostic.span.start,
            left.diagnostic.span.end,
            left.diagnostic.code.json_code(),
            left.occurrence,
        )
            .cmp(&(
                right.diagnostic.span.start,
                right.diagnostic.span.end,
                right.diagnostic.code.json_code(),
                right.occurrence,
            ))
    });
    Ok(diagnostics
        .into_iter()
        .map(|ordered| ordered.diagnostic)
        .collect())
}

fn validate_model_contract(model: &SemanticModel) -> Result<(), SemanticInvariantError> {
    if !model.is_constructed() || !model.fact_counts_are_complete() {
        return Err(SemanticInvariantError::invariant_violation());
    }
    let source_len = model.source_len();
    let declarations = model.semantic_declarations();
    if declarations.iter().enumerate().any(|(index, declaration)| {
        declaration.id().raw() != index as u32
            || declaration.name().is_empty()
            || !span_is_valid(declaration.span(), source_len)
            || !span_is_valid(declaration.name_span(), source_len)
            || !span_contains(declaration.span(), declaration.name_span())
    }) || declarations
        .windows(2)
        .any(|pair| pair[0].span().start > pair[1].span().start)
    {
        return Err(SemanticInvariantError::invariant_violation());
    }

    let references = model.semantic_references();
    if references.iter().enumerate().any(|(index, reference)| {
        reference.id().raw() != index as u32
            || reference.name().is_empty()
            || !span_is_valid(reference.span(), source_len)
            || reference
                .resolved_declaration()
                .is_some_and(|id| id.raw() as usize >= declarations.len())
            || reference
                .enclosing_declaration()
                .is_some_and(|id| id.raw() as usize >= declarations.len())
    }) || references
        .windows(2)
        .any(|pair| pair[0].span().start > pair[1].span().start)
    {
        return Err(SemanticInvariantError::invariant_violation());
    }

    let options = model.semantic_options();
    if options
        .iter()
        .any(|option| option.name().is_empty() || !span_is_valid(option.span(), source_len))
        || options.windows(2).any(|pair| {
            pair[0].span().start > pair[1].span().start
                || (pair[0].owner_ordinal() == pair[1].owner_ordinal()
                    && pair[0].owner_local_ordinal() >= pair[1].owner_local_ordinal())
        })
    {
        return Err(SemanticInvariantError::invariant_violation());
    }

    let attributes = model.semantic_attributes();
    if attributes.iter().any(|attribute| {
        attribute.name().is_empty() || !span_is_valid(attribute.span(), source_len)
    }) || attributes.windows(2).any(|pair| {
        pair[0].span().start > pair[1].span().start
            || (pair[0].owner_ordinal() == pair[1].owner_ordinal()
                && pair[0].owner_local_ordinal() >= pair[1].owner_local_ordinal())
    }) {
        return Err(SemanticInvariantError::invariant_violation());
    }

    let mut expected_variant_id = 0u32;
    for (matcher_index, matcher) in model.semantic_matchers().iter().enumerate() {
        if matcher.id().raw() != matcher_index as u32
            || !span_is_valid(matcher.span(), source_len)
            || !span_is_valid(matcher.keyword_span(), source_len)
            || !span_is_valid(matcher.selector_span(), source_len)
            || matcher.selectors().iter().any(|id| {
                model
                    .reference(*id)
                    .is_none_or(|reference| reference.kind() != ReferenceKind::Selector)
            })
            || matcher
                .variants()
                .iter()
                .enumerate()
                .any(|(variant_index, variant)| {
                    let invalid = variant.id().raw() != expected_variant_id
                        || variant.matcher() != matcher.id()
                        || variant.order() != variant_index as u32
                        || !span_is_valid(variant.span(), source_len)
                        || !span_is_valid(variant.key_span(), source_len)
                        || !span_is_valid(variant.body_span(), source_len)
                        || !span_contains(variant.span(), variant.key_span())
                        || !span_contains(variant.span(), variant.body_span());
                    expected_variant_id = expected_variant_id.saturating_add(1);
                    invalid
                })
        {
            return Err(SemanticInvariantError::invariant_violation());
        }
    }
    Ok(())
}

fn validate_declarations(
    model: &SemanticModel,
    source: SourceId,
    diagnostics: &mut Vec<OrderedDiagnostic>,
    occurrence: &mut u64,
) -> BTreeSet<DeclarationId> {
    let declarations = model.semantic_declarations();
    let references = model.semantic_references();
    let mut invalid = BTreeSet::new();
    for (index, declaration) in declarations.iter().enumerate() {
        let dependency_sites: Vec<&SemanticReference> = references
            .iter()
            .filter(|reference| {
                reference.is_declaration_dependency()
                    && reference.name() == declaration.name()
                    && reference
                        .enclosing_declaration()
                        .is_none_or(|owner| owner == declaration.id() || !invalid.contains(&owner))
                    && (reference.enclosing_declaration() == Some(declaration.id())
                        || reference.span().start < declaration.name_span().start)
            })
            .collect();
        if !dependency_sites.is_empty() {
            invalid.insert(declaration.id());
            let labels = dependency_sites
                .into_iter()
                .take(MAX_DIAGNOSTIC_LABELS)
                .map(|reference| DiagnosticLabel {
                    source,
                    span: reference.span(),
                    message: "dependency reference appears here",
                })
                .collect();
            push_diagnostic(
                diagnostics,
                occurrence,
                source,
                SemanticDiagnosticCode::InvalidDeclarationDependency,
                declaration.name_span(),
                labels,
            );
            continue;
        }
        if let Some(first) = declarations[..index]
            .iter()
            .find(|earlier| earlier.name() == declaration.name())
        {
            push_diagnostic(
                diagnostics,
                occurrence,
                source,
                SemanticDiagnosticCode::DuplicateDeclaration,
                declaration.name_span(),
                vec![DiagnosticLabel {
                    source,
                    span: first.name_span(),
                    message: "first declaration appears here",
                }],
            );
        }
    }
    invalid
}

fn validate_selectors(
    model: &SemanticModel,
    source: SourceId,
    invalid_declarations: &BTreeSet<DeclarationId>,
    diagnostics: &mut Vec<OrderedDiagnostic>,
    occurrence: &mut u64,
) {
    for matcher in model.semantic_matchers() {
        for selector_id in matcher.selectors() {
            let Some(selector) = model.reference(*selector_id) else {
                continue;
            };
            let state =
                selector
                    .resolved_declaration()
                    .map_or(AnnotationState::Missing, |declaration| {
                        declaration_annotation_state(
                            model,
                            declaration,
                            invalid_declarations,
                            &mut BTreeSet::new(),
                        )
                    });
            if state == AnnotationState::Missing {
                let labels = selector
                    .resolved_declaration()
                    .and_then(|id| model.declaration(id))
                    .map(|declaration| {
                        vec![DiagnosticLabel {
                            source,
                            span: declaration.name_span(),
                            message: "selector declaration has no annotation",
                        }]
                    })
                    .unwrap_or_default();
                push_diagnostic(
                    diagnostics,
                    occurrence,
                    source,
                    SemanticDiagnosticCode::MissingSelectorAnnotation,
                    selector.span(),
                    labels,
                );
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AnnotationState {
    Annotated,
    Missing,
    Broken,
}

fn declaration_annotation_state(
    model: &SemanticModel,
    declaration_id: DeclarationId,
    invalid_declarations: &BTreeSet<DeclarationId>,
    visiting: &mut BTreeSet<DeclarationId>,
) -> AnnotationState {
    if invalid_declarations.contains(&declaration_id) || !visiting.insert(declaration_id) {
        return AnnotationState::Broken;
    }
    let Some(declaration) = model.declaration(declaration_id) else {
        return AnnotationState::Broken;
    };
    if declaration.function_annotation().is_some() {
        visiting.remove(&declaration_id);
        return AnnotationState::Annotated;
    }
    if declaration.kind() == DeclarationKind::Input {
        visiting.remove(&declaration_id);
        return AnnotationState::Missing;
    }

    let mut state = AnnotationState::Missing;
    for reference in model
        .semantic_references()
        .iter()
        .filter(|reference| reference.enclosing_declaration() == Some(declaration_id))
    {
        let dependency_state = if let Some(resolved) = reference.resolved_declaration() {
            declaration_annotation_state(model, resolved, invalid_declarations, visiting)
        } else if model.semantic_declarations().iter().any(|candidate| {
            candidate.name() == reference.name() && invalid_declarations.contains(&candidate.id())
        }) {
            AnnotationState::Broken
        } else {
            AnnotationState::Missing
        };
        match dependency_state {
            AnnotationState::Annotated => {
                state = AnnotationState::Annotated;
                break;
            }
            AnnotationState::Broken => state = AnnotationState::Broken,
            AnnotationState::Missing => {}
        }
    }
    visiting.remove(&declaration_id);
    state
}

fn validate_matchers(
    model: &SemanticModel,
    source: SourceId,
    diagnostics: &mut Vec<OrderedDiagnostic>,
    occurrence: &mut u64,
) {
    for matcher in model.semantic_matchers() {
        validate_matcher(model, source, matcher, diagnostics, occurrence);
    }
}

fn validate_matcher(
    _model: &SemanticModel,
    source: SourceId,
    matcher: &SemanticMatcher,
    diagnostics: &mut Vec<OrderedDiagnostic>,
    occurrence: &mut u64,
) {
    let selector_count = matcher.selectors().len();
    let mut valid_variants = Vec::new();
    for variant in matcher.variants() {
        if variant.keys().len() == selector_count {
            valid_variants.push(variant);
        } else {
            push_diagnostic(
                diagnostics,
                occurrence,
                source,
                SemanticDiagnosticCode::VariantKeyArityMismatch,
                variant.key_span(),
                vec![DiagnosticLabel {
                    source,
                    span: matcher.selector_span(),
                    message: "selector list defines the expected arity",
                }],
            );
        }
    }

    if !valid_variants.iter().any(|variant| {
        !variant.keys().is_empty()
            && variant
                .keys()
                .iter()
                .all(|key| matches!(key, SemanticVariantKey::CatchAll))
    }) {
        push_diagnostic(
            diagnostics,
            occurrence,
            source,
            SemanticDiagnosticCode::MissingFallbackVariant,
            matcher.keyword_span(),
            Vec::new(),
        );
    }

    for (index, variant) in valid_variants.iter().enumerate() {
        if let Some(first) = valid_variants[..index]
            .iter()
            .find(|earlier| earlier.keys() == variant.keys())
        {
            push_diagnostic(
                diagnostics,
                occurrence,
                source,
                SemanticDiagnosticCode::DuplicateVariant,
                variant.key_span(),
                vec![DiagnosticLabel {
                    source,
                    span: first.key_span(),
                    message: "first equivalent variant appears here",
                }],
            );
        }
    }
}

fn validate_options(
    model: &SemanticModel,
    source: SourceId,
    diagnostics: &mut Vec<OrderedDiagnostic>,
    occurrence: &mut u64,
) {
    let options = model.semantic_options();
    for (index, option) in options.iter().enumerate() {
        if let Some(first) = options[..index].iter().find(|earlier| {
            earlier.owner_kind() == option.owner_kind()
                && earlier.owner_ordinal() == option.owner_ordinal()
                && earlier.name() == option.name()
        }) {
            push_diagnostic(
                diagnostics,
                occurrence,
                source,
                SemanticDiagnosticCode::DuplicateOptionName,
                option.span(),
                vec![DiagnosticLabel {
                    source,
                    span: first.span(),
                    message: "first option appears here",
                }],
            );
        }
    }
}

fn push_diagnostic(
    diagnostics: &mut Vec<OrderedDiagnostic>,
    occurrence: &mut u64,
    source: SourceId,
    code: SemanticDiagnosticCode,
    span: Span,
    labels: Vec<DiagnosticLabel>,
) {
    diagnostics.push(OrderedDiagnostic {
        diagnostic: SemanticDiagnostic {
            source,
            code,
            severity: code.severity(),
            message: code.static_message(),
            span,
            labels,
        },
        occurrence: *occurrence,
    });
    *occurrence = (*occurrence).saturating_add(1);
}

const fn span_is_valid(span: Span, source_len: u32) -> bool {
    span.start <= span.end && span.end <= source_len
}

const fn span_contains(container: Span, child: Span) -> bool {
    container.start <= child.start && child.end <= container.end
}

#[cfg(test)]
mod tests {
    use super::{SemanticDiagnosticCode, SemanticInvariantErrorKind};

    #[test]
    fn diagnostic_catalog_is_complete_and_collision_free() {
        let codes = SemanticDiagnosticCode::all();
        assert_eq!(codes.len(), 7);
        for (index, code) in codes.iter().enumerate() {
            assert!(!code.json_code().is_empty());
            assert!(!codes[..index]
                .iter()
                .any(|earlier| earlier.json_code() == code.json_code()));
        }
    }

    #[test]
    fn invariant_error_kinds_are_closed_and_distinct() {
        assert_ne!(
            SemanticInvariantErrorKind::ApiMisuse,
            SemanticInvariantErrorKind::InvariantViolation
        );
    }
}
