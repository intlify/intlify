<!-- @license MIT -->

# Conformance suite

This directory is the root of design 015's [Project Profile Resolver Conformance Suite](../../../design/015-intlify-project-profile-and-locale-policy-design.md#conformance-and-fixtures) revision `"0"`. The suite root is the directory that holds the Suite Index, `suite-index-v0.json`, and every path the index names is relative to it.

The Suite Index pins every artifact of the suite with its identity, its revision, a suite-root-relative path, and its file digest. Its schema, `schemas/suite-index-v0.schema.json`, is the conformance runner's trust root: the runner pins the schema's identity and file digest, reads the schema only from that fixed path, and validates the index against it before it follows any path.

This crate writes the schema and the index from its own values, as it writes the construction artifacts. The index pins the Root Package and its members in [`construction/`](./construction/README.md), the construction authorities in [`authorities/`](./authorities/README.md), and the Registry Package in [`registries/`](./registries/README.md). It pins the Case Schema, `schemas/case-v0.schema.json`, the closed form of the suite's five manifest kinds, the same way, and lists each manifest by its ID, path, and file digest in the array of its kind. The arrays stay empty until the cases are written, and the forms of the fixtures a manifest names join the Case Schema with the cases that read them. A file digest covers every byte, so these files are kept out of the repository formatter and out of line-ending conversion.

## Running it

```sh
# every unit test of the index, the Case Schema, and the runner
cargo test -p intlify_config --all-features --lib conformance

# rewrite the schemas and the index after a construction artifact changes; it
# prints the two schemas' file digests to pin in src/conformance/index.rs and
# src/conformance/case.rs
cargo run -p intlify_config --features conformance --example generate_conformance_suite -- --write

# open the suite at an explicit index: the runner has no default index
cargo run -p intlify_config --features conformance --example run_conformance_suite -- crates/intlify_config/conformance/suite-index-v0.json

# the committed bytes, the runner, and the independent Node check
vp run conformance:check
```

## Opening a suite

The runner is handed the index's path and opens the suite in this order. The first failure stops it with a Conformance harness failure, which is never a construction failure, an input-admission failure, a Finding, or a resolver outcome. It never reads through a symbolic link below the suite root: a link could reach a file outside the suite with the bytes the index declares, so a path that passes through one is `SymbolicLink`.

1. The trust root: the schema at `schemas/suite-index-v0.schema.json` below the suite root has the file digest the runner pins. Otherwise `Missing`, `SymbolicLink`, or `DigestMismatch`.
2. The index is strict UTF-8 JSON and valid against the schema. Otherwise `Missing`, `Malformed`, or `SchemaInvalid`. An index that lacks a member leaves a body it must list unlisted, and is `SchemaInvalid`.
3. The index is a revision-`"0"` index: each member pins the identity, revision, and digests of the artifact this crate is compiled from, and the Root Package's and the Registry Package's roles come in their fixed order. Where a body lies below the suite root is the index's to say, except for the schema's fixed path, and the Profile Specification is pinned by its semantic digest rather than its file digest. Otherwise `NotRevisionZero`.
4. Every manifest is listed once: each array lists its IDs in ascending unsigned byte order, and no ID occurs in two arrays. Otherwise `NotRevisionZero`.
5. The closure: every body the index names has the file digest the index declares, the Profile Specification's body has the semantic digest the index declares, and each manifest is strict UTF-8 JSON, valid against the Case Schema, with the `id` that lists it and the `manifestKind` of its array. Otherwise `Missing`, `SymbolicLink`, `DigestMismatch`, `Malformed`, `SchemaInvalid`, or `NotRevisionZero`.

A suite-root-relative path is one or more segments joined by `/`, each starting with an ASCII letter or digit and holding only ASCII letters, digits, `.`, `_`, and `-`. No such path is absolute or empty, or holds a `.` or `..` segment, a backslash, a URL's scheme, or a drive.

## Files

| File | Contents |
| --- | --- |
| `suite-index-v0.json` | The Suite Index: the suite revision, the schema, the Root Package with its five members, the four construction authorities with the mapped configuration schema, the Case Schema, the Registry Package with its six members, and the five manifest arrays, empty until the cases are written |
| `schemas/suite-index-v0.schema.json` | The Draft 7 schema of the index. It closes every object and gives each value its form, and leaves which identity, revision, and digests a member pins to the runner |
| `schemas/case-v0.schema.json` | The Case Schema: the closed form of the five manifest kinds, with their ID and rule-ID grammars, `traceability`, construction expectations and reasons, each boundary kind's reasons and admitted input, the admission steps' failures, Resolver Case entries and relations, Handoff Vectors, and the eight conditional obligations |

The Node checker shares no code with this crate. It reads every JSON file strictly, refusing a repeated member, and reports an unreadable path or a document that is not an object. It checks that the schema closes every object with exactly the keywords of a closed object, that the index holds exactly its own copy of 015's members, roles, identities, and revisions, that every path names a file with the declared digest, that the containers list the digests the index declares for their members, that the Profile Specification has the semantic digest its own framing computes, and that every digest is the one the crate pins in its source. It also checks that each manifest array lists references of its kind, once and in ascending byte order, and that each manifest has the ID that lists it and the kind of its array.

## The matrix

The unit tests are at the end of `src/conformance/index.rs`, `src/conformance/case.rs`, and `src/conformance/runner.rs`.

| 015 rule | Checked by |
| --- | --- |
| The suite root is the directory that holds the Suite Index, and the runner is handed the index's path; it never searches for an index (D15:2858) | `runner.rs`: `the_suite_root_is_the_directory_that_holds_the_index` |
| The runner's trust root is the Suite Index Schema's identity and digest; it reads the schema from its fixed path and validates the index before following any path (D15:3029) | `index.rs`: `the_pinned_schema_digest_is_the_written_schema_s`. `runner.rs`: `the_schema_is_read_from_its_fixed_path_and_must_be_the_pinned_one`, `an_index_the_schema_refuses_does_not_open`. Node: the pinned digest |
| Every reference carries a suite-root-relative path and a file digest; absolute paths, `..` segments, backslashes, and URLs are prohibited (D15:2858) | `index.rs`: `a_path_is_suite_root_relative`, `the_schema_closes_every_object_and_value`. Node: the path grammar |
| The index is closed: a missing or unknown member, or a value out of its form, makes it no revision-`"0"` index (D15:3031) | `index.rs`: `the_schema_closes_every_object_and_value`, `the_schema_checks_form_only`. `runner.rs`: `an_index_the_schema_refuses_does_not_open`. Node: the closed schema objects |
| Every construction-root and construction-authority tuple matches its canonical checked-in artifact, the roles come in their fixed order, and the Profile Specification carries its semantic digest (D15:3031) | `runner.rs`: `a_member_that_pins_another_artifact_is_not_revision_zero`, `roles_out_of_their_order_are_not_revision_zero`, `the_profile_specification_is_pinned_by_its_semantic_digest`. Node: the identities, roles, containers, semantic digest, and pins |
| Missing content, a digest mismatch, or malformed content is a Conformance harness failure (D15:2858) | `runner.rs`: `an_index_that_is_missing_or_not_strict_json_does_not_open`, `a_body_that_is_missing_or_changed_does_not_open`, `each_failure_says_what_failed` |
| A reference names a file below the suite root, not one a symbolic link reaches (D15:2858) | `runner.rs`: `a_body_reached_through_a_symbolic_link_does_not_open` (on Unix, where tests can make links) |
| The index pins the committed artifacts by their file digests | `index.rs`: `the_committed_schema_and_index_are_the_ones_this_crate_writes`, `the_index_names_every_artifact_this_crate_writes`, `the_written_index_is_valid_against_the_draft_7_schema`. `runner.rs`: `the_committed_suite_opens_and_every_body_it_names_is_verified`, `a_body_may_lie_anywhere_below_the_suite_root`. Node: every file digest |
| The Case Schema is pinned by identity, revision, and file digest (D15:3389) | `case.rs`: `the_committed_case_schema_is_the_one_this_crate_writes`, `the_case_schema_is_a_draft_7_schema_both_validators_read`. `runner.rs`: `a_member_that_pins_another_artifact_is_not_revision_zero`. Node: the pinned digest |
| Every listed ID occurs once within and across the five arrays, in ascending unsigned byte order; case and vector IDs are `015-` and lowercase kebab-case, obligation IDs `015-`, `XV`, `AD`, or `HV`, and three digits (D15:3031) | `index.rs`: `a_manifest_array_lists_ids_of_its_kind`. `runner.rs`: `manifests_are_listed_once_in_ascending_order`. `case.rs`: `ids_and_traceability_have_their_grammar`. Node: the order, the IDs, and their forms |
| Every referenced manifest passes the complete Case Schema, with its own `id` and the `manifestKind` of its array (D15:3031) | `runner.rs`: `listed_manifests_open_and_are_verified`, `a_listed_manifest_passes_the_case_schema_and_its_listing`. `case.rs`: `each_manifest_kind_is_closed`. Node: each manifest's ID and kind |
| A construction case expects one exact identity, or no identity and one closed reason, the input-limit reason alone with its bound (D15:3060, 3105) | `case.rs`: `a_construction_case_expects_an_identity_or_one_closed_reason`, `the_vocabularies_are_this_crate_s` |
| An admission case's boundary kind closes its failure reasons and admitted input; the two admission steps list failures of their own slots (D15:3140-3168, 3256-3265) | `case.rs`: `an_admission_case_pairs_each_boundary_kind_with_its_own_reasons`, `an_admitted_input_has_its_kind_s_form`, `an_admission_step_lists_failures_of_its_own_slots`, `the_boundary_table_is_the_reasons_the_boundaries_give`, `an_admitted_snapshot_node_is_closed` |
| A Resolver Case pairs its entries, or names a closed reason and an owner for the unpaired one; its relations are closed (D15:3233-3237, 3305) | `case.rs`: `a_resolver_case_pairs_its_entries_or_names_why_one_is_unpaired`, `a_relation_names_a_class_of_its_role` |
| A Handoff Vector names its producing case, its facts by semantic group, its bindings, and one handoff owner (D15:3309-3346) | `case.rs`: `a_handoff_vector_names_its_producer_facts_and_owner` |
| A conditional obligation is one row of the obligation table, inactive with its not-applicable reason or active without one (D15:3371-3382) | `case.rs`: `a_conditional_obligation_is_one_row_of_the_table` |
