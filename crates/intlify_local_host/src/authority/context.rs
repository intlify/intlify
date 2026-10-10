// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The local authority one host establishes for one bounded invocation.
//!
//! Design 018's authority comes from a trusted host, through a path outside
//! the source and artifacts being analyzed. It binds one application owner's
//! registry destination, the analysis context, the sources the host acquired,
//! the registry anchors it accepted, the actions each established principal
//! holds, and whether a development session is active. Nothing here is read
//! from data: there is no deserialization, and the only way to establish an
//! authority in this phase is the explicitly test-owned one.
//!
//! An authority is immutable. A change to any of it is a different authority,
//! established anew, and everything issued under the old one stays with the
//! old one.

use std::cmp::Ordering;
use std::sync::Arc;

use intlify_authoring::{
    ArtifactKind, AuthoringArtifactReference, AuthoringBasis, ContextKind, OwnerIdentity,
    OwnerKind, PrimitiveError, SourceSnapshot, Token,
};
use intlify_authoring_identity::RegistryIdentity;

use super::action::{Action, ActionSet};
use super::failure::AuthorizationFailure;

/// A caller the host established.
///
/// Two principals are equal only when the host established them as one
/// identity. A matching display name is not the same principal, and a name
/// read from source or an artifact is never one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Principal(Token);

impl Principal {
    #[cfg_attr(
        not(any(test, feature = "test-authority")),
        expect(
            dead_code,
            reason = "only the test-owned entry establishes an authority in this phase"
        )
    )]
    pub(crate) fn new(id: &str) -> Result<Self, PrimitiveError> {
        Ok(Self(Token::new(id)?))
    }

    /// Borrow the host's identifier for this principal, safe to retain in
    /// provenance. It is not a credential.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// The host's binding of one application owner's registry chain.
///
/// The binding identity is the host's own; the owner and owning scope are
/// the chain's. An initialized binding names the chain's registry identity,
/// and an uninitialized one is an admitted new owner with no chain yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Destination {
    binding: Token,
    owner: OwnerIdentity,
    scope: Token,
    registry: Option<RegistryIdentity>,
}

impl Destination {
    #[cfg_attr(
        not(any(test, feature = "test-authority")),
        expect(
            dead_code,
            reason = "only the test-owned entry establishes an authority in this phase"
        )
    )]
    pub(crate) fn new(
        binding: &str,
        owner: OwnerIdentity,
        scope: &str,
        registry: Option<RegistryIdentity>,
    ) -> Result<Self, PrimitiveError> {
        Ok(Self {
            binding: Token::new(binding)?,
            owner,
            scope: Token::new(scope)?,
            registry,
        })
    }

    /// Borrow the host's binding identity.
    #[must_use]
    pub const fn binding(&self) -> &Token {
        &self.binding
    }

    /// Borrow the owner.
    #[must_use]
    pub const fn owner(&self) -> &OwnerIdentity {
        &self.owner
    }

    /// Borrow the owning scope.
    #[must_use]
    pub const fn scope(&self) -> &Token {
        &self.scope
    }

    /// Borrow the registry identity of the chain, absent before
    /// initialization.
    #[must_use]
    pub const fn registry(&self) -> Option<&RegistryIdentity> {
        self.registry.as_ref()
    }
}

/// Which named bound an authority or an authorization exhausted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AuthorityLimitKind {
    /// Principals one authority grants actions to.
    Principals,
    /// Source snapshots one authority holds as acquired.
    Sources,
    /// Registry anchors one authority accepts.
    Anchors,
    /// Confirmations one update request carries.
    Confirmations,
}

/// Inclusive upper bounds on what one authority holds and one request
/// carries.
///
/// Like the other limits of this workspace there is no `Default`: a caller
/// that does not know a bound decides one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthorityLimits {
    /// Principals one authority grants actions to.
    pub principals: u64,
    /// Source snapshots one authority holds as acquired.
    pub sources: u64,
    /// Registry anchors one authority accepts.
    pub anchors: u64,
    /// Confirmations one update request carries.
    pub confirmations: u64,
}

/// Why an authority could not be established.
///
/// Each variant names one rule, so a malformed setup is refused for a reason
/// rather than repaired into something else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstablishmentFailure {
    /// The destination's owner is not an application.
    NotApplication,
    /// The analysis context is not test-owned. This phase establishes
    /// test-owned authority only.
    ProductionContext,
    /// Two grant entries name one principal. Entries are never merged.
    DuplicatePrincipal,
    /// One grant entry repeats an action.
    RepeatedAction,
    /// An acquired snapshot belongs to another owner than the destination's.
    ForeignSource,
    /// Two acquired snapshots name one unit.
    DuplicateSource,
    /// An anchor does not name an `intent-registry` under the schema revision
    /// and specification this reader implements.
    AnchorKind,
    /// A named bound was exhausted.
    Limit(AuthorityLimitKind),
}

/// Everything one authority binds, before it is checked.
#[cfg_attr(
    not(any(test, feature = "test-authority")),
    expect(
        dead_code,
        reason = "only the test-owned entry establishes an authority in this phase"
    )
)]
pub(crate) struct Establishment {
    pub(crate) destination: Destination,
    pub(crate) context: AuthoringBasis,
    pub(crate) grants: Vec<(Principal, Vec<Action>)>,
    pub(crate) acquired: Vec<SourceSnapshot>,
    pub(crate) anchors: Vec<AuthoringArtifactReference>,
    pub(crate) development: bool,
}

#[derive(Debug)]
struct AuthorityState {
    destination: Destination,
    context: AuthoringBasis,
    grants: Box<[(Principal, ActionSet)]>,
    acquired: Box<[SourceSnapshot]>,
    anchors: Box<[AuthoringArtifactReference]>,
    development: bool,
}

/// One immutable authority a host established for a bounded invocation.
///
/// A clone is the same authority. Establishing another, even with the same
/// content, gives a different one: confirmations and permits issued under
/// one are not valid under another. Equality compares that identity, not
/// the content.
#[derive(Debug, Clone)]
pub struct LocalAuthority {
    state: Arc<AuthorityState>,
}

impl PartialEq for LocalAuthority {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.state, &other.state)
    }
}

impl Eq for LocalAuthority {}

impl LocalAuthority {
    /// Check an establishment and seal it into one authority.
    #[cfg_attr(
        not(any(test, feature = "test-authority")),
        expect(
            dead_code,
            reason = "only the test-owned entry establishes an authority in this phase"
        )
    )]
    pub(crate) fn establish(
        establishment: Establishment,
        limits: &AuthorityLimits,
    ) -> Result<Self, EstablishmentFailure> {
        let Establishment {
            destination,
            context,
            grants,
            mut acquired,
            anchors,
            development,
        } = establishment;
        if destination.owner().kind() != OwnerKind::Application {
            return Err(EstablishmentFailure::NotApplication);
        }
        if context.context_kind() != ContextKind::TestContext {
            return Err(EstablishmentFailure::ProductionContext);
        }
        for (count, bound, kind) in [
            (
                grants.len(),
                limits.principals,
                AuthorityLimitKind::Principals,
            ),
            (acquired.len(), limits.sources, AuthorityLimitKind::Sources),
            (anchors.len(), limits.anchors, AuthorityLimitKind::Anchors),
        ] {
            if count as u64 > bound {
                return Err(EstablishmentFailure::Limit(kind));
            }
        }

        let mut held = Vec::with_capacity(grants.len());
        for (principal, actions) in grants {
            let mut set = ActionSet::default();
            for action in actions {
                if !set.insert(action) {
                    return Err(EstablishmentFailure::RepeatedAction);
                }
            }
            held.push((principal, set));
        }
        held.sort_by(|left, right| left.0.cmp(&right.0));
        if held.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err(EstablishmentFailure::DuplicatePrincipal);
        }

        if acquired
            .iter()
            .any(|snapshot| snapshot.owner() != destination.owner())
        {
            return Err(EstablishmentFailure::ForeignSource);
        }
        acquired.sort_by(|left, right| left.unit().cmp(right.unit()));
        if acquired
            .windows(2)
            .any(|pair| pair[0].unit().cmp(pair[1].unit()) == Ordering::Equal)
        {
            return Err(EstablishmentFailure::DuplicateSource);
        }
        if anchors
            .iter()
            .any(|anchor| !anchor.is_current(ArtifactKind::IntentRegistry))
        {
            return Err(EstablishmentFailure::AnchorKind);
        }
        Ok(Self {
            state: Arc::new(AuthorityState {
                destination,
                context,
                grants: held.into_boxed_slice(),
                acquired: acquired.into_boxed_slice(),
                anchors: anchors.into_boxed_slice(),
                development,
            }),
        })
    }

    /// Borrow the destination.
    #[must_use]
    pub fn destination(&self) -> &Destination {
        &self.state.destination
    }

    /// Borrow the analysis context every admitted inventory is resolved
    /// against.
    #[must_use]
    pub fn context(&self) -> &AuthoringBasis {
        &self.state.context
    }

    /// Borrow the source snapshots the host acquired, in unit order.
    #[must_use]
    pub fn acquired(&self) -> &[SourceSnapshot] {
        &self.state.acquired
    }

    /// Borrow the registry anchors the host accepted for this invocation.
    #[must_use]
    pub fn anchors(&self) -> &[AuthoringArtifactReference] {
        &self.state.anchors
    }

    /// Return whether an explicitly enabled development session is active
    /// for this destination.
    #[must_use]
    pub fn development_session(&self) -> bool {
        self.state.development
    }

    /// Return the actions a principal holds, absent for a principal this
    /// authority did not establish.
    #[must_use]
    pub fn actions(&self, principal: &Principal) -> Option<ActionSet> {
        let grants = &self.state.grants;
        grants
            .binary_search_by(|(held, _)| held.cmp(principal))
            .ok()
            .map(|index| grants[index].1)
    }

    /// Select the caller of one invocation.
    pub fn invoke(&self, caller: &Principal) -> Result<Invocation<'_>, AuthorizationFailure> {
        let actions = self
            .actions(caller)
            .ok_or(AuthorizationFailure::UnknownCaller)?;
        Ok(Invocation {
            authority: self,
            caller: caller.clone(),
            actions,
        })
    }
}

/// One established caller acting under one authority.
#[derive(Debug, Clone)]
pub struct Invocation<'a> {
    authority: &'a LocalAuthority,
    caller: Principal,
    actions: ActionSet,
}

impl<'a> Invocation<'a> {
    /// Borrow the authority the caller acts under.
    #[must_use]
    pub const fn authority(&self) -> &'a LocalAuthority {
        self.authority
    }

    /// Borrow the caller.
    #[must_use]
    pub const fn caller(&self) -> &Principal {
        &self.caller
    }

    /// Refuse the invocation unless the caller holds an action.
    pub fn require(&self, action: Action) -> Result<(), AuthorizationFailure> {
        if self.actions.contains(action) {
            Ok(())
        } else {
            Err(AuthorizationFailure::Denied(action))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority::fixtures::{basis, chain_destination, establishment, limits, Chain};

    fn alice() -> Principal {
        Principal::new("alice").unwrap()
    }

    #[test]
    fn an_authority_holds_what_it_was_established_with() {
        let chain = Chain::load();
        let authority = LocalAuthority::establish(
            establishment(
                &chain,
                3,
                &[
                    ("bob", &[Action::ReadRegistry]),
                    ("alice", &[Action::UpdateRegistry, Action::AnalyzeSource]),
                ],
            ),
            &limits(),
        )
        .unwrap();
        assert_eq!(authority.destination(), &chain_destination());
        assert_eq!(authority.context(), &basis());
        let units: Vec<&str> = authority
            .acquired()
            .iter()
            .map(|snapshot| snapshot.unit().as_str())
            .collect();
        assert_eq!(units, ["checkout", "nav"]);
        assert_eq!(authority.anchors(), [chain.registry(2).reference()]);
        assert!(!authority.development_session());
        let held = authority.actions(&alice()).unwrap();
        assert_eq!(
            held.iter().collect::<Vec<_>>(),
            [Action::AnalyzeSource, Action::UpdateRegistry]
        );
        assert_eq!(authority.actions(&Principal::new("carol").unwrap()), None);
    }

    #[test]
    fn an_invocation_holds_only_its_callers_actions() {
        let chain = Chain::load();
        let authority = LocalAuthority::establish(
            establishment(&chain, 3, &[("alice", &[Action::UpdateRegistry])]),
            &limits(),
        )
        .unwrap();
        let invocation = authority.invoke(&alice()).unwrap();
        assert_eq!(invocation.caller(), &alice());
        assert_eq!(invocation.authority(), &authority);
        assert_eq!(invocation.require(Action::UpdateRegistry), Ok(()));
        // One grant implies no other.
        for action in [
            Action::AnalyzeSource,
            Action::ReadRegistry,
            Action::InitializeRegistry,
            Action::ResolveIdentity,
        ] {
            assert_eq!(
                invocation.require(action),
                Err(AuthorizationFailure::Denied(action))
            );
        }
        // A name the authority never established is not a caller, even one
        // spelled like a principal elsewhere.
        assert_eq!(
            authority.invoke(&Principal::new("mallory").unwrap()).err(),
            Some(AuthorizationFailure::UnknownCaller)
        );
    }

    #[test]
    fn an_authority_is_the_one_established_not_its_content() {
        let chain = Chain::load();
        let make = || {
            LocalAuthority::establish(establishment(&chain, 3, &[("alice", &[])]), &limits())
                .unwrap()
        };
        let (first, second) = (make(), make());
        assert_eq!(first.clone(), first);
        assert_ne!(first, second);
    }

    #[test]
    fn a_malformed_setup_is_refused_for_its_rule() {
        let chain = Chain::load();
        let refused = |change: &dyn Fn(&mut Establishment)| {
            let mut setup = establishment(&chain, 3, &[("alice", &[Action::ReadRegistry])]);
            change(&mut setup);
            LocalAuthority::establish(setup, &limits()).err()
        };
        assert_eq!(
            refused(&|setup| {
                setup.destination.owner =
                    OwnerIdentity::new(OwnerKind::Library, "storefront").unwrap();
            }),
            Some(EstablishmentFailure::NotApplication)
        );
        assert_eq!(
            refused(&|setup| {
                setup.grants.push((alice(), vec![Action::AnalyzeSource]));
            }),
            Some(EstablishmentFailure::DuplicatePrincipal)
        );
        assert_eq!(
            refused(&|setup| {
                setup.grants[0].1.push(Action::ReadRegistry);
            }),
            Some(EstablishmentFailure::RepeatedAction)
        );
        assert_eq!(
            refused(&|setup| {
                let first = setup.acquired[0].clone();
                setup.acquired.push(first);
            }),
            Some(EstablishmentFailure::DuplicateSource)
        );
        assert_eq!(
            refused(&|setup| {
                let source = setup.acquired[0].clone();
                setup.acquired[0] = SourceSnapshot::new(
                    OwnerIdentity::new(OwnerKind::Application, "elsewhere").unwrap(),
                    source.unit().as_str(),
                    source.revision().as_str(),
                    source.grammar().clone(),
                    source.byte_length(),
                    source.utf8_digest().as_str(),
                )
                .unwrap();
            }),
            Some(EstablishmentFailure::ForeignSource)
        );
        assert_eq!(
            refused(&|setup| {
                setup.anchors.push(chain.inventory(3).reference());
            }),
            Some(EstablishmentFailure::AnchorKind)
        );
    }

    #[test]
    fn only_a_test_owned_context_establishes_authority_in_this_phase() {
        let chain = Chain::load();
        let mut setup = establishment(&chain, 3, &[]);
        let basis: serde_json::Value = serde_json::to_value(&setup.context).unwrap();
        let mut production = basis.clone();
        production["contextKind"] = serde_json::json!("application-profile");
        setup.context = serde_json::from_value(production).unwrap();
        assert_eq!(
            LocalAuthority::establish(setup, &limits()).err(),
            Some(EstablishmentFailure::ProductionContext)
        );
    }

    #[test]
    fn every_bound_admits_its_exact_value_and_refuses_one_less() {
        let chain = Chain::load();
        let setup = || {
            let mut setup = establishment(
                &chain,
                3,
                &[
                    ("alice", &[Action::ReadRegistry]),
                    ("bob", &[Action::AnalyzeSource]),
                ],
            );
            setup.anchors.push(chain.registry(1).reference());
            setup
        };
        // Two principals, two sources and two anchors.
        for kind in [
            AuthorityLimitKind::Principals,
            AuthorityLimitKind::Sources,
            AuthorityLimitKind::Anchors,
        ] {
            let bound = |value: u64| {
                let mut bounds = limits();
                *match kind {
                    AuthorityLimitKind::Principals => &mut bounds.principals,
                    AuthorityLimitKind::Sources => &mut bounds.sources,
                    _ => &mut bounds.anchors,
                } = value;
                bounds
            };
            assert!(
                LocalAuthority::establish(setup(), &bound(2)).is_ok(),
                "{kind:?}"
            );
            assert_eq!(
                LocalAuthority::establish(setup(), &bound(1)).err(),
                Some(EstablishmentFailure::Limit(kind)),
                "{kind:?}"
            );
        }
    }
}
