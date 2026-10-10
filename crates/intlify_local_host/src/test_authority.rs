// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! An explicitly test-owned local authority.
//!
//! This module is compiled only under the non-default `test-authority`
//! feature, so an ordinary build has no way to establish an authority at
//! all. Design 018's pure core runs against independently established test
//! authority; establishing production authority needs checked 015 inputs and
//! a 029 host adapter, which a later plan supplies.
//!
//! Test-owned describes how the authority was established, not what runs.
//! The grants, the bindings and the confirmations are checked by the real
//! evaluator, and the authority still admits only a test-owned analysis
//! context.

use intlify_authoring::{
    AuthoringArtifactReference, AuthoringBasis, OwnerIdentity, PrimitiveError, SourceSnapshot,
};
use intlify_authoring_identity::RegistryIdentity;

use crate::authority::{
    Action, AuthorityLimits, Destination, Establishment, EstablishmentFailure, LocalAuthority,
    Principal,
};

/// A test-owned authority being set up, as a host's trusted setup path
/// would set one up.
pub struct TestAuthority {
    setup: Establishment,
}

impl TestAuthority {
    /// Name one principal, as the host's session mechanism would establish
    /// one.
    pub fn principal(id: &str) -> Result<Principal, PrimitiveError> {
        Principal::new(id)
    }

    /// Bind one owner's registry destination: with the chain's registry
    /// identity once initialized, or without one for an admitted new owner.
    pub fn destination(
        binding: &str,
        owner: OwnerIdentity,
        scope: &str,
        registry: Option<RegistryIdentity>,
    ) -> Result<Destination, PrimitiveError> {
        Destination::new(binding, owner, scope, registry)
    }

    /// Start an authority for one destination and one analysis context.
    #[must_use]
    pub fn new(destination: Destination, context: AuthoringBasis) -> Self {
        Self {
            setup: Establishment {
                destination,
                context,
                grants: Vec::new(),
                acquired: Vec::new(),
                anchors: Vec::new(),
                development: false,
            },
        }
    }

    /// Grant a principal its actions. Each call is one grant entry.
    #[must_use]
    pub fn grant(
        mut self,
        principal: &Principal,
        actions: impl IntoIterator<Item = Action>,
    ) -> Self {
        self.setup
            .grants
            .push((principal.clone(), actions.into_iter().collect()));
        self
    }

    /// Record one source snapshot the host acquired for the invocation.
    #[must_use]
    pub fn acquired(mut self, snapshot: SourceSnapshot) -> Self {
        self.setup.acquired.push(snapshot);
        self
    }

    /// Accept one registry snapshot as an anchor for the invocation.
    #[must_use]
    pub fn anchor(mut self, registry: AuthoringArtifactReference) -> Self {
        self.setup.anchors.push(registry);
        self
    }

    /// Enable a development session for the destination.
    #[must_use]
    pub fn development_session(mut self) -> Self {
        self.setup.development = true;
        self
    }

    /// Check the setup and establish the authority.
    pub fn establish(
        self,
        limits: &AuthorityLimits,
    ) -> Result<LocalAuthority, EstablishmentFailure> {
        LocalAuthority::establish(self.setup, limits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority::fixtures::{basis, chain_destination, limits, Chain};

    #[test]
    fn the_test_owned_entry_sets_up_exactly_what_it_is_given() {
        let chain = Chain::load();
        let alice = TestAuthority::principal("alice").unwrap();
        let checkout = chain.inventory(3).inventory().units()[0].source().clone();
        let authority = TestAuthority::new(chain_destination(), basis())
            .grant(&alice, [Action::AnalyzeSource, Action::UpdateRegistry])
            .acquired(checkout.clone())
            .anchor(chain.registry(2).reference())
            .development_session()
            .establish(&limits())
            .unwrap();
        assert_eq!(authority.destination(), &chain_destination());
        assert_eq!(authority.acquired(), [checkout]);
        assert_eq!(authority.anchors(), [chain.registry(2).reference()]);
        assert!(authority.development_session());
        assert_eq!(
            authority
                .actions(&alice)
                .unwrap()
                .iter()
                .collect::<Vec<_>>(),
            [Action::AnalyzeSource, Action::UpdateRegistry]
        );
        // Each call is its own entry, so naming a principal twice is the
        // duplicate the evaluator refuses, not a merge.
        assert_eq!(
            TestAuthority::new(chain_destination(), basis())
                .grant(&alice, [Action::AnalyzeSource])
                .grant(&alice, [Action::ReadRegistry])
                .establish(&limits())
                .err(),
            Some(EstablishmentFailure::DuplicatePrincipal)
        );
        // Without a session the authority has none.
        assert!(!TestAuthority::new(chain_destination(), basis())
            .establish(&limits())
            .unwrap()
            .development_session());
        // A destination keeps the chain it names, or none before
        // initialization.
        let destination = |registry| {
            TestAuthority::destination(
                "storefront-registry",
                chain_destination().owner().clone(),
                "storefront-web",
                registry,
            )
            .unwrap()
        };
        assert_eq!(
            destination(chain_destination().registry().cloned()),
            chain_destination()
        );
        let uninitialized = destination(None);
        assert_eq!(uninitialized.registry(), None);
        assert_eq!(uninitialized.binding().as_str(), "storefront-registry");
    }
}
