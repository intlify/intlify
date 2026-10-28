// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015 Phases 7 and 8 over the selected declaration of a complete
//! configuration: every locale spelling counted, canonicalized through the
//! finite provider or the ICU4X provider, and checked, then the negotiation,
//! message fallback, and coverage policies resolved from what Phase 7 left,
//! under a fixture-owned confirmed policy and fixture-owned specifications and
//! vocabulary. Neither the result nor its observation is a Profile or a shared
//! artifact.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::locale::{
    CanonicalLocale, CanonicalizationFailure, Canonicalizer, Provider, ProviderFailure,
    VersionedIdentity,
};
use crate::model::{CoverageMode, ProfileId};
use crate::references::TargetProfileReference;
use crate::resolver::coverage::{
    resolve_coverage, CoveragePolicy, CoverageSubject, Decision, Domain, RuleContent, RuleMember,
    SurfaceKey,
};
use crate::resolver::evaluation::{
    Actual, Admitted, Causes, Check, Evaluation, IssueKey, LimitExceeded, Status, Subject, Unit,
};
use crate::resolver::fallback::{
    resolve_fallback, Candidate, FallbackSubject, MessageFallbackPolicy,
};
use crate::resolver::limits::ConfirmedPolicy;
use crate::resolver::locales::{
    resolve_locales, AliasLocales, Content, FallbackLocale, FallbackLocales, LocaleFields,
    LocaleInvariant, LocaleKey, LocaleSet, LocaleSubject, Location, ProjectLocales,
    ResolvedLocales, TargetLocales,
};
use crate::resolver::negotiation::{resolve_negotiation, NegotiationProfile, NegotiationSubject};
use crate::resolver::vocabulary::SurfaceVocabulary;

use super::locale;
use super::locale_icu4x::PreparedEngine;
use super::observation::{Frame, Observation};
use super::operation::{Analysis, Config, OutputFailure};
use super::quantity::Quantity;

/// The canonicalizer Phases 7 and 8 run under: the finite provider's, or the
/// ICU4X provider's engine, which a case reuses or admits fresh before each
/// invocation.
pub(super) enum Canonicalizers {
    Finite(Arc<locale::Core>),
    Icu4x(PreparedEngine),
}

/// The members of the formal test vocabulary,
/// `fixtures/vocabulary/vectors.json`'s `fixture`.
pub(super) const VOCABULARY: [&str; 3] = ["checkout", "internal-tool", "nav"];

/// What Phase 5 hands Phase 8, stated by the fixture owner: the Locale
/// Negotiation, Message Locale Fallback, and Coverage Specifications at
/// revision "0", and the Intent Surface-Class Vocabulary.
pub(super) struct PolicyInputs {
    pub(super) negotiation: Admitted<VersionedIdentity<&'static str>>,
    pub(super) fallback: Admitted<VersionedIdentity<&'static str>>,
    pub(super) coverage: Admitted<VersionedIdentity<&'static str>>,
    pub(super) vocabulary: SurfaceVocabulary,
}

impl PolicyInputs {
    pub(super) fn fixture_owned() -> Self {
        let revision_zero = |identity| {
            Ok(VersionedIdentity {
                identity,
                revision: "0",
            })
        };
        Self {
            negotiation: revision_zero("intlify-locale-negotiation"),
            fallback: revision_zero("intlify-message-locale-fallback"),
            coverage: revision_zero("intlify-localization-coverage"),
            vocabulary: SurfaceVocabulary::fixture_owned(&VOCABULARY),
        }
    }
}

/// A case's complete configuration, its selected declaration, and the policy,
/// Phase 5 results, and canonicalizer Phases 7 and 8 run under.
pub(super) struct PreparedLocales {
    pub(super) analysis: Analysis,
    pub(super) config: Config,
    pub(super) selected: ProfileId,
    pub(super) policy: ConfirmedPolicy,
    pub(super) inputs: PolicyInputs,
    pub(super) canonicalizer: Canonicalizers,
}

impl PreparedLocales {
    /// The finite provider's canonicalizer, when the case runs under it.
    pub(super) const fn finite(&self) -> Option<&Arc<locale::Core>> {
        match &self.canonicalizer {
            Canonicalizers::Finite(core) => Some(core),
            Canonicalizers::Icu4x(_) => None,
        }
    }

    /// The ICU4X provider's engine, when the case runs under it.
    pub(super) const fn icu4x(&self) -> Option<&PreparedEngine> {
        match &self.canonicalizer {
            Canonicalizers::Icu4x(engine) => Some(engine),
            Canonicalizers::Finite(_) => None,
        }
    }

    /// The selected declaration's locale fields, borrowed before the interval.
    pub(super) fn fields(&self) -> Option<LocaleFields<'_, TargetProfileReference>> {
        if !self.analysis.is_complete() {
            return None;
        }
        self.config
            .profiles()
            .get(&self.selected)
            .map(LocaleFields::declared)
    }
}

/// What Phases 7 and 8 resolved: the locales, then each policy or the causes
/// it is unavailable.
pub(super) struct Policies {
    pub(super) locales: ResolvedLocales,
    pub(super) negotiation: Admitted<NegotiationProfile<&'static str>>,
    pub(super) fallback: Admitted<MessageFallbackPolicy<&'static str>>,
    pub(super) coverage: Admitted<CoveragePolicy<&'static str>>,
}

/// Phases 7 and 8's record of every unit they ran and what they resolved:
/// the complete ordinary output.
pub(super) struct Resolved {
    pub(super) evaluation: Evaluation,
    pub(super) result: Result<Policies, LocaleInvariant>,
}

/// Phases 7 and 8 over a fresh evaluation: what preparation and every timed
/// invocation run, through either provider.
pub(super) fn resolve<P: Provider>(
    policy: &ConfirmedPolicy,
    inputs: &PolicyInputs,
    canonicalizer: &Canonicalizer<P>,
    fields: &LocaleFields<'_, TargetProfileReference>,
) -> Resolved {
    let mut evaluation = Evaluation::default();
    let result = policies(&mut evaluation, policy, inputs, canonicalizer, fields);
    Resolved { evaluation, result }
}

/// Phase 7, then Phase 8's negotiation, message fallback, and coverage, in
/// the order the resolver's entry runs them.
fn policies<P: Provider>(
    evaluation: &mut Evaluation,
    policy: &ConfirmedPolicy,
    inputs: &PolicyInputs,
    canonicalizer: &Canonicalizer<P>,
    fields: &LocaleFields<'_, TargetProfileReference>,
) -> Result<Policies, LocaleInvariant> {
    let locales = resolve_locales(evaluation, Ok(policy), Ok(canonicalizer), fields)?;
    let negotiation = resolve_negotiation(evaluation, Ok(policy), &inputs.negotiation, &locales)?;
    let fallback = resolve_fallback(evaluation, Ok(policy), &inputs.fallback, &locales)?;
    let coverage = resolve_coverage(
        evaluation,
        Ok(policy),
        &inputs.coverage,
        Ok(&inputs.vocabulary),
        Ok(canonicalizer),
        &locales,
        fields.coverage.clone(),
    )?;
    Ok(Policies {
        locales,
        negotiation,
        fallback,
        coverage,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct InputFacts {
    scope: String,
    selected_profile: String,
    policy: String,
    locale_occurrence_limit: Quantity,
    occurrence_domain: [String; 10],
    requested_locale_limit: Quantity,
    requested_cardinality_domain: String,
    identifier_byte_limit: Quantity,
    negotiation_alias_limit: Quantity,
    fallback_source_limit: Quantity,
    fallback_candidates_per_source_limit: Quantity,
    coverage_rule_limit: Quantity,
    coverage_locale_selector_occurrence_limit: Quantity,
    coverage_surface_selector_occurrence_limit: Quantity,
    coverage_decision_cell_limit: Quantity,
    coverage_rule_cell_comparison_limit: Quantity,
    specifications: Vec<String>,
    vocabulary: Vec<String>,
    prerequisites: String,
    input_storage: String,
    structural_preparation: String,
    ordinary_output: String,
}

impl InputFacts {
    pub(super) fn observe(prepared: &PreparedLocales) -> Self {
        let limits = &prepared.policy.limits().locale_resolution;
        let inputs = &prepared.inputs;
        Self {
            scope: "selected-declaration-every-locale-occurrence-and-locale-policy".into(),
            selected_profile: prepared.selected.as_str().into(),
            policy: "fixture-owned-confirmed-policy-not-an-admitted-resource-limit-policy".into(),
            locale_occurrence_limit: Quantity::new(limits.max_locale_occurrences.get()),
            // 015's maxLocaleOccurrences roles, in its order.
            occurrence_domain: [
                "project-requested-locales-item".into(),
                "present-default-source-locale".into(),
                "project-default-requested-locale".into(),
                "negotiation-alias-key".into(),
                "negotiation-alias-value".into(),
                "message-fallback-key".into(),
                "message-fallback-literal-candidate".into(),
                "coverage-requested-locale-selector-item".into(),
                "target-requested-locales-item".into(),
                "present-target-default-requested-locale".into(),
            ],
            requested_locale_limit: Quantity::new(limits.max_requested_locales.get()),
            requested_cardinality_domain: "unique-canonical-project-requested-locales".into(),
            identifier_byte_limit: Quantity::new(limits.max_locale_identifier_bytes.get()),
            negotiation_alias_limit: Quantity::new(limits.max_negotiation_aliases.get()),
            fallback_source_limit: Quantity::new(limits.max_fallback_sources.get()),
            fallback_candidates_per_source_limit: Quantity::new(
                limits.max_fallback_candidates_per_source.get(),
            ),
            coverage_rule_limit: Quantity::new(limits.max_coverage_rules.get()),
            coverage_locale_selector_occurrence_limit: Quantity::new(
                limits.max_coverage_locale_selector_occurrences.get(),
            ),
            coverage_surface_selector_occurrence_limit: Quantity::new(
                limits.max_coverage_surface_selector_occurrences.get(),
            ),
            coverage_decision_cell_limit: Quantity::new(limits.max_coverage_decision_cells.get()),
            coverage_rule_cell_comparison_limit: Quantity::new(
                limits.max_coverage_rule_cell_comparisons.get(),
            ),
            // The admitted assertions, in the slots' order.
            specifications: [&inputs.negotiation, &inputs.fallback, &inputs.coverage]
                .into_iter()
                .filter_map(|admitted| admitted.as_ref().ok())
                .map(|specification| {
                    format!("{}/{}", specification.identity, specification.revision)
                })
                .collect(),
            vocabulary: inputs
                .vocabulary
                .members()
                .iter()
                .map(|member| member.as_str().into())
                .collect(),
            prerequisites:
                "admitted-policy-specifications-vocabulary-canonicalizer-and-whole-declaration"
                    .into(),
            input_storage: "borrowed-selected-fields-of-resident-complete-config".into(),
            structural_preparation: "complete-model-selection-and-borrowed-fields-before-interval"
                .into(),
            ordinary_output:
                "fresh-evaluation-with-resolved-locales-and-locale-policies-or-resolver-defect"
                    .into(),
        }
    }
}

/// A resolver defect, or a spelling the provider cannot answer, is no blocked
/// outcome: the case fails rather than completing as an expected failure.
/// Logical work uses this screen too.
pub(super) fn require_observable(resolved: &Resolved) -> Result<&Policies, OutputFailure> {
    match &resolved.result {
        Ok(policies) => Ok(policies),
        Err(LocaleInvariant::Canonicalization(CanonicalizationFailure::Provider(
            ProviderFailure::UnsupportedInput | ProviderFailure::Unavailable,
        ))) => Err(OutputFailure::LocaleProviderUnavailable),
        Err(LocaleInvariant::Canonicalization(_)) => Err(OutputFailure::LocaleProviderInvariant),
        Err(LocaleInvariant::Evaluation(_)) => Err(OutputFailure::LocalePolicyResolutionInvariant),
    }
}

fn count(value: usize) -> Result<u64, OutputFailure> {
    u64::try_from(value).map_err(|_| OutputFailure::LocalePolicyResolutionInvariant)
}

fn key(frame: &mut Frame, key: &LocaleKey) {
    match key {
        LocaleKey::Canonical(locale) => {
            frame.uint(0);
            frame.text(locale.as_str());
        }
        LocaleKey::Content(content) => {
            frame.uint(1);
            frame.text(content.as_str());
        }
    }
}

fn subject(frame: &mut Frame, subject: &LocaleSubject) -> Result<(), OutputFailure> {
    match subject {
        LocaleSubject::Occurrences => frame.uint(0),
        LocaleSubject::RequestedLocale(own) => {
            frame.uint(1);
            key(frame, own);
        }
        LocaleSubject::RequestedLocales => frame.uint(2),
        LocaleSubject::DefaultSourceLocale(own) => {
            frame.uint(3);
            key(frame, own);
        }
        LocaleSubject::DefaultRequestedLocale(own) => {
            frame.uint(4);
            key(frame, own);
        }
        LocaleSubject::NegotiationAliasKey(own) => {
            frame.uint(5);
            key(frame, own);
        }
        LocaleSubject::NegotiationAliasValue(owner, own) => {
            frame.uint(6);
            key(frame, owner);
            key(frame, own);
        }
        LocaleSubject::MessageFallbackKey(own) => {
            frame.uint(7);
            key(frame, own);
        }
        LocaleSubject::MessageFallbackCandidate(owner, position, own) => {
            frame.uint(8);
            key(frame, owner);
            frame.uint(count(*position)?);
            key(frame, own);
        }
        LocaleSubject::TargetRequestedLocale(id, own) => {
            frame.uint(9);
            frame.text(id.as_str());
            key(frame, own);
        }
        LocaleSubject::TargetDefaultRequestedLocale(id, own) => {
            frame.uint(10);
            frame.text(id.as_str());
            key(frame, own);
        }
    }
    Ok(())
}

fn negotiation_subject(frame: &mut Frame, subject: &NegotiationSubject) {
    match subject {
        NegotiationSubject::Aliases => frame.uint(0),
        NegotiationSubject::Key(own) => {
            frame.uint(1);
            key(frame, own);
        }
        NegotiationSubject::Destination(owner, own) => {
            frame.uint(2);
            key(frame, owner);
            key(frame, own);
        }
    }
}

fn fallback_subject(frame: &mut Frame, subject: &FallbackSubject) {
    match subject {
        FallbackSubject::Sources => frame.uint(0),
        FallbackSubject::Source(own) => {
            frame.uint(1);
            key(frame, own);
        }
        FallbackSubject::Literal(owner, own) => {
            frame.uint(2);
            key(frame, owner);
            key(frame, own);
        }
        FallbackSubject::IntentSource(owner) => {
            frame.uint(3);
            key(frame, owner);
        }
    }
}

fn mode(frame: &mut Frame, mode: CoverageMode) {
    frame.uint(match mode {
        CoverageMode::DirectRequired => 0,
        CoverageMode::FallbackAllowed => 1,
    });
}

fn contents(frame: &mut Frame, items: &[Content]) -> Result<(), OutputFailure> {
    frame.uint(count(items.len())?);
    for item in items {
        frame.text(item.as_str());
    }
    Ok(())
}

/// A rule as authored, which names it until it has a normalized domain.
fn rule(frame: &mut Frame, rule: &RuleContent) -> Result<(), OutputFailure> {
    frame.uint(count(rule.members().len())?);
    for member in rule.members() {
        match member {
            RuleMember::IntentSurfaceClasses(items) => {
                frame.uint(0);
                contents(frame, items)?;
            }
            RuleMember::Mode(own) => {
                frame.uint(1);
                mode(frame, *own);
            }
            RuleMember::RequestedLocales(items) => {
                frame.uint(2);
                contents(frame, items)?;
            }
        }
    }
    Ok(())
}

fn domain(frame: &mut Frame, domain: &Domain) -> Result<(), OutputFailure> {
    frame.uint(count(domain.locales().len())?);
    for locale in domain.locales() {
        frame.text(locale.as_str());
    }
    frame.uint(count(domain.surfaces().len())?);
    for surface in domain.surfaces() {
        frame.text(surface.as_str());
    }
    Ok(())
}

fn coverage_subject(frame: &mut Frame, subject: &CoverageSubject) -> Result<(), OutputFailure> {
    match subject {
        CoverageSubject::Rules => frame.uint(0),
        CoverageSubject::LocaleSelectors => frame.uint(1),
        CoverageSubject::SurfaceSelectors => frame.uint(2),
        CoverageSubject::Locale(owner, own) => {
            frame.uint(3);
            rule(frame, owner)?;
            key(frame, own);
        }
        CoverageSubject::SurfaceClass(owner, own) => {
            frame.uint(4);
            rule(frame, owner)?;
            match own {
                SurfaceKey::Member(member) => {
                    frame.uint(0);
                    frame.text(member.as_str());
                }
                SurfaceKey::Content(content) => {
                    frame.uint(1);
                    frame.text(content.as_str());
                }
            }
        }
        CoverageSubject::Domain(own) => {
            frame.uint(5);
            domain(frame, own)?;
        }
        CoverageSubject::Cells => frame.uint(6),
        CoverageSubject::Comparisons => frame.uint(7),
        CoverageSubject::Cell(locale, surface) => {
            frame.uint(8);
            frame.text(locale.as_str());
            frame.text(surface.as_str());
        }
    }
    Ok(())
}

/// Only Phase 7's and Phase 8's own units can appear: every prerequisite of
/// another phase was admitted.
fn unit(frame: &mut Frame, unit: &Unit) -> Result<(), OutputFailure> {
    frame.text(unit.phase.id());
    match &unit.subject {
        Subject::Locale(own) => {
            frame.uint(0);
            subject(frame, own)?;
        }
        Subject::Negotiation(own) => {
            frame.uint(1);
            negotiation_subject(frame, own);
        }
        Subject::Fallback(own) => {
            frame.uint(2);
            fallback_subject(frame, own);
        }
        Subject::Coverage(own) => {
            frame.uint(3);
            coverage_subject(frame, own)?;
        }
        Subject::Slot(_)
        | Subject::Vocabulary
        | Subject::CanonicalizationData
        | Subject::ResourcePolicy(_)
        | Subject::DeclarationField(_)
        | Subject::ArtifactCollection(_)
        | Subject::ArtifactReference(_)
        | Subject::DeliveryPlacement => return Err(OutputFailure::LocalePolicyResolutionInvariant),
    }
    frame.uint(match unit.check {
        Check::WithinBound => 0,
        Check::CanonicalLocale => 1,
        Check::UniqueLocale => 2,
        Check::RequestedMember => 3,
        Check::DistinctFromSource => 4,
        Check::UniqueCandidate => 5,
        Check::VocabularyMember => 6,
        Check::UniqueSurfaceClass => 7,
        Check::UniqueDomain => 8,
        Check::EffectiveMode => 9,
        Check::SupportedAssertion
        | Check::MatchingData
        | Check::IntactArtifact
        | Check::AssertedArtifact
        | Check::OpaqueIdentity
        | Check::WithinCapability
        | Check::BootstrapCandidate
        | Check::CompatiblePolicy
        | Check::AdmissibleMember
        | Check::SingleOccurrence
        | Check::ConsistentContent
        | Check::ResolvedReference => return Err(OutputFailure::LocalePolicyResolutionInvariant),
    });
    Ok(())
}

fn issue(frame: &mut Frame, key: &IssueKey) -> Result<(), OutputFailure> {
    unit(frame, key.unit())?;
    frame.text(key.code().as_str());
    frame.flag(key.reason().is_some());
    if let Some(reason) = key.reason() {
        frame.text(reason.as_str());
    }
    Ok(())
}

fn causes(frame: &mut Frame, causes: &Causes) -> Result<(), OutputFailure> {
    let keys = causes.keys().collect::<Vec<_>>();
    frame.uint(count(keys.len())?);
    for key in keys {
        issue(frame, key)?;
    }
    Ok(())
}

fn actual(frame: &mut Frame, actual: Actual) {
    match actual {
        Actual::Exact(value) => {
            frame.uint(0);
            frame.uint(value);
        }
        Actual::AtLeast(value) => {
            frame.uint(1);
            frame.uint(value);
        }
    }
}

fn limit(frame: &mut Frame, exceeded: Option<LimitExceeded>) {
    frame.flag(exceeded.is_some());
    if let Some(exceeded) = exceeded {
        frame.text(exceeded.bound.group.path());
        frame.text(exceeded.bound.name);
        frame.uint(exceeded.limit.get());
        actual(frame, exceeded.actual);
    }
}

/// A value Phase 7 or 8 resolved, or the causes it is unavailable.
fn admitted<T>(
    frame: &mut Frame,
    value: &Admitted<T>,
    present: impl FnOnce(&mut Frame, &T) -> Result<(), OutputFailure>,
) -> Result<(), OutputFailure> {
    match value {
        Ok(value) => {
            frame.uint(0);
            present(frame, value)
        }
        Err(why) => {
            frame.uint(1);
            causes(frame, why)
        }
    }
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "every value framer has one signature"
)]
fn locale(frame: &mut Frame, locale: &CanonicalLocale) -> Result<(), OutputFailure> {
    frame.text(locale.as_str());
    Ok(())
}

fn set(frame: &mut Frame, set: &LocaleSet) -> Result<(), OutputFailure> {
    frame.uint(count(set.as_slice().len())?);
    for member in set.as_slice() {
        frame.text(member.as_str());
    }
    Ok(())
}

fn project(frame: &mut Frame, project: &ProjectLocales) -> Result<(), OutputFailure> {
    frame.flag(project.source_default().is_some());
    if let Some(source) = project.source_default() {
        frame.text(source.as_str());
    }
    set(frame, project.requested())?;
    frame.text(project.requested_default().as_str());
    Ok(())
}

fn aliases(frame: &mut Frame, aliases: &[AliasLocales]) -> Result<(), OutputFailure> {
    frame.uint(count(aliases.len())?);
    for alias in aliases {
        admitted(frame, &alias.key.locale, locale)?;
        admitted(frame, &alias.value.locale, locale)?;
    }
    Ok(())
}

fn fallback(frame: &mut Frame, sources: &[FallbackLocales]) -> Result<(), OutputFailure> {
    frame.uint(count(sources.len())?);
    for source in sources {
        admitted(frame, &source.key.locale, locale)?;
        frame.uint(count(source.candidates.len())?);
        for candidate in &source.candidates {
            match candidate {
                FallbackLocale::Literal(literal) => {
                    frame.uint(0);
                    admitted(frame, &literal.locale, locale)?;
                }
                FallbackLocale::IntentSource => frame.uint(1),
            }
        }
    }
    Ok(())
}

fn targets(frame: &mut Frame, targets: &[TargetLocales]) -> Result<(), OutputFailure> {
    frame.uint(count(targets.len())?);
    for target in targets {
        frame.text(target.id.as_str());
        admitted(frame, &target.requested, set)?;
        admitted(frame, &target.default, |frame, default| {
            frame.flag(default.is_some());
            if let Some(default) = default {
                frame.text(default.as_str());
            }
            Ok(())
        })?;
    }
    Ok(())
}

fn specification(frame: &mut Frame, specification: &VersionedIdentity<&'static str>) {
    frame.text(specification.identity);
    frame.text(specification.revision);
}

fn negotiation(
    frame: &mut Frame,
    profile: &NegotiationProfile<&'static str>,
) -> Result<(), OutputFailure> {
    specification(frame, profile.specification());
    frame.uint(count(profile.aliases().len())?);
    for (key, destination) in profile.aliases() {
        frame.text(key.as_str());
        frame.text(destination.as_str());
    }
    Ok(())
}

fn message_fallback(
    frame: &mut Frame,
    policy: &MessageFallbackPolicy<&'static str>,
) -> Result<(), OutputFailure> {
    specification(frame, policy.specification());
    frame.uint(count(policy.sequences().len())?);
    for (source, candidates) in policy.sequences() {
        frame.text(source.as_str());
        frame.uint(count(candidates.len())?);
        for candidate in candidates {
            match candidate {
                Candidate::Literal(literal) => {
                    frame.uint(0);
                    frame.text(literal.as_str());
                }
                Candidate::IntentSource => frame.uint(1),
            }
        }
    }
    Ok(())
}

/// The decision table with each cell's Coverage Decision Basis: Shared
/// reporting, since a Basis names no source.
fn coverage(frame: &mut Frame, policy: &CoveragePolicy<&'static str>) -> Result<(), OutputFailure> {
    specification(frame, policy.specification());
    frame.uint(count(policy.basis().len())?);
    for ((locale, surface), basis) in policy.basis() {
        frame.text(locale.as_str());
        frame.text(surface.as_str());
        mode(frame, basis.mode);
        match &basis.decision {
            Decision::Default => frame.uint(0),
            Decision::Rules(domains) => {
                frame.uint(1);
                frame.uint(count(domains.len())?);
                for own in domains {
                    domain(frame, own)?;
                }
            }
        }
    }
    Ok(())
}

fn location(frame: &mut Frame, location: &Location) -> Result<(), OutputFailure> {
    match location {
        Location::RequestedLocale(index) => {
            frame.uint(0);
            frame.uint(count(*index)?);
        }
        Location::DefaultSourceLocale => frame.uint(1),
        Location::DefaultRequestedLocale => frame.uint(2),
        Location::NegotiationAliasKey(rank) => {
            frame.uint(3);
            frame.uint(count(*rank)?);
        }
        Location::NegotiationAliasValue(rank) => {
            frame.uint(4);
            frame.uint(count(*rank)?);
        }
        Location::MessageFallbackKey(rank) => {
            frame.uint(5);
            frame.uint(count(*rank)?);
        }
        Location::MessageFallbackCandidate(rank, position) => {
            frame.uint(6);
            frame.uint(count(*rank)?);
            frame.uint(count(*position)?);
        }
        Location::TargetRequestedLocale(id, index) => {
            frame.uint(7);
            frame.text(id.as_str());
            frame.uint(count(*index)?);
        }
        Location::TargetDefaultRequestedLocale(id) => {
            frame.uint(8);
            frame.text(id.as_str());
        }
    }
    Ok(())
}

/// The shared domain is what Phases 7 and 8 decided, none of it positional:
/// every unit's status, the blocking results with their details, the
/// suggestions, the occurrence total, the values Phase 7 hands Phase 8, and
/// Phase 8's three policies. The entry domain is where each spelling sits, its
/// subject, and whether it was corrected, so reordering a set changes the
/// entry domain alone. Phase 8 names every subject by content, so it adds
/// nothing to the entry domain.
pub(super) fn observe(resolved: &Resolved) -> Result<Observation, OutputFailure> {
    let policies = require_observable(resolved)?;
    let locales = &policies.locales;
    let evaluation = &resolved.evaluation;
    let mut shared = Frame::new("locale-policy-resolution-result");
    shared.uint(count(evaluation.statuses().count())?);
    for (recorded, status) in evaluation.statuses() {
        unit(&mut shared, recorded)?;
        match status {
            Status::Evaluated => shared.uint(0),
            Status::NotEvaluated(why) => {
                shared.uint(1);
                causes(&mut shared, why)?;
            }
        }
    }
    shared.uint(count(evaluation.issues().count())?);
    for key in evaluation.issues() {
        issue(&mut shared, key)?;
        limit(&mut shared, evaluation.limit(key));
    }
    shared.uint(count(evaluation.notices().count())?);
    for key in evaluation.notices() {
        issue(&mut shared, key)?;
    }
    shared.flag(locales.occurrence_total.is_some());
    if let Some(total) = locales.occurrence_total {
        actual(&mut shared, total);
    }
    admitted(&mut shared, &locales.requested_set, set)?;
    admitted(&mut shared, &locales.project, project)?;
    admitted(&mut shared, &locales.negotiation_aliases, |frame, list| {
        aliases(frame, list)
    })?;
    admitted(&mut shared, &locales.message_fallback, |frame, list| {
        fallback(frame, list)
    })?;
    admitted(&mut shared, &locales.targets, |frame, list| {
        targets(frame, list)
    })?;
    admitted(&mut shared, &policies.negotiation, negotiation)?;
    admitted(&mut shared, &policies.fallback, message_fallback)?;
    admitted(&mut shared, &policies.coverage, coverage)?;
    // The entry domain holds Phase 7's spellings alone, so it keeps Phase 7's
    // name.
    let mut entry = Frame::new("locale-resolution-entry");
    entry.uint(count(locales.occurrences.len())?);
    for occurrence in &locales.occurrences {
        location(&mut entry, &occurrence.location)?;
        subject(&mut entry, &occurrence.subject)?;
        entry.flag(occurrence.corrected);
    }
    Ok(Observation {
        shared: shared.finish(),
        entry: Some(entry.finish()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locale::ProviderFailure;
    use crate::resolver::coverage::unit as coverage_unit;
    use crate::resolver::evaluation::EvaluationInvariant;
    use crate::resolver::fallback::unit as fallback_unit;
    use crate::resolver::locales::fixtures::{canonical, content, key};
    use crate::resolver::locales::unit as locale_unit;
    use crate::resolver::negotiation::unit as negotiation_unit;
    use crate::resolver::specification::Slot;

    fn digest(frame_with: impl FnOnce(&mut Frame)) -> super::super::observation::Digest {
        let mut frame = Frame::new("locale-resolution-test");
        frame_with(&mut frame);
        frame.finish()
    }

    fn distinct(digests: &[super::super::observation::Digest]) {
        for (index, left) in digests.iter().enumerate() {
            assert!(
                digests[index + 1..].iter().all(|right| right != left),
                "digest {index} repeats"
            );
        }
    }

    #[test]
    fn a_resolver_defect_or_an_unanswered_spelling_fails_the_case() {
        let defect = |invariant| Resolved {
            evaluation: Evaluation::default(),
            result: Err(invariant),
        };
        let provider =
            |failure| LocaleInvariant::Canonicalization(CanonicalizationFailure::Provider(failure));
        for (invariant, failure) in [
            (
                provider(ProviderFailure::UnsupportedInput),
                OutputFailure::LocaleProviderUnavailable,
            ),
            (
                provider(ProviderFailure::Unavailable),
                OutputFailure::LocaleProviderUnavailable,
            ),
            (
                LocaleInvariant::Canonicalization(CanonicalizationFailure::ProviderOutputInvariant),
                OutputFailure::LocaleProviderInvariant,
            ),
            (
                LocaleInvariant::Evaluation(EvaluationInvariant::UnitRecordedTwice(locale_unit(
                    LocaleSubject::Occurrences,
                    Check::WithinBound,
                ))),
                OutputFailure::LocalePolicyResolutionInvariant,
            ),
        ] {
            let resolved = defect(invariant);
            assert_eq!(require_observable(&resolved).err(), Some(failure));
            assert_eq!(observe(&resolved).err(), Some(failure));
        }
    }

    #[test]
    fn a_witness_frames_apart_from_an_exact_total() {
        let total = |value| digest(|frame| actual(frame, value));
        assert_ne!(total(Actual::Exact(3)), total(Actual::AtLeast(3)));
        assert_ne!(total(Actual::AtLeast(3)), total(Actual::AtLeast(4)));
        assert_ne!(total(Actual::Exact(3)), total(Actual::Exact(4)));
    }

    #[test]
    fn only_phase_7_and_8_units_frame() {
        let mut frame = Frame::new("locale-resolution-test");
        for subject in [
            Subject::Slot(Slot::Coverage),
            Subject::Vocabulary,
            Subject::CanonicalizationData,
            Subject::DeliveryPlacement,
        ] {
            let foreign = Unit {
                subject,
                ..locale_unit(LocaleSubject::Occurrences, Check::WithinBound)
            };
            assert_eq!(
                unit(&mut frame, &foreign),
                Err(OutputFailure::LocalePolicyResolutionInvariant)
            );
        }
        for check in [
            Check::SupportedAssertion,
            Check::MatchingData,
            Check::IntactArtifact,
            Check::AssertedArtifact,
            Check::OpaqueIdentity,
            Check::WithinCapability,
            Check::BootstrapCandidate,
            Check::CompatiblePolicy,
            Check::AdmissibleMember,
            Check::SingleOccurrence,
            Check::ConsistentContent,
            Check::ResolvedReference,
        ] {
            assert_eq!(
                unit(&mut frame, &locale_unit(LocaleSubject::Occurrences, check)),
                Err(OutputFailure::LocalePolicyResolutionInvariant)
            );
        }
        let checks = [
            Check::WithinBound,
            Check::CanonicalLocale,
            Check::UniqueLocale,
            Check::RequestedMember,
            Check::DistinctFromSource,
            Check::UniqueCandidate,
            Check::VocabularyMember,
            Check::UniqueSurfaceClass,
            Check::UniqueDomain,
            Check::EffectiveMode,
        ]
        .map(|check| {
            digest(|frame| {
                unit(frame, &locale_unit(LocaleSubject::Occurrences, check)).unwrap();
            })
        });
        distinct(&checks);
    }

    /// Each Phase 8 subject frames apart from every other, across the three
    /// policies and within each, and apart from Phase 7's.
    #[test]
    fn phase_8_subjects_frame_apart() {
        let rule = |spelling: &str| {
            let rule: crate::model::CoverageRule = serde_json::from_value(serde_json::json!({
                "requestedLocales": [spelling],
                "mode": "fallback-allowed"
            }))
            .unwrap();
            RuleContent::of(&rule)
        };
        let token = |member| intlify_shared_json::token::Token::new(member).unwrap();
        let units = [
            locale_unit(LocaleSubject::Occurrences, Check::WithinBound),
            negotiation_unit(NegotiationSubject::Aliases, Check::WithinBound),
            negotiation_unit(NegotiationSubject::Key(key("en")), Check::UniqueLocale),
            negotiation_unit(NegotiationSubject::Key(content("en")), Check::UniqueLocale),
            negotiation_unit(
                NegotiationSubject::Destination(key("en"), key("ja")),
                Check::RequestedMember,
            ),
            fallback_unit(FallbackSubject::Sources, Check::WithinBound),
            fallback_unit(FallbackSubject::Source(key("en")), Check::UniqueLocale),
            fallback_unit(
                FallbackSubject::Literal(key("en"), key("ja")),
                Check::UniqueCandidate,
            ),
            fallback_unit(
                FallbackSubject::IntentSource(key("en")),
                Check::UniqueCandidate,
            ),
            coverage_unit(CoverageSubject::Rules, Check::WithinBound),
            coverage_unit(CoverageSubject::LocaleSelectors, Check::WithinBound),
            coverage_unit(CoverageSubject::SurfaceSelectors, Check::WithinBound),
            coverage_unit(
                CoverageSubject::Locale(rule("ja"), key("ja")),
                Check::CanonicalLocale,
            ),
            coverage_unit(
                CoverageSubject::Locale(rule("en"), key("ja")),
                Check::CanonicalLocale,
            ),
            coverage_unit(
                CoverageSubject::SurfaceClass(rule("ja"), SurfaceKey::Member(token("nav"))),
                Check::VocabularyMember,
            ),
            coverage_unit(
                CoverageSubject::SurfaceClass(rule("ja"), SurfaceKey::Content(Content::new("nav"))),
                Check::VocabularyMember,
            ),
            coverage_unit(
                CoverageSubject::SurfaceClass(
                    rule("ja"),
                    SurfaceKey::Content(Content::new("navigation")),
                ),
                Check::VocabularyMember,
            ),
            coverage_unit(CoverageSubject::Cells, Check::WithinBound),
            coverage_unit(CoverageSubject::Comparisons, Check::WithinBound),
            coverage_unit(
                CoverageSubject::Cell(canonical("ja"), token("nav")),
                Check::EffectiveMode,
            ),
        ]
        .map(|recorded| digest(|frame| unit(frame, &recorded).unwrap()));
        distinct(&units);
    }
}
