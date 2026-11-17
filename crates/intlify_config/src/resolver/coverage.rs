// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's Phase 8 for the selected declaration's coverage: the submitted rules
//! under `maxCoverageRules` and their selector items under
//! `maxCoverageLocaleSelectorOccurrences` and
//! `maxCoverageSurfaceSelectorOccurrences`; each locale item canonicalized,
//! named once in its selector, and a requested locale; each surface item a
//! member of the admitted vocabulary, named once in its selector; each rule
//! normalized to a domain no other rule holds; each cell of the decision table
//! over every requested locale and surface class, under
//! `maxCoverageDecisionCells` and `maxCoverageRuleCellComparisons`, decided
//! with its effective mode and Coverage Decision Basis; and the table, once
//! every cell is decided, which owns every Basis. All of it is under the
//! admitted Coverage Specification, and the order rules or items were authored
//! in never decides anything. A rule and its items are named by the rule's
//! raw content until a Subject content projection replaces it. Once the table
//! is established, it also owns the Coverage Decision Evidence mapping each
//! Basis to the Evidence of its decision sources, observed in the
//! declaration's file (D15:1609, 1611). Not the vocabulary's admission.
//!
//! A count past its bound is observed where its table's Scope sits in the
//! declaration, beside the confirmed policy, which supplies the bound
//! (D15:1901-1905, 2249): the rules for the rule and selector-item counts,
//! the nearest location holding every item, and for the rule-to-cell
//! comparisons, the factor that bound adds; the requested locales for the
//! cells, the factor the declaration supplies (plan 025 decision #198). Each
//! rule holding a selector is a raw parent of its own, so a selector item's
//! Findings, and a locale item's notice, are observed at the items naming
//! their subject in every such rule (D15:2190, 2387). A domain several rules
//! hold, and a cell whose maximally specific rules select two modes, are
//! observed at each of those rules beside the others (D15:1609, 2387, 2410).

use std::collections::BTreeMap;

use intlify_shared_json::token::Token;

use crate::input_limits::Bound;
use crate::locale::{CanonicalLocale, Canonicalizer, Provider, VersionedIdentity};
use crate::model::{Coverage, CoverageMode, CoverageRule};
use crate::structural::Edge;

use super::decision_evidence::DecisionEvidence;
use super::declaration_observer::{beside, DeclarationObserver, ObserverInvariant};
use super::evaluation::{
    both, within, Actual, Admitted, Blocking, Causes, Check, Code, Evaluation, EvaluationInvariant,
    Found, LimitExceeded, Notice, Observation, Phase, Reason, Subject, Unit,
};
use super::evidence::EvidenceProjection;
use super::evidence_component::ComponentPath;
use super::limits::{BoundId, ConfirmedPolicy, Group, LocaleResolutionLimits};
use super::locales::{
    canonicalize, Content, LocaleInvariant, LocaleKey, LocaleSet, ResolvedLocales,
};
use super::specification::Slot;
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
    /// The content that stands for every rule in a logical subject: a rule is
    /// an object, and `redacted-structure` keeps only that type, so every rule
    /// shares one subject (Decision 015-174).
    pub(crate) fn projected() -> Self {
        Self(Vec::new())
    }

    /// The rule's members, in the byte order of their names.
    pub(crate) fn members(&self) -> &[RuleMember] {
        &self.0
    }

    /// The content identity of `rule`.
    pub(crate) fn of(rule: &CoverageRule) -> Self {
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
    /// A domain of these sets, each already sorted, for another module's
    /// tests.
    #[cfg(test)]
    pub(crate) fn of(locales: Vec<CanonicalLocale>, surfaces: Vec<Token>) -> Self {
        Self { locales, surfaces }
    }

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
/// rule's selector items, each rule normalized from its items, the normalized
/// domains, then the decision table's two bounds, its cells, and the table.
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
    /// A rule, the parent of its selector items, normalized to its domain.
    Rule(RuleContent),
    /// A rule's normalized domain.
    Domain(Domain),
    /// The decision table's cells, under `maxCoverageDecisionCells`.
    Cells,
    /// Every submitted rule against every cell, under
    /// `maxCoverageRuleCellComparisons`.
    Comparisons,
    /// One cell of the decision table: a requested locale and a surface class.
    Cell(CanonicalLocale, Token),
    /// The decision table, complete once every cell is decided: the owner of
    /// every cell's Coverage Decision Basis.
    Table,
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

/// Each normalized domain with the positions of the rule occurrences holding
/// it, in ascending order, and the mode of the first distinct rule.
type Domains = BTreeMap<Domain, (Vec<usize>, CoverageMode)>;

/// The checked coverage, 015's coverage semantic group: the admitted
/// specification, the vocabulary its table was checked against, the
/// effective project default mode, and the complete decision table over the
/// requested locales.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CoveragePolicy<Id> {
    specification: VersionedIdentity<Id>,
    vocabulary: SurfaceVocabulary,
    requested: LocaleSet,
    default_mode: CoverageMode,
    cells: Cells,
}

impl<Id> CoveragePolicy<Id> {
    pub(crate) const fn specification(&self) -> &VersionedIdentity<Id> {
        &self.specification
    }

    /// The admitted vocabulary whose surface classes the table spans.
    pub(crate) const fn vocabulary(&self) -> &SurfaceVocabulary {
        &self.vocabulary
    }

    /// The requested locales the table spans.
    pub(crate) const fn requested(&self) -> &LocaleSet {
        &self.requested
    }

    /// The declared default mode, or `direct-required` when none is declared.
    pub(crate) const fn default_mode(&self) -> CoverageMode {
        self.default_mode
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
/// Phase 7 resolved, and the vocabulary and canonicalizer Phase 5 admitted,
/// observed through `observer` when the declaration's file is there; the
/// caller hands it what Phase 5 bound. Every unit waits on the
/// specification, through a count or through the cells' bound; the policy
/// exists only when every unit passed.
pub(crate) fn resolve_coverage<Id: Clone, P: Provider>(
    evaluation: &mut Evaluation,
    confirmed: Result<&ConfirmedPolicy, &Causes>,
    specification: &Admitted<VersionedIdentity<Id>>,
    (vocabulary, canonicalizer): (
        Result<&SurfaceVocabulary, &Causes>,
        Result<&Canonicalizer<P>, &Causes>,
    ),
    locales: &ResolvedLocales,
    coverage: Admitted<Option<&Coverage>>,
    observer: Option<&DeclarationObserver<'_>>,
) -> Result<Admitted<CoveragePolicy<Id>>, LocaleInvariant> {
    let policy = confirmed.map_err(Causes::clone);
    let vocabulary = vocabulary.map_err(Causes::clone);
    let requested = locales.requested_set.as_ref().map_err(Causes::clone);
    let rules = coverage
        .as_ref()
        .ok()
        .copied()
        .flatten()
        .and_then(|coverage| coverage.rules.as_option())
        .map_or(&[][..], Vec::as_slice);
    let default_declared = coverage
        .as_ref()
        .ok()
        .copied()
        .flatten()
        .is_some_and(|coverage| coverage.default_mode.as_option().is_some());
    let declared = both(
        both(policy.clone(), specification.clone()),
        coverage.clone(),
    )
    .map(|((policy, _), _)| policy);
    // The rules are in the declaration once one is submitted, and no count
    // over none is past its bound.
    let at_rules = if rules.is_empty() {
        None
    } else {
        beside(confirmed, observer, |observer| {
            observer.value(&["coverage", "rules"])
        })?
    };
    let rules_bounded = count(
        evaluation,
        (&declared, at_rules.clone()),
        CoverageSubject::Rules,
        MAX_COVERAGE_RULES,
        |limits| limits.max_coverage_rules,
        rules.len(),
    )?;
    let locale_items_bounded = count(
        evaluation,
        (&declared, at_rules.clone()),
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
        (&declared, at_rules.clone()),
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
        observer,
        policy: confirmed.ok().map(ConfirmedPolicy::evidence),
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
    // The cells span the requested locales, the factor the declaration
    // supplies, observed beside the policy alone (plan 025 decision #198).
    let at_requested = match &requested {
        Ok(_) => beside(confirmed, observer, |observer| {
            observer.value(&["requestedLocales"])
        })?,
        Err(_) => None,
    };
    let span = both(requested, vocabulary);
    let sized = evaluation.check_found(
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
            .map_err(|blocking| Found::through(blocking, at_requested))
        },
    )?;
    let compared = evaluation.check_found(
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
            .map_err(|blocking| Found::through(blocking, at_rules))
        },
    )?;
    let default = coverage.map(|coverage| {
        coverage
            .and_then(|coverage| coverage.default_mode.as_option().copied())
            .unwrap_or(CoverageMode::DirectRequired)
    });
    let decidable =
        both(default.clone(), both(checked.clone(), compared)).map(|(default, _)| default);
    // A cell is named only once the bound proves the table may be built, and
    // every cell waits on the comparison bound. With no cell to wait on them,
    // the rules' own checks still decide whether the policy exists.
    let cells = match both(span.clone(), sized) {
        Ok(((requested, vocabulary), ())) => decide(
            evaluation,
            (&decidable, &selectors),
            requested,
            vocabulary,
            &domains,
        )?,
        Err(causes) => Err(causes),
    };
    // The table owns every cell's Basis and its Coverage Decision Evidence,
    // so neither exists unless every cell is decided (D15:1609; plan 025
    // decision #225). The Evidence observes the decision sources in the
    // declaration's file.
    let owner = unit(CoverageSubject::Table, Check::CompleteTable);
    let table = evaluation.check(owner.clone(), cells, Ok)?;
    if let Ok(cells) = &table {
        let decisions = match observer {
            Some(observer) => decision_evidence(cells, &domains, (observer, default_declared))?,
            None => Vec::new(),
        };
        evaluation.decided(&owner, cells.clone(), decisions)?;
    }
    // The default mode exists whenever the declared coverage does, which the
    // rule counts wait on too, and the span whenever the table does, so
    // neither adds a cause of its own.
    Ok(both(
        both(both(specification.clone(), checked), table),
        both(default, span),
    )
    .map(
        |(((specification, ()), cells), (default_mode, (requested, vocabulary)))| CoveragePolicy {
            specification,
            vocabulary: vocabulary.clone(),
            requested: requested.clone(),
            default_mode,
            cells,
        },
    ))
}

/// A count's unit: every submitted item counts, within its bound, and past
/// it what the count observed is found.
fn count(
    evaluation: &mut Evaluation,
    (declared, observed): (&Admitted<&ConfirmedPolicy>, Option<Observation>),
    subject: CoverageSubject,
    bound: BoundId,
    limit: fn(&LocaleResolutionLimits) -> Bound,
    total: usize,
) -> Result<Admitted<()>, EvaluationInvariant> {
    evaluation.check_found(
        unit(subject, Check::WithinBound),
        declared.clone(),
        |policy| {
            within(bound, limit(&policy.limits().locale_resolution), total)
                .map_err(|blocking| Found::through(blocking, observed))
        },
    )
}

/// What the selector items of every rule wait on: the canonicalizer, once the
/// rule, locale-item, and occurrence counts passed; the vocabulary, once the
/// rule and surface-item counts passed; and the sets an omitted selector
/// stands for. And where an item is observed: in the declaration's file
/// when it is there, beside the confirmed policy's Evidence when there is
/// one.
struct Selectors<'a, P: Provider> {
    spelling: Admitted<&'a Canonicalizer<P>>,
    naming: Admitted<&'a SurfaceVocabulary>,
    requested: Admitted<&'a LocaleSet>,
    vocabulary: Admitted<&'a SurfaceVocabulary>,
    observer: Option<&'a DeclarationObserver<'a>>,
    policy: Option<&'a EvidenceProjection>,
}

impl<P: Provider> Selectors<'_, P> {
    /// The items at `positions` of the selector `selector` of each rule at
    /// `raws`, rule by rule, each at its token, or none without the
    /// declaration's file: each raw rule is a parent of its own (D15:790,
    /// 2190).
    fn items(
        &self,
        raws: &[usize],
        selector: &str,
        positions: &[usize],
    ) -> Result<Vec<Vec<EvidenceProjection>>, ObserverInvariant> {
        let Some(observer) = self.observer else {
            return Ok(Vec::new());
        };
        let index = |at: usize| u64::try_from(at).map_err(|_| ObserverInvariant::Missing);
        raws.iter()
            .map(|&rule| {
                positions
                    .iter()
                    .map(|&position| {
                        observer.nested(&[
                            Edge::Member("coverage"),
                            Edge::Member("rules"),
                            Edge::Index(index(rule)?),
                            Edge::Member(selector),
                            Edge::Index(index(position)?),
                        ])
                    })
                    .collect()
            })
            .collect()
    }

    /// The rules at `positions`, each at its token, or none without the
    /// declaration's file (D15:790).
    fn rules(
        &self,
        positions: &[usize],
    ) -> Result<Vec<Option<EvidenceProjection>>, ObserverInvariant> {
        let Some(observer) = self.observer else {
            return Ok(Vec::new());
        };
        positions
            .iter()
            .map(|&position| rule_token(observer, position).map(Some))
            .collect()
    }
}

/// Each distinct rule's selector units and its unit normalizing them to a
/// domain, then one unit per domain: no two rule occurrences may share one.
/// Returns whether every rule normalized with a domain of its own, and each
/// domain with how many rules hold it and the mode of the first.
fn normalize<P: Provider>(
    evaluation: &mut Evaluation,
    selectors: &Selectors<'_, P>,
    rules: &[CoverageRule],
) -> Result<(Admitted<()>, Domains), LocaleInvariant> {
    // Each distinct rule with the positions of the rules holding it.
    let mut contents: BTreeMap<RuleContent, (Vec<usize>, &CoverageRule)> = BTreeMap::new();
    for (position, rule) in rules.iter().enumerate() {
        contents
            .entry(RuleContent::of(rule))
            .or_insert((Vec::new(), rule))
            .0
            .push(position);
    }
    let mut normalized = Ok(());
    let mut domains = Domains::new();
    for (content, (raws, rule)) in &contents {
        let locales = match rule.requested_locales() {
            Some(items) => selected_locales(evaluation, selectors, (content, raws), items)?,
            None => selectors
                .requested
                .clone()
                .map(|requested| requested.as_slice().to_vec()),
        };
        let surfaces = match rule.intent_surface_classes() {
            Some(items) => selected_surfaces(evaluation, selectors, (content, raws), items)?,
            None => selectors
                .vocabulary
                .clone()
                .map(|vocabulary| vocabulary.members().to_vec()),
        };
        let domain = evaluation.check(
            unit(
                CoverageSubject::Rule(content.clone()),
                Check::NormalizedDomain,
            ),
            both(locales, surfaces),
            |(locales, surfaces)| Ok(Domain { locales, surfaces }),
        )?;
        match domain {
            Ok(domain) => {
                let (holding, _) = domains.entry(domain).or_insert((Vec::new(), rule.mode()));
                holding.extend(raws);
                holding.sort_unstable();
            }
            Err(causes) => normalized = both(normalized, Err::<(), _>(causes)).map(|_| ()),
        }
    }
    for (domain, (holding, _)) in &domains {
        // Only a rule that normalized holds a domain, so what the domain waits
        // on, those rules' units, has passed. Rules sharing it are found each
        // at its own rule, beside the others (D15:2387, 2410).
        let sharing = if holding.len() > 1 {
            selectors.rules(holding)?
        } else {
            Vec::new()
        };
        let unique = evaluation.check_found(
            unit(CoverageSubject::Domain(domain.clone()), Check::UniqueDomain),
            Ok(()),
            |()| {
                if holding.len() == 1 {
                    Ok(())
                } else {
                    Err(Found::repeated(
                        issue(Code::CoverageRuleDuplicate, None),
                        &sharing,
                    ))
                }
            },
        )?;
        normalized = both(normalized, unique).map(|_| ());
    }
    Ok((normalized, domains))
}

/// The member a rule names its locale selector by.
const REQUESTED_LOCALES: &str = "requestedLocales";

/// One rule's locale selector: each item canonicalized, with one unit per
/// subject as Phase 7 does, and each locale named once and requested. Returns
/// the selector's canonical locales as a set once every unit passed.
fn selected_locales<P: Provider>(
    evaluation: &mut Evaluation,
    selectors: &Selectors<'_, P>,
    (rule, raws): (&RuleContent, &[usize]),
    items: &[String],
) -> Result<Admitted<Vec<CanonicalLocale>>, LocaleInvariant> {
    // Each subject's result and the positions of the items that spell it,
    // each with whether it differs from the canonical form.
    let mut subjects: BTreeMap<LocaleKey, (Attempt, Vec<(usize, bool)>)> = BTreeMap::new();
    for (position, raw) in items.iter().enumerate() {
        // An item's key can stand for several spellings, under a rule keyed
        // by its content: past the byte bound it reports only the first-over
        // witness (Decision 015-157).
        let attempt = match selectors.spelling {
            Ok(canonicalizer) => {
                Ok(canonicalize(canonicalizer, raw)?.map_err(Blocking::first_over))
            }
            Err(ref causes) => Err(causes.clone()),
        };
        let (key, corrected) = match &attempt {
            Ok(Ok((locale, corrected))) => (LocaleKey::Canonical(locale.clone()), *corrected),
            _ => (LocaleKey::Content(Content::new(raw)), false),
        };
        let attempt = attempt.map(|attempt| attempt.map(|(locale, _)| locale));
        subjects
            .entry(key)
            .or_insert((attempt, Vec::new()))
            .1
            .push((position, corrected));
    }
    let mut selected = Ok(Vec::with_capacity(subjects.len()));
    for (key, spelled) in subjects {
        let locale = locale_units(evaluation, selectors, (rule, raws), &key, spelled)?;
        selected = both(selected, locale).map(|(mut selected, locale)| {
            selected.push(locale);
            selected
        });
    }
    Ok(selected)
}

/// A locale item's canonicalization, before its unit records it.
type Attempt = Admitted<Result<CanonicalLocale, Blocking>>;

/// The three units of one subject of a rule's locale selector. Each rule
/// holding the selector is a raw parent of its own, so a unit's candidates
/// are the items spelling its subject in every such rule, each at its token
/// (D15:2190, 2387): one past the identifier bound beside the confirmed
/// policy, which supplies the bound; a corrected one as its notice's
/// occurrence; a repeated one beside the others repeating it in the same
/// rule (D15:1567, 2410); and one outside the requested locales beside them,
/// a set that counts as one location (D15:2249). Returns the subject's
/// canonical locale once every unit passed.
fn locale_units<P: Provider>(
    evaluation: &mut Evaluation,
    selectors: &Selectors<'_, P>,
    (rule, raws): (&RuleContent, &[usize]),
    key: &LocaleKey,
    (attempt, spellings): (Attempt, Vec<(usize, bool)>),
) -> Result<Admitted<CanonicalLocale>, LocaleInvariant> {
    let subject = |check| unit(CoverageSubject::Locale(rule.clone(), key.clone()), check);
    let positions: Vec<usize> = spellings.iter().map(|&(position, _)| position).collect();
    let failed = match &attempt {
        Ok(Err(blocking)) => {
            let related = match (blocking.code, selectors.policy) {
                (Code::ResourceLimitExceeded, Some(policy)) => vec![policy.clone()],
                _ => Vec::new(),
            };
            observed(
                selectors.items(raws, REQUESTED_LOCALES, &positions)?,
                &related,
            )
        }
        _ => Vec::new(),
    };
    let locale = evaluation.check_found(subject(Check::CanonicalLocale), attempt, |attempt| {
        attempt.map_err(|blocking| Found::each(blocking, failed))
    })?;
    if let Ok(canonical) = &locale {
        let corrected: Vec<usize> = spellings
            .iter()
            .filter(|&&(_, corrected)| corrected)
            .map(|&(position, _)| position)
            .collect();
        let mut notices = observed(selectors.items(raws, REQUESTED_LOCALES, &corrected)?, &[]);
        // Without the declaration's file, the subject's one notice.
        if notices.is_empty() && !corrected.is_empty() {
            notices.push(None);
        }
        for observation in notices {
            evaluation.notice(
                subject(Check::CanonicalLocale),
                Notice::non_canonical(canonical.clone()),
                observation,
            )?;
        }
    }
    // Each rule compares only its own items.
    let repeated = if positions.len() > 1 {
        selectors.items(raws, REQUESTED_LOCALES, &positions)?
    } else {
        Vec::new()
    };
    let once = evaluation.check_found(subject(Check::UniqueLocale), locale.clone(), |_| {
        if positions.len() == 1 {
            Ok(())
        } else {
            Err(each_rule(issue(Code::LocaleDuplicate, None), repeated))
        }
    })?;
    let outside = match (selectors.observer, &locale, &selectors.requested) {
        (Some(observer), Ok(locale), Ok(requested)) if !requested.contains(locale) => observed(
            selectors.items(raws, REQUESTED_LOCALES, &positions)?,
            &[observer.value(&[REQUESTED_LOCALES])?],
        ),
        _ => Vec::new(),
    };
    let member = evaluation.check_found(
        subject(Check::RequestedMember),
        both(locale.clone(), selectors.requested.clone()),
        |(locale, requested)| {
            if requested.contains(&locale) {
                Ok(())
            } else {
                Err(Found::each(
                    issue(Code::CoverageRuleInvalid, Some(Reason::LocaleNotRequested)),
                    outside,
                ))
            }
        },
    )?;
    Ok(both(locale, both(once, member)).map(|(locale, _)| locale))
}

/// What each item at `items` observed, beside `related`.
fn observed(
    items: Vec<Vec<EvidenceProjection>>,
    related: &[EvidenceProjection],
) -> Vec<Option<Observation>> {
    items
        .into_iter()
        .flatten()
        .map(|primary| {
            Some(Observation {
                primary,
                related: related.to_vec(),
            })
        })
        .collect()
}

/// One candidate of `blocking` for each of the rules' repeated items, each
/// beside the other items repeating it in the same rule (D15:1567, 2410), or
/// the one candidate without the declaration's file.
fn each_rule(blocking: Blocking, rules: Vec<Vec<EvidenceProjection>>) -> Found {
    rules
        .into_iter()
        .map(|items| Found::repeated(blocking, &items.into_iter().map(Some).collect::<Vec<_>>()))
        .reduce(Found::and)
        .unwrap_or_else(|| Found::from(blocking))
}

/// The member a rule names its surface-class selector by.
const INTENT_SURFACE_CLASSES: &str = "intentSurfaceClasses";

/// One rule's surface-class selector: each item a member of the admitted
/// vocabulary and each member named once. A repeated item that names no
/// member is unknown, not a duplicate. Each rule holding the selector is a
/// raw parent of its own, so a unit's candidates are the items naming its
/// subject in every such rule, each at its token (D15:2190, 2387): an
/// unknown one beside the vocabulary's root, which it was compared with
/// (D15:2249), and a repeated one beside the others repeating it in the same
/// rule (D15:1567, 2410). Returns the selector's surface classes as a set
/// once every unit passed.
fn selected_surfaces<P: Provider>(
    evaluation: &mut Evaluation,
    selectors: &Selectors<'_, P>,
    (rule, raws): (&RuleContent, &[usize]),
    items: &[String],
) -> Result<Admitted<Vec<Token>>, LocaleInvariant> {
    let naming = &selectors.naming;
    // Each subject with an item that spells it and the positions of the
    // items that do.
    let mut subjects: BTreeMap<SurfaceKey, (&str, Vec<usize>)> = BTreeMap::new();
    for (position, raw) in items.iter().enumerate() {
        let key = match naming
            .as_ref()
            .ok()
            .and_then(|vocabulary| vocabulary.member(raw))
        {
            Some(member) => SurfaceKey::Member(member.clone()),
            None => SurfaceKey::Content(Content::new(raw)),
        };
        subjects
            .entry(key)
            .or_insert((raw, Vec::new()))
            .1
            .push(position);
    }
    let mut selected = Ok(Vec::with_capacity(subjects.len()));
    for (key, (raw, positions)) in subjects {
        let subject = |check| {
            unit(
                CoverageSubject::SurfaceClass(rule.clone(), key.clone()),
                check,
            )
        };
        let unknown = match naming {
            Ok(vocabulary) if vocabulary.member(raw).is_none() => observed(
                selectors.items(raws, INTENT_SURFACE_CLASSES, &positions)?,
                &[EvidenceProjection::resolver_input(
                    ComponentPath::vocabulary_root(),
                )],
            ),
            _ => Vec::new(),
        };
        let member = evaluation.check_found(
            subject(Check::VocabularyMember),
            naming.clone(),
            |vocabulary| {
                vocabulary.member(raw).cloned().ok_or_else(|| {
                    Found::each(
                        issue(Code::CoverageRuleInvalid, Some(Reason::SurfaceClassUnknown)),
                        unknown,
                    )
                })
            },
        )?;
        // Each rule compares only its own items.
        let repeated = if positions.len() > 1 {
            selectors.items(raws, INTENT_SURFACE_CLASSES, &positions)?
        } else {
            Vec::new()
        };
        let once =
            evaluation.check_found(subject(Check::UniqueSurfaceClass), member.clone(), |_| {
                if positions.len() == 1 {
                    Ok(())
                } else {
                    Err(each_rule(
                        issue(
                            Code::CoverageRuleInvalid,
                            Some(Reason::SurfaceClassDuplicate),
                        ),
                        repeated,
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
/// bound passed. A cell whose maximally specific rules select two modes is
/// found at each of those rules, beside the others (D15:1609, 2410).
fn decide<P: Provider>(
    evaluation: &mut Evaluation,
    (decidable, selectors): (&Admitted<CoverageMode>, &Selectors<'_, P>),
    requested: &LocaleSet,
    vocabulary: &SurfaceVocabulary,
    domains: &Domains,
) -> Result<Admitted<Cells>, LocaleInvariant> {
    let mut cells = Ok(BTreeMap::new());
    for locale in requested.as_slice() {
        for surface in vocabulary.members() {
            let maximal = maximal(locale, surface, domains);
            // Every domain is a rule's own once the cell is decidable.
            let conflicting = if decidable.is_ok() && conflict(&maximal) {
                let positions: Vec<usize> = maximal
                    .iter()
                    .flat_map(|(_, (holding, _))| holding.iter().copied())
                    .collect();
                selectors.rules(&positions)?
            } else {
                Vec::new()
            };
            let decided = evaluation.check_found(
                unit(
                    CoverageSubject::Cell(locale.clone(), surface.clone()),
                    Check::EffectiveMode,
                ),
                decidable.clone(),
                |default| {
                    basis(default, &maximal)
                        .map_err(|blocking| Found::repeated(blocking, &conflicting))
                },
            )?;
            cells = both(cells, decided).map(|(mut cells, basis)| {
                cells.insert((locale.clone(), surface.clone()), basis);
                cells
            });
        }
    }
    Ok(cells)
}

/// The rules matching one cell that no narrower matching rule outranks, each
/// with its domain, in domain order.
fn maximal<'d>(
    locale: &CanonicalLocale,
    surface: &Token,
    domains: &'d Domains,
) -> Vec<(&'d Domain, &'d (Vec<usize>, CoverageMode))> {
    let matching: Vec<_> = domains
        .iter()
        .filter(|(domain, _)| domain.contains(locale, surface))
        .collect();
    matching
        .iter()
        .filter(|(domain, _)| {
            !matching
                .iter()
                .any(|(other, _)| other.narrower_than(domain))
        })
        .copied()
        .collect()
}

/// Whether the maximally specific rules select two modes.
fn conflict(maximal: &[(&Domain, &(Vec<usize>, CoverageMode))]) -> bool {
    maximal.windows(2).any(|pair| pair[0].1 .1 != pair[1].1 .1)
}

/// One cell's decision from its maximally specific rules. None is the
/// default; rules of one mode decide it together; two modes conflict,
/// whatever order the rules were authored in.
fn basis(
    default: CoverageMode,
    maximal: &[(&Domain, &(Vec<usize>, CoverageMode))],
) -> Result<Basis, Blocking> {
    let Some(&(_, &(_, mode))) = maximal.first() else {
        return Ok(Basis {
            mode: default,
            decision: Decision::Default,
        });
    };
    if conflict(maximal) {
        Err(issue(Code::CoverageRuleConflict, None))
    } else {
        Ok(Basis {
            mode,
            decision: Decision::Rules(
                maximal
                    .iter()
                    .map(|(domain, _)| (*domain).clone())
                    .collect(),
            ),
        })
    }
}

/// The Coverage Decision Evidence of an established table, observed in the
/// declaration's file: one mapping for each `default` Basis, to the declared
/// project default or, when it is omitted, to the Coverage Specification
/// input, and one for each matched domain of a `rule` Basis, to the one rule
/// holding it, in cell order and then domain order (D15:1609, 1611). A rule
/// deciding several cells is mapped once for each.
fn decision_evidence(
    cells: &Cells,
    domains: &Domains,
    (observer, declared): (&DeclarationObserver<'_>, bool),
) -> Result<Vec<DecisionEvidence>, ObserverInvariant> {
    let default = if declared {
        observer.value(&["coverage", "defaultMode"])?
    } else {
        EvidenceProjection::resolver_input(ComponentPath::specification(Slot::Coverage))
    };
    let mut decisions = Vec::new();
    for ((locale, surface), basis) in cells {
        let cell = (locale.clone(), surface.clone());
        match &basis.decision {
            Decision::Default => decisions.push(DecisionEvidence {
                cell,
                source: None,
                evidence: default.clone(),
            }),
            Decision::Rules(matched) => {
                for domain in matched {
                    // An established table rejected every duplicate domain,
                    // so one rule holds each matched domain (D15:1611).
                    let (holding, _) = &domains[domain];
                    decisions.push(DecisionEvidence {
                        cell: cell.clone(),
                        source: Some(domain.clone()),
                        evidence: rule_token(observer, holding[0])?,
                    });
                }
            }
        }
    }
    Ok(decisions)
}

/// The rule at `position`, at its token (D15:790).
fn rule_token(
    observer: &DeclarationObserver<'_>,
    position: usize,
) -> Result<EvidenceProjection, ObserverInvariant> {
    let position = u64::try_from(position).map_err(|_| ObserverInvariant::Missing)?;
    observer.nested(&[
        Edge::Member("coverage"),
        Edge::Member("rules"),
        Edge::Index(position),
    ])
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
        details: None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;
    use crate::fixtures::{minimal_config, FixtureTargetReference};
    use crate::model::CoverageMode::{DirectRequired, FallbackAllowed};
    use crate::resolver::declaration_observer::fixtures::Observed;
    use crate::resolver::declaration_observer::DeclarationObserver;
    use crate::resolver::evaluation::Status;
    use crate::resolver::fallback::FallbackSubject;
    use crate::resolver::locales::fixtures::{
        canonical, canonicalizer, content, declaration, key, limits, notices, rejected, stand_in,
        units, waits_on, Declaration,
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
        resolve_observed(fields, setup, None)
    }

    /// The same, where Phase 7 observes what it finds through `observer`.
    fn resolve_observed(
        fields: &LocaleFields<'_, FixtureTargetReference>,
        setup: &Setup<'_>,
        observer: Option<&DeclarationObserver<'_>>,
    ) -> Resolved {
        let mut evaluation = Evaluation::default();
        let canonicalizer = canonicalizer(setup.identifier_bytes);
        // The phases after Phase 5 observe through what it bound.
        let observer = observer.map(|observer| observer.with_locales(&canonicalizer));
        let locales = resolve_locales(
            &mut evaluation,
            Ok(&locales::fixtures::policy(
                setup.identifier_bytes,
                setup.occurrences,
                100,
            )),
            Ok(&canonicalizer),
            fields,
            observer.as_ref(),
        )
        .unwrap();
        let policy = resolve_coverage(
            &mut evaluation,
            setup.policy.as_ref(),
            &setup.specification,
            (setup.vocabulary, Ok(&canonicalizer)),
            &locales,
            fields.coverage.clone(),
            observer.as_ref(),
        )
        .unwrap();
        Resolved { evaluation, policy }
    }

    /// Phases 7 and 8 over `declaration`, observed in it as a file writes it.
    fn resolve_under(
        declaration: &Declaration,
        bounds: Bounds,
        vocabulary: &SurfaceVocabulary,
    ) -> Resolved {
        Observed::declaration(declaration).app(|observer| {
            resolve_observed(
                &LocaleFields::declared(declaration),
                &setup(bounds, vocabulary),
                Some(observer),
            )
        })
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

    /// What each of Phase 8's occurrence candidates observed, with the unit
    /// of its Finding.
    fn observed(evaluation: &Evaluation) -> Vec<(Unit, Observation)> {
        evaluation
            .occurrence_candidates()
            .iter()
            .filter(|candidate| candidate.finding.unit().phase == Phase::LocalePolicyResolution)
            .map(|candidate| {
                (
                    candidate.finding.unit().clone(),
                    Observation {
                        primary: candidate.primary.clone(),
                        related: candidate.related.clone(),
                    },
                )
            })
            .collect()
    }

    /// The value at `path` in `declaration`, observed as the phases after
    /// Phase 5 observe it, beside the confirmed policy, which supplies a
    /// bound.
    fn beside_policy(declaration: &Declaration, path: &[&str]) -> Observation {
        let canonicalizer = canonicalizer(64);
        let primary = Observed::declaration(declaration)
            .app(|observer| observer.with_locales(&canonicalizer).value(path).unwrap());
        Observation {
            primary,
            related: vec![policy(ROOM).evidence().clone()],
        }
    }

    /// The item at `position` of the locale selector of the rule at `rule` in
    /// `declaration`, at its token.
    fn locale_item(declaration: &Declaration, rule: u64, position: u64) -> EvidenceProjection {
        selector_item(declaration, "requestedLocales", rule, position)
    }

    /// The rule at `rule` in `declaration`, at its token.
    fn rule_at(declaration: &Declaration, rule: u64) -> EvidenceProjection {
        use crate::structural::Edge::{Index, Member};
        let canonicalizer = canonicalizer(64);
        Observed::declaration(declaration).app(|observer| {
            observer
                .with_locales(&canonicalizer)
                .nested(&[Member("coverage"), Member("rules"), Index(rule)])
                .unwrap()
        })
    }

    /// Each of two rules found at its token, beside the other, as Findings of
    /// `unit`.
    fn each_beside_the_other(
        declaration: &Declaration,
        unit: &Unit,
        [first, second]: [u64; 2],
    ) -> [(Unit, Observation); 2] {
        let beside = |primary, related| {
            (
                unit.clone(),
                Observation {
                    primary: rule_at(declaration, primary),
                    related: vec![rule_at(declaration, related)],
                },
            )
        };
        [beside(first, second), beside(second, first)]
    }

    /// The item at `position` of the selector `selector` of the rule at
    /// `rule` in `declaration`, at its token.
    fn selector_item(
        declaration: &Declaration,
        selector: &str,
        rule: u64,
        position: u64,
    ) -> EvidenceProjection {
        use crate::structural::Edge::{Index, Member};
        let canonicalizer = canonicalizer(64);
        Observed::declaration(declaration).app(|observer| {
            observer
                .with_locales(&canonicalizer)
                .nested(&[
                    Member("coverage"),
                    Member("rules"),
                    Index(rule),
                    Member(selector),
                    Index(position),
                ])
                .unwrap()
        })
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

    /// Past `limit`, the first-over witness, which a key that can stand for
    /// several values reports.
    fn first_over(bound: BoundId, limit: u64) -> LimitExceeded {
        LimitExceeded {
            bound,
            limit: Bound::new(limit).unwrap(),
            actual: Actual::AtLeast(limit + 1),
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

    // A rule has no identity before normalization, so every rule shares one
    // logical unit, and so do the items of one key under every rule (Decision
    // 015-174). These helpers give the logical units the evaluation records,
    // whose rule and surface content no longer tell the raw values apart.

    fn locale_unit(owner: &RuleContent, key: LocaleKey, check: Check) -> Unit {
        unit(CoverageSubject::Locale(owner.clone(), key), check).logical()
    }

    fn member_unit(owner: &RuleContent, member: &str, check: Check) -> Unit {
        unit(
            CoverageSubject::SurfaceClass(owner.clone(), SurfaceKey::Member(token(member))),
            check,
        )
        .logical()
    }

    fn unknown_unit(owner: &RuleContent, raw: &str, check: Check) -> Unit {
        unit(
            CoverageSubject::SurfaceClass(owner.clone(), SurfaceKey::Content(Content::new(raw))),
            check,
        )
        .logical()
    }

    fn rule_unit(owner: &RuleContent) -> Unit {
        unit(
            CoverageSubject::Rule(owner.clone()),
            Check::NormalizedDomain,
        )
        .logical()
    }

    fn domain_unit(domain: Domain) -> Unit {
        unit(CoverageSubject::Domain(domain), Check::UniqueDomain)
    }

    fn table_unit() -> Unit {
        unit(CoverageSubject::Table, Check::CompleteTable)
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
            CoverageSubject::Locale(locale_only.clone(), content("zz")),
            CoverageSubject::SurfaceClass(surface_only.clone(), SurfaceKey::Member(token("nav"))),
            CoverageSubject::SurfaceClass(
                surface_only.clone(),
                SurfaceKey::Content(Content::new("help")),
            ),
            // A rule waits on its items, and a domain on the rules holding it.
            CoverageSubject::Rule(surface_only),
            CoverageSubject::Rule(locale_only),
            CoverageSubject::Domain(domain(&["en", "fr"], &[])),
            CoverageSubject::Domain(domain(&["fr"], &["nav"])),
            CoverageSubject::Cells,
            CoverageSubject::Comparisons,
            CoverageSubject::Cell(canonical("en"), token("nav")),
            CoverageSubject::Cell(canonical("fr"), token("checkout")),
            // The table waits on every cell.
            CoverageSubject::Table,
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
        let declaration = declared(json!({"rules": [locale_only, named]}));
        let resolved = resolve(&declaration);
        let duplicate = domain_unit(domain(&["fr", "ja"], &EVERY_SURFACE));
        assert_eq!(
            issues(&resolved.evaluation),
            [(
                duplicate.clone(),
                "project-profile-coverage-rule-duplicate",
                None
            )]
        );
        // The second rule's content orders first, but the rules are found in
        // their declared order.
        assert_eq!(
            observed(&resolved.evaluation),
            each_beside_the_other(&declaration, &duplicate, [0, 1])
        );
        for cell in cell_units() {
            assert_eq!(
                waits_on(&resolved.evaluation, &cell),
                Some(vec![duplicate.clone()])
            );
        }
        assert_eq!(units(&resolved.policy.unwrap_err()), [duplicate]);
    }

    /// Every rule shares the one rule unit, identical rules included.
    #[test]
    fn identical_or_aliased_rules_are_duplicates_and_share_the_one_rule_unit() {
        let fr = json!({"requestedLocales": ["fr"], "mode": "fallback-allowed"});
        let declaration = declared(json!({"rules": [fr, fr]}));
        let resolved = resolve(&declaration);
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
        // The two occurrences share one rule unit, which normalizes them to
        // the domain they both hold.
        assert_eq!(
            statuses(&resolved.evaluation)
                .into_iter()
                .filter(|(unit, _)| matches!(
                    &unit.subject,
                    Subject::Coverage(CoverageSubject::Rule(_))
                ))
                .collect::<Vec<_>>(),
            [(rule_unit(&owner), Status::Evaluated)]
        );
        let duplicate = domain_unit(domain(&["fr"], &EVERY_SURFACE));
        assert_eq!(
            issues(&resolved.evaluation),
            [(
                duplicate.clone(),
                "project-profile-coverage-rule-duplicate",
                None
            )]
        );
        // Each rule holding the domain is found at its token, beside the
        // other (D15:2387, 2410).
        assert_eq!(
            observed(&resolved.evaluation),
            each_beside_the_other(&declaration, &duplicate, [0, 1])
        );
        // An alias spelling names the same locale as its canonical form.
        let aliased = json!({"requestedLocales": ["EN"], "mode": "fallback-allowed"});
        let declaration = declared(json!({"rules": [
            aliased,
            {"requestedLocales": ["en"], "mode": "fallback-allowed"}
        ]}));
        let resolved = resolve(&declaration);
        let duplicate = domain_unit(domain(&["en"], &EVERY_SURFACE));
        assert_eq!(
            issues(&resolved.evaluation),
            [(
                duplicate.clone(),
                "project-profile-coverage-rule-duplicate",
                None
            )]
        );
        assert_eq!(
            observed(&resolved.evaluation)
                .into_iter()
                .filter(|(unit, _)| unit == &duplicate)
                .collect::<Vec<_>>(),
            each_beside_the_other(&declaration, &duplicate, [0, 1])
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
        let declaration = declared(json!({"rules": [selector]}));
        let resolved = resolve(&declaration);
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
        // Each item is found at its token: `de` beside the requested
        // locales, the corrected `EN` as its notice's occurrence, each `en`
        // beside the other, and `zz` alone (D15:2249, 2410).
        let item = |position| locale_item(&declaration, 0, position);
        let requested = beside_policy(&declaration, &["requestedLocales"]).primary;
        let at = |unit: Unit, primary, related: &[EvidenceProjection]| {
            (
                unit,
                Observation {
                    primary,
                    related: related.to_vec(),
                },
            )
        };
        assert_eq!(
            observed(&resolved.evaluation),
            [
                at(de(Check::RequestedMember), item(2), &[requested]),
                at(en(Check::CanonicalLocale), item(0), &[]),
                at(en(Check::UniqueLocale), item(0), &[item(1)]),
                at(en(Check::UniqueLocale), item(1), &[item(0)]),
                at(zz(Check::CanonicalLocale), item(3), &[]),
            ]
        );
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
        // An invalid rule has no domain: its unit waits on its items, and
        // every cell, so the table, waits on them too.
        let causes = found.map(|(unit, ..)| unit).to_vec();
        assert!(statuses(&resolved.evaluation)
            .iter()
            .all(|(unit, _)| !matches!(
                unit.subject,
                Subject::Coverage(CoverageSubject::Domain(_))
            )));
        assert_eq!(
            waits_on(&resolved.evaluation, &rule_unit(&owner)),
            Some(causes.clone())
        );
        for cell in cell_units().iter().chain([&table_unit()]) {
            assert_eq!(waits_on(&resolved.evaluation, cell), Some(causes.clone()));
        }
        assert_eq!(units(&resolved.policy.unwrap_err()), causes);
    }

    /// Identical rules share their units but are each a raw parent of their
    /// own: a locale repeated in their selector is found at each rule's
    /// items, each beside the other items of its own rule (D15:1567, 2190,
    /// 2387).
    #[test]
    fn identical_rules_are_each_a_parent_of_their_own_items() {
        let repeated = json!({"requestedLocales": ["fr", "fr"], "mode": "fallback-allowed"});
        let declaration = declared(json!({"rules": [repeated, repeated]}));
        let resolved = resolve(&declaration);
        let duplicate = locale_unit(&rule(&repeated), key("fr"), Check::UniqueLocale);
        assert_eq!(
            issues(&resolved.evaluation),
            [(duplicate.clone(), "project-profile-locale-duplicate", None)]
        );
        let beside = |rule, position, other| {
            (
                duplicate.clone(),
                Observation {
                    primary: locale_item(&declaration, rule, position),
                    related: vec![locale_item(&declaration, rule, other)],
                },
            )
        };
        assert_eq!(
            observed(&resolved.evaluation),
            [
                beside(0, 0, 1),
                beside(0, 1, 0),
                beside(1, 0, 1),
                beside(1, 1, 0)
            ]
        );
    }

    /// The same holds for surface items: in identical rules, a repeated
    /// member is found at each rule's items beside the other of its own rule,
    /// and an unknown item at each rule's item beside the vocabulary's root
    /// (D15:1567, 2190, 2387).
    #[test]
    fn identical_rules_are_each_a_parent_of_their_own_surface_items() {
        let selector =
            json!({"intentSurfaceClasses": ["nav", "nav", "help"], "mode": "fallback-allowed"});
        let declaration = declared(json!({"rules": [selector, selector]}));
        let resolved = resolve(&declaration);
        let owner = rule(&selector);
        let duplicate = member_unit(&owner, "nav", Check::UniqueSurfaceClass);
        let unknown = unknown_unit(&owner, "help", Check::VocabularyMember);
        assert_eq!(
            issues(&resolved.evaluation),
            [
                (
                    duplicate.clone(),
                    "project-profile-coverage-rule-invalid",
                    Some("surface-class-duplicate")
                ),
                (
                    unknown.clone(),
                    "project-profile-coverage-rule-invalid",
                    Some("surface-class-unknown")
                ),
            ]
        );
        let item =
            |rule, position| selector_item(&declaration, "intentSurfaceClasses", rule, position);
        let root = EvidenceProjection::resolver_input(ComponentPath::vocabulary_root());
        let at = |unit: &Unit, primary, related: Vec<EvidenceProjection>| {
            (unit.clone(), Observation { primary, related })
        };
        assert_eq!(
            observed(&resolved.evaluation),
            [
                at(&duplicate, item(0, 0), vec![item(0, 1)]),
                at(&duplicate, item(0, 1), vec![item(0, 0)]),
                at(&duplicate, item(1, 0), vec![item(1, 1)]),
                at(&duplicate, item(1, 1), vec![item(1, 0)]),
                at(&unknown, item(0, 2), vec![root.clone()]),
                at(&unknown, item(1, 2), vec![root]),
            ]
        );
    }

    #[test]
    fn a_surface_item_is_a_vocabulary_member_named_once_in_its_selector() {
        let selector = json!({"intentSurfaceClasses": ["nav", "help", "nav", "Nav", "help"], "mode": "fallback-allowed"});
        let declaration = declared(json!({"rules": [selector]}));
        let resolved = resolve(&declaration);
        let owner = rule(&selector);
        assert_eq!(
            issues(&resolved.evaluation),
            [
                (
                    member_unit(&owner, "nav", Check::UniqueSurfaceClass),
                    "project-profile-coverage-rule-invalid",
                    Some("surface-class-duplicate"),
                ),
                // `Nav` and `help` name no member, so they share one unit and
                // give one issue.
                (
                    unknown_unit(&owner, "Nav", Check::VocabularyMember),
                    "project-profile-coverage-rule-invalid",
                    Some("surface-class-unknown"),
                ),
            ]
        );
        // Each `nav` is found beside the other, and each item naming no
        // member beside the vocabulary's root, which it was compared with
        // (D15:1567, 2249, 2410).
        let item = |position| selector_item(&declaration, "intentSurfaceClasses", 0, position);
        let vocabulary_root = EvidenceProjection::resolver_input(ComponentPath::vocabulary_root());
        let duplicate = member_unit(&owner, "nav", Check::UniqueSurfaceClass);
        let unknown = unknown_unit(&owner, "Nav", Check::VocabularyMember);
        let at = |unit: &Unit, primary, related: &[EvidenceProjection]| {
            (
                unit.clone(),
                Observation {
                    primary,
                    related: related.to_vec(),
                },
            )
        };
        assert_eq!(
            observed(&resolved.evaluation),
            [
                at(&duplicate, item(0), &[item(2)]),
                at(&duplicate, item(2), &[item(0)]),
                at(&unknown, item(3), std::slice::from_ref(&vocabulary_root)),
                at(&unknown, item(1), std::slice::from_ref(&vocabulary_root)),
                at(&unknown, item(4), std::slice::from_ref(&vocabulary_root)),
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
                unknown_unit(&owner, "help", Check::VocabularyMember),
            ]
        );
    }

    #[test]
    fn incomparable_maximal_rules_of_two_modes_conflict_only_where_they_overlap() {
        let locale_only = json!({"requestedLocales": ["fr"], "mode": "fallback-allowed"});
        let surface_only = json!({"intentSurfaceClasses": ["checkout"], "mode": "direct-required"});
        let declaration = declared(json!({"rules": [locale_only, surface_only]}));
        let resolved = resolve(&declaration);
        let conflict = cell_unit("fr", "checkout");
        assert_eq!(
            issues(&resolved.evaluation),
            [(
                conflict.clone(),
                "project-profile-coverage-rule-conflict",
                None
            )]
        );
        // Each maximally specific rule is found at its token, beside the
        // other, in domain order: the surface rule spans every locale
        // (D15:1609, 2410).
        assert_eq!(
            observed(&resolved.evaluation),
            each_beside_the_other(&declaration, &conflict, [1, 0])
        );
        assert!(cell_units()
            .iter()
            .all(|cell| resolved.evaluation.status(cell) == Some(&Status::Evaluated)));
        // The table is not complete, so no cell keeps its Basis.
        assert_eq!(
            waits_on(&resolved.evaluation, &table_unit()),
            Some(vec![conflict.clone()])
        );
        assert_eq!(units(&resolved.policy.unwrap_err()), [conflict]);
        // A rule over just the overlap is narrower than both and decides it.
        let resolved = resolve(&declared(json!({"rules": [
            locale_only,
            surface_only,
            {"requestedLocales": ["fr"], "intentSurfaceClasses": ["checkout"], "mode": "fallback-allowed"}
        ]})));
        assert_eq!(issues(&resolved.evaluation), []);
        assert_eq!(waits_on(&resolved.evaluation, &table_unit()), None);
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
        // Past it, the rules are observed beside the confirmed policy, which
        // supplies the bound (D15:1901, 2249).
        assert_eq!(
            observed(&over.evaluation),
            [(
                rules.clone(),
                beside_policy(&declaration, &["coverage", "rules"])
            )]
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
        // Every item sits in a rule, so the rules are the nearest location
        // holding them all.
        assert_eq!(
            observed(&over.evaluation)
                .into_iter()
                .filter(|(unit, _)| unit == &locale_items)
                .collect::<Vec<_>>(),
            [(
                locale_items.clone(),
                beside_policy(&declaration, &["coverage", "rules"])
            )]
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
            observed(&over.evaluation)
                .into_iter()
                .filter(|(unit, _)| unit == &surface_items)
                .collect::<Vec<_>>(),
            [(
                surface_items.clone(),
                beside_policy(&declaration, &["coverage", "rules"])
            )]
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
        assert_eq!(waits_on(&at.evaluation, &table_unit()), None);
        assert_eq!(at.policy.unwrap().basis().len(), 9);
        let over = resolve_under(&declaration, Bounds { cells: 8, ..ROOM }, &vocabulary());
        let cells = count_unit(CoverageSubject::Cells);
        assert_eq!(
            exceeded(&over.evaluation, &cells),
            exactly(MAX_COVERAGE_DECISION_CELLS, 8, 9)
        );
        // Observed at the factor the declaration supplies, beside the policy
        // alone (plan 025 decision #198).
        assert_eq!(
            observed(&over.evaluation),
            [(
                cells.clone(),
                beside_policy(&declaration, &["requestedLocales"])
            )]
        );
        assert!(!statuses(&over.evaluation)
            .iter()
            .any(|(unit, _)| names_a_cell(unit)));
        assert_eq!(
            waits_on(&over.evaluation, &table_unit()),
            Some(vec![cells.clone()])
        );
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
        // Observed at the factor this bound adds to the cells, the rules,
        // beside the policy alone (plan 025 decision #198).
        assert_eq!(
            observed(&over.evaluation)
                .into_iter()
                .filter(|(unit, _)| unit == &compared)
                .collect::<Vec<_>>(),
            [(
                compared.clone(),
                beside_policy(&declaration, &["coverage", "rules"])
            )]
        );
        // The cells are still named, and wait on it.
        for cell in cell_units() {
            assert_eq!(
                waits_on(&over.evaluation, &cell),
                Some(vec![duplicate.clone(), compared.clone()])
            );
        }
    }

    /// The Coverage Decision Evidence `resolved` recorded, as each mapping's
    /// cell, its matched domain or none for the default, and its Evidence.
    fn decisions(resolved: &Resolved) -> Vec<(&str, &str, Option<Domain>, EvidenceProjection)> {
        resolved
            .evaluation
            .decision_evidence()
            .iter()
            .map(|decision| {
                (
                    decision.cell.0.as_str(),
                    decision.cell.1.as_str(),
                    decision.source.clone(),
                    decision.evidence.clone(),
                )
            })
            .collect()
    }

    /// An established table maps each `default` Basis to the Coverage
    /// Specification input when the default is omitted, or to the declared
    /// default, and each matched domain of a `rule` Basis to the rule holding
    /// it, cell by cell, a rule deciding several cells once for each
    /// (D15:1609, 1611).
    #[test]
    fn an_established_table_maps_each_basis_to_its_decision_sources() {
        let fr = json!({"requestedLocales": ["fr"], "mode": "fallback-allowed"});
        let specification =
            EvidenceProjection::resolver_input(ComponentPath::specification(Slot::Coverage));
        for (coverage, default) in [
            (json!({"rules": [fr]}), None),
            (
                json!({"defaultMode": "fallback-allowed", "rules": [fr]}),
                Some(&["coverage", "defaultMode"][..]),
            ),
        ] {
            let declaration = declared(coverage);
            let resolved = resolve(&declaration);
            let default = default.map_or_else(
                || specification.clone(),
                |path| beside_policy(&declaration, path).primary,
            );
            let rule = rule_at(&declaration, 0);
            let mut expected = Vec::new();
            for locale in EVERY_LOCALE {
                for surface in EVERY_SURFACE {
                    expected.push(if locale == "fr" {
                        (
                            locale,
                            surface,
                            Some(domain(&["fr"], &EVERY_SURFACE)),
                            rule.clone(),
                        )
                    } else {
                        (locale, surface, None, default.clone())
                    });
                }
            }
            assert_eq!(decisions(&resolved), expected);
        }
    }

    /// A cell two rules of one mode decide maps to each, in domain order. An
    /// established table owns every cell's Basis, the declaration's file or
    /// not, and a table that is not established keeps neither Basis nor
    /// mapping, nor does an evaluation without the file keep a mapping
    /// (D15:1607, 1609, 1611).
    #[test]
    fn each_matched_domain_is_mapped_and_an_unestablished_table_keeps_none() {
        let locale_only = json!({"requestedLocales": ["fr"], "mode": "fallback-allowed"});
        let same_mode = json!({"intentSurfaceClasses": ["checkout"], "mode": "fallback-allowed"});
        let declaration = declared(json!({"rules": [locale_only, same_mode]}));
        let resolved = resolve(&declaration);
        let basis = resolved.policy.as_ref().unwrap().basis();
        assert_eq!(resolved.evaluation.decision_basis(), basis);
        let both: Vec<_> = decisions(&resolved)
            .into_iter()
            .filter(|(locale, surface, ..)| (*locale, *surface) == ("fr", "checkout"))
            .collect();
        assert_eq!(
            both,
            [
                (
                    "fr",
                    "checkout",
                    Some(domain(&EVERY_LOCALE, &["checkout"])),
                    rule_at(&declaration, 1)
                ),
                (
                    "fr",
                    "checkout",
                    Some(domain(&["fr"], &EVERY_SURFACE)),
                    rule_at(&declaration, 0)
                ),
            ]
        );
        // The rules conflict once their modes differ: no table, no mapping.
        let other_mode = json!({"intentSurfaceClasses": ["checkout"], "mode": "direct-required"});
        let conflicting = resolve(&declared(json!({"rules": [locale_only, other_mode]})));
        assert!(conflicting.policy.is_err());
        assert!(conflicting.evaluation.decision_basis().is_empty());
        assert_eq!(decisions(&conflicting), []);
        // Without the declaration's file, nothing is observed.
        let unobserved = resolve_fields(
            &LocaleFields::declared(&declaration),
            &setup(ROOM, &vocabulary()),
        );
        assert!(unobserved.policy.is_ok());
        assert_eq!(unobserved.evaluation.decision_basis(), basis);
        assert_eq!(decisions(&unobserved), []);
    }

    /// The recorded mappings become Coverage Decision Evidence records under
    /// the invocation's tokens, each owned by the decision table: the declared
    /// rule's Materialized Value Evidence, and the Coverage Specification
    /// input for the omitted default (D15:1609, 2485).
    #[test]
    fn the_recorded_mappings_become_records_under_the_decision_table() {
        use crate::resolver::decision_evidence::{records, redacted};
        use crate::resolver::evidence_token::EvidenceTokens;
        use crate::resolver::origin::EvidenceKind;
        use crate::resolver::token::{Sha256, Specification};
        const PROFILE: Specification<'static> = Specification {
            identity: "intlify-project-profile",
            revision: "0",
        };
        let declaration = declared(json!({
            "rules": [{"requestedLocales": ["fr"], "mode": "fallback-allowed"}],
        }));
        let resolved = resolve(&declaration);
        let candidates = resolved.evaluation.decision_evidence();
        let tokens = EvidenceTokens::assign(PROFILE, redacted(candidates), &Sha256);
        let records = records(candidates, PROFILE, &tokens).unwrap();
        assert_eq!(records.len(), EVERY_LOCALE.len() * EVERY_SURFACE.len());
        let table = unit(CoverageSubject::Table, Check::CompleteTable);
        for record in &records {
            let kind = if record.cell().0.as_str() == "fr" {
                EvidenceKind::ConfigurationValue
            } else {
                EvidenceKind::SpecificationInput
            };
            assert_eq!(record.evidence().kind(), kind);
            assert_eq!(record.owner(), &table);
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
        let table = [
            count_unit(CoverageSubject::Cells),
            count_unit(CoverageSubject::Comparisons),
            table_unit(),
        ];
        let rules = [rule_unit(&owner)];
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
                [counts.as_slice(), &items, &rules, &table].concat(),
            ),
            (
                &declared_fields,
                Setup {
                    policy: Err(unconfirmed.clone()),
                    ..setup(ROOM, &vocabulary)
                },
                &unconfirmed,
                [counts.as_slice(), &items, &rules, &table].concat(),
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
        let table = [
            count_unit(CoverageSubject::Cells),
            count_unit(CoverageSubject::Comparisons),
            rule_unit(&owner),
            table_unit(),
        ];
        let missing = stand_in(Slot::IntentSurfaceClassVocabulary);
        let declaration = declared(json!({"rules": [selector]}));
        let resolved = Observed::declaration(&declaration).app(|observer| {
            resolve_observed(
                &LocaleFields::declared(&declaration),
                &Setup {
                    vocabulary: Err(&missing),
                    ..setup(ROOM, &vocabulary())
                },
                Some(observer),
            )
        });
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
            LocaleSubject::RequestedLocale(rejected()),
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
    fn a_rule_omitting_a_selector_waits_on_the_set_it_stands_for() {
        let surface_only = json!({"intentSurfaceClasses": ["nav"], "mode": "fallback-allowed"});
        let locale_only = json!({"requestedLocales": ["fr"], "mode": "fallback-allowed"});
        // The omitted locale selector stands for a requested set Phase 7 could
        // not form, while the surface item needs no locale.
        let resolved = resolve(&requesting(|app| {
            app["requestedLocales"] = json!(["en", "fr", "ja", "zz"]);
            app["coverage"] = json!({"rules": [surface_only]});
        }));
        let owner = rule(&surface_only);
        let invalid = locales::unit(
            LocaleSubject::RequestedLocale(rejected()),
            Check::CanonicalLocale,
        );
        assert_eq!(
            waits_on(
                &resolved.evaluation,
                &member_unit(&owner, "nav", Check::UniqueSurfaceClass)
            ),
            None
        );
        assert_eq!(
            waits_on(&resolved.evaluation, &rule_unit(&owner)),
            Some(vec![invalid])
        );
        // The omitted surface selector stands for the vocabulary, which is
        // missing, while the locale item needs no vocabulary.
        let missing = stand_in(Slot::IntentSurfaceClassVocabulary);
        let declaration = declared(json!({"rules": [locale_only]}));
        let resolved = Observed::declaration(&declaration).app(|observer| {
            resolve_observed(
                &LocaleFields::declared(&declaration),
                &Setup {
                    vocabulary: Err(&missing),
                    ..setup(ROOM, &vocabulary())
                },
                Some(observer),
            )
        });
        let owner = rule(&locale_only);
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
        assert_eq!(
            waits_on(&resolved.evaluation, &rule_unit(&owner)),
            Some(units(&missing))
        );
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
        let over = Observed::declaration(&declaration).app(|observer| {
            resolve_observed(
                &fields,
                &Setup {
                    occurrences: 6,
                    ..setup(ROOM, &vocabulary)
                },
                Some(observer),
            )
        });
        assert_eq!(
            waits_on(
                &over.evaluation,
                &locale_unit(&owner, content("EN-us"), Check::CanonicalLocale)
            ),
            Some(vec![occurrences])
        );
        let at = Observed::declaration(&declaration).app(|observer| {
            resolve_observed(
                &fields,
                &Setup {
                    occurrences: 7,
                    ..setup(ROOM, &vocabulary)
                },
                Some(observer),
            )
        });
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
        // The item is the notice's occurrence, and found outside the
        // requested locales beside them (D15:2249, 2410).
        let item = locale_item(&declaration, 0, 0);
        let requested = beside_policy(&declaration, &["requestedLocales"]).primary;
        assert_eq!(
            observed(&at.evaluation),
            [
                (
                    corrected(Check::CanonicalLocale),
                    Observation {
                        primary: item.clone(),
                        related: Vec::new(),
                    }
                ),
                (
                    corrected(Check::RequestedMember),
                    Observation {
                        primary: item.clone(),
                        related: vec![requested],
                    }
                ),
            ]
        );
        // Each item is measured in bytes before the provider sees it, and
        // under a rule keyed by its content reports only the first-over
        // witness.
        let short = Observed::declaration(&declaration).app(|observer| {
            resolve_observed(
                &fields,
                &Setup {
                    identifier_bytes: 4,
                    ..setup(ROOM, &vocabulary)
                },
                Some(observer),
            )
        });
        let long = locale_unit(&owner, content("EN-us"), Check::CanonicalLocale);
        assert_eq!(
            exceeded(&short.evaluation, &long),
            first_over(
                BoundId {
                    group: Group::LocaleResolution,
                    name: "maxLocaleIdentifierBytes",
                },
                4
            )
        );
        // Found at the item beside the confirmed policy, which supplies the
        // bound.
        assert_eq!(
            observed(&short.evaluation),
            [(
                long.clone(),
                Observation {
                    primary: item,
                    related: vec![policy(ROOM).evidence().clone()],
                }
            )]
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
        // With no cell to decide, the table is complete at once.
        assert_eq!(waits_on(&resolved.evaluation, &table_unit()), None);
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
        // With no cell to wait on it, the rule still blocks the policy, though
        // the empty table is complete.
        assert_eq!(waits_on(&resolved.evaluation, &table_unit()), None);
        assert_eq!(
            waits_on(&resolved.evaluation, &rule_unit(&rule(&selector))),
            Some(vec![unknown.clone()])
        );
        assert_eq!(units(&resolved.policy.unwrap_err()), [unknown]);
    }
}
