// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's Phase 1, `entry-admission`, over strict file entry. The
//! phase's first kind owns the entry dependency records and waits on nothing
//! (D15:2516). The complete input is one singleton subject whose ten checks
//! run in check-rank order, each waiting on the checks whose result it
//! needs: the parser's tokens on the file's bytes, strict JSON on the tokens
//! (D15:1794, 1795), duplicate members and the JSON-compatible domain on a
//! syntactically parsed file (D15:802, 2289), and the five root-value bounds
//! on both, since they bound the complete root that entry materializes
//! (D15:1876). A check whose prerequisite failed is not evaluated, with the
//! causes of the failure (D15:2149), and a value exists only when every check
//! admits it; otherwise there is no Shared projection, and the causes are
//! why (D15:2369).
//!
//! Each check reports what strict file entry found: the file's bytes exactly,
//! the parser's tokens as their first-over witness, one Finding for a syntax or
//! UTF-8 failure, one for the duplicate members, one for each reason a value
//! is outside the JSON-compatible domain, and each root-value bound the
//! complete value exceeds, the single-string bound as its first-over witness
//! (D15:1884, 1999-2006, 2203, 2287-2289, 2356). What the Findings were
//! observed through comes with their occurrences.

use std::sync::Arc;

use crate::input_limits::{CountRelation, InputBound, InputLimits, LimitViolation};
use crate::materialize::{
    materialize_file, Inadmissible, Incompatibility, InputFailure, MaterializedDocument,
};

use super::capability::{BootstrapBoundId, BootstrapGroup};
use super::evaluation::{
    and, Actual, Admitted, Blocking, BootstrapLimitExceeded, Check, Code, Evaluation,
    EvaluationInvariant, Found, LimitExceeded, Phase, Reason, Subject, Unit,
};
use super::limits::{BoundId, Group};

/// The unit that owns the entry dependency records.
pub(crate) const OWNER: Unit = Unit {
    phase: Phase::EntryAdmission,
    subject: Subject::EntryDependencies,
    check: Check::EstablishedSlots,
};

/// The unit of the complete input's `check`.
pub(crate) const fn input(check: Check) -> Unit {
    Unit {
        phase: Phase::EntryAdmission,
        subject: Subject::CompleteInput,
        check,
    }
}

/// The root-value bounds' checks, in check-rank order, with the bound each
/// applies.
const ROOT_VALUE_BOUNDS: [(Check, InputBound, &str); 5] = [
    (Check::WithinNodes, InputBound::Nodes, "maxNodes"),
    (Check::WithinDepth, InputBound::Depth, "maxDepth"),
    (
        Check::WithinCollectionEntries,
        InputBound::CollectionEntries,
        "maxCollectionEntries",
    ),
    (
        Check::WithinTotalStringBytes,
        InputBound::TotalStringBytes,
        "maxTotalStringBytes",
    ),
    (
        Check::WithinSingleStringBytes,
        InputBound::SingleStringBytes,
        "maxSingleStringBytes",
    ),
];

/// A resolver bug Phase 1 found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EntryInvariant {
    Evaluation(EvaluationInvariant),
    /// Strict file entry counted past `u64`, which the file's length bounds.
    AccountingOverflow,
    /// Strict file entry found a failure that no check reported.
    UnreportedFailure,
}

impl From<EvaluationInvariant> for EntryInvariant {
    fn from(invariant: EvaluationInvariant) -> Self {
        Self::Evaluation(invariant)
    }
}

/// Phase 1 over the file `source`, under the bounds `limits` the Admitted
/// Implementation Capability gives `bootstrapOnly.rawInput` and
/// `policyComparable.configurationInput`: the materialized document, or the
/// causes of its absence.
///
/// # Errors
///
/// Returns the resolver bug Phase 1 found.
pub(crate) fn admit_entry(
    evaluation: &mut Evaluation,
    source: Arc<[u8]>,
    limits: InputLimits,
) -> Result<Admitted<MaterializedDocument>, EntryInvariant> {
    // The owner records the entry dependencies, and nothing waits on it.
    let _: Admitted<()> = evaluation.check(OWNER, Ok(()), |()| Ok::<(), Blocking>(()))?;
    let entry = materialize_file(source, limits);
    let failure = entry.as_ref().err().map(|error| &error.reason);
    if matches!(failure, Some(InputFailure::AccountingOverflow { .. })) {
        return Err(EntryInvariant::AccountingOverflow);
    }
    let file_bytes =
        evaluation.check(input(Check::WithinFileBytes), Ok(()), |()| match failure {
            Some(InputFailure::FileBytes(violation)) => Err(raw_bound("maxFileBytes", violation)),
            _ => Ok(()),
        })?;
    let tokens = evaluation.check(
        input(Check::WithinParserTokens),
        file_bytes,
        |()| match failure {
            Some(InputFailure::ParserTokens { violation, .. }) => {
                Err(raw_bound("maxParserTokens", violation))
            }
            _ => Ok(()),
        },
    )?;
    let syntax = evaluation.check(input(Check::WellFormedJson), tokens, |()| match failure {
        Some(InputFailure::InvalidUtf8 { .. } | InputFailure::Syntax { .. }) => {
            Err(found(Code::JsonInvalid, None))
        }
        _ => Ok(()),
    })?;
    let parsed: Option<&Inadmissible> = match failure {
        Some(InputFailure::Inadmissible(facts)) => Some(facts),
        _ => None,
    };
    let unique = evaluation.check(
        input(Check::UniqueMembers),
        syntax.clone(),
        |()| match parsed {
            Some(facts) if !facts.duplicates.is_empty() => {
                Err(found(Code::JsonMemberDuplicate, None))
            }
            _ => Ok(()),
        },
    )?;
    let compatible = evaluation.check_found(input(Check::JsonCompatible), syntax, |()| {
        parsed.and_then(incompatible).map_or(Ok(()), Err)
    })?;
    let materializable = and(unique, compatible);
    let mut bounded = Ok(());
    for (check, bound, name) in ROOT_VALUE_BOUNDS {
        let violation = parsed.and_then(|facts| {
            facts
                .violations
                .iter()
                .find(|violation| violation.bound == bound)
        });
        let within = evaluation.check(input(check), materializable.clone(), |()| {
            violation.map_or(Ok(()), |violation| {
                Err(Blocking::limit(LimitExceeded {
                    bound: BoundId {
                        group: Group::ConfigurationInput,
                        name,
                    },
                    limit: violation.limit,
                    actual: actual(violation),
                }))
            })
        })?;
        bounded = and(bounded, within);
    }
    match (bounded, entry) {
        (Err(causes), _) => Ok(Err(causes)),
        (Ok(()), Ok(document)) => Ok(Ok(document)),
        (Ok(()), Err(_)) => Err(EntryInvariant::UnreportedFailure),
    }
}

/// The blocking result `code` with `reason`, which has no details.
const fn found(code: Code, reason: Option<Reason>) -> Blocking {
    Blocking {
        code,
        reason,
        details: None,
    }
}

/// The Findings of the values outside the JSON-compatible domain: one for
/// each reason, in 015's reason order, or none (D15:2289).
fn incompatible(facts: &Inadmissible) -> Option<Found> {
    [
        (
            Incompatibility::NonPortableNumber,
            Reason::NonPortableNumber,
        ),
        (Incompatibility::InvalidUnicode, Reason::InvalidUnicode),
    ]
    .into_iter()
    .filter(|(kind, _)| facts.incompatible.iter().any(|value| value.reason == *kind))
    .map(|(_, reason)| Found::from(found(Code::InputNotJsonCompatible, Some(reason))))
    .reduce(Found::and)
}

/// A `bootstrapOnly.rawInput` bound's Finding (D15:2356).
fn raw_bound(name: &'static str, violation: &LimitViolation) -> Blocking {
    Blocking::bootstrap_limit(BootstrapLimitExceeded {
        bound: BootstrapBoundId {
            group: BootstrapGroup::RawInput,
            name,
        },
        limit: violation.limit,
        actual: actual(violation),
    })
}

fn actual(violation: &LimitViolation) -> Actual {
    match violation.relation {
        CountRelation::Exact => Actual::Exact(violation.actual),
        CountRelation::AtLeast => Actual::AtLeast(violation.actual),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use crate::input_limits::{Bound, InputBound, InputLimits};
    use crate::materialize_tests::limits;
    use crate::resolver::capability::{BootstrapBoundId, BootstrapGroup};
    use crate::resolver::evaluation::{
        Actual, BootstrapLimitExceeded, Check, Code, Evaluation, IssueKey, LimitExceeded, Reason,
        Status, Unit,
    };
    use crate::resolver::limits::Group;

    use super::{admit_entry, input, OWNER, ROOT_VALUE_BOUNDS};

    /// Phase 1 over `source` under `limits`: the evaluation, and whether a
    /// document was admitted.
    fn entered(source: &[u8], limits: InputLimits) -> (Evaluation, bool) {
        let mut evaluation = Evaluation::default();
        let admitted = admit_entry(&mut evaluation, Arc::from(source), limits)
            .unwrap()
            .is_ok();
        (evaluation, admitted)
    }

    /// Every Finding, as its check, code, and reason.
    fn found(evaluation: &Evaluation) -> Vec<(Check, Code, Option<Reason>)> {
        evaluation
            .findings()
            .iter()
            .map(|finding| {
                (
                    finding.key.unit().check,
                    finding.key.code(),
                    finding.key.reason(),
                )
            })
            .collect()
    }

    /// Each check's status, with the checks of its causes when it was not
    /// evaluated.
    fn statuses(evaluation: &Evaluation) -> BTreeMap<Check, Option<Vec<Check>>> {
        evaluation
            .statuses()
            .map(|(unit, status)| {
                let causes = match status {
                    Status::Evaluated => None,
                    Status::NotEvaluated(causes) => {
                        Some(causes.keys().map(|key| key.unit().check).collect())
                    }
                };
                (unit.check, causes)
            })
            .collect()
    }

    /// The checks after `failed` in check-rank order, each with what it then
    /// waits on.
    fn waiting(evaluation: &Evaluation) -> Vec<(Check, Vec<Check>)> {
        statuses(evaluation)
            .into_iter()
            .filter_map(|(check, causes)| causes.map(|causes| (check, causes)))
            .collect()
    }

    #[test]
    fn a_strict_file_passes_every_check_and_admits_its_document() {
        let (evaluation, admitted) = entered(br#"{"a":[1,"b"]}"#, limits());
        assert!(admitted);
        assert_eq!(found(&evaluation), []);
        let units: Vec<Unit> = evaluation
            .statuses()
            .map(|(unit, _)| unit.clone())
            .collect();
        assert_eq!(units.len(), 11);
        assert_eq!(units[0], OWNER);
        assert!(units[1..]
            .iter()
            .zip(crate::resolver::subject::SubjectKind::CompleteInput.checks())
            .all(|(unit, &check)| *unit == input(check)));
        assert!(statuses(&evaluation).values().all(Option::is_none));
    }

    /// A failure of each stage leaves the checks that need its result not
    /// evaluated, through the chain of prerequisites, and no document; the
    /// owner of the entry dependencies waits on nothing.
    #[test]
    fn each_stage_holds_back_the_checks_that_need_its_result() {
        let mut small = limits();
        small.raw.max_file_bytes = Bound::new(4).unwrap();
        let (file, admitted) = entered(b"[1, 2]", small);
        assert!(!admitted);
        assert_eq!(
            found(&file),
            [(Check::WithinFileBytes, Code::BootstrapLimitExceeded, None)]
        );
        assert_eq!(statuses(&file)[&Check::EstablishedSlots], None);
        assert_eq!(
            waiting(&file),
            [
                Check::WithinParserTokens,
                Check::WellFormedJson,
                Check::UniqueMembers,
                Check::JsonCompatible,
                Check::WithinNodes,
                Check::WithinDepth,
                Check::WithinCollectionEntries,
                Check::WithinTotalStringBytes,
                Check::WithinSingleStringBytes,
            ]
            .map(|check| (check, vec![Check::WithinFileBytes]))
        );
        let mut few = limits();
        few.raw.max_parser_tokens = Bound::new(2).unwrap();
        let (tokens, _) = entered(b"[1, 2]", few);
        assert_eq!(
            found(&tokens),
            [(
                Check::WithinParserTokens,
                Code::BootstrapLimitExceeded,
                None
            )]
        );
        assert_eq!(waiting(&tokens).len(), 8);
        let (syntax, admitted) = entered(br#"{"x":1,"x":2,]"#, limits());
        assert!(!admitted);
        assert_eq!(
            found(&syntax),
            [(Check::WellFormedJson, Code::JsonInvalid, None)]
        );
        // A duplicate before the syntax failure is not reported.
        assert_eq!(
            waiting(&syntax)
                .iter()
                .map(|(check, causes)| (*check, causes.clone()))
                .collect::<Vec<_>>()[..2],
            [
                (Check::UniqueMembers, vec![Check::WellFormedJson]),
                (Check::JsonCompatible, vec![Check::WellFormedJson])
            ]
        );
        let (utf8, _) = entered(b"[\"\xff\"]", limits());
        assert_eq!(
            found(&utf8),
            [(Check::WellFormedJson, Code::JsonInvalid, None)]
        );
    }

    /// A document exists only when every check admits it: a check a test
    /// makes block holds the document back as a failure that strict file
    /// entry found would (D15:2369).
    #[test]
    fn a_document_needs_every_check_to_admit_it() {
        for &check in crate::resolver::subject::SubjectKind::CompleteInput.checks() {
            let mut evaluation = Evaluation::blocking(input(check));
            let admitted =
                admit_entry(&mut evaluation, Arc::from(&br#"{"a":1}"#[..]), limits()).unwrap();
            assert!(admitted.is_err(), "{check:?}");
        }
    }

    /// Duplicate members and values outside the JSON-compatible domain are
    /// both reported from one parsed file, each reason a Finding of its own,
    /// and the root-value bounds wait on both (D15:802, 2289).
    #[test]
    fn duplicates_and_incompatible_values_are_reported_together_and_hold_back_the_bounds() {
        let (evaluation, admitted) =
            entered(br#"{"x":1,"x":1e999,"y":"\ud800","z":1e999}"#, limits());
        assert!(!admitted);
        assert_eq!(
            found(&evaluation),
            [
                (Check::UniqueMembers, Code::JsonMemberDuplicate, None),
                (
                    Check::JsonCompatible,
                    Code::InputNotJsonCompatible,
                    Some(Reason::InvalidUnicode)
                ),
                (
                    Check::JsonCompatible,
                    Code::InputNotJsonCompatible,
                    Some(Reason::NonPortableNumber)
                ),
            ]
        );
        let held: Vec<(Check, Vec<Check>)> = ROOT_VALUE_BOUNDS
            .iter()
            .map(|&(check, _, _)| {
                (
                    check,
                    vec![
                        Check::UniqueMembers,
                        Check::JsonCompatible,
                        Check::JsonCompatible,
                    ],
                )
            })
            .collect();
        assert_eq!(waiting(&evaluation), held);
        // Either alone holds the bounds back too.
        let (duplicate, _) = entered(br#"{"x":1,"x":2}"#, limits());
        assert_eq!(waiting(&duplicate).len(), 5);
        let (incompatible, _) = entered(b"[1e999]", limits());
        assert_eq!(
            found(&incompatible),
            [(
                Check::JsonCompatible,
                Code::InputNotJsonCompatible,
                Some(Reason::NonPortableNumber)
            )]
        );
        assert_eq!(waiting(&incompatible).len(), 5);
    }

    /// The raw bounds report the file's bytes exactly and the parser's tokens
    /// as their first-over witness, each under its `bootstrapOnly.rawInput`
    /// bound (D15:1999-2006, 2356).
    #[test]
    fn the_raw_bounds_report_their_bootstrap_details() {
        let key =
            |evaluation: &Evaluation| -> IssueKey { evaluation.issues().next().unwrap().clone() };
        let mut small = limits();
        small.raw.max_file_bytes = Bound::new(4).unwrap();
        let (file, _) = entered(b"[1, 2]", small);
        assert_eq!(
            file.bootstrap_limit(&key(&file)),
            Some(BootstrapLimitExceeded {
                bound: BootstrapBoundId {
                    group: BootstrapGroup::RawInput,
                    name: "maxFileBytes",
                },
                limit: Bound::new(4).unwrap(),
                actual: Actual::Exact(6),
            })
        );
        let mut few = limits();
        few.raw.max_parser_tokens = Bound::new(2).unwrap();
        let (tokens, _) = entered(b"[1, 2]", few);
        assert_eq!(
            tokens.bootstrap_limit(&key(&tokens)),
            Some(BootstrapLimitExceeded {
                bound: BootstrapBoundId {
                    group: BootstrapGroup::RawInput,
                    name: "maxParserTokens",
                },
                limit: Bound::new(2).unwrap(),
                actual: Actual::AtLeast(3),
            })
        );
    }

    /// The root-value bounds report every total the complete value exceeds
    /// exactly, and the single-string bound its first-over witness, each under
    /// its `policyComparable.configurationInput` bound (D15:1876, 1884, 2203).
    #[test]
    fn the_root_value_bounds_report_exact_totals_and_the_single_string_witness() {
        let mut over = limits();
        over.value.max_nodes = Bound::new(3).unwrap();
        over.value.max_depth = Bound::new(2).unwrap();
        over.value.max_collection_entries = Bound::new(2).unwrap();
        over.value.max_total_string_bytes = Bound::new(5).unwrap();
        over.value.max_single_string_bytes = Bound::new(2).unwrap();
        let (evaluation, admitted) = entered(br#"{"abc":["def",true]}"#, over);
        assert!(!admitted);
        let limits: Vec<(&str, u64, Actual)> = evaluation
            .issues()
            .map(|key| {
                let LimitExceeded {
                    bound,
                    limit,
                    actual,
                } = evaluation.limit(key).unwrap();
                assert_eq!(bound.group, Group::ConfigurationInput);
                (bound.name, limit.get(), actual)
            })
            .collect();
        assert_eq!(
            limits,
            [
                ("maxNodes", 3, Actual::Exact(4)),
                ("maxDepth", 2, Actual::Exact(3)),
                ("maxCollectionEntries", 2, Actual::Exact(3)),
                ("maxTotalStringBytes", 5, Actual::Exact(6)),
                ("maxSingleStringBytes", 2, Actual::AtLeast(3)),
            ]
        );
    }

    /// Any one root-value bound the complete value exceeds holds the
    /// document back on its own.
    #[test]
    fn each_root_value_bound_alone_holds_the_document_back() {
        for (check, bound, name) in ROOT_VALUE_BOUNDS {
            let mut tight = limits();
            let value = &mut tight.value;
            let limit = match bound {
                InputBound::Nodes => &mut value.max_nodes,
                InputBound::Depth => &mut value.max_depth,
                InputBound::CollectionEntries => &mut value.max_collection_entries,
                InputBound::TotalStringBytes => &mut value.max_total_string_bytes,
                InputBound::SingleStringBytes => &mut value.max_single_string_bytes,
                InputBound::FileBytes | InputBound::ParserTokens => unreachable!("{name}"),
            };
            *limit = Bound::new(1).unwrap();
            let (evaluation, admitted) = entered(br#"{"abc":["def",true]}"#, tight);
            assert!(!admitted, "{name}");
            let checks: Vec<Check> = evaluation.issues().map(|key| key.unit().check).collect();
            assert_eq!(checks, [check], "{name}");
        }
    }
}
