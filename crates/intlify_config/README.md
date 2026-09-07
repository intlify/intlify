# Intlify configuration foundations

Workspace-internal configuration code for [design 015](../../design/015-intlify-project-profile-and-locale-policy-design.md). This crate is unpublished and does not reserve a product configuration or Profile API.

## Current implementation

- The CLI uses the shared duplicate-aware JSON compatibility decoder, byte positions, Draft 7 generation, and deterministic schema formatting.
- The private authoring model covers the complete **015-owned** field vocabulary: named declarations, locale inputs, coverage, policy slots, targets, groups, and delivery.
- Policy and Target Profile reference representations remain generic type parameters. The only current instantiations use explicitly test-owned types.
- Private strict file materialization validates UTF-8, JSON syntax, decoded duplicate keys, Unicode scalars, and Portable JSON Numbers while retaining raw bytes and key/value/container spans.
- An internal schema compiler/evaluator follows the generated Draft 7 subset, retains independent fragment admission, and bounds applicable structural work. An owned deserialization bridge reads the normalized value tree without re-parsing source.
- The outer configuration-version/profile-bound admission, guarded complete-authoring constructor, selection, locale resolution, and 026 measurement path are not implemented yet. The lower-level schema evaluator is not a complete configuration resolver or product entry.

This is **not Phase 1 completion**, a complete revision-`"0"` resolver, or a public `LocalizationProjectProfile`.

## Schema gate

Design 017 has not yet fixed the JSON encodings of `PolicyReference` and `TargetProfileReference`. The authoring model must not invent those encodings or replace their validation with an open `serde_json::Value`.

The test fixtures use closed `$testPolicy` / `$testTarget` objects with finite token enums. They exist only under `cfg(test)`; they are not fallback production inputs, artifact identities, or deployment data. The generated test schema keeps those fixture type names.

Accordingly, this step intentionally does **not** create `schema/project-profile-config-v0.schema.json`, a public project-profile schema generator, or a published schema path. Completing that artifact and its freshness gate requires the admitted shared reference definitions. The existing CLI schema is unchanged.

## Structural rules

The model preserves authoring state rather than resolving semantics.

| Field class | Omission | Explicit null |
| --- | --- | --- |
| Required declarations and mandatory policy references | Rejected | Rejected |
| Optional source default, inline declarations, and optional nested fields | Preserved as absence | Rejected |
| `providerRouting` / `glossarySet` | Rejected | Preserved as explicit absence |

`Presence<T>` and `RequiredNullable<T>` encode these different rules. No locale, policy preset, target default, or delivery policy is inferred by structural deserialization.

Profile, Project, Selection Scope, Target, and Group identities share the exact ASCII syntax while remaining separate Rust types. Non-empty arrays and maps preserve all admitted occurrences; duplicate locales, alias collisions, membership, and group partition checks are later semantic work. A malformed sibling prevents construction of the complete root. The schema evaluator separately records admitted/invalid/type-unavailable fragments; a failing sibling does not erase a valid declaration or field.

The legacy JSON helper is **not** the 015 strict materializer. Its serde-based numeric behavior is preserved for CLI compatibility. The new `materialize` module admits numbers as finite binary64 values within magnitude `9007199254740991`, normalizes negative zero, and keeps the unchanged raw token separately.

## Strict file entry and ownership

The file entry takes immutable shared bytes and explicit finite input limits. A bounded iterative lexer/parser first retains a raw token index, scoped duplicate-key indexes, and complete logical counts. String tokens carry decoded byte lengths without allocating their values; only member names needed for duplicate checks are decoded during parsing. Logical node/depth/entry/string limits are checked before a second pass constructs owned value nodes.

The second pass consumes the admitted token index; it does not tokenize input again. Value nodes refer to children by indices, and the source map stores half-open UTF-8 byte coordinates. Deep input and failure cleanup therefore do not recurse through a Rust value tree. Returned documents retain their own values and share only immutable raw source. Parser scratch is invocation-owned and discarded; this step introduces no workspace, arena allocator, global cache, or claimed capacity reuse.

Private resource observations distinguish exact complete totals from an `at-least` token-limit witness. Raw byte/token limits and logical value limits are separate explicit inputs, not policy defaults or a partial formal capability. A logical-limit failure retains complete counts but no materialized document. Bound-centric summaries are not Finding records; final per-occurrence evidence projection and the remaining profile/selector limits belong to later admission work. Error observations never include rejected key or scalar text.

## Internal schema evaluation

The compiler accepts only the keyword/reference/pattern vocabulary explicitly implemented for the generated authoring model. Unrecognized behavior, external or dangling references, cycles, and unsupported dialects fail closed. It is not a general-purpose JSON Schema implementation, and it does not acquire schema bodies or accept `$schema` metadata as authority. The actual reference instantiations remain test-owned.

Evaluation counts one applicable schema-keyword occurrence per logical subject. Annotation keywords are excluded; a wrong type suppresses dependent constraints and descendants, while independent siblings continue in unsigned UTF-8 member order. Every `anyOf` alternative is visited for deterministic accounting. Unmatched alternatives retain contextual observations only when the aggregate fails; they do not create blocking issues for an accepted alternative.

A count-only traversal preflights the complete applicable domain before allocating fragment/issue records. The recording traversal must produce the same work count. Exact limits succeed; an overrun returns the exact complete total and no evaluation prefix. Records are private schema observations, not a substitute for version admission, profile/selector limits, typed configuration admission, or the final Finding Registry.

Owned authoring deserialization uses the normalized flat tree directly. It preserves positive-zero normalization and admitted binary64 rounding, rejects unconsumed collection tails, and never embeds serde's rejected values/keys in errors. Successful deserialization alone is not proof that the outer admission prerequisites have succeeded.

## Verification

`src/model_tests.rs` records an explicit inventory of every fixed object's fields and checks positive, negative, omission, null, wrong-type, empty-collection, identity, and sibling-failure fixtures. It compares the internal evaluator, typed deserialization, and the independently compiled Draft 7 schema. The external schema oracle is a pinned dev dependency with HTTP/file retrieval, TLS, and IDNA data disabled; it is not part of the normal dependency graph.

Relevant traceability:

| 015 decision / section | Tests or implementation |
| --- | --- |
| 015-007, existing configuration reuse | `json`, `location`, `schema`; unchanged CLI config/schema tests |
| 015-008, independent configuration version | `root_version_and_shape_are_closed` |
| 015-009 / 015-038, complete closed field vocabulary | `field_inventory_matches_required_optional_and_wrong_type_behavior`, `fixed_objects_reject_unknown_members_at_every_level` |
| Profile Scope and Identity | Distinct identity types; identity syntax and control-character fixtures |
| Required/optional/null distinctions | `every_optional_member_rejects_null`, `all_policy_members_are_required_and_only_two_are_nullable` |
| Coverage selector structure | `coverage_requires_a_mode_and_at_least_one_constrained_dimension` |
| No partial authoring root | `an_invalid_sibling_never_constructs_a_partial_root` |
| Semantic checks remain separate | `semantic_duplicates_and_references_are_not_silently_normalized` |
| Strict file entry / Portable JSON Number | `portable_numbers_use_binary64_values_and_preserve_raw_tokens`, strict syntax / UTF-8 / Unicode / duplicate tests |
| Portable Source Span byte coordinates | `source_map_retains_key_value_container_and_eof_positions`, malformed UTF-8 byte-span tests |
| 015-128 / logical input accounting | Resource-bound parsing, complete aggregate / exact / first-over / ordering tests |
| 026 storage ownership | Owned result isolation, 20,000-level arrays and 10,000-level objects including failure cleanup; no shared mutable workspace or cache |
| Parser / materializer agreement | Finite generated corpus, Unicode escape lengths, and single-byte mutation tests against the independent JSON decoder |
| Schema-guided fragment admission | `bad_sibling_does_not_erase_a_fully_admitted_declaration`, missing-field/source-span and type-prerequisite tests |
| Structural work limit | `work_limit_uses_complete_exact_total_and_returns_no_evaluation_prefix`, order/repeated-invocation tests |
| Generated schema source | All definitions compiled; unsupported keyword/dialect/pattern/reference, cycle and malformed-schema tests |
| Normalized typed input | Complete owned model, negative zero / binary64 rounding, tuple-tail and content-free error tests in `materialize::typed` |

From the repository root:

```sh
rtk proxy cargo test -p intlify_config --all-targets
rtk proxy cargo test -p intlify_config --doc
rtk proxy cargo check -p intlify_config --lib --no-default-features
rtk proxy cargo clippy -p intlify_config --all-targets --all-features -- -D warnings
rtk proxy cargo test -p intlify_cli --test config --test schema
rtk proxy vp run schema:cli:check
```

No runtime benchmark, common Measurement Evidence, or full 015/026 conformance claim is made by these tests. PR 3 remains incomplete until structural admission, selection, and the applicable benchmark projection/report checks are implemented and verified.
