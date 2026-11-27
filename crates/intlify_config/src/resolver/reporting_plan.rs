// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's Reporting Slice Plan: each diagnostic domain's ordinary
//! bounds over the slices its units report, in canonical unit order
//! (D15:1941-1975, 2210-2212, 2392-2409).
//!
//! Each domain plans its slices on its own, unit by unit, and neither domain
//! consumes or reduces the other's capacity (D15:1941, 1971). A slice's
//! candidate stage is preflighted first, against the bounds that count each
//! candidate occurrence, each single item, and what comes before
//! deduplication; only then does the record stage build the slice's records
//! and preflight the bounds that count them after deduplication, with the
//! domain-wide `maxRecords` and `maxTotalCanonicalBytes` (D15:2210). A slice
//! that passes both commits whole. The first that does not fixes the domain's
//! terminal cursor at its unit, and no later slice of that domain is planned,
//! while the other domain goes on (D15:2212). `maxEvaluationStatusEntries`
//! counts retained statuses, none of the candidate stage's kinds, so the
//! record stage checks it; the bound the terminal Finding names is the first,
//! in the domain's declaration order, that the refused slice exceeds at the
//! stage that refused it (D15:2409; plan 025 interpretation #226).
//!
//! Until a Resource Limit Policy is admitted, the capability's
//! `policyComparable` values are the sole authority. Phase 4 admits the Policy
//! once it is compatible with the capability, whatever its rechecks find, and
//! each domain then recounts the slices it committed before, from zero and in
//! canonical unit order, under the Policy's values, and goes on from the
//! recounted counts under them (D15:2133, 2392). The admitting unit's own
//! slice is planned after its check admitted the Policy, so under it. A domain
//! the capability exhausted before the admission keeps its terminal cursor and
//! bound unless the recount of what it committed refuses an earlier slice, and
//! a domain with no slice at or after the admitting unit recounts once it has
//! planned them all (plan 025 interpretation #228).
//!
//! When the resolver's own Evidence metadata work exceeded an
//! `evidenceMetadata` bound of a domain's scope, that domain plans no slice:
//! it is exhausted at the first unit that reports into it, and its terminal
//! Finding names that bound by its capability path. A bound of the Shared
//! scope exhausts both domains; one that only the complete invocation exceeds
//! exhausts only the Entry domain, and the Shared domain plans its slices as
//! it would within every bound (D15:1852; plan 025 decisions #229, #231, and
//! #247).
//!
//! Counts are checked: a count past `u64` is past every bound. The terminal
//! Finding and state occupy the domain's reserved slots, not its ordinary
//! bounds (D15:1973).

use std::collections::BTreeSet;

use crate::artifacts::count;
use crate::input_limits::Bound;

use super::evaluation::{Check, ExhaustedBound, Status, Unit};
use super::evidence::Evidence;
use super::limits::{
    BoundId, DiagnosticsLimits, EntryDiagnosticsLimits, Group, SharedDiagnosticsLimits,
};
use super::reporting::{
    Candidates, Record, Reporting, ReportingDomain, ReportingInvariant, Retained, Slice,
};
use super::resource_policy::{self, PolicySubject};
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
/// (D15:2409).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Exhausted<'e> {
    pub(crate) cursor: &'e Unit,
    pub(crate) bound: ExhaustedBound,
}

/// What both domains report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Reported<'e> {
    pub(crate) entry: Committed<'e>,
    pub(crate) shared: Committed<'e>,
}

/// Plans and commits the slices of `reporting`'s `domain` in canonical unit
/// order, under `capability`'s limits until Phase 4 admits a Resource Limit
/// Policy, whose limits `policy` gives, and under the Policy's from then on.
///
/// # Errors
///
/// Returns the resolver bug that leaves a slice without its records.
pub(crate) fn report_domain<'e>(
    reporting: &Reporting<'e, '_>,
    domain: ReportingDomain,
    capability: &DiagnosticsLimits,
    policy: Option<&DiagnosticsLimits>,
) -> Result<Committed<'e>, ReportingInvariant> {
    plan(
        reporting,
        domain,
        Bounds::of(domain, capability),
        policy.map(|policy| Bounds::of(domain, policy)),
    )
}

/// The slices of `domain`: none when the resolver's own Evidence metadata
/// work exceeded a bound of the domain's scope, the domain then exhausted at
/// the first unit that reports into it, its terminal Finding naming that
/// bound, if any unit does (D15:1852, 2409; plan 025 decisions #229 and
/// #247); otherwise those `commit` plans.
fn plan<'e>(
    reporting: &Reporting<'e, '_>,
    domain: ReportingDomain,
    capability: Bounds<'_>,
    policy: Option<Bounds<'_>>,
) -> Result<Committed<'e>, ReportingInvariant> {
    match reporting.exceeded(domain) {
        Some(bound) => Ok(Committed {
            slices: Vec::new(),
            exhausted: reporting.first(domain).map(|cursor| Exhausted {
                cursor,
                bound: ExhaustedBound::Capability(bound.id()),
            }),
        }),
        None => commit(reporting, domain, capability, policy),
    }
}

/// The slices of `domain`, planned in canonical unit order under `capability`
/// and, from the unit that admits it, under `policy`, until one exceeds a
/// bound.
fn commit<'e>(
    reporting: &Reporting<'e, '_>,
    domain: ReportingDomain,
    capability: Bounds<'_>,
    mut policy: Option<Bounds<'_>>,
) -> Result<Committed<'e>, ReportingInvariant> {
    let admission = resource_policy::unit(PolicySubject::ResourceLimits, Check::CompatiblePolicy);
    let mut bounds = capability;
    let mut counted = Counters::default();
    let mut indexed: BTreeSet<Evidence> = BTreeSet::new();
    let mut slices = Vec::new();
    for unit in reporting.units() {
        let Some(candidates) = reporting.candidates(unit, domain)? else {
            continue;
        };
        if *unit >= admission {
            if let Some(admitted) = policy.take() {
                if let Some((index, bound)) = recount(&slices, admitted) {
                    return Ok(cut(slices, index, bound));
                }
                bounds = admitted;
            }
        }
        let staged = match bounds.candidates(counted, &candidates) {
            Ok(staged) => staged,
            Err(bound) => return Ok(settle(slices, Some((unit, bound)), policy)),
        };
        let records = reporting.records(unit, domain, &indexed)?;
        counted = match bounds.records(staged, &records) {
            Ok(staged) => staged,
            Err(bound) => return Ok(settle(slices, Some((unit, bound)), policy)),
        };
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
    Ok(settle(slices, None, policy))
}

/// What a domain committed under the capability alone, `slices`, and the slice
/// it refused, if any: once the Policy the domain never reached the admitting
/// unit of, `pending`, recounts them, the first slice it refuses ends the
/// domain instead (D15:2392).
fn settle<'e>(
    slices: Vec<Slice<'e>>,
    refused: Option<(&'e Unit, BoundId)>,
    pending: Option<Bounds<'_>>,
) -> Committed<'e> {
    if let Some((index, bound)) = pending.and_then(|policy| recount(&slices, policy)) {
        return cut(slices, index, bound);
    }
    Committed {
        slices,
        exhausted: refused.map(|(cursor, bound)| Exhausted {
            cursor,
            bound: ExhaustedBound::Domain(bound),
        }),
    }
}

/// The slices before `index` of `slices`, the domain ending at the unit of the
/// slice at `index`, which exceeds `bound`.
fn cut(mut slices: Vec<Slice<'_>>, index: usize, bound: BoundId) -> Committed<'_> {
    let cursor = slices[index].unit;
    slices.truncate(index);
    Committed {
        slices,
        exhausted: Some(Exhausted {
            cursor,
            bound: ExhaustedBound::Domain(bound),
        }),
    }
}

/// The first of `slices` that `bounds` refuse, by its index and the bound it
/// exceeds, when the slices are counted again from zero in their order. What
/// a slice counts does not depend on the bounds, so a domain whose slices all
/// pass goes on from the counts it already has.
fn recount(slices: &[Slice<'_>], bounds: Bounds<'_>) -> Option<(usize, BoundId)> {
    slices
        .iter()
        .enumerate()
        .try_fold(Counters::default(), |counted, (index, slice)| {
            bounds
                .candidates(counted, &slice.candidates)
                .and_then(|staged| bounds.records(staged, &slice.records))
                .map_err(|bound| (index, bound))
        })
        .err()
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

impl<'l> Bounds<'l> {
    /// `domain`'s bounds in `limits`.
    const fn of(domain: ReportingDomain, limits: &'l DiagnosticsLimits) -> Self {
        match domain {
            ReportingDomain::Entry => Self::Entry(&limits.entry),
            ReportingDomain::Shared => Self::Shared(&limits.shared),
        }
    }
}

impl Bounds<'_> {
    /// `counted` once a slice's candidate stage counted `candidates`, or the
    /// first bound it exceeds.
    fn candidates(self, counted: Counters, candidates: &Candidates) -> Result<Counters, BoundId> {
        let staged = counted.with_candidates(candidates);
        self.candidate_stage(&staged, candidates)
            .map_or(Ok(staged), Err)
    }

    /// `staged` once a slice's record stage counted `records`, or the first
    /// bound they exceed.
    fn records(self, staged: Counters, records: &[Retained<'_>]) -> Result<Counters, BoundId> {
        let staged = staged.with_records(records);
        self.record_stage(&staged, records).map_or(Ok(staged), Err)
    }

    /// The first bound, in declaration order, that the candidate stage of a
    /// slice whose candidates are `candidates` exceeds once `staged` counts
    /// them: each candidate occurrence, each single item, and what comes
    /// before deduplication (D15:1947, 1950-1952, 1955, 1963, 1967, 2210).
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
    /// `maxTotalCanonicalBytes` (D15:1948, 1949, 1953, 1954, 1956, 1957,
    /// 1964-1966, 1968, 1969, 2210).
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

/// Plans and commits the slices of `reporting`, each domain on its own.
///
/// # Errors
///
/// Returns the resolver bug that leaves a slice without its records.
#[cfg(test)]
pub(crate) fn report<'e>(
    reporting: &Reporting<'e, '_>,
    capability: &DiagnosticsLimits,
    policy: Option<&DiagnosticsLimits>,
) -> Result<Reported<'e>, ReportingInvariant> {
    Ok(Reported {
        entry: report_domain(reporting, ReportingDomain::Entry, capability, policy)?,
        shared: report_domain(reporting, ReportingDomain::Shared, capability, policy)?,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use serde_json::json;

    use crate::artifacts::count;
    use crate::construction::registry::check::RecordKind;
    use crate::input_limits::Bound;
    use crate::materialize_tests::limits;
    use crate::resolver::dependency::fixtures::file_entry;
    use crate::resolver::entry::admit_entry;
    use crate::resolver::entry::fixtures::FileFixture;
    use crate::resolver::evaluation::{
        Causes, Check, Evaluation, ExhaustedBound, IssueKey, Phase, Reason, Status, Unit,
    };
    use intlify_shared_json::encoding::encoded_length;

    use crate::resolver::capability::EvidenceMetadataLimits;
    use crate::resolver::evidence::Evidence;
    use crate::resolver::evidence_metadata::fixtures::{unbounded, with};
    use crate::resolver::evidence_metadata::MetadataBound;
    use crate::resolver::limits::{
        fixture_limits, BoundId, DiagnosticsLimits, Group, ResolutionLimits,
    };
    use crate::resolver::profile_resolution::tests::Inputs;
    use crate::resolver::record_view::RecordViews;
    use crate::resolver::reporting::{Record, Reporting, ReportingDomain, Retained, Slice};
    use crate::resolver::resolution_evidence::Projection;
    use crate::resolver::resource_policy::{self, PolicySubject};
    use crate::resolver::token::{Sha256, Specification, Tokens};

    use super::{report, report_domain, Committed, Exhausted, Reported};

    pub(crate) const PROFILE: Specification<'static> = Specification {
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
    pub(crate) fn limits_with(bound: Option<(BoundId, u64)>) -> DiagnosticsLimits {
        limits_of(bound.as_slice())
    }

    /// Every diagnostic bound at its largest value, but each of `tight` at
    /// its limit.
    fn limits_of(tight: &[(BoundId, u64)]) -> DiagnosticsLimits {
        let largest = Bound::new(u64::MAX - 1).unwrap();
        ResolutionLimits::from_bounds(|id| {
            tight
                .iter()
                .find(|(bound, _)| *bound == id)
                .map_or(Some(largest), |&(_, limit)| Bound::new(limit))
        })
        .unwrap()
        .diagnostics
    }

    fn entry(name: &'static str) -> BoundId {
        BoundId {
            group: Group::DiagnosticsEntry,
            name,
        }
    }

    fn shared(name: &'static str) -> BoundId {
        BoundId {
            group: Group::DiagnosticsShared,
            name,
        }
    }

    /// The unit whose check admits the Resource Limit Policy.
    fn admission() -> Unit {
        resource_policy::unit(PolicySubject::ResourceLimits, Check::CompatiblePolicy)
    }

    /// What `slices` count together for `bound`.
    fn summed(bound: &str, slices: &[Slice<'_>]) -> u64 {
        slices.iter().map(|slice| measure(bound, slice)).sum()
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
    /// bound (D15:2210, 2212, 2409).
    fn exactly_and_one_over(reporting: &Reporting<'_, '_>, domain: ReportingDomain) {
        let group = match domain {
            ReportingDomain::Entry => Group::DiagnosticsEntry,
            ReportingDomain::Shared => Group::DiagnosticsShared,
        };
        let unbounded = report(reporting, &limits_with(None), None).unwrap();
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
            let at = report(reporting, &limits_with(Some((bound, exact))), None).unwrap();
            assert_eq!(of(&at, domain), of(&unbounded, domain), "{}", bound.name);
            let over = report(reporting, &limits_with(Some((bound, exact - 1))), None).unwrap();
            assert_eq!(
                of(&over, domain),
                &Committed {
                    slices: all[..refused].to_vec(),
                    exhausted: Some(Exhausted {
                        cursor: all[refused].unit,
                        bound: ExhaustedBound::Domain(bound),
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
    /// (D15:803, 2150, 2370).
    pub(crate) fn entered() -> (Evaluation, Causes) {
        let fixture = FileFixture::new(&fixture_limits(100));
        let mut evaluation = Evaluation::default();
        let file = fixture.file(Arc::from(&br#"{"a":1,"a":2,"a":3,"b":1e400}"#[..]));
        let Ok(Err(causes)) = admit_entry(&mut evaluation, &file, limits(), &unbounded()) else {
            panic!("the file is not admitted");
        };
        (evaluation, causes)
    }

    /// `complete`'s profile, whose two targets each request a locale outside
    /// the project's that their Target Profiles have no direction for, and
    /// whose coverage rule names a surface class outside the vocabulary, so
    /// the decision table and the `targets` group's owner wait.
    pub(crate) fn failing() -> Inputs {
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
        let reporting = Reporting::new(
            &evaluation,
            PROFILE,
            (Some(&causes), file_entry()),
            &unbounded(),
            &Sha256,
        );
        exactly_and_one_over(&reporting, ReportingDomain::Entry);
    }

    #[test]
    fn each_shared_bound_passes_exactly_and_refuses_one_over() {
        failing().resolve(|evaluation, _| {
            let reporting = Reporting::new(
                &evaluation,
                PROFILE,
                (None, file_entry()),
                &unbounded(),
                &Sha256,
            );
            exactly_and_one_over(&reporting, ReportingDomain::Shared);
        });
    }

    /// Unbounded, each domain commits every slice as 5g-1's `slices` builds
    /// it, and the Entry domain indexes each Evidence once, in the slice of
    /// the first unit to refer to it, though both targets relate the
    /// project's locales (D15:1973, 2206).
    #[test]
    fn unbounded_domains_commit_every_slice_and_index_each_evidence_once() {
        failing().resolve(|evaluation, _| {
            let reporting = Reporting::new(
                &evaluation,
                PROFILE,
                (None, file_entry()),
                &unbounded(),
                &Sha256,
            );
            let reported = report(&reporting, &limits_with(None), None).unwrap();
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
    /// first in declaration order (D15:2210, 2409; plan 025 interpretation
    /// #226).
    #[test]
    fn a_refused_slice_names_the_first_bound_of_the_stage_that_refused_it() {
        let (evaluation, causes) = entered();
        let reporting = Reporting::new(
            &evaluation,
            PROFILE,
            (Some(&causes), file_entry()),
            &unbounded(),
            &Sha256,
        );
        let all = report(&reporting, &limits_with(None), None)
            .unwrap()
            .entry
            .slices;
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
            |limits: &DiagnosticsLimits| report(&reporting, limits, None).unwrap().entry.exhausted;
        let mut limits = limits_with(Some((bound("maxFindings"), 1)));
        assert_eq!(
            refused(&limits),
            Some(Exhausted {
                cursor: all[second].unit,
                bound: ExhaustedBound::Domain(bound("maxFindings"))
            })
        );
        limits.entry.max_total_evidence_bytes =
            Bound::new(before("maxTotalEvidenceBytes")).unwrap();
        assert_eq!(
            refused(&limits),
            Some(Exhausted {
                cursor: all[second].unit,
                bound: ExhaustedBound::Domain(bound("maxTotalEvidenceBytes"))
            })
        );
        limits.entry.max_finding_occurrences = Bound::new(before("maxFindingOccurrences")).unwrap();
        assert_eq!(
            refused(&limits),
            Some(Exhausted {
                cursor: all[second].unit,
                bound: ExhaustedBound::Domain(bound("maxFindingOccurrences"))
            })
        );
    }

    /// An exhausted domain's terminal Finding is the one check over the
    /// diagnostic domain, its reason the domain and its details the bound,
    /// and its terminal state names the domain, the refused slice's unit,
    /// and that Finding; the two domains' Findings have two keys, the Entry
    /// domain's first (D15:2360, 2409, 2490).
    #[test]
    fn an_exhausted_domain_reports_its_terminal_finding_and_state() {
        failing().resolve(|evaluation, _| {
            let reporting = Reporting::new(
                &evaluation,
                PROFILE,
                (None, file_entry()),
                &unbounded(),
                &Sha256,
            );
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
                    report(&reporting, &limits_with(Some((records(domain), 1))), None).unwrap();
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

    /// Every record kind but the two Findings (D15:2519).
    const NOT_FINDINGS: [RecordKind; 10] = [
        RecordKind::EntryEvaluationStatus,
        RecordKind::FindingOccurrence,
        RecordKind::CoverageDecisionEvidence,
        RecordKind::EvidenceIndex,
        RecordKind::EntryDependency,
        RecordKind::SharedUnavailabilityCause,
        RecordKind::SharedEvaluationStatus,
        RecordKind::CoverageDecisionBasis,
        RecordKind::SemanticDependency,
        RecordKind::MaterializedDependency,
    ];

    /// For each record kind of `reporting`'s slices but a Finding, a
    /// domain-wide bound that its first record after the domain's first is
    /// the first to exceed, `maxRecords` at the records before it or
    /// `maxTotalCanonicalBytes` one byte under the bytes through it, refuses
    /// its owner's slice whole and ends the domain at that owner, whose
    /// projection names it as the terminal cursor. Returns the kinds it
    /// found.
    fn first_over_by_kind(
        evaluation: &Evaluation,
        reporting: &Reporting<'_, '_>,
    ) -> BTreeSet<RecordKind> {
        let views = RecordViews::assign(PROFILE, evaluation, &Sha256);
        let within = report(reporting, &limits_with(None), None).unwrap();
        let mut found = BTreeSet::new();
        for (domain, group) in [
            (ReportingDomain::Entry, Group::DiagnosticsEntry),
            (ReportingDomain::Shared, Group::DiagnosticsShared),
        ] {
            let slices = &of(&within, domain).slices;
            let records: Vec<(usize, &Retained<'_>)> = slices
                .iter()
                .enumerate()
                .flat_map(|(at, slice)| slice.records.iter().map(move |record| (at, record)))
                .collect();
            for kind in NOT_FINDINGS {
                let Some(at) = (1..records.len()).find(|&at| records[at].1.kind == kind) else {
                    continue;
                };
                let (slice, _) = records[at];
                let owner = slices[slice].unit;
                // The unit that produced the record, never a synthetic one.
                assert_ne!(owner.phase, Phase::ReportingFinalization);
                let bytes: u64 = records[..=at].iter().map(|(_, record)| record.bytes).sum();
                for (name, limit) in [
                    ("maxRecords", count(at)),
                    ("maxTotalCanonicalBytes", bytes - 1),
                ] {
                    let bound = BoundId { group, name };
                    let over = report(reporting, &limits_with(Some((bound, limit))), None).unwrap();
                    let committed = of(&over, domain).clone();
                    assert_eq!(
                        committed,
                        Committed {
                            slices: slices[..slice].to_vec(),
                            exhausted: Some(Exhausted {
                                cursor: owner,
                                bound: ExhaustedBound::Domain(bound),
                            }),
                        },
                        "{kind:?} {name}"
                    );
                    let projection = Projection::finalized(reporting, domain, committed).unwrap();
                    let terminal = projection.terminal.unwrap();
                    assert_eq!(terminal.exhausted.cursor, owner, "{kind:?} {name}");
                    assert_eq!(terminal.state["nextUnitCursor"], views.unit(owner).unwrap());
                }
                found.insert(kind);
            }
        }
        found
    }

    /// A record of every kind but a Finding can be the first past a
    /// domain-wide bound, and the domain then ends at that record's owner,
    /// never at reporting finalization: statuses, occurrences, the evidence
    /// index, entry dependencies, and the cause references of an entry that
    /// materialized no value, and the complete invocation's decision records
    /// and semantic and materialized dependencies (D15:2206, 2210, 2212,
    /// 2409, 3590).
    #[test]
    fn a_record_of_each_kind_past_a_domain_wide_bound_ends_its_domain_at_its_owner() {
        let (evaluation, causes) = entered();
        let reporting = Reporting::new(
            &evaluation,
            PROFILE,
            (Some(&causes), file_entry()),
            &unbounded(),
            &Sha256,
        );
        let mut found = first_over_by_kind(&evaluation, &reporting);
        Inputs::complete().resolve(|evaluation, _| {
            let reporting = Reporting::new(
                &evaluation,
                PROFILE,
                (None, file_entry()),
                &unbounded(),
                &Sha256,
            );
            found.extend(first_over_by_kind(&evaluation, &reporting));
        });
        assert_eq!(found, BTreeSet::from(NOT_FINDINGS));
    }

    /// Past a bound of the Shared scope, no slice is planned: each domain is
    /// exhausted at the first unit that reports into it, its terminal Finding
    /// naming the bound by its capability path and its terminal state the
    /// cursor that a complete assignment of the subject tokens gives (D15:1852,
    /// 2360, 2409; plan 025 decisions #229, #231, and #247, interpretation
    /// #237).
    #[test]
    fn past_a_shared_scope_bound_every_domain_is_exhausted_at_its_first_unit() {
        failing().resolve(|evaluation, _| {
            let views = RecordViews::assign(PROFILE, &evaluation, &Sha256);
            let units: Vec<&Unit> = evaluation.statuses().map(|(unit, _)| unit).collect();
            for bound in [
                MetadataBound::SubjectOccurrences,
                MetadataBound::TokenInputs,
                MetadataBound::MetadataWorkUnits,
            ] {
                let limits = with(bound, 1);
                let reporting =
                    Reporting::new(&evaluation, PROFILE, (None, file_entry()), &limits, &Sha256);
                let reported = report(&reporting, &limits_with(None), None).unwrap();
                let exhausted = ExhaustedBound::Capability(bound.id());
                for domain in [ReportingDomain::Entry, ReportingDomain::Shared] {
                    assert_eq!(reporting.exceeded(domain), Some(bound), "{bound:?}");
                    let cursor = units
                        .iter()
                        .copied()
                        .find(|unit| ReportingDomain::of(unit.phase) == domain)
                        .unwrap();
                    assert_eq!(
                        of(&reported, domain),
                        &Committed {
                            slices: Vec::new(),
                            exhausted: Some(Exhausted {
                                cursor,
                                bound: exhausted,
                            }),
                        },
                        "{bound:?}"
                    );
                    let (finding, state) = reporting.terminal(domain, cursor, exhausted).unwrap();
                    assert_eq!(
                        finding["details"],
                        json!({
                            "reason": domain.id(),
                            "bound": format!("bootstrapOnly.evidenceMetadata.{}", bound.id().name),
                        })
                    );
                    assert_eq!(state["nextUnitCursor"], views.unit(cursor).unwrap());
                }
                // Only the subject tokens the terminal records name are
                // assigned, so a unit of another subject kind has no view
                // (plan 025 interpretation #237).
                let kinds: Vec<_> = [ReportingDomain::Entry, ReportingDomain::Shared]
                    .into_iter()
                    .map(|domain| reporting.first(domain).unwrap().subject.kind())
                    .collect();
                let other = units
                    .iter()
                    .copied()
                    .find(|unit| !kinds.contains(&unit.subject.kind()))
                    .unwrap();
                assert!(reporting
                    .terminal(ReportingDomain::Shared, other, exhausted)
                    .is_err());
            }
        });
    }

    /// The counts of the units that report into the Shared domain: their
    /// candidates, their subjects, and their tokens' comparisons and records.
    fn shared_scope(evaluation: &Evaluation) -> [(MetadataBound, u64); 4] {
        let shared = |unit: &Unit| ReportingDomain::of(unit.phase) == ReportingDomain::Shared;
        let subjects: Vec<_> = evaluation
            .statuses()
            .filter(|(unit, _)| shared(unit))
            .map(|(unit, _)| &unit.subject)
            .collect();
        let tokens = Tokens::assign(PROFILE, subjects.iter().copied(), &Sha256);
        [
            (
                MetadataBound::SubjectOccurrences,
                count(
                    evaluation
                        .candidates()
                        .filter(|(unit, _)| shared(unit))
                        .count(),
                ),
            ),
            (MetadataBound::TokenInputs, count(subjects.len())),
            (MetadataBound::CollisionComparisons, tokens.comparisons()),
            (MetadataBound::MetadataWorkUnits, tokens.records()),
        ]
    }

    /// Past a bound that only the complete invocation exceeds, a count down
    /// to what the Shared scope counts or a value locator past a path bound,
    /// only the Entry domain is exhausted, at its first unit and naming the
    /// bound, while the Shared domain commits exactly what it would within
    /// every bound, so that paired entries keep equal Shared projections
    /// (D15:1852, 2660, 3233; plan 025 decision #247). Under this digest no
    /// record collides, so the comparisons are not counted here.
    #[test]
    fn past_only_an_invocation_bound_only_the_entry_domain_is_exhausted() {
        failing().resolve(|evaluation, _| {
            let views = RecordViews::assign(PROFILE, &evaluation, &Sha256);
            let within = Reporting::new(
                &evaluation,
                PROFILE,
                (None, file_entry()),
                &unbounded(),
                &Sha256,
            );
            let within = report(&within, &limits_with(None), None).unwrap().shared;
            assert!(!within.slices.is_empty());
            let mut bounds = vec![(MetadataBound::LogicalPathSegments, 1)];
            bounds.extend(
                shared_scope(&evaluation)
                    .into_iter()
                    .filter(|(bound, _)| *bound != MetadataBound::CollisionComparisons),
            );
            for (bound, limit) in bounds {
                let limits = with(bound, limit);
                let reporting =
                    Reporting::new(&evaluation, PROFILE, (None, file_entry()), &limits, &Sha256);
                assert_eq!(
                    reporting.exceeded(ReportingDomain::Entry),
                    Some(bound),
                    "{bound:?}"
                );
                assert_eq!(
                    reporting.exceeded(ReportingDomain::Shared),
                    None,
                    "{bound:?}"
                );
                let reported = report(&reporting, &limits_with(None), None).unwrap();
                let cursor = reporting.first(ReportingDomain::Entry).unwrap();
                let exhausted = ExhaustedBound::Capability(bound.id());
                assert_eq!(
                    reported.entry,
                    Committed {
                        slices: Vec::new(),
                        exhausted: Some(Exhausted {
                            cursor,
                            bound: exhausted,
                        }),
                    },
                    "{bound:?}"
                );
                let (_, state) = reporting
                    .terminal(ReportingDomain::Entry, cursor, exhausted)
                    .unwrap();
                assert_eq!(state["nextUnitCursor"], views.unit(cursor).unwrap());
                assert_eq!(reported.shared, within, "{bound:?}");
            }
        });
    }

    /// The Shared projection stage plans the Shared domain before the Entry
    /// projection stage counts the complete invocation, under the subject
    /// tokens of the Shared scope and those the Entry domain's reserved
    /// terminal records name, and commits what it would under every unit's,
    /// a Phase 0 Finding that a Shared status names among them (D15:1852,
    /// 2146, 2517, 2660, 2776; plan 025 decision #253, interpretation #257).
    #[test]
    fn the_shared_domain_commits_alike_under_its_own_scopes_tokens() {
        let alike = |evaluation: Evaluation| {
            let limits = unbounded();
            let scope = crate::resolver::evidence_metadata::shared_scope(
                &evaluation,
                PROFILE,
                &limits,
                &Sha256,
            );
            let shared = Reporting::shared(&evaluation, PROFILE, None, &scope, &Sha256);
            let alone =
                report_domain(&shared, ReportingDomain::Shared, &limits_with(None), None).unwrap();
            assert!(!alone.slices.is_empty());
            let complete =
                Reporting::new(&evaluation, PROFILE, (None, file_entry()), &limits, &Sha256);
            let both = report(&complete, &limits_with(None), None).unwrap();
            assert_eq!(alone, both.shared);
        };
        Inputs::complete().resolve(|evaluation, _| alike(evaluation));
        failing().resolve(|evaluation, _| alike(evaluation));
        // A group's owner then waits on the Phase 0 Finding, which its Shared
        // status names.
        let mut unasserted = Inputs::complete();
        unasserted.unasserted();
        unasserted.resolve(|evaluation, _| alike(evaluation));
    }

    /// With no unit that reports into the Shared domain, a count past its
    /// bound exhausts only the Entry domain, and the Shared domain has
    /// nothing to commit (D15:1852, 2409).
    #[test]
    fn without_a_shared_unit_a_count_past_its_bound_exhausts_only_the_entry_domain() {
        let (evaluation, causes) = entered();
        let limits = with(MetadataBound::SubjectOccurrences, 1);
        let reporting = Reporting::new(
            &evaluation,
            PROFILE,
            (Some(&causes), file_entry()),
            &limits,
            &Sha256,
        );
        let reported = report(&reporting, &limits_with(None), None).unwrap();
        let first = evaluation.statuses().next().unwrap().0;
        assert_eq!(reported.entry.exhausted.map(|at| at.cursor), Some(first));
        assert_eq!(
            reported.shared,
            Committed {
                slices: Vec::new(),
                exhausted: None,
            }
        );
    }

    /// A Logical input path past a path bound is absent from every record
    /// that would carry it, and reporting goes on (D15:777; plan 025
    /// interpretation #234).
    #[test]
    fn a_logical_input_path_past_a_path_bound_is_absent_and_reporting_goes_on() {
        let (evaluation, causes) = entered();
        let paths = |limits: &EvidenceMetadataLimits| -> Vec<serde_json::Value> {
            let reporting = Reporting::new(
                &evaluation,
                PROFILE,
                (Some(&causes), file_entry()),
                limits,
                &Sha256,
            );
            assert_eq!(reporting.exceeded(ReportingDomain::Entry), None);
            report(&reporting, &limits_with(None), None)
                .unwrap()
                .entry
                .slices
                .iter()
                .flat_map(|slice| &slice.records)
                .filter_map(|retained| match &retained.record {
                    Record::Evidence(evidence) => evidence.view().get("path").cloned(),
                    _ => None,
                })
                .collect()
        };
        let kept = paths(&unbounded());
        assert!(!kept.is_empty());
        let largest = kept
            .iter()
            .map(|path| encoded_length(path).unwrap())
            .max()
            .unwrap();
        let tight = EvidenceMetadataLimits {
            max_logical_path_canonical_bytes: Bound::new(largest - 1).unwrap(),
            ..unbounded()
        };
        let left = paths(&tight);
        assert!(left.len() < kept.len());
        assert!(left
            .iter()
            .all(|path| encoded_length(path).unwrap() < largest));
    }

    /// Once admitted, the Policy recounts from zero the slices committed
    /// before it, and refuses a slice the capability let through, the domain
    /// keeping the slices before it while the other domain goes on. Exactly
    /// what they count passes, and the Policy then goes on from the recounted
    /// counts: one less than the domain's total refuses its last slice, past
    /// the admission (D15:2133, 2392).
    #[test]
    fn the_admitted_policy_recounts_what_was_committed_before_it() {
        failing().resolve(|evaluation, _| {
            let reporting = Reporting::new(
                &evaluation,
                PROFILE,
                (None, file_entry()),
                &unbounded(),
                &Sha256,
            );
            let generous = limits_of(&[]);
            let unbounded = report(&reporting, &generous, None).unwrap();
            let all = &unbounded.shared.slices;
            let before = all
                .iter()
                .position(|slice| *slice.unit >= admission())
                .unwrap();
            let records = summed("maxRecords", &all[..before]);
            assert!(records >= 2);
            let policy = limits_of(&[(shared("maxRecords"), records - 1)]);
            let reported = report(&reporting, &generous, Some(&policy)).unwrap();
            assert_eq!(
                reported.shared,
                Committed {
                    slices: all[..before - 1].to_vec(),
                    exhausted: Some(Exhausted {
                        cursor: all[before - 1].unit,
                        bound: ExhaustedBound::Domain(shared("maxRecords")),
                    }),
                }
            );
            assert_eq!(reported.entry, unbounded.entry);
            let total = summed("maxRecords", all);
            let exact = limits_of(&[(shared("maxRecords"), total)]);
            assert_eq!(
                report(&reporting, &generous, Some(&exact)).unwrap().shared,
                unbounded.shared
            );
            let last = all.len() - 1;
            assert!(*all[last].unit > admission());
            let over = limits_of(&[(shared("maxRecords"), total - 1)]);
            assert_eq!(
                report(&reporting, &generous, Some(&over)).unwrap().shared,
                Committed {
                    slices: all[..last].to_vec(),
                    exhausted: Some(Exhausted {
                        cursor: all[last].unit,
                        bound: ExhaustedBound::Domain(shared("maxRecords")),
                    }),
                }
            );
        });
    }

    /// The admitting unit's own slice is planned after its check admitted the
    /// Policy, so under it: when the capability would refuse the slice for
    /// one bound and the Policy for one declared before it, the terminal
    /// Finding names the Policy's (plan 025 interpretation #228).
    #[test]
    fn the_admitting_units_own_slice_is_planned_under_the_policy() {
        failing().resolve(|evaluation, _| {
            let reporting = Reporting::new(
                &evaluation,
                PROFILE,
                (None, file_entry()),
                &unbounded(),
                &Sha256,
            );
            let unbounded = report(&reporting, &limits_of(&[]), None).unwrap();
            let all = &unbounded.shared.slices;
            let at = all
                .iter()
                .position(|slice| *slice.unit == admission())
                .unwrap();
            let records = summed("maxRecords", &all[..at]);
            let statuses = summed("maxEvaluationStatusEntries", &all[..at]);
            let capability = limits_of(&[(shared("maxRecords"), records)]);
            let policy = limits_of(&[
                (shared("maxRecords"), records),
                (shared("maxEvaluationStatusEntries"), statuses),
            ]);
            let refused = |policy| report(&reporting, &capability, policy).unwrap().shared;
            assert_eq!(
                refused(None).exhausted,
                Some(Exhausted {
                    cursor: all[at].unit,
                    bound: ExhaustedBound::Domain(shared("maxRecords")),
                })
            );
            assert_eq!(
                refused(Some(&policy)),
                Committed {
                    slices: all[..at].to_vec(),
                    exhausted: Some(Exhausted {
                        cursor: all[at].unit,
                        bound: ExhaustedBound::Domain(shared("maxEvaluationStatusEntries")),
                    }),
                }
            );
        });
    }

    /// A domain the capability exhausted before the admission keeps its
    /// terminal cursor and bound when the Policy's recount of what it
    /// committed passes, even when the Policy would have refused the slice
    /// for another bound, and ends earlier where the recount refuses
    /// (D15:2392; plan 025 interpretation #228).
    #[test]
    fn a_domain_exhausted_before_the_admission_ends_no_later_after_the_recount() {
        failing().resolve(|evaluation, _| {
            let reporting = Reporting::new(
                &evaluation,
                PROFILE,
                (None, file_entry()),
                &unbounded(),
                &Sha256,
            );
            let unbounded = report(&reporting, &limits_of(&[]), None).unwrap();
            let all = &unbounded.shared.slices;
            let before = all
                .iter()
                .position(|slice| *slice.unit >= admission())
                .unwrap();
            let refused = before - 1;
            assert!(refused >= 2);
            let records = summed("maxRecords", &all[..refused]);
            let statuses = summed("maxEvaluationStatusEntries", &all[..refused]);
            let capability = limits_of(&[(shared("maxRecords"), records)]);
            let passing = limits_of(&[(shared("maxEvaluationStatusEntries"), statuses)]);
            assert_eq!(
                report(&reporting, &capability, Some(&passing))
                    .unwrap()
                    .shared,
                Committed {
                    slices: all[..refused].to_vec(),
                    exhausted: Some(Exhausted {
                        cursor: all[refused].unit,
                        bound: ExhaustedBound::Domain(shared("maxRecords")),
                    }),
                }
            );
            let refusing = limits_of(&[(shared("maxRecords"), records - 1)]);
            assert_eq!(
                report(&reporting, &capability, Some(&refusing))
                    .unwrap()
                    .shared,
                Committed {
                    slices: all[..refused - 1].to_vec(),
                    exhausted: Some(Exhausted {
                        cursor: all[refused - 1].unit,
                        bound: ExhaustedBound::Domain(shared("maxRecords")),
                    }),
                }
            );
        });
    }

    /// A domain with no slice at or after the admitting unit recounts under
    /// the Policy once it has planned every slice (plan 025 interpretation
    /// #228).
    #[test]
    fn a_domain_with_no_slice_after_the_admission_recounts_at_its_end() {
        let (evaluation, causes) = entered();
        let reporting = Reporting::new(
            &evaluation,
            PROFILE,
            (Some(&causes), file_entry()),
            &unbounded(),
            &Sha256,
        );
        let generous = limits_of(&[]);
        let all = report(&reporting, &generous, None).unwrap().entry.slices;
        assert!(all.iter().all(|slice| *slice.unit < admission()));
        let total = summed("maxRecords", &all);
        let policy = limits_of(&[(entry("maxRecords"), total - 1)]);
        let last = all.len() - 1;
        assert_eq!(
            report(&reporting, &generous, Some(&policy)).unwrap().entry,
            Committed {
                slices: all[..last].to_vec(),
                exhausted: Some(Exhausted {
                    cursor: all[last].unit,
                    bound: ExhaustedBound::Domain(entry("maxRecords")),
                }),
            }
        );
    }
}
