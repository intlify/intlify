// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Observational measurement of this crate's own operations.
//!
//! Two operations are measured, under design 016's vocabulary:
//! `source_discovery / parse_and_classify` reads one admitted unit into its
//! classified declarations, uses and exclusions, and
//! `authoring_result / inventory_assembly` merges analyzed units into a sealed
//! inventory and its bytes. The run that captures and seals them, the records,
//! and their admission belong to `intlify_measurement`; what is here is what
//! only this owner knows: the fixtures, the operations they invoke, and what
//! counts as the same result.
//!
//! The harness calls the ordinary internal operations. Source discovery stops
//! where `analyze_unit` hands declarations to `intlify_authoring`, through the
//! same function `analyze_unit` runs first, so nothing is reimplemented for
//! measurement. A parse-free recognition measurement would be a different
//! method and is not defined.

pub mod facade;

mod cases;
mod descriptor;
mod operation;
mod owner;
mod projection;
