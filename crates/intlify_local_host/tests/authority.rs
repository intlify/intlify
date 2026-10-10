// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 018's conformance cases for the authority evaluator.
//!
//! Each test is one row of 018's fixture table, as far as an evaluator can
//! answer it without a host's current state: establishment, least
//! authority, input binding, confirmations, automatic updates, owner
//! semantics, and the binding of a permit to its request. The rows that need
//! a current registry and a live authority at the point of publication
//! (stale bases, competing initializations, an authority or session that
//! changes before commit) belong to the host.
//!
//! Every authority is established through the test-owned entry, every plan
//! is made by the real reconciliation from the committed chain's evidence,
//! and every answer comes from the real evaluator.

mod support;

use intlify_authoring::test_context::admit_inventory;
use intlify_authoring::{
    AnalysisWorkspace, AuthoringLimits, InventoryArtifact, OwnerIdentity, OwnerKind,
};
use intlify_authoring_identity::{
    CandidateFailure, Plan, ReconcileFailure, Reconciliation, TransitionFailure,
};
use intlify_local_host::test_authority::TestAuthority;
use intlify_local_host::{
    Action, AuthorizationFailure, EstablishmentFailure, LocalAuthority, PermitMismatch, UpdateMode,
    UpdateRequest,
};
use serde_json::json;
use support::{
    artifact, authority_over, basis, bytes, chain_destination, id_of, limits, owner, principal,
    reconciled, restore, the_edit, Case, Chain,
};

const ALL: &[Action] = &Action::ALL;

fn planned(result: Result<Reconciliation, ReconcileFailure>) -> Plan {
    match result {
        Ok(Reconciliation::Planned(plan)) => *plan,
        other => panic!("not planned: {other:?}"),
    }
}

/// Update 3 of the chain, planned again: two proven continuations and an
/// explicit restore.
fn restoration(chain: &Chain) -> Plan {
    planned(reconciled(
        chain,
        (2, 3),
        &Case {
            edits: &[the_edit(3)],
            explicit: &[restore(chain)],
            ..Case::default()
        },
    ))
}

/// Update 1 of the chain, planned again: three proven allocations.
fn allocation(chain: &Chain) -> Plan {
    planned(reconciled(
        chain,
        (0, 1),
        &Case {
            candidates: &[id_of(1, 0), id_of(1, 2), id_of(1, 1)],
            ..Case::default()
        },
    ))
}

fn request<'r>(
    chain: &'r Chain,
    (base, current): (usize, usize),
    plan: &'r Plan,
    confirmations: &'r [intlify_local_host::Confirmation],
    mode: UpdateMode,
) -> UpdateRequest<'r> {
    UpdateRequest {
        base: chain.registry(base),
        inventory: chain.inventory(current),
        plan,
        confirmations,
        mode,
    }
}

#[test]
fn establishment_is_explicit_and_refuses_what_it_cannot_trust() {
    let chain = Chain::load();
    // An explicitly established local authority admits its own caller.
    let authority = authority_over(&chain, 3, &[("alice", &[Action::ReadRegistry])], false);
    assert!(authority.invoke(&principal("alice")).is_ok());
    // A principal is not a name. One the authority never established, even
    // spelled like a real one elsewhere, is no caller here.
    assert_eq!(
        authority.invoke(&principal("mallory")).err(),
        Some(AuthorizationFailure::UnknownCaller)
    );

    let setup = || TestAuthority::new(chain_destination(), basis());
    let alice = principal("alice");
    // Duplicate or conflicting grant entries are refused, never merged.
    assert_eq!(
        setup()
            .grant(&alice, [Action::ReadRegistry])
            .grant(&alice, [Action::UpdateRegistry])
            .establish(&limits())
            .err(),
        Some(EstablishmentFailure::DuplicatePrincipal)
    );
    assert_eq!(
        setup()
            .grant(&alice, [Action::ReadRegistry, Action::ReadRegistry])
            .establish(&limits())
            .err(),
        Some(EstablishmentFailure::RepeatedAction)
    );
    // A library owner is not this subset's application binding.
    let library = TestAuthority::destination(
        "storefront-registry",
        OwnerIdentity::new(OwnerKind::Library, "storefront").unwrap(),
        "storefront-web",
        None,
    )
    .unwrap();
    assert_eq!(
        TestAuthority::new(library, basis())
            .establish(&limits())
            .err(),
        Some(EstablishmentFailure::NotApplication)
    );
    // A context labelled production is not test-owned, and a test authority
    // never vouches for it.
    let mut production = serde_json::to_value(basis()).unwrap();
    production["contextKind"] = json!("application-profile");
    assert_eq!(
        TestAuthority::new(
            chain_destination(),
            serde_json::from_value(production).unwrap()
        )
        .establish(&limits())
        .err(),
        Some(EstablishmentFailure::ProductionContext)
    );
}

#[test]
fn least_authority_denies_each_missing_grant_on_its_own() {
    let chain = Chain::load();
    let plan = restoration(&chain);
    let operations = |actions: &[Action]| {
        let authority = authority_over(&chain, 3, &[("alice", actions)], false);
        let invocation = authority.invoke(&principal("alice")).unwrap();
        let analysis = invocation.authorize_analysis(chain.inventory(3), Some(chain.registry(2)));
        let confirm = invocation.confirm(chain.registry(2), chain.inventory(3), &restore(&chain));
        let update = invocation.authorize_update(
            &request(&chain, (2, 3), &plan, &[], UpdateMode::Manual),
            &limits(),
        );
        let initialize = invocation.authorize_initialization(chain.registry(0));
        (
            analysis.err(),
            confirm.err(),
            update.err(),
            initialize.err(),
        )
    };
    // Read-only analysis succeeds with its read grants and no write grant.
    let (analysis, ..) = operations(&[Action::AnalyzeSource, Action::ReadRegistry]);
    assert_eq!(analysis, None);

    // Every write is denied when its own grant is the one missing, whatever
    // else the caller holds.
    let without = |missing: Action| -> Vec<Action> {
        ALL.iter()
            .copied()
            .filter(|action| *action != missing)
            .collect()
    };
    let denied = |action| Some(AuthorizationFailure::Denied(action));
    let (_, confirm, ..) = operations(&without(Action::ResolveIdentity));
    assert_eq!(confirm, denied(Action::ResolveIdentity));
    let (_, _, update, _) = operations(&without(Action::UpdateRegistry));
    assert_eq!(update, denied(Action::UpdateRegistry));
    let (.., initialize) = operations(&without(Action::InitializeRegistry));
    assert_eq!(initialize, denied(Action::InitializeRegistry));
    let (analysis, ..) = operations(&without(Action::ReadRegistry));
    assert_eq!(analysis, denied(Action::ReadRegistry));

    // One grant implies no other.
    for held in ALL {
        let (analysis, confirm, update, initialize) = operations(&[*held]);
        for (needs, refused) in [
            (Action::ResolveIdentity, confirm),
            (Action::UpdateRegistry, update),
            (Action::InitializeRegistry, initialize),
        ] {
            if needs != *held {
                assert_eq!(refused, denied(needs), "{held:?} implied {needs:?}");
            }
        }
        if *held != Action::AnalyzeSource {
            assert_eq!(analysis, denied(Action::AnalyzeSource), "{held:?}");
        }
    }
}

#[test]
fn input_binding_admits_the_exact_acquired_inputs_only() {
    let chain = Chain::load();
    let reader = &[Action::AnalyzeSource, Action::ReadRegistry][..];
    // The exact acquired source and base.
    let exact = authority_over(&chain, 3, &[("alice", reader)], false);
    assert_eq!(
        exact
            .invoke(&principal("alice"))
            .unwrap()
            .authorize_analysis(chain.inventory(3), Some(chain.registry(2))),
        Ok(())
    );

    let analyze = |authority: &LocalAuthority| {
        authority
            .invoke(&principal("alice"))
            .unwrap()
            .authorize_analysis(chain.inventory(3), Some(chain.registry(2)))
            .err()
    };
    let acquired_three = |setup: TestAuthority| {
        chain
            .inventory(3)
            .inventory()
            .units()
            .iter()
            .fold(setup, |setup, unit| setup.acquired(unit.source().clone()))
    };
    let with = |destination, anchor: bool| {
        let setup = TestAuthority::new(destination, basis())
            .grant(&principal("alice"), reader.iter().copied());
        let setup = if anchor {
            setup.anchor(chain.registry(2).reference())
        } else {
            setup
        };
        acquired_three(setup).establish(&limits()).unwrap()
    };
    // Another registry identity for the same owner and scope.
    let other_chain = TestAuthority::destination(
        "storefront-registry",
        owner(),
        "storefront-web",
        Some(intlify_authoring_identity::RegistryIdentity::retained(&"1".repeat(32)).unwrap()),
    )
    .unwrap();
    assert_eq!(
        analyze(&with(other_chain, true)),
        Some(AuthorizationFailure::RegistryMismatch)
    );
    // Another owning scope.
    let other_scope =
        TestAuthority::destination("storefront-registry", owner(), "storefront-admin", None)
            .unwrap();
    assert_eq!(
        analyze(&with(other_scope, true)),
        Some(AuthorizationFailure::ScopeMismatch)
    );
    // A base the host did not accept for this invocation.
    assert_eq!(
        analyze(&with(chain_destination(), false)),
        Some(AuthorizationFailure::Unanchored)
    );
    // Another owner, whose authority acquires none of these sources.
    let other_owner = TestAuthority::destination(
        "storefront-registry",
        OwnerIdentity::new(OwnerKind::Application, "other-shop").unwrap(),
        "storefront-web",
        None,
    )
    .unwrap();
    assert_eq!(
        TestAuthority::new(other_owner, basis())
            .grant(&principal("alice"), reader.iter().copied())
            .establish(&limits())
            .unwrap()
            .invoke(&principal("alice"))
            .unwrap()
            .authorize_analysis(chain.inventory(3), None),
        Err(AuthorizationFailure::OwnerMismatch)
    );
    // Another revision's bytes than the ones acquired.
    let earlier = authority_over(&chain, 2, &[("alice", reader)], false);
    assert_eq!(
        earlier
            .invoke(&principal("alice"))
            .unwrap()
            .authorize_analysis(chain.inventory(3), None),
        Err(AuthorizationFailure::SourceNotAcquired)
    );
    // Another context than the one the authority binds.
    let mut changed = serde_json::to_value(basis()).unwrap();
    changed["context"]["revision"] = json!("2");
    let other_context = acquired_three(
        TestAuthority::new(
            chain_destination(),
            serde_json::from_value(changed).unwrap(),
        )
        .grant(&principal("alice"), reader.iter().copied()),
    )
    .establish(&limits())
    .unwrap();
    assert_eq!(
        other_context
            .invoke(&principal("alice"))
            .unwrap()
            .authorize_analysis(chain.inventory(3), None),
        Err(AuthorizationFailure::ContextMismatch)
    );
}

#[test]
fn confirmations_bind_the_exact_choice_actor_and_authority() {
    let chain = Chain::load();
    let plan = restoration(&chain);
    let roles = &[
        ("alice", &[Action::UpdateRegistry][..]),
        ("carol", &[Action::ResolveIdentity][..]),
    ][..];
    let authority = authority_over(&chain, 3, roles, false);
    let publish = |confirmations: &[intlify_local_host::Confirmation]| {
        authority
            .invoke(&principal("alice"))
            .unwrap()
            .authorize_update(
                &request(&chain, (2, 3), &plan, confirmations, UpdateMode::Manual),
                &limits(),
            )
    };
    // The reason the plan records is data, not a confirmation.
    assert_eq!(
        publish(&[]).err(),
        Some(AuthorizationFailure::ConfirmationMissing(id_of(3, 1)))
    );
    // A principal without the grant confirms nothing.
    assert_eq!(
        authority
            .invoke(&principal("alice"))
            .unwrap()
            .confirm(chain.registry(2), chain.inventory(3), &restore(&chain))
            .err(),
        Some(AuthorizationFailure::Denied(Action::ResolveIdentity))
    );
    // The confirmer and the publisher may be different principals.
    let confirmation = authority
        .invoke(&principal("carol"))
        .unwrap()
        .confirm(chain.registry(2), chain.inventory(3), &restore(&chain))
        .unwrap();
    let permit = publish(std::slice::from_ref(&confirmation)).unwrap();
    assert_eq!(permit.publisher(), &principal("alice"));
    assert_eq!(permit.confirmations()[0].confirmer(), &principal("carol"));
    assert_eq!(
        permit.confirmations()[0].decision(),
        &plan.update().update().decisions()[1]
    );

    // Under a changed authority, here the same grants established again,
    // the old confirmation is stale.
    let changed = authority_over(&chain, 3, roles, false);
    assert_eq!(
        changed
            .invoke(&principal("alice"))
            .unwrap()
            .authorize_update(
                &request(
                    &chain,
                    (2, 3),
                    &plan,
                    std::slice::from_ref(&confirmation),
                    UpdateMode::Manual
                ),
                &limits()
            )
            .err(),
        Some(AuthorizationFailure::ConfirmationStale)
    );
    // A decision made for another base or inventory is not confirmed.
    let elsewhere = intlify_authoring_identity::ExplicitDecision::new(
        chain.registry(1).reference(),
        chain.inventory(3).reference(),
        support::committed(3, 1),
    )
    .unwrap();
    assert_eq!(
        authority
            .invoke(&principal("carol"))
            .unwrap()
            .confirm(chain.registry(2), chain.inventory(3), &elsewhere)
            .err(),
        Some(AuthorizationFailure::UnboundDecision)
    );
    // A restoration is never accepted automatically, confirmed or not.
    let session = authority_over(&chain, 3, roles, true);
    let confirmed = session
        .invoke(&principal("carol"))
        .unwrap()
        .confirm(chain.registry(2), chain.inventory(3), &restore(&chain))
        .unwrap();
    assert_eq!(
        session
            .invoke(&principal("alice"))
            .unwrap()
            .authorize_update(
                &request(
                    &chain,
                    (2, 3),
                    &plan,
                    std::slice::from_ref(&confirmed),
                    UpdateMode::Development
                ),
                &limits()
            )
            .err(),
        Some(AuthorizationFailure::NotAutomatic)
    );
}

#[test]
fn automatic_updates_need_the_session_and_proven_decisions() {
    let chain = Chain::load();
    let publisher = &[("alice", &[Action::UpdateRegistry][..])][..];
    let develop = |authority: &LocalAuthority, plan: &Plan, (base, current)| {
        authority
            .invoke(&principal("alice"))
            .unwrap()
            .authorize_update(
                &request(&chain, (base, current), plan, &[], UpdateMode::Development),
                &limits(),
            )
    };
    // Proven allocations in an enabled session.
    let allocations = allocation(&chain);
    let enabled = authority_over(&chain, 1, publisher, true);
    let permit = develop(&enabled, &allocations, (0, 1)).unwrap();
    assert_eq!(permit.mode(), UpdateMode::Development);
    // A proven continuation, retirement and allocation, too.
    let edited = planned(reconciled(
        &chain,
        (1, 2),
        &Case {
            edits: &[the_edit(2)],
            candidates: &[id_of(2, 2)],
            ..Case::default()
        },
    ));
    let session = authority_over(&chain, 2, publisher, true);
    assert!(develop(&session, &edited, (1, 2)).is_ok());
    // Without a session, or with one that has ended, nothing is automatic.
    let disabled = authority_over(&chain, 1, publisher, false);
    assert_eq!(
        develop(&disabled, &allocations, (0, 1)).err(),
        Some(AuthorizationFailure::SessionInactive)
    );
    // Ambiguous history never reaches authorization: with no edit, nothing
    // shows where the old declarations went, and there is no plan.
    assert!(matches!(
        reconciled(&chain, (1, 2), &Case::default()),
        Ok(Reconciliation::Unresolved(_))
    ));
    // An update cannot initialize: a binding with no chain has no base.
    let new_owner = TestAuthority::new(
        TestAuthority::destination("storefront-registry", owner(), "storefront-web", None).unwrap(),
        basis(),
    )
    .grant(&principal("alice"), [Action::UpdateRegistry])
    .anchor(chain.registry(0).reference())
    .development_session();
    let new_owner = chain
        .inventory(1)
        .inventory()
        .units()
        .iter()
        .fold(new_owner, |setup, unit| {
            setup.acquired(unit.source().clone())
        })
        .establish(&limits())
        .unwrap();
    assert_eq!(
        develop(&new_owner, &allocations, (0, 1)).err(),
        Some(AuthorizationFailure::RegistryMismatch)
    );
}

/// Inventory 3 with its nav unit blocked, resealed and admitted again: a
/// faithful record of an analysis that did not check every unit.
fn with_blocked_unit() -> intlify_authoring::AdmittedInventory {
    let mut value = artifact("inventory-3");
    value["body"]["units"][1]["outcome"] = json!("blocked");
    let body = serde_json::from_value(value["body"].clone()).unwrap();
    let sealed = InventoryArtifact::seal(body).unwrap();
    admit_inventory(
        &bytes(&serde_json::to_value(sealed).unwrap()),
        &support::context(),
        &[],
        &AuthoringLimits {
            declarations: 1024,
            message_text_bytes: 64 * 1024,
            emitted_mf2_bytes: 128 * 1024,
            extraction_segments: 4096,
            parameter_names: 64,
            parameter_name_bytes: 256,
            metadata_value_bytes: 4096,
            vocabulary_members: 256,
            projection_nodes: 4096,
            projection_depth: 32,
            diagnostics: 256,
        }
        .validate()
        .unwrap(),
        &mut AnalysisWorkspace::new(),
    )
    .unwrap()
}

#[test]
fn owner_semantics_cannot_be_overridden_by_authority() {
    let chain = Chain::load();
    // A caller holding every grant still has nothing to authorize where
    // design 016 refuses the change: a candidate colliding with an ID the
    // base holds, a unit that was not checked, and history the evidence
    // does not show.
    assert_eq!(
        reconciled(
            &chain,
            (1, 2),
            &Case {
                edits: &[the_edit(2)],
                candidates: &[id_of(1, 0)],
                ..Case::default()
            },
        ),
        Err(ReconcileFailure::Candidates(CandidateFailure::Collision(
            id_of(1, 0)
        )))
    );
    let blocked = with_blocked_unit();
    assert_eq!(
        intlify_authoring_identity::reconcile(
            chain.registry(2),
            &blocked,
            &intlify_authoring_identity::ReconcileInputs {
                evidence: intlify_authoring_identity::ContinuityInputs {
                    sources: &intlify_authoring_identity::RetainedSources::new(
                        std::iter::empty::<(intlify_authoring::SourceSnapshot, &[u8])>()
                    )
                    .unwrap(),
                    edits: &[],
                    membership: &[],
                    previous: None,
                },
                explicit: &[],
                links: &[],
                candidates: &[],
            },
            &support::identity_limits(),
            &mut intlify_authoring_identity::IdentityWorkspace::new(),
        ),
        Err(ReconcileFailure::Pairing(TransitionFailure::UncheckedUnit))
    );
    assert!(matches!(
        reconciled(&chain, (1, 2), &Case::default()),
        Ok(Reconciliation::Unresolved(_))
    ));

    // And a plan is authorized only for the inputs it was made from.
    let authority = authority_over(&chain, 3, &[("alice", ALL)], false);
    let other = allocation(&chain);
    assert_eq!(
        authority
            .invoke(&principal("alice"))
            .unwrap()
            .authorize_update(
                &request(&chain, (2, 3), &other, &[], UpdateMode::Manual),
                &limits()
            )
            .err(),
        Some(AuthorizationFailure::PlanMismatch)
    );
}

#[test]
fn a_permit_is_local_to_its_request_and_its_authority() {
    let chain = Chain::load();
    let plan = restoration(&chain);
    let authority = authority_over(&chain, 3, &[("alice", ALL)], false);
    let invocation = authority.invoke(&principal("alice")).unwrap();
    let confirmation = invocation
        .confirm(chain.registry(2), chain.inventory(3), &restore(&chain))
        .unwrap();
    let permit = invocation
        .authorize_update(
            &request(
                &chain,
                (2, 3),
                &plan,
                std::slice::from_ref(&confirmation),
                UpdateMode::Manual,
            ),
            &limits(),
        )
        .unwrap();
    assert_eq!(permit.check(&authority, chain.registry(2), &plan), Ok(()));
    // The result to publish is the plan's own.
    assert_eq!(permit.result(), plan.result());
    // Reused for another request, or under a changed authority.
    assert_eq!(
        permit.check(&authority, chain.registry(2), &allocation(&chain)),
        Err(PermitMismatch::OtherRequest)
    );
    let changed = authority_over(&chain, 3, &[("alice", ALL)], false);
    assert_eq!(
        permit.check(&changed, chain.registry(2), &plan),
        Err(PermitMismatch::AuthorityChanged)
    );
    // An inventory nothing changed in plans nothing, so there is nothing to
    // authorize and nothing to publish.
    assert_eq!(
        reconciled(&chain, (1, 1), &Case::default()),
        Ok(Reconciliation::Unchanged)
    );
}
