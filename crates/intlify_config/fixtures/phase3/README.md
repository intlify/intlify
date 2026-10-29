<!-- @license MIT -->

# Phase 3 fixtures

Design 015's Phase 3, the Semantic Profile Resolver, defines profile scope, named-profile selection, identity, the inline locale-policy inputs, the admission of the Intent Surface-Class Vocabulary Artifact, typed Policy and Target Profile references, the typed Profile Resolution Specification Input, Bootstrap Resource Policy Candidate Admission, the admission of the Submitted Profile Resolution Artifact Collection and its duplicate-free Set, Resolved Profile Artifact Bindings, delivery and placement resolution, and deterministic semantic resolution. Plan 022 implements it as crate-private semantic state. This file maps the phase's coverage to the tests that check it, and names what other phases and designs own.

Phase 3 produces no public `LocalizationProjectProfile`, outcome, Finding, Evidence, or Resolver Construction Identity; Phase 5 builds those from this state. Where a row says a check is blocking, the resolver records a blocking result with 015's code and reason on the unit that found it, and the units that need its value are not evaluated, with that result as their cause.

Each name in a "Checked by" column is a test function in this crate. `tests/fixture_matrix.rs` reads this file and fails when a named test is not a test here, so a renamed or deleted test cannot leave a row pointing at nothing. A row naming a later phase or another design states an absence that is a decision, not an oversight.

## Running them

```sh
# every unit and integration test, with the ICU4X provider and the measurements
cargo test -p intlify_config --all-targets --all-features

# the committed schema, and the vectors an independent reader checks
vp run schema:vocabulary:check
vp run vectors:vocabulary:check
vp run vectors:envelope:check

# the observational measurement, produced and re-admitted from disk
vp run bench:config:smoke
```

## Data files

| File | Contents | Read by |
| --- | --- | --- |
| [`fixtures/envelope/vectors.json`](../envelope/vectors.json) | Collection members as 017's envelopes: tuples, canonical submitted bodies, integrity, and the closed content of the five kinds | The envelope, body, and bootstrap tests; `tools/shared-json-vectors` |
| [`fixtures/vocabulary/vectors.json`](../vocabulary/vectors.json) | Intent Surface-Class Vocabulary Artifacts, their digests, and the boundary failures | The vocabulary and coverage tests; `tools/shared-json-vectors` |
| [`schema/intent-surface-class-vocabulary-v0.schema.json`](../../schema/intent-surface-class-vocabulary-v0.schema.json) | The closed Draft 7 schema of the vocabulary artifact | The vocabulary schema tests |
| [`src/benchmark/cases/expectations-v0.json`](../../src/benchmark/cases/expectations-v0.json) | The fixed input context, result, and work of every measured case, Phase 3's four operations included | The measurement tests and `bench:config:smoke` |

## Completion conditions

Plan 017 lists five exit criteria for Phase 3.

| Condition | Checked by |
| --- | --- |
| Resolution starts only from a complete configuration and a complete invocation input | `only_a_complete_configuration_with_its_selection_starts_a_resolution`, `every_phase_admits_a_complete_invocation`, `an_evaluation_runs_one_invocation` |
| A missing required policy, a mismatched reference, and duplicate or conflicting artifacts resolve in 015's precedence | `all_policy_members_are_required_and_only_two_are_nullable`, `a_failed_reference_takes_the_first_decision_that_applies`, `different_bodies_under_one_name_are_conflicting_content_first`, `repeated_exact_occurrences_are_duplicates_before_any_body_is_admitted`, `a_second_envelope_of_the_selected_policy_is_the_bootstraps_duplicate`, `a_second_envelope_of_another_artifact_is_the_collections_duplicate`, `two_bodies_sharing_all_but_the_digest_are_conflicting_content` |
| The basis of semantic equality and canonical projection is fixed | `reordering_sets_objects_and_the_collection_changes_no_semantic_state`, `reordering_sets_moves_only_locations`, `sequences_keep_their_declared_order_while_sources_and_literals_form_sorted_sets`, `the_most_specific_matching_rules_decide_each_cell_whatever_the_authoring_order`, `the_decision_does_not_depend_on_submission_order`. Profile equality and the profile digest themselves are Phase 5's |
| The resolver acquires no artifact and makes no network, credential, or Store query | `no_resolver_module_reads_files_the_network_the_environment_or_processes`, `only_a_complete_configuration_with_its_selection_starts_a_resolution` |
| The locale-policy and artifact measurements record their logical work and semantic checksum reproducibly, project them as 026 adopts them, and keep accounting, diagnostics, order, and the checked or blocked result under reuse | `every_declared_case_is_admitted_against_fixed_input_result_and_work_expectations`, `fresh_and_reused_engines_agree_on_phases_7_and_8_for_every_input`, `planning_work_is_complete_and_checked_before_fixture_preparation`, and `vp run bench:config:smoke` |

Design 018 does not yet define the common Resource Limit Policy body, the Trust Policy body, or the Resource Limit Policy Verification Input. Plan 022 takes them through injected seams (its decision #1), so Phase 3 stays in progress even with every condition checked.

## Acceptance examples

Plan 022 fixes nine examples of the integrated path.

| Input | Result | Checked by |
| --- | --- | --- |
| A complete declaration, eight specification slots, and an exactly matching artifact for every reference | Every phase admits it; the requested set is in byte order, and each distinct reference has one binding | `every_phase_admits_a_complete_invocation` |
| Requested `en-us` and `en-US` | One duplicate naming both occurrences | `two_spellings_of_one_requested_locale_are_a_duplicate_keeping_both` |
| A `resourceLimits` candidate differing only in its digest | The bootstrap's digest mismatch, with no other policy selected | `a_selected_reference_differing_only_in_its_digest_selects_no_other_policy` |
| Two envelopes of the same exact reference | The bootstrap's duplicate envelope | `a_second_envelope_of_the_selected_policy_is_the_bootstraps_duplicate` |
| Two bodies sharing kind, identity, revision, and specification revision but not the digest | Conflicting content | `two_bodies_sharing_all_but_the_digest_are_conflicting_content` |
| No `project-profile` slot | Phase 0 blocks; the independent phases go on | `a_missing_project_profile_slot_blocks_phase_0_and_nothing_waits_on_it` |
| A policy needing more than `policyComparable` allows | Incompatible with the capability; the selection is not confirmed | `a_policy_needing_more_than_the_capability_leaves_the_selection_unconfirmed` |
| A `trust` reference without its artifact | Not found; the other references resolve | `a_trust_reference_without_its_artifact_is_not_found_and_the_rest_resolve` |
| Locale sets, objects, and the collection reordered | An equal semantic state, compared by value; its byte projection is Phase 5's profile projection | `reordering_sets_objects_and_the_collection_changes_no_semantic_state` |

## The matrix

The rows come from design 015's [Coverage inventory](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#coverage-inventory), grouped by the resolution phase that checks them.

### The specification input

| Requirement | Checked by |
| --- | --- |
| One typed Profile Resolution Specification Input with eight fixed slots in 015's order, forming one Set only when every slot is present and compatible | `slots_are_named_and_ordered_as_015_lists_them`, `a_complete_input_admits_every_slot_and_forms_the_set`, `a_set_missing_several_members_names_every_cause` |
| A missing slot, `project-profile` and `locale-canonicalization` included, is `missing-required-slot` and names its slot | `a_missing_slot_is_incomplete_and_names_its_slot`, `a_missing_project_profile_slot_blocks_phase_0_and_nothing_waits_on_it` |
| Another identity is unsupported and another revision incompatible in every slot, with no host fallback for canonicalization | `another_identity_is_unsupported_and_another_revision_incompatible_in_every_specification_slot`, `data_naming_another_specification_or_dataset_is_not_admitted` |
| The configuration `schemaVersion` and the specification revisions are separate version domains | `the_configuration_version_and_the_specification_revisions_are_separate_domains` |
| Phase 0 suppresses nothing itself; a unit needing a slot waits on it | `phase_0_suppresses_nothing_but_keeps_the_set_from_forming`, `the_canonicalization_data_waits_for_its_slot`, `a_missing_slot_hides_only_the_comparison` |

### Resource policy and selection

| Requirement | Checked by |
| --- | --- |
| The selected `resourceLimits` reference is checked before any candidate, and one exact candidate is found by 015's five-field and four-field predicates whatever the submission order | `the_reference_is_checked_before_any_candidate`, `the_sole_exact_candidate_is_admitted_with_its_tuple_and_bounds`, `without_an_exact_candidate_the_closest_tier_decides`, `a_member_without_a_tuple_joins_no_group`, `the_decision_does_not_depend_on_submission_order` |
| Unequal bodies under one name conflict before repeated exact occurrences are duplicates, and both come before any body is admitted | `different_bodies_under_one_name_are_conflicting_content_first`, `repeated_exact_occurrences_are_duplicates_before_any_body_is_admitted` |
| The sole candidate's body is admitted in 015's order, and the envelope vectors' Resource Limit Policies reach their checks | `the_sole_candidates_body_is_admitted_in_015s_order`, `the_envelope_vectors_resource_limit_policies_reach_their_checks` |
| Before the policy is known, the collection and every present reference are counted under the capability, each bound at its exact value and one more | `the_collection_is_counted_as_015_counts_it`, `each_collection_bound_admits_its_exact_value_and_reports_one_more`, `every_present_reference_of_the_declaration_is_counted_once`, `each_reference_is_checked_alone_and_the_selected_ones_bytes_gate_the_decision`, `a_reference_field_structural_admission_did_not_admit_is_counted_only_past_the_bound`, `without_the_selected_reference_only_the_collection_and_the_visible_references_are_checked` |
| A policy needing more than a `policyComparable` bound is incompatible at the first such bound in canonical order | `a_policy_within_every_capability_bound_is_admitted_and_compatible`, `the_first_bound_in_canonical_order_the_capability_lacks_makes_the_policy_incompatible` |
| The selection is confirmed only after a compatible policy and a recheck of what Phases 1 to 3 measured, `maxProfiles` and `maxProfileIdBytes` included, with no other profile selected | `a_compatible_policy_every_recheck_holds_under_confirms_the_selection`, `each_configuration_bound_admits_its_exact_value_and_reports_one_more`, `the_selector_is_rechecked_under_the_policy_and_keeps_only_the_first_over_witness`, `phase_4_rechecks_what_phases_1_to_3_measured`, `a_recheck_whose_input_is_unavailable_leaves_the_selection_unconfirmed`, `without_a_compatible_policy_nothing_is_rechecked_and_the_capability_stays_the_projection_authority` |
| The selected `resourceLimits` artifact is counted again by the policy's `artifactAdmission` bounds | `the_artifact_bounds_are_rechecked_under_the_policy_as_they_were_counted_before_it` |
| Every `projectProfileResolution` bound is a shortest decimal from 1 through `u64::MAX - 1`, in 015's groups and canonical order | `the_section_has_015s_groups_and_bounds_in_canonical_order`, `every_group_and_domain_is_closed_and_holds_every_bound`, `a_bound_is_a_shortest_decimal_string_from_one_through_one_below_u64_max`, `limits_are_positive_shortest_decimals_in_the_u64_range` |
| Without a confirmed policy, the later phases evaluate nothing | `an_unconfirmed_policy_leaves_every_phase_5_unit_not_evaluated`, `an_unconfirmed_policy_leaves_both_fields_not_evaluated`, `unconfirmed_policy_or_unbound_data_leaves_every_locale_not_evaluated`, `without_a_confirmed_policy_nothing_is_admitted` |

### Identity

| Requirement | Checked by |
| --- | --- |
| Declarations with different Profile IDs and equal fields have one project identity, and each selection keeps its own declaration's | `declarations_with_different_profile_ids_and_the_same_fields_have_one_identity`, `each_selection_keeps_its_own_declarations_identity`, `fields_are_named_and_ordered_as_the_declaration_spells_them` |
| Independent named declarations are never merged | `identical_profiles_remain_independent_named_declarations` |
| A field structural admission rejected leaves only its own unit not evaluated | `a_field_structural_admission_rejected_leaves_only_its_own_unit_not_evaluated` |

### Locales

| Requirement | Checked by |
| --- | --- |
| Every locale occurrence of the selected declaration is counted, and every one outside the coverage selectors is canonicalized | `the_complete_blueprint_resolves_every_locale`, `every_role_counts_toward_the_occurrence_bound`, `subjects_order_so_that_every_prerequisite_comes_first` |
| Spellings of one locale share a unit, and a differing spelling gets a non-blocking correction that changes no value | `spellings_of_one_locale_share_a_unit_and_a_differing_one_is_corrected`, `aliases_add_suggestions_and_never_change_a_value_or_unit` |
| Exact duplicates and aliases of one locale are one blocking duplicate naming every occurrence, with no first-wins | `a_locale_two_spellings_name_is_a_duplicate_whether_equal_or_aliases`, `two_spellings_of_one_requested_locale_are_a_duplicate_keeping_both`, `repeats_size_and_membership_are_reported_together` |
| One locale in independent roles or sets is valid, and each target keeps its own set and default | `each_target_keeps_its_own_set_and_default`, `the_complete_blueprint_resolves_every_locale` |
| Sets are ordered by unsigned UTF-8 bytes after canonicalization, and reordering moves only locations | `canonical_locale_order_is_unsigned_byte_order_and_keeps_prefixes`, `reordering_sets_moves_only_locations` |
| Missing and empty requested sets are rejected without inference | `each_required_declaration_member_rejects_omission`, `required_collections_and_present_selectors_are_nonempty` |
| A wildcard, `all`, or a query in place of a requested-locale list is refused structurally; within the list, a wildcard, a range, or a protocol token is an invalid identifier, while `all` is the language CLDR lists under that code | `requested_locales_are_listed_never_declared_dynamically`, `wildcards_ranges_and_protocol_tokens_are_no_locales_but_all_is_a_language` |
| A requested default must be a member of the requested set, and a source default is resolved only when declared | `repeats_size_and_membership_are_reported_together`, `a_source_default_is_resolved_only_when_declared` |
| An invalid spelling suppresses only what needs its identity, and a set waits for every canonical locale it holds | `an_invalid_locale_suppresses_only_what_needs_its_identity`, `a_rejected_spelling_stands_for_itself_and_is_an_issue_an_author_can_fix`, `each_unavailable_field_reaches_only_the_values_that_need_it`, `a_set_needs_every_canonical_locale_and_names_every_one_missing` |
| `maxLocaleOccurrences` counts raw occurrences before duplicates, `maxRequestedLocales` bounds the canonical set, and identifier bytes bound the raw and then the canonical spelling, each exact and first-over | `the_occurrence_bound_holds_at_its_limit_and_reports_the_first_over`, `the_requested_set_is_bounded_by_its_canonical_size`, `identifier_bytes_bound_the_raw_and_then_the_canonical_spelling`, `an_over_bound_declaration_has_nothing_canonicalized`, `an_unavailable_field_leaves_the_bound_unproved_until_the_rest_exceeds_it` |
| A provider failure no author can fix is the resolver's, not the configuration's | `failures_no_author_can_fix_stop_the_pass`, `a_spelling_the_provider_cannot_answer_is_a_resolver_defect` |

### Negotiation and message fallback

| Requirement | Checked by |
| --- | --- |
| An alias map resolves to canonical keys in byte order, and no aliases is an empty map | `a_valid_alias_map_resolves_to_canonical_keys_in_byte_order`, `no_aliases_resolve_to_an_empty_map` |
| Two keys naming one locale are a duplicate, `conflicting-destination` or `same-destination` | `two_keys_naming_one_locale_are_a_duplicate_with_the_reason_their_destinations_give`, `a_conflict_needs_two_admitted_destinations_and_sameness_needs_every_one`, `a_duplicate_whose_reason_waits_on_an_invalid_destination_is_not_evaluated` |
| A destination outside the project's requested set blocks; a key outside it does not | `a_destination_outside_the_project_set_blocks_but_a_key_outside_it_does_not` |
| Omitted and empty fallback are one empty policy | `omission_and_an_empty_mapping_are_one_empty_policy` |
| A sequence keeps its declared order and never expands through another source's sequence | `sequences_keep_their_declared_order_while_sources_and_literals_form_sorted_sets`, `a_sequence_is_never_expanded_through_another_sources_sequence` |
| A source must be requested; a literal may name any locale | `a_literal_may_name_any_locale_but_a_source_must_be_requested` |
| A self-reference, a repeated literal, and a repeated Intent source are blocking with their own reasons | `each_sequence_defect_is_an_issue_with_its_own_reason` |
| Two spellings of one source are a locale duplicate, and each sequence is counted alone | `two_keys_naming_one_source_are_a_locale_duplicate_and_each_sequence_counts_alone` |
| `maxNegotiationAliases`, `maxFallbackSources`, and `maxFallbackCandidatesPerSource` count every submitted entry before any duplicate check | `the_alias_bound_counts_every_submitted_entry_before_any_collision`, `the_source_bound_counts_every_submitted_key_before_any_collision`, `the_candidate_bound_counts_both_kinds_before_any_duplicate_check` |
| Negotiation and fallback are separate policies under their own slots | `each_phase_resolves_its_own_field_under_its_own_slot`, `every_role_hands_each_locale_policy_input_to_phase_8_which_resolves_each_policy` |

### The vocabulary

| Requirement | Checked by |
| --- | --- |
| The vocabulary the slot asserts is admitted with members sorted by their bytes and found only by exact spelling | `the_vocabulary_the_slot_asserts_is_admitted_with_its_members`, `members_are_sorted_by_their_bytes`, `a_member_is_found_only_by_its_exact_spelling` |
| The assertion is compared by identity, then revision, then digest, and a vocabulary that is not intact is never compared | `the_assertion_is_compared_by_identity_then_revision_then_digest`, `an_artifact_that_is_not_intact_is_reported_and_never_compared`, `each_reader_failure_is_its_015_reason` |
| The boundary refuses what is not the closed artifact, in 017's order and within its limits | `an_absent_artifact_is_missing_input`, `each_boundary_failure_has_its_015_reason`, `each_reader_step_fails_in_017s_order`, `member_limits_count_whatever_the_members_are` |
| The semantic digest names the members and nothing else | `the_semantic_digest_names_the_members_and_nothing_else`, `the_preimages_are_the_artifact_without_its_digest_and_the_members_alone` |

### Coverage

| Requirement | Checked by |
| --- | --- |
| Omitted coverage, an empty declaration, and an explicit `direct-required` default are one strict table | `omission_an_empty_declaration_and_an_explicit_direct_default_are_one_table` |
| Only `direct-required` and `fallback-allowed` are modes, and every rule constrains a dimension | `fixed_tags_and_modes_have_no_compatibility_aliases`, `coverage_requires_a_mode_and_at_least_one_constrained_dimension`, `fixed_objects_reject_unknown_members_at_every_level` |
| The most specific matching rules decide each cell whatever the authoring order | `the_most_specific_matching_rules_decide_each_cell_whatever_the_authoring_order`, `a_domain_is_narrower_only_as_a_strict_subset_in_both_dimensions`, `reordered_rules_and_selector_items_change_no_observation` |
| An omitted selector and one naming every member are one domain, and identical or aliased rules are duplicates | `an_omitted_selector_and_one_naming_every_member_are_one_domain`, `identical_or_aliased_rules_are_duplicates_and_identical_ones_share_a_subject` |
| Incomparable maximal rules of two modes conflict only where they overlap | `incomparable_maximal_rules_of_two_modes_conflict_only_where_they_overlap` |
| A locale item is canonicalized, named once, and requested; a surface item is a vocabulary member named once | `a_locale_item_is_canonicalized_named_once_in_its_selector_and_requested`, `a_surface_item_is_a_vocabulary_member_named_once_in_its_selector`, `a_coverage_selector_phase_7_counts_is_phase_8s_to_reject` |
| Rules, selector items, cells, and comparisons are counted before any is checked, with no suffix truncated | `the_rule_bound_counts_every_submitted_rule_before_any_selector_is_checked`, `each_selector_bound_counts_every_raw_item_before_its_items_are_checked`, `the_cell_bound_measures_locales_times_classes_and_no_cell_is_named_past_it`, `the_comparison_bound_counts_every_submitted_rule_against_every_cell`, `a_product_past_u64_exceeds_its_bound_and_a_zero_factor_makes_it_zero` |
| Subjects, rules, domains, and cells are in 015's order | `subjects_order_the_counts_first_then_rules_then_domains_then_the_table`, `a_rule_is_named_by_its_content_with_sorted_selectors_and_every_repeat` |
| A missing vocabulary or requested set leaves only what needs it not evaluated, and an empty vocabulary spans no cell | `a_missing_vocabulary_or_requested_set_leaves_only_what_needs_it_not_evaluated`, `locale_items_wait_on_phase_7s_count_and_each_on_its_byte_bound`, `an_empty_vocabulary_spans_no_cell_and_has_no_member_to_name` |

### Artifact envelopes and bodies

| Requirement | Checked by |
| --- | --- |
| Five closed kinds, each establishing its tuple, with another kind or schema revision unsupported | `kinds_are_spelled_governed_and_ordered_as_017_registers_them`, `a_kind_is_declared_only_by_an_object_with_a_string_kind`, `another_kind_or_schema_revision_string_is_unsupported`, `every_admitted_kind_establishes_its_tuple`, `a_missing_or_malformed_envelope_member_establishes_no_tuple` |
| A body is admitted in 017's order, integrity, then specification revision `"0"`, then closed content, and a failure keeps its tuple | `body_admission_checks_the_integrity_then_the_specification_revision_then_the_content`, `only_specification_revision_zero_is_admitted_for_every_kind`, `a_failing_integrity_or_content_keeps_the_tuple`, `every_admitted_kind_admits_its_closed_content`, `each_policy_content_admits_exactly_its_closed_values` |
| The semantic digest pins kind, specification revision, and content, and the canonical body ignores transport spelling | `the_semantic_digest_pins_kind_specification_revision_and_content_only`, `the_integrity_covers_the_member_but_its_own_digest`, `the_canonical_body_ignores_transport_spelling_and_keeps_the_integrity_digest` |
| A reference names the tuple an envelope establishes, by the canonical bytes of its value | `a_reference_names_the_tuple_an_envelope_establishes`, `a_reference_has_the_canonical_bytes_of_its_value_whatever_its_spelling` |
| A range or selector in a reference's identity or revision fails structural admission, and a label such as `latest` names only an artifact of exactly that revision | `exact_tokens_have_no_normalization_or_selector_coercion`, `a_revision_label_names_only_an_artifact_of_exactly_that_revision` |

### The collection and its Set

| Requirement | Checked by |
| --- | --- |
| The collection container is checked before invocation, within its limits, and keeps every occurrence in submitted order | `an_absent_collection_is_missing_and_an_empty_one_is_admitted`, `each_failure_has_its_015_reason`, `limits_come_before_any_member_is_decoded`, `every_occurrence_is_kept_in_the_order_submitted` |
| Admissible single members form the Set in tuple order, and any member failure leaves no Set | `admissible_single_members_form_the_set_in_tuple_order`, `each_member_failure_has_its_reason_and_leaves_no_set`, `members_with_a_tuple_come_before_members_without_one`, `the_set_holds_each_admitted_body_under_its_tuple` |
| A repeated body is a duplicate whether admissible or not, and members sharing all but the digest or the body conflict | `a_repeated_body_is_a_duplicate_whether_or_not_it_is_admissible`, `members_sharing_all_but_the_digest_or_the_body_conflict`, `a_second_envelope_of_another_artifact_is_the_collections_duplicate` |

### References and bindings

| Requirement | Checked by |
| --- | --- |
| Every present reference names exactly one artifact, in field order, a Target Profile reference like a policy | `every_present_reference_names_its_artifact_in_field_order`, `a_target_profile_reference_is_resolved_like_a_policy` |
| A failed reference takes the first decision that applies, and the other references go on | `a_failed_reference_takes_the_first_decision_that_applies`, `a_trust_reference_without_its_artifact_is_not_found_and_the_rest_resolve` |
| An explicit `null` has no unit and names nothing | `an_explicit_null_has_no_unit_and_names_nothing` |
| Without the Set, no reference is evaluated | `without_the_set_no_reference_is_evaluated`, `a_field_structural_admission_did_not_admit_has_no_references` |
| One binding per distinct reference, none for an explicit `null` or an unreferenced artifact, and bindings only from a complete resolution | `each_distinct_reference_is_bound_once_to_its_artifact`, `an_explicit_null_and_an_unreferenced_artifact_make_no_binding`, `only_a_complete_resolution_makes_bindings` |

### Delivery

| Requirement | Checked by |
| --- | --- |
| Omitted and explicit `duplicate` placement resolve to one policy | `omitted_and_explicit_duplicate_placement_resolve_to_one_policy`, `an_unadmitted_slot_or_field_leaves_the_placement_not_evaluated` |
| `hoist`, an unknown mode, and per-target placement are rejected | `fixed_tags_and_modes_have_no_compatibility_aliases`, `fixed_objects_reject_unknown_members_at_every_level` |

### Dependencies and order

| Requirement | Checked by |
| --- | --- |
| A unit runs when its prerequisites are admitted; otherwise it is not evaluated with their canonical causes, never a cascade of results | `an_admitted_prerequisite_is_evaluated_and_a_blocking_result_becomes_the_cause`, `an_unavailable_prerequisite_leaves_the_unit_not_evaluated_and_reports_nothing`, `causes_from_several_prerequisites_form_one_canonical_set`, `a_notice_belongs_to_an_evaluated_unit_and_is_never_a_cause` |
| Statuses and results are in 015's order whatever order the units ran in | `statuses_and_issues_follow_015s_order_whatever_order_the_units_ran_in`, `units_order_by_phase_then_subject_then_check`, `issue_keys_order_by_unit_then_by_the_bytes_of_code_and_reason` |
| Independent siblings are all checked rather than stopping at the first blocking result | `a_trust_reference_without_its_artifact_is_not_found_and_the_rest_resolve`, `an_invalid_locale_suppresses_only_what_needs_its_identity`, `a_set_missing_several_members_names_every_cause` |
| Each unit is recorded once in one evaluation | `a_unit_is_recorded_once`, `resolving_twice_into_one_evaluation_is_a_resolver_defect`, `each_phases_defect_is_the_resolvers` |

### Measurement

| Requirement | Checked by |
| --- | --- |
| `profile_locale_resolve / locale_policy_resolution` covers Phases 7 and 8 over every role, blocked workloads, bound edges, reordering, and scales, through both providers and engines | `every_role_hands_each_locale_policy_input_to_phase_8_which_resolves_each_policy`, `phase_8_workloads_block_exactly_the_check_each_change_names`, `phase_8_bounds_count_every_role_at_their_exact_values_and_first_over`, `reordered_rules_and_selector_items_change_no_observation`, `the_policy_scales_grow_one_dimension_each_through_icu4x`, `fresh_and_reused_engines_agree_on_phases_7_and_8_for_every_input` |
| `profile_artifact_admit / resource_policy_bootstrap` | `each_decision_stage_reports_its_own_code_and_reason`, `each_bound_edge_holds_at_its_exact_value_and_blocks_one_below`, `the_inputs_hand_phase_4_what_the_earlier_phases_measured` |
| `profile_artifact_admit / complete_artifact_collection_admission` | `an_admitted_collection_is_the_set_of_its_distinct_tuples`, `each_blocked_recipe_reports_its_one_kind_of_failure`, `phase_4_confirms_every_collection_phase_9_checks` |
| `profile_artifact_admit / resolved_binding_construction` | `a_complete_resolution_binds_each_distinct_reference_once`, `unreferenced_artifacts_change_the_work_but_not_the_result`, `phase_4_confirms_every_declaration_phase_9_resolves` |

## Rows other phases own

| Requirement | Owner |
| --- | --- |
| Target locale subsets, effective defaults, Target and Group IDs, the group partition, and hydration relations | [Phase 4](../phase4/README.md) (Algo 11 to 13). Phase 3 canonicalizes and counts every target locale occurrence |
| Construction, the Root Package, the capability as a constructed input, origins, tokens, locators, Findings, Evidence, outcomes, profile equality and digests, replay, and reporting bounds | Phase 5 |
| `bootstrapOnly.artifactBootstrap`'s four bounds on the bootstrap's own work: `maxEnvelopeDepth`, `maxDecodedAllocationBytes`, `maxMetadataScanUnits`, and `maxValidationWorkUnits` | Phase 5, whose Capability Specification closes their units (plan 022 decision #119). Until then the bootstrap counts none of them and reports no `project-profile-bootstrap-limit-exceeded`; the collection's limits still bound every member |
| The Suite Index, case manifests, and traceability | Phase 6 |
| Compatibility conversion, the CLI, and product inputs | Phase 7 and design 029 |
| The negotiation invocation: the portable lookup over a target's supported subset and effective default | Design 027's runtime, over Phase 4's target facts, with design 023 bounding each invocation. Phase 3 resolves the alias map it reads |
| The Intent source locale | Design 016 |
| Fallback probes, Requirement Plans, delivery graphs, and coverage debt | Design 020 |
| Provider routing and synchronization | Design 022 |
| Invalidation slices | Design 019 |

## Deliberately not covered in Phase 3

- **018's bodies and Verification Input** — injected seams (plan 022 decision #1). The trust and Resource Limit Policy contents are read as 017's closed content, and their verification waits on 018.
- **The Admitted Implementation Capability** — test-owned values. Phase 5 constructs it.
