<!-- @license MIT -->

# Registry Package

Design 015's [Normative Resolver Registry Package](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#normative-resolver-registry-package) is the fifth authority resolver construction is given. It has six members in fixed roles: three closed schemas, and the Safe Origin Kind, Check, and Finding Registries they admit. This directory holds the revision `"0"` members at the paths 015's Suite Index example uses below the conformance suite's root, `conformance/`. So far it holds the Safe Origin Kind Registry and its schema. The Check and Finding Registries, their schemas, and the package's container come with their own stages.

This crate writes every member from its own values, and compiles in only their file digests, which construction compares with the members it is given, as the last row of the construction-failure precedence.

A file digest covers every byte, so these files are kept out of the repository formatter and out of line-ending conversion. Change them only by changing the values and rewriting them.

## Running them

```sh
# every unit test, including the committed bytes and the pinned digests
cargo test -p intlify_config --all-features --lib construction

# rewrite the members after changing a value; it prints the digests to pin
# in src/construction/registry/safe_origin.rs
cargo run -p intlify_config --features vectors --example generate_construction_artifacts -- --write

# the committed bytes, and the independent Node checks
vp run construction:check
```

## Files

| File | Role and identity | Contents |
| --- | --- | --- |
| `safe-origin-kind-v0.schema.json` | `safe-origin-kind-registry-schema`, `intlify-project-profile-safe-origin-kind-registry-schema` | The Draft 7 schema of the registry's container and rows, generated from the registry's types |
| `safe-origin-kind-v0.json` | `safe-origin-kind-registry`, `intlify-project-profile-safe-origin-kind-registry` | 015's ten Safe Origin Kinds in rank order, each with its ID, its rank, and the evidence kinds it permits |

Every member is at revision `"0"`.

## The Safe Origin Kind Registry

- **Values.** The registry is written from the crate's vocabulary of 015's seven evidence kinds and ten Safe Origin Kinds (`src/resolver/origin.rs`). A row is `{ id, rank, permittedEvidenceKinds }`. The rank is a shortest decimal string, as every ordinal in 015's records is, and the permitted evidence kinds keep the order of 015's table. The table's descriptions of each input boundary are documentation, not registry fields.
- **Schema.** The schema closes the container and every row, and requires each member. It closes the vocabularies: an ID is one of the ten, and an evidence kind one of the seven. A rank is a shortest decimal string, and a row permits at least one evidence kind, none twice. It does not say which rank or which evidence kinds an ID has. The registry's file digest, compiled in, fixes those.
- **Closed graph.** Construction reads the registry it admitted, and refuses one it cannot read as its type, or one in which an ID or a rank repeats, with the row of the registry cross-references.

The Node checker shares no code with this crate. It builds the registry from its own copy of 015's Safe Origin Kind table, checks that the schema closes the container, every row, the IDs, the evidence kinds, and a rank's spelling, hashes both members, and reads the two pinned digests out of the crate's source. It does not read the design document.

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
- **Cross-references so far.** Row 41 reads the Safe Origin Kind Registry and checks that no ID and no rank repeats. The Check and Finding Registries are checked against their admitted schemas, and the graph they take part in comes with their stages.

## The matrix

The unit tests are at the end of each source file, named here by their function.

| 015 rule | Checked by |
| --- | --- |
| Seven evidence kinds, and ten Safe Origin Kinds in rank order, each permitting the evidence kinds of its row | `src/resolver/origin.rs`: `evidence_kinds_are_015s_seven`, `safe_origin_kinds_are_015s_ten_in_rank_order`, `every_evidence_kind_enters_through_some_origin`. Node: the registry |
| The registry lists every kind in rank order with its rank and permitted evidence kinds, and names what construction admits | `src/construction/registry/safe_origin.rs`: `the_registry_lists_015s_ten_kinds_in_rank_order`, `the_registry_names_what_construction_admits`, `the_registry_reads_back_as_its_type`. Node: the registry |
| The schema closes the container, every row, and both vocabularies | `safe_origin.rs`: `the_schema_closes_the_container_every_row_and_both_vocabularies`. Node: the schema |
| No ID and no rank repeats | `safe_origin.rs`: `a_repeated_id_or_rank_is_not_part_of_the_closed_graph`. `src/construction/registry.rs`: `the_safe_origin_kind_registry_must_not_repeat_an_id_or_a_rank` |
| The six roles, in role order, each admitted under `intlify-project-profile-<role>` | `registry.rs`: `the_roles_are_015s_six_in_their_fixed_order`, `a_registry_is_checked_against_its_own_schema_and_a_schema_against_none` |
| The package's rows 31 to 35, each refusing with its row | `registry.rs`: `construction_admits_a_package_with_the_committed_safe_origin_kind_registry`, `each_package_check_refuses_with_its_row` |
| Each member's rows 36 to 40, all of one member's before the next's, a registry against its admitted schema | `registry.rs`: `each_member_check_refuses_with_its_row`, `each_member_passes_all_its_rows_before_the_next`, `a_registry_is_checked_against_the_schema_its_package_admitted` |
| The committed bytes are the ones this crate writes, and the pinned digests are theirs | `src/construction/generate.rs`: `the_committed_registries_are_the_ones_this_crate_writes`, `the_pinned_registry_digests_are_the_ones_the_registries_make`, `every_artifact_ends_with_one_newline_and_has_no_carriage_return`. Node: both digests |
