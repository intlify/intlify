// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Shared deterministic limit and collection validation helpers.
//!
//! This module owns the small mechanics used by policy, mapping, graph, and
//! request admission. It does not choose stage ordering or semantic subjects;
//! each owning module supplies those contract decisions explicitly.

use intlify_contract::{
    LinkLimitCounter, LinkLimitEvidence, LinkLimitObservation, LinkLimitSubject, LinkLimits,
};

use crate::LinkOperationalError;

pub(crate) fn usize_count(value: usize) -> u64 {
    u64::try_from(value).expect("supported Rust targets cannot represent usize above u64")
}

pub(crate) fn check_first_over(
    counter: LinkLimitCounter,
    subject: LinkLimitSubject,
    observed: u64,
    limits: &LinkLimits,
) -> Result<(), LinkOperationalError> {
    let effective_limit = limits.effective_limit(counter);
    if observed <= effective_limit {
        return Ok(());
    }
    Err(LinkLimitEvidence::try_new(
        counter,
        subject,
        effective_limit,
        LinkLimitObservation::Exact(effective_limit + 1),
    )
    .expect("counter and subject are fixed by the linker contract")
    .into())
}

pub(crate) fn check_exact(
    counter: LinkLimitCounter,
    subject: LinkLimitSubject,
    observed: u64,
    limits: &LinkLimits,
) -> Result<(), LinkOperationalError> {
    let effective_limit = limits.effective_limit(counter);
    if observed <= effective_limit {
        return Ok(());
    }
    Err(LinkLimitEvidence::try_new(
        counter,
        subject,
        effective_limit,
        LinkLimitObservation::Exact(observed),
    )
    .expect("counter and subject are fixed by the linker contract")
    .into())
}

pub(crate) fn arithmetic_overflow(
    counter: LinkLimitCounter,
    subject: LinkLimitSubject,
    limits: &LinkLimits,
) -> LinkOperationalError {
    LinkLimitEvidence::try_new(
        counter,
        subject,
        limits.effective_limit(counter),
        LinkLimitObservation::ArithmeticOverflow,
    )
    .expect("overflow-capable counter and subject are fixed by the linker contract")
    .into()
}

#[cfg(test)]
mod tests {
    use intlify_contract::{
        DeliveryUnitId, LinkLimitCounter, LinkLimitObservation, LinkLimitSubject, LinkLimits,
    };

    use super::{arithmetic_overflow, check_exact, check_first_over, usize_count};
    use crate::LinkOperationalError;

    #[test]
    fn usize_count_preserves_supported_host_counts() {
        assert_eq!(usize_count(0), 0);
        assert_eq!(usize_count(usize::MAX), usize::MAX as u64);
    }

    #[test]
    fn check_first_over_accepts_the_boundary_and_reports_only_the_first_excess() {
        let counter = LinkLimitCounter::DeliveryGraphNodes;
        let subject = LinkLimitSubject::DeliveryGraph;
        let limits = LinkLimits::default().try_with_limit(counter, 2).unwrap();

        assert!(check_first_over(counter, subject.clone(), 2, &limits).is_ok());

        let error = check_first_over(counter, subject.clone(), 99, &limits).unwrap_err();
        let LinkOperationalError::Limit(evidence) = error else {
            panic!("expected limit evidence");
        };
        assert_eq!(evidence.counter(), counter);
        assert_eq!(evidence.subject(), &subject);
        assert_eq!(evidence.effective_limit(), 2);
        assert_eq!(evidence.observation(), LinkLimitObservation::Exact(3));
    }

    #[test]
    fn check_exact_accepts_the_boundary_and_reports_the_attempted_total() {
        let counter = LinkLimitCounter::DeliveryGraphIdBytes;
        let subject = LinkLimitSubject::DeliveryUnitGroup(DeliveryUnitId::main());
        let limits = LinkLimits::default().try_with_limit(counter, 3).unwrap();

        assert!(check_exact(counter, subject.clone(), 3, &limits).is_ok());

        let error = check_exact(counter, subject.clone(), 9, &limits).unwrap_err();
        let LinkOperationalError::Limit(evidence) = error else {
            panic!("expected limit evidence");
        };
        assert_eq!(evidence.counter(), counter);
        assert_eq!(evidence.subject(), &subject);
        assert_eq!(evidence.effective_limit(), 3);
        assert_eq!(evidence.observation(), LinkLimitObservation::Exact(9));
    }

    #[test]
    fn arithmetic_overflow_preserves_the_counter_subject_and_effective_limit() {
        let counter = LinkLimitCounter::DeliveryGraphIdBytes;
        let subject = LinkLimitSubject::DeliveryUnitGroup(DeliveryUnitId::main());
        let limits = LinkLimits::default().try_with_limit(counter, 3).unwrap();

        let error = arithmetic_overflow(counter, subject.clone(), &limits);
        let LinkOperationalError::Limit(evidence) = error else {
            panic!("expected limit evidence");
        };
        assert_eq!(evidence.counter(), counter);
        assert_eq!(evidence.subject(), &subject);
        assert_eq!(evidence.effective_limit(), 3);
        assert_eq!(
            evidence.observation(),
            LinkLimitObservation::ArithmeticOverflow
        );
    }
}
