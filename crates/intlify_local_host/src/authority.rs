// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 018's local authority for registry operations, evaluated in
//! process.
//!
//! The evaluator answers one question: may this established caller, under
//! this authority, perform this exact operation on these exact inputs? It
//! keeps the neighbouring questions apart. Whether the bytes and the identity
//! change are valid is answered before anything here runs, by admission and
//! design 016's checks; nothing here can change that answer. Whether an
//! authorized change becomes current is the host's conditional commit, which
//! checks the base and the authority again at the point of publication.
//!
//! The stages run in 018's order: the caller and its grants, the inputs
//! against the authority, then the exact operation. Each refusal names its
//! cause, so a denied action is never reported as a broken input, or the
//! other way round.

mod action;
mod binding;
mod confirmation;
mod context;
mod failure;
#[cfg(test)]
pub(crate) mod fixtures;
mod permit;

pub use self::action::{Action, ActionSet};
pub use self::confirmation::Confirmation;
#[cfg(feature = "test-authority")]
pub(crate) use self::context::Establishment;
pub use self::context::{
    AuthorityLimitKind, AuthorityLimits, Destination, EstablishmentFailure, Invocation,
    LocalAuthority, Principal,
};
pub use self::failure::AuthorizationFailure;
pub use self::permit::{
    InitializationPermit, PermitMismatch, UpdateMode, UpdatePermit, UpdateRequest,
};
