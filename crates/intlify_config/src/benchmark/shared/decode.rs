// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Bounded strict reading for the adopted common records.
//!
//! The capacities, duplicate rejection, and number policy belong to
//! `intlify_shared_json`, and the common reader to `intlify_measurement`. This
//! module only narrows them to the visibility used inside the implementation.

pub(in crate::benchmark) use intlify_measurement::decode::DecodeFailure;
pub(super) use intlify_measurement::decode::{typed, value};
