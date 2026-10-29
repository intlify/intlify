<!-- @license MIT -->

# Phase 4 fixtures

Design 015's Phase 4, Target and Compatibility Groups, partitions the Target IDs into Deployment Compatibility Groups exactly, fixes the canonical order of targets, groups, members, and relations, validates the hydration relations, and hands design 020 the Group ID selection vectors. Its resolution phases are 11 to 13: `target-resolution`, `group-resolution`, and `hydration-validation`. Plan 023 implements them as crate-private semantic state on top of Phase 3's. This file maps the phase's coverage to the tests that check it, and names what other phases and designs own.

Like Phase 3, Phase 4 produces no public `LocalizationProjectProfile`, outcome, Finding, or Evidence; Phase 5 builds those from this state. Where a row says a check is blocking, the resolver records a blocking result with 015's code and reason on the unit that found it, and the units that need its value are not evaluated, with that result as their cause.

Each name in a "Checked by" column is a test function in this crate. `tests/fixture_matrix.rs` reads this file and fails when a named test is not a test here. A row naming a later phase or another design states an absence that is a decision, not an oversight.

## Running them

```sh
# every unit and integration test, with the ICU4X provider and the measurements
cargo test -p intlify_config --all-targets --all-features

# the observational measurement, produced and re-admitted from disk
vp run bench:config:smoke
```

## Data files

| File | Contents | Read by |
| --- | --- | --- |
| [`fixtures/handoff/group-selection.json`](../handoff/group-selection.json) | Twelve Handoff Vectors for 020: the six selector forms over a profile with one group and over one with two | The `resolver::handoff` tests |
| [`src/benchmark/cases/expectations-v0.json`](../../src/benchmark/cases/expectations-v0.json) | The fixed input context, result, and work of every measured case, Phase 4's two operations included | The measurement tests and `bench:config:smoke` |

## Completion conditions

Plan 017 lists five exit criteria for Phase 4.

| Condition | Checked by |
| --- | --- |
| Each Target ID belongs to exactly one group | `targets_partitioned_exactly_once_form_their_groups`, `one_target_in_one_group_is_a_partition`, `a_target_in_no_group_or_in_two_is_no_partition`, `a_target_in_no_group_leaves_only_the_groups_unresolved` |
| The authoring order of members and relations does not change semantic equality | `reordering_targets_and_their_locales_changes_nothing_phase_11_records`, `reordering_groups_and_members_changes_nothing_phase_12_records`, `reordering_relations_changes_nothing_phase_13_records`, `reordering_sets_objects_and_the_collection_changes_no_semantic_state`. Equality is by value; profile equality and the digest are Phase 5's |
| A hydration mismatch is a deterministic blocking result | `equal_endpoints_and_a_repeated_pair_are_invalid_relations`, `an_endpoint_naming_no_target_or_another_groups_member_is_invalid`, `a_target_serving_and_hydrating_in_one_group_takes_two_roles`, `endpoints_with_other_locales_or_defaults_are_incompatible_with_nothing_intersected`, `a_statically_known_incompatibility_blocks_the_relation`, `the_roles_come_from_each_target_profiles_body`. The Finding itself is Phase 5's |
| Requirement planning and linking are not implemented on 015's side | `every_vector_has_015s_fields_and_020_verifies_it`, `no_selector_changes_what_the_profile_hands_over`: the vectors leave every relationship to 020, and the resolver admits no selector |
| The target and group measurement keeps its Target, Group, membership, and relation work and a deterministic checksum in the common projection, with accounting, diagnostics, and order kept as 026 adopts them | `every_declared_case_is_admitted_against_fixed_input_result_and_work_expectations`, `the_work_counts_the_bounds_units_and_what_resolved`, `the_work_counts_relations_roles_units_and_what_was_checked`, `the_profile_declares_exactly_the_execution_states_its_cases_run_in`, and `vp run bench:config:smoke`. Neither operation reuses an index or scratch, so every case runs fresh (plan 023 decision #15) |

Design 024 does not yet define the hydration roles of a Target Profile (the SSR renderer and the Browser hydration client) or its static Target and Locale Service compatibility. Plan 023 takes them through an injected seam (its decision ◆#1), so Phase 4 stays in progress even with every condition checked.

## Acceptance examples

Plan 023 fixes thirteen examples.

| Input | Result | Checked by |
| --- | --- | --- |
| Two targets with their subsets, one overriding the default, in one group with one SSR-to-Browser relation whose roles the seam answers | Phases 11 to 13 admit it: each target's subset and effective default, the group's members, and the relation | `each_target_resolves_its_subset_effective_default_and_target_profile`, `targets_partitioned_exactly_once_form_their_groups`, `fifteens_example_relation_holds_and_keeps_its_pair`, `every_phase_admits_a_complete_invocation` |
| A target subset with a locale the project does not request | `project-profile-target-locales-not-subset`; the other targets are still checked | `a_locale_outside_the_project_is_no_subset_and_the_other_checks_go_on`, `a_target_locale_outside_the_project_leaves_the_target_and_its_relation_unresolved` |
| An override outside the subset, or no override and a project default outside it | `project-profile-target-default-locale-not-supported` with `explicit-override` or `inherited-project-default`; no locale of the subset is picked instead | `an_effective_default_outside_the_subset_says_where_it_came_from`, `an_inherited_default_waits_on_the_project_default_it_inherits` |
| A target in no group, or in two | `project-profile-target-unassigned` or `project-profile-target-assigned-to-multiple-groups`; no partial partition | `a_target_in_no_group_or_in_two_is_no_partition` |
| A member naming no target, or named twice in its group | `project-profile-group-member-unknown` or `project-profile-group-member-duplicate` | `a_member_naming_no_target_is_unknown_and_leaves_no_partition`, `a_member_named_twice_in_its_group_is_one_duplicate` |
| Equal endpoints, an unknown endpoint, another group's member, a repeated pair, or a target in both roles | `project-profile-hydration-relation-invalid` with `equal-endpoint`, `unknown-endpoint`, `cross-group`, `duplicate-relation`, or `dual-role` | `equal_endpoints_and_a_repeated_pair_are_invalid_relations`, `an_endpoint_naming_no_target_or_another_groups_member_is_invalid`, `a_target_serving_and_hydrating_in_one_group_takes_two_roles` |
| Endpoints with other subsets or other effective defaults | `project-profile-hydration-locale-policy-incompatible` with `requested-locales` or `effective-default`; nothing is intersected | `endpoints_with_other_locales_or_defaults_are_incompatible_with_nothing_intersected` |
| An endpoint whose Target Profile lacks its role | `project-profile-hydration-relation-invalid` with `server-capability-missing` or `client-capability-missing` | `the_roles_come_from_each_target_profiles_body`, `a_server_without_its_024_role_leaves_only_the_hydration_unresolved` |
| One server hydrating two clients, and two servers hydrating one client | Both valid | `fan_out_and_fan_in_are_valid` |
| The target map, the group map, members, and relations reordered | An equal state, compared by value | `reordering_targets_and_their_locales_changes_nothing_phase_11_records`, `reordering_groups_and_members_changes_nothing_phase_12_records`, `reordering_relations_changes_nothing_phase_13_records` |
| Each `targetGrouping` bound and the endpoints' bytes at their exact value and one more | The exact value holds; one more is a blocking `project-profile-resource-limit-exceeded`, with nothing truncated | `the_map_holds_at_its_bound_and_one_target_more_leaves_every_target_unchecked`, `each_target_id_holds_at_its_bound_and_one_byte_more_blocks_only_that_target`, `the_group_map_holds_at_its_bound_and_one_group_more_leaves_each_group_unchecked`, `every_member_occurrence_counts_toward_the_total_and_one_more_leaves_members_unchecked`, `each_group_id_and_member_count_holds_at_its_bound_and_one_more_blocks_that_group`, `the_relations_hold_at_their_bound_and_one_occurrence_more_leaves_every_relation_unchecked`, `each_endpoint_holds_at_the_target_id_bound_and_one_byte_more_blocks_its_relation` |
| One group, with the selector omitted or naming it | Vectors in which 020 selects that group | `the_committed_vectors_are_the_resolvers`, `no_selector_changes_what_the_profile_hands_over` |
| Several groups, with the selector omitted, unknown, repeated, a Target ID, or part of a group | Vectors in which 020 rejects the selection; 020 verifies them | `the_committed_vectors_are_the_resolvers`, `every_vector_has_015s_fields_and_020_verifies_it` |

## The matrix

The rows come from design 015's [Coverage inventory](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#coverage-inventory), grouped by the resolution phase that checks them.

### Targets (Phase 11)

| Requirement | Checked by |
| --- | --- |
| Each Target ID entry declares a non-empty subset of the project's requested locales | `a_locale_outside_the_project_is_no_subset_and_the_other_checks_go_on`, `required_collections_and_present_selectors_are_nonempty` |
| An override wins over the project default only inside the target's subset; without one, the project default is inherited only when the subset has it, with no first, sole, sorted, or negotiated locale instead | `each_target_resolves_its_subset_effective_default_and_target_profile`, `an_effective_default_outside_the_subset_says_where_it_came_from`, `an_inherited_default_waits_on_the_project_default_it_inherits` |
| A target excluding a project locale is valid | `each_target_resolves_its_subset_effective_default_and_target_profile` |
| A target's locales are checked even when its Target Profile does not resolve, and a repeated locale leaves the target unresolved | `a_target_profile_phase_9_could_not_resolve_leaves_only_that_target_unresolved`, `a_repeated_target_locale_is_still_checked_but_leaves_the_target_unresolved`, `invalid_target_locales_leave_the_checks_that_need_them_not_evaluated` |
| `maxTargetProfiles` and `maxTargetIdBytes` at their exact value and one more | `the_map_holds_at_its_bound_and_one_target_more_leaves_every_target_unchecked`, `each_target_id_holds_at_its_bound_and_one_byte_more_blocks_only_that_target` |
| Without a confirmed policy, or a map structural admission did not admit, nothing more is checked | `without_a_confirmed_policy_no_target_is_checked`, `a_map_structural_admission_did_not_admit_records_only_its_bound` |
| Target IDs order by their unsigned bytes | `subjects_order_the_map_first_then_each_target_after_every_phase_10_unit`, `reordering_targets_and_their_locales_changes_nothing_phase_11_records` |

### Groups (Phase 12)

| Requirement | Checked by |
| --- | --- |
| One Target Profile in one single-target group, and several partitioned exactly once | `one_target_in_one_group_is_a_partition`, `targets_partitioned_exactly_once_form_their_groups` |
| Missing or empty groups and duplicate Group IDs are rejected before resolution | `each_required_declaration_member_rejects_omission`, `required_collections_and_present_selectors_are_nonempty`, `unicode_keys_are_decoded_before_scoped_duplicate_detection` |
| Unknown or duplicate members, unassigned targets, and overlapping membership are blocking, with no inferred or partial group | `a_member_naming_no_target_is_unknown_and_leaves_no_partition`, `a_member_named_twice_in_its_group_is_one_duplicate`, `a_target_in_no_group_or_in_two_is_no_partition` |
| A member waits on the target map's bounds, and a target's place only on its own Target ID | `a_member_waits_on_the_target_map_phase_11_bounds`, `a_place_waits_on_its_own_target_id_alone` |
| `maxDeploymentGroups`, `maxGroupIdBytes`, `maxMembersPerGroup`, and `maxMembershipOccurrences` at their exact value and one more | `the_group_map_holds_at_its_bound_and_one_group_more_leaves_each_group_unchecked`, `every_member_occurrence_counts_toward_the_total_and_one_more_leaves_members_unchecked`, `each_group_id_and_member_count_holds_at_its_bound_and_one_more_blocks_that_group` |
| Without a confirmed policy, or a map structural admission did not admit, nothing more is checked | `without_a_confirmed_policy_no_group_is_checked`, `a_group_map_structural_admission_did_not_admit_records_only_its_two_bounds` |
| Group IDs and members order by their unsigned bytes | `subjects_order_the_two_bounds_then_groups_members_and_assignments`, `reordering_groups_and_members_changes_nothing_phase_12_records` |

### Hydration (Phase 13)

| Requirement | Checked by |
| --- | --- |
| Hydration is declared only through explicit SSR-to-Browser relations, and a group may have none | `fifteens_example_relation_holds_and_keeps_its_pair` |
| Fan-out and fan-in are valid; duplicate pairs, equal endpoints, endpoints outside the group, a missing role, and one target in both roles are blocking | `fan_out_and_fan_in_are_valid`, `equal_endpoints_and_a_repeated_pair_are_invalid_relations`, `an_endpoint_naming_no_target_or_another_groups_member_is_invalid`, `the_roles_come_from_each_target_profiles_body`, `a_target_serving_and_hydrating_in_one_group_takes_two_roles` |
| Both endpoints have equal subsets and effective defaults, and a mismatch is never intersected | `endpoints_with_other_locales_or_defaults_are_incompatible_with_nothing_intersected` |
| A statically known incompatibility of the two Target Profiles is blocking | `a_statically_known_incompatibility_blocks_the_relation` |
| Both endpoints share the project's negotiation profile by construction, so no endpoint-mismatch reason exists | `the_shared_negotiation_profile_is_kept_but_never_compared`, `the_relations_share_the_negotiation_profile_and_never_compare_it` |
| Each check waits only on the facts it compares, and a relation with an unresolved target still checks its endpoints | `each_check_waits_only_on_the_facts_it_compares`, `a_relation_with_an_unresolved_target_still_checks_its_endpoints`, `an_endpoint_naming_a_target_waits_on_the_target_map_phase_11_bounds` |
| `maxHydrationRelations` and the endpoints' `maxTargetIdBytes` at their exact value and one more | `the_relations_hold_at_their_bound_and_one_occurrence_more_leaves_every_relation_unchecked`, `each_endpoint_holds_at_the_target_id_bound_and_one_byte_more_blocks_its_relation` |
| Without a confirmed policy, or a group map structural admission did not admit, nothing more is checked | `without_a_confirmed_policy_no_relation_is_checked`, `a_group_map_structural_admission_did_not_admit_records_only_the_bound` |
| Relations order by `(server, client)` | `subjects_order_the_bound_then_relations_then_roles_after_every_phase_12_unit`, `reordering_relations_changes_nothing_phase_13_records` |

### Group selection handed to 020

| Requirement | Checked by |
| --- | --- |
| Omitted and exact selection are equivalent for a one-group profile | `the_committed_vectors_are_the_resolvers`, `no_selector_changes_what_the_profile_hands_over` |
| With several groups, omission, an unknown Group ID, several selectors, and part of a group fail 020's admission without invalidating the profile | `the_committed_vectors_are_the_resolvers`, `every_vector_has_015s_fields_and_020_verifies_it` |
| The producer fields come from the resolver: every group's members with their locale facts, and the bindings planning one group consumes | `each_group_hands_over_its_members_checked_locale_facts`, `planning_a_group_consumes_its_members_target_profiles_and_the_resource_limits`, `a_profile_whose_targets_do_not_all_resolve_hands_over_no_group`, `a_profile_whose_groups_do_not_partition_its_targets_hands_over_no_group`, `reordering_the_profile_changes_nothing_it_hands_over` |
| Membership and relations are semantic, while the selected Group ID stays compiler-transaction input | `no_selector_changes_what_the_profile_hands_over`. The profile equality that includes them is Phase 5's |

### Measurement

| Requirement | Checked by |
| --- | --- |
| `profile_target_group_resolve / target_and_group_resolution` covers Phases 11 and 12 over 015's example, two scales, a reordering, each decision, the failures before them, and the six bounds' edges | `a_complete_case_resolves_every_target_and_partitions_them`, `each_blocked_recipe_reports_its_one_decision`, `a_prerequisite_failure_leaves_only_what_waits_on_it`, `each_bound_holds_at_its_count_and_one_less_blocks_its_unit`, `reordering_changes_neither_domain_nor_the_work`, `the_work_counts_the_bounds_units_and_what_resolved` |
| `profile_target_group_resolve / hydration_relation_validation` covers Phase 13 over 015's relation, fans, a scale, a reordering, each code and reason, the failures before them, and the two bounds' edges | `a_complete_case_keeps_every_declared_relation`, `each_admitted_target_profile_carries_what_its_body_declares`, `the_work_counts_relations_roles_units_and_what_was_checked`, `hydration_relation_descriptor_binds_the_configuration_and_the_collection` |
| The rows before them stay byte for byte | `target_group_cases_keep_the_252_rows_and_append_after_them`, `hydration_cases_keep_the_281_rows_and_append_after_them` |

## Rows other phases own

| Requirement | Owner |
| --- | --- |
| Findings, Evidence, outcomes, and the profile equality and digest that include targets, groups, and relations | Phase 5 |
| Resolution from a configuration whose structural admission admitted only part of it | Phase 5 (plan 023 decision ◆#2) |
| The Handoff Vectors' closed schema, the Suite Index, and traceability | Phase 6 |
| Selector admission, Requirement and Message Bundle Plans, graph-artifact applicability, and coverage debt | Design 020 |
| Group-scoped Releases, both hydration outputs in one Release Snapshot, and independent publication of groups | Design 025 |
| Render equivalence between hydration outputs | Designs 026 and 030 |
| The negotiation invocation over a target's subset and effective default | Design 027's runtime, with design 023 bounding each invocation |

## Deliberately not covered in Phase 4

- **024's hydration roles and static compatibility** — the injected `hydration::TargetCapabilities` seam, which only tests implement (plan 023 decision ◆#1). Design 024 defines no SSR-renderer or Browser hydration-client capability yet, and 017's revision-`"0"` Target Profile body has no field for one.
- **Index and scratch reuse in the target and group measurement** — both operations build a fresh evaluation and state for every case, so there is no reuse to compare (plan 023 decision #15).
