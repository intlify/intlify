// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Observational measurement of this crate's own operations.
//!
//! Two operations are measured, under design 016's vocabulary:
//! `identity_reconciliation / association_planning` reconciles an admitted
//! inventory against an exact base into association decisions and a
//! proposed plan, with fixed allocation candidates, and
//! `identity_reconciliation / registry_replay` verifies a retained chain from
//! its head back to its genesis and replays it forward. The run that captures
//! and seals them, the records, and their admission belong to
//! `intlify_measurement`; what is here is what only this owner knows: the
//! fixtures, the operations they invoke, and what counts as the same result.
//!
//! The harness calls the ordinary public operations, `reconcile` and
//! `verify_history`, so nothing is reimplemented for measurement. Allocation
//! is not measured: drawing an ID belongs to the host, outside deterministic
//! reconciliation, and a fixture's candidates are fixed values.

pub mod facade;

mod cases;
mod descriptor;
mod operation;
mod owner;
mod projection;
