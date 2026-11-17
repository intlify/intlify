// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's Reporting Slice Plan: each diagnostic domain's ordinary
//! bounds over the slices its units report, in canonical unit order
//! (D15:1940-1974, 2209-2211, 2391-2408).
//!
//! Each domain plans its slices on its own, unit by unit, and neither domain
//! consumes or reduces the other's capacity (D15:1940, 1970). A slice's
//! candidate stage is preflighted first, against the bounds that count each
//! candidate occurrence, each single item, and what comes before
//! deduplication; only then does the record stage build the slice's records
//! and preflight the bounds that count them after deduplication, with the
//! domain-wide `maxRecords` and `maxTotalCanonicalBytes` (D15:2209). A slice
//! that passes both commits whole. The first that does not fixes the domain's
//! terminal cursor at its unit, and no later slice of that domain is planned,
//! while the other domain goes on (D15:2211). `maxEvaluationStatusEntries`
//! counts retained statuses, none of the candidate stage's kinds, so the
//! record stage checks it; the bound the terminal Finding names is the first,
//! in the domain's declaration order, that the refused slice exceeds at the
//! stage that refused it (D15:2408; plan 025 interpretation #226).
//!
//! Counts are checked: a count past `u64` is past every bound. The terminal
//! Finding and state occupy the domain's reserved slots, not its ordinary
//! bounds (D15:1972). The bounds here are one authority's, the
//! capability's `policyComparable` values before a Resource Limit Policy is
//! admitted (D15:2391); the Policy's, with a recount of what was committed
//! before, come with plan 025 PR 5g-3.

use std::collections::BTreeSet;

use crate::artifacts::count;
use crate::input_limits::Bound;

use super::evaluation::{Status, Unit};
use super::evidence::Evidence;
use super::limits::{
    BoundId, DiagnosticsLimits, EntryDiagnosticsLimits, Group, SharedDiagnosticsLimits,
};
use super::reporting::{
    Candidates, Record, Reporting, ReportingDomain, ReportingInvariant, Retained, Slice,
};
use super::tally::{total, Tally};

/// What one domain reports: the slices it committed, in canonical unit
/// order, and how a slice it refused exhausted it, if one did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Committed<'e> {
    pub(crate) slices: Vec<Slice<'e>>,
    pub(crate) exhausted: Option<Exhausted<'e>>,
}

/// How a domain was exhausted: the unit whose slice it refused, which is the
/// domain's terminal cursor, and the bound its terminal Finding names
/// (D15:2408).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Exhausted<'e> {
    pub(crate) cursor: &'e Unit,
    pub(crate) bound: BoundId,
}

/// What both domains report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Reported<'e> {
    pub(crate) entry: Committed<'e>,
    pub(crate) shared: Committed<'e>,
}

/// Plans and commits the slices of `reporting` under `limits`, each domain
/// on its own in canonical unit order.
///
/// # Errors
///
/// Returns the resolver bug that leaves a slice without its records.
pub(crate) fn report<'e>(
    reporting: &Reporting<'e, '_>,
    limits: &DiagnosticsLimits,
) -> Result<Reported<'e>, ReportingInvariant> {
    Ok(Reported {
        entry: commit(
            reporting,
            ReportingDomain::Entry,
            Bounds::Entry(&limits.entry),
        )?,
        shared: commit(
            reporting,
            ReportingDomain::Shared,
            Bounds::Shared(&limits.shared),
        )?,
    })
}

/// The slices of `domain`, planned in canonical unit order until one exceeds
/// a bound of `bounds`.
fn commit<'e>(
    reporting: &Reporting<'e, '_>,
    domain: ReportingDomain,
    bounds: Bounds<'_>,
) -> Result<Committed<'e>, ReportingInvariant> {
    let mut counted = Counters::default();
    let mut indexed: BTreeSet<Evidence> = BTreeSet::new();
    let mut slices = Vec::new();
    for unit in reporting.units() {
        let Some(candidates) = reporting.candidates(unit, domain)? else {
            continue;
        };
        let staged = counted.with_candidates(&candidates);
        if let Some(bound) = bounds.candidate_stage(&staged, &candidates) {
            let exhausted = Some(Exhausted {
                cursor: unit,
                bound,
            });
            return Ok(Committed { slices, exhausted });
        }
        let records = reporting.records(unit, domain, &indexed)?;
        let staged = staged.with_records(&records);
        if let Some(bound) = bounds.record_stage(&staged, &records) {
            let exhausted = Some(Exhausted {
                cursor: unit,
                bound,
            });
            return Ok(Committed { slices, exhausted });
        }
        counted = staged;
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
    Ok(Committed {
        slices,
        exhausted: None,
    })
}

/// What a domain has counted so far for each bound that counts across its
/// slices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Counters {
    finding_occurrences: Tally,
    related_occurrences: Tally,
    evidence_bytes: Tally,
    cause_occurrences: Tally,
    findings: Tally,
    statuses: Tally,
    records: Tally,
    canonical_bytes: Tally,
}

impl Default for Counters {
    fn default() -> Self {
        let none = Tally::exactly(0);
        Self {
            finding_occurrences: none,
            related_occurrences: none,
            evidence_bytes: none,
            cause_occurrences: none,
            findings: none,
            statuses: none,
            records: none,
            canonical_bytes: none,
        }
    }
}

impl Counters {
    /// These counts once a slice's candidate stage counted `candidates`.
    fn with_candidates(self, candidates: &Candidates) -> Self {
        let evidence = total(candidates.evidence.iter().copied().map(Tally::exactly));
        Self {
            finding_occurrences: self
                .finding_occurrences
                .plus(Tally::exactly(candidates.findings)),
            related_occurrences: self
                .related_occurrences
                .plus(Tally::exactly(candidates.related)),
            evidence_bytes: self.evidence_bytes.plus(evidence),
            cause_occurrences: self
                .cause_occurrences
                .plus(Tally::exactly(candidates.causes)),
            ..self
        }
    }

    /// These counts once a slice's record stage built `records`.
    fn with_records(self, records: &[Retained<'_>]) -> Self {
        let of = |kind: fn(&Record<'_>) -> bool| {
            Tally::of(
                records
                    .iter()
                    .filter(|retained| kind(&retained.record))
                    .count(),
            )
        };
        let bytes = total(
            records
                .iter()
                .map(|retained| Tally::exactly(retained.bytes)),
        );
        Self {
            findings: self
                .findings
                .plus(of(|record| matches!(record, Record::Finding(_)))),
            statuses: self
                .statuses
                .plus(of(|record| matches!(record, Record::Status(_)))),
            records: self.records.plus(Tally::of(records.len())),
            canonical_bytes: self.canonical_bytes.plus(bytes),
            ..self
        }
    }
}

/// One domain's ordinary bounds.
#[derive(Debug, Clone, Copy)]
enum Bounds<'l> {
    Entry(&'l EntryDiagnosticsLimits),
    Shared(&'l SharedDiagnosticsLimits),
}

impl Bounds<'_> {
    /// The first bound, in declaration order, that the candidate stage of a
    /// slice whose candidates are `candidates` exceeds once `staged` counts
    /// them: each candidate occurrence, each single item, and what comes
    /// before deduplication (D15:1946, 1949-1951, 1954, 1962, 1966, 2209).
    fn candidate_stage(self, staged: &Counters, candidates: &Candidates) -> Option<BoundId> {
        match self {
            Self::Entry(limits) => first_over(
                Group::DiagnosticsEntry,
                [
                    (
                        "maxFindingOccurrences",
                        limits.max_finding_occurrences,
                        staged.finding_occurrences,
                    ),
                    (
                        "maxRelatedEvidenceOccurrences",
                        limits.max_related_evidence_occurrences,
                        staged.related_occurrences,
                    ),
                    (
                        "maxSingleEvidenceBytes",
                        limits.max_single_evidence_bytes,
                        largest(candidates.evidence.iter().copied()),
                    ),
                    (
                        "maxTotalEvidenceBytes",
                        limits.max_total_evidence_bytes,
                        staged.evidence_bytes,
                    ),
                    (
                        "maxSuppressionCauseOccurrences",
                        limits.max_suppression_cause_occurrences,
                        staged.cause_occurrences,
                    ),
                ],
            ),
            Self::Shared(limits) => first_over(
                Group::DiagnosticsShared,
                [
                    (
                        "maxFindingOccurrences",
                        limits.max_finding_occurrences,
                        staged.finding_occurrences,
                    ),
                    (
                        "maxSuppressionCauseOccurrences",
                        limits.max_suppression_cause_occurrences,
                        staged.cause_occurrences,
                    ),
                ],
            ),
        }
    }

    /// The first bound, in declaration order, that the record stage of a
    /// slice whose records are `records` exceeds once `staged` counts them:
    /// what comes after deduplication, and the domain-wide `maxRecords` and
    /// `maxTotalCanonicalBytes` (D15:1947, 1948, 1952, 1953, 1955, 1956,
    /// 1963-1965, 1967, 1968, 2209).
    fn record_stage(self, staged: &Counters, records: &[Retained<'_>]) -> Option<BoundId> {
        let related = largest(records.iter().filter_map(|retained| {
            if let Record::Occurrence(occurrence) = &retained.record {
                Some(count(occurrence.related().len()))
            } else {
                None
            }
        }));
        let causes = largest(records.iter().filter_map(|retained| {
            if let Record::Status(Status::NotEvaluated(causes)) = &retained.record {
                Some(count(causes.keys().count()))
            } else {
                None
            }
        }));
        match self {
            Self::Entry(limits) => first_over(
                Group::DiagnosticsEntry,
                [
                    ("maxFindings", limits.max_findings, staged.findings),
                    (
                        "maxRelatedEvidencePerOccurrenceGroup",
                        limits.max_related_evidence_per_occurrence_group,
                        related,
                    ),
                    (
                        "maxEvaluationStatusEntries",
                        limits.max_evaluation_status_entries,
                        staged.statuses,
                    ),
                    (
                        "maxSuppressionCausesPerStatus",
                        limits.max_suppression_causes_per_status,
                        causes,
                    ),
                    ("maxRecords", limits.max_records, staged.records),
                    (
                        "maxTotalCanonicalBytes",
                        limits.max_total_canonical_bytes,
                        staged.canonical_bytes,
                    ),
                ],
            ),
            Self::Shared(limits) => first_over(
                Group::DiagnosticsShared,
                [
                    ("maxFindings", limits.max_findings, staged.findings),
                    (
                        "maxEvaluationStatusEntries",
                        limits.max_evaluation_status_entries,
                        staged.statuses,
                    ),
                    (
                        "maxSuppressionCausesPerStatus",
                        limits.max_suppression_causes_per_status,
                        causes,
                    ),
                    ("maxRecords", limits.max_records, staged.records),
                    (
                        "maxTotalCanonicalBytes",
                        limits.max_total_canonical_bytes,
                        staged.canonical_bytes,
                    ),
                ],
            ),
        }
    }
}

/// The largest of `items`, which a bound of one item counts, or nothing.
fn largest(items: impl Iterator<Item = u64>) -> Tally {
    Tally::exactly(items.max().unwrap_or(0))
}

/// The first of `bounds`, each with its limit and its count, whose count
/// exceeds its limit.
fn first_over<const N: usize>(
    group: Group,
    bounds: [(&'static str, Bound, Tally); N],
) -> Option<BoundId> {
    bounds
        .into_iter()
        .find(|&(_, limit, counted)| counted.exceeds(limit))
        .map(|(name, ..)| BoundId { group, name })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use serde_json::json;

    use crate::input_limits::Bound;
    use crate::materialize_tests::limits;
    use crate::resolver::entry::admit_entry;
    use crate::resolver::entry::fixtures::FileFixture;
    use crate::resolver::evaluation::{Causes, Evaluation, IssueKey, Reason, Status};
    use crate::resolver::evidence::Evidence;
    use crate::resolver::limits::{
        fixture_limits, BoundId, DiagnosticsLimits, Group, ResolutionLimits,
    };
    use crate::resolver::profile_resolution::tests::Inputs;
    use crate::resolver::record_view::RecordViews;
    use crate::resolver::reporting::{Record, Reporting, ReportingDomain, Slice};
    use crate::resolver::token::{Sha256, Specification};

    use super::{report, Committed, Exhausted, Reported};

    const PROFILE: Specification<'static> = Specification {
        identity: "intlify-project-profile",
        revision: "0",
    };

    /// The bounds that count one item, not what accumulates across slices.
    const ITEMS: [&str; 3] = [
        "maxRelatedEvidencePerOccurrenceGroup",
        "maxSingleEvidenceBytes",
        "maxSuppressionCausesPerStatus",
    ];

    /// Every diagnostic bound at its largest value, but `bound` at `limit`.
    fn limits_with(bound: Option<(BoundId, u64)>) -> DiagnosticsLimits {
        let largest = Bound::new(u64::MAX - 1).unwrap();
        ResolutionLimits::from_bounds(|id| match bound {
            Some((tight, limit)) if tight == id => Bound::new(limit),
            _ => Some(largest),
        })
        .unwrap()
        .diagnostics
    }

    /// The bounds of `group`, in their declaration order.
    fn bounds_of(group: Group) -> Vec<BoundId> {
        ResolutionLimits::from_bounds(|_| Bound::new(1))
            .unwrap()
            .bounds()
            .into_iter()
            .map(|(id, _)| id)
            .filter(|id| id.group == group)
            .collect()
    }

    /// What `slice` adds to what `bound` counts across slices, or, for a
    /// bound of one item, the slice's largest item.
    fn measure(bound: &str, slice: &Slice<'_>) -> u64 {
        let size = |items: usize| u64::try_from(items).unwrap();
        let records = &slice.records;
        let candidates = &slice.candidates;
        match bound {
            "maxFindingOccurrences" => candidates.findings,
            "maxFindings" => size(
                records
                    .iter()
                    .filter(|retained| matches!(retained.record, Record::Finding(_)))
                    .count(),
            ),
            "maxRelatedEvidencePerOccurrenceGroup" => records
                .iter()
                .filter_map(|retained| match &retained.record {
                    Record::Occurrence(occurrence) => Some(size(occurrence.related().len())),
                    _ => None,
                })
                .max()
                .unwrap_or(0),
            "maxRelatedEvidenceOccurrences" => candidates.related,
            "maxSingleEvidenceBytes" => candidates.evidence.iter().copied().max().unwrap_or(0),
            "maxTotalEvidenceBytes" => candidates.evidence.iter().sum(),
            "maxEvaluationStatusEntries" => size(
                records
                    .iter()
                    .filter(|retained| matches!(retained.record, Record::Status(_)))
                    .count(),
            ),
            "maxSuppressionCausesPerStatus" => records
                .iter()
                .filter_map(|retained| match retained.record {
                    Record::Status(Status::NotEvaluated(causes)) => {
                        Some(size(causes.keys().count()))
                    }
                    _ => None,
                })
                .max()
                .unwrap_or(0),
            "maxSuppressionCauseOccurrences" => candidates.causes,
            "maxRecords" => size(records.len()),
            "maxTotalCanonicalBytes" => records.iter().map(|retained| retained.bytes).sum(),
            _ => unreachable!("{bound} is not a diagnostic bound"),
        }
    }

    fn of<'r, 'e>(reported: &'r Reported<'e>, domain: ReportingDomain) -> &'r Committed<'e> {
        match domain {
            ReportingDomain::Entry => &reported.entry,
            ReportingDomain::Shared => &reported.shared,
        }
    }

    /// For each bound of `domain`, exactly what the slices `reporting` plans
    /// count passes, and one less refuses the slice that reaches it, the one
    /// that adds last to a count across slices or the first holding the
    /// largest item: the domain keeps the slices before it and names its unit
    /// and the bound, while the other domain reports as it would without the
    /// bound (D15:2209, 2211, 2408).
    fn exactly_and_one_over(reporting: &Reporting<'_, '_>, domain: ReportingDomain) {
        let group = match domain {
            ReportingDomain::Entry => Group::DiagnosticsEntry,
            ReportingDomain::Shared => Group::DiagnosticsShared,
        };
        let unbounded = report(reporting, &limits_with(None)).unwrap();
        let other = match domain {
            ReportingDomain::Entry => ReportingDomain::Shared,
            ReportingDomain::Shared => ReportingDomain::Entry,
        };
        let all = &of(&unbounded, domain).slices;
        assert!(of(&unbounded, domain).exhausted.is_none());
        assert_eq!(
            all,
            &reporting
                .slices()
                .unwrap()
                .into_iter()
                .filter(|slice| slice.domain == domain)
                .collect::<Vec<_>>()
        );
        for bound in bounds_of(group) {
            let measures: Vec<u64> = all.iter().map(|slice| measure(bound.name, slice)).collect();
            let (exact, refused) = if ITEMS.contains(&bound.name) {
                let largest = measures.iter().copied().max().unwrap();
                (
                    largest,
                    measures.iter().position(|&m| m == largest).unwrap(),
                )
            } else {
                let total = measures.iter().sum();
                (total, measures.iter().rposition(|&m| m > 0).unwrap())
            };
            assert!(exact >= 2, "{} counts {exact}", bound.name);
            let at = report(reporting, &limits_with(Some((bound, exact)))).unwrap();
            assert_eq!(of(&at, domain), of(&unbounded, domain), "{}", bound.name);
            let over = report(reporting, &limits_with(Some((bound, exact - 1)))).unwrap();
            assert_eq!(
                of(&over, domain),
                &Committed {
                    slices: all[..refused].to_vec(),
                    exhausted: Some(Exhausted {
                        cursor: all[refused].unit,
                        bound,
                    }),
                },
                "{}",
                bound.name
            );
            assert_eq!(of(&over, other), of(&unbounded, other), "{}", bound.name);
        }
    }

    /// Phase 1 over a file that repeats a member and holds a number no
    /// portable double represents: two Entry Findings, the duplicate's
    /// occurrences each relating the other members of its name, and the
    /// root-value bounds waiting on both, with no Shared projection
    /// (D15:802, 2149, 2369).
    fn entered() -> (Evaluation, Causes) {
        let fixture = FileFixture::new(&fixture_limits(100));
        let mut evaluation = Evaluation::default();
        let file = fixture.file(Arc::from(&br#"{"a":1,"a":2,"a":3,"b":1e400}"#[..]));
        let Ok(Err(causes)) = admit_entry(&mut evaluation, &file, limits()) else {
            panic!("the file is not admitted");
        };
        (evaluation, causes)
    }

    /// `complete`'s profile, whose two targets each request a locale outside
    /// the project's that their Target Profiles have no direction for, and
    /// whose coverage rule names a surface class outside the vocabulary, so
    /// the decision table and the `targets` group's owner wait.
    fn failing() -> Inputs {
        let mut inputs = Inputs::complete();
        for target in ["browser", "server"] {
            inputs.declaration()["targetProfiles"][target]["requestedLocales"] =
                json!(["ja", "en", "fr"]);
        }
        inputs.declaration()["coverage"]["rules"] =
            json!([{"intentSurfaceClasses": ["unknown"], "mode": "fallback-allowed"}]);
        inputs
    }

    #[test]
    fn each_entry_bound_passes_exactly_and_refuses_one_over() {
        let (evaluation, causes) = entered();
        let reporting = Reporting::new(&evaluation, PROFILE, Some(&causes), &Sha256);
        exactly_and_one_over(&reporting, ReportingDomain::Entry);
    }

    #[test]
    fn each_shared_bound_passes_exactly_and_refuses_one_over() {
        failing().resolve(|evaluation, _| {
            let reporting = Reporting::new(&evaluation, PROFILE, None, &Sha256);
            exactly_and_one_over(&reporting, ReportingDomain::Shared);
        });
    }

    /// Unbounded, each domain commits every slice as 5g-1's `slices` builds
    /// it, and the Entry domain indexes each Evidence once, in the slice of
    /// the first unit to refer to it, though both targets relate the
    /// project's locales (D15:1972, 2205).
    #[test]
    fn unbounded_domains_commit_every_slice_and_index_each_evidence_once() {
        failing().resolve(|evaluation, _| {
            let reporting = Reporting::new(&evaluation, PROFILE, None, &Sha256);
            let reported = report(&reporting, &limits_with(None)).unwrap();
            let slices = reporting.slices().unwrap();
            for domain in [ReportingDomain::Entry, ReportingDomain::Shared] {
                let committed = of(&reported, domain);
                assert!(committed.exhausted.is_none());
                assert_eq!(
                    committed.slices,
                    slices
                        .iter()
                        .filter(|slice| slice.domain == domain)
                        .cloned()
                        .collect::<Vec<_>>()
                );
            }
            let indexed: Vec<&Evidence> = reported
                .entry
                .slices
                .iter()
                .flat_map(|slice| &slice.records)
                .filter_map(|retained| match &retained.record {
                    Record::Evidence(evidence) => Some(evidence),
                    _ => None,
                })
                .collect();
            let referred: BTreeSet<&Evidence> = reported
                .entry
                .slices
                .iter()
                .flat_map(|slice| &slice.records)
                .filter_map(|retained| match &retained.record {
                    Record::Occurrence(occurrence) => Some(occurrence),
                    _ => None,
                })
                .flat_map(|occurrence| {
                    std::iter::once(occurrence.primary()).chain(occurrence.related())
                })
                .collect();
            assert_eq!(indexed.len(), referred.len());
            assert_eq!(indexed.into_iter().collect::<BTreeSet<_>>(), referred);
        });
    }

    /// A slice that exceeds bounds of both stages is refused at its
    /// candidate stage, whose bound the Finding names even when a record-stage
    /// bound comes first in declaration order; of two bounds of one stage, the
    /// first in declaration order (D15:2209, 2408; plan 025 interpretation
    /// #226).
    #[test]
    fn a_refused_slice_names_the_first_bound_of_the_stage_that_refused_it() {
        let (evaluation, causes) = entered();
        let reporting = Reporting::new(&evaluation, PROFILE, Some(&causes), &Sha256);
        let all = report(&reporting, &limits_with(None)).unwrap().entry.slices;
        let found: Vec<usize> = all
            .iter()
            .enumerate()
            .filter(|(_, slice)| measure("maxFindings", slice) > 0)
            .map(|(index, _)| index)
            .collect();
        let [_, second] = found[..] else {
            panic!("two units find an Entry Finding: {found:?}");
        };
        let before = |bound: &str| {
            all[..second]
                .iter()
                .map(|slice| measure(bound, slice))
                .sum::<u64>()
        };
        let bound = |name| BoundId {
            group: Group::DiagnosticsEntry,
            name,
        };
        let refused =
            |limits: &DiagnosticsLimits| report(&reporting, limits).unwrap().entry.exhausted;
        let mut limits = limits_with(Some((bound("maxFindings"), 1)));
        assert_eq!(
            refused(&limits),
            Some(Exhausted {
                cursor: all[second].unit,
                bound: bound("maxFindings")
            })
        );
        limits.entry.max_total_evidence_bytes =
            Bound::new(before("maxTotalEvidenceBytes")).unwrap();
        assert_eq!(
            refused(&limits),
            Some(Exhausted {
                cursor: all[second].unit,
                bound: bound("maxTotalEvidenceBytes")
            })
        );
        limits.entry.max_finding_occurrences = Bound::new(before("maxFindingOccurrences")).unwrap();
        assert_eq!(
            refused(&limits),
            Some(Exhausted {
                cursor: all[second].unit,
                bound: bound("maxFindingOccurrences")
            })
        );
    }

    /// An exhausted domain's terminal Finding is the one check over the
    /// diagnostic domain, its reason the domain and its details the bound,
    /// and its terminal state names the domain, the refused slice's unit,
    /// and that Finding; the two domains' Findings have two keys, the Entry
    /// domain's first (D15:2359, 2408, 2489).
    #[test]
    fn an_exhausted_domain_reports_its_terminal_finding_and_state() {
        failing().resolve(|evaluation, _| {
            let reporting = Reporting::new(&evaluation, PROFILE, None, &Sha256);
            let views = RecordViews::assign(PROFILE, &evaluation, &Sha256);
            let records = |domain| BoundId {
                group: match domain {
                    ReportingDomain::Entry => Group::DiagnosticsEntry,
                    ReportingDomain::Shared => Group::DiagnosticsShared,
                },
                name: "maxRecords",
            };
            let mut keys = Vec::new();
            for domain in [ReportingDomain::Entry, ReportingDomain::Shared] {
                let reported =
                    report(&reporting, &limits_with(Some((records(domain), 1)))).unwrap();
                let exhausted = of(&reported, domain).exhausted.unwrap();
                let (finding, state) = reporting
                    .terminal(domain, exhausted.cursor, exhausted.bound)
                    .unwrap();
                let reason = match domain {
                    ReportingDomain::Entry => Reason::Entry,
                    ReportingDomain::Shared => Reason::Shared,
                };
                let key = IssueKey::terminal(reason);
                assert_eq!(
                    finding,
                    json!({
                        "key": views.finding_key(&key).unwrap(),
                        "severity": "error",
                        "blocking": true,
                        "details": { "reason": domain.id(), "bound": "maxRecords" },
                    })
                );
                assert_eq!(
                    finding["key"]["check"],
                    json!("reporting.diagnostic-domain.complete")
                );
                assert_eq!(finding["key"]["phase"], json!("reporting-finalization"));
                assert_eq!(finding["key"]["reason"], json!(domain.id()));
                assert_eq!(
                    state,
                    json!({
                        "domain": domain.id(),
                        "nextUnitCursor": views.unit(exhausted.cursor).unwrap(),
                        "cause": finding["key"],
                    })
                );
                keys.push(key);
            }
            assert_ne!(keys[0], keys[1]);
            assert!(keys[0] < keys[1], "the Entry Finding comes first");
        });
    }
}
