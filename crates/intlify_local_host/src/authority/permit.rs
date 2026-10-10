// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Authorizing one exact registry write.
//!
//! Design 018 binds a publication permit to the complete checked request:
//! the authority it was given under, the publisher, the exact base and
//! inventory, the complete update and the result it produces, the
//! invocation mode, and the confirmation of every explicit decision. What is
//! authorized is a plan reconciliation already made, so a permit can never
//! stand in for design 016's checks: an input with a collision, a failed
//! unit, an unproven continuity or an unresolved choice has no plan to
//! authorize.
//!
//! A permit is local to its request and its authority. It has no
//! serialization and no public constructor, and checking it against another
//! request or a changed authority fails. Making it current is the host's
//! conditional commit, which re-checks the base and the authority at the
//! point of publication.

use intlify_authoring::{AdmittedInventory, AuthoringArtifactReference};
use intlify_authoring_identity::{AdmittedRegistry, Eligibility, IntentRegistrySnapshot, Plan};

use super::action::Action;
use super::confirmation::Confirmation;
use super::context::{AuthorityLimitKind, AuthorityLimits, Invocation, LocalAuthority, Principal};
use super::failure::AuthorizationFailure;

/// How an update was requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateMode {
    /// An explicit request from the caller. Proven decisions need nothing
    /// more; every explicit decision needs its confirmation.
    Manual,
    /// An update in an explicitly enabled development session. Only proven
    /// decisions are accepted; a plan with an explicit decision is manual
    /// work.
    Development,
}

/// One requested update: a plan reconciliation made, the inputs it was made
/// from, and what the caller brings to authorize it.
#[derive(Debug, Clone, Copy)]
pub struct UpdateRequest<'r> {
    /// The base the plan was made against.
    pub base: &'r AdmittedRegistry,
    /// The inventory the plan was made from.
    pub inventory: &'r AdmittedInventory,
    /// The plan.
    pub plan: &'r Plan,
    /// A confirmation for each explicit decision of the plan, and nothing
    /// else.
    pub confirmations: &'r [Confirmation],
    /// How the update was requested.
    pub mode: UpdateMode,
}

/// Why a permit does not authorize a publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermitMismatch {
    /// The authority in force is not the one the permit was given under.
    AuthorityChanged,
    /// The publication is not the request the permit was given for.
    OtherRequest,
}

/// The authorization of one exact update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdatePermit {
    authority: LocalAuthority,
    publisher: Principal,
    mode: UpdateMode,
    base: AuthoringArtifactReference,
    inventory: AuthoringArtifactReference,
    update: AuthoringArtifactReference,
    result: IntentRegistrySnapshot,
    confirmations: Box<[Confirmation]>,
}

impl UpdatePermit {
    /// Borrow the authority the permit was given under.
    #[must_use]
    pub const fn authority(&self) -> &LocalAuthority {
        &self.authority
    }

    /// Borrow the principal who may publish.
    #[must_use]
    pub const fn publisher(&self) -> &Principal {
        &self.publisher
    }

    /// Return how the update was requested.
    #[must_use]
    pub const fn mode(&self) -> UpdateMode {
        self.mode
    }

    /// Borrow the reference to the exact base.
    #[must_use]
    pub const fn base(&self) -> &AuthoringArtifactReference {
        &self.base
    }

    /// Borrow the reference to the inventory the plan was made from.
    #[must_use]
    pub const fn inventory(&self) -> &AuthoringArtifactReference {
        &self.inventory
    }

    /// Borrow the reference to the authorized update.
    #[must_use]
    pub const fn update(&self) -> &AuthoringArtifactReference {
        &self.update
    }

    /// Borrow the registry state the update produces, the one to publish.
    #[must_use]
    pub const fn result(&self) -> &IntentRegistrySnapshot {
        &self.result
    }

    /// Borrow the confirmations of the plan's explicit decisions, in
    /// decision order.
    #[must_use]
    pub fn confirmations(&self) -> &[Confirmation] {
        &self.confirmations
    }

    /// Check that this permit authorizes publishing this plan on this base
    /// under this authority.
    pub fn check(
        &self,
        authority: &LocalAuthority,
        base: &AdmittedRegistry,
        plan: &Plan,
    ) -> Result<(), PermitMismatch> {
        if self.authority != *authority {
            return Err(PermitMismatch::AuthorityChanged);
        }
        if self.base != base.reference() || self.update != plan.update().reference() {
            return Err(PermitMismatch::OtherRequest);
        }
        Ok(())
    }
}

/// The authorization of one initialization: one empty genesis for an
/// admitted new owner binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitializationPermit {
    authority: LocalAuthority,
    publisher: Principal,
    genesis: AuthoringArtifactReference,
    snapshot: IntentRegistrySnapshot,
}

impl InitializationPermit {
    /// Borrow the authority the permit was given under.
    #[must_use]
    pub const fn authority(&self) -> &LocalAuthority {
        &self.authority
    }

    /// Borrow the principal who may publish.
    #[must_use]
    pub const fn publisher(&self) -> &Principal {
        &self.publisher
    }

    /// Borrow the reference to the authorized genesis.
    #[must_use]
    pub const fn genesis(&self) -> &AuthoringArtifactReference {
        &self.genesis
    }

    /// Borrow the genesis to publish.
    #[must_use]
    pub const fn snapshot(&self) -> &IntentRegistrySnapshot {
        &self.snapshot
    }

    /// Check that this permit authorizes publishing this genesis under this
    /// authority.
    pub fn check(
        &self,
        authority: &LocalAuthority,
        genesis: &AdmittedRegistry,
    ) -> Result<(), PermitMismatch> {
        if self.authority != *authority {
            return Err(PermitMismatch::AuthorityChanged);
        }
        if self.genesis != genesis.reference() {
            return Err(PermitMismatch::OtherRequest);
        }
        Ok(())
    }
}

impl Invocation<'_> {
    /// Authorize one update a reconciliation planned.
    ///
    /// The checks run in design 018's stages: the caller's grant, the inputs
    /// against the authority, the plan against the inputs, the invocation
    /// mode, and the confirmation of every explicit decision.
    pub fn authorize_update(
        &self,
        request: &UpdateRequest<'_>,
        limits: &AuthorityLimits,
    ) -> Result<UpdatePermit, AuthorizationFailure> {
        self.require(Action::UpdateRegistry)?;
        if request.confirmations.len() as u64 > limits.confirmations {
            return Err(AuthorizationFailure::Limit(
                AuthorityLimitKind::Confirmations,
            ));
        }
        let authority = self.authority();
        authority.bind_registry(request.base)?;
        authority.bind_inventory(request.inventory)?;
        let (base, inventory) = (request.base.reference(), request.inventory.reference());
        let planned = request.plan.update();
        if planned.update().base() != &base || planned.update().inventory() != &inventory {
            return Err(AuthorizationFailure::PlanMismatch);
        }
        if request.mode == UpdateMode::Development {
            if !authority.development_session() {
                return Err(AuthorizationFailure::SessionInactive);
            }
            if request.plan.requires_confirmation() {
                return Err(AuthorizationFailure::NotAutomatic);
            }
        }
        let confirmations = self.matched(request, &base, &inventory)?;
        Ok(UpdatePermit {
            authority: authority.clone(),
            publisher: self.caller().clone(),
            mode: request.mode,
            base,
            inventory,
            update: planned.reference(),
            result: request.plan.result().clone(),
            confirmations,
        })
    }

    /// Pair every explicit decision of the plan with its confirmation, and
    /// refuse a confirmation that pairs with none.
    fn matched(
        &self,
        request: &UpdateRequest<'_>,
        base: &AuthoringArtifactReference,
        inventory: &AuthoringArtifactReference,
    ) -> Result<Box<[Confirmation]>, AuthorizationFailure> {
        let plan = request.plan;
        let decisions = plan.update().update().decisions();
        let mut used = vec![false; request.confirmations.len()];
        let mut matched = Vec::new();
        for (decision, (id, eligibility)) in decisions.iter().zip(plan.eligibility()) {
            if *eligibility != Eligibility::RequiresConfirmation {
                continue;
            }
            // Missing unless a confirmation names this ID; otherwise why the
            // last one that names it fails. The first one that matches is used.
            let mut refused = AuthorizationFailure::ConfirmationMissing(id.clone());
            let mut found = None;
            for (index, confirmation) in request.confirmations.iter().enumerate() {
                if confirmation.decision().intent_id() != id {
                    continue;
                }
                if confirmation.authority() != self.authority() {
                    refused = AuthorizationFailure::ConfirmationStale;
                } else if confirmation.base() != base
                    || confirmation.inventory() != inventory
                    || confirmation.decision() != decision
                {
                    refused = AuthorizationFailure::ConfirmationMismatch;
                } else {
                    found = Some(index);
                    break;
                }
            }
            let index = found.ok_or(refused)?;
            used[index] = true;
            matched.push(request.confirmations[index].clone());
        }
        if used.contains(&false) {
            return Err(AuthorizationFailure::UnusedConfirmation);
        }
        Ok(matched.into_boxed_slice())
    }

    /// Authorize one initialization: an empty genesis for this authority's
    /// admitted new owner binding.
    ///
    /// The binding has to be uninitialized in the authority the host
    /// established; the host checks again that it still is when it commits.
    pub fn authorize_initialization(
        &self,
        genesis: &AdmittedRegistry,
    ) -> Result<InitializationPermit, AuthorizationFailure> {
        self.require(Action::InitializeRegistry)?;
        let authority = self.authority();
        let destination = authority.destination();
        if destination.registry().is_some() {
            return Err(AuthorizationFailure::AlreadyInitialized);
        }
        let snapshot = genesis.snapshot();
        if !snapshot.is_genesis() {
            return Err(AuthorizationFailure::NotGenesis);
        }
        if snapshot.owner() != destination.owner() {
            return Err(AuthorizationFailure::OwnerMismatch);
        }
        if snapshot.scope() != destination.scope() {
            return Err(AuthorizationFailure::ScopeMismatch);
        }
        Ok(InitializationPermit {
            authority: authority.clone(),
            publisher: self.caller().clone(),
            genesis: genesis.reference(),
            snapshot: snapshot.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use intlify_authoring::{AuthoringArtifact, OwnerIdentity, OwnerKind};
    use intlify_authoring_identity::{
        admit_registry, ExplicitBasis, ExplicitDecision, IdentityDecision, IntentRegistrySnapshot,
        RegistryArtifact, RegistryIdentity,
    };

    use super::*;
    use crate::authority::context::{Destination, Establishment};
    use crate::authority::fixtures::{
        allocation_plan, basis, committed, establishment, id_of, identity_limits, limits, owner,
        plan as planned, restoration_plan, restore, the_edit, Chain,
    };

    const PUBLISHER: &[Action] = &[Action::UpdateRegistry, Action::ResolveIdentity];

    fn alice() -> Principal {
        Principal::new("alice").unwrap()
    }

    fn established(setup: Establishment) -> LocalAuthority {
        LocalAuthority::establish(setup, &limits()).unwrap()
    }

    /// An authority over inventory `n` with the registry before it as its
    /// anchor, and alice holding `actions`.
    fn over(chain: &Chain, n: usize, actions: &[Action]) -> LocalAuthority {
        established(establishment(chain, n, &[("alice", actions)]))
    }

    fn manual<'r>(
        chain: &'r Chain,
        plan: &'r Plan,
        confirmations: &'r [Confirmation],
    ) -> UpdateRequest<'r> {
        UpdateRequest {
            base: chain.registry(2),
            inventory: chain.inventory(3),
            plan,
            confirmations,
            mode: UpdateMode::Manual,
        }
    }

    fn confirm(
        authority: &LocalAuthority,
        chain: &Chain,
        decision: &ExplicitDecision,
    ) -> Confirmation {
        authority
            .invoke(&alice())
            .unwrap()
            .confirm(chain.registry(2), chain.inventory(3), decision)
            .unwrap()
    }

    #[test]
    fn an_explicit_decision_is_published_only_with_its_confirmation() {
        let chain = Chain::load();
        let plan = restoration_plan(&chain);
        let authority = over(&chain, 3, PUBLISHER);
        let invocation = authority.invoke(&alice()).unwrap();
        assert_eq!(
            invocation.authorize_update(&manual(&chain, &plan, &[]), &limits()),
            Err(AuthorizationFailure::ConfirmationMissing(id_of(3, 1)))
        );
        let confirmation = confirm(&authority, &chain, &restore(&chain));
        let permit = invocation
            .authorize_update(
                &manual(&chain, &plan, std::slice::from_ref(&confirmation)),
                &limits(),
            )
            .unwrap();
        assert_eq!(permit.authority(), &authority);
        assert_eq!(permit.publisher(), &alice());
        assert_eq!(permit.mode(), UpdateMode::Manual);
        assert_eq!(permit.base(), &chain.registry(2).reference());
        assert_eq!(permit.inventory(), &chain.inventory(3).reference());
        assert_eq!(permit.update(), &plan.update().reference());
        assert_eq!(permit.result(), plan.result());
        assert_eq!(permit.confirmations(), [confirmation]);
    }

    #[test]
    fn a_proven_plan_needs_no_confirmation_and_takes_none() {
        let chain = Chain::load();
        let plan = allocation_plan(&chain);
        let authority = over(&chain, 1, PUBLISHER);
        let request = |confirmations: &[Confirmation]| {
            authority.invoke(&alice()).unwrap().authorize_update(
                &UpdateRequest {
                    base: chain.registry(0),
                    inventory: chain.inventory(1),
                    plan: &plan,
                    confirmations,
                    mode: UpdateMode::Manual,
                },
                &limits(),
            )
        };
        let permit = request(&[]).unwrap();
        assert!(permit.confirmations().is_empty());
        // A confirmation of a decision the plan does not hold is refused,
        // not ignored.
        let other = over(&chain, 3, PUBLISHER);
        let stray = confirm(&other, &chain, &restore(&chain));
        assert_eq!(
            request(std::slice::from_ref(&stray)),
            Err(AuthorizationFailure::UnusedConfirmation)
        );
    }

    #[test]
    fn a_confirmation_counts_only_for_its_exact_choice_under_this_authority() {
        let chain = Chain::load();
        let plan = restoration_plan(&chain);
        let authority = over(&chain, 3, PUBLISHER);
        let invocation = authority.invoke(&alice()).unwrap();
        let attempt = |confirmations: &[Confirmation]| {
            invocation.authorize_update(&manual(&chain, &plan, confirmations), &limits())
        };
        // Given under another authority, even one with the same content.
        let earlier = over(&chain, 3, PUBLISHER);
        assert_eq!(
            attempt(&[confirm(&earlier, &chain, &restore(&chain))]),
            Err(AuthorizationFailure::ConfirmationStale)
        );
        // The same restoration with another reason is another choice.
        let restored = committed(3, 1);
        let reworded = ExplicitDecision::new(
            chain.registry(2).reference(),
            chain.inventory(3).reference(),
            IdentityDecision::restoration(
                restored.intent_id().clone(),
                restored.from().unwrap().clone(),
                restored.to().unwrap().clone(),
                ExplicitBasis::new("Someone else's reason.").unwrap(),
            ),
        )
        .unwrap();
        assert_eq!(
            attempt(&[confirm(&authority, &chain, &reworded)]),
            Err(AuthorizationFailure::ConfirmationMismatch)
        );
        // The same choice confirmed against another base.
        let anchored = established({
            let mut setup = establishment(&chain, 3, &[("alice", PUBLISHER)]);
            setup.anchors.push(chain.registry(1).reference());
            setup
        });
        let elsewhere = anchored
            .invoke(&alice())
            .unwrap()
            .confirm(
                chain.registry(1),
                chain.inventory(3),
                &ExplicitDecision::new(
                    chain.registry(1).reference(),
                    chain.inventory(3).reference(),
                    committed(3, 1),
                )
                .unwrap(),
            )
            .unwrap();
        let plan_here = restoration_plan(&chain);
        let anchored_invocation = anchored.invoke(&alice()).unwrap();
        assert_eq!(
            anchored_invocation.authorize_update(
                &manual(&chain, &plan_here, std::slice::from_ref(&elsewhere)),
                &limits()
            ),
            Err(AuthorizationFailure::ConfirmationMismatch)
        );
        // One confirmation too many, even an identical one.
        let confirmation = confirm(&authority, &chain, &restore(&chain));
        assert_eq!(
            attempt(&[confirmation.clone(), confirmation]),
            Err(AuthorizationFailure::UnusedConfirmation)
        );
    }

    #[test]
    fn a_development_update_needs_the_session_and_only_proven_decisions() {
        let chain = Chain::load();
        let session = |n: usize, development: bool| {
            let mut setup = establishment(&chain, n, &[("alice", PUBLISHER)]);
            setup.development = development;
            established(setup)
        };
        let proven = allocation_plan(&chain);
        let develop = |authority: &LocalAuthority| {
            authority.invoke(&alice()).unwrap().authorize_update(
                &UpdateRequest {
                    base: chain.registry(0),
                    inventory: chain.inventory(1),
                    plan: &proven,
                    confirmations: &[],
                    mode: UpdateMode::Development,
                },
                &limits(),
            )
        };
        assert_eq!(
            develop(&session(1, false)),
            Err(AuthorizationFailure::SessionInactive)
        );
        let permit = develop(&session(1, true)).unwrap();
        assert_eq!(permit.mode(), UpdateMode::Development);
        // A restoration is manual work, confirmed or not.
        let authority = session(3, true);
        let explicit = restoration_plan(&chain);
        let confirmation = confirm(&authority, &chain, &restore(&chain));
        assert_eq!(
            authority.invoke(&alice()).unwrap().authorize_update(
                &UpdateRequest {
                    mode: UpdateMode::Development,
                    ..manual(&chain, &explicit, std::slice::from_ref(&confirmation))
                },
                &limits()
            ),
            Err(AuthorizationFailure::NotAutomatic)
        );
    }

    #[test]
    fn an_update_is_authorized_only_for_its_grant_inputs_and_plan() {
        let chain = Chain::load();
        let plan = restoration_plan(&chain);
        // Every other grant together does not publish.
        let reader = over(
            &chain,
            3,
            &[
                Action::AnalyzeSource,
                Action::ReadRegistry,
                Action::InitializeRegistry,
                Action::ResolveIdentity,
            ],
        );
        assert_eq!(
            reader
                .invoke(&alice())
                .unwrap()
                .authorize_update(&manual(&chain, &plan, &[]), &limits()),
            Err(AuthorizationFailure::Denied(Action::UpdateRegistry))
        );
        // A base the authority did not accept, checked before the
        // inventory.
        let elsewhere = over(&chain, 2, PUBLISHER);
        assert_eq!(
            elsewhere
                .invoke(&alice())
                .unwrap()
                .authorize_update(&manual(&chain, &plan, &[]), &limits()),
            Err(AuthorizationFailure::Unanchored)
        );
        // An inventory the host did not acquire.
        let unacquired = established({
            let mut setup = establishment(&chain, 3, &[("alice", PUBLISHER)]);
            setup.acquired.remove(0);
            setup
        });
        assert_eq!(
            unacquired
                .invoke(&alice())
                .unwrap()
                .authorize_update(&manual(&chain, &plan, &[]), &limits()),
            Err(AuthorizationFailure::SourceNotAcquired)
        );
        // A plan made for another base, and one made from another inventory.
        let authority = over(&chain, 3, PUBLISHER);
        let other_base = allocation_plan(&chain);
        assert_eq!(
            authority
                .invoke(&alice())
                .unwrap()
                .authorize_update(&manual(&chain, &other_base, &[]), &limits()),
            Err(AuthorizationFailure::PlanMismatch)
        );
        let from_one = established({
            let mut setup = establishment(&chain, 3, &[("alice", PUBLISHER)]);
            setup.anchors = vec![chain.registry(1).reference()];
            setup
        });
        let other_inventory = planned(&chain, (1, 2), &[the_edit(2)], &[], &[id_of(2, 2)]);
        assert_eq!(
            from_one.invoke(&alice()).unwrap().authorize_update(
                &UpdateRequest {
                    base: chain.registry(1),
                    inventory: chain.inventory(3),
                    plan: &other_inventory,
                    confirmations: &[],
                    mode: UpdateMode::Manual,
                },
                &limits()
            ),
            Err(AuthorizationFailure::PlanMismatch)
        );
    }

    #[test]
    fn the_confirmations_a_request_carries_are_bounded() {
        let chain = Chain::load();
        let plan = restoration_plan(&chain);
        let authority = over(&chain, 3, PUBLISHER);
        let confirmation = confirm(&authority, &chain, &restore(&chain));
        let bounded = |confirmations| AuthorityLimits {
            confirmations,
            ..limits()
        };
        let request = manual(&chain, &plan, std::slice::from_ref(&confirmation));
        let invocation = authority.invoke(&alice()).unwrap();
        assert!(invocation.authorize_update(&request, &bounded(1)).is_ok());
        assert_eq!(
            invocation.authorize_update(&request, &bounded(0)),
            Err(AuthorizationFailure::Limit(
                AuthorityLimitKind::Confirmations
            ))
        );
    }

    #[test]
    fn a_permit_covers_its_own_request_under_its_own_authority_only() {
        let chain = Chain::load();
        let plan = restoration_plan(&chain);
        let authority = over(&chain, 3, PUBLISHER);
        let confirmation = confirm(&authority, &chain, &restore(&chain));
        let permit = authority
            .invoke(&alice())
            .unwrap()
            .authorize_update(
                &manual(&chain, &plan, std::slice::from_ref(&confirmation)),
                &limits(),
            )
            .unwrap();
        assert_eq!(permit.check(&authority, chain.registry(2), &plan), Ok(()));
        // The authority changed: the same content established again.
        let again = over(&chain, 3, PUBLISHER);
        assert_eq!(
            permit.check(&again, chain.registry(2), &plan),
            Err(PermitMismatch::AuthorityChanged)
        );
        // The same plan on another base, and another plan on the same base.
        assert_eq!(
            permit.check(&authority, chain.registry(1), &plan),
            Err(PermitMismatch::OtherRequest)
        );
        assert_eq!(
            permit.check(&authority, chain.registry(2), &allocation_plan(&chain)),
            Err(PermitMismatch::OtherRequest)
        );
    }

    /// An authority over an admitted new owner binding, with no chain yet.
    fn new_owner(actions: &[Action]) -> LocalAuthority {
        established(Establishment {
            destination: Destination::new("storefront-registry", owner(), "storefront-web", None)
                .unwrap(),
            context: basis(),
            grants: vec![(alice(), actions.to_vec())],
            acquired: vec![],
            anchors: vec![],
            development: false,
        })
    }

    fn genesis_of(
        owner: OwnerIdentity,
        scope: &str,
    ) -> intlify_authoring_identity::AdmittedRegistry {
        let snapshot = IntentRegistrySnapshot::genesis(
            owner,
            scope,
            RegistryIdentity::retained("0123456789abcdef0123456789abcdef").unwrap(),
        )
        .unwrap();
        let sealed = RegistryArtifact::seal(snapshot).unwrap();
        admit_registry(&serde_json::to_vec(&sealed).unwrap(), &identity_limits()).unwrap()
    }

    #[test]
    fn an_initialization_is_one_empty_genesis_for_an_uninitialized_binding() {
        let chain = Chain::load();
        let authority = new_owner(&[Action::InitializeRegistry]);
        let invocation = authority.invoke(&alice()).unwrap();
        let genesis = genesis_of(owner(), "storefront-web");
        let permit = invocation.authorize_initialization(&genesis).unwrap();
        assert_eq!(permit.authority(), &authority);
        assert_eq!(permit.publisher(), &alice());
        assert_eq!(permit.genesis(), &genesis.reference());
        assert_eq!(permit.snapshot(), genesis.snapshot());
        assert_eq!(permit.check(&authority, &genesis), Ok(()));
        assert_eq!(
            permit.check(&new_owner(&[Action::InitializeRegistry]), &genesis),
            Err(PermitMismatch::AuthorityChanged)
        );
        assert_eq!(
            permit.check(&authority, chain.registry(0)),
            Err(PermitMismatch::OtherRequest)
        );

        // Not a genesis, or the genesis of another owner or scope.
        assert_eq!(
            invocation.authorize_initialization(chain.registry(1)).err(),
            Some(AuthorizationFailure::NotGenesis)
        );
        assert_eq!(
            invocation
                .authorize_initialization(&genesis_of(
                    OwnerIdentity::new(OwnerKind::Application, "other-shop").unwrap(),
                    "storefront-web"
                ))
                .err(),
            Some(AuthorizationFailure::OwnerMismatch)
        );
        assert_eq!(
            invocation
                .authorize_initialization(&genesis_of(owner(), "storefront-admin"))
                .err(),
            Some(AuthorizationFailure::ScopeMismatch)
        );
    }

    #[test]
    fn initialization_needs_its_own_grant_and_an_uninitialized_binding() {
        let chain = Chain::load();
        // Every other grant together does not initialize.
        let others = new_owner(&[
            Action::AnalyzeSource,
            Action::ReadRegistry,
            Action::UpdateRegistry,
            Action::ResolveIdentity,
        ]);
        assert_eq!(
            others
                .invoke(&alice())
                .unwrap()
                .authorize_initialization(chain.registry(0))
                .err(),
            Some(AuthorizationFailure::Denied(Action::InitializeRegistry))
        );
        // A binding that already has its chain is never initialized again.
        let existing = over(&chain, 3, &[Action::InitializeRegistry]);
        assert_eq!(
            existing
                .invoke(&alice())
                .unwrap()
                .authorize_initialization(chain.registry(0))
                .err(),
            Some(AuthorizationFailure::AlreadyInitialized)
        );
    }
}
