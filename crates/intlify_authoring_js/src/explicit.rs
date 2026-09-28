// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Recognizing `intent`, `mf2` and `noIntent` in one unit.
//!
//! The recognizer walks the tree once. A call or a tagged template is an
//! authoring form only when its callee or tag is an identifier the semantic
//! pass resolved to an intrinsic binding; nothing is recognized by spelling.
//! Every other reference to an intrinsic binding is reported, because a known
//! intrinsic used in a way this profile does not read must not pass for an
//! ordinary value.
//!
//! What comes out is still host evidence: decoded message text with its input
//! map, occurrences, and parameter positions. Whether a message is well
//! formed, what it requires, and whether a use site supplies it are decided
//! afterwards by `intlify_authoring`.
//!
//! The first argument of `intent()` is classified by its syntax, seen through
//! transparent wrappers:
//!
//! | Argument | Result |
//! | --- | --- |
//! | a string, or a template without substitutions | an `intent-literal` declaration used here |
//! | an `mf2` tagged template | an `mf2-declaration` used here |
//! | a `const` binding initialized with an `mf2` tag | a use of that shared declaration |
//! | an alias of such a binding | unsupported (`declaration-alias`) |
//! | an imported binding | unsupported (`module-reference`) |
//! | a conditional or logical expression | unsupported (`conditional-selection`) |
//! | `noIntent(...)` | unsupported (`explicit-forms-nested`) |
//! | a template with substitutions | dynamic (`template-substitution`) |
//! | anything else | dynamic (`message-dynamic`) |
//!
//! A use names a shared declaration only through the `const` binding whose
//! initializer is the tag itself. Nothing is inferred about which message a
//! value might hold at run time.

use std::collections::{BTreeMap, BTreeSet};

use intlify_authoring::{
    Exclusion, InputSegment, Occurrence, OccurrenceRole, ParameterBinding, ReasonFamily,
};
use oxc_ast::ast::{
    BindingPattern, CallExpression, Expression, IdentifierReference, ImportExpression,
    TaggedTemplateExpression, VariableDeclarationKind, VariableDeclarator,
};
use oxc_ast::AstKind;
use oxc_ast_visit::{walk, Visit};
use oxc_semantic::{ReferenceId, Scoping, SymbolFlags, SymbolId};
use oxc_span::{GetSpan, Span};

use crate::binding::{self, Bindings, Intrinsic, IntrinsicSymbols};
use crate::cooked::{cook_string, cook_template, settle, Cooked};
use crate::detail;
use crate::failure::ProducerFailure;
use crate::limits::{JsAuthoringLimits, JsLimitKind};
use crate::parameters;
use crate::parse::Parsed;
use crate::report::Reporter;
use crate::syntax::{static_string, transparent};

/// How many nodes are visited between two cancellation probes.
const PROBE_INTERVAL: u32 = 1024;

/// One declaration found in the unit, ready for the shared crate.
#[derive(Debug)]
pub(crate) struct Declared {
    pub(crate) occurrence: Occurrence,
    pub(crate) text: String,
    pub(crate) input_map: Vec<InputSegment>,
    /// What the declaration's own use site supplied, when it is used where it
    /// is declared and that use site could be read.
    pub(crate) parameters: Option<Vec<ParameterBinding>>,
}

/// One use site found in the unit.
#[derive(Debug)]
pub(crate) struct Used {
    pub(crate) occurrence: Occurrence,
    /// Index of the declaration it uses.
    pub(crate) declaration: usize,
    /// Whether the declaration is shared, so this use site's parameters are
    /// compared on their own rather than as the declaration's.
    pub(crate) shared: bool,
    pub(crate) parameters: Vec<ParameterBinding>,
}

/// Everything recognized in one unit.
#[derive(Debug, Default)]
pub(crate) struct Recognized {
    pub(crate) declarations: Vec<Declared>,
    pub(crate) uses: Vec<Used>,
    pub(crate) exclusions: Vec<Exclusion>,
}

/// Recognize the explicit authoring forms of one accepted unit.
pub(crate) fn recognize<C>(
    parsed: &Parsed<'_>,
    text: &str,
    bindings: &Bindings,
    limits: &JsAuthoringLimits,
    reporter: &mut Reporter,
    cancelled: &C,
) -> Result<Recognized, ProducerFailure>
where
    C: Fn() -> bool + ?Sized,
{
    let intrinsics = binding::scan(parsed.program, bindings, reporter)?;
    let mut recognizer = Recognizer {
        text,
        scoping: parsed.semantic.scoping(),
        intrinsics: &intrinsics,
        bindings,
        limits,
        reporter,
        cancelled,
        visited: 0,
        failure: None,
        allowed: BTreeSet::new(),
        claimed: BTreeSet::new(),
        constants: BTreeMap::new(),
        tags: BTreeMap::new(),
        pending: Vec::new(),
        found: Recognized::default(),
    };
    recognizer.visit_program(parsed.program);
    if let Some(failure) = recognizer.failure {
        return Err(failure);
    }
    recognizer.resolve()?;
    recognizer.check_overlap()?;
    Ok(recognizer.found)
}

/// What a `const` binding is initialized with, when that matters here.
#[derive(Debug, Clone, Copy)]
enum Initializer {
    /// An `mf2` tagged template, by its span.
    Mf2((u32, u32)),
    /// Another binding, which makes this one an alias.
    Alias(SymbolId),
}

/// A use of an identifier, resolved once the whole unit has been read.
///
/// The identifier may name a declaration written later in the unit, so it
/// cannot be resolved where it is met.
#[derive(Debug)]
struct Pending {
    span: Span,
    reference: ReferenceId,
    occurrence: Occurrence,
    parameters: Option<Vec<ParameterBinding>>,
}

/// What an identifier handed to `intent()` turned out to name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Named {
    /// A declaration, by index.
    Declaration(usize),
    /// A declaration that could not be established; it reported why.
    Failed,
    /// An intrinsic, which its own reference already reported.
    Intrinsic,
    Alias,
    Module,
    Dynamic,
}

struct Recognizer<'r, C: ?Sized> {
    text: &'r str,
    scoping: &'r Scoping,
    intrinsics: &'r IntrinsicSymbols,
    bindings: &'r Bindings,
    limits: &'r JsAuthoringLimits,
    reporter: &'r mut Reporter,
    cancelled: &'r C,
    visited: u32,
    /// The first operational failure. Once set, nothing more is recognized.
    failure: Option<ProducerFailure>,
    /// References to intrinsics in a position this profile reads.
    allowed: BTreeSet<ReferenceId>,
    /// Calls and tags already read as part of an enclosing form.
    claimed: BTreeSet<(u32, u32)>,
    constants: BTreeMap<SymbolId, Initializer>,
    /// `mf2` declarations by the span of their tag.
    tags: BTreeMap<(u32, u32), usize>,
    pending: Vec<Pending>,
    found: Recognized,
}

const fn key(span: Span) -> (u32, u32) {
    (span.start, span.end)
}

impl<C> Recognizer<'_, C>
where
    C: Fn() -> bool + ?Sized,
{
    fn live(&self) -> bool {
        self.failure.is_none()
    }

    fn record(&mut self, result: Result<(), ProducerFailure>) {
        if let Err(failure) = result {
            self.failure.get_or_insert(failure);
        }
    }

    fn report(
        &mut self,
        family: ReasonFamily,
        detail: intlify_authoring::Detail,
        span: Span,
    ) -> Result<(), ProducerFailure> {
        self.reporter.at(family, detail, span)
    }

    /// Return the intrinsic an expression names directly, and the reference
    /// that names it.
    ///
    /// Only a bare identifier counts. A parenthesized, asserted, or member
    /// expression is a wrapper, and a wrapper is not read.
    fn intrinsic(&self, expression: &Expression<'_>) -> Option<(Intrinsic, ReferenceId)> {
        let Expression::Identifier(identifier) = expression else {
            return None;
        };
        let reference = identifier.reference_id.get()?;
        let symbol = self.scoping.get_reference(reference).symbol_id()?;
        Some((self.intrinsics.get(symbol)?, reference))
    }

    /// Return the intrinsic a call is a direct call of.
    fn called(&self, call: &CallExpression<'_>) -> Option<(Intrinsic, ReferenceId)> {
        if call.optional {
            return None;
        }
        self.intrinsic(&call.callee)
    }

    fn claim(&mut self, span: Span, reference: ReferenceId) {
        self.claimed.insert(key(span));
        self.allowed.insert(reference);
    }

    fn declare(
        &mut self,
        span: Span,
        role: OccurrenceRole,
        cooked: Cooked,
        parameters: Option<Vec<ParameterBinding>>,
    ) -> Result<usize, ProducerFailure> {
        if self.found.declarations.len() as u64 >= self.limits.authoring.declarations {
            return Err(ProducerFailure::Authoring(
                intlify_authoring::AuthoringFailure::Limit(
                    intlify_authoring::LimitKind::Declarations,
                ),
            ));
        }
        let occurrence = self.reporter.occurrence(span, role)?;
        self.found.declarations.push(Declared {
            occurrence,
            text: cooked.text,
            input_map: cooked.input_map,
            parameters,
        });
        Ok(self.found.declarations.len() - 1)
    }

    fn use_declaration(
        &mut self,
        occurrence: Occurrence,
        declaration: usize,
        shared: bool,
        parameters: Vec<ParameterBinding>,
    ) -> Result<(), ProducerFailure> {
        if self.found.uses.len() as u64 >= self.limits.references {
            return Err(ProducerFailure::Limit(JsLimitKind::References));
        }
        self.found.uses.push(Used {
            occurrence,
            declaration,
            shared,
            parameters,
        });
        Ok(())
    }

    /// Declare one `mf2` tagged template.
    fn mf2(
        &mut self,
        tag: &TaggedTemplateExpression<'_>,
        parameters: Option<Vec<ParameterBinding>>,
    ) -> Result<Option<usize>, ProducerFailure> {
        // An authoring tag holds no substitutions: runtime values belong in
        // MF2 variables and the parameter object.
        if !tag.quasi.expressions.is_empty() {
            self.report(
                ReasonFamily::AuthoringSourceDynamic,
                detail::template_substitution(),
                tag.span,
            )?;
            return Ok(None);
        }
        let Some(element) = tag.quasi.quasis.first() else {
            return Ok(None);
        };
        let Some(cooked) = settle(
            cook_template(self.text, element, self.limits),
            self.reporter,
        )?
        else {
            return Ok(None);
        };
        let index = self.declare(tag.span, OccurrenceRole::Mf2Declaration, cooked, parameters)?;
        self.tags.insert(key(tag.span), index);
        Ok(Some(index))
    }

    /// Read one direct `intent()` call.
    fn intent(&mut self, call: &CallExpression<'_>) -> Result<(), ProducerFailure> {
        let unsupported = ReasonFamily::AuthoringFormUnsupported;
        if let Some(spread) = call.arguments.iter().find(|argument| argument.is_spread()) {
            return self.report(unsupported, detail::intent_arguments(), spread.span());
        }
        let (source, parameters) = match call.arguments.as_slice() {
            [] => return self.report(unsupported, detail::intent_arguments(), call.span),
            [source] => (source, None),
            [source, parameters] => (source, Some(parameters)),
            // There is no third argument: metadata is not an argument (016-002).
            [_, _, extra, ..] => {
                return self.report(unsupported, detail::intent_arguments(), extra.span())
            }
        };
        let Some(source) = source.as_expression() else {
            return Ok(());
        };
        let supplied = match parameters {
            None => Some(Vec::new()),
            Some(argument) => parameters::read(argument, self.text, self.limits, self.reporter)?,
        };
        let occurrence = self
            .reporter
            .occurrence(call.span, OccurrenceRole::Reference)?;

        match transparent(source) {
            Expression::StringLiteral(literal) => {
                let cooked = settle(cook_string(self.text, literal, self.limits), self.reporter)?;
                self.inline(literal.span, cooked, occurrence, supplied)
            }
            Expression::TemplateLiteral(template) if template.expressions.is_empty() => {
                let Some(element) = template.quasis.first() else {
                    return Ok(());
                };
                let cooked = settle(
                    cook_template(self.text, element, self.limits),
                    self.reporter,
                )?;
                self.inline(template.span, cooked, occurrence, supplied)
            }
            Expression::TemplateLiteral(_) => self.report(
                ReasonFamily::AuthoringSourceDynamic,
                detail::template_substitution(),
                source.span(),
            ),
            Expression::TaggedTemplateExpression(tag) => match self.intrinsic(&tag.tag) {
                Some((Intrinsic::Mf2, reference)) => {
                    self.claim(tag.span, reference);
                    let Some(index) = self.mf2(tag, supplied.clone())? else {
                        return Ok(());
                    };
                    match supplied {
                        Some(parameters) => {
                            self.use_declaration(occurrence, index, false, parameters)
                        }
                        None => Ok(()),
                    }
                }
                _ => self.report(
                    ReasonFamily::AuthoringSourceDynamic,
                    detail::message_dynamic(),
                    source.span(),
                ),
            },
            Expression::Identifier(identifier) => {
                if let Some(reference) = identifier.reference_id.get() {
                    self.pending.push(Pending {
                        span: identifier.span,
                        reference,
                        occurrence,
                        parameters: supplied,
                    });
                }
                Ok(())
            }
            Expression::ConditionalExpression(_) | Expression::LogicalExpression(_) => {
                self.report(unsupported, detail::conditional_selection(), source.span())
            }
            Expression::CallExpression(inner) => match self.called(inner) {
                Some((Intrinsic::NoIntent, reference)) => {
                    self.claim(inner.span, reference);
                    self.report(unsupported, detail::explicit_forms_nested(), call.span)
                }
                _ => self.report(
                    ReasonFamily::AuthoringSourceDynamic,
                    detail::message_dynamic(),
                    source.span(),
                ),
            },
            _ => self.report(
                ReasonFamily::AuthoringSourceDynamic,
                detail::message_dynamic(),
                source.span(),
            ),
        }
    }

    /// Declare a literal `intent()` source and its use at the call.
    fn inline(
        &mut self,
        span: Span,
        cooked: Option<Cooked>,
        occurrence: Occurrence,
        supplied: Option<Vec<ParameterBinding>>,
    ) -> Result<(), ProducerFailure> {
        let Some(cooked) = cooked else {
            return Ok(());
        };
        let index = self.declare(
            span,
            OccurrenceRole::IntentLiteral,
            cooked,
            supplied.clone(),
        )?;
        match supplied {
            Some(parameters) => self.use_declaration(occurrence, index, false, parameters),
            None => Ok(()),
        }
    }

    /// Read one direct `noIntent()` call.
    fn exclusion(&mut self, call: &CallExpression<'_>) -> Result<(), ProducerFailure> {
        let unsupported = ReasonFamily::AuthoringFormUnsupported;
        if let Some(spread) = call.arguments.iter().find(|argument| argument.is_spread()) {
            return self.report(unsupported, detail::exclusion_arguments(), spread.span());
        }
        let (value, reason) = match call.arguments.as_slice() {
            [] | [_] => {
                return self.report(unsupported, detail::exclusion_reason_missing(), call.span)
            }
            [value, reason] => (value, reason),
            [_, _, extra, ..] => {
                return self.report(unsupported, detail::exclusion_arguments(), extra.span())
            }
        };
        let (Some(value), Some(reason)) = (value.as_expression(), reason.as_expression()) else {
            return Ok(());
        };

        // A value that is itself explicitly localized cannot also be excluded.
        // The order the two are written in does not decide between them.
        match transparent(value) {
            Expression::CallExpression(inner) => {
                if let Some((Intrinsic::Intent, reference)) = self.called(inner) {
                    self.claim(inner.span, reference);
                    return self.report(unsupported, detail::explicit_forms_nested(), call.span);
                }
            }
            Expression::TaggedTemplateExpression(tag) => {
                if let Some((Intrinsic::Mf2, reference)) = self.intrinsic(&tag.tag) {
                    self.claim(tag.span, reference);
                    return self.report(unsupported, detail::explicit_forms_nested(), call.span);
                }
            }
            _ => {}
        }

        let cooked = match transparent(reason) {
            Expression::StringLiteral(literal) => {
                settle(cook_string(self.text, literal, self.limits), self.reporter)?
            }
            Expression::TemplateLiteral(template) if template.expressions.is_empty() => {
                let Some(element) = template.quasis.first() else {
                    return Ok(());
                };
                settle(
                    cook_template(self.text, element, self.limits),
                    self.reporter,
                )?
            }
            _ => {
                return self.report(
                    unsupported,
                    detail::exclusion_reason_dynamic(),
                    reason.span(),
                )
            }
        };
        let Some(cooked) = cooked else {
            return Ok(());
        };
        if cooked.text.is_empty() {
            return self.report(unsupported, detail::exclusion_reason_empty(), reason.span());
        }
        if self.found.exclusions.len() as u64 >= self.limits.exclusions {
            return Err(ProducerFailure::Limit(JsLimitKind::Exclusions));
        }
        let occurrence = self
            .reporter
            .occurrence(call.span, OccurrenceRole::Exclusion)?;
        let exclusion = Exclusion::new(occurrence, &cooked.text).map_err(|_| {
            ProducerFailure::ParserInvariant {
                unit: self.reporter.source().unit().clone(),
            }
        })?;
        self.found.exclusions.push(exclusion);
        Ok(())
    }

    /// Resolve every identifier handed to `intent()`.
    fn resolve(&mut self) -> Result<(), ProducerFailure> {
        for pending in std::mem::take(&mut self.pending) {
            let symbol = self.scoping.get_reference(pending.reference).symbol_id();
            match self.named(symbol) {
                Named::Declaration(index) => {
                    if let Some(parameters) = pending.parameters {
                        self.use_declaration(pending.occurrence, index, true, parameters)?;
                    }
                }
                Named::Failed | Named::Intrinsic => {}
                Named::Alias => self.report(
                    ReasonFamily::AuthoringFormUnsupported,
                    detail::declaration_alias(),
                    pending.span,
                )?,
                Named::Module => self.report(
                    ReasonFamily::AuthoringFormUnsupported,
                    detail::module_reference(),
                    pending.span,
                )?,
                Named::Dynamic => self.report(
                    ReasonFamily::AuthoringSourceDynamic,
                    detail::message_dynamic(),
                    pending.span,
                )?,
            }
        }
        Ok(())
    }

    /// Return what one identifier's binding names.
    fn named(&self, symbol: Option<SymbolId>) -> Named {
        // An unresolved identifier is a global, whose value is not known here.
        let Some(mut symbol) = symbol else {
            return Named::Dynamic;
        };
        let mut aliased = false;
        // Each step follows one `const` alias, and an alias chain cannot be
        // longer than the number of constants, even written as a cycle.
        for _ in 0..=self.constants.len() {
            match self.constants.get(&symbol) {
                Some(Initializer::Mf2(span)) => {
                    return match (aliased, self.tags.get(span)) {
                        (true, _) => Named::Alias,
                        (false, Some(&index)) => Named::Declaration(index),
                        (false, None) => Named::Failed,
                    };
                }
                Some(Initializer::Alias(target)) => {
                    aliased = true;
                    symbol = *target;
                }
                None => break,
            }
        }
        if self.intrinsics.get(symbol).is_some() {
            Named::Intrinsic
        } else if self
            .scoping
            .symbol_flags(symbol)
            .contains(SymbolFlags::Import)
        {
            Named::Module
        } else {
            Named::Dynamic
        }
    }

    /// Check that no two declarations claim overlapping source.
    fn check_overlap(&self) -> Result<(), ProducerFailure> {
        let mut ranges: Vec<_> = self
            .found
            .declarations
            .iter()
            .map(|declared| declared.occurrence.range())
            .collect();
        ranges.sort_unstable();
        if ranges
            .windows(2)
            .any(|pair| pair[1].start() < pair[0].end())
        {
            return Err(ProducerFailure::DeclarationOverlap {
                unit: self.reporter.source().unit().clone(),
            });
        }
        Ok(())
    }
}

impl<'a, C> Visit<'a> for Recognizer<'_, C>
where
    C: Fn() -> bool + ?Sized,
{
    fn enter_node(&mut self, _kind: AstKind<'a>) {
        self.visited = self.visited.wrapping_add(1);
        if self.visited.is_multiple_of(PROBE_INTERVAL) && self.live() && (self.cancelled)() {
            self.failure = Some(ProducerFailure::Cancelled);
        }
    }

    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        if !self.live() {
            return;
        }
        match self.called(call) {
            Some((intrinsic @ (Intrinsic::Intent | Intrinsic::NoIntent), reference)) => {
                self.allowed.insert(reference);
                if !self.claimed.contains(&key(call.span)) {
                    let result = if intrinsic == Intrinsic::Intent {
                        self.intent(call)
                    } else {
                        self.exclusion(call)
                    };
                    self.record(result);
                }
            }
            // `mf2` is a tag, not a function; calling it is reported where it
            // is referenced.
            Some((Intrinsic::Mf2, _)) | None => {}
        }
        walk::walk_call_expression(self, call);
    }

    fn visit_tagged_template_expression(&mut self, tag: &TaggedTemplateExpression<'a>) {
        if !self.live() {
            return;
        }
        if let Some((Intrinsic::Mf2, reference)) = self.intrinsic(&tag.tag) {
            self.allowed.insert(reference);
            if !self.claimed.contains(&key(tag.span)) {
                // A tag not used where it is written is still a declaration;
                // it keeps its identity until a later change retires it.
                let result = self.mf2(tag, None).map(|_| ());
                self.record(result);
            }
        }
        walk::walk_tagged_template_expression(self, tag);
    }

    fn visit_variable_declarator(&mut self, declarator: &VariableDeclarator<'a>) {
        if !self.live() {
            return;
        }
        if declarator.kind == VariableDeclarationKind::Const {
            if let (BindingPattern::BindingIdentifier(binding), Some(init)) =
                (&declarator.id, &declarator.init)
            {
                if let Some(symbol) = binding.symbol_id.get() {
                    let initializer = match transparent(init) {
                        Expression::TaggedTemplateExpression(tag)
                            if matches!(self.intrinsic(&tag.tag), Some((Intrinsic::Mf2, _))) =>
                        {
                            Some(Initializer::Mf2(key(tag.span)))
                        }
                        Expression::Identifier(identifier) => identifier
                            .reference_id
                            .get()
                            .and_then(|reference| self.scoping.get_reference(reference).symbol_id())
                            .map(Initializer::Alias),
                        _ => None,
                    };
                    if let Some(initializer) = initializer {
                        self.constants.insert(symbol, initializer);
                    }
                }
            }
        }
        walk::walk_variable_declarator(self, declarator);
    }

    fn visit_identifier_reference(&mut self, identifier: &IdentifierReference<'a>) {
        if !self.live() {
            return;
        }
        let Some(reference) = identifier.reference_id.get() else {
            return;
        };
        let resolved = self.scoping.get_reference(reference);
        // A type position names the binding without using its value.
        if !resolved.is_value() || self.allowed.contains(&reference) {
            return;
        }
        if resolved
            .symbol_id()
            .is_some_and(|symbol| self.intrinsics.get(symbol).is_some())
        {
            let result = self.report(
                ReasonFamily::AuthoringFormUnsupported,
                detail::intrinsic_use_unsupported(),
                identifier.span,
            );
            self.record(result);
        }
    }

    fn visit_import_expression(&mut self, import: &ImportExpression<'a>) {
        if !self.live() {
            return;
        }
        if static_string(&import.source).is_some_and(|module| self.bindings.registers(module)) {
            let result = self.report(
                ReasonFamily::AuthoringFormUnsupported,
                detail::import_form_unsupported(),
                import.span,
            );
            self.record(result);
        }
        walk::walk_import_expression(self, import);
    }
}
