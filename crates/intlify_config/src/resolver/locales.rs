// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's Phase 7: every locale spelling of the selected declaration, counted
//! before any is canonicalized, then canonicalized with its canonical form
//! suggested, each requested-locale set checked for a locale named twice, and
//! the project's source, requested, and default locales resolved. Not the
//! locale-policy checks Phase 8 makes over these values (alias and fallback
//! keys, fallback sequences, and coverage selectors, which Phase 7 only
//! counts), not a target's subset or effective default (Phase 11), and not a
//! Finding, Evidence, or 015's Logical subject key strategy, which 015 Phase 5
//! owns. A subject here carries the locale's canonical identity, or its
//! spelling when it has none; where each spelling sits is kept beside the
//! units, never in their keys.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use crate::locale::{CanonicalLocale, CanonicalizationFailure, Canonicalizer, Provider};
use crate::model::{
    Coverage, FallbackCandidate, LocaleNegotiation, NonEmptyMap, NonEmptyVec, ProfileDeclaration,
    TargetDeclaration, TargetId,
};

use super::evaluation::{
    both, Actual, Admitted, Blocking, Causes, Check, Code, Evaluation, EvaluationInvariant, Phase,
    Subject, Unit,
};
use super::limits::ConfirmedPolicy;

pub(crate) use spelling::canonicalize;
use spelling::{set, unique, Pass, Scope};

mod bound;
mod spelling;

/// A spelling canonicalization gave no identity, standing for itself. Its
/// Debug shows only its length: rejected authoring text never reaches a log.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Content(Arc<str>);

impl fmt::Debug for Content {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Content({} bytes)", self.0.len())
    }
}

impl Content {
    /// A spelling that stands for itself.
    pub(crate) fn new(spelling: &str) -> Self {
        Self(Arc::from(spelling))
    }

    /// The spelling itself, for a caller that compares or digests it; never
    /// for display.
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// What names a locale in its scope: the canonical identity its spellings
/// share, or, without one, the spelling's content.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LocaleKey {
    Canonical(CanonicalLocale),
    Content(Content),
}

/// What one Phase 7 unit checks. Kinds are declared so that each unit's
/// prerequisites order before it: the occurrence bound before any locale, and
/// the project's requested locales before their set and the default's
/// membership in it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LocaleSubject {
    /// Every locale spelling of the declaration, under `maxLocaleOccurrences`.
    Occurrences,
    /// A member of the project's `requestedLocales`.
    RequestedLocale(LocaleKey),
    /// The project's requested-locale set, under `maxRequestedLocales`.
    RequestedLocales,
    DefaultSourceLocale(LocaleKey),
    DefaultRequestedLocale(LocaleKey),
    /// A key of `localeNegotiation.aliases`.
    NegotiationAliasKey(LocaleKey),
    /// An alias destination, under its key's subject key.
    NegotiationAliasValue(LocaleKey, LocaleKey),
    /// A key of `messageFallback`.
    MessageFallbackKey(LocaleKey),
    /// A literal candidate, under its key's subject key at its position in
    /// the sequence.
    MessageFallbackCandidate(LocaleKey, usize, LocaleKey),
    /// A member of one target's `requestedLocales`.
    TargetRequestedLocale(TargetId, LocaleKey),
    /// One target's `defaultRequestedLocale`.
    TargetDefaultRequestedLocale(TargetId, LocaleKey),
}

/// The unit Phase 7 records for `check` over `subject`.
pub(crate) fn unit(subject: LocaleSubject, check: Check) -> Unit {
    Unit {
        phase: Phase::LocaleResolution,
        subject: Subject::Locale(subject),
        check,
    }
}

/// Where one spelling sits in the declaration: an array index, a map member's
/// rank in the unsigned byte order of member names, or a Target ID. Kept beside
/// the units, so reordering a set moves locations and nothing else.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Location {
    RequestedLocale(usize),
    DefaultSourceLocale,
    DefaultRequestedLocale,
    NegotiationAliasKey(usize),
    NegotiationAliasValue(usize),
    MessageFallbackKey(usize),
    MessageFallbackCandidate(usize, usize),
    TargetRequestedLocale(TargetId, usize),
    TargetDefaultRequestedLocale(TargetId),
}

/// One spelling Phase 7 canonicalized, or could not: where it sits, the
/// subject it belongs to, and whether it differs from its canonical form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocaleOccurrence {
    pub(crate) location: Location,
    pub(crate) subject: LocaleSubject,
    pub(crate) corrected: bool,
}

/// Unique canonical locales in ascending unsigned UTF-8 byte order, the order
/// 015 gives every locale set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocaleSet(Vec<CanonicalLocale>);

impl LocaleSet {
    pub(crate) fn new(mut locales: Vec<CanonicalLocale>) -> Self {
        locales.sort_unstable();
        locales.dedup();
        Self(locales)
    }

    pub(crate) fn as_slice(&self) -> &[CanonicalLocale] {
        &self.0
    }

    pub(crate) fn contains(&self, locale: &CanonicalLocale) -> bool {
        self.0.binary_search(locale).is_ok()
    }
}

/// The selected declaration's locale-bearing fields as structural admission
/// left them, each admitted or unavailable with the causes behind it.
pub(crate) struct LocaleFields<'a, Target> {
    pub(crate) requested_locales: Admitted<&'a [String]>,
    pub(crate) default_requested_locale: Admitted<&'a str>,
    pub(crate) default_source_locale: Admitted<Option<&'a str>>,
    pub(crate) locale_negotiation: Admitted<Option<&'a LocaleNegotiation>>,
    pub(crate) message_fallback:
        Admitted<Option<&'a BTreeMap<String, NonEmptyVec<FallbackCandidate>>>>,
    pub(crate) coverage: Admitted<Option<&'a Coverage>>,
    pub(crate) target_profiles: Admitted<&'a NonEmptyMap<TargetId, TargetDeclaration<Target>>>,
}

impl<'a, Target> LocaleFields<'a, Target> {
    /// The fields of a declaration structural admission admitted whole.
    pub(crate) fn declared<Policy>(declaration: &'a ProfileDeclaration<Policy, Target>) -> Self {
        Self {
            requested_locales: Ok(declaration.requested_locales.as_slice()),
            default_requested_locale: Ok(&declaration.default_requested_locale),
            default_source_locale: Ok(declaration
                .default_source_locale
                .as_option()
                .map(String::as_str)),
            locale_negotiation: Ok(declaration.locale_negotiation.as_option()),
            message_fallback: Ok(declaration.message_fallback.as_option()),
            coverage: Ok(declaration.coverage.as_option()),
            target_profiles: Ok(&declaration.target_profiles),
        }
    }
}

/// One spelling as Phase 7 left it: its subject key, and its canonical locale
/// or why it has none. A later phase names the same subject with the key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Spelled {
    pub(crate) key: LocaleKey,
    pub(crate) locale: Admitted<CanonicalLocale>,
}

/// An alias's key and destination as Phase 7 left them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AliasLocales {
    pub(crate) key: Spelled,
    pub(crate) value: Spelled,
}

/// A fallback candidate as Phase 7 left it, in its declared position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FallbackLocale {
    Literal(Spelled),
    IntentSource,
}

/// A fallback key and its whole sequence as Phase 7 left them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FallbackLocales {
    pub(crate) key: Spelled,
    pub(crate) candidates: Vec<FallbackLocale>,
}

/// One target's locales as Phase 7 left them. The set holds the canonical
/// identities even when a member repeats; Phase 11 checks the set and the
/// default against the project's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TargetLocales {
    pub(crate) id: TargetId,
    pub(crate) requested: Admitted<LocaleSet>,
    pub(crate) default: Admitted<Option<CanonicalLocale>>,
}

/// The project's locales once every Phase 7 check over them passed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectLocales {
    source_default: Option<CanonicalLocale>,
    requested: LocaleSet,
    requested_default: CanonicalLocale,
}

impl ProjectLocales {
    pub(crate) const fn source_default(&self) -> Option<&CanonicalLocale> {
        self.source_default.as_ref()
    }

    pub(crate) const fn requested(&self) -> &LocaleSet {
        &self.requested
    }

    pub(crate) const fn requested_default(&self) -> &CanonicalLocale {
        &self.requested_default
    }
}

/// What Phase 7 established for later phases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedLocales {
    /// The total the occurrence bound measured, when it measured one.
    pub(crate) occurrence_total: Option<Actual>,
    /// The occurrence bound's result, which canonicalizing any spelling waits
    /// on: Phase 8 canonicalizes the coverage selectors Phase 7 only counted.
    pub(crate) occurrence_bound: Admitted<()>,
    /// Every spelling Phase 7 reached, in location order.
    pub(crate) occurrences: Vec<LocaleOccurrence>,
    /// The project's canonical requested locales, even when one repeats or
    /// there are too many: membership needs only their identities.
    pub(crate) requested_set: Admitted<LocaleSet>,
    pub(crate) project: Admitted<ProjectLocales>,
    pub(crate) negotiation_aliases: Admitted<Vec<AliasLocales>>,
    pub(crate) message_fallback: Admitted<Vec<FallbackLocales>>,
    pub(crate) targets: Admitted<Vec<TargetLocales>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LocaleInvariant {
    Evaluation(EvaluationInvariant),
    /// The provider failed in a way 015 never reports to an author: an
    /// adapter or resolver defect, not an invalid locale.
    Canonicalization(CanonicalizationFailure),
}

impl From<EvaluationInvariant> for LocaleInvariant {
    fn from(invariant: EvaluationInvariant) -> Self {
        Self::Evaluation(invariant)
    }
}

/// Phase 7 under the confirmed policy and the canonicalizer Phase 5 bound.
/// Every spelling is counted first; none is canonicalized unless the count is
/// proved within `maxLocaleOccurrences`, so an over-bound declaration never has
/// a valid prefix resolved.
pub(crate) fn resolve_locales<P: Provider, Target>(
    evaluation: &mut Evaluation,
    policy: Result<&ConfirmedPolicy, &Causes>,
    canonicalizer: Result<&Canonicalizer<P>, &Causes>,
    fields: &LocaleFields<'_, Target>,
) -> Result<ResolvedLocales, LocaleInvariant> {
    let (occurrence_total, bounded) = bound::occurrences(evaluation, policy, fields)?;
    let mut pass = Pass::new(
        evaluation,
        both(canonicalizer.map_err(Causes::clone), bounded.clone())
            .map(|(canonicalizer, ())| canonicalizer),
    );
    let (requested_set, project) = project(&mut pass, policy, fields)?;
    let negotiation_aliases = match fields.locale_negotiation {
        Ok(negotiation) => Ok(pass.aliases(negotiation.and_then(|n| n.aliases.as_option()))?),
        Err(ref causes) => Err(causes.clone()),
    };
    let message_fallback = match fields.message_fallback {
        Ok(fallback) => Ok(pass.fallback(fallback)?),
        Err(ref causes) => Err(causes.clone()),
    };
    let targets = match fields.target_profiles {
        Ok(targets) => Ok(pass.targets(targets)?),
        Err(ref causes) => Err(causes.clone()),
    };
    Ok(ResolvedLocales {
        occurrence_total,
        occurrence_bound: bounded,
        occurrences: pass.into_occurrences(),
        requested_set,
        project,
        negotiation_aliases,
        message_fallback,
        targets,
    })
}

/// The project's requested set, checked for repeats and size, the default's
/// membership in it, and the source default, with every independent check
/// run.
fn project<P: Provider, Target>(
    pass: &mut Pass<'_, '_, P>,
    policy: Result<&ConfirmedPolicy, &Causes>,
    fields: &LocaleFields<'_, Target>,
) -> Result<(Admitted<LocaleSet>, Admitted<ProjectLocales>), LocaleInvariant> {
    let requested = match fields.requested_locales {
        Ok(requested) => Ok(pass.spell(
            requested
                .iter()
                .enumerate()
                .map(|(index, raw)| {
                    (
                        Location::RequestedLocale(index),
                        Scope::RequestedLocale,
                        raw.as_str(),
                    )
                })
                .collect(),
        )?),
        Err(ref causes) => Err(causes.clone()),
    };
    let distinct = match &requested {
        Ok(spelled) => unique(pass.evaluation, spelled, LocaleSubject::RequestedLocale)?,
        Err(causes) => Err(causes.clone()),
    };
    let requested_set = requested.and_then(|spelled| set(&spelled));
    let size = bound::requested(pass.evaluation, policy, &requested_set)?;
    let default = match fields.default_requested_locale {
        Ok(raw) => Ok(pass.spell_one(
            Location::DefaultRequestedLocale,
            Scope::DefaultRequestedLocale,
            raw,
        )?),
        Err(ref causes) => Err(causes.clone()),
    };
    let member = match &default {
        Ok(spelled) => member(pass.evaluation, spelled, &requested_set)?,
        Err(causes) => Err(causes.clone()),
    };
    let source = match fields.default_source_locale {
        Ok(Some(raw)) => pass
            .spell_one(
                Location::DefaultSourceLocale,
                Scope::DefaultSourceLocale,
                raw,
            )?
            .locale
            .map(Some),
        Ok(None) => Ok(None),
        Err(ref causes) => Err(causes.clone()),
    };
    let project = both(
        both(source, requested_set.clone()),
        both(
            both(default.and_then(|spelled| spelled.locale), member),
            both(distinct, size),
        ),
    )
    .map(
        |((source_default, requested), ((requested_default, ()), ((), ())))| ProjectLocales {
            source_default,
            requested,
            requested_default,
        },
    );
    Ok((requested_set, project))
}

/// The default's unit: its canonical locale must be one of the project's
/// requested locales.
fn member(
    evaluation: &mut Evaluation,
    default: &Spelled,
    requested: &Admitted<LocaleSet>,
) -> Result<Admitted<()>, EvaluationInvariant> {
    evaluation.check(
        unit(
            LocaleSubject::DefaultRequestedLocale(default.key.clone()),
            Check::RequestedMember,
        ),
        both(
            default.locale.clone(),
            requested.as_ref().map_err(Causes::clone),
        ),
        |(locale, requested)| {
            if requested.contains(&locale) {
                Ok(())
            } else {
                Err(issue(Code::DefaultRequestedLocaleNotInProject))
            }
        },
    )
}

const fn issue(code: Code) -> Blocking {
    Blocking {
        code,
        reason: None,
        details: None,
    }
}

/// The confirmed policy a fixture owner runs Phase 7 under: the three bounds
/// Phase 7 applies as given, and distinct fixture values for the bounds it
/// never reads.
#[cfg(any(test, feature = "benchmark", feature = "dev-example"))]
pub(crate) fn fixture_policy(
    identifier_bytes: crate::input_limits::Bound,
    occurrences: crate::input_limits::Bound,
    requested: crate::input_limits::Bound,
) -> ConfirmedPolicy {
    let mut limits = super::limits::fixture_limits(100);
    let locale = &mut limits.locale_resolution;
    locale.max_locale_identifier_bytes = identifier_bytes;
    locale.max_locale_occurrences = occurrences;
    locale.max_requested_locales = requested;
    ConfirmedPolicy::fixture_owned(limits)
}

/// What the tests of Phase 7 and the phases after it share: declarations,
/// bounds, the finite provider, and stand-ins for causes earlier phases report.
#[cfg(test)]
pub(crate) mod fixtures {
    use serde_json::Value;

    use super::*;
    use crate::fixtures::{FixturePolicyReference, FixtureTargetReference};
    use crate::input_limits::Bound;
    use crate::locale::fixtures::{fixture_pins, FixtureProvider};
    use crate::resolver::evaluation::{Reason, Status};
    use crate::resolver::limits::ResolutionLimits;
    use crate::resolver::specification::Slot;

    pub(crate) type Declaration =
        ProfileDeclaration<FixturePolicyReference, FixtureTargetReference>;

    /// The `app` declaration of `config` once `edit` changed it.
    pub(crate) fn declaration(config: &Value, edit: impl FnOnce(&mut Value)) -> Declaration {
        let mut app = config["profiles"]["app"].clone();
        edit(&mut app);
        serde_json::from_value(app).unwrap()
    }

    /// Distinct test bounds, with Phase 7's three set as given.
    pub(crate) fn limits(
        identifier_bytes: u64,
        occurrences: u64,
        requested: u64,
    ) -> ResolutionLimits {
        policy(identifier_bytes, occurrences, requested)
            .limits()
            .clone()
    }

    pub(crate) fn policy(
        identifier_bytes: u64,
        occurrences: u64,
        requested: u64,
    ) -> ConfirmedPolicy {
        fixture_policy(
            Bound::new(identifier_bytes).unwrap(),
            Bound::new(occurrences).unwrap(),
            Bound::new(requested).unwrap(),
        )
    }

    /// The finite provider bound as Phase 5 binds it, under the identifier
    /// bound the policy sets.
    pub(crate) fn canonicalizer(identifier_bytes: u64) -> Canonicalizer<FixtureProvider> {
        Canonicalizer::bind(
            &fixture_pins(),
            Some(FixtureProvider::new()),
            Bound::new(identifier_bytes).unwrap(),
        )
        .unwrap()
    }

    /// The canonical locale the finite provider gives `spelling`.
    pub(crate) fn canonical(spelling: &str) -> CanonicalLocale {
        canonicalizer(64)
            .canonicalize(spelling)
            .unwrap()
            .into_locale()
    }

    /// A canonical subject key.
    pub(crate) fn key(spelling: &str) -> LocaleKey {
        LocaleKey::Canonical(canonical(spelling))
    }

    /// A subject key for a spelling without a canonical identity.
    pub(crate) fn content(raw: &str) -> LocaleKey {
        LocaleKey::Content(Content(Arc::from(raw)))
    }

    /// What Phase 7 hands on for a spelling with a canonical identity.
    pub(crate) fn spelled(spelling: &str) -> Spelled {
        Spelled {
            key: key(spelling),
            locale: Ok(canonical(spelling)),
        }
    }

    pub(crate) fn target(id: &str) -> TargetId {
        serde_json::from_value(Value::from(id)).unwrap()
    }

    /// Causes a caller passes for a value an earlier phase left unavailable.
    /// Those phases' keys arrive with them, so a Phase 5 slot's key stands in.
    pub(crate) fn stand_in(slot: Slot) -> Causes {
        Evaluation::default()
            .check(
                Unit {
                    phase: Phase::SpecificationAdmission,
                    subject: Subject::Slot(slot),
                    check: Check::SupportedAssertion,
                },
                Ok(()),
                |()| {
                    Err::<(), _>(Blocking {
                        code: Code::SpecificationInputIncomplete,
                        reason: Some(Reason::MissingRequiredSlot),
                        details: None,
                    })
                },
            )
            .unwrap()
            .unwrap_err()
    }

    /// The units behind `causes`.
    pub(crate) fn units(causes: &Causes) -> Vec<Unit> {
        causes.keys().map(|key| key.unit().clone()).collect()
    }

    /// Every issue as its unit, code, and details, in 015's order.
    pub(crate) fn issues(
        evaluation: &Evaluation,
    ) -> Vec<(
        Unit,
        &'static str,
        Option<crate::resolver::evaluation::LimitExceeded>,
    )> {
        evaluation
            .issues()
            .map(|key| {
                (
                    key.unit().clone(),
                    key.code().as_str(),
                    evaluation.limit(key),
                )
            })
            .collect()
    }

    /// Every unit that suggests a canonical form.
    pub(crate) fn notices(evaluation: &Evaluation) -> Vec<Unit> {
        evaluation
            .notices()
            .map(|key| {
                assert_eq!(key.code(), Code::LocaleNonCanonical);
                key.unit().clone()
            })
            .collect()
    }

    /// The units a unit left not evaluated waits on, or `None` when it was
    /// evaluated.
    pub(crate) fn waits_on(evaluation: &Evaluation, unit: &Unit) -> Option<Vec<Unit>> {
        match evaluation.status(unit).expect("the unit was recorded") {
            Status::Evaluated => None,
            Status::NotEvaluated(causes) => Some(units(causes)),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::fixtures::{
        canonical, canonicalizer, content, declaration, issues, key, notices, policy, spelled,
        stand_in, target, units, waits_on, Declaration,
    };
    use super::*;
    use crate::fixtures::{complete_config, minimal_config};
    use crate::input_limits::Bound;
    use crate::resolver::evaluation::{LimitExceeded, Status};
    use crate::resolver::limits::{BoundId, Group};
    use crate::resolver::specification::Slot;

    /// Phase 7 over `declaration` under the identifier, occurrence, and
    /// requested-locale bounds given.
    fn resolve_under(
        declaration: &Declaration,
        (bytes, occurrences, requested): (u64, u64, u64),
    ) -> (Evaluation, ResolvedLocales) {
        let mut evaluation = Evaluation::default();
        let resolved = resolve_locales(
            &mut evaluation,
            Ok(&policy(bytes, occurrences, requested)),
            Ok(&canonicalizer(bytes)),
            &LocaleFields::declared(declaration),
        )
        .unwrap();
        (evaluation, resolved)
    }

    fn resolve(declaration: &Declaration) -> (Evaluation, ResolvedLocales) {
        resolve_under(declaration, (64, 100, 100))
    }

    fn set(locales: &[&str]) -> LocaleSet {
        LocaleSet::new(locales.iter().map(|locale| canonical(locale)).collect())
    }

    fn statuses(evaluation: &Evaluation) -> Vec<(Unit, Status)> {
        evaluation
            .statuses()
            .map(|(unit, status)| (unit.clone(), status.clone()))
            .collect()
    }

    fn occurrence_unit() -> Unit {
        unit(LocaleSubject::Occurrences, Check::WithinBound)
    }

    #[test]
    fn subjects_order_so_that_every_prerequisite_comes_first() {
        let ordered = [
            LocaleSubject::Occurrences,
            LocaleSubject::RequestedLocale(key("en")),
            LocaleSubject::RequestedLocale(content("zz")),
            LocaleSubject::RequestedLocales,
            LocaleSubject::DefaultSourceLocale(key("en")),
            LocaleSubject::DefaultRequestedLocale(key("en")),
            LocaleSubject::NegotiationAliasKey(key("fr")),
            LocaleSubject::NegotiationAliasValue(key("de"), key("fr")),
            LocaleSubject::NegotiationAliasValue(key("en"), key("de")),
            LocaleSubject::MessageFallbackKey(key("ja")),
            LocaleSubject::MessageFallbackCandidate(key("de"), 1, key("fr")),
            LocaleSubject::MessageFallbackCandidate(key("ja"), 0, key("fr")),
            LocaleSubject::MessageFallbackCandidate(key("ja"), 1, key("de")),
            LocaleSubject::TargetRequestedLocale(target("browser"), key("fr")),
            LocaleSubject::TargetRequestedLocale(target("server"), key("de")),
            LocaleSubject::TargetDefaultRequestedLocale(target("browser"), key("fr")),
            LocaleSubject::TargetDefaultRequestedLocale(target("server"), key("de")),
        ];
        assert!(ordered.windows(2).all(|pair| pair[0] < pair[1]));
        // A locale's own check runs before the checks that need its identity.
        let en = LocaleSubject::RequestedLocale(key("en"));
        assert!(unit(en.clone(), Check::CanonicalLocale) < unit(en, Check::UniqueLocale));
        let default = LocaleSubject::DefaultRequestedLocale(key("en"));
        assert!(
            unit(default.clone(), Check::CanonicalLocale) < unit(default, Check::RequestedMember)
        );
        assert_eq!(occurrence_unit().phase, Phase::LocaleResolution);
        let locations = [
            Location::RequestedLocale(0),
            Location::RequestedLocale(1),
            Location::DefaultSourceLocale,
            Location::DefaultRequestedLocale,
            Location::NegotiationAliasKey(0),
            Location::NegotiationAliasValue(0),
            Location::MessageFallbackKey(0),
            Location::MessageFallbackCandidate(0, 1),
            Location::MessageFallbackCandidate(1, 0),
            Location::TargetRequestedLocale(target("browser"), 1),
            Location::TargetRequestedLocale(target("server"), 0),
            Location::TargetDefaultRequestedLocale(target("browser")),
        ];
        assert!(locations.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn a_rejected_spelling_never_shows_in_debug_output() {
        let rejected = "en_US.UTF-8";
        let declaration = declaration(&minimal_config(), |app| {
            app["requestedLocales"] = json!(["en", rejected]);
        });
        let (evaluation, resolved) = resolve(&declaration);
        assert_eq!(evaluation.issues().count(), 1);
        let shown = format!("{evaluation:?}{resolved:?}");
        assert!(!shown.contains(rejected), "{shown}");
        assert!(shown.contains("Content(11 bytes)"));
    }

    #[test]
    fn the_complete_blueprint_resolves_every_locale() {
        let declaration = declaration(&complete_config(), |_| {});
        let (evaluation, resolved) = resolve(&declaration);
        assert_eq!(issues(&evaluation), []);
        assert_eq!(notices(&evaluation), []);
        let canonical_unit = |subject| unit(subject, Check::CanonicalLocale);
        assert_eq!(
            statuses(&evaluation),
            [
                occurrence_unit(),
                canonical_unit(LocaleSubject::RequestedLocale(key("en"))),
                unit(
                    LocaleSubject::RequestedLocale(key("en")),
                    Check::UniqueLocale
                ),
                unit(LocaleSubject::RequestedLocales, Check::WithinBound),
                canonical_unit(LocaleSubject::DefaultSourceLocale(key("en"))),
                canonical_unit(LocaleSubject::DefaultRequestedLocale(key("en"))),
                unit(
                    LocaleSubject::DefaultRequestedLocale(key("en")),
                    Check::RequestedMember
                ),
                canonical_unit(LocaleSubject::NegotiationAliasKey(key("fr"))),
                canonical_unit(LocaleSubject::NegotiationAliasValue(
                    key("fr"),
                    key("fr-FR")
                )),
                canonical_unit(LocaleSubject::MessageFallbackKey(key("ja"))),
                canonical_unit(LocaleSubject::MessageFallbackCandidate(
                    key("ja"),
                    0,
                    key("en")
                )),
                canonical_unit(LocaleSubject::TargetRequestedLocale(
                    target("browser"),
                    key("en")
                )),
                unit(
                    LocaleSubject::TargetRequestedLocale(target("browser"), key("en")),
                    Check::UniqueLocale
                ),
                canonical_unit(LocaleSubject::TargetRequestedLocale(
                    target("server"),
                    key("en")
                )),
                unit(
                    LocaleSubject::TargetRequestedLocale(target("server"), key("en")),
                    Check::UniqueLocale
                ),
                canonical_unit(LocaleSubject::TargetDefaultRequestedLocale(
                    target("browser"),
                    key("en")
                )),
            ]
            .map(|unit| (unit, Status::Evaluated))
        );
        let project = resolved.project.as_ref().unwrap();
        assert_eq!(project.source_default(), Some(&canonical("en")));
        assert_eq!(project.requested(), &set(&["en"]));
        assert_eq!(project.requested_default(), &canonical("en"));
        assert_eq!(resolved.requested_set, Ok(set(&["en"])));
        assert_eq!(
            resolved.negotiation_aliases,
            Ok(vec![AliasLocales {
                key: spelled("fr"),
                value: spelled("fr-FR"),
            }])
        );
        assert_eq!(
            resolved.message_fallback,
            Ok(vec![FallbackLocales {
                key: spelled("ja"),
                candidates: vec![
                    FallbackLocale::Literal(spelled("en")),
                    FallbackLocale::IntentSource,
                ],
            }])
        );
        assert_eq!(
            resolved.targets,
            Ok(vec![
                TargetLocales {
                    id: target("browser"),
                    requested: Ok(set(&["en"])),
                    default: Ok(Some(canonical("en"))),
                },
                TargetLocales {
                    id: target("server"),
                    requested: Ok(set(&["en"])),
                    default: Ok(None),
                },
            ])
        );
        // 12 spellings are counted; the two coverage selector items wait for
        // Phase 8's coverage bounds before anything reads them.
        assert_eq!(resolved.occurrence_total, Some(Actual::Exact(12)));
        // In location order, though the default is spelled before the source.
        assert_eq!(
            resolved
                .occurrences
                .iter()
                .map(|occurrence| occurrence.location.clone())
                .collect::<Vec<_>>(),
            [
                Location::RequestedLocale(0),
                Location::DefaultSourceLocale,
                Location::DefaultRequestedLocale,
                Location::NegotiationAliasKey(0),
                Location::NegotiationAliasValue(0),
                Location::MessageFallbackKey(0),
                Location::MessageFallbackCandidate(0, 0),
                Location::TargetRequestedLocale(target("browser"), 0),
                Location::TargetRequestedLocale(target("server"), 0),
                Location::TargetDefaultRequestedLocale(target("browser")),
            ]
        );
    }

    #[test]
    fn repeats_size_and_membership_are_reported_together() {
        let declaration = declaration(&minimal_config(), |app| {
            app["requestedLocales"] = json!(["iw-IL", "he-IL", "en", "EN", "de"]);
            app["defaultRequestedLocale"] = json!("fr");
        });
        let duplicate = |spelling| {
            (
                unit(
                    LocaleSubject::RequestedLocale(key(spelling)),
                    Check::UniqueLocale,
                ),
                "project-profile-locale-duplicate",
                None,
            )
        };
        let size = unit(LocaleSubject::RequestedLocales, Check::WithinBound);
        let membership = unit(
            LocaleSubject::DefaultRequestedLocale(key("fr")),
            Check::RequestedMember,
        );
        // Three locales, though five spellings: the bound holds at 3 and the
        // first value over it is 2.
        for (limit, over) in [(3, None), (2, Some(2))] {
            let (evaluation, resolved) = resolve_under(&declaration, (64, 100, limit));
            let mut expected = vec![duplicate("en"), duplicate("he-IL")];
            if let Some(limit) = over {
                expected.push((
                    size.clone(),
                    "project-profile-resource-limit-exceeded",
                    Some(LimitExceeded {
                        bound: BoundId {
                            group: Group::LocaleResolution,
                            name: "maxRequestedLocales",
                        },
                        limit: Bound::new(limit).unwrap(),
                        actual: Actual::Exact(3),
                    }),
                ));
            }
            expected.push((
                membership.clone(),
                "project-profile-default-requested-locale-not-in-project",
                None,
            ));
            assert_eq!(issues(&evaluation), expected, "{limit}");
            assert_eq!(
                notices(&evaluation),
                ["en", "he-IL"].map(|spelling| unit(
                    LocaleSubject::RequestedLocale(key(spelling)),
                    Check::CanonicalLocale
                ))
            );
            // Repeats and the size block the project's locales, never the
            // canonical set membership is checked against.
            assert_eq!(resolved.requested_set, Ok(set(&["de", "en", "he-IL"])));
            assert_eq!(
                units(&resolved.project.unwrap_err()),
                expected
                    .iter()
                    .map(|(unit, _, _)| unit.clone())
                    .collect::<Vec<_>>()
            );
            let located = |spelling| {
                resolved
                    .occurrences
                    .iter()
                    .filter(|occurrence| {
                        occurrence.subject == LocaleSubject::RequestedLocale(key(spelling))
                    })
                    .map(|occurrence| occurrence.location.clone())
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                located("he-IL"),
                [Location::RequestedLocale(0), Location::RequestedLocale(1)]
            );
            assert_eq!(
                located("en"),
                [Location::RequestedLocale(2), Location::RequestedLocale(3)]
            );
        }
    }

    #[test]
    fn an_invalid_locale_suppresses_only_what_needs_its_identity() {
        let invalid_members = declaration(&minimal_config(), |app| {
            app["requestedLocales"] = json!(["en", "zz"]);
            app["defaultSourceLocale"] = json!("Latn");
        });
        let (evaluation, resolved) = resolve(&invalid_members);
        let invalid = |subject| {
            (
                unit(subject, Check::CanonicalLocale),
                "project-profile-locale-invalid",
                None,
            )
        };
        let zz = unit(
            LocaleSubject::RequestedLocale(content("zz")),
            Check::CanonicalLocale,
        );
        assert_eq!(
            issues(&evaluation),
            [
                invalid(LocaleSubject::RequestedLocale(content("zz"))),
                invalid(LocaleSubject::DefaultSourceLocale(content("Latn"))),
            ]
        );
        for waiting in [
            unit(
                LocaleSubject::RequestedLocale(content("zz")),
                Check::UniqueLocale,
            ),
            unit(LocaleSubject::RequestedLocales, Check::WithinBound),
            unit(
                LocaleSubject::DefaultRequestedLocale(key("en")),
                Check::RequestedMember,
            ),
        ] {
            assert_eq!(waits_on(&evaluation, &waiting), Some(vec![zz.clone()]));
        }
        for independent in [
            unit(
                LocaleSubject::RequestedLocale(key("en")),
                Check::UniqueLocale,
            ),
            unit(
                LocaleSubject::DefaultRequestedLocale(key("en")),
                Check::CanonicalLocale,
            ),
        ] {
            assert_eq!(waits_on(&evaluation, &independent), None);
        }
        assert_eq!(
            units(&resolved.requested_set.unwrap_err()),
            std::slice::from_ref(&zz)
        );
        assert_eq!(
            units(&resolved.project.unwrap_err()),
            [
                zz,
                unit(
                    LocaleSubject::DefaultSourceLocale(content("Latn")),
                    Check::CanonicalLocale
                ),
            ]
        );
        assert!(resolved.targets.is_ok());

        // An invalid default leaves only its membership not evaluated.
        let invalid_default = declaration(&minimal_config(), |app| {
            app["defaultRequestedLocale"] = json!("zz");
        });
        let (evaluation, resolved) = resolve(&invalid_default);
        let default = unit(
            LocaleSubject::DefaultRequestedLocale(content("zz")),
            Check::CanonicalLocale,
        );
        assert_eq!(
            waits_on(
                &evaluation,
                &unit(
                    LocaleSubject::DefaultRequestedLocale(content("zz")),
                    Check::RequestedMember
                )
            ),
            Some(vec![default.clone()])
        );
        assert_eq!(
            waits_on(
                &evaluation,
                &unit(LocaleSubject::RequestedLocales, Check::WithinBound)
            ),
            None
        );
        assert_eq!(units(&resolved.project.unwrap_err()), [default]);
    }

    #[test]
    fn an_over_bound_declaration_has_nothing_canonicalized() {
        let declaration = declaration(&complete_config(), |_| {});
        let (evaluation, resolved) = resolve_under(&declaration, (64, 11, 100));
        assert_eq!(
            issues(&evaluation)
                .into_iter()
                .map(|(unit, code, _)| (unit, code))
                .collect::<Vec<_>>(),
            [(occurrence_unit(), "project-profile-resource-limit-exceeded")]
        );
        assert_eq!(notices(&evaluation), []);
        let waiting: Vec<_> = statuses(&evaluation)
            .into_iter()
            .filter(|(unit, _)| unit != &occurrence_unit())
            .collect();
        assert_eq!(waiting.len(), 15);
        assert!(waiting.iter().all(|(_, status)| matches!(
            status,
            Status::NotEvaluated(causes) if units(causes) == [occurrence_unit()]
        )));
        assert!(resolved.occurrences.iter().all(|occurrence| {
            !occurrence.corrected
                && matches!(
                    occurrence.subject,
                    LocaleSubject::RequestedLocale(LocaleKey::Content(_))
                        | LocaleSubject::DefaultSourceLocale(LocaleKey::Content(_))
                        | LocaleSubject::DefaultRequestedLocale(LocaleKey::Content(_))
                        | LocaleSubject::NegotiationAliasKey(LocaleKey::Content(_))
                        | LocaleSubject::NegotiationAliasValue(_, LocaleKey::Content(_))
                        | LocaleSubject::MessageFallbackKey(LocaleKey::Content(_))
                        | LocaleSubject::MessageFallbackCandidate(_, _, LocaleKey::Content(_))
                        | LocaleSubject::TargetRequestedLocale(_, LocaleKey::Content(_))
                        | LocaleSubject::TargetDefaultRequestedLocale(_, LocaleKey::Content(_))
                )
        }));
        let over = resolved.requested_set.clone().unwrap_err();
        assert_eq!(units(&over), [occurrence_unit()]);
        assert_eq!(resolved.project.unwrap_err(), over);
        // Each spelling stands for itself and waits on the bound.
        let aliases = resolved.negotiation_aliases.unwrap();
        assert_eq!(
            aliases[0],
            AliasLocales {
                key: Spelled {
                    key: content("fr"),
                    locale: Err(over.clone()),
                },
                value: Spelled {
                    key: content("fr-FR"),
                    locale: Err(over),
                },
            }
        );
        let targets = resolved.targets.unwrap();
        assert!(targets[0].requested.is_err() && targets[0].default.is_err());
        assert_eq!(targets[1].default, Ok(None));
    }

    #[test]
    fn reordering_sets_moves_only_locations() {
        let resolved = |requested, browser| {
            let declaration = declaration(&complete_config(), |app| {
                app["requestedLocales"] = json!(requested);
                app["defaultRequestedLocale"] = json!("de");
                app["targetProfiles"]["browser"]["requestedLocales"] = json!(browser);
                app["localeNegotiation"] = json!({"aliases": {"fr": "fr-FR", "EN": "en-US"}});
            });
            let (evaluation, mut resolved) = resolve(&declaration);
            let mut spellings: Vec<_> = resolved
                .occurrences
                .drain(..)
                .map(|occurrence| (occurrence.subject, occurrence.corrected))
                .collect();
            spellings.sort();
            (
                statuses(&evaluation),
                issues(&evaluation),
                notices(&evaluation),
                spellings,
                resolved,
            )
        };
        let authored = resolved(["he-IL", "en", "de", "EN", "zz"], ["en", "de", "iw-IL"]);
        let reordered = resolved(["zz", "EN", "de", "en", "he-IL"], ["iw-IL", "de", "en"]);
        assert_eq!(authored, reordered);
        assert!(!authored.1.is_empty() && !authored.2.is_empty());
    }

    #[test]
    fn each_unavailable_field_reaches_only_the_values_that_need_it() {
        let declaration = declaration(&minimal_config(), |_| {});
        let mut fields = LocaleFields::declared(&declaration);
        let [requested, default, source, negotiation, fallback, coverage, targets] = [
            Slot::ProjectProfile,
            Slot::LocaleCanonicalization,
            Slot::LocaleNegotiation,
            Slot::MessageLocaleFallback,
            Slot::Coverage,
            Slot::IntentSurfaceClassVocabulary,
            Slot::DeliveryGraph,
        ]
        .map(stand_in);
        fields.requested_locales = Err(requested.clone());
        fields.default_requested_locale = Err(default.clone());
        fields.default_source_locale = Err(source.clone());
        fields.locale_negotiation = Err(negotiation.clone());
        fields.message_fallback = Err(fallback.clone());
        fields.coverage = Err(coverage.clone());
        fields.target_profiles = Err(targets.clone());
        let mut evaluation = Evaluation::default();
        let resolved = resolve_locales(
            &mut evaluation,
            Ok(&policy(64, 100, 100)),
            Ok(&canonicalizer(64)),
            &fields,
        )
        .unwrap();
        let every = requested
            .clone()
            .union(default.clone())
            .union(source.clone())
            .union(negotiation.clone())
            .union(fallback.clone())
            .union(coverage)
            .union(targets.clone());
        // With nothing to spell, only the two singletons are recorded.
        assert_eq!(
            statuses(&evaluation),
            [
                (occurrence_unit(), Status::NotEvaluated(every)),
                (
                    unit(LocaleSubject::RequestedLocales, Check::WithinBound),
                    Status::NotEvaluated(requested.clone())
                ),
            ]
        );
        assert_eq!(resolved.occurrence_total, None);
        assert_eq!(resolved.requested_set, Err(requested.clone()));
        assert_eq!(
            resolved.project,
            Err(source.union(requested).union(default))
        );
        assert_eq!(resolved.negotiation_aliases, Err(negotiation));
        assert_eq!(resolved.message_fallback, Err(fallback));
        assert_eq!(resolved.targets, Err(targets));
        assert_eq!(resolved.occurrences, []);
    }

    #[test]
    fn unconfirmed_policy_or_unbound_data_leaves_every_locale_not_evaluated() {
        let declaration = declaration(&minimal_config(), |_| {});
        let fields = LocaleFields::declared(&declaration);
        let unconfirmed = stand_in(Slot::Coverage);
        let unbound = stand_in(Slot::LocaleCanonicalization);
        let canonicalizer = canonicalizer(64);
        let policy = policy(64, 100, 100);
        for (policy, data, causes) in [
            (Err(&unconfirmed), Ok(&canonicalizer), unconfirmed.clone()),
            (Ok(&policy), Err(&unbound), unbound.clone()),
            (
                Err(&unconfirmed),
                Err(&unbound),
                unbound.clone().union(unconfirmed.clone()),
            ),
        ] {
            let mut evaluation = Evaluation::default();
            let resolved = resolve_locales(&mut evaluation, policy, data, &fields).unwrap();
            let spelled = unit(
                LocaleSubject::RequestedLocale(content("en")),
                Check::CanonicalLocale,
            );
            assert_eq!(waits_on(&evaluation, &spelled), Some(units(&causes)));
            assert_eq!(resolved.requested_set, Err(causes.clone()));
            assert_eq!(issues(&evaluation), []);
        }
    }

    #[test]
    fn a_source_default_is_resolved_only_when_declared() {
        let (evaluation, resolved) = resolve(&declaration(&minimal_config(), |_| {}));
        assert_eq!(resolved.project.unwrap().source_default(), None);
        assert!(evaluation.statuses().all(|(unit, _)| !matches!(
            unit.subject,
            Subject::Locale(LocaleSubject::DefaultSourceLocale(_))
        )));
        let (evaluation, resolved) = resolve(&declaration(&minimal_config(), |app| {
            app["defaultSourceLocale"] = json!("EN");
        }));
        assert_eq!(
            resolved.project.unwrap().source_default(),
            Some(&canonical("en"))
        );
        assert_eq!(
            notices(&evaluation),
            [unit(
                LocaleSubject::DefaultSourceLocale(key("en")),
                Check::CanonicalLocale
            )]
        );
    }

    #[test]
    fn resolving_twice_into_one_evaluation_is_a_resolver_defect() {
        let declaration = declaration(&minimal_config(), |_| {});
        let fields = LocaleFields::declared(&declaration);
        let canonicalizer = canonicalizer(64);
        let policy = policy(64, 100, 100);
        let mut evaluation = Evaluation::default();
        resolve_locales(&mut evaluation, Ok(&policy), Ok(&canonicalizer), &fields).unwrap();
        assert_eq!(
            resolve_locales(&mut evaluation, Ok(&policy), Ok(&canonicalizer), &fields).err(),
            Some(LocaleInvariant::Evaluation(
                EvaluationInvariant::UnitRecordedTwice(occurrence_unit())
            ))
        );
    }
}
