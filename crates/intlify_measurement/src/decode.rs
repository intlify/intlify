// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Bounded strict reading for the common records.
//!
//! The capacities, duplicate rejection, and number policy belong to
//! `intlify_shared_json`. This module only re-exports the shared reader so a
//! common record and an owner document are read under the same rules.

pub use intlify_shared_json::decode::{typed, value, DecodeFailure};
