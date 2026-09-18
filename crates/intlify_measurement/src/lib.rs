// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The owner-neutral measurement records of design 026.
//!
//! This crate owns the common representation an observing owner produces: the
//! record envelope and its references, the registered instance-identity and
//! digest domains, the reason vocabulary, and the Run Plan, Evidence, Run
//! Evaluation, and structured report bodies.
//!
//! It owns no fixtures, no clock, no workload, and no owner vocabulary. An
//! owner supplies its own case projection, descriptors, and native checksum
//! through the adapter in [`owner`], and this crate never recaptures a
//! duration, invents an absent observation, or turns a submitted record into
//! its own input authority.
//!
//! Producing a record is not admission. Integrity, binding, inventory, and
//! projection eligibility remain separate checks, and a failed or absent case
//! stays in the record rather than being removed from it.

pub mod decode;
pub mod encoding;
pub mod environment;
pub mod identity;
pub mod reason;
pub mod record;
pub mod schema;
