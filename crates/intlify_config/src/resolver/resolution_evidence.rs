// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's Resolution Evidence projections: what reporting finalization
//! makes of the slices each diagnostic domain committed (D15:1162-1184,
//! 2144).
//!
//! Finalization only orders. Every record keeps the owner and bytes its slice
//! committed it with, and no record is added, dropped, or moved to the other
//! domain (D15:2144, 2212). A projection holds its records record kind by
//! record kind, in 015's order of record kinds (D15:2519), and each kind's
//! records in canonical order. The slices commit unit by unit in canonical
//! unit order, so their order is already canonical for every kind but the
//! evidence index: Findings in their total order and occurrences in theirs
//! (D15:2214, 2390), Shared-unavailability causes by the Finding Keys they
//! name (D15:1166), statuses and semantic dependencies by their owners, and
//! the decision table's records and the dependency records of one owner in
//! the order their slice holds them (D15:1608, 1612; plan 025 interpretation
//! #244). A slice holds an index
//! record for each Evidence it is the first to refer to, so the domain's
//! index is reordered by the Evidence comparisons (D15:841-843, 2506; plan
//! 025 interpretation #250).
//!
//! A domain that a bound exhausted also holds its terminal Finding and terminal
//! state, which take its reserved slots rather than being records of a slice
//! (D15:1973, 2409, 2519), and, without a Shared projection, the Entry reserve
//! holds the Shared-unavailability cause that names the Entry terminal Finding
//! (D15:2015). Resolution Evidence reserves no Rust struct or wire encoding
//! (D15:1162), so the projection keeps them beside its records.

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

    use intlify_shared_json::encoding::{digest_bytes, encode, encoded_length};
    use serde_json::{json, Value};

    use crate::construction::registry::check::RecordKind;
    use crate::resolver::dependency::fixtures::file_entry;
    use crate::resolver::evaluation::{IssueKey, Reason, Unit};
    use crate::resolver::evidence::Evidence;
    use crate::resolver::evidence_metadata::fixtures::unbounded;
    use crate::resolver::evidence_source::hex;
    use crate::resolver::limits::{BoundId, Group};
    use crate::resolver::profile_resolution::tests::Inputs;
    use crate::resolver::record_view::RecordViews;
    use crate::resolver::reporting::{Record, Reporting, ReportingDomain, Retained};
    use crate::resolver::reporting_plan::tests::{entered, failing, limits_with, PROFILE};
    use crate::resolver::reporting_plan::{report, Committed};
    use crate::resolver::token::fixtures::LastByte;
    use crate::resolver::token::{Digest, Sha256};

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
    /// Each record's view is the one its bytes count (D15:2519).
    fn each_projection(mut check: impl FnMut(&Projection<'_>, &[(&Unit, Retained<'_>)])) {
        each_viewed_projection(|projection, records, _| check(projection, records));
    }

    /// `each_projection`, with the views each projection's records were
    /// charged under.
    fn each_viewed_projection(
        mut check: impl FnMut(&Projection<'_>, &[(&Unit, Retained<'_>)], &RecordViews<'_>),
    ) {
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
                check(&projection, &records, views);
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
    /// twelve kinds between them (D15:2144, 2212, 2519).
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

    /// One record of each of the twelve kinds is charged the frozen `C` of
    /// its accounting view. A Python script wrote each view from D15:2473-2519
    /// and framed it apart from this crate, reading only each row's strategy,
    /// scope, projection, severity, and blocking state from the committed
    /// registries (`.plans/drafts/025-pr7/t2-vectors.py`; D15:3455).
    #[test]
    fn each_record_kinds_accounting_view_is_its_frozen_vector() {
        let vectors: Value =
            serde_json::from_str(include_str!("../../fixtures/evidence-records/vectors.json"))
                .unwrap();
        let vectors = vectors["accountingViews"].as_array().unwrap();
        assert_eq!(vectors.len(), 12);
        let mut matched = BTreeSet::new();
        each_viewed_projection(|projection, _, views| {
            for (owner, retained) in &projection.records {
                let accounting = views
                    .accounting(owner, retained.kind, retained.view.clone())
                    .unwrap();
                let Some(vector) = vectors
                    .iter()
                    .find(|vector| vector["accounting"] == accounting)
                else {
                    continue;
                };
                let kind = retained.kind.id();
                assert_eq!(vector["recordKind"], kind);
                let bytes = encode(&accounting).unwrap();
                assert_eq!(hex(&bytes), vector["preimage"].as_str().unwrap(), "{kind}");
                assert_eq!(
                    format!("sha256:{}", hex(&digest_bytes(&bytes))),
                    vector["digest"].as_str().unwrap(),
                    "{kind}"
                );
                assert_eq!(
                    retained.bytes,
                    u64::try_from(bytes.len()).unwrap(),
                    "{kind}"
                );
                matched.insert(kind);
            }
        });
        assert_eq!(matched.len(), 12, "{matched:?}");
    }

    /// The view of every record `inputs` retains in either domain, its
    /// tokens assigned under `digest`.
    fn retained_views(inputs: &Inputs, digest: &impl Digest) -> Vec<Value> {
        inputs.resolve(|evaluation, _| {
            let reporting = Reporting::new(
                &evaluation,
                PROFILE,
                (None, file_entry()),
                &unbounded(),
                digest,
            );
            let reported = report(&reporting, &limits_with(None), None).unwrap();
            [reported.entry, reported.shared]
                .into_iter()
                .flat_map(|committed| committed.slices)
                .flat_map(|slice| slice.records)
                .map(|retained| retained.view)
                .collect()
        })
    }

    /// The `redacted-key` token of each occurrence's primary Evidence.
    fn redacted_keys(views: &[Value]) -> BTreeSet<String> {
        views
            .iter()
            .filter_map(|view| view.get("primary")?.get("component")?.get(1))
            .filter_map(|key| key.get("redacted-key"))
            .map(Value::to_string)
            .collect()
    }

    /// Rejected envelopes and an unsafe vocabulary member reach retained
    /// records and Finding Occurrence Keys only as the `redacted-key` of their
    /// JSON type, whose frozen tokens the vectors hold (D15:2469). Their raw
    /// bodies, keys, secret-resolving locators, and direct digests are absent,
    /// the position each was submitted at changes nothing, and an injected
    /// collision keeps the two keys apart by their ordinals (D15:826, 828,
    /// 835, 3503).
    #[test]
    fn rejected_inputs_leave_nothing_raw_in_retained_records() {
        const CANARIES: [&str; 5] = [
            "t2-canary-kind",
            "vault://t2-canary-locator",
            "t2-canary-key",
            "t2-canary-body",
            "T2-CANARY-SURFACE",
        ];
        let vectors: Value =
            serde_json::from_str(include_str!("../../fixtures/evidence-records/vectors.json"))
                .unwrap();
        let frozen = |origin: &str, json_type: &str| {
            let row = vectors["evidenceTokens"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| {
                    row["evidenceKind"] == "resolver-input-component-key"
                        && row["origin"] == origin
                        && row["projected"]["type"] == json_type
                })
                .unwrap();
            json!({ "token": row["token"] }).to_string()
        };
        let envelope = json!({
            "kind": "t2-canary-kind",
            "schemaRevision": "0",
            "secretLocator": "vault://t2-canary-locator",
            "t2-canary-key": "t2-canary-body",
        });
        let string = json!("t2-canary-body");
        let (mut first, mut second) = (Inputs::complete(), Inputs::complete());
        first.submit(envelope.clone());
        first.submit(string.clone());
        second.submit(string.clone());
        second.submit(envelope.clone());
        let views = retained_views(&first, &Sha256);
        assert_eq!(
            views,
            retained_views(&second, &Sha256),
            "a submitted index reached a record"
        );
        let collection = "profile-resolution-artifact-collection";
        assert_eq!(
            redacted_keys(&views),
            BTreeSet::from([frozen(collection, "object"), frozen(collection, "string")])
        );
        let colliding = retained_views(&first, &LastByte(&[]));
        let ordinals: BTreeSet<String> = redacted_keys(&colliding)
            .iter()
            .map(|key| serde_json::from_str::<Value>(key).unwrap()["collision"].to_string())
            .collect();
        assert_eq!(
            ordinals,
            BTreeSet::from([json!("0").to_string(), json!("1").to_string()])
        );
        let mut vocabulary = Inputs::complete();
        vocabulary.submit_surfaces(&["nav", "T2-CANARY-SURFACE"]);
        let surface = retained_views(&vocabulary, &Sha256);
        assert_eq!(
            redacted_keys(&surface),
            BTreeSet::from([frozen("intent-surface-class-vocabulary", "string")])
        );
        let text: String = views
            .iter()
            .chain(&colliding)
            .chain(&surface)
            .map(Value::to_string)
            .collect();
        for canary in CANARIES {
            assert!(!text.contains(canary), "{canary}");
            assert!(
                !text.contains(&hex(&digest_bytes(canary.as_bytes()))),
                "{canary}"
            );
        }
        for body in [&envelope, &string] {
            let bytes = encode(body).unwrap();
            assert!(!text.contains(&hex(&bytes)));
            assert!(!text.contains(&hex(&digest_bytes(&bytes))));
        }
    }

    /// The index takes the Evidence comparisons, not the order of the slices
    /// that first referred to each Evidence, which differ for at least one of
    /// the invocations; each Evidence has one index record (D15:841-843,
    /// 2506; plan 025 interpretation #250).
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
    /// over at least two records (D15:2214, 2390; plan 025 interpretation
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
    /// state of how it was exhausted (D15:1186, 2212, 2409, 2519).
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
    /// Finding (D15:2015, 2409).
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
