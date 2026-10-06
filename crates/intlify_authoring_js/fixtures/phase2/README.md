<!-- @license MIT -->

# Phase 2 fixture matrix

Design 016's Phase 2 names the families of behaviour a host Producer has to fix in tests. This file maps each family to where it is actually checked, so a reader can go from a requirement to the test that pins it without searching, and can see which parts are deliberately left to a later phase.

Phase 2 spans three crates. Host recognition is in this one. The `authoring-inventory` representation, and the language-neutral extensions a host needed, are in `intlify_authoring`; their tests are marked _(authoring)_. The owner run every measuring owner shares is in `intlify_measurement`; its tests are marked _(measurement)_. Phase 1's own families keep their matrix in [`intlify_authoring/fixtures/phase1/README.md`](../../../intlify_authoring/fixtures/phase1/README.md).

Each name in a "Checked by" column is a test function. `tests/structure.rs` reads this file and fails when a named test is not a test in the crate its row says, so a renamed or deleted test cannot leave a row pointing at nothing. A row naming a later phase states an absence that is a decision, not an oversight; nothing here is covered by approximation.

## Running them

```sh
# every unit and integration test, with the measurement owner
cargo test -p intlify_authoring_js --all-targets --all-features
cargo test -p intlify_authoring --all-targets --all-features
cargo test -p intlify_measurement --all-targets --all-features

# the committed inventory schema and the independent inventory vectors
vp run schema:authoring:check
vp run vectors:authoring:check

# the observational measurement, produced and re-admitted from disk
vp run bench:authoring-js:smoke

# what an ordinary build of this crate is made of
cargo tree -p intlify_authoring_js -e normal,features
```

## Data files

| File | Contents | Read by |
| --- | --- | --- |
| `extraction.json` | The host-side cases of 016's Extraction family: what each literal in a file decodes to, and where each run of decoded text came from | `src/cooked/tests.rs` |
| `extraction/*` | The source files those cases name | the same test, through the case's digest |
| `representative-application.js` | Design 028's representative application, byte for byte as the design writes it | `tests/inventory.rs`, and the `vertical-slice-module` and inventory measurement fixtures in `src/benchmark/cases.rs` |

Most host fixtures are written inline in the test that reads them, with every expected range found by searching the source text for the bytes it covers, never read back from the analysis under test.

## Exact bytes

Several files carry bytes a checkout or a formatter would otherwise rewrite: CRLF inside a template, a line continuation before CRLF, a byte order mark. The directory is marked `-text` in `.gitattributes` and left out of formatting and linting in `vite.config.ts`.

Neither setting is relied on alone. Every case names its file's byte length and SHA-256 digest, and the test checks the file against them before decoding anything, the same way a Producer checks supplied bytes against a snapshot. A file whose line endings were rewritten fails there, instead of passing against different source than the case describes.

## Expected values

Every expected value was derived by hand from the ECMAScript grammar and the file's bytes, not copied from the decoder. A string literal's span includes its quotes; a template element's span is its content alone. A decoded literal gives its text and its input map as `[decodedStart, decodedEnd, sourceStart, sourceEnd]` runs. A literal the profile does not accept gives the form and the source range of the escape responsible.

The decoder's output is also compared with the parser's own cooked value on every literal. Hand-derived values and the second decoder check different things: the first catches a decoder that agrees with the parser on the text but maps it to the wrong bytes, and the second catches a decoder whose text is wrong.

## Completion conditions

016 completes Phase 2 when recognition and diagnostic fixtures pass, each source is parsed under an explicit profile, no duplicate extraction or host-code execution occurs, conditional selection remains unsupported, and the minimum verification and measurement gate passes.

| Condition | Checked by |
| --- | --- |
| Recognition and diagnostic fixtures pass | every family below |
| Each source is parsed once, under the grammar and profile the caller names | `each_text_unit_is_parsed_exactly_once`, `the_grammar_comes_from_the_snapshot_never_from_the_unit_name`, `every_grammar_sets_its_goal_rather_than_inferring_it`, `a_context_pinning_another_profile_is_refused`, `no_public_function_reads_a_grammar_from_a_path_or_a_suffix` |
| No duplicate extraction | `explicit_authoring_at_a_known_sink_is_read_once_as_explicit`, `declarations_overlap_when_they_share_a_byte_in_any_order`, `the_representative_application_is_one_complete_checked_inventory`, `the_artifact_does_not_depend_on_order_or_scheduling` |
| No host code is executed | `no_javascript_engine_is_anywhere_in_the_dependency_tree` |
| Conditional selection remains unsupported | `a_conditional_selection_is_reported_and_neither_alternative_is_chosen` |
| The minimum verification and measurement gate | [Determinism and storage](#determinism-and-storage), [Performance safety](#performance-safety), [Measurement](#measurement) |

## The matrix

The first nineteen families follow 016's [Conformance and Fixtures](../../../../design/016-intlify-source-authoring-and-intent-identity-design.md#conformance-and-fixtures) table in its order. The last four are what Phase 2 adopts from designs 017 and 026: the inventory, its artifact and admission, determinism and storage, and measurement.

### Binding identity

| Requirement | Checked by |
| --- | --- |
| A named import and an import alias bind the intrinsic | `a_named_import_and_its_alias_both_bind_the_intrinsic`, `a_direct_named_import_binds_its_local_name_whatever_it_is` |
| Shadowing by a parameter or a nested binding, an unrelated module's export, and a spelling with no import are not intrinsics and report nothing | `a_name_that_does_not_resolve_to_the_import_is_not_the_intrinsic` |
| A wrapper, member access, a reassigned alias, an optional or constructed call, and an export are reported where used | `a_known_intrinsic_used_other_than_directly_is_reported_where_it_is_used` |
| Default, namespace, re-export, literal dynamic and TypeScript `import =` forms of a registered module are reported once | `a_registered_module_imported_another_way_is_reported_once_where_it_is_imported`, `each_other_form_importing_a_registered_module_is_reported_where_it_is_written`, `a_typescript_import_require_of_a_registered_module_is_reported` |
| `import type` and a type position bind no value | `a_type_only_import_or_type_position_binds_no_value`, `a_type_only_import_or_export_binds_no_value` |
| An unregistered name from a registered module is ordinary | `an_unregistered_name_from_a_registered_module_is_ordinary`, `re_exporting_what_is_not_an_intrinsic_is_ordinary` |
| A contradictory binding configuration builds no profile | `a_binding_set_that_contradicts_itself_builds_no_profile`, `a_binding_set_that_contradicts_itself_is_refused` |
| An empty binding configuration recognizes nothing | `a_profile_that_registers_nothing_recognizes_nothing`, `a_profile_that_registers_nothing_reads_no_form_but_still_reads_the_unit` |
| A script has no imports, and `require()` is not one | `a_script_has_no_imports_and_so_no_intrinsics` |

### Automatic UI

| Requirement | Checked by |
| --- | --- |
| A static `textContent` assignment from each admitted origin, with the usage its sink gives | `a_literal_assigned_to_each_admitted_origin_is_displayed_text`, `a_proven_literal_is_declared_as_displayed_text_and_used_where_assigned` |
| Displayed text keeps its braces as characters | `displayed_text_keeps_its_braces_as_characters` |
| An origin call as the receiver itself | `an_origin_call_can_be_the_receiver_itself` |
| A shadowed or declared `document`, `window.document`, a computed member, another DOM API, and a non-DOM `textContent` | `only_the_standard_document_and_its_two_calls_are_origins`, `a_declared_document_is_not_the_global`, `only_a_direct_call_on_the_global_is_an_origin` |
| A type assertion keeps a proof but makes none | `a_type_assertion_keeps_a_proof_but_makes_none` |
| An immutable alias, a null guard, and the design's own examples | `the_design_examples_read_as_their_comments_say` |
| An unknown value, `+=`, and an `mf2` tag at a known sink | `a_proven_sink_assigned_anything_but_a_literal_is_reported` |
| A chained assignment | `a_chained_assignment_shows_the_inner_literal_and_reports_the_outer_value` |
| Explicit authoring inside a known sink is read once, as explicit, and its revision ignores the receiver | `explicit_authoring_at_a_known_sink_is_read_once_as_explicit`, `an_explicit_revision_does_not_depend_on_the_receiver_around_it` |
| An origin without exactly one static string | `an_origin_without_one_static_string_proves_nothing` |
| A profile admitting no DOM global, and a context that does not register the usage profile | `a_profile_admitting_no_dom_global_recognizes_and_counts_nothing`, `a_context_has_to_register_the_usage_profile_this_producer_assigns_from` |
| What each assignment says about its receiver and value | `each_text_content_assignment_says_what_its_receiver_and_value_are`, `evidence_follows_the_receiver_and_the_walk` |

### DOM receiver aliases

| Requirement | Checked by |
| --- | --- |
| A `const` chain in one function and at module level | `a_const_chain_is_followed_in_one_function_and_at_module_level`, `only_a_const_made_from_an_admitted_origin_or_aliasing_one_is_followed` |
| The chain's length is bounded exactly | `an_alias_chain_is_bounded_exactly`, `an_alias_past_the_bound_is_not_followed` |
| `let`, `var`, and an alias the tracer does not follow are reported | `a_binding_the_tracer_does_not_follow_is_reported_at_each_literal` |
| Destructuring and object or array storage are outside the profile | `a_receiver_reached_only_through_data_is_outside_the_profile` |
| An argument and a returned value have no known origin | `a_receiver_reached_only_through_data_is_outside_the_profile`, `a_receiver_that_comes_back_from_a_call_has_no_known_origin` |
| A reference from another function is reported there | `a_receiver_used_in_another_function_is_reported_there` |
| A script's top level is not followed | `a_scripts_top_level_const_is_shared_and_not_followed`, `a_scripts_top_level_is_not_followed_but_its_functions_are` |

### Receiver escape

| Requirement | Checked by |
| --- | --- |
| An unanalyzed call, `new`, a spread and a tagged template invalidate every alias | `passing_a_receiver_anywhere_invalidates_every_alias_of_it` |
| The call is not an error itself, and `intent()` stays valid without restoring evidence | `the_call_itself_is_not_an_error_and_intent_stays_valid` |
| A method call on the receiver and a harmless read keep the evidence | `a_method_call_or_a_harmless_read_keeps_the_evidence` |
| Ordinary insertion calls and callbacks block the sinks after them | `inserting_a_receiver_or_writing_it_from_a_callback_blocks_the_sinks_after_it`; see the crate README's [What the empty analyzed-callee list blocks](../../README.md#what-the-empty-analyzed-callee-list-blocks) |

### Non-call receiver effects

| Requirement | Checked by |
| --- | --- |
| Redefining `textContent`, `delete`, a prototype change, a reflective member, object or array storage, and assignment elsewhere | `every_other_effect_on_a_receiver_invalidates_it`, `only_members_that_reach_the_prototype_or_redefine_properties_are_reflective` |
| `return`, `yield` and `export` | `yielding_or_exporting_a_receiver_exposes_it`, `an_exported_binding_is_recorded` |
| A closure capture counts from where the closure is made, a hoisted declaration from where its function starts | `a_capture_counts_from_where_the_closure_is_made`, `a_hoisted_declaration_captures_from_where_its_function_starts`, `a_function_written_before_the_declaration_captures_it_too`, `a_function_declaration_captures_from_the_start_of_its_owner` |
| Consecutive ordinary updates keep the evidence | `the_design_examples_read_as_their_comments_say`, `a_method_call_or_a_harmless_read_keeps_the_evidence` |
| An alias made after invalidation restores nothing | `an_alias_made_after_invalidation_does_not_restore_evidence` |
| A scope no walk reads proves nothing | `a_sink_in_a_scope_no_walk_reads_is_not_proven` |

### Receiver control flow

| Requirement | Checked by |
| --- | --- |
| Every branch, one branch, no `else`, and branches inside expressions | `branches_join_on_every_path`, `a_join_keeps_only_live_paths_and_merges_them` |
| `switch` fallthrough, `break`, and case tests | `a_switch_falls_through_and_a_break_stops_it` |
| Invalidation carried from an earlier iteration, and an element made anew each iteration | `a_loop_carries_what_an_earlier_iteration_did`, `a_new_element_starts_each_iteration_with_its_own_evidence` |
| Labels, `break` and `continue` | `labels_send_a_state_where_the_jump_goes`, `a_label_leads_to_a_loop_through_more_labels_only` |
| `try`, `catch` and `finally`, including jumps through a `finally` | `a_try_statement_passes_every_path_through_its_finally` |
| An early `return` or `throw`, and unreachable code | `a_path_that_leaves_the_function_does_not_reach_the_sink` |
| `with` and a sloppy direct `eval` | `a_dynamically_scoped_function_admits_no_origin`, `with_and_sloppy_eval_make_a_scope_and_those_inside_it_dynamic` |
| A sink holds only if its evidence held every time it was reached | `a_sink_holds_only_if_its_origin_held_every_time_it_was_reached` |
| Proof steps and tracked origins are bounded exactly, and an unfinished proof proves nothing | `proof_steps_are_bounded_exactly_and_an_unfinished_proof_proves_nothing`, `proof_steps_count_entered_nodes_and_bound_them_inclusively`, `origins_one_function_tracks_are_bounded_exactly`, `a_function_with_more_origins_than_it_may_track_proves_nothing` |
| Stopping during a proof yields no result | `stopping_at_any_probe_during_a_proof_yields_no_result`, `a_probe_asking_to_stop_stops_the_walk` |

### Exclusion

| Requirement | Checked by |
| --- | --- |
| Literal and dynamic values keep their reason and make no Intent or reference | `an_exclusion_keeps_its_reason_and_makes_no_intent`, `an_exclusion_keeps_its_decoded_reason` |
| A missing, empty or dynamic reason, and extra arguments, are reported | `a_reason_that_is_missing_empty_or_computed_is_reported` |
| Nested markers that contradict are reported once, and neither side is read | `localizing_and_excluding_one_value_is_a_contradiction`, `a_contradiction_is_reported_once_at_the_outer_call_and_neither_side_is_read` |
| A message inside an excluded expression is no contradiction | `a_message_inside_an_excluded_expression_is_not_a_contradiction` |
| An exclusion at a known sink | `explicit_authoring_at_a_known_sink_is_read_once_as_explicit` |
| Exclusions of one unit are bounded exactly | `exclusions_one_unit_makes_are_bounded_exactly` |
| The host value is unchanged at run time | **Runtime.** Nothing here rewrites or runs host code; what `noIntent` returns is the runtime package's (designs 024, 027 and 029). |

### Metadata

| Requirement | Checked by |
| --- | --- |
| One annotation means the same above each declaration form | `one_annotation_means_the_same_above_every_declaration_form` |
| Malformed JSON, invalid encoding, a non-object, duplicate and unknown members, and empty or non-string values are reported and withhold their declaration | `an_invalid_annotation_is_reported_and_withholds_its_declaration`, `an_annotation_is_one_object_of_nonempty_strings_under_three_names` |
| Omitted members | `every_member_is_optional` |
| A line comment and a misspelled block are reported; other comments are not annotations | `only_a_block_comment_starting_with_intlify_is_an_annotation`, `only_a_block_comment_starting_with_the_marker_is_an_annotation` |
| Exactly one declaration of the next statement; no search past it; no function, block or class-wide application; a misplaced annotation | `an_annotation_describes_exactly_one_declaration_of_the_next_statement` |
| A reported annotation leaves the declarations it does not own alone | `a_declaration_a_statement_does_not_own_stays_established` |
| Two annotations for one statement are both reported and not merged | `two_annotations_before_one_statement_are_both_reported_and_neither_applies` |
| A use cannot redefine its declaration's metadata | `a_use_of_a_shared_declaration_cannot_redefine_its_metadata` |
| An occurrence already reported gets no second record | `an_occurrence_already_reported_gets_no_second_record` |
| Annotation bytes are bounded exactly, in bytes | `an_annotation_is_bounded_exactly_by_its_bytes` |
| A profile declaring only by DOM still reads annotations | `a_profile_that_declares_only_by_dom_still_reads_annotations` |
| Stopping during placement yields no result | `a_probe_asking_to_stop_stops_the_placement` |

### Scope defaults

| Requirement | Checked by |
| --- | --- |
| The invocation's default class, a per-declaration override, and an unknown or absent class | `an_annotated_class_comes_before_the_invocation_default` |
| A use cannot change its declaration's class or description | `a_use_of_a_shared_declaration_cannot_redefine_its_metadata` |
| No shared description default | `one_annotation_means_the_same_above_every_declaration_form` |
| An imported declaration keeps its own context | **Phase 4.** A module reference is refused here (`only_a_const_bound_to_the_tag_itself_names_a_declaration`), so no consumer default can reach one yet. |

### Extraction

| Requirement | Checked by |
| --- | --- |
| Quotes, a newline escape and a written newline, MF2 backslashes carried by a host escape, `\xHH`, `\uHHHH`, `\u{...}`, a surrogate pair, a line continuation, template CRLF and CR, multi-byte text, a byte order mark, a hashbang, and no trim or dedent decode to their hand-derived text and map | `extraction_fixtures_decode_to_their_hand_derived_text_and_map`, over cases `quotes`, `newline`, `mf2-backslash`, `unicode`, `continuation`, `template-crlf`, `bom`, `hashbang`, `no-trim` and `typescript` |
| Each run of a decoded literal | `verbatim_text_is_one_positional_run`, `each_escape_is_a_run_of_its_own`, `numeric_escapes_answer_for_their_whole_spelling`, `an_escaped_ordinary_character_is_that_character`, `a_line_continuation_decodes_to_nothing_but_keeps_its_source`, `a_template_reads_every_line_ending_as_a_line_feed`, `template_delimiters_escape_to_themselves`, `each_template_element_is_decoded_on_its_own`, `an_empty_literal_still_says_where_its_text_came_from` |
| A lone surrogate, a pair spelled across two escape forms, an invalid tagged escape, and a legacy octal escape are refused at the escape | `extraction_fixtures_decode_to_their_hand_derived_text_and_map`, over cases `surrogates`, `tagged-invalid-escape` and `legacy-escape`; `a_surrogate_is_accepted_only_as_two_adjacent_four_digit_escapes`, `the_parser_does_not_pair_the_spellings_the_profile_refuses`, `legacy_escapes_are_refused_where_sloppy_code_admits_them`, `a_tagged_template_keeps_invalid_escapes_and_they_are_refused`, `the_first_refused_form_in_source_order_is_the_one_reported`, `each_refused_form_names_its_own_detail`, `an_escape_the_profile_refuses_is_reported_where_it_is_written` |
| The decoder agrees with the parser's cooked value, or the invocation stops | `a_disagreement_with_the_parser_stops_instead_of_choosing`, `settling_a_disagreement_stops_the_analysis_and_reports_nothing` |
| An `intent()` literal and an `mf2` tag with the same cooked content are the same message | `equal_text_at_separate_declarations_is_separate_facts_with_one_meaning`, `host_spelling_and_parameter_values_leave_a_revision_alone` |
| Input segments are bounded exactly | `input_segments_are_bounded_exactly`, `settling_a_literal_past_its_bound_reports_it_at_the_literal_and_yields_nothing` |
| Every decoded map is one the shared crate composes | `every_decoded_map_is_one_the_shared_crate_accepts`, `settling_a_decoded_literal_hands_it_over_and_reports_nothing` |
| Composition: many-to-one, zero-width delimiters, end of unit, multi-byte boundaries, CRLF two-to-one, and an omitted map against a broken one | `a_host_escape_keeps_its_own_span_and_splits_the_run_around_it` _(authoring)_, `a_verbatim_run_slices_and_the_generated_delimiters_become_insertion_points` _(authoring)_, `a_declaration_ending_at_the_unit_boundary_composes` _(authoring)_, `multibyte_text_composes_on_scalar_boundaries` _(authoring)_, `a_boundary_inside_a_scalar_is_refused_rather_than_sliced` _(authoring)_, `a_crlf_the_host_normalized_answers_for_both_of_its_bytes` _(authoring)_, `a_host_map_moves_the_extraction_map_into_source_and_omitting_one_does_not` _(authoring)_, `a_host_map_that_does_not_describe_the_text_fails_the_invocation` _(authoring)_ |

### Parser ownership

| Requirement | Checked by |
| --- | --- |
| Syntax and semantic codes stay the parser's through the host, and dependent work is skipped | `the_mf2_parser_keeps_its_own_codes_through_the_host` |
| A message error is located in host source, also for a blocked declaration | `a_message_error_is_located_where_it_is_written_past_what_the_host_decoded`, `a_message_record_says_where_in_source_it_is_even_when_its_declaration_is_blocked` _(authoring)_ |
| Host syntax errors fail the unit at the earliest range the parser names | `a_unit_the_host_rejects_fails_at_a_range_inside_it`, `the_earliest_checkable_label_of_any_diagnostic_is_reported`, `each_stage_that_rejects_a_unit_names_where`, `a_unit_the_host_rejects_fails_where_the_host_says` |
| An invariant failure is operational | `a_parse_that_stops_without_a_diagnostic_is_the_parsers_own_failure`, `a_span_the_unit_cannot_hold_is_the_parser_failing` |
| One parser meaning across authoring forms | `equal_text_at_separate_declarations_is_separate_facts_with_one_meaning` |

### Parameters and selection

| Requirement | Checked by |
| --- | --- |
| Names in source order and values by position, never evaluated | `a_use_site_supplies_its_parameters_by_position_in_source_order`, `a_plain_object_supplies_its_names_in_source_order_and_its_values_by_position` |
| A string key is its decoded text, and a shorthand `__proto__` is an ordinary property | `a_string_key_is_its_decoded_text`, `a_shorthand_proto_is_an_ordinary_property` |
| Missing, extra and duplicate names, at an inline and at a shared declaration | `a_mismatch_at_a_declarations_own_use_site_blocks_the_declaration`, `a_mismatch_at_a_shared_declarations_use_blocks_only_that_use` |
| A spread, computed and numeric keys, accessors, methods, `__proto__: value`, and an opaque object are refused | `a_parameter_object_whose_names_could_change_at_run_time_is_refused`, `every_property_whose_name_could_change_at_run_time_is_reported`, `an_argument_that_is_not_an_object_literal_is_opaque` |
| An unreadable object keeps the declaration without a use | `an_unreadable_parameter_object_keeps_the_declaration_without_a_use` |
| Parameters of one use site are bounded exactly | `parameters_one_use_site_supplies_are_bounded_exactly` |
| Conditional and logical selection is unsupported, and neither alternative is chosen | `a_conditional_selection_is_reported_and_neither_alternative_is_chosen` |
| An alias and an imported binding are not declaration references | `only_a_const_bound_to_the_tag_itself_names_a_declaration` |
| A dynamic or unbounded source, and the wrong arguments | `a_source_computed_at_run_time_is_dynamic`, `intent_takes_a_source_and_at_most_a_parameter_object` |
| Incompatible alternatives | **Later**, with conditional selection (016-010), which this profile reports as unsupported. |

### Locale handoff

| Requirement | Checked by |
| --- | --- |
| An explicit locale and the inherited default reaching one canonical locale give one revision | `an_explicit_locale_reaching_the_default_gives_the_same_revision` |
| An absent default, invalid and unsupported input, a changed binding | Language-neutral, in the [Phase 1 matrix](../../../intlify_authoring/fixtures/phase1/README.md#locale-handoff) |
| A library's source locale is preserved | **Phase 4**, with module references. |

### Identity equivalence

| Requirement | Checked by |
| --- | --- |
| Equal text at separate declarations stays separate facts | `equal_text_at_separate_declarations_is_separate_facts_with_one_meaning` |
| Every use of a shared declaration names one occurrence | `every_use_of_a_shared_declaration_names_the_same_occurrence` |
| Changed source evidence with an unchanged projection | `moving_source_changes_the_artifact_and_leaves_every_revision_alone` _(authoring)_ |
| The same declaration after an edit or a move, and significant wording or context edits | **Phase 3.** |

### Revision comparison

| Requirement | Checked by |
| --- | --- |
| Host quoting, escapes, wrappers and parameter value expressions leave a revision alone; characters, spaces and case are compared exactly, without normalization | `host_spelling_and_parameter_values_leave_a_revision_alone` |
| Host declarations reach the independent revision vectors | `host_declarations_reach_the_committed_independent_revision_vectors` |
| Explicit forms take no usage from the receiver around them | `an_explicit_revision_does_not_depend_on_the_receiver_around_it` |
| The language-neutral pairs | In the [Phase 1 matrix](../../../intlify_authoring/fixtures/phase1/README.md#revision) |

### Inventory completeness

| Requirement | Checked by |
| --- | --- |
| Caller-supplied owner, membership and snapshots | `units_are_admitted_in_unit_order_whatever_order_they_arrive_in`, `a_unit_is_named_once_in_the_scope_and_once_in_the_supply`, `a_unit_outside_the_declared_scope_is_refused`, `membership_is_sorted_once_and_matched_at_its_revision` |
| A complete scope against a partial one | `a_partial_scope_is_checked_for_its_part_and_claims_no_more`, `complete_and_partial_assemble_the_same_facts_under_different_claims` |
| A missing unit | `a_complete_scope_needs_the_bytes_of_every_member`, `a_complete_scope_has_to_have_every_member_read`, `only_a_complete_scope_needs_every_member_supplied` |
| A unit that could not be read, by syntax or by encoding | `a_unit_that_could_not_be_read_is_recorded_as_failed_and_claims_nothing`, `a_unit_that_is_not_text_is_admitted_for_analysis_to_report` |
| A unit deleted from a complete inventory | Assembly refuses a complete scope without one member's analysis (`a_complete_scope_has_to_have_every_member_read`). Admission cannot see a unit removed from a sealed artifact and resealed, because the inventory does not record membership; that question is open with designs 017 and 019. |
| A unit omitted from a partial view | `a_partial_scope_is_checked_for_its_part_and_claims_no_more` |
| Cancellation | `a_cancelled_unit_leaves_nothing_to_assemble`, `cancellation_at_any_probe_is_an_operational_failure`, `stopping_at_any_probe_yields_no_result` |
| An unreferenced live declaration | `an_unreferenced_declaration_is_still_a_live_declaration` |
| No complete claim from a partial or blocked result | `a_partial_checked_inventory_is_never_complete_input`, `a_blocked_result_is_never_complete_input`, `a_complete_scope_with_a_failed_unit_is_a_record_but_not_checked_input` _(authoring)_ |
| An empty complete scope | `an_empty_complete_scope_is_admitted`, `an_empty_complete_scope_is_complete_checked_input` |
| No accidental retirement | **Phase 3.** Nothing here retires; an inventory records what one analysis found. |

### Test-only Profile inputs

| Requirement | Checked by |
| --- | --- |
| A context pinning another profile, and a production context, are refused before anything is read | `a_context_pinning_another_profile_is_refused`, `a_production_context_is_refused_before_anything_is_read` |
| The context registers the usage profile this Producer assigns from | `a_context_has_to_register_the_usage_profile_this_producer_assigns_from` |
| Missing information stays missing | `an_annotated_class_comes_before_the_invocation_default`, `absent_inputs_stay_absent_rather_than_acquiring_a_hidden_default` _(authoring)_ |
| The profile, usage and grammar pins are exact | `the_pin_is_the_exact_registered_pair`, `dom_globals_are_a_set_and_the_usage_pin_is_fixed`, `a_grammar_is_found_only_by_its_exact_identity_and_revision` |
| An ordinary build cannot construct the test context | The test context is enabled only by this crate's development dependencies and its `benchmark` feature, so the `cargo tree` command above lists no `test-context` feature. |

### Handoff

| Requirement | Checked by |
| --- | --- |
| Finite references make no claim of final reachability | `a_reference_records_where_it_is_written_not_whether_it_runs` |
| A blocked result, or a partial checked one, is never complete input | `a_blocked_result_is_never_complete_input`, `a_partial_scope_is_checked_for_its_part_and_claims_no_more` |
| The inventory hands on occurrences and no identity | `the_inventory_hands_on_occurrences_and_no_identity` |
| A later phase cites the artifact by its reference | `the_sealed_artifact_is_admitted_again_against_its_own_bytes` |
| A stable ID distinct from a target handle, and a source artifact without Provider work | **Phase 3** for the ID, **Phase 4** for the consumer artifacts. |

### Performance safety

| Requirement | Checked by |
| --- | --- |
| Units, unit bytes and total bytes | `unit_counts_are_bounded_exactly_for_the_scope_and_the_supply`, `unit_bytes_are_bounded_exactly_and_counted_as_bytes`, `total_bytes_are_bounded_exactly`, `each_bound_admits_exactly_its_value` |
| Syntax tree nodes, before the semantic checks | `syntax_tree_nodes_are_bounded_exactly`, `the_node_limit_applies_before_the_semantic_checks` |
| Declarations, references and records of one unit | `declarations_one_unit_holds_are_the_shared_bound_exactly`, `references_one_unit_makes_are_bounded_exactly`, `diagnostics_are_the_shared_bound_for_host_and_shared_records_together`, `records_are_retained_up_to_the_budget_exactly`, `what_one_unit_hands_over_is_bounded_exactly` |
| A bound on one literal, use site or message blocks only what it bounds | `a_bound_on_one_literal_blocks_only_its_declaration`, `a_shared_bound_on_one_message_blocks_only_its_declaration`, `a_bound_on_one_literal_or_use_site_is_scoped_to_it` |
| Bounds no invocation could satisfy are refused | `bounds_that_no_invocation_could_satisfy_are_rejected` |
| The other bounds | In their families: alias chain, proof steps and tracked origins, annotation bytes, parameters, exclusions, input segments, and the whole invocation |
| Optional instrumentation stays out of ordinary builds | The `benchmark` feature is off by default, so the `cargo tree` command above lists no measurement crate. |
| A bounded registry lookup | **Phase 3.** |

### Inventory

| Requirement | Checked by |
| --- | --- |
| The representative application is one complete checked inventory | `the_representative_application_is_one_complete_checked_inventory` |
| Canonical order and duplicate occurrences | `facts_come_in_canonical_order_whatever_order_they_were_found_in`, `each_structural_rule_is_refused_by_its_own_name` _(authoring)_, `a_decoded_record_is_held_to_the_rules_the_builder_would_have_applied` _(authoring)_ |
| Roles and arrays, owners, sources outside `units`, unresolved references, duplicate parameters, and facts from a failed unit | `each_structural_rule_is_refused_by_its_own_name` _(authoring)_, `a_checked_unit_cannot_hold_a_reference_that_did_not_resolve` _(authoring)_, `a_reference_target_has_to_be_its_declaration_exactly` _(authoring)_ |
| Whether a record is complete checked input is answered apart from whether it is well formed | `an_empty_complete_scope_is_complete_checked_input`, `a_complete_scope_with_a_failed_unit_is_a_record_but_not_checked_input` _(authoring)_ |
| Analyses are checked against the scope, owner, context and profile before any fact is read | `analyses_are_checked_against_the_scope_before_any_fact_is_read`, `a_unit_of_another_owner_is_refused`, `units_read_under_other_inputs_are_not_put_into_one_inventory`, `an_invocation_this_producer_cannot_run_is_refused_first` |
| Declarations and diagnostics of the whole invocation are bounded exactly | `the_whole_invocation_is_bounded_exactly` |
| Diagnostics come in reporting order across units | `diagnostics_come_in_reporting_order_across_units` |

### Artifact and admission

| Requirement | Checked by |
| --- | --- |
| The sealed artifact is admitted again from its own bytes | `the_sealed_artifact_is_admitted_again_against_its_own_bytes`, `a_sealed_inventory_is_admitted_from_its_own_bytes` _(authoring)_ |
| Integrity | `content_changed_after_sealing_is_refused_before_anything_reads_it` _(authoring)_, `one_reference_naming_two_contents_is_a_conflict_not_a_duplicate` _(authoring)_ |
| Independent vectors | `every_committed_vector_is_admitted_from_its_committed_bytes` _(authoring)_, and `vp run vectors:authoring:check` |
| A forged range, digest or role | `a_recorded_range_that_splits_a_character_of_the_real_source_is_refused` _(authoring)_, `supplied_bytes_are_checked_against_the_unit_they_claim` _(authoring)_, `a_decoded_record_is_held_to_the_rules_the_builder_would_have_applied` _(authoring)_ |
| A retained MF2 source that does not rebuild its projection | `a_retained_mf2_source_that_does_not_reproduce_its_projection_is_refused` _(authoring)_ |
| A context or basis the inventory was not resolved against | `an_inventory_is_admitted_only_under_the_context_it_was_resolved_against` _(authoring)_, `a_locale_or_class_the_context_would_not_have_produced_is_refused` _(authoring)_ |
| Production admission refuses a test inventory | `the_ordinary_entry_never_admits_a_test_inventory` _(authoring)_ |
| The schema and the reader accept the same set | `the_committed_schema_refuses_what_the_reader_refuses` _(authoring)_, `the_committed_inventory_schema_matches_the_current_artifact` _(authoring)_, and `vp run schema:authoring:check` |
| Source evidence changes the integrity and nothing a message means | `moving_source_changes_the_artifact_and_leaves_every_revision_alone` _(authoring)_ |
| Bytes outside the shared encoding or the closed shape, and a tuple this reader does not implement | `bytes_outside_the_shared_encoding_or_the_closed_shape_are_refused_by_name` _(authoring)_, `a_tuple_this_reader_does_not_implement_is_unsupported_not_malformed` _(authoring)_ |
| A unit whose bytes were not supplied is named as unverified | `a_unit_without_supplied_bytes_is_admitted_but_named_as_unverified` _(authoring)_ |
| Admission is bounded by the caller | `admission_is_bounded_by_the_caller_limits` _(authoring)_ |

### Determinism and storage

| Requirement | Checked by |
| --- | --- |
| Fresh and reused workspaces agree, after a success, a failure and a cancellation | `a_reused_workspace_gives_what_a_fresh_one_gives`, `a_reused_workspace_gives_what_a_fresh_one_gives_for_recognized_units`, `a_reused_workspace_agrees_with_a_fresh_one_after_success_failure_and_cancellation` |
| Lending the scratch drops what the last unit left and keeps its capacity | `lending_the_scratch_drops_what_the_last_unit_left_and_keeps_its_capacity`, `a_new_workspace_owns_no_arena_memory_yet` |
| Unit order and threads give the same bytes | `the_artifact_does_not_depend_on_order_or_scheduling`, `units_are_admitted_in_unit_order_whatever_order_they_arrive_in`, `the_failure_reported_does_not_depend_on_supply_order` |
| No hash order reaches what is returned | `facts_come_in_canonical_order_whatever_order_they_were_found_in`, `records_come_out_in_reporting_order_whatever_order_they_arrived_in`, `the_artifact_does_not_depend_on_order_or_scheduling` |
| The grammar's revision is pinned by a corpus | `every_grammar_accepts_and_rejects_what_its_revision_was_pinned_with`, `every_grammar_is_pinned_by_both_an_acceptance_and_a_rejection` |
| A script carries no module syntax, and a byte order mark is part of a unit | `a_script_carrying_module_syntax_is_rejected_in_either_language`, `a_byte_order_mark_and_a_hashbang_are_read_as_part_of_the_unit` |

### Measurement

The owner run is shared, so the rejections below are pinned twice: on this owner and on the minimal test owner in `intlify_measurement`, with the same reasons.

| Requirement | Checked by |
| --- | --- |
| Every operation is covered, and no fixture name repeats | `every_operation_is_covered_and_no_fixture_name_repeats` |
| Every fixture takes the path it declares, and one that does not is never planned | `every_fixture_takes_the_path_it_declares`, `a_fixture_that_takes_another_path_than_it_declares_is_never_planned` _(measurement)_ |
| A first-over bound is one below its exact partner | `each_first_over_bound_is_one_below_its_exact_partner` |
| Classifying reaches the conclusion the whole analysis does, and stops before Phase 1 | `the_whole_analysis_reaches_the_same_conclusion_as_classifying`, `classifying_stops_before_any_message_is_analyzed` |
| Two captures agree on observation and counted work | `every_fixture_captures_and_repeats_the_same_observation` |
| Each count of the work vector says how it is known | `the_work_vector_says_how_each_count_is_known` |
| An observation tells the results it compares apart | `a_classification_is_observed_through_what_each_use_and_record_says`, `an_assembly_is_observed_through_the_artifact_it_produced`, `an_inventory_holding_a_blocked_unit_reports_why_rather_than_completing`, `complete_and_partial_assemble_the_same_facts_under_different_claims` |
| The boundary says what is inside and outside the interval | `a_boundary_names_its_operation_and_keeps_the_two_marker_sets_disjoint`, `source_discovery_excludes_the_phase_1_semantics_it_stops_before`, `assembly_includes_the_bytes_it_hands_over_and_lends_no_scratch` |
| A case projection is distinct, stable, carries no run dimension, and includes its bound | `every_fixture_projects_to_a_distinct_case_identity`, `a_case_identity_does_not_change_between_two_preparations`, `a_projection_carries_no_run_clock_or_sample_dimension`, `a_bound_is_part_of_the_case_it_measures`, `the_projection_names_the_result_schema_revision_the_run_records` |
| A run re-admits from its own bytes | `a_run_produces_records_that_re_admit_from_their_own_bytes` |
| A missing record | `a_withheld_record_is_missing`, `a_withheld_record_is_not_a_successful_run` _(measurement)_ |
| A saved record changed and its digest recomputed | `a_rehashed_change_to_a_saved_record_is_not_admitted`, `a_resealed_owner_document_is_not_what_was_captured` _(measurement)_ |
| A duplicate, foreign or absent owner document | `a_duplicate_a_foreign_or_no_owner_document_resolves_no_evidence`, `two_owner_documents_for_one_run_are_an_ambiguous_binding` _(measurement)_, `a_document_from_another_run_does_not_bind_to_this_one` _(measurement)_ |
| A repetition sum past the quantity domain | `a_repetition_sum_past_the_quantity_domain_fails_a_case`, `a_repetition_sum_past_the_quantity_domain_fails_the_case` _(measurement)_ |
| A different result is a mismatch, not a sample | `a_different_result_is_a_semantic_mismatch_rather_than_a_sample` _(measurement)_, `a_result_that_drifts_from_its_expectation_invalidates_the_run` _(measurement)_ |
| Warmups are counted, never sampled | `warmups_are_counted_and_every_sample_aggregates_its_repetitions` _(measurement)_, `a_panic_inside_a_warmup_fails_the_case_without_a_sample` _(measurement)_ |
| A failed measurement leaves the run incomplete | `a_measurement_that_fails_leaves_the_run_incomplete_rather_than_invalid` _(measurement)_ |
| The provider revision recorded is the locked one | `the_provider_revision_is_the_locked_dependency` _(measurement)_ |
| The two earlier owners still produce and re-admit their records | `vp run bench:authoring:smoke` and `vp run bench:config:smoke` |

## Families a later phase owns

016 lists these families too. Each needs a capability Phase 2 does not have, so none is approximated here.

| Family | Owner |
| --- | --- |
| Identity ownership, continuity evidence, reconciliation, registry update history, automatic continuation updates, automatic allocation and retirement, registry recovery | Phase 3, which allocates and reconciles persistent identity |
| Module references | A Phase 4 extension. A module reference is reported as unsupported (`only_a_const_bound_to_the_tag_itself_names_a_declaration`). |

## Deliberately not covered in Phase 2

These are named so that their absence is a decision rather than an oversight.

- **Identity** — no `MessageIntentId` is allocated, read or reconciled, and no fixture ID stands in for one. The `message-intent`, `message-reference`, `intent-registry` and `intent-registry-update` codecs are Phase 3's.
- **Conditional selection** — 016-010 is accepted but not implemented. It is reported as unsupported, and neither alternative is chosen.
- **Production inputs** — the only context admitted is the test context. A production `LocalizationProjectProfile` and production locale canonicalization are design 015's Phase 2.
- **Host behaviour** — nothing here rewrites or runs host code. Evaluation order is kept as reference evidence; whether generated code preserves it is proven against the real consumer in designs 024 and 028.
- **Profile extensions** — an analyzed-callee list, other sinks such as `aria-label`, JSX and TSX, and console output each need an explicit profile revision and its own fixtures.
