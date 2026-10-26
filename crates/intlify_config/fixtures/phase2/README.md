<!-- @license MIT -->

# Phase 2 fixtures

Design 015's Phase 2, the Locale Canonicalization Boundary, defines the Locale Canonicalization Specification, the provider boundary, the admission of a canonicalization data artifact, and a data-free integration into this crate, and then evaluates the ICU4X reference adapter against conformance fixtures. Plan 022 implements it. This file maps the phase's coverage to the tests that check it, and names what it leaves to later phases.

Phase 2 spans two crates. The binding, the classification of admission failures, the provider behind the non-default `icu4x` feature, and the canonicalization measurements behind `benchmark` are in this one. The artifact reader, the conformance layer, the generator, and the corpus gate are in [`intlify_locale_icu4x`](../../../intlify_locale_icu4x/README.md); its tests are marked _(adapter)_.

Each name in a "Checked by" column is a test function. `tests/fixture_matrix.rs` reads this file and fails when a named test is not a test in the crate its row says, so a renamed or deleted test cannot leave a row pointing at nothing. A row naming a later phase states an absence that is a decision, not an oversight.

## Running them

```sh
# every unit and integration test, with the ICU4X provider, the measurements,
# and the adapter's generators and corpus gate
cargo test -p intlify_config --all-targets --all-features
cargo test -p intlify_locale_icu4x --all-targets --all-features

# the committed schemas, the vectors an independent reader checks, and the
# corpus gate with its independent check
vp run schema:locale:check
vp run vectors:locale:check
vp run corpus:locale:check

# the observational measurement, produced and re-admitted from disk
vp run bench:config:smoke

# what an ordinary build is made of: no ICU4X in this crate, and no compiled
# locale data in the adapter
cargo tree -p intlify_config -e normal -f '{p} [{f}]'
cargo tree -p intlify_locale_icu4x -e normal -f '{p} [{f}]'
```

## Data files

Phase 2's data is the adapter's. This crate's tests and measurements read the adapter's committed artifact and handwritten cases from there; its own finite test provider is compiled only for tests, measurements, and the contributor example.

| File | Contents | Read by |
| --- | --- | --- |
| [`data/`](../../../intlify_locale_icu4x/data) | The complete artifact of revision `"0"`: its manifest, the conformance data generated from CLDR 48.2.0, and the four ICU4X 2.2.0 marker families in one blob | The adapter's reader, layer, and corpus tests; this crate's provider tests and measurements; `tools/shared-json-vectors` |
| [`cldr/`](../../../intlify_locale_icu4x/cldr) | CLDR 48.2.0's BCP 47 and validity records and its locale canonicalization corpus, vendored unchanged from tag `release-48-2` | The generator, the corpus gate, `tools/shared-json-vectors` |
| [`fixtures/dataset/vectors.json`](../../../intlify_locale_icu4x/fixtures/dataset/vectors.json) | A small dataset's payloads, its preimage, and its semantic digest | The adapter's dataset tests, `tools/shared-json-vectors` |
| [`fixtures/artifact/`](../../../intlify_locale_icu4x/fixtures/artifact) | That dataset as artifacts: one blob in each layout and three manifests | The adapter's reader and schema tests, this crate's provider tests, `tools/shared-json-vectors` |
| [`fixtures/layer/cases.json`](../../../intlify_locale_icu4x/fixtures/layer/cases.json) | 95 handwritten cases in 17 groups, each group quoting the design 015 rule it checks | The adapter's layer and corpus tests, this crate's provider tests |
| [`evidence/gap-registry.json`](../../../intlify_locale_icu4x/evidence/gap-registry.json) | The corpus gate's registry: every corpus case with both answers and its disposition | The adapter's corpus tests, `tools/shared-json-vectors` |

## Completion conditions

Design 015 completes Phase 2 when the specification, the provider boundary, the data-artifact admission, and the data-free integration exist and the ICU4X reference adapter passes the conformance fixtures, with canonicalization measured as design 026 adopts it. Plan 022 checks that through four conditions.

| Condition | Checked by |
| --- | --- |
| Canonicalization depends on no host locale API and no ambient CLDR version | `an_ordinary_build_links_neither_icu4x_nor_its_data`, `only_the_generators_link_icu4x_compiled_data` _(adapter)_, `the_engine_is_the_icu4x_release_cargo_locked` _(adapter)_, `payloads_are_acquired_at_the_addresses_the_manifest_names_once_it_is_read`, `the_committed_artifact_is_the_dataset_revision_0_admits`, `the_corpus_is_read_at_the_release_the_artifact_was_generated_from` _(adapter)_; CI runs the same cases on Linux, macOS, and Windows |
| Provider identity, specification revision, and dataset identity and digest are reproducible | `the_binding_reports_the_manifest_and_icu4x`, `revision_0_is_the_one_supported_specification_and_admits_cldr_48_2_0`, `the_committed_artifact_is_written_byte_for_byte` _(adapter)_, `the_digests_are_the_vectors_an_independent_implementation_checked` _(adapter)_, and `vp run vectors:locale:check` |
| Missing, mismatched, and unsupported canonicalization input is a typed failure | `provider_data_is_required_and_every_semantic_pin_is_compared`, `binding_failures_are_classified_where_015_reports_them`, `the_asserted_specification_decides_which_dataset_a_provider_must_carry`, `a_reader_failure_is_reported_where_015_reports_it`, `an_admitted_artifact_of_another_dataset_is_not_revision_0s`, `a_rejection_is_an_invalid_identifier_and_an_adapter_failure_leaves_it_unavailable`, `each_failure_has_its_015_reason` _(adapter)_ |
| Measured canonicalization uses no ambient data, keeps small, scaled, and invalid inputs, the binding, and the engine state in the common projection, and gives the same observations from a fresh and a reused engine | `the_committed_artifact_binds_the_dataset_revision_0_admits`, `icu4x_cases_follow_the_original_catalog_with_both_engines_for_each_input`, `icu4x_descriptors_record_the_engine_state_and_the_admitted_binding`, `fresh_and_reused_engines_agree_on_every_semantic_and_work_observation`, `actual_owner_run_projects_all_seven_pairs_and_raw_samples_through_report_admission`, and `vp run bench:config:smoke` |

## The matrix

The rows come from design 015's [Coverage inventory](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#coverage-inventory), grouped by topic, from what makes an identifier valid to the CLDR corpus. The canonicalization data envelope comes from the inventory's item on pre-invocation boundaries, and the measurement row from design 026's Phase 2 adoption.

### The identifier domain

| Requirement | Checked by |
| --- | --- |
| Valid Unicode BCP 47 locale identifiers with a language, script, region, and Unicode extensions are admitted | `the_handwritten_015_cases_hold` _(adapter)_, `the_handwritten_015_cases_hold_at_this_boundary` |
| Opaque and platform identifiers, `en_US`, `root`, script-leading, POSIX, and legacy ICU forms are rejected | `the_handwritten_015_cases_hold` _(adapter)_, `what_uts_35_does_not_call_well_formed_is_malformed` _(adapter)_, `inputs_icu4x_does_not_parse_are_unparsable` _(adapter)_ |
| A well-formed primary language subtag longer than three letters is rejected by the validity data before ICU4X parses it | `the_handwritten_015_cases_hold` _(adapter)_, `the_handwritten_015_cases_hold_at_this_boundary` |
| A later specification that admitted such a subtag would block the ICU4X 2.2.0 adapter | `an_identifier_icu4x_cannot_parse_blocks_the_adapter` _(adapter)_ |
| Well-formed but invalid identifiers such as `zz`, `en-u-ca-madeup`, and `en-u-zz-abc` are rejected before ICU4X canonicalizes them, though ICU4X alone would keep them | `the_handwritten_015_cases_hold` _(adapter)_, `step_1_names_015s_rules_in_order` _(adapter)_, `forms_revision_0_rejects_still_pass_through_the_raw_engine` _(adapter)_, `admitted_data_canonicalizes_through_the_layer` _(adapter)_ |
| Generated validity boundaries, positive and negative, for languages, scripts, regions, variants, and `u` and `t` keys and types | `every_listed_code_fares_as_its_status_says` _(adapter)_, `every_key_and_literal_type_fares_as_its_listing_says` _(adapter)_, `a_class_admits_its_values_and_marks_deprecated_codes` _(adapter)_ |
| An adapter that rejects a normative valid case is refused, not reported as a user's invalid input | `a_blocking_case_refuses_the_adapter` _(adapter)_, `a_rejection_is_an_invalid_identifier_and_an_adapter_failure_leaves_it_unavailable` |
| A deprecated value is admitted only when a deterministic replacement is present | `a_deprecated_value_waits_for_step_5_only_while_the_input_is_read` _(adapter)_, `every_listed_code_fares_as_its_status_says` _(adapter)_, `the_handwritten_015_cases_hold` _(adapter)_ |
| `regular`, `special`, `macroregion`, and `unknown` components are admitted, including `en-XA`, `ar-XB`, `en-XK`, `und`, and `en-ZZ` | `the_handwritten_015_cases_hold` _(adapter)_, `every_listed_code_fares_as_its_status_says` _(adapter)_ |
| `reserved` and `private_use` components such as `qfz`, `Qaaq`, and `XC` are rejected, in the identifier and in a `t` extension's transformed language | `the_handwritten_015_cases_hold` _(adapter)_, `a_language_identifier_is_checked_part_by_part` _(adapter)_, `every_listed_code_fares_as_its_status_says` _(adapter)_ |

### Extensions

| Requirement | Checked by |
| --- | --- |
| Registered and valid `u` and `t` fields are admitted, including a publicly registered private-use function | `the_handwritten_015_cases_hold` _(adapter)_, `every_key_and_literal_type_fares_as_its_listing_says` _(adapter)_ |
| `en-x-brand`, `en-a-foo`, and other private-use or non-`u`/`t` extensions reject the complete identifier, never a shorter one | `the_handwritten_015_cases_hold` _(adapter)_, `step_1_names_015s_rules_in_order` _(adapter)_ |
| `u` attributes and a repeated extension or key reject the complete identifier rather than keep the first | `the_handwritten_015_cases_hold` _(adapter)_, `step_1_names_015s_rules_in_order` _(adapter)_, `a_second_extension_with_one_singleton_is_read_for_the_layer_to_reject` _(adapter)_ |
| Every `valueType` boundary | `the_handwritten_015_cases_hold` _(adapter)_, `no_type_stands_for_true_only_where_the_key_admits_true` _(adapter)_ |
| Every class boundary, and the deprecated keys `kh` and `vt` | `the_handwritten_015_cases_hold` _(adapter)_, `a_class_admits_its_values_and_marks_deprecated_codes` _(adapter)_, `each_special_type_is_one_of_the_six_classes` _(adapter)_ |
| Deprecated literal types without a preferred value, such as `co-big5han` and `hc-c12`, are rejected while `ca-islamicc` is corrected | `the_handwritten_015_cases_hold` _(adapter)_, `every_key_and_literal_type_fares_as_its_listing_says` _(adapter)_, `a_deprecated_literal_type_left_in_a_result_is_rejected` _(adapter)_ |
| A deprecated subdivision in `rg` or `sd` is admitted only when ICU4X replaces it, and anything deprecated left in a result rejects the identifier | `the_handwritten_015_cases_hold` _(adapter)_, `every_listed_code_fares_as_its_status_says` _(adapter)_, `a_deprecated_value_waits_for_step_5_only_while_the_input_is_read` _(adapter)_ |

### Generated corrections

| Requirement | Checked by |
| --- | --- |
| `und-u-ca-islamicc` is corrected to `und-u-ca-islamic-civil` by generated data | `admitted_data_canonicalizes_through_the_layer` _(adapter)_, `every_correction_is_applied_and_its_result_is_stable` _(adapter)_ |
| Every valid direct-input key and type alias is generated, with ICU4X's own mappings told apart from Intlify's corrections and no handwritten exception list | `corrections_come_from_preferred_values_and_well_formed_aliases` _(adapter)_, `a_mapping_is_realized_when_icu4x_gives_both_one_canonical_form` _(adapter)_, `the_vendored_records_give_revision_0s_conformance_data` _(adapter)_ |
| Legacy-only aliases stay out of the corrections, left to explicit compatibility conversion | `corrections_come_from_preferred_values_and_well_formed_aliases` _(adapter)_, `the_handwritten_015_cases_hold` _(adapter)_ |
| Alias corrections such as `ms-imperial`, `ca-ethiopic-amete-alem`, and `kn-yes`, and no mapping from an alias that names another listed type | `corrections_come_from_preferred_values_and_well_formed_aliases` _(adapter)_, `corrections_are_spliced_in_subtag_order` _(adapter)_, `the_handwritten_015_cases_hold` _(adapter)_ |
| Generation fails when ICU4X 2.2.0 already realizes a direct-input type alias | `a_mapping_icu4x_realizes_fails_generation` _(adapter)_, `a_mapping_icu4x_already_realizes_fails_the_artifact` _(adapter)_ |
| Alias chains are flattened to their final value, and a corrected result is unchanged by a second pass | `a_chain_is_flattened_to_its_final_value` _(adapter)_, `a_correction_maps_straight_to_a_value_nothing_corrects_further` _(adapter)_, `every_correction_is_applied_and_its_result_is_stable` _(adapter)_ |
| A cyclic, unrepresentable, or non-deterministic correction mapping refuses the artifact or the adapter | `a_mapping_that_is_not_deterministic_or_final_fails_generation` _(adapter)_, `a_correction_maps_straight_to_a_value_nothing_corrects_further` _(adapter)_, `a_result_steps_3_and_4_change_blocks_the_adapter` _(adapter)_ |

### The canonical form

| Requirement | Checked by |
| --- | --- |
| Casing, deprecated aliases, and extension order converge on one canonical identifier | `icu4x_canonicalizes_case_aliases_and_order_without_adding_subtags` _(adapter)_, `every_canonical_spelling_is_a_fixed_point` _(adapter)_, `the_handwritten_015_cases_hold` _(adapter)_ |
| `en` stays `en`, with no likely subtags added, and is another locale than `en-US` | `icu4x_canonicalizes_case_aliases_and_order_without_adding_subtags` _(adapter)_ |
| The same canonical output on every host, whatever its ICU, ECMA-402, or operating-system locale data | The first completion condition, and `the_handwritten_015_cases_hold` _(adapter)_ and `the_handwritten_015_cases_hold_at_this_boundary`, which CI runs on Linux, macOS, and Windows |

### Specification, data, and provider

| Requirement | Checked by |
| --- | --- |
| The canonicalization data envelope's accepted and rejected cases, with each closed reason before invocation | `every_fixture_artifact_is_admitted_with_the_vectors_semantic_digest` _(adapter)_, `a_manifest_at_the_boundary_is_an_envelope_failure` _(adapter)_, `each_failure_has_its_015_reason` _(adapter)_, `a_reader_failure_is_reported_where_015_reports_it`, `binding_failures_are_classified_where_015_reports_them` |
| This crate builds and runs without ICU4X's compiled data or an embedded CLDR-derived table | `an_ordinary_build_links_neither_icu4x_nor_its_data`, `only_the_generators_link_icu4x_compiled_data` _(adapter)_ |
| The reference adapter builds ICU4X's extended canonicalizer from a serialized provider with all four marker families, and never maximizes | `the_committed_blob_holds_exactly_the_four_markers` _(adapter)_, `icu4x_blobs_without_a_family_or_with_another_are_refused` _(adapter)_, `icu4x_canonicalizes_case_aliases_and_order_without_adding_subtags` _(adapter)_ |
| Revision `"0"` admits only the selected marker data of `icu_locale_data` 2.2.0 and the conformance data derived from CLDR 48.2.0 | `revision_0_is_the_one_supported_specification_and_admits_cldr_48_2_0`, `the_committed_artifact_is_the_dataset_revision_0_admits`, `the_marker_data_checksum_is_the_one_cargo_locked` _(adapter)_, `the_vendored_records_give_revision_0s_conformance_data` _(adapter)_ |
| One logical dataset has one semantic digest across its physical exports, each with its own file digest | `the_blob_projects_what_icu4x_compiled_data_holds` _(adapter)_, `two_layouts_or_moved_sources_keep_one_dataset_and_change_the_artifact` _(adapter)_. ICU4X's compiled data stands in for a baked export: its payloads are the blob's |
| The manifest records the upstream checksum, the source revision, the marker set, and the semantic digest | `the_committed_artifact_is_written_byte_for_byte` _(adapter)_, `the_marker_data_checksum_is_the_one_cargo_locked` _(adapter)_, `the_four_markers_are_named_as_017_records_them_in_name_order` _(adapter)_, `the_manifest_schema_admits_the_fixtures_and_pins_one_kind_and_revision` _(adapter)_ |
| Missing or truncated data, a provider schema or digest mismatch, and an omitted marker family or conformance data are refused before canonicalization | `a_payload_must_be_supplied_with_its_declared_length_and_digest` _(adapter)_, `a_declared_length_or_digest_that_disagrees_with_the_bytes_is_refused` _(adapter)_, `a_dataset_schema_or_payload_format_this_reader_does_not_implement_is_refused` _(adapter)_, `blob_schemas_from_before_icu4x_2_are_not_read` _(adapter)_, `a_semantic_digest_other_than_the_one_of_the_data_is_refused` _(adapter)_, `a_blob_with_a_family_missing_or_added_is_refused_by_name` _(adapter)_, `an_artifact_that_is_not_admitted_never_becomes_a_provider` |
| The representation, the provider schema, and file digests stay out of the semantics, which compare the specification and the dataset identity and digest | `admission_facts_are_kept_as_reported_and_never_compared`, `provider_data_is_required_and_every_semantic_pin_is_compared`, `two_layouts_or_moved_sources_keep_one_dataset_and_change_the_artifact` _(adapter)_ |
| A provider does no implicit I/O and cannot change its data while it resolves | `payloads_are_acquired_at_the_addresses_the_manifest_names_once_it_is_read`, `provider_binding_changes_fail_before_any_new_locale_work` |

### Adapter conformance

| Requirement | Checked by |
| --- | --- |
| The adapter passes every Intlify locale conformance fixture, each known gap wrapped, outside the admitted domain, or rejected | `the_handwritten_015_cases_hold` _(adapter)_, `the_handwritten_015_cases_hold_at_this_boundary`, `the_finite_fixtures_answers_are_revision_0s`, `the_committed_registry_is_the_gate_over_the_vendored_corpus` _(adapter)_ |
| A valid but non-canonical identifier is corrected by ICU4X or by Intlify's data, and a merely well-formed invalid one is rejected | `every_correction_is_applied_and_its_result_is_stable` _(adapter)_, `a_disposition_follows_the_domain_and_both_answers` _(adapter)_, `the_handwritten_015_cases_hold` _(adapter)_ |
| A gap in the reference engine that nothing corrects refuses the adapter | `a_blocking_case_refuses_the_adapter` _(adapter)_, `an_identifier_icu4x_cannot_parse_blocks_the_adapter` _(adapter)_, `a_result_steps_3_and_4_change_blocks_the_adapter` _(adapter)_ |
| Handwritten regression cases supplement the corpus and never override what it expects | `a_handwritten_case_never_overrides_what_the_corpus_expects` _(adapter)_ |

### The CLDR corpus

| Requirement | Checked by |
| --- | --- |
| All four sets of CLDR 48.2.0's locale canonicalization corpus are projected into hyphen-separated input | `every_upstream_case_is_read_once_in_its_set` _(adapter)_, `the_committed_registry_is_the_gate_over_the_vendored_corpus` _(adapter)_ |
| Every case outside the domain is kept with a machine-readable reason, and no line is skipped | `the_domain_comes_from_the_pinned_validity_data` _(adapter)_, `the_registry_records_each_answer_and_reason` _(adapter)_, `a_line_the_gate_does_not_recognize_stops_it` _(adapter)_ |
| Each admitted case is compared across the expected result, raw ICU4X 2.2.0, and ICU4X through the conformance layer | `a_disposition_follows_the_domain_and_both_answers` _(adapter)_, `the_registry_records_each_answer_and_reason` _(adapter)_ |
| A generated registry records every disposition, and one unexplained mismatch refuses the adapter and fails CI | `the_committed_registry_is_the_gate_over_the_vendored_corpus` _(adapter)_, `each_disposition_counts_in_its_own_column` _(adapter)_, `a_blocking_case_refuses_the_adapter` _(adapter)_, and `vp run corpus:locale:check` |
| The corpus release, path, SHA-256, registry, and outcome are the adapter's evidence, and the corpus is no semantic input | `the_corpus_is_read_at_the_release_the_artifact_was_generated_from` _(adapter)_, `the_registry_records_each_answer_and_reason` _(adapter)_, `the_preimage_is_exactly_the_dataset_017_frames` _(adapter)_ |

### Measurement

| Requirement | Checked by |
| --- | --- |
| Canonicalization is measured on the shared path with its exact binding and explicit engine state, and a fresh engine gives what a reused one gives | `icu4x_cases_follow_the_original_catalog_with_both_engines_for_each_input`, `fresh_and_reused_engines_agree_on_every_semantic_and_work_observation`, `icu4x_descriptors_record_the_engine_state_and_the_admitted_binding`, `the_admitted_facts_pin_the_binding_both_limits_and_the_engine_state`, `a_fresh_engine_that_binds_otherwise_is_never_invoked`, `both_icu4x_engines_run_the_canonicalization_boundary_between_the_same_markers` |

## Rows other phases own

Design 015 lists these beside the Phase 2 items. None is approximated here; Phase 2 gives them a canonical answer or a rejection to build on.

| Requirement | Owner |
| --- | --- |
| Explicit compatibility conversions such as `en_US` to `en-US`, `root` to `und`, and `Latn` to `und-Latn`, with conversion evidence | Design 029, whose compatibility adapter converts a legacy spelling before resolution (Decision 015-018) |
| The blocking configuration Finding for an invalid identifier, and the non-blocking Finding that carries a canonical replacement and its source | Phase 5. This crate's provider reports the rejection or the replacement |
| Duplicate locales after canonicalization, one locale in independent roles, set order by unsigned bytes, permutations, and explicitly ordered sequences | Phase 3. The byte order itself is `canonical_locale_order_is_unsigned_byte_order_and_keeps_prefixes` |
| Authoring spellings and the canonicalization specification in profile equality and digests | Phase 5 |
| A missing `locale-canonicalization` slot or an unsupported specification in the specification input, and the configuration `schemaVersion` staying the same when only the canonicalization specification changes | Phase 3, which brings the specification input to the resolver |
| Canonicalization data in the `storefront` blueprint, Resolver Input Component Paths, the semantic inventory, replay sets, and suppression of downstream checks | Phases 5 and 6 |
| Requested locales, negotiation, and the default source locale | Phase 3 |

## Deliberately not covered in Phase 2

These are named so that their absence is a decision rather than an oversight.

- **A baked provider** — plan 022 ships the blob alone. A test loads ICU4X's own baked data for the four marker families and finds the blob's payloads, so one dataset digest covers both. Equal profiles from a baked and a blob provider need a baked provider and a profile.
- **A later specification revision** — admitting private-use or another extension would take a new canonicalization specification revision with its own scope, matching, and consumer rules. Revision `"0"` rejects those inputs, and the binding admits revision `"0"` alone.
