// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 016's read-only compilation: an inventory's declarations and use
//! sites resolved against one exact registry, as Intent and reference
//! artifacts.
//!
//! Compilation reads identities; it never makes one. A declaration takes the
//! ID of the active entry that holds it exactly, or of the one entry a
//! verified edit carries onto it while nothing else claims it. That is step
//! 4 of the reconciliation procedure, decided by the same code. Any other
//! declaration, new or with a history nobody shows, leaves the scope
//! unresolved: compilation reports what an identity update has to settle and
//! returns no artifact, never a subset that reads as the whole scope. It
//! takes no candidate IDs and no way to write a registry, and it leaves its
//! inputs as they were.
//!
//! An Intent artifact carries a continuity only where its declaration is not
//! the one the registry holds, and then it is the one verified edit that
//! carried the ID. A build can use a checked edit or move that way before
//! the update recording it is published, without publishing it in secret.
//!
//! An entry whose declaration is gone does not stop compilation. Whether it
//! is to be retired is an update's question; no current declaration takes
//! its identity here.

mod scope;

use std::collections::BTreeSet;

use intlify_authoring::{
    intent_revision, AdmittedInventory, AuthoringArtifact, AuthoringInventory, Diagnostic,
    MessageIntentId,
};

pub use self::scope::{CompiledScope, IntentMismatch, ReferenceMismatch};
use crate::continuity::{
    carried_by, check_previous, check_supplied, ContinuityFailure, Evidence, PreviousUpdate,
    RetainedSources,
};
use crate::intent::{
    IntentContinuity, MessageIntentArtifact, MessageIntentBody, MessageReferenceArtifact,
    MessageReferenceBody, ReferenceTarget,
};
use crate::limits::{IdentityLimitKind, IdentityLimits};
use crate::reconcile::{
    association_missing, position, Account, Associations, Carried, DeclarationClass,
};
use crate::registry::{check_pairing, AdmittedRegistry, SourceEdit, TransitionFailure};
use crate::workspace::IdentityWorkspace;

/// The continuity evidence a compilation may read.
///
/// It is the evidence reconciliation reads, less the owning scope's
/// membership: compilation never shows a declaration new or gone, so it has
/// no use for it.
#[derive(Debug, Clone, Copy)]
pub struct CompileEvidence<'a> {
    /// The bytes of every snapshot an edit names.
    pub sources: &'a RetainedSources<'a>,
    /// How each changed unit got from the registry's snapshot to its current
    /// one. The same rules hold as for reconciliation: each unit at most once
    /// on each side, from a snapshot the registry has, to one the inventory
    /// holds.
    pub edits: &'a [SourceEdit],
    /// The update that produced the registry, absent for a genesis. Its
    /// inventory names the registry's other units, which an edit may start
    /// from.
    pub previous: Option<PreviousUpdate<'a>>,
}

/// What compiling an inventory against a registry gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Compilation {
    /// Every declaration has an identity in the registry, kept or carried.
    Compiled(Box<CompiledScope>),
    /// Some declaration has no identity compilation can give it, or inputs
    /// conflict about one. An identity update has to settle it first; no
    /// artifact of the scope is returned. The diagnostics are in 016's
    /// reporting order.
    Unresolved(Box<[Diagnostic]>),
}

/// Why compilation stopped without a result.
///
/// These are operational: the inputs cannot be compiled at all, or the
/// caller stopped the work. None of them says anything about identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileFailure {
    /// The inventory cannot be compiled against this registry: another owner
    /// or scope, or a unit that was not checked.
    Pairing(TransitionFailure),
    /// The continuity evidence is not admissible.
    Evidence(ContinuityFailure),
    /// A named bound was exceeded: the supplied edits, the diagnostics, or
    /// the targets one reference would name.
    Limit(IdentityLimitKind),
    /// An artifact, or the revision it records, could not be encoded within
    /// the encoder's capacity.
    Unsealable,
    /// The caller's probe asked compilation to stop. Nothing is established
    /// about the declarations it did reach.
    Cancelled,
}

/// Compile an inventory against an exact registry.
pub fn compile(
    registry: &AdmittedRegistry,
    inventory: &AdmittedInventory,
    evidence: &CompileEvidence<'_>,
    limits: &IdentityLimits,
    workspace: &mut IdentityWorkspace,
) -> Result<Compilation, CompileFailure> {
    compile_with_cancellation(registry, inventory, evidence, limits, workspace, &|| false)
}

/// Compile an inventory against an exact registry under a caller-owned
/// cancellation probe.
///
/// The probe is consulted between steps and before each artifact is
/// sealed. Cancelling yields no partial result, so a stopped compilation can
/// never be read as a smaller scope.
pub fn compile_with_cancellation<C>(
    registry: &AdmittedRegistry,
    inventory: &AdmittedInventory,
    evidence: &CompileEvidence<'_>,
    limits: &IdentityLimits,
    workspace: &mut IdentityWorkspace,
    cancelled: &C,
) -> Result<Compilation, CompileFailure>
where
    C: Fn() -> bool + ?Sized,
{
    workspace.clear();
    let current = inventory.inventory();
    admit(registry, current, evidence, limits)?;
    let stop = || {
        if cancelled() {
            Err(CompileFailure::Cancelled)
        } else {
            Ok(())
        }
    };

    let gathered = Evidence::gather(&[], evidence.edits, evidence.sources, current);
    let claims = gathered.claims(registry);
    let associations = Associations {
        base: registry,
        current,
        evidence: &gathered,
        claims: &claims,
    };
    workspace.classes.resize(current.declarations().len(), None);
    stop()?;
    let continued = associate(&associations, workspace);
    stop()?;
    let identities = identities(&associations, continued, workspace);
    if workspace.diagnostics.len() as u64 > limits.diagnostics {
        return Err(CompileFailure::Limit(IdentityLimitKind::Diagnostics));
    }
    if !workspace.diagnostics.is_empty() {
        workspace.diagnostics.sort_by(Diagnostic::reporting_cmp);
        return Ok(Compilation::Unresolved(
            workspace.diagnostics.drain(..).collect(),
        ));
    }

    let intents = seal_intents(registry, inventory, identities, &stop)?;
    let references = seal_references(inventory, &intents, limits, &stop)?;
    Ok(Compilation::Compiled(Box::new(CompiledScope::new(
        inventory.reference(),
        registry.reference(),
        current.completeness(),
        intents,
        references,
    ))))
}

/// Hold the inputs to the rules that make them inputs at all: the inventory
/// pairs with the registry, and the evidence is admissible against both.
fn admit(
    registry: &AdmittedRegistry,
    current: &AuthoringInventory,
    evidence: &CompileEvidence<'_>,
    limits: &IdentityLimits,
) -> Result<(), CompileFailure> {
    check_pairing(registry, current).map_err(CompileFailure::Pairing)?;
    check_previous(registry, evidence.previous).map_err(CompileFailure::Evidence)?;
    check_supplied(evidence.edits, registry, current, evidence.previous, limits)
        .map_err(CompileFailure::Evidence)
}

/// Step 4 alone: every active entry keeps its exact declaration, and one
/// whose declaration is gone continues where one edit carries it and nothing
/// competes. Competing claims are reported as they are found.
///
/// Returns the entries that continue, in declaration order.
fn associate<'a>(
    associations: &Associations<'a>,
    workspace: &mut IdentityWorkspace,
) -> Vec<Carried<'a>> {
    // Compilation takes no explicit decision, so no entry is moved by one.
    let gone = associations.retained(&BTreeSet::new(), workspace);
    let proposed = gone
        .into_iter()
        .filter_map(|entry| match associations.account(entry) {
            Account::Carried(edit, index) => Some((entry, edit, index)),
            // An entry with no continuation names no current declaration,
            // and whether it is gone is an update's question.
            Account::Gone(_) | Account::Unseen | Account::Unshown(_) => None,
        })
        .collect();
    let mut continued = associations.compete(proposed, workspace);
    continued.sort_unstable_by_key(|(_, _, index)| *index);
    continued
}

/// Read each declaration's identity off its class, and report every
/// declaration step 4 gave none.
fn identities(
    associations: &Associations<'_>,
    continued: Vec<Carried<'_>>,
    workspace: &mut IdentityWorkspace,
) -> Vec<(MessageIntentId, Option<IntentContinuity>)> {
    let declarations = associations.current.declarations();
    let snapshot = associations.base.snapshot();
    // Continued entries are in declaration order, one per continued
    // declaration, so each one's is the next.
    let mut carried = continued.into_iter();
    let mut identities = Vec::with_capacity(declarations.len());
    for (index, facts) in declarations.iter().enumerate() {
        match &workspace.classes[index] {
            Some(DeclarationClass::Retained(id)) => identities.push((id.clone(), None)),
            Some(DeclarationClass::Continued(id)) => {
                let (entry, edit, _) = carried
                    .next()
                    .expect("every continued declaration was carried");
                let continuity =
                    IntentContinuity::new(entry.declaration().clone(), carried_by(edit));
                identities.push((id.clone(), Some(continuity)));
            }
            // Step 4 gives a declaration nothing else, and a competing claim
            // was reported where it was found.
            Some(_) => {}
            None => {
                let declaration = facts.occurrence();
                let related = associations
                    .claims
                    .on(declaration)
                    .filter_map(|id| snapshot.entry(id))
                    .map(|claimant| claimant.declaration().clone())
                    .collect();
                workspace
                    .diagnostics
                    .push(association_missing(declaration.clone(), related));
            }
        }
    }
    identities
}

/// Seal one Intent artifact per declaration, in canonical declaration order.
fn seal_intents<S>(
    registry: &AdmittedRegistry,
    inventory: &AdmittedInventory,
    identities: Vec<(MessageIntentId, Option<IntentContinuity>)>,
    stop: &S,
) -> Result<Vec<MessageIntentArtifact>, CompileFailure>
where
    S: Fn() -> Result<(), CompileFailure>,
{
    let registry_reference = registry.reference();
    let inventory_reference = inventory.reference();
    let declarations = inventory.inventory().declarations();
    let mut intents = Vec::with_capacity(declarations.len());
    for (facts, (id, continuity)) in declarations.iter().zip(identities) {
        stop()?;
        // The revision is the current declaration's, never the registry's.
        let revision =
            intent_revision(facts.projection()).map_err(|_| CompileFailure::Unsealable)?;
        let body = MessageIntentBody::new(
            id,
            revision,
            inventory_reference.clone(),
            facts.occurrence().clone(),
            registry_reference.clone(),
            continuity,
        )
        .expect("an admitted registry and inventory give well-formed bodies");
        intents.push(MessageIntentArtifact::seal(body).map_err(|_| CompileFailure::Unsealable)?);
    }
    Ok(intents)
}

/// Seal one reference artifact per use site, in canonical occurrence order,
/// each naming the Intent artifacts of the declarations it may use.
///
/// A use site names one target per declaration. One that would name more
/// than a reader admits under the same bounds is refused here, so nothing
/// compilation returns is an artifact its reader cannot read.
fn seal_references<S>(
    inventory: &AdmittedInventory,
    intents: &[MessageIntentArtifact],
    limits: &IdentityLimits,
    stop: &S,
) -> Result<Vec<MessageReferenceArtifact>, CompileFailure>
where
    S: Fn() -> Result<(), CompileFailure>,
{
    let current = inventory.inventory();
    let inventory_reference = inventory.reference();
    let mut references = Vec::with_capacity(current.references().len());
    for reference in current.references() {
        stop()?;
        if reference.declarations().len() as u64 > limits.targets {
            return Err(CompileFailure::Limit(IdentityLimitKind::Targets));
        }
        let targets = reference
            .declarations()
            .iter()
            .map(|declaration| {
                let index = position(current, declaration)
                    .expect("an admitted inventory's references name its declarations");
                let intent = &intents[index];
                ReferenceTarget::new(
                    intent.body().intent_id().clone(),
                    intent.body().intent_revision().clone(),
                    intent.reference(),
                )
            })
            .collect();
        let body = MessageReferenceBody::new(
            inventory_reference.clone(),
            reference.occurrence().clone(),
            targets,
        )
        .expect("one identity per declaration gives well-formed bodies");
        references
            .push(MessageReferenceArtifact::seal(body).map_err(|_| CompileFailure::Unsealable)?);
    }
    Ok(references)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use intlify_authoring::{ByteRange, Completeness, Occurrence};

    use super::*;
    use crate::continuity::EditSetFailure;
    use crate::reconcile::detail;
    use crate::registry::fixtures::{
        declaration, genesis, genesis_of_scope, id, inventory_of, limits, Base, Chain, Unit,
    };
    use crate::registry::Replacement;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn retained<'u>(units: &[&'u Unit]) -> RetainedSources<'u> {
        RetainedSources::new(
            units
                .iter()
                .map(|unit| (unit.snapshot(), unit.text.as_bytes())),
        )
        .unwrap()
    }

    fn at(start: u64, end: u64) -> ByteRange {
        ByteRange::new(start, end).unwrap()
    }

    /// An edit from one unit revision to another.
    fn edit(before: &Unit, after: &Unit, replacements: &[(u64, u64, &str)]) -> SourceEdit {
        SourceEdit::new(
            Some(before.snapshot()),
            Some(after.snapshot()),
            replacements
                .iter()
                .map(|(start, end, text)| Replacement::new(at(*start, *end), text))
                .collect(),
        )
    }

    /// Compile with the given bytes and edits, and no previous update.
    fn run(
        registry: &AdmittedRegistry,
        inventory: &AdmittedInventory,
        sources: &RetainedSources<'_>,
        edits: &[SourceEdit],
    ) -> Result<Compilation, CompileFailure> {
        let evidence = CompileEvidence {
            sources,
            edits,
            previous: None,
        };
        compile(
            registry,
            inventory,
            &evidence,
            &limits(),
            &mut IdentityWorkspace::new(),
        )
    }

    fn compiled(result: Result<Compilation, CompileFailure>) -> CompiledScope {
        match result {
            Ok(Compilation::Compiled(scope)) => *scope,
            other => panic!("not compiled: {other:?}"),
        }
    }

    /// Each diagnostic's detail, location and related occurrences.
    fn unresolved(
        result: Result<Compilation, CompileFailure>,
    ) -> Vec<(&'static str, Occurrence, Vec<Occurrence>)> {
        match result {
            Ok(Compilation::Unresolved(diagnostics)) => diagnostics
                .iter()
                .map(|diagnostic| {
                    (
                        diagnostic.detail().unwrap().as_str(),
                        diagnostic.occurrence().unwrap().clone(),
                        diagnostic.related().to_vec(),
                    )
                })
                .collect(),
            other => panic!("not unresolved: {other:?}"),
        }
    }

    fn ids(scope: &CompiledScope) -> Vec<MessageIntentId> {
        scope
            .intents()
            .iter()
            .map(|intent| intent.body().intent_id().clone())
            .collect()
    }

    #[test]
    fn the_declarations_a_registry_holds_compile_to_their_ids_with_no_continuity() {
        let app = Unit::new("app.js", "1", "intent('Save')\nintent('Cancel')\n");
        let base = Base::of(&[&app], &[A, B]);
        let scope = compiled(run(&base.registry, &base.inventory, &retained(&[]), &[]));
        assert_eq!(scope.inventory(), &base.inventory.reference());
        assert_eq!(scope.registry(), &base.registry.reference());
        assert_eq!(scope.completeness(), Completeness::Complete);
        assert_eq!(ids(&scope), [id(A), id(B)]);

        let declarations = base.inventory.inventory().declarations();
        for (intent, facts) in scope.intents().iter().zip(declarations) {
            let body = intent.body();
            assert_eq!(body.declaration(), facts.occurrence());
            // The revision is the current declaration's own.
            assert_eq!(
                body.intent_revision(),
                &intent_revision(facts.projection()).unwrap()
            );
            assert_eq!(body.inventory(), &base.inventory.reference());
            assert_eq!(body.registry(), &base.registry.reference());
            assert_eq!(body.continuity(), None);
        }

        // One reference per use site, each naming its declaration's Intent.
        let uses = base.inventory.inventory().references();
        assert_eq!(scope.references().len(), uses.len());
        for (reference, (facts, intent)) in scope
            .references()
            .iter()
            .zip(uses.iter().zip(scope.intents()))
        {
            let body = reference.body();
            assert_eq!(body.inventory(), &base.inventory.reference());
            assert_eq!(body.occurrence(), facts.occurrence());
            let [target] = body.targets() else {
                panic!("one target");
            };
            assert_eq!(target.intent_id(), intent.body().intent_id());
            assert_eq!(target.intent_revision(), intent.body().intent_revision());
            assert_eq!(target.intent_artifact(), &intent.reference());
        }
    }

    #[test]
    fn a_verified_edit_carries_each_id_onto_its_moved_declaration() {
        let first = Unit::new("app.js", "1", "intent('Save')\nintent('Cancel')\n");
        let second = Unit::new(
            "app.js",
            "2",
            "// actions\nintent('Save')\nintent('Cancel')\n",
        );
        // The IDs run against declaration order, so each continuity has to
        // find its own entry, not the next one in ID order.
        let base = Base::of(&[&first], &[B, A]);
        let current = inventory_of(&[&second], Completeness::Complete);
        let header = edit(&first, &second, &[(0, 0, "// actions\n")]);
        let scope = compiled(run(
            &base.registry,
            &current,
            &retained(&[&first, &second]),
            std::slice::from_ref(&header),
        ));
        assert_eq!(ids(&scope), [id(B), id(A)]);
        for (nth, intent) in scope.intents().iter().enumerate() {
            let continuity = intent.body().continuity().expect("a continuity");
            assert_eq!(
                continuity.from(),
                &declaration(&base.inventory, &first, nth)
            );
            assert_eq!(continuity.basis(), &carried_by(&header));
            assert_eq!(
                intent.body().declaration(),
                &declaration(&current, &second, nth)
            );
        }

        // Without the edit nothing carries either ID, and nothing is
        // compiled.
        let missing = unresolved(run(
            &base.registry,
            &current,
            &retained(&[&first, &second]),
            &[],
        ));
        assert_eq!(
            missing,
            [
                (
                    detail::association_missing().as_str(),
                    declaration(&current, &second, 0),
                    vec![]
                ),
                (
                    detail::association_missing().as_str(),
                    declaration(&current, &second, 1),
                    vec![]
                ),
            ]
        );
    }

    #[test]
    fn a_new_wording_keeps_its_id_and_takes_a_new_revision() {
        let first = Unit::new("app.js", "1", "intent('Welcome')\n");
        let second = Unit::new("app.js", "2", "intent('Welcome back')\n");
        let base = Base::of(&[&first], &[A]);
        let current = inventory_of(&[&second], Completeness::Complete);
        // Only the inside of the literal is replaced.
        let reworded = edit(&first, &second, &[(8, 15, "Welcome back")]);
        let scope = compiled(run(
            &base.registry,
            &current,
            &retained(&[&first, &second]),
            &[reworded],
        ));
        let before = compiled(run(&base.registry, &base.inventory, &retained(&[]), &[]));
        assert_eq!(ids(&scope), [id(A)]);
        assert_ne!(
            scope.intents()[0].body().intent_revision(),
            before.intents()[0].body().intent_revision()
        );
    }

    #[test]
    fn a_declaration_nothing_carries_an_id_onto_is_unresolved_even_when_new() {
        let first = Unit::new("app.js", "1", "intent('Save')\n");
        let second = Unit::new("app.js", "2", "intent('Save')\nintent('Cancel')\n");
        let base = Base::of(&[&first], &[A]);
        let current = inventory_of(&[&second], Completeness::Complete);
        // The edit shows the cancel line was inserted, which would make it
        // new for an update. Compilation allocates nothing either way.
        let inserted = edit(&first, &second, &[(15, 15, "intent('Cancel')\n")]);
        assert_eq!(
            unresolved(run(
                &base.registry,
                &current,
                &retained(&[&first, &second]),
                &[inserted],
            )),
            [(
                detail::association_missing().as_str(),
                declaration(&current, &second, 1),
                vec![]
            )]
        );
        // A base with no history gives nothing an identity either.
        let root = genesis("0123456789abcdef0123456789abcdef");
        let empty = Unit::new("app.js", "1", "// nothing to say\n");
        let quiet = inventory_of(&[&empty], Completeness::Complete);
        let scope = compiled(run(&root, &quiet, &retained(&[]), &[]));
        assert!(scope.intents().is_empty() && scope.references().is_empty());
        assert_eq!(
            unresolved(run(&root, &base.inventory, &retained(&[]), &[])).len(),
            1
        );
    }

    #[test]
    fn an_edit_at_a_boundary_carries_nothing() {
        let first = Unit::new("app.js", "1", "intent('Save')\n");
        // The same text at a new revision, through an edit that rewrites the
        // opening quote with its neighbour.
        let second = Unit::new("app.js", "2", "intent('Save')\n");
        let base = Base::of(&[&first], &[A]);
        let current = inventory_of(&[&second], Completeness::Complete);
        let touching = edit(&first, &second, &[(6, 8, "('")]);
        assert_eq!(
            unresolved(run(
                &base.registry,
                &current,
                &retained(&[&first, &second]),
                &[touching],
            )),
            [(
                detail::association_missing().as_str(),
                declaration(&current, &second, 0),
                vec![]
            )]
        );
    }

    #[test]
    fn an_entry_carried_onto_a_declaration_another_holds_competes_for_it() {
        let app = Unit::new("app.js", "1", "intent('Save')\n");
        let other = Unit::new("other.js", "1", "intent('Save')\n");
        let base = Base::of(&[&app, &other], &[A, B]);
        // A new unit before both holds a declaration nothing carries.
        let added = Unit::new("aaa.js", "1", "intent('Hello')\n");
        let current = inventory_of(&[&added, &app], Completeness::Complete);
        // The other unit's bytes "became" the app unit's, which A still holds.
        let onto = edit(&other, &app, &[]);
        let found = unresolved(run(
            &base.registry,
            &current,
            &retained(&[&app, &other]),
            &[onto],
        ));
        // The contest was found first, and is reported after the earlier
        // unit's missing association.
        assert_eq!(
            found,
            [
                (
                    detail::association_missing().as_str(),
                    declaration(&current, &added, 0),
                    vec![]
                ),
                (
                    detail::competing_claim().as_str(),
                    declaration(&current, &app, 0),
                    vec![
                        declaration(&base.inventory, &app, 0),
                        declaration(&base.inventory, &other, 0)
                    ]
                ),
            ]
        );
    }

    #[test]
    fn the_original_of_a_copy_is_related_to_the_copy_it_gives_no_id() {
        let app = Unit::new("app.js", "1", "intent('Save')\n");
        let copy = Unit::new("copy.js", "1", "intent('Save')\n");
        let base = Base::of(&[&app], &[A]);
        let current = inventory_of(&[&app, &copy], Completeness::Complete);
        let copied = edit(&app, &copy, &[]);
        assert_eq!(
            unresolved(run(
                &base.registry,
                &current,
                &retained(&[&app, &copy]),
                &[copied],
            )),
            [(
                detail::association_missing().as_str(),
                declaration(&current, &copy, 0),
                vec![declaration(&base.inventory, &app, 0)]
            )]
        );
    }

    #[test]
    fn an_entry_whose_declaration_is_gone_does_not_stop_compilation() {
        let app = Unit::new("app.js", "1", "intent('Save')\n");
        let nav = Unit::new("nav.js", "1", "intent('Home')\n");
        let base = Base::of(&[&app, &nav], &[A, B]);
        // The nav unit left the scope with no account of it.
        let complete = inventory_of(&[&app], Completeness::Complete);
        let scope = compiled(run(&base.registry, &complete, &retained(&[]), &[]));
        assert_eq!(ids(&scope), [id(A)]);
        assert_eq!(scope.completeness(), Completeness::Complete);
        // A partial view leaves it out, and says it is partial.
        let partial = inventory_of(&[&app], Completeness::Partial);
        let scope = compiled(run(&base.registry, &partial, &retained(&[]), &[]));
        assert_eq!(ids(&scope), [id(A)]);
        assert_eq!(scope.completeness(), Completeness::Partial);
    }

    #[test]
    fn equal_text_in_separate_declarations_takes_separate_ids_and_one_revision() {
        let chain = Chain::load();
        // Revision 2 of checkout holds pay and its copy, both 'Pay now'.
        let scope = compiled(run(
            chain.registry(2),
            chain.inventory(2),
            &retained(&[]),
            &[],
        ));
        let checkout: Vec<&MessageIntentArtifact> = scope
            .intents()
            .iter()
            .filter(|intent| intent.body().declaration().source().unit().as_str() == "checkout")
            .collect();
        let [pay, copy] = checkout.as_slice() else {
            panic!("two declarations in checkout");
        };
        assert_ne!(pay.body().intent_id(), copy.body().intent_id());
        assert_eq!(pay.body().intent_revision(), copy.body().intent_revision());
        assert_ne!(pay.reference(), copy.reference());
    }

    #[test]
    fn inputs_that_cannot_be_compiled_against_the_registry_are_refused() {
        let app = Unit::new("app.js", "1", "intent('Save')\n");
        let base = Base::of(&[&app], &[A]);
        assert_eq!(
            run(
                &genesis_of_scope("another-scope"),
                &base.inventory,
                &retained(&[]),
                &[]
            ),
            Err(CompileFailure::Pairing(TransitionFailure::ScopeMismatch))
        );
        // A genesis has no update to supply.
        let root = genesis("0123456789abcdef0123456789abcdef");
        let evidence = CompileEvidence {
            sources: &retained(&[]),
            edits: &[],
            previous: Some(base.previous()),
        };
        assert_eq!(
            compile(
                &root,
                &base.inventory,
                &evidence,
                &limits(),
                &mut IdentityWorkspace::new()
            ),
            Err(CompileFailure::Evidence(
                ContinuityFailure::PreviousMismatch
            ))
        );
        // An edit that writes from nothing a unit the base already has.
        let second = Unit::new("app.js", "2", "// actions\nintent('Save')\n");
        let current = inventory_of(&[&second], Completeness::Complete);
        let from_nothing = SourceEdit::new(
            None,
            Some(second.snapshot()),
            vec![Replacement::new(at(0, 0), &second.text)],
        );
        assert_eq!(
            run(
                &base.registry,
                &current,
                &retained(&[&second]),
                &[from_nothing]
            ),
            Err(CompileFailure::Evidence(ContinuityFailure::EditSet(
                EditSetFailure::UnitNotNew
            )))
        );
        // The supplied edits are held to the caller's bounds.
        let header = edit(&app, &second, &[(0, 0, "// actions\n")]);
        let evidence = CompileEvidence {
            sources: &retained(&[&app, &second]),
            edits: std::slice::from_ref(&header),
            previous: Some(base.previous()),
        };
        let none = IdentityLimits {
            source_edits: 0,
            ..limits()
        };
        assert_eq!(
            compile(
                &base.registry,
                &current,
                &evidence,
                &none,
                &mut IdentityWorkspace::new()
            ),
            Err(CompileFailure::Evidence(ContinuityFailure::Limit(
                IdentityLimitKind::SourceEdits
            )))
        );
        // With its previous update, the same evidence compiles.
        assert!(matches!(
            compile(
                &base.registry,
                &current,
                &evidence,
                &limits(),
                &mut IdentityWorkspace::new()
            ),
            Ok(Compilation::Compiled(_))
        ));
    }

    #[test]
    fn diagnostics_are_bounded_by_the_caller() {
        let root = genesis("0123456789abcdef0123456789abcdef");
        let app = Unit::new("app.js", "1", "intent('Save')\nintent('Cancel')\n");
        let current = inventory_of(&[&app], Completeness::Complete);
        let bounded = |diagnostics| {
            let evidence = CompileEvidence {
                sources: &retained(&[]),
                edits: &[],
                previous: None,
            };
            compile(
                &root,
                &current,
                &evidence,
                &IdentityLimits {
                    diagnostics,
                    ..limits()
                },
                &mut IdentityWorkspace::new(),
            )
        };
        assert!(matches!(bounded(2), Ok(Compilation::Unresolved(found)) if found.len() == 2));
        assert_eq!(
            bounded(1),
            Err(CompileFailure::Limit(IdentityLimitKind::Diagnostics))
        );
    }

    #[test]
    fn every_reference_it_returns_stays_within_the_target_bound() {
        let app = Unit::new("app.js", "1", "intent('Save')\nintent('Cancel')\n");
        let base = Base::of(&[&app], &[A, B]);
        let bounded = |targets| IdentityLimits {
            targets,
            ..limits()
        };
        let evidence = CompileEvidence {
            sources: &retained(&[]),
            edits: &[],
            previous: None,
        };
        // Each use site names one declaration, so one target is exactly the
        // bound, and what compilation returns its reader admits.
        let exact = bounded(1);
        let scope = compiled(compile(
            &base.registry,
            &base.inventory,
            &evidence,
            &exact,
            &mut IdentityWorkspace::new(),
        ));
        for reference in scope.references() {
            let bytes = serde_json::to_vec(reference).unwrap();
            assert!(crate::intent::admit_reference(&bytes, &exact).is_ok());
        }
        assert_eq!(
            compile(
                &base.registry,
                &base.inventory,
                &evidence,
                &bounded(0),
                &mut IdentityWorkspace::new()
            ),
            Err(CompileFailure::Limit(IdentityLimitKind::Targets))
        );
    }

    #[test]
    fn a_stopped_or_reused_compilation_gives_nothing_or_the_fresh_result() {
        let app = Unit::new("app.js", "1", "intent('Save')\nintent('Cancel')\n");
        let base = Base::of(&[&app], &[A, B]);
        let sources = retained(&[]);
        let evidence = CompileEvidence {
            sources: &sources,
            edits: &[],
            previous: Some(base.previous()),
        };
        let fresh = compile(
            &base.registry,
            &base.inventory,
            &evidence,
            &limits(),
            &mut IdentityWorkspace::new(),
        );
        assert!(matches!(fresh, Ok(Compilation::Compiled(_))));

        // Two probes between the steps, then one before each of two Intents
        // and each of two references.
        let probes = Cell::new(0);
        let counting = || {
            probes.set(probes.get() + 1);
            false
        };
        let mut workspace = IdentityWorkspace::new();
        let counted = compile_with_cancellation(
            &base.registry,
            &base.inventory,
            &evidence,
            &limits(),
            &mut workspace,
            &counting,
        );
        assert_eq!(counted, fresh);
        assert_eq!(probes.get(), 6);
        for stop_at in 1..=6 {
            let seen = Cell::new(0);
            let stopping = || {
                seen.set(seen.get() + 1);
                seen.get() == stop_at
            };
            assert_eq!(
                compile_with_cancellation(
                    &base.registry,
                    &base.inventory,
                    &evidence,
                    &limits(),
                    &mut workspace,
                    &stopping,
                ),
                Err(CompileFailure::Cancelled),
                "stopped at probe {stop_at}"
            );
            // The same workspace then gives the fresh result.
            assert_eq!(
                compile(
                    &base.registry,
                    &base.inventory,
                    &evidence,
                    &limits(),
                    &mut workspace
                ),
                fresh
            );
        }

        // An unresolved run leaves nothing behind for the next one.
        let root = genesis("0123456789abcdef0123456789abcdef");
        let unsettled = CompileEvidence {
            sources: &sources,
            edits: &[],
            previous: None,
        };
        assert!(matches!(
            compile(
                &root,
                &base.inventory,
                &unsettled,
                &limits(),
                &mut workspace
            ),
            Ok(Compilation::Unresolved(_))
        ));
        assert_eq!(
            compile(
                &base.registry,
                &base.inventory,
                &evidence,
                &limits(),
                &mut workspace
            ),
            fresh
        );
    }

    #[test]
    fn compilation_leaves_its_inputs_as_they_were_and_names_only_active_ids() {
        let chain = Chain::load();
        let (registry, inventory) = (chain.registry(3).clone(), chain.inventory(3).clone());
        let scope = compiled(run(
            chain.registry(3),
            chain.inventory(3),
            &retained(&[]),
            &[],
        ));
        assert_eq!(chain.registry(3), &registry);
        assert_eq!(chain.inventory(3), &inventory);
        let active: Vec<&MessageIntentId> = registry
            .snapshot()
            .entries()
            .iter()
            .filter(|entry| entry.state() == crate::registry::EntryState::Active)
            .map(crate::registry::RegistryEntry::intent_id)
            .collect();
        assert_eq!(scope.intents().len(), active.len());
        for intent in scope.intents() {
            assert!(active.contains(&intent.body().intent_id()));
        }
    }
}
