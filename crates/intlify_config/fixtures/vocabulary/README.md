<!-- @license MIT -->

# Intent Surface-Class Vocabulary fixtures

Design 017's [Profile resolution input admission and fixtures](../../../../design/017-intlify-shared-artifact-and-version-admission-design.md#profile-resolution-input-admission-and-fixtures) asks for a closed Draft 7 schema of the vocabulary artifact and for independent expectations in its Vocabulary row. This file maps each one to where it is checked, so a reader can go from a requirement to the code that pins it.

The vocabularies here are test values. Their members are the surface classes the coverage tests use, and none of them is a vocabulary any project admits.

## Running them

```sh
# every unit test
cargo test -p intlify_config --all-targets --all-features

# the committed schema and vectors, and the independent Node check of the vectors
vp run schema:vocabulary:check
vp run vectors:vocabulary:check
```

## Data files

| File | Contents | Read by |
| --- | --- | --- |
| `vectors.json` | Five sealed vocabularies with their integrity and semantic preimages and semantic digests, the pairs whose digests must agree or differ, and four sealed artifacts that must each fail one of the reader's steps 3 to 6 | `src/artifacts/vectors/vocabulary.rs`, `src/artifacts/vocabulary.rs` tests; `tools/shared-json-vectors` |
| [`../../schema/intent-surface-class-vocabulary-v0.schema.json`](../../schema/intent-surface-class-vocabulary-v0.schema.json) | The closed Draft 7 schema of the artifact | `src/artifacts/vocabulary.rs` tests |

The vectors' digests cover values, not bytes, so the repository formatter may lay the file out. The Node checker shares no code with this crate: it removes the integrity member itself, rebuilds the semantic preimage from the artifact, frames and hashes with its own implementation, and decides for every refused artifact which step fails first.

## The matrix

The unit tests are at the end of each source file, named here by their function.

| 017 requirement | Checked by |
| --- | --- |
| Frozen integrity and semantic-digest preimages and digests | `artifacts/vectors/vocabulary.rs`: `the_committed_vectors_are_the_ones_this_crate_writes`, `the_digests_are_the_vectors_an_independent_implementation_checked`. `artifacts/vocabulary.rs`: `the_preimages_are_the_artifact_without_its_digest_and_the_members_alone`, `the_integrity_covers_everything_but_its_own_member`. Node: both digests recomputed from each artifact |
| A new identity or revision keeping the semantic digest while one changed member alters it | `artifacts/vocabulary.rs`: `the_semantic_digest_names_the_members_and_nothing_else`. Node: the `sameSemanticDigest` and `differentSemanticDigest` pairs |
| An empty vocabulary | `artifacts/vocabulary.rs`: `an_intact_vocabulary_keeps_its_name_and_members`. Node: the `empty` vocabulary |
| Each reader step's failure | `artifacts/vocabulary.rs`: `an_absent_artifact_is_missing_input`, `the_boundary_refuses_what_is_not_the_closed_artifact`, `a_repeated_member_comes_after_the_shape_and_an_unknown_one_last`, `a_member_that_is_not_an_identity_token_passes_the_boundary`, `each_boundary_failure_has_its_015_reason`, `each_reader_step_fails_in_017s_order` (a member that is not an `IdentityToken`, a repeat that is not adjacent, and unsorted unique members). `artifacts/vectors/vocabulary.rs`: `every_refused_vector_fails_where_the_independent_implementation_says`. Node: the first failing step of every refused artifact |
| Exact-bound and first-over cases for each limit | `artifacts/vocabulary.rs`: `each_limit_admits_its_exact_value_and_refuses_the_next`, `member_limits_count_whatever_the_members_are`, `a_member_over_its_own_limit_is_reported_before_the_total`, `limits_come_before_anything_is_decoded` |
| The closed Draft 7 schema | `artifacts/vocabulary.rs`: `the_committed_schema_matches_the_current_type`, `the_schema_is_closed_and_pins_one_kind_and_revision`, `the_schema_refuses_the_boundary_shapes_and_an_invalid_member_but_no_repeat_or_order` |

Design 015's Phase 5 admits the artifact against the `intent-surface-class-vocabulary` slot. Its checks are in `resolver/vocabulary.rs`: the reader's failures and their reasons, the comparison by identity, revision, and semantic digest, and what waits on the slot or on the Resource Limit Policy.
