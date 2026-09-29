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

/// How many nodes the walk enters between two cancellation probes.
///
/// Identifier references are read without being entered, so they do not
/// count toward the interval.
const PROBE_INTERVAL: u32 = 1024;

/// What a declaration's text is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Source {
    /// MF2, as `intent()` sources and `mf2` tags are.
    Authored,
    /// Displayed text, as a literal assigned to a proven sink is. Its braces
    /// are characters, and it carries the sink's usage.
    Displayed,
}

/// One declaration found in the unit, ready for the shared crate.
#[derive(Debug)]
pub(crate) struct Declared {
    pub(crate) occurrence: Occurrence,
    pub(crate) source: Source,
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
    /// The unit's intrinsic bindings, which later recognition reads to leave
    /// explicit forms to this recognizer.
    pub(crate) intrinsics: IntrinsicSymbols,
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
    let mut found = recognizer.found;
    found.intrinsics = intrinsics;
    Ok(found)
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
            source: Source::Authored,
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
            tag.span,
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
                let cooked = settle(
                    cook_string(self.text, literal, self.limits),
                    literal.span,
                    self.reporter,
                )?;
                self.inline(literal.span, cooked, occurrence, supplied)
            }
            Expression::TemplateLiteral(template) if template.expressions.is_empty() => {
                let Some(element) = template.quasis.first() else {
                    return Ok(());
                };
                let cooked = settle(
                    cook_template(self.text, element, self.limits),
                    template.span,
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
            Expression::StringLiteral(literal) => settle(
                cook_string(self.text, literal, self.limits),
                literal.span,
                self.reporter,
            )?,
            Expression::TemplateLiteral(template) if template.expressions.is_empty() => {
                let Some(element) = template.quasis.first() else {
                    return Ok(());
                };
                settle(
                    cook_template(self.text, element, self.limits),
                    template.span,
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
        if overlapping(&self.found.declarations) {
            return Err(ProducerFailure::DeclarationOverlap {
                unit: self.reporter.source().unit().clone(),
            });
        }
        Ok(())
    }
}

/// Return whether two declarations claim overlapping source.
///
/// Declarations that only touch, one ending where the next begins, do not
/// overlap.
pub(crate) fn overlapping(declarations: &[Declared]) -> bool {
    let mut ranges: Vec<_> = declarations
        .iter()
        .map(|declared| declared.occurrence.range())
        .collect();
    ranges.sort_unstable();
    ranges
        .windows(2)
        .any(|pair| pair[1].start() < pair[0].end())
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

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::fmt::Write;

    use intlify_authoring::{AuthoringFailure, ByteRange, LimitKind, Location};
    use oxc_allocator::Allocator;

    use super::*;
    use crate::grammar::Grammar;
    use crate::limits::tests::generous;
    use crate::test_support::{at, bindings, nth, parse, reporter, snapshot, PRELUDE, UNIT};

    type Range = (u64, u64);

    /// Each record a unit made, as its detail and range.
    type Records = Vec<(&'static str, Range)>;

    /// Everything one unit's recognition handed over, reduced to what a
    /// fixture asserts: each declaration's role, range, decoded text and the
    /// names its own use site supplied; each use's range, the declaration it
    /// uses, whether that is shared, and the names it supplied; each
    /// exclusion's range and reason; and each record's detail and range.
    #[derive(Debug, PartialEq)]
    struct Seen {
        declarations: Vec<(OccurrenceRole, Range, String, Option<Vec<String>>)>,
        uses: Vec<(Range, usize, bool, Vec<String>)>,
        exclusions: Vec<(Range, String)>,
        records: Records,
    }

    fn range(occurrence: &Occurrence) -> Range {
        let range = occurrence.range();
        (range.start(), range.end())
    }

    fn names(parameters: &[ParameterBinding]) -> Vec<String> {
        parameters
            .iter()
            .map(|binding| binding.name().to_owned())
            .collect()
    }

    /// Recognize the explicit forms in `text`, returning what was handed
    /// over as it is and the records the unit made.
    fn run<C>(
        grammar: Grammar,
        text: &str,
        limits: &JsAuthoringLimits,
        cancelled: &C,
    ) -> Result<(Recognized, Records), ProducerFailure>
    where
        C: Fn() -> bool + ?Sized,
    {
        let arena = Allocator::default();
        let parsed = parse(&arena, text, grammar);
        let mut reporter = reporter(text);
        let recognized = recognize(&parsed, text, &bindings(), limits, &mut reporter, cancelled)?;
        let records = reporter
            .into_diagnostics()
            .iter()
            .map(|record| {
                let Location::Region(region) = record.location() else {
                    panic!("a host record points into the unit");
                };
                (
                    record.detail().expect("a detail").as_str(),
                    (region.range().start(), region.range().end()),
                )
            })
            .collect();
        Ok((recognized, records))
    }

    fn seen_as(grammar: Grammar, text: &str) -> Seen {
        let (recognized, records) =
            run(grammar, text, &generous(), &|| false).expect("recognition runs");
        Seen {
            declarations: recognized
                .declarations
                .iter()
                .map(|declared| {
                    (
                        declared.occurrence.role(),
                        range(&declared.occurrence),
                        declared.text.clone(),
                        declared.parameters.as_deref().map(names),
                    )
                })
                .collect(),
            uses: recognized
                .uses
                .iter()
                .map(|used| {
                    assert_eq!(used.occurrence.role(), OccurrenceRole::Reference);
                    (
                        range(&used.occurrence),
                        used.declaration,
                        used.shared,
                        names(&used.parameters),
                    )
                })
                .collect(),
            exclusions: recognized
                .exclusions
                .iter()
                .map(|exclusion| {
                    assert_eq!(exclusion.occurrence().role(), OccurrenceRole::Exclusion);
                    (range(exclusion.occurrence()), exclusion.reason().to_owned())
                })
                .collect(),
            records,
        }
    }

    fn seen(body: &str) -> (String, Seen) {
        let text = format!("{PRELUDE}{body}");
        let seen = seen_as(Grammar::JsModule, &text);
        (text, seen)
    }

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn a_literal_source_is_decoded_and_used_where_it_is_written() {
        let (text, seen) = seen("intent('Pay\\x20now')\n");
        assert_eq!(
            seen,
            Seen {
                declarations: vec![(
                    OccurrenceRole::IntentLiteral,
                    at(&text, "'Pay\\x20now'"),
                    "Pay now".to_owned(),
                    Some(vec![]),
                )],
                uses: vec![(at(&text, "intent('Pay\\x20now')"), 0, false, vec![])],
                exclusions: vec![],
                records: vec![],
            }
        );
        // The input map leads each run of decoded text back to its source:
        // the text either side of the escape by position, and the escape as
        // a whole.
        let (recognized, _) = run(Grammar::JsModule, &text, &generous(), &|| false).unwrap();
        let map: Vec<(Range, Range)> = recognized.declarations[0]
            .input_map
            .iter()
            .map(|segment| {
                let (input, source) = (segment.input(), segment.source());
                ((input.start(), input.end()), (source.start(), source.end()))
            })
            .collect();
        assert_eq!(
            map,
            [
                ((0, 3), at(&text, "Pay")),
                ((3, 4), at(&text, "\\x20")),
                ((4, 7), at(&text, "now")),
            ]
        );
    }

    #[test]
    fn a_nested_tag_is_declared_once_with_its_use_sites_parameters() {
        let (text, seen) = seen("render(intent(mf2`Hello {$name}!`, { name }))\n");
        let tag = at(&text, "mf2`Hello {$name}!`");
        assert_eq!(
            seen,
            Seen {
                declarations: vec![(
                    OccurrenceRole::Mf2Declaration,
                    tag,
                    "Hello {$name}!".to_owned(),
                    Some(strings(&["name"])),
                )],
                uses: vec![(
                    at(&text, "intent(mf2`Hello {$name}!`, { name })"),
                    0,
                    false,
                    strings(&["name"])
                )],
                exclusions: vec![],
                // The tag's reference was read as part of the call, so it is
                // not reported as a use of `mf2` somewhere else.
                records: vec![],
            }
        );
    }

    #[test]
    fn a_shared_declaration_is_resolved_wherever_it_is_written() {
        let (text, seen) = seen(
            "export function render(name) {\n  return intent(later, { name })\n}\n\
             const later = mf2`Hello {$name}!`\nintent(later)\n",
        );
        assert_eq!(
            seen,
            Seen {
                // A shared declaration has no use site of its own; each use
                // is compared on its own.
                declarations: vec![(
                    OccurrenceRole::Mf2Declaration,
                    at(&text, "mf2`Hello {$name}!`"),
                    "Hello {$name}!".to_owned(),
                    None,
                )],
                uses: vec![
                    (
                        at(&text, "intent(later, { name })"),
                        0,
                        true,
                        strings(&["name"])
                    ),
                    (at(&text, "intent(later)"), 0, true, vec![]),
                ],
                exclusions: vec![],
                records: vec![],
            }
        );
    }

    #[test]
    fn a_tag_nothing_uses_is_still_a_declaration() {
        let (text, seen) = seen("export const unused = mf2`Not shown yet`\n");
        assert_eq!(
            seen.declarations,
            [(
                OccurrenceRole::Mf2Declaration,
                at(&text, "mf2`Not shown yet`"),
                "Not shown yet".to_owned(),
                None,
            )]
        );
        assert_eq!(seen.uses, []);
    }

    #[test]
    fn an_unreadable_parameter_object_keeps_the_declaration_without_a_use() {
        let (text, seen) =
            seen("intent('Pay now', params)\nconst greeting = mf2`Hi`\nintent(greeting, params)\n");
        assert_eq!(
            seen,
            Seen {
                declarations: vec![
                    (
                        OccurrenceRole::IntentLiteral,
                        at(&text, "'Pay now'"),
                        "Pay now".to_owned(),
                        None,
                    ),
                    (
                        OccurrenceRole::Mf2Declaration,
                        at(&text, "mf2`Hi`"),
                        "Hi".to_owned(),
                        None,
                    ),
                ],
                uses: vec![],
                exclusions: vec![],
                records: vec![
                    ("parameters-opaque", nth(&text, "params", 0)),
                    ("parameters-opaque", nth(&text, "params", 1)),
                ],
            }
        );
    }

    #[test]
    fn an_exclusion_keeps_its_decoded_reason() {
        let (text, seen) =
            seen("noIntent(brand, 'Product\\x20name')\nnoIntent(comment, `User content`)\n");
        assert_eq!(
            seen,
            Seen {
                declarations: vec![],
                uses: vec![],
                exclusions: vec![
                    (
                        at(&text, "noIntent(brand, 'Product\\x20name')"),
                        "Product name".to_owned()
                    ),
                    (
                        at(&text, "noIntent(comment, `User content`)"),
                        "User content".to_owned()
                    ),
                ],
                records: vec![],
            }
        );
    }

    #[test]
    fn a_contradiction_is_reported_once_at_the_outer_call_and_neither_side_is_read() {
        let (text, seen) = seen(
            "intent(noIntent(value, 'Brand'))\nnoIntent(mf2`Pay`, 'Brand')\n\
             noIntent(intent('Pay'), 'Brand')\n",
        );
        let nested = "explicit-forms-nested";
        assert_eq!(
            seen,
            Seen {
                declarations: vec![],
                uses: vec![],
                exclusions: vec![],
                // The inner form was claimed by the outer one: it is neither
                // read again nor reported as a use of an intrinsic.
                records: vec![
                    (nested, at(&text, "intent(noIntent(value, 'Brand'))")),
                    (nested, at(&text, "noIntent(mf2`Pay`, 'Brand')")),
                    (nested, at(&text, "noIntent(intent('Pay'), 'Brand')")),
                ],
            }
        );
    }

    #[test]
    fn an_identifier_is_resolved_by_what_its_binding_names() {
        let (text, seen) = seen(
            "import { shared } from './messages'\n\
             const tagged = mf2`Tagged ${x}`\n\
             const greeting = mf2`Hi`\n\
             const alias = greeting\n\
             const plain = 'Pay now'\n\
             let changing = greeting\n\
             const first = second\n\
             const second = first\n\
             intent(tagged)\n\
             intent(alias)\n\
             intent(shared)\n\
             intent(plain)\n\
             intent(changing)\n\
             intent(first)\n\
             intent(intent)\n\
             intent(undeclared)\n",
        );
        let (itself, _) = at(&text, "intent(intent)");
        assert_eq!(
            seen,
            Seen {
                declarations: vec![(
                    OccurrenceRole::Mf2Declaration,
                    at(&text, "mf2`Hi`"),
                    "Hi".to_owned(),
                    None,
                )],
                uses: vec![],
                exclusions: vec![],
                records: vec![
                    // The tag reported why it is no declaration, so the use
                    // of its binding adds nothing.
                    ("template-substitution", at(&text, "mf2`Tagged ${x}`")),
                    ("declaration-alias", nth(&text, "alias", 1)),
                    ("module-reference", nth(&text, "shared", 1)),
                    // A `const` holding anything but a tag is a value.
                    ("message-dynamic", nth(&text, "plain", 1)),
                    ("message-dynamic", nth(&text, "changing", 1)),
                    // An alias cycle ends; it names no declaration.
                    ("message-dynamic", nth(&text, "first", 2)),
                    // The intrinsic is reported where it is used, once.
                    ("intrinsic-use-unsupported", (itself + 7, itself + 13)),
                    ("message-dynamic", at(&text, "undeclared")),
                ],
            }
        );
    }

    #[test]
    fn a_type_position_names_an_intrinsic_without_using_it() {
        let text = format!(
            "{PRELUDE}const kind = typeof intent\ntype Kind = typeof mf2\nintent<string>('Pay now')\n"
        );
        let seen = seen_as(Grammar::TsModule, &text);
        let (use_of, _) = at(&text, "typeof intent");
        // A value read of the intrinsic is reported; a type query is not, and
        // type arguments leave a direct call direct.
        assert_eq!(
            seen,
            Seen {
                declarations: vec![(
                    OccurrenceRole::IntentLiteral,
                    at(&text, "'Pay now'"),
                    "Pay now".to_owned(),
                    Some(vec![]),
                )],
                uses: vec![(at(&text, "intent<string>('Pay now')"), 0, false, vec![])],
                exclusions: vec![],
                records: vec![("intrinsic-use-unsupported", (use_of + 7, use_of + 13))],
            }
        );
    }

    /// Recognize `text` with a probe that stops at call `stop_at`, returning
    /// whether recognition ran and how many times the probe was asked.
    fn probed(text: &str, stop_at: Option<u32>) -> (Result<(), ProducerFailure>, u32) {
        let asked = Cell::new(0);
        let probe = || {
            asked.set(asked.get() + 1);
            stop_at.is_some_and(|stop| asked.get() >= stop)
        };
        let result = run(Grammar::JsModule, text, &generous(), &probe).map(|_| ());
        (result, asked.get())
    }

    #[test]
    fn the_walk_asks_the_probe_once_per_interval_and_never_after_a_stop() {
        // Too few nodes for the walk to ask at all.
        let small = format!("{PRELUDE}intent('Pay now')\n");
        assert_eq!(probed(&small, Some(1)), (Ok(()), 0));

        // The interval counts the nodes the walk enters. Identifier
        // references are read without being entered, so each call below
        // enters three nodes: its statement, the call, and the literal.
        let mut body = String::new();
        for index in 0..800 {
            writeln!(body, "intent('Message {index}')").expect("writing to a string");
        }
        let large = format!("{PRELUDE}{body}");
        let (finished, asked) = probed(&large, None);
        assert_eq!(finished, Ok(()));
        assert_eq!(asked, 2, "2,400 entered nodes span two intervals");
        for stop_at in 1..=asked {
            assert_eq!(
                probed(&large, Some(stop_at)),
                (Err(ProducerFailure::Cancelled), stop_at),
                "stopping at {stop_at}"
            );
        }
    }

    #[test]
    fn what_one_unit_hands_over_is_bounded_exactly() {
        let run_with = |body: &str, limits: &JsAuthoringLimits| {
            run(
                Grammar::JsModule,
                &format!("{PRELUDE}{body}"),
                limits,
                &|| false,
            )
            .map(|_| ())
        };
        let declarations = "intent('One')\nintent('Two')\n";
        let mut limits = generous();
        limits.authoring.declarations = 2;
        assert_eq!(run_with(declarations, &limits), Ok(()));
        limits.authoring.declarations = 1;
        assert_eq!(
            run_with(declarations, &limits),
            Err(ProducerFailure::Authoring(AuthoringFailure::Limit(
                LimitKind::Declarations
            )))
        );

        // Uses of a shared declaration count when they are resolved.
        let uses = "const shared = mf2`Shared`\nintent(shared)\nintent(shared)\n";
        let mut limits = generous();
        limits.references = 2;
        assert_eq!(run_with(uses, &limits), Ok(()));
        limits.references = 1;
        assert_eq!(
            run_with(uses, &limits),
            Err(ProducerFailure::Limit(JsLimitKind::References))
        );

        let exclusions = "noIntent(a, 'One')\nnoIntent(b, 'Two')\n";
        let mut limits = generous();
        limits.exclusions = 2;
        assert_eq!(run_with(exclusions, &limits), Ok(()));
        limits.exclusions = 1;
        assert_eq!(
            run_with(exclusions, &limits),
            Err(ProducerFailure::Limit(JsLimitKind::Exclusions))
        );
    }

    fn declared(start: u64, end: u64) -> Declared {
        Declared {
            occurrence: Occurrence::new(
                snapshot(UNIT, Grammar::JsModule, b"0123456789"),
                ByteRange::new(start, end).unwrap(),
                OccurrenceRole::IntentLiteral,
            )
            .unwrap(),
            source: Source::Authored,
            text: String::new(),
            input_map: Vec::new(),
            parameters: None,
        }
    }

    #[test]
    fn declarations_overlap_when_they_share_a_byte_in_any_order() {
        assert!(!overlapping(&[]));
        assert!(!overlapping(&[declared(0, 4)]));
        // Touching is not overlapping.
        assert!(!overlapping(&[declared(0, 4), declared(4, 9)]));
        assert!(!overlapping(&[declared(4, 9), declared(0, 4)]));
        assert!(overlapping(&[declared(0, 9), declared(2, 4)]));
        assert!(overlapping(&[declared(2, 4), declared(2, 4)]));
        assert!(overlapping(&[declared(5, 9), declared(0, 6)]));
        assert!(overlapping(&[
            declared(7, 9),
            declared(0, 2),
            declared(1, 3)
        ]));
    }
}
