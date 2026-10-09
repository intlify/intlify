// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The artifacts one compilation gives, and the check of a supplied artifact
//! against them.
//!
//! A reader handed an Intent or reference artifact does not take its word
//! for anything. It compiles the same inventory against the same registry
//! with the same evidence, and the artifact has to be exactly the one that
//! compilation gives for its declaration or use site. That is replay's rule
//! for the registry kinds, a complete comparison rather than a digest, and it
//! makes every rule of compilation a rule of the check without a second copy
//! of any of them. Each difference is reported by name.

use intlify_authoring::{AuthoringArtifact, AuthoringArtifactReference, Completeness, Occurrence};

use crate::intent::{
    AdmittedIntent, AdmittedReference, MessageIntentArtifact, MessageReferenceArtifact,
};

/// The Intent and reference artifacts of one compiled scope.
///
/// A scope compiled from a partial inventory holds checked facts for that
/// smaller scope only. It is never complete build input, whatever it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledScope {
    inventory: AuthoringArtifactReference,
    registry: AuthoringArtifactReference,
    completeness: Completeness,
    intents: Box<[MessageIntentArtifact]>,
    references: Box<[MessageReferenceArtifact]>,
}

/// Why a supplied Intent artifact is not the one compilation gives for its
/// declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentMismatch {
    /// It names another inventory.
    Inventory,
    /// It names another registry.
    Registry,
    /// Its declaration is not one of the inventory's.
    UnknownDeclaration,
    /// It gives the declaration another ID than the registry does.
    Identity,
    /// Its revision is not the one the declaration's current projection
    /// gives.
    Revision,
    /// Its continuity is not the one compilation records: one where the
    /// registry holds the declaration itself, none where an edit carried the
    /// ID, or another account of how it got there. An explicit choice is never
    /// a continuity here, because the confirmation it needs cannot travel
    /// with an artifact.
    Continuity,
}

/// Why a supplied reference artifact is not the one compilation gives for its
/// use site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceMismatch {
    /// It names another inventory.
    Inventory,
    /// Its use site is not one of the inventory's references.
    UnknownReference,
    /// Its targets are not the identities of the declarations the use site
    /// may use.
    Targets,
    /// A target's revision is not its declaration's current revision.
    TargetRevision,
    /// A target names another Intent artifact than the one compiled for its
    /// declaration.
    TargetArtifact,
}

impl CompiledScope {
    pub(super) fn new(
        inventory: AuthoringArtifactReference,
        registry: AuthoringArtifactReference,
        completeness: Completeness,
        intents: Vec<MessageIntentArtifact>,
        references: Vec<MessageReferenceArtifact>,
    ) -> Self {
        Self {
            inventory,
            registry,
            completeness,
            intents: intents.into_boxed_slice(),
            references: references.into_boxed_slice(),
        }
    }

    /// Borrow the reference to the inventory that was compiled.
    #[must_use]
    pub const fn inventory(&self) -> &AuthoringArtifactReference {
        &self.inventory
    }

    /// Borrow the reference to the registry it was compiled against.
    #[must_use]
    pub const fn registry(&self) -> &AuthoringArtifactReference {
        &self.registry
    }

    /// Return whether the artifacts cover the owning scope, or only the
    /// smaller one a partial inventory analyzed.
    #[must_use]
    pub const fn completeness(&self) -> Completeness {
        self.completeness
    }

    /// Borrow the Intent artifacts, one per declaration, in canonical
    /// declaration order.
    #[must_use]
    pub fn intents(&self) -> &[MessageIntentArtifact] {
        &self.intents
    }

    /// Borrow the reference artifacts, one per use site, in canonical
    /// occurrence order.
    #[must_use]
    pub fn references(&self) -> &[MessageReferenceArtifact] {
        &self.references
    }

    /// Find the Intent artifact compiled for exactly this declaration.
    #[must_use]
    pub fn intent_for(&self, declaration: &Occurrence) -> Option<&MessageIntentArtifact> {
        // The canonical order leaves out a snapshot's declared length, so the
        // neighbour a search finds has to be compared in full.
        self.intents
            .binary_search_by(|intent| intent.body().declaration().canonical_cmp(declaration))
            .ok()
            .map(|index| &self.intents[index])
            .filter(|intent| intent.body().declaration() == declaration)
    }

    /// Find the reference artifact compiled for exactly this use site.
    #[must_use]
    pub fn reference_for(&self, occurrence: &Occurrence) -> Option<&MessageReferenceArtifact> {
        self.references
            .binary_search_by(|reference| reference.body().occurrence().canonical_cmp(occurrence))
            .ok()
            .map(|index| &self.references[index])
            .filter(|reference| reference.body().occurrence() == occurrence)
    }

    /// Check that a supplied Intent artifact is exactly the one this
    /// compilation gives for its declaration.
    pub fn check_intent(&self, supplied: &AdmittedIntent) -> Result<(), IntentMismatch> {
        let body = supplied.body();
        if body.inventory() != &self.inventory {
            return Err(IntentMismatch::Inventory);
        }
        if body.registry() != &self.registry {
            return Err(IntentMismatch::Registry);
        }
        let compiled = self
            .intent_for(body.declaration())
            .ok_or(IntentMismatch::UnknownDeclaration)?
            .body();
        if body.intent_id() != compiled.intent_id() {
            return Err(IntentMismatch::Identity);
        }
        if body.intent_revision() != compiled.intent_revision() {
            return Err(IntentMismatch::Revision);
        }
        if body.continuity() != compiled.continuity() {
            return Err(IntentMismatch::Continuity);
        }
        Ok(())
    }

    /// Check that a supplied reference artifact is exactly the one this
    /// compilation gives for its use site.
    pub fn check_reference(&self, supplied: &AdmittedReference) -> Result<(), ReferenceMismatch> {
        let body = supplied.body();
        if body.inventory() != &self.inventory {
            return Err(ReferenceMismatch::Inventory);
        }
        let compiled = self
            .reference_for(body.occurrence())
            .ok_or(ReferenceMismatch::UnknownReference)?
            .body();
        let (targets, expected) = (body.targets(), compiled.targets());
        if targets.len() != expected.len()
            || targets
                .iter()
                .zip(expected)
                .any(|(target, expected)| target.intent_id() != expected.intent_id())
        {
            return Err(ReferenceMismatch::Targets);
        }
        if targets
            .iter()
            .zip(expected)
            .any(|(target, expected)| target.intent_revision() != expected.intent_revision())
        {
            return Err(ReferenceMismatch::TargetRevision);
        }
        if targets
            .iter()
            .zip(expected)
            .any(|(target, expected)| target.intent_artifact() != expected.intent_artifact())
        {
            return Err(ReferenceMismatch::TargetArtifact);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use intlify_authoring::{ByteRange, Completeness, IntegrityDigest, Occurrence, SourceSnapshot};
    use serde::de::DeserializeOwned;
    use serde_json::{json, Value};

    use super::*;
    use crate::compile::{compile, Compilation, CompileEvidence};
    use crate::continuity::RetainedSources;
    use crate::intent::{admit_intent, admit_reference, IntentContinuity};
    use crate::registry::fixtures::{declaration, id, inventory_of, limits, Base, Chain, Unit};
    use crate::registry::{ContinuationBasis, ExplicitBasis, Replacement, SourceEdit};
    use crate::workspace::IdentityWorkspace;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const C: &str = "cccccccccccccccccccccccccccccccc";

    /// A base holding two calls, the same unit after a header went in above
    /// them, and the scopes compiled from both.
    struct Scene {
        base: Base,
        first: Unit,
        kept: CompiledScope,
        carried: CompiledScope,
    }

    fn scene() -> Scene {
        let first = Unit::new("app.js", "1", "intent('Save')\nintent('Cancel')\n");
        let second = Unit::new(
            "app.js",
            "2",
            "// actions\nintent('Save')\nintent('Cancel')\n",
        );
        let base = Base::of(&[&first], &[A, B]);
        let current = inventory_of(&[&second], Completeness::Complete);
        let header = SourceEdit::new(
            Some(first.snapshot()),
            Some(second.snapshot()),
            vec![Replacement::new(
                ByteRange::new(0, 0).unwrap(),
                "// actions\n",
            )],
        );
        let sources = RetainedSources::new([
            (first.snapshot(), first.text.as_bytes()),
            (second.snapshot(), second.text.as_bytes()),
        ])
        .unwrap();
        let run = |inventory, edits: &[SourceEdit]| {
            let evidence = CompileEvidence {
                sources: &sources,
                edits,
                previous: None,
            };
            match compile(
                &base.registry,
                inventory,
                &evidence,
                &limits(),
                &mut IdentityWorkspace::new(),
            ) {
                Ok(Compilation::Compiled(scope)) => *scope,
                other => panic!("not compiled: {other:?}"),
            }
        };
        let kept = run(&base.inventory, &[]);
        let carried = run(&current, &[header]);
        Scene {
            base,
            first,
            kept,
            carried,
        }
    }

    /// Reseal a changed Intent body the way a forger with the specification
    /// would, and admit it, so only the check against the compilation can
    /// refuse it.
    fn intent(
        scope: &CompiledScope,
        nth: usize,
        change: impl FnOnce(&mut Value),
    ) -> AdmittedIntent {
        let mut body = serde_json::to_value(scope.intents()[nth].body()).unwrap();
        change(&mut body);
        let sealed = MessageIntentArtifact::seal(decoded(body)).unwrap();
        admit_intent(&serde_json::to_vec(&sealed).unwrap(), &limits())
            .expect("an admissible artifact")
    }

    /// Reseal and admit a changed reference body the same way.
    fn reference(
        scope: &CompiledScope,
        nth: usize,
        change: impl FnOnce(&mut Value),
    ) -> AdmittedReference {
        let mut body = serde_json::to_value(scope.references()[nth].body()).unwrap();
        change(&mut body);
        let sealed = MessageReferenceArtifact::seal(decoded(body)).unwrap();
        admit_reference(&serde_json::to_vec(&sealed).unwrap(), &limits())
            .expect("an admissible artifact")
    }

    fn decoded<T: DeserializeOwned>(value: Value) -> T {
        serde_json::from_value(value).expect("the closed shape")
    }

    #[test]
    fn each_artifact_is_found_by_its_exact_occurrence_only() {
        let scene = scene();
        let scope = &scene.kept;
        for intent in scope.intents() {
            assert_eq!(scope.intent_for(intent.body().declaration()), Some(intent));
        }
        for reference in scope.references() {
            assert_eq!(
                scope.reference_for(reference.body().occurrence()),
                Some(reference)
            );
        }
        // A declaration is not a use site, and the other way round.
        let save = declaration(&scene.base.inventory, &scene.first, 0);
        assert_eq!(scope.reference_for(&save), None);
        let call = scope.references()[0].body().occurrence().clone();
        assert_eq!(scope.intent_for(&call), None);
        // A neighbour at the same position in a snapshot of another declared
        // length is a different occurrence.
        let source = save.source();
        let longer = SourceSnapshot::new(
            source.owner().clone(),
            source.unit().as_str(),
            source.revision().as_str(),
            source.grammar().clone(),
            source.byte_length() + 1,
            source.utf8_digest().as_str(),
        )
        .unwrap();
        let neighbour = Occurrence::new(longer.clone(), save.range(), save.role()).unwrap();
        assert_eq!(scope.intent_for(&neighbour), None);
        let neighbour = Occurrence::new(longer, call.range(), call.role()).unwrap();
        assert_eq!(scope.reference_for(&neighbour), None);
    }

    #[test]
    fn a_compiled_intent_passes_and_each_difference_is_named() {
        let scene = scene();
        let scope = &scene.kept;
        assert_eq!(scope.check_intent(&intent(scope, 0, |_| {})), Ok(()));
        let carried = &scene.carried;
        assert_eq!(carried.check_intent(&intent(carried, 1, |_| {})), Ok(()));

        let chain = Chain::load();
        let elsewhere =
            |member: &'static str, value: Value| intent(scope, 0, |body| body[member] = value);
        let other_inventory = serde_json::to_value(chain.inventory(1).reference()).unwrap();
        assert_eq!(
            scope.check_intent(&elsewhere("inventory", other_inventory)),
            Err(IntentMismatch::Inventory)
        );
        let other_registry = serde_json::to_value(chain.registry(1).reference()).unwrap();
        assert_eq!(
            scope.check_intent(&elsewhere("registry", other_registry)),
            Err(IntentMismatch::Registry)
        );
        let unknown = intent(scope, 0, |body| {
            body["declaration"]["range"] = json!({ "start": "0", "end": "1" });
        });
        assert_eq!(
            scope.check_intent(&unknown),
            Err(IntentMismatch::UnknownDeclaration)
        );
        let renamed = intent(scope, 0, |body| {
            body["intentId"] = serde_json::to_value(id(C)).unwrap();
        });
        assert_eq!(scope.check_intent(&renamed), Err(IntentMismatch::Identity));
        let stale = intent(scope, 0, |body| {
            body["intentRevision"] = json!(IntegrityDigest::from_hash([9; 32]).as_str());
        });
        assert_eq!(scope.check_intent(&stale), Err(IntentMismatch::Revision));
    }

    #[test]
    fn a_continuity_is_accepted_only_as_compilation_records_it() {
        let scene = scene();
        // A declaration the registry holds needs no continuity, not even an
        // unchanged one.
        let kept = &scene.kept;
        let save = kept.intents()[0].body().declaration().clone();
        let unchanged = intent(kept, 0, |body| {
            body["continuity"] = serde_json::to_value(IntentContinuity::new(
                save.clone(),
                ContinuationBasis::unchanged_snapshot(),
            ))
            .unwrap();
        });
        assert_eq!(
            kept.check_intent(&unchanged),
            Err(IntentMismatch::Continuity)
        );

        // A carried declaration needs its edit, and an explicit choice cannot
        // stand in for it: its confirmation does not travel with an artifact.
        let carried = &scene.carried;
        let bare = intent(carried, 0, |body| {
            body.as_object_mut().unwrap().remove("continuity");
        });
        assert_eq!(carried.check_intent(&bare), Err(IntentMismatch::Continuity));
        let chosen = intent(carried, 0, |body| {
            body["continuity"]["basis"] =
                serde_json::to_value(ExplicitBasis::new("Chosen by hand.").unwrap()).unwrap();
        });
        assert_eq!(
            carried.check_intent(&chosen),
            Err(IntentMismatch::Continuity)
        );
    }

    #[test]
    fn a_compiled_reference_passes_and_each_difference_is_named() {
        let scene = scene();
        let scope = &scene.kept;
        assert_eq!(scope.check_reference(&reference(scope, 0, |_| {})), Ok(()));

        let chain = Chain::load();
        let other = reference(scope, 0, |body| {
            body["inventory"] = serde_json::to_value(chain.inventory(1).reference()).unwrap();
        });
        assert_eq!(
            scope.check_reference(&other),
            Err(ReferenceMismatch::Inventory)
        );
        let unknown = reference(scope, 0, |body| {
            body["occurrence"]["range"] = json!({ "start": "0", "end": "1" });
        });
        assert_eq!(
            scope.check_reference(&unknown),
            Err(ReferenceMismatch::UnknownReference)
        );

        // Another identity, one more, or one fewer is another target set.
        let target = |value: &str| {
            json!({
                "intentId": serde_json::to_value(id(value)).unwrap(),
                "intentRevision": IntegrityDigest::from_hash([9; 32]).as_str(),
                "intentArtifact": serde_json::to_value(scope.intents()[0].reference()).unwrap(),
            })
        };
        let renamed = reference(scope, 0, |body| {
            body["targets"][0]["intentId"] = serde_json::to_value(id(C)).unwrap();
        });
        assert_eq!(
            scope.check_reference(&renamed),
            Err(ReferenceMismatch::Targets)
        );
        let more = reference(scope, 0, |body| {
            body["targets"].as_array_mut().unwrap().push(target(C));
        });
        assert_eq!(
            scope.check_reference(&more),
            Err(ReferenceMismatch::Targets)
        );
        let stale = reference(scope, 0, |body| {
            body["targets"][0]["intentRevision"] =
                json!(IntegrityDigest::from_hash([9; 32]).as_str());
        });
        assert_eq!(
            scope.check_reference(&stale),
            Err(ReferenceMismatch::TargetRevision)
        );
        // The cancel call's target, pointed at the save Intent's artifact.
        let crossed = reference(scope, 1, |body| {
            body["targets"][0]["intentArtifact"] =
                serde_json::to_value(scope.intents()[0].reference()).unwrap();
        });
        assert_eq!(
            scope.check_reference(&crossed),
            Err(ReferenceMismatch::TargetArtifact)
        );
        // The save call as the earlier revision compiled it names the same ID
        // and revision, through another Intent artifact.
        let moved = &scene.carried;
        let earlier = serde_json::to_value(scope.references()[0].body().targets()).unwrap();
        let replayed = reference(moved, 0, |body| body["targets"] = earlier);
        assert_eq!(
            moved.check_reference(&replayed),
            Err(ReferenceMismatch::TargetArtifact)
        );
    }
}
