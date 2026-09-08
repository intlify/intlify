# Minimum profile-resolution fixture index

This is the implementation review index for the test-owned minimum 015 path. It does not admit a configuration schema, shared artifact, Profile, formal Resolver Outcome, or revision-`"0"` conformance suite.

The tests are in [`src/minimum_tests.rs`](../../src/minimum_tests.rs); [`harness.rs`](../../src/minimum_tests/harness.rs) contains only test orchestration over the ordinary component calls. Shared reference shapes come from the closed test types in [`fixtures.rs`](../../src/fixtures.rs). Locale data comes from the finite [`FixtureProvider`](../../src/locale/fixtures.rs), with an explicit binding and no host fallback. `FixtureLimits::finite()` supplies test-owned input/structural/identifier/active-locale bounds; these are not product defaults or full Resource Limit Policy admission.

## Scope and traceability

The review labels below are local index labels, not common Case IDs. Expected values, exact failure kinds, locations, and related indices are asserted in the tests; no implementation output is regenerated and automatically accepted as an oracle. Input serialization and bounded permutations happen in memory.

| Review label | Test in `minimum_tests` | 015 rule | Required observation |
| --- | --- | --- | --- |
| MIN-001 | `strict_bytes_reach_only_the_selected_private_locale_core` | [Input stages](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#input-stages); [source default](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#source-locale-defaults); [requested default](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#requested-locale-default-resolution) | Strict bytes reach complete configuration, exact selection, canonical requested set and explicit default; absent source stays absent; corrections remain separate |
| MIN-002 | `invalid_raw_inputs_stop_before_structure_selection_and_locale_processing` | [Input stages](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#input-stages) | Invalid UTF-8/JSON, comments/trailing syntax, decoded duplicate member, non-scalar Unicode and non-portable number retain their exact materialization failure; no later stage is called |
| MIN-003 | `root_and_version_failures_stop_before_model_construction` | [Configuration shape](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#version-0-project-profile-configuration-shape) | Wrong root, missing/wrong-type/unsupported version retain exact structural admission reasons; no complete model or later call exists |
| MIN-004 | `incomplete_root_and_invalid_sibling_never_reach_selected_profile_resolution` | [Authoring model](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#intlifyconfig-and-json-schema) | Missing/null/empty required locale fields, invalid policy shape, unknown fields and invalid sibling prevent the whole root from being constructed, even with a valid selected declaration |
| MIN-005 | `named_selection_is_exact_and_failures_never_start_locale_work` | [Profile scope](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#profile-scope-and-identity) | Single-profile omission works; multiple-profile omission and unknown/invalid IDs fail; explicit selection resolves that declaration rather than map order |
| MIN-006 | `semantic_failures_preserve_the_exact_cause_without_a_partial_locale_core` | [Canonicalization](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#locale-identity-and-canonicalization); [requested set](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#requested-locale-set) | Invalid source/requested/default, alias duplicate and default-outside-set preserve exact causes; every duplicate index survives; unsupported fixture coverage is not mislabeled invalid |
| MIN-007 | `file_spelling_and_authoring_permutations_preserve_core_meaning_not_corrections` | [Canonicalization](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#locale-identity-and-canonicalization); [performance observations](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#workloads-and-observations) | Whitespace, LF/CRLF, object order, set order and accepted aliases preserve core value; corrections are separate; source/default/set semantic mutations change core value |
| MIN-008 | `resource_edges_stop_at_their_owning_stage_without_later_semantic_work` | [Resource limits](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#project-profile-resolution-resource-limits) | File-byte and active-occurrence exact/first-over edges and profile-count failure stop at their owning call and retain the actual bound witness; test-owned active counts do not claim the full 015 locale domain |
| MIN-009 | `owned_results_survive_interleaved_failures_input_release_and_runner_release` | [Memory ownership](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#memory-ownership-and-reuse) | Raw, structural and locale failures do not contaminate another call; retained result and structural analysis remain valid after input/context release |
| MIN-010 | `explicit_parallel_callers_keep_fixture_association_and_results_deterministic` | [Memory ownership](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#memory-ownership-and-reuse) | Native caller-created threads can share immutable schema/provider and keep input/result association; the core creates no scheduler or mutable shared workspace. This native scheduling test is not run on Wasm |

The call trace is local test instrumentation, not Finding/Evidence output or a timer. Construction/selection errors that contradict an admitted test prerequisite fail the test rather than becoming fabricated semantic failures.

## 026 verification and remaining gates

These fixtures apply [026 memory-lifetime and reuse requirements](../../../../design/026-intlify-conformance-and-measurement-design.md#memory-lifetime-classes) and normal-build isolation. Compile-fail documentation keeps the test runner inaccessible to consumers. Running the same matrix with and without `benchmark` verifies that benchmark code is not needed to resolve the minimum test input.

Measurement verification remains separate: the [crate's benchmark traceability](../../README.md#verification) covers six active pairs, 127 fixed owner cases, raw collection, checksum, logical work, descriptors, and inventory relationship checks. The vertical slice is not a complete workflow measurement and does not register `profile_resolve_e2e` or peak-memory cases.

The following gates are still required by the minimum implementation plan:

- formal 017-owned reference schemas and generated configuration-schema freshness;
- complete common case/build/environment/record inputs, Run Plan issuance, projection/report and their verification;
- standalone benchmark smoke and its CI integration;
- the remaining minimum fixture matrix, feature-isolation and handoff audit; and
- review/merge evidence for the implementation PRs.

This index does not mark those gates, PR 6, Implementation Phase 1, or the minimum milestone complete. Full Phase 2/3 semantics, production locale data/adapter, public Profile output and the Phase 6 conformance suite remain outside this minimum slice.

From the repository root:

```sh
rtk proxy cargo test -p intlify_config --lib minimum_tests
rtk proxy cargo test -p intlify_config --lib --features benchmark minimum_tests
rtk proxy cargo test -p intlify_config --doc
rtk proxy cargo check -p intlify_config --lib --no-default-features
rtk proxy cargo check -p intlify_config --lib --features benchmark
```
