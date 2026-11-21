// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's reporting finalization checks: what Phase 15 verifies of the
//! records the earlier stages fixed before it returns an outcome, and the
//! mandatory reserve that the outcome's reserved records take (D15:1185,
//! 1611, 2143, 2385, 2518).
//!
//! Finalization only orders: it cannot create or deduplicate a candidate,
//! assign or change an ordinal, key, owner, or byte size, repair a mapping, or
//! move a record between domains (D15:2143). It verifies what the slices
//! committed instead. Each record is of its projection's domain, and the Check
//! Registry row of its owner's check lists its kind (D15:2518, 3224); a
//! Finding, its occurrences, and the Shared-unavailability cause naming it are
//! owned by the Finding's unit, and an evidence-index record by the first unit
//! that refers to its Evidence (D15:1972, 2205). Every Finding candidate but
//! the terminal one contributed an occurrence candidate, and the occurrences
//! sharing a Finding Key and a primary Evidence have the ordinals `0..n-1`
//! (D15:2385). Every Evidence that an occurrence or a Coverage Decision
//! Evidence refers to has its evidence-index record, and every Finding Key that
//! an occurrence, a not-evaluated status, or a Shared-unavailability cause
//! names has its Finding, unless the Finding lies in the omitted suffix of its
//! exhausted domain (D15:1185). When both domains are complete, the Coverage
//! Decision Evidence covers each Basis exactly: one record for a `default`
//! Basis and one for each matched rule domain of a `rule` one, in Basis cell
//! order and then matched-domain order, and no other (D15:1611). A check that
//! fails is a resolver bug, not an outcome. A replay, which has no entry,
//! verifies within the Shared domain what those checks verify of it
//! (D15:2659).
//!
//! The mandatory reserve holds what no ordinary bound counts: the outcome
//! envelope `{ outcome, constructionIdentity, complete }`, each exhausted
//! domain's terminal Finding and `TerminalState`, and the Entry reserve's
//! Shared-unavailability cause. The lengths of `C` of their views together fit
//! the capability's `maxCanonicalBytes`, which conformance proves of the
//! largest outcome its construction permits (D15:1847, 1849, 2518), so an
//! outcome past it is a resolver bug too.

use std::collections::{BTreeMap, BTreeSet};

use intlify_shared_json::encoding::encoded_length;
use serde_json::{json, Value};

use crate::construction::registry::check::{possible_record_kinds, RecordKind};

use super::capability::MandatoryReserveLimits;
use super::coverage::Decision;
use super::evaluation::{Evaluation, IssueKey, Status, Unit};
use super::evidence::Evidence;
use super::outcome::{Outcome, ResolutionEvidence};
use super::reporting::{Record, ReportingDomain};
use super::resolution_evidence::Projection;

/// A resolver bug that reporting finalization finds in what the earlier
/// stages fixed, which no input causes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FinalizationInvariant {
    /// A record of `kind` in the other domain's projection, or owned by a
    /// unit that may not own it.
    Owner { owner: Unit, kind: RecordKind },
    /// An evidence-index record owned by another unit than the first that
    /// refers to its Evidence.
    IndexOwner(Unit),
    /// A Finding candidate that carried nothing it observed (D15:2385).
    Unobserved(IssueKey),
    /// Occurrences of this Finding sharing a primary Evidence whose ordinals
    /// are not `0..n-1` (D15:2385).
    Ordinals(IssueKey),
    /// A Finding Key, named by a record, whose Finding is neither retained
    /// nor in an omitted suffix (D15:1185).
    UnresolvedFinding(IssueKey),
    /// A record of this unit referring to Evidence without its
    /// evidence-index record (D15:1182).
    UnresolvedEvidence(Unit),
    /// Coverage Decision Evidence that does not cover each Basis exactly
    /// (D15:1611).
    Cover,
    /// The mandatory reserve's `bytes` past the capability's `limit`.
    Reserve { bytes: u64, limit: u64 },
}

/// Verifies what `evaluation` recorded of every Finding candidate and what
/// the slices of `evidence` committed.
///
/// # Errors
///
/// Returns the first resolver bug a check finds.
pub(crate) fn verify(
    evaluation: &Evaluation,
    evidence: &ResolutionEvidence<'_>,
) -> Result<(), FinalizationInvariant> {
    observed(evaluation)?;
    for projection in std::iter::once(&evidence.entry).chain(&evidence.shared) {
        owners(projection)?;
    }
    ordinals(&evidence.entry)?;
    references(evidence)?;
    if evidence.complete() {
        cover(evidence)?;
    }
    Ok(())
}

/// Verifies, within the Shared domain alone, what `evaluation` recorded of
/// every Finding candidate and what the slices of a replay's `shared`
/// projection committed: each record's domain and owner, and every Finding
/// Key and Evidence a record refers to of that domain (D15:2659). A Finding
/// Key of the Entry domain, the Phase 0 Finding a status may name, is the
/// complete bundle's to resolve, as are the occurrence ordinals and the
/// exact cover, which are Entry records (D15:1185, 2143).
///
/// # Errors
///
/// Returns the first resolver bug a check finds.
pub(crate) fn verify_shared(
    evaluation: &Evaluation,
    shared: &Projection<'_>,
) -> Result<(), FinalizationInvariant> {
    observed(evaluation)?;
    owners(shared)?;
    resolved(&[shared], &[ReportingDomain::Shared])
}

/// Whether every Finding candidate carried what it observed (D15:2385).
fn observed(evaluation: &Evaluation) -> Result<(), FinalizationInvariant> {
    evaluation.unobserved().first().map_or(Ok(()), |key| {
        Err(FinalizationInvariant::Unobserved(key.clone()))
    })
}

/// Whether each record of `projection` is of its domain and owned by a unit
/// that may own it, a unit that produced or first referred to it where the
/// record names it (D15:1972, 2205, 2518).
fn owners(projection: &Projection<'_>) -> Result<(), FinalizationInvariant> {
    let mut first: BTreeMap<&Evidence, &Unit> = BTreeMap::new();
    for (owner, retained) in &projection.records {
        let kind = retained.kind;
        let produced = match &retained.record {
            Record::Finding(finding) => finding.key.unit() == *owner,
            Record::Occurrence(occurrence) => occurrence.owner() == *owner,
            Record::UnavailabilityCause(key) => key.unit() == *owner,
            _ => true,
        };
        if ReportingDomain::holding(kind) != projection.domain
            || !possible_record_kinds(owner.subject.kind(), owner.check).contains(&kind)
            || !produced
        {
            return Err(FinalizationInvariant::Owner {
                owner: (*owner).clone(),
                kind,
            });
        }
        for (referrer, evidence) in referred_evidence(&retained.record) {
            let earliest = first.entry(evidence).or_insert(referrer);
            *earliest = (*earliest).min(referrer);
        }
    }
    for (owner, retained) in &projection.records {
        if let Record::Evidence(evidence) = &retained.record {
            if first.get(evidence) != Some(owner) {
                return Err(FinalizationInvariant::IndexOwner((*owner).clone()));
            }
        }
    }
    Ok(())
}

/// Whether each group of occurrences sharing a Finding Key and a primary
/// Evidence has the ordinals `0..n-1`, in their order (D15:2385, 2389).
fn ordinals(entry: &Projection<'_>) -> Result<(), FinalizationInvariant> {
    let mut groups: BTreeMap<(&IssueKey, &Evidence), Vec<u64>> = BTreeMap::new();
    for (_, retained) in &entry.records {
        if let Record::Occurrence(occurrence) = &retained.record {
            groups
                .entry((occurrence.finding(), occurrence.primary()))
                .or_default()
                .push(occurrence.ordinal());
        }
    }
    for ((finding, _), ordinals) in groups {
        if !ordinals
            .iter()
            .copied()
            .eq(0..crate::artifacts::count(ordinals.len()))
        {
            return Err(FinalizationInvariant::Ordinals(finding.clone()));
        }
    }
    Ok(())
}

/// Whether every reference a record of the bundle `evidence` holds resolves,
/// whichever domain it names ([`resolved`]).
fn references(evidence: &ResolutionEvidence<'_>) -> Result<(), FinalizationInvariant> {
    let projections: Vec<&Projection<'_>> = std::iter::once(&evidence.entry)
        .chain(&evidence.shared)
        .collect();
    resolved(
        &projections,
        &[ReportingDomain::Entry, ReportingDomain::Shared],
    )
}

/// Whether every Finding Key of `domains` that a record of `projections`
/// names has its Finding, retained or in the omitted suffix of its exhausted
/// domain, and every Evidence a record refers to its evidence-index record
/// (D15:1182, 1185).
fn resolved(
    projections: &[&Projection<'_>],
    domains: &[ReportingDomain],
) -> Result<(), FinalizationInvariant> {
    let projections = || projections.iter().copied();
    let mut findings: BTreeSet<&IssueKey> = BTreeSet::new();
    let mut indexed: BTreeSet<&Evidence> = BTreeSet::new();
    for (_, retained) in projections().flat_map(|projection| &projection.records) {
        match &retained.record {
            Record::Finding(finding) => {
                findings.insert(finding.key);
            }
            Record::Evidence(evidence) => {
                indexed.insert(evidence);
            }
            _ => {}
        }
    }
    // A Finding past its exhausted domain's terminal cursor is omitted with
    // that domain's suffix (D15:2408).
    let omitted = |key: &IssueKey| {
        let domain = ReportingDomain::of(key.unit().phase);
        projections()
            .find(|projection| projection.domain == domain)
            .and_then(|projection| projection.terminal.as_ref())
            .is_some_and(|terminal| key.unit() >= terminal.exhausted.cursor)
    };
    let unresolved = |key: &&IssueKey| {
        domains.contains(&ReportingDomain::of(key.unit().phase))
            && !findings.contains(key)
            && !omitted(key)
    };
    for (owner, retained) in projections().flat_map(|projection| &projection.records) {
        if let Some(key) = named_findings(&retained.record).find(unresolved) {
            return Err(FinalizationInvariant::UnresolvedFinding(key.clone()));
        }
        if referred_evidence(&retained.record).any(|(_, evidence)| !indexed.contains(evidence)) {
            return Err(FinalizationInvariant::UnresolvedEvidence((*owner).clone()));
        }
    }
    Ok(())
}

/// The Finding Keys `record` names: an occurrence's Finding, a not-evaluated
/// status's causes, and a Shared-unavailability cause.
fn named_findings<'r>(record: &'r Record<'_>) -> Box<dyn Iterator<Item = &'r IssueKey> + 'r> {
    match record {
        Record::Occurrence(occurrence) => Box::new(std::iter::once(occurrence.finding())),
        Record::Status(Status::NotEvaluated(causes)) => Box::new(causes.keys()),
        Record::UnavailabilityCause(key) => Box::new(std::iter::once(*key)),
        _ => Box::new(std::iter::empty()),
    }
}

/// The Evidence `record` refers to, each under the unit that refers to it:
/// an occurrence's primary and related Evidence and a Coverage Decision
/// Evidence's (D15:1972).
fn referred_evidence<'r>(
    record: &'r Record<'_>,
) -> Box<dyn Iterator<Item = (&'r Unit, &'r Evidence)> + 'r> {
    match record {
        Record::Occurrence(occurrence) => Box::new(occurrence.references()),
        Record::DecisionEvidence(decision) => Box::new(std::iter::once(decision.reference())),
        _ => Box::new(std::iter::empty()),
    }
}

/// Whether the Coverage Decision Evidence covers each Coverage Decision
/// Basis exactly, in Basis cell order and then matched-domain order
/// (D15:1611).
fn cover(evidence: &ResolutionEvidence<'_>) -> Result<(), FinalizationInvariant> {
    let bases = evidence
        .shared
        .iter()
        .flat_map(|shared| &shared.records)
        .filter_map(|(_, retained)| match &retained.record {
            Record::DecisionBasis(cell, basis) => Some((*cell, *basis)),
            _ => None,
        });
    let expected: Vec<_> = bases
        .flat_map(|(cell, basis)| match &basis.decision {
            Decision::Default => vec![(cell, None)],
            Decision::Rules(domains) => domains.iter().map(|domain| (cell, Some(domain))).collect(),
        })
        .collect();
    let mapped: Vec<_> = evidence
        .entry
        .records
        .iter()
        .filter_map(|(_, retained)| match &retained.record {
            Record::DecisionEvidence(decision) => Some((decision.cell(), decision.source())),
            _ => None,
        })
        .collect();
    if mapped == expected {
        Ok(())
    } else {
        Err(FinalizationInvariant::Cover)
    }
}

/// Whether the mandatory reserve of `outcome` fits `limits`
/// (D15:1847, 2014, 2518).
///
/// # Errors
///
/// Returns the resolver bug of a reserve past `maxCanonicalBytes`.
pub(crate) fn reserve(
    outcome: &Outcome<'_, '_>,
    limits: &MandatoryReserveLimits,
) -> Result<(), FinalizationInvariant> {
    let bytes = reserved_bytes(outcome);
    let limit = limits.max_canonical_bytes.get();
    if bytes > limit {
        return Err(FinalizationInvariant::Reserve { bytes, limit });
    }
    Ok(())
}

/// The mandatory reserve of `outcome` in `maxCanonicalBytes`' unit: the
/// lengths of `C` of the outcome envelope, of each exhausted domain's
/// terminal Finding and `TerminalState`, and of the Entry reserve's
/// Shared-unavailability cause, together (D15:1847, 2014, 2518).
pub(crate) fn reserved_bytes(outcome: &Outcome<'_, '_>) -> u64 {
    let evidence = &outcome.evidence;
    let envelope = json!({
        "outcome": if outcome.checked.is_some() { "checked" } else { "blocked" },
        "constructionIdentity": outcome.identity.text(),
        "complete": evidence.complete(),
    });
    let reserved = evidence.terminal_states().flat_map(|terminal| {
        [&terminal.finding, &terminal.state]
            .into_iter()
            .chain(&terminal.cause)
    });
    std::iter::once(&envelope)
        .chain(reserved)
        .map(bytes)
        .fold(0, u64::saturating_add)
}

/// The length of `C` of `view`, a reserved view without numbers.
fn bytes(view: &Value) -> u64 {
    encoded_length(view).expect("a reserved view is a shallow number-free value")
}

#[cfg(test)]
mod tests {
    use intlify_shared_json::encoding::encoded_length;
    use serde_json::json;

    use crate::construction::registry::check::{possible_record_kinds, RecordKind};
    use crate::input_limits::Bound;
    use crate::resolver::capability::{BootstrapLimits, MandatoryReserveLimits};
    use crate::resolver::coverage::{self, CoverageSubject};
    use crate::resolver::evaluation::{Blocking, Check, Code, Evaluation, Notice, Phase};
    use crate::resolver::locales::fixtures::canonical;
    use crate::resolver::outcome::tests::{blocked, entry, identity, no_json, reporting};
    use crate::resolver::outcome::{outcome, replayed, ResolutionEvidence};
    use crate::resolver::profile_resolution::tests::Inputs;
    use crate::resolver::profile_resolution::{Invocation, ResolverInvariant};
    use crate::resolver::reporting::{Record, ReportingDomain};
    use crate::resolver::resolution_evidence::Projection;
    use crate::resolver::resource_policy::{self, PolicySubject};
    use crate::resolver::token::Sha256;

    use std::collections::{BTreeMap, BTreeSet};

    use crate::resolver::evaluation::Unit;
    use crate::resolver::evidence::Evidence;

    use super::{
        cover, observed, ordinals, owners, references, referred_evidence, reserve, resolved,
        verify_shared, FinalizationInvariant as F,
    };

    /// The position of the first record of `kind` in `projection`.
    fn first(projection: &Projection<'_>, kind: RecordKind) -> usize {
        projection
            .records
            .iter()
            .position(|(_, retained)| retained.kind == kind)
            .unwrap()
    }

    /// The positions of every record of `kind` in `projection`.
    fn each(projection: &Projection<'_>, kind: RecordKind) -> Vec<usize> {
        projection
            .records
            .iter()
            .enumerate()
            .filter(|(_, (_, retained))| retained.kind == kind)
            .map(|(index, _)| index)
            .collect()
    }

    /// The mandatory reserve's minimum vector, but `maxCanonicalBytes` at
    /// `bytes`.
    fn reserving(bytes: u64) -> MandatoryReserveLimits {
        MandatoryReserveLimits {
            max_canonical_bytes: Bound::new(bytes).unwrap(),
            ..BootstrapLimits::MINIMUM.mandatory_reserve
        }
    }

    /// A record of the other domain's kind, or owned by a unit whose Check
    /// Registry row does not list its kind, is a resolver bug (D15:2143,
    /// 2518, 3224).
    #[test]
    fn a_record_outside_its_domain_or_its_owners_row_is_a_resolver_bug() {
        blocked().outcome(None, identity(), |_, _, outcome| {
            let evidence = &outcome.evidence;
            let status =
                &evidence.entry.records[first(&evidence.entry, RecordKind::EntryEvaluationStatus)];
            let phase_0 = status.0;
            assert_eq!(phase_0.phase, Phase::ResolverBootstrapAdmission);
            let mut shared = evidence.shared.clone().unwrap();
            assert_eq!(owners(&shared), Ok(()));
            shared.records.push(status.clone());
            assert_eq!(
                owners(&shared),
                Err(F::Owner {
                    owner: phase_0.clone(),
                    kind: RecordKind::EntryEvaluationStatus
                })
            );
            let mut shared = evidence.shared.clone().unwrap();
            let index = first(&shared, RecordKind::SharedEvaluationStatus);
            shared.records[index].0 = phase_0;
            assert_eq!(
                owners(&shared),
                Err(F::Owner {
                    owner: phase_0.clone(),
                    kind: RecordKind::SharedEvaluationStatus
                })
            );
        });
    }

    /// A Finding, an occurrence, or a Shared-unavailability cause owned by
    /// another unit than the Finding's is a resolver bug, though that unit's
    /// row lists the record's kind (D15:2205).
    #[test]
    fn a_record_owned_by_another_unit_than_its_findings_is_a_resolver_bug() {
        blocked().outcome(None, identity(), |_, _, outcome| {
            let evidence = &outcome.evidence;
            for (projection, kind) in [
                (evidence.shared.as_ref().unwrap(), RecordKind::SharedFinding),
                (&evidence.entry, RecordKind::FindingOccurrence),
            ] {
                let found = each(projection, kind);
                assert_eq!(found.len(), 2);
                let mut swapped = projection.clone();
                let other = swapped.records[found[1]].0;
                assert_ne!(swapped.records[found[0]].0, other);
                swapped.records[found[0]].0 = other;
                assert_eq!(
                    owners(&swapped),
                    Err(F::Owner {
                        owner: other.clone(),
                        kind
                    })
                );
            }
        });
        reporting(&Inputs::complete(), &[], &[]).outcome(
            Some(no_json()),
            identity(),
            |evaluation, _, outcome| {
                let mut entry = outcome.evidence.entry.clone();
                let index = first(&entry, RecordKind::SharedUnavailabilityCause);
                let owner = entry.records[index].0;
                // Another unit of entry admission that reports codes.
                let other = evaluation
                    .statuses()
                    .map(|(unit, _)| unit)
                    .find(|unit| {
                        *unit != owner
                            && possible_record_kinds(unit.subject.kind(), unit.check)
                                .contains(&RecordKind::SharedUnavailabilityCause)
                    })
                    .unwrap();
                entry.records[index].0 = other;
                assert_eq!(
                    owners(&entry),
                    Err(F::Owner {
                        owner: other.clone(),
                        kind: RecordKind::SharedUnavailabilityCause
                    })
                );
            },
        );
    }

    /// An evidence-index record owned by another unit than the first that
    /// refers to its Evidence is a resolver bug (D15:1972, 2205).
    #[test]
    fn an_index_record_owned_by_another_unit_than_the_first_referrer_is_a_resolver_bug() {
        blocked().outcome(None, identity(), |_, _, outcome| {
            let mut entry = outcome.evidence.entry.clone();
            let index = first(&entry, RecordKind::EvidenceIndex);
            let owner = entry.records[index].0;
            let later = entry
                .records
                .iter()
                .map(|(unit, _)| *unit)
                .filter(|unit| *unit > owner)
                .find(|unit| {
                    possible_record_kinds(unit.subject.kind(), unit.check)
                        .contains(&RecordKind::EvidenceIndex)
                })
                .unwrap();
            entry.records[index].0 = later;
            assert_eq!(owners(&entry), Err(F::IndexOwner(later.clone())));
        });
    }

    /// A Finding candidate, blocking or not, that carried nothing it observed
    /// is a resolver bug (D15:2385).
    #[test]
    fn a_candidate_observed_through_nothing_is_a_resolver_bug() {
        let unit = resource_policy::unit(PolicySubject::ResourceLimits, Check::CompatiblePolicy);
        let made_up = Blocking {
            code: Code::JsonInvalid,
            reason: None,
            details: None,
        };
        let mut blocking = Evaluation::fabricated();
        assert!(blocking
            .check(unit.clone(), Ok(()), |()| Err::<(), _>(made_up))
            .unwrap()
            .is_err());
        let mut notice = Evaluation::fabricated();
        notice
            .check(unit.clone(), Ok(()), |()| Ok::<(), Blocking>(()))
            .unwrap()
            .unwrap();
        assert_eq!(observed(&notice), Ok(()));
        notice
            .notice(unit, Notice::made_up(Code::LocaleNonCanonical), None)
            .unwrap();
        for (evaluation, key) in [
            (&blocking, blocking.issues().next()),
            (&notice, notice.notices().next()),
        ] {
            assert_eq!(
                observed(evaluation),
                Err(F::Unobserved(key.unwrap().clone()))
            );
        }
    }

    /// Within the Shared domain alone, a record of the Entry domain, a
    /// Shared Finding Key without its Finding, and a candidate observed
    /// through nothing are resolver bugs, while the Phase 0 Finding a Shared
    /// status names is the complete bundle's to resolve (D15:1185, 2659).
    #[test]
    fn a_replay_verifies_the_shared_domain_alone() {
        blocked().outcome(None, identity(), |evaluation, _, outcome| {
            let evidence = &outcome.evidence;
            let shared = evidence.shared.as_ref().unwrap();
            assert_eq!(verify_shared(evaluation, shared), Ok(()));
            let status =
                &evidence.entry.records[first(&evidence.entry, RecordKind::EntryEvaluationStatus)];
            let mut misplaced = shared.clone();
            misplaced.records.push(status.clone());
            assert!(matches!(
                verify_shared(evaluation, &misplaced),
                Err(F::Owner { .. })
            ));
            let mut without = shared.clone();
            let (_, removed) = without
                .records
                .remove(first(&without, RecordKind::SharedFinding));
            let Record::Finding(finding) = removed.record else {
                panic!("a Finding record holds a Finding");
            };
            assert_eq!(
                verify_shared(evaluation, &without),
                Err(F::UnresolvedFinding(finding.key.clone()))
            );
            let unit =
                resource_policy::unit(PolicySubject::ResourceLimits, Check::CompatiblePolicy);
            let mut notice = Evaluation::fabricated();
            notice
                .check(unit.clone(), Ok(()), |()| Ok::<(), Blocking>(()))
                .unwrap()
                .unwrap();
            notice
                .notice(unit, Notice::made_up(Code::LocaleNonCanonical), None)
                .unwrap();
            assert!(matches!(
                verify_shared(&notice, shared),
                Err(F::Unobserved(_))
            ));
        });
        let mut unasserted = Inputs::complete();
        unasserted.unasserted();
        reporting(&unasserted, &[], &[]).outcome(None, identity(), |evaluation, _, outcome| {
            let shared = outcome.evidence.shared.as_ref().unwrap();
            assert_eq!(verify_shared(evaluation, shared), Ok(()));
            let both = [ReportingDomain::Entry, ReportingDomain::Shared];
            assert!(matches!(
                resolved(&[shared], &both),
                Err(F::UnresolvedFinding(key)) if key.unit().phase == Phase::ResolverBootstrapAdmission
            ));
        });
    }

    /// Occurrences of one Finding and primary Evidence whose ordinals are not
    /// `0..n-1` are a resolver bug (D15:2385).
    #[test]
    fn occurrences_whose_ordinals_do_not_count_from_zero_are_a_resolver_bug() {
        blocked().outcome(None, identity(), |_, _, outcome| {
            let mut entry = outcome.evidence.entry.clone();
            assert_eq!(ordinals(&entry), Ok(()));
            let index = first(&entry, RecordKind::FindingOccurrence);
            let Record::Occurrence(occurrence) = &entry.records[index].1.record else {
                panic!("an occurrence record holds an occurrence");
            };
            let finding = occurrence.finding().clone();
            let renumbered = occurrence.renumbered(occurrence.ordinal() + 1);
            entry.records[index].1.record = Record::Occurrence(renumbered);
            assert_eq!(ordinals(&entry), Err(F::Ordinals(finding)));
        });
    }

    /// In a complete bundle, a Finding Key that names no retained Finding, or
    /// Evidence without its evidence-index record, is a resolver bug: an
    /// occurrence's or a not-evaluated status's Finding, and a
    /// Shared-unavailability cause's (D15:1182, 1185).
    #[test]
    fn a_reference_a_complete_bundle_does_not_resolve_is_a_resolver_bug() {
        blocked().outcome(None, identity(), |_, _, outcome| {
            let evidence: &ResolutionEvidence<'_> = &outcome.evidence;
            assert_eq!(references(evidence), Ok(()));
            let mut without = evidence.clone();
            let shared = without.shared.as_mut().unwrap();
            let (_, removed) = shared
                .records
                .remove(first(shared, RecordKind::SharedFinding));
            let Record::Finding(finding) = removed.record else {
                panic!("a Finding record holds a Finding");
            };
            assert_eq!(
                references(&without),
                Err(F::UnresolvedFinding(finding.key.clone()))
            );
            // A status alone naming the Finding.
            without
                .entry
                .records
                .retain(|(_, retained)| retained.kind != RecordKind::FindingOccurrence);
            assert_eq!(
                references(&without),
                Err(F::UnresolvedFinding(finding.key.clone()))
            );
            let mut without = evidence.clone();
            without
                .entry
                .records
                .remove(first(&without.entry, RecordKind::EvidenceIndex));
            assert!(matches!(
                references(&without),
                Err(F::UnresolvedEvidence(_))
            ));
        });
        // Only its occurrence names a warning, which nothing waits on.
        let mut warned = reporting(&Inputs::complete(), &[], &[]);
        warned.declaration()["requestedLocales"] = json!(["ja", "EN"]);
        warned.outcome(None, identity(), |_, _, outcome| {
            let mut without = outcome.evidence.clone();
            let shared = without.shared.as_mut().unwrap();
            let (_, removed) = shared
                .records
                .remove(first(shared, RecordKind::SharedFinding));
            let Record::Finding(finding) = removed.record else {
                panic!("a Finding record holds a Finding");
            };
            assert_eq!(
                references(&without),
                Err(F::UnresolvedFinding(finding.key.clone()))
            );
        });
        // A cause alone naming its Finding.
        reporting(&Inputs::complete(), &[], &[]).outcome(
            Some(no_json()),
            identity(),
            |_, _, outcome| {
                let mut causes = outcome.evidence.clone();
                causes
                    .entry
                    .records
                    .retain(|(_, retained)| retained.kind == RecordKind::SharedUnavailabilityCause);
                let key = outcome.evidence.unavailable()[0].clone();
                assert_eq!(references(&causes), Err(F::UnresolvedFinding(key)));
            },
        );
    }

    /// An evidence-index record two units' occurrences refer to is owned by
    /// the earlier one, and owned by the later one is a resolver bug: each
    /// target requesting a locale outside the project relates the project's
    /// requested locales (D15:1972).
    #[test]
    fn an_index_record_two_units_refer_to_is_the_earlier_ones() {
        let mut inputs = reporting(&Inputs::complete(), &[], &[]);
        for target in ["browser", "server"] {
            inputs.declaration()["targetProfiles"][target]["requestedLocales"] =
                json!(["ja", "de"]);
        }
        inputs.outcome(None, identity(), |_, _, outcome| {
            let records = &outcome.evidence.entry.records;
            let mut referrers: BTreeMap<Evidence, BTreeSet<Unit>> = BTreeMap::new();
            for (_, retained) in records {
                for (unit, evidence) in referred_evidence(&retained.record) {
                    referrers
                        .entry(evidence.clone())
                        .or_default()
                        .insert(unit.clone());
                }
            }
            let (shared, units) = referrers
                .into_iter()
                .find(|(_, units)| units.len() > 1)
                .unwrap();
            let index = records
                .iter()
                .position(|(_, retained)| matches!(&retained.record, Record::Evidence(evidence) if *evidence == shared))
                .unwrap();
            assert_eq!(records[index].0, units.first().unwrap());
            let later = records
                .iter()
                .map(|(owner, _)| *owner)
                .find(|owner| *owner == units.last().unwrap())
                .unwrap();
            let mut entry = outcome.evidence.entry.clone();
            entry.records[index].0 = later;
            assert_eq!(owners(&entry), Err(F::IndexOwner(later.clone())));
        });
    }

    /// When both domains are complete, Coverage Decision Evidence that leaves
    /// a Basis without its record, or maps one twice, is a resolver bug; an
    /// incomplete bundle may hold the Basis without the Evidence (D15:1611).
    #[test]
    fn decision_evidence_covers_each_basis_exactly_once_both_domains_are_complete() {
        let inputs = reporting(&Inputs::complete(), &[], &[]);
        let table = coverage::unit(CoverageSubject::Table, Check::CompleteTable);
        let before = inputs.outcome(None, identity(), |_, _, outcome| {
            let evidence = &outcome.evidence;
            assert!(evidence.complete());
            assert_eq!(cover(evidence), Ok(()));
            let index = first(&evidence.entry, RecordKind::CoverageDecisionEvidence);
            let mut without = evidence.clone();
            let removed = without.entry.records.remove(index);
            assert_eq!(cover(&without), Err(F::Cover));
            let mut twice = evidence.clone();
            twice.entry.records.insert(index, removed);
            assert_eq!(cover(&twice), Err(F::Cover));
            evidence
                .entry
                .records
                .iter()
                .filter(|(owner, _)| **owner < table)
                .count()
        });
        // The Entry domain refuses the table's slice; the Shared one keeps
        // the Basis.
        let room = (entry("maxRecords"), crate::artifacts::count(before));
        reporting(&Inputs::complete(), &[room], &[room]).outcome(
            None,
            identity(),
            |_, _, outcome| {
                let evidence = &outcome.evidence;
                assert_eq!(
                    evidence.entry.terminal.as_ref().unwrap().exhausted.cursor,
                    &table
                );
                assert!(each(&evidence.entry, RecordKind::CoverageDecisionEvidence).is_empty());
                let shared = evidence.shared.as_ref().unwrap();
                assert!(!each(shared, RecordKind::CoverageDecisionBasis).is_empty());
                assert_eq!(cover(evidence), Err(F::Cover));
            },
        );
    }

    /// The mandatory reserve counts the outcome envelope, each exhausted
    /// domain's terminal Finding and `TerminalState`, and the Entry reserve's
    /// cause: an outcome fits a `maxCanonicalBytes` of exactly their total,
    /// and not one less (D15:1847, 2518).
    #[test]
    fn the_reserve_fits_exactly_the_length_of_its_views() {
        reporting(&Inputs::complete(), &[(entry("maxRecords"), 1)], &[]).outcome(
            Some(no_json()),
            identity(),
            |_, _, outcome| {
                let terminal = outcome.evidence.entry.terminal.as_ref().unwrap();
                let envelope = json!({
                    "outcome": "blocked",
                    "constructionIdentity": identity().text(),
                    "complete": false,
                });
                let total: u64 = [
                    &envelope,
                    &terminal.finding,
                    &terminal.state,
                    terminal.cause.as_ref().unwrap(),
                ]
                .into_iter()
                .map(|view| encoded_length(view).unwrap())
                .sum();
                assert_eq!(reserve(&outcome, &reserving(total)), Ok(()));
                assert_eq!(
                    reserve(&outcome, &reserving(total - 1)),
                    Err(F::Reserve {
                        bytes: total,
                        limit: total - 1
                    })
                );
            },
        );
    }

    /// An outcome runs reporting finalization's checks before it is
    /// returned: a candidate that observed nothing, or a reserve past the
    /// capability's, leaves the invocation without an outcome, and the
    /// candidate leaves a replay without a result (D15:2143, 2385, 2518,
    /// 2659).
    #[test]
    fn an_invocation_without_finalizations_checks_has_no_outcome() {
        reporting(&Inputs::complete(), &[], &[]).entered_under(
            None,
            |invocation, evaluation, invoked| {
                let capability = invocation
                    .capability
                    .clone()
                    .with_mandatory_reserve(reserving(1));
                let tight = Invocation {
                    capability: &capability,
                    ..*invocation
                };
                assert!(matches!(
                    outcome(&evaluation, &invoked, identity(), &tight, &Sha256),
                    Err(ResolverInvariant::Finalization(invariant))
                    if matches!(*invariant, F::Reserve { limit: 1, .. })
                ));
                // A notice the views accept, observed through nothing.
                let unit = evaluation.statuses().next().unwrap().0.clone();
                let mut evaluation = evaluation.fabricating();
                evaluation
                    .notice(unit, Notice::non_canonical(canonical("en")), None)
                    .unwrap();
                let key = evaluation.notices().next().unwrap().clone();
                let unobserved = ResolverInvariant::Finalization(Box::new(F::Unobserved(key)));
                assert_eq!(
                    outcome(&evaluation, &invoked, identity(), invocation, &Sha256),
                    Err(unobserved.clone())
                );
                let state = invoked.state.as_ref().unwrap();
                assert_eq!(
                    replayed(&evaluation, state, invocation, &Sha256).err(),
                    Some(unobserved)
                );
            },
        );
    }
}
