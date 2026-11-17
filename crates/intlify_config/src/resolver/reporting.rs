// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's reporting slices: the records each evaluation unit reports
//! into the two diagnostic domains, before their bounds (D15:1940,
//! 2205-2211).
//!
//! Phases 0 and 1 report their Findings and statuses into the Entry domain,
//! and every later phase into the Shared one, while the occurrences of every
//! Finding, with the Evidence they refer to, are Entry records whatever the
//! phase (D15:1942, 2145, 2369; plan 025 interpretation #222). A unit's Entry
//! slice therefore holds its Entry Findings and status, the occurrences of all
//! its Findings, the Coverage Decision Evidence it owns, the
//! source-evidence-index records it is the first to refer to, and the
//! Shared-unavailability causes that name its Findings; its Shared slice holds
//! its Shared Findings and status and the Coverage Decision Basis it owns
//! (D15:1607-1609, 2207). The unit owns every record of its slices, and units
//! report in canonical unit order, so the earliest unit that refers to an
//! Evidence owns its index record (D15:1972, 2205). The dependency records
//! come later (plan 025 decision #221).
//!
//! A slice has two stages (D15:2209). Its candidates are what the candidate
//! stage counts before any ordinal or record exists: each Finding candidate,
//! as an occurrence candidate in the Entry domain and before key
//! deduplication in the Shared one (D15:1946, 1962, 1970); in the Entry domain
//! each related-Evidence occurrence and `C` of each primary and related
//! Evidence occurrence (D15:1949-1951, 2518); and each causal Finding Key of
//! the slice's not-evaluated status, each value's before they merge into one
//! duplicate-free set (D15:1954, 1966, 2184, 2369; plan 025
//! interpretation #223). Its records are what the record stage builds: each
//! retained record with `C` of its accounting view (D15:2518). The bounds of
//! each stage come with their own stage.
//!
//! Before any slice, the Evidence of every candidate passes path admission,
//! and the resolver's own Evidence metadata work is checked against the
//! capability's `evidenceMetadata` bounds, over the complete invocation and
//! over its Shared scope (D15:1851). A domain whose scope exceeds one of them
//! plans no slice, and only the subject tokens of the Shared scope, when it
//! fits, and those the reserved terminal records name are assigned (plan 025
//! decisions #229, #236, and #247, interpretation #237).

use std::collections::{BTreeMap, BTreeSet};

use intlify_shared_json::encoding::encoded_length;
use intlify_shared_json::token::Token;
use serde_json::{json, Value};

use crate::construction::registry::check::RecordKind;
use crate::locale::CanonicalLocale;

use super::capability::EvidenceMetadataLimits;
use super::coverage::{self, Basis, CoverageSubject, Decision};
use super::decision_evidence::{
    self, CoverageDecisionEvidence, DecisionEvidence, DecisionInvariant,
};
use super::evaluation::{
    Causes, Check, Details, Evaluation, ExhaustedBound, Finding, IssueKey, Phase, Reason, Status,
    Unit,
};
use super::evidence::Evidence;
use super::evidence_metadata::{self, Admission, Exceeded, MetadataBound};
use super::evidence_token::EvidenceTokens;
use super::occurrence::{self, FindingOccurrence, OccurrenceCandidate, OccurrenceInvariant};
use super::record_view::{RecordViews, ViewInvariant};
use super::subject::SubjectKind;
use super::token::{rule_domain, Digest, Specification, Tokens};

/// One of 015's two diagnostic domains (D15:1940).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ReportingDomain {
    Entry,
    Shared,
}

impl ReportingDomain {
    /// The domain of the Findings and status of a unit of `phase`: Phases 0
    /// and 1 report into the Entry domain, every later phase into the Shared
    /// one (D15:2145, 2369).
    pub(crate) const fn of(phase: Phase) -> Self {
        match phase {
            Phase::ResolverBootstrapAdmission | Phase::EntryAdmission => Self::Entry,
            _ => Self::Shared,
        }
    }

    /// The domain's ID, as its terminal state names it (D15:2489).
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::Entry => "entry",
            Self::Shared => "shared",
        }
    }

    /// The stable reason of the domain's terminal reporting-limit Finding,
    /// which equals the domain, so the two domains' Findings have two keys
    /// (D15:2408).
    pub(crate) const fn reason(self) -> Reason {
        match self {
            Self::Entry => Reason::Entry,
            Self::Shared => Reason::Shared,
        }
    }
}

/// What a slice's candidate stage counts (D15:2209).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Candidates {
    /// Each Finding candidate (D15:1946, 1962, 1970).
    pub(crate) findings: u64,
    /// In the Entry domain, each related-Evidence occurrence (D15:1949).
    pub(crate) related: u64,
    /// In the Entry domain, `C` of each primary and related Evidence
    /// occurrence, each candidate's primary first (D15:1950, 1951, 2518).
    pub(crate) evidence: Vec<u64>,
    /// Each causal Finding Key of the slice's not-evaluated status, each
    /// value's before they merge (D15:1954, 1966, 2184).
    pub(crate) causes: u64,
}

/// One retained ordinary record of a slice (D15:2207).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Record<'e> {
    Finding(Finding<'e>),
    Status(&'e Status),
    Occurrence(FindingOccurrence),
    DecisionEvidence(CoverageDecisionEvidence),
    /// A source-evidence-index record.
    Evidence(Evidence),
    /// A Shared-unavailability cause, naming the blocking Finding of this
    /// key.
    UnavailabilityCause(&'e IssueKey),
    DecisionBasis(&'e (CanonicalLocale, Token), &'e Basis),
}

/// A retained record, of its record kind, with `C` of its accounting view
/// (D15:2518).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Retained<'e> {
    pub(crate) kind: RecordKind,
    pub(crate) record: Record<'e>,
    pub(crate) bytes: u64,
}

/// One unit's slice in one domain, as its two stages build it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Slice<'e> {
    pub(crate) unit: &'e Unit,
    pub(crate) domain: ReportingDomain,
    pub(crate) candidates: Candidates,
    pub(crate) records: Vec<Retained<'e>>,
}

/// A resolver bug that leaves a slice without its records, boxed since a
/// rare bug need not widen every result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReportingInvariant {
    View(Box<ViewInvariant>),
    Occurrence(Box<OccurrenceInvariant>),
    Decision(Box<DecisionInvariant>),
}

impl From<ViewInvariant> for ReportingInvariant {
    fn from(invariant: ViewInvariant) -> Self {
        Self::View(Box::new(invariant))
    }
}

impl From<OccurrenceInvariant> for ReportingInvariant {
    fn from(invariant: OccurrenceInvariant) -> Self {
        Self::Occurrence(Box::new(invariant))
    }
}

impl From<DecisionInvariant> for ReportingInvariant {
    fn from(invariant: DecisionInvariant) -> Self {
        Self::Decision(Box::new(invariant))
    }
}

/// The reporting of one evaluation: what each unit it recorded found, under
/// the invocation's subject and evidence tokens.
#[derive(Debug)]
pub(crate) struct Reporting<'e, 's> {
    evaluation: &'e Evaluation,
    specification: Specification<'s>,
    views: RecordViews<'s>,
    tokens: EvidenceTokens,
    /// Why the Shared projection is unavailable, when it is (D15:2369).
    unavailable: Option<&'e Causes>,
    /// The statuses of each unit's raw candidates (D15:2203).
    statuses: BTreeMap<Unit, Vec<&'e Status>>,
    /// How many occurrence candidates each unit that found something has: a
    /// unit that found something reports into the Entry domain (D15:2207).
    found: BTreeMap<&'e Unit, u64>,
    /// The occurrence candidates of each unit's Findings as path admission
    /// left them, in the order the units found them; none when the
    /// resolver's own Evidence metadata work exceeded a bound of the Entry
    /// domain's scope.
    occurrences: BTreeMap<&'e Unit, Vec<OccurrenceCandidate>>,
    /// The Coverage Decision Evidence candidates as path admission left them.
    decisions: Vec<DecisionEvidence>,
    /// Each unit's Findings, in Finding order.
    findings: BTreeMap<&'e Unit, Vec<Finding<'e>>>,
    /// The `evidenceMetadata` bound each domain's scope exceeds, if one does
    /// (D15:1851).
    exceeded: Exceeded,
}

impl<'e, 's> Reporting<'e, 's> {
    /// The reporting of `evaluation`, whose Shared projection `unavailable`
    /// names the causes of when it is absent. Path admission and the
    /// resolver's own Evidence metadata work come first, under the
    /// capability's `limits`, and only within them does `digest` assign the
    /// subject tokens of every unit it recorded and the evidence tokens of
    /// every Evidence its records may carry, together, before any slice is
    /// planned (D15:1851, 2194). When only the complete invocation exceeds a
    /// bound, `digest` assigns the subject tokens of the units that report
    /// into the Shared domain, which reports as within every bound (plan 025
    /// decision #247). The reserved terminal records of a domain whose scope
    /// exceeds a bound name the subject token of its first unit, whose subject
    /// kind's colliding sets are its own, so `digest` assigns the tokens of
    /// that kind's subjects alone (plan 025 interpretation #237).
    pub(crate) fn new(
        evaluation: &'e Evaluation,
        specification: Specification<'s>,
        unavailable: Option<&'e Causes>,
        limits: &EvidenceMetadataLimits,
        digest: &impl Digest,
    ) -> Self {
        let mut statuses: BTreeMap<Unit, Vec<&Status>> = BTreeMap::new();
        for (raw, status) in evaluation.candidates() {
            statuses.entry(raw.logical()).or_default().push(status);
        }
        let mut found: BTreeMap<&Unit, u64> = BTreeMap::new();
        for candidate in evaluation.occurrence_candidates() {
            *found.entry(candidate.finding.unit()).or_default() += 1;
        }
        let mut occurrences: BTreeMap<&Unit, Vec<OccurrenceCandidate>> = BTreeMap::new();
        let mut findings: BTreeMap<&Unit, Vec<Finding<'_>>> = BTreeMap::new();
        for finding in evaluation.findings() {
            findings
                .entry(finding.key.unit())
                .or_default()
                .push(finding);
        }
        let admission = evidence_metadata::admit(evaluation, specification, limits, digest);
        let exceeded = admission.exceeded();
        let no_evidence = || EvidenceTokens::assign(specification, std::iter::empty(), digest);
        // The reserved terminal records of an exhausted domain name the
        // subject token of its first unit, whose subject kind's colliding sets
        // are its own, so the tokens of that kind's subjects alone are the
        // invocation's (plan 025 interpretation #237).
        let reserved = |domains: &[ReportingDomain]| {
            let kinds: BTreeSet<SubjectKind> = domains
                .iter()
                .filter_map(|domain| first(evaluation, &found, *domain))
                .map(|unit| unit.subject.kind())
                .collect();
            let subjects = evaluation
                .statuses()
                .map(|(unit, _)| &unit.subject)
                .filter(|subject| kinds.contains(&subject.kind()));
            Tokens::assign(specification, subjects, digest)
        };
        let (subjects, tokens, decisions) = match admission {
            Admission::Admitted(admitted) => {
                let raw = evaluation.occurrence_candidates();
                for (raw, candidate) in raw.iter().zip(admitted.occurrences) {
                    occurrences
                        .entry(raw.finding.unit())
                        .or_default()
                        .push(candidate);
                }
                (admitted.subjects, admitted.evidence, admitted.decisions)
            }
            Admission::EntryExhausted { shared, .. } => (
                shared.merge(reserved(&[ReportingDomain::Entry])),
                no_evidence(),
                Vec::new(),
            ),
            Admission::Exhausted { .. } => (
                reserved(&[ReportingDomain::Entry, ReportingDomain::Shared]),
                no_evidence(),
                Vec::new(),
            ),
        };
        Self {
            evaluation,
            specification,
            views: RecordViews::new(specification, subjects, digest),
            tokens,
            unavailable,
            statuses,
            found,
            occurrences,
            decisions,
            findings,
            exceeded,
        }
    }

    /// The `evidenceMetadata` bound `domain`'s scope exceeds, if one does:
    /// then the domain plans no ordinary slice (D15:1851; plan 025 decisions
    /// #229 and #247).
    pub(crate) const fn exceeded(&self, domain: ReportingDomain) -> Option<MetadataBound> {
        self.exceeded.of(domain)
    }

    /// The first unit, in canonical unit order, that reports into `domain`.
    pub(crate) fn first(&self, domain: ReportingDomain) -> Option<&'e Unit> {
        first(self.evaluation, &self.found, domain)
    }

    /// Whether `unit` reports into `domain`.
    fn reports_into(&self, unit: &Unit, domain: ReportingDomain) -> bool {
        reports_into(self.evaluation, &self.found, unit, domain)
    }

    /// Every unit the evaluation recorded, in canonical unit order: the
    /// order in which their slices report (D15:2211).
    pub(crate) fn units(&self) -> impl Iterator<Item = &'e Unit> {
        self.evaluation.statuses().map(|(unit, _)| unit)
    }

    /// What the candidate stage of `unit`'s slice in `domain` counts, or
    /// `None` when the unit does not report into the domain.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a component the evidence tokens leave out.
    pub(crate) fn candidates(
        &self,
        unit: &Unit,
        domain: ReportingDomain,
    ) -> Result<Option<Candidates>, ReportingInvariant> {
        if !self.reports_into(unit, domain) {
            return Ok(None);
        }
        let found = self.occurrences.get(unit).map_or(&[][..], Vec::as_slice);
        // Every raw occurrence candidate, whether or not its Evidence was
        // admitted, so the Shared domain counts the same Finding candidates
        // when only the Entry domain is exhausted (plan 025 decision #247).
        let mut candidates = Candidates {
            findings: self.found.get(unit).copied().unwrap_or(0),
            ..Candidates::default()
        };
        // A unit is not evaluated only when its prerequisites exclude every
        // value, each with the keys of the checks that exclude it (D15:2184;
        // plan 025 interpretation #223).
        if let Some(Status::NotEvaluated(_)) = self.evaluation.status(unit) {
            for status in self.statuses.get(unit).into_iter().flatten() {
                if let Status::NotEvaluated(keys) = status {
                    candidates.causes += count(keys.keys().count());
                }
            }
        }
        if domain == ReportingDomain::Entry {
            for candidate in found {
                candidates.related += count(candidate.related.len());
                for projection in std::iter::once(&candidate.primary).chain(&candidate.related) {
                    let evidence = projection
                        .retained(self.specification, &self.tokens)
                        .ok_or_else(|| {
                            OccurrenceInvariant::UnassignedToken(candidate.finding.clone())
                        })?;
                    candidates.evidence.push(bytes(&evidence.view()));
                }
            }
        }
        Ok(Some(candidates))
    }

    /// The records the record stage of `unit`'s slice in `domain` builds, in
    /// the order of their record kinds (D15:2518). `indexed` is the Evidence
    /// that the index records of earlier slices hold: the slice holds an
    /// index record for every other Evidence its records refer to
    /// (D15:1972).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug that leaves a record without its view.
    pub(crate) fn records(
        &self,
        unit: &'e Unit,
        domain: ReportingDomain,
        indexed: &BTreeSet<Evidence>,
    ) -> Result<Vec<Retained<'e>>, ReportingInvariant> {
        let mut records = Vec::new();
        if ReportingDomain::of(unit.phase) == domain {
            let (finding, status) = match domain {
                ReportingDomain::Entry => {
                    (RecordKind::EntryFinding, RecordKind::EntryEvaluationStatus)
                }
                ReportingDomain::Shared => (
                    RecordKind::SharedFinding,
                    RecordKind::SharedEvaluationStatus,
                ),
            };
            for found in self.findings.get(unit).into_iter().flatten() {
                let view = self.views.finding(found)?;
                records.push(self.retained(unit, finding, Record::Finding(*found), view)?);
            }
            if let Some(state) = self.evaluation.status(unit) {
                let view = self.views.status(unit, state)?;
                records.push(self.retained(unit, status, Record::Status(state), view)?);
            }
        }
        match domain {
            ReportingDomain::Entry => self.entry(unit, indexed, &mut records)?,
            ReportingDomain::Shared => {
                if *unit == table() {
                    for (cell, basis) in self.evaluation.decision_basis() {
                        records.push(self.retained(
                            unit,
                            RecordKind::CoverageDecisionBasis,
                            Record::DecisionBasis(cell, basis),
                            basis_view(cell, basis),
                        )?);
                    }
                }
            }
        }
        Ok(records)
    }

    /// Every slice, unit by unit in canonical order and each unit's Entry
    /// slice first, as its stages build it when no bound stops them.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug that leaves a slice without its records.
    pub(crate) fn slices(&self) -> Result<Vec<Slice<'e>>, ReportingInvariant> {
        let mut indexed = BTreeSet::new();
        let mut slices = Vec::new();
        for unit in self.units() {
            for domain in [ReportingDomain::Entry, ReportingDomain::Shared] {
                let Some(candidates) = self.candidates(unit, domain)? else {
                    continue;
                };
                let records = self.records(unit, domain, &indexed)?;
                for retained in &records {
                    if let Record::Evidence(evidence) = &retained.record {
                        indexed.insert(evidence.clone());
                    }
                }
                slices.push(Slice {
                    unit,
                    domain,
                    candidates,
                    records,
                });
            }
        }
        Ok(slices)
    }

    /// The views of `domain`'s terminal reporting-limit Finding, which names
    /// `bound`, and of its terminal state, whose omitted slices start at
    /// `cursor`'s. Both occupy the domain's reserved slots, so they are not
    /// records of any slice (D15:1972, 2359, 2408, 2489).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug that leaves either without its view.
    pub(crate) fn terminal(
        &self,
        domain: ReportingDomain,
        cursor: &Unit,
        bound: ExhaustedBound,
    ) -> Result<(Value, Value), ReportingInvariant> {
        let key = IssueKey::terminal(domain.reason());
        let details = Details::Exhausted(bound);
        let finding = self.views.finding(&Finding {
            key: &key,
            details: Some(&details),
            suggestion: None,
        })?;
        let state = self.views.terminal_state(domain.id(), cursor, &key)?;
        Ok((finding, state))
    }

    /// The Entry records `unit` owns besides its own Findings and status:
    /// the occurrences of its Findings, the Coverage Decision Evidence of
    /// the decision table, an index record for each Evidence they refer to
    /// that `indexed` does not hold, and the Shared-unavailability causes
    /// that name its Findings (D15:2207).
    fn entry(
        &self,
        unit: &'e Unit,
        indexed: &BTreeSet<Evidence>,
        records: &mut Vec<Retained<'e>>,
    ) -> Result<(), ReportingInvariant> {
        let found = self.occurrences.get(unit).into_iter().flatten();
        let occurrences = occurrence::occurrences(found, self.specification, &self.tokens)?;
        let decisions = if *unit == table() {
            decision_evidence::records(&self.decisions, self.specification, &self.tokens)?
        } else {
            Vec::new()
        };
        let referred: BTreeSet<Evidence> = occurrences
            .iter()
            .flat_map(FindingOccurrence::references)
            .chain(decisions.iter().map(CoverageDecisionEvidence::reference))
            .map(|(_, evidence)| evidence)
            .filter(|evidence| !indexed.contains(*evidence))
            .cloned()
            .collect();
        for found in occurrences {
            let view = found.view(&self.views)?;
            records.push(self.retained(
                unit,
                RecordKind::FindingOccurrence,
                Record::Occurrence(found),
                view,
            )?);
        }
        for decision in decisions {
            let view = decision.view();
            records.push(self.retained(
                unit,
                RecordKind::CoverageDecisionEvidence,
                Record::DecisionEvidence(decision),
                view,
            )?);
        }
        for evidence in referred {
            let view = evidence.view();
            records.push(self.retained(
                unit,
                RecordKind::EvidenceIndex,
                Record::Evidence(evidence),
                view,
            )?);
        }
        for cause in self
            .unavailable
            .into_iter()
            .flat_map(Causes::keys)
            .filter(|cause| cause.unit() == unit)
        {
            let view = self.views.shared_unavailability_cause(cause)?;
            records.push(self.retained(
                unit,
                RecordKind::SharedUnavailabilityCause,
                Record::UnavailabilityCause(cause),
                view,
            )?);
        }
        Ok(())
    }

    /// `record`, of kind `kind` and view `view`, retained under `owner`.
    fn retained(
        &self,
        owner: &Unit,
        kind: RecordKind,
        record: Record<'e>,
        view: Value,
    ) -> Result<Retained<'e>, ReportingInvariant> {
        let accounting = self.views.accounting(owner, kind, view)?;
        Ok(Retained {
            kind,
            record,
            bytes: bytes(&accounting),
        })
    }
}

/// The first unit of `evaluation`, in canonical unit order, that reports
/// into `domain`, `found` counting the occurrence candidates of each unit
/// that found something.
fn first<'e>(
    evaluation: &'e Evaluation,
    found: &BTreeMap<&Unit, u64>,
    domain: ReportingDomain,
) -> Option<&'e Unit> {
    evaluation
        .statuses()
        .map(|(unit, _)| unit)
        .find(|unit| reports_into(evaluation, found, unit, domain))
}

/// Whether `unit` reports into `domain`: a unit reports into the domain of
/// its own Findings and status, and into the Entry domain whenever it found
/// something or owns Coverage Decision Evidence (D15:2207).
fn reports_into(
    evaluation: &Evaluation,
    found: &BTreeMap<&Unit, u64>,
    unit: &Unit,
    domain: ReportingDomain,
) -> bool {
    let decides = *unit == table() && !evaluation.decision_evidence().is_empty();
    ReportingDomain::of(unit.phase) == domain
        || (domain == ReportingDomain::Entry && (found.contains_key(unit) || decides))
}

/// The decision table's unit, which owns every Coverage Decision Basis and
/// Evidence record (D15:1609).
fn table() -> Unit {
    coverage::unit(CoverageSubject::Table, Check::CompleteTable)
}

/// `{ cell: { locale, surfaceClass }, mode, decision, domains }`: a cell's
/// Coverage Decision Basis, decided by the `default` with no domain or by the
/// `rule` of its matched domains in their order (D15:1607, 2483, 2487).
fn basis_view((locale, surface): &(CanonicalLocale, Token), basis: &Basis) -> Value {
    let (decision, domains) = match &basis.decision {
        Decision::Default => ("default", Vec::new()),
        Decision::Rules(domains) => ("rule", domains.iter().map(rule_domain).collect()),
    };
    json!({
        "cell": { "locale": locale.as_str(), "surfaceClass": surface.as_str() },
        "mode": basis.mode,
        "decision": decision,
        "domains": domains,
    })
}

/// The length of `C` of `view`, past the encoder's reader capacity
/// (D15:2518; plan 025 decision #224).
fn bytes(view: &Value) -> u64 {
    encoded_length(view).expect("a record view is a shallow number-free value")
}

/// An in-memory count.
fn count(length: usize) -> u64 {
    crate::artifacts::count(length)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use intlify_shared_json::encoding::encoded_length;
    use serde_json::{json, Value};

    use crate::construction::registry::check::RecordKind;
    use crate::materialize_tests::limits;
    use crate::resolver::entry::admit_entry;
    use crate::resolver::entry::fixtures::FileFixture;
    use crate::resolver::evaluation::{Blocking, Check, Code, Evaluation, Phase, Status, Unit};
    use crate::resolver::evidence::Evidence;
    use crate::resolver::evidence_metadata::fixtures::unbounded;
    use crate::resolver::limits::fixture_limits;
    use crate::resolver::profile_resolution::tests::Inputs;
    use crate::resolver::record_view::RecordViews;
    use crate::resolver::specification::Slot;
    use crate::resolver::token::{Sha256, Specification};

    use super::{basis_view, table, Candidates, Record, Reporting, ReportingDomain, Slice};

    const PROFILE: Specification<'static> = Specification {
        identity: "intlify-project-profile",
        revision: "0",
    };

    /// The record kinds of `slice`, in order.
    fn kinds(slice: &Slice<'_>) -> Vec<RecordKind> {
        slice.records.iter().map(|retained| retained.kind).collect()
    }

    /// The Evidence of `slice`'s index records, in order.
    fn indexed<'s>(slice: &'s Slice<'_>) -> Vec<&'s Evidence> {
        slice
            .records
            .iter()
            .filter_map(|retained| match &retained.record {
                Record::Evidence(evidence) => Some(evidence),
                _ => None,
            })
            .collect()
    }

    /// The length of `C` of `evidence`'s view.
    fn charged(evidence: &Evidence) -> u64 {
        encoded_length(&evidence.view()).unwrap()
    }

    #[test]
    fn phases_0_and_1_report_into_the_entry_domain_and_later_phases_into_the_shared_one() {
        for phase in Phase::ALL {
            let entry = matches!(
                phase,
                Phase::ResolverBootstrapAdmission | Phase::EntryAdmission
            );
            assert_eq!(
                ReportingDomain::of(phase) == ReportingDomain::Entry,
                entry,
                "{phase:?}"
            );
        }
    }

    /// A complete invocation finds nothing, so each unit reports only its
    /// status, into its own domain, charged `C` of its accounting view. The
    /// decision table also reports a Coverage Decision Basis for each cell
    /// into the Shared domain, and into the Entry domain the Coverage
    /// Decision Evidence with an index record for each Evidence it refers to
    /// (D15:1607, 1609, 2207, 2518).
    #[test]
    fn a_complete_invocation_reports_each_status_into_its_own_domain() {
        Inputs::complete().resolve(|evaluation, _| {
            assert!(evaluation.findings().is_empty());
            let reporting = Reporting::new(&evaluation, PROFILE, None, &unbounded(), &Sha256);
            let slices = reporting.slices().unwrap();
            let views = RecordViews::assign(PROFILE, &evaluation, &Sha256);
            let table = table();
            let mut expected = Vec::new();
            for (unit, _) in evaluation.statuses() {
                if *unit == table {
                    expected.push((unit, ReportingDomain::Entry));
                }
                expected.push((unit, ReportingDomain::of(unit.phase)));
            }
            let reported: Vec<(&Unit, ReportingDomain)> = slices
                .iter()
                .map(|slice| (slice.unit, slice.domain))
                .collect();
            assert_eq!(reported, expected);
            assert!(reported.iter().any(|(unit, domain)| unit.phase
                == Phase::ResolverBootstrapAdmission
                && *domain == ReportingDomain::Entry));
            let cells = evaluation.decision_basis().len();
            let decisions = evaluation.decision_evidence().len();
            assert_eq!((cells, decisions), (6, 6));
            for slice in &slices {
                assert_eq!(slice.candidates, Candidates::default(), "{:?}", slice.unit);
                if slice.domain == ReportingDomain::Entry && slice.unit == &table {
                    let sources: BTreeSet<&Evidence> = slice
                        .records
                        .iter()
                        .filter_map(|retained| match &retained.record {
                            Record::DecisionEvidence(decision) => Some(decision.evidence()),
                            _ => None,
                        })
                        .collect();
                    assert_eq!(
                        kinds(slice),
                        [
                            vec![RecordKind::CoverageDecisionEvidence; decisions],
                            vec![RecordKind::EvidenceIndex; sources.len()],
                        ]
                        .concat()
                    );
                    assert_eq!(indexed(slice), sources.into_iter().collect::<Vec<_>>());
                    continue;
                }
                let status = match slice.domain {
                    ReportingDomain::Entry => RecordKind::EntryEvaluationStatus,
                    ReportingDomain::Shared => RecordKind::SharedEvaluationStatus,
                };
                let basis = if slice.unit == &table { cells } else { 0 };
                assert_eq!(
                    kinds(slice),
                    [vec![status], vec![RecordKind::CoverageDecisionBasis; basis]].concat(),
                    "{:?}",
                    slice.unit
                );
                let state = evaluation.status(slice.unit).unwrap();
                let view = views.status(slice.unit, state).unwrap();
                let accounting = views.accounting(slice.unit, status, view).unwrap();
                assert_eq!(slice.records[0].bytes, encoded_length(&accounting).unwrap());
            }
            let basis: Vec<Value> = slices
                .iter()
                .flat_map(|slice| &slice.records)
                .filter_map(|retained| match retained.record {
                    Record::DecisionBasis(cell, basis) => Some(basis_view(cell, basis)),
                    _ => None,
                })
                .collect();
            assert_eq!(basis.len(), cells);
            assert!(basis.contains(&json!({
                "cell": { "locale": "en", "surfaceClass": "internal-tool" },
                "mode": "direct-required",
                "decision": "default",
                "domains": [],
            })));
            assert!(basis.contains(&json!({
                "cell": { "locale": "ja", "surfaceClass": "checkout" },
                "mode": "fallback-allowed",
                "decision": "rule",
                "domains": [{ "locales": ["ja"], "surfaceClasses": ["checkout"] }],
            })));
        });
    }

    /// A target whose locales are not a subset of the project's reports its
    /// Finding and status into the Shared domain and the Finding's
    /// occurrence into the Entry domain, which counts the occurrence, its
    /// related Evidence, and `C` of each Evidence it carries. Both targets
    /// relate the project's locales, and the first in canonical unit order
    /// holds that Evidence's index record (D15:1949-1951, 1972, 2207).
    #[test]
    fn the_first_unit_to_refer_to_an_evidence_holds_its_index_record() {
        let mut inputs = Inputs::complete();
        for target in ["browser", "server"] {
            inputs.declaration()["targetProfiles"][target]["requestedLocales"] =
                json!(["ja", "en", "fr"]);
        }
        inputs.resolve(|evaluation, _| {
            let reporting = Reporting::new(&evaluation, PROFILE, None, &unbounded(), &Sha256);
            let slices = reporting.slices().unwrap();
            let subsets: Vec<&Slice<'_>> = slices
                .iter()
                .filter(|slice| slice.unit.check == Check::ProjectSubset)
                .collect();
            let [entry, shared, later, later_shared] = subsets[..] else {
                panic!("each target reports one slice into each domain: {subsets:?}");
            };
            assert!(entry.unit < later.unit);
            for slice in [shared, later_shared] {
                assert_eq!(slice.domain, ReportingDomain::Shared);
                assert_eq!(
                    kinds(slice),
                    [
                        RecordKind::SharedFinding,
                        RecordKind::SharedEvaluationStatus
                    ]
                );
                let Record::Finding(finding) = slice.records[0].record else {
                    panic!("a Finding comes first");
                };
                assert_eq!(finding.key.code(), Code::TargetLocalesNotSubset);
                assert_eq!(
                    slice.candidates,
                    Candidates {
                        findings: 1,
                        ..Candidates::default()
                    }
                );
            }
            let occurrence = |slice: &Slice<'_>| match &slice.records[0].record {
                Record::Occurrence(occurrence) => occurrence.clone(),
                _ => panic!("an occurrence comes first"),
            };
            let (first, second) = (occurrence(entry), occurrence(later));
            assert_eq!(first.related(), second.related());
            let [project] = first.related() else {
                panic!("the occurrence relates the project's locales");
            };
            assert_eq!(
                indexed(entry),
                BTreeSet::from([first.primary(), project])
                    .into_iter()
                    .collect::<Vec<_>>()
            );
            assert_eq!(indexed(later), [second.primary()]);
            assert_eq!(
                kinds(entry),
                [
                    RecordKind::FindingOccurrence,
                    RecordKind::EvidenceIndex,
                    RecordKind::EvidenceIndex
                ]
            );
            for (slice, occurrence) in [(entry, &first), (later, &second)] {
                assert_eq!(slice.domain, ReportingDomain::Entry);
                assert_eq!(
                    slice.candidates,
                    Candidates {
                        findings: 1,
                        related: 1,
                        evidence: vec![charged(occurrence.primary()), charged(project)],
                        causes: 0,
                    }
                );
            }
        });
    }

    /// Without a materialized value there is no Shared projection, so every
    /// slice is an Entry slice, and the unit of each blocking Finding the
    /// projection is unavailable for holds the cause that names it, after
    /// its Finding, status, occurrence, and index record (D15:2207, 2369).
    #[test]
    fn without_a_value_the_entry_findings_hold_the_unavailability_causes() {
        let fixture = FileFixture::new(&fixture_limits(100));
        let mut evaluation = Evaluation::default();
        let file = fixture.file(Arc::from(&br#"{"a":"#[..]));
        let Ok(Err(causes)) = admit_entry(&mut evaluation, &file, limits()) else {
            panic!("the file is not strict JSON");
        };
        let reporting = Reporting::new(&evaluation, PROFILE, Some(&causes), &unbounded(), &Sha256);
        let slices = reporting.slices().unwrap();
        assert!(slices
            .iter()
            .all(|slice| slice.domain == ReportingDomain::Entry));
        let failed: Vec<&Slice<'_>> = slices
            .iter()
            .filter(|slice| {
                slice
                    .records
                    .iter()
                    .any(|retained| retained.kind == RecordKind::SharedUnavailabilityCause)
            })
            .collect();
        let [failed] = failed[..] else {
            panic!("one Finding names the unavailable projection: {failed:?}");
        };
        assert_eq!(failed.unit.check, Check::WellFormedJson);
        assert_eq!(
            kinds(failed),
            [
                RecordKind::EntryFinding,
                RecordKind::EntryEvaluationStatus,
                RecordKind::FindingOccurrence,
                RecordKind::EvidenceIndex,
                RecordKind::SharedUnavailabilityCause,
            ]
        );
        let Record::UnavailabilityCause(cause) = failed.records[4].record else {
            panic!("the cause comes last");
        };
        assert_eq!(cause.unit(), failed.unit);
        let waiting = slices
            .iter()
            .filter(|slice| matches!(evaluation.status(slice.unit), Some(Status::NotEvaluated(_))))
            .count();
        assert!(waiting > 0);
        for slice in &slices {
            let causes = match evaluation.status(slice.unit) {
                Some(Status::NotEvaluated(causes)) => u64::try_from(causes.keys().count()).unwrap(),
                _ => 0,
            };
            assert_eq!(slice.candidates.causes, causes, "{:?}", slice.unit);
        }
    }

    /// A unit over several values is not evaluated only when its
    /// prerequisites exclude every value, and then each value's causes count
    /// before they merge into its one duplicate-free set; a unit that
    /// evaluated a value counts none (D15:1966, 2184, 2369).
    #[test]
    fn a_unit_counts_each_values_causes_only_when_it_waits_on_every_value() {
        let rejected = |raw: &str| {
            crate::resolver::locales::unit(
                crate::resolver::locales::LocaleSubject::RequestedLocale(
                    crate::resolver::locales::fixtures::content(raw),
                ),
                Check::CanonicalLocale,
            )
        };
        let waits = || crate::resolver::locales::fixtures::stand_in(Slot::LocaleCanonicalization);
        let mut waiting = Evaluation::default();
        for raw in ["yy", "zz"] {
            waiting
                .check(rejected(raw), Err::<(), _>(waits()), Ok::<(), Blocking>)
                .unwrap()
                .unwrap_err();
        }
        let unit = rejected("zz").logical();
        let Some(Status::NotEvaluated(causes)) = waiting.status(&unit) else {
            panic!("every value waits");
        };
        assert_eq!(causes.keys().count(), 1);
        let reporting = Reporting::new(&waiting, PROFILE, None, &unbounded(), &Sha256);
        let counted = |reporting: &Reporting<'_, '_>| {
            reporting
                .candidates(&unit, ReportingDomain::Shared)
                .unwrap()
                .unwrap()
                .causes
        };
        assert_eq!(counted(&reporting), 2);
        let mut evaluated = Evaluation::default();
        evaluated
            .check(rejected("yy"), Ok(()), Ok::<(), Blocking>)
            .unwrap()
            .unwrap();
        evaluated
            .check(rejected("zz"), Err::<(), _>(waits()), Ok::<(), Blocking>)
            .unwrap()
            .unwrap_err();
        assert_eq!(evaluated.status(&unit), Some(&Status::Evaluated));
        assert_eq!(
            counted(&Reporting::new(
                &evaluated,
                PROFILE,
                None,
                &unbounded(),
                &Sha256
            )),
            0
        );
    }

    /// A decision table that waits, here on a rule naming a surface class
    /// outside the vocabulary, owns no Coverage Decision Basis or Evidence,
    /// so it reports only its status, into the Shared domain, with its causes
    /// counted (D15:1609).
    #[test]
    fn a_table_that_waits_reports_only_its_status() {
        let mut inputs = Inputs::complete();
        inputs.declaration()["coverage"]["rules"] =
            json!([{"intentSurfaceClasses": ["unknown"], "mode": "fallback-allowed"}]);
        inputs.resolve(|evaluation, _| {
            let reporting = Reporting::new(&evaluation, PROFILE, None, &unbounded(), &Sha256);
            let slices = reporting.slices().unwrap();
            let table = table();
            let owned: Vec<&Slice<'_>> =
                slices.iter().filter(|slice| slice.unit == &table).collect();
            let [owned] = owned[..] else {
                panic!("the table reports one slice: {owned:?}");
            };
            assert_eq!(owned.domain, ReportingDomain::Shared);
            assert_eq!(kinds(owned), [RecordKind::SharedEvaluationStatus]);
            let Some(Status::NotEvaluated(causes)) = evaluation.status(&table) else {
                panic!("the table waits");
            };
            assert_eq!(
                owned.candidates.causes,
                u64::try_from(causes.keys().count()).unwrap()
            );
            assert!(owned.candidates.causes > 0);
        });
    }
}
