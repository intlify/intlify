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
use crate::static_eval::{
    capture_static_expression, evaluate_static_expression, StaticExpressionView, StaticString,
};

/// Fixed inclusive byte ceiling for one selected source snapshot.
pub const SOURCE_BYTES_LIMIT: u64 = 64 * 1024 * 1024;

pub(crate) const DYNAMIC_LOOKUP_REASON: &str = "lookup argument is not statically known";
pub(crate) const BOUNDED_SET_REASON: &str = "bounded set declared by configured recognizer";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrontendStage {
    SourceParse,
    RecognizerMatchAndStaticEvaluation,
    KeyAndProvenanceConstruction,
}

pub(crate) trait FrontendObserver {
    fn enabled(&self) -> bool {
        false
    }

    fn begin(&mut self, _stage: FrontendStage) {}

    fn finish(&mut self, _stage: FrontendStage) {}

    fn observe(&mut self, _stage: FrontendStage, _checksum: u32) {}
}

pub(crate) struct NoopFrontendObserver;

impl FrontendObserver for NoopFrontendObserver {}

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

    pub(crate) fn into_references(self) -> Vec<MessageReference> {
        self.references.into_vec()
    }
}

/// Scan one exact source snapshot without caller cancellation.
pub fn scan_source(
    source: &SourceDocumentIdentity,
    source_bytes: &[u8],
    recognizers: &JsRecognizerSet,
) -> Result<JsSourceScan, JsProducerError> {
    scan_source_with_observer(
        source,
        source_bytes,
        recognizers,
        &|| false,
        &mut NoopFrontendObserver,
    )
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
    scan_source_with_observer(
        source,
        source_bytes,
        recognizers,
        cancelled,
        &mut NoopFrontendObserver,
    )
}

pub(crate) fn scan_source_with_observer<C, O>(
    source: &SourceDocumentIdentity,
    source_bytes: &[u8],
    recognizers: &JsRecognizerSet,
    cancelled: &C,
    observer: &mut O,
) -> Result<JsSourceScan, JsProducerError>
where
    C: Fn() -> bool + ?Sized,
    O: FrontendObserver + ?Sized,
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
                observer,
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
                observer,
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
                observer,
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
                        observer,
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
        // Public evidence identifies the first rejected byte so known-length
        // and streaming snapshots select the same deterministic observation.
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

fn parse_and_scan<C, O>(
    source: &SourceDocumentIdentity,
    source_text: &str,
    recognizers: &JsRecognizerSet,
    profile: JsSourceProfile,
    selected_goal: JsSelectedSourceGoal,
    cancelled: &C,
    observer: &mut O,
) -> Result<ParseAttempt, JsProducerError>
where
    C: Fn() -> bool + ?Sized,
    O: FrontendObserver + ?Sized,
{
    if cancelled() {
        return Err(JsProducerError::Cancelled);
    }

    observer.begin(FrontendStage::SourceParse);
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source_text, profile.source_type(selected_goal)).parse();
    if cancelled() {
        return Err(JsProducerError::Cancelled);
    }
    if parsed.panicked && parsed.diagnostics.is_empty() {
        return Err(JsProducerError::InternalInvariant);
    }
    if !parsed.diagnostics.is_empty() {
        let span = parsed
            .diagnostics
            .iter()
            .flat_map(|diagnostic| diagnostic.labels.iter())
            .filter_map(|label| {
                safe_diagnostic_span(
                    source_text,
                    usize::try_from(label.offset()).ok()?,
                    usize::try_from(label.len()).ok()?,
                )
            })
            .min();
        observer.finish(FrontendStage::SourceParse);
        if observer.enabled() {
            observer.observe(
                FrontendStage::SourceParse,
                checksum_parse_failure(profile, selected_goal, source_text, span),
            );
        }
        // OXC diagnostics and recovery state are dependency-owned. The frontend
        // rejects the entire AST and intentionally retains neither diagnostic
        // text nor recovery identifiers in its output contract. Only the
        // smallest independently checked byte range crosses this boundary.
        return Ok(ParseAttempt::SyntaxRejected(span));
    }

    let mut collector = CallEventCollector::new(source_text, cancelled);
    collector.visit_program(&parsed.program);
    if collector.cancelled {
        return Err(JsProducerError::Cancelled);
    }
    if collector.invariant_failed {
        return Err(JsProducerError::InternalInvariant);
    }
    observer.finish(FrontendStage::SourceParse);
    if observer.enabled() {
        observer.observe(
            FrontendStage::SourceParse,
            checksum_syntax_events(profile, selected_goal, source_text, &collector.events),
        );
    }

    observer.begin(FrontendStage::RecognizerMatchAndStaticEvaluation);
    let evaluation = evaluate_recognizer_events(source, recognizers, collector.events, cancelled)?;
    observer.finish(FrontendStage::RecognizerMatchAndStaticEvaluation);
    if observer.enabled() {
        observer.observe(
            FrontendStage::RecognizerMatchAndStaticEvaluation,
            checksum_recognizer_evaluation(&evaluation),
        );
    }

    observer.begin(FrontendStage::KeyAndProvenanceConstruction);
    let construction = construct_references(source, evaluation, cancelled)?;
    observer.finish(FrontendStage::KeyAndProvenanceConstruction);
    if observer.enabled() {
        observer.observe(
            FrontendStage::KeyAndProvenanceConstruction,
            checksum_reference_construction(&construction),
        );
    }

    if let Some(failure) = construction
        .failures
        .into_iter()
        .min_by(JsProducerFailure::cmp_within_source)
    {
        return Err(failure.into());
    }

    let mut references = construction.references;
    references.sort_by(compare_references_by_origin);
    if references.windows(2).any(|pair| {
        pair[0].origin().map(SourceOrigin::span) == pair[1].origin().map(SourceOrigin::span)
    }) {
        return Err(JsProducerError::InternalInvariant);
    }

    Ok(ParseAttempt::Admitted(JsSourceScan {
        profile,
        selected_goal,
        references: references.into_boxed_slice(),
    }))
}

enum CallArgument {
    Missing,
    Spread(Option<SourceUtf8Span>),
    Expression {
        span: Option<SourceUtf8Span>,
        view: StaticExpressionView,
    },
    Invalid,
}

struct CallSyntaxEvent {
    callee: String,
    call_span: Option<SourceUtf8Span>,
    argument: CallArgument,
}

struct CallEventCollector<'source, 'cancel, C: ?Sized> {
    source_text: &'source str,
    cancelled_probe: &'cancel C,
    events: Vec<CallSyntaxEvent>,
    cancelled: bool,
    invariant_failed: bool,
}

impl<'source, 'cancel, C> CallEventCollector<'source, 'cancel, C>
where
    C: Fn() -> bool + ?Sized,
{
    fn new(source_text: &'source str, cancelled_probe: &'cancel C) -> Self {
        Self {
            source_text,
            cancelled_probe,
            events: Vec::new(),
            cancelled: false,
            invariant_failed: false,
        }
    }
}

impl<'ast, C> Visit<'ast> for CallEventCollector<'ast, '_, C>
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
                let argument = match call.arguments.first() {
                    None => CallArgument::Missing,
                    Some(Argument::SpreadElement(spread)) => {
                        CallArgument::Spread(safe_span(self.source_text, spread.span))
                    }
                    Some(argument) => match argument.as_expression() {
                        Some(expression) => CallArgument::Expression {
                            span: safe_span(self.source_text, expression.span()),
                            view: capture_static_expression(expression),
                        },
                        None => CallArgument::Invalid,
                    },
                };
                self.events.push(CallSyntaxEvent {
                    callee,
                    call_span: safe_span(self.source_text, call.span),
                    argument,
                });
            }
        }
        walk::walk_call_expression(self, call);
    }
}

enum EvaluatedStaticString {
    Known(Box<str>),
    KnownInvalid,
    Dynamic,
}

struct RecognizerMatchFact {
    binding: JsRecognizerBinding,
    value: EvaluatedStaticString,
    span: SourceUtf8Span,
}

struct RecognizerEvaluation {
    matches: Vec<RecognizerMatchFact>,
    failures: Vec<JsProducerFailure>,
}

fn evaluate_recognizer_events<C>(
    source: &SourceDocumentIdentity,
    recognizers: &JsRecognizerSet,
    events: Vec<CallSyntaxEvent>,
    cancelled: &C,
) -> Result<RecognizerEvaluation, JsProducerError>
where
    C: Fn() -> bool + ?Sized,
{
    let mut matches = Vec::new();
    let mut failures = Vec::new();
    for event in events {
        if cancelled() {
            return Err(JsProducerError::Cancelled);
        }
        let Some(binding) = recognizers.find(&event.callee) else {
            continue;
        };
        let (view, span) = match event.argument {
            CallArgument::Missing => {
                failures.push(JsProducerFailure::with_span(
                    JsProducerFailureReason::SelectorArgumentMissing,
                    source.clone(),
                    event.call_span,
                ));
                continue;
            }
            CallArgument::Spread(span) => {
                failures.push(JsProducerFailure::with_span(
                    JsProducerFailureReason::SelectorArgumentSpread,
                    source.clone(),
                    span,
                ));
                continue;
            }
            CallArgument::Expression {
                span: Some(span),
                view,
            } => (view, span),
            CallArgument::Expression { span: None, .. } | CallArgument::Invalid => {
                return Err(JsProducerError::InternalInvariant);
            }
        };
        let value = match evaluate_static_expression(&view) {
            StaticString::Known(value) => EvaluatedStaticString::Known(value.into()),
            StaticString::KnownInvalid => EvaluatedStaticString::KnownInvalid,
            StaticString::Dynamic => EvaluatedStaticString::Dynamic,
        };
        matches.push(RecognizerMatchFact {
            binding: binding.clone(),
            value,
            span,
        });
    }
    Ok(RecognizerEvaluation { matches, failures })
}

struct ReferenceConstruction {
    references: Vec<MessageReference>,
    failures: Vec<JsProducerFailure>,
}

fn construct_references<C>(
    source: &SourceDocumentIdentity,
    evaluation: RecognizerEvaluation,
    cancelled: &C,
) -> Result<ReferenceConstruction, JsProducerError>
where
    C: Fn() -> bool + ?Sized,
{
    let mut references = Vec::new();
    let mut failures = evaluation.failures;
    for fact in evaluation.matches {
        if cancelled() {
            return Err(JsProducerError::Cancelled);
        }
        let (selector, reason) = match fact.value {
            EvaluatedStaticString::Known(value) => {
                let Ok(selector) = convert_static_selector(&fact.binding, &value) else {
                    failures.push(JsProducerFailure::with_span(
                        selector_invalid_reason(fact.binding.kind()),
                        source.clone(),
                        Some(fact.span),
                    ));
                    continue;
                };
                let reason = match fact.binding.kind() {
                    JsRecognizerCallKind::Lookup => None,
                    JsRecognizerCallKind::Set => Some(reason_text(BOUNDED_SET_REASON)),
                };
                (selector, reason)
            }
            EvaluatedStaticString::KnownInvalid => {
                failures.push(JsProducerFailure::with_span(
                    selector_invalid_reason(fact.binding.kind()),
                    source.clone(),
                    Some(fact.span),
                ));
                continue;
            }
            EvaluatedStaticString::Dynamic => match fact.binding.kind() {
                JsRecognizerCallKind::Lookup => (
                    MessageSelector::UnboundedDynamic,
                    Some(reason_text(DYNAMIC_LOOKUP_REASON)),
                ),
                JsRecognizerCallKind::Set => {
                    failures.push(JsProducerFailure::with_span(
                        JsProducerFailureReason::SetSelectorDynamic,
                        source.clone(),
                        Some(fact.span),
                    ));
                    continue;
                }
            },
        };
        let origin = SourceOrigin::new(source.clone(), fact.span);
        match MessageReference::try_new(
            fact.binding.scope().clone(),
            fact.binding.domain(),
            selector,
            reason,
            Some(origin),
        ) {
            Ok(reference) => references.push(reference),
            Err(_) => return Err(JsProducerError::InternalInvariant),
        }
    }
    Ok(ReferenceConstruction {
        references,
        failures,
    })
}

fn checksum_parse_failure(
    profile: JsSourceProfile,
    selected_goal: JsSelectedSourceGoal,
    source_text: &str,
    span: Option<SourceUtf8Span>,
) -> u32 {
    let mut checksum = FrontendChecksum::new(b"intlify-js-source-parse-v1");
    checksum.write_profile(0x01, profile, selected_goal);
    checksum.write_bytes(0x02, source_text.as_bytes());
    checksum.write_str(0x03, "syntax-invalid");
    checksum.write_optional_span(0x04, span);
    checksum.finish()
}

fn checksum_syntax_events(
    profile: JsSourceProfile,
    selected_goal: JsSelectedSourceGoal,
    source_text: &str,
    events: &[CallSyntaxEvent],
) -> u32 {
    let mut checksum = FrontendChecksum::new(b"intlify-js-source-parse-v1");
    checksum.write_profile(0x01, profile, selected_goal);
    checksum.write_bytes(0x02, source_text.as_bytes());
    checksum.write_u64(0x03, events.len() as u64);
    for event in events {
        checksum.write_str(0x04, &event.callee);
        checksum.write_optional_span(0x05, event.call_span);
        match event.argument {
            CallArgument::Missing => checksum.write_str(0x06, "missing"),
            CallArgument::Spread(span) => {
                checksum.write_str(0x06, "spread");
                checksum.write_optional_span(0x07, span);
            }
            CallArgument::Expression { span, .. } => {
                checksum.write_str(0x06, "expression");
                checksum.write_optional_span(0x07, span);
            }
            CallArgument::Invalid => checksum.write_str(0x06, "invalid"),
        }
    }
    checksum.finish()
}

fn checksum_recognizer_evaluation(evaluation: &RecognizerEvaluation) -> u32 {
    let mut checksum = FrontendChecksum::new(b"intlify-js-recognizer-static-evaluation-v1");
    checksum.write_u64(0x01, evaluation.matches.len() as u64);
    for fact in &evaluation.matches {
        checksum.write_binding(0x02, &fact.binding);
        checksum.write_optional_span(0x03, Some(fact.span));
        match &fact.value {
            EvaluatedStaticString::Known(value) => {
                checksum.write_str(0x04, "known");
                checksum.write_str(0x05, value.as_ref());
            }
            EvaluatedStaticString::KnownInvalid => checksum.write_str(0x04, "known-invalid"),
            EvaluatedStaticString::Dynamic => checksum.write_str(0x04, "dynamic"),
        }
    }
    checksum.write_u64(0x06, evaluation.failures.len() as u64);
    for failure in &evaluation.failures {
        checksum.write_failure(0x07, failure);
    }
    checksum.finish()
}

fn checksum_reference_construction(construction: &ReferenceConstruction) -> u32 {
    let mut checksum = FrontendChecksum::new(b"intlify-js-key-provenance-v1");
    checksum.write_u64(0x01, construction.references.len() as u64);
    for reference in &construction.references {
        checksum.write_scope(0x02, reference.scope());
        checksum.write_str(0x03, reference.domain().as_str());
        checksum.write_str(0x04, reference.selector().kind_str());
        match reference.selector() {
            MessageSelector::Exact(key) => checksum.write_str(0x05, key.as_str()),
            MessageSelector::Prefix(prefix) => checksum.write_str(0x05, prefix.as_str()),
            MessageSelector::Pattern(pattern) => checksum.write_str(0x05, pattern.as_str()),
            MessageSelector::AllInScope | MessageSelector::UnboundedDynamic => {}
        }
        match reference.reason() {
            Some(reason) => {
                checksum.write_bool(0x06, true);
                checksum.write_str(0x07, reason.as_str());
            }
            None => checksum.write_bool(0x06, false),
        }
        match reference.origin() {
            Some(origin) => {
                checksum.write_bool(0x08, true);
                checksum.write_source(0x09, origin.source());
                checksum.write_optional_span(0x0a, Some(origin.span()));
            }
            None => checksum.write_bool(0x08, false),
        }
    }
    checksum.write_u64(0x0b, construction.failures.len() as u64);
    for failure in &construction.failures {
        checksum.write_failure(0x0c, failure);
    }
    checksum.finish()
}

struct FrontendChecksum {
    state: u32,
}

impl FrontendChecksum {
    const OFFSET_BASIS: u32 = 2_166_136_261;
    const MULTIPLIER: u32 = 16_777_619;

    fn new(domain: &[u8]) -> Self {
        let mut checksum = Self {
            state: Self::OFFSET_BASIS,
        };
        checksum.update(domain);
        checksum.update(&[0]);
        checksum
    }

    const fn finish(self) -> u32 {
        self.state
    }

    fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.state ^= u32::from(*byte);
            self.state = self.state.wrapping_mul(Self::MULTIPLIER);
        }
    }

    fn write_bytes(&mut self, tag: u8, value: &[u8]) {
        self.update(&[tag]);
        self.update(&(value.len() as u64).to_be_bytes());
        self.update(value);
    }

    fn write_str(&mut self, tag: u8, value: &str) {
        self.write_bytes(tag, value.as_bytes());
    }

    fn write_u64(&mut self, tag: u8, value: u64) {
        self.write_bytes(tag, &value.to_be_bytes());
    }

    fn write_bool(&mut self, tag: u8, value: bool) {
        self.write_bytes(tag, &[u8::from(value)]);
    }

    fn write_optional_span(&mut self, tag: u8, span: Option<SourceUtf8Span>) {
        match span {
            Some(span) => {
                self.write_bool(tag, true);
                self.write_u64(tag.saturating_add(1), u64::from(span.start()));
                self.write_u64(tag.saturating_add(2), u64::from(span.end()));
            }
            None => self.write_bool(tag, false),
        }
    }

    fn write_profile(
        &mut self,
        tag: u8,
        profile: JsSourceProfile,
        selected_goal: JsSelectedSourceGoal,
    ) {
        let grammar = match profile.grammar() {
            crate::JsGrammarProfile::JavaScript => "javascript",
            crate::JsGrammarProfile::JavaScriptJsx => "javascript-jsx",
            crate::JsGrammarProfile::TypeScript => "typescript",
            crate::JsGrammarProfile::TypeScriptJsx => "typescript-jsx",
            crate::JsGrammarProfile::TypeScriptDeclaration => "typescript-declaration",
        };
        let goal = match selected_goal {
            JsSelectedSourceGoal::Module => "module",
            JsSelectedSourceGoal::Script => "script",
        };
        self.write_str(tag, grammar);
        self.write_str(tag.saturating_add(1), goal);
    }

    fn write_binding(&mut self, tag: u8, binding: &JsRecognizerBinding) {
        self.write_str(tag, binding.callee());
        self.write_str(tag.saturating_add(1), binding.kind().as_str());
        self.write_scope(tag.saturating_add(2), binding.scope());
        self.write_str(tag.saturating_add(3), binding.domain().as_str());
        self.write_str(tag.saturating_add(4), binding.key_syntax().as_str());
    }

    fn write_scope(&mut self, tag: u8, scope: &intlify_contract::CatalogScopeId) {
        self.write_str(tag, scope.namespace().as_str());
        self.write_str(tag.saturating_add(1), scope.name().as_str());
    }

    fn write_source(&mut self, tag: u8, source: &SourceDocumentIdentity) {
        self.write_str(tag, source.namespace().as_str());
        self.write_u64(tag.saturating_add(1), source.path().segments().len() as u64);
        for segment in source.path().segments() {
            self.write_str(tag.saturating_add(2), segment.as_str());
        }
    }

    fn write_failure(&mut self, tag: u8, failure: &JsProducerFailure) {
        self.write_str(tag, failure.stage().as_str());
        self.write_str(tag.saturating_add(1), failure.reason().as_str());
        self.write_source(tag.saturating_add(2), failure.source());
        self.write_optional_span(tag.saturating_add(3), failure.span());
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
