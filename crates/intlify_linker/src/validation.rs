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
