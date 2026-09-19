<!-- @license MIT -->

# Phase 1 fixture matrix

Design 016's Phase 1 names the families of behaviour this crate has to fix in tests. This file maps each family to where it is actually checked, so a reader can go from a requirement to the code that pins it without searching, and can see which parts are deliberately not covered yet.

A row that says "deferred" names the phase that owns it. Nothing here is covered by approximation: a family is either checked or named as absent.

## Running them

```sh
# every logical fixture, unit test and integration test in this crate
cargo test -p intlify_authoring --all-targets --all-features

# the reachability gate: the test context must not exist in an ordinary build
cargo test -p intlify_authoring --doc

# the committed projection schema and the independent revision vectors
vp run schema:authoring:check
vp run vectors:authoring:check

# the observational measurement, produced and re-admitted from disk
vp run bench:authoring:smoke
```

`cargo test --all-targets` does not run doctests, which is why the reachability gate above is a separate command.

## Data files

| File | Contents | Read by |
| --- | --- | --- |
| `message-analysis.json` | 22 cases: 13 extraction, 4 parser ownership, 5 parameters | `tests/message_analysis.rs` |
| `revision-vectors.json` | 11 vectors, each with its digest preimage and digest | `tests/revision_vectors.rs`, `tools/shared-json-vectors` |

The revision vectors carry the preimage beside the digest, and an independent Node implementation that shares no code re-frames and re-hashes them. A disagreement therefore says whether the framing or the hash is wrong.

The extraction rows below name the tests that assert through a shared `check_segments` helper, which is where the non-overlap, ordering and coverage conditions are actually written.

## The matrix

### Literal encoding

| Requirement | Checked by |
| --- | --- |
| Braces, backslash and pipe | `literal-braces-are-not-interpolation`, `literal-backslash`, `literal-pipe-stays-unescaped`; `only_backslash_and_braces_are_escaped` |
| Leading and trailing whitespace | `literal-leading-whitespace-is-preserved`, `literal-trailing-whitespace-is-preserved` |
| Text beginning with `.` | `literal-leading-period-is-not-a-declaration`; `the_quoted_pattern_covers_strings_a_simple_message_would_special_case` |
| Empty string | `literal-empty-text`; same unit test |
| Newline and CRLF | `literal-newline-is-preserved-without-trimming`, `literal-crlf-is-preserved` |
| Multi-byte text | `literal-multibyte-maps-on-scalar-boundaries`; `multibyte_scalars_keep_byte_ranges_on_scalar_boundaries` |
| A scalar outside `text-char` | `unrepresentable-scalar-is-an-operational-failure`; `the_one_unrepresentable_scalar_is_reported_with_its_offset` |
| Round-trip invariant | `displayed_text_round_trips_byte_for_byte`, `the_inverse_rejects_inputs_this_encoder_never_produces` |

### Extraction map

| Requirement | Checked by |
| --- | --- |
| One source scalar to several emitted bytes | `escape_segments_map_several_emitted_bytes_to_one_source_scalar` |
| Zero-width segments for the generated `{{` and `}}` | `displayed_text_round_trips_byte_for_byte` |
| Non-overlapping, ordered, complete coverage | `displayed_text_round_trips_byte_for_byte`, `multibyte_scalars_keep_byte_ranges_on_scalar_boundaries`, `authored_mf2_maps_to_itself_so_the_segments_cover_the_emitted_message` |
| A reported range resolves back through the map | `a_reported_range_addresses_real_bytes_and_resolves_through_the_segments` |
| Composition with a host's own map | `a_verbatim_run_slices_and_the_generated_delimiters_become_insertion_points`, `a_host_escape_keeps_its_own_span_and_splits_the_run_around_it`, `an_mf2_escape_inside_a_verbatim_run_resolves_to_the_character_it_escaped`, `multibyte_text_composes_on_scalar_boundaries`, `a_crlf_the_host_normalized_answers_for_both_of_its_bytes`, `text_the_host_dropped_is_not_part_of_the_content_it_surrounds`, `empty_text_still_has_a_position_in_source`, `a_declaration_ending_at_the_unit_boundary_composes`, `authored_mf2_composes_through_its_identity_segment` |
| A map that does not describe the text | `a_map_that_does_not_describe_the_text_is_refused_for_the_reason_it_fails`, `a_host_map_that_does_not_describe_the_text_fails_the_invocation` |
| Omitting a map is not supplying a broken one | `a_host_map_moves_the_extraction_map_into_source_and_omitting_one_does_not` |
| The composed map is bounded | `the_composed_map_is_bounded_by_the_invocation` |
| Supplying the map | **Deferred to Phase 2.** A host reads its own escapes; this crate composes whatever map the host establishes. |

### Parser ownership

| Requirement | Checked by |
| --- | --- |
| Syntax diagnostics keep their own code | `syntax-failure-suppresses-dependent-work`; `syntax_failure_suppresses_dependent_semantic_work_but_keeps_its_own_code` |
| Semantic diagnostics keep their own code | `semantic-failure-keeps-its-parser-owned-code`, `missing-selector-annotation-is-a-semantic-failure`; `parser_owned_codes_are_preserved_rather_than_relabelled` |
| The complete code catalogues | `syntax_diagnostic_catalogue_is_pinned_for_this_revision` (17 codes), `semantic_diagnostic_catalogue_is_pinned_for_this_revision` (7 codes) |
| An invariant failure is operational, not a diagnostic | `a_generated_literal_that_the_parser_rejects_is_an_operational_failure` |
| Displayed text and authored MF2 get the same parser meaning | `equal-cooked-content-analyses-identically`; `identical_cooked_content_analyses_identically_across_authoring_forms` |
| The `mf2Specification` registry pin | `the_pin_is_the_exact_registered_pair` |

### External parameters

| Requirement | Checked by |
| --- | --- |
| `.input` declares a caller parameter | `input-declaration-requires-its-name` |
| `.local` does not | `local-declarations-are-not-caller-parameters`; `mf2_local_declarations_are_not_caller_parameters` |
| A local initialised from outside still requires that value | `a-local-initialized-from-outside-still-requires-that-value` |
| Names are sorted by unsigned UTF-8 bytes | `parameter-names-are-sorted-by-unsigned-utf8-bytes` |
| Repeated uses collapse to one requirement | `repeated-uses-collapse-to-one-requirement` |

### Projection

| Requirement | Checked by |
| --- | --- |
| Names are cooked the way the parser cooks them | `cooked_names_agree_with_the_parser_for_every_declaration` |
| Literal content is never normalised | `literal_content_is_never_normalized_the_way_a_name_is`; vectors `decomposed-literal-stays-distinct`, `precomposed-literal` |
| Quoted and unquoted forms are equivalent | vector `equivalent-quoted-pattern`; `host_spelling_and_wrappers_do_not_change_a_revision` |
| Adjacent text merges, empty runs are omitted | `adjacent_text_runs_merge_and_empty_runs_are_omitted` |
| Order is preserved without alpha-renaming | `local_declarations_participate_without_alpha_renaming`; `message_structure_changes_the_revision` |
| Namespaced identifiers keep their separator | `a_namespaced_identifier_keeps_its_separator` |
| The committed schema matches the model | `the_committed_schema_matches_the_current_projection`, `the_schema_admits_a_real_projection_and_rejects_a_damaged_one` |

### Revision

| Requirement | Checked by |
| --- | --- |
| Equal and changed spellings from 016's table | `host_spelling_and_wrappers_do_not_change_a_revision`, `literal_content_is_compared_exactly`, `message_structure_changes_the_revision`, `parameter_and_function_requirements_change_the_revision`, `markup_and_attributes_participate` |
| Independent vectors agree | `tests/revision_vectors.rs` against `revision-vectors.json`; `vp run vectors:authoring:check` re-hashes them in Node |
| Domain separation | `the_pin_is_the_exact_registered_pair`; the digest domain is registered in `projection/revision.rs` |
| The same locale reached two ways has one revision | `the_same_canonical_locale_reached_two_ways_has_one_revision` |

### Locale handoff

| Requirement | Checked by |
| --- | --- |
| Explicit wins over the default | `an_explicit_locale_is_canonicalized_and_wins_over_the_default` |
| The default applies only in its absence | `the_context_default_applies_only_when_no_explicit_locale_exists` |
| No basis at all blocks | `a_declaration_without_any_locale_basis_is_blocked` |
| Invalid, unsupported and unavailable stay apart | `an_invalid_locale_blocks_but_an_unavailable_provider_is_operational`; `the_provider_separates_invalid_unsupported_and_unavailable`, `locale_failures_stay_separable` |
| The basis is evidence, not meaning | `the_same_canonical_locale_reached_two_ways_has_one_revision` |

### Scope defaults and surface class

| Requirement | Checked by |
| --- | --- |
| Default class, per-declaration override, absence, unknown class | `surface_class_falls_back_to_the_default_and_must_be_a_vocabulary_member` |
| Membership is exact | `a_vocabulary_is_exact_and_rejects_duplicates_and_empty_members` |
| No shared description default | `a_description_participates_and_absence_stays_absence` |

### Metadata values

| Requirement | Checked by |
| --- | --- |
| An empty value is not a value | `a_source_locale_and_usage_value_cannot_be_empty` |
| A value past its bound blocks rather than being truncated | `a_metadata_value_past_its_bound_blocks_rather_than_being_truncated` |
| Usage is admitted only under a registered profile | `usage_is_admitted_only_under_a_registered_profile` |

### Parameters at the use site

| Requirement | Checked by |
| --- | --- |
| Missing, extra and duplicate are separate diagnostics | `parameter_mismatches_are_reported_and_a_match_is_accepted` |
| The host's evaluation order is retained | same test; the expression position travels with each binding |
| An expression is never evaluated or serialised | `ParameterBinding` keeps only an `Occurrence`; `retained_evidence_is_readable_by_a_caller` |
| A bound is reported by its exact kind | `parameter_bounds_report_the_exact_exhausted_limit` |

### Facts and outcome

| Requirement | Checked by |
| --- | --- |
| Canonical occurrence order | `facts_are_ordered_canonically_and_duplicate_occurrences_are_rejected`; `canonical_order_compares_offsets_numerically_not_as_decimal_text` |
| Duplicate occurrences are rejected | same test |
| Checked, blocked and operational stay apart | `an_invalid_locale_blocks_but_an_unavailable_provider_is_operational`, `only_errors_block_a_checked_result` |
| A blocked result keeps independently established facts | `a_blocked_declaration_does_not_suppress_facts_about_the_others` |
| Source outside the invocation's owner is refused | `source_owned_by_another_owner_is_outside_this_invocation` |
| The diagnostics budget bounds what is collected | `the_diagnostics_budget_bounds_what_is_collected_not_only_what_is_returned`, `the_sink_never_grows_past_its_budget_however_many_arrive` |

### Test-only inputs

| Requirement | Checked by |
| --- | --- |
| Missing required information | `absent_inputs_stay_absent_rather_than_acquiring_a_hidden_default` |
| No hidden defaults | same test |
| Relabelling a test context is refused | `relabelling_a_test_context_as_production_does_not_admit_it` |
| Distinct fixtures carry distinct basis evidence | `distinct_fixture_inputs_produce_distinct_basis_evidence` |
| Unreachable from an ordinary build | the `compile_fail` example in `src/lib.rs`, run by `cargo test --doc` |

### Determinism and storage

| Requirement | Checked by |
| --- | --- |
| A reused workspace agrees with fresh ones | `a_reused_workspace_agrees_with_fresh_ones_across_the_whole_fixture`, `fresh_and_reused_workspaces_agree_after_success_and_failure` |
| Reuse after success and after failure | same tests |
| Clearing leaves no semantic state and keeps capacity | `clearing_retains_capacity_and_leaves_no_semantic_state` |
| Returned values borrow nothing from the scratch | `retained_evidence_is_readable_by_a_caller`; the encoder's contract is stated on `literal::encode` |
| Bounds that no invocation could satisfy are rejected | `bounds_that_no_invocation_could_satisfy_are_rejected` |
| Cancellation yields no partial scope | `a_cancelled_invocation_returns_no_facts_at_all` |

### Measurement

| Requirement | Checked by |
| --- | --- |
| Every fixture prepares and takes its declared path | `every_fixture_prepares_from_the_fixed_inputs_alone`, `every_fixture_takes_the_path_it_declares` |
| Two captures agree on observation and counted work | `every_fixture_captures_and_repeats_the_same_observation` |
| A missing record | `a_withheld_record_is_not_a_successful_run`, `no_owner_document_at_all_is_not_a_complete_run`; the smoke repeats this on the files it wrote |
| An altered record with its digest recomputed | `a_rehashed_change_to_a_saved_record_is_not_admitted` |
| A duplicate owner document | `two_owner_documents_for_one_run_are_an_ambiguous_binding` |
| A document from another run | `a_document_from_another_run_does_not_bind_to_this_one` |
| A repetition sum past the quantity domain | `a_repetition_sum_past_the_quantity_domain_fails_the_case` |
| Warmup is counted, never sampled | `every_fixture_captures_and_repeats_the_same_observation`; the boundary declares it excluded in `the_expectation_and_the_warmup_are_outside_the_interval` |
| The case projection carries no run dimension | `a_projection_carries_no_run_clock_or_sample_dimension`, `a_case_identity_does_not_change_between_two_preparations` |
| Every unobserved environment field states its reason | `every_unobserved_field_carries_its_reason_and_names_this_record` |
| The existing 015 path is unchanged | `vp run bench:config:smoke`: 127 planned, 127 measured, complete |

## Deliberately not covered in Phase 1

These are named so that their absence is a decision rather than an oversight.

- **Host language analysis** — source discovery, annotation syntax, intrinsic bindings, and reading a host's own escapes belong to Phase 2. This crate takes already decoded text and occurrence evidence, and composes the map a host establishes rather than establishing one.
- **Identity** — allocating a `MessageIntentId`, reading or publishing a registry, and reconciling declaration history are Phase 3. A checked result here is authoring evidence, not an identity-resolved result.
- **Artifact codecs** — `message-intent`, `message-reference`, `intent-registry` and their schemas are Phase 3.
- **Production canonicalisation** — the provider behind `AuthoringContext` is a finite declared rule table. The production one is 015's Phase 2, and the trait duplicated here is unified at that integration.
- **Conditional selection and the production profile** — no production `ContextKind` is admitted, and a context claiming one is refused.
- **Kernel and toolchain observation** — the measurement harness reports these environment fields as not collected. Acquiring them belongs in the shared acquisition module so both owners report the same way.
