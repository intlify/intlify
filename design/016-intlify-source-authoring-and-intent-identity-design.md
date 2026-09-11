# Intlify Source Authoring and Intent Identity Design

## Purpose

This design defines how application-owned source becomes identifiable, language-neutral Message Intents and references. It answers four questions that a source-first compiler must answer before localization planning can begin:

- Which source occurrences communicate a localizable message, and which are deliberately excluded?
- What are the source message, source locale, parameter requirements, and communication context?
- Is an occurrence the continuation of an existing Intent, a new Intent, or an ambiguous identity change?
- What checked facts can later planning, synchronization, tooling, and source lowering consume?

For example, an application can keep a simple label in its original form:

```js
const button = document.querySelector('#pay')
button.textContent = 'Pay now'
```

A supported Producer proves the UI destination, extracts the literal message, resolves its source locale and context, and associates it with a persistent Intent identity. A move to another file does not inherently create a new message. Changing the wording changes the Intent's localization-relevant revision, not inherently its persistent identity. The actual rewrite to a generated target reference belongs to the later build and lowering stages.

![Source authoring, shared message analysis, and Intent identity reconciliation produce checked facts for later compilation](./assets/016-intlify-source-authoring-and-intent-identity-overview.svg)

The design separates language-neutral meaning and identity rules from a first JavaScript/TypeScript authoring profile. It does not require every Producer to recognize the same syntax. Unless recorded as accepted below, public authoring spellings, identity-update ergonomics, and the open choices remain proposals for review. An accepted design choice does not imply an implemented capability.

## Goals

- Preserve readable source-locale messages without requiring developer-maintained translation keys or catalogs.
- Define bounded automatic UI recognition, explicit MF2 authoring, and inspectable non-localizable declarations.
- Reuse the shared MF2 parser and semantic model for all recognized authoring forms.
- Keep static message structure separate from runtime parameter expressions and finite message selection.
- Define the source-locale and surface-class handoff from 015 without implementing another configuration resolver.
- Separate persistent identity, semantic revision, source evidence, and target-generated Message Handles.
- Preserve identity through justified continuations and require explicit resolution of ambiguous history reuse.
- Define read-only compilation and an explicit, atomic identity-registry update workflow.
- Produce bounded, deterministic authoring outcomes suitable for later shared-artifact encoding and project-graph queries.
- Apply 026's allocation, ownership, measurement, and optional-profiling requirements from the first implementation.

## Non-Goals

- Implementing translation, Provider calls, TMS synchronization, approval, candidate selection, or Translation Store publication.
- Computing final reachability, requested-locale requirements, message-locale fallback, delivery placement, or Release membership.
- Defining portable runtime value encodings, MF2 function implementations, runtime locale ownership, or formatting execution.
- Rewriting host code, selecting generated handles, or freezing exporter and Runtime ABIs.
- Defining the wire schemas, persistent ID encoding, registry file format, or canonical digest framing owned by 017.
- Defining repository discovery, commands, editor/LSP protocols, package names, or automatic update scheduling owned by 019 and 029.
- Translating arbitrary runtime-generated strings, treating all string constants as UI, or providing a general-purpose data-flow analyzer.
- Completing Vue, JSX/TSX UI, Swift, Kotlin, Rust, C/C++, Go, or .NET Producer profiles in the first JavaScript/TypeScript implementation.
- Promoting PR #183's placeholder grammar, path/content-derived IDs, or global runtime helper into production specifications.

## Ownership and Dependencies

| Owner | Responsibility relevant to this design |
| --- | --- |
| 000 | Source-first direction, authoring principles, artifact roles, identity lifecycle, and separation of synchronization, build, and execution |
| 001, 002, 012 | MF2 syntax, source ownership, CST and semantic facts, parser diagnostics, and parser-owned semantic validation |
| 015 | Checked project identity, optional canonical default source locale, exact surface-class vocabulary binding, and applicable configuration and policy inputs |
| 016 | Recognition semantics, declaration/reference distinction, host-to-MF2 extraction, static parameter bindings, context assignment, owner-qualified identity uniqueness and continuity, revision meaning, reconciliation, and authoring diagnostics |
| 017 | Shared Intent/reference/source-evidence representations, identity-domain representations and encodings, registry representation, digest framing, and version admission |
| 018 | Authentication and authorization of source, imported artifacts, registry publication actors, and shared trust/resource-policy admission |
| 019 | Project graph, common diagnostic envelope, incremental scheduling/cache management, identity and inventory queries, and client projections |
| 020 | Reachability, complete Requirement Plans, source fulfillment, final library composition, and linking |
| 021, 022 | Governance and Store history, plus explicit localization synchronization; identity continuity alone grants neither approval nor candidate eligibility |
| 023 | Portable parameter/value/function semantics and execution behavior; 016 supplies source-derived requirements without independently implementing them |
| 024, 028 | Checked source-lowering plans, target references, host rewriting, and the first end-to-end Web integration |
| 026 | Cross-component performance requirements, common conformance/measurement records, optional profiling, and comparison admission |
| 029 | Acquisition of source and configuration inputs, declaration-binding configuration, registry storage/update UX, commands, package layout, and product orchestration |
| 030–035 | Additional host, framework, mobile, library-distribution, and native integration profiles |

These are ownership relationships, not a requirement to finish every downstream design before discussing 016. The shared meaning can be designed now. An implementation needs the exact upstream specifications for the subset it claims to support; test-owned inputs may exercise a smaller slice but cannot masquerade as complete checked project or artifact inputs.

In particular, PR #205 supplies 015's minimum configuration/canonicalization/locale-core path and 017's initial configuration-reference and measurement subset. It does not supply a complete `LocalizationProjectProfile`, Intent artifact schema, production registry, or production locale-data implementation. Those gaps remain explicit adoption prerequisites rather than being filled with hidden Producer defaults.

## Inherited Decisions from 000

- Application source is the authoring source of truth; localization artifacts are managed outputs.
- Automatic recognition requires a provable supported UI surface. Explicit authoring is a first-class programmable interface, not merely error recovery.
- Message structure is static; runtime values and finite selection among known messages may vary.
- `intent()` establishes an MF2 authoring context without requiring an additional `mf2` tag. Standalone `mf2` authoring remains useful.
- Every checked Intent has exactly one source locale. Application defaults apply only when omitted; library source locales are not reinterpreted by consumers.
- Identical text at unrelated declaration sites does not merge identity.
- Persistent `MessageIntentId` is opaque and independent of source text, file path, and occurrence order. Target-local handles are a different identity domain.
- Localization-relevant semantics affect `IntentRevision`; source coordinates and non-semantic formatting do not.
- Ambiguous copies, splits, merges, and identity conflicts require explicit reconciliation. Retired identities are not silently recycled.
- Normal build and production execution do not invoke translation services or acquire governance authority.

## Terminology

| Term | Meaning in this design |
| --- | --- |
| Authoring profile | Versioned rules for one supported host syntax, intrinsic-binding recognition, UI surfaces, bounded analysis, extraction, and context assignment |
| Source unit | One immutable host source snapshot with an exact identity/revision, bytes, grammar selection, and owning application or library scope |
| Authoring inventory | Explicit finite source-unit membership for one declared scope, including whether the inventory is complete or a partial editor/incremental view |
| Declaration | A source occurrence that establishes a message's source semantics; an inline `intent()` literal, a reusable `mf2` declaration, or an automatically recognized UI literal |
| Reference | A source occurrence that may use an already declared message; several references can point to one declaration |
| Occurrence locator | Source-unit and syntax-role evidence used to locate a declaration/reference in a particular snapshot; not a persistent Intent ID |
| Semantic UI usage | The declaration's communication purpose, such as a button label or accessibility label, when established by bounded Producer rules |
| Surface class | A coverage-facing class from the exact vocabulary admitted through 015; distinct from a DOM tag or inferred UI role |
| Authoring semantic projection | The source message, canonical source locale, parameter/selector requirements, semantic usage, explicit context, and localization constraints that determine revision |
| Identity registry snapshot | Immutable compiler-managed associations between declaration lineage and persistent IDs, with active/retired state and exact update ancestry |
| Continuity evidence | Checked evidence that a current declaration continues one previous declaration; a content match or similarity score alone is insufficient |
| Reconciliation plan | A bounded proposed registry change against one exact base snapshot, recording continuations, allocations, retirements, and unresolved conflicts |
| Checked authoring result | A complete result for its declared input scope whose supported syntax, source semantics, identity associations, and finite references passed the applicable checks |
| Diagnostic | A structured error, warning, or informational record with a stable code, severity, evidence, typed dependency cause, affected entities, and an optional suggested action or edit; distinct from the overall authoring outcome |

These names describe conceptual values and operations. They do not reserve Rust type names, JavaScript exports, or serialized member names.

## Design Overview

The Producer pipeline has five logical operations. The overview diagram groups them into three boxes: discovery; message analysis plus context resolution; and identity resolution plus result construction. A physical implementation may share parsed state between them, but it must preserve their distinct inputs, failures, and measurement meaning.

| Operation | Reads | Produces |
| --- | --- | --- |
| Host admission and discovery | Source snapshots, explicit grammar/profile, intrinsic bindings, finite inventory | Candidate declarations, references, exclusion evidence, and host diagnostics |
| Message analysis | Extracted literal/MF2 text, source maps, parser specification | Parser-validated message facts and external parameter/selector requirements |
| Context resolution | Message facts, source annotations, projected 015 inputs | One canonical source locale, checked surface class, semantic usage/context, and semantic projection |
| Identity resolution | Current declarations, one registry snapshot, admitted continuity decisions | Stable IDs, revisions, or a reconciliation-required outcome |
| Result construction | Admitted declarations, finite references, source evidence, completeness facts | Complete checked authoring result or blocked outcome; optional inspection projections |

The registry update operation is separate. Compilation may explain why an update is needed and produce a candidate plan, but it does not write files or allocate durable identity as a hidden side effect. An authorized host validates and publishes an accepted plan, after which compilation uses the resulting immutable registry snapshot.

## Inputs and Results

### Invocation inputs

Each invocation receives already acquired, bounded inputs:

- An authoring profile and its supported specification revisions, including the exact MF2 parser/semantic behavior it uses.
- A finite authoring inventory with source-unit identities, revisions, content verification, owner scope, grammar, and completeness evidence.
- Exact intrinsic bindings and any admitted module-reference summaries. The shared semantic core does not resolve package names or read the filesystem.
- The applicable checked projection of 015: project/owner context, optional default source locale, and exact surface-class vocabulary.
- Separately, at most one optional default `surfaceClass` explicitly supplied by the caller for this invocation's declared authoring scope. This versioned assignment input is checked against the supplied vocabulary; it is not a new 015 configuration field.
- A canonicalization service/data binding compatible with the checked profile for any explicit source-locale declarations. No ambient OS locale or network lookup is permitted.
- One immutable identity-registry snapshot, plus any explicit, validated reconciliation decisions or continuity evidence needed by the requested operation.
- Explicit limits and requested diagnostic/inspection projections. Trust and policy admission belongs to the relevant owner, not a boolean supplied as a substitute for validation.

Application and library compilation are distinct owner scopes. A published library's supplied Intent and reference artifacts enter through 017/018/020 admission; an application Producer does not rescan them as new application declarations or replace their source locale.

### Complete and partial source inventories

The accepted input rule makes the caller, such as a CLI or build integration, responsible for supplying the finite source-unit list, its immutable snapshots, and its owning scope. The Producer does not discover the repository or acquire additional files to complete that list.

The inventory explicitly declares whether it covers the whole stated analysis scope or is a partial view, such as an editor request. A complete inventory with missing or unsuccessfully analyzed source units cannot produce a complete checked result. A partial result may support inspection within its own scope, but cannot satisfy a complete build or authorize identity retirement.

Completeness describes the declared analysis scope and must agree with the supplied source membership and returned results. It does not assert that every language or UI API in the repository is supported.

### Outcome states

The initial result model distinguishes `checked`, `blocked`, and operational failure. Cancellation, inconsistent snapshot attachment, parser invariant failure, and unsupported implementation specifications are not successful empty results.

- `checked` contains the entire declaration/reference result for the declared invocation scope, required minimum source evidence, exclusions, exact input dependencies, and completeness information.
- `blocked` contains bounded diagnostics and independently established inspection facts, but no result that consumers may treat as complete authoring input.
- A partial editor inventory may yield checked facts for that explicitly partial scope. It never establishes complete-project absence, authorizes retirement, or replaces a complete build inventory.
- An exceeded limit, unanalysed eligible source unit, unresolved declaration identity, or non-enumerable known message reference prevents complete success for the affected scope.

Inventory membership is supplied by the host and checked for consistency with returned source-unit results. This design does not claim that scanning only a supported file subset proves localization coverage for every file or UI API in a repository.

## Authoring Classification

Every occurrence considered by a supported recognizer receives one of the following classifications:

| Classification | Required behavior |
| --- | --- |
| Proven ordinary UI literal | Create a distinct declaration with literal-message semantics and a reference at that use site |
| Explicit MF2 declaration | Analyze its static MF2 source even when it is not attached to a UI sink |
| Reference to existing declaration(s) | Retain the exact finite possible declaration set and parameter-expression evidence |
| Explicit non-localizable occurrence | Preserve host behavior, emit exclusion evidence, and create no Intent or localization requirement |
| Known UI destination with unsupported message source | Emit a blocking authoring diagnostic; do not silently translate or silently exclude it |
| Source outside the selected recognition profile | Do not reinterpret it as localizable; report profile coverage in inventory/inspect output rather than claiming it was proven non-localizable |

An arbitrary object property named `textContent`, a string constant containing an HTTP error description, and a function named `intent` are not sufficient evidence by spelling alone. Explicitly excluded occurrences and occurrences outside the supported profile remain different facts.

The classification pass gives recognized explicit authoring precedence over automatic extraction at the same use site. `button.textContent = intent('Pay now')` creates one declaration/reference pair, not an additional automatic Intent for the surrounding assignment.

## Proposed JavaScript/TypeScript Authoring Profile

This section proposes the first concrete host profile. Its accepted decisions and any remaining proposed features are distinguished in the Decision Log below. Other host languages may implement equivalent semantics through macros, annotations, templates, or compiler-recognized declarations.

Unless an annotation says otherwise, the conceptual examples assume an admitted source-locale default of `en` and an explicit authoring-scope surface-class default that belongs to the supplied vocabulary. These are example inputs, not implicit product defaults. The snippets illustrate authoring rather than an already available production package.

### Intrinsic binding recognition

The accepted recognition rule identifies `intent`, `mf2`, and `noIntent` by lexical binding identity under an explicitly admitted module/export mapping, not by callee spelling. Package resolution and actual distribution names belong to 029.

The first profile supports direct named imports and their import aliases. Shadowed names, unrelated functions, arbitrary wrappers, reassigned aliases, namespace/member lookups, and unregistered re-exports are not implicitly recognized. A binding that is known to be an authoring intrinsic but used in an unsupported way produces a diagnostic rather than being treated as an ordinary call.

Examples below assume `intent`, `mf2`, and `noIntent` are recognized bindings. They do not define a package import path or a runtime implementation.

### Inline programmable messages

```js
intent('Pay now')
intent('Hello {$name}!', { name })
intent('Total: {$amount :number}', { amount: calculateTotal() })
```

The accepted authoring API shape is `intent(sourceOrDeclaration, parameters?)`. The first argument accepts static MF2 source text or a reference to a reusable `mf2` declaration:

- A static string literal in the first argument creates one declaration and one use-site reference. An untagged, substitution-free template literal is also static source.
- A reference to a reusable `mf2` declaration creates a use-site reference to that declaration. Calls sharing the declaration share its Intent identity; equal text at separate declarations does not imply shared identity.
- The string contains MF2, not a separate placeholder language. Requiring ``intent(mf2`...`)`` is unnecessary; an explicitly nested static tag may still be recognized as one declaration.
- The second argument contains interpolation parameters only. It is not an options/metadata object, and this API shape has no third metadata argument.
- Each separate inline declaration has separate identity by default. To reuse one Intent, reference an explicitly shared declaration rather than relying on equal text.
- Locale selection and the execution context are not authoring arguments here. Their binding and generated execution API belong to 023/024/027.

### Reusable MF2 declarations

```js
const inboxMessage = mf2`.input {$count :number}
.match $count
one {{You have one message}}
* {{You have {$count} messages}}`

intent(inboxMessage, { count })
```

The standalone `mf2` tag establishes a reusable message declaration, not a formatted string or a translation-service request. Passing this declaration to `intent()` is an accepted authoring form. Calls referring to the same declaration share its Intent identity. Exact runtime representation, TypeScript declaration types, and whether the tag is fully erased by lowering are not frozen here.

The initial profile admits direct immutable local declaration references. Cross-module references require an exact admitted module/export graph or source-first library manifest; dynamic module discovery is not performed by this operation. Until that capability is implemented, a Producer reports an unsupported reference rather than claiming to have enumerated it.

An authoring tag contains no JavaScript `${...}` substitutions. Runtime values belong in MF2 variables and the `intent()` parameter object. A bare tagged descriptor assigned directly to a DOM string property is not a supported rendering form in this proposed profile; use `intent(descriptor, parameters)`.

### Bounded automatic DOM recognition

The accepted minimum automatic surface is static literal assignment to `textContent` of a proven DOM receiver originating from standard `document.querySelector()` or `document.createElement()`. These are the only admitted receiver origins in the initial profile. Additional DOM APIs and `setAttribute('aria-label', literal)` require explicit profile extensions and fixtures, as do other setters, HTML parsing, JSX text, template whitespace, and framework-specific surfaces.

Receiver evidence must start from the admitted standard DOM bindings for these two APIs, not from their spelling alone. A shadowed or user-defined `document` does not establish a standard DOM origin. The accepted alias rule follows only simple `const` initializations within the same function or at module level: a locally named receiver starts from an admitted DOM call, and each additional alias directly names an already established receiver binding. The chain remains bounded by the invocation's analysis limits.

The initial DOM receiver tracer does not follow `let`/`var` bindings, storage and retrieval through objects or arrays, function argument/return flows, or references crossing into another function, including closure capture. These restrictions apply to DOM receiver tracing; they do not redefine authoring-intrinsic import aliases or reusable MF2 declaration references.

The literal selector/tag and receiver chain are retained as evidence; the compiler does not run selectors or assume that an element exists at runtime. `const` fixes the variable binding, not the DOM element's properties or behavior. Host null checks, exceptions, and control flow must remain unchanged by later lowering.

Type assertions or a `textContent` property name alone do not prove a DOM receiver. Mutation, escaping aliases, unknown interprocedural flow, or exhausted analysis limits cannot strengthen proof.

```js
const button = document.querySelector('#pay')
const target = button
if (target) {
  target.textContent = 'Pay now' // The local const alias retains the receiver origin.
}

const label = document.createElement('span')
label.textContent = 'Total' // Also a supported receiver origin and sink.

const status = { textContent: 'internal-state' } // Not a proven DOM sink.
const errorDescription = 'Service unavailable' // Not automatically localized.
```

The accepted escape rule invalidates the evidence needed for later automatic DOM localization when a tracked receiver is passed to a call whose effects on that receiver are not analyzed. The same invalidation applies to all tracked aliases of that receiver. Its known DOM origin is retained as source evidence; this does not assert that the object has ceased to be a DOM element. It means that standard display-property behavior is no longer established.

```js
const button = document.createElement('button')
const target = button
customize(button) // Effects on the receiver are outside the analyzed profile.
target.textContent = 'Pay now' // Error: automatic localization lacks receiver evidence.
```

Such a subsequent automatic-localization candidate produces a blocking authoring diagnostic and is not automatically transformed or silently classified as non-localizable. The call itself is not a localization error merely because the receiver is passed to it. Replacing the final message expression with `intent('Pay now')` explicitly declares the message and remains supported under the normal explicit-authoring rules. This does not restore automatic receiver evidence or change the host property's behavior.

The accepted non-call effect rules also invalidate receiver evidence when its `textContent` property is redefined, its prototype is changed, or the receiver is stored in an object/array or otherwise exposed outside the tracked scope. Invalidation applies to all tracked aliases using the same control-flow rules as call-based escape. These operations alone do not produce a localization error; later automatic-localization candidates are diagnosed, and explicit `intent()` authoring remains supported.

Supported ordinary assignments to a verified receiver's `textContent` and supported local `const` alias creation preserve evidence that is already valid. Updating displayed content is distinct from redefining the display property. Neither operation restores evidence that has already been invalidated.

```js
const button = document.createElement('button')
button.textContent = 'Loading'
button.textContent = 'Ready' // Both ordinary updates retain the receiver evidence.
```

The accepted control-flow rule permits automatic localization only when receiver evidence remains valid on every execution path that may reach the assignment. At a branch join, evidence invalidated on any potentially reaching path is not restored by merging with a path that retained it. The analysis does not guess runtime condition values to discard such a path.

```js
const button = document.createElement('button')
if (needsCustomization) {
  customize(button)
}
button.textContent = 'Pay now' // Error: one reaching path invalidates the evidence.
```

Loop analysis must account for invalidation carried from earlier iterations, not just the first iteration or textual statement order. If the analysis cannot establish the all-path requirement, the affected automatic candidate is blocked with a diagnostic. Exhausted analysis limits use the resource-limit outcome rather than treating incomplete proof as success. These rules do not authorize evaluating application code or changing its branches, iteration count, or exceptions.

The first automatic profile accepts static literals only. A runtime string, interpolated JavaScript template, or arbitrary string-producing call at a known UI destination requires explicit MF2 authoring or explicit exclusion. The Producer does not evaluate application code to recover message source.

### Explicit non-localizable values

The accepted exclusion-marker signature is `noIntent(value, reason)`, with a required nonempty static string reason and an identity-bound intrinsic. It explicitly excludes the value from Message Intent generation; it does not mean that an `intent()` call is merely absent. Supported ordinary UI literals remain eligible for automatic recognition without `intent()`.

Omitting the reason, supplying an empty string, or supplying a runtime-dependent reason produces an authoring diagnostic rather than a checked exclusion. Requiring the reason makes the author's intent inspectable by reviewers and AI agents; recognizing the exclusion marker itself does not technically require that explanation. The compiler does not infer a missing reason.

```js
button.textContent = noIntent('Intlify', 'Product name')
comment.textContent = noIntent(userComment, 'User-authored content')
```

The marker returns the same host value, evaluated once in the same position. It is not escaping, sanitization, formatting, or trust admission. The reason is inspectable exclusion evidence, not a translation key. Contradictory nested localizable and non-localizable markers are rejected in the first profile rather than resolved by marker order.

### Source metadata without parameter overloading

The accepted metadata syntax is a static `@intlify` block comment containing a JSON object. The same syntax supplies declaration metadata for supported ordinary UI text, inline `intent()` messages, and reusable `mf2` declarations without adding authoring API arguments:

```js
/* @intlify {
  "sourceLocale": "en",
  "surfaceClass": "checkout",
  "description": "Primary payment action"
} */
button.textContent = 'Pay now'
```

The accepted metadata members are optional `sourceLocale` (source language), `surfaceClass` (a project-defined surface classification), and `description` (an explanation of the message's meaning or use). Authors specify only the members they need. A recognized `@intlify` comment is compiler input; ordinary explanatory comments are not semantic input.

The accepted initial validation rules restrict the JSON object to `sourceLocale`, `surfaceClass`, and `description`, and require present values to be nonempty strings. Unused members are omitted. Malformed JSON, invalid encoding, unknown or duplicate members, and empty or non-string values produce error diagnostics rather than being silently ignored or accepted. Multiple annotations for one declaration are also errors; the compiler does not merge them or choose one by order. For example, a misspelled `descripton` member is reported as unknown instead of silently losing the intended explanation.

The accepted placement rule attaches the annotation to the next statement in the same lexical statement list, with no intervening statement. That statement must contain exactly one eligible declaration. Multiple candidates, an intervening scope change, or no declaration produces an attachment error diagnostic rather than selecting the first string or searching later statements. The annotation does not apply to an entire function or block and does not establish inherited defaults. Nested annotations must not compete for the same declaration.

```js
/* @intlify { "description": "Primary action" } */
render(intent('Save'), intent('Cancel')) // Error: two candidate declarations.
```

A reference to an existing descriptor cannot redefine its source locale, surface class, or description. Authors needing a different meaning declare a different message. A reference's own source/UI evidence remains available for diagnostics without silently changing the shared declaration.

Vocabulary authoring and assignment defaults remain separate inputs. The accepted initial default is one optional `surfaceClass` value supplied by the caller before source analysis, with the scope and precedence defined under Source Locale, Usage, and Context. CLI or build integrations supply this input; this document does not add fields to `intlify.config.json` or define public configuration/CLI spellings. Those user-facing forms belong to the owning integration design. The Producer does not scan source to invent the 015 vocabulary or equate a `button` role with a coverage class named `checkout`. A larger semantic-context vocabulary remains a later profile extension.

## MF2 Extraction and Parameter Bindings

### Literal text versus explicit MF2

Ordinary UI text has literal semantics. The Producer encodes it as a literal MF2 pattern using shared MF2 escaping rules and then uses the same parser/semantic pipeline. For example, braces in ordinary displayed text do not become interpolation merely because the application now enables localization. The literal encoder must round-trip to exactly the host-profile-defined displayed text, with no external MF2 variables.

Explicit `intent()` strings and `mf2` declarations contain MF2 syntax. The accepted extraction rule uses host-cooked string/template content for both: JavaScript escape decoding happens before MF2 parsing, and the tag's raw template content is not used as the MF2 input.

```js
intent('Line 1\nLine 2')
const multilineMessage = mf2`Line 1\nLine 2`
```

Both forms above pass an actual line feed to MF2 where `\n` appears in the JavaScript source. A backslash intended for MF2 must itself be escaped at the JavaScript layer in either form. This keeps escape handling consistent when moving text between inline authoring and reusable declarations; it does not merge their Intent identities.

The initial profile rejects invalid cooked escapes and non-scalar strings, preserves literal newlines, and performs no implicit trimming, dedenting, or Unicode normalization. Quote/escape spelling differences that cook to the same content are source-evidence differences, not different message text.

Host decoding and MF2 literal escaping produce a mapping from extracted MF2 byte ranges to original source ranges. Escapes may map several generated bytes to one host range; mapping must not fabricate one-to-one offsets. Any future HTML/template profile must define its own displayed-text and whitespace extraction rules separately.

### Shared parser and semantics

The pipeline follows 012: parse first; only diagnostic-free parsing permits semantic-model construction; only a successfully constructed model permits parser-owned semantic validation. Parser and semantic diagnostics preserve their owning codes. An invariant failure is operational failure, not an ordinary malformed-message diagnostic.

016 consumes parser-owned facts to derive external parameter names, declarations, selectors, and function/option requirements. It does not implement a regex placeholder parser, duplicate MF2 selector validation, or infer runtime formatting behavior from host types.

Function references and source annotations become symbolic requirements. Checking supported portable value families, function implementations, and target capabilities belongs to 023/024. A checked authoring result therefore does not assert runtime or target conformance.

### Parameter object rules

For the proposed initial profile:

- Parameter names must be statically enumerable and unique. Plain object properties with static identifier/string keys and shorthand properties are supported.
- Spreads, computed keys, getters, setters, methods, prototype-derived members, and passing an opaque runtime object as the parameter set are unsupported.
- Parameter values may be ordinary runtime expressions, including side-effecting calls. The compiler never executes them during discovery.
- The provided name set must exactly match the required external MF2 inputs. Missing and extra names are distinct diagnostics. No-parameter messages may omit the object or pass an empty plain object.
- MF2-local declarations are not caller parameters. Selector/function requirements are retained for later type and capability checks, not replaced by JavaScript string coercion.
- A parameter expression must be evaluated once, in original JavaScript argument/property order, at its original control-flow point. Optional metadata collection cannot cause an additional evaluation.

Parameter-expression source text, variable renames in surrounding host code, and value evaluation locations are reference evidence. They do not change Intent revision when the parameter names and source-derived requirements remain identical.

## Static References and Finite Selection

The first finite-selection form is a conditional expression whose alternatives are admitted immutable message declarations:

```js
const loading = mf2`Loading`
const done = mf2`Done`
intent(pending ? loading : done)
```

Both declarations remain possible references even when a compiler cannot predict `pending`. The condition remains runtime code. An implementation may add finite arrays, maps, enums, switches, aliases, and module edges only under explicit bounded profile rules; user-provided labels claiming completeness are not proof.

The initial rule for one shared parameter object requires compatible external-name and symbolic-requirement shapes across every alternative. Otherwise authors put separate `intent()` calls in the branches. Richer discriminated bindings require a later explicit extension, not an unbounded runtime source path.

Every member of the conservative finite set is retained for 020. The Producer does not prune an alternative based on translation availability, current locale, Store state, or guessed runtime values. Build-provided applicability and delivery evidence are recorded separately; 020 owns final reachability and target/group partitioning.

`intent(createMessageAtRuntime())`, dynamic imports without a finite admitted module set, and unbounded reference selection produce a blocking diagnostic. UGC or text localized by another subsystem uses explicit exclusion rather than a hidden runtime translation fallback.

### Module-reference handoff

For supported module references, the caller resolves modules/packages and supplies verifiable reference-target information under the selected bounded authoring profile. The Producer consumes that information rather than searching files or resolving packages itself.

A use retains the referenced declaration's owner-qualified Intent ID, revision, source locale, and declaration-owned metadata. Multiple screens using one declaration in a shared module create references to that declaration, not new Intents per screen. Use-site source evidence remains separate; the consumer does not overwrite the declaration with its own defaults.

Missing targets, target information inconsistent with the specified revision, or a target set that cannot be finitely established produce diagnostics and prevent complete success. These shared handoff rules do not admit every `import` form into the first profile: supported forms still require explicit bounded profile rules. Shared representations and artifact admission retain their 017/018 ownership.

## Source Locale, Usage, and Context

Source locale is resolved per declaration:

1. Use and canonicalize an explicit source-locale annotation when present.
2. Otherwise, for an application-owned declaration, use the canonical default supplied by 015 when present.
3. Otherwise, block the declaration with a source-locale diagnostic. Do not substitute the requested locale, host locale, `und`, or inferred text language.

A source-first library is compiled with its own admitted authoring context and publishes the resulting exact source locale. Consumers retain that locale; no application default is applied to imported Intents. Locale equality follows the admitted canonicalization specification/data, not raw spelling equality.

Context has three separate projections:

| Projection | Examples | Effect |
| --- | --- | --- |
| Source evidence | Unit/revision, span, declaration/reference role, nearby source, extraction mapping | Navigation and explanations; not semantic revision by itself |
| Semantic UI usage and explicit context | Proven accessibility-label purpose, declaration description, later structured semantic constraints | Participates in revision when communication meaning changes |
| Coverage-facing surface class | An exact declared class such as `checkout` | Must belong to the vocabulary pinned by 015; contributes a planning dependency |

An automatically extracted inline UI declaration may acquire semantic usage from its proven sink. A standalone reusable declaration owns its declared meaning; call-site evidence does not silently specialize it or mutate an imported library Intent. If a use requires a different communication purpose, it needs a distinct declaration or an explicit later specialization feature.

Every declaration requires a checked surface-class assignment. The accepted initial authoring-scope input supplies at most one optional default `surfaceClass` for declarations authored within one invocation's declared owner and source inventory. This is the scope of the analysis input, not a JavaScript block or an inherited annotation setting. An explicit per-declaration `surfaceClass` takes precedence; only its absence allows the invocation default to apply. Explicit assignments and any supplied default must belong to the exact vocabulary pinned by 015. If neither an explicit assignment nor a default is present, or the applicable class is unknown, the declaration and a complete checked authoring result are blocked.

The versioned default input exists independently of the current source scan. For example, a caller may supply `checkout` when analyzing checkout UI sources, allowing ordinary UI messages without individual class annotations. The default does not redefine referenced or imported declarations. Class changes update planning dependencies; class spelling alone does not change translation semantics unless the same edit changes semantic context.

`description` has no shared default. It is supplied only for declarations that need an explicit explanation; omission leaves it absent and does not prevent proven UI usage from being recorded separately. The application source-locale precedence remains explicit annotation, then the applicable 015 default, then a blocking diagnostic as specified above; no additional source-locale default is introduced by the scope-assignment input.

Physical path, component name, DOM tag, or nearby words never invent a vocabulary member. An AI agent may propose context or assignment changes, but they become authoring inputs only after they are explicitly recorded and recompiled.

## Intent Identity and Revision

### Four distinct identities

| Identity | Stability and purpose |
| --- | --- |
| Source unit/revision and occurrence locator | Identifies source evidence in a particular snapshot; may change during edits or moves |
| Owner-qualified persistent `MessageIntentId` | Identifies one message lineage independently of wording and physical location |
| `IntentRevision` | Identifies a versioned localization-relevant semantic projection of that lineage |
| Generated Message Handle | Compact target/Release-local execution reference; produced downstream, never stored as persistent identity |

A declaration is the unit of identity allocation, not each call that references it. Two inline `intent('Open')` calls or two unrelated automatic `Open` labels have distinct IDs. Two calls referencing the same named `mf2` declaration share its ID and revision. Internal reuse of parsed content must never merge these identity decisions.

### Owner-qualified Intent identity

The accepted persistent identity is logically the complete pair `(owner identity, owner-local Intent ID)`. The owner identifies the owning application or library, not a file or its path. This is an identification rule, not a serialized tuple or a choice of public field names.

Identity equality compares both parts. Equal owner-local ID values under different owners identify different Intents; comparing only the local value must not merge application and library identities. Within one owner, new allocations must reject collisions with both active and retired IDs. A retired ID remains reserved and cannot be assigned to a new message lineage. The separate explicit-restoration rules below concern restoring the same historical identity, not reusing its ID for a different lineage.

The owner-local value is opaque: its spelling does not describe the source message or its file position, and it is not derived from those values. UUID selection, length, byte/string representation, and serialization belong to 017, subject to these uniqueness rules. 016 fixes the logical identity scope without selecting those formats.

### Semantic projection and revision changes

The accepted revision input is a versioned projection of:

- literal content after host escape handling and MF2 parsing;
- MF2 semantic structure, including local variable names, initialization expressions, declaration order and reference relationships, selectors and their order, variant conditions/bodies and their order, function references, and options;
- canonical source locale;
- external parameter names and source-derived parameter/selector requirements; and
- semantic UI usage, explicit semantic context such as `description`, and localization constraints attached to the declaration.

The projection excludes host source coordinates, quote spelling, non-semantic CST trivia, reference counts/order, parameter-expression implementation, and source locators. It is not a hash of an entire host file or raw tagged-template spelling. Literal characters, spaces, and newlines are compared without implicit trimming, whitespace compression, or Unicode normalization. Formatting that leaves both the parsed structure and literal content unchanged does not change revision.

For example, `intent('Save')` and `intent("Save")` have equal literal content, while `intent('Save ')` changes it. Changing only `{ name: userName }` to `{ name: displayName }` for the same `Hello {$name}` message does not change revision when the message-side parameter requirements remain identical. Renaming the MF2 external parameter itself, or changing a formatting function or its options, does change revision.

The initial comparison is conservative about MF2 structure. Adding/removing branches, changing selector or variant order, renaming MF2-local variables, changing initialization expressions, declaration order, or reference relationships changes revision even when the output may coincide. It does not infer equivalence through alpha-renaming, expression expansion, or proof over all runtime inputs. This restriction does not make host-language value-variable names part of the projection.

Canonical source locale, description, and semantic UI usage changes affect revision. Switching between explicit and inherited locale evidence with the same canonical locale does not. Coverage-facing `surfaceClass`, requested locales, policies, glossaries, targets, and Provider revisions remain separate dependencies; unchanged Intent revision does not waive any downstream revalidation or localization work required by changes to those inputs.

017 must freeze the exact canonical representation, digest domain, and revision encoding before persisted production revisions are implemented. 016 supplies the semantic inclusion/exclusion rules and independent equivalence vectors. It does not reuse a fast in-process hash or PR #183's truncated path/source payload as a persistent ID.

| Change, with an established identity continuation | Persistent ID | Semantic revision | Other affected facts |
| --- | --- | --- | --- |
| Move file, change host indentation, equivalent host escaping | Preserved | Preserved | Source evidence and registry locator associations |
| Edit literal wording or significant message whitespace | Preserved | Changes | Source artifact, localization freshness, downstream evidence eligibility |
| Change MF2 external parameter name, selector, function, or option | Preserved | Changes | Parameter and capability requirements |
| Add/remove a branch, change its condition/body, or reorder selectors/variants | Preserved | Changes | MF2 structure and localization freshness |
| Rename an MF2-local variable or change its initializer, declaration order, or reference relationships | Preserved | Changes | Conservative structural comparison, even when output may coincide |
| Change a host parameter-value variable/expression without changing message-side requirements | Preserved | Preserved | Parameter-expression mapping and later lowering |
| Switch explicit/inherited source evidence but retain the same canonical locale | Preserved | Preserved | Dependency/evidence basis |
| Change canonical source locale or semantic purpose/description | Preserved | Changes | Source artifact and applicable localization work |
| Change only coverage class, policy, glossary, requested locales, target, or Provider revision | Preserved | Preserved | Separate planning, selection, validation, or synchronization dependencies |
| Copy an unrelated declaration with identical wording | New identity required | Computed for the new declaration | Possible reuse suggestions, never automatic history adoption |

Equal revision labels in a lineage must identify equal projections. Full source/message artifacts may have changed envelope identities even when localization semantics are equal. Equal rendered output alone is not sufficient to establish equal revisions under the accepted conservative projection.

## Identity Registry and Reconciliation

### Registry responsibilities

The registry retains owner-qualified IDs, declaration lineage associations, active/retired state, and an exact snapshot/update ancestry. Locator and continuity evidence are indexes into source history, not the definition of identity. The registry contains no translations, approval decisions, requested-locale catalogs, runtime handles, or Provider credentials.

The logical ID uniqueness rules are defined above. Exact owner/ID representations, allocation and serialization formats, integrity rules, and schema evolution are 017 work. A product may use a file such as `intent.lock`, but that filename and the update commands remain 029 decisions.

### Read-only compilation

Given the same source snapshots, authoring/profile specifications, accepted identity associations, and registry snapshot, compilation must produce the same logical result. It never invents durable IDs from traversal order, wall-clock time, source text, or filesystem location.

Existing associations may be reused through checked continuity evidence even when locators change. A new declaration, missing registry, conflicting association, or insufficient continuity evidence produces a reconciliation-required outcome. A normal build must not silently pick an old ID by text similarity or publish a new registry snapshot. Developer tooling may propose updates automatically, but acceptance remains a separate explicit update operation with its own result. The accepted developer-tooling rules below permit automatic acceptance of eligible updates only in an explicitly enabled development update mode.

The intent is to preserve IDs across ordinary edits and unambiguous moves, not to promise that every arbitrary source refactoring can be inferred correctly. The accepted continuity, automatic-update, and registry-recovery conditions are defined below.

### Accepted continuity evidence

The accepted rule retains an existing Intent ID through either of these checked associations:

1. For an unchanged source snapshot, reuse the base registry's existing association with that exact declaration and snapshot.
2. For edited or moved source, validate supplied edit/move evidence against the exact base registry and the previous and current source snapshots. The evidence must identify the prior and current declarations and establish a verifiable one-to-one continuation. The file and source position may change.

The association must agree with the registry's owning scope and the declarations in the actual snapshots. Multiple current declarations claiming the same prior declaration, or multiple prior declarations remaining possible for one current declaration, do not establish one-to-one continuity. Matching wording, file path, position, occurrence order, or a similarity score alone is insufficient.

For example, checked edit/move information can establish that the declaration in `checkout.js` moved to `payment.js`. Finding `'Pay now'` in both files cannot establish that history by itself. The developer is not required to write persistent IDs or translation keys into application source.

Missing, invalid, or ambiguous continuity evidence requires reconciliation. Compilation must not silently select an existing ID or allocate a replacement ID merely because it could not establish continuity. Confirmed new declarations and explicit identity decisions remain subject to the reconciliation procedure below.

Retaining an ID does not establish an unchanged semantic revision or continued translation freshness. A checked continuation with localization-relevant source changes retains its ID while changing `IntentRevision` as specified above. Evidence encoding belongs to 017; tooling acquisition and update UX belong to 029. Neither changes the continuity validity rules owned here.

### Accepted registry update history and publication

Registry updates are explicit operations separate from ordinary read-only compilation. Each accepted update retains a checkable relationship among:

- the owning scope and exact base registry snapshot;
- the authoring inventory and immutable source revisions used for the update;
- the accepted identity decisions and their required evidence; and
- the resulting immutable registry snapshot.

This records which registry was changed, on which source and decision basis, and into which result. Update ancestry identifies the exact accepted base, not a timestamp, a similarity match, or an unversioned reference to the latest state. These are logical relationships; 017 chooses their concrete representation and integrity encoding, while 029 owns persistence and update UX.

The authorized host may publish the complete accepted result only if its exact base is still the current registry snapshot. Checking that condition and publishing the whole update must form one atomic operation: an intervening update must not be overwritten, and no partially applied associations may become current. All required identity decisions must be resolved before publication.

For example, an update prepared against `R1` cannot be applied unchanged after another update has advanced the current registry to `R2`. It produces a conflict requiring rechecking and replanning against the current base, without silently overwriting or merging that state. Normal compilation never performs this update as a side effect. Every accepted development auto-update below must preserve these publication rules.

### Accepted development auto-updates

When a developer explicitly enables a development auto-update mode, an authorized tooling host may accept and apply the following eligible updates without per-update confirmation. Without this mode, these rules do not permit automatic acceptance.

| Update class | Required evidence and effect |
| --- | --- |
| Existing-ID continuation | Verify the one-to-one continuation against the accepted evidence rules. Ordinary wording edits or file moves qualify only through that evidence; apparent simplicity is not proof |
| New allocation | Confirm that the declaration is new; insufficient evidence of an old continuation is not proof of newness. The registry-update host generates an opaque ID outside normal compilation, fixes the candidate in the replayable update plan, and rejects collisions with active or retired IDs in the owning domain |
| Retirement | Establish absence from a successfully analyzed complete owning inventory with no unresolved identity associations. Partial views, parse failures, and mere lack of references cannot authorize retirement. A live declaration keeps its ID; retirement changes registry state without deleting identity/translation history or making the ID reusable |

Ambiguous associations or changes requiring a choice of historical identity require an explicit reconciliation decision. Tooling may present candidates, but enabling automatic updates does not authorize it to choose a lineage from similarity or bypass unresolved decisions.

Automatic acceptance remains a separate registry-update operation, with the same evidence, exact-base check, all-or-nothing publication, and requirement that all necessary decisions be resolved. Normal compilation stays read-only. Retaining an ID does not suppress `IntentRevision` changes or downstream freshness checks when localization-relevant semantics change.

Commands, settings, scheduling, and update UX belong to 029; this design does not reserve a command or configuration field for the mode. Registry initialization and recovery remain distinct explicit operations under the following rules.

### Registry initialization and recovery

A missing or corrupt registry is not evidence that this is a first-time project. The tooling host must not automatically issue fresh IDs for all declarations or use the auto-update mode to replace unavailable history.

Prefer recovering the existing registry from source control, a backup, or other retained history. Before reuse, validate the owning scope, integrity, and associations with the relevant source snapshots. A recovered candidate is not automatically the current registry merely because it exists; stale candidates must not be adopted without the required checks. Matching message text or source positions alone cannot reconstruct historical identity.

If a valid recovery cannot be established, emit diagnostics and stop registry updates pending explicit action. Initial creation for a new project is a separate explicit operation from recovery of an existing project's history; registry absence alone never selects between them. Concrete acquisition, recovery commands, and persistence remain 029 responsibilities subject to 017 admission and these identity rules.

### Proposed reconciliation procedure

1. Admit the exact base registry, owning scope, finite source inventory, source revisions, and supplied continuity or explicit resolution evidence.
2. Classify and analyze current declarations independently from matching history. Invalid declarations cannot acquire checked identity merely because a matching old record exists.
3. Apply explicit identity decisions bound to the exact base and current inputs, rejecting foreign-owner IDs, conflicting decisions, and resurrection without an explicit restore operation.
4. Retain exact source-snapshot associations and verified one-to-one continuations under the accepted continuity rules above. Edit/move evidence must bind the exact base registry and previous/current source snapshots; matching text, path, or ordinal alone is not sufficient.
5. Classify remaining declarations as new or unresolved. Similarity and refactoring heuristics may suggest candidates but cannot break ambiguous ties authoritatively.
6. Propose fresh opaque IDs for confirmed new declarations through the registry-update host, checking the entire retained owner domain for collisions, including retired IDs. Candidate IDs become replayable inputs of the accepted plan, not nondeterminism inside normal compilation.
7. Propose retirement only for declarations proven absent from a complete owning inventory. Missing units, parse errors, cancellation, partial editor views, and mere unreachability are not proof of deletion.
8. Emit one plan bound to the exact base snapshot, current inventory, proposed associations, and unresolved diagnostics. No accepted snapshot is produced while any required identity decision is unresolved.
9. The authorized host conditionally publishes the complete accepted snapshot under the atomic publication rules above, retaining its exact base/source/decision/result linkage. A changed base causes a conflict/replan, not a silent last-writer-wins merge.

016 owns validity of associations and plan preconditions. 017 owns encoding and integrity, 018 owns actor authority, and 029 owns persistence, atomic file/storage replacement, and update UX. The operation never publishes a translation, approval, or Release.

### Copy, split, merge, restoration, and retirement

- Copying a declaration creates a separate identity unless it is an explicit reference to the existing declaration. Reusing translation memory is independent from identity reuse.
- A split or merge requires an explicit plan. At most one current declaration may continue one prior identity; all other new declarations require fresh IDs. Lineage links are audit facts, not approval transfer.
- Restoring a retired identity requires an explicit, exact restore decision. Newly allocated IDs cannot collide with or recycle tombstones.
- A currently unreferenced but still declared message retains its identity. 020 may omit it from a particular build without instructing 016 to retire it.
- Retirement changes future authoring inventory; it does not delete immutable Store or Release history. Historical execution and rollback continue to use their pinned artifacts.
- Source-controlled registry conflicts cannot be resolved by unioning duplicate entries or choosing the newest timestamp. The same association and plan checks apply after branch merges.

## Source Evidence and Consumer Handoff

Minimum evidence binds each declaration/reference to its exact source unit/revision, half-open source range, authoring form, and extraction/parameter mapping needed for correctness. Evidence is validated against the actual immutable snapshot; an arbitrary supplied locator is not trusted as proof of origin.

UTF-8 byte ranges are the shared coordinate basis. Host bindings with UTF-16 or another indexing model convert against the exact source snapshot. Cooked host escapes and generated MF2 literal escaping retain segment mappings; multi-byte text, CRLF, zero-width insertion ranges, and EOF positions require explicit fixtures. Detailed snippets, navigation labels, and expanded maps may be lazy, but evidence required for correct admission and diagnostics is not optional.

The checked result supplies the logical fields needed by:

| Consumer artifact/model | 016 contribution | Not established by 016 |
| --- | --- | --- |
| `MessageIntentArtifact` | Persistent ID/revision, checked source semantics, source locale, context, parameter/selector requirements | Approval, selectable candidates, target compatibility |
| `SourceLocaleMessageArtifact` | Deterministic source definition derivable from one checked Intent revision and its source locale | Source authentication/approval or a Selection Decision |
| `MessageReferenceArtifact` | Exact or conservative finite Intent references, parameter bindings, source and supplied host applicability evidence | Final reachability, fallback, coverage, or delivery placement |
| Project graph and diagnostics | Input dependencies, source evidence, explicit exclusions, completeness, typed authoring failures | Query storage, client protocols, or a second common diagnostic format |
| Later source-lowering plan | Host occurrence and evaluation-order facts associated with checked references | Generated accessor name, compact handle, runtime ABI, or host rewrite |

Source-locale message derivation requires no Provider and does not place a translated candidate payload into the Store. Its envelope/digests and source-admission dependencies must use the later 017/018 definitions. Governance and linking still verify the applicable source evidence before use in a Release.

## Diagnostics and Failure Model

This document uses Diagnostic for the shared diagnostic/informational result defined in 000. The terminology does not introduce a new reporting format or change component ownership, diagnostic codes, severity, or failure behavior.

The following are proposed component-owned reason families, not a frozen global code registry. 019 owns the common envelope; MF2 parser/semantic codes retain their existing owners.

| Reason family | Meaning and remediation |
| --- | --- |
| `authoring-input-invalid` | Source/profile/inventory/binding inputs are malformed, incompatible, or inconsistently attached |
| `authoring-form-unsupported` | A recognized intrinsic or known UI surface uses an unsupported form; use a supported static declaration or explicit exclusion |
| `authoring-source-dynamic` | Message source or reference identity cannot be enumerated finitely |
| `authoring-metadata-invalid` | Metadata is malformed, multiply attached, ambiguously placed, or attempts to redefine an existing declaration |
| `authoring-source-locale-missing` / `authoring-source-locale-invalid` | No permitted source-locale basis exists, or the supplied locale fails the admitted canonicalization rules |
| `authoring-surface-class-invalid` | Required assignment is missing or not a member of the exact admitted vocabulary |
| `authoring-parameter-mismatch` | Missing/extra/duplicate parameter names or incompatible finite-alternative requirements |
| `authoring-identity-update-required` | New or moved source needs accepted identity associations before complete compilation |
| `authoring-identity-conflict` | Several lineages compete, one ID is assigned twice, a retired ID is reused, or plan/base inputs conflict |
| `authoring-resource-limit` | A named source, analysis, reference-set, registry, mapping, or reporting limit was exceeded |

Known-UI classification failures, MF2 validity failures, and identity failures remain separate. Failure in MF2 syntax suppresses dependent semantic/revision work for that declaration, not independent diagnostics in other admitted units. Source parse failure prevents claims about declarations absent from that unit.

Logical diagnostics use deterministic ordering independent of worker scheduling or hash-map iteration. The proposed component order is stage, admitted source-unit identity, source range, component reason, then a stable related-occurrence discriminator. Related locations use the same admitted source domain. Any reporting truncation is explicit and cannot turn a blocked result into checked success.

## Dependency, Invalidation, and Reproducibility

016 emits dependency facts; 019 owns cache keys, graph scheduling, and client-visible invalidation queries.

- Host discovery depends on exact source bytes/revision, grammar, authoring profile, intrinsic bindings, and any admitted module/receiver-analysis inputs.
- Message analysis depends on extracted MF2/literal content and exact parser/semantic/projection specifications. Parsing shared content once is allowed without merging declaration identities.
- Source-locale resolution depends on explicit locale evidence or the applicable 015 default, plus canonicalization specification/data.
- Surface assignment depends on explicit assignment/default rules and the exact vocabulary binding. A spelling-preserving vocabulary revision can invalidate admission evidence without changing message semantics.
- Identity resolution depends on owner scope, registry snapshot, continuity/reconciliation inputs, and complete versus partial inventory state.
- Reference/evaluation evidence depends on host expressions and source ranges even when an Intent revision is unchanged.

Locale requests, policies, glossary revisions, Store snapshots, and target changes may invalidate downstream work without invalidating the host syntax parse or allocating new Intent IDs. A consumer must not use `IntentRevision` as a cache key for source evidence or all planning inputs.

Fresh, reused, incremental, and differently scheduled runs over equivalent admitted inputs must produce equivalent logical results. Source maps and source coordinates may differ only when their own source inputs differ. Read-only compilation performs no registry, Store, or network mutation.

## Security and Resource Limits

Source, metadata, registry contents, module summaries, and context are untrusted inputs until admitted. Discovery never executes module initializers, property accessors, macros outside the selected trusted Producer mechanism, arbitrary plugins supplied as source, user functions, or template substitutions to find text.

The invocation must have finite limits for source-unit count/bytes, host parser capacity, declaration/reference count, extracted MF2 bytes, mapping segments, metadata bytes/depth, alias/module traversal work, finite-selection members, registry entries/candidate associations, and retained diagnostics/evidence. Work exhaustion is reported separately from an ordinary semantic mismatch. Partial scans never authorize retirement.

The first reference implementation uses caller-owned scheduling and safe APIs. Digests establish identity/integrity, not authorization. Registry continuity is not a signature, and source-derived metadata is not authority to execute instructions in later Provider prompts. 018/022 own safe transport and trust treatment when context leaves the compiler.

Absolute paths, source snippets, parameter values, and user-provided descriptions must not enter telemetry or profiler labels by default. Runtime parameter expressions are never evaluated or serialized as values by this stage. Exclusion is not sanitization and does not confer trust on excluded UGC.

## Performance and Measurement

The [026 Performance Implementation Architecture](./026-intlify-conformance-and-measurement-design.md#performance-implementation-architecture), [Profiling Specification](./026-intlify-conformance-and-measurement-design.md#profiling-specification), and [Adoption and Verification Requirements](./026-intlify-conformance-and-measurement-design.md#adoption-and-verification-requirements) apply alongside each adopting implementation phase.

### Storage and hot paths

- Parse each physical source snapshot once per admitted grammar/profile rather than reparsing per recognizer, message, or reference. Reuse parser-owned MF2 facts at an exact semantic revision.
- Start with cheap binding/node-kind checks, then perform bounded receiver/reference proof. A text search may locate candidates but never replace semantic recognition or absence proof.
- Keep host ASTs, source snapshots, extracted-message storage, scratch indexes, immutable authoring results, and optional projections in explicit lifetime classes. Host-specific nodes never enter shared artifacts.
- Prefer bounded reusable vectors/maps/buffers in a per-worker Scratch Workspace. Use an arena only for an actual common lifetime demonstrated by measurements; retained results must not borrow resettable storage.
- Intern repeated admitted names or use dense local IDs for repeated traversal. A fast local hash, including an FxHash-style map where appropriate, is not a persistent ID/digest and must not expose unbounded attacker-controlled collision work.
- Use safe byte-search primitives such as `memchr` for measured delimiter/escape scans where useful. SIMD, custom layouts, and unsafe paths require the separate evidence and review described by 026; they are not prerequisites for this design.
- Keep registry candidate search indexed and bounded. Do not compare every declaration against every historical source fragment or run an unbounded global similarity search on each build.
- Construct snippets, expanded maps, detailed explanations, and host projections only when requested, without disabling minimum correctness evidence or required diagnostics.

### Initial measured operations

The proposed component vocabulary is below. Exact Method Descriptors, cases, and checksum projections must be frozen with the implementing slice; these labels are not claims of existing benchmarks.

| Operation | Core interval | Important observations |
| --- | --- | --- |
| `source-discovery` | Admitted in-memory source/profile to classified declarations/references, including host parsing and binding analysis | Source bytes, parse attempts, visited nodes, proof steps, declarations/references, complete/blocked outcome |
| `message-analysis` | Extracted messages to parser/semantic facts and parameter requirements | Message count/bytes, literal versus MF2 cases, parser/semantic outcomes, reuse state |
| `context-resolution` | Source semantics and admitted projected context to locale/class/context facts | Declaration count, explicit/default locale cases, canonicalization/assignment work |
| `identity-reconciliation` | Admitted base/current inventory to association decisions and a proposed plan | Entries, candidate comparisons, ambiguous/new/continued/retired counts, exact plan observation |
| `authoring-result` | Admitted component facts to complete immutable result and requested projections | Output/evidence bytes, ownership transfer, logical result checksum, allocation domain when measured |

File discovery/I/O, registry publication, package resolution, and host/native transfer are separate product-workflow or host-boundary cases. An optional parsed-input recognition measurement is a separate Method Descriptor, not a relabeling of the parse-inclusive `source-discovery` interval. ID allocation outside read-only compilation is measured separately from deterministic reconciliation with a fixed allocation input. Requested-locale count must not multiply source parsing work.

Required workloads include no candidates, many short UI literals, repeated equal text with distinct IDs, complex MF2, sparse/dense parameter maps, many references to one declaration, long alias chains, large finite selections, ambiguous copies, complete versus partial inventories, invalid source, and exact/first-over limits. Repeated workspace use must agree with fresh execution after success, failure, and cancellation.

The initial gate checks semantic observations, required-case coverage, record integrity, and fresh/reused equivalence. Physical time/allocation observations do not become numeric pass/fail thresholds without 026's applicable method, comparison, and budget requirements. Optional profiling is feature-isolated and uses controlled labels; profiler observations are not substituted for primary benchmark samples.

## Conformance and Fixtures

The implementing subset must provide independent expected logical results, not only round trips or snapshots regenerated by the Producer under test.

| Fixture family | Required examples |
| --- | --- |
| Binding identity | Named import, alias, shadowing, unrelated `intent`, unsupported wrapper/member access, and conflicting binding configuration |
| Automatic UI | Static `textContent` assignments from each admitted `document.querySelector`/`document.createElement` origin, shadowed/custom `document`, other DOM APIs outside the initial profile, immutable receiver alias, non-DOM `textContent`, type assertion without receiver proof, null-guarded use, unknown value at known sink, and explicit authoring inside a known sink without double extraction |
| DOM receiver aliases | Simple local `const` chains within one function or at module level; unsupported `let`/`var`, object/array-mediated references, argument/return flows, and references crossing into another function |
| Receiver escape | Unanalyzed call receiving a tracked DOM value, invalidation across its tracked aliases, blocking diagnostics at later automatic assignments, no localization error from the call alone, and explicit `intent()` authoring without restoring automatic receiver evidence |
| Non-call receiver effects | `textContent` redefinition, prototype changes, object/array storage, exposure outside the tracked scope, consecutive supported `textContent` updates, and valid `const` aliases that preserve but never restore invalidated evidence |
| Receiver control flow | All reaching branches retain evidence, one reaching branch invalidates it, loop-carried invalidation from a prior iteration, and incomplete or limit-exhausted analysis that must not admit automatic localization |
| Exclusion | Literal and dynamic values with reasons, malformed/nested markers, inspect evidence, unchanged host value semantics, and no resulting Intent/reference |
| Metadata | Exact attachment, malformed JSON/encoding, omitted optional fields, empty/non-string values, duplicate/unknown fields, multiple annotations without merging, multiple or absent candidate declarations, no search past unrelated statements, no function/block-wide application, scope changes, missing/invalid class, explicit scope defaults, and invalid reference-side overrides |
| Scope defaults | Caller-supplied class for the declared analysis scope, per-declaration override, absent/unknown class, unchanged referenced/imported declarations, and absent descriptions without a shared default |
| Extraction | Literal braces/backslashes, explicit MF2 interpolation, matching cooked content from `intent()` literals and `mf2` tags (including newline and MF2 backslash escapes), multiline content, no dedent/trim, invalid cooked escapes and Unicode, CRLF and multi-byte source mapping |
| Parser ownership | Syntax versus semantic diagnostics, skipped dependent work, invariant failure, and identical parser meaning across authoring forms |
| Parameters and selection | Missing/extra/duplicate names, unsupported object forms, side-effecting values, conditional descriptors, incompatible alternatives, and unbounded source/reference rejection |
| Locale handoff | Explicit locale, inherited canonical default, absent default, invalid/unsupported canonicalization input, changed canonicalization binding, and preserved library source locale |
| Identity equivalence | Same declaration after justified edit/move, same text at distinct declarations, shared descriptor references, unchanged semantic projection with changed source evidence, and significant wording/context edits |
| Revision comparison | Independent equality/change pairs for host quoting and value-expression changes, literal characters/spaces/newlines without Unicode normalization, MF2 parameter/function/options, branch additions/removals and order, local names/initializers/declaration order/references, canonical locale and description/usage, and separate policy/class/Provider dependencies |
| Identity ownership | Equality of complete owner/local-ID pairs, equal local values under different owners remaining distinct, application/library identity separation, and rejection of new allocations colliding with active or retired IDs in the same owner |
| Continuity evidence | Exact unchanged-snapshot association; checked edit and cross-file move; stale registry/source attachment; conflicting one-to-many or many-to-one mappings; text/path/position-only matches; missing or ambiguous evidence requiring reconciliation without replacement-ID allocation; and retained identity with a changed semantic revision |
| Reconciliation | New allocation, collision with active/retired ID, ambiguous copy, explicit split/merge/restore, mismatched base, simultaneous conflicting updates, and atomic accepted publication |
| Registry update history | Exact owner/base/source/decision/result linkage, mismatched decision or source attachment, competing updates from one base with the stale update rejected, all-or-nothing publication with no partial current associations, and ordinary compilation leaving the registry unchanged |
| Automatic continuation updates | Explicitly enabled versus disabled mode, verified one-to-one edits/moves accepted without per-update confirmation, ambiguity requiring an explicit decision, unresolved plans or stale bases preventing publication, and retained IDs with changed semantic revisions and unchanged read-only compilation behavior |
| Automatic allocation and retirement | Enabled/disabled mode, confirmed-new versus unresolved continuation, replayable allocated IDs and active/retired collision rejection, complete-inventory absence with all identity decisions resolved, and state-only retirement without deleting history or reusing IDs |
| Registry recovery | Missing/corrupt registry not treated as first-time initialization, checked recovery from retained history, stale or foreign-owner candidates, invalid source associations, no reconstruction by text/position, and stopped updates when valid recovery is unavailable |
| Inventory completeness | Caller-supplied owner/list/snapshots, declared complete versus partial scope, missing or unsuccessfully analyzed units, deleted unit in a complete inventory, omitted unit in a partial view, parse failure/cancellation, unreferenced live declaration, and no accidental retirement or complete-build claim from partial results |
| Module references | Shared declaration retains ID/revision/locale/metadata across uses, consumer defaults do not overwrite it, missing or revision-inconsistent targets, non-finite target sets, and import forms outside the explicitly supported profile |
| Test-only Profile inputs | Explicit minimal owner/default-locale/vocabulary inputs, absent required information without hidden defaults, test-owned or PR #205 private inputs not admitted as a complete production Profile, and required production-subset adoption checks |
| Handoff | Stable ID distinct from target handle, source artifact without Provider work, finite references without final reachability claims, and blocked result never accepted as complete input |
| Performance safety | Exact/first-over limits, workspace reset, bounded registry lookup, fresh/reused equality, deterministic parallel merge, and optional instrumentation isolation |

Host evaluation-order preservation must eventually be tested against the actual lowering consumer in 024/028. Producer-only facts are not proof that generated code behaves correctly. Similarly, native/mobile equivalence cannot be claimed from a JavaScript-only fixture suite.

## Implementation Phasing

These are proposed implementation-readiness gates, not PR boundaries or a claim that implementation has begun. Each phase adopts the applicable 026 checks at its own active operation.

| Phase | Scope | Completion condition |
| --- | --- | --- |
| 1 — Shared authoring semantics | Literal/MF2 extraction distinction, parser handoff, parameter requirements, source-locale/context/class inputs, and semantic revision projection | Language-neutral logical fixtures pass, unsupported inputs stay explicit, and 015 fixture projections cannot be mistaken for complete production Profiles |
| 2 — Initial JavaScript/TypeScript Producer | Exact bindings, explicit forms, accepted metadata/exclusion syntax, bounded DOM recognition, source maps, and local finite references | Recognition and diagnostic fixtures pass; each source is parsed under an explicit profile; no duplicate extraction or host-code execution occurs |
| 3 — Persistent identity and reconciliation | Registry admission, stable association rules, replayable allocation inputs, conflict/restore/retirement handling, and read-only compile operation | Necessary 017 representation/identity decisions are fixed; independent history fixtures pass; production publication adopts the applicable 018 authorization slice and host adapter exact-base/atomicity checks |
| 4 — Shared consumer handoff | Versioned Intent/source/reference artifacts, admitted module/library references, graph dependency/diagnostic projection, and broader bounded selection | Adopted 017/019 interfaces validate complete versus partial outcomes; consumers cannot infer source approval, final reachability, or target validity from authoring success |
| 5 — Integration and conformance closure | 020/024/028 source-first integration, evaluated host behavior, adopted-case inventory, and performance baseline evidence | One supported Web path proves source → stable Intent → checked reference → preserved host execution, with complete conformance and measurement records for the claimed subset |

The accepted startup rule permits Phases 1 and 2 to use finite, explicitly test-owned project/registry inputs while shared encodings are being completed. Minimal profile inputs state the information needed by 016, including owning context, optional default source locale, exact surface-class vocabulary, and the applicable supplied canonicalization binding. Missing information must not be filled with hidden defaults.

Neither these fixture inputs nor PR #205's minimum configuration/locale-core results constitute a complete production `LocalizationProjectProfile`. They may exercise the declared subset, but that experiment is not persistent-identity support or full authoring conformance.

Before production use of persistent IDs or shared artifacts, adopt the necessary checked 015 inputs and 017 representation/admission specifications for that supported subset. This does not require every feature of 015 or 017 to be finished. Production identity claims still require Phase 3 and its other admission/publication checks; end-to-end localization claims require the corresponding downstream adoption, not merely a checked Producer result.

The existing `intlify_producer_js` offers reusable host parsing, bounded static analysis, source grouping, and scheduling/cache foundations, but its current configured-callee/key-reference model is not the source-first Intent specification. Reuse code selectively without making that format authoritative. The PR #183 implementation remains behavioral evidence, not a migration protocol or stable public API.

## Decision Log

`Inherited` records a constraint from existing designs. `Proposed` records an initial choice to review interactively; it does not represent user acceptance. `Accepted` records an explicitly agreed design choice, not an implementation claim.

| ID | Decision | State | Rationale |
| --- | --- | --- | --- |
| 016-001 | Separate language-neutral authoring/identity meaning from host recognition and later source rewriting | Inherited | Keeps host ASTs out of shared artifacts and preserves 000 ownership |
| 016-002 | Accept static MF2 source or a reusable `mf2` declaration as the first `intent()` argument; put only interpolation parameters in the optional second argument | Accepted | Supports inline messages without mandatory nested tags and explicit declaration sharing with the same Intent identity, without metadata/parameter ambiguity |
| 016-003 | Limit initial automatic recognition to static `textContent` assignments on receivers originating from admitted standard `document.querySelector()` or `document.createElement()` bindings | Accepted | Establishes a bounded Web surface without claiming all strings or DOM APIs; property spelling, type assertions, and a shadowed/custom `document` do not prove a receiver |
| 016-004 | Distinguish ordinary literal text from explicit MF2 and use one MF2 semantic pipeline | Inherited | Prevents accidental placeholders and divergent message semantics |
| 016-005 | Use the same host-cooked MF2 content for `intent()` source literals and `mf2` tagged templates | Accepted | Keeps JavaScript escape handling consistent across inline and reusable authoring; MF2 backslashes must also be escaped at the JavaScript layer |
| 016-006 | Keep explicit exclusions and static semantic metadata separate from parameter objects | Accepted | Preserves inspectable exclusions and metadata while keeping `intent()` parameters interpolation-only |
| 016-007 | Resolve source locale and surface assignment from explicit source/checked inputs, never ambient locale or inferred vocabulary | Inherited | Respects 015's defaults, absence, and vocabulary ownership |
| 016-008 | Separate declaration identity from references, semantic revisions, and target-local handles | Inherited | Preserves history without developer-maintained translation keys |
| 016-009 | Make compilation read-only and registry updates exact-base, explicit, and atomic | Accepted | Makes identity changes reviewable and prevents nondeterministic build mutations |
| 016-010 | Use conservative finite references and require compatible parameter requirements in the first selection form | Proposed | Gives planning a complete finite set while keeping evaluation-order obligations explicit |
| 016-011 | Require complete-inventory absence for retirement and never transfer approval through lineage links | Proposed | Refines the inherited retirement/history rules without inferring deletion from partial or unreachable source |
| 016-012 | Activate owner measurement, storage-lifetime rules, and optional profiling isolation with implementation phases | Inherited | Applies 026 without creating a separate performance implementation phase |
| 016-013 | Name the explicit exclusion marker `noIntent()` | Accepted | Pairs with `intent()` and expresses deliberate exclusion from Message Intent generation, not merely an absent declaration |
| 016-014 | Use Diagnostic for structured error, warning, and informational records | Accepted | Uses one term for parser and authoring reports while preserving component-owned codes and the shared reporting responsibility |
| 016-015 | Require a nonempty static string reason as the second `noIntent()` argument | Accepted | Preserves an inspectable explanation for intentional exclusion instead of inferring why a value should not be localized |
| 016-016 | Recognize authoring intrinsics by binding to registered module exports; initially support direct named imports and their import aliases | Accepted | Avoids confusing unrelated or shadowed names with authoring APIs while keeping wrapper, namespace/member, reassigned-alias, and unregistered re-export tracing outside the first profile; package resolution remains with 029 |
| 016-017 | Write declaration metadata as JSON in an `@intlify` block comment, with optional `sourceLocale`, `surfaceClass`, and `description` members | Accepted | Gives ordinary UI text and explicit message declarations one compiler-readable metadata syntax without changing the authoring API arguments |
| 016-018 | Attach an annotation only to the next statement in the same statement list containing exactly one eligible declaration; reject missing or ambiguous targets | Accepted | Prevents metadata from silently moving to a later message, crossing scopes, or applying to an entire function or block |
| 016-019 | Validate annotation JSON against the three allowed optional members with nonempty string values; reject malformed JSON, unknown/duplicate members, invalid values, and multiple annotations without merging | Accepted | Reports authoring mistakes before localization context can be silently lost or changed; absence is expressed by omission |
| 016-020 | Allow one caller-supplied optional default `surfaceClass` per authoring invocation, overridden by explicit declaration metadata; retain 015 source-locale defaults and no shared description default | Accepted | Avoids per-message classification boilerplate without source-based vocabulary inference, new configuration fields, or silently shared message descriptions |
| 016-021 | Trace DOM receiver aliases only through bounded simple `const` initializations within one function or at module level; exclude mutable bindings, containers, and cross-function flows | Accepted | Preserves recognizable local aliases without requiring interprocedural or heap analysis; a fixed binding is not evidence that the DOM element is immutable |
| 016-022 | Invalidate automatic DOM receiver evidence across tracked aliases after passing the receiver to unanalyzed code; block later automatic localization while permitting explicit `intent()` authoring | Accepted | Reports the affected message instead of assuming unchanged display behavior or silently excluding it; explicit message authoring does not require re-establishing the DOM proof |
| 016-023 | Require valid receiver evidence on every potentially reaching execution path, including loop-carried effects; diagnose incomplete or limit-exhausted proof | Accepted | Prevents branch joins or first-iteration-only analysis from restoring unsupported automatic localization while preserving host control flow |
| 016-024 | Invalidate receiver evidence after display-property redefinition, prototype changes, or non-call escape; preserve existing valid evidence through ordinary supported `textContent` updates and `const` aliases | Accepted | Distinguishes changing displayed content from changing display behavior or losing track of the receiver; diagnoses later automatic candidates without forbidding explicit authoring |
| 016-025 | Retain Intent IDs through exact unchanged-snapshot associations or verifiable one-to-one edit/move evidence bound to the base registry and old/current source snapshots; require reconciliation when continuity cannot be established | Accepted | Preserves justified history without inferring identity from wording or location, silently allocating replacement IDs, or confusing identity continuity with unchanged semantic revision |
| 016-026 | Identify an Intent by the complete application/library owner identity and owner-local opaque ID; reject new allocations colliding with active or retired IDs in that owner, leaving concrete formats to 017 | Accepted | Keeps independently owned Intents distinct without embedding source meaning or location in IDs, and prevents retired IDs from being reassigned to new message lineages |
| 016-027 | Retain exact registry base/source/decision/result linkage and publish the complete update only through an atomic current-base check; reject a stale base for rechecking and replanning | Accepted | Makes accepted history inspectable without losing intervening updates or exposing partial associations; 017 owns representation and 029 owns persistence and update UX |
| 016-028 | Permit automatic acceptance of verified one-to-one continuations only in an explicitly enabled development update mode; require explicit decisions for ambiguous history choices and preserve read-only compilation and atomic publication | Accepted | Avoids per-edit confirmation for proven continuations without weakening evidence, revision/freshness, or publication checks, or implicitly authorizing other update classes |
| 016-029 | Permit automatic ID allocation for confirmed new declarations in the enabled development update mode, with host-generated candidates fixed in the plan and checked against active/retired owner IDs | Accepted | Removes per-message allocation confirmation without treating unresolved continuity as newness or adding nondeterminism to normal compilation |
| 016-030 | Permit automatic state-only retirement in the enabled development update mode after complete owning-scope analysis proves absence and all identity associations are resolved | Accepted | Keeps partial views, failures, and unreferenced live declarations from erasing identity history or making retired IDs reusable |
| 016-031 | Separate initial registry creation from recovery; validate retained history before reuse and stop updates if recovery cannot be established | Accepted | Prevents missing/corrupt registries, stale backups, or text/location matching from silently resetting existing identities |
| 016-032 | Compare decoded/parsed literal content without trimming, whitespace compression, or Unicode normalization; ignore host quoting, positions, and formatting that preserves structure and content | Accepted | Preserves actual message characters while avoiding revisions caused only by equivalent host spelling or layout |
| 016-033 | Include MF2 external parameter names and function/options in revision; exclude host value-variable/expression changes when message-side requirements are unchanged | Accepted | Separates changes to the localized message requirements from changes to application value computation |
| 016-034 | Include branch conditions/bodies, additions/removals, and selector/variant order in revision without inferring all-input equivalence | Accepted | Uses a conservative structural comparison instead of assuming reordered or rewritten branches have identical behavior |
| 016-035 | Include MF2-local names, initializers, declaration order, and reference relationships; do not normalize through alpha-renaming or expression expansion | Accepted | Keeps the first projection bounded and inspectable even when different MF2 structures happen to render the same output |
| 016-036 | Include canonical source locale, description, and semantic usage in revision; retain class/policy/glossary/requested-locale/Provider changes as separate dependencies | Accepted | Preserves the distinction between message meaning and other localization inputs without suppressing downstream revalidation |
| 016-037 | Receive caller-supplied finite source inventories, snapshots, and owner scope with explicit complete/partial coverage; do not discover the repository in the Producer | Accepted | Makes result scope explicit and prevents missing/failed units or partial views from satisfying a complete build or authorizing retirement |
| 016-038 | Resolve supported module references from caller-supplied verifiable target information and retain declaration-owned identity, revision, locale, and metadata | Accepted | Shares declarations across uses without consumer-side redefinition and rejects missing, revision-inconsistent, or unbounded targets without claiming all import forms |
| 016-039 | Allow explicit test-only minimal profile inputs in Phases 1–2; require the adopted checked 015 inputs and 017 representation/admission subset before production use | Accepted | Enables bounded implementation work without promoting private/test inputs to a complete Profile or requiring unrelated upstream features |

## Open Questions

Q1 is resolved by decisions 016-002, 016-013, 016-015, and 016-016; Q2 by decision 016-005; Q3 by decisions 016-017–016-020; Q4 by decisions 016-003 and 016-021–016-025. Q5's 016-owned identity and update-history rules, including the division of responsibilities, are resolved by decisions 016-009, 016-026, and 016-027. Concrete ID, registry, and decision encodings remain a required 017 extension before Phase 3; resolving the logical rules here does not define those formats.

Q6 is resolved by decisions 016-028–016-031; Q7 by decisions 016-032–016-036; Q8 by decisions 016-037–016-039. The originally listed Q1–Q8 items are therefore resolved at the logical-rule level owned by 016. The revision comparison rules and fixture requirements above supply the equality/change expectations; the implementation must provide independent fixtures for the adopted parser/profile revision.

These agreements do not mark other choices still recorded as `Proposed` as accepted, define 017's wire formats, or turn test inputs into production admission. The remaining representation, workflow, and integration prerequisites stay with their owning documents and the implementation-readiness gates above.

## Deferred Follow-Up Notes

- Vue/template, JSX/TSX UI, mobile, and native authoring profiles should refine the same declaration, exclusion, context, and identity semantics in their owning integration documents.
- Rich structured semantic context, more ergonomic source annotations, message specialization, parameter-object inference, and finite container selection need separate versioned profile extensions.
- Registry file distribution, source-control merge tooling, automated update triggers, migration/recovery UX, and public authoring package layout belong to 029, constrained by the reconciliation rules here.
- 017 must expand beyond its current 015/measurement subset to encode persisted Intent identity, source/reference artifacts, and registry/evidence representations. This draft does not silently extend that subset.
- 018/019/020/023/024 remain owners of trust, graph/query, final planning, portable execution, and target/source lowering respectively. Their unfinished details are not redefined here.
- Broad similarity matching, cross-language refactoring inference, cryptographic source attestation, and long-history registry compaction are not prerequisites for the first bounded Producer experiment.

## Relationship to Other Documents

| Document | Relationship |
| --- | --- |
| [000 — Intlify overview](./000-intlify-overview-design.md) | Product direction and I0/I1 authoring/identity responsibilities; this document refines rather than replaces them |
| [001 — Toolchain foundation](./001-ox-mf2-toolchain-foundation.md), [012 — Parser semantic validation](./012-ox-mf2-parser-semantic-validation-design.md) | Shared source, syntax, and semantic facts reused by all authoring forms |
| [014 — Message linker](./014-ox-mf2-message-linker-design.md) | Existing reference-extraction and host-grouping implementation evidence; its key-oriented artifact semantics do not define this source-first interface |
| [015 — Project profile and locale policy](./015-intlify-project-profile-and-locale-policy-design.md) | Source defaults, canonicalization and surface-vocabulary handoff; a partial test core is not a checked production Profile |
| [017 — Shared artifacts and version admission](./017-intlify-shared-artifact-and-version-admission-design.md) | Required later representation of the semantic and identity rules defined here |
| [018 — Security, trust, and provenance](./018-intlify-security-trust-and-provenance-design.md), [019 — Project graph and queries](./019-intlify-project-graph-query-and-incremental-design.md) | Trust admission, authoring inventory/identity queries, common diagnostics, and incremental scheduling |
| [020 — Requirement planning and linking](./020-intlify-requirement-planning-and-linking-design.md) | Consumes checked finite references and Intent source facts; owns final requirement and reachability decisions |
| [023 — Localization execution](./023-intlify-localization-execution-specification-design.md), [024 — Target Profile and export](./024-intlify-target-profile-and-export-design.md) | Portable parameter/function meaning, target admission, and host-lowering obligations |
| [026 — Conformance and measurement](./026-intlify-conformance-and-measurement-design.md) | Performance architecture and verification requirements adopted by every active implementation slice |
| [028 — JavaScript/Web vertical slice](./028-intlify-javascript-web-vertical-slice-design.md), [029 — Product workflow and packaging](./029-intlify-product-workflow-and-packaging-design.md) | First integration evidence, public authoring packages, source/registry acquisition, and explicit update workflow |
