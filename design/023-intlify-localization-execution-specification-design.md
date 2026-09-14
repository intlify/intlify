# Intlify Localization Execution Specification Design

## Purpose

This design defines the smallest language-neutral execution behavior needed by [028](./028-intlify-javascript-web-vertical-slice-design.md)'s generated Web application. It answers what happens after the compiler has selected a message: which values the message accepts, how its text is produced, which locale and direction apply, and what happens when a call cannot succeed.

[021](./021-intlify-translation-store-and-governance-design.md) and [022](./022-intlify-provider-and-localization-sync-design.md) supply validated translation candidates and separate governance. [020](./020-intlify-requirement-planning-and-linking-design.md) chooses exact definitions. Execution receives those choices through admitted target artifacts; it does not acquire translations, select another candidate, or search another locale.

For example, the selected Japanese message `こんにちは、{$name}!` and an admitted string value `Ada` produce Japanese presentation text with that value inserted and isolated for bidirectional display. A Runtime evaluator and generated ahead-of-time code must produce the same exact characters and failure classifications. Merely looking the same in a screenshot is insufficient.

| Design | Question answered |
| --- | --- |
| 023, this subset | What must evaluating an admitted text/interpolation message mean on every participating target? |
| 024 | How are those semantics represented in target artifacts, generated calls, capabilities, and host bindings? |
| 027 | How does the reference Runtime physically prepare messages, own Localizers, and reuse execution state? |
| 028 | Does the generated application preserve host behavior and render equivalent results through both physical paths? |

The initial deliverable is a strict, typed, plain-text execution profile for literals and external string interpolation. It is not the complete MF2 formatter, the full portable Message Value family, a public Runtime API, or a requirement that every target use the same engine.

## Goals

- Define one observable text/interpolation behavior shared by Runtime-backed and ahead-of-time execution.
- Reuse actual MF2 parsing and semantic facts instead of introducing another placeholder language.
- Admit external text values without implicit host conversion, code execution, or silent Unicode replacement.
- Preserve exact text, parameter occurrence order, and bidirectional-isolation characters.
- Separate immutable locale binding, selected-definition evaluation, argument failures, and artifact/setup failures.
- Make output and resource-limit failures complete for the affected call and independent of cache state.
- Supply finite conformance cases and performance obligations that 024/027 can implement before broader features.
- Keep declaration/selector/function extensions possible without making them prerequisites for 028.

## Non-Goals

- Source recognition, Intent identity, requirement planning, translation acquisition/governance, or runtime message-locale fallback.
- Numeric/date/plural formatting, MF2 declarations or matchers, custom functions, options/attributes, rich markup, or a public structured-parts API.
- General JavaScript coercion, arbitrary host objects, executable value resolvers, lazy parameter callbacks, or runtime MF2 source arguments.
- Public function/package names, a compiled-message wire format, Runtime IR, generated-handle ABI, or FFI ownership layout.
- Automatic locale negotiation, OS/browser locale inference, framework reactivity, SSR/hydration, or DOM lifecycle management.
- Full Unicode MessageFormat conformance, all default functions, or all broader execution cases listed in 026/027.
- Provider, Store, Release-publication, or deployment authority in production formatting.

## Ownership and Dependencies

| Owner | Responsibility in this subset |
| --- | --- |
| 000 | Shared logical execution behavior with potentially different physical engines; source-first and build/execution separation |
| 002/012 | MF2 syntax, cooked names/text, source-backed semantic construction, and parser-owned validation |
| 015 | Canonical locale facts and applicable supplied policy inputs; negotiation remains outside single-message evaluation |
| 016/017 | Checked source meaning, parameter names and references, Intent revision, shared artifact associations, and version admission |
| 018 | Artifact/value input trust, disclosure-safe diagnostics, and applicable resource authority |
| 019 | Common Diagnostic and dependency projection |
| 020 | Exact selected source/localized definition and requested-locale placement |
| 021/022 | Candidate semantic compatibility and provenance before execution artifacts are produced; no supply operation is invoked here |
| 023 | Supported evaluation profile, portable text/arguments, format-context meaning, exact text/bidi result, failure classes, and logical resource accounting |
| 024 | Capability admission, target encodings, generated references, host-value/result projection, and source-lowering obligations |
| 025 | Release compatibility, publication/activation handoff, and execution-admission coordination |
| 026 | Conformance, Logical Render Equivalence, measurement, storage lifetime, and evidence admission |
| 027 | Reference preparation/evaluation, Localizer construction, loaders, caches, and internal physical representation |
| 028/029 | Actual Web execution pair, trusted local integration, host behavior preservation, and workflow/packaging |

023 fixes logical inputs and outcomes, not a second artifact format or a competing parser. A generated or target-native evaluator conforms through the same applicable observations; it need not instantiate 027's component classes.

## Standards Basis and Minimum Capability

The processing reference is the published [Unicode MessageFormat specification in LDML 48 / UTS #35 revision 76](https://www.unicode.org/reports/tr35/tr35-76/tr35-messageFormat.html). This document selects a deliberately smaller Intlify execution profile. Its typed-call restrictions are Intlify admission rules, not claims that otherwise valid MF2 is invalid syntax. Adoption records the exact parser, semantic, and execution-profile revisions; it does not silently upgrade existing parser or artifact specifications.

The logical capability is called the **minimum text-interpolation profile** below. Its eventual encoded identity/revision is adopted through 017/024, not inferred from a crate version or a filename. Every claim identifies this exact profile and its finite cases; supporting it does not advertise the complete MF2 function or error-recovery surface.

| Dimension | Initial supported behavior |
| --- | --- |
| Message structure | One simple or quoted pattern, with no MF2 declarations or matcher |
| Pattern content | Decoded literal text and unannotated external variable expressions such as `{$name}` |
| Text preservation | Unicode scalar sequences; exact cooked text, whitespace, escapes, and repeated occurrences |
| Arguments | A complete, finite, duplicate-free set of required canonical parameter names, each bound to one portable Text value |
| Variable behavior | Resolve the already admitted text value; no inferred number/date function, host stringification, or custom resolver |
| Output | One exact plain-text result; diagnostic information is separate, not inserted into the message |
| Bidi | Default Unicode MF2 isolation with unknown-direction interpolated text; no per-expression direction override or opt-out |
| Locale | Exact supplied requested and definition locales; the 028 direct-only profile requires equality |
| Locale services | No locale-service calls or locale-data payload for this subset; absence is explicit |
| Failures | Strict argument/setup/resource failures with no successful presentation value; broader recoverable MF2 evaluation is not adopted |

The allowlist is closed. Function annotations, including explicit `:string`, literal-operand expressions, declarations, selectors, markup, options, attributes, and unknown node kinds are unsupported by this first execution profile. A shared parser may correctly accept them; 024 then reports an unsupported capability before output publication. Existing 021's bounded candidate validation and this target capability check remain distinct operations.

A reusable JavaScript declaration using an `mf2` tagged template, as in 028, is host authoring syntax, not an MF2 `.local` declaration. Reusing that host descriptor is supported when its actual message body fits this profile.

## Terminology

| Term | Meaning here |
| --- | --- |
| Selected definition | One exact source or localized artifact already chosen for an Intent revision and requested locale by 020 |
| Text | An immutable logical sequence of Unicode scalar values; not an arbitrary object with a string conversion method |
| Parameter specification | The exact finite canonical external-name set and required Text kind derived from admitted message facts |
| Admitted arguments | Values that passed the parameter and resource checks for this exact selected definition/profile |
| Format context | Immutable requested/definition locale, message direction, execution profile, explicit locale-service absence, and applicable limits |
| Ready message | A semantically admitted selected message with all preparation dependencies available for synchronous formatting; not a prescribed Runtime type |
| Presentation text | The final logical character sequence, including any inserted isolation controls; not HTML, MF2 source, or a translation lookup key |
| Logical observation | Success/failure, exact text or reason/subjects, bound semantic context, and applicable deterministic work counts used for conformance |

017/024 own shared encodings and 027 owns runtime storage. These logical terms do not reserve a public `MessageValue` union, Rust struct, JSON envelope, or exception class.

## Minimum Inputs and Supported Context

Each logical operation consumes immutable admitted values and its exact supported profile/specification revisions. An artifact digest, capability label, or fixture name alone is not the actual message or proof of compatibility.

| Input | Minimum content |
| --- | --- |
| Definition and association | Actual checked selected message, exact source/localized ArtifactDigest and Intent revision, definition locale, and its admitted requested-locale association |
| Semantic facts | Actual diagnostic-free parse, semantic model/validation, cooked pattern content, and external-name facts, or an owner-admitted equivalent target representation with verified derivation |
| Execution profile | This exact text/interpolation subset, plain-text output, strict typed arguments, bidi behavior, and explicit unsupported features |
| Context | Exact canonical requested/definition locales; message direction as `LTR`, `RTL`, or `unknown`; explicit locale-service absence |
| Arguments | For a formatting call, finite canonical-name/Text entries or an equivalent checked slot projection preserving the same logical map |
| Limits | Explicit finite message/semantic, parameter/name/value-byte, work, output-byte, diagnostic, scratch, and retained-state capacities |
| Integration admission | The relevant 024/025 checked target, handle, output-set, and Release associations before a generated call reaches evaluation |

For 028's `en` and `ja` direct definitions, this profile requires the integration host to supply `LTR` message direction for both. These are explicit fixture/target inputs, not hard-coded locale defaults, a new 015 configuration field, or a text-based direction detector. Direction belongs to the selected message's context, not the current argument's content.

The logical profile accepts all three supplied message-direction values without querying a platform service. Its argument Text values have unknown direction, so the same isolation rule applies in each case. Broader direction-bearing values or expression overrides require a separately adopted profile.

The first core tests may use closed test-owned associations, but the Web path must use actual 020 selections and 017/024/025 admission. A private fixture cannot supply a prechecked flag to stand in for a missing parser, artifact, or capability validator.

## Design Overview

| Step | Required behavior | Not part of this step |
| --- | --- | --- |
| 1. Admit the selected message | Validate actual semantics, parameter facts, supported nodes, locale/context association, profile, and limits | Source discovery, translation selection, or Provider acquisition |
| 2. Establish readiness | Prepare or admit reusable immutable evaluation state and prove every required dependency is available | Requiring one physical AST, bytecode, or Runtime implementation |
| 3. Admit call arguments | Verify exact names, Text kind, Unicode validity, and finite capacities after host expressions have been evaluated | Calling getters/resolvers or coercing arbitrary objects |
| 4. Evaluate and isolate | Traverse the selected pattern in logical order and construct the exact text under the admitted bidi rule | Parsing argument strings as MF2 or searching another locale |
| 5. Return or fail completely | Return the complete presentation value, or a typed failure with no successful partial value | Logging, UI mutation, automatic fallback, or rollback of unrelated host statements |

Loading/readiness may be asynchronous in 027/028. Once ready, argument admission and evaluation are synchronous regardless of cache state. They do not initiate I/O or return a promise on a cache miss. A not-ready message is an explicit setup failure until the host completes the separate readiness operation.

## Selected-Message Admission and Preparation

The semantic admission procedure is:

1. Verify exact definition/context/profile association and finite input limits using the owning checks. A direct-only invocation rejects a requested/definition locale mismatch rather than invoking locale fallback.
2. Follow 012's parse → semantic construction → semantic validation gates for source-backed input. Preserve parser-owned errors and distinguish operational invariant failures. Equivalent compiled input needs actual 024 representation/derivation admission, not reinterpretation of a Binary AST snapshot as executable state.
3. Check every node against the supported pattern allowlist. Do not remove unsupported attributes, fold an unsupported expression to a guessed constant, or reinterpret a function as plain interpolation.
4. Derive the duplicate-free canonical external-name set and required Text kind. Compare it with the actual parameter specification and source/localized compatibility facts; do not trust caller-supplied lists alone.
5. Retain ordered decoded text runs and variable occurrences, their canonical source associations for diagnostics, and complete context/profile dependencies. The logical sequence merges adjacent literal runs and omits empty literal runs; separate variable occurrences remain distinguishable. A different physical segmentation cannot change that sequence's logical observations or resource accounting.
6. Produce a complete immutable ready result or fail without registering a partially usable message. It must own or safely share all storage needed after caller scratch resets.

Canonical MF2 names come from the shared parser/semantic rules used by 016/017. The initial core receives those canonical names without the `$` sigil and does not introduce another normalization rule. Literal message text and runtime Text values never inherit name normalization.

Logical preparation is required; a particular preparation API is not. AOT may discharge it at build time, while a reference Runtime may perform it during artifact admission. Both retain the necessary associations and reject unsupported or mismatched inputs before evaluating a supposedly ready message.

## Portable Text and Parameter Admission

### Text values

Text contains Unicode scalar values, including empty strings, supplementary-plane characters, combining marks, NUL, and existing directional controls. Preserve their exact sequence. Do not trim, dedent, normalize, replace characters, interpret escape sequences again, or parse Text as MF2/HTML.

The logical text value has no callbacks, locale selector, direction override, lazy iterator, or function registry. Host bindings must establish immutable text for the call. Rust may borrow valid immutable strings for the call's lifetime; JavaScript may retain primitive strings; neither physical choice changes the logical value.

For 028, only primitive JavaScript strings convert to Text. Reject numbers, booleans, null-like values, `undefined`, boxed strings, arrays, objects, symbols, and functions. Never use `String(value)`, `toString`, `valueOf`, a JSON conversion, or a locale-sensitive conversion to make them fit. An unpaired UTF-16 surrogate is a host-value admission failure; replacing it with U+FFFD is not a conforming conversion. Exact JavaScript entry-point mechanics belong to 024/028.

### Argument map

The logical call receives a finite map with exactly the required external names. Names are canonical names established under 016/017; a raw or noncanonical external name must not be silently accepted as a different identity. A host adapter accepting named data must use the owning name-admission rules before producing this map. Generated slot calls instead prove their exact parameter-specification association and arity.

Reject duplicate names, missing required names, unexpected names, and non-Text values. An absent entry, a present null-like/undefined value, and a present empty Text are distinct: the first two fail for different reasons, while empty Text is valid. A parameterless message requires the empty map; omission of a host argument object is permitted only when its binding maps it to that empty set.

Count submitted entries/name/value bytes before deduplication or optional projection. A decoder cannot overwrite duplicate names before the validator sees them. Extra values are not silently discarded, and a slot mismatch cannot fall back to name-based guessing.

The minimum logical validator is deterministic and fail-fast by category: input/profile/resource admission; invalid or duplicate names; missing names; unexpected names; incompatible Text values; then evaluation. Within a category, choose the first safely admitted subject in unsigned UTF-8 canonical-name order, retaining the owning reason. Raw decoding/host-conversion failures retain their own stage and input position; they are not reordered by evaluating more host code. No successful text is returned for any failed category.

### Host evaluation order

023 consumes values, not host expressions. The 024/028 lowering evaluates every source parameter expression once, in original JavaScript property/argument order and at the same control-flow point, before entering argument validation. Generated slots may reorder already evaluated immutable values, not their evaluation or side effects. A translated pattern that repeats or reorders parameters does not repeat or reorder host calls.

An exception while evaluating application code remains a host exception and preserves that language's evaluation order. A later localization type error does not justify skipping earlier argument-object evaluation or executing later source statements. Neither the Runtime nor AOT path receives an expression callback to run during interpolation.

## Evaluation and Exact Text Output

After complete argument admission, evaluate the one selected pattern in its logical order:

1. Start with an empty logical result.
2. Append every literal run's exact decoded characters.
3. For each unannotated variable occurrence, resolve its canonical name to the already admitted Text value and append its bidi-isolated representation defined below.
4. Return the complete result with no formatting diagnostics in this successful subset.

No function dispatch or general host conversion is needed for this implicit text behavior. It follows the adopted MF2 variable-resolution path for the supported value family without claiming that every bare variable in all MF2 profiles is a string. Explicit functions remain unsupported in this minimum.

Repeated occurrences use the same immutable admitted value but contribute text and isolation separately at each occurrence. Literal braces produced by source escaping remain literal braces; braces, dollar signs, backslashes, or apparent function syntax inside argument Text are never reparsed. Equal message text at distinct Intent/artifact identities does not merge those identities.

No output is HTML-escaped or treated as markup by the core. 028 writes presentation text to `textContent`; 024 must quote/escape generated JavaScript literal data safely. Output encoding for another sink is an adapter responsibility and cannot change the core's logical text observation silently.

## Locale, Direction, and Bidirectional Isolation

Requested locale identifies the admitted application/Localizer view. Definition locale identifies the already selected message's language context. They remain separately retained even though this initial direct-only profile requires equality. A future fallback profile may permit different values only through actual 020 selection and 024 admission; the evaluator never searches a second locale.

Two ready contexts for `en` and `ja` remain usable concurrently. Interleaving them does not change either context, prepared message, or argument. A locale transition creates or chooses another admitted immutable context after readiness; no process-global or module-global mutable locale is part of this specification.

The profile adopts the [Unicode MF2 Default Bidi Strategy](https://www.unicode.org/reports/tr35/tr35-76/tr35-messageFormat.html#handling-bidirectional-text). Its plain Text values have direction `unknown`; each interpolation therefore emits U+2068 FIRST STRONG ISOLATE, the unchanged value, then U+2069 POP DIRECTIONAL ISOLATE. Literal text is unchanged. No character-content inspection or locale-derived guess changes an argument's direction.

For this profile, that rule also applies to empty Text, repeated occurrences, ASCII names, and values already containing directional controls. Do not omit controls because both locales are LTR or because the output looks unchanged. Per-expression `u:dir`, known-direction values, alternate strategies, and no-isolation modes require a later capability.

The example notation below spells invisible controls as escapes for readability. The actual output contains the corresponding code points, not six-character escape strings:

| Selected MF2           | Argument   | Exact presentation text in escaped notation |
| ---------------------- | ---------- | ------------------------------------------- |
| `Save`                 | Empty map  | `Save`                                      |
| `保存`                 | Empty map  | `保存`                                      |
| `Hello {$name}!`       | Text `Ada` | `Hello \u2068Ada\u2069!`                    |
| `こんにちは、{$name}!` | Text `Ada` | `こんにちは、\u2068Ada\u2069!`              |
| `Hello {$name}!`       | Empty Text | `Hello \u2068\u2069!`                       |
| `{$name}/{$name}`      | Text `Ada` | `\u2068Ada\u2069/\u2068Ada\u2069`           |

028's readable display table remains a human-facing illustration. Its actual oracle must include these characters in `textContent` and conformance observations; stripping bidi controls before comparing the execution pair is not allowed. The host supplies the message/container direction where its presentation model needs it. This document does not implement the browser's Unicode layout algorithm or claim that isolation sanitizes hostile text or prevents every visual-spoofing attack.

This subset calls no number/date/plural, timezone, or direction-discovery service and requires no bundled CLDR/ICU payload. The absence of a locale service is an explicit capability fact with exact output and no platform-managed variation allowance. Existing parser name normalization and 015 canonicalization retain their own recorded dependencies outside the repeated text-evaluation path.

## Outcomes, Diagnostics, and Fallback Separation

The minimum logical result is either complete success with exact presentation text and no evaluation diagnostics, or failure with an owning stage, reason, and safely established subjects. Selected-definition and context identity remain associated with the observation. A host may expose a string-returning helper and a typed exception/result projection, but 024 must preserve the same logical distinction and 026 evidence.

| Failure family | Required behavior |
| --- | --- |
| Invalid source/semantic input | Preserve parser/semantic ownership; no ready message from invalid MF2 |
| Unsupported execution feature/profile | Capability failure, not an invented syntax error or stripped feature |
| Invalid context/not ready | Setup failure; no implicit loading, locale default, or global-context lookup |
| Missing/mismatched handle, artifact, or Release | 024/025 admission/integration failure; no source text, alternate handle, or second-locale fallback |
| Invalid/duplicate/missing/unexpected argument name | Typed argument failure with the deterministic subject/category rules above |
| Incompatible value/invalid Unicode | Typed host-value or portable-value failure; no coercion, replacement, or source fallback |
| Resource exhaustion/cancellation | No successful partial text or truncated diagnostic-success result; retained prior results remain valid |
| Violated ready-message invariant | Operational implementation/integrity failure, not a user MF2 resolution error invented to hide the defect |

Use [019's minimum Diagnostic projection](./019-intlify-project-graph-query-and-incremental-design.md#minimum-common-diagnostic-projection). Retain exact semantic subjects, canonical source/occurrence association when available, and stable owning reason. Do not fabricate a source span for a source-less admitted target representation. Localized explanations, exceptions, or optional sinks must not change the reason, blocking effect, or call outcome.

Argument values and raw host exception text are not default diagnostic/profiler fields. Return diagnostics to the caller; do not log, invoke an untrusted callback, mutate the UI, or embed an error string in presentation text from inside the evaluator. A diagnostic sink is an adapter concern after the outcome is established.

MF2 expression fallback values, message-locale fallback, and application error presentation are different operations. Unicode MF2 defines recoverable evaluation behavior for its broader formatting surface; this initial strict typed entry rejects incomplete/incompatible input before expression evaluation and does not claim that broader error-recovery surface. Since its admitted variables are all present Text values and it has no fallible functions, there is no ordinary recoverable expression failure after successful admission. A future lower-level/recovering formatter must implement the applicable [MF2 error and fallback rules](https://www.unicode.org/reports/tr35/tr35-76/tr35-messageFormat.html#error-handling) explicitly, not reinterpret these argument failures as empty text or successful source-language output.

Call completeness applies to the affected formatting operation, not an entire render transaction. On a failed call, no partial message is returned or assigned by its generated helper. Earlier independent DOM writes or application side effects are not rolled back by 023.

## Resource Accounting and Output Ownership

All active operations receive explicit finite capacities. Missing limits do not mean unlimited work. Bounds apply before costly allocation where possible and during bounded work otherwise; checked arithmetic precedes multiplication, expansion, and capacity reservation.

| Quantity | Logical accounting in this subset |
| --- | --- |
| Submitted arguments | Count every submitted entry and its name/value bytes before duplicate elimination or rejection; invalid raw encodings have separate host-input bounds |
| Parameter/value size | Canonical UTF-8 bytes for admitted names and Text, with per-value and aggregate limits; count a map value once here even if referenced repeatedly |
| Pattern size | Admitted nonempty merged literal runs plus every variable occurrence; preparation also bounds the complete submitted semantic input |
| Evaluation work | Required parameter count plus literal-run count plus variable-occurrence count for a fully admitted call; physical optimization cannot change this logical charge |
| Output size | UTF-8 bytes of decoded literal runs plus every expanded Text occurrence plus six bytes per interpolation for FSI/PDI |
| Scratch/retention | Explicit physical workspace, prepared-state, cache-entry/residency, and retained-diagnostic limits under their owners |

Name ordering and equivalent pattern normalization must be established before evaluation accounting. UTF-8 output bytes are the portable output-size limit, not JavaScript string length, grapheme count, or a memory-allocation claim. 024 separately bounds UTF-16/FFI representation and checked conversion sizes; 026 records physical memory in its declared domain.

The initial implementation should calculate the exact output byte length from immutable admitted values before constructing the result. Each occurrence contributes even when it shares a parameter slot. Logical evaluation work is a precomputed charge from those admitted inputs, not a count of the physical instructions an optimizer happened to execute. An earlier admission failure does not claim a completed evaluation charge. Reject the first-over limit or arithmetic overflow before exposing output. AOT constant folding and Runtime caching must still charge the same logical work/output limits; compile-time knowledge is not permission to skip current call limits.

An owned-result API returns output independent of resettable scratch. A reusable-output API is also permitted: it preserves the caller's previous prefix on failure, exposes no partial append, and does not return a borrowed result that survives reset incorrectly. Its complete append length is the logical output size, while the whole buffer remains subject to the caller/adapter's physical capacity. These are ownership guarantees, not prescribed method names.

Reset workspaces after success, failure, and cancellation without mutating admitted arguments, prepared messages, previous returned results, or another call's state. Observe cancellation at bounded operation points before successful return; an adapter cannot promise rollback of already evaluated application expressions. Out-of-memory handling follows the actual host/runtime failure profile and must not be reported as successful truncated text.

## Reuse and Performance Requirements

Apply [026's performance implementation architecture](./026-intlify-conformance-and-measurement-design.md#performance-implementation-architecture):

- Prepare parser/semantic and capability facts once per exact compatible immutable message/profile basis; never parse source or rebuild maps on every interpolation.
- Use dense local parameter slots and contiguous literal runs when useful. Keep complete logical names/associations for admission and diagnostics without repeated string hashing in the hot path.
- Validate/measure each distinct Text input once per admitted call, then reuse its byte length and immutable value for repeated occurrences. Host expressions still execute exactly as required by 016/024.
- Pre-size bounded output and append runs directly; a plain-text call need not allocate a part object or intermediate string for every expression.
- Keep reusable invocation/worker scratch separate from retained prepared state, shared immutable caches, and output storage. No shared mutable call scratch or mandatory arena is required.
- Avoid allocation for an empty success-diagnostic collection and omit unrequested expanded traces. Do not omit required validation or identity associations.
- Start with synchronous sequential evaluation. SIMD, unsafe code, custom allocators, parallel per-message work, and a persistent cache are not minimum prerequisites.

A retained ready-message/cache basis includes the exact selected artifact or admitted derivation, parameter specification, execution/semantic revisions, relevant locale/direction context, output/bidi behavior, and actual preparation dependencies. Different provenance/artifact identity is not silently interchangeable because text matches. More selective physical sharing requires a proof that it preserves those associations and applicable admission.

Do not reuse formatted output across changed argument values or contexts. Limits and current handle/context compatibility are checked even when preparation is cached. Eviction and scratch reset cannot change text, failure category, or diagnostics; an absent preparation may delay readiness outside formatting but cannot cause a formatting-time network request.

| Operation/surface | Minimum observations |
| --- | --- |
| Semantic admission/preparation, core | Actual source/semantic bytes and node kinds, required names/occurrences, accepted/unsupported result, retained/scratch storage, parse/prepare count, and duration |
| Argument admission, core/host boundary | Submitted entries/bytes, conversion and validation counts, exact failure/success, transferred/materialized bytes, duration, and applicable allocations |
| Evaluation, core | Logical work, expanded UTF-8 bytes, exact text or failure observation, cold/reused workspace state, and duration |
| Localizer/handle/cache, 027 | Readiness, checked lookup, preparation reuse, retained state, and setup failures separately from one-message evaluation |
| Generated Web pair, 024/028 | Actual AOT/Runtime observations, host behavior, output footprint, loading topology, and complete applicable 026 evidence |

The implementation plan pins Method Descriptors, intervals, input/case identities, Memory Observation Domains, cache/repetition state, and independent semantic observations. Timings/allocation counts start as validated descriptive baselines; this document invents no numerical performance pass threshold. Correctness and evidence completeness are completion gates. Optional profiling is a separately validated non-default build feature, not part of primary timing samples.

## Conformance and Minimum Integration

The common case inventory supplies actual message bodies and argument values, independent expected semantic observations, supported profiles/limits, and exact source/artifact associations. The following cases are mandatory for a claim covering this minimum; unsupported broader features appear as negative capability cases, not skipped positive cases.

| Case family | Required observations |
| --- | --- |
| Literal semantics | Empty pattern, simple/quoted equivalence, whitespace/newlines, escaped braces/backslashes, combining marks, supplementary characters, and exact Unicode preservation |
| Interpolation | One/multiple/repeated/reordered parameters; empty Text; parameter content resembling MF2/HTML; actual parser-derived names and no reparsing/coercion |
| Argument admission | Missing/extra/duplicate/noncanonical names; null-like/undefined/number/object/boxed values; lone surrogates; wrong slot association; deterministic multiple-error precedence |
| Capability admission | Valid MF2 with explicit functions, declarations, selectors, attributes/options, literal expressions, or markup is unsupported; malformed MF2 preserves parser errors |
| Bidi | Exact FSI/PDI for ASCII, empty, RTL/mixed, combining, and already control-bearing Text; repeated occurrences; all three supplied message directions; no control stripping |
| Locale/context | Interleaved `en`, `ja`, `en`; unchanged prior context; direct-locale mismatch; unsupported/not-ready context; no ambient locale or service call |
| Fail-complete behavior | Exact and first-over limits, arithmetic overflow, cancellation/failure followed by reuse, unchanged caller output prefix, and no valid-looking partial message |
| Identity and reuse | Cold/warm/full-rebuild equivalence, changed artifacts/profile/context/arguments/limits, no old-result mutation, and no cross-Release handle reuse through 024/025 |
| Host integration | Once-only source parameter evaluation in original order, changed render arguments without rebuild/sync, preserved host exceptions, inert `textContent`, and no synthetic application rollback |
| Supply/execution separation | Actual 020/021 selected definitions, zero Provider/Store/governance calls, no new candidate or fallback choice, and no source/parameter disclosure through normal diagnostics |

The 028 success pair uses the three English and three selected Japanese definitions. `Save` and `Welcome` remain parameterless; both greeting occurrences receive the same admitted current argument. With `Ada`, the greeting's exact English output is 16 UTF-8 bytes and its Japanese output is 28 UTF-8 bytes, including isolation. These sizes provide independent exact/first-over output-limit cases, not global capacity defaults.

Run the same cases through an actual Runtime evaluator and actual generated AOT execution. AOT must derive code from admitted semantic input, not embed expected DOM answers or call the Runtime evaluator. A shared compiler analysis is allowed; independently specified expected strings/failures are still required. Compare complete logical character sequences, context associations, failure stages/reasons/subjects, and logical resource accounting. Merely comparing two implementations to each other can reproduce the same defect.

The integration also changes `Ada` to `Kai`, reuses both Localizers, and verifies that arguments containing braces or script-like text remain data. Provider invocation and source analysis counts remain zero during these repeated renders. Real browser observations and output-set/Release checks belong to 024/025/028, not a mocked selected-message evaluator.

Full parts, selectors, functions, recovering MF2 errors, platform-managed locale services, native bindings, and hydration equivalence are not declared capabilities of this subset. Their 026 cases become applicable only when their owning profiles are adopted; passing this suite is not full I1 or cross-platform implementation evidence.

## Adoption with 016 and 028

| Adopting work | Required scope and claim |
| --- | --- |
| 023 logical-core development | Actual parser/semantic input, supported-node validation, Text/argument rules, exact evaluation/bidi behavior, bounded failures, and independent fixtures |
| 016 Phases 1–3 | Authoring/identity work remains independently implementable; accepted authoring syntax does not imply this target supports every MF2 feature |
| 021 candidate validation / 020 handoff | Derive and compare the actual Text parameter requirements while preserving candidate versus target admission and exact selection associations |
| 024 minimum Web export | Materialize this capability/profile, safe host conversion and lowering, exact generated references/outputs, and equivalent AOT semantics before publication |
| 027 reference Runtime | Adopt the ready-message/Localizer behavior, immutable storage, synchronous calls, typed outcomes, and actual 024/025 admission without defining competing semantics |
| 016 Phase 5 / 028 execution pair | Use real upstream supply/selection, generated code, Release admission, Runtime/AOT execution, host-behavior checks, and applicable 026 evidence |

This is a minimum owner specification, not an implementation-completion claim. Exact downstream formats and host mechanics must be adopted before their dependent integration. A test-owned logical core can start without completing every future artifact family, but cannot masquerade as a full production Profile, Release, or deployed Runtime.

## Decision Log

| ID | Decision | Rationale |
| --- | --- | --- |
| 023-001 | Define one strict plain-text/interpolation profile rather than the complete MF2 execution surface | Makes 028 implementable while keeping broader functions and recovery explicit |
| 023-002 | Use actual shared semantic facts and a closed node allowlist | Avoids a second placeholder parser or silently weakened target behavior |
| 023-003 | Admit only immutable scalar Text with exact required names and no host coercion | Keeps cross-target value behavior predictable and side effects outside evaluation |
| 023-004 | Keep source parameter evaluation order separate from translated occurrence order | Reordering or repeating translated parameters cannot rerun application code |
| 023-005 | Use unknown-direction text and default FSI/PDI isolation at every interpolation | Gives the execution pair exact bidi behavior without locale-data inference |
| 023-006 | Retain requested/definition locales separately while starting with direct equality | Preserves linker ownership and future fallback semantics without runtime searching |
| 023-007 | Reject invalid typed calls before evaluation and defer broader MF2 recovery explicitly | Distinguishes argument failure from expression fallback and application error presentation |
| 023-008 | Bound UTF-8 expansion and logical work independently of physical optimization | Prevents AOT/caches from changing limits or exposing truncated text |
| 023-009 | Require immutable readiness, synchronous formatting, and independent output ownership | Supports different engines and reuse without scratch lifetime or ambient-state defects |
| 023-010 | Verify exact characters and failures against independent oracles across both real paths | Visual equality or matching fixtures alone cannot prove semantic equivalence |

## Deferred Follow-Up Notes

### Required follow-up for minimum Web execution

- 017/024: adopt the exact execution-capability/context/parameter and selected-message target representations, logical result/diagnostic projections, identity, and version checks used here. Do not reinterpret current authoring or data-only ESM records as complete execution artifacts.
- 018/024/025: instantiate the applicable artifact/value trust, resource limits, output-set/Release/handle compatibility, and execution-admission inputs. Formatting receives checked immutable evidence, never supply or publication credentials.
- 024: define the smallest Runtime-backed and AOT Web outputs, generated call/binding rules, primitive-string conversion and surrogate rejection, safe literal emission, once-only host evaluation, and output-set admission. This is the next target-side specification.
- 027: define and implement actual preparation/evaluation, loader/readiness and locale-bound entry points, cache keys, output/workspace lifetime, and typed failure projection for this profile.
- 028/029: integrate actual selected definitions, explicit direction inputs, exact bidi-aware oracles, local Release admission, and the two real browser paths; retain complete applicable 026 evidence.

### Broader extensions

- Explicit default functions, MF2 declarations/selectors, richer Text/direction metadata, portable numeric/temporal values, and immutable custom-function capability registries.
- Recoverable MF2 resolution/function errors and fallback values, alternate strict/recovering entry profiles, and parts/markup projections with independent safety and ordering rules.
- Locale Service Profile encodings, pinned/platform-managed services and data, linker-materialized fallback across definition locales, and negotiation adapters under their existing owners.
- Native/FFI value and result ownership, broader Unicode host representations, framework/hydration behavior, richer diagnostics, and additional execution-equivalence profiles.
- Measured specialized representations, vectorized copying/validation, more selective sharing/caches, and broader performance budgets under 026.

## Relationship to Other Documents

- [000](./000-intlify-overview-design.md#language-and-target-strategy) defines shared semantics with different physical engines; this minimum does not choose one universal Runtime implementation.
- [016](./016-intlify-source-authoring-and-intent-identity-design.md#parameter-object-rules) owns authoring names and expression-evaluation evidence; 023 assigns runtime value and formatting meaning to the supported subset.
- [028](./028-intlify-javascript-web-vertical-slice-design.md#minimum-scenario) fixes the finite Web goal; its readable output examples are complemented by exact isolation-aware observations here.
- [027](./027-intlify-reference-runtime-design.md#formatting-output) consumes these normative behaviors; its broader physical architecture remains available for later profiles.
- [026](./026-intlify-conformance-and-measurement-design.md#execution-conformance) owns common execution/equivalence campaigns and applicable evidence, not a separate implementation phase.
