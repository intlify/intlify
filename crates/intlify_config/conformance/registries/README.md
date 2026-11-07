<!-- @license MIT -->

# Registry Package

Design 015's [Normative Resolver Registry Package](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#normative-resolver-registry-package) is the fifth authority resolver construction is given. It has six members in fixed roles: three closed schemas, and the Safe Origin Kind, Check, and Finding Registries they admit. This directory holds the revision `"0"` members at the paths 015's Suite Index example uses below the conformance suite's root, `conformance/`. So far it holds the Safe Origin Kind Registry, the Check Registry, and their schemas. The Finding Registry, its schema, and the package's container come with their own stages.

This crate writes every member from its own values, and compiles in only their file digests, which construction compares with the members it is given, as the last row of the construction-failure precedence.

A file digest covers every byte, so these files are kept out of the repository formatter and out of line-ending conversion. Change them only by changing the values and rewriting them.

## Running them

```sh
# every unit test, including the committed bytes and the pinned digests
cargo test -p intlify_config --all-features --lib construction

# rewrite the members after changing a value; it prints the digests to pin
# in src/construction/registry/safe_origin.rs and check.rs
cargo run -p intlify_config --features vectors --example generate_construction_artifacts -- --write

# the committed bytes, and the independent Node checks
vp run construction:check
```

## Files

| File | Role and identity | Contents |
| --- | --- | --- |
| `safe-origin-kind-v0.schema.json` | `safe-origin-kind-registry-schema`, `intlify-project-profile-safe-origin-kind-registry-schema` | The Draft 7 schema of the registry's container and rows, generated from the registry's types |
| `safe-origin-kind-v0.json` | `safe-origin-kind-registry`, `intlify-project-profile-safe-origin-kind-registry` | 015's ten Safe Origin Kinds in rank order, each with its ID, its rank, and the evidence kinds it permits |
| `check-v0.schema.json` | `check-registry-schema`, `intlify-project-profile-check-registry-schema` | The Draft 7 schema of the registry's container and rows, generated from the registry's types |
| `check-v0.json` | `check-registry`, `intlify-project-profile-check-registry` | One row for each check of each subject kind the resolver evaluates or declares, 164 in all, in phase, kind-rank, and check-rank order |

Every member is at revision `"0"`.

## The Safe Origin Kind Registry

- **Values.** The registry is written from the crate's vocabulary of 015's seven evidence kinds and ten Safe Origin Kinds (`src/resolver/origin.rs`). A row is `{ id, rank, permittedEvidenceKinds }`. The rank is a shortest decimal string, as every ordinal in 015's records is, and the permitted evidence kinds keep the order of 015's table. The table's descriptions of each input boundary are documentation, not registry fields.
- **Schema.** The schema closes the container and every row, and requires each member. It closes the vocabularies: an ID is one of the ten, and an evidence kind one of the seven. A rank is a shortest decimal string, and a row permits at least one evidence kind, none twice. It does not say which rank or which evidence kinds an ID has. The registry's file digest, compiled in, fixes those.
- **Closed graph.** Construction reads the registry it admitted, and refuses one it cannot read as its type, or one in which an ID or a rank repeats, with the row of the registry cross-references.

The Node checker shares no code with this crate. It builds the registry from its own copy of 015's Safe Origin Kind table, checks that the schema closes the container, every row, the IDs, the evidence kinds, and a rank's spelling, hashes both members, and reads the two pinned digests out of the crate's source. It does not read the design document.

## The Check Registry

- **Values.** The registry is written from the resolver's subject kinds and their checks (`src/resolver/subject.rs`), so every `(phase, subject kind, check)` the resolver evaluates has one row, and so does every check of the kinds declared before their units exist. A row is `{ checkId, phaseId, ruleId, subjectKind, subjectKeyStrategy, subjectContentProjection, owningScope, parentSubject, subjectKindRank, checkRank }`; `parentSubject` is absent for an unscoped subject. The prerequisites, the codes and Evidence record kinds a row may produce, the dependencies it owns, and its payload strategy come with later stages, which add their members to every row.
- **IDs.** 015 leaves the IDs of kinds, scopes, and checks to the registry. A kind's ID is its name in kebab case, such as `relation-endpoint` or `diagnostic-domain`. A check ID reads `<area>.<subject>.<check>`, the area naming the phase, and 015's five examples, such as `locale.default-requested.member`, name the checks they describe. A rule ID is the check ID after `015.`.
- **Keys.** A kind keyed by nothing is a `singleton`, whose owning scope is its own kind. A single-valued field is a `logical-field`, scoped by the entry its path starts from: the root, a profile, a Target ID, or a Group ID in structural admission, and the selected declaration later. A kind keyed by an identity or content is `canonical-identity-or-content` and projects its content as `redacted-structure`, because classifying invalid content as non-secret would expose arbitrary values (015-157); the fallback candidates' positions are `semantic-position`. A kind whose key is scoped by another names that kind's phase and ID as its parent.
- **Schema.** The schema closes the container, every row, and a parent, and the vocabularies of phases, kinds, strategies, and projections, and it fixes the spelling of check IDs, rule IDs, scopes, and ranks. It does not say which values a row has, or whether the rows agree with each other: the registry's file digest, compiled in, fixes the values.
- **Closed graph.** Construction reads the registry it admitted, with the row of the registry cross-references, and refuses one it cannot read as its type or whose rows do not relate as 015 requires: a check ID names one row, under its own rule ID; the rows of one kind in one phase agree on its strategy, projection, scope, parent, and rank; kinds rank uniquely within a phase and checks within a kind; only a kind keyed by an identity or content names a projection; a singleton has no parent; and a parent is a kind with rows in the same or an earlier phase, from which following parents ends at an unscoped kind. Ranks are compared as the spellings the schema fixes. The conventions 015 leaves to the registry, such as ranks counting up from zero or `redacted-structure` over `non-secret-content`, are not part of the graph, and neither is whether the rows are the resolver's own kinds and checks: a closed registry that is not the one compiled in fails construction's last row by its file digest.

The Node checker does not rebuild the rows. It checks that they agree with each other as 015's registry rules require: a rule ID is its check ID under `015.`, check IDs do not repeat, the rows follow phase, kind-rank, and check-rank order, every row of a kind has the kind's shape, ranks count up from zero, only identity and content keys carry a projection and none is `non-secret-content`, a singleton's scope is its own kind, a parent is an identity-keyed kind of the same or an earlier phase, no chain of parents leads back to a kind, every phase but profile projection has rows, and 015's five example check IDs name their checks. It checks that the schema closes the container, every row, a parent, and the four vocabularies with the right spellings, and it compares both members' file digests with the ones pinned in `src/construction/registry/check.rs`.

## Construction's checks

Construction checks the package it is given in the order of 015's construction-failure precedence, and refuses with the first row that fails. Rows 31 to 41 are the package's:

| Rows | Component | Checks, in order |
| --- | --- | --- |
| 31–35 | Registry Package | Presence; the declared tuple, strict UTF-8 JSON, and the container's envelope schema with exactly the six roles in role order; identity; revision; file digest |
| 36–40 | Each member, in role order | Presence of its bytes; identity; revision; the file digest the container declares; strict UTF-8 JSON, then, for a schema, a root `$schema` naming Draft 7, the Draft 7 meta-schema, and the validator's reading, or, for a registry, validity against the schema its package admitted for it |
| 41 | Registry Package | The registry cross-references |

The identity and revisions each role admits, the container's envelope schema, and the meta-schema are read from the built-in Root Package.

- **One member at a time.** Each member passes all of its rows before the next is checked, so a schema member is admitted before the registry it admits.
- **The admitted schema.** A registry is checked against the schema its own package admitted for its role, not against the one this crate writes. A schema that admits a registry this crate cannot read moves the refusal to row 41; the digests compiled in are compared only by construction's last row.
- **Cross-references so far.** Row 41 reads the Safe Origin Kind Registry and checks that no ID and no rank repeats, and reads the Check Registry and checks that its rows form the closed graph above. The Finding Registry is checked against its admitted schema, and the references between registries come with its stage.

## The matrix

The unit tests are at the end of each source file, named here by their function.

| 015 rule | Checked by |
| --- | --- |
| Seven evidence kinds, and ten Safe Origin Kinds in rank order, each permitting the evidence kinds of its row | `src/resolver/origin.rs`: `evidence_kinds_are_015s_seven`, `safe_origin_kinds_are_015s_ten_in_rank_order`, `every_evidence_kind_enters_through_some_origin`. Node: the registry |
| The registry lists every kind in rank order with its rank and permitted evidence kinds, and names what construction admits | `src/construction/registry/safe_origin.rs`: `the_registry_lists_015s_ten_kinds_in_rank_order`, `the_registry_names_what_construction_admits`, `the_registry_reads_back_as_its_type`. Node: the registry |
| The schema closes the container, every row, and both vocabularies | `safe_origin.rs`: `the_schema_closes_the_container_every_row_and_both_vocabularies`. Node: the schema |
| No ID and no rank repeats | `safe_origin.rs`: `a_repeated_id_or_rank_is_not_part_of_the_closed_graph`. `src/construction/registry.rs`: `the_safe_origin_kind_registry_must_not_repeat_an_id_or_a_rank` |
| The six roles, in role order, each admitted under `intlify-project-profile-<role>` | `registry.rs`: `the_roles_are_015s_six_in_their_fixed_order`, `a_registry_is_checked_against_its_own_schema_and_a_schema_against_none` |
| The package's rows 31 to 35, each refusing with its row | `registry.rs`: `construction_admits_a_package_with_the_committed_registries`, `each_package_check_refuses_with_its_row` |
| Each member's rows 36 to 40, all of one member's before the next's, a registry against its admitted schema | `registry.rs`: `each_member_check_refuses_with_its_row`, `each_member_passes_all_its_rows_before_the_next`, `a_registry_is_checked_against_the_schema_its_package_admitted` |
| One row for each check of each subject kind, in unit order, with unique check IDs and rule IDs under `015.` | `src/construction/registry/check.rs`: `the_registry_has_one_row_for_each_check_of_each_kind_in_unit_order`, `check_ids_are_unique_and_each_rule_id_is_its_check_id_under_015`. Node: the Check Registry |
| 015's five example check IDs name the checks they describe | `check.rs`: `the_example_check_ids_of_015_name_their_checks`. Node: the Check Registry |
| A kind's ID is its name in kebab case, every row of a kind has its shape, and only identity and content keys carry a projection | `check.rs`: `a_kinds_id_is_its_name_in_kebab_case_and_a_phases_its_015_id`, `the_rows_of_a_kind_share_its_shape_and_only_identities_project_content`, `a_singletons_scope_is_its_own_kind`. Node: the Check Registry |
| A parent is a kind keyed by an identity or content, of the same or an earlier phase, and 015's parent-scoped subjects are elements under their parents | `check.rs`: `a_parent_is_an_identity_kind_of_the_same_or_an_earlier_phase`, `a_parented_row_names_its_parents_phase_and_kind`, `the_parent_scoped_subjects_015_names_are_elements_under_their_parents`. Node: the Check Registry |
| The schema closes the container, every row, a parent, and the vocabularies | `check.rs`: `the_schema_closes_the_container_every_row_and_the_vocabularies`. Node: the Check Registry's schema |
| Construction reads the Check Registry as a closed graph: a check ID names one row under its own rule ID, a kind's rows agree, ranks do not repeat, projections fit strategies, a singleton has no parent, and parents have rows in the same or an earlier phase and lead back to no kind | `check.rs`: `construction_reads_the_registry_as_a_closed_graph`, `a_check_id_names_one_row_under_its_own_rule_id`, `the_rows_of_a_kind_agree_on_its_shape_parent_and_rank`, `kinds_rank_uniquely_within_a_phase_and_checks_within_a_kind`, `only_identity_and_content_keys_name_a_projection`, `a_parent_is_a_kind_with_rows_in_the_same_or_an_earlier_phase`, `no_kind_scopes_itself_through_its_parents`. `registry.rs`: `construction_admits_a_package_with_the_committed_registries`, `the_check_registry_must_form_a_closed_graph`. Node: the Check Registry |
| The committed bytes are the ones this crate writes, and the pinned digests are theirs | `src/construction/generate.rs`: `the_committed_registries_are_the_ones_this_crate_writes`, `the_pinned_registry_digests_are_the_ones_the_registries_make`, `every_artifact_ends_with_one_newline_and_has_no_carriage_return`. Node: both digests |
