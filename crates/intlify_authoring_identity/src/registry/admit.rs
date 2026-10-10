// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Reading a registry snapshot or update and deciding whether to admit it.
//!
//! The order follows design 017: the shared read (bounded strict decoding, the
//! exact kind, schema and specification, the closed body, the integrity
//! digest), then this reader's bounds, then the body's structural rules.
//!
//! What admission establishes is narrow on purpose. An admitted snapshot is
//! well formed and unaltered since sealing; that is not a proof that it is the
//! result of its update, that its chain starts from an accepted anchor, or that
//! it is the current registry. An admitted update is well formed; that is not
//! a proof that it applies to its base, that its bases hold, or that anyone may
//! publish it. Replay, continuity checks and the host each answer one of those.

use intlify_authoring::{read_sealed, AuthoringArtifact, AuthoringArtifactReference, Occurrence};

use super::artifact::{RegistryArtifact, RegistryUpdateArtifact};
use super::snapshot::{
    same_declaration_order, EntryState, IntentRegistrySnapshot, RegistryEntry, SnapshotFailure,
};
use super::update::{IntentRegistryUpdate, SourceEdit, UpdateFailure};
use crate::admission::{within, IdentityAdmissionFailure};
use crate::limits::{IdentityLimitKind, IdentityLimits};

/// One `intent-registry` artifact that passed admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmittedRegistry {
    artifact: RegistryArtifact,
    // The active entries, by declaration, built once here so that a later
    // lookup by declaration never scans every entry.
    active: Box<[usize]>,
}

impl AdmittedRegistry {
    /// Borrow the admitted artifact.
    #[must_use]
    pub const fn artifact(&self) -> &RegistryArtifact {
        &self.artifact
    }

    /// Borrow the admitted snapshot.
    #[must_use]
    pub fn snapshot(&self) -> &IntentRegistrySnapshot {
        self.artifact.body()
    }

    /// Return the reference another artifact uses to name this one.
    #[must_use]
    pub fn reference(&self) -> AuthoringArtifactReference {
        self.artifact.reference()
    }

    /// Find the active entry that holds exactly this declaration.
    pub(crate) fn active_entry(&self, declaration: &Occurrence) -> Option<&RegistryEntry> {
        let entries = self.snapshot().entries();
        self.active
            .binary_search_by(|index| {
                same_declaration_order(entries[*index].declaration(), declaration)
            })
            .ok()
            .map(|position| &entries[self.active[position]])
    }
}

/// One `intent-registry-update` artifact that passed admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmittedUpdate {
    artifact: RegistryUpdateArtifact,
}

impl AdmittedUpdate {
    /// Borrow the admitted artifact.
    #[must_use]
    pub const fn artifact(&self) -> &RegistryUpdateArtifact {
        &self.artifact
    }

    /// Borrow the admitted update.
    #[must_use]
    pub fn update(&self) -> &IntentRegistryUpdate {
        self.artifact.body()
    }

    /// Return the reference another artifact uses to name this one.
    #[must_use]
    pub fn reference(&self) -> AuthoringArtifactReference {
        self.artifact.reference()
    }
}

/// Admit one `intent-registry` artifact.
pub fn admit_registry(
    bytes: &[u8],
    limits: &IdentityLimits,
) -> Result<AdmittedRegistry, IdentityAdmissionFailure<SnapshotFailure>> {
    let artifact: RegistryArtifact = read_sealed(bytes).map_err(IdentityAdmissionFailure::Read)?;
    let snapshot = artifact.body();
    within(
        snapshot.entries().len(),
        limits.entries,
        IdentityLimitKind::Entries,
    )?;
    snapshot
        .validate()
        .map_err(IdentityAdmissionFailure::Structure)?;
    let entries = snapshot.entries();
    let mut active: Vec<usize> = (0..entries.len())
        .filter(|index| entries[*index].state() == EntryState::Active)
        .collect();
    // Validation refused two active entries on one declaration, so the order
    // is strict and a search finds at most one.
    active.sort_unstable_by(|left, right| {
        same_declaration_order(entries[*left].declaration(), entries[*right].declaration())
    });
    Ok(AdmittedRegistry {
        artifact,
        active: active.into_boxed_slice(),
    })
}

/// Admit one `intent-registry-update` artifact.
pub fn admit_update(
    bytes: &[u8],
    limits: &IdentityLimits,
) -> Result<AdmittedUpdate, IdentityAdmissionFailure<UpdateFailure>> {
    let artifact: RegistryUpdateArtifact =
        read_sealed(bytes).map_err(IdentityAdmissionFailure::Read)?;
    admit_update_artifact(artifact, limits)
}

/// Admit an update artifact this crate already holds, such as a plan it has
/// just sealed: the same bounds and structural rules as one read from bytes.
pub(crate) fn admit_update_artifact(
    artifact: RegistryUpdateArtifact,
    limits: &IdentityLimits,
) -> Result<AdmittedUpdate, IdentityAdmissionFailure<UpdateFailure>> {
    let update = artifact.body();
    within(
        update.decisions().len(),
        limits.decisions,
        IdentityLimitKind::Decisions,
    )?;
    within(
        update.lineage_links().len(),
        limits.lineage_links,
        IdentityLimitKind::LineageLinks,
    )?;
    within(
        update
            .lineage_links()
            .iter()
            .map(|link| link.predecessors().len() + link.successors().len())
            .sum(),
        limits.lineage_members,
        IdentityLimitKind::LineageMembers,
    )?;
    within(
        update.source_edits().count(),
        limits.source_edits,
        IdentityLimitKind::SourceEdits,
    )?;
    within(
        update
            .source_edits()
            .map(|edit| edit.replacements().len())
            .sum(),
        limits.replacements,
        IdentityLimitKind::Replacements,
    )?;
    within(
        update
            .source_edits()
            .flat_map(SourceEdit::replacements)
            .map(|replacement| replacement.text().len())
            .sum(),
        limits.replacement_bytes,
        IdentityLimitKind::ReplacementBytes,
    )?;
    update
        .validate()
        .map_err(IdentityAdmissionFailure::Structure)?;
    Ok(AdmittedUpdate { artifact })
}
