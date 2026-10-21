<!-- @license MIT -->

# Artifact envelope fixtures

Design 017's [Profile resolution input admission and fixtures](../../../../design/017-intlify-shared-artifact-and-version-admission-design.md#profile-resolution-input-admission-and-fixtures) asks for independent expectations about the members of 015's Submitted Profile Resolution Artifact Collection: their Policy envelopes, tuple establishment, canonical submitted bodies, and keys and bytes. This file maps each one to where it is checked, so a reader can go from a requirement to the code that pins it.

The members here are test values. Their contents follow 017's shapes, and none of them is an artifact any project admits.

## Running them

```sh
# every unit test
cargo test -p intlify_config --all-targets --all-features

# the committed vectors, and the independent Node check of them
vp run vectors:envelope:check
```

## Data files

| File | Contents | Read by |
| --- | --- | --- |
| `vectors.json` | The eight admitted kinds' registry; 38 members, each with what it declares, the tuple it establishes, whether its integrity holds, and its canonical bytes; the members whose semantic digests must agree or differ; four spellings of one member; the members that must stay unequal; two texts a JSON number makes unrepresentable; the members within one tuple and without one in canonical-body order; and two references with their canonical bytes | `src/artifacts/vectors/envelope.rs` tests; `tools/shared-json-vectors` |

The digests and byte counts cover values, not the file's bytes, so the repository formatter may lay the file out; the spellings are JSON strings so their whitespace survives. The Node checker shares no code with this crate: it decides each member's declaration and tuple from 017's rules, frames and hashes with its own implementation, and sorts the members itself.

## The matrix

The unit tests are at the end of each source file, named here by their function.

| 017 requirement | Checked by |
| --- | --- |
| Policy envelopes: the semantic digests of a Resource Limit Policy, Trust Policy, and Glossary Set content, and a Resource Limit Policy content without `projectProfileResolution`, with a non-object section, and with an invalid section interior | The vectors' `resource-limit-policy` members, `trust-policy`, and `glossary-set`, whose digests Node recomputes. Each still establishes its tuple and keeps its integrity here; which 015 or 018 check each reaches is decided when the Resource Limit Policy is admitted |
| Tuple establishment | `artifacts/envelope.rs`: `every_admitted_kind_establishes_its_tuple`, `a_missing_or_malformed_envelope_member_establishes_no_tuple` (a wrong governing specification and a body without an object content among them), `another_kind_or_schema_revision_string_is_unsupported`, `a_kind_is_declared_only_by_an_object_with_a_string_kind`, `a_failing_integrity_or_content_keeps_the_tuple`. `artifacts/collection.rs`: `what_the_strict_decoder_refuses_is_unrepresentable`. Node: every member's declaration and tuple, and the unrepresentable texts |
| Canonical submitted body | `artifacts/envelope.rs`: `the_canonical_body_ignores_transport_spelling_and_keeps_the_integrity_digest`. `artifacts/collection.rs`: `every_occurrence_is_kept_in_the_order_submitted`. Node: the spellings and the unequal members |
| Keys and bytes | `artifacts/envelope.rs`: `canonical_bodies_order_by_their_unsigned_bytes`, `tuples_order_by_each_part_in_turn`, `a_reference_names_the_tuple_an_envelope_establishes`, `a_reference_has_the_canonical_bytes_of_its_value_whatever_its_spelling`. Node: the order within one tuple and without one, every member's and reference's canonical bytes |
| Integrity and semantic digests | `artifacts/envelope.rs`: `the_integrity_covers_the_member_but_its_own_digest` (a nested `integrityDigest` stays), `the_semantic_digest_pins_kind_specification_revision_and_content_only`, `kinds_are_spelled_governed_and_ordered_as_017_registers_them`. Node: every integrity and semantic digest, and the kind registry |

015's pre-invocation boundary for the collection is in `artifacts/collection.rs`: a missing collection, each limit's exact and first-over value, the limits before any decoding, and a capacity failure before an unrepresentable member. The checks over what these facts say, such as duplicates, conflicts, unsupported kinds, invalid envelopes, and the bounds of a Resource Limit Policy, belong to 015's resolver phases, and the closed contents of the five kinds 017 defines are admitted after the integrity.
