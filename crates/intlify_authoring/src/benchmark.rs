// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Observational measurement of this crate's own operations.
//!
//! The records, their admission, the duration acquisition, and the owner run
//! that captures and seals them belong to `intlify_measurement`. What lives
//! here is what only this owner knows: which operations are measured, which
//! fixtures they run against, and what counts as the same result.
//!
//! The harness calls the ordinary internal operations. It does not reimplement
//! any of them for measurement, and nothing it observes is derived from a
//! duration.

pub mod facade;

mod cases;
mod descriptor;
mod operation;
mod owner;
mod projection;
