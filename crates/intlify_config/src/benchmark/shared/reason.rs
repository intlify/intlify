// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The revision-zero reason vocabulary, narrowed to this implementation.
//!
//! Codes, stages, selectors, typed details, canonical ordering, and the
//! code/detail compatibility rule belong to `intlify_measurement`.

pub(in crate::benchmark) use intlify_measurement::reason::valid_reasons;
pub(super) use intlify_measurement::reason::{
    ordered, BuildField, CommonCode, Detail, EnvironmentField, InvocationFailure,
    MissingObservation, Reason, Selector, Stage,
};
