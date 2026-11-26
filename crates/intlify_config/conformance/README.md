<!-- @license MIT -->

# Conformance suite

This directory is the root of design 015's [Project Profile Resolver Conformance Suite](../../../design/015-intlify-project-profile-and-locale-policy-design.md#conformance-and-fixtures) revision `"0"`. The suite root is the directory that holds the Suite Index, `suite-index-v0.json`, and every path the index names is relative to it.

The Suite Index pins every artifact of the suite with its identity, its revision, a suite-root-relative path, and its file digest. Its schema, `schemas/suite-index-v0.schema.json`, is the conformance runner's trust root: the runner pins the schema's identity and file digest, reads the schema only from that fixed path, and validates the index against it before it follows any path.

This crate writes the schema and the index from its own values, as it writes the construction artifacts. The index pins the Root Package and its members in [`construction/`](./construction/README.md), the construction authorities in [`authorities/`](./authorities/README.md), and the Registry Package in [`registries/`](./registries/README.md). The Case Schema and the five manifest arrays join the index as they are written. A file digest covers every byte, so these files are kept out of the repository formatter and out of line-ending conversion.

## Running it

```sh
# every unit test of the index and the runner
cargo test -p intlify_config --all-features --lib conformance

# rewrite the schema and the index after a construction artifact changes; it
# prints the schema's file digest to pin in src/conformance/index.rs
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
4. The closure: every body the index names has the file digest the index declares, and the Profile Specification's body has the semantic digest the index declares. Otherwise `Missing`, `SymbolicLink`, `DigestMismatch`, `Malformed`, or `NotRevisionZero`.

A suite-root-relative path is one or more segments joined by `/`, each starting with an ASCII letter or digit and holding only ASCII letters, digits, `.`, `_`, and `-`. No such path is absolute or empty, or holds a `.` or `..` segment, a backslash, a URL's scheme, or a drive.

## Files

| File | Contents |
| --- | --- |
| `suite-index-v0.json` | The Suite Index: the suite revision, the schema, the Root Package with its five members, the four construction authorities with the mapped configuration schema, and the Registry Package with its six members |
| `schemas/suite-index-v0.schema.json` | The Draft 7 schema of the index. It closes every object and gives each value its form, and leaves which identity, revision, and digests a member pins to the runner |

The Node checker shares no code with this crate. It reads every JSON file strictly, refusing a repeated member, and reports an unreadable path or a document that is not an object. It checks that the schema closes every object with exactly the keywords of a closed object, that the index holds exactly its own copy of 015's members, roles, identities, and revisions, that every path names a file with the declared digest, that the containers list the digests the index declares for their members, that the Profile Specification has the semantic digest its own framing computes, and that every digest is the one the crate pins in its source.

## The matrix

The unit tests are at the end of `src/conformance/index.rs` and `src/conformance/runner.rs`.

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
