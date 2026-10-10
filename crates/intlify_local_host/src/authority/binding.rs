// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Holding inputs to the scope one authority admits.
//!
//! Design 018 binds every operation to its exact inputs. An inventory has to
//! be the destination owner's, of its owning scope, resolved against the
//! authority's context, and made only of source snapshots the host acquired
//! for this invocation; a complete one has to be made of all of them. A
//! registry has to be the destination's chain and an anchor the host
//! accepted. A self-consistent chain is not an anchor, and a snapshot an
//! inventory merely names is not acquired.
//!
//! Passing these checks says the inputs are the ones this authority speaks
//! for. It says nothing about whether they are valid; admission and design
//! 016's checks answer that, before and independently.

use intlify_authoring::{AdmittedInventory, Completeness};
use intlify_authoring_identity::AdmittedRegistry;

use super::action::Action;
use super::context::{Invocation, LocalAuthority};
use super::failure::AuthorizationFailure;

impl LocalAuthority {
    /// Check that an inventory is within this authority's scope.
    pub(crate) fn bind_inventory(
        &self,
        inventory: &AdmittedInventory,
    ) -> Result<(), AuthorizationFailure> {
        let current = inventory.inventory();
        let destination = self.destination();
        if current.owner() != destination.owner() {
            return Err(AuthorizationFailure::OwnerMismatch);
        }
        if current.scope() != destination.scope() {
            return Err(AuthorizationFailure::ScopeMismatch);
        }
        if current.basis() != self.context() {
            return Err(AuthorizationFailure::ContextMismatch);
        }
        let acquired = self.acquired();
        for unit in current.units() {
            let source = unit.source();
            // The search finds the unit; the snapshot has to match in full.
            let held = acquired
                .binary_search_by(|snapshot| snapshot.unit().cmp(source.unit()))
                .is_ok_and(|index| acquired[index] == *source);
            if !held {
                return Err(AuthorizationFailure::SourceNotAcquired);
            }
        }
        // Units are unique and each one is acquired, so equal counts mean
        // the inventory covers everything the host acquired.
        if current.completeness() == Completeness::Complete
            && current.units().len() != acquired.len()
        {
            return Err(AuthorizationFailure::IncompleteScope);
        }
        Ok(())
    }

    /// Check that a registry is the destination's chain and an accepted
    /// anchor.
    pub(crate) fn bind_registry(
        &self,
        registry: &AdmittedRegistry,
    ) -> Result<(), AuthorizationFailure> {
        let snapshot = registry.snapshot();
        let destination = self.destination();
        if snapshot.owner() != destination.owner() {
            return Err(AuthorizationFailure::OwnerMismatch);
        }
        if snapshot.scope() != destination.scope() {
            return Err(AuthorizationFailure::ScopeMismatch);
        }
        if destination.registry() != Some(snapshot.registry_identity()) {
            return Err(AuthorizationFailure::RegistryMismatch);
        }
        if !self.anchors().contains(&registry.reference()) {
            return Err(AuthorizationFailure::Unanchored);
        }
        Ok(())
    }
}

impl Invocation<'_> {
    /// Authorize a read-only analysis of an inventory, against a registry
    /// when one is given.
    ///
    /// Analysis needs `analyze-source`, and reading a registry needs
    /// `read-registry` as well. No write grant is needed or implied: what an
    /// analysis returns carries no power to publish.
    pub fn authorize_analysis(
        &self,
        inventory: &AdmittedInventory,
        registry: Option<&AdmittedRegistry>,
    ) -> Result<(), AuthorizationFailure> {
        self.require(Action::AnalyzeSource)?;
        if registry.is_some() {
            self.require(Action::ReadRegistry)?;
        }
        // The registry first, as every operation binds its base before its
        // inventory, so one input always reports the same failure.
        let authority = self.authority();
        if let Some(registry) = registry {
            authority.bind_registry(registry)?;
        }
        authority.bind_inventory(inventory)
    }

    /// Authorize reading a registry on its own, the current one or a pinned
    /// earlier one, without analyzing any source.
    ///
    /// Reading needs `read-registry` alone. Whether the registry is current is
    /// not decided here: a pinned earlier snapshot is read as it is, and
    /// reading it grants nothing about the current chain.
    pub fn authorize_read(&self, registry: &AdmittedRegistry) -> Result<(), AuthorizationFailure> {
        self.require(Action::ReadRegistry)?;
        self.authority().bind_registry(registry)
    }
}

#[cfg(test)]
mod tests {
    use intlify_authoring::{OwnerIdentity, OwnerKind, SourceSnapshot};
    use intlify_authoring_identity::RegistryIdentity;

    use super::*;
    use crate::authority::context::{Destination, Establishment, Principal};
    use crate::authority::fixtures::{establishment, limits, owner, partial, Chain};

    const READER: &[Action] = &[Action::AnalyzeSource, Action::ReadRegistry];

    fn authority(setup: Establishment) -> LocalAuthority {
        LocalAuthority::establish(setup, &limits()).unwrap()
    }

    fn alice() -> Principal {
        Principal::new("alice").unwrap()
    }

    #[test]
    fn a_read_only_analysis_needs_its_read_grants_and_nothing_more() {
        let chain = Chain::load();
        let authority = authority(establishment(&chain, 3, &[("alice", READER)]));
        let invocation = authority.invoke(&alice()).unwrap();
        assert_eq!(
            invocation.authorize_analysis(chain.inventory(3), Some(chain.registry(2))),
            Ok(())
        );
        // Without the registry, the source grant is enough.
        let only_source = authority_with(&chain, &[Action::AnalyzeSource]);
        let invocation = only_source.invoke(&alice()).unwrap();
        assert_eq!(
            invocation.authorize_analysis(chain.inventory(3), None),
            Ok(())
        );
        assert_eq!(
            invocation.authorize_analysis(chain.inventory(3), Some(chain.registry(2))),
            Err(AuthorizationFailure::Denied(Action::ReadRegistry))
        );
        let only_registry = authority_with(&chain, &[Action::ReadRegistry]);
        assert_eq!(
            only_registry
                .invoke(&alice())
                .unwrap()
                .authorize_analysis(chain.inventory(3), Some(chain.registry(2))),
            Err(AuthorizationFailure::Denied(Action::AnalyzeSource))
        );
    }

    fn authority_with(chain: &Chain, actions: &[Action]) -> LocalAuthority {
        authority(establishment(chain, 3, &[("alice", actions)]))
    }

    #[test]
    fn a_registry_is_read_with_its_read_grant_alone() {
        let chain = Chain::load();
        let reader = authority_with(&chain, &[Action::ReadRegistry]);
        let read = |authority: &LocalAuthority, n: usize| {
            authority
                .invoke(&alice())
                .unwrap()
                .authorize_read(chain.registry(n))
        };
        // No source is analyzed, so no source grant is needed.
        assert_eq!(read(&reader, 2), Ok(()));
        // Every other grant together does not read.
        let others = authority_with(
            &chain,
            &[
                Action::AnalyzeSource,
                Action::InitializeRegistry,
                Action::UpdateRegistry,
                Action::ResolveIdentity,
            ],
        );
        assert_eq!(
            read(&others, 2),
            Err(AuthorizationFailure::Denied(Action::ReadRegistry))
        );
        // An earlier snapshot reads only where the host pinned it.
        assert_eq!(read(&reader, 1), Err(AuthorizationFailure::Unanchored));
        let mut setup = establishment(&chain, 3, &[("alice", &[Action::ReadRegistry])]);
        setup.anchors.push(chain.registry(1).reference());
        assert_eq!(read(&authority(setup), 1), Ok(()));
    }

    #[test]
    fn an_inventory_outside_the_scope_is_refused_for_what_differs() {
        let chain = Chain::load();
        let refused = |change: &dyn Fn(&mut Establishment)| {
            let mut setup = establishment(&chain, 3, &[("alice", READER)]);
            change(&mut setup);
            authority(setup)
                .invoke(&alice())
                .unwrap()
                .authorize_analysis(chain.inventory(3), None)
                .err()
        };
        assert_eq!(
            refused(&|setup| {
                // Another owner's authority acquires that owner's sources
                // only, so it holds none of these.
                setup.destination = Destination::new(
                    "elsewhere",
                    OwnerIdentity::new(OwnerKind::Application, "other-shop").unwrap(),
                    "storefront-web",
                    setup.destination.registry().cloned(),
                )
                .unwrap();
                setup.acquired.clear();
            }),
            Some(AuthorizationFailure::OwnerMismatch)
        );
        assert_eq!(
            refused(&|setup| {
                setup.destination = Destination::new(
                    "elsewhere",
                    owner(),
                    "storefront-admin",
                    setup.destination.registry().cloned(),
                )
                .unwrap();
            }),
            Some(AuthorizationFailure::ScopeMismatch)
        );
        assert_eq!(
            refused(&|setup| {
                let mut basis = serde_json::to_value(&setup.context).unwrap();
                basis["context"]["revision"] = serde_json::json!("2");
                setup.context = serde_json::from_value(basis).unwrap();
            }),
            Some(AuthorizationFailure::ContextMismatch)
        );
        // Another revision of a unit the inventory holds is not what was
        // acquired, even though the unit is.
        assert_eq!(
            refused(&|setup| {
                let source = setup.acquired[0].clone();
                setup.acquired[0] = SourceSnapshot::new(
                    source.owner().clone(),
                    source.unit().as_str(),
                    "9",
                    source.grammar().clone(),
                    source.byte_length(),
                    source.utf8_digest().as_str(),
                )
                .unwrap();
            }),
            Some(AuthorizationFailure::SourceNotAcquired)
        );
        // A unit the host did not acquire at all.
        assert_eq!(
            refused(&|setup| {
                setup.acquired.remove(1);
            }),
            Some(AuthorizationFailure::SourceNotAcquired)
        );
        // A complete inventory that leaves out an acquired unit.
        assert_eq!(
            refused(&|setup| {
                let source = setup.acquired[0].clone();
                setup.acquired.push(
                    SourceSnapshot::new(
                        source.owner().clone(),
                        "payment",
                        "1",
                        source.grammar().clone(),
                        source.byte_length(),
                        source.utf8_digest().as_str(),
                    )
                    .unwrap(),
                );
            }),
            Some(AuthorizationFailure::IncompleteScope)
        );
    }

    #[test]
    fn only_a_complete_inventory_has_to_cover_every_acquired_unit() {
        let chain = Chain::load();
        let mut setup = establishment(&chain, 3, &[("alice", READER)]);
        let source = setup.acquired[0].clone();
        setup.acquired.push(
            SourceSnapshot::new(
                source.owner().clone(),
                "payment",
                "1",
                source.grammar().clone(),
                source.byte_length(),
                source.utf8_digest().as_str(),
            )
            .unwrap(),
        );
        let authority = authority(setup);
        let invocation = authority.invoke(&alice()).unwrap();
        // A partial view, such as an editor request, covers what it names.
        assert_eq!(invocation.authorize_analysis(&partial(3), None), Ok(()));
        assert_eq!(
            invocation.authorize_analysis(chain.inventory(3), None),
            Err(AuthorizationFailure::IncompleteScope)
        );
    }

    #[test]
    fn an_analysis_reports_its_registry_before_its_inventory() {
        let chain = Chain::load();
        // Accepting registry 1 and inventory 2's sources, the authority
        // anchors no registry 2 and holds no revision 3 of checkout.
        let earlier = authority(establishment(&chain, 2, &[("alice", READER)]));
        assert_eq!(
            earlier
                .invoke(&alice())
                .unwrap()
                .authorize_analysis(chain.inventory(3), Some(chain.registry(2))),
            Err(AuthorizationFailure::Unanchored)
        );
        assert_eq!(
            earlier
                .invoke(&alice())
                .unwrap()
                .authorize_analysis(chain.inventory(3), None),
            Err(AuthorizationFailure::SourceNotAcquired)
        );
    }

    #[test]
    fn a_registry_outside_the_destination_or_its_anchors_is_refused() {
        let chain = Chain::load();
        let read = |setup: Establishment, n: usize| {
            authority(setup)
                .invoke(&alice())
                .unwrap()
                .authorize_analysis(chain.inventory(3), Some(chain.registry(n)))
                .err()
        };
        // Registry 1 is a real snapshot of this chain, and no anchor.
        assert_eq!(
            read(establishment(&chain, 3, &[("alice", READER)]), 1),
            Some(AuthorizationFailure::Unanchored)
        );
        // The same chain bound under another registry identity, or before
        // initialization.
        for registry in [
            Some(RegistryIdentity::retained(&"0".repeat(32)).unwrap()),
            None,
        ] {
            let mut setup = establishment(&chain, 3, &[("alice", READER)]);
            setup.destination =
                Destination::new("storefront-registry", owner(), "storefront-web", registry)
                    .unwrap();
            assert_eq!(read(setup, 2), Some(AuthorizationFailure::RegistryMismatch));
        }
    }

    #[test]
    fn a_registry_of_another_owner_or_scope_is_refused_before_its_identity() {
        let chain = Chain::load();
        // The destination's owner and scope differ from the registry's.
        let other = |owner: OwnerIdentity, scope: &str| {
            let mut setup = establishment(&chain, 3, &[("alice", READER)]);
            setup.acquired.clear();
            setup.destination = Destination::new(
                "storefront-registry",
                owner,
                scope,
                setup.destination.registry().cloned(),
            )
            .unwrap();
            authority(setup)
        };
        let foreign = other(
            OwnerIdentity::new(OwnerKind::Application, "other-shop").unwrap(),
            "storefront-web",
        );
        assert_eq!(
            foreign.bind_registry(chain.registry(2)),
            Err(AuthorizationFailure::OwnerMismatch)
        );
        let scoped = other(owner(), "storefront-admin");
        assert_eq!(
            scoped.bind_registry(chain.registry(2)),
            Err(AuthorizationFailure::ScopeMismatch)
        );
    }
}
