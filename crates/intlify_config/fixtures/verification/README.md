<!-- @license MIT -->

# Resource Limit Policy Verification Input fixtures

Design 017's [Resource Limit Policy Verification Input](../../../../design/017-intlify-shared-artifact-and-version-admission-design.md#resource-limit-policy-verification-input) is the exact-pin input that design 018's [trusted setup supplies](../../../../design/018-intlify-security-trust-and-provenance-design.md#resource-limit-policy-verification-input). Design 015 admits it at its `resource-limit-policy-verification-input` boundary, whose ten reasons and their order its [Resolver Input Admission Case manifest](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#resolver-input-admission-case-manifest) closes. Design 017's [Profile resolution input admission and fixtures](../../../../design/017-intlify-shared-artifact-and-version-admission-design.md#profile-resolution-input-admission-and-fixtures) asks for independent expectations in its Verification input row. This file maps each one to where it is checked.

The inputs here are test values. Their pins name test Resource Limit Policies, and none of them is an input trusted setup supplies.

## Running them

```sh
# every unit test
cargo test -p intlify_config --all-targets --all-features

# the committed vectors, and the independent Node check of them
vp run vectors:verification:check
```

## Data files

| File | Contents | Read by |
| --- | --- | --- |
| `vectors.json` | Five sealed inputs with their integrity and semantic preimages and semantic digests, the pairs whose digests must agree or differ, and forty-eight raw inputs, each with its bounds, the assertion it is compared with, and the outcome of the boundary | `src/artifacts/vectors/verification.rs` tests; `tools/shared-json-vectors` |

The digests cover values, not bytes, so the repository formatter may lay the file out. A raw input is a string, so its exact text stays as written, a repeated member included.

The Node checker shares no code with this crate. It removes the integrity member itself, rebuilds the semantic preimage, and frames and hashes both with its own implementation. It reads every raw input in the boundary's order: whether there is one, its bytes and depth before anything is decoded, its syntax, its pin count, its closed shape, a repeated member, a member outside the shape that 018 does not reserve, a reserved secret-bearing member, a reserved ambient-authority member, its integrity, its mode, its pin rules, and the asserted identity, revision, and semantic digest. Each raw input is admitted or refused at the first check it fails, and the checker compares that check and its reason with the committed outcome.

## The boundary list

| Check | Raw inputs |
| --- | --- |
| Admitted | `admitted`, and each bound at its exact value: `bytes-exact`, `depth-exact`, `pins-exact` |
| Presence | `missing` |
| Bounds | `bytes-first-over`, `depth-first-over`, `pins-first-over`, each with the other bounds at their largest, and `brackets-in-a-string`, which brackets inside a string leave at the exact depth |
| Syntax and shape | `syntax`, `another-kind`, `identity-not-a-token`, `pin-of-a-target-profile` |
| Members | `repeated-member`, `repeated-member-spelled-otherwise` (a name with an escape), `repeated-member-in-a-pin`, `unknown-member`, `unknown-member-in-a-pin`, `private-key`, `credential-in-a-pin`, `secret-locator`, `trust-store`, `network-source-in-a-pin`, `host-trust`, `reserved-before-integrity` |
| Integrity, mode, and pins | `stale-integrity`, `another-mode`, `no-pin`, `unsorted-pins`, `repeated-pin`, `trust-policy-pin` |
| Assertion | `identity-mismatch`, `revision-mismatch`, `digest-mismatch` |
| Order | Two neighbouring checks failing at once, the earlier deciding, for every pair from the bytes to the semantic digest: `bytes-before-depth`, `depth-before-syntax`, `syntax-before-pin-count`, `pin-count-before-shape`, `shape-before-a-repeated-member`, `repeated-before-unknown`, `unknown-before-reserved`, `secret-bearing-before-ambient-authority`, `ambient-authority-before-integrity`, `integrity-before-mode`, `mode-before-pins`, `pins-before-the-assertion`, `identity-before-revision`, `revision-before-digest` |

## The matrix

The unit tests are at the end of each source file, named here by their function.

| 017 requirement | Checked by |
| --- | --- |
| Frozen integrity and semantic-digest preimages and digests | `artifacts/vectors/verification.rs`: `the_committed_vectors_are_the_ones_this_crate_writes`. `artifacts/verification.rs`: `the_integrity_covers_everything_but_its_own_member`, `the_semantic_digest_names_the_pins_and_not_the_input`. Node: both digests recomputed from each input |
| A new identity or revision keeping the semantic digest while one changed pin alters it | `artifacts/vectors/verification.rs`: `the_semantic_digest_pins_the_pins_and_not_the_name`. Node: the `sameSemanticDigest` and `differentSemanticDigest` pairs |
| Each reader step's failure, including empty, unsorted, repeated, and non-`resource-limit-policy` pins, another mode, a reserved member, and an unknown member | `boundary/verification.rs`: `what_is_not_the_closed_shape_is_malformed`, `a_repeated_member_comes_after_the_shape_and_before_an_unknown_one`, `a_member_outside_the_shape_is_unknown_wherever_it_is`, `a_reserved_member_is_refused_after_an_unknown_one`, `the_integrity_mode_and_pins_come_after_the_reserved_members`, `the_assertion_is_compared_last`. `artifacts/vectors/verification.rs`: `every_boundary_vector_reaches_the_outcome_it_states`, `the_boundary_vectors_reach_every_check`. Node: the first failing check of every raw input |
| Exact-bound and first-over cases for each limit | `boundary/verification.rs`: `each_bound_admits_its_exact_value_and_refuses_the_next`, `the_bounds_come_where_015_puts_them`, `the_capability_bounds_the_input`. Node: the six bound cases |

The bounds are the Admitted Implementation Capability's `bootstrapOnly.inputBoundary` bytes, depth, and pin count. A vector states its own, so a bound case needs no capability; `the_capability_bounds_the_input` checks that the boundary takes them from the capability.
