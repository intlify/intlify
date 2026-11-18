// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's Resolution Evidence projections: what reporting finalization
//! makes of the slices each diagnostic domain committed (D15:1161-1183,
//! 2143).
//!
//! Finalization only orders. Every record keeps the owner and bytes its slice
//! committed it with, and no record is added, dropped, or moved to the other
//! domain (D15:2143, 2211). A projection holds its records record kind by
//! record kind, in 015's order of record kinds (D15:2518), and each kind's
//! records in canonical order. The slices commit unit by unit in canonical
//! unit order, so their order is already canonical for every kind but the
//! evidence index: Findings in their total order and occurrences in theirs
//! (D15:2213, 2389), Shared-unavailability causes by the Finding Keys they
//! name (D15:1165), statuses and semantic dependencies by their owners, and
//! the decision table's records and the dependency records of one owner in
//! the order their slice holds them (D15:1607, 1611; plan 025 interpretation
//! #244). A slice holds an index
//! record for each Evidence it is the first to refer to, so the domain's
//! index is reordered by the Evidence comparisons (D15:840-842, 2505; plan
//! 025 interpretation #250).
//!
//! A domain that a bound exhausted also holds its terminal Finding and terminal
//! state, which take its reserved slots rather than being records of a slice
//! (D15:1972, 2408, 2518), and, without a Shared projection, the Entry reserve
//! holds the Shared-unavailability cause that names the Entry terminal Finding
//! (D15:2014). Resolution Evidence reserves no Rust struct or wire encoding
//! (D15:1161), so the projection keeps them beside its records.

use serde_json::Value;

use crate::construction::registry::check::RecordKind;

use super::evaluation::Unit;
use super::evidence::Evidence;
use super::reporting::{Record, Reporting, ReportingDomain, ReportingInvariant, Retained};
use super::reporting_plan::{Committed, Exhausted};

/// An exhausted domain's terminal reporting-limit Finding and terminal state,
/// as their views, with how the domain was exhausted, and the view of the
/// Shared-unavailability cause its reserve holds, if it holds one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Terminal<'e> {
    pub(crate) exhausted: Exhausted<'e>,
    pub(crate) finding: Value,
    pub(crate) state: Value,
    pub(crate) cause: Option<Value>,
}

/// One diagnostic domain's Resolution Evidence projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Projection<'e> {
    pub(crate) domain: ReportingDomain,
    /// Every record the domain committed, with its Evidence record owner.
    pub(crate) records: Vec<(&'e Unit, Retained<'e>)>,
    /// The terminal Finding and state, when a bound exhausted the domain.
    pub(crate) terminal: Option<Terminal<'e>>,
}

impl<'e> Projection<'e> {
    /// The projection of what `domain` committed of `reporting`'s slices.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug that leaves the terminal Finding or state
    /// without its view.
    pub(crate) fn finalized(
        reporting: &Reporting<'e, '_>,
        domain: ReportingDomain,
        committed: Committed<'e>,
    ) -> Result<Self, ReportingInvariant> {
        let mut records: Vec<(&Unit, Retained)> = committed
            .slices
            .into_iter()
            .flat_map(|slice| {
                let unit = slice.unit;
                slice
                    .records
                    .into_iter()
                    .map(move |retained| (unit, retained))
            })
            .collect();
        // Stable, so each kind's records keep their canonical unit order.
        records.sort_by_key(|(_, retained)| retained.kind);
        let start =
            records.partition_point(|(_, retained)| retained.kind < RecordKind::EvidenceIndex);
        let end =
            records.partition_point(|(_, retained)| retained.kind <= RecordKind::EvidenceIndex);
        records[start..end].sort_by(|(_, left), (_, right)| indexed(left).cmp(&indexed(right)));
        let terminal = committed
            .exhausted
            .map(|exhausted| {
                let (finding, state) =
                    reporting.terminal(domain, exhausted.cursor, exhausted.bound)?;
                Ok::<_, ReportingInvariant>(Terminal {
                    exhausted,
                    finding,
                    state,
                    cause: reporting.reserved_cause()?,
                })
            })
            .transpose()?;
        Ok(Self {
            domain,
            records,
            terminal,
        })
    }
}

/// The Evidence of an evidence-index record.
fn indexed<'r>(retained: &'r Retained<'_>) -> Option<&'r Evidence> {
    match &retained.record {
        Record::Evidence(evidence) => Some(evidence),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use intlify_shared_json::encoding::encoded_length;

    use crate::construction::registry::check::RecordKind;
    use crate::resolver::dependency::fixtures::file_entry;
    use crate::resolver::evaluation::{IssueKey, Reason, Unit};
    use crate::resolver::evidence::Evidence;
    use crate::resolver::evidence_metadata::fixtures::unbounded;
    use crate::resolver::limits::{BoundId, Group};
    use crate::resolver::profile_resolution::tests::Inputs;
    use crate::resolver::record_view::RecordViews;
    use crate::resolver::reporting::{Record, Reporting, ReportingDomain, Retained};
    use crate::resolver::reporting_plan::tests::{entered, failing, limits_with, PROFILE};
    use crate::resolver::reporting_plan::{report, Committed};
    use crate::resolver::token::Sha256;

    use super::{indexed, Projection, Terminal};

    /// Each slice's records, slice by slice, with the slice's unit.
    fn owned<'e>(committed: &Committed<'e>) -> Vec<(&'e Unit, Retained<'e>)> {
        committed
            .slices
            .iter()
            .flat_map(|slice| {
                slice
                    .records
                    .iter()
                    .map(move |retained| (slice.unit, retained.clone()))
            })
            .collect()
    }

    fn of<'r, 'e>(
        records: &'r [(&'e Unit, Retained<'e>)],
        kind: RecordKind,
    ) -> Vec<&'r (&'e Unit, Retained<'e>)> {
        records
            .iter()
            .filter(|(_, retained)| retained.kind == kind)
            .collect()
    }

    /// Whether `items` strictly increase.
    fn increasing<T: Ord>(items: &[T]) -> bool {
        items.windows(2).all(|pair| pair[0] < pair[1])
    }

    /// What `check` asserts of each domain's projection within every bound,
    /// with the domain's committed records, for each of three invocations: the
    /// complete one, `failing`'s, and `entered`'s, which materializes no value.
    /// Each record's view is the one its bytes count (D15:2518).
    fn each_projection(mut check: impl FnMut(&Projection<'_>, &[(&Unit, Retained<'_>)])) {
        let mut project = |reporting: &Reporting<'_, '_>, views: &RecordViews<'_>| {
            let reported = report(reporting, &limits_with(None), None).unwrap();
            for (domain, committed) in [
                (ReportingDomain::Entry, reported.entry),
                (ReportingDomain::Shared, reported.shared),
            ] {
                assert!(committed.exhausted.is_none());
                let records = owned(&committed);
                let projection = Projection::finalized(reporting, domain, committed).unwrap();
                assert_eq!(projection.domain, domain);
                assert!(projection.terminal.is_none());
                for (owner, retained) in &projection.records {
                    let accounting = views
                        .accounting(owner, retained.kind, retained.view.clone())
                        .unwrap();
                    assert_eq!(encoded_length(&accounting).unwrap(), retained.bytes);
                }
                check(&projection, &records);
            }
        };
        for inputs in [Inputs::complete(), failing()] {
            inputs.resolve(|evaluation, _| {
                project(
                    &Reporting::new(
                        &evaluation,
                        PROFILE,
                        (None, file_entry()),
                        &unbounded(),
                        &Sha256,
                    ),
                    &RecordViews::assign(PROFILE, &evaluation, &Sha256),
                );
            });
        }
        let (evaluation, causes) = entered();
        project(
            &Reporting::new(
                &evaluation,
                PROFILE,
                (Some(&causes), file_entry()),
                &unbounded(),
                &Sha256,
            ),
            &RecordViews::assign(PROFILE, &evaluation, &Sha256),
        );
    }

    /// A projection holds the records its domain committed, record kind by
    /// record kind in 015's order of record kinds, each with the owner and
    /// bytes it was committed with. Every kind but the evidence index keeps
    /// the order its slices committed it in; the three invocations hold all
    /// twelve kinds between them (D15:2143, 2211, 2518).
    #[test]
    fn a_projection_groups_its_records_by_kind_and_keeps_each_kinds_order() {
        let mut kinds = BTreeSet::new();
        each_projection(|projection, committed| {
            let order: Vec<RecordKind> = projection
                .records
                .iter()
                .map(|(_, retained)| retained.kind)
                .collect();
            assert!(order.is_sorted(), "{order:?}");
            assert_eq!(projection.records.len(), committed.len());
            for kind in committed.iter().map(|(_, retained)| retained.kind) {
                kinds.insert(kind);
                let projected = of(&projection.records, kind);
                let mut held = of(committed, kind);
                if kind == RecordKind::EvidenceIndex {
                    held.sort_by(|(_, left), (_, right)| indexed(left).cmp(&indexed(right)));
                }
                assert_eq!(projected, held, "{kind:?}");
            }
        });
        assert_eq!(kinds.len(), 12, "{kinds:?}");
    }

    /// The index takes the Evidence comparisons, not the order of the slices
    /// that first referred to each Evidence, which differ for at least one of
    /// the invocations; each Evidence has one index record (D15:840-842,
    /// 2505; plan 025 interpretation #250).
    #[test]
    fn the_evidence_index_takes_the_evidence_order() {
        let mut reordered = false;
        each_projection(|projection, committed| {
            let evidence = |records: &[(&Unit, Retained<'_>)]| -> Vec<Evidence> {
                of(records, RecordKind::EvidenceIndex)
                    .into_iter()
                    .map(|(_, retained)| indexed(retained).unwrap().clone())
                    .collect()
            };
            let index = evidence(&projection.records);
            assert!(increasing(&index), "{index:?}");
            reordered |= evidence(committed) != index;
        });
        assert!(reordered, "no slice order differs from the Evidence order");
    }

    /// What the slices' canonical unit order gives each kind whose records
    /// several units own: Findings in their total order, occurrences by
    /// Finding, primary Evidence, and ordinal, and statuses, causes, and
    /// semantic dependencies by their owners, with every kind's order checked
    /// over at least two records (D15:2213, 2389; plan 025 interpretation
    /// #244).
    #[test]
    fn the_kinds_several_units_own_come_in_their_canonical_order() {
        let mut checked = BTreeSet::new();
        each_projection(|projection, _| {
            let records = &projection.records;
            let mut findings: Vec<&IssueKey> = Vec::new();
            let mut occurrences = Vec::new();
            let mut causes: Vec<&IssueKey> = Vec::new();
            for (_, retained) in records {
                match &retained.record {
                    Record::Finding(finding) => findings.push(finding.key),
                    Record::Occurrence(occurrence) => occurrences.push(occurrence.key()),
                    Record::UnavailabilityCause(cause) => causes.push(*cause),
                    _ => {}
                }
            }
            let owners = |kind: RecordKind| -> Vec<&Unit> {
                of(records, kind)
                    .into_iter()
                    .map(|(unit, _)| *unit)
                    .collect()
            };
            let orders = [
                ("findings", findings.len(), increasing(&findings)),
                ("occurrences", occurrences.len(), increasing(&occurrences)),
                ("causes", causes.len(), increasing(&causes)),
            ]
            .into_iter()
            .chain(
                [
                    RecordKind::EntryEvaluationStatus,
                    RecordKind::SharedEvaluationStatus,
                    RecordKind::SemanticDependency,
                ]
                .into_iter()
                .map(|kind| {
                    let owners = owners(kind);
                    (kind.id(), owners.len(), increasing(&owners))
                }),
            );
            for (name, count, increases) in orders {
                assert!(increases, "{name}");
                if count >= 2 {
                    checked.insert(name);
                }
            }
        });
        assert_eq!(
            checked,
            BTreeSet::from([
                "causes",
                "entry-evaluation-status",
                "findings",
                "occurrences",
                "semantic-dependency",
                "shared-evaluation-status",
            ])
        );
    }

    /// A domain that a bound exhausted projects the prefix it committed as
    /// it projects it within every bound, and holds the terminal Finding and
    /// state of how it was exhausted (D15:1185, 2211, 2408, 2518).
    #[test]
    fn an_exhausted_domain_holds_its_terminal_finding_and_state() {
        failing().resolve(|evaluation, _| {
            let reporting = Reporting::new(
                &evaluation,
                PROFILE,
                (None, file_entry()),
                &unbounded(),
                &Sha256,
            );
            let within = report(&reporting, &limits_with(None), None).unwrap();
            for (domain, group, all) in [
                (
                    ReportingDomain::Entry,
                    Group::DiagnosticsEntry,
                    &within.entry,
                ),
                (
                    ReportingDomain::Shared,
                    Group::DiagnosticsShared,
                    &within.shared,
                ),
            ] {
                let half = u64::try_from(owned(all).len() / 2).unwrap();
                let bound = BoundId {
                    group,
                    name: "maxRecords",
                };
                let reported = report(&reporting, &limits_with(Some((bound, half))), None).unwrap();
                let committed = match domain {
                    ReportingDomain::Entry => reported.entry,
                    ReportingDomain::Shared => reported.shared,
                };
                let exhausted = committed.exhausted.unwrap();
                let prefix = Committed {
                    slices: committed.slices.clone(),
                    exhausted: None,
                };
                let projection = Projection::finalized(&reporting, domain, committed).unwrap();
                let records = Projection::finalized(&reporting, domain, prefix)
                    .unwrap()
                    .records;
                assert!(!records.is_empty());
                assert_eq!(projection.records, records);
                let (finding, state) = reporting
                    .terminal(domain, exhausted.cursor, exhausted.bound)
                    .unwrap();
                // With a Shared projection, no reserve holds a cause.
                assert_eq!(
                    projection.terminal,
                    Some(Terminal {
                        exhausted,
                        finding,
                        state,
                        cause: None,
                    })
                );
            }
        });
    }

    /// Without a Shared projection, the reserve of the exhausted Entry domain
    /// also holds the Shared-unavailability cause that names its terminal
    /// Finding (D15:2014, 2408).
    #[test]
    fn without_a_value_the_entry_reserve_holds_the_unavailability_cause() {
        let (evaluation, causes) = entered();
        let views = RecordViews::assign(PROFILE, &evaluation, &Sha256);
        let reporting = Reporting::new(
            &evaluation,
            PROFILE,
            (Some(&causes), file_entry()),
            &unbounded(),
            &Sha256,
        );
        let bound = BoundId {
            group: Group::DiagnosticsEntry,
            name: "maxRecords",
        };
        let reported = report(&reporting, &limits_with(Some((bound, 1))), None).unwrap();
        let entry = Projection::finalized(&reporting, ReportingDomain::Entry, reported.entry)
            .unwrap()
            .terminal
            .unwrap();
        let cause = views
            .shared_unavailability_cause(&IssueKey::terminal(Reason::Entry))
            .unwrap();
        assert_eq!(entry.cause, Some(cause));
    }
}
