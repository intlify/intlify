# Intlify configuration foundations

Workspace-internal configuration code for [design 015](../../design/015-intlify-project-profile-and-locale-policy-design.md). This crate is unpublished and does not reserve a product configuration or Profile API.

## Current implementation

- The CLI uses the shared duplicate-aware JSON compatibility decoder, byte positions, Draft 7 generation, and deterministic schema formatting.
- The private authoring model covers the complete **015-owned** field vocabulary: named declarations, locale inputs, coverage, policy slots, targets, groups, and delivery.
- Policy and Target Profile reference representations remain generic type parameters. The only current instantiations use explicitly test-owned types.
- Private strict file materialization validates UTF-8, JSON syntax, decoded duplicate keys, Unicode scalars, and Portable JSON Numbers while retaining raw bytes and key/value/container spans.
- An internal schema compiler/evaluator follows the generated Draft 7 subset, retains independent fragment admission, and bounds applicable structural work. An owned deserialization bridge reads the normalized value tree without re-parsing source.
- Outer admission selects configuration version `"0"`, preflights profile count/ID byte bounds, and retains independently admitted fields. Only complete structural success can construct `IntlifyConfig`; the root has no raw `Deserialize` route.
- Private provisional selection handles omission, exact matching, invalid/unknown/over-limit inputs, and unavailable structural prerequisites without choosing from repository layout or a filtered profile map.
- Feature-isolated measurement support retains exact quantities, acquires the reported monotonic-clock resolution, measures four actual core operations, captures raw samples, and validates owner-local method/interval/execution descriptors. It does not yet produce complete Owner Results or common evidence.
- Checked-in expectations bind all 77 finite cases to their preparation/input context, complete result observation, and logical work. Ordinary collection requires an immutable admitted fixture rather than caller-chosen expected results.
- Internal inventory relationship checks distinguish missing evaluation rows from explicit unavailable attempts, enforce planned requirements/bindings, and reject cross-run evidence or unproven non-applicability. They do not issue common Run Plans or admit raw records.
- Locale resolution and the remaining 026 harness/record/validation path are not implemented yet. These internal operations are not a complete configuration resolver or product entry.

This is **not Phase 1 completion**, a complete revision-`"0"` resolver, or a public `LocalizationProjectProfile`.

## Schema gate

Design 017 has not yet fixed the JSON encodings of `PolicyReference` and `TargetProfileReference`. The authoring model must not invent those encodings or replace their validation with an open `serde_json::Value`.

The test fixtures use closed `$testPolicy` / `$testTarget` objects with finite token enums. They exist only under `cfg(test)` or the non-default `benchmark` feature; they are not fallback production inputs, artifact identities, or deployment data. The generated test schema keeps those fixture type names.

Accordingly, this step intentionally does **not** create `schema/project-profile-config-v0.schema.json`, a public project-profile schema generator, or a published schema path. Completing that artifact and its freshness gate requires the admitted shared reference definitions. The existing CLI schema is unchanged.

## Structural rules

The model preserves authoring state rather than resolving semantics.

| Field class | Omission | Explicit null |
| --- | --- | --- |
| Required declarations and mandatory policy references | Rejected | Rejected |
| Optional source default, inline declarations, and optional nested fields | Preserved as absence | Rejected |
| `providerRouting` / `glossarySet` | Rejected | Preserved as explicit absence |

`Presence<T>` and `RequiredNullable<T>` encode these different rules. No locale, policy preset, target default, or delivery policy is inferred by structural deserialization.

Profile, Project, Selection Scope, Target, and Group identities share the exact ASCII syntax while remaining separate Rust types. Non-empty arrays and maps preserve all admitted occurrences; duplicate locales, alias collisions, membership, and group partition checks are later semantic work. A malformed sibling prevents construction of the complete root. The schema evaluator separately records admitted/invalid/type-unavailable/resource-unavailable fragments; a failing sibling does not erase a valid declaration or field.

The legacy JSON helper is **not** the 015 strict materializer. Its serde-based numeric behavior is preserved for CLI compatibility. The new `materialize` module admits numbers as finite binary64 values within magnitude `9007199254740991`, normalizes negative zero, and keeps the unchanged raw token separately.

## Strict file entry and ownership

The file entry takes immutable shared bytes and explicit finite input limits. A bounded iterative lexer/parser first retains a raw token index, scoped duplicate-key indexes, and complete logical counts. String tokens carry decoded byte lengths without allocating their values; only member names needed for duplicate checks are decoded during parsing. Logical node/depth/entry/string limits are checked before a second pass constructs owned value nodes.

The second pass consumes the admitted token index; it does not tokenize input again. Value nodes refer to children by indices, and the source map stores half-open UTF-8 byte coordinates. Deep input and failure cleanup therefore do not recurse through a Rust value tree. Returned documents retain their own values and share only immutable raw source. Parser scratch is invocation-owned and discarded; this step introduces no workspace, arena allocator, global cache, or claimed capacity reuse.

Private resource observations distinguish exact complete totals from an `at-least` token-limit witness. Raw byte/token limits and logical value limits are separate explicit inputs, not policy defaults or a partial formal capability. A logical-limit failure retains complete counts but no materialized document. Bound-centric summaries are not Finding records; final per-occurrence evidence projection remains later work. Profile and selector bounds are applied by the subsequent private admission stages. Error observations never include rejected key or scalar text.

## Internal schema evaluation

The compiler accepts only the keyword/reference/pattern vocabulary explicitly implemented for the generated authoring model. Unrecognized behavior, external or dangling references, cycles, and unsupported dialects fail closed. It is not a general-purpose JSON Schema implementation, and it does not acquire schema bodies or accept `$schema` metadata as authority. The actual reference instantiations remain test-owned.

Evaluation counts one applicable schema-keyword occurrence per logical subject. Annotation keywords are excluded; a wrong type suppresses dependent constraints and descendants, while independent siblings continue in unsigned UTF-8 member order. Every `anyOf` alternative is visited for deterministic accounting. Unmatched alternatives retain contextual observations only when the aggregate fails; they do not create blocking issues for an accepted alternative.

A count-only traversal preflights the complete applicable domain before allocating fragment/issue records. The recording traversal must produce the same work count. Exact limits succeed; an overrun returns the exact complete total and no evaluation prefix. Records are private schema observations, not the final Finding Registry.

Owned authoring deserialization uses the normalized flat tree directly. It preserves positive-zero normalization and admitted binary64 rounding, rejects unconsumed collection tails, and never embeds serde's rejected values/keys in errors. Successful deserialization alone is not proof that the outer admission prerequisites have succeeded.

## Configuration admission and provisional selection

An immutable `AuthoringSchema<Policy, Target>` binds the generated schema to its authoring/reference types. `StructuralAnalysis` owns the materialized document and shares that schema binding. Missing, invalid, or unsupported `schemaVersion` suppresses schema-dependent work; `$schema` is metadata only. Profile count and decoded ID byte lengths are checked before affected descendant validation. Independent sibling checks continue, but any failure withholds the complete root.

`IntlifyConfig` wraps a private field definition used by schema generation and owned decoding. Its constructor needs an internal complete-root proof that only a successful analysis can create. A passing `anyOf` branch at the same node is insufficient for a typed field: the field's exact enclosing schema edge must be admitted. Tests also prevent accidentally adding `Deserialize` to the root.

Selection uses a separate normalized input with an explicit matching bootstrap bound. It accepts absence, a complete bounded Rust string, an over-limit marker, or an invalid top-level JSON type tag. Over-limit values are not copied and retain only the smallest first-over witness, not their final length; invalid containers have no contents in this input. No live-host inspection is implemented here.

Omission uses the original profile count, including malformed declarations. Exact matching needs the admitted version, a bounded non-empty profile container, an admitted declared ID, the selected declaration's immediate object shape, and its independently admitted `resourceLimits` reference. Invalid nested fields or unrelated profiles may leave those prerequisites available, but never produce a partial `IntlifyConfig`. Selection outputs own their selected ID/reference and survive release of the analysis. Unknown or rejected selector text is never exposed in the content-free failure observations.

This is provisional bootstrap selection only. Resource Policy admission/recheck, confirmed selection, final selector Evidence, and checked Profile construction are not implemented or implied.

## Measurement support in progress

The non-default `benchmark` feature isolates the measurement code from ordinary library builds. Unit tests may also compile these helpers. There is no public benchmark facade or runnable benchmark harness yet.

The initial numeric helper follows 026's full `0..=u64::MAX` quantity domain, rather than reusing 015's smaller positive resource-bound domain. JSON values are shortest unsigned decimal strings, including values above JavaScript's safe-integer range. Repetition counts must be positive. A reversed interval, nanosecond conversion overflow, or accumulation overflow is a typed failure, never a zero, saturated, or wrapped sample. Zero duration remains a valid observation rather than being replaced with a fabricated clock minimum.

The measurement-only clock uses the pinned safe `rustix` API on Linux and macOS. It obtains `CLOCK_MONOTONIC` resolution from `clock_getres`, rather than inferring resolution from nanosecond storage or an observed latency. Full raw timestamp subtraction precedes checked conversion to `u64` nanoseconds. Other platforms remain explicitly unsupported by this initial clock provider; ordinary configuration code is unaffected. Neither `rustix` nor the observation codec's `blake3` dependency enters the default normal-dependency graph.

Prepared calls measure strict file materialization, structural analysis, complete authoring-model construction, and provisional profile selection. Dispatch and immutable-handle preparation precede the start marker. The preserved indirect invocation and complete-output black box are inside the interval; observation encoding, validation, and destruction are outside. These unavoidable included costs are declared and never removed through estimated subtraction. Outputs stay owned through the end marker and observation.

The bounded sampler excludes warmup, retains ordered raw samples, and validates every repetition against a separately supplied fixture observation. Per-sample durations are checked sums of separately measured single-invocation intervals, not a continuous workflow duration or a precomputed average. Panic, clock failure, overflow, or unexpected output withholds the complete case; earlier samples are diagnostic prefixes only. Failure-only mismatch payloads keep both complete observations without making ordinary calls carry their large inline storage.

The observation codec covers the complete applicable ordinary output, including normalized values, schema/fragment/issue information, or selected identity/reference and typed failure facts. Shared semantics and entry-specific source facts use separate framed checksum domains. Raw source is observed only for the finite owner-controlled successful materialization fixtures; rejected selector/key/scalar text is not copied into diagnostics. These checksums are not shared-artifact digests, Profile identities, or authenticity proofs.

Owner-local descriptors fix each active phase/cost, interval markers and non-overlap, conversion and aggregation, barrier placement, and independent process/engine/preparation/cache/heap/scratch/output states. A duration method retains 026's `qualified-runner` eligibility class; the eventual observational profile must separately prohibit numeric decisions. Clock resolution remains an acquisition observation, not part of the semantic checksum. Revalidation compares decoded descriptors and samples to separately supplied operation, acquisition, sampling, run/case, and expected-output inputs. A record cannot select its own validation expectations.

`collect_operation` connects the acquired provider, actual core calls, descriptors, logical work, and sampler into a serializable owner fragment. It accepts only an immutable `AdmittedFixture` from the fixed expectation registry; arbitrary prepared calls and expected results are confined to lower-level negative tests. The retained fragment includes the exact fixture-input context, and revalidation requires the admitted fixture separately. Native-clock tests exercise every declared case and reject missing, unknown, mutated, or cross-run/cross-case data, including changed input bounds with identical output and work. The sampler checks logical work independently of the semantic checksum in every repetition; its observation-only test seam is not available in ordinary collection. This fragment is not a complete Owner Result, Measurement Evidence Set, or admitted Run Plan.

The current workload vocabulary has 14 ordered facts covering bytes, visited parser tokens, complete logical values/depth/entries/string bytes, profiles and ID bytes, applicable structural work, retained internal records, and selector bytes. Each fact distinguishes the operation's output from previously prepared input, and records its exact counting unit. Incomplete parsing or unavailable schema prerequisites are not fabricated zero totals. Actual empty retained-record collections may have an exact zero count without claiming that unavailable schema work was zero. Over-limit selectors retain only the admitted first-over witness. These internal record counts are not final Finding/Evidence counts, and later locale/target work is not invented as zero-valued fields.

The finite case catalog declares 77 ordered cases: representative and independently scaled inputs, member permutations, raw-input errors, structural failures, provisional selection outcomes, and exact/first-over pairs for ten input/structural bounds. All data generation and limit preparation are outside measured intervals. The repeated locale fixture is structurally valid authoring input, not a claim that semantic duplicate-locale resolution succeeds. Tests compare applicable unbounded cases to an independent Draft 7 oracle, verify every prepared case's result/work agreement with ordinary calls, and check all 20 exact/first-over edges.

Preparation produces an explicitly unadmitted `Candidate`. The separate `cases/registry` gate admits it against `cases/expectations-v0.json`, whose closed revisioned document contains the exact ordered declaration inventory, fixture-input context, shared/entry result checksums, and a checksum over the complete typed logical-work vector. These are fixed logical expectations, not checked-in timing or memory baselines. They are compiled only with benchmark/test support and require no runtime file access, network, directory scan, or caller-selected registry.

Input-context observation covers the declared recipe, explicit preparation limits, actual operation input, and its schema/bounds or full prepared structural analysis where applicable. A selector must match its finite fixture input without exporting or hashing an arbitrary rejected string; an over-limit marker remains only its normalized first-over witness. Admission re-observes complete ordinary output and work before comparison with the pinned row, so cached candidate summaries cannot certify themselves. Only success creates the immutable token; no measured sample or failed expectation test supplies a new expected result.

The ignored `print_candidate_expectations_for_review` developer test prints candidates to stdout and never writes a file or admits its output. Changes require explicit fixture review, passing independent schema/semantic tests, and the affected input/boundary/observation revision decisions before accepting new expectations. Registry mutation tests cover missing/duplicate/reordered declarations, missing nullable fields, changed inputs/limits/results/work, cached-summary substitution, and identical-output selector changes. Fresh preparation of all 77 declarations must continue to match the pinned data.

The non-serialized `inventory` checker freezes an owner-selected ordered case inventory and verifies submission/evidence relationships against it. Missing, duplicate, unknown, reordered, or weakened inventory/evaluation rows are invalid. All six explicit unavailable kinds retain required/optional accounting: a required unavailable case makes the run incomplete, while an optional unavailable case remains diagnostic. Invalid takes precedence over incomplete and complete. An invalid submitted run exposes no successful-reference prefix, while an incomplete run can retain references to independently successful cases; unavailable diagnostic samples are never consumed here.

Measured references must resolve to the exact run, plan, profile, subject, build, and case. Missing or stale evidence remains distinguishable from corrupt or ambiguously bound evidence. Non-applicability requires the planned rule and a separately checked owner fact bound to the same run/plan/profile/subject/build, never an environment-failure label. Lookup indexes are built once, duplicate references never choose a first/last record, and internal issues have deterministic typed ordering. Native integration tests account for all 77 checked fixture collections, then remove an attempt or make it explicitly unavailable to verify the different outcomes.

This checker consumes views supplied by an enclosing admitted record layer. It does not validate full reason bodies, prove applicability, verify sample content/integrity, invent an identity encoding, issue one globally unique immutable Run Plan per run, or create common evidence. The enclosing source records must retain their complete reasons and diagnostic payloads; the relationship view is not their wire representation. Complete common case identities, Build/Environment acquisition and records, Run Plan issuance and common record admission, projection/report validation, standalone harness, and CI smoke remain unfinished. The internal checks do not by themselves complete those gates or the shared-reference schema gate.

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
| Outer version/profile admission | `structural::admission_tests`: absent/invalid/unsupported version, metadata isolation, exact/first-over count and decoded-ID bounds |
| Complete root construction | Sealed proof and no-`Deserialize` assertion; independent typed siblings, aggregate `anyOf` proof, work-overrun suppression |
| 015 external selector / provisional selection | `structural::selection::tests`: omission, exact match, original-map count, structural prerequisites, explicit bootstrap-bound binding |
| 015-089 / 015-090 / provisional portion of 015-091 | Closed top-level selector types, exact UTF-8 limits, redacted unknown/invalid observations, no retained provisional Evidence |
| 026 invocation isolation | Shared immutable schema/input; complete roots and selections survive producer release; failures do not contaminate repeated calls |
| 026 exact numeric representation | `benchmark::quantity::tests`: JSON precision, decimal canonicality, positive repetitions, reversed clock, exact/first-over duration and accumulation overflow |
| 026 clock and interval boundaries | `benchmark::clock::tests`, `benchmark::measure::tests`: acquired resolution, wide subtraction, first-over conversion, invocation/lifetime markers, panic and failed-clock paths |
| 015 complete semantic observation | `benchmark::operation::tests`: actual four operations, full output mutation, member permutations, separate entry facts, content-free rejected selectors, iterative deep-document encoding |
| 026 raw sample capture | `benchmark::sample::tests`: warmup exclusion, fixed repetitions, diagnostic-only failure prefixes, checked aggregation/capacity, raw ordering, exact run/case/sample binding |
| 026 method and execution descriptors | `benchmark::descriptor::tests`: all descriptor leaves, missing/unknown fields, marker/overlap mutation, actual clock binding, independent reuse states, duration eligibility |
| Connected owner observation | `benchmark::collect::tests`: native-clock capture for all four pairs, serialization and separate-input revalidation, cross-case/run rejection, no successful failure prefix |
| Active logical work vector | `benchmark::work::tests`: complete ordered vocabulary/units/stages, raw parsing versus complete counts, structural first-over totals, retained-record zeroes, private selector witnesses, tamper and per-repetition mismatch checks |
| Finite case catalog and preparation | `benchmark::cases::tests`: 77 unique ordered declarations, independent schema checks, scaled/permuted inputs, 20 exact/first-over edges, exact raw-error kinds, undeclared-case rejection |
| Fixed expected input/result/work | `benchmark::cases::registry::tests`: closed pinned inventory and nullable presence, actual context/result/work revalidation, changed-but-equivalent inputs, secret-safe selector matching, cached-summary and operation substitution |
| Admitted fixture collection | `benchmark::collect::tests`: all 77 pinned cases through native collection/decode/revalidation; changed input context rejected even when result, logical work, run, and case labels coincide |
| 026 run inventory relationships | `benchmark::inventory_tests`: required/optional unavailable kinds, missing/duplicate/unknown/reordered rows, exact run/plan/profile/subject/build/case bindings, scoped applicability proof, absent/stale/corrupt/ambiguous evidence, outcome precedence and canonical internal issue ordering |
| Inventory / owner-collection integration | All 77 pinned native collections accounted for through non-serialized test views; explicit required unavailability is incomplete, a missing attempt is invalid, and diagnostic prefixes never enter successful-reference output |

From the repository root:

```sh
rtk proxy cargo test -p intlify_config --all-targets
rtk proxy cargo test -p intlify_config --all-targets --all-features
rtk proxy cargo test -p intlify_config --doc
rtk proxy cargo check -p intlify_config --lib --no-default-features
rtk proxy cargo clippy -p intlify_config --all-targets --all-features -- -D warnings
rtk proxy cargo test -p intlify_cli --test config --test schema
rtk proxy vp run schema:cli:check
```

No standalone benchmark result, common Measurement Evidence, or full 015/026 conformance claim is made by these tests. PR 3 remains incomplete until the applicable benchmark registry, records, and projection/report checks are implemented and verified. The shared-reference schema gate separately prevents claiming Phase 1 completion.
