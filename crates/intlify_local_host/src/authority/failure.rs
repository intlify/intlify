// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Why an operation was not authorized.
//!
//! The variants follow design 018's failure categories: an unknown caller, a
//! denied action, inputs outside the authority's scope, a plan that is not
//! for these inputs, a confirmation that is missing or no longer valid, a
//! development update the session does not allow, and an initialization the
//! binding does not allow. None of them says whether an identity change is
//! valid. That is design 016's question, answered before anything is
//! authorized, and authorization cannot change its answer.

use intlify_authoring::MessageIntentId;

use super::action::Action;
use super::context::AuthorityLimitKind;

/// Why an invocation was refused an operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizationFailure {
    /// The caller is not a principal this authority established.
    UnknownCaller,
    /// The caller holds no grant of this action.
    Denied(Action),
    /// An input belongs to another owner than the destination's.
    OwnerMismatch,
    /// An input declares another owning scope than the destination's.
    ScopeMismatch,
    /// The inventory was resolved against another context than the
    /// authority's.
    ContextMismatch,
    /// The inventory names a source snapshot the host did not acquire for
    /// this invocation.
    SourceNotAcquired,
    /// The inventory claims to be complete without every unit the host
    /// acquired.
    IncompleteScope,
    /// The registry is not the destination's chain.
    RegistryMismatch,
    /// The registry is not an anchor this authority accepted. A chain that
    /// only agrees with itself anchors nothing.
    Unanchored,
    /// The plan was not made for this base and inventory.
    PlanMismatch,
    /// The explicit decision was made for another base or inventory.
    UnboundDecision,
    /// An explicit decision of the plan has no confirmation.
    ConfirmationMissing(MessageIntentId),
    /// A confirmation was issued under another authority: grants, session or
    /// settings have changed since, so it has to be given again.
    ConfirmationStale,
    /// A confirmation is for another base, inventory or action than the
    /// plan's.
    ConfirmationMismatch,
    /// A confirmation matches no explicit decision of the plan.
    UnusedConfirmation,
    /// A development update was requested without an active development
    /// session.
    SessionInactive,
    /// A development update holds a decision that needs confirmation, which
    /// only a manual update accepts.
    NotAutomatic,
    /// The destination already has a registry chain.
    AlreadyInitialized,
    /// The registry offered for initialization is not an empty genesis.
    NotGenesis,
    /// A named bound was exhausted.
    Limit(AuthorityLimitKind),
}
