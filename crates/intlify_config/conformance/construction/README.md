<!-- @license MIT -->

# Construction artifacts

Design 015's [Resolver Construction Root Package](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#active-project-profile-specification-and-profile-resolution-specification-input) is the finite root every resolver construction is validated under. Its five members, in fixed roles, close the construction bounds, the identities and revisions each construction slot admits, the envelope schemas, the JSON Schema dialect, the construction-failure precedence, and the cross-authority rules. This directory holds the revision `"0"` artifacts at the paths 015's Suite Index example uses below the conformance suite's root, `conformance/`.

This crate writes every artifact from its own values and compiles them in. On first use it checks the package against the two digests it pins: the Root Package artifact's file digest, and the canonical digest framed from the package identity, its revision, and each member's role and file digest. A failure there is an implementation failure, never a construction result.

A file digest covers every byte, so these files are kept out of the repository formatter and out of line-ending conversion. Change them only by changing the values and rewriting them.

## Running them

```sh
# every unit test, including the committed bytes and the pinned digests
cargo test -p intlify_config --all-features --lib construction

# rewrite the artifacts after changing a value; it prints the two digests to
# pin in src/construction/root.rs, and writes the construction vectors once
# the written authorities construct under the pinned digests
cargo run -p intlify_config --features vectors --example generate_construction_artifacts -- --write

# the committed bytes, and the independent Node check
vp run construction:check
```

## Files

| File | Role | Contents |
| --- | --- | --- |
| `root-package-v0.json` | The Root Package artifact | Each member's file digest, in role order |
| `construction-admission-v0.json` | `construction-admission-specification` | The 18 construction bounds with their values and units; the five components with the identity, revisions, and envelope schemas each admits; the mapped configuration schemas and their dialect; the six Registry Package members; the 43-row construction-failure precedence; and the cross-authority rules in their order |
| `construction-admission-v0.schema.json` | `construction-admission-schema` | The Draft 7 schema of that body, generated from the same types |
| `authority-envelope-schemas-v0.json` | `authority-envelope-schema-package` | Closed Draft 7 schemas of a component's declared tuple, the Profile Specification's tuple, the two containers callers supply, and the Capability Specification's body, which closes its bounds by value |
| `json-schema-meta-schemas-v0.json` | `json-schema-meta-schema-package` | Draft 7, the one dialect revision `"0"` admits, with its published meta-schema |
| `cross-authority-rules-v0.json` | `cross-authority-rules` | The four rules with the relationship each requires |
| `construction-vectors-v0.json` | Not a member: construction vectors | The Root Package's canonical preimage and digest, and the Resolver Construction Identity of the committed authorities and registries: its preimage, its digest, and its `rci0_` text |

The bound values are this implementation's; 015 makes the checked-in body their sole source. The Draft 7 meta-schema is the text json-schema.org publishes, kept byte for byte in [`data/json-schema-draft-07.json`](../../data/json-schema-draft-07.json) and pinned by its digest.

The construction vectors pin the preimages and digests 015 asks construction vectors to pin (D15:987). The generator writes them by constructing the authorities and registries it writes, under the digests the crate pins, and deriving the identity from what construction admitted.

The Node checker shares no code with this crate. It hashes every artifact, frames the canonical digest and the construction identity itself, lists the roles, bounds, units, each component's envelope schemas, the package's schemas, and the precedence from its own copy of 015's tables, and reads the two pinned digests out of `src/construction/root.rs`. It does not read the design document.

## The matrix

The unit tests are at the end of each source file below `src/construction/`, named here by their function.

| 015 rule | Checked by |
| --- | --- |
| The Root Package holds exactly five members in fixed roles and order | `root.rs`: `the_roles_are_015s_five_in_their_fixed_order`, `the_roles_must_be_the_five_in_their_fixed_order`. Node: the roles |
| The canonical digest is SHA-256 over the domain separator and the framed identity, revision, and member records, and no path enters it | `root.rs`: `the_canonical_digest_frames_the_identity_the_revision_and_each_member`, `the_canonical_digest_commits_every_role_and_member_but_no_path`. Node: the canonical digest |
| The implementation pins both digests and checks its built-in package against them; a missing or changed member, a role disagreement, or a digest mismatch is a startup failure | `root.rs`: `the_built_in_package_is_the_one_the_implementation_pins`, `a_changed_artifact_is_refused_by_its_file_digest`, `a_member_whose_bytes_changed_is_named`, `members_handed_over_out_of_role_order_disagree_with_the_artifact`, `a_malformed_artifact_is_refused`, `a_canonical_digest_that_is_not_the_pinned_one_is_refused`, `a_digest_is_read_back_only_from_its_exact_presentation` |
| A file digest covers every byte of the artifact | `generate.rs`: `the_committed_artifacts_are_the_ones_this_crate_writes`, `the_pinned_digests_are_the_ones_the_artifacts_make`, `every_artifact_ends_with_one_newline_and_has_no_carriage_return`. Node: every file digest |
| The construction-admission body holds the 18 bounds in canonical order, each with a ResourceBoundValue and its unit | `admission.rs`: `the_bounds_are_015s_eighteen_in_canonical_order`, `each_bound_counts_in_the_unit_015s_table_gives_it`, `every_bound_value_reads_back_as_a_bound`, `a_single_bound_never_exceeds_its_total`. Node: every bound |
| It fixes the identity and revision each component and Registry Package member admits, the envelope schemas each component is checked against, and the configuration schemas' dialect | `admission.rs`: `the_body_admits_015s_components_members_and_dialect_at_revision_zero`, `a_component_lists_its_tuple_s_schema_and_then_its_artifact_s`. Node: components, their envelope schemas, members, and dialect |
| It holds the complete construction-failure precedence | `failure.rs`: `the_precedence_has_015s_forty_three_rows_and_thirty_eight_reasons`, `every_reason_is_spelled_as_015_writes_it`, `the_rows_keep_015s_order_of_components_and_checks`, `a_row_is_written_with_its_three_cells`. Node: every row |
| The body validates against the construction-admission schema, which closes its inventory | `generate.rs`: `the_admission_body_validates_against_its_own_schema` |
| The envelope schemas constrain form only, and each container holds exactly its members | `envelope.rs`: `every_envelope_schema_is_a_valid_draft_7_schema`, `a_tuple_is_closed_and_checks_form_only`, `the_configuration_schema_authority_set_maps_schema_versions_to_members`, `the_registry_package_holds_exactly_the_six_roles`, `the_package_holds_every_envelope_schema_by_name`. Node: the package's schemas |
| The Capability Specification body's schema closes its groups, bounds, units, order, and minimum vector | `capability.rs`: `the_body_schema_closes_the_body_s_values`. Node: the schema, in the construction authorities' checker |
| Revision `"0"` admits Draft 7 alone, under its package-pinned exact meta-schema | `meta_schema.rs`: `the_meta_schema_is_the_pinned_bytes`, `the_package_names_draft_7_and_its_own_meta_schema`. Node: the meta-schema |
| An artifact counts in the bounds' units, its depth with the root at one, its values, its collection entries, and the UTF-8 bytes of its strings and keys, only when it is strict UTF-8 JSON, which a scan without recursion decides at any depth | `scan.rs`: `the_counts_are_the_units_of_the_construction_bounds`, `the_committed_artifacts_count_as_their_values_do`, `the_scan_is_as_strict_as_the_shared_decoder`, `the_scan_agrees_with_the_shared_decoder_on_changed_bytes`, `the_scan_decides_depths_the_shared_decoder_cannot_read`. `component.rs`: `an_artifact_too_deep_for_the_decoder_still_counts` |
| Construction totals every bound over the complete input before any other row, in 015's units and scopes, and refuses at row 1 with the first bound, in the canonical order, whose total exceeds its value, naming it; an exact total is admitted, and a total past 64 bits exceeds every bound | `bounds.rs`: `the_totals_follow_the_canonical_order_of_the_admission_body`, `the_committed_input_totals_what_its_artifacts_count`, `each_scope_counts_what_construction_is_given_for_it`, `the_cross_authority_checks_grow_with_what_the_rules_compare`, `the_first_bound_over_its_value_is_named`, `the_arithmetic_is_checked_and_never_saturates_into_a_bound`, `a_refusal_at_row_1_names_its_bound_and_no_other_row_does`. `flow.rs`: `the_bounds_come_before_every_other_row`, `an_exact_total_constructs_and_one_more_is_refused`, `of_several_exceeded_bounds_the_first_is_named` |
| The rules member and the admission body list the four cross-authority rules in their order, each with its relationship | `rules.rs`: `the_rules_are_015s_four_in_their_order`, `each_rule_is_recorded_with_its_relationship`. Node: both lists |
| Construction checks the five components in the precedence's order and reports only the first failing row | `flow.rs`: `construction_admits_the_committed_input`, `the_components_are_checked_in_the_precedence_order` |
| Construction then applies the four rules, each to the relationship 015 states, and refuses with row 42 when any fails; a body its admitted schema left open fails the rule that reads it | `rules.rs`: `the_committed_authorities_satisfy_every_rule`, `the_association_is_the_profile_specifications_identity_revision_and_semantic_digest`, `the_body_names_each_admitted_authority_by_identity_and_revision`, `the_bounds_mirror_the_capability_paths_in_order`, `every_phase_the_check_registry_uses_is_listed`, `a_member_a_rule_reads_may_be_missing_or_of_another_type`. `flow.rs`: `each_cross_authority_rule_refuses_with_row_42`, `a_rule_fails_on_a_body_its_schema_left_open` |
| Construction ends with the implemented authority: the Profile Specification's semantic digest, and every other component's and member's file digest, equal the ones compiled in | `flow.rs`: `an_authority_the_resolver_does_not_implement_fails_only_at_row_43`, `each_compared_digest_must_be_the_one_compiled_in`, `a_byte_different_profile_specification_with_an_equal_semantic_digest_constructs`. `registry.rs`: `each_role_implements_its_committed_member` |
| A successful construction's Resolver Construction Identity is SHA-256 over the domain separator and one unframed record of 015's eight fields: the Root Package by its canonical digest, each component by its identity, revision, and file digest, the Profile Specification by its semantic digest, the set's association, each mapping in ascending `schemaVersion` order, and the registry members in role order; strings are their UTF-8 bytes and digests their 32 bytes | `identity.rs`: `the_preimage_is_015s_record_of_eight_fields`, `every_field_moves_the_identity`. Node: the identity, framed from the committed artifacts |
| The same authorities in another layout, a Profile Specification with other whitespace and member order, or tuples with their members in another order, have one identity | `identity.rs`: `the_same_authorities_in_another_layout_have_one_identity` |
| The construction vectors pin the Root Package's canonical preimage and digest and the committed input's identity: its preimage, its digest, and its `rci0_` text | `identity.rs`: `the_committed_input_has_the_identity_the_vectors_pin`. `generate.rs`: `the_committed_vectors_are_the_ones_this_crate_writes`. Node: both preimages and digests, and the text |
| A successful construction keeps the input it was given in an immutable resolver, which supports in each specification slot what the Profile Specification it admitted pins, with the dataset it admits, and no refused input builds one | `resolver.rs`: `a_resolver_keeps_the_input_it_was_built_from`, `a_refused_input_builds_no_resolver`, `the_supported_values_are_the_profile_specifications` |
| The resolver analyzes a configuration with the document the admitted set maps to the `schemaVersion` the model reads, compiled only after construction admits it, and selects it only for a root `schemaVersion` the mapping maps; that document is the schema generated from the model, and no member the set does not map is read | `resolver.rs`: `the_configuration_schema_is_the_admitted_document`, `the_admitted_schema_is_the_one_generated_from_the_model`, `a_version_the_mapping_does_not_map_is_unsupported`, `an_unmapped_member_leaves_the_configuration_schema`, `a_refused_input_builds_no_resolver`. `structural/admission_tests.rs`: `admission_selects_only_the_version_the_schema_is_mapped_to` |

The Capability Specification body's envelope schema is written from the same capability inventory as the body. The validator that counts its work is described with the [validation fixtures](../../fixtures/json-schema-test-suite/README.md), and the Profile Specification Schema, the Profile Specification, the Configuration Schema Authority Set, and the Resolver Capability Specification are the [construction authorities](../authorities/README.md). The Registry Package's members are the [registries](../registries/README.md). The construction flow in `src/construction/flow.rs` applies this precedence, starting with the construction-root bounds of row 1 in `src/construction/bounds.rs`. A successful construction keeps its input in an immutable resolver, `src/construction/resolver.rs`, which compiles the configuration schema it admitted, and `src/construction/identity.rs` derives its identity after it. The Suite Index that pins these paths and digests comes with its own stage.
