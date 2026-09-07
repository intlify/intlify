// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Workspace-internal, host-independent configuration foundations.
//!
//! This crate currently extracts the CLI's JSON compatibility decoder and
//! deterministic schema helpers. It does not expose a project-profile resolver
//! or claim support for Profile Specification revision "0".
//!
//! Discovery, file I/O, JSONC adaptation, and product diagnostics stay with their
//! callers. Decoded values own storage; no mutable global or shared scratch is
//! required. This crate is unpublished, and its workspace helpers are not a
//! reserved product API.

pub mod json;
pub mod location;
pub mod schema;
