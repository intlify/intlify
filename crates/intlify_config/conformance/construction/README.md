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
# pin in src/construction/root.rs
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

The bound values are this implementation's; 015 makes the checked-in body their sole source. The Draft 7 meta-schema is the text json-schema.org publishes, kept byte for byte in [`data/json-schema-draft-07.json`](../../data/json-schema-draft-07.json) and pinned by its digest.

The Node checker shares no code with this crate. It hashes every artifact, frames the canonical digest itself, lists the roles, bounds, units, each component's envelope schemas, the package's schemas, and the precedence from its own copy of 015's tables, and reads the two pinned digests out of `src/construction/root.rs`. It does not read the design document.

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
| The rules member and the admission body list the four cross-authority rules in their order, each with its relationship | `rules.rs`: `the_rules_are_015s_four_in_their_order`, `each_rule_is_recorded_with_its_relationship`. Node: both lists |
| Construction checks the five components in the precedence's order and reports only the first failing row | `flow.rs`: `construction_admits_the_committed_input`, `the_components_are_checked_in_the_precedence_order` |
| Construction then applies the four rules, each to the relationship 015 states, and refuses with row 42 when any fails; a body its admitted schema left open fails the rule that reads it | `rules.rs`: `the_committed_authorities_satisfy_every_rule`, `the_association_is_the_profile_specifications_identity_revision_and_semantic_digest`, `the_body_names_each_admitted_authority_by_identity_and_revision`, `the_bounds_mirror_the_capability_paths_in_order`, `every_phase_the_check_registry_uses_is_listed`, `a_member_a_rule_reads_may_be_missing_or_of_another_type`. `flow.rs`: `each_cross_authority_rule_refuses_with_row_42`, `a_rule_fails_on_a_body_its_schema_left_open` |
| Construction ends with the implemented authority: the Profile Specification's semantic digest, and every other component's and member's file digest, equal the ones compiled in | `flow.rs`: `an_authority_the_resolver_does_not_implement_fails_only_at_row_43`, `each_compared_digest_must_be_the_one_compiled_in`, `a_byte_different_profile_specification_with_an_equal_semantic_digest_constructs`. `registry.rs`: `each_role_implements_its_committed_member` |

The Capability Specification body's envelope schema is written from the same capability inventory as the body. The validator that counts its work is described with the [validation fixtures](../../fixtures/json-schema-test-suite/README.md), and the Profile Specification Schema, the Profile Specification, the Configuration Schema Authority Set, and the Resolver Capability Specification are the [construction authorities](../authorities/README.md). The Registry Package's members are the [registries](../registries/README.md). The construction flow in `src/construction/flow.rs` applies this precedence from row 2 on. The construction-root bounds of row 1, and the Suite Index that pins these paths and digests, come with their own stages.
