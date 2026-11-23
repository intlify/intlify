// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Phase 5's reading of the Locale Canonicalization Data Artifact past its
//! boundary, as the checks of its singleton subject (D15:1990). A
//! `bootstrapOnly.localeData` bound is a check of its own, reporting
//! `project-profile-bootstrap-limit-exceeded` with the bound's capability path
//! (D15:2356, 3356; Decision 015-160).
//!
//! The two bounds the manifest decides come first and do not hide each other
//! (D15:2008), then the match with the admitted specification, which still
//! reads the rest of the data under the other seven bounds, which have no check
//! of their own yet. Past `maxPayloadBytes` neither payload is read: the bound
//! is one check of a singleton, and what a check waits on does not vary with
//! the value it finds (D15:2149).
//!
//! Only the match waits on the `locale-canonicalization` slot. Reading the
//! data and measuring it need the confirmed policy and no value of the slot,
//! so a bound's violation stays provable beside an invalid slot (D15:2133,
//! 2365).

use crate::input_limits::Bound;
use crate::locale::{
    Canonicalizer, DataCount, DataFacts, DataNotAdmitted, FormalData, LocaleDataLimits, Provider,
    SemanticPins,
};

use super::capability::{BootstrapBoundId, BootstrapGroup, CapabilityBound};
use super::evaluation::{
    both, Actual, Admitted, Blocking, BootstrapLimitExceeded, Causes, Check, Code, Evaluation,
    Found, Observation, Phase, Subject, Unit,
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

/// The `localeData` bounds with a check of their own, in the Capability
/// Specification's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DataBound {
    Payloads,
    PayloadBytes,
}

impl DataBound {
    /// The check that judges the bound.
    pub(crate) const fn check(self) -> Check {
        match self {
            Self::Payloads => Check::WithinDataPayloads,
            Self::PayloadBytes => Check::WithinDataPayloadBytes,
        }
    }

    /// The bound's place in `bootstrapOnly`.
    pub(crate) const fn id(self) -> BootstrapBoundId {
        BootstrapBoundId {
            group: BootstrapGroup::LocaleData,
            name: match self {
                Self::Payloads => "maxPayloads",
                Self::PayloadBytes => "maxPayloadBytes",
            },
        }
    }

    /// The bound's value in the admitted capability.
    const fn limit(self, limits: &LocaleDataLimits) -> Bound {
        match self {
            Self::Payloads => limits.max_payloads,
            Self::PayloadBytes => limits.max_payload_bytes,
        }
    }
}

/// The first-over witness of a total no exact count can report: `Bound`
/// excludes `u64::MAX`, so it is representable (D15:2002, 2006).
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

/// The checks of the data, under the confirmed `policy`, and its canonicalizer
/// when it reads within every bound and is the data `pins` admit. Without a
/// confirmed policy nothing is read.
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
        facts,
        |facts, limit| facts.payload_bytes_over.then(|| first_over(limit)),
    )?;
    // The match compares the data with the slot it must be, so it alone is
    // observed beside the slot (D15:833, 1140, 2249).
    let observed = Observation {
        primary: root(),
        related: vec![EvidenceProjection::resolver_input(
            ComponentPath::specification(Slot::LocaleCanonicalization),
        )],
    };
    Ok(evaluation.check_found(
        DATA,
        both(both(pins, policy), both(payloads, payload_bytes)).map(|(admitted, _)| admitted),
        |(pins, policy): (SemanticPins<Id>, &ConfirmedPolicy)| {
            let mismatch = || Found::observed(NOT_ADMITTED, observed.clone());
            let provider = provider.map_err(|DataNotAdmitted| mismatch())?;
            Canonicalizer::bind(
                &pins,
                Some(provider),
                policy
                    .limits()
                    .locale_resolution
                    .max_locale_identifier_bytes,
            )
            .map_err(|_| mismatch())
        },
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locale::fixtures::FixtureProvider;
    use crate::locale::DataReading;
    use crate::resolver::capability::BootstrapLimits;
    use crate::resolver::evaluation::{Reason, Status};
    use crate::resolver::limits::fixture_limits;

    const LIMIT: u64 = 1;
    const AT: DataCount = DataCount::Exact(LIMIT);
    const OVER: DataCount = DataCount::Exact(LIMIT + 1);

    /// Each bound, in its order, with its capability name.
    const BOUNDS: [(DataBound, &str); 2] = [
        (DataBound::Payloads, "maxPayloads"),
        (DataBound::PayloadBytes, "maxPayloadBytes"),
    ];

    /// The three checks, in their canonical order.
    const CHECKS: [Check; 3] = [
        Check::WithinDataPayloads,
        Check::WithinDataPayloadBytes,
        Check::MatchingData,
    ];

    /// The capability's minima, each bound at [`LIMIT`].
    const fn limits() -> LocaleDataLimits {
        BootstrapLimits::MINIMUM.locale_data
    }

    /// Facts with every count exactly at its bound.
    const fn at_every_bound() -> DataFacts {
        DataFacts {
            payloads: AT,
            payload_bytes_over: false,
        }
    }

    /// The facts with what `bound` judges past it, as `count` says.
    fn with(mut facts: DataFacts, bound: DataBound, count: DataCount) -> DataFacts {
        match bound {
            DataBound::Payloads => facts.payloads = count,
            DataBound::PayloadBytes => facts.payload_bytes_over = true,
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

    fn pins() -> SemanticPins<&'static str> {
        FixtureProvider::new().binding().pins.clone()
    }

    /// The causes of a `locale-canonicalization` slot that names nothing.
    fn slot_causes() -> Causes {
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
        let policy = ConfirmedPolicy::fixture_owned(fixture_limits(100));
        let mut evaluation = Evaluation::default();
        let bound = admit_data(&mut evaluation, &Read(facts), &limits(), pins, Ok(&policy));
        (evaluation, bound.unwrap().is_ok())
    }

    fn read(facts: DataFacts) -> (Evaluation, bool) {
        run(facts, Ok(pins()))
    }

    /// The checks that found something, each with its code.
    fn found(evaluation: &Evaluation) -> Vec<(Check, Code)> {
        evaluation
            .issues()
            .map(|key| (key.unit().check, key.code()))
            .collect()
    }

    /// The checks not evaluated, in canonical order.
    fn waiting(evaluation: &Evaluation) -> Vec<Check> {
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

    /// D15:3302: each bound admits its exact value, and the next is its own
    /// check's Finding with its capability path and total, the payload length
    /// its first-over witness (D15:3859), leaving unevaluated only what needs
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
            assert_eq!(waiting(&evaluation), [Check::MatchingData], "{data:?}");
        }
    }

    /// D15:1997: a count past a `u64` is past the bound, with the witness.
    #[test]
    fn a_count_past_u64_reports_the_first_over_witness() {
        for (data, _) in BOUNDS {
            let facts = with(at_every_bound(), data, DataCount::BeyondU64);
            let actual = exceeded(&read(facts).0, data).actual;
            assert_eq!(actual, Actual::AtLeast(LIMIT + 1), "{data:?}");
        }
    }

    /// D15:2008: the two bounds the manifest decides do not hide each other.
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

    /// D15:2133, 2365: without a value of the slot the bounds still report,
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

    /// A bound is observed at the data's root beside the capability's bound
    /// (D15:2249).
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
