// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Observational measurement of this crate's own operations.
//!
//! The records, their admission, and the duration acquisition belong to
//! `intlify_measurement`. What lives here is what only this owner knows: which
//! operations are measured, which fixtures they run against, what counts as
//! the same result, and how each planned case turned out.
//!
//! The harness calls the ordinary internal operations. It does not reimplement
//! any of them for measurement, and nothing it observes is derived from a
//! duration.

pub mod facade;

mod adapter;
mod capture;
mod cases;
mod context;
mod descriptor;
mod observation;
mod operation;
mod projection;
mod run;
