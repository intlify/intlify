// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Reconciliation against real source: plan 021's acceptance examples.
//!
//! Every case builds its inventories from source text the way a Producer
//! does, gives reconciliation only what a host would have, and publishes a
//! plan by sealing its result, so the next case starts from the registry the
//! previous one produced. Publishing is the host's operation; here it only
//! moves the fixture along.

mod support;

use intlify_authoring::{
    AdmittedInventory, AuthoringArtifact, ByteRange, Completeness, MessageIntentId, OwnerIdentity,
    OwnerKind, Token,
};
use intlify_authoring_identity::{
    detail, reconcile, AdmittedRegistry, AdmittedUpdate, CandidateFailure, ContinuationBasis,
    ContinuityInputs, DeclarationClass, Eligibility, EntryClass, EntryState, ExplicitBasis,
    ExplicitDecision, IdentityDecision, Plan, PreviousUpdate, ReconcileFailure, ReconcileInputs,
    ReconcileWorkspace, Reconciliation, RegistryArtifact, Replacement, RetainedSources, SourceEdit,
    TransitionFailure,
};
use serde_json::json;
use support::{declaration, genesis, id, inventory, inventory_of, limits, sealed, Unit};

const GREETING: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SAVE: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const WELCOME: &str = "cccccccccccccccccccccccccccccccc";
const FRESH: &str = "dddddddddddddddddddddddddddddddd";

const APP: &str = "\
const greeting = intent('Hello')
const save = intent('Save')
const welcome = intent('Welcome')
";

/// One registry state, and the update and inventory that produced it.
struct State {
    registry: AdmittedRegistry,
    produced: Option<(AdmittedUpdate, AdmittedInventory)>,
}

impl State {
    fn genesis() -> Self {
        Self {
            registry: genesis("0123456789abcdef0123456789abcdef"),
            produced: None,
        }
    }

    /// Reconcile an inventory against this state, as a host would.
    fn reconcile(
        &self,
        current: &AdmittedInventory,
        units: &[&Unit],
        edits: &[SourceEdit],
        membership: &[&str],
        explicit: &[ExplicitDecision],
        candidates: &[MessageIntentId],
    ) -> Result<Reconciliation, ReconcileFailure> {
        let sources = RetainedSources::new(
            units
                .iter()
                .map(|unit| (unit.snapshot(), unit.text.as_bytes())),
        )
        .unwrap();
        let membership: Vec<Token> = membership
            .iter()
            .map(|unit| Token::new(unit).unwrap())
            .collect();
        let inputs = ReconcileInputs {
            evidence: ContinuityInputs {
                sources: &sources,
                edits,
                membership: &membership,
                previous: self
                    .produced
                    .as_ref()
                    .map(|(update, inventory)| PreviousUpdate { update, inventory }),
            },
            explicit,
            links: &[],
            candidates,
        };
        reconcile(
            &self.registry,
            current,
            &inputs,
            &limits(),
            &mut ReconcileWorkspace::new(),
        )
    }

    /// Publish a plan: its result becomes the current registry.
    fn publish(&self, plan: &Plan, current: &AdmittedInventory) -> Self {
        let sealed = RegistryArtifact::seal(plan.result().clone()).unwrap();
        let registry = support::registry(&serde_json::to_value(sealed).unwrap());
        assert_eq!(
            registry.snapshot().base(),
            Some(&self.registry.reference()),
            "the result names its exact base"
        );
        Self {
            registry,
            produced: Some((plan.update().clone(), current.clone())),
        }
    }
}

fn planned(result: Result<Reconciliation, ReconcileFailure>) -> Plan {
    match result {
        Ok(Reconciliation::Planned(plan)) => *plan,
        other => panic!("not planned: {other:?}"),
    }
}

fn details(result: Result<Reconciliation, ReconcileFailure>) -> Vec<&'static str> {
    match result {
        Ok(Reconciliation::Unresolved(report)) => report
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.detail().unwrap().as_str())
            .collect(),
        other => panic!("not unresolved: {other:?}"),
    }
}

/// The first registry with history: the genesis, then every declaration of
/// app.js revision 1 allocated.
fn first() -> (State, Unit, AdmittedInventory) {
    let app = Unit::new("app.js", "1", APP);
    let current = inventory_of(&[&app], Completeness::Complete);
    let root = State::genesis();
    let plan = planned(root.reconcile(
        &current,
        &[&app],
        &[],
        &["app.js"],
        &[],
        &[id(GREETING), id(SAVE), id(WELCOME)],
    ));
    let state = root.publish(&plan, &current);
    (state, app, current)
}

/// An edit of app.js that replaces one range with text.
fn edited(from: &Unit, to: &Unit, start: u64, end: u64, text: &str) -> SourceEdit {
    SourceEdit::new(
        Some(from.snapshot()),
        Some(to.snapshot()),
        vec![Replacement::new(ByteRange::new(start, end).unwrap(), text)],
    )
}

fn offset(text: &str, needle: &str) -> u64 {
    text.find(needle).unwrap() as u64
}

#[test]
fn the_first_reconciliation_allocates_every_declaration_from_the_genesis() {
    let app = Unit::new("app.js", "1", APP);
    let current = inventory_of(&[&app], Completeness::Complete);
    let plan = planned(State::genesis().reconcile(
        &current,
        &[&app],
        &[],
        &["app.js"],
        &[],
        &[id(GREETING), id(SAVE), id(WELCOME)],
    ));
    // The base has no history, so all three are new, and take the
    // candidates in canonical order.
    assert!(plan
        .update()
        .update()
        .decisions()
        .iter()
        .all(|decision| matches!(decision, IdentityDecision::Allocate(_))));
    assert!(!plan.requires_confirmation());
    let entries = plan.result().entries();
    assert_eq!(entries.len(), 3);
    for (value, nth) in [(GREETING, 0), (SAVE, 1), (WELCOME, 2)] {
        assert_eq!(
            plan.result().entry(&id(value)).unwrap().declaration(),
            &declaration(&current, &app, nth)
        );
    }
}

#[test]
fn the_same_source_again_changes_nothing() {
    let (state, app, current) = first();
    assert_eq!(
        state.reconcile(&current, &[&app], &[], &["app.js"], &[], &[]),
        Ok(Reconciliation::Unchanged)
    );
}

#[test]
fn a_wording_edit_inside_a_literal_keeps_the_id_and_moves_the_entry() {
    let (state, app, _) = first();
    let changed = Unit::new("app.js", "2", &APP.replace("'Welcome'", "'Welcome back'"));
    let current = inventory_of(&[&changed], Completeness::Complete);
    let at = offset(APP, "Welcome") + "Welcome".len() as u64;
    let edit = edited(&app, &changed, at, at, " back");
    let plan =
        planned(state.reconcile(&current, &[&app, &changed], &[edit], &["app.js"], &[], &[]));
    assert_eq!(
        plan.eligibility(),
        [
            (id(GREETING), Eligibility::Automatic),
            (id(SAVE), Eligibility::Automatic),
            (id(WELCOME), Eligibility::Automatic)
        ]
    );
    assert_eq!(
        plan.result().entry(&id(WELCOME)).unwrap().declaration(),
        &declaration(&current, &changed, 2)
    );

    // The same change without the edit: no old ID is chosen and no new one
    // is allocated, though a candidate is offered.
    assert_eq!(
        details(state.reconcile(
            &current,
            &[&app, &changed],
            &[],
            &["app.js"],
            &[],
            &[id(FRESH)],
        )),
        [
            detail::continuity_missing().as_str(),
            detail::continuity_missing().as_str(),
            detail::continuity_missing().as_str(),
            detail::new_unproven().as_str(),
            detail::new_unproven().as_str(),
            detail::new_unproven().as_str()
        ]
    );
}

#[test]
fn a_whole_unit_moved_to_another_unit_keeps_every_id() {
    let (state, app, _) = first();
    let checkout = Unit::new("checkout.js", "1", APP);
    let current = inventory_of(&[&checkout], Completeness::Complete);
    let moved = SourceEdit::new(Some(app.snapshot()), Some(checkout.snapshot()), vec![]);
    let plan = planned(state.reconcile(
        &current,
        &[&app, &checkout],
        &[moved],
        &["checkout.js"],
        &[],
        &[],
    ));
    for (value, nth) in [(GREETING, 0), (SAVE, 1), (WELCOME, 2)] {
        assert_eq!(
            plan.result().entry(&id(value)).unwrap().declaration(),
            &declaration(&current, &checkout, nth)
        );
    }
}

#[test]
fn a_second_declaration_with_the_same_text_gets_a_new_id() {
    let (state, app, _) = first();
    let text = format!("{APP}const again = intent('Welcome')\n");
    let changed = Unit::new("app.js", "2", &text);
    let current = inventory_of(&[&changed], Completeness::Complete);
    let end = APP.len() as u64;
    let edit = edited(
        &app,
        &changed,
        end,
        end,
        "const again = intent('Welcome')\n",
    );
    let plan = planned(state.reconcile(
        &current,
        &[&app, &changed],
        &[edit],
        &["app.js"],
        &[],
        &[id(FRESH)],
    ));
    // The original continues; the inserted one is new, and its ID is the
    // candidate, not the old one with the same wording.
    assert_eq!(
        plan.result().entry(&id(WELCOME)).unwrap().declaration(),
        &declaration(&current, &changed, 2)
    );
    assert_eq!(
        plan.result().entry(&id(FRESH)).unwrap().declaration(),
        &declaration(&current, &changed, 3)
    );
    assert_eq!(
        plan.classification().declarations()[3].1,
        DeclarationClass::New
    );
}

#[test]
fn a_deleted_line_retires_its_id_only_from_a_complete_view() {
    let (state, app, _) = first();
    let line = "const save = intent('Save')\n";
    let changed = Unit::new("app.js", "2", &APP.replace(line, ""));
    let start = offset(APP, line);
    let edit = edited(&app, &changed, start, start + line.len() as u64, "");

    let complete = inventory_of(&[&changed], Completeness::Complete);
    let plan = planned(state.reconcile(
        &complete,
        &[&app, &changed],
        std::slice::from_ref(&edit),
        &["app.js"],
        &[],
        &[],
    ));
    let save = plan.result().entry(&id(SAVE)).unwrap();
    assert_eq!(save.state(), EntryState::Retired);
    assert_eq!(
        save.declaration(),
        &declaration(&state.inventory(), &app, 1)
    );
    assert_eq!(
        plan.classification().entries(),
        [
            (
                id(GREETING),
                EntryClass::Continued(declaration(&complete, &changed, 0))
            ),
            (id(SAVE), EntryClass::Absent),
            (
                id(WELCOME),
                EntryClass::Continued(declaration(&complete, &changed, 1))
            )
        ]
    );

    // The same deletion in a partial view retires nothing: save stays as
    // it was.
    let partial = inventory_of(&[&changed], Completeness::Partial);
    let plan =
        planned(state.reconcile(&partial, &[&app, &changed], &[edit], &["app.js"], &[], &[]));
    let save = plan.result().entry(&id(SAVE)).unwrap();
    assert_eq!(save.state(), EntryState::Active);
    assert!(plan
        .update()
        .update()
        .decisions()
        .iter()
        .all(|decision| !matches!(decision, IdentityDecision::Retire(_))));
}

#[test]
fn a_retired_id_is_never_a_fresh_one_and_comes_back_only_by_restore() {
    let (state, app, _) = first();
    let line = "const save = intent('Save')\n";
    let removed = Unit::new("app.js", "2", &APP.replace(line, ""));
    let start = offset(APP, line);
    let current = inventory_of(&[&removed], Completeness::Complete);
    let plan = planned(state.reconcile(
        &current,
        &[&app, &removed],
        &[edited(&app, &removed, start, start + line.len() as u64, "")],
        &["app.js"],
        &[],
        &[],
    ));
    let retired = state.publish(&plan, &current);

    // The line comes back.
    let back = Unit::new("app.js", "3", APP);
    let again = inventory_of(&[&back], Completeness::Complete);
    let returned = edited(&removed, &back, start, start, line);
    let run = |explicit: &[ExplicitDecision], candidates: &[MessageIntentId]| {
        retired.reconcile(
            &again,
            &[&removed, &back],
            std::slice::from_ref(&returned),
            &["app.js"],
            explicit,
            candidates,
        )
    };
    // As new text it is new, and the retired ID is not a candidate for it.
    assert_eq!(
        run(&[], &[id(SAVE)]),
        Err(ReconcileFailure::Candidates(CandidateFailure::Collision(
            id(SAVE)
        )))
    );
    let fresh = planned(run(&[], &[id(FRESH)]));
    assert_eq!(
        fresh.result().entry(&id(FRESH)).unwrap().declaration(),
        &declaration(&again, &back, 1)
    );
    // Only an explicit restore brings the old ID back, and it waits for
    // confirmation.
    let restore = ExplicitDecision::new(
        retired.registry.reference(),
        again.reference(),
        IdentityDecision::restoration(
            id(SAVE),
            declaration(&state.inventory(), &app, 1),
            declaration(&again, &back, 1),
            ExplicitBasis::new("The save action came back.").unwrap(),
        ),
    )
    .unwrap();
    let restored = planned(run(&[restore], &[]));
    assert!(restored.requires_confirmation());
    assert_eq!(
        restored.result().entry(&id(SAVE)).unwrap().state(),
        EntryState::Active
    );
}

#[test]
fn explicit_choices_are_bound_checked_and_left_to_confirm() {
    let (state, app, _) = first();
    let changed = Unit::new("app.js", "2", &APP.replace("'Welcome'", "'Hi there'"));
    let current = inventory_of(&[&changed], Completeness::Complete);
    let start = offset(APP, "'Welcome'");
    let edit = edited(
        &app,
        &changed,
        start,
        start + "'Welcome'".len() as u64,
        "'Hi there'",
    );
    let bound = |decision| {
        ExplicitDecision::new(state.registry.reference(), current.reference(), decision).unwrap()
    };
    let reason = || ExplicitBasis::new("The same message, reworded.").unwrap();
    let keep = IdentityDecision::continuation(
        id(WELCOME),
        declaration(&state.inventory(), &app, 2),
        declaration(&current, &changed, 2),
        ContinuationBasis::Explicit(reason()),
    );
    let run = |explicit: &[ExplicitDecision]| {
        state.reconcile(
            &current,
            &[&app, &changed],
            std::slice::from_ref(&edit),
            &["app.js"],
            explicit,
            &[],
        )
    };
    // The literal was replaced whole: the edit shows no continuation, and
    // the choice to keep the ID is the host's to confirm.
    let plan = planned(run(&[bound(keep.clone())]));
    assert!(plan
        .eligibility()
        .contains(&(id(WELCOME), Eligibility::RequiresConfirmation)));

    // Another owner's ID, or the same ID decided twice, is refused.
    let foreign = MessageIntentId::retained(
        OwnerIdentity::new(OwnerKind::Application, "elsewhere").unwrap(),
        WELCOME,
    )
    .unwrap();
    let elsewhere = IdentityDecision::allocation(
        foreign,
        declaration(&current, &changed, 2),
        intlify_authoring_identity::AllocationBasis::Explicit(reason()),
    );
    assert!(details(run(&[bound(elsewhere)])).contains(&detail::foreign_owner().as_str()));
    let twice = IdentityDecision::continuation(
        id(WELCOME),
        declaration(&state.inventory(), &app, 2),
        declaration(&current, &changed, 1),
        ContinuationBasis::Explicit(reason()),
    );
    assert!(
        details(run(&[bound(keep), bound(twice)])).contains(&detail::competing_claim().as_str())
    );
}

#[test]
fn an_inventory_with_a_unit_that_was_not_checked_plans_nothing() {
    let (state, app, current) = first();
    for outcome in ["failed", "blocked"] {
        let mut broken = serde_json::to_value(current.artifact()).unwrap();
        let mut unit = broken["body"]["units"][0].clone();
        unit["source"]["unit"] = json!("broken.js");
        unit["outcome"] = json!(outcome);
        broken["body"]["units"].as_array_mut().unwrap().push(unit);
        let broken = inventory(&sealed(broken));
        assert_eq!(
            state.reconcile(&broken, &[&app], &[], &["app.js", "broken.js"], &[], &[]),
            Err(ReconcileFailure::Pairing(TransitionFailure::UncheckedUnit)),
            "a {outcome} unit"
        );
    }
}

#[test]
fn the_same_inputs_give_the_same_plan_on_any_thread() {
    let (state, app, _) = first();
    let text = format!("{APP}const again = intent('Welcome')\n");
    let changed = Unit::new("app.js", "2", &text);
    let current = inventory_of(&[&changed], Completeness::Complete);
    let end = APP.len() as u64;
    let edit = edited(
        &app,
        &changed,
        end,
        end,
        "const again = intent('Welcome')\n",
    );
    let run = || {
        state.reconcile(
            &current,
            &[&app, &changed],
            std::slice::from_ref(&edit),
            &["app.js"],
            &[],
            &[id(FRESH)],
        )
    };
    let here = run();
    let there = std::thread::scope(|scope| {
        let first = scope.spawn(run);
        let second = scope.spawn(run);
        (first.join().unwrap(), second.join().unwrap())
    });
    assert_eq!(here, there.0);
    assert_eq!(here, there.1);
}

impl State {
    /// The inventory that produced this state.
    fn inventory(&self) -> AdmittedInventory {
        self.produced.as_ref().unwrap().1.clone()
    }
}
