<!-- @license MIT -->

# Phase 3 fixtures

Design 016's Phase 3 adds persistent identity on top of the inventories Phases 1 and 2 produce. Plan 021 implements its pure core: the registry and artifact representations and their admission, transitions and replay, continuity, reconciliation, read-only compilation, design 018's authority evaluation, and design 029's registry operations against an in-memory host. This file describes the data files that pin the representation, and maps each fixture family of the phase to where it is actually checked, so a reader can go from a requirement to the test that pins it without searching, and can see what is deliberately left to a later plan.

The pure core spans two crates. Persistent identity is in this one. Authority and the in-memory host are in `intlify_local_host`; their tests are marked _(local host)_. The ID primitives and the artifact envelope Phase 3 shares with the earlier phases are in `intlify_authoring`, marked _(authoring)_, and the owner run every measuring owner shares is in `intlify_measurement`, marked _(measurement)_. The earlier phases keep their own matrices, for [Phase 1](../../../intlify_authoring/fixtures/phase1/README.md) and [Phase 2](../../../intlify_authoring_js/fixtures/phase2/README.md).

Each name in a "Checked by" column is a test function. `tests/fixture_matrix.rs` reads this file and fails when a named test is not a test in the crate its row says, so a renamed or deleted test cannot leave a row pointing at nothing. A row naming a later plan or phase states an absence that is a decision, not an oversight.

## Running them

```sh
# every unit and integration test, with the measurement owner and the test-owned host
cargo test -p intlify_authoring_identity --all-targets --all-features
cargo test -p intlify_local_host --all-targets --all-features

# the committed schemas, and the vectors an independent reader checks
vp run schema:authoring:check
vp run vectors:authoring:check

# the observational measurement, produced and re-admitted from disk
vp run bench:authoring-identity:smoke

# what an ordinary build of each crate is made of: no randomness, no
# test-owned context, no measurement
cargo tree -p intlify_authoring_identity -e normal -f '{p} [{f}]'
cargo tree -p intlify_local_host -e normal -f '{p} [{f}]'
```

## Data files

| File | Contents | Read by |
| --- | --- | --- |
| `registry-vectors.json` | One registry chain: a genesis and three updates, the three inventories they were planned from, and the source texts those inventories name | `tests/registry_admission.rs`, `tests/registry_transitions.rs`, `tools/shared-json-vectors` |
| `compile-vectors.json` | Design 028's representative module carried from source to compiled artifacts: two revisions read by the JS Producer, a genesis, the update that allocates its declarations and the registry it produces, and the Intents and references compiled for each revision | `tests/compilation.rs`, `tools/shared-json-vectors` |

### The registry chain

The chain is one story about two units, `checkout` and `nav`:

| Update | Source change | Decisions |
| --- | --- | --- |
| 1 | Checkout revision 1 has pay and cancel; nav has home | Three allocations with `confirmed-new`. The base is the genesis, so nothing has history |
| 2 | A header goes in above pay, and cancel's line becomes a copy of pay | Pay continues with `verified-edit`, cancel retires with `complete-absence`, the copy gets a new ID, and a `copy` link ties it to pay. Home's unit did not change, so it needs no decision |
| 3 | Cancel's line comes back at the end | Pay and the copy continue with `verified-edit`, and cancel's old ID is restored with an `explicit` basis |

Every snapshot after the genesis is written out by hand from its base and its update, not produced by applying one. That makes it an expectation for replay to be checked against, rather than a record of what the code currently does: `tests/registry_transitions.rs` applies each update to its base and requires exactly the hand-written snapshot, and verifies the whole chain from its genesis.

The Intent ID and registry identity values are fixed fixture values. They stand in for what a host draws from operating-system randomness. The edits claim the verifier profile `intlify-continuity-edit-replay` revision `0`, whose rules are the continuity phase's to implement.

### The compiled module

`compile-vectors.json` follows [design 028's representative module](../../../intlify_authoring_js/fixtures/phase2/representative-application.js) through the first half of plan 021's acceptance examples. The JS Producer reads it as unit `app`, with three declarations (the greeting `mf2` tag, the `'Save'` text and the `'Welcome'` literal) and four use sites, two of which use the greeting.

| Step | What the vectors hold |
| --- | --- |
| Allocate | The genesis has no history, so all three declarations are new: one update with three allocations, and the registry it produces |
| Compile revision 1 | Each declaration is held exactly by its entry, so its Intent carries no continuity. The two greeting use sites have the same target |
| Compile revision 2 | `'Welcome'` becomes `'Welcome back'` inside its quotes. The registry is unchanged; every Intent continues from its revision 1 declaration across that one edit, and only the welcome's revision changes |

The update, the registry and every Intent and reference are written out by hand from the inventories' declarations and the fixed IDs, not produced by reconciling or compiling. `tests/compilation.rs` requires reconciliation to plan exactly that update and registry from the genesis, and compilation to give exactly those Intents and references.

### What the independent checker verifies

The Node checker in `tools/shared-json-vectors` shares no code with this crate. For the registry chain, and for the registry steps of the compiled module, it:

- re-derives every artifact's integrity digest, with only the top-level `integrityDigest` removed;
- resolves every reference to exactly one earlier artifact of the kind its member requires, so the chain is built in one direction and nothing names its own result;
- checks that a snapshot's update was planned against that snapshot's base, and that the chain keeps its owner, scope and registry identity;
- checks every source snapshot against the retained text it names, by digest and byte length;
- replays every source edit over the retained bytes and compares the result with the after bytes.

For the compiled module's Intents and references it also:

- recomputes each Intent's revision from its declaration's projection, and requires each compilation, an inventory against a registry, to give every declaration of the inventory exactly one Intent;
- requires each Intent's ID to be active in its registry, held at its declaration, or, with a continuity, held at where the continuity starts;
- requires a continuity to be one edit under the edit-replay profile, from that snapshot to the declaration's, carrying the old range exactly onto the declaration with no other active entry held or carried there;
- requires each reference's targets in Intent ID order, each the ID and revision of the Intent it names, and those Intents' declarations to be exactly the ones the use site may use.

### Regenerating

```sh
cargo run -p intlify_authoring_identity --example generate_registry_vectors -- --write
cargo run -p intlify_authoring_identity --example generate_compile_vectors -- --write
vp run vectors:authoring:check
```

A vector is regenerated only when the representation or the inventories it is built from change on purpose. Regenerating to make a failing check pass would turn the independent check into a record of whatever this crate currently emits.

## Completion conditions

016 completes Phase 3 when the adopted 017 minimum identity and registry representations and their admission checks are implemented, independent history fixtures pass, and production publication adopts the applicable 018 authorization slice and 029 host exact-base and atomicity checks. Plan 021 delivers the pure core of that. Production publication needs checked 015 inputs and a 029 host adapter, and is plan 022's.

| Condition | Checked by |
| --- | --- |
| The four 017 kinds are represented, sealed and admitted | `every_committed_vector_is_admitted_and_names_what_came_before_it`, `each_snapshot_rule_is_refused_by_its_own_name`, `each_update_rule_is_refused_by_its_own_name`, `a_sealed_intent_and_reference_are_admitted_as_they_are`, `each_kind_is_read_only_as_itself_and_only_unaltered`, `a_body_that_breaks_a_rule_is_refused_after_its_digest_and_bounds` |
| The committed schemas and the readers accept the same set | `the_committed_schemas_match_the_current_artifacts`, `the_committed_schemas_refuse_what_the_readers_refuse`, `the_committed_schemas_accept_what_the_reader_admits_and_refuse_the_same_damage`, and `vp run schema:authoring:check` |
| Independent history fixtures pass | `every_committed_update_reproduces_the_snapshot_written_by_hand`, `the_committed_chain_is_planned_again_from_its_own_evidence`, `the_committed_chain_proves_every_basis_it_claims`, `compilation_gives_exactly_the_committed_artifacts_and_one_target_per_declaration`, and `vp run vectors:authoring:check` |
| 018's authorization slice is evaluated | `establishment_is_explicit_and_refuses_what_it_cannot_trust` _(local host)_, `least_authority_denies_each_missing_grant_on_its_own` _(local host)_, `input_binding_admits_the_exact_acquired_inputs_only` _(local host)_, `confirmations_bind_the_exact_choice_actor_and_authority` _(local host)_, `automatic_updates_need_the_session_and_proven_decisions` _(local host)_, `owner_semantics_cannot_be_overridden_by_authority` _(local host)_, `a_permit_is_local_to_its_request_and_its_authority` _(local host)_ |
| 029's exact-base and atomicity checks run, against in-memory state | `of_two_updates_prepared_from_one_base_at_most_one_is_published` _(local host)_, `of_two_initializations_exactly_one_succeeds` _(local host)_, `a_change_of_authority_or_session_before_the_commit_refuses_the_write` _(local host)_, `a_publication_is_refused_once_its_base_or_authority_moved` _(local host)_ |
| Production publication | **Plan 022.** Nothing here writes a registry anywhere. A publication says it lives in memory (`Durability::InMemory`), and the only authority established is test-owned (`only_a_test_owned_context_establishes_authority_in_this_phase` _(local host)_). |

## The matrix

The first eleven families are the ones Phase 3 takes from 016's [Conformance and Fixtures](../../../../design/016-intlify-source-authoring-and-intent-identity-design.md#conformance-and-fixtures) table, in its order. The last four are what plan 021 adopts from designs 017, 018, 029 and 026: ID domains, local authority, the in-memory host, and measurement.

### Identity equivalence

| Requirement | Checked by |
| --- | --- |
| A justified edit or move keeps a declaration's identity | `a_wording_edit_inside_a_literal_keeps_the_id_and_moves_the_entry`, `a_whole_unit_moved_to_another_unit_keeps_every_id`, `a_file_moved_with_evidence_of_the_move_keeps_its_ids`, `a_verified_edit_carries_each_id_onto_its_moved_declaration` |
| The same text at distinct declarations is distinct identities with one revision | `equal_text_in_separate_declarations_takes_separate_ids_and_one_revision`, `a_second_declaration_with_the_same_text_gets_a_new_id`, `equal_text_keeps_one_identity_and_gives_the_copy_its_own` |
| Several use sites of one declaration share its target | `compilation_gives_exactly_the_committed_artifacts_and_one_target_per_declaration` |
| Source evidence that moves leaves a revision alone, and a new wording changes the revision and keeps the ID | `a_reworded_welcome_keeps_its_id_across_its_one_edit`, `a_new_wording_keeps_its_id_and_takes_a_new_revision` |

### Identity ownership

| Requirement | Checked by |
| --- | --- |
| Owner kind, owner identity and local value are compared together, so equal values under two owners stay two Intents | `owner_equality_compares_the_complete_kind_and_identity_pair` _(authoring)_, `equal_local_intent_values_under_different_owners_stay_distinct` _(authoring)_ |
| Another owner's candidate, explicit choice or registry is refused | `candidates_are_used_in_order_and_never_made_up`, `an_explicit_decision_is_taken_as_bound_and_fitting_or_refused`, `a_registry_of_another_owner_or_scope_is_refused_before_its_identity` _(local host)_ |
| A new allocation colliding with an active or retired ID of the same owner is refused | `candidates_are_used_in_order_and_never_made_up`, `a_retired_id_is_never_a_fresh_one_and_comes_back_only_by_restore` |

### Continuity evidence

| Requirement | Checked by |
| --- | --- |
| An unchanged snapshot keeps each declaration's entry exactly | `an_entry_whose_declaration_is_still_there_keeps_it`, `a_declaration_still_in_place_keeps_its_id_against_any_label`, `the_declarations_a_registry_holds_compile_to_their_ids_with_no_continuity`, `a_declaration_in_a_unit_that_did_not_change_is_not_gone` |
| A checked edit, and a move across units, carry an ID | `one_edit_carries_an_entry_onto_one_declaration`, `a_range_is_carried_only_by_replacements_clear_of_its_ends`, `replay_needs_the_exact_after_bytes_of_the_exact_snapshots`, `a_file_moved_with_evidence_of_the_move_keeps_its_ids`, `a_whole_unit_moved_to_another_unit_keeps_every_id` |
| A stale registry or source attachment proves nothing | `the_inputs_themselves_are_checked_before_any_basis`, `newness_and_absence_wait_for_the_pins_of_the_base`, `an_edit_starts_at_a_snapshot_the_base_has`, `an_edit_ends_at_a_snapshot_the_inventory_holds`, `bytes_that_are_not_the_snapshot_s_and_repeats_are_refused` |
| One-to-many and many-to-one mappings are ambiguous | `two_old_declarations_carried_onto_one_are_ambiguous`, `one_old_declaration_carried_onto_two_is_ambiguous`, `another_old_declaration_carried_onto_the_same_place_makes_a_continuation_ambiguous`, `an_entry_carried_onto_a_place_another_entry_keeps_competes`, `an_id_carried_onto_a_declaration_another_id_still_holds_is_ambiguous` |
| A match of text, path or position alone is no evidence | `the_same_text_in_two_files_is_not_evidence_of_anything`, `swapping_two_declarations_that_did_not_move_is_ambiguous`, `inserted_text_in_another_file_is_not_evidence`, `a_declaration_whose_role_changed_is_not_the_same_declaration` |
| Missing or ambiguous evidence is reported, and no replacement ID is allocated | `what_no_edit_accounts_for_is_neither_continued_nor_new`, `what_the_evidence_does_not_show_is_reported_and_nothing_is_planned`, `an_entry_that_is_not_seen_any_more_is_not_absent_without_evidence`, `a_declaration_nothing_carries_an_id_onto_is_unresolved_even_when_new` |
| An edit touching a declaration's boundary carries nothing | `a_wording_edit_continues_and_an_edit_at_the_boundary_does_not`, `an_edit_at_a_boundary_carries_nothing` |
| A retained identity takes a changed semantic revision | `a_new_wording_keeps_its_id_and_takes_a_new_revision`, `a_reworded_welcome_keeps_its_id_across_its_one_edit` |
| Only revision 0 of the edit-replay verifier profile is implemented, and a continuation records it | `only_revision_0_of_the_edit_replay_profile_is_implemented`, `a_carried_continuation_records_its_one_edit_under_this_profile` |

### Reconciliation

| Requirement | Checked by |
| --- | --- |
| A genesis allocates every declaration, and a declaration is new only where the evidence shows it | `the_first_reconciliation_allocates_every_declaration_from_the_genesis`, `a_declaration_is_new_only_where_the_evidence_shows_it`, `a_new_declaration_needs_an_empty_base_or_inserted_text`, `a_declaration_both_claimed_and_inserted_is_not_new` |
| A candidate colliding with an active or retired ID | `candidates_are_used_in_order_and_never_made_up`, `a_retired_id_is_never_a_fresh_one_and_comes_back_only_by_restore` |
| A copy beside its original is new when an edit shows it, and nothing is decided when no edit does | `a_copy_beside_its_original_is_new_and_the_original_continues`, `equal_text_keeps_one_identity_and_gives_the_copy_its_own`, `the_original_of_a_copy_is_related_to_the_copy_it_gives_no_id` |
| A retired ID comes back only by an explicit restore, and split, merge and copy links name IDs the base and the result hold | `a_retired_id_comes_back_only_by_restore`, `links_name_ids_the_base_and_the_result_hold`, `a_split_may_keep_its_predecessor_among_its_successors`, `each_update_rule_is_refused_by_its_own_name` |
| An explicit choice is bound to its base and inventory, left to confirm, and refused for any other | `explicit_choices_are_bound_checked_and_left_to_confirm`, `an_explicit_decision_is_taken_as_bound_and_fitting_or_refused`, `an_explicit_decision_that_names_no_declaration_or_twice_one_id_is_refused`, `the_order_explicit_decisions_come_in_does_not_matter` |
| A mismatched base or input stops reconciliation | `inputs_that_cannot_be_planned_from_stop_the_reconciliation`, `an_update_applies_only_with_the_inventory_and_owner_it_names` |
| Of simultaneous conflicting updates, one is published and the other is stale | `of_two_updates_prepared_from_one_base_at_most_one_is_published` _(local host)_, `a_plan_prepared_on_an_earlier_base_is_stale_in_any_session` _(local host)_ |
| An accepted plan is published completely or not at all, in memory | `a_publication_is_refused_once_its_base_or_authority_moved` _(local host)_, `a_failed_draw_makes_nothing_current` _(local host)_, `the_host_bounds_refuse_a_write_rather_than_drop_anything` _(local host)_ |
| An inventory in which nothing changed changes nothing | `an_inventory_nothing_changed_in_changes_nothing`, `the_same_source_again_changes_nothing` |

### Registry update history

| Requirement | Checked by |
| --- | --- |
| An update names its owner, base, inventory, sources, decisions and result exactly | `every_committed_vector_is_admitted_and_names_what_came_before_it`, `a_successor_pairs_its_base_only_with_an_update_planned_against_it`, `replay_checks_the_link_before_it_applies_anything`, `each_generation_keeps_the_update_that_produced_it` _(local host)_ |
| A mismatched decision or source attachment is refused by name | `each_update_rule_is_refused_by_its_own_name`, `each_transition_rule_is_refused_by_its_own_name`, `what_the_update_names_is_checked_before_what_it_decides`, `replay_refuses_a_stored_result_the_update_does_not_produce` |
| Of two updates from one base, the stale one is refused | `of_two_updates_prepared_from_one_base_at_most_one_is_published` _(local host)_, `a_plan_prepared_on_an_earlier_base_is_stale_in_any_session` _(local host)_ |
| Publication is all or nothing | `a_publication_is_refused_once_its_base_or_authority_moved` _(local host)_, `the_host_bounds_refuse_a_write_rather_than_drop_anything` _(local host)_ |
| Ordinary compilation leaves the registry unchanged | `compilation_leaves_its_inputs_as_they_were_and_names_only_active_ids`, `compilation_allocates_nothing_and_leaves_its_inputs_as_they_were`, `analysis_compiles_against_the_current_registry_and_changes_nothing` _(local host)_ |
| An update without decisions reuses its base | `an_update_without_decisions_reuses_its_base`, `an_update_without_decisions_is_unchanged_only_after_every_check`, `a_snapshot_recorded_for_an_update_that_changes_nothing_does_not_replay` |
| Every decision leaves the entry the transition table describes, and nothing else changes | `the_transition_table_admits_exactly_its_rows`, `each_decision_leaves_the_entry_the_table_describes`, `the_result_keeps_every_undecided_entry_and_adds_what_each_decision_leaves`, `applying_never_changes_what_the_base_holds_for_an_undecided_id` |

### Automatic continuation updates

| Requirement | Checked by |
| --- | --- |
| A development session enables automatic updates, and without one nothing is accepted automatically | `development_updates_need_an_active_session_and_only_proven_decisions` _(local host)_, `a_development_session_changes_the_authority_and_no_generation` _(local host)_, `a_development_update_needs_the_session_and_only_proven_decisions` _(local host)_ |
| A verified one-to-one edit or move is accepted without a confirmation | `a_proven_plan_needs_no_confirmation_and_takes_none` _(local host)_, `a_development_session_publishes_only_proven_plans` _(local host)_ |
| Ambiguity needs an explicit decision, and that decision its confirmation | `explicit_choices_are_bound_checked_and_left_to_confirm`, `an_explicit_decision_is_published_only_with_its_confirmation` _(local host)_ |
| An unresolved plan or a stale base is never published | `an_unresolved_preparation_is_reported_as_it_is` _(local host)_, `a_plan_prepared_on_an_earlier_base_is_stale_in_any_session` _(local host)_ |
| A retained ID takes a changed revision, and read-only compilation behaves as before | `a_reworded_welcome_keeps_its_id_across_its_one_edit`, `the_same_source_and_registry_give_the_same_bytes_and_nothing_to_update` |

### Automatic allocation and retirement

| Requirement | Checked by |
| --- | --- |
| Enabled and disabled mode | `automatic_updates_need_the_session_and_proven_decisions` _(local host)_, `development_updates_need_an_active_session_and_only_proven_decisions` _(local host)_ |
| Confirmed-new is told apart from an unresolved continuation | `a_declaration_is_new_only_where_the_evidence_shows_it`, `what_no_edit_accounts_for_is_neither_continued_nor_new`, `a_declaration_both_carried_and_inserted_is_not_new` |
| Allocated IDs replay, and a collision is refused | `the_committed_chain_is_planned_again_from_its_own_evidence`, `preparing_draws_exactly_the_ids_new_declarations_need` _(local host)_, `candidates_are_used_in_order_and_never_made_up` |
| Absence from a complete inventory, with every decision resolved, retires | `a_deleted_line_retires_its_id_only_from_a_complete_view`, `absence_needs_a_resolved_plan_and_a_replacing_edit_or_a_removed_unit`, `a_complete_view_retires_what_a_partial_view_keeps` |
| Retirement changes only an entry's state: history stays, and the ID is never reused | `a_retired_entry_keeps_a_declaration_an_active_one_may_hold_again`, `retired_entries_claim_nothing`, `a_retired_id_does_not_claim_the_place_it_last_held`, `a_retired_id_does_not_take_back_its_declaration_without_an_explicit_restore`, `a_retired_id_is_never_a_fresh_one_and_comes_back_only_by_restore` |

### Registry recovery

| Requirement | Checked by |
| --- | --- |
| A missing or unavailable registry is never taken for a first initialization | `a_binding_whose_control_state_is_lost_is_never_new_again` _(local host)_, `enrollment_happens_once_and_never_from_an_unavailable_binding` _(local host)_, `a_guard_a_panic_left_behind_reads_as_unavailable` _(local host)_ |
| A chain is verified from its genesis, or from an anchor the host accepted | `a_chain_verifies_from_the_genesis_or_from_a_snapshot_the_host_accepted`, `a_chain_is_verified_only_from_the_anchor_the_host_names`, `a_genesis_anchor_needs_a_genesis_and_an_accepted_one_takes_the_host_s_word`, `an_anchor_later_than_the_head_never_anchors_it` |
| A stale or foreign candidate registry is refused | `a_registry_outside_the_destination_or_its_anchors_is_refused` _(local host)_, `a_registry_of_another_owner_or_scope_is_refused_before_its_identity` _(local host)_, `a_pinned_earlier_registry_is_read_and_never_treated_as_current` _(local host)_ |
| A forged link, a stored result the update does not produce, and bytes that are not a snapshot's are named | `a_chain_is_replayed_within_its_bound_and_a_forged_link_is_named`, `replay_refuses_a_stored_result_the_update_does_not_produce`, `bytes_that_are_not_the_snapshot_s_and_repeats_are_refused` |
| Nothing is rebuilt from text or position, and a missing input blocks the replay | `a_chain_missing_an_input_is_blocked_and_nothing_stands_in`, `a_blocked_walk_names_the_update_then_its_inventory_then_the_base` |

### Inventory completeness

Recognizing units, and claiming a complete or partial scope, are Phase 2's. What identity does with that claim is here.

| Requirement | Checked by |
| --- | --- |
| A complete inventory has to be the host's membership exactly | `a_partial_inventory_needs_only_its_own_units_in_the_membership`, `a_unit_without_declarations_is_still_one_of_the_base_s`, `only_a_complete_inventory_has_to_cover_every_acquired_unit` _(local host)_ |
| A partial view keeps what it does not cover, and retires nothing | `a_partial_inventory_keeps_what_it_does_not_show`, `a_partial_inventory_updates_what_it_covers_and_keeps_what_it_cannot_see`, `an_entry_a_partial_view_does_not_cover_stays_even_if_an_edit_brings_it_in`, `a_partial_view_keeps_what_it_does_not_see` _(local host)_ |
| A unit that was not checked plans nothing | `an_inventory_with_a_unit_that_was_not_checked_plans_nothing`, `no_update_is_planned_from_a_unit_that_was_not_checked` |
| A unit removed in a complete inventory is gone from the scope | `a_unit_an_edit_removes_has_to_be_gone_from_the_scope`, `only_a_unit_gone_from_the_inventory_is_removed`, `a_removal_of_a_unit_still_in_the_scope_is_not_an_absence` |
| The edits are one account of the change | `edits_from_the_base_to_the_inventory_are_one_account`, `each_unit_is_on_each_side_at_most_once`, `only_a_unit_new_to_the_base_is_written_from_nothing` |
| Test-owned inputs are never production inputs | `the_ordinary_entry_never_admits_a_test_inventory` _(authoring)_, `only_a_test_owned_context_establishes_authority_in_this_phase` _(local host)_ |

### Handoff

| Requirement | Checked by |
| --- | --- |
| A stable ID is distinct from a target handle | **Phase 4 and 5.** No target handle is allocated here. |
| A source artifact needs no Provider work | `compilation_gives_exactly_the_committed_artifacts_and_one_target_per_declaration`, `a_sealed_intent_and_reference_are_admitted_as_they_are` |
| A reference names a finite set of targets and claims no final reachability | `targets_are_a_nonempty_set_of_the_use_sites_owner_in_id_order`, `every_reference_it_returns_stays_within_the_target_bound` |
| A blocked result is never accepted as complete input | `an_inventory_with_a_unit_that_was_not_checked_plans_nothing`, `no_update_is_planned_from_a_unit_that_was_not_checked`, `inputs_that_cannot_be_compiled_against_the_registry_are_refused` |
| A supplied artifact is checked against the compilation it came from | `a_supplied_artifact_is_checked_against_the_compilation_it_came_from`, `a_compiled_intent_passes_and_each_difference_is_named`, `a_compiled_reference_passes_and_each_difference_is_named`, `a_continuity_is_accepted_only_as_compilation_records_it` |

### Performance safety

| Requirement | Checked by |
| --- | --- |
| Every bound admits its exact value and refuses the next | `a_count_at_its_bound_is_within_it_and_one_more_is_not`, `every_bound_admits_its_exact_value_and_refuses_one_less`, `bounds_and_cancellation_stop_before_anything_is_planned`, `diagnostics_are_bounded_by_the_caller`, `the_bound_counts_the_updates_replayed_from_the_head`, `the_candidate_bound_admits_its_exact_value` _(local host)_, `the_candidate_bound_is_checked_before_anything_is_drawn` _(local host)_ |
| A workspace resets, and a reused one gives what a fresh one gives, after a success, a failure and a cancellation | `clearing_keeps_capacity_and_drops_state`, `a_reused_workspace_gives_what_a_fresh_one_gives`, `a_stopped_or_reused_compilation_gives_nothing_or_the_fresh_result`, `a_reused_workspace_agrees_with_a_fresh_one_after_success_failure_and_cancellation` |
| A lookup is by exact key, never by search | `a_retained_artifact_is_found_only_by_its_exact_reference`, `each_artifact_is_found_by_its_exact_occurrence_only`, `a_declaration_is_found_only_when_it_is_exactly_one_of_the_inventory`, `bytes_are_found_only_by_the_exact_snapshot_that_names_them` |
| Order and threads do not change a result | `the_same_inputs_give_the_same_plan_on_any_thread`, `the_builders_record_canonical_order_and_report_repeats`, `the_order_explicit_decisions_come_in_does_not_matter`, `grants_and_sources_are_found_in_whatever_order_the_host_gives_them` _(local host)_ |
| Instrumentation stays out of ordinary builds | the `cargo tree` commands in [Running them](#running-them) |

### ID domains

| Requirement | Checked by |
| --- | --- |
| An Intent ID's value and a registry identity are 32 lowercase hexadecimal digits | `sixteen_bytes_spell_the_registered_lowercase_digits` _(authoring)_, `a_registry_identity_is_exactly_32_lowercase_hexadecimal_digits`, `drawn_bytes_are_spelled_in_order_for_the_owner` _(local host)_ |
| A registry identity names a chain and is never an Intent ID | `a_value_names_a_chain_without_becoming_another_value` |
| A fresh value comes from operating-system randomness, and a failed draw allocates nothing | `the_operating_system_gives_fresh_values_each_time` _(local host)_, `a_failure_gives_no_value_and_no_part_of_a_list` _(local host)_, `a_failed_draw_allocates_nothing_and_nothing_stands_in` _(local host)_, `operating_system_randomness_gives_each_chain_its_own_identity` _(local host)_ |
| A candidate colliding with an active, retired or chosen ID, a repeated one, and another owner's are refused, and too few are an allocation failure | `candidates_are_used_in_order_and_never_made_up` |
| Replay never generates a value again | `every_committed_update_reproduces_the_snapshot_written_by_hand`, `the_host_carries_the_committed_chain_from_its_genesis` _(local host)_ |

### Local authority

| Requirement | Checked by |
| --- | --- |
| Establishment is explicit, test-owned in this phase, and refuses what it cannot trust | `establishment_is_explicit_and_refuses_what_it_cannot_trust` _(local host)_, `only_a_test_owned_context_establishes_authority_in_this_phase` _(local host)_, `a_malformed_setup_is_refused_for_its_rule` _(local host)_, `the_test_owned_entry_sets_up_exactly_what_it_is_given` _(local host)_ |
| Each action needs its own grant | `least_authority_denies_each_missing_grant_on_its_own` _(local host)_, `a_read_only_analysis_needs_its_read_grants_and_nothing_more` _(local host)_, `a_registry_is_read_with_its_read_grant_alone` _(local host)_, `each_action_has_the_spelling_design_018_gives_it` _(local host)_ |
| Inputs are bound to exactly what was acquired | `input_binding_admits_the_exact_acquired_inputs_only` _(local host)_, `an_inventory_outside_the_scope_is_refused_for_what_differs` _(local host)_, `an_analysis_reports_its_registry_before_its_inventory` _(local host)_ |
| A historical read admits a pinned registry without making it current | `a_registry_outside_the_destination_or_its_anchors_is_refused` _(local host)_, `a_pinned_earlier_registry_is_read_and_never_treated_as_current` _(local host)_ |
| A genesis is initialized once, under its own grant | `an_initialization_is_one_empty_genesis_for_an_uninitialized_binding` _(local host)_, `initialization_needs_its_own_grant_and_an_uninitialized_binding` _(local host)_ |
| A confirmation binds the exact choice, actor and authority | `confirmations_bind_the_exact_choice_actor_and_authority` _(local host)_, `a_confirmation_binds_the_choice_to_its_inputs_and_authority` _(local host)_, `a_confirmation_counts_only_for_its_exact_choice_under_this_authority` _(local host)_ |
| An automatic update needs the session and only proven decisions | `automatic_updates_need_the_session_and_proven_decisions` _(local host)_, `a_development_update_needs_the_session_and_only_proven_decisions` _(local host)_ |
| Authority never overrides owner semantics | `owner_semantics_cannot_be_overridden_by_authority` _(local host)_ |
| A permit covers its own request under its own authority | `a_permit_is_local_to_its_request_and_its_authority` _(local host)_, `a_permit_covers_its_own_request_under_its_own_authority_only` _(local host)_, `an_authority_is_the_one_established_not_its_content` _(local host)_ |

### In-memory host

| Requirement | Checked by |
| --- | --- |
| Reads, analyses and empty plans publish nothing | `reads_analyses_and_empty_plans_publish_nothing` _(local host)_, `a_read_says_whether_its_registry_is_current` _(local host)_ |
| A binding is enrolled and initialized once | `of_two_initializations_exactly_one_succeeds` _(local host)_, `enrollment_happens_once_and_never_from_an_unavailable_binding` _(local host)_, `initialization_draws_one_identity_and_makes_an_empty_genesis_current` _(local host)_, `initialization_draws_nothing_for_a_caller_without_its_grant` _(local host)_ |
| Preparation draws exactly the IDs new declarations need, after the bound is checked | `preparing_draws_exactly_the_ids_new_declarations_need` _(local host)_, `the_candidate_bound_is_checked_before_anything_is_drawn` _(local host)_ |
| The committed chain is carried from its genesis: verified continuations, a copy link, and an explicit restore | `the_host_carries_the_committed_chain_from_its_genesis` _(local host)_ |
| A partial view keeps what it does not see | `a_partial_view_keeps_what_it_does_not_see` _(local host)_ |
| An unchanged result holds only while its base is current | `an_unchanged_result_holds_only_while_its_base_is_current` _(local host)_ |
| A change of authority or session before the commit refuses the write | `a_change_of_authority_or_session_before_the_commit_refuses_the_write` _(local host)_, `a_publication_is_authorized_at_the_commit` _(local host)_ |
| Each publication keeps its provenance | `each_publication_keeps_its_provenance` _(local host)_, `each_generation_keeps_the_update_that_produced_it` _(local host)_ |
| A session is established from the host state it opened in, for a caller the host established | `a_session_is_established_from_the_host_state_it_opened_in` _(local host)_, `opening_refuses_what_the_host_cannot_establish` _(local host)_, `a_caller_the_host_did_not_establish_opens_nothing` _(local host)_ |

### Measurement

The owner run is shared, so a rejection pinned on the minimal test owner in `intlify_measurement` holds here too.

| Requirement | Checked by |
| --- | --- |
| Every operation is covered, and no fixture name repeats | `every_operation_is_covered_and_no_fixture_name_repeats` |
| Every fixture takes the path it declares, and one that does not is never planned | `every_fixture_takes_the_path_it_declares`, `a_fixture_that_takes_another_path_than_it_declares_is_never_planned` _(measurement)_ |
| A bound sits at the fixture's own count, or one below | `each_bound_is_set_at_the_fixture_count_or_one_below_it`, `a_count_only_another_path_reports_is_no_fixture` |
| A fixture is given what a host retains, and the catalog is many short UI literals | `every_plan_is_given_what_a_host_retains`, `the_catalog_is_many_short_ui_literals` |
| Two captures agree on observation and counted work, in the workspace the case lends | `every_fixture_captures_and_repeats_the_same_observation`, `a_planning_capture_reconciles_in_the_workspace_the_case_lends`, `the_measured_invocation_reconciles_in_the_case_workspace` |
| Each count of the work vector says how it is known | `every_fixture_reports_the_work_it_did_and_how_each_count_is_known`, `each_class_is_counted_and_framed_under_its_own_name_and_tag`, `each_decision_is_counted_under_its_own_kind` |
| An observation is framed item by item, apart from where it points | `a_record_is_framed_as_this_revision_of_the_observation_writes_it`, `a_classification_is_framed_declaration_by_declaration_then_entry_by_entry`, `eligibility_is_framed_in_decision_order_with_what_needs_confirmation`, `each_planning_path_is_framed_under_its_own_tag`, `each_replay_path_is_framed_under_its_own_tag` |
| The boundary says what is inside and outside the interval | `a_boundary_names_its_operation_and_keeps_the_two_marker_sets_disjoint`, `both_intervals_hold_the_operation_and_none_of_the_harness_work`, `planning_excludes_allocation_and_everything_its_inputs_need`, `replay_excludes_indexing_the_history_and_lends_no_scratch`, `a_case_is_described_by_its_own_interval_and_the_clock_it_was_given` |
| A case projection is distinct, stable, carries no run dimension, and names what it measured | `every_fixture_projects_to_a_distinct_case_identity`, `a_case_identity_does_not_change_between_two_preparations`, `a_projection_carries_no_run_clock_or_sample_dimension`, `every_fixture_names_what_it_reads_and_the_bound_it_is_measured_at`, `a_case_names_its_owner_its_profile_and_what_it_measured`, `the_projection_names_the_result_schema_revision_the_run_records`, `the_operations_and_their_intervals_are_spelt_as_registered` |
| A run re-admits from its own bytes, and a withheld, rehashed, duplicate or foreign record is refused | `a_run_produces_records_that_re_admit_from_their_own_bytes`, `a_withheld_record_is_missing`, `a_rehashed_change_to_a_saved_record_is_not_admitted`, `a_duplicate_a_foreign_or_no_owner_document_resolves_no_evidence` |
| A repetition sum past the quantity domain fails the case | `a_repetition_sum_past_the_quantity_domain_fails_a_case` |
| The package says whether its assertions run | `the_package_says_whether_its_assertions_run` |
| The three earlier owners still produce and re-admit their records | `vp run bench:authoring:smoke`, `vp run bench:authoring-js:smoke` and `vp run bench:config:smoke` |

## Families other phases own

016 lists these families too. None is approximated here.

| Family | Owner |
| --- | --- |
| Binding identity, Automatic UI, DOM receiver aliases, Receiver escape, Non-call receiver effects, Receiver control flow, Exclusion, Metadata, Scope defaults, Extraction, Parser ownership, Parameters and selection, Locale handoff, Revision comparison, Test-only Profile inputs | Phases 1 and 2, in their own matrices. A unit's recognition and its complete or partial claim are Phase 2's as well |
| Module references | A Phase 4 extension. A module reference is still reported as unsupported |

## Deliberately not covered in Phase 3

These are named so that their absence is a decision rather than an oversight.

- **Local persistence and production publication** — generations on disk, one current pointer, pending markers, the outcome of an interrupted transaction, locks, fault injection, and reopen, restart and multi-process tests (design 029), with retained provenance (design 018). They are plan 022's, once checked 015 inputs, policy bodies and a bootstrap format exist.
- **Production authority** — only the explicitly test-owned context establishes authority. A production `LocalizationProjectProfile` and its checked inputs are design 015's.
- **Target handles and the consumer handoff** — Phases 4 and 5, through designs 017, 019, 020, 024 and 028.
- **Conditional selection** — 016-010 is accepted but not implemented, as in Phase 2.
- **Allocation as a measured operation** — drawing an ID is the host's, and is measured with plan 022's host. The measurement reports candidate comparisons as unavailable rather than counting them.
- **Two questions to the 017 and 019 owners** — an update does not record the membership it was planned against, so a reader replaying it cannot check that an inventory claiming to be complete covered every unit (plan 021, interpretation #11); and an inventory's basis does not include the binding configuration, so a configuration change is seen only where it changes the basis, which stops automatic newness and absence (interpretation #12).
