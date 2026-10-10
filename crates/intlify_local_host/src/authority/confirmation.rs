// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Confirming one explicit identity choice.
//!
//! An explicit decision records a historical choice the evidence does not
//! prove: an `explicit` continuation or allocation, or a `restore`. Design
//! 018 lets it count only when a principal holding `resolve-identity`
//! confirms it through the host, for the exact base, inventory and action
//! it was made for, under the authority in force. The serialized reason is
//! data to check, never a confirmation, and a confirmation waives none of
//! design 016's checks: the plan that carries the decision still has to
//! hold on its own.
//!
//! A confirmation precedes the plan it is used in, so it names the base and
//! the inventory, never a plan or result that does not exist yet.

use intlify_authoring::{AdmittedInventory, AuthoringArtifactReference};
use intlify_authoring_identity::{AdmittedRegistry, ExplicitDecision, IdentityDecision};

use super::action::Action;
use super::context::{Invocation, LocalAuthority, Principal};
use super::failure::AuthorizationFailure;

/// One explicit identity choice, confirmed under one authority.
///
/// A confirmation exists only as this in-process value: it has no
/// serialization and no public constructor, so neither a label nor a copy of
/// its fields is one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirmation {
    authority: LocalAuthority,
    confirmer: Principal,
    base: AuthoringArtifactReference,
    inventory: AuthoringArtifactReference,
    decision: IdentityDecision,
}

impl Confirmation {
    /// Borrow the authority it was given under.
    #[must_use]
    pub const fn authority(&self) -> &LocalAuthority {
        &self.authority
    }

    /// Borrow the principal who confirmed the choice.
    #[must_use]
    pub const fn confirmer(&self) -> &Principal {
        &self.confirmer
    }

    /// Borrow the reference to the base the choice was made against.
    #[must_use]
    pub const fn base(&self) -> &AuthoringArtifactReference {
        &self.base
    }

    /// Borrow the reference to the inventory the choice was made from.
    #[must_use]
    pub const fn inventory(&self) -> &AuthoringArtifactReference {
        &self.inventory
    }

    /// Borrow the confirmed decision: its ID, occurrences, basis and reason.
    #[must_use]
    pub const fn decision(&self) -> &IdentityDecision {
        &self.decision
    }
}

impl Invocation<'_> {
    /// Confirm one explicit decision for exactly the base and inventory it
    /// was made for.
    pub fn confirm(
        &self,
        base: &AdmittedRegistry,
        inventory: &AdmittedInventory,
        decision: &ExplicitDecision,
    ) -> Result<Confirmation, AuthorizationFailure> {
        self.require(Action::ResolveIdentity)?;
        let authority = self.authority();
        authority.bind_registry(base)?;
        authority.bind_inventory(inventory)?;
        let (base, inventory) = (base.reference(), inventory.reference());
        if decision.base() != &base || decision.inventory() != &inventory {
            return Err(AuthorizationFailure::UnboundDecision);
        }
        Ok(Confirmation {
            authority: authority.clone(),
            confirmer: self.caller().clone(),
            base,
            inventory,
            decision: decision.decision().clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority::fixtures::{committed, establishment, limits, restore, Chain};

    fn confirmer(chain: &Chain, actions: &[Action]) -> LocalAuthority {
        LocalAuthority::establish(establishment(chain, 3, &[("carol", actions)]), &limits())
            .unwrap()
    }

    fn carol() -> Principal {
        Principal::new("carol").unwrap()
    }

    #[test]
    fn a_confirmation_binds_the_choice_to_its_inputs_and_authority() {
        let chain = Chain::load();
        let authority = confirmer(&chain, &[Action::ResolveIdentity]);
        let restore = restore(&chain);
        let confirmation = authority
            .invoke(&carol())
            .unwrap()
            .confirm(chain.registry(2), chain.inventory(3), &restore)
            .unwrap();
        assert_eq!(confirmation.authority(), &authority);
        assert_eq!(confirmation.confirmer(), &carol());
        assert_eq!(confirmation.base(), &chain.registry(2).reference());
        assert_eq!(confirmation.inventory(), &chain.inventory(3).reference());
        assert_eq!(confirmation.decision(), &committed(3, 1));
    }

    #[test]
    fn only_resolve_identity_confirms_and_only_for_the_inputs_it_names() {
        let chain = Chain::load();
        let restore = restore(&chain);
        // Every other grant together still confirms nothing.
        let without = confirmer(
            &chain,
            &[
                Action::AnalyzeSource,
                Action::ReadRegistry,
                Action::InitializeRegistry,
                Action::UpdateRegistry,
            ],
        );
        assert_eq!(
            without
                .invoke(&carol())
                .unwrap()
                .confirm(chain.registry(2), chain.inventory(3), &restore)
                .err(),
            Some(AuthorizationFailure::Denied(Action::ResolveIdentity))
        );
        // A decision made against another base or from another inventory
        // is not confirmed for this one.
        let authority = confirmer(&chain, &[Action::ResolveIdentity]);
        let elsewhere = [
            ExplicitDecision::new(
                chain.registry(1).reference(),
                chain.inventory(3).reference(),
                committed(3, 1),
            )
            .unwrap(),
            ExplicitDecision::new(
                chain.registry(2).reference(),
                chain.inventory(2).reference(),
                committed(3, 1),
            )
            .unwrap(),
        ];
        for decision in &elsewhere {
            assert_eq!(
                authority
                    .invoke(&carol())
                    .unwrap()
                    .confirm(chain.registry(2), chain.inventory(3), decision)
                    .err(),
                Some(AuthorizationFailure::UnboundDecision)
            );
        }
        // Inputs outside the authority are refused before the decision is
        // read: here registry 1 is no anchor.
        assert_eq!(
            authority
                .invoke(&carol())
                .unwrap()
                .confirm(chain.registry(1), chain.inventory(3), &elsewhere[0])
                .err(),
            Some(AuthorizationFailure::Unanchored)
        );
    }

    #[test]
    fn an_inventory_outside_the_scope_is_refused_before_the_decision() {
        let chain = Chain::load();
        let authority = confirmer(&chain, &[Action::ResolveIdentity]);
        // Inventory 2 holds checkout revision 2, which was not acquired.
        let decision = ExplicitDecision::new(
            chain.registry(2).reference(),
            chain.inventory(2).reference(),
            committed(3, 1),
        )
        .unwrap();
        assert_eq!(
            authority
                .invoke(&carol())
                .unwrap()
                .confirm(chain.registry(2), chain.inventory(2), &decision)
                .err(),
            Some(AuthorizationFailure::SourceNotAcquired)
        );
    }
}
