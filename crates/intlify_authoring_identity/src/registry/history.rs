// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Replaying registry history: rebuilding each retained state from its base,
//! update and inventory, and comparing it with what was stored.
//!
//! 017 asks a reader to replay a retained update and compare the complete
//! resulting snapshot, not its digest. A chain is verified from an anchor the
//! host names: the genesis it initialized, or a snapshot it already accepted.
//! Walking back to some genesis shows only that a chain is consistent with
//! itself; it does not make that genesis trusted, so the anchor is never
//! inferred from the chain.
//!
//! Replay needs every base, update and inventory on the way. When one is
//! missing the chain is blocked, and nothing stands in for it: not an older
//! snapshot, not a matching text.

use std::cmp::Ordering;

use intlify_authoring::{AdmittedInventory, AuthoringArtifactReference};

use super::admit::{AdmittedRegistry, AdmittedUpdate};
use super::apply::{apply, Transition, TransitionFailure};
use crate::limits::{IdentityLimitKind, IdentityLimits};

/// Why a stored snapshot is not what its update produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayFailure {
    /// The stored snapshot does not name this base and this update.
    Unlinked,
    /// The update changes nothing, so 017 records no new state for it, yet a
    /// snapshot claims to be its result.
    NoNewState,
    /// The update does not apply to its base.
    Transition(TransitionFailure),
    /// The update applies, but its result is not the stored snapshot.
    Mismatch,
}

/// Rebuild one stored snapshot from its base, update and inventory, and
/// compare the complete result with it.
///
/// The comparison is of the whole snapshot. Two artifacts sealed over equal
/// bodies have equal digests, but a digest is never compared in place of the
/// content it names.
pub fn replay(
    base: &AdmittedRegistry,
    update: &AdmittedUpdate,
    inventory: &AdmittedInventory,
    stored: &AdmittedRegistry,
) -> Result<(), ReplayFailure> {
    let snapshot = stored.snapshot();
    if snapshot.base() != Some(&base.reference()) || snapshot.update() != Some(&update.reference())
    {
        return Err(ReplayFailure::Unlinked);
    }
    match apply(base, update, inventory).map_err(ReplayFailure::Transition)? {
        Transition::Unchanged => Err(ReplayFailure::NoNewState),
        Transition::Applied(result) if *result == *snapshot => Ok(()),
        Transition::Applied(_) => Err(ReplayFailure::Mismatch),
    }
}

/// Where the host trusts a chain to start.
#[derive(Debug, Clone, Copy)]
pub struct Anchor<'a> {
    registry: &'a AdmittedRegistry,
}

impl<'a> Anchor<'a> {
    /// Anchor a chain at the genesis the host's explicit initialization
    /// created.
    ///
    /// Only the host knows which genesis that is. A genesis found by walking
    /// back along a chain is not one.
    pub fn genesis(registry: &'a AdmittedRegistry) -> Result<Self, HistoryFailure> {
        if !registry.snapshot().is_genesis() {
            return Err(HistoryFailure::NotGenesis);
        }
        Ok(Self { registry })
    }

    /// Anchor a chain at a snapshot the host has already accepted.
    ///
    /// Acceptance is the host's statement, made under its own authority.
    /// Nothing here establishes it.
    #[must_use]
    pub const fn accepted(registry: &'a AdmittedRegistry) -> Self {
        Self { registry }
    }
}

/// The finite set of admitted artifacts a chain is verified with.
///
/// Each artifact is found by its exact reference. A reference that names
/// nothing here leaves the chain blocked; it is never matched by kind,
/// position or content.
#[derive(Debug)]
pub struct RetainedHistory<'a> {
    registries: Vec<&'a AdmittedRegistry>,
    updates: Vec<&'a AdmittedUpdate>,
    inventories: Vec<&'a AdmittedInventory>,
}

impl<'a> RetainedHistory<'a> {
    /// Collect the retained artifacts.
    ///
    /// Submitting one artifact twice is a duplicate input, reported rather
    /// than merged.
    pub fn new(
        registries: &'a [AdmittedRegistry],
        updates: &'a [AdmittedUpdate],
        inventories: &'a [AdmittedInventory],
    ) -> Result<Self, HistoryFailure> {
        Ok(Self {
            registries: indexed(registries, AdmittedRegistry::reference)?,
            updates: indexed(updates, AdmittedUpdate::reference)?,
            inventories: indexed(inventories, AdmittedInventory::reference)?,
        })
    }

    fn registry(&self, reference: &AuthoringArtifactReference) -> Option<&'a AdmittedRegistry> {
        find(&self.registries, reference, AdmittedRegistry::reference)
    }

    fn update(&self, reference: &AuthoringArtifactReference) -> Option<&'a AdmittedUpdate> {
        find(&self.updates, reference, AdmittedUpdate::reference)
    }

    fn inventory(&self, reference: &AuthoringArtifactReference) -> Option<&'a AdmittedInventory> {
        find(&self.inventories, reference, AdmittedInventory::reference)
    }
}

fn indexed<T>(
    artifacts: &[T],
    reference: fn(&T) -> AuthoringArtifactReference,
) -> Result<Vec<&T>, HistoryFailure> {
    let mut keyed: Vec<(AuthoringArtifactReference, &T)> = artifacts
        .iter()
        .map(|artifact| (reference(artifact), artifact))
        .collect();
    keyed.sort_unstable_by(|left, right| left.0.canonical_cmp(&right.0));
    if keyed
        .windows(2)
        .any(|pair| pair[0].0.canonical_cmp(&pair[1].0) == Ordering::Equal)
    {
        return Err(HistoryFailure::DuplicateArtifact);
    }
    Ok(keyed.into_iter().map(|(_, artifact)| artifact).collect())
}

fn find<'a, T>(
    artifacts: &[&'a T],
    wanted: &AuthoringArtifactReference,
    reference: fn(&T) -> AuthoringArtifactReference,
) -> Option<&'a T> {
    artifacts
        .binary_search_by(|artifact| reference(artifact).canonical_cmp(wanted))
        .ok()
        .map(|index| artifacts[index])
}

/// What verifying a chain found, when nothing in it was wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryOutcome {
    /// Every update from the anchor to the head replays exactly.
    Verified {
        /// How many updates were replayed.
        steps: u64,
    },
    /// An artifact the chain needs is not retained, so it was not replayed.
    Blocked {
        /// The reference that named nothing retained.
        missing: AuthoringArtifactReference,
    },
}

/// Why a chain does not verify.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryFailure {
    /// A genesis anchor was asked for with a snapshot that has history.
    NotGenesis,
    /// The chain reaches a genesis that is not the anchor.
    NotAnchored,
    /// One artifact was submitted twice.
    DuplicateArtifact,
    /// A named bound was exhausted before the anchor was reached.
    Limit(IdentityLimitKind),
    /// A retained update does not reproduce the snapshot that names it.
    Replay {
        /// The snapshot that did not replay.
        snapshot: AuthoringArtifactReference,
        /// Why it did not.
        failure: ReplayFailure,
    },
}

/// Verify a chain from its head back to an anchor, then replay it forward.
///
/// The walk follows each snapshot's base reference until it reaches the
/// anchor, and stops at the caller's bound on history steps. Every step is
/// resolved before anything is replayed, so a blocked chain reports the
/// first reference that named nothing, and no partial verification.
pub fn verify_history(
    head: &AdmittedRegistry,
    anchor: &Anchor<'_>,
    retained: &RetainedHistory<'_>,
    limits: &IdentityLimits,
) -> Result<HistoryOutcome, HistoryFailure> {
    let anchored = anchor.registry.reference();
    let mut steps = Vec::new();
    let mut current = head;
    while current.reference() != anchored {
        let snapshot = current.snapshot();
        // A snapshot without history that is not the anchor ends a chain
        // that is only consistent with itself.
        let (Some(base), Some(update)) = (snapshot.base(), snapshot.update()) else {
            return Err(HistoryFailure::NotAnchored);
        };
        if steps.len() as u64 >= limits.history_steps {
            return Err(HistoryFailure::Limit(IdentityLimitKind::HistorySteps));
        }
        let Some(update) = retained.update(update) else {
            return Ok(HistoryOutcome::Blocked {
                missing: update.clone(),
            });
        };
        let Some(inventory) = retained.inventory(update.update().inventory()) else {
            return Ok(HistoryOutcome::Blocked {
                missing: update.update().inventory().clone(),
            });
        };
        let previous = if *base == anchored {
            anchor.registry
        } else if let Some(previous) = retained.registry(base) {
            previous
        } else {
            return Ok(HistoryOutcome::Blocked {
                missing: base.clone(),
            });
        };
        steps.push((previous, update, inventory, current));
        current = previous;
    }
    for (base, update, inventory, stored) in steps.iter().rev() {
        replay(base, update, inventory, stored).map_err(|failure| HistoryFailure::Replay {
            snapshot: stored.reference(),
            failure,
        })?;
    }
    Ok(HistoryOutcome::Verified {
        steps: steps.len() as u64,
    })
}

#[cfg(test)]
mod tests {
    use intlify_authoring::AuthoringArtifact;
    use serde_json::json;

    use super::*;
    use crate::registry::admit::{admit_registry, admit_update};
    use crate::registry::artifact::{RegistryArtifact, RegistryUpdateArtifact};
    use crate::registry::fixtures::{artifact, limits, owner, Chain};
    use crate::registry::snapshot::IntentRegistrySnapshot;
    use crate::registry::update::IntentRegistryUpdate;

    fn retained(chain: &Chain) -> RetainedHistory<'_> {
        RetainedHistory::new(&chain.registries, &chain.updates, &chain.inventories).unwrap()
    }

    #[test]
    fn a_retained_artifact_is_found_only_by_its_exact_reference() {
        let chain = Chain::load();
        let retained = retained(&chain);
        let wanted = chain.registry(1).reference();
        assert_eq!(
            retained.registry(&wanted).map(AdmittedRegistry::reference),
            Some(wanted.clone())
        );
        // The same digest under another kind or schema revision names
        // nothing: a reference is compared in full, never by digest alone.
        for (member, value) in [
            ("kind", json!("intent-registry-update")),
            ("schemaRevision", json!("1")),
        ] {
            let mut other = serde_json::to_value(&wanted).unwrap();
            other[member] = value;
            let other: AuthoringArtifactReference = serde_json::from_value(other).unwrap();
            assert!(retained.registry(&other).is_none(), "{member}");
        }
        // Each kind is looked up among its own artifacts.
        assert!(retained.registry(&chain.update(1).reference()).is_none());
        assert!(retained.update(&chain.update(1).reference()).is_some());
        assert!(retained
            .inventory(&chain.inventory(1).reference())
            .is_some());
    }

    #[test]
    fn one_artifact_submitted_twice_is_a_duplicate_of_any_kind() {
        let chain = Chain::load();
        let registries = [chain.registry(0).clone(), chain.registry(0).clone()];
        assert_eq!(
            RetainedHistory::new(&registries, &[], &[]).err(),
            Some(HistoryFailure::DuplicateArtifact)
        );
        let updates = [chain.update(1).clone(), chain.update(1).clone()];
        assert_eq!(
            RetainedHistory::new(&[], &updates, &[]).err(),
            Some(HistoryFailure::DuplicateArtifact)
        );
        let inventories = [chain.inventory(1).clone(), chain.inventory(1).clone()];
        assert_eq!(
            RetainedHistory::new(&[], &[], &inventories).err(),
            Some(HistoryFailure::DuplicateArtifact)
        );
        // Distinct artifacts submitted in any order are not.
        let reversed: Vec<AdmittedRegistry> = chain.registries.iter().rev().cloned().collect();
        assert!(RetainedHistory::new(&reversed, &[], &[]).is_ok());
    }

    #[test]
    fn a_genesis_anchor_needs_a_genesis_and_an_accepted_one_takes_the_host_s_word() {
        let chain = Chain::load();
        assert!(Anchor::genesis(chain.registry(0)).is_ok());
        assert_eq!(
            Anchor::genesis(chain.registry(1)).err(),
            Some(HistoryFailure::NotGenesis)
        );
        let accepted = Anchor::accepted(chain.registry(2));
        assert_eq!(accepted.registry.reference(), chain.registry(2).reference());
    }

    #[test]
    fn an_anchor_later_than_the_head_never_anchors_it() {
        // Walking back from registry-1 passes registry-0 and ends there; an
        // anchor at registry-2 lies ahead of the head and is never met.
        let chain = Chain::load();
        assert_eq!(
            verify_history(
                chain.registry(1),
                &Anchor::accepted(chain.registry(2)),
                &retained(&chain),
                &limits()
            ),
            Err(HistoryFailure::NotAnchored)
        );
    }

    #[test]
    fn a_blocked_walk_names_the_update_then_its_inventory_then_the_base() {
        let chain = Chain::load();
        let head = chain.registry(2);
        let anchor = Anchor::genesis(chain.registry(0)).unwrap();
        let blocked = |missing: AuthoringArtifactReference| Ok(HistoryOutcome::Blocked { missing });

        let nothing = RetainedHistory::new(&[], &[], &[]).unwrap();
        assert_eq!(
            verify_history(head, &anchor, &nothing, &limits()),
            blocked(chain.update(2).reference())
        );
        let updates = [chain.update(2).clone()];
        let with_update = RetainedHistory::new(&[], &updates, &[]).unwrap();
        assert_eq!(
            verify_history(head, &anchor, &with_update, &limits()),
            blocked(chain.inventory(2).reference())
        );
        let inventories = [chain.inventory(2).clone()];
        let without_base = RetainedHistory::new(&[], &updates, &inventories).unwrap();
        assert_eq!(
            verify_history(head, &anchor, &without_base, &limits()),
            blocked(chain.registry(1).reference())
        );
    }

    #[test]
    fn the_bound_counts_the_updates_replayed_from_the_head() {
        let chain = Chain::load();
        let retained = retained(&chain);
        let anchor = Anchor::genesis(chain.registry(0)).unwrap();
        let bound = |history_steps| IdentityLimits {
            history_steps,
            ..limits()
        };
        // The anchor itself replays nothing, so even a zero bound admits it.
        assert_eq!(
            verify_history(chain.registry(0), &anchor, &retained, &bound(0)),
            Ok(HistoryOutcome::Verified { steps: 0 })
        );
        assert_eq!(
            verify_history(chain.registry(1), &anchor, &retained, &bound(0)),
            Err(HistoryFailure::Limit(IdentityLimitKind::HistorySteps))
        );
        assert_eq!(
            verify_history(chain.registry(1), &anchor, &retained, &bound(1)),
            Ok(HistoryOutcome::Verified { steps: 1 })
        );
    }

    #[test]
    fn replay_checks_the_link_before_it_applies_anything() {
        // registry-1 names update-1. Paired with update-2 it is unlinked,
        // and that is what is reported, although update-2 would not apply to
        // registry-0 either.
        let chain = Chain::load();
        assert_eq!(
            replay(
                chain.registry(0),
                chain.update(2),
                chain.inventory(2),
                chain.registry(1)
            ),
            Err(ReplayFailure::Unlinked)
        );
        assert_eq!(
            replay(
                chain.registry(0),
                chain.update(1),
                chain.inventory(1),
                chain.registry(1)
            ),
            Ok(())
        );
    }

    #[test]
    fn a_snapshot_recorded_for_an_update_that_changes_nothing_does_not_replay() {
        // 017 reuses the base for an update with no decisions and no links. A
        // snapshot claiming to be such an update's result records a state
        // that should not exist, even though its entries are the base's.
        let chain = Chain::load();
        let base = chain.registry(3);
        let nothing = IntentRegistryUpdate::new(
            owner(),
            base.reference(),
            chain.inventory(3).reference(),
            vec![],
            vec![],
        )
        .unwrap();
        let nothing = RegistryUpdateArtifact::seal(nothing).unwrap();
        let nothing = admit_update(&serde_json::to_vec(&nothing).unwrap(), &limits()).unwrap();

        let mut claimed = artifact("registry-3")["body"].clone();
        claimed["base"] = serde_json::to_value(base.reference()).unwrap();
        claimed["update"] = serde_json::to_value(nothing.reference()).unwrap();
        let claimed: IntentRegistrySnapshot = serde_json::from_value(claimed).unwrap();
        let claimed = RegistryArtifact::seal(claimed).unwrap();
        let claimed = admit_registry(&serde_json::to_vec(&claimed).unwrap(), &limits()).unwrap();

        assert_eq!(
            replay(base, &nothing, chain.inventory(3), &claimed),
            Err(ReplayFailure::NoNewState)
        );
    }
}
