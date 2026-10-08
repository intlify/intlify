// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Applying an update to its base, replaying a stored result, and verifying a
//! chain from an anchor.
//!
//! The committed chain's snapshots after the genesis were written by hand
//! from their base and update, so replaying each update is checked against an
//! expectation rather than against itself. Every refusal starts from that
//! chain and changes one thing about it.

mod support;

use intlify_authoring::{AdmittedInventory, AuthoringArtifact};
use intlify_authoring_identity::{
    apply, replay, verify_history, AdmittedRegistry, AdmittedUpdate, Anchor, EntryState,
    HistoryFailure, HistoryOutcome, IdentityLimitKind, IdentityLimits, IntentRegistrySnapshot,
    RegistryArtifact, RegistryEntry, RegistryIdentity, RegistryUpdateArtifact, ReplayFailure,
    RetainedHistory, SnapshotFailure, Transition, TransitionFailure,
};
use serde_json::{json, Value};
use support::{artifact, bytes, inventory, limits, owner, registry, sealed, update};

/// The committed chain, admitted.
struct Chain {
    registries: Vec<AdmittedRegistry>,
    updates: Vec<AdmittedUpdate>,
    inventories: Vec<AdmittedInventory>,
}

impl Chain {
    fn load() -> Self {
        Self {
            registries: (0..=3)
                .map(|n| registry(&artifact(&format!("registry-{n}"))))
                .collect(),
            updates: (1..=3)
                .map(|n| update(&artifact(&format!("update-{n}"))))
                .collect(),
            inventories: (1..=3)
                .map(|n| inventory(&artifact(&format!("inventory-{n}"))))
                .collect(),
        }
    }

    fn registry(&self, n: usize) -> &AdmittedRegistry {
        &self.registries[n]
    }

    fn update(&self, n: usize) -> &AdmittedUpdate {
        &self.updates[n - 1]
    }

    fn inventory(&self, n: usize) -> &AdmittedInventory {
        &self.inventories[n - 1]
    }
}

/// Point an update at another inventory, and reseal it.
fn planned_from(mut plan: Value, inventory: &AdmittedInventory) -> AdmittedUpdate {
    plan["body"]["inventory"] = serde_json::to_value(inventory.reference()).unwrap();
    update(&sealed(plan))
}

/// A decision of a committed update, by its position there.
fn decision(plan: &Value, index: usize) -> Value {
    plan["body"]["decisions"][index].clone()
}

/// An update with no decisions and no links.
fn empty_update(owner_kind: &str, base: &AdmittedRegistry, inventory: &AdmittedInventory) -> Value {
    sealed(json!({
        "kind": "intent-registry-update",
        "schemaRevision": "0",
        "authoringSpecification": {"identity": "intlify-design-016", "revision": "0"},
        "body": {
            "owner": {"kind": owner_kind, "identity": "storefront"},
            "base": base.reference(),
            "inventory": inventory.reference(),
            "decisions": [],
            "lineageLinks": [],
        },
        "integrityDigest": format!("sha256:{}", "0".repeat(64)),
    }))
}

#[test]
fn every_committed_update_reproduces_the_snapshot_written_by_hand() {
    let chain = Chain::load();
    for n in 1..=3 {
        let result = apply(chain.registry(n - 1), chain.update(n), chain.inventory(n))
            .unwrap_or_else(|failure| panic!("update-{n} does not apply: {failure:?}"));
        assert_eq!(
            result,
            Transition::Applied(Box::new(chain.registry(n).snapshot().clone())),
            "update-{n}"
        );
        assert_eq!(
            replay(
                chain.registry(n - 1),
                chain.update(n),
                chain.inventory(n),
                chain.registry(n)
            ),
            Ok(()),
            "registry-{n}"
        );
    }
}

#[test]
fn a_chain_verifies_from_the_genesis_or_from_a_snapshot_the_host_accepted() {
    let chain = Chain::load();
    let retained =
        RetainedHistory::new(&chain.registries, &chain.updates, &chain.inventories).unwrap();
    let head = chain.registry(3);
    let genesis = Anchor::genesis(chain.registry(0)).unwrap();
    assert_eq!(
        verify_history(head, &genesis, &retained, &limits()),
        Ok(HistoryOutcome::Verified { steps: 3 })
    );
    assert_eq!(
        verify_history(
            head,
            &Anchor::accepted(chain.registry(1)),
            &retained,
            &limits()
        ),
        Ok(HistoryOutcome::Verified { steps: 2 })
    );
    assert_eq!(
        verify_history(head, &Anchor::accepted(head), &retained, &limits()),
        Ok(HistoryOutcome::Verified { steps: 0 }),
        "the anchor itself needs no replay"
    );
}

/// A labelled change to a committed update, applied to the base it names.
type Case = (&'static str, usize, fn(&mut Value), TransitionFailure);

#[test]
fn each_transition_rule_is_refused_by_its_own_name() {
    // update-2 continues pay (index 0), retires cancel (1) and allocates the
    // copy (2) against registry-1. update-3 continues pay (0), restores cancel
    // (1) and continues the copy (2) against registry-2, where cancel is
    // retired.
    let cases: [Case; 15] = [
        (
            "a continuation of a retired ID",
            3,
            |v| v["body"]["decisions"][1]["kind"] = json!("continue"),
            TransitionFailure::NotActive,
        ),
        (
            "a continuation of an ID the base does not hold",
            2,
            |v| v["body"]["decisions"][0]["intentId"]["value"] = json!("0".repeat(32)),
            TransitionFailure::NotActive,
        ),
        (
            "a continuation from a declaration its entry does not hold",
            2,
            |v| v["body"]["decisions"][0]["from"]["range"] = json!({"start": "0", "end": "5"}),
            TransitionFailure::FromMismatch,
        ),
        (
            "an allocation of an active ID",
            2,
            |v| {
                // Home is active in registry-1. Its ID sorts between pay's and
                // cancel's, and the copy link has to go with the allocation.
                let mut allocation = decision(v, 2);
                allocation["intentId"]["value"] = json!("6ad022577a9ed7bb6d5f141f06e99df9");
                let decisions = v["body"]["decisions"].as_array_mut().unwrap();
                decisions.remove(2);
                decisions.insert(1, allocation);
                v["body"]["lineageLinks"] = json!([]);
            },
            TransitionFailure::AlreadyAllocated,
        ),
        (
            "an allocation of a retired ID",
            3,
            |v| {
                let restoration = decision(v, 1);
                v["body"]["decisions"][1] = json!({
                    "kind": "allocate",
                    "intentId": restoration["intentId"],
                    "to": restoration["to"],
                    "basis": {"kind": "confirmed-new"},
                });
            },
            TransitionFailure::AlreadyAllocated,
        ),
        (
            "a retirement of a retired ID",
            3,
            |v| {
                let restoration = decision(v, 1);
                v["body"]["decisions"][1] = json!({
                    "kind": "retire",
                    "intentId": restoration["intentId"],
                    "from": restoration["from"],
                    "basis": {"kind": "complete-absence"},
                });
            },
            TransitionFailure::NotActive,
        ),
        (
            "a restoration of an active ID",
            2,
            |v| {
                let home = artifact("registry-1")["body"]["entries"][1].clone();
                let restoration = json!({
                    "kind": "restore",
                    "intentId": home["intentId"],
                    "from": home["declaration"],
                    "to": home["declaration"],
                    "basis": {"kind": "explicit", "reason": "Home was never gone."},
                });
                v["body"]["decisions"]
                    .as_array_mut()
                    .unwrap()
                    .insert(1, restoration);
            },
            TransitionFailure::NotRetired,
        ),
        (
            "a restoration of an ID the base does not hold",
            3,
            |v| v["body"]["decisions"][1]["intentId"]["value"] = json!("9".repeat(32)),
            TransitionFailure::NotRetired,
        ),
        (
            "a restoration from a declaration its entry does not hold",
            3,
            |v| v["body"]["decisions"][1]["from"]["range"] = json!({"start": "0", "end": "5"}),
            TransitionFailure::FromMismatch,
        ),
        (
            "an identity for something that is not a declaration",
            2,
            |v| v["body"]["decisions"][2]["to"]["range"]["end"] = json!("82"),
            TransitionFailure::UnknownDeclaration,
        ),
        (
            "an identity for a declaration an unchanged entry holds",
            2,
            |v| {
                v["body"]["decisions"][2]["to"] =
                    artifact("registry-1")["body"]["entries"][1]["declaration"].clone();
            },
            TransitionFailure::DeclarationTaken,
        ),
        (
            "a declaration left without an identity",
            2,
            |v| {
                v["body"]["decisions"].as_array_mut().unwrap().remove(2);
                v["body"]["lineageLinks"] = json!([]);
            },
            TransitionFailure::Uncovered,
        ),
        (
            "a link from an ID the base does not hold",
            2,
            |v| {
                v["body"]["lineageLinks"][0]["predecessors"][0]["value"] = json!("0".repeat(32));
            },
            TransitionFailure::UnresolvedLink,
        ),
        (
            "a link to an ID the result does not hold",
            2,
            |v| {
                let pay = decision(v, 0)["intentId"].clone();
                let mut stranger = pay.clone();
                stranger["value"] = json!("f".repeat(32));
                v["body"]["lineageLinks"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"kind": "split", "predecessors": [pay.clone()], "successors": [pay, stranger]}));
            },
            TransitionFailure::UnresolvedLink,
        ),
        (
            "an update applied to another base than its own",
            2,
            |v| v["body"]["base"] = artifact("update-3")["body"]["base"].clone(),
            TransitionFailure::BaseMismatch,
        ),
    ];
    let chain = Chain::load();
    for (label, n, change, expected) in cases {
        let mut plan = artifact(&format!("update-{n}"));
        change(&mut plan);
        let plan = update(&sealed(plan));
        assert_eq!(
            apply(chain.registry(n - 1), &plan, chain.inventory(n)),
            Err(expected),
            "{label}"
        );
    }
}

#[test]
fn an_update_applies_only_with_the_inventory_and_owner_it_names() {
    let chain = Chain::load();
    assert_eq!(
        apply(chain.registry(1), chain.update(2), chain.inventory(3)),
        Err(TransitionFailure::InventoryMismatch)
    );

    let library = update(&empty_update(
        "library",
        chain.registry(1),
        chain.inventory(1),
    ));
    assert_eq!(
        apply(chain.registry(1), &library, chain.inventory(1)),
        Err(TransitionFailure::OwnerMismatch)
    );

    let mut elsewhere = artifact("inventory-2");
    elsewhere["body"]["scope"] = json!("elsewhere-web");
    let elsewhere = inventory(&sealed(elsewhere));
    assert_eq!(
        apply(
            chain.registry(1),
            &planned_from(artifact("update-2"), &elsewhere),
            &elsewhere
        ),
        Err(TransitionFailure::ScopeMismatch)
    );
}

#[test]
fn no_update_is_planned_from_a_unit_that_was_not_checked() {
    let chain = Chain::load();
    for outcome in ["failed", "blocked"] {
        // A third unit that was read, or not, and established nothing.
        let mut broken = artifact("inventory-2");
        let mut unit = broken["body"]["units"][0].clone();
        unit["source"]["unit"] = json!("broken");
        unit["outcome"] = json!(outcome);
        broken["body"]["units"]
            .as_array_mut()
            .unwrap()
            .insert(0, unit);
        let broken = inventory(&sealed(broken));
        assert_eq!(
            apply(
                chain.registry(1),
                &planned_from(artifact("update-2"), &broken),
                &broken
            ),
            Err(TransitionFailure::UncheckedUnit),
            "a {outcome} unit"
        );
    }
}

/// inventory-2 cut down to the checkout unit, as an editor might ask for it.
fn checkout_only() -> AdmittedInventory {
    let mut partial = artifact("inventory-2");
    let body = &mut partial["body"];
    body["completeness"] = json!("partial");
    let in_checkout = |value: &Value, path: &[&str]| {
        let mut source = value;
        for member in path {
            source = &source[*member];
        }
        source["unit"] == json!("checkout")
    };
    body["units"]
        .as_array_mut()
        .unwrap()
        .retain(|unit| in_checkout(unit, &["source"]));
    body["declarations"]
        .as_array_mut()
        .unwrap()
        .retain(|facts| in_checkout(facts, &["occurrence", "source"]));
    body["references"]
        .as_array_mut()
        .unwrap()
        .retain(|reference| in_checkout(reference, &["occurrence", "source"]));
    inventory(&sealed(partial))
}

#[test]
fn a_partial_inventory_updates_what_it_covers_and_keeps_what_it_cannot_see() {
    let chain = Chain::load();
    let partial = checkout_only();

    // update-2 retires cancel, which a partial view can never justify.
    assert_eq!(
        apply(
            chain.registry(1),
            &planned_from(artifact("update-2"), &partial),
            &partial
        ),
        Err(TransitionFailure::RetirementFromPartialInventory)
    );

    // Without the retirement, the rest applies. Home lives in a unit this
    // view does not hold, and cancel was decided nothing about: both stay
    // exactly as the base has them.
    let mut plan = artifact("update-2");
    plan["body"]["decisions"].as_array_mut().unwrap().remove(1);
    let Ok(Transition::Applied(result)) =
        apply(chain.registry(1), &planned_from(plan, &partial), &partial)
    else {
        panic!("the update applies");
    };
    let base = chain.registry(1).snapshot();
    for kept in [&base.entries()[1], &base.entries()[2]] {
        assert_eq!(result.entry(kept.intent_id()), Some(kept));
    }
    assert_eq!(result.entries().len(), 4);
}

#[test]
fn an_update_without_decisions_reuses_its_base() {
    let chain = Chain::load();
    let nothing = update(&empty_update(
        "application",
        chain.registry(3),
        chain.inventory(3),
    ));
    assert_eq!(
        apply(chain.registry(3), &nothing, chain.inventory(3)),
        Ok(Transition::Unchanged)
    );

    // Doing nothing is still an update, so it still has to cover every
    // declaration: registry-2 holds none of checkout revision 3's.
    let stale = update(&empty_update(
        "application",
        chain.registry(2),
        chain.inventory(3),
    ));
    assert_eq!(
        apply(chain.registry(2), &stale, chain.inventory(3)),
        Err(TransitionFailure::Uncovered)
    );

    // And no state is ever recorded as its result.
    let base: RegistryArtifact = serde_json::from_value(artifact("registry-3")).unwrap();
    let plan: RegistryUpdateArtifact = serde_json::from_slice(&bytes(&empty_update(
        "application",
        chain.registry(3),
        chain.inventory(3),
    )))
    .unwrap();
    assert_eq!(
        IntentRegistrySnapshot::successor(&base, &plan, base.body().entries().to_vec()),
        Err(SnapshotFailure::EmptyUpdate)
    );
}

#[test]
fn replay_refuses_a_stored_result_the_update_does_not_produce() {
    let chain = Chain::load();

    // One entry changed and resealed: well formed, admitted, and wrong.
    let mut forged = artifact("registry-2");
    forged["body"]["entries"][2]["state"] = json!("active");
    let forged = registry(&sealed(forged));
    assert_eq!(
        replay(
            chain.registry(1),
            chain.update(2),
            chain.inventory(2),
            &forged
        ),
        Err(ReplayFailure::Mismatch)
    );

    assert_eq!(
        replay(
            chain.registry(1),
            chain.update(2),
            chain.inventory(2),
            chain.registry(3)
        ),
        Err(ReplayFailure::Unlinked)
    );

    // Another update against the same base: registry-2 names update-2, not
    // this one, even though this one applies too.
    let mut unlinked = artifact("update-2");
    unlinked["body"]["lineageLinks"] = json!([]);
    assert_eq!(
        replay(
            chain.registry(1),
            &update(&sealed(unlinked)),
            chain.inventory(2),
            chain.registry(2)
        ),
        Err(ReplayFailure::Unlinked)
    );

    // A snapshot claiming to record an update that changes nothing.
    let nothing = empty_update("application", chain.registry(3), chain.inventory(3));
    let mut claimed = artifact("registry-3");
    claimed["body"]["base"] = serde_json::to_value(chain.registry(3).reference()).unwrap();
    claimed["body"]["update"] = serde_json::to_value(update(&nothing).reference()).unwrap();
    assert_eq!(
        replay(
            chain.registry(3),
            &update(&nothing),
            chain.inventory(3),
            &registry(&sealed(claimed))
        ),
        Err(ReplayFailure::NoNewState)
    );

    // A stored snapshot that names an update which does not apply.
    let mut plan = artifact("update-2");
    plan["body"]["decisions"].as_array_mut().unwrap().remove(2);
    plan["body"]["lineageLinks"] = json!([]);
    let plan = update(&sealed(plan));
    let mut claimed = artifact("registry-2");
    claimed["body"]["update"] = serde_json::to_value(plan.reference()).unwrap();
    assert_eq!(
        replay(
            chain.registry(1),
            &plan,
            chain.inventory(2),
            &registry(&sealed(claimed))
        ),
        Err(ReplayFailure::Transition(TransitionFailure::Uncovered))
    );
}

#[test]
fn a_chain_is_verified_only_from_the_anchor_the_host_names() {
    let chain = Chain::load();
    let retained =
        RetainedHistory::new(&chain.registries, &chain.updates, &chain.inventories).unwrap();
    let head = chain.registry(3);

    assert_eq!(
        Anchor::genesis(chain.registry(1)).err(),
        Some(HistoryFailure::NotGenesis)
    );

    // Another chain's genesis: walking back from the head reaches this
    // chain's own genesis, which is consistent with itself and still not the
    // anchor.
    let other = registry(
        &serde_json::to_value(
            RegistryArtifact::seal(
                IntentRegistrySnapshot::genesis(
                    owner(),
                    "storefront-web",
                    RegistryIdentity::retained(&"1".repeat(32)).unwrap(),
                )
                .unwrap(),
            )
            .unwrap(),
        )
        .unwrap(),
    );
    assert_eq!(
        verify_history(
            head,
            &Anchor::genesis(&other).unwrap(),
            &retained,
            &limits()
        ),
        Err(HistoryFailure::NotAnchored)
    );
}

#[test]
fn a_chain_missing_an_input_is_blocked_and_nothing_stands_in() {
    let chain = Chain::load();
    let head = chain.registry(3);
    let genesis = Anchor::genesis(chain.registry(0)).unwrap();

    let without_inventory = [chain.inventory(1).clone(), chain.inventory(3).clone()];
    let retained =
        RetainedHistory::new(&chain.registries, &chain.updates, &without_inventory).unwrap();
    assert_eq!(
        verify_history(head, &genesis, &retained, &limits()),
        Ok(HistoryOutcome::Blocked {
            missing: chain.inventory(2).reference()
        })
    );

    let without_base = [
        chain.registry(0).clone(),
        chain.registry(2).clone(),
        chain.registry(3).clone(),
    ];
    let retained = RetainedHistory::new(&without_base, &chain.updates, &chain.inventories).unwrap();
    assert_eq!(
        verify_history(head, &genesis, &retained, &limits()),
        Ok(HistoryOutcome::Blocked {
            missing: chain.registry(1).reference()
        })
    );

    let twice = [chain.update(1).clone(), chain.update(1).clone()];
    assert_eq!(
        RetainedHistory::new(&chain.registries, &twice, &chain.inventories).err(),
        Some(HistoryFailure::DuplicateArtifact)
    );
}

#[test]
fn a_chain_is_replayed_within_its_bound_and_a_forged_link_is_named() {
    let chain = Chain::load();
    let retained =
        RetainedHistory::new(&chain.registries, &chain.updates, &chain.inventories).unwrap();
    let genesis = Anchor::genesis(chain.registry(0)).unwrap();
    let bound = |history_steps| IdentityLimits {
        history_steps,
        ..limits()
    };
    assert_eq!(
        verify_history(chain.registry(3), &genesis, &retained, &bound(3)),
        Ok(HistoryOutcome::Verified { steps: 3 })
    );
    assert_eq!(
        verify_history(chain.registry(3), &genesis, &retained, &bound(2)),
        Err(HistoryFailure::Limit(IdentityLimitKind::HistorySteps))
    );

    let mut forged = artifact("registry-2");
    forged["body"]["entries"][2]["state"] = json!("active");
    let forged = registry(&sealed(forged));
    assert_eq!(
        verify_history(&forged, &genesis, &retained, &limits()),
        Err(HistoryFailure::Replay {
            snapshot: forged.reference(),
            failure: ReplayFailure::Mismatch
        })
    );
}

#[test]
fn applying_never_changes_what_the_base_holds_for_an_undecided_id() {
    // Retirement keeps the last declaration, and every ID no decision names
    // is carried over exactly.
    let chain = Chain::load();
    let Ok(Transition::Applied(result)) =
        apply(chain.registry(1), chain.update(2), chain.inventory(2))
    else {
        panic!("update-2 applies");
    };
    let base = chain.registry(1).snapshot();
    let cancel = &base.entries()[2];
    let retired = result.entry(cancel.intent_id()).unwrap();
    assert_eq!(retired.state(), EntryState::Retired);
    assert_eq!(retired.declaration(), cancel.declaration());
    let home = &base.entries()[1];
    assert_eq!(result.entry(home.intent_id()), Some(home));
}

/// An update of the committed owner, with decisions written out in Intent ID
/// order.
fn plan_of(
    base: &AdmittedRegistry,
    inventory: &AdmittedInventory,
    decisions: Vec<Value>,
) -> AdmittedUpdate {
    let mut plan = empty_update("application", base, inventory);
    plan["body"]["decisions"] = Value::Array(decisions);
    update(&sealed(plan))
}

/// One entry of a committed snapshot, by its position there.
fn entry(registry_id: &str, index: usize) -> Value {
    artifact(registry_id)["body"]["entries"][index].clone()
}

#[test]
fn a_declaration_an_update_frees_can_be_given_to_another_id() {
    // Retiring cancel frees its declaration within the same update, so a new
    // ID may take it. The transition allows this; that cancel is really gone
    // is the absence check's to prove, and here it is not.
    let chain = Chain::load();
    let cancel = entry("registry-1", 2);
    let mut newcomer = cancel["intentId"].clone();
    newcomer["value"] = json!("d".repeat(32));
    let plan = plan_of(
        chain.registry(1),
        chain.inventory(1),
        vec![
            json!({"kind": "retire", "intentId": cancel["intentId"], "from": cancel["declaration"], "basis": {"kind": "complete-absence"}}),
            json!({"kind": "allocate", "intentId": newcomer, "to": cancel["declaration"], "basis": {"kind": "confirmed-new"}}),
        ],
    );
    let Ok(Transition::Applied(result)) = apply(chain.registry(1), &plan, chain.inventory(1))
    else {
        panic!("the update applies");
    };
    let states: Vec<EntryState> = result.entries().iter().map(RegistryEntry::state).collect();
    assert_eq!(
        states,
        [
            EntryState::Active,
            EntryState::Active,
            EntryState::Retired,
            EntryState::Active
        ]
    );
}

#[test]
fn retiring_an_id_whose_declaration_is_still_current_leaves_it_uncovered() {
    let chain = Chain::load();
    let cancel = entry("registry-1", 2);
    let plan = plan_of(
        chain.registry(1),
        chain.inventory(1),
        vec![
            json!({"kind": "retire", "intentId": cancel["intentId"], "from": cancel["declaration"], "basis": {"kind": "complete-absence"}}),
        ],
    );
    assert_eq!(
        apply(chain.registry(1), &plan, chain.inventory(1)),
        Err(TransitionFailure::Uncovered)
    );
}

#[test]
fn a_retired_id_does_not_take_back_its_declaration_without_an_explicit_restore() {
    // Checkout goes back to revision 1, where cancel's old declaration is
    // exactly where it was. Cancel is retired in registry-2, so that
    // declaration has no identity until a restore gives it one.
    let chain = Chain::load();
    let pay = entry("registry-2", 0);
    let cancel = entry("registry-2", 2);
    let pay_back = json!({
        "kind": "continue",
        "intentId": pay["intentId"],
        "from": pay["declaration"],
        "to": entry("registry-1", 0)["declaration"],
        "basis": {"kind": "explicit", "reason": "Checkout went back to revision 1."},
    });
    let plan = plan_of(
        chain.registry(2),
        chain.inventory(1),
        vec![pay_back.clone()],
    );
    assert_eq!(
        apply(chain.registry(2), &plan, chain.inventory(1)),
        Err(TransitionFailure::Uncovered)
    );

    let restore = json!({
        "kind": "restore",
        "intentId": cancel["intentId"],
        "from": cancel["declaration"],
        "to": cancel["declaration"],
        "basis": {"kind": "explicit", "reason": "Cancel came back unchanged."},
    });
    let plan = plan_of(
        chain.registry(2),
        chain.inventory(1),
        vec![pay_back, restore],
    );
    let Ok(Transition::Applied(result)) = apply(chain.registry(2), &plan, chain.inventory(1))
    else {
        panic!("the update applies");
    };
    let restored: RegistryEntry = serde_json::from_value(cancel.clone()).unwrap();
    let back = result.entry(restored.intent_id()).unwrap();
    assert_eq!(back.state(), EntryState::Active);
    assert_eq!(back.declaration(), restored.declaration());
}
