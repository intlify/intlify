// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 017's `IntentRegistryUpdate`: one fully specified candidate update
//! against one exact base.
//!
//! An update records decisions, not their proof. Each decision names the ID it
//! acts on, the exact base declaration it starts from, the current declaration
//! it ends at, and the basis it claims. A `verified-edit` or `confirmed-new`
//! label proves nothing by being well formed: the transition, continuity and
//! newness checks test it against the base, the inventory and the source.
//!
//! The rules checked here hold of any well-formed update on its own: one
//! owner, decisions in Intent ID order with one decision per ID, declaration
//! roles, no current declaration given two identities, source edits in the
//! order 017 fixes and with replacements that can be replayed, and lineage
//! links of the shape their kind requires.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use intlify_authoring::{
    ArtifactKind, AuthoringArtifactReference, ByteRange, MessageIntentId, NonemptyText, Occurrence,
    OwnerIdentity, PrimitiveError, SourceSnapshot, Token, VersionedIdentity,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::snapshot::same_declaration_order;

/// Define a closed single-value tag used as a variant discriminator.
macro_rules! tag {
    ($name:ident, $wire:literal, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
        enum $name {
            #[serde(rename = $wire)]
            Value,
        }
    };
}

tag!(ContinueTag, "continue", "Discriminator of a continuation.");
tag!(AllocateTag, "allocate", "Discriminator of an allocation.");
tag!(RetireTag, "retire", "Discriminator of a retirement.");
tag!(RestoreTag, "restore", "Discriminator of a restoration.");
tag!(
    UnchangedSnapshotTag,
    "unchanged-snapshot",
    "Discriminator of an unchanged-snapshot basis."
);
tag!(
    VerifiedEditTag,
    "verified-edit",
    "Discriminator of a verified-edit basis."
);
tag!(
    ExplicitTag,
    "explicit",
    "Discriminator of an explicit basis."
);
tag!(
    ConfirmedNewTag,
    "confirmed-new",
    "Discriminator of a confirmed-new basis."
);
tag!(
    CompleteAbsenceTag,
    "complete-absence",
    "Discriminator of a complete-absence basis."
);

/// A choice recorded with its explanation.
///
/// The reason is untrusted, possibly sensitive text. It explains a historical
/// choice; it is not the actor's authorization, which the host establishes
/// separately for this exact base, inventory and action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExplicitBasis {
    kind: ExplicitTag,
    reason: NonemptyText,
}

impl ExplicitBasis {
    /// Record an explicit choice and its explanation.
    pub fn new(reason: &str) -> Result<Self, PrimitiveError> {
        Ok(Self {
            kind: ExplicitTag::Value,
            reason: NonemptyText::from_validated(reason)?,
        })
    }

    /// Borrow the stated reason.
    #[must_use]
    pub fn reason(&self) -> &str {
        self.reason.as_str()
    }
}

/// The claim that a declaration did not change at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UnchangedSnapshot {
    kind: UnchangedSnapshotTag,
}

/// The claim that a continuity verifier accepts a set of source edits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerifiedEdit {
    kind: VerifiedEditTag,
    profile: VersionedIdentity,
    changes: Box<[SourceEdit]>,
}

impl VerifiedEdit {
    /// Borrow the verifier profile the edits claim to satisfy.
    #[must_use]
    pub const fn profile(&self) -> &VersionedIdentity {
        &self.profile
    }

    /// Borrow the edits, in before/after source order.
    #[must_use]
    pub fn changes(&self) -> &[SourceEdit] {
        &self.changes
    }
}

/// Why an ID continues from one declaration to another.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ContinuationBasis {
    UnchangedSnapshot(UnchangedSnapshot),
    VerifiedEdit(VerifiedEdit),
    Explicit(ExplicitBasis),
}

impl ContinuationBasis {
    /// Claim that the declaration is exactly where it was.
    #[must_use]
    pub const fn unchanged_snapshot() -> Self {
        Self::UnchangedSnapshot(UnchangedSnapshot {
            kind: UnchangedSnapshotTag::Value,
        })
    }

    /// Claim that a verifier profile accepts these edits.
    ///
    /// The edits are put into 017's before/after source order. A unit that
    /// appears twice on one side is kept, so validation can report it.
    #[must_use]
    pub fn verified_edit(profile: VersionedIdentity, mut changes: Vec<SourceEdit>) -> Self {
        changes.sort_by(SourceEdit::canonical_cmp);
        Self::VerifiedEdit(VerifiedEdit {
            kind: VerifiedEditTag::Value,
            profile,
            changes: changes.into_boxed_slice(),
        })
    }
}

/// The claim that a declaration is new.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfirmedNew {
    kind: ConfirmedNewTag,
}

/// Why a new ID is given to a declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum AllocationBasis {
    ConfirmedNew(ConfirmedNew),
    Explicit(ExplicitBasis),
}

impl AllocationBasis {
    /// Claim that the declaration is new.
    #[must_use]
    pub const fn confirmed_new() -> Self {
        Self::ConfirmedNew(ConfirmedNew {
            kind: ConfirmedNewTag::Value,
        })
    }
}

/// The claim that a declaration is gone from a complete owning inventory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompleteAbsence {
    kind: CompleteAbsenceTag,
}

/// Keep an Intent ID across a change to its declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Continuation {
    kind: ContinueTag,
    intent_id: MessageIntentId,
    from: Occurrence,
    to: Occurrence,
    basis: ContinuationBasis,
}

impl Continuation {
    /// Borrow the base declaration the ID continues from.
    #[must_use]
    pub const fn from(&self) -> &Occurrence {
        &self.from
    }

    /// Borrow the current declaration the ID continues to.
    #[must_use]
    pub const fn to(&self) -> &Occurrence {
        &self.to
    }

    /// Borrow the claimed basis.
    #[must_use]
    pub const fn basis(&self) -> &ContinuationBasis {
        &self.basis
    }
}

/// Give a new Intent ID to a current declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Allocation {
    kind: AllocateTag,
    intent_id: MessageIntentId,
    to: Occurrence,
    basis: AllocationBasis,
}

impl Allocation {
    /// Borrow the current declaration the new ID is given to.
    #[must_use]
    pub const fn to(&self) -> &Occurrence {
        &self.to
    }

    /// Borrow the claimed basis.
    #[must_use]
    pub const fn basis(&self) -> &AllocationBasis {
        &self.basis
    }
}

/// Retire an Intent ID whose declaration is gone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Retirement {
    kind: RetireTag,
    intent_id: MessageIntentId,
    from: Occurrence,
    basis: CompleteAbsence,
}

impl Retirement {
    /// Borrow the base declaration the ID last held.
    #[must_use]
    pub const fn from(&self) -> &Occurrence {
        &self.from
    }
}

/// Make a retired Intent ID active again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Restoration {
    kind: RestoreTag,
    intent_id: MessageIntentId,
    from: Occurrence,
    to: Occurrence,
    basis: ExplicitBasis,
}

impl Restoration {
    /// Borrow the declaration the retired ID kept.
    #[must_use]
    pub const fn from(&self) -> &Occurrence {
        &self.from
    }

    /// Borrow the current declaration the ID returns to.
    #[must_use]
    pub const fn to(&self) -> &Occurrence {
        &self.to
    }

    /// Borrow the explicit choice.
    #[must_use]
    pub const fn basis(&self) -> &ExplicitBasis {
        &self.basis
    }
}

/// One action on one Intent ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum IdentityDecision {
    Continue(Continuation),
    Allocate(Allocation),
    Retire(Retirement),
    Restore(Restoration),
}

impl IdentityDecision {
    /// Keep an ID from a base declaration to a current one.
    #[must_use]
    pub const fn continuation(
        intent_id: MessageIntentId,
        from: Occurrence,
        to: Occurrence,
        basis: ContinuationBasis,
    ) -> Self {
        Self::Continue(Continuation {
            kind: ContinueTag::Value,
            intent_id,
            from,
            to,
            basis,
        })
    }

    /// Give a new ID to a current declaration.
    ///
    /// The value is the host's, drawn once from operating-system randomness
    /// and retained in the plan. Nothing here generates one.
    #[must_use]
    pub const fn allocation(
        intent_id: MessageIntentId,
        to: Occurrence,
        basis: AllocationBasis,
    ) -> Self {
        Self::Allocate(Allocation {
            kind: AllocateTag::Value,
            intent_id,
            to,
            basis,
        })
    }

    /// Retire an ID whose declaration is gone.
    #[must_use]
    pub const fn retirement(intent_id: MessageIntentId, from: Occurrence) -> Self {
        Self::Retire(Retirement {
            kind: RetireTag::Value,
            intent_id,
            from,
            basis: CompleteAbsence {
                kind: CompleteAbsenceTag::Value,
            },
        })
    }

    /// Make a retired ID active again, by explicit choice.
    #[must_use]
    pub const fn restoration(
        intent_id: MessageIntentId,
        from: Occurrence,
        to: Occurrence,
        basis: ExplicitBasis,
    ) -> Self {
        Self::Restore(Restoration {
            kind: RestoreTag::Value,
            intent_id,
            from,
            to,
            basis,
        })
    }

    /// Borrow the Intent ID this decision acts on.
    #[must_use]
    pub const fn intent_id(&self) -> &MessageIntentId {
        match self {
            Self::Continue(decision) => &decision.intent_id,
            Self::Allocate(decision) => &decision.intent_id,
            Self::Retire(decision) => &decision.intent_id,
            Self::Restore(decision) => &decision.intent_id,
        }
    }

    /// Borrow the base declaration, absent for an allocation.
    #[must_use]
    pub const fn from(&self) -> Option<&Occurrence> {
        match self {
            Self::Continue(decision) => Some(&decision.from),
            Self::Retire(decision) => Some(&decision.from),
            Self::Restore(decision) => Some(&decision.from),
            Self::Allocate(_) => None,
        }
    }

    /// Borrow the current declaration, absent for a retirement.
    #[must_use]
    pub const fn to(&self) -> Option<&Occurrence> {
        match self {
            Self::Continue(decision) => Some(&decision.to),
            Self::Allocate(decision) => Some(&decision.to),
            Self::Restore(decision) => Some(&decision.to),
            Self::Retire(_) => None,
        }
    }

    /// Borrow the edits a `verified-edit` continuation carries.
    fn changes(&self) -> &[SourceEdit] {
        match self {
            Self::Continue(Continuation {
                basis: ContinuationBasis::VerifiedEdit(edit),
                ..
            }) => &edit.changes,
            _ => &[],
        }
    }
}

/// One replacement, in the coordinates of the bytes before the edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Replacement {
    range: ByteRange,
    text: String,
}

impl Replacement {
    /// Replace the bytes in `range` with `text`.
    #[must_use]
    pub fn new(range: ByteRange, text: &str) -> Self {
        Self {
            range,
            text: text.to_owned(),
        }
    }

    /// Return the replaced range, in `before` coordinates.
    #[must_use]
    pub const fn range(&self) -> ByteRange {
        self.range
    }

    /// Borrow the replacement text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// One unit's change between two exact snapshots.
///
/// An absent `before` is an empty starting buffer, as for a new unit, and an
/// absent `after` claims the unit is gone. Replaying the replacements in order
/// over the `before` bytes has to give exactly the `after` bytes; that, and
/// which declarations the edit carries across, is the verifier's to check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceEdit {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    before: Option<SourceSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    after: Option<SourceSnapshot>,
    replacements: Box<[Replacement]>,
}

impl SourceEdit {
    /// Record one edit.
    ///
    /// Replacements are put into `before` order, and zero-width insertions at
    /// one position are joined into one in the order given, which is how 017
    /// requires them to be recorded. Nothing else is merged or dropped.
    #[must_use]
    pub fn new(
        before: Option<SourceSnapshot>,
        after: Option<SourceSnapshot>,
        mut replacements: Vec<Replacement>,
    ) -> Self {
        // A stable sort keeps insertions at one position in the order given,
        // which is the order their text is joined in.
        replacements
            .sort_by_key(|replacement| (replacement.range.start(), replacement.range.end()));
        let mut recorded: Vec<Replacement> = Vec::with_capacity(replacements.len());
        for replacement in replacements {
            match recorded.last_mut() {
                Some(previous)
                    if previous.range.is_empty()
                        && replacement.range.is_empty()
                        && previous.range.start() == replacement.range.start() =>
                {
                    previous.text.push_str(&replacement.text);
                }
                _ => recorded.push(replacement),
            }
        }
        Self {
            before,
            after,
            replacements: recorded.into_boxed_slice(),
        }
    }

    /// Borrow the snapshot the edit starts from, absent for an empty buffer.
    #[must_use]
    pub const fn before(&self) -> Option<&SourceSnapshot> {
        self.before.as_ref()
    }

    /// Borrow the snapshot the edit ends at, absent when the unit is gone.
    #[must_use]
    pub const fn after(&self) -> Option<&SourceSnapshot> {
        self.after.as_ref()
    }

    /// Borrow the replacements, in `before` order.
    #[must_use]
    pub fn replacements(&self) -> &[Replacement] {
        &self.replacements
    }

    /// Compare two edits in 017's change-list order.
    ///
    /// The order is the before snapshot, then the after snapshot, with an
    /// absent side first. Unlike an occurrence's order, a source tuple here
    /// ends with the numeric byte length, after the digest.
    #[must_use]
    pub fn canonical_cmp(&self, other: &Self) -> Ordering {
        side_cmp(self.before.as_ref(), other.before.as_ref())
            .then_with(|| side_cmp(self.after.as_ref(), other.after.as_ref()))
    }
}

fn side_cmp(left: Option<&SourceSnapshot>, right: Option<&SourceSnapshot>) -> Ordering {
    match (left, right) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(left), Some(right)) => left
            .canonical_cmp(right)
            .then_with(|| left.byte_length().cmp(&right.byte_length())),
    }
}

/// What a lineage link records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum LineageKind {
    /// A new declaration copied from an existing one.
    Copy,
    /// One declaration divided into several.
    Split,
    /// Several declarations combined into one.
    Merge,
}

impl LineageKind {
    /// Return the exact wire spelling, used for deterministic ordering.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Copy => "copy",
            Self::Split => "split",
            Self::Merge => "merge",
        }
    }
}

/// A copy, split or merge between Intent IDs.
///
/// A link keeps that intent for later readers. It assigns no identity,
/// authorizes no retirement or restoration, and carries no translation
/// approval from one ID to another.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LineageLink {
    kind: LineageKind,
    predecessors: Box<[MessageIntentId]>,
    successors: Box<[MessageIntentId]>,
}

impl LineageLink {
    /// Record one link. Both sets are put into Intent ID order.
    #[must_use]
    pub fn new(
        kind: LineageKind,
        mut predecessors: Vec<MessageIntentId>,
        mut successors: Vec<MessageIntentId>,
    ) -> Self {
        predecessors.sort();
        successors.sort();
        Self {
            kind,
            predecessors: predecessors.into_boxed_slice(),
            successors: successors.into_boxed_slice(),
        }
    }

    /// Return what the link records.
    #[must_use]
    pub const fn kind(&self) -> LineageKind {
        self.kind
    }

    /// Borrow the IDs in the base, in Intent ID order.
    #[must_use]
    pub fn predecessors(&self) -> &[MessageIntentId] {
        &self.predecessors
    }

    /// Borrow the IDs in the result, in Intent ID order.
    #[must_use]
    pub fn successors(&self) -> &[MessageIntentId] {
        &self.successors
    }

    /// Compare two links in 017's order: kind spelling, then the predecessor
    /// set, then the successor set.
    #[must_use]
    pub fn canonical_cmp(&self, other: &Self) -> Ordering {
        self.kind
            .as_str()
            .as_bytes()
            .cmp(other.kind.as_str().as_bytes())
            .then_with(|| self.predecessors.cmp(&other.predecessors))
            .then_with(|| self.successors.cmp(&other.successors))
    }
}

/// One fully specified candidate update against one exact base.
///
/// Deserializing one reads its shape only. [`IntentRegistryUpdate::validate`]
/// checks the structural rules. Neither says the update applies to its base,
/// that its bases hold, or that anyone may publish it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IntentRegistryUpdate {
    owner: OwnerIdentity,
    #[schemars(schema_with = "crate::schema::registry_reference")]
    base: AuthoringArtifactReference,
    #[schemars(schema_with = "crate::schema::inventory_reference")]
    inventory: AuthoringArtifactReference,
    decisions: Box<[IdentityDecision]>,
    lineage_links: Box<[LineageLink]>,
}

/// Why an update is not well formed.
///
/// Each variant names one rule, so a caller can tell which one a damaged or
/// forged update broke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateFailure {
    /// The base does not name an `intent-registry`, or the inventory an
    /// `authoring-inventory`, under the schema revision and specification
    /// this reader implements.
    ReferenceKind,
    /// A decision, source edit or lineage link names an owner other than the
    /// update's.
    ForeignOwner,
    /// Two decisions act on one Intent ID.
    DuplicateDecision,
    /// Decisions are not in Intent ID order.
    DecisionsUnordered,
    /// A decision's base or current declaration does not have a declaration
    /// role.
    RoleMismatch,
    /// A decision's base or current declaration runs past the end of its
    /// unit.
    RangeOutsideSource,
    /// Two decisions give identities to one current declaration.
    SharedDeclaration,
    /// An `unchanged-snapshot` continuation names two different declarations.
    UnchangedSnapshotMoved,
    /// A source edit has neither a before nor an after snapshot.
    EmptyEdit,
    /// One unit appears twice on one side of a change list.
    DuplicateUnit,
    /// A change list is not in before/after source order.
    ChangesUnordered,
    /// A replacement runs past the end of the before snapshot. Without one,
    /// the starting buffer is empty.
    ReplacementOutsideSource,
    /// Replacements are not in before order, or overlap.
    ReplacementsUnordered,
    /// Two zero-width insertions at one position were recorded separately.
    UncombinedInsertions,
    /// A lineage link does not have the sets its kind requires: a copy one
    /// predecessor and one other successor, a split one predecessor and at
    /// least two successors, a merge at least two predecessors and one
    /// successor.
    LinkCardinality,
    /// A lineage link's predecessors or successors are not in Intent ID order,
    /// or repeat an ID.
    LinkMembersUnordered,
    /// Lineage links are not in canonical order.
    LinksUnordered,
    /// Two lineage links are the same link.
    DuplicateLink,
    /// A copy's successor is not allocated by this update.
    CopyNotAllocated,
}

impl IntentRegistryUpdate {
    /// Record one update and validate it.
    ///
    /// Decisions are put into Intent ID order and links into canonical order;
    /// a repeated decision or link is reported, never dropped.
    pub fn new(
        owner: OwnerIdentity,
        base: AuthoringArtifactReference,
        inventory: AuthoringArtifactReference,
        mut decisions: Vec<IdentityDecision>,
        mut lineage_links: Vec<LineageLink>,
    ) -> Result<Self, UpdateFailure> {
        decisions.sort_by(|left, right| left.intent_id().cmp(right.intent_id()));
        lineage_links.sort_by(LineageLink::canonical_cmp);
        let update = Self {
            owner,
            base,
            inventory,
            decisions: decisions.into_boxed_slice(),
            lineage_links: lineage_links.into_boxed_slice(),
        };
        update.validate()?;
        Ok(update)
    }

    /// Borrow the owning application or library.
    #[must_use]
    pub const fn owner(&self) -> &OwnerIdentity {
        &self.owner
    }

    /// Borrow the reference to the exact base registry.
    #[must_use]
    pub const fn base(&self) -> &AuthoringArtifactReference {
        &self.base
    }

    /// Borrow the reference to the inventory the update was planned from.
    #[must_use]
    pub const fn inventory(&self) -> &AuthoringArtifactReference {
        &self.inventory
    }

    /// Borrow the decisions, in Intent ID order.
    #[must_use]
    pub fn decisions(&self) -> &[IdentityDecision] {
        &self.decisions
    }

    /// Borrow the lineage links, in canonical order.
    #[must_use]
    pub fn lineage_links(&self) -> &[LineageLink] {
        &self.lineage_links
    }

    /// Return whether the update changes nothing.
    ///
    /// 017 publishes no new registry for such an update: the base is reused.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.decisions.is_empty() && self.lineage_links.is_empty()
    }

    /// Return every source edit the decisions carry, in decision order.
    pub(crate) fn source_edits(&self) -> impl Iterator<Item = &SourceEdit> {
        self.decisions.iter().flat_map(IdentityDecision::changes)
    }

    /// Check every structural rule a well-formed update satisfies.
    pub fn validate(&self) -> Result<(), UpdateFailure> {
        if !self.base.is_current(ArtifactKind::IntentRegistry)
            || !self.inventory.is_current(ArtifactKind::AuthoringInventory)
        {
            return Err(UpdateFailure::ReferenceKind);
        }
        for decision in &*self.decisions {
            self.validate_decision(decision)?;
        }
        for pair in self.decisions.windows(2) {
            match pair[0].intent_id().cmp(pair[1].intent_id()) {
                Ordering::Less => {}
                Ordering::Equal => return Err(UpdateFailure::DuplicateDecision),
                Ordering::Greater => return Err(UpdateFailure::DecisionsUnordered),
            }
        }
        let mut current: Vec<&Occurrence> = self
            .decisions
            .iter()
            .filter_map(IdentityDecision::to)
            .collect();
        current.sort_unstable_by(|left, right| same_declaration_order(left, right));
        if current.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(UpdateFailure::SharedDeclaration);
        }

        for link in &*self.lineage_links {
            self.validate_link(link)?;
        }
        for pair in self.lineage_links.windows(2) {
            match pair[0].canonical_cmp(&pair[1]) {
                Ordering::Less => {}
                Ordering::Equal => return Err(UpdateFailure::DuplicateLink),
                Ordering::Greater => return Err(UpdateFailure::LinksUnordered),
            }
        }
        Ok(())
    }

    fn validate_decision(&self, decision: &IdentityDecision) -> Result<(), UpdateFailure> {
        if decision.intent_id().owner() != &self.owner {
            return Err(UpdateFailure::ForeignOwner);
        }
        for occurrence in decision.from().into_iter().chain(decision.to()) {
            if occurrence.source().owner() != &self.owner {
                return Err(UpdateFailure::ForeignOwner);
            }
            if !occurrence.role().is_declaration() {
                return Err(UpdateFailure::RoleMismatch);
            }
            // Decoding does not run `Occurrence::new`, so a decoded range has
            // not been checked against its unit yet.
            if occurrence.range().end() > occurrence.source().byte_length() {
                return Err(UpdateFailure::RangeOutsideSource);
            }
        }
        if let IdentityDecision::Continue(continuation) = decision {
            match &continuation.basis {
                ContinuationBasis::UnchangedSnapshot(_) if continuation.from != continuation.to => {
                    return Err(UpdateFailure::UnchangedSnapshotMoved);
                }
                ContinuationBasis::VerifiedEdit(edit) => self.validate_changes(&edit.changes)?,
                ContinuationBasis::UnchangedSnapshot(_) | ContinuationBasis::Explicit(_) => {}
            }
        }
        Ok(())
    }

    fn validate_changes(&self, changes: &[SourceEdit]) -> Result<(), UpdateFailure> {
        let mut before_units: BTreeSet<&Token> = BTreeSet::new();
        let mut after_units: BTreeSet<&Token> = BTreeSet::new();
        for edit in changes {
            validate_edit(edit, &self.owner)?;
            for (side, units) in [
                (&edit.before, &mut before_units),
                (&edit.after, &mut after_units),
            ] {
                if let Some(snapshot) = side {
                    if !units.insert(snapshot.unit()) {
                        return Err(UpdateFailure::DuplicateUnit);
                    }
                }
            }
        }
        // With no unit repeated on either side, two edits can no longer
        // compare equal, so anything but strictly increasing is out of order.
        if changes
            .windows(2)
            .any(|pair| pair[0].canonical_cmp(&pair[1]) != Ordering::Less)
        {
            return Err(UpdateFailure::ChangesUnordered);
        }
        Ok(())
    }

    fn validate_link(&self, link: &LineageLink) -> Result<(), UpdateFailure> {
        for id in link.predecessors.iter().chain(&*link.successors) {
            if id.owner() != &self.owner {
                return Err(UpdateFailure::ForeignOwner);
            }
        }
        for set in [&link.predecessors, &link.successors] {
            if set.windows(2).any(|pair| pair[0] >= pair[1]) {
                return Err(UpdateFailure::LinkMembersUnordered);
            }
        }
        let (predecessors, successors) = (link.predecessors.len(), link.successors.len());
        let shaped = match link.kind {
            LineageKind::Copy => {
                predecessors == 1 && successors == 1 && link.predecessors[0] != link.successors[0]
            }
            LineageKind::Split => predecessors == 1 && successors >= 2,
            LineageKind::Merge => predecessors >= 2 && successors == 1,
        };
        if !shaped {
            return Err(UpdateFailure::LinkCardinality);
        }
        if link.kind == LineageKind::Copy {
            let successor = &link.successors[0];
            let allocated = self
                .decisions
                .binary_search_by(|decision| decision.intent_id().cmp(successor))
                .is_ok_and(|index| matches!(self.decisions[index], IdentityDecision::Allocate(_)));
            if !allocated {
                return Err(UpdateFailure::CopyNotAllocated);
            }
        }
        Ok(())
    }
}

/// Check one edit on its own: a side to start or end at, one owner, and
/// replacements that can be replayed as recorded.
///
/// A change list adds its own rules on top, one unit per side and its order.
/// An edit a host supplies as evidence is held to these alone.
pub(crate) fn validate_edit(edit: &SourceEdit, owner: &OwnerIdentity) -> Result<(), UpdateFailure> {
    if edit.before.is_none() && edit.after.is_none() {
        return Err(UpdateFailure::EmptyEdit);
    }
    if edit
        .before
        .iter()
        .chain(&edit.after)
        .any(|snapshot| snapshot.owner() != owner)
    {
        return Err(UpdateFailure::ForeignOwner);
    }
    validate_replacements(edit)
}

/// Check that an edit's replacements can be replayed as recorded.
///
/// In order means each replacement starts at or after the end of the one
/// before it. That also puts an insertion ahead of a replacement starting at
/// the same position, and leaves two insertions at one position as the only
/// case it allows that 017 forbids, which is checked by its own name.
fn validate_replacements(edit: &SourceEdit) -> Result<(), UpdateFailure> {
    let length = edit.before.as_ref().map_or(0, SourceSnapshot::byte_length);
    let mut previous: Option<ByteRange> = None;
    for replacement in &*edit.replacements {
        let range = replacement.range;
        if range.end() > length {
            return Err(UpdateFailure::ReplacementOutsideSource);
        }
        if let Some(previous) = previous {
            if previous.is_empty() && range.is_empty() && previous.start() == range.start() {
                return Err(UpdateFailure::UncombinedInsertions);
            }
            if range.start() < previous.end() {
                return Err(UpdateFailure::ReplacementsUnordered);
            }
        }
        previous = Some(range);
    }
    Ok(())
}
