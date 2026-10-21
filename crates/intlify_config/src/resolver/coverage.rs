// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's Phase 8 for the selected declaration's coverage: the submitted rules
//! under `maxCoverageRules` and their selector items under
//! `maxCoverageLocaleSelectorOccurrences` and
//! `maxCoverageSurfaceSelectorOccurrences`; each locale item canonicalized,
//! named once in its selector, and a requested locale; each surface item a
//! member of the admitted vocabulary, named once in its selector; each rule's
//! normalized domain held by no other rule; and the decision table over every
//! requested locale and surface class, under `maxCoverageDecisionCells` and
//! `maxCoverageRuleCellComparisons`, with each cell's effective mode and
//! Coverage Decision Basis. All of it is under the admitted Coverage
//! Specification, and the order rules or items were authored in never decides
//! anything. Not Coverage Decision Evidence, which maps a Basis to its sources
//! through Evidence 015 Phase 5 owns, and not the vocabulary's admission.

use std::collections::BTreeMap;

use intlify_shared_json::token::Token;

use crate::input_limits::Bound;
use crate::locale::{CanonicalLocale, Canonicalizer, Provider, VersionedIdentity};
use crate::model::{Coverage, CoverageMode, CoverageRule};

use super::evaluation::{
    both, within, Actual, Admitted, Blocking, Causes, Check, Code, Evaluation, EvaluationInvariant,
    LimitExceeded, Phase, Reason, Subject, Unit,
};
use super::limits::{BoundId, ConfirmedPolicy, Group, LocaleResolutionLimits};
use super::locales::{
    canonicalize, Content, LocaleInvariant, LocaleKey, LocaleSet, ResolvedLocales,
};
use super::vocabulary::SurfaceVocabulary;

const fn coverage_bound(name: &'static str) -> BoundId {
    BoundId {
        group: Group::LocaleResolution,
        name,
    }
}

const MAX_COVERAGE_RULES: BoundId = coverage_bound("maxCoverageRules");
const MAX_COVERAGE_LOCALE_SELECTOR_OCCURRENCES: BoundId =
    coverage_bound("maxCoverageLocaleSelectorOccurrences");
const MAX_COVERAGE_SURFACE_SELECTOR_OCCURRENCES: BoundId =
    coverage_bound("maxCoverageSurfaceSelectorOccurrences");
const MAX_COVERAGE_DECISION_CELLS: BoundId = coverage_bound("maxCoverageDecisionCells");
const MAX_COVERAGE_RULE_CELL_COMPARISONS: BoundId =
    coverage_bound("maxCoverageRuleCellComparisons");

/// A rule as authored, which names it until it has a normalized domain: 015's
/// Canonical content identity of the rule object, with its members in the byte
/// order of their names and each selector's spellings sorted, every repeat
/// kept. Rules with equal content share it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct RuleContent(Vec<RuleMember>);

/// One member of a rule's content. Variants follow the byte order of the
/// member names, so a sequence of members orders as 015 orders an object.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RuleMember {
    IntentSurfaceClasses(Vec<Content>),
    Mode(CoverageMode),
    RequestedLocales(Vec<Content>),
}

impl RuleContent {
    fn of(rule: &CoverageRule) -> Self {
        let sorted = |spellings: &[String]| {
            let mut spellings: Vec<Content> = spellings
                .iter()
                .map(|spelling| Content::new(spelling))
                .collect();
            spellings.sort();
            spellings
        };
        let mut members = Vec::with_capacity(3);
        if let Some(surfaces) = rule.intent_surface_classes() {
            members.push(RuleMember::IntentSurfaceClasses(sorted(surfaces)));
        }
        members.push(RuleMember::Mode(rule.mode()));
        if let Some(locales) = rule.requested_locales() {
            members.push(RuleMember::RequestedLocales(sorted(locales)));
        }
        Self(members)
    }
}

/// What names a surface class a selector item spells: the admitted vocabulary
/// member it is, or, when it is none or the vocabulary is unavailable, the
/// item's content.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SurfaceKey {
    Member(Token),
    Content(Content),
}

/// A rule's normalized domain: the canonical locales and the surface classes
/// it matches, each a sorted set, where an omitted selector stands for every
/// requested locale or every vocabulary member. Domains order by their locale
/// sequences, then by their surface sequences, a shorter exact prefix first.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Domain {
    locales: Vec<CanonicalLocale>,
    surfaces: Vec<Token>,
}

impl Domain {
    pub(crate) fn locales(&self) -> &[CanonicalLocale] {
        &self.locales
    }

    pub(crate) fn surfaces(&self) -> &[Token] {
        &self.surfaces
    }

    fn contains(&self, locale: &CanonicalLocale, surface: &Token) -> bool {
        self.locales.binary_search(locale).is_ok() && self.surfaces.binary_search(surface).is_ok()
    }

    /// Whether `self` is a strict subset of `other`, so more specific.
    fn narrower_than(&self, other: &Self) -> bool {
        self != other
            && subset(&self.locales, &other.locales)
            && subset(&self.surfaces, &other.surfaces)
    }
}

fn subset<T: Ord>(inner: &[T], outer: &[T]) -> bool {
    inner
        .iter()
        .all(|member| outer.binary_search(member).is_ok())
}

/// What one coverage unit checks. Kinds are declared so that each unit's
/// prerequisites come first: the three counts of what was submitted, each
/// rule's selector items, the normalized domains, then the decision table's
/// two bounds and its cells.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CoverageSubject {
    /// The submitted rules, under `maxCoverageRules`.
    Rules,
    /// Every locale selector item, under `maxCoverageLocaleSelectorOccurrences`.
    LocaleSelectors,
    /// Every surface selector item, under
    /// `maxCoverageSurfaceSelectorOccurrences`.
    SurfaceSelectors,
    /// A locale one rule's selector names.
    Locale(RuleContent, LocaleKey),
    /// A surface class one rule's selector names.
    SurfaceClass(RuleContent, SurfaceKey),
    /// A rule's normalized domain.
    Domain(Domain),
    /// The decision table's cells, under `maxCoverageDecisionCells`.
    Cells,
    /// Every submitted rule against every cell, under
    /// `maxCoverageRuleCellComparisons`.
    Comparisons,
    /// One cell of the decision table: a requested locale and a surface class.
    Cell(CanonicalLocale, Token),
}

/// The unit Phase 8 records for `check` over `subject`.
pub(crate) fn unit(subject: CoverageSubject, check: Check) -> Unit {
    Unit {
        phase: Phase::LocalePolicyResolution,
        subject: Subject::Coverage(subject),
        check,
    }
}

/// How a cell's mode was decided: by the project default, or by the maximally
/// specific rules that match the cell, named by their normalized domains in
/// order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Decision {
    Default,
    Rules(Vec<Domain>),
}

/// One cell's Coverage Decision Basis: its effective mode and how that was
/// decided. It names no source, so equivalent declarations share it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Basis {
    pub(crate) mode: CoverageMode,
    pub(crate) decision: Decision,
}

/// Each cell of a decision table with its Basis, ordered by locale and then
/// by surface class.
pub(crate) type Cells = BTreeMap<(CanonicalLocale, Token), Basis>;

/// Each normalized domain with how many rule occurrences hold it and the mode
/// of the first.
type Domains = BTreeMap<Domain, (usize, CoverageMode)>;

/// The checked coverage: the admitted specification and the complete decision
/// table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CoveragePolicy<Id> {
    specification: VersionedIdentity<Id>,
    cells: Cells,
}

impl<Id> CoveragePolicy<Id> {
    pub(crate) const fn specification(&self) -> &VersionedIdentity<Id> {
        &self.specification
    }

    /// The decision table: each cell with its effective mode, in cell order.
    pub(crate) fn table(&self) -> impl Iterator<Item = (&CanonicalLocale, &Token, CoverageMode)> {
        self.cells
            .iter()
            .map(|((locale, surface), basis)| (locale, surface, basis.mode))
    }

    /// Each cell's Coverage Decision Basis, in cell order: Shared reporting
    /// beside the table, not profile semantics.
    pub(crate) const fn basis(&self) -> &Cells {
        &self.cells
    }
}

/// Phase 8's coverage checks over the declaration's `coverage` field, what
/// Phase 7 resolved, and the vocabulary and canonicalizer Phase 5 admitted.
/// Every unit waits on the specification, through a count or through the
/// cells' bound; the policy exists only when every unit passed.
pub(crate) fn resolve_coverage<Id: Clone, P: Provider>(
    evaluation: &mut Evaluation,
    policy: Result<&ConfirmedPolicy, &Causes>,
    specification: &Admitted<VersionedIdentity<Id>>,
    vocabulary: Result<&SurfaceVocabulary, &Causes>,
    canonicalizer: Result<&Canonicalizer<P>, &Causes>,
    locales: &ResolvedLocales,
    coverage: Admitted<Option<&Coverage>>,
) -> Result<Admitted<CoveragePolicy<Id>>, LocaleInvariant> {
    let policy = policy.map_err(Causes::clone);
    let vocabulary = vocabulary.map_err(Causes::clone);
    let requested = locales.requested_set.as_ref().map_err(Causes::clone);
    let rules = coverage
        .as_ref()
        .ok()
        .copied()
        .flatten()
        .and_then(|coverage| coverage.rules.as_option())
        .map_or(&[][..], Vec::as_slice);
    let declared = both(
        both(policy.clone(), specification.clone()),
        coverage.clone(),
    )
    .map(|((policy, _), _)| policy);
    let rules_bounded = count(
        evaluation,
        &declared,
        CoverageSubject::Rules,
        MAX_COVERAGE_RULES,
        |limits| limits.max_coverage_rules,
        rules.len(),
    )?;
    let locale_items_bounded = count(
        evaluation,
        &declared,
        CoverageSubject::LocaleSelectors,
        MAX_COVERAGE_LOCALE_SELECTOR_OCCURRENCES,
        |limits| limits.max_coverage_locale_selector_occurrences,
        rules
            .iter()
            .map(|rule| rule.requested_locales().map_or(0, <[String]>::len))
            .sum(),
    )?;
    let surface_items_bounded = count(
        evaluation,
        &declared,
        CoverageSubject::SurfaceSelectors,
        MAX_COVERAGE_SURFACE_SELECTOR_OCCURRENCES,
        |limits| limits.max_coverage_surface_selector_occurrences,
        rules
            .iter()
            .map(|rule| rule.intent_surface_classes().map_or(0, <[String]>::len))
            .sum(),
    )?;
    let selectors = Selectors {
        spelling: both(
            both(
                canonicalizer.map_err(Causes::clone),
                locales.occurrence_bound.clone(),
            ),
            both(rules_bounded.clone(), locale_items_bounded.clone()),
        )
        .map(|((canonicalizer, ()), _)| canonicalizer),
        naming: both(
            vocabulary.clone(),
            both(rules_bounded.clone(), surface_items_bounded.clone()),
        )
        .map(|(vocabulary, _)| vocabulary),
        requested: requested.clone(),
        vocabulary: vocabulary.clone(),
    };
    let (normalized, domains) = normalize(evaluation, &selectors, rules)?;
    let checked = both(
        both(
            both(rules_bounded, locale_items_bounded),
            surface_items_bounded,
        ),
        normalized,
    )
    .map(|_| ());
    let span = both(requested, vocabulary);
    let sized = evaluation.check(
        unit(CoverageSubject::Cells, Check::WithinBound),
        both(both(policy, specification.clone()), span.clone()),
        |((policy, _), (requested, vocabulary))| {
            within_product(
                MAX_COVERAGE_DECISION_CELLS,
                policy
                    .limits()
                    .locale_resolution
                    .max_coverage_decision_cells,
                &[requested.as_slice().len(), vocabulary.members().len()],
            )
        },
    )?;
    let compared = evaluation.check(
        unit(CoverageSubject::Comparisons, Check::WithinBound),
        both(declared, span.clone()),
        |(policy, (requested, vocabulary))| {
            within_product(
                MAX_COVERAGE_RULE_CELL_COMPARISONS,
                policy
                    .limits()
                    .locale_resolution
                    .max_coverage_rule_cell_comparisons,
                &[
                    rules.len(),
                    requested.as_slice().len(),
                    vocabulary.members().len(),
                ],
            )
        },
    )?;
    let default = coverage.map(|coverage| {
        coverage
            .and_then(|coverage| coverage.default_mode.as_option().copied())
            .unwrap_or(CoverageMode::DirectRequired)
    });
    let decidable = both(default, both(checked.clone(), compared)).map(|(default, _)| default);
    // A cell is named only once the bound proves the table may be built, and
    // every cell waits on the comparison bound. With no cell to wait on them,
    // the rules' own checks still decide whether the policy exists.
    let cells = match both(span, sized) {
        Ok(((requested, vocabulary), ())) => {
            decide(evaluation, &decidable, requested, vocabulary, &domains)?
        }
        Err(causes) => Err(causes),
    };
    Ok(
        both(both(specification.clone(), checked), cells).map(|((specification, ()), cells)| {
            CoveragePolicy {
                specification,
                cells,
            }
        }),
    )
}

/// A count's unit: every submitted item counts, within its bound.
fn count(
    evaluation: &mut Evaluation,
    declared: &Admitted<&ConfirmedPolicy>,
    subject: CoverageSubject,
    bound: BoundId,
    limit: fn(&LocaleResolutionLimits) -> Bound,
    total: usize,
) -> Result<Admitted<()>, EvaluationInvariant> {
    evaluation.check(
        unit(subject, Check::WithinBound),
        declared.clone(),
        |policy| within(bound, limit(&policy.limits().locale_resolution), total),
    )
}

/// What the selector items of every rule wait on: the canonicalizer, once the
/// rule, locale-item, and occurrence counts passed; the vocabulary, once the
/// rule and surface-item counts passed; and the sets an omitted selector
/// stands for.
struct Selectors<'a, P: Provider> {
    spelling: Admitted<&'a Canonicalizer<P>>,
    naming: Admitted<&'a SurfaceVocabulary>,
    requested: Admitted<&'a LocaleSet>,
    vocabulary: Admitted<&'a SurfaceVocabulary>,
}

/// Each distinct rule's selector units and normalized domain, then one unit
/// per domain: no two rule occurrences may share one. Returns whether every
/// rule normalized with a domain of its own, and each domain with how many
/// rules hold it and the mode of the first.
fn normalize<P: Provider>(
    evaluation: &mut Evaluation,
    selectors: &Selectors<'_, P>,
    rules: &[CoverageRule],
) -> Result<(Admitted<()>, Domains), LocaleInvariant> {
    let mut contents: BTreeMap<RuleContent, (usize, &CoverageRule)> = BTreeMap::new();
    for rule in rules {
        contents.entry(RuleContent::of(rule)).or_insert((0, rule)).0 += 1;
    }
    let mut normalized = Ok(());
    let mut domains = Domains::new();
    for (content, (occurrences, rule)) in &contents {
        let locales = match rule.requested_locales() {
            Some(items) => selected_locales(evaluation, selectors, content, items)?,
            None => selectors
                .requested
                .clone()
                .map(|requested| requested.as_slice().to_vec()),
        };
        let surfaces = match rule.intent_surface_classes() {
            Some(items) => selected_surfaces(evaluation, &selectors.naming, content, items)?,
            None => selectors
                .vocabulary
                .clone()
                .map(|vocabulary| vocabulary.members().to_vec()),
        };
        match both(locales, surfaces) {
            Ok((locales, surfaces)) => {
                domains
                    .entry(Domain { locales, surfaces })
                    .or_insert((0, rule.mode()))
                    .0 += occurrences;
            }
            Err(causes) => normalized = both(normalized, Err::<(), _>(causes)).map(|_| ()),
        }
    }
    for (domain, &(occurrences, _)) in &domains {
        let unique = evaluation.check(
            unit(CoverageSubject::Domain(domain.clone()), Check::UniqueDomain),
            Ok(()),
            |()| {
                if occurrences == 1 {
                    Ok(())
                } else {
                    Err(issue(Code::CoverageRuleDuplicate, None))
                }
            },
        )?;
        normalized = both(normalized, unique).map(|_| ());
    }
    Ok((normalized, domains))
}

/// One rule's locale selector: each item canonicalized, with one unit per
/// subject as Phase 7 does, and each locale named once and requested. Returns
/// the selector's canonical locales as a set once every unit passed.
fn selected_locales<P: Provider>(
    evaluation: &mut Evaluation,
    selectors: &Selectors<'_, P>,
    rule: &RuleContent,
    items: &[String],
) -> Result<Admitted<Vec<CanonicalLocale>>, LocaleInvariant> {
    type Attempt = Admitted<Result<CanonicalLocale, Blocking>>;
    // Each subject's result, whether an item differs from the canonical form,
    // and how many items spell it.
    let mut subjects: BTreeMap<LocaleKey, (Attempt, bool, usize)> = BTreeMap::new();
    for raw in items {
        let attempt = match selectors.spelling {
            Ok(canonicalizer) => Ok(canonicalize(canonicalizer, raw)?),
            Err(ref causes) => Err(causes.clone()),
        };
        let (key, corrected) = match &attempt {
            Ok(Ok((locale, corrected))) => (LocaleKey::Canonical(locale.clone()), *corrected),
            _ => (LocaleKey::Content(Content::new(raw)), false),
        };
        let attempt = attempt.map(|attempt| attempt.map(|(locale, _)| locale));
        let subject = subjects.entry(key).or_insert((attempt, false, 0));
        subject.1 |= corrected;
        subject.2 += 1;
    }
    let mut selected = Ok(Vec::with_capacity(subjects.len()));
    for (key, (attempt, corrected, spellings)) in subjects {
        let subject = |check| unit(CoverageSubject::Locale(rule.clone(), key.clone()), check);
        let locale =
            evaluation.check(subject(Check::CanonicalLocale), attempt, |attempt| attempt)?;
        if corrected {
            evaluation.notice(subject(Check::CanonicalLocale), Code::LocaleNonCanonical)?;
        }
        let once = evaluation.check(subject(Check::UniqueLocale), locale.clone(), |_| {
            if spellings == 1 {
                Ok(())
            } else {
                Err(issue(Code::LocaleDuplicate, None))
            }
        })?;
        let member = evaluation.check(
            subject(Check::RequestedMember),
            both(locale.clone(), selectors.requested.clone()),
            |(locale, requested)| {
                if requested.contains(&locale) {
                    Ok(())
                } else {
                    Err(issue(
                        Code::CoverageRuleInvalid,
                        Some(Reason::LocaleNotRequested),
                    ))
                }
            },
        )?;
        selected =
            both(selected, both(locale, both(once, member))).map(|(mut selected, (locale, _))| {
                selected.push(locale);
                selected
            });
    }
    Ok(selected)
}

/// One rule's surface-class selector: each item a member of the admitted
/// vocabulary and each member named once. A repeated item that names no
/// member is unknown, not a duplicate. Returns the selector's surface classes
/// as a set once every unit passed.
fn selected_surfaces(
    evaluation: &mut Evaluation,
    naming: &Admitted<&SurfaceVocabulary>,
    rule: &RuleContent,
    items: &[String],
) -> Result<Admitted<Vec<Token>>, EvaluationInvariant> {
    // Each subject with an item that spells it and how many items do.
    let mut subjects: BTreeMap<SurfaceKey, (&str, usize)> = BTreeMap::new();
    for raw in items {
        let key = match naming
            .as_ref()
            .ok()
            .and_then(|vocabulary| vocabulary.member(raw))
        {
            Some(member) => SurfaceKey::Member(member.clone()),
            None => SurfaceKey::Content(Content::new(raw)),
        };
        subjects.entry(key).or_insert((raw, 0)).1 += 1;
    }
    let mut selected = Ok(Vec::with_capacity(subjects.len()));
    for (key, (raw, spellings)) in subjects {
        let subject = |check| {
            unit(
                CoverageSubject::SurfaceClass(rule.clone(), key.clone()),
                check,
            )
        };
        let member = evaluation.check(
            subject(Check::VocabularyMember),
            naming.clone(),
            |vocabulary| {
                vocabulary.member(raw).cloned().ok_or(issue(
                    Code::CoverageRuleInvalid,
                    Some(Reason::SurfaceClassUnknown),
                ))
            },
        )?;
        let once = evaluation.check(subject(Check::UniqueSurfaceClass), member.clone(), |_| {
            if spellings == 1 {
                Ok(())
            } else {
                Err(issue(
                    Code::CoverageRuleInvalid,
                    Some(Reason::SurfaceClassDuplicate),
                ))
            }
        })?;
        selected = both(selected, both(member, once)).map(|(mut selected, (member, ()))| {
            selected.push(member);
            selected
        });
    }
    Ok(selected)
}

/// One unit per cell, locale first, each waiting on `decidable`: the default
/// mode, once every rule normalized to a domain of its own and the comparison
/// bound passed.
fn decide(
    evaluation: &mut Evaluation,
    decidable: &Admitted<CoverageMode>,
    requested: &LocaleSet,
    vocabulary: &SurfaceVocabulary,
    domains: &Domains,
) -> Result<Admitted<Cells>, EvaluationInvariant> {
    let mut cells = Ok(BTreeMap::new());
    for locale in requested.as_slice() {
        for surface in vocabulary.members() {
            let decided = evaluation.check(
                unit(
                    CoverageSubject::Cell(locale.clone(), surface.clone()),
                    Check::EffectiveMode,
                ),
                decidable.clone(),
                |default| basis(default, locale, surface, domains),
            )?;
            cells = both(cells, decided).map(|(mut cells, basis)| {
                cells.insert((locale.clone(), surface.clone()), basis);
                cells
            });
        }
    }
    Ok(cells)
}

/// One cell's decision from the rules matching it, keeping each that no
/// narrower matching rule outranks. None kept is the default; kept rules of
/// one mode decide it together; two modes conflict, whatever order the rules
/// were authored in.
fn basis(
    default: CoverageMode,
    locale: &CanonicalLocale,
    surface: &Token,
    domains: &Domains,
) -> Result<Basis, Blocking> {
    let matching: Vec<(&Domain, CoverageMode)> = domains
        .iter()
        .filter(|(domain, _)| domain.contains(locale, surface))
        .map(|(domain, &(_, mode))| (domain, mode))
        .collect();
    let maximal: Vec<(&Domain, CoverageMode)> = matching
        .iter()
        .filter(|(domain, _)| {
            !matching
                .iter()
                .any(|(other, _)| other.narrower_than(domain))
        })
        .copied()
        .collect();
    let Some(&(_, mode)) = maximal.first() else {
        return Ok(Basis {
            mode: default,
            decision: Decision::Default,
        });
    };
    if maximal.iter().all(|&(_, other)| other == mode) {
        Ok(Basis {
            mode,
            decision: Decision::Rules(
                maximal
                    .into_iter()
                    .map(|(domain, _)| domain.clone())
                    .collect(),
            ),
        })
    } else {
        Err(issue(Code::CoverageRuleConflict, None))
    }
}

/// Whether the product of `factors` stays within `limit`. The product uses
/// checked arithmetic; past `u64`, the issue reports the smallest witness that
/// it exceeds `limit`.
fn within_product(bound: BoundId, limit: Bound, factors: &[usize]) -> Result<(), Blocking> {
    let product = if factors.contains(&0) {
        Some(0)
    } else {
        factors.iter().try_fold(1_u64, |product, &factor| {
            u64::try_from(factor)
                .ok()
                .and_then(|factor| product.checked_mul(factor))
        })
    };
    // `Bound` excludes `u64::MAX`, so the witness is representable.
    match product.map_or(Actual::AtLeast(limit.get() + 1), Actual::Exact) {
        Actual::Exact(total) if total <= limit.get() => Ok(()),
        actual => Err(Blocking::limit(LimitExceeded {
            bound,
            limit,
            actual,
        })),
    }
}

const fn issue(code: Code, reason: Option<Reason>) -> Blocking {
    Blocking {
        code,
        reason,
        limit: None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;
    use crate::fixtures::{minimal_config, FixtureTargetReference};
    use crate::model::CoverageMode::{DirectRequired, FallbackAllowed};
    use crate::resolver::evaluation::Status;
    use crate::resolver::fallback::FallbackSubject;
    use crate::resolver::locales::fixtures::{
        canonical, canonicalizer, content, declaration, key, limits, notices, stand_in, units,
        waits_on, Declaration,
    };
    use crate::resolver::locales::{self, resolve_locales, LocaleFields, LocaleSubject};
    use crate::resolver::specification::Slot;

    type Id = &'static str;

    const SPECIFICATION: VersionedIdentity<Id> = VersionedIdentity {
        identity: "intlify-localization-coverage",
        revision: "0",
    };

    const EVERY_LOCALE: [&str; 3] = ["en", "fr", "ja"];
    const EVERY_SURFACE: [&str; 3] = ["checkout", "internal-tool", "nav"];

    /// The five coverage bounds.
    #[derive(Clone, Copy)]
    struct Bounds {
        rules: u64,
        locale_items: u64,
        surface_items: u64,
        cells: u64,
        comparisons: u64,
    }

    const ROOM: Bounds = Bounds {
        rules: 100,
        locale_items: 100,
        surface_items: 100,
        cells: 100,
        comparisons: 1000,
    };

    /// A confirmed policy with these coverage bounds and room for everything
    /// Phase 7 bounds.
    fn policy(bounds: Bounds) -> ConfirmedPolicy {
        let bound = |value| Bound::new(value).unwrap();
        let mut limits = limits(64, 100, 100);
        let locale = &mut limits.locale_resolution;
        locale.max_coverage_rules = bound(bounds.rules);
        locale.max_coverage_locale_selector_occurrences = bound(bounds.locale_items);
        locale.max_coverage_surface_selector_occurrences = bound(bounds.surface_items);
        locale.max_coverage_decision_cells = bound(bounds.cells);
        locale.max_coverage_rule_cell_comparisons = bound(bounds.comparisons);
        ConfirmedPolicy::fixture_owned(limits)
    }

    fn vocabulary() -> SurfaceVocabulary {
        SurfaceVocabulary::fixture_owned(&EVERY_SURFACE)
    }

    /// The minimal declaration requesting `en`, `fr`, and `ja`, `en` its
    /// default, once `edit` changed it.
    fn requesting(edit: impl FnOnce(&mut Value)) -> Declaration {
        declaration(&minimal_config(), |app| {
            app["requestedLocales"] = json!(EVERY_LOCALE);
            edit(app);
        })
    }

    fn declared(coverage: Value) -> Declaration {
        requesting(|app| app["coverage"] = coverage)
    }

    /// What a test runs Phases 7 and 8 under.
    struct Setup<'v> {
        /// Phase 7's `maxLocaleOccurrences`.
        occurrences: u64,
        /// `maxLocaleIdentifierBytes` as the canonicalizer applies it.
        identifier_bytes: u64,
        policy: Result<ConfirmedPolicy, Causes>,
        specification: Admitted<VersionedIdentity<Id>>,
        vocabulary: Result<&'v SurfaceVocabulary, &'v Causes>,
    }

    /// Everything admitted, with room for whatever a test declares.
    fn setup(bounds: Bounds, vocabulary: &SurfaceVocabulary) -> Setup<'_> {
        Setup {
            occurrences: 100,
            identifier_bytes: 64,
            policy: Ok(policy(bounds)),
            specification: Ok(SPECIFICATION),
            vocabulary: Ok(vocabulary),
        }
    }

    struct Resolved {
        evaluation: Evaluation,
        policy: Admitted<CoveragePolicy<Id>>,
    }

    /// Phase 7 over `fields`, then Phase 8's coverage, into one evaluation.
    fn resolve_fields(
        fields: &LocaleFields<'_, FixtureTargetReference>,
        setup: &Setup<'_>,
    ) -> Resolved {
        let mut evaluation = Evaluation::default();
        let canonicalizer = canonicalizer(setup.identifier_bytes);
        let locales = resolve_locales(
            &mut evaluation,
            Ok(&locales::fixtures::policy(
                setup.identifier_bytes,
                setup.occurrences,
                100,
            )),
            Ok(&canonicalizer),
            fields,
        )
        .unwrap();
        let policy = resolve_coverage(
            &mut evaluation,
            setup.policy.as_ref(),
            &setup.specification,
            setup.vocabulary,
            Ok(&canonicalizer),
            &locales,
            fields.coverage.clone(),
        )
        .unwrap();
        Resolved { evaluation, policy }
    }

    fn resolve_under(
        declaration: &Declaration,
        bounds: Bounds,
        vocabulary: &SurfaceVocabulary,
    ) -> Resolved {
        resolve_fields(
            &LocaleFields::declared(declaration),
            &setup(bounds, vocabulary),
        )
    }

    fn resolve(declaration: &Declaration) -> Resolved {
        resolve_under(declaration, ROOM, &vocabulary())
    }

    /// Phase 8's units and their statuses, in 015's order.
    fn statuses(evaluation: &Evaluation) -> Vec<(Unit, Status)> {
        evaluation
            .statuses()
            .filter(|(unit, _)| unit.phase == Phase::LocalePolicyResolution)
            .map(|(unit, status)| (unit.clone(), status.clone()))
            .collect()
    }

    /// Phase 8's issues as their unit, code, and reason.
    fn issues(evaluation: &Evaluation) -> Vec<(Unit, &'static str, Option<&'static str>)> {
        evaluation
            .issues()
            .filter(|key| key.unit().phase == Phase::LocalePolicyResolution)
            .map(|key| {
                (
                    key.unit().clone(),
                    key.code().as_str(),
                    key.reason().map(Reason::as_str),
                )
            })
            .collect()
    }

    /// The details of the resource issue `unit` found.
    fn exceeded(evaluation: &Evaluation, unit: &Unit) -> LimitExceeded {
        let key = evaluation
            .issues()
            .find(|key| key.unit() == unit)
            .expect("the unit found an issue");
        evaluation.limit(key).expect("a resource issue")
    }

    fn exactly(bound: BoundId, limit: u64, actual: u64) -> LimitExceeded {
        LimitExceeded {
            bound,
            limit: Bound::new(limit).unwrap(),
            actual: Actual::Exact(actual),
        }
    }

    fn rule(rule: &Value) -> RuleContent {
        RuleContent::of(&serde_json::from_value(rule.clone()).unwrap())
    }

    fn token(member: &str) -> Token {
        Token::new(member).unwrap()
    }

    fn domain(locales: &[&str], surfaces: &[&str]) -> Domain {
        let mut locales: Vec<_> = locales.iter().map(|locale| canonical(locale)).collect();
        locales.sort();
        let mut surfaces: Vec<_> = surfaces.iter().map(|surface| token(surface)).collect();
        surfaces.sort();
        Domain { locales, surfaces }
    }

    fn count_unit(subject: CoverageSubject) -> Unit {
        unit(subject, Check::WithinBound)
    }

    fn locale_unit(owner: &RuleContent, key: LocaleKey, check: Check) -> Unit {
        unit(CoverageSubject::Locale(owner.clone(), key), check)
    }

    fn member_unit(owner: &RuleContent, member: &str, check: Check) -> Unit {
        unit(
            CoverageSubject::SurfaceClass(owner.clone(), SurfaceKey::Member(token(member))),
            check,
        )
    }

    fn unknown_unit(owner: &RuleContent, raw: &str, check: Check) -> Unit {
        unit(
            CoverageSubject::SurfaceClass(owner.clone(), SurfaceKey::Content(Content::new(raw))),
            check,
        )
    }

    fn domain_unit(domain: Domain) -> Unit {
        unit(CoverageSubject::Domain(domain), Check::UniqueDomain)
    }

    fn cell_unit(locale: &str, surface: &str) -> Unit {
        unit(
            CoverageSubject::Cell(canonical(locale), token(surface)),
            Check::EffectiveMode,
        )
    }

    /// Every cell's unit, in cell order.
    fn cell_units() -> Vec<Unit> {
        EVERY_LOCALE
            .iter()
            .flat_map(|locale| {
                EVERY_SURFACE
                    .iter()
                    .map(|surface| cell_unit(locale, surface))
            })
            .collect()
    }

    fn names_a_cell(unit: &Unit) -> bool {
        matches!(unit.subject, Subject::Coverage(CoverageSubject::Cell(..)))
    }

    /// The decision table as spellings, in cell order.
    fn table(policy: &CoveragePolicy<Id>) -> Vec<(&str, &str, CoverageMode)> {
        policy
            .table()
            .map(|(locale, surface, mode)| (locale.as_str(), surface.as_str(), mode))
            .collect()
    }

    fn basis_of<'p>(policy: &'p CoveragePolicy<Id>, locale: &str, surface: &str) -> &'p Basis {
        &policy.basis()[&(canonical(locale), token(surface))]
    }

    #[test]
    fn subjects_order_the_counts_first_then_rules_then_domains_then_the_table() {
        let surface_only =
            rule(&json!({"intentSurfaceClasses": ["nav"], "mode": "direct-required"}));
        let locale_only = rule(&json!({"requestedLocales": ["fr"], "mode": "fallback-allowed"}));
        let ordered = [
            CoverageSubject::Rules,
            CoverageSubject::LocaleSelectors,
            CoverageSubject::SurfaceSelectors,
            CoverageSubject::Locale(surface_only.clone(), key("fr")),
            CoverageSubject::Locale(locale_only.clone(), key("fr")),
            CoverageSubject::Locale(locale_only, content("zz")),
            CoverageSubject::SurfaceClass(surface_only.clone(), SurfaceKey::Member(token("nav"))),
            CoverageSubject::SurfaceClass(surface_only, SurfaceKey::Content(Content::new("help"))),
            CoverageSubject::Domain(domain(&["en", "fr"], &[])),
            CoverageSubject::Domain(domain(&["fr"], &["nav"])),
            CoverageSubject::Cells,
            CoverageSubject::Comparisons,
            CoverageSubject::Cell(canonical("en"), token("nav")),
            CoverageSubject::Cell(canonical("fr"), token("checkout")),
        ];
        assert!(ordered.windows(2).all(|pair| pair[0] < pair[1]));
        // Fallback's units commit before coverage's.
        assert!(
            crate::resolver::fallback::unit(
                FallbackSubject::IntentSource(content("zz")),
                Check::UniqueCandidate
            ) < count_unit(CoverageSubject::Rules)
        );
    }

    #[test]
    fn a_rule_is_named_by_its_content_with_sorted_selectors_and_every_repeat() {
        let authored =
            rule(&json!({"mode": "direct-required", "requestedLocales": ["ja", "en", "ja"]}));
        assert_eq!(
            authored,
            rule(&json!({"requestedLocales": ["en", "ja", "ja"], "mode": "direct-required"}))
        );
        assert_ne!(
            authored,
            rule(&json!({"requestedLocales": ["en", "ja"], "mode": "direct-required"}))
        );
        assert_ne!(
            authored,
            rule(&json!({"requestedLocales": ["en", "ja", "ja"], "mode": "fallback-allowed"}))
        );
        // A spelling is content here, so an alias is not its canonical form.
        assert_ne!(
            rule(&json!({"requestedLocales": ["EN"], "mode": "direct-required"})),
            rule(&json!({"requestedLocales": ["en"], "mode": "direct-required"}))
        );
        // Members compare in the order of their names, so the first member
        // two rules differ in decides, and a shorter prefix comes first.
        let ordered = [
            rule(&json!({"intentSurfaceClasses": ["checkout"], "mode": "fallback-allowed"})),
            rule(&json!({"intentSurfaceClasses": ["nav"], "mode": "direct-required"})),
            rule(&json!({
                "intentSurfaceClasses": ["nav"],
                "mode": "direct-required",
                "requestedLocales": ["en"]
            })),
            rule(&json!({"mode": "direct-required", "requestedLocales": ["ja"]})),
            rule(&json!({"mode": "fallback-allowed", "requestedLocales": ["en"]})),
        ];
        assert!(ordered.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn a_domain_is_narrower_only_as_a_strict_subset_in_both_dimensions() {
        let wide = domain(&["en", "fr"], &["checkout", "nav"]);
        assert!(!wide.narrower_than(&wide));
        for narrower in [
            domain(&["fr"], &["checkout", "nav"]),
            domain(&["en", "fr"], &["nav"]),
            domain(&["fr"], &["nav"]),
        ] {
            assert!(narrower.narrower_than(&wide));
            assert!(!wide.narrower_than(&narrower));
        }
        assert!(!domain(&["fr", "ja"], &["nav"]).narrower_than(&wide));
        assert!(wide.contains(&canonical("fr"), &token("nav")));
        assert!(!wide.contains(&canonical("ja"), &token("nav")));
        assert!(!wide.contains(&canonical("fr"), &token("internal-tool")));
    }

    #[test]
    fn omission_an_empty_declaration_and_an_explicit_direct_default_are_one_table() {
        let omitted = resolve(&requesting(|_| ())).policy.unwrap();
        assert_eq!(omitted.specification(), &SPECIFICATION);
        assert_eq!(
            table(&omitted),
            EVERY_LOCALE
                .iter()
                .flat_map(|locale| EVERY_SURFACE.iter().map(move |surface| (
                    *locale,
                    *surface,
                    DirectRequired
                )))
                .collect::<Vec<_>>()
        );
        let default = |mode| Basis {
            mode,
            decision: Decision::Default,
        };
        assert!(omitted
            .basis()
            .values()
            .all(|basis| basis == &default(DirectRequired)));
        for coverage in [
            json!({}),
            json!({"defaultMode": "direct-required"}),
            json!({"rules": []}),
            json!({"defaultMode": "direct-required", "rules": []}),
        ] {
            let resolved = resolve(&declared(coverage));
            assert_eq!(issues(&resolved.evaluation), []);
            assert_eq!(resolved.policy.unwrap(), omitted);
        }
        let fallback = resolve(&declared(json!({"defaultMode": "fallback-allowed"})))
            .policy
            .unwrap();
        assert_eq!(fallback.basis().len(), 9);
        assert!(fallback
            .basis()
            .values()
            .all(|basis| basis == &default(FallbackAllowed)));
    }

    #[test]
    fn the_most_specific_matching_rules_decide_each_cell_whatever_the_authoring_order() {
        let resolved = resolve(&declared(json!({"rules": [
            {"requestedLocales": ["fr"], "mode": "fallback-allowed"},
            {"intentSurfaceClasses": ["internal-tool"], "mode": "fallback-allowed"},
            {"requestedLocales": ["fr"], "intentSurfaceClasses": ["checkout"], "mode": "direct-required"}
        ]})));
        assert_eq!(issues(&resolved.evaluation), []);
        assert!(cell_units()
            .iter()
            .all(|cell| resolved.evaluation.status(cell) == Some(&Status::Evaluated)));
        let policy = resolved.policy.unwrap();
        assert_eq!(
            table(&policy),
            [
                ("en", "checkout", DirectRequired),
                ("en", "internal-tool", FallbackAllowed),
                ("en", "nav", DirectRequired),
                ("fr", "checkout", DirectRequired),
                ("fr", "internal-tool", FallbackAllowed),
                ("fr", "nav", FallbackAllowed),
                ("ja", "checkout", DirectRequired),
                ("ja", "internal-tool", FallbackAllowed),
                ("ja", "nav", DirectRequired),
            ]
        );
        let locale_only = domain(&["fr"], &EVERY_SURFACE);
        let surface_only = domain(&EVERY_LOCALE, &["internal-tool"]);
        // The combined rule is narrower than the locale-only rule it overlaps.
        assert_eq!(
            basis_of(&policy, "fr", "checkout"),
            &Basis {
                mode: DirectRequired,
                decision: Decision::Rules(vec![domain(&["fr"], &["checkout"])]),
            }
        );
        // Two incomparable rules of one mode decide together, in domain order.
        assert_eq!(
            basis_of(&policy, "fr", "internal-tool"),
            &Basis {
                mode: FallbackAllowed,
                decision: Decision::Rules(vec![surface_only.clone(), locale_only.clone()]),
            }
        );
        assert_eq!(
            basis_of(&policy, "fr", "nav").decision,
            Decision::Rules(vec![locale_only])
        );
        assert_eq!(
            basis_of(&policy, "ja", "internal-tool").decision,
            Decision::Rules(vec![surface_only])
        );
        assert_eq!(basis_of(&policy, "en", "nav").decision, Decision::Default);
        // Reordering rules, selector items, and members changes nothing.
        let reordered = resolve(&declared(json!({"rules": [
            {"mode": "direct-required", "intentSurfaceClasses": ["checkout"], "requestedLocales": ["fr"]},
            {"mode": "fallback-allowed", "intentSurfaceClasses": ["internal-tool"]},
            {"mode": "fallback-allowed", "requestedLocales": ["fr"]}
        ]})));
        assert_eq!(reordered.policy.unwrap(), policy);
    }

    #[test]
    fn an_omitted_selector_and_one_naming_every_member_are_one_domain() {
        let locale_only = json!({"requestedLocales": ["fr", "ja"], "mode": "fallback-allowed"});
        let named = json!({
            "requestedLocales": ["ja", "fr"],
            "intentSurfaceClasses": ["nav", "checkout", "internal-tool"],
            "mode": "direct-required"
        });
        let alone = |rule: &Value| {
            resolve(&declared(json!({"rules": [rule]})))
                .policy
                .unwrap()
                .basis()
                .iter()
                .map(|(cell, basis)| (cell.clone(), basis.decision.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(alone(&locale_only), alone(&named));
        // Together they are one domain held twice, whatever their modes.
        let resolved = resolve(&declared(json!({"rules": [locale_only, named]})));
        let duplicate = domain_unit(domain(&["fr", "ja"], &EVERY_SURFACE));
        assert_eq!(
            issues(&resolved.evaluation),
            [(
                duplicate.clone(),
                "project-profile-coverage-rule-duplicate",
                None
            )]
        );
        for cell in cell_units() {
            assert_eq!(
                waits_on(&resolved.evaluation, &cell),
                Some(vec![duplicate.clone()])
            );
        }
        assert_eq!(units(&resolved.policy.unwrap_err()), [duplicate]);
    }

    #[test]
    fn identical_or_aliased_rules_are_duplicates_and_identical_ones_share_a_subject() {
        let fr = json!({"requestedLocales": ["fr"], "mode": "fallback-allowed"});
        let resolved = resolve(&declared(json!({"rules": [fr, fr]})));
        let owner = rule(&fr);
        assert_eq!(
            statuses(&resolved.evaluation)
                .into_iter()
                .filter(|(unit, _)| matches!(
                    &unit.subject,
                    Subject::Coverage(CoverageSubject::Locale(..))
                ))
                .map(|(unit, _)| unit)
                .collect::<Vec<_>>(),
            [
                Check::CanonicalLocale,
                Check::UniqueLocale,
                Check::RequestedMember
            ]
            .map(|check| locale_unit(&owner, key("fr"), check))
        );
        assert_eq!(
            issues(&resolved.evaluation),
            [(
                domain_unit(domain(&["fr"], &EVERY_SURFACE)),
                "project-profile-coverage-rule-duplicate",
                None
            )]
        );
        // An alias spelling names the same locale as its canonical form.
        let aliased = json!({"requestedLocales": ["EN"], "mode": "fallback-allowed"});
        let resolved = resolve(&declared(json!({"rules": [
            aliased,
            {"requestedLocales": ["en"], "mode": "fallback-allowed"}
        ]})));
        assert_eq!(
            issues(&resolved.evaluation),
            [(
                domain_unit(domain(&["en"], &EVERY_SURFACE)),
                "project-profile-coverage-rule-duplicate",
                None
            )]
        );
        assert_eq!(
            notices(&resolved.evaluation),
            [locale_unit(
                &rule(&aliased),
                key("en"),
                Check::CanonicalLocale
            )]
        );
    }

    #[test]
    fn a_locale_item_is_canonicalized_named_once_in_its_selector_and_requested() {
        let selector =
            json!({"requestedLocales": ["EN", "en", "de", "zz", "fr"], "mode": "fallback-allowed"});
        let resolved = resolve(&declared(json!({"rules": [selector]})));
        let owner = rule(&selector);
        let en = |check| locale_unit(&owner, key("en"), check);
        let de = |check| locale_unit(&owner, key("de"), check);
        let zz = |check| locale_unit(&owner, content("zz"), check);
        let found = [
            (
                de(Check::RequestedMember),
                "project-profile-coverage-rule-invalid",
                Some("locale-not-requested"),
            ),
            (
                en(Check::UniqueLocale),
                "project-profile-locale-duplicate",
                None,
            ),
            (
                zz(Check::CanonicalLocale),
                "project-profile-locale-invalid",
                None,
            ),
        ];
        assert_eq!(issues(&resolved.evaluation), found);
        assert_eq!(notices(&resolved.evaluation), [en(Check::CanonicalLocale)]);
        for check in [Check::UniqueLocale, Check::RequestedMember] {
            assert_eq!(
                waits_on(&resolved.evaluation, &zz(check)),
                Some(vec![zz(Check::CanonicalLocale)])
            );
            assert_eq!(
                waits_on(&resolved.evaluation, &locale_unit(&owner, key("fr"), check)),
                None
            );
        }
        // An invalid rule has no domain, and every cell waits on it.
        let causes = found.map(|(unit, ..)| unit).to_vec();
        assert!(statuses(&resolved.evaluation)
            .iter()
            .all(|(unit, _)| !matches!(
                unit.subject,
                Subject::Coverage(CoverageSubject::Domain(_))
            )));
        for cell in cell_units() {
            assert_eq!(waits_on(&resolved.evaluation, &cell), Some(causes.clone()));
        }
        assert_eq!(units(&resolved.policy.unwrap_err()), causes);
    }

    #[test]
    fn a_surface_item_is_a_vocabulary_member_named_once_in_its_selector() {
        let selector = json!({"intentSurfaceClasses": ["nav", "help", "nav", "Nav", "help"], "mode": "fallback-allowed"});
        let resolved = resolve(&declared(json!({"rules": [selector]})));
        let owner = rule(&selector);
        assert_eq!(
            issues(&resolved.evaluation),
            [
                (
                    member_unit(&owner, "nav", Check::UniqueSurfaceClass),
                    "project-profile-coverage-rule-invalid",
                    Some("surface-class-duplicate"),
                ),
                (
                    unknown_unit(&owner, "Nav", Check::VocabularyMember),
                    "project-profile-coverage-rule-invalid",
                    Some("surface-class-unknown"),
                ),
                (
                    unknown_unit(&owner, "help", Check::VocabularyMember),
                    "project-profile-coverage-rule-invalid",
                    Some("surface-class-unknown"),
                ),
            ]
        );
        // A repeated item that names no member is unknown, never a duplicate.
        assert_eq!(
            waits_on(
                &resolved.evaluation,
                &unknown_unit(&owner, "help", Check::UniqueSurfaceClass)
            ),
            Some(vec![unknown_unit(&owner, "help", Check::VocabularyMember)])
        );
        assert_eq!(
            waits_on(
                &resolved.evaluation,
                &member_unit(&owner, "nav", Check::VocabularyMember)
            ),
            None
        );
        assert_eq!(
            units(&resolved.policy.unwrap_err()),
            [
                member_unit(&owner, "nav", Check::UniqueSurfaceClass),
                unknown_unit(&owner, "Nav", Check::VocabularyMember),
                unknown_unit(&owner, "help", Check::VocabularyMember),
            ]
        );
    }

    #[test]
    fn incomparable_maximal_rules_of_two_modes_conflict_only_where_they_overlap() {
        let locale_only = json!({"requestedLocales": ["fr"], "mode": "fallback-allowed"});
        let surface_only = json!({"intentSurfaceClasses": ["checkout"], "mode": "direct-required"});
        let resolved = resolve(&declared(json!({"rules": [locale_only, surface_only]})));
        let conflict = cell_unit("fr", "checkout");
        assert_eq!(
            issues(&resolved.evaluation),
            [(
                conflict.clone(),
                "project-profile-coverage-rule-conflict",
                None
            )]
        );
        assert!(cell_units()
            .iter()
            .all(|cell| resolved.evaluation.status(cell) == Some(&Status::Evaluated)));
        assert_eq!(units(&resolved.policy.unwrap_err()), [conflict]);
        // A rule over just the overlap is narrower than both and decides it.
        let resolved = resolve(&declared(json!({"rules": [
            locale_only,
            surface_only,
            {"requestedLocales": ["fr"], "intentSurfaceClasses": ["checkout"], "mode": "fallback-allowed"}
        ]})));
        assert_eq!(issues(&resolved.evaluation), []);
        let policy = resolved.policy.unwrap();
        assert_eq!(
            basis_of(&policy, "fr", "checkout"),
            &Basis {
                mode: FallbackAllowed,
                decision: Decision::Rules(vec![domain(&["fr"], &["checkout"])]),
            }
        );
        assert_eq!(basis_of(&policy, "en", "checkout").mode, DirectRequired);
        assert_eq!(basis_of(&policy, "fr", "nav").mode, FallbackAllowed);
    }

    #[test]
    fn the_rule_bound_counts_every_submitted_rule_before_any_selector_is_checked() {
        let fr = json!({"requestedLocales": ["fr"], "mode": "fallback-allowed"});
        let nav = json!({"intentSurfaceClasses": ["nav"], "mode": "fallback-allowed"});
        let declaration = declared(json!({"rules": [fr, fr, nav]}));
        // At the bound every selector is checked, and the repeat is found.
        let at = resolve_under(&declaration, Bounds { rules: 3, ..ROOM }, &vocabulary());
        assert_eq!(
            issues(&at.evaluation),
            [(
                domain_unit(domain(&["fr"], &EVERY_SURFACE)),
                "project-profile-coverage-rule-duplicate",
                None
            )]
        );
        let over = resolve_under(&declaration, Bounds { rules: 2, ..ROOM }, &vocabulary());
        let rules = count_unit(CoverageSubject::Rules);
        assert_eq!(
            exceeded(&over.evaluation, &rules),
            exactly(MAX_COVERAGE_RULES, 2, 3)
        );
        // Nothing past the count is checked: no item, no domain, and no cell.
        assert_eq!(
            waits_on(
                &over.evaluation,
                &locale_unit(&rule(&fr), content("fr"), Check::CanonicalLocale)
            ),
            Some(vec![rules.clone()])
        );
        assert_eq!(
            waits_on(
                &over.evaluation,
                &unknown_unit(&rule(&nav), "nav", Check::VocabularyMember)
            ),
            Some(vec![rules.clone()])
        );
        for cell in cell_units() {
            assert_eq!(waits_on(&over.evaluation, &cell), Some(vec![rules.clone()]));
        }
        assert_eq!(units(&over.policy.unwrap_err()), [rules]);
    }

    #[test]
    fn each_selector_bound_counts_every_raw_item_before_its_items_are_checked() {
        let locales_only = json!({"requestedLocales": ["fr", "fr"], "mode": "fallback-allowed"});
        let surfaces = json!({
            "requestedLocales": ["ja"],
            "intentSurfaceClasses": ["nav", "checkout", "nav"],
            "mode": "direct-required"
        });
        let declaration = declared(json!({"rules": [locales_only, surfaces]}));
        let bounds = Bounds {
            locale_items: 3,
            surface_items: 3,
            ..ROOM
        };
        // At both bounds every item is checked, so each repeat is found.
        let at = resolve_under(&declaration, bounds, &vocabulary());
        assert_eq!(
            issues(&at.evaluation)
                .into_iter()
                .map(|(unit, code, reason)| (unit.check, code, reason))
                .collect::<Vec<_>>(),
            [
                (
                    Check::UniqueLocale,
                    "project-profile-locale-duplicate",
                    None
                ),
                (
                    Check::UniqueSurfaceClass,
                    "project-profile-coverage-rule-invalid",
                    Some("surface-class-duplicate")
                ),
            ]
        );
        let locale_items = count_unit(CoverageSubject::LocaleSelectors);
        let over = resolve_under(
            &declaration,
            Bounds {
                locale_items: 2,
                ..bounds
            },
            &vocabulary(),
        );
        assert_eq!(
            exceeded(&over.evaluation, &locale_items),
            exactly(MAX_COVERAGE_LOCALE_SELECTOR_OCCURRENCES, 2, 3)
        );
        assert_eq!(
            waits_on(
                &over.evaluation,
                &locale_unit(&rule(&locales_only), content("fr"), Check::CanonicalLocale)
            ),
            Some(vec![locale_items])
        );
        assert_eq!(
            waits_on(
                &over.evaluation,
                &member_unit(&rule(&surfaces), "nav", Check::UniqueSurfaceClass)
            ),
            None
        );
        let surface_items = count_unit(CoverageSubject::SurfaceSelectors);
        let over = resolve_under(
            &declaration,
            Bounds {
                surface_items: 2,
                ..bounds
            },
            &vocabulary(),
        );
        assert_eq!(
            exceeded(&over.evaluation, &surface_items),
            exactly(MAX_COVERAGE_SURFACE_SELECTOR_OCCURRENCES, 2, 3)
        );
        assert_eq!(
            waits_on(
                &over.evaluation,
                &unknown_unit(&rule(&surfaces), "nav", Check::VocabularyMember)
            ),
            Some(vec![surface_items])
        );
        assert_eq!(
            waits_on(
                &over.evaluation,
                &locale_unit(&rule(&locales_only), key("fr"), Check::UniqueLocale)
            ),
            None
        );
    }

    #[test]
    fn the_cell_bound_measures_locales_times_classes_and_no_cell_is_named_past_it() {
        let declaration = declared(json!({}));
        let at = resolve_under(&declaration, Bounds { cells: 9, ..ROOM }, &vocabulary());
        assert_eq!(at.policy.unwrap().basis().len(), 9);
        let over = resolve_under(&declaration, Bounds { cells: 8, ..ROOM }, &vocabulary());
        let cells = count_unit(CoverageSubject::Cells);
        assert_eq!(
            exceeded(&over.evaluation, &cells),
            exactly(MAX_COVERAGE_DECISION_CELLS, 8, 9)
        );
        assert!(!statuses(&over.evaluation)
            .iter()
            .any(|(unit, _)| names_a_cell(unit)));
        assert_eq!(units(&over.policy.unwrap_err()), [cells]);
    }

    #[test]
    fn the_comparison_bound_counts_every_submitted_rule_against_every_cell() {
        // Three rule occurrences, two the same rule, against nine cells.
        let fr = json!({"requestedLocales": ["fr"], "mode": "fallback-allowed"});
        let declaration = declared(json!({"rules": [
            fr,
            fr,
            {"intentSurfaceClasses": ["nav"], "mode": "fallback-allowed"}
        ]}));
        let compared = count_unit(CoverageSubject::Comparisons);
        let duplicate = domain_unit(domain(&["fr"], &EVERY_SURFACE));
        let at = resolve_under(
            &declaration,
            Bounds {
                comparisons: 27,
                ..ROOM
            },
            &vocabulary(),
        );
        assert_eq!(at.evaluation.status(&compared), Some(&Status::Evaluated));
        assert_eq!(
            units(&at.policy.unwrap_err()),
            std::slice::from_ref(&duplicate)
        );
        let over = resolve_under(
            &declaration,
            Bounds {
                comparisons: 26,
                ..ROOM
            },
            &vocabulary(),
        );
        assert_eq!(
            exceeded(&over.evaluation, &compared),
            exactly(MAX_COVERAGE_RULE_CELL_COMPARISONS, 26, 27)
        );
        // The cells are still named, and wait on it.
        for cell in cell_units() {
            assert_eq!(
                waits_on(&over.evaluation, &cell),
                Some(vec![duplicate.clone(), compared.clone()])
            );
        }
    }

    #[test]
    fn a_product_past_u64_exceeds_its_bound_and_a_zero_factor_makes_it_zero() {
        let limit = Bound::new(5).unwrap();
        let within =
            |factors: &[usize]| within_product(MAX_COVERAGE_DECISION_CELLS, limit, factors);
        let past = |actual| {
            Err(Blocking::limit(LimitExceeded {
                bound: MAX_COVERAGE_DECISION_CELLS,
                limit,
                actual,
            }))
        };
        assert_eq!(within(&[5, 1]), Ok(()));
        assert_eq!(within(&[3, 2]), past(Actual::Exact(6)));
        assert_eq!(
            within(&[usize::MAX, usize::MAX, usize::MAX]),
            past(Actual::AtLeast(6))
        );
        assert_eq!(within(&[usize::MAX, usize::MAX, 0]), Ok(()));
    }

    #[test]
    fn an_unadmitted_specification_policy_or_field_leaves_every_unit_not_evaluated() {
        let selector = json!({"requestedLocales": ["fr"], "intentSurfaceClasses": ["nav"], "mode": "fallback-allowed"});
        let declaration = declared(json!({"rules": [selector]}));
        let declared_fields = LocaleFields::declared(&declaration);
        let vocabulary = vocabulary();
        let owner = rule(&selector);
        let counts = [
            CoverageSubject::Rules,
            CoverageSubject::LocaleSelectors,
            CoverageSubject::SurfaceSelectors,
        ]
        .map(count_unit);
        let items = [
            locale_unit(&owner, content("fr"), Check::CanonicalLocale),
            locale_unit(&owner, content("fr"), Check::UniqueLocale),
            locale_unit(&owner, content("fr"), Check::RequestedMember),
            unknown_unit(&owner, "nav", Check::VocabularyMember),
            unknown_unit(&owner, "nav", Check::UniqueSurfaceClass),
        ];
        let table = [CoverageSubject::Cells, CoverageSubject::Comparisons].map(count_unit);
        let slot = stand_in(Slot::Coverage);
        let unconfirmed = stand_in(Slot::LocaleCanonicalization);
        let field = stand_in(Slot::DeliveryGraph);
        let fields_without = LocaleFields {
            coverage: Err(field.clone()),
            ..LocaleFields::declared(&declaration)
        };
        for (fields, setup, causes, recorded) in [
            (
                &declared_fields,
                Setup {
                    specification: Err(slot.clone()),
                    ..setup(ROOM, &vocabulary)
                },
                &slot,
                [counts.as_slice(), &items, &table].concat(),
            ),
            (
                &declared_fields,
                Setup {
                    policy: Err(unconfirmed.clone()),
                    ..setup(ROOM, &vocabulary)
                },
                &unconfirmed,
                [counts.as_slice(), &items, &table].concat(),
            ),
            // Without the field no rule can be named, and Phase 7 cannot
            // count its selectors, so not even the requested set forms.
            (
                &fields_without,
                setup(ROOM, &vocabulary),
                &field,
                [counts.as_slice(), &table].concat(),
            ),
        ] {
            let resolved = resolve_fields(fields, &setup);
            assert_eq!(
                statuses(&resolved.evaluation),
                recorded
                    .into_iter()
                    .map(|unit| (unit, Status::NotEvaluated(causes.clone())))
                    .collect::<Vec<_>>()
            );
            assert_eq!(units(&resolved.policy.unwrap_err()), units(causes));
        }
    }

    #[test]
    fn a_missing_vocabulary_or_requested_set_leaves_only_what_needs_it_not_evaluated() {
        let selector = json!({"requestedLocales": ["fr"], "intentSurfaceClasses": ["nav"], "mode": "fallback-allowed"});
        let owner = rule(&selector);
        let table = [CoverageSubject::Cells, CoverageSubject::Comparisons].map(count_unit);
        let missing = stand_in(Slot::IntentSurfaceClassVocabulary);
        let declaration = declared(json!({"rules": [selector]}));
        let resolved = resolve_fields(
            &LocaleFields::declared(&declaration),
            &Setup {
                vocabulary: Err(&missing),
                ..setup(ROOM, &vocabulary())
            },
        );
        for check in [
            Check::CanonicalLocale,
            Check::UniqueLocale,
            Check::RequestedMember,
        ] {
            assert_eq!(
                waits_on(&resolved.evaluation, &locale_unit(&owner, key("fr"), check)),
                None
            );
        }
        for unit in [
            unknown_unit(&owner, "nav", Check::VocabularyMember),
            unknown_unit(&owner, "nav", Check::UniqueSurfaceClass),
        ]
        .iter()
        .chain(&table)
        {
            assert_eq!(waits_on(&resolved.evaluation, unit), Some(units(&missing)));
        }
        assert_eq!(units(&resolved.policy.unwrap_err()), units(&missing));
        // A requested set Phase 7 could not form leaves membership waiting.
        let resolved = resolve(&requesting(|app| {
            app["requestedLocales"] = json!(["en", "fr", "ja", "zz"]);
            app["coverage"] = json!({"rules": [selector]});
        }));
        let invalid = locales::unit(
            LocaleSubject::RequestedLocale(content("zz")),
            Check::CanonicalLocale,
        );
        assert_eq!(
            waits_on(
                &resolved.evaluation,
                &locale_unit(&owner, key("fr"), Check::RequestedMember)
            ),
            Some(vec![invalid.clone()])
        );
        for unit in [
            locale_unit(&owner, key("fr"), Check::UniqueLocale),
            member_unit(&owner, "nav", Check::UniqueSurfaceClass),
        ] {
            assert_eq!(waits_on(&resolved.evaluation, &unit), None);
        }
        for unit in &table {
            assert_eq!(
                waits_on(&resolved.evaluation, unit),
                Some(vec![invalid.clone()])
            );
        }
        assert_eq!(units(&resolved.policy.unwrap_err()), [invalid]);
    }

    #[test]
    fn locale_items_wait_on_phase_7s_count_and_each_on_its_byte_bound() {
        let selector = json!({"requestedLocales": ["EN-us", "fr"], "mode": "fallback-allowed"});
        let declaration = declared(json!({"rules": [selector]}));
        let fields = LocaleFields::declared(&declaration);
        let owner = rule(&selector);
        let vocabulary = vocabulary();
        // Phase 7 counts three requested locales, the default, the target's
        // one, and these two items.
        let occurrences = locales::unit(LocaleSubject::Occurrences, Check::WithinBound);
        let over = resolve_fields(
            &fields,
            &Setup {
                occurrences: 6,
                ..setup(ROOM, &vocabulary)
            },
        );
        assert_eq!(
            waits_on(
                &over.evaluation,
                &locale_unit(&owner, content("EN-us"), Check::CanonicalLocale)
            ),
            Some(vec![occurrences])
        );
        let at = resolve_fields(
            &fields,
            &Setup {
                occurrences: 7,
                ..setup(ROOM, &vocabulary)
            },
        );
        // At it, each item is canonicalized: `EN-us` is `en-US`, which the
        // project does not request.
        let corrected = |check| locale_unit(&owner, key("en-US"), check);
        assert_eq!(
            issues(&at.evaluation),
            [(
                corrected(Check::RequestedMember),
                "project-profile-coverage-rule-invalid",
                Some("locale-not-requested")
            )]
        );
        assert_eq!(notices(&at.evaluation), [corrected(Check::CanonicalLocale)]);
        // Each item is measured in bytes before the provider sees it.
        let short = resolve_fields(
            &fields,
            &Setup {
                identifier_bytes: 4,
                ..setup(ROOM, &vocabulary)
            },
        );
        let long = locale_unit(&owner, content("EN-us"), Check::CanonicalLocale);
        assert_eq!(
            exceeded(&short.evaluation, &long),
            exactly(
                BoundId {
                    group: Group::LocaleResolution,
                    name: "maxLocaleIdentifierBytes",
                },
                4,
                5
            )
        );
        assert_eq!(
            waits_on(
                &short.evaluation,
                &locale_unit(&owner, key("fr"), Check::RequestedMember)
            ),
            None
        );
    }

    #[test]
    fn an_empty_vocabulary_spans_no_cell_and_has_no_member_to_name() {
        let empty = SurfaceVocabulary::fixture_owned(&[]);
        let resolved = resolve_under(
            &declared(json!({"rules": [{"requestedLocales": ["fr"], "mode": "fallback-allowed"}]})),
            ROOM,
            &empty,
        );
        assert_eq!(issues(&resolved.evaluation), []);
        assert!(resolved.policy.unwrap().basis().is_empty());
        let selector = json!({"intentSurfaceClasses": ["checkout"], "mode": "fallback-allowed"});
        let resolved = resolve_under(&declared(json!({"rules": [selector]})), ROOM, &empty);
        let unknown = unknown_unit(&rule(&selector), "checkout", Check::VocabularyMember);
        assert_eq!(
            issues(&resolved.evaluation),
            [(
                unknown.clone(),
                "project-profile-coverage-rule-invalid",
                Some("surface-class-unknown")
            )]
        );
        // With no cell to wait on it, the rule still blocks the policy.
        assert_eq!(units(&resolved.policy.unwrap_err()), [unknown]);
    }
}
