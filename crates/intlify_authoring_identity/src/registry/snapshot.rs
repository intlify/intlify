// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 017's `IntentRegistrySnapshot` and the structural rules it carries.
//!
//! A snapshot is one immutable registry state for one owner and scope: which
//! Intent IDs exist, whether each is active or retired, and the declaration
//! each was last associated with. It records identity history. It is not the
//! authority for a message's current meaning, which is computed from the
//! current declaration, never read from here.
//!
//! The rules checked here hold of any well-formed snapshot on its own: one
//! owner, entries in Intent ID order with one entry per ID, declaration roles,
//! no declaration held by two active IDs, and a history that is either absent
//! (a genesis, with no entries) or complete (the base and the update that
//! produced this state). Whether the snapshot really is what applying that
//! update to that base produces is replay's question, and whether it is the
//! current registry is the host's.

use std::cmp::Ordering;

use intlify_authoring::{
    ArtifactKind, AuthoringArtifact, AuthoringArtifactReference, MessageIntentId, Occurrence,
    OwnerIdentity, PrimitiveError, Token,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::artifact::{RegistryArtifact, RegistryUpdateArtifact};
use crate::id::RegistryIdentity;

/// Whether an Intent ID is in use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum EntryState {
    /// The ID names a current declaration.
    Active,
    /// The ID is retired. It keeps its last declaration and is never reused.
    Retired,
}

/// One Intent ID, its state, and the declaration it was last associated with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegistryEntry {
    intent_id: MessageIntentId,
    state: EntryState,
    declaration: Occurrence,
}

impl RegistryEntry {
    /// Retain one entry.
    #[must_use]
    pub const fn new(
        intent_id: MessageIntentId,
        state: EntryState,
        declaration: Occurrence,
    ) -> Self {
        Self {
            intent_id,
            state,
            declaration,
        }
    }

    /// Borrow the Intent ID.
    #[must_use]
    pub const fn intent_id(&self) -> &MessageIntentId {
        &self.intent_id
    }

    /// Return whether the ID is active or retired.
    #[must_use]
    pub const fn state(&self) -> EntryState {
        self.state
    }

    /// Borrow the declaration the ID was last associated with.
    #[must_use]
    pub const fn declaration(&self) -> &Occurrence {
        &self.declaration
    }
}

/// One immutable registry state for one owner and scope.
///
/// Deserializing one reads its shape only. [`IntentRegistrySnapshot::validate`]
/// checks the structural rules, and neither says that this state is the
/// result of its update or that it is current.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IntentRegistrySnapshot {
    owner: OwnerIdentity,
    scope: Token,
    registry_identity: RegistryIdentity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "crate::schema::optional_registry_reference")]
    base: Option<AuthoringArtifactReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "crate::schema::optional_update_reference")]
    update: Option<AuthoringArtifactReference>,
    entries: Box<[RegistryEntry]>,
}

/// Why a snapshot is not well formed.
///
/// Each variant names one rule, so a caller can tell which one a damaged or
/// forged snapshot broke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotFailure {
    /// The snapshot names a base without the update that produced it, or an
    /// update without its base. A genesis names neither, and every later
    /// snapshot names both.
    UnpairedHistory,
    /// A genesis holds entries. A new chain starts empty.
    GenesisEntries,
    /// The base does not name an `intent-registry`, or the update an
    /// `intent-registry-update`, under the schema revision and specification
    /// this reader implements.
    ReferenceKind,
    /// An entry's ID or declaration belongs to an owner other than the
    /// registry's, or a successor is built from another owner's update.
    ForeignOwner,
    /// Two entries hold one Intent ID.
    DuplicateEntry,
    /// Entries are not in Intent ID order.
    EntriesUnordered,
    /// An entry's declaration does not have a declaration role.
    RoleMismatch,
    /// An entry's declaration runs past the end of its unit.
    RangeOutsideSource,
    /// Two active entries hold one declaration.
    SharedDeclaration,
    /// A successor is built from an update that was planned against another
    /// base. Only a snapshot built from the two artifacts can be checked for
    /// this on its own: a decoded snapshot holds references alone, and for one
    /// of those it is replaying the update from its base that finds it.
    UpdateBaseMismatch,
}

impl IntentRegistrySnapshot {
    /// Start a new chain: no history and no entries.
    ///
    /// The registry identity comes from the host, which draws it once from
    /// operating-system randomness when an authorized initialization creates
    /// the chain. Building this body is not that initialization: it publishes
    /// nothing and makes nothing current, and a missing or damaged registry
    /// never calls for a new one.
    pub fn genesis(
        owner: OwnerIdentity,
        scope: &str,
        registry_identity: RegistryIdentity,
    ) -> Result<Self, PrimitiveError> {
        Ok(Self {
            owner,
            scope: Token::new(scope)?,
            registry_identity,
            base: None,
            update: None,
            entries: Box::new([]),
        })
    }

    /// Record the state an update claims to produce from its base.
    ///
    /// The update has to be planned against `base` and belong to its owner;
    /// otherwise the two would not be one link of a chain. The owner, scope
    /// and registry identity are the base's, because a chain keeps them for
    /// its whole life. Entries are put into Intent ID order and the result is
    /// validated; a repeated ID is reported, never dropped.
    ///
    /// Nothing here applies the update or compares its result. A snapshot
    /// built this way is a claim, and replaying the update from its base is
    /// what tests it.
    pub fn successor(
        base: &RegistryArtifact,
        update: &RegistryUpdateArtifact,
        mut entries: Vec<RegistryEntry>,
    ) -> Result<Self, SnapshotFailure> {
        let previous = base.body();
        let base_reference = base.reference();
        if update.body().base() != &base_reference {
            return Err(SnapshotFailure::UpdateBaseMismatch);
        }
        if update.body().owner() != &previous.owner {
            return Err(SnapshotFailure::ForeignOwner);
        }
        entries.sort_by(|left, right| left.intent_id.cmp(&right.intent_id));
        let snapshot = Self {
            owner: previous.owner.clone(),
            scope: previous.scope.clone(),
            registry_identity: previous.registry_identity.clone(),
            base: Some(base_reference),
            update: Some(update.reference()),
            entries: entries.into_boxed_slice(),
        };
        snapshot.validate()?;
        Ok(snapshot)
    }

    /// Borrow the owning application or library.
    #[must_use]
    pub const fn owner(&self) -> &OwnerIdentity {
        &self.owner
    }

    /// Borrow the owning scope.
    #[must_use]
    pub const fn scope(&self) -> &Token {
        &self.scope
    }

    /// Borrow the identity of the chain this state belongs to.
    #[must_use]
    pub const fn registry_identity(&self) -> &RegistryIdentity {
        &self.registry_identity
    }

    /// Borrow the reference to the previous state, absent for a genesis.
    #[must_use]
    pub const fn base(&self) -> Option<&AuthoringArtifactReference> {
        self.base.as_ref()
    }

    /// Borrow the reference to the update that produced this state, absent
    /// for a genesis.
    #[must_use]
    pub const fn update(&self) -> Option<&AuthoringArtifactReference> {
        self.update.as_ref()
    }

    /// Borrow the entries, in Intent ID order.
    #[must_use]
    pub fn entries(&self) -> &[RegistryEntry] {
        &self.entries
    }

    /// Return whether this state starts a chain.
    #[must_use]
    pub const fn is_genesis(&self) -> bool {
        self.base.is_none() && self.update.is_none()
    }

    /// Find the entry for one Intent ID.
    #[must_use]
    pub fn entry(&self, intent_id: &MessageIntentId) -> Option<&RegistryEntry> {
        self.entries
            .binary_search_by(|entry| entry.intent_id.cmp(intent_id))
            .ok()
            .map(|index| &self.entries[index])
    }

    /// Check every structural rule a well-formed snapshot satisfies.
    pub fn validate(&self) -> Result<(), SnapshotFailure> {
        match (&self.base, &self.update) {
            (None, None) if !self.entries.is_empty() => {
                return Err(SnapshotFailure::GenesisEntries);
            }
            (None, None) => {}
            (Some(base), Some(update)) => {
                if !base.is_current(ArtifactKind::IntentRegistry)
                    || !update.is_current(ArtifactKind::IntentRegistryUpdate)
                {
                    return Err(SnapshotFailure::ReferenceKind);
                }
            }
            (Some(_), None) | (None, Some(_)) => return Err(SnapshotFailure::UnpairedHistory),
        }

        for entry in &*self.entries {
            let declaration = &entry.declaration;
            if entry.intent_id.owner() != &self.owner || declaration.source().owner() != &self.owner
            {
                return Err(SnapshotFailure::ForeignOwner);
            }
            if !declaration.role().is_declaration() {
                return Err(SnapshotFailure::RoleMismatch);
            }
            // Decoding does not run `Occurrence::new`, so a decoded range has
            // not been checked against its unit yet.
            if declaration.range().end() > declaration.source().byte_length() {
                return Err(SnapshotFailure::RangeOutsideSource);
            }
        }
        for pair in self.entries.windows(2) {
            match pair[0].intent_id.cmp(&pair[1].intent_id) {
                Ordering::Less => {}
                Ordering::Equal => return Err(SnapshotFailure::DuplicateEntry),
                Ordering::Greater => return Err(SnapshotFailure::EntriesUnordered),
            }
        }

        // Retired entries keep their last declaration, and a later active ID
        // may legitimately sit where one of them once did. Only two active IDs
        // on one declaration contradict each other.
        let mut active: Vec<&Occurrence> = self
            .entries
            .iter()
            .filter(|entry| entry.state == EntryState::Active)
            .map(|entry| &entry.declaration)
            .collect();
        active.sort_unstable_by(|left, right| same_declaration_order(left, right));
        if active.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(SnapshotFailure::SharedDeclaration);
        }
        Ok(())
    }
}

/// Order occurrences so that exactly equal ones are adjacent.
///
/// The canonical order leaves out a snapshot's declared length, so two
/// occurrences it calls equal may still differ there. Breaking the tie on the
/// length puts every pair of exactly equal occurrences next to each other.
pub(crate) fn same_declaration_order(left: &Occurrence, right: &Occurrence) -> Ordering {
    left.canonical_cmp(right).then_with(|| {
        left.source()
            .byte_length()
            .cmp(&right.source().byte_length())
    })
}
