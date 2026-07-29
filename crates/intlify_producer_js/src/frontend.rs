// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Complete one-source parser and reference-extraction frontend.
//!
//! This module owns snapshot admission, bounded source-goal selection, recovered
//! AST rejection, exact call recognition, selector conversion, and canonical
//! source ordering. It does not own project discovery, physical-source groups,
//! artifact identity, caching, worker scheduling, or linker policy.

use intlify_contract::{
    MessageReference, MessageSelector, ReasonText, SourceDocumentIdentity, SourceOrigin,
    SourceUtf8Span,
};
use oxc_allocator::Allocator;
use oxc_ast::ast::{Argument, CallExpression, Expression};
use oxc_ast_visit::{walk, Visit};
use oxc_parser::Parser;
use oxc_span::{GetSpan, Span};

use crate::error::{JsProducerError, JsProducerFailure, JsProducerFailureReason};
use crate::key::convert_static_selector;
use crate::profile::{JsSelectedSourceGoal, JsSourceGoal, JsSourceProfile};
use crate::recognizer::{JsRecognizerBinding, JsRecognizerCallKind, JsRecognizerSet};
use crate::static_eval::{evaluate_static_string, StaticString};

/// Fixed inclusive byte ceiling for one selected source snapshot.
pub const SOURCE_BYTES_LIMIT: u64 = 64 * 1024 * 1024;

const DYNAMIC_LOOKUP_REASON: &str = "lookup argument is not statically known";
const BOUNDED_SET_REASON: &str = "bounded set declared by configured recognizer";

/// Complete deterministic output of one admitted source snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsSourceScan {
    profile: JsSourceProfile,
    selected_goal: JsSelectedSourceGoal,
    references: Box<[MessageReference]>,
}

impl JsSourceScan {
    /// Return the suffix-selected grammar and source-goal policy.
    #[must_use]
    pub const fn profile(&self) -> JsSourceProfile {
        self.profile
    }

    /// Return the source goal that produced the admitted complete AST.
    #[must_use]
    pub const fn selected_goal(&self) -> JsSelectedSourceGoal {
        self.selected_goal
    }

    /// Return checked references in canonical source-span order.
    #[must_use]
    pub const fn references(&self) -> &[MessageReference] {
        &self.references
    }
}

/// Scan one exact source snapshot without caller cancellation.
pub fn scan_source(
    source: &SourceDocumentIdentity,
    source_bytes: &[u8],
    recognizers: &JsRecognizerSet,
) -> Result<JsSourceScan, JsProducerError> {
    scan_source_with_cancellation(source, source_bytes, recognizers, &|| false)
}

/// Scan one exact source snapshot with a caller-owned cancellation probe.
///
/// Cancellation is a control-flow result rather than stable producer evidence.
/// The probe may be called at parser-attempt and AST-walk boundaries and must
/// therefore be cheap, thread-safe under the caller's chosen execution model,
/// and free of assumptions about the exact call count.
pub fn scan_source_with_cancellation<C>(
    source: &SourceDocumentIdentity,
    source_bytes: &[u8],
    recognizers: &JsRecognizerSet,
    cancelled: &C,
) -> Result<JsSourceScan, JsProducerError>
where
    C: Fn() -> bool + ?Sized,
{
    let profile = JsSourceProfile::from_source(source).map_err(|_| {
        JsProducerFailure::without_optional(
            JsProducerFailureReason::UnsupportedSourceSuffix,
            source.clone(),
        )
    })?;
    admit_source_bytes(source, source_bytes.len() as u64)?;
    let source_text = std::str::from_utf8(source_bytes).map_err(|_| {
        JsProducerFailure::without_optional(JsProducerFailureReason::InvalidUtf8, source.clone())
    })?;

    match profile.goal() {
        JsSourceGoal::Module => finish_fixed_attempt(
            source,
            parse_and_scan(
                source,
                source_text,
                recognizers,
                profile,
                JsSelectedSourceGoal::Module,
                cancelled,
            )?,
        ),
        JsSourceGoal::CommonJs => finish_fixed_attempt(
            source,
            parse_and_scan(
                source,
                source_text,
                recognizers,
                profile,
                JsSelectedSourceGoal::Script,
                cancelled,
            )?,
        ),
        JsSourceGoal::BoundedUnambiguous => {
            let module = parse_and_scan(
                source,
                source_text,
                recognizers,
                profile,
                JsSelectedSourceGoal::Module,
                cancelled,
            )?;
            match module {
                ParseAttempt::Admitted(scan) => Ok(scan),
                ParseAttempt::SyntaxRejected(module_span) => {
                    let script = parse_and_scan(
                        source,
                        source_text,
                        recognizers,
                        profile,
                        JsSelectedSourceGoal::Script,
                        cancelled,
                    )?;
                    match script {
                        ParseAttempt::Admitted(scan) => Ok(scan),
                        ParseAttempt::SyntaxRejected(script_span) => {
                            Err(JsProducerFailure::with_span(
                                JsProducerFailureReason::SyntaxInvalid,
                                source.clone(),
                                earlier_span(module_span, script_span),
                            )
                            .into())
                        }
                    }
                }
            }
        }
    }
}

fn admit_source_bytes(
    source: &SourceDocumentIdentity,
    observed: u64,
) -> Result<(), JsProducerError> {
    if observed > SOURCE_BYTES_LIMIT {
        return Err(JsProducerFailure::with_limit(
            JsProducerFailureReason::SourceBytesLimit,
            source.clone(),
            SOURCE_BYTES_LIMIT,
            SOURCE_BYTES_LIMIT + 1,
        )
        .into());
    }
    Ok(())
}

enum ParseAttempt {
    Admitted(JsSourceScan),
    SyntaxRejected(Option<SourceUtf8Span>),
}

fn finish_fixed_attempt(
    source: &SourceDocumentIdentity,
    attempt: ParseAttempt,
) -> Result<JsSourceScan, JsProducerError> {
    match attempt {
        ParseAttempt::Admitted(scan) => Ok(scan),
        ParseAttempt::SyntaxRejected(span) => Err(JsProducerFailure::with_span(
            JsProducerFailureReason::SyntaxInvalid,
            source.clone(),
            span,
        )
        .into()),
    }
}

fn parse_and_scan<C>(
    source: &SourceDocumentIdentity,
    source_text: &str,
    recognizers: &JsRecognizerSet,
    profile: JsSourceProfile,
    selected_goal: JsSelectedSourceGoal,
    cancelled: &C,
) -> Result<ParseAttempt, JsProducerError>
where
    C: Fn() -> bool + ?Sized,
{
    if cancelled() {
        return Err(JsProducerError::Cancelled);
    }

    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source_text, profile.source_type(selected_goal)).parse();
    if cancelled() {
        return Err(JsProducerError::Cancelled);
    }
    if parsed.panicked && parsed.errors.is_empty() {
        return Err(JsProducerError::InternalInvariant);
    }
    if !parsed.errors.is_empty() {
        let span = parsed
            .errors
            .iter()
            .flat_map(|diagnostic| diagnostic.labels.iter().flatten())
            .filter_map(|label| safe_diagnostic_span(source_text, label.offset(), label.len()))
            .min();
        // OXC diagnostics and recovery state are dependency-owned. The frontend
        // rejects the entire AST and intentionally retains neither diagnostic
        // text nor recovery identifiers in its output contract. Only the
        // smallest independently checked byte range crosses this boundary.
        return Ok(ParseAttempt::SyntaxRejected(span));
    }

    let mut scanner = ReferenceScanner::new(source, source_text, recognizers, cancelled);
    scanner.visit_program(&parsed.program);
    if scanner.cancelled {
        return Err(JsProducerError::Cancelled);
    }
    if scanner.invariant_failed {
        return Err(JsProducerError::InternalInvariant);
    }
    if let Some(failure) = scanner
        .failures
        .into_iter()
        .min_by(JsProducerFailure::cmp_within_source)
    {
        return Err(failure.into());
    }

    scanner.references.sort_by(compare_references_by_origin);
    if scanner.references.windows(2).any(|pair| {
        pair[0].origin().map(SourceOrigin::span) == pair[1].origin().map(SourceOrigin::span)
    }) {
        return Err(JsProducerError::InternalInvariant);
    }

    Ok(ParseAttempt::Admitted(JsSourceScan {
        profile,
        selected_goal,
        references: scanner.references.into_boxed_slice(),
    }))
}

struct ReferenceScanner<'source, 'config, 'cancel, C: ?Sized> {
    source: &'source SourceDocumentIdentity,
    source_text: &'source str,
    recognizers: &'config JsRecognizerSet,
    cancelled_probe: &'cancel C,
    references: Vec<MessageReference>,
    failures: Vec<JsProducerFailure>,
    cancelled: bool,
    invariant_failed: bool,
}

impl<'source, 'config, 'cancel, C> ReferenceScanner<'source, 'config, 'cancel, C>
where
    C: Fn() -> bool + ?Sized,
{
    fn new(
        source: &'source SourceDocumentIdentity,
        source_text: &'source str,
        recognizers: &'config JsRecognizerSet,
        cancelled_probe: &'cancel C,
    ) -> Self {
        Self {
            source,
            source_text,
            recognizers,
            cancelled_probe,
            references: Vec::new(),
            failures: Vec::new(),
            cancelled: false,
            invariant_failed: false,
        }
    }

    fn process_call(&mut self, call: &CallExpression<'_>, binding: &JsRecognizerBinding) {
        let Some(argument) = call.arguments.first() else {
            self.fail(
                JsProducerFailureReason::SelectorArgumentMissing,
                safe_span(self.source_text, call.span),
            );
            return;
        };
        if let Argument::SpreadElement(spread) = argument {
            self.fail(
                JsProducerFailureReason::SelectorArgumentSpread,
                safe_span(self.source_text, spread.span),
            );
            return;
        }
        let Some(expression) = argument.as_expression() else {
            self.invariant_failed = true;
            return;
        };
        let Some(span) = safe_span(self.source_text, expression.span()) else {
            self.invariant_failed = true;
            return;
        };

        let (selector, reason) = match evaluate_static_string(expression) {
            StaticString::Known(value) => {
                let Ok(selector) = convert_static_selector(binding, value) else {
                    self.fail(selector_invalid_reason(binding.kind()), Some(span));
                    return;
                };
                let reason = match binding.kind() {
                    JsRecognizerCallKind::Lookup => None,
                    JsRecognizerCallKind::Set => Some(reason_text(BOUNDED_SET_REASON)),
                };
                (selector, reason)
            }
            StaticString::KnownInvalid => {
                self.fail(selector_invalid_reason(binding.kind()), Some(span));
                return;
            }
            StaticString::Dynamic => match binding.kind() {
                JsRecognizerCallKind::Lookup => (
                    MessageSelector::UnboundedDynamic,
                    Some(reason_text(DYNAMIC_LOOKUP_REASON)),
                ),
                JsRecognizerCallKind::Set => {
                    self.fail(JsProducerFailureReason::SetSelectorDynamic, Some(span));
                    return;
                }
            },
        };
        self.push_reference(binding, selector, reason, span);
    }

    fn push_reference(
        &mut self,
        binding: &JsRecognizerBinding,
        selector: MessageSelector,
        reason: Option<ReasonText>,
        span: SourceUtf8Span,
    ) {
        let origin = SourceOrigin::new(self.source.clone(), span);
        match MessageReference::try_new(
            binding.scope().clone(),
            binding.domain(),
            selector,
            reason,
            Some(origin),
        ) {
            Ok(reference) => self.references.push(reference),
            Err(_) => self.invariant_failed = true,
        }
    }

    fn fail(&mut self, reason: JsProducerFailureReason, span: Option<SourceUtf8Span>) {
        self.failures.push(JsProducerFailure::with_span(
            reason,
            self.source.clone(),
            span,
        ));
    }
}

impl<'ast, C> Visit<'ast> for ReferenceScanner<'_, '_, '_, C>
where
    C: Fn() -> bool + ?Sized,
{
    fn visit_call_expression(&mut self, call: &CallExpression<'ast>) {
        if self.cancelled || self.invariant_failed {
            return;
        }
        if (self.cancelled_probe)() {
            self.cancelled = true;
            return;
        }
        if !call.optional {
            if let Some(callee) = static_callee(&call.callee) {
                if let Some(binding) = self.recognizers.find(&callee) {
                    self.process_call(call, binding);
                }
            }
        }
        walk::walk_call_expression(self, call);
    }
}

fn static_callee(expression: &Expression<'_>) -> Option<String> {
    let mut current = expression;
    let mut reversed = Vec::new();
    loop {
        match current {
            Expression::Identifier(identifier) => {
                reversed.push(identifier.name.as_str());
                break;
            }
            Expression::ThisExpression(_) => {
                reversed.push("this");
                break;
            }
            Expression::StaticMemberExpression(member) if !member.optional => {
                reversed.push(member.property.name.as_str());
                if reversed.len() == 64 {
                    return None;
                }
                current = &member.object;
            }
            _ => return None,
        }
    }
    reversed.reverse();

    let byte_count = reversed.iter().map(|segment| segment.len()).sum::<usize>()
        + reversed.len().saturating_sub(1);
    if byte_count > 255 {
        return None;
    }
    Some(reversed.join("."))
}

const fn selector_invalid_reason(kind: JsRecognizerCallKind) -> JsProducerFailureReason {
    match kind {
        JsRecognizerCallKind::Lookup => JsProducerFailureReason::LookupSelectorInvalid,
        JsRecognizerCallKind::Set => JsProducerFailureReason::SetSelectorInvalid,
    }
}

fn reason_text(value: &'static str) -> ReasonText {
    ReasonText::try_new(value)
        .unwrap_or_else(|_| unreachable!("fixed reason text satisfies the contract"))
}

fn safe_span(source_text: &str, span: Span) -> Option<SourceUtf8Span> {
    if span.start > span.end {
        return None;
    }
    safe_diagnostic_span(
        source_text,
        span.start as usize,
        (span.end - span.start) as usize,
    )
}

fn safe_diagnostic_span(source_text: &str, offset: usize, length: usize) -> Option<SourceUtf8Span> {
    let end = offset.checked_add(length)?;
    let start = offset;
    if start > end
        || end > source_text.len()
        || !source_text.is_char_boundary(start)
        || !source_text.is_char_boundary(end)
    {
        return None;
    }
    SourceUtf8Span::try_new(u32::try_from(start).ok()?, u32::try_from(end).ok()?).ok()
}

fn earlier_span(
    left: Option<SourceUtf8Span>,
    right: Option<SourceUtf8Span>,
) -> Option<SourceUtf8Span> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(span), None) | (None, Some(span)) => Some(span),
        (None, None) => None,
    }
}

fn compare_references_by_origin(
    left: &MessageReference,
    right: &MessageReference,
) -> std::cmp::Ordering {
    left.origin()
        .map(SourceOrigin::span)
        .cmp(&right.origin().map(SourceOrigin::span))
        .then_with(|| left.cmp(right))
}

#[cfg(test)]
mod tests {
    use super::{admit_source_bytes, safe_diagnostic_span, SOURCE_BYTES_LIMIT};
    use intlify_contract::{
        ArtifactNamespace, PortablePathSegment, PortableRelativePath, SourceDocumentIdentity,
        SourceUtf8Span,
    };

    fn source() -> SourceDocumentIdentity {
        SourceDocumentIdentity::new(
            ArtifactNamespace::Project,
            PortableRelativePath::try_new(vec![
                PortablePathSegment::try_new("src").unwrap(),
                PortablePathSegment::try_new("limit.js").unwrap(),
            ])
            .unwrap(),
        )
    }

    #[test]
    fn source_limit_reports_the_first_over_limit_observation() {
        assert!(admit_source_bytes(&source(), SOURCE_BYTES_LIMIT).is_ok());
        let error = admit_source_bytes(&source(), SOURCE_BYTES_LIMIT + 99).unwrap_err();
        let JsProducerError::Failed(failure) = error else {
            panic!("expected source limit failure");
        };
        assert_eq!(failure.limit(), Some(SOURCE_BYTES_LIMIT));
        assert_eq!(failure.observed(), Some(SOURCE_BYTES_LIMIT + 1));
    }

    #[test]
    fn dependency_spans_cross_the_boundary_only_when_exact_and_utf8_safe() {
        let source_text = "aあb";
        assert_eq!(
            safe_diagnostic_span(source_text, 1, "あ".len()),
            SourceUtf8Span::try_new(1, 4).ok()
        );
        assert!(safe_diagnostic_span(source_text, 2, 1).is_none());
        assert!(safe_diagnostic_span(source_text, source_text.len(), 1).is_none());
        assert!(safe_diagnostic_span(source_text, usize::MAX, 1).is_none());
    }

    use crate::JsProducerError;
}
