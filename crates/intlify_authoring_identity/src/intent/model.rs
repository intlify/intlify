// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 017's `MessageIntentBody` and `MessageReferenceBody`, and the
//! structural rules each carries.
//!
//! An Intent body records the identity one declaration has under one exact
//! registry: its Intent ID, the revision its current projection gives, the
//! inventory and registry it was compiled from, and, when the declaration is
//! not the one the registry holds, the continuity that carried the ID onto
//! it. A reference body records what one use site resolves to: for every
//! declaration it may use, that declaration's Intent ID, revision and Intent
//! artifact.
//!
//! The rules checked here hold of any well-formed body on its own. Whether a
//! body is what compiling its inventory against its registry gives is
//! compilation's question: a body naming a retired ID, a stale revision or a
//! continuity nothing proves is still well formed.

use std::cmp::Ordering;

use intlify_authoring::{
    ArtifactKind, AuthoringArtifactReference, MessageIntentId, Occurrence, OccurrenceRole,
    OwnerIdentity, SemanticDigest,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::registry::{validate_continuation, ContinuationBasis, UpdateFailure};

/// How an Intent ID reached a declaration other than the one the registry
/// holds for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IntentContinuity {
    from: Occurrence,
    basis: ContinuationBasis,
}

impl IntentContinuity {
    /// Record that an ID continues from the declaration its active entry
    /// holds, on the given basis.
    #[must_use]
    pub const fn new(from: Occurrence, basis: ContinuationBasis) -> Self {
        Self { from, basis }
    }

    /// Borrow the declaration the registry's active entry holds.
    #[must_use]
    pub const fn from(&self) -> &Occurrence {
        &self.from
    }

    /// Borrow the basis that carries the ID from there to the declaration.
    #[must_use]
    pub const fn basis(&self) -> &ContinuationBasis {
        &self.basis
    }
}

/// One declaration's persistent identity under one exact registry.
///
/// Deserializing one reads its shape only. [`MessageIntentBody::validate`]
/// checks the structural rules, and neither says the body is what compiling
/// its inventory against its registry gives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MessageIntentBody {
    intent_id: MessageIntentId,
    intent_revision: SemanticDigest,
    #[schemars(schema_with = "crate::schema::inventory_reference")]
    inventory: AuthoringArtifactReference,
    declaration: Occurrence,
    #[schemars(schema_with = "crate::schema::registry_reference")]
    registry: AuthoringArtifactReference,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    continuity: Option<IntentContinuity>,
}

/// Why an Intent body is not well formed.
///
/// Each variant names one rule, so a caller can tell which one a damaged or
/// forged body broke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentFailure {
    /// The inventory member does not name an `authoring-inventory`, or the
    /// registry member an `intent-registry`, under the schema revision and
    /// specification this reader implements.
    ReferenceKind,
    /// The declaration, or the one the continuity starts from, belongs to an
    /// owner other than the Intent ID's.
    ForeignOwner,
    /// The declaration, or the one the continuity starts from, does not have
    /// a declaration role.
    RoleMismatch,
    /// The declaration, or the one the continuity starts from, runs past the
    /// end of its unit.
    RangeOutsideSource,
    /// The continuity breaks a rule a `continue` decision between the same
    /// two declarations would break.
    Continuity(UpdateFailure),
}

impl MessageIntentBody {
    /// Record one Intent body and validate it.
    pub fn new(
        intent_id: MessageIntentId,
        intent_revision: SemanticDigest,
        inventory: AuthoringArtifactReference,
        declaration: Occurrence,
        registry: AuthoringArtifactReference,
        continuity: Option<IntentContinuity>,
    ) -> Result<Self, IntentFailure> {
        let body = Self {
            intent_id,
            intent_revision,
            inventory,
            declaration,
            registry,
            continuity,
        };
        body.validate()?;
        Ok(body)
    }

    /// Borrow the Intent ID.
    #[must_use]
    pub const fn intent_id(&self) -> &MessageIntentId {
        &self.intent_id
    }

    /// Borrow the revision the declaration's current projection gives.
    #[must_use]
    pub const fn intent_revision(&self) -> &SemanticDigest {
        &self.intent_revision
    }

    /// Borrow the reference to the inventory that holds the declaration.
    #[must_use]
    pub const fn inventory(&self) -> &AuthoringArtifactReference {
        &self.inventory
    }

    /// Borrow the declaration.
    #[must_use]
    pub const fn declaration(&self) -> &Occurrence {
        &self.declaration
    }

    /// Borrow the reference to the registry the ID is active in.
    #[must_use]
    pub const fn registry(&self) -> &AuthoringArtifactReference {
        &self.registry
    }

    /// Borrow the continuity, absent when the registry holds the declaration
    /// itself.
    #[must_use]
    pub const fn continuity(&self) -> Option<&IntentContinuity> {
        self.continuity.as_ref()
    }

    /// Check every structural rule a well-formed Intent body satisfies.
    pub fn validate(&self) -> Result<(), IntentFailure> {
        if !self.inventory.is_current(ArtifactKind::AuthoringInventory)
            || !self.registry.is_current(ArtifactKind::IntentRegistry)
        {
            return Err(IntentFailure::ReferenceKind);
        }
        let owner = self.intent_id.owner();
        declared(&self.declaration, owner)?;
        if let Some(continuity) = &self.continuity {
            declared(&continuity.from, owner)?;
            validate_continuation(
                &continuity.from,
                &self.declaration,
                &continuity.basis,
                owner,
            )
            .map_err(IntentFailure::Continuity)?;
        }
        Ok(())
    }
}

/// Check an occurrence an Intent body gives its ID to, or carries it from.
fn declared(occurrence: &Occurrence, owner: &OwnerIdentity) -> Result<(), IntentFailure> {
    if occurrence.source().owner() != owner {
        return Err(IntentFailure::ForeignOwner);
    }
    if !occurrence.role().is_declaration() {
        return Err(IntentFailure::RoleMismatch);
    }
    // Decoding does not run `Occurrence::new`, so a decoded range has not been
    // checked against its unit yet.
    if occurrence.range().end() > occurrence.source().byte_length() {
        return Err(IntentFailure::RangeOutsideSource);
    }
    Ok(())
}

/// One declaration a use site may use, as an identity: its Intent ID, its
/// revision, and the Intent artifact that records both.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[expect(clippy::struct_field_names, reason = "mirror 017's member names")]
pub struct ReferenceTarget {
    intent_id: MessageIntentId,
    intent_revision: SemanticDigest,
    #[schemars(schema_with = "crate::schema::intent_reference")]
    intent_artifact: AuthoringArtifactReference,
}

impl ReferenceTarget {
    /// Record one target.
    #[must_use]
    pub const fn new(
        intent_id: MessageIntentId,
        intent_revision: SemanticDigest,
        intent_artifact: AuthoringArtifactReference,
    ) -> Self {
        Self {
            intent_id,
            intent_revision,
            intent_artifact,
        }
    }

    /// Borrow the Intent ID.
    #[must_use]
    pub const fn intent_id(&self) -> &MessageIntentId {
        &self.intent_id
    }

    /// Borrow the revision.
    #[must_use]
    pub const fn intent_revision(&self) -> &SemanticDigest {
        &self.intent_revision
    }

    /// Borrow the reference to the Intent artifact.
    #[must_use]
    pub const fn intent_artifact(&self) -> &AuthoringArtifactReference {
        &self.intent_artifact
    }
}

/// One use site's exact finite references, resolved to identities.
///
/// The parameters the use site supplies, their expressions and their
/// evaluation order stay in the inventory the body names; they are not
/// copied here. Deserializing one reads its shape only.
/// [`MessageReferenceBody::validate`] checks the structural rules, and
/// neither says the body is what compiling its inventory gives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MessageReferenceBody {
    #[schemars(schema_with = "crate::schema::inventory_reference")]
    inventory: AuthoringArtifactReference,
    occurrence: Occurrence,
    targets: Box<[ReferenceTarget]>,
}

/// Why a reference body is not well formed.
///
/// Each variant names one rule, so a caller can tell which one a damaged or
/// forged body broke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceFailure {
    /// The inventory member does not name an `authoring-inventory`, or a
    /// target's Intent artifact a `message-intent`, under the schema revision
    /// and specification this reader implements.
    ReferenceKind,
    /// The use site does not have the reference role.
    RoleMismatch,
    /// The use site runs past the end of its unit.
    RangeOutsideSource,
    /// The body names no target. A use site names at least one declaration.
    EmptyTargets,
    /// A target's Intent ID belongs to an owner other than the use site's.
    ForeignOwner,
    /// Two targets name one Intent ID.
    DuplicateTarget,
    /// Targets are not in Intent ID order.
    TargetsUnordered,
}

impl MessageReferenceBody {
    /// Record one reference body and validate it.
    ///
    /// Targets are put into Intent ID order; a repeated ID is reported, never
    /// dropped.
    pub fn new(
        inventory: AuthoringArtifactReference,
        occurrence: Occurrence,
        mut targets: Vec<ReferenceTarget>,
    ) -> Result<Self, ReferenceFailure> {
        targets.sort_by(|left, right| left.intent_id.cmp(&right.intent_id));
        let body = Self {
            inventory,
            occurrence,
            targets: targets.into_boxed_slice(),
        };
        body.validate()?;
        Ok(body)
    }

    /// Borrow the reference to the inventory that holds the use site.
    #[must_use]
    pub const fn inventory(&self) -> &AuthoringArtifactReference {
        &self.inventory
    }

    /// Borrow the use site.
    #[must_use]
    pub const fn occurrence(&self) -> &Occurrence {
        &self.occurrence
    }

    /// Borrow the targets, in Intent ID order.
    #[must_use]
    pub fn targets(&self) -> &[ReferenceTarget] {
        &self.targets
    }

    /// Check every structural rule a well-formed reference body satisfies.
    pub fn validate(&self) -> Result<(), ReferenceFailure> {
        if !self.inventory.is_current(ArtifactKind::AuthoringInventory)
            || self.targets.iter().any(|target| {
                !target
                    .intent_artifact
                    .is_current(ArtifactKind::MessageIntent)
            })
        {
            return Err(ReferenceFailure::ReferenceKind);
        }
        if self.occurrence.role() != OccurrenceRole::Reference {
            return Err(ReferenceFailure::RoleMismatch);
        }
        // Decoding does not run `Occurrence::new`, so a decoded range has not
        // been checked against its unit yet.
        if self.occurrence.range().end() > self.occurrence.source().byte_length() {
            return Err(ReferenceFailure::RangeOutsideSource);
        }
        if self.targets.is_empty() {
            return Err(ReferenceFailure::EmptyTargets);
        }
        let owner = self.occurrence.source().owner();
        if self
            .targets
            .iter()
            .any(|target| target.intent_id.owner() != owner)
        {
            return Err(ReferenceFailure::ForeignOwner);
        }
        for pair in self.targets.windows(2) {
            match pair[0].intent_id.cmp(&pair[1].intent_id) {
                Ordering::Less => {}
                Ordering::Equal => return Err(ReferenceFailure::DuplicateTarget),
                Ordering::Greater => return Err(ReferenceFailure::TargetsUnordered),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use intlify_authoring::{
        AuthoringArtifact, ByteRange, Completeness, IntegrityDigest, OwnerKind, SourceSnapshot,
        VersionedIdentity,
    };
    use serde_json::{json, Value};

    use super::*;
    use crate::continuity::{EDIT_REPLAY_PROFILE, EDIT_REPLAY_REVISION};
    use crate::registry::fixtures::{declaration, id, inventory_of, Base, Unit};
    use crate::registry::SourceEdit;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn revision() -> SemanticDigest {
        IntegrityDigest::from_hash([7; 32])
    }

    /// A base holding `intent('Save')`, the same unit after a header went in
    /// above it, and the edit between the two.
    struct Scene {
        base: Base,
        first: Unit,
        second: Unit,
        edit: SourceEdit,
    }

    fn scene() -> Scene {
        let first = Unit::new("app.js", "1", "intent('Save')\n");
        let second = Unit::new("app.js", "2", "// actions\nintent('Save')\n");
        let edit = SourceEdit::new(
            Some(first.snapshot()),
            Some(second.snapshot()),
            vec![crate::registry::Replacement::new(
                ByteRange::new(0, 0).unwrap(),
                "// actions\n",
            )],
        );
        Scene {
            base: Base::of(&[&first], &[A]),
            first,
            second,
            edit,
        }
    }

    fn verified(edit: SourceEdit) -> ContinuationBasis {
        ContinuationBasis::verified_edit(
            VersionedIdentity::literal(EDIT_REPLAY_PROFILE, EDIT_REPLAY_REVISION),
            vec![edit],
        )
    }

    /// The Intent body for the base's own declaration, with no continuity.
    fn kept(scene: &Scene) -> MessageIntentBody {
        MessageIntentBody::new(
            id(A),
            revision(),
            scene.base.inventory.reference(),
            declaration(&scene.base.inventory, &scene.first, 0),
            scene.base.registry.reference(),
            None,
        )
        .expect("a well-formed body")
    }

    /// Read a value as a body without validating it, as decoding does.
    fn decoded<T: serde::de::DeserializeOwned>(value: Value) -> T {
        serde_json::from_value(value).expect("the closed shape")
    }

    fn another_owner() -> OwnerIdentity {
        OwnerIdentity::new(OwnerKind::Library, "storefront").unwrap()
    }

    /// The same occurrence, moved into another owner's unit.
    fn foreign(occurrence: &Occurrence) -> Occurrence {
        let source = occurrence.source();
        let snapshot = SourceSnapshot::new(
            another_owner(),
            source.unit().as_str(),
            source.revision().as_str(),
            source.grammar().clone(),
            source.byte_length(),
            source.utf8_digest().as_str(),
        )
        .unwrap();
        Occurrence::new(snapshot, occurrence.range(), occurrence.role()).unwrap()
    }

    fn with_role(occurrence: &Occurrence, role: OccurrenceRole) -> Occurrence {
        Occurrence::new(occurrence.source().clone(), occurrence.range(), role).unwrap()
    }

    /// A value whose occurrence at `path` ends one byte past its unit.
    fn past_the_end(mut value: Value, path: &[&str]) -> Value {
        let mut occurrence = &mut value;
        for member in path {
            occurrence = &mut occurrence[*member];
        }
        let length = occurrence["source"]["byteLength"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap();
        occurrence["range"]["end"] = json!((length + 1).to_string());
        value
    }

    #[test]
    fn an_intent_body_without_continuity_is_well_formed_and_reads_back() {
        let scene = scene();
        let body = kept(&scene);
        assert_eq!(body.intent_id(), &id(A));
        assert_eq!(body.intent_revision(), &revision());
        assert_eq!(body.inventory(), &scene.base.inventory.reference());
        assert_eq!(
            body.declaration(),
            &declaration(&scene.base.inventory, &scene.first, 0)
        );
        assert_eq!(body.registry(), &scene.base.registry.reference());
        assert_eq!(body.continuity(), None);
        // The absent continuity is left out of the encoding, and reading it
        // back gives the same body.
        let value = serde_json::to_value(&body).unwrap();
        assert!(value.get("continuity").is_none());
        assert_eq!(decoded::<MessageIntentBody>(value), body);
    }

    #[test]
    fn an_intent_body_with_continuity_carries_its_from_and_basis() {
        let scene = scene();
        let current = inventory_of(&[&scene.second], Completeness::Complete);
        let from = declaration(&scene.base.inventory, &scene.first, 0);
        let to = declaration(&current, &scene.second, 0);
        let continuity = IntentContinuity::new(from.clone(), verified(scene.edit.clone()));
        let body = MessageIntentBody::new(
            id(A),
            revision(),
            current.reference(),
            to,
            scene.base.registry.reference(),
            Some(continuity.clone()),
        )
        .expect("a well-formed body");
        assert_eq!(body.continuity(), Some(&continuity));
        assert_eq!(continuity.from(), &from);
        assert_eq!(continuity.basis(), &verified(scene.edit));
    }

    #[test]
    fn each_reference_member_names_its_own_kind() {
        let scene = scene();
        let body = kept(&scene);
        let mut swapped = serde_json::to_value(&body).unwrap();
        swapped["inventory"] = serde_json::to_value(scene.base.registry.reference()).unwrap();
        assert_eq!(
            decoded::<MessageIntentBody>(swapped).validate(),
            Err(IntentFailure::ReferenceKind)
        );
        let mut swapped = serde_json::to_value(&body).unwrap();
        swapped["registry"] = serde_json::to_value(scene.base.inventory.reference()).unwrap();
        assert_eq!(
            decoded::<MessageIntentBody>(swapped).validate(),
            Err(IntentFailure::ReferenceKind)
        );
        // A later schema revision names a kind this reader cannot resolve.
        let mut later = serde_json::to_value(&body).unwrap();
        later["registry"]["schemaRevision"] = json!("1");
        assert_eq!(
            decoded::<MessageIntentBody>(later).validate(),
            Err(IntentFailure::ReferenceKind)
        );
    }

    #[test]
    fn the_declaration_belongs_to_the_owner_has_a_declaration_role_and_lies_in_its_unit() {
        let scene = scene();
        let body = kept(&scene);
        let declared = body.declaration().clone();
        let with = |declaration: Occurrence| {
            MessageIntentBody::new(
                id(A),
                revision(),
                body.inventory().clone(),
                declaration,
                body.registry().clone(),
                None,
            )
        };
        assert_eq!(with(foreign(&declared)), Err(IntentFailure::ForeignOwner));
        assert_eq!(
            with(with_role(&declared, OccurrenceRole::Reference)),
            Err(IntentFailure::RoleMismatch)
        );
        let outside = past_the_end(serde_json::to_value(&body).unwrap(), &["declaration"]);
        assert_eq!(
            decoded::<MessageIntentBody>(outside).validate(),
            Err(IntentFailure::RangeOutsideSource)
        );
        // An ID of another owner is foreign to the declaration as well.
        let other = MessageIntentId::retained(another_owner(), A).unwrap();
        assert_eq!(
            MessageIntentBody::new(
                other,
                revision(),
                body.inventory().clone(),
                declared,
                body.registry().clone(),
                None,
            ),
            Err(IntentFailure::ForeignOwner)
        );
    }

    #[test]
    fn the_continuity_starts_from_a_declaration_of_the_owner_inside_its_unit() {
        let scene = scene();
        let current = inventory_of(&[&scene.second], Completeness::Complete);
        let from = declaration(&scene.base.inventory, &scene.first, 0);
        let to = declaration(&current, &scene.second, 0);
        let with = |from: Occurrence| {
            MessageIntentBody::new(
                id(A),
                revision(),
                current.reference(),
                to.clone(),
                scene.base.registry.reference(),
                Some(IntentContinuity::new(from, verified(scene.edit.clone()))),
            )
        };
        assert!(with(from.clone()).is_ok());
        assert_eq!(with(foreign(&from)), Err(IntentFailure::ForeignOwner));
        assert_eq!(
            with(with_role(&from, OccurrenceRole::Reference)),
            Err(IntentFailure::RoleMismatch)
        );
        let body = with(from).unwrap();
        let outside = past_the_end(
            serde_json::to_value(&body).unwrap(),
            &["continuity", "from"],
        );
        assert_eq!(
            decoded::<MessageIntentBody>(outside).validate(),
            Err(IntentFailure::RangeOutsideSource)
        );
    }

    #[test]
    fn a_continuity_is_shaped_like_a_continue_decision() {
        let scene = scene();
        let current = inventory_of(&[&scene.second], Completeness::Complete);
        let from = declaration(&scene.base.inventory, &scene.first, 0);
        let to = declaration(&current, &scene.second, 0);
        let with = |basis: ContinuationBasis| {
            MessageIntentBody::new(
                id(A),
                revision(),
                current.reference(),
                to.clone(),
                scene.base.registry.reference(),
                Some(IntentContinuity::new(from.clone(), basis)),
            )
        };
        // An unchanged snapshot joins a declaration to itself only.
        assert_eq!(
            with(ContinuationBasis::unchanged_snapshot()),
            Err(IntentFailure::Continuity(
                UpdateFailure::UnchangedSnapshotMoved
            ))
        );
        // The change list is held to the update's rules.
        assert_eq!(
            with(verified(SourceEdit::new(None, None, vec![]))),
            Err(IntentFailure::Continuity(UpdateFailure::EmptyEdit))
        );
        assert_eq!(
            with(ContinuationBasis::verified_edit(
                VersionedIdentity::literal(EDIT_REPLAY_PROFILE, EDIT_REPLAY_REVISION),
                vec![scene.edit.clone(), scene.edit.clone()],
            )),
            Err(IntentFailure::Continuity(UpdateFailure::DuplicateUnit))
        );
        // An explicit choice is well formed here; whether it holds is the
        // compilation's question.
        assert!(with(ContinuationBasis::Explicit(
            crate::registry::ExplicitBasis::new("Chosen by hand.").unwrap()
        ))
        .is_ok());
    }

    /// A reference body for the base's `intent('Save')` call, naming the
    /// given targets.
    fn reference_with(
        scene: &Scene,
        targets: Vec<ReferenceTarget>,
    ) -> Result<MessageReferenceBody, ReferenceFailure> {
        let call = scene.base.inventory.inventory().references()[0]
            .occurrence()
            .clone();
        MessageReferenceBody::new(scene.base.inventory.reference(), call, targets)
    }

    /// A target naming a sealed Intent artifact for an ID.
    fn target(scene: &Scene, value: &str) -> ReferenceTarget {
        let artifact = crate::intent::MessageIntentArtifact::seal(kept(scene)).unwrap();
        ReferenceTarget::new(id(value), revision(), artifact.reference())
    }

    #[test]
    fn a_reference_body_puts_its_targets_in_id_order_and_reads_back() {
        let scene = scene();
        let body = reference_with(&scene, vec![target(&scene, B), target(&scene, A)])
            .expect("a well-formed body");
        let ids: Vec<&MessageIntentId> = body
            .targets()
            .iter()
            .map(ReferenceTarget::intent_id)
            .collect();
        assert_eq!(ids, [&id(A), &id(B)]);
        assert_eq!(body.inventory(), &scene.base.inventory.reference());
        assert_eq!(
            body.occurrence(),
            scene.base.inventory.inventory().references()[0].occurrence()
        );
        let first = &body.targets()[0];
        assert_eq!(first.intent_revision(), &revision());
        assert_eq!(first.intent_artifact(), &target(&scene, A).intent_artifact);
        let value = serde_json::to_value(&body).unwrap();
        assert_eq!(decoded::<MessageReferenceBody>(value), body);
    }

    #[test]
    fn each_reference_member_of_a_reference_body_names_its_own_kind() {
        let scene = scene();
        let body = reference_with(&scene, vec![target(&scene, A)]).unwrap();
        let mut swapped = serde_json::to_value(&body).unwrap();
        swapped["inventory"] = serde_json::to_value(scene.base.registry.reference()).unwrap();
        assert_eq!(
            decoded::<MessageReferenceBody>(swapped).validate(),
            Err(ReferenceFailure::ReferenceKind)
        );
        let mut swapped = serde_json::to_value(&body).unwrap();
        swapped["targets"][0]["intentArtifact"] =
            serde_json::to_value(scene.base.inventory.reference()).unwrap();
        assert_eq!(
            decoded::<MessageReferenceBody>(swapped).validate(),
            Err(ReferenceFailure::ReferenceKind)
        );
    }

    #[test]
    fn the_use_site_is_a_reference_inside_its_unit() {
        let scene = scene();
        let call = scene.base.inventory.inventory().references()[0]
            .occurrence()
            .clone();
        let literal = declaration(&scene.base.inventory, &scene.first, 0);
        assert_eq!(
            MessageReferenceBody::new(
                scene.base.inventory.reference(),
                literal,
                vec![target(&scene, A)]
            ),
            Err(ReferenceFailure::RoleMismatch)
        );
        let body = MessageReferenceBody::new(
            scene.base.inventory.reference(),
            call,
            vec![target(&scene, A)],
        )
        .unwrap();
        let outside = past_the_end(serde_json::to_value(&body).unwrap(), &["occurrence"]);
        assert_eq!(
            decoded::<MessageReferenceBody>(outside).validate(),
            Err(ReferenceFailure::RangeOutsideSource)
        );
    }

    #[test]
    fn targets_are_a_nonempty_set_of_the_use_sites_owner_in_id_order() {
        let scene = scene();
        assert_eq!(
            reference_with(&scene, vec![]),
            Err(ReferenceFailure::EmptyTargets)
        );
        let foreign = ReferenceTarget::new(
            MessageIntentId::retained(another_owner(), A).unwrap(),
            revision(),
            target(&scene, A).intent_artifact,
        );
        assert_eq!(
            reference_with(&scene, vec![foreign]),
            Err(ReferenceFailure::ForeignOwner)
        );
        assert_eq!(
            reference_with(&scene, vec![target(&scene, A), target(&scene, A)]),
            Err(ReferenceFailure::DuplicateTarget)
        );
        // Construction sorts; only a decoded body can be out of order.
        let body = reference_with(&scene, vec![target(&scene, A), target(&scene, B)]).unwrap();
        let mut value = serde_json::to_value(&body).unwrap();
        let targets = value["targets"].as_array_mut().unwrap();
        targets.swap(0, 1);
        assert_eq!(
            decoded::<MessageReferenceBody>(value).validate(),
            Err(ReferenceFailure::TargetsUnordered)
        );
    }
}
