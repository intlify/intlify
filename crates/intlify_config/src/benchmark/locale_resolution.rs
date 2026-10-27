// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015 Phase 7 over the selected declaration of a complete configuration:
//! every locale spelling counted, canonicalized through the finite provider
//! or the ICU4X provider, and checked under a fixture-owned confirmed policy.
//! Neither the result nor its observation is a Profile or a shared artifact.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::locale::{
    CanonicalLocale, CanonicalizationFailure, Canonicalizer, Provider, ProviderFailure,
};
use crate::model::ProfileId;
use crate::references::TargetProfileReference;
use crate::resolver::evaluation::{
    Actual, Admitted, Causes, Check, Evaluation, IssueKey, LimitExceeded, Status, Subject, Unit,
};
use crate::resolver::limits::ConfirmedPolicy;
use crate::resolver::locales::{
    resolve_locales, AliasLocales, FallbackLocale, FallbackLocales, LocaleFields, LocaleInvariant,
    LocaleKey, LocaleSet, LocaleSubject, Location, ProjectLocales, ResolvedLocales, TargetLocales,
};

use super::locale;
use super::locale_icu4x::PreparedEngine;
use super::observation::{Frame, Observation};
use super::operation::{Analysis, Config, OutputFailure};
use super::quantity::Quantity;

/// The canonicalizer Phase 7 runs under: the finite provider's, or the
/// ICU4X provider's engine, which a case reuses or admits fresh before each
/// invocation.
pub(super) enum Canonicalizers {
    Finite(Arc<locale::Core>),
    Icu4x(PreparedEngine),
}

/// A case's complete configuration, its selected declaration, and the policy
/// and canonicalizer Phase 7 runs under.
pub(super) struct PreparedLocales {
    pub(super) analysis: Analysis,
    pub(super) config: Config,
    pub(super) selected: ProfileId,
    pub(super) policy: ConfirmedPolicy,
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

/// Phase 7's record of every unit it ran and what it resolved: the complete
/// ordinary output.
pub(super) struct Resolved {
    pub(super) evaluation: Evaluation,
    pub(super) result: Result<ResolvedLocales, LocaleInvariant>,
}

/// Phase 7 over a fresh evaluation: what preparation and every timed
/// invocation run, through either provider.
pub(super) fn resolve<P: Provider>(
    policy: &ConfirmedPolicy,
    canonicalizer: &Canonicalizer<P>,
    fields: &LocaleFields<'_, TargetProfileReference>,
) -> Resolved {
    let mut evaluation = Evaluation::default();
    let result = resolve_locales(&mut evaluation, Ok(policy), Ok(canonicalizer), fields);
    Resolved { evaluation, result }
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
    prerequisites: String,
    input_storage: String,
    structural_preparation: String,
    ordinary_output: String,
}

impl InputFacts {
    pub(super) fn observe(prepared: &PreparedLocales) -> Self {
        let limits = &prepared.policy.limits().locale_resolution;
        Self {
            scope: "selected-declaration-every-locale-occurrence".into(),
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
            prerequisites: "admitted-policy-canonicalizer-and-whole-declaration".into(),
            input_storage: "borrowed-selected-fields-of-resident-complete-config".into(),
            structural_preparation: "complete-model-selection-and-borrowed-fields-before-interval"
                .into(),
            ordinary_output: "fresh-evaluation-with-resolved-locales-or-resolver-defect".into(),
        }
    }
}

/// A resolver defect, or a spelling the provider cannot answer, is no blocked
/// outcome: the case fails rather than completing as an expected failure.
/// Logical work uses this screen too.
pub(super) fn require_observable(resolved: &Resolved) -> Result<&ResolvedLocales, OutputFailure> {
    match &resolved.result {
        Ok(locales) => Ok(locales),
        Err(LocaleInvariant::Canonicalization(CanonicalizationFailure::Provider(
            ProviderFailure::UnsupportedInput | ProviderFailure::Unavailable,
        ))) => Err(OutputFailure::LocaleProviderUnavailable),
        Err(LocaleInvariant::Canonicalization(_)) => Err(OutputFailure::LocaleProviderInvariant),
        Err(LocaleInvariant::Evaluation(_)) => Err(OutputFailure::LocaleResolutionInvariant),
    }
}

fn count(value: usize) -> Result<u64, OutputFailure> {
    u64::try_from(value).map_err(|_| OutputFailure::LocaleResolutionInvariant)
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

/// Only Phase 7's own units can appear: every prerequisite of another phase
/// was admitted.
fn unit(frame: &mut Frame, unit: &Unit) -> Result<(), OutputFailure> {
    let Subject::Locale(locale) = &unit.subject else {
        return Err(OutputFailure::LocaleResolutionInvariant);
    };
    frame.text(unit.phase.id());
    subject(frame, locale)?;
    frame.uint(match unit.check {
        Check::WithinBound => 0,
        Check::CanonicalLocale => 1,
        Check::UniqueLocale => 2,
        Check::RequestedMember => 3,
        Check::SupportedAssertion
        | Check::MatchingData
        | Check::IntactArtifact
        | Check::AssertedArtifact
        | Check::OpaqueIdentity
        | Check::DistinctFromSource
        | Check::UniqueCandidate
        | Check::EffectiveMode
        | Check::VocabularyMember
        | Check::UniqueSurfaceClass
        | Check::UniqueDomain
        | Check::WithinCapability
        | Check::BootstrapCandidate
        | Check::CompatiblePolicy
        | Check::AdmissibleMember
        | Check::SingleOccurrence
        | Check::ConsistentContent
        | Check::ResolvedReference => return Err(OutputFailure::LocaleResolutionInvariant),
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

/// A value Phase 7 hands later phases, or the causes it is unavailable.
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

/// The shared domain is what Phase 7 decided, none of it positional: every
/// unit's status, the blocking results with their details, the suggestions,
/// the occurrence total, and the values later phases take. The entry domain
/// is where each spelling sits, its subject, and whether it was corrected,
/// so reordering a set changes the entry domain alone.
pub(super) fn observe(resolved: &Resolved) -> Result<Observation, OutputFailure> {
    let locales = require_observable(resolved)?;
    let evaluation = &resolved.evaluation;
    let mut shared = Frame::new("locale-resolution-result");
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
    use crate::resolver::evaluation::EvaluationInvariant;
    use crate::resolver::locales::unit as locale_unit;

    fn digest(frame_with: impl FnOnce(&mut Frame)) -> super::super::observation::Digest {
        let mut frame = Frame::new("locale-resolution-test");
        frame_with(&mut frame);
        frame.finish()
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
                OutputFailure::LocaleResolutionInvariant,
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
    fn only_phase_7_units_frame() {
        let mut frame = Frame::new("locale-resolution-test");
        for subject in [
            Subject::Vocabulary,
            Subject::CanonicalizationData,
            Subject::Negotiation(crate::resolver::negotiation::NegotiationSubject::Aliases),
            Subject::Fallback(crate::resolver::fallback::FallbackSubject::Sources),
            Subject::Coverage(crate::resolver::coverage::CoverageSubject::Rules),
            Subject::DeliveryPlacement,
        ] {
            let foreign = Unit {
                subject,
                ..locale_unit(LocaleSubject::Occurrences, Check::WithinBound)
            };
            assert_eq!(
                unit(&mut frame, &foreign),
                Err(OutputFailure::LocaleResolutionInvariant)
            );
        }
        for check in [
            Check::SupportedAssertion,
            Check::MatchingData,
            Check::IntactArtifact,
            Check::AssertedArtifact,
            Check::OpaqueIdentity,
            Check::DistinctFromSource,
            Check::UniqueCandidate,
            Check::EffectiveMode,
            Check::VocabularyMember,
            Check::UniqueSurfaceClass,
            Check::UniqueDomain,
            Check::AdmissibleMember,
            Check::SingleOccurrence,
            Check::ConsistentContent,
            Check::ResolvedReference,
        ] {
            assert_eq!(
                unit(&mut frame, &locale_unit(LocaleSubject::Occurrences, check)),
                Err(OutputFailure::LocaleResolutionInvariant)
            );
        }
        let checks = [
            Check::WithinBound,
            Check::CanonicalLocale,
            Check::UniqueLocale,
            Check::RequestedMember,
        ]
        .map(|check| {
            digest(|frame| {
                unit(frame, &locale_unit(LocaleSubject::Occurrences, check)).unwrap();
            })
        });
        for (index, left) in checks.iter().enumerate() {
            assert!(checks[index + 1..].iter().all(|right| right != left));
        }
    }
}
