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

use std::collections::BTreeSet;

use crate::context::{AuthoringContext, ContextKind, LocaleFailure};
use crate::diagnostic::{
    detail, Detail, Diagnostic, DiagnosticOrigin, MessageRange, ReasonFamily, Severity, Stage,
};
use crate::limits::{AuthoringLimits, LimitKind};
use crate::message::{
    analyze_message, compose_extraction_map, validate_input_map, ExtractionSegment, InputSegment,
    MappingError, MessageFailure, MessageInput,
};
use crate::primitives::{ByteRange, NonemptyText, Occurrence, PrimitiveError};
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
    /// Where the decoded message text came from in host source.
    ///
    /// Supplying it moves the returned extraction map into host coordinates.
    /// Omitting it leaves that map in the coordinates of the supplied text,
    /// which is a different fact rather than a missing one: a caller that
    /// decoded nothing has nothing to compose.
    pub input_map: Option<&'a [InputSegment]>,
    /// The metadata values the host recognized.
    pub metadata: DeclarationMetadata<'a>,
    /// A proven semantic usage, admitted only under the context's profile.
    pub usage: Option<&'a str>,
    /// The parameters supplied at the use site, in host evaluation order.
    ///
    /// `None` says this declaration has no use site in this invocation, which
    /// is what a reusable declaration looks like before anything references
    /// it. An empty slice says a use site supplied nothing, so a message that
    /// requires a name reports it missing. Collapsing the two would make every
    /// standalone declaration of a message with parameters fail.
    pub parameters: Option<&'a [ParameterBinding]>,
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

    /// Return the mapping from emitted MF2 bytes back to their origin.
    ///
    /// The source side is in host coordinates when the caller supplied an
    /// input map, and in the coordinates of the supplied text otherwise.
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
    /// A supplied input map does not describe the supplied text.
    ///
    /// Exhausting a bound while composing is reported as [`Self::Limit`]
    /// instead, because that map does describe the text.
    InputMap(MappingError),
    /// The caller's probe asked this invocation to stop.
    ///
    /// Cancellation is a control-flow result, never evidence. A cancelled
    /// invocation establishes nothing about the declarations it did reach.
    Cancelled,
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
    resolve_declarations_with_cancellation(context, inputs, limits, workspace, &|| false)
}

/// Resolve declarations under a caller-owned cancellation probe.
///
/// The probe is consulted at declaration boundaries, so it must be cheap,
/// usable under the caller's own execution model, and free of assumptions
/// about how many times it is called. Cancelling yields no partial result: a
/// caller that stops the work learns nothing about the source, which is what
/// keeps a cancelled run from being read as an absence of declarations.
pub fn resolve_declarations_with_cancellation<C>(
    context: &dyn AuthoringContext,
    inputs: &[DeclarationInput<'_>],
    limits: &AuthoringLimits,
    workspace: &mut AnalysisWorkspace,
    cancelled: &C,
) -> Result<AuthoringResult, AuthoringFailure>
where
    C: Fn() -> bool + ?Sized,
{
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
        if cancelled() {
            return Err(AuthoringFailure::Cancelled);
        }
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

/// Reject two declarations that claim one position in one snapshot.
///
/// Comparison uses the canonical order, which covers every part of an
/// occurrence except the declared byte length. Two occurrences that agree on
/// everything else while disagreeing on the length of their unit are also
/// rejected here, because that pair is a contradiction about the source rather
/// than two positions to keep apart.
fn reject_duplicate_occurrences(inputs: &[DeclarationInput<'_>]) -> Result<(), AuthoringFailure> {
    let mut order: Vec<&Occurrence> = inputs.iter().map(|input| &input.occurrence).collect();
    order.sort_by(|left, right| left.canonical_cmp(right));
    if order
        .windows(2)
        .any(|pair| pair[0].canonical_cmp(pair[1]) == std::cmp::Ordering::Equal)
    {
        return Err(AuthoringFailure::DuplicateOccurrence);
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
        diagnostics.push(
            authoring(
                Stage::ResultConstruction,
                ReasonFamily::AuthoringInputInvalid,
                input,
            )
            .with_detail(detail::occurrence_role_invalid()),
        );
        return Ok(None);
    }
    // One invocation resolves one owner's declarations. Source that belongs to
    // another owner is outside this scope, and admitting it would attribute
    // the message to the wrong application or library. This is checked before
    // any analysis, because out-of-scope input is not work to be done.
    if input.occurrence.source().owner() != context.owner() {
        diagnostics.push(
            authoring(
                Stage::ResultConstruction,
                ReasonFamily::AuthoringInputInvalid,
                input,
            )
            .with_detail(detail::occurrence_owner_foreign()),
        );
        return Ok(None);
    }

    // A map that does not describe the text is the host's mistake whatever the
    // author wrote, so it is checked before anything that could block this
    // declaration first. Checking it only on the way out would report a host
    // integration bug for a clean declaration and stay silent for a blocked
    // one, which is the opposite of how the two kinds of failure are meant to
    // separate.
    if let Some(map) = input.input_map {
        validate_input_map(map, input.message.text(), &input.occurrence)
            .map_err(input_map_failure)?;
    }

    let analysis = match analyze_message(input.message, &input.occurrence, limits, workspace) {
        Ok(analysis) => analysis,
        Err(MessageFailure::UnrepresentableScalar { offset }) => {
            // The text came from source an author can edit, so this is a
            // reportable authoring form rather than a failed invocation. The
            // encoder alone cannot know that, which is why it reports the
            // position and leaves the decision to the caller that has one.
            diagnostics.push(
                unrepresentable(input, offset)
                    .ok_or(MessageFailure::UnrepresentableScalar { offset })?,
            );
            return Ok(None);
        }
        Err(other) => return Err(other.into()),
    };
    let mut blocked = analysis.is_blocked();
    diagnostics.extend(analysis.diagnostics().iter().cloned());

    let locale = resolve_locale(context, input, diagnostics)?;
    let class = resolve_surface_class(context, input, diagnostics);
    let usage = resolve_usage(context, input, limits, diagnostics);
    let description = resolve_description(input, limits, diagnostics);

    let Some(facts) = analysis.facts() else {
        return Ok(None);
    };
    if let Some(supplied) = input.parameters {
        let matched = compare_parameters(
            facts.parameters(),
            supplied,
            &input.occurrence,
            &mut |record| {
                diagnostics.push(record);
            },
        );
        if !matched {
            blocked = true;
        }
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
        extraction_map: extraction_map(input, &analysis, limits)?,
    }))
}

/// Return the map from emitted MF2 bytes to wherever the caller can point.
///
/// Without a host map the extraction map is already final, because the
/// supplied text is as far back as this crate can see.
fn extraction_map(
    input: &DeclarationInput<'_>,
    analysis: &crate::message::MessageAnalysis,
    limits: &AuthoringLimits,
) -> Result<Box<[ExtractionSegment]>, AuthoringFailure> {
    let Some(map) = input.input_map else {
        return Ok(analysis.extraction_map().to_vec().into_boxed_slice());
    };
    compose_extraction_map(
        analysis.extraction_map(),
        map,
        input.message.text(),
        &input.occurrence,
        limits,
    )
    .map(Vec::into_boxed_slice)
    .map_err(input_map_failure)
}

/// Report a composition failure as the kind of failure it actually is.
///
/// A named bound reports as that bound whichever path exhausted it. A host
/// matching on the failure to name which limit it hit would otherwise have to
/// know whether it happened to supply a map.
const fn input_map_failure(failure: MappingError) -> AuthoringFailure {
    match failure {
        MappingError::Limit(kind) => AuthoringFailure::Limit(kind),
        other => AuthoringFailure::InputMap(other),
    }
}

/// Report displayed text that MF2 pattern text cannot carry.
///
/// Returns `None` only if the reported position cannot be expressed as a
/// range, which would mean the encoder and this crate disagree about the text.
fn unrepresentable(input: &DeclarationInput<'_>, offset: u64) -> Option<Diagnostic> {
    // U+0000 is the one scalar the encoder rejects, and it is one byte, so the
    // reported position names exactly the character an author has to remove.
    let range = ByteRange::new(offset, offset + 1).ok()?;
    Some(
        authoring(
            Stage::MessageAnalysis,
            ReasonFamily::AuthoringFormUnsupported,
            input,
        )
        .with_detail(detail::unrepresentable_scalar())
        .with_message_range(MessageRange::Supplied(range)),
    )
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
                diagnostics.push(
                    authoring(
                        Stage::ContextResolution,
                        ReasonFamily::AuthoringSourceLocaleInvalid,
                        input,
                    )
                    .with_detail(detail::source_locale_rejected()),
                );
                Ok(None)
            }
        };
    }
    if let Some(default) = context.default_source_locale() {
        return Ok(Some((default.clone(), SourceLocaleBasis::ContextDefault)));
    }
    diagnostics.push(
        authoring(
            Stage::ContextResolution,
            ReasonFamily::AuthoringSourceLocaleMissing,
            input,
        )
        .with_detail(detail::source_locale_absent()),
    );
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
        candidate => {
            // Absent and unknown are different mistakes with different fixes:
            // one needs an assignment, the other needs the right vocabulary.
            let detail = if candidate.is_some() {
                detail::surface_class_unknown()
            } else {
                detail::surface_class_absent()
            };
            diagnostics.push(
                authoring(
                    Stage::ContextResolution,
                    ReasonFamily::AuthoringSurfaceClassInvalid,
                    input,
                )
                .with_detail(detail),
            );
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
        diagnostics.push(
            authoring(
                Stage::ContextResolution,
                ReasonFamily::AuthoringMetadataInvalid,
                input,
            )
            .with_detail(detail::usage_profile_unregistered()),
        );
        return Err(());
    };
    let Ok(value) = admitted_text(value, limits) else {
        diagnostics.push(
            authoring(
                Stage::ContextResolution,
                ReasonFamily::AuthoringMetadataInvalid,
                input,
            )
            .with_detail(detail::metadata_value_invalid()),
        );
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
    diagnostics.push(
        authoring(
            Stage::ContextResolution,
            ReasonFamily::AuthoringMetadataInvalid,
            input,
        )
        .with_detail(detail::metadata_value_invalid()),
    );
    Err(())
}

fn admitted_text(value: &str, limits: &AuthoringLimits) -> Result<NonemptyText, ()> {
    if value.len() as u64 > limits.metadata_value_bytes {
        return Err(());
    }
    NonemptyText::from_validated(value).map_err(|_| ())
}

/// Compare what a message requires with what one use site supplies.
///
/// A Producer calls this for a reference, whose declaration was analyzed
/// separately and may be in another statement or another function. The use
/// site attached to a declaration goes through the same comparison, so one
/// message cannot acquire different parameter rules according to which syntax
/// carried it.
///
/// Missing, extra, and duplicate names are separate records because they have
/// different fixes. `occurrence` is the site that supplied the parameters, so
/// a reference reports at the reference rather than at the declaration it
/// shares with every other use.
///
/// Records go to `report` one at a time rather than into a returned list, so a
/// caller's own bound decides how many are retained. One use site can supply
/// any number of parameters, and materialising a record for each one before
/// any budget is consulted is the allocation a bounded collector exists to
/// prevent. For the same reason the return value says whether the use site
/// matched: a caller whose sink stopped accepting records cannot learn that by
/// counting what it received.
pub fn compare_parameters(
    required: &[String],
    supplied: &[ParameterBinding],
    occurrence: &Occurrence,
    report: &mut impl FnMut(Diagnostic),
) -> bool {
    let mismatch = |detail: Detail| {
        Diagnostic::new(
            Stage::ContextResolution,
            DiagnosticOrigin::Authoring(ReasonFamily::AuthoringParameterMismatch),
            Severity::Error,
            occurrence.clone(),
        )
        .with_detail(detail)
    };

    // Nothing bounds how many parameters a use site supplies, so membership is
    // answered by ordered sets rather than by scanning a list for each name.
    // The records still come out in the order a reader expects: supplied
    // problems in host evaluation order, then missing names in the order the
    // message requires them.
    let wanted: BTreeSet<&str> = required.iter().map(String::as_str).collect();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut matched = true;
    for binding in supplied {
        if !seen.insert(binding.name()) {
            matched = false;
            report(
                mismatch(detail::parameter_duplicate())
                    .with_parameter(binding.name())
                    .with_related(vec![binding.expression().clone()]),
            );
        } else if !wanted.contains(binding.name()) {
            matched = false;
            report(
                mismatch(detail::parameter_extra())
                    .with_parameter(binding.name())
                    .with_related(vec![binding.expression().clone()]),
            );
        }
    }
    for name in required {
        if !seen.contains(name.as_str()) {
            matched = false;
            report(mismatch(detail::parameter_missing()).with_parameter(name));
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
    fn a_mismatch_is_answered_by_the_return_value_not_by_what_survived_the_bound() {
        // Records arrive at the caller's sink one at a time, so a bound can
        // drop most of them. Whether the use site matched therefore cannot be
        // read from how many records the caller kept, which is why it is the
        // return value. That the sink is also what limits peak allocation is
        // structural and not observable from here; the assertion below pins
        // the part that is.
        let occurrence = diagnostic()
            .occurrence()
            .expect("a classified site")
            .clone();
        let supplied: Vec<ParameterBinding> = (0..1_000)
            .map(|index| ParameterBinding::new(&format!("extra{index}"), occurrence.clone()))
            .collect();

        let mut sink = DiagnosticSink::new(3);
        let matched = compare_parameters(&[], &supplied, &occurrence, &mut |record| {
            sink.push(record);
        });

        assert!(
            !matched,
            "a thousand unusable names is a mismatch even when three were kept"
        );
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

/// The seams the observational harness measures at.
///
/// These exist only under the non-default `benchmark` feature and add no
/// behaviour: each one composes the same private helpers `resolve_one` uses, so
/// what is measured is the ordinary operation rather than a copy of it.
#[cfg(feature = "benchmark")]
pub(crate) mod measured {
    use super::{
        compare_parameters, extraction_map, intent_projection, resolve_description, resolve_locale,
        resolve_surface_class, resolve_usage, AuthoringContext, AuthoringFailure, AuthoringLimits,
        DeclarationFacts, DeclarationInput, DiagnosticSink, SourceLocaleBasis, Usage,
    };
    use crate::context::CanonicalLocale;
    use crate::message::MessageAnalysis;
    use crate::projection::intent_revision;
    use intlify_shared_json::token::IntegrityDigest;

    /// Everything the context contributes to one declaration.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) struct ContextFacts {
        pub(crate) locale: CanonicalLocale,
        pub(crate) basis: SourceLocaleBasis,
        pub(crate) surface_class: String,
        pub(crate) usage: Option<Usage>,
        pub(crate) description: Option<String>,
    }

    /// Resolve the context-owned facts, or report that the declaration is blocked.
    ///
    /// The diagnostic count is returned rather than the diagnostics themselves:
    /// the harness observes that the same fixture produced the same number of
    /// the same kind, not the reporting text.
    pub(crate) fn context_facts(
        context: &dyn AuthoringContext,
        input: &DeclarationInput<'_>,
        limits: &AuthoringLimits,
    ) -> Result<Result<ContextFacts, usize>, AuthoringFailure> {
        let mut diagnostics = DiagnosticSink::new(limits.diagnostics);
        let locale = resolve_locale(context, input, &mut diagnostics)?;
        let class = resolve_surface_class(context, input, &mut diagnostics);
        let usage = resolve_usage(context, input, limits, &mut diagnostics);
        let description = resolve_description(input, limits, &mut diagnostics);
        let reported = diagnostics.into_reported().len();
        let (Some((locale, basis)), Some(surface_class), Ok(usage), Ok(description)) =
            (locale, class, usage, description)
        else {
            return Ok(Err(reported));
        };
        Ok(Ok(ContextFacts {
            locale,
            basis,
            surface_class,
            usage,
            description,
        }))
    }

    /// Assemble the immutable facts of one declaration whose inputs are admitted.
    ///
    /// The revision is taken here because it is what an authoring result is
    /// for: a consumer that holds the facts without it cannot yet say whether
    /// a translation stays valid. It is the same public function the ordinary
    /// path calls, not a second implementation.
    pub(crate) fn declaration_facts(
        input: &DeclarationInput<'_>,
        analysis: &MessageAnalysis,
        context: &ContextFacts,
        limits: &AuthoringLimits,
    ) -> Result<Result<(DeclarationFacts, IntegrityDigest), usize>, AuthoringFailure> {
        let Some(message) = analysis.facts() else {
            return Ok(Err(0));
        };
        if let Some(supplied) = input.parameters {
            let mut reported = 0_usize;
            let matched = compare_parameters(
                message.parameters(),
                supplied,
                &input.occurrence,
                &mut |_| {
                    reported += 1;
                },
            );
            if !matched {
                return Ok(Err(reported));
            }
        }
        let projection = intent_projection(
            message.message().clone(),
            message.parameters().to_vec().into_boxed_slice(),
            context.locale.as_str(),
            context.usage.clone(),
            context.description.as_deref(),
        )
        .map_err(AuthoringFailure::ContextInvalid)?;
        let revision = intent_revision(&projection).map_err(|_| {
            AuthoringFailure::ContextInvalid(crate::primitives::PrimitiveError::InvalidToken)
        })?;
        Ok(Ok((
            DeclarationFacts {
                occurrence: input.occurrence.clone(),
                mf2_source: analysis.mf2_source().to_owned(),
                projection,
                source_locale_basis: context.basis,
                surface_class: context.surface_class.clone(),
                extraction_map: extraction_map(input, analysis, limits)?,
            },
            revision,
        )))
    }
}
