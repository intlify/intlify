// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Bounded strict reading for the adopted common records.
//!
//! The capacities, duplicate rejection, and number policy belong to
//! `intlify_shared_json`. This module only narrows the shared reader to the
//! visibility used inside the measurement implementation.

pub(in crate::benchmark) use intlify_shared_json::decode::DecodeFailure;
pub(super) use intlify_shared_json::decode::{typed, value};
