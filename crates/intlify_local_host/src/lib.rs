// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The local host for design 016's persistent identity: design 018's
//! authority evaluated in process, and design 029's registry operations
//! against an in-memory host.
//!
//! `intlify_authoring_identity` decides whether an identity change holds.
//! This crate decides who may make it: which established caller may analyze,
//! read, confirm an explicit choice, initialize or update one application's
//! registry, for which exact inputs, under which authority. It never weakens
//! a check made there. Its host makes an authorized change current only
//! after checking the base and the authority again at that point, and only
//! in memory: nothing it publishes is written anywhere yet.
//!
//! The only authority this phase establishes is explicitly test-owned, and
//! an ordinary build cannot establish one at all, nor draw a fresh identity.
//! The example below is
//! compiled only in that build, because an ordinary build is the one whose
//! reachability it describes; with the feature enabled the import is
//! supposed to resolve.
#![cfg_attr(
    not(feature = "test-authority"),
    doc = "```compile_fail",
    doc = "use intlify_local_host::test_authority::TestAuthority;",
    doc = "```"
)]
//! Neither a confirmation nor a permit can be read from data, so a
//! serialized reason, a label or a copied set of fields never becomes one:
//!
//! ```compile_fail
//! use intlify_local_host::Confirmation;
//!
//! let forged: Confirmation = serde_json::from_str("{}").unwrap();
//! ```

mod authority;
#[cfg(feature = "test-authority")]
pub mod host;
#[cfg(feature = "test-authority")]
pub mod test_authority;

pub use authority::{
    Action, ActionSet, AuthorityLimitKind, AuthorityLimits, AuthorizationFailure, Confirmation,
    Destination, EstablishmentFailure, InitializationPermit, Invocation, LocalAuthority,
    PermitMismatch, Principal, UpdateMode, UpdatePermit, UpdateRequest,
};
