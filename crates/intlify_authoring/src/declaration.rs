// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Resolving declarations against a checked context.
//!
//! This is where message analysis and resolved context meet. Each declaration
//! gets one canonical source locale, one admitted surface class, its checked
//! parameter requirements, and the projection a revision is taken over.
//!
//! The resolution order is fixed by 016 and is not a preference. An explicit
//! annotation wins; otherwise the context default applies; otherwise the
//! declaration is blocked. Nothing substitutes a requested locale, a host
//! locale, `und`, or a language inferred from the text, because a wrong source
//! locale silently mistranslates rather than failing.

use crate::context::{AuthoringContext, ContextKind, LocaleFailure};
use crate::diagnostic::{Diagnostic, DiagnosticOrigin, ReasonFamily, Severity, Stage};
use crate::limits::{AuthoringLimits, LimitKind};
use crate::message::{analyze_message, ExtractionSegment, MessageFailure, MessageInput};
use crate::primitives::{NonemptyText, Occurrence, PrimitiveError};
use crate::projection::{intent_projection, IntentProjection, Usage};
use crate::workspace::AnalysisWorkspace;

/// Which basis established a declaration's source locale.
///
/// Switching between the two with the same canonical locale does not change a
/// revision, so the basis is retained as evidence rather than folded into the
/// projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SourceLocaleBasis {
    Explicit,
    ContextDefault,
}

impl SourceLocaleBasis {
    /// Return the exact wire spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Explicit => "explicit",
            Self::ContextDefault => "context-default",
        }
    }
}

/// The metadata values a host recognized for one declaration.
///
/// Only values arrive here. Recognizing the annotation, rejecting duplicates,
/// and deciding which declaration it attaches to are host-side concerns.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DeclarationMetadata<'a> {
    /// An explicitly authored source locale, before canonicalization.
    pub source_locale: Option<&'a str>,
    /// An explicitly authored surface class.
    pub surface_class: Option<&'a str>,
    /// An explanation of the message's meaning or use.
    pub description: Option<&'a str>,
}

/// One parameter the host supplied at a use site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParameterBinding {
    name: String,
    expression: Occurrence,
}

impl ParameterBinding {
    /// Retain one binding, keeping the host's evaluation position.
    #[must_use]
    pub fn new(name: &str, expression: Occurrence) -> Self {
        Self {
            name: name.to_owned(),
            expression,
        }
    }

    /// Borrow the external parameter name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Borrow the expression's source position.
    ///
    /// The expression is never evaluated or serialized as a value.
    #[must_use]
    pub const fn expression(&self) -> &Occurrence {
        &self.expression
    }
}

/// One declaration to resolve.
#[derive(Debug, Clone)]
pub struct DeclarationInput<'a> {
    /// Where the declaration appears.
    pub occurrence: Occurrence,
    /// The already host-decoded message.
    pub message: MessageInput<'a>,
    /// The metadata values the host recognized.
    pub metadata: DeclarationMetadata<'a>,
    /// A proven semantic usage, admitted only under the context's profile.
    pub usage: Option<&'a str>,
    /// The parameters supplied at the use site, in host evaluation order.
    pub parameters: &'a [ParameterBinding],
}

/// The checked facts for one declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclarationFacts {
    occurrence: Occurrence,
    mf2_source: String,
    projection: IntentProjection,
    source_locale_basis: SourceLocaleBasis,
    surface_class: String,
    extraction_map: Box<[ExtractionSegment]>,
}

impl DeclarationFacts {
    /// Borrow the declaration's source position.
    #[must_use]
    pub const fn occurrence(&self) -> &Occurrence {
        &self.occurrence
    }

    /// Borrow the exact MF2 source that was analyzed.
    #[must_use]
    pub fn mf2_source(&self) -> &str {
        &self.mf2_source
    }

    /// Borrow the projection a revision is computed from.
    #[must_use]
    pub const fn projection(&self) -> &IntentProjection {
        &self.projection
    }

    /// Return which basis established the source locale.
    #[must_use]
    pub const fn source_locale_basis(&self) -> SourceLocaleBasis {
        self.source_locale_basis
    }

    /// Borrow the admitted surface class.
    #[must_use]
    pub fn surface_class(&self) -> &str {
        &self.surface_class
    }

    /// Return the mapping from emitted MF2 bytes back to the supplied text.
    #[must_use]
    pub fn extraction_map(&self) -> &[ExtractionSegment] {
        &self.extraction_map
    }
}

/// Whether an invocation produced a complete result for its declared scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Checked,
    Blocked,
}

/// The complete result of one authoring invocation.
///
/// A blocked result still carries the facts that were independently
/// established, because they support inspection. It deliberately makes them
/// reachable only through a differently named accessor, so a consumer cannot
/// mistake them for the complete authoring input a build requires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoringResult {
    outcome: Outcome,
    declarations: Box<[DeclarationFacts]>,
    diagnostics: Box<[Diagnostic]>,
}

impl AuthoringResult {
    /// Return whether the invocation is complete for its declared scope.
    #[must_use]
    pub const fn outcome(&self) -> Outcome {
        self.outcome
    }

    /// Return the complete checked declarations, when the result is checked.
    ///
    /// A blocked result returns `None` rather than a shorter list, so partial
    /// facts cannot be consumed as though the scope had been covered.
    #[must_use]
    pub fn checked(&self) -> Option<&[DeclarationFacts]> {
        matches!(self.outcome, Outcome::Checked).then_some(&*self.declarations)
    }

    /// Return the independently established facts, whatever the outcome.
    ///
    /// These support inspection only. They are not complete authoring input.
    #[must_use]
    pub fn inspection_facts(&self) -> &[DeclarationFacts] {
        &self.declarations
    }

    /// Return the diagnostics in deterministic reporting order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

/// Complete failure of an authoring invocation.
///
/// These are operational, not authoring mistakes. A caller cannot fix them by
/// editing source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthoringFailure {
    /// A named bound was exhausted.
    Limit(LimitKind),
    /// A message could not be analyzed.
    Message(MessageFailure),
    /// The canonicalization provider could not answer.
    LocaleProviderUnavailable,
    /// A context input was structurally invalid.
    ContextInvalid(PrimitiveError),
    /// The context claims a production kind this phase does not admit.
    ProductionContextUnsupported(ContextKind),
    /// Two declarations claim the same occurrence.
    DuplicateOccurrence,
}

impl From<MessageFailure> for AuthoringFailure {
    fn from(failure: MessageFailure) -> Self {
        match failure {
            MessageFailure::Limit(kind) => Self::Limit(kind),
            other => Self::Message(other),
        }
    }
}

/// A bounded collector for the diagnostics one invocation reports.
///
/// The budget is enforced as each diagnostic is added rather than after the
/// fact. One declaration can report a diagnostic per supplied parameter, so a
/// caller that supplies many would otherwise buy an unbounded vector before
/// the limit is ever consulted.
struct DiagnosticSink {
    reported: Vec<Diagnostic>,
    budget: u64,
    exhausted: bool,
}

impl DiagnosticSink {
    const fn new(budget: u64) -> Self {
        Self {
            reported: Vec::new(),
            budget,
            exhausted: false,
        }
    }

    /// Report one diagnostic, or record that the budget is exhausted.
    ///
    /// Nothing is appended past the budget, so the failure below is reported
    /// from a bounded collector rather than from an oversized one.
    fn push(&mut self, diagnostic: Diagnostic) {
        if self.reported.len() as u64 >= self.budget {
            self.exhausted = true;
            return;
        }
        self.reported.push(diagnostic);
    }

    fn extend(&mut self, diagnostics: impl IntoIterator<Item = Diagnostic>) {
        for diagnostic in diagnostics {
            self.push(diagnostic);
        }
    }

    const fn exhausted(&self) -> bool {
        self.exhausted
    }

    fn into_reported(mut self) -> Vec<Diagnostic> {
        self.reported.sort_by(Diagnostic::reporting_cmp);
        self.reported
    }
}

/// Resolve a finite set of declarations against a checked context.
///
/// Every declaration is analyzed, so one blocked declaration does not suppress
/// independent facts about the others. The invocation is checked only when no
/// declaration was blocked.
pub fn resolve_declarations(
    context: &dyn AuthoringContext,
    inputs: &[DeclarationInput<'_>],
    limits: &AuthoringLimits,
    workspace: &mut AnalysisWorkspace,
) -> Result<AuthoringResult, AuthoringFailure> {
    // Production admission needs the checked 015 inputs and the 017/018
    // representation and authority work that later phases own. Accepting the
    // label here would let a fixture be read later as production evidence.
    let kind = context.basis().context_kind();
    if kind != ContextKind::TestContext {
        return Err(AuthoringFailure::ProductionContextUnsupported(kind));
    }
    if inputs.len() as u64 > limits.declarations {
        return Err(AuthoringFailure::Limit(LimitKind::Declarations));
    }
    if context.surface_vocabulary().members().len() as u64 > limits.vocabulary_members {
        return Err(AuthoringFailure::Limit(LimitKind::VocabularyMembers));
    }
    reject_duplicate_occurrences(inputs)?;

    let mut declarations = Vec::new();
    let mut diagnostics = DiagnosticSink::new(limits.diagnostics);
    let mut blocked = false;

    for input in inputs {
        match resolve_one(context, input, limits, workspace, &mut diagnostics)? {
            Some(facts) => declarations.push(facts),
            None => blocked = true,
        }
        if diagnostics.exhausted() {
            return Err(AuthoringFailure::Limit(LimitKind::Diagnostics));
        }
    }

    declarations.sort_by(|left, right| left.occurrence.canonical_cmp(&right.occurrence));

    Ok(AuthoringResult {
        outcome: if blocked {
            Outcome::Blocked
        } else {
            Outcome::Checked
        },
        declarations: declarations.into_boxed_slice(),
        diagnostics: diagnostics.into_reported().into_boxed_slice(),
    })
}

fn reject_duplicate_occurrences(inputs: &[DeclarationInput<'_>]) -> Result<(), AuthoringFailure> {
    for (index, input) in inputs.iter().enumerate() {
        for other in &inputs[index + 1..] {
            if input.occurrence == other.occurrence {
                return Err(AuthoringFailure::DuplicateOccurrence);
            }
        }
    }
    Ok(())
}

/// Resolve one declaration, returning `None` when it is blocked.
fn resolve_one(
    context: &dyn AuthoringContext,
    input: &DeclarationInput<'_>,
    limits: &AuthoringLimits,
    workspace: &mut AnalysisWorkspace,
    diagnostics: &mut DiagnosticSink,
) -> Result<Option<DeclarationFacts>, AuthoringFailure> {
    if !input.occurrence.role().is_declaration() {
        diagnostics.push(authoring(
            Stage::ResultConstruction,
            ReasonFamily::AuthoringInputInvalid,
            input,
        ));
        return Ok(None);
    }
    // One invocation resolves one owner's declarations. Source that belongs to
    // another owner is outside this scope, and admitting it would attribute
    // the message to the wrong application or library. This is checked before
    // any analysis, because out-of-scope input is not work to be done.
    if input.occurrence.source().owner() != context.owner() {
        diagnostics.push(authoring(
            Stage::ResultConstruction,
            ReasonFamily::AuthoringInputInvalid,
            input,
        ));
        return Ok(None);
    }

    let analysis = analyze_message(input.message, &input.occurrence, limits, workspace)?;
    let mut blocked = analysis.is_blocked();
    diagnostics.extend(analysis.diagnostics().iter().cloned());

    let locale = resolve_locale(context, input, diagnostics)?;
    let class = resolve_surface_class(context, input, diagnostics);
    let usage = resolve_usage(context, input, limits, diagnostics);
    let description = resolve_description(input, limits, diagnostics);

    let Some(facts) = analysis.facts() else {
        return Ok(None);
    };
    if !match_parameters(input, facts.parameters(), diagnostics) {
        blocked = true;
    }

    let (Some((locale, basis)), Some(class), Ok(usage), Ok(description)) =
        (locale, class, usage, description)
    else {
        return Ok(None);
    };
    if blocked {
        return Ok(None);
    }

    let projection = intent_projection(
        facts.message().clone(),
        facts.parameters().to_vec().into_boxed_slice(),
        locale.as_str(),
        usage,
        description.as_deref(),
    )
    .map_err(AuthoringFailure::ContextInvalid)?;

    Ok(Some(DeclarationFacts {
        occurrence: input.occurrence.clone(),
        mf2_source: analysis.mf2_source().to_owned(),
        projection,
        source_locale_basis: basis,
        surface_class: class,
        extraction_map: analysis.extraction_map().to_vec().into_boxed_slice(),
    }))
}

type ResolvedLocale = Option<(crate::context::CanonicalLocale, SourceLocaleBasis)>;

fn resolve_locale(
    context: &dyn AuthoringContext,
    input: &DeclarationInput<'_>,
    diagnostics: &mut DiagnosticSink,
) -> Result<ResolvedLocale, AuthoringFailure> {
    if let Some(authored) = input.metadata.source_locale {
        return match context.canonicalize(authored) {
            Ok(locale) => Ok(Some((locale, SourceLocaleBasis::Explicit))),
            Err(LocaleFailure::Unavailable) => {
                // The author wrote nothing wrong; the provider could not answer.
                Err(AuthoringFailure::LocaleProviderUnavailable)
            }
            Err(LocaleFailure::InvalidIdentifier | LocaleFailure::UnsupportedInput) => {
                diagnostics.push(authoring(
                    Stage::ContextResolution,
                    ReasonFamily::AuthoringSourceLocaleInvalid,
                    input,
                ));
                Ok(None)
            }
        };
    }
    if let Some(default) = context.default_source_locale() {
        return Ok(Some((default.clone(), SourceLocaleBasis::ContextDefault)));
    }
    diagnostics.push(authoring(
        Stage::ContextResolution,
        ReasonFamily::AuthoringSourceLocaleMissing,
        input,
    ));
    Ok(None)
}

fn resolve_surface_class(
    context: &dyn AuthoringContext,
    input: &DeclarationInput<'_>,
    diagnostics: &mut DiagnosticSink,
) -> Option<String> {
    // An explicit assignment wins; only its absence lets the default apply.
    let candidate = input
        .metadata
        .surface_class
        .or_else(|| context.basis().default_surface_class());
    match candidate {
        Some(class) if context.surface_vocabulary().admits(class) => Some(class.to_owned()),
        _ => {
            diagnostics.push(authoring(
                Stage::ContextResolution,
                ReasonFamily::AuthoringSurfaceClassInvalid,
                input,
            ));
            None
        }
    }
}

fn resolve_usage(
    context: &dyn AuthoringContext,
    input: &DeclarationInput<'_>,
    limits: &AuthoringLimits,
    diagnostics: &mut DiagnosticSink,
) -> Result<Option<Usage>, ()> {
    let Some(value) = input.usage else {
        return Ok(None);
    };
    let Some(profile) = context.usage_profile() else {
        // Usage is only meaningful under a registered profile. A coverage class
        // or a DOM tag cannot substitute for one.
        diagnostics.push(authoring(
            Stage::ContextResolution,
            ReasonFamily::AuthoringMetadataInvalid,
            input,
        ));
        return Err(());
    };
    let Ok(value) = admitted_text(value, limits) else {
        diagnostics.push(authoring(
            Stage::ContextResolution,
            ReasonFamily::AuthoringMetadataInvalid,
            input,
        ));
        return Err(());
    };
    Ok(Some(Usage {
        profile: profile.clone(),
        value,
    }))
}

fn resolve_description(
    input: &DeclarationInput<'_>,
    limits: &AuthoringLimits,
    diagnostics: &mut DiagnosticSink,
) -> Result<Option<String>, ()> {
    let Some(description) = input.metadata.description else {
        // There is no shared description default: absence stays absence.
        return Ok(None);
    };
    if admitted_text(description, limits).is_ok() {
        return Ok(Some(description.to_owned()));
    }
    diagnostics.push(authoring(
        Stage::ContextResolution,
        ReasonFamily::AuthoringMetadataInvalid,
        input,
    ));
    Err(())
}

fn admitted_text(value: &str, limits: &AuthoringLimits) -> Result<NonemptyText, ()> {
    if value.len() as u64 > limits.metadata_value_bytes {
        return Err(());
    }
    NonemptyText::from_validated(value).map_err(|_| ())
}

/// Compare the host's parameters with what the message requires.
///
/// Missing, extra, and duplicate names are separate diagnostics because they
/// have different fixes. Returning false blocks the declaration.
fn match_parameters(
    input: &DeclarationInput<'_>,
    required: &[String],
    diagnostics: &mut DiagnosticSink,
) -> bool {
    let mut matched = true;
    let mut seen: Vec<&str> = Vec::new();
    for binding in input.parameters {
        if seen.contains(&binding.name()) {
            diagnostics.push(
                authoring(
                    Stage::ContextResolution,
                    ReasonFamily::AuthoringParameterMismatch,
                    input,
                )
                .with_related(vec![binding.expression().clone()]),
            );
            matched = false;
            continue;
        }
        seen.push(binding.name());
        if !required.iter().any(|name| name == binding.name()) {
            diagnostics.push(
                authoring(
                    Stage::ContextResolution,
                    ReasonFamily::AuthoringParameterMismatch,
                    input,
                )
                .with_related(vec![binding.expression().clone()]),
            );
            matched = false;
        }
    }
    for name in required {
        if !seen.contains(&name.as_str()) {
            diagnostics.push(authoring(
                Stage::ContextResolution,
                ReasonFamily::AuthoringParameterMismatch,
                input,
            ));
            matched = false;
        }
    }
    matched
}

fn authoring(stage: Stage, family: ReasonFamily, input: &DeclarationInput<'_>) -> Diagnostic {
    Diagnostic::new(
        stage,
        DiagnosticOrigin::Authoring(family),
        Severity::Error,
        input.occurrence.clone(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::{ByteRange, OccurrenceRole, OwnerIdentity, OwnerKind, SourceSnapshot};
    use intlify_shared_json::token::VersionedIdentity;

    fn diagnostic() -> Diagnostic {
        let source = SourceSnapshot::new(
            OwnerIdentity::new(OwnerKind::Application, "storefront").unwrap(),
            "checkout",
            "1",
            VersionedIdentity::literal("intlify-fixture-grammar", "0"),
            4096,
            &format!("sha256:{}", "0".repeat(64)),
        )
        .unwrap();
        Diagnostic::new(
            Stage::ContextResolution,
            DiagnosticOrigin::Authoring(ReasonFamily::AuthoringParameterMismatch),
            Severity::Error,
            Occurrence::new(
                source,
                ByteRange::new(0, 8).unwrap(),
                OccurrenceRole::UiLiteral,
            )
            .unwrap(),
        )
    }

    #[test]
    fn the_sink_never_grows_past_its_budget_however_many_arrive() {
        let mut sink = DiagnosticSink::new(3);
        for _ in 0..1_000 {
            sink.push(diagnostic());
        }
        // The point of the budget is the memory, not only the failure: a
        // thousand arrivals leave three retained diagnostics behind.
        assert!(sink.exhausted());
        assert_eq!(sink.into_reported().len(), 3);
    }

    #[test]
    fn a_sink_inside_its_budget_reports_everything_and_is_not_exhausted() {
        let mut sink = DiagnosticSink::new(3);
        sink.extend([diagnostic(), diagnostic()]);
        assert!(!sink.exhausted());
        assert_eq!(sink.into_reported().len(), 2);

        // The boundary is exact: filling the budget is not exceeding it.
        let mut exact = DiagnosticSink::new(2);
        exact.extend([diagnostic(), diagnostic()]);
        assert!(!exact.exhausted());
        assert_eq!(exact.into_reported().len(), 2);
    }
}
