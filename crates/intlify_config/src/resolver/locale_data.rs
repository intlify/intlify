// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Phase 5's reading of the Locale Canonicalization Data Artifact past its
//! boundary, as the checks of its singleton subject (D15:1991). Each of the
//! nine `bootstrapOnly.localeData` bounds is a check of its own, reporting
//! `project-profile-bootstrap-limit-exceeded` with the bound's capability path
//! (D15:2357, 3481; Decision 015-160).
//!
//! The checks run in the bounds' order, except that a bound measured from a
//! decoded payload follows the read that decodes it (D15:2009): the two bounds
//! the manifest decides, the manifest's read, the markers' read and their
//! bound, the conformance payload's read and its six bounds, what is left of
//! the reader's steps (the closed conformance structure, the marker payloads,
//! and the semantic digest), and the match with the admitted specification
//! (D17:1069 to 1072, 1074). One payload's failure leaves the other's bounds to
//! report (D15:2366). Past `maxPayloadBytes`
//! neither payload is read: the bound is one check of a singleton, and what a
//! check waits on does not vary with the value it finds (D15:2150).
//!
//! Only the match waits on the `locale-canonicalization` slot. Reading the
//! data, measuring it, and proving that its dataset holds need the confirmed
//! policy and no value of the slot, so each of those failures stays provable
//! beside an invalid slot (D15:2134, 2366).

use crate::input_limits::Bound;
use crate::locale::{
    Canonicalizer, DataCount, DataCounts, DataFacts, DataNotAdmitted, FormalData, LocaleDataLimits,
    Provider, SemanticPins,
};

use super::capability::{BootstrapBoundId, BootstrapGroup, CapabilityBound};
use super::evaluation::{
    both, Actual, Admitted, Blocking, BootstrapLimitExceeded, Causes, Check, Code, Evaluation,
    EvaluationInvariant, Found, Observation, Phase, Subject, Unit,
};
use super::evidence::EvidenceProjection;
use super::evidence_component::ComponentPath;
use super::limits::ConfirmedPolicy;
use super::specification::{Slot, SpecificationInvariant};

/// The unit of one of the data's checks.
pub(crate) const fn unit(check: Check) -> Unit {
    Unit {
        phase: Phase::SpecificationAdmission,
        subject: Subject::CanonicalizationData,
        check,
    }
}

/// The unit Phase 5 binds the canonicalization data in.
pub(crate) const DATA: Unit = unit(Check::MatchingData);

/// The nine `localeData` bounds, in the Capability Specification's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DataBound {
    Payloads,
    PayloadBytes,
    MarkerBytes,
    ValidityGroups,
    Codes,
    ExtensionKeys,
    Types,
    Corrections,
    ValidationWork,
}

impl DataBound {
    /// The check that judges the bound.
    pub(crate) const fn check(self) -> Check {
        match self {
            Self::Payloads => Check::WithinDataPayloads,
            Self::PayloadBytes => Check::WithinDataPayloadBytes,
            Self::MarkerBytes => Check::WithinDataMarkerBytes,
            Self::ValidityGroups => Check::WithinDataValidityGroups,
            Self::Codes => Check::WithinDataCodes,
            Self::ExtensionKeys => Check::WithinDataExtensionKeys,
            Self::Types => Check::WithinDataTypes,
            Self::Corrections => Check::WithinDataCorrections,
            Self::ValidationWork => Check::WithinDataValidationWork,
        }
    }

    /// The bound's place in `bootstrapOnly`.
    pub(crate) const fn id(self) -> BootstrapBoundId {
        BootstrapBoundId {
            group: BootstrapGroup::LocaleData,
            name: match self {
                Self::Payloads => "maxPayloads",
                Self::PayloadBytes => "maxPayloadBytes",
                Self::MarkerBytes => "maxMarkerBytes",
                Self::ValidityGroups => "maxValidityGroups",
                Self::Codes => "maxCodes",
                Self::ExtensionKeys => "maxExtensionKeys",
                Self::Types => "maxTypes",
                Self::Corrections => "maxCorrections",
                Self::ValidationWork => "maxValidationWorkUnits",
            },
        }
    }

    /// The bound's value in the admitted capability.
    const fn limit(self, limits: &LocaleDataLimits) -> Bound {
        match self {
            Self::Payloads => limits.max_payloads,
            Self::PayloadBytes => limits.max_payload_bytes,
            Self::MarkerBytes => limits.max_marker_bytes,
            Self::ValidityGroups => limits.max_validity_groups,
            Self::Codes => limits.max_codes,
            Self::ExtensionKeys => limits.max_extension_keys,
            Self::Types => limits.max_types,
            Self::Corrections => limits.max_corrections,
            Self::ValidationWork => limits.max_validation_work_units,
        }
    }
}

/// What reads one count from the conformance payload's.
type CountOf = fn(&DataCounts) -> DataCount;

/// The six bounds of what the conformance payload counts, with each count.
const CONFORMANCE: [(DataBound, CountOf); 6] = [
    (DataBound::ValidityGroups, |counts| counts.validity_groups),
    (DataBound::Codes, |counts| counts.codes),
    (DataBound::ExtensionKeys, |counts| counts.extension_keys),
    (DataBound::Types, |counts| counts.types),
    (DataBound::Corrections, |counts| counts.corrections),
    (DataBound::ValidationWork, |counts| counts.validation_work),
];

/// The first-over witness of a total no exact count can report: `Bound`
/// excludes `u64::MAX`, so it is representable (D15:2003, 2007).
const fn first_over(limit: Bound) -> Actual {
    Actual::AtLeast(limit.get() + 1)
}

/// What `count` reports past `limit`, `None` within it.
const fn past(count: DataCount, limit: Bound) -> Option<Actual> {
    match count {
        DataCount::Exact(total) if total <= limit.get() => None,
        DataCount::Exact(total) => Some(Actual::Exact(total)),
        DataCount::BeyondU64 => Some(first_over(limit)),
    }
}

/// The data's root, where each of its checks observes it.
fn root() -> EvidenceProjection {
    EvidenceProjection::resolver_input(ComponentPath::data_root())
}

const NOT_ADMITTED: Blocking = Blocking {
    code: Code::LocaleDataNotAdmitted,
    reason: None,
    details: None,
};

/// The check of `bound`: what `measure` finds past its value is reported at
/// the data's root beside the capability's bound.
fn check_bound<T>(
    evaluation: &mut Evaluation,
    bound: DataBound,
    limits: &LocaleDataLimits,
    prerequisite: Admitted<T>,
    measure: impl FnOnce(T, Bound) -> Option<Actual>,
) -> Result<Admitted<()>, SpecificationInvariant> {
    let limit = bound.limit(limits);
    let observed = Observation {
        primary: root(),
        related: vec![EvidenceProjection::resolver_input(
            ComponentPath::capability(CapabilityBound::BootstrapOnly(bound.id()))?,
        )],
    };
    Ok(
        evaluation.check_found(unit(bound.check()), prerequisite, |value| {
            measure(value, limit).map_or(Ok(()), |actual| {
                Err(Found::observed(
                    Blocking::bootstrap_limit(BootstrapLimitExceeded {
                        bound: bound.id(),
                        limit,
                        actual,
                    }),
                    observed,
                ))
            })
        })?,
    )
}

/// The check of one read, whose failure is the data's, observed at its root
/// and compared with nothing else.
fn check_read<P, T>(
    evaluation: &mut Evaluation,
    check: Check,
    prerequisite: Admitted<P>,
    read: impl FnOnce(P) -> Result<T, DataNotAdmitted>,
) -> Result<Admitted<T>, EvaluationInvariant> {
    evaluation.check_found(unit(check), prerequisite, |value| {
        read(value).map_err(|DataNotAdmitted| {
            Found::observed(
                NOT_ADMITTED,
                Observation {
                    primary: root(),
                    related: Vec::new(),
                },
            )
        })
    })
}

/// The checks of the data, under the confirmed `policy`, and its canonicalizer
/// when it reads within every bound, its dataset holds, and it is the data
/// `pins` admit. Without a confirmed policy nothing is read.
pub(crate) fn admit_data<Id, D>(
    evaluation: &mut Evaluation,
    data: &D,
    limits: &LocaleDataLimits,
    pins: Admitted<SemanticPins<Id>>,
    policy: Result<&ConfirmedPolicy, &Causes>,
) -> Result<Admitted<Canonicalizer<D::Provider>>, SpecificationInvariant>
where
    Id: Clone + Eq + AsRef<str>,
    D: FormalData,
    D::Provider: Provider<Identity = Id>,
{
    let policy = policy.map_err(Causes::clone);
    let (facts, provider): (Admitted<DataFacts>, _) = match &policy {
        Ok(_) => {
            let reading = data.read(limits);
            (Ok(reading.facts), reading.provider)
        }
        Err(causes) => (Err(causes.clone()), Err(DataNotAdmitted)),
    };
    let payloads = check_bound(
        evaluation,
        DataBound::Payloads,
        limits,
        facts.clone(),
        |facts, limit| past(facts.payloads, limit),
    )?;
    let payload_bytes = check_bound(
        evaluation,
        DataBound::PayloadBytes,
        limits,
        facts.clone(),
        |facts, limit| facts.payload_bytes_over.then(|| first_over(limit)),
    )?;
    let manifest = check_read(
        evaluation,
        Check::DataManifest,
        both(both(payloads, payload_bytes), facts).map(|(_, facts)| facts),
        |facts| facts.read,
    )?;
    let markers = check_read(evaluation, Check::DataMarkers, manifest.clone(), |read| {
        read.markers
    })?;
    let mut within = check_bound(evaluation, DataBound::MarkerBytes, limits, markers, past)?;
    let conformance = check_read(evaluation, Check::DataConformance, manifest, |read| {
        read.conformance
    })?;
    for (bound, count) in CONFORMANCE {
        let checked = check_bound(
            evaluation,
            bound,
            limits,
            conformance.clone(),
            |counts, limit| past(count(&counts), limit),
        )?;
        within = both(within, checked).map(|((), ())| ());
    }
    // What is left of the reader's steps needs no value of the slot
    // (D15:2366; D17:1071, 1072, 1074).
    let dataset = check_read(
        evaluation,
        Check::IntactDataset,
        within.map(|()| provider),
        |provider| provider,
    )?;
    // The match compares the data with the slot it must be, so it alone is
    // observed beside the slot (D15:834, 1141, 2250).
    let observed = Observation {
        primary: root(),
        related: vec![EvidenceProjection::resolver_input(
            ComponentPath::specification(Slot::LocaleCanonicalization),
        )],
    };
    Ok(evaluation.check_found(
        DATA,
        both(both(pins, policy), dataset),
        |((pins, policy), provider): ((SemanticPins<Id>, &ConfirmedPolicy), D::Provider)| {
            Canonicalizer::bind(
                &pins,
                Some(provider),
                policy
                    .limits()
                    .locale_resolution
                    .max_locale_identifier_bytes,
            )
            .map_err(|_| Found::observed(NOT_ADMITTED, observed))
        },
    )?)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::locale::fixtures::FixtureProvider;
    use crate::locale::{DataReading, PayloadFacts};
    use crate::resolver::capability::BootstrapLimits;
    use crate::resolver::evaluation::{Reason, Status};
    use crate::resolver::limits::fixture_limits;

    const LIMIT: u64 = 1;
    const AT: DataCount = DataCount::Exact(LIMIT);
    const OVER: DataCount = DataCount::Exact(LIMIT + 1);

    /// Each bound, in its order, with its capability name.
    const BOUNDS: [(DataBound, &str); 9] = [
        (DataBound::Payloads, "maxPayloads"),
        (DataBound::PayloadBytes, "maxPayloadBytes"),
        (DataBound::MarkerBytes, "maxMarkerBytes"),
        (DataBound::ValidityGroups, "maxValidityGroups"),
        (DataBound::Codes, "maxCodes"),
        (DataBound::ExtensionKeys, "maxExtensionKeys"),
        (DataBound::Types, "maxTypes"),
        (DataBound::Corrections, "maxCorrections"),
        (DataBound::ValidationWork, "maxValidationWorkUnits"),
    ];

    /// The fourteen checks, in their canonical order.
    const CHECKS: [Check; 14] = [
        Check::WithinDataPayloads,
        Check::WithinDataPayloadBytes,
        Check::DataManifest,
        Check::DataMarkers,
        Check::WithinDataMarkerBytes,
        Check::DataConformance,
        Check::WithinDataValidityGroups,
        Check::WithinDataCodes,
        Check::WithinDataExtensionKeys,
        Check::WithinDataTypes,
        Check::WithinDataCorrections,
        Check::WithinDataValidationWork,
        Check::IntactDataset,
        Check::MatchingData,
    ];

    /// Where the dataset's check stands in [`CHECKS`].
    const LAST: usize = 12;

    /// The capability's minima, each bound at [`LIMIT`].
    const fn limits() -> LocaleDataLimits {
        BootstrapLimits::MINIMUM.locale_data
    }

    /// Facts with every count exactly at its bound.
    const fn at_every_bound() -> DataFacts {
        DataFacts {
            payloads: AT,
            payload_bytes_over: false,
            read: Ok(PayloadFacts {
                markers: Ok(AT),
                conformance: Ok(DataCounts {
                    validity_groups: AT,
                    codes: AT,
                    extension_keys: AT,
                    types: AT,
                    corrections: AT,
                    validation_work: AT,
                }),
            }),
        }
    }

    /// The facts with what `bound` judges past it, as `count` says.
    fn with(mut facts: DataFacts, bound: DataBound, count: DataCount) -> DataFacts {
        let read = facts.read.as_mut().unwrap();
        let counts = read.conformance.as_mut().unwrap();
        match bound {
            DataBound::Payloads => facts.payloads = count,
            DataBound::PayloadBytes => facts.payload_bytes_over = true,
            DataBound::MarkerBytes => read.markers = Ok(count),
            DataBound::ValidityGroups => counts.validity_groups = count,
            DataBound::Codes => counts.codes = count,
            DataBound::ExtensionKeys => counts.extension_keys = count,
            DataBound::Types => counts.types = count,
            DataBound::Corrections => counts.corrections = count,
            DataBound::ValidationWork => counts.validation_work = count,
        }
        facts
    }

    /// Data that reads as its facts, with the fixture's provider.
    #[derive(PartialEq, Eq)]
    struct Read(DataFacts);

    impl FormalData for Read {
        type Provider = FixtureProvider;

        fn read(&self, _: &LocaleDataLimits) -> DataReading<FixtureProvider> {
            DataReading {
                facts: self.0,
                provider: Ok(FixtureProvider::new()),
            }
        }
    }

    /// Data within every bound whose dataset does not hold.
    #[derive(PartialEq, Eq)]
    struct Broken;

    impl FormalData for Broken {
        type Provider = FixtureProvider;

        fn read(&self, _: &LocaleDataLimits) -> DataReading<FixtureProvider> {
            DataReading {
                facts: at_every_bound(),
                provider: Err(DataNotAdmitted),
            }
        }
    }

    fn pins() -> SemanticPins<&'static str> {
        FixtureProvider::new().binding().pins.clone()
    }

    /// The causes of a `locale-canonicalization` slot that names nothing.
    pub(crate) fn slot_causes() -> Causes {
        let slot = Unit {
            phase: Phase::SpecificationAdmission,
            subject: Subject::Slot(Slot::LocaleCanonicalization),
            check: Check::SupportedAssertion,
        };
        let blocking = Blocking {
            code: Code::SpecificationInputIncomplete,
            reason: Some(Reason::MissingRequiredSlot),
            details: None,
        };
        Evaluation::fabricated()
            .check_found(slot, Ok(()), |()| Err::<(), _>(Found::from(blocking)))
            .unwrap()
            .unwrap_err()
    }

    /// The checks of data reading as `facts` under a confirmed policy, and
    /// whether a canonicalizer was bound.
    fn run(facts: DataFacts, pins: Admitted<SemanticPins<&'static str>>) -> (Evaluation, bool) {
        run_data(&Read(facts), pins)
    }

    /// The checks of `data` under a confirmed policy.
    fn run_data(
        data: &impl FormalData<Provider = FixtureProvider>,
        pins: Admitted<SemanticPins<&'static str>>,
    ) -> (Evaluation, bool) {
        let policy = ConfirmedPolicy::fixture_owned(fixture_limits(100));
        let mut evaluation = Evaluation::default();
        let bound = admit_data(&mut evaluation, data, &limits(), pins, Ok(&policy));
        (evaluation, bound.unwrap().is_ok())
    }

    fn read(facts: DataFacts) -> (Evaluation, bool) {
        run(facts, Ok(pins()))
    }

    /// The checks that found something, each with its code.
    pub(crate) fn found(evaluation: &Evaluation) -> Vec<(Check, Code)> {
        evaluation
            .issues()
            .map(|key| (key.unit().check, key.code()))
            .collect()
    }

    /// The checks not evaluated, in canonical order.
    pub(crate) fn waiting(evaluation: &Evaluation) -> Vec<Check> {
        evaluation
            .statuses()
            .filter(|(_, status)| matches!(status, Status::NotEvaluated(_)))
            .map(|(unit, _)| unit.check)
            .collect()
    }

    fn exceeded(evaluation: &Evaluation, bound: DataBound) -> BootstrapLimitExceeded {
        let key = evaluation
            .issues()
            .find(|key| key.unit().check == bound.check())
            .unwrap();
        evaluation.bootstrap_limit(key).unwrap()
    }

    const fn over(bound: DataBound) -> (Check, Code) {
        (bound.check(), Code::BootstrapLimitExceeded)
    }

    /// D15:3395: each bound admits its exact value, and the next is its own
    /// check's Finding with its capability path and total, the payload length
    /// its first-over witness (D15:3984), leaving unevaluated only what needs
    /// the bound.
    #[test]
    fn each_bound_admits_its_limit_and_reports_the_next() {
        let (evaluation, bound) = read(at_every_bound());
        assert_eq!((found(&evaluation), waiting(&evaluation)), (vec![], vec![]));
        assert!(bound);
        for (data, name) in BOUNDS {
            let (evaluation, bound) = read(with(at_every_bound(), data, OVER));
            assert_eq!(found(&evaluation), [over(data)]);
            let reported = exceeded(&evaluation, data);
            assert_eq!(reported.bound.group, BootstrapGroup::LocaleData);
            assert_eq!(reported.bound.name, name);
            assert_eq!(reported.limit.get(), LIMIT);
            let actual = if data == DataBound::PayloadBytes {
                Actual::AtLeast(LIMIT + 1)
            } else {
                Actual::Exact(LIMIT + 1)
            };
            assert_eq!(reported.actual, actual, "{data:?}");
            assert!(!bound, "{data:?}");
            let manifest = matches!(data, DataBound::Payloads | DataBound::PayloadBytes);
            let expected = if manifest {
                &CHECKS[2..]
            } else {
                &CHECKS[LAST..]
            };
            assert_eq!(waiting(&evaluation), expected, "{data:?}");
        }
    }

    /// D15:1998: a count past a `u64` is past the bound, with the witness.
    #[test]
    fn a_count_past_u64_reports_the_first_over_witness() {
        for (data, _) in BOUNDS {
            let facts = with(at_every_bound(), data, DataCount::BeyondU64);
            let actual = exceeded(&read(facts).0, data).actual;
            assert_eq!(actual, Actual::AtLeast(LIMIT + 1), "{data:?}");
        }
    }

    /// D15:2009: no violation hides another, and the match waits on them all.
    #[test]
    fn every_bound_past_its_limit_is_reported_in_order() {
        let facts = BOUNDS[2..]
            .iter()
            .fold(at_every_bound(), |facts, (data, _)| {
                with(facts, *data, OVER)
            });
        let (evaluation, _) = read(facts);
        let expected: Vec<_> = BOUNDS[2..].iter().map(|(data, _)| over(*data)).collect();
        assert_eq!(found(&evaluation), expected);
        assert_eq!(waiting(&evaluation), CHECKS[LAST..]);
        let (_, status) = evaluation
            .statuses()
            .find(|(unit, _)| **unit == DATA)
            .unwrap();
        let Status::NotEvaluated(causes) = status else {
            panic!("the match waits");
        };
        assert_eq!(causes.keys().count(), expected.len());
    }

    /// D15:2009: the two bounds the manifest decides do not hide each other.
    #[test]
    fn both_bounds_the_manifest_decides_are_reported_in_order() {
        let both = with(
            with(at_every_bound(), DataBound::Payloads, OVER),
            DataBound::PayloadBytes,
            OVER,
        );
        let (evaluation, _) = read(both);
        assert_eq!(
            found(&evaluation),
            [over(DataBound::Payloads), over(DataBound::PayloadBytes)]
        );
        assert_eq!(waiting(&evaluation), CHECKS[2..]);
    }

    /// D15:2366: a manifest that does not read stops what reads the payloads.
    #[test]
    fn a_manifest_that_does_not_read_leaves_the_payloads_unread() {
        let (evaluation, bound) = read(DataFacts {
            read: Err(DataNotAdmitted),
            ..at_every_bound()
        });
        assert_eq!(
            found(&evaluation),
            [(Check::DataManifest, Code::LocaleDataNotAdmitted)]
        );
        assert_eq!(waiting(&evaluation), CHECKS[3..]);
        assert!(!bound);
    }

    /// D15:2366: one payload that does not read leaves the other's bounds to
    /// report.
    #[test]
    fn one_payloads_failure_leaves_the_others_bounds_to_report() {
        let mut markers = with(at_every_bound(), DataBound::Codes, OVER);
        markers.read.as_mut().unwrap().markers = Err(DataNotAdmitted);
        let (evaluation, bound) = read(markers);
        assert_eq!(
            found(&evaluation),
            [
                (Check::DataMarkers, Code::LocaleDataNotAdmitted),
                over(DataBound::Codes),
            ]
        );
        assert_eq!(
            waiting(&evaluation),
            [
                Check::WithinDataMarkerBytes,
                Check::IntactDataset,
                Check::MatchingData
            ]
        );
        assert!(!bound);

        let mut conformance = with(at_every_bound(), DataBound::MarkerBytes, OVER);
        conformance.read.as_mut().unwrap().conformance = Err(DataNotAdmitted);
        let (evaluation, bound) = read(conformance);
        assert_eq!(
            found(&evaluation),
            [
                over(DataBound::MarkerBytes),
                (Check::DataConformance, Code::LocaleDataNotAdmitted),
            ]
        );
        assert_eq!(waiting(&evaluation), CHECKS[6..]);
        assert!(!bound);
    }

    /// D15:2134, 2366: without a value of the slot the bounds still report,
    /// and of the checks only the match waits on it.
    #[test]
    fn a_bound_is_reported_without_the_slot_and_only_the_match_waits() {
        let (evaluation, bound) = run(at_every_bound(), Err(slot_causes()));
        assert_eq!(found(&evaluation), []);
        assert_eq!(waiting(&evaluation), [Check::MatchingData]);
        assert!(!bound);
        let facts = with(at_every_bound(), DataBound::Payloads, OVER);
        let (evaluation, _) = run(facts, Err(slot_causes()));
        assert_eq!(found(&evaluation), [over(DataBound::Payloads)]);
    }

    /// A bound is observed at the data's root beside the capability's bound,
    /// and a read at the root alone (D15:2250).
    #[test]
    fn each_check_is_observed_at_the_data_root() {
        for (data, _) in BOUNDS {
            let (evaluation, _) = read(with(at_every_bound(), data, OVER));
            let [candidate] = evaluation.occurrence_candidates() else {
                panic!("one candidate");
            };
            let bound = ComponentPath::capability(CapabilityBound::BootstrapOnly(data.id()));
            assert_eq!(candidate.primary, root());
            assert_eq!(
                candidate.related,
                [EvidenceProjection::resolver_input(bound.unwrap())]
            );
        }
        let (evaluation, _) = read(DataFacts {
            read: Err(DataNotAdmitted),
            ..at_every_bound()
        });
        let [candidate] = evaluation.occurrence_candidates() else {
            panic!("one candidate");
        };
        assert_eq!(candidate.primary, root());
        assert_eq!(candidate.related, []);
    }

    /// D15:2366, D17:1071, 1072, 1074: a dataset that does not hold is proved
    /// without the slot, at the data's root, and only the match waits.
    #[test]
    fn a_dataset_that_does_not_hold_is_reported_with_or_without_the_slot() {
        for pins in [Ok(pins()), Err(slot_causes())] {
            let (evaluation, bound) = run_data(&Broken, pins);
            assert_eq!(
                found(&evaluation),
                [(Check::IntactDataset, Code::LocaleDataNotAdmitted)]
            );
            assert_eq!(waiting(&evaluation), [Check::MatchingData]);
            assert!(!bound);
            let [candidate] = evaluation.occurrence_candidates() else {
                panic!("one candidate");
            };
            assert_eq!(candidate.primary, root());
            assert_eq!(candidate.related, []);
        }
    }

    /// D15:2366: past a bound the dataset is not evaluated, and the bound's
    /// Finding is its cause.
    #[test]
    fn a_bound_past_its_limit_is_the_cause_the_dataset_waits_on() {
        for (data, _) in BOUNDS {
            let (evaluation, _) = read(with(at_every_bound(), data, OVER));
            let dataset = unit(Check::IntactDataset);
            let status = evaluation.statuses().find(|(unit, _)| **unit == dataset);
            let Some((_, Status::NotEvaluated(causes))) = status else {
                panic!("the dataset waits past {data:?}");
            };
            assert!(causes.keys().eq(evaluation.issues()), "{data:?}");
        }
    }

    /// D17:1074: a dataset that holds and is not the one the pins admit is the
    /// match's Finding alone, at the data's root beside its slot.
    #[test]
    fn pins_that_differ_are_a_finding_of_the_match_at_root_and_slot() {
        let mut other = pins();
        other.dataset.semantic_digest = "sha256:another-dataset";
        let (evaluation, bound) = run(at_every_bound(), Ok(other));
        assert_eq!(
            found(&evaluation),
            [(Check::MatchingData, Code::LocaleDataNotAdmitted)]
        );
        assert_eq!(waiting(&evaluation), []);
        assert!(!bound);
        let [candidate] = evaluation.occurrence_candidates() else {
            panic!("one candidate");
        };
        let slot = ComponentPath::specification(Slot::LocaleCanonicalization);
        assert_eq!(candidate.primary, root());
        assert_eq!(
            candidate.related,
            [EvidenceProjection::resolver_input(slot)]
        );
    }

    /// Each bound the manifest decides reads its own value of the capability.
    #[test]
    fn each_bound_reads_its_own_value() {
        let mut limits = limits();
        limits.max_payloads = Bound::new(1).unwrap();
        limits.max_payload_bytes = Bound::new(2).unwrap();
        assert_eq!(DataBound::Payloads.limit(&limits).get(), 1);
        assert_eq!(DataBound::PayloadBytes.limit(&limits).get(), 2);
    }
}
