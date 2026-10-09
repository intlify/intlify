// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Explicit identity decisions, bound to the inputs they were made for.
//!
//! An explicit decision records a historical choice that the evidence does
//! not prove: an `explicit` continuation or allocation, or a `restore`.
//! Reconciliation takes it as data. It checks that the decision was made for
//! exactly this base and this inventory and that it fits them, and marks the
//! result as needing confirmation. Whether the actor may make the choice is
//! 018's question, answered by the host before publication, never here.

use intlify_authoring::AuthoringArtifactReference;

use crate::registry::{AllocationBasis, ContinuationBasis, IdentityDecision};

/// The decision handed in as explicit is one the evidence has to prove
/// instead: a `verified-edit`, `unchanged-snapshot` or `confirmed-new`
/// basis, or a retirement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotExplicit;

/// One explicit identity decision, with the exact base and inventory it was
/// made for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplicitDecision {
    base: AuthoringArtifactReference,
    inventory: AuthoringArtifactReference,
    decision: IdentityDecision,
}

impl ExplicitDecision {
    /// Bind one explicit decision to the base and inventory it was made for.
    pub fn new(
        base: AuthoringArtifactReference,
        inventory: AuthoringArtifactReference,
        decision: IdentityDecision,
    ) -> Result<Self, NotExplicit> {
        let explicit = match &decision {
            IdentityDecision::Continue(continuation) => {
                matches!(continuation.basis(), ContinuationBasis::Explicit(_))
            }
            IdentityDecision::Allocate(allocation) => {
                matches!(allocation.basis(), AllocationBasis::Explicit(_))
            }
            IdentityDecision::Restore(_) => true,
            IdentityDecision::Retire(_) => false,
        };
        if !explicit {
            return Err(NotExplicit);
        }
        Ok(Self {
            base,
            inventory,
            decision,
        })
    }

    /// Borrow the base registry the decision was made against.
    #[must_use]
    pub const fn base(&self) -> &AuthoringArtifactReference {
        &self.base
    }

    /// Borrow the inventory the decision was made from.
    #[must_use]
    pub const fn inventory(&self) -> &AuthoringArtifactReference {
        &self.inventory
    }

    /// Borrow the decision.
    #[must_use]
    pub const fn decision(&self) -> &IdentityDecision {
        &self.decision
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::fixtures::{id, Chain};
    use crate::registry::ExplicitBasis;

    #[test]
    fn only_a_choice_the_evidence_does_not_prove_is_explicit() {
        let chain = Chain::load();
        let (base, inventory) = (
            chain.registry(1).reference(),
            chain.inventory(2).reference(),
        );
        let entries = chain.registry(1).snapshot().entries();
        let pay = &entries[0];
        let current = chain.inventory(2).inventory().declarations();
        let to = current[0].occurrence().clone();
        let reason = || ExplicitBasis::new("Chosen by hand.").unwrap();
        let bind = |decision| ExplicitDecision::new(base.clone(), inventory.clone(), decision);

        let explicit = [
            IdentityDecision::continuation(
                pay.intent_id().clone(),
                pay.declaration().clone(),
                to.clone(),
                ContinuationBasis::Explicit(reason()),
            ),
            IdentityDecision::allocation(
                id(&"f".repeat(32)),
                to.clone(),
                AllocationBasis::Explicit(reason()),
            ),
            IdentityDecision::restoration(
                pay.intent_id().clone(),
                pay.declaration().clone(),
                to.clone(),
                reason(),
            ),
        ];
        for decision in explicit {
            let bound = bind(decision.clone()).unwrap();
            assert_eq!(bound.decision(), &decision);
            assert_eq!(bound.base(), &base);
            assert_eq!(bound.inventory(), &inventory);
        }

        let proven = [
            IdentityDecision::continuation(
                pay.intent_id().clone(),
                pay.declaration().clone(),
                to.clone(),
                ContinuationBasis::unchanged_snapshot(),
            ),
            IdentityDecision::allocation(id(&"f".repeat(32)), to, AllocationBasis::confirmed_new()),
            IdentityDecision::retirement(pay.intent_id().clone(), pay.declaration().clone()),
        ];
        for decision in proven {
            assert_eq!(bind(decision), Err(NotExplicit));
        }
    }
}
