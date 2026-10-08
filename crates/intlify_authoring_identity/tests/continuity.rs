// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Continuity, newness and absence, checked against real source.
//!
//! Every case builds its inventories from source text the way a Producer
//! does, and gives the verifier only what a host would have: the bytes of
//! each snapshot, the edits between them, and the units of the owning
//! scope. The cases are design 016's: a file moved with evidence of the move,
//! the same text in two files without it, a copy beside its original, and
//! declarations that cannot be told apart.

mod support;

use intlify_authoring::{
    AdmittedInventory, ByteRange, Completeness, OccurrenceRole, Token, VersionedIdentity,
};
use intlify_authoring_identity::{
    verify_bases, AdmittedRegistry, AdmittedUpdate, AllocationBasis, BasisGap, BasisVerdict,
    ContinuationBasis, ContinuityInputs, IdentityDecision, IntentRegistryUpdate, LineageKind,
    LineageLink, PreviousUpdate, Replacement, RetainedSources, SourceEdit, EDIT_REPLAY_PROFILE,
    EDIT_REPLAY_REVISION,
};
use support::{
    applied, declaration, genesis, id, inventory_of, inventory_with_role, limits, owner,
    sealed_update, Unit,
};

const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const C: &str = "cccccccccccccccccccccccccccccccc";

/// A base with history: every declaration of `units` allocated from a
/// genesis, and the update and inventory that produced it.
struct Base {
    registry: AdmittedRegistry,
    update: AdmittedUpdate,
    inventory: AdmittedInventory,
}

impl Base {
    fn of(units: &[&Unit], ids: &[&str]) -> Self {
        let root = genesis("0123456789abcdef0123456789abcdef");
        let inventory = inventory_of(units, Completeness::Complete);
        let allocations = inventory
            .inventory()
            .declarations()
            .iter()
            .zip(ids)
            .map(|(facts, value)| {
                IdentityDecision::allocation(
                    id(value),
                    facts.occurrence().clone(),
                    AllocationBasis::confirmed_new(),
                )
            })
            .collect();
        let update = sealed_update(
            IntentRegistryUpdate::new(
                owner(),
                root.reference(),
                inventory.reference(),
                allocations,
                vec![],
            )
            .unwrap(),
        );
        let registry = applied(&root, &update, &inventory);
        Self {
            registry,
            update,
            inventory,
        }
    }

    /// Plan an update against this base.
    fn plan(
        &self,
        inventory: &AdmittedInventory,
        decisions: Vec<IdentityDecision>,
        links: Vec<LineageLink>,
    ) -> AdmittedUpdate {
        sealed_update(
            IntentRegistryUpdate::new(
                owner(),
                self.registry.reference(),
                inventory.reference(),
                decisions,
                links,
            )
            .unwrap(),
        )
    }

    /// Check the bases of an update against this base.
    fn verdicts(
        &self,
        plan: &AdmittedUpdate,
        inventory: &AdmittedInventory,
        units: &[&Unit],
        edits: &[SourceEdit],
        membership: &[&str],
    ) -> Vec<BasisVerdict> {
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
        let inputs = ContinuityInputs {
            sources: &sources,
            edits,
            membership: &membership,
            previous: Some(PreviousUpdate {
                update: &self.update,
                inventory: &self.inventory,
            }),
        };
        verify_bases(&self.registry, plan, inventory, &inputs, &limits())
            .unwrap()
            .verdicts()
            .iter()
            .map(|(_, verdict)| *verdict)
            .collect()
    }
}

fn edit_replay(changes: Vec<SourceEdit>) -> ContinuationBasis {
    ContinuationBasis::verified_edit(
        VersionedIdentity::literal(EDIT_REPLAY_PROFILE, EDIT_REPLAY_REVISION),
        changes,
    )
}

/// An edit that leaves the bytes as they were, from one unit to another.
fn renamed(from: &Unit, to: &Unit) -> SourceEdit {
    SourceEdit::new(Some(from.snapshot()), Some(to.snapshot()), vec![])
}

/// An edit that creates a unit from nothing.
fn created(unit: &Unit) -> SourceEdit {
    SourceEdit::new(
        None,
        Some(unit.snapshot()),
        vec![Replacement::new(ByteRange::new(0, 0).unwrap(), &unit.text)],
    )
}

fn gap(gap: BasisGap) -> BasisVerdict {
    BasisVerdict::Unproven(gap)
}

const PAY: &str = "export const pay = intent('Pay now')\n";

#[test]
fn a_file_moved_with_evidence_of_the_move_keeps_its_ids() {
    // Design 016's example: the declaration in checkout.js moves to
    // payment.js. The edit between the two snapshots establishes it.
    let checkout = Unit::new("checkout.js", "1", PAY);
    let base = Base::of(&[&checkout], &[A]);
    let payment = Unit::new("payment.js", "1", PAY);
    let current = inventory_of(&[&payment], Completeness::Complete);
    let plan = base.plan(
        &current,
        vec![IdentityDecision::continuation(
            id(A),
            declaration(&base.inventory, &checkout, 0),
            declaration(&current, &payment, 0),
            edit_replay(vec![renamed(&checkout, &payment)]),
        )],
        vec![],
    );
    assert_eq!(
        base.verdicts(
            &plan,
            &current,
            &[&checkout, &payment],
            &[],
            &["payment.js"]
        ),
        [BasisVerdict::Proven]
    );
}

#[test]
fn the_same_text_in_two_files_is_not_evidence_of_anything() {
    // checkout.js stays as it was and payment.js appears with the same
    // text. Finding 'Pay now' in both does not say which is which.
    let checkout = Unit::new("checkout.js", "1", PAY);
    let base = Base::of(&[&checkout], &[A]);
    let payment = Unit::new("payment.js", "1", PAY);
    let current = inventory_of(&[&checkout, &payment], Completeness::Complete);
    let units = [&checkout, &payment];
    let membership = ["checkout.js", "payment.js"];

    // Labelling the copy new proves nothing without an account of it.
    let labelled = base.plan(
        &current,
        vec![IdentityDecision::allocation(
            id(B),
            declaration(&current, &payment, 0),
            AllocationBasis::confirmed_new(),
        )],
        vec![],
    );
    assert_eq!(
        base.verdicts(&labelled, &current, &units, &[], &membership),
        [gap(BasisGap::NewUnproven)]
    );
    // The host's account that payment.js was created from nothing shows it.
    assert_eq!(
        base.verdicts(
            &labelled,
            &current,
            &units,
            &[created(&payment)],
            &membership
        ),
        [BasisVerdict::Proven]
    );

    // Claiming the move while checkout.js is still there: the unchanged
    // snapshot is a second account of where pay went, and pay's old place
    // is not new either.
    let claimed = base.plan(
        &current,
        vec![
            IdentityDecision::continuation(
                id(A),
                declaration(&base.inventory, &checkout, 0),
                declaration(&current, &payment, 0),
                edit_replay(vec![renamed(&checkout, &payment)]),
            ),
            IdentityDecision::allocation(
                id(B),
                declaration(&current, &checkout, 0),
                AllocationBasis::confirmed_new(),
            ),
        ],
        vec![],
    );
    assert_eq!(
        base.verdicts(&claimed, &current, &units, &[], &membership),
        [gap(BasisGap::Ambiguous), gap(BasisGap::NewUnproven)]
    );
}

#[test]
fn a_copy_beside_its_original_is_new_and_the_original_continues() {
    let first = Unit::new("app.js", "1", "const save = intent('Save')\n");
    let base = Base::of(&[&first], &[A]);
    let second = Unit::new(
        "app.js",
        "2",
        "const save = intent('Save')\nconst again = intent('Save')\n",
    );
    let current = inventory_of(&[&second], Completeness::Complete);
    let at_end = first.text.len() as u64;
    let edit = SourceEdit::new(
        Some(first.snapshot()),
        Some(second.snapshot()),
        vec![Replacement::new(
            ByteRange::new(at_end, at_end).unwrap(),
            "const again = intent('Save')\n",
        )],
    );
    let plan = base.plan(
        &current,
        vec![
            IdentityDecision::continuation(
                id(A),
                declaration(&base.inventory, &first, 0),
                declaration(&current, &second, 0),
                edit_replay(vec![edit]),
            ),
            IdentityDecision::allocation(
                id(B),
                declaration(&current, &second, 1),
                AllocationBasis::confirmed_new(),
            ),
        ],
        vec![LineageLink::new(
            LineageKind::Copy,
            vec![id(A)],
            vec![id(B)],
        )],
    );
    assert_eq!(
        base.verdicts(&plan, &current, &[&first, &second], &[], &["app.js"]),
        [BasisVerdict::Proven, BasisVerdict::Proven]
    );
}

#[test]
fn a_wording_edit_continues_and_an_edit_at_the_boundary_does_not() {
    let first = Unit::new("app.js", "1", "intent('Save')\n");
    let base = Base::of(&[&first], &[A]);
    let check = |second: &Unit, replacement: Replacement| {
        let current = inventory_of(&[second], Completeness::Complete);
        let edit = SourceEdit::new(
            Some(first.snapshot()),
            Some(second.snapshot()),
            vec![replacement],
        );
        let plan = base.plan(
            &current,
            vec![IdentityDecision::continuation(
                id(A),
                declaration(&base.inventory, &first, 0),
                declaration(&current, second, 0),
                edit_replay(vec![edit]),
            )],
            vec![],
        );
        base.verdicts(&plan, &current, &[&first, second], &[], &["app.js"])
    };
    // 'Save' becomes 'Saved' inside the literal: the same message, worded
    // differently, keeps its ID.
    let worded = Unit::new("app.js", "2", "intent('Saved')\n");
    assert_eq!(
        check(
            &worded,
            Replacement::new(ByteRange::new(12, 12).unwrap(), "d")
        ),
        [BasisVerdict::Proven]
    );
    // A line break goes in right before the opening quote: the edit touches
    // the declaration's start, and no rule reads where it went.
    let broken = Unit::new("app.js", "2", "intent(\n'Save')\n");
    assert_eq!(
        check(
            &broken,
            Replacement::new(ByteRange::new(7, 7).unwrap(), "\n")
        ),
        [gap(BasisGap::EditAtBoundary)]
    );
}

#[test]
fn two_old_declarations_carried_onto_one_are_ambiguous() {
    // app.js and nav.js held the same text, and both were merged into
    // common.js. Each edit replays exactly, and each carries its declaration
    // onto the one in common.js.
    let app = Unit::new("app.js", "1", "intent('Save')\n");
    let nav = Unit::new("nav.js", "1", "intent('Save')\n");
    let base = Base::of(&[&app, &nav], &[A, B]);
    let common = Unit::new("common.js", "1", "intent('Save')\n");
    let current = inventory_of(&[&common], Completeness::Complete);
    let plan = base.plan(
        &current,
        vec![
            IdentityDecision::continuation(
                id(A),
                declaration(&base.inventory, &app, 0),
                declaration(&current, &common, 0),
                edit_replay(vec![renamed(&app, &common)]),
            ),
            IdentityDecision::retirement(id(B), declaration(&base.inventory, &nav, 0)),
        ],
        vec![],
    );
    let units = [&app, &nav, &common];
    assert_eq!(
        base.verdicts(
            &plan,
            &current,
            &units,
            &[renamed(&nav, &common)],
            &["common.js"]
        ),
        [gap(BasisGap::Ambiguous), gap(BasisGap::AbsenceUnproven)]
    );
    // Without the account of nav.js, it is simply a removed unit.
    assert_eq!(
        base.verdicts(&plan, &current, &units, &[], &["common.js"]),
        [BasisVerdict::Proven, BasisVerdict::Proven]
    );
}

#[test]
fn one_old_declaration_carried_onto_two_is_ambiguous() {
    // app.js was copied to both left.js and right.js and then removed.
    let app = Unit::new("app.js", "1", "intent('Save')\n");
    let base = Base::of(&[&app], &[A]);
    let left = Unit::new("left.js", "1", "intent('Save')\n");
    let right = Unit::new("right.js", "1", "intent('Save')\n");
    let current = inventory_of(&[&left, &right], Completeness::Complete);
    let plan = base.plan(
        &current,
        vec![
            IdentityDecision::continuation(
                id(A),
                declaration(&base.inventory, &app, 0),
                declaration(&current, &left, 0),
                edit_replay(vec![renamed(&app, &left)]),
            ),
            IdentityDecision::allocation(
                id(B),
                declaration(&current, &right, 0),
                AllocationBasis::confirmed_new(),
            ),
        ],
        vec![],
    );
    let units = [&app, &left, &right];
    let membership = ["left.js", "right.js"];
    assert_eq!(
        base.verdicts(
            &plan,
            &current,
            &units,
            &[renamed(&app, &right)],
            &membership
        ),
        [gap(BasisGap::Ambiguous), gap(BasisGap::NewUnproven)]
    );
    // With an account that right.js was written from nothing instead, the
    // move to left.js stands and right.js is new.
    assert_eq!(
        base.verdicts(&plan, &current, &units, &[created(&right)], &membership),
        [BasisVerdict::Proven, BasisVerdict::Proven]
    );
}

#[test]
fn an_entry_that_is_not_seen_any_more_is_not_absent_without_evidence() {
    // nav.js changed and 'Home' is no longer found where it was. Without an
    // account of the edit, it may have moved or been reworded: neither its
    // absence nor the newness of 'Start' is shown.
    let app = Unit::new("app.js", "1", "intent('Save')\n");
    let nav = Unit::new("nav.js", "1", "intent('Home')\n");
    let base = Base::of(&[&app, &nav], &[A, B]);
    let changed = Unit::new("nav.js", "2", "intent('Start')\n");
    let current = inventory_of(&[&app, &changed], Completeness::Complete);
    let plan = base.plan(
        &current,
        vec![
            IdentityDecision::retirement(id(B), declaration(&base.inventory, &nav, 0)),
            IdentityDecision::allocation(
                id(C),
                declaration(&current, &changed, 0),
                AllocationBasis::confirmed_new(),
            ),
        ],
        vec![],
    );
    let units = [&app, &nav, &changed];
    let membership = ["app.js", "nav.js"];
    assert_eq!(
        base.verdicts(&plan, &current, &units, &[], &membership),
        [gap(BasisGap::AbsenceUnproven), gap(BasisGap::NewUnproven)]
    );
    // The edit that replaced the literal shows both.
    let replaced = SourceEdit::new(
        Some(nav.snapshot()),
        Some(changed.snapshot()),
        vec![Replacement::new(ByteRange::new(7, 13).unwrap(), "'Start'")],
    );
    assert_eq!(
        base.verdicts(&plan, &current, &units, &[replaced], &membership),
        [BasisVerdict::Proven, BasisVerdict::Proven]
    );
}

#[test]
fn a_declaration_whose_role_changed_is_not_the_same_declaration() {
    // The bytes did not change, but the declaration is now read as a UI
    // literal. Landing on the same range is not landing on the same
    // declaration.
    let first = Unit::new("app.js", "1", "intent('Save')\n");
    let base = Base::of(&[&first], &[A]);
    let second = Unit::new("app.js", "2", "intent('Save')\n");
    let current = inventory_with_role(
        &[&second],
        Completeness::Complete,
        OccurrenceRole::UiLiteral,
    );
    let plan = base.plan(
        &current,
        vec![IdentityDecision::continuation(
            id(A),
            declaration(&base.inventory, &first, 0),
            declaration(&current, &second, 0),
            edit_replay(vec![renamed(&first, &second)]),
        )],
        vec![],
    );
    assert_eq!(
        base.verdicts(&plan, &current, &[&first, &second], &[], &["app.js"]),
        [gap(BasisGap::MappedElsewhere)]
    );
}

#[test]
fn an_id_carried_onto_a_declaration_another_id_still_holds_is_ambiguous() {
    // app.js was folded into common.js, which already held the same text
    // under its own ID and did not change. Either ID could be the one that
    // lives on.
    let app = Unit::new("app.js", "1", "intent('Save')\n");
    let common = Unit::new("common.js", "1", "intent('Save')\n");
    let base = Base::of(&[&app, &common], &[A, B]);
    let current = inventory_of(&[&common], Completeness::Complete);
    let plan = base.plan(
        &current,
        vec![
            IdentityDecision::continuation(
                id(A),
                declaration(&base.inventory, &app, 0),
                declaration(&current, &common, 0),
                edit_replay(vec![renamed(&app, &common)]),
            ),
            IdentityDecision::retirement(id(B), declaration(&base.inventory, &common, 0)),
        ],
        vec![],
    );
    assert_eq!(
        base.verdicts(&plan, &current, &[&app, &common], &[], &["common.js"]),
        [gap(BasisGap::Ambiguous), gap(BasisGap::AbsenceUnproven)]
    );
}

#[test]
fn inserted_text_in_another_file_is_not_evidence() {
    // Two new files with the same text: an account of one is no account of
    // the other, though their declarations sit at the same positions.
    let app = Unit::new("app.js", "1", "intent('Save')\n");
    let base = Base::of(&[&app], &[A]);
    let left = Unit::new("left.js", "1", "intent('Next')\n");
    let right = Unit::new("right.js", "1", "intent('Next')\n");
    let current = inventory_of(&[&app, &left, &right], Completeness::Complete);
    let plan = base.plan(
        &current,
        vec![
            IdentityDecision::allocation(
                id(B),
                declaration(&current, &left, 0),
                AllocationBasis::confirmed_new(),
            ),
            IdentityDecision::allocation(
                id(C),
                declaration(&current, &right, 0),
                AllocationBasis::confirmed_new(),
            ),
        ],
        vec![],
    );
    assert_eq!(
        base.verdicts(
            &plan,
            &current,
            &[&app, &left, &right],
            &[created(&left)],
            &["app.js", "left.js", "right.js"]
        ),
        [BasisVerdict::Proven, gap(BasisGap::NewUnproven)]
    );
}

#[test]
fn a_unit_an_edit_removes_has_to_be_gone_from_the_scope() {
    // The host's edit says nav.js was deleted, yet nav.js is still in the
    // scope at another revision. The two accounts disagree, so 'Home' is
    // not shown to be gone.
    let app = Unit::new("app.js", "1", "intent('Save')\n");
    let nav = Unit::new("nav.js", "1", "intent('Home')\n");
    let base = Base::of(&[&app, &nav], &[A, B]);
    let changed = Unit::new("nav.js", "2", "intent('Start')\n");
    let current = inventory_of(&[&app, &changed], Completeness::Complete);
    let removal = SourceEdit::new(
        Some(nav.snapshot()),
        None,
        vec![Replacement::new(
            ByteRange::new(0, nav.text.len() as u64).unwrap(),
            "",
        )],
    );
    let plan = base.plan(
        &current,
        vec![
            IdentityDecision::retirement(id(B), declaration(&base.inventory, &nav, 0)),
            IdentityDecision::allocation(
                id(C),
                declaration(&current, &changed, 0),
                AllocationBasis::confirmed_new(),
            ),
        ],
        vec![],
    );
    assert_eq!(
        base.verdicts(
            &plan,
            &current,
            &[&app, &nav, &changed],
            &[removal],
            &["app.js", "nav.js"]
        ),
        [gap(BasisGap::AbsenceUnproven), gap(BasisGap::NewUnproven)]
    );
}

#[test]
fn a_declaration_in_a_unit_that_did_not_change_is_not_gone() {
    // checkout.js is unchanged. An edit from it to payment.js that replaces
    // 'Pay now' accounts for payment.js, not for a loss in checkout.js.
    let checkout = Unit::new("checkout.js", "1", PAY);
    let base = Base::of(&[&checkout], &[A]);
    let payment = Unit::new(
        "payment.js",
        "1",
        "export const pay = intent('Pay later')\n",
    );
    let current = inventory_of(&[&checkout, &payment], Completeness::Complete);
    let reworded = SourceEdit::new(
        Some(checkout.snapshot()),
        Some(payment.snapshot()),
        vec![Replacement::new(
            ByteRange::new(26, 35).unwrap(),
            "'Pay later'",
        )],
    );
    let plan = base.plan(
        &current,
        vec![
            IdentityDecision::retirement(id(A), declaration(&base.inventory, &checkout, 0)),
            IdentityDecision::allocation(
                id(B),
                declaration(&current, &checkout, 0),
                AllocationBasis::confirmed_new(),
            ),
            IdentityDecision::allocation(
                id(C),
                declaration(&current, &payment, 0),
                AllocationBasis::confirmed_new(),
            ),
        ],
        vec![],
    );
    assert_eq!(
        base.verdicts(
            &plan,
            &current,
            &[&checkout, &payment],
            &[reworded],
            &["checkout.js", "payment.js"]
        ),
        [
            gap(BasisGap::AbsenceUnproven),
            gap(BasisGap::NewUnproven),
            BasisVerdict::Proven
        ]
    );
}
