<!-- @license MIT -->

# Handoff vectors

Design 015's [Project Profile Handoff Vector Set](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#project-profile-handoff-vector-set) fixes what a checked profile hands a downstream specification and what that specification must conclude from it. Its [Target Profiles and Deployment Compatibility Groups](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#target-profiles-and-deployment-compatibility-groups) gives 015 the normative Group ID selection vectors and gives design 020 the compiler-transaction selector admission. This directory holds those vectors.

The profiles here are test values: the entry's complete test profile `web` with one group, and the same profile with a second group.

## Running them

```sh
# every unit test, the reproduction of the producer fields included
cargo test -p intlify_config --all-targets --all-features

# print the vectors the resolver produces, to review before replacing the file
cargo test -p intlify_config --lib resolver::handoff::tests::print_vectors_for_review -- --ignored --nocapture
```

## Data files

| File | Contents | Read by |
| --- | --- | --- |
| `group-selection.json` | Twelve vectors for 020: each of the six selector forms (omitted, one Group ID, an unknown Group ID, several selectors, a Target ID, a subset of a group) over a profile with one group and over one with two | `src/resolver/handoff.rs` tests |

Each vector has 015's ten fields, in 015's order:

| Field | Contents |
| --- | --- |
| `suiteRevision`, `manifestKind` | `"0"` and `handoff-vector` |
| `vectorId` | `group-selection/` followed by the profile and the selector form |
| `producerProfileFacts` | Every Deployment Compatibility Group in Group ID order, each member in Target ID order with its checked supported requested-locale subset and its effective default requested locale |
| `producerArtifactBindings` | For a selected group, the Resolved Profile Artifact Bindings its planning consumes (015's Requirement planner row): each member's Target Profile and the Resource Limit Policy, as exact tuples in the tuples' order. `null` when no group is selected, since nothing is projected for planning |
| `consumerInputs` | The transaction's Group ID selectors, and the Target IDs it names when it asks for part of a group |
| `expectedRelationship` | The selected group with every member planned, or a rejection: `selector-required`, `unknown-group-id`, `multiple-selectors`, `target-id-selector`, or `group-subset`. These are 015's rules in its words, not 020's codes |
| `verificationOwner` | `020` |
| `relatedDecisionIds`, `relatedRuleIds` | Decisions 015-034, 015-044, and 015-049; no rules until 015's Check Registry exists |

The comparison is between decoded values, so the repository formatter may lay the file out.

## Who checks what

This crate validates each vector's fields and reproduces its two producer fields from the resolver: `resolver::handoff`'s tests resolve both profiles through the entry and compare the committed file with what they produce. 020 verifies that the producer fields and the consumer inputs lead to the expected relationship, in its own suite. No 020 Finding, plan, or output is part of these vectors, and the selector and the selected group never enter the profile's state.

## Provisional shape

The field contents, the rejection words, and the file's place are provisional. 015's Implementation Phase 6 closes the Conformance Suite's schemas and pins the vectors in its Suite Index; `relatedRuleIds` stays empty until Implementation Phase 5's Check Registry names the rules.
