// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 029's registry operations against an in-memory host.
//!
//! The host turns design 018's evaluator into operations. It holds one owner
//! binding and its registry chain, establishes each session's authority from
//! its own state, prepares updates by reconciling and drawing fresh Intent
//! IDs from operating-system randomness, and makes an authorized result
//! current only after checking, under one guard, that the base is still
//! current and the authority unchanged.
//!
//! The host is test-owned and in memory. It is compiled only under the
//! `test-authority` feature, like the authority it establishes, and nothing
//! it publishes is written anywhere: a publication here is not evidence of
//! disk durability. Local persistence and production publication come with a
//! later plan, once checked 015 inputs and a 029 host adapter exist.

#[cfg(test)]
mod fixtures;
mod memory;
mod outcome;
mod random;
mod session;

pub use self::memory::{Acquisition, BindingState, HostLimits, HostSetup, MemoryHost};
pub use self::outcome::{
    Blocked, Conflict, Currency, Durability, HostFailure, Operation, Preparation, PreparedUpdate,
    Publication, PublicationRecord, RegistryRead,
};
pub use self::random::{OsRandomness, Randomness, RandomnessFailure};
pub use self::session::{PrepareInputs, Session};
