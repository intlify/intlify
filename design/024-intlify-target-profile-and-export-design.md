# Intlify Target Profile and Export Design

## Purpose

This design defines the smallest target export and host-lowering specification needed to turn [020](./020-intlify-requirement-planning-and-linking-design.md)'s complete selected-message handoff into [028](./028-intlify-javascript-web-vertical-slice-design.md)'s executable Web application.

[023](./023-intlify-localization-execution-specification-design.md) defines what evaluating a message means. 024 defines how a target admits that capability, represents the selected definitions, generates references and application code, and supplies a complete compatible output set. It does not choose another translation, acquire missing content, publish a Release, or require every platform to use one physical Runtime.

The initial outputs are two explicit modes of a minimum Web text profile. The Runtime-backed mode ships selected MF2 as data and prepares it through 027 before formatting. The ahead-of-time mode emits actual literal/interpolation code from the same checked semantics. Both keep application authoring unchanged, bind locale outside authored message calls, and preserve 023's exact text, argument failures, resource accounting, and bidi controls.

| Owner | Question answered |
| --- | --- |
| 020 | Which exact definition satisfies each requirement, and where is it logically placed? |
| 023 | What values, evaluation behavior, text, and failures does that selected message permit? |
| 024 | Which target can preserve that meaning, and which generated code, bindings, data, and mappings implement it? |
| 025 | Do all target outputs belong to one compatible Release, and may that Release be published and activated? |
| 027/028 | Can the actual reference Runtime and generated Web execution pair load, run, and demonstrate those guarantees? |

This is the initial application-only export slice for 016 Phase 5, not the complete target framework, Locale Capsule format, library linker, or production deployment design. A complete output set is an input to Release Assembly, not an already published or executable-by-authority Release.

## Goals

- Admit actual selected-message capabilities before generating an apparently usable output.
- Preserve exact Intent/revision, selected artifact, requested/definition locale, reference occurrence, and delivery associations.
- Define a small generated binding model without exposing application-authored keys, handles, or locale arguments.
- Preserve JavaScript evaluation order, declaration initialization, exceptions, exclusions, and original DOM assignment behavior.
- Produce genuinely different Runtime-backed and AOT paths with identical applicable logical observations.
- Define complete output dependencies, deterministic local ordering, source maps, and a non-circular Release handoff.
- Keep untrusted message text as data and separate generated-code trust from payload integrity.
- Apply 026's bounded generation, storage lifetimes, reuse, and whole-output measurement from the first implementation.

## Non-Goals

- Source discovery, Intent allocation/reconciliation, reachability inference, requirement construction, Store/governance changes, or translation selection.
- Public package names, CLI commands, bundler plugins, application installation conventions, or stable application-facing Runtime APIs.
- General JavaScript module transformation, TypeScript-specific syntax, JSX/Vue, SSR/hydration, mobile/native output, or source-first library distribution.
- MF2 declarations, selectors, explicit functions, markup/parts, number/date/plural formatting, or broader value and locale-service profiles.
- Lazy delivery, multiple Delivery Units per target, module graphs, partial locale installation, automatic negotiation, or runtime message-locale fallback.
- A portable binary execution IR, mandatory WASM engine, full Locale Capsule, cross-process handle serialization, or changes to existing ESM `0.1`.
- Release assembly/publication/activation, remote authenticity, revocation lookup, or deployment rollback implemented inside the exporter.
- Minification, tree-shaking optimization, localization-only releases, persistent caches, parallel generation, or numerical performance budgets as minimum prerequisites.

## Ownership and Dependencies

| Owner | Responsibility in this subset |
| --- | --- |
| 000 | Source-first generation, shared execution meaning, complete Target Profile output sets, and Release/execution separation |
| 002/012 | Actual MF2 parsing, decoded text/names, semantic construction, and parser-owned validation |
| 015 | Checked target/group/locale/topology facts, Target Profile references, applicable policies and binding projections |
| 016/017 | Checked source, intrinsic binding identity, Intent/revision, parameter/expression facts, source coordinates, shared representations, and version admission |
| 018 | Input acquisition/trust, disclosure, and applicable artifact/code admission powers |
| 019 | Complete immutable source handoff, dependency projection, and common Diagnostic |
| 020/021/022 | Exact source/localized selection and supply/governance evidence before export; no supply workflow is invoked here |
| 023 | Minimum text-interpolation semantics, strict Text values, bidi, failures, and logical resource limits |
| 024 | Target capability admission, binding/layout decisions, lowering plan, target payloads, output-set checks, and JavaScript value/result projection |
| 025 | Group-wide Release Assembly, publication/activation, and coordination of execution admission |
| 026 | Conformance/equivalence, performance architecture, measurement, and evidence admission |
| 027 | Reference MF2 preparation/evaluation, immutable Localizers, loaders/readiness, caches, and internal physical representation |
| 028/029 | Actual host rewriting/execution, trusted local acquisition/staging, browser tests, and later product packaging |

The tables below fix closed logical contents and owner obligations. They do not extend 017's existing authoring envelope or register new kinds in an unrelated reader. Shared wire schemas, digest domains, and exact version tuples must be adopted in 017 before those serialized artifacts are exchanged. 024 owns their target-specific meaning; a codec registration must not invent different lowering or execution semantics.

## Minimum Web Target Profiles

The minimum Web text profile has exactly two execution modes: `runtime-backed` and `aot`. A supplied Target Profile body chooses one mode and explicitly pins the following dimensions. Its 015 Target ID and 017 Target Profile reference remain distinct identities; neither a mode string nor a package version is a complete profile.

| Dimension | Initial supported value or obligation |
| --- | --- |
| Host | One admitted JavaScript ESM application under the bounded module profile below; standard DOM `textContent` |
| Execution | 023's exact minimum text-interpolation profile; strict primitive-string admission and plain-text output |
| Specifications | Exact MF2/parser-semantic, 023 execution, 024 target/binding/payload, host-lowering, and source-map codec revisions |
| Physical mode | Runtime-backed selected MF2 data, or generated AOT message functions; no automatic mode fallback |
| Runtime compatibility | Exact supported loader/binding/preparation adapter revision for Runtime-backed output; AOT code/helper profile for AOT output |
| Locale | Supplied supported requested locales and direct-only selection; retain the definition locale separately |
| Direction/services | Explicit message direction; unknown-direction Text with 023 isolation; explicit absence of locale services and data |
| Delivery | One supplied logical `["main"]` unit per target, no delivery edges, and eager preparation of all supported locales |
| Output | External JavaScript ESM modules, data-only JSON where used, target metadata, and required retained source maps |
| Limits | Finite source, message, reference, map, file, byte, preparation, argument, output, and retained-state capacities |
| Integration | Exact group/target/profile and selected-message associations; 025 admission before application construction/execution |

028 supplies `en` and `ja`, direct coverage, and `LTR` for both selected definition contexts. These are scenario inputs, not new configuration defaults or inferred locale directions. The complete 020 bundle carries six selected logical definitions and twelve placements across the two targets. Each export consumes its six target/unit placements while retaining that common selection basis. A target cannot select different wording to fit its backend.

The first emitter uses ECMAScript 2022-compatible external module syntax. Source support is bounded by the admitted host grammar and module rules, not a claim to lower every ECMAScript feature. Unknown or unimplemented profile/schema/ABI revisions fail explicitly; there is no version-range guess or fallback to the current exporter.

## Terminology

| Term | Meaning here |
| --- | --- |
| Export basis | Exact complete Bundle Plan, source snapshots/evidence, target/profile, selected bodies, specifications, and finite limits |
| Binding table | Immutable association of compact message slots and source-use slots with full Intent, parameter, and source identities |
| Reference template | Deployment-neutral generated reference to an exact binding table and its slots; not an application key or executable permission |
| Bound Message Handle | Private in-process reference established for an admitted Release/target/locale context; not an arbitrary supplied integer |
| Lowering plan | Complete snapshot-bound host edits, evaluation obligations, intrinsic-use accounting, and mapping inputs |
| Locale payload | One target/mode's representation of all selected definitions for a requested locale and the sole Delivery Unit |
| Output set | Complete immutable target payload inventory, dependency relations, generated bindings, and required evidence |
| Ready binding | Immutable locale-bound formatting entry and handles after all eager dependencies and compatibility checks succeed |

Message slots are not persistent Intent IDs. Equal text, slot numbers, filenames, or even equal bytes do not establish equal selected-artifact provenance or permission to combine Releases.

## Minimum Inputs and Results

Each export consumes actual immutable checked inputs, not labels claiming that an earlier stage succeeded.

| Input | Required content |
| --- | --- |
| Bundle | One complete current 020 Bundle Plan covering the selected group; all selections, placements, reachable references, and source/governance evidence |
| Target | One exact member Target ID, its actual supported Profile body/reference, group and Delivery Unit association, and applicable 015 binding projection |
| Messages | Actual source/localized artifacts and retained MF2/parameter facts; exact ArtifactDigest, Intent revision, requested and definition locales |
| Source | Exact 016/019 host snapshots, grammar/intrinsic mapping, declared root, descriptor/exclusion facts, parameter evaluation order, and source ranges |
| Generation | Pinned exporter/lowering implementation, binding/emitter/map profiles, trusted helper dependencies, and finite capacities |
| Admission | Required read/origin/disclosure checks; no registry/Store writer, Provider client, governance mutation, or Release-publication capability |

Success returns the complete lowering result, binding table, locale payloads, dependency inventory, target output metadata, source mappings, and retained admission/derivation observations. Failure returns owner-preserving diagnostics and no consumable partial output set. Separately authorized inspection may retain intermediate facts, but cannot label them a successful export.

The first core tests may construct closed test-owned inputs and run the actual validators. They cannot replace complete source/localized artifacts with 017 authoring records, promote PR #205's partial configuration core to a production Profile, or use fixture booleans as capability/trust evidence.

## Design Overview

| Step | Operation and checked result | Required separation |
| --- | --- | --- |
| 1. Admit basis | Verify complete bundle, exact target/profile, source snapshots, placement, bodies, and limits | No source rediscovery or new selection |
| 2. Admit capability | Apply actual 023 semantic/value requirements to every selected message | Parser validity and candidate validation are not target acceptance |
| 3. Plan references and lowering | Freeze slots, all source-use associations, bounded edits, and evaluation/mapping obligations | No application expression evaluation |
| 4. Emit target and host outputs | Produce actual Runtime data or AOT functions, bindings, transformed application, and maps | No publication or execution of emitted code |
| 5. Validate and freeze | Recheck payload/binding/source correspondence, dependency completeness, bytes, and typed outcome | A complete target set is still not a Release |
| 6. Assemble and run downstream | 025 admits the group/Release; 027/028 prepare and execute the two paths | Separate owners and acceptance gates |

An exporter operation handles one target while retaining the complete group-scoped selection basis. The host may generate the two targets sequentially. A successful target can be retained if the other fails, but the incomplete group cannot be assembled or presented as the completed 028 pair.

## Selected-Message Capability Admission

1. Verify the exact source/localized selections and target/unit applicability against the actual Bundle Plan. Reject missing, duplicate, extra, conflicting, or wrong-revision rows before generation.
2. Reuse compatible actual 012 parse/semantic results or run its parse → construct → validate gates. A source string, supplied name list, or parse-success flag is not an admitted semantic result.
3. Apply 023's closed allowlist to every node, including nodes an optimizer might otherwise erase. This admits one pattern of decoded literal runs and unannotated external variables only.
4. Derive the exact required canonical name set and Text requirements, compare them with the source and localized facts, and establish ordered occurrences and 023 logical work/output metadata.
5. Verify exact requested/definition locale equality, supplied direction, service absence, output mode, and supported physical adapter profile.
6. Preserve the complete derivation from the selected artifact and semantic/profile inputs to each generated record/function. Freeze only after all rows and required evidence succeed.

Valid MF2 with explicit `:string`, another function, declaration, selector, literal-operand expression, attribute, option, or markup remains unsupported by this target. Report capability failure, not an invented parser error. No stripping, guessed constant folding, host coercion, or alternate candidate selection makes it supported.

Runtime-backed export checks the full capability now even though 027 will parse/prepare the admitted MF2 at readiness. AOT must derive actual code from the same checked pattern, not fixture expectations or output observed by evaluating the message during the build.

## Binding Tables and Generated References

Each target has one immutable binding table for its sole Delivery Unit. It records the following separate relations:

| Relation | Required contents |
| --- | --- |
| Message slots | Dense zero-based ordinal → complete owner-qualified Intent ID, revision, canonical parameter specification, and Delivery Unit |
| Source-use slots | Dense zero-based ordinal → exact source/reference occurrence, message slot, parameter-expression/evaluation mapping, and owning diagnostic association |
| Locale selections | Each supported requested locale × message slot → exact selected ArtifactDigest, definition locale, context, and locale-payload entry |
| Binding basis | Exact target/profile, source/reference handoff, bundle association, and binding specification revision |

Assign message slots by 017's complete Intent ID order, then revision; reject conflicting current revisions rather than picking one. Assign source-use slots by admitted source-unit identity, source revision, and original UTF-8 occurrence range. Canonical parameter order is unsigned UTF-8 name order, not object-property evaluation order. These orders are deterministic for the exact basis; no slot stability is promised across changed bases.

Counts and ordinals are exact integers under the adopted representation. Conversion to a JavaScript index checks exact representability, actual collection bounds, and configured capacity before allocation or access. A number in range alone is not a valid reference. Empty checked demand produces empty message/selection relations, not a fabricated sentinel message.

Separate use slots preserve the two greeting occurrences in 028 even though they share one message slot. Their diagnostics and host expression positions stay distinct. Locale payloads use the same message-slot association, but retain their different selected artifact and definition locale. No name, DOM selector, or source-text hash becomes the runtime key.

A generated reference template is bound to the exact binding table and its message/use slots. After 025 admission, the host creates private bound handles for one Release, target, requested locale, unit, and binding table. Handles cannot be reconstructed from a caller's ordinal, Intent ID, filename, or `trusted` property. The adapter may use private identity tokens or equivalent checked storage; these tokens are not shared artifact digests.

The bound formatting entry validates the handle and use-slot association against its own ready context before evaluating a message. Mixing old/new Release handles, targets, locale contexts, tables, or unrelated occurrences fails explicitly. An unchanged deployment-neutral template may be reused in another Release only after that Release independently admits it and creates its own bound references.

## Source-Lowering Plan

A lowering plan is a complete, immutable proposal against the actual source snapshot, not a set of search-and-replace strings.

| Part | Required meaning |
| --- | --- |
| Basis | Exact source bytes/revision, grammar, intrinsic mapping, host profile, binding table, and checked reference artifacts |
| Replacements | Original half-open UTF-8 ranges or equivalent checked AST nodes, replacement kind, target message/use slots, and expected original node facts |
| Evaluation obligations | Receiver/assignment position, parameter expressions and original order, declaration initialization, exclusions, branch and exception behavior |
| Binding accounting | Every recognized intrinsic use and reusable declaration accounted for; permitted removal or replacement justified |
| Mapping inputs | Original host-to-MF2 associations, replacement segment relations, synthetic regions, and source-map profile |
| Outcome | Complete success only when all required uses, edits, generated dependencies, and maps are validated |

The 028 host backend applies this plan to its actual parsed representation. It rejects stale snapshots, invalid scalar boundaries, wrong node kinds, overlapping incompatible edits, unaccounted descriptor uses, exhausted proof limits, or incomplete maps. UTF-16 host offsets are converted against the exact UTF-8 snapshot; they are not reused as byte ranges.

Reparse the generated application under the pinned output grammar without executing it. Verify that all required references bind to the intended generated context and that no unsupported recognized intrinsic remains. A syntactically valid generated module with the wrong handle association is still invalid output.

### Bounded module construction

The first module profile admits 028's explicitly identified exported `render` function, intrinsic-only named imports, and immutable local `mf2` declarations. Ordinary control flow and side effects inside the admitted function remain application code. It does not infer roots from a function's spelling.

Generated output exposes a private application factory receiving one already admitted ready binding. It retains the original render signature inside that factory and returns that render entry to the test/application host. The host constructs separate instances for `en` and `ja`; no author-written locale argument, mutable module singleton, or DOM observer is introduced.

This wrapper is allowed only when the host checker proves that moving the admitted declarations into instance construction preserves the bounded module's behavior. General imports/re-exports and live bindings, observable module-level initialization, top-level await, `import.meta` or top-level context dependence, source-text introspection, direct eval/dynamic scope, and other context-sensitive forms not covered by that proof are unsupported. The compiler must not duplicate observable module side effects to obtain another locale instance.

Generated helper bindings use deterministic fresh lexical names checked against the whole affected scope, including free references that a new binding could capture. A fixed prefix is not sufficient hygiene. Synthetic setup does not evaluate parameter expressions, query DOM receivers, or change the original function's `this`, `arguments`, branch structure, or exception flow.

### Replacement rules

| Checked source form | Minimum lowering |
| --- | --- |
| Ordinary static DOM literal | Replace only the checked right-hand message expression with the bound formatting call for its existing declaration/use |
| Inline `intent(source, parameters?)` | Replace the intrinsic call with the generated entry and its checked message/use references; retain the original parameter-object expression |
| `intent(localDeclaration, parameters?)` | Keep a read of that local binding as the handle operand and preserve its lexical scope and initialization behavior |
| Immutable `mf2` declaration | Replace the tag expression with its bound handle at the original declaration position; keep the declaration where its reads require it |
| `noIntent(value, reason)` | Replace the marker with the parenthesized original value expression; retain exclusion/reason evidence separately |
| Intrinsic import | Remove only the accounted-for intrinsic-only import under the admitted side-effect-free fixture mapping |

Retaining the reusable binding avoids erasing a temporal-dead-zone read or moving initialization merely because the message identity is known statically. Descriptor escape, unsupported aliasing, or a descriptor used as a normal value is rejected, not turned into a runtime MF2 string. A completely unused side-effect-free descriptor declaration may be removed only with complete use accounting; this does not retire its Intent or mutate its source inventory.

The host assignment itself remains intact: receiver evaluation, property access, null behavior, setters, and assignment completion remain JavaScript operations at their original positions. No eager formatting is hoisted before a branch or receiver operation. Explicit `intent()` does not strengthen automatic DOM proof or change the property's behavior.

### Parameters and host values

The first generated calling convention keeps the authored parameter object expression, including property order and each value expression. The generated entry receives a bound handle, a source-use slot, and that evaluated object when present. New operands are immutable pre-established references or constants, not fallible loaders or callbacks.

The complete object evaluates before formatting argument checks begin. Its values are evaluated once in original JavaScript order; after that, the adapter may project those immutable values into canonical dense slots. Translated parameter order and repeated occurrences never determine host evaluation order. Do not introduce an IIFE, callback, reordered object, or hoisted temporary that changes `await`, exception, or lexical evaluation behavior.

Only 016's fresh, statically enumerable ordinary data-property object forms enter this generated convention. Spreads, computed members, accessors, methods, opaque parameter containers, and prototype-setting `__proto__: value` definitions are unsupported host forms. The last case must not be treated as an ordinary data property merely because its key spelling is static. An otherwise valid canonical parameter name is not globally banned by that host-syntax restriction.

The adapter reads only the proven object's own data entries, not inherited members or application getters. It rechecks the exact required set and values under 023. A missing object maps to the empty set only for a parameterless call; an empty string value is not missing. Static authoring validation does not remove required runtime value checks.

Accept only primitive JavaScript strings as Text. Scan UTF-16 without coercion: a high surrogate requires an immediately following low surrogate, and an isolated low surrogate fails. Reject non-string values before any string conversion; do not use replacement decoding, normalization, `toString`, or locale services. Compute each distinct value's exact scalar/UTF-8 length once under current limits and retain separate host UTF-16/storage bounds. Validation uses trusted host intrinsics established during adapter construction, not per-call method lookup through mutable application prototypes.

This is a generated-call binding, not a public API accepting arbitrary Proxy objects, runtime source strings, or user-built handles. Core entry-list fixtures can test duplicate/extra names without constructing a JavaScript object that has already lost duplicates. Host exceptions raised while constructing arguments propagate unchanged; the localization adapter does not wrap or replay them.

### Illustrative transformed shape

The following abbreviated example assumes the fixture's admitted slot mapping places Save, Welcome, and the greeting at 0, 1, and 2. Actual slot assignment follows the canonical identity rules above. Names here are explanatory, not public exports or a byte-level emitter template.

```js
export function createApplication(binding) {
  const format = binding.format
  const handles = binding.handles
  const greeting = handles[2]

  function render(name) {
    const save = document.querySelector('#save')
    const heading = document.querySelector('#heading')
    const first = document.querySelector('#first')
    const second = document.querySelector('#second')
    const brand = document.querySelector('#brand')

    save.textContent = format(handles[0], 0)
    heading.textContent = format(handles[1], 1)
    first.textContent = format(greeting, 2, { name })
    second.textContent = format(greeting, 3, { name })
    brand.textContent = 'Intlify'
  }

  return { render }
}
```

The full generated module also carries its exact binding-table requirement, omitted from this abbreviated body. The factory is reached only through the admitted host construction path. Its immutable binding supplies a receiver-independent formatting closure and checked handles, with no application-defined getters. Calling an exported-looking function with an arbitrary object is not the supported admission path or evidence of a secure sandbox.

## Target Output Set and Locale Payloads

The minimum physical layout uses one locale payload per supported requested locale in the sole logical Delivery Unit, plus the shared target roles below. Logical role and reference identity determine membership; filenames are only safe deployment-relative addresses.

| Role | Runtime-backed mode | AOT mode |
| --- | --- | --- |
| Generated application | ESM application factory with exact binding requirement and lowered source uses | Equivalent host lowering for the AOT binding |
| Binding metadata | Complete immutable slot/use/selection table and exact profile requirements | Same logical associations, with its own target/profile identity |
| Locale payload | Data-only JSON containing selected MF2 records for that locale/unit | ESM containing generated message functions and matching immutable entry metadata |
| Formatting dependency | Actual 027 Runtime/preparation adapter and all shipped dependencies | Generated AOT functions plus bounded argument/context/result helpers; no Runtime MF2 evaluator |
| Loading/construction | Closed eager dependency map and ready-binding construction adapter | Closed eager module map and ready-binding construction adapter |
| Target descriptor | Complete output inventory, byte-integrity references, dependency relations, and derivation/admission associations | Equivalent complete inventory for this physical mode |
| Source maps/evidence | Required generated-to-source maps and retained semantic/host derivation evidence | Required application and generated-message mappings/evidence |

The inventory distinguishes execution-required files from verification-only maps/evidence. Complete export validation requires both, but eager browser readiness follows only the execution dependency relation. Shipped bindings carry necessary safe identity/use associations, not full source snapshots, parameter-expression text, governance history, or credentials. Detailed derivation stays in authorized retained inputs; execution consumes the checked immutable projection required by 025.

For 028 this means two locale payloads per target, not twelve message files. Both locales are loaded/admitted before this initial target becomes ready; neither changing locale nor formatting initiates a lazy load. Required Runtime, WASM, helper, or platform-adapter files are counted if the selected implementation uses them. Calling several physical files one Delivery Unit does not remove their requests or initialization cost.

The runtime lookup relation is exactly requested locale × Target ID × logical unit → ordered admitted artifact references. There is no parent-locale or default-locale search. The AOT binding establishes the equivalent closed function/entry relation during construction. Missing locales or entries do not become empty arrays or source-language success.

### Runtime-backed message data

The minimum Runtime payload carries exact selected MF2, not a new binary AST or a claim to contain prepared Runtime state.

| Logical part | Required content |
| --- | --- |
| Format/basis | Exact target payload schema/profile, binding-table reference, target/unit, MF2 and execution specification pins |
| Locale context | Canonical requested locale and explicit service absence; each record retains its own definition locale and supplied direction |
| Records | Complete message-slot-ordered entries with exact Intent/revision, selected ArtifactDigest, exact selected MF2 string, and canonical required Text names |
| Preparation facts | Actual selected-definition/semantic derivation associations and bounded preparation inputs needed by 027 |

JSON decoding preserves exact scalar text and rejects duplicate members, unknown variants, malformed Unicode, invalid ordinals, and excessive input before constructing an admitted map. A JSON string is decoded once to the selected MF2; the shared parser then applies MF2 escaping. No host JavaScript escape decoding is applied a second time.

027 validates the admitted payload and parses/builds/validates its actual MF2 under the pinned parser-semantic profile, then prepares immutable evaluation state before readiness. It compares derived names and capability facts with the binding metadata rather than trusting supplied counts. Compatible retained preparation can be reused; repeated formatting does not parse source or rebuild name maps.

This keeps the initial transport limited to selected MF2 data and leaves parsing/preparation to readiness. A future compiled capsule may remove readiness-time MF2 parsing through a separately versioned target format; its own admission and preparation obligations still apply. JSON transport and the reference Runtime's internal prepared IR remain different objects and ownership domains.

### Ahead-of-time functions

For each selected message, the AOT emitter traverses the admitted decoded pattern and emits a straight-line function over already admitted Text slots. Literal runs become safely encoded JavaScript string data. Each variable occurrence reads its assigned immutable slot and inserts 023's FSI/value/PDI sequence. Repeated occurrences remain separate output contributions.

The generated call wrapper performs current context/handle/argument/resource admission first, then invokes the exact function selected at construction. It calculates 023's logical work and exact expanded UTF-8 output size from the admitted message facts and values before constructing text. It also checks physical JavaScript output capacity; successful constant folding never waives current call limits.

For example, after admission and bounds checks, the wrapper may call a generated function for the selected Japanese greeting:

```js
function message2(textSlots) {
  return 'こんにちは、' + '\u2068' + textSlots[0] + '\u2069' + '!'
}
```

The body is generated from actual MF2 semantics, not a table of expected render answers. It never receives source strings to parse, invokes the Runtime evaluator, performs generic node interpretation, or dispatches a formatting function by a translated name. Shared non-evaluating argument, Unicode, limit, and binding helpers are allowed; their shared use does not replace independent expected-result fixtures.

The AOT module carries a complete entry/metadata association to the binding table, selected artifacts, function ordinals, definition locales, and derivation inputs. Static constants and function declarations have no application side effects. Selected message content never controls imports, identifiers, property access, comments, or executable syntax.

### Deterministic emission and safe placement

Use one pinned emitter/lowering profile and explicit canonical ordering. The same complete inputs and producing implementations generate the same bytes. Generated helper names, local file roles, line endings, escape policy, and source-map emission are part of that profile; timestamps, absolute workspace paths, random IDs, unordered map iteration, or an ambient formatter are not.

Emit JavaScript strings through a checked scalar writer/AST emitter, escaping quotes, backslashes, control characters, U+2028, and U+2029 while preserving the resulting string exactly. Independently parse/round-trip adversarial literals. JSON encoding follows its own admitted codec. These are external module/data files, not HTML-safe inline script fragments; the host must not embed them directly into HTML or evaluate translation text.

Paths and import specifiers come from an exporter-owned finite role mapping. Reject traversal, absolute paths, scheme/authority injection, conflicting file roles, and unrepresentable host paths under the placement adapter. Do not derive an executable import, filesystem path, or JavaScript identifier from an MF2 name, description, Provider field, or DOM selector.

Normal generation receives/returns immutable bytes and logical paths. Filesystem staging is a host operation into a fresh private destination; it cannot overwrite the authored module or make partial new outputs active. A failed export preserves the previously usable output/Release.

## Source Maps and Diagnostic Associations

Retain the exact UTF-8 source-coordinate evidence from 016/017 and compose it with the actual host rewrite. Escapes and many-to-one ranges use segment associations, not arithmetic guesses. Synthetic factory/helpers and generated scaffolding are explicitly unmapped; a localized literal retains its selected-message association rather than a fabricated span in the original English text.

The initial browser map is a regular non-index source map using the [ECMA-426 source-map format](https://tc39.es/ecma426/). Its fixed JSON `version: 3` is not an Intlify schema revision. JavaScript columns use zero-based UTF-16 code-unit positions; convert from the admitted UTF-8 ranges against the actual original/generated text. Pin the supported map codec/specification in the host-emitter profile.

Use non-secret logical source locators and omit embedded `sourcesContent` in this minimum. Original snapshots and message/source associations remain available to the authorized verification host. Keep required maps/evidence as retained build outputs; making them publicly retrievable is a separate disclosure decision, not implied by browser execution. If the harness serves a map, its address and bytes must match the admitted output relation.

Required fixtures cover multibyte and supplementary characters, combining marks, CRLF, cooked escapes, repeated references, empty replacement ranges where applicable, EOF, and synthetic regions. An unsupported/unrepresentable required map fails complete lowering; diagnostics must not invent an original location to hide missing evidence.

## Output Integrity and Non-Circular Release Handoff

Construction follows an acyclic dependency order:

1. Freeze the binding table and selected-message associations. Its locale-entry locators are entry ordinals, not hashes of future locale payloads; it contains no output-set or Release identity.
2. Emit payloads, generated application/bindings, helpers, and maps referencing that binding basis. No payload embeds its own digest, the future descriptor digest, or a future Release digest.
3. Freeze the target descriptor over the complete final byte inventory and dependency/derivation relations. Every entry records its role, exact format, safe logical address, byte length, and admitted integrity reference. Apply 017's adopted self-exclusion/identity rules to the descriptor itself.
4. 025 assembles a Release that references the complete target descriptors and their exact dependencies. It does not patch already hashed target bytes to insert the Release identity.

Generated module/source-map address relationships need not be hash cycles: the enclosing descriptor pins their exact bytes and associations. Distinguish exact file-byte integrity from canonical record integrity and selected Message Artifact identity. None can be substituted for another merely because the values use SHA-256.

The complete set validator checks required roles, unique safe addresses, every dependency endpoint, all locale/unit/slot associations, application binding requirements, required maps, and actual bytes. It rejects missing files, conflicting duplicates, unlisted localization executable dependencies, altered metadata, and mismatched generation evidence. An internally self-consistent rehashed artifact is not proof of trusted generation.

Supply 025 with the exact project/group/target/profile, pinned Bundle Plan and selected definitions, binding table, output descriptor, all required payloads/dependencies, and applicable capability/derivation evidence. Both target sets must preserve the group-wide selection. 025 separately supplies the Release, publication record, activation association, and required execution-admission evidence.

## Execution Admission and Result Projection

The host first establishes the deployment-selected Release and exact output set under 025. It verifies the generated code/data bytes and their trusted origin before executable modules can run, not by importing an untrusted ESM file and inspecting its exports afterward. The trusted local profile must prevent replacement between verification and use; a mutable URL or matching filename is not that guarantee.

Eager construction then resolves the closed dependencies, verifies profiles and binding/locale tables, prepares Runtime state or admits AOT functions, and creates immutable ready contexts. Omitted dependencies, unsupported modes/ABIs, failed preparation, or unknown requested locales produce setup/admission failures. No partly ready binding is exposed to application code.

The first browser path uses an explicitly trusted local generation/staging/serving adapter with actual protected byte associations; it does not claim remote signing or sandboxing. 018/025/029 must adopt that adapter's code-origin and execution-admission rules before the real Web claim. Existing registry authority is not authority to execute or publish generated artifacts.

Once ready, the bound formatting entry is synchronous and returns one primitive string on success. It exposes localization failure through a typed exception carrying the immutable owning stage/reason/subjects and applicable 019 Diagnostic projection. Public exception names remain unreserved. An original application exception is propagated unchanged, not converted into a localization failure.

No failure returns an empty/source/partial string, assigns a failed message, triggers a Provider/Store request, retries another locale, or changes the Localizer. Earlier independent application effects remain intact; this is not a DOM transaction. Interleaving separately constructed `en`, `ja`, and `en` instances cannot change their bound Release, handles, or locale.

Format-call observations retain both message and use-site association where available. Raw parameter values, source snippets, and host exception text are not normal diagnostic/profiler fields. Generated artifact/setup failures use safe input subjects when source evidence is unavailable, and optional diagnostic rendering cannot change the failure class.

## Resource Bounds, Reuse, and Performance

Apply [026's performance implementation architecture](./026-intlify-conformance-and-measurement-design.md#performance-implementation-architecture) and 023's logical execution accounting. Do not describe smaller locale files alone as a complete performance result.

- Retain checked source/messages and derivation facts immutably. Share compatible parsing within the build; target count, requested locales, and repeated uses must not multiply identical source discovery.
- Use dense invocation-local slots and contiguous literal runs. Build canonical name/reference indexes once; no source-text search, per-use Store scan, or repeated string hashing is required in hot formatting.
- Keep source/AST storage, generation Scratch Workspaces, retained binding/semantic artifacts, output buffers, and Runtime prepared caches in separate lifetimes. No returned set, map, diagnostic, or bound handle borrows resettable scratch.
- Generate into bounded private buffers, accounting for escape expansion, generated wrappers, source maps, helper dependencies, file counts, per-file bytes, and total output bytes. Check arithmetic and budget before extending buffers; never publish a valid prefix.
- Bound source-edit and mapping work, submitted records before deduplication, dependency edges, name/value conversion, eager preparation, and retained/cache capacity. Distinguish logical 023 UTF-8 limits from host UTF-16 and physical storage observations.
- Reset scratch on success, failure, and cancellation; retain bounded useful capacity between invocations and release pathological high-water capacity explicitly outside the ordinary path.
- Reuse only with the exact consumed source, binding, bundle/selection, target/profile, emitter/helper/map, policy/admission, and relevant limit inputs. Cache eviction and a clean rebuild must reproduce the same semantic output and failure behavior.
- Keep current call limits and handle compatibility enforced even with cached preparation or AOT constants. A changed immutable execution context is newly admitted rather than mutating a previous Localizer.
- Start with sequential bounded generation and synchronous formatting after readiness. Safe byte-search/escaping and internal fast hashes may follow 026's measured input rules; SIMD, unsafe code, a universal arena, and persistent caching are not requirements.
- Keep optional snippets, expanded traces, and profiler instrumentation off the unrequested path. The required source maps and correctness/derivation evidence for this profile remain mandatory.

| Operation/surface | Minimum observations |
| --- | --- |
| Capability/semantic admission | Actual selected input bytes/nodes, admitted/unsupported outcomes, parse/reuse counts, logical work, and retained/scratch storage |
| Binding/lowering planning | Message/use slots, checked edits, source/evaluation associations, map segments, failures, work, and duration |
| Message/code emission | Runtime data versus AOT generation intervals, output bytes, escape expansion, scratch/allocation domain, and exact output identity |
| Host rewrite/maps | Actual transformation and map generation/validation intervals, source/output bytes, host materialization, and semantic trace association |
| Complete generated output | All application, bindings, locale, helper/engine, manifest/Release, and required delivery dependencies; raw/compressed bytes, eager closure, and requests |
| Loading and execution | Module/engine admission, both locales' readiness, Localizer construction, first/hot formatting, exact 023 outcomes, and explicit browser/JIT/cache state |

The adopting plan pins 026 Method Descriptors, workload/Run Plan, intervals, Memory Observation Domains, compiler/runtime/helper versions, independent checksums, and fresh/reused states before accepting evidence. Generation measurements do not include application execution; integrated build/browser measurements separately include their I/O and loading costs. Unsupported memory observers remain explicit, not zero allocations.

Start with descriptive validated baselines, not numerical speed thresholds. Correctness, bounded failures, and required evidence completeness are completion gates. [Optional profiling](./026-intlify-conformance-and-measurement-design.md#profiling-specification) is an isolated non-default build feature and does not supply primary timing samples.

## Conformance and Minimum Completion

Use actual shared MF2 analysis, emitted bytes, generated modules, and the real Runtime path. A file snapshot or test-owned result table cannot replace host execution or independent expected semantics.

| Case family | Required independent observations |
| --- | --- |
| Selection and scope | Complete versus partial bundle; missing/extra locale/slot; changed Intent revision/artifact; same text at separate identities; no reselection |
| Capability | Supported literal/interpolation; valid but unsupported MF2 features; malformed MF2 keeps parser ownership; no unsupported-node removal |
| Binding and identity | Three message slots and four use slots for 028; shared greeting handles with separate occurrences; deterministic reordered-input output; wrong-table/use/context rejection |
| Host evaluation | Original receiver/property/argument order, once-only values, unexecuted branches, thrown host expressions/setters, null receivers, local declaration initialization and temporal dead zones |
| Authoring/lowering | Imported aliases and shadowing, fresh-name hygiene, descriptor escape, unsupported module effects, exclusion value identity, no remaining unhandled intrinsic |
| Parameters | Exact required names, empty maps/Text, non-string and lone-surrogate failures, prototype-setting property rejection, no getters/coercion, repeated/reordered translated occurrences |
| Literal safety | Quotes, backslashes, line separators, CRLF, combining/supplementary characters, NUL, MF2-looking and script-like parameter text; exact Unicode and inert DOM output |
| Maps | Correct original positions and selected-message associations; UTF-8/UTF-16 conversion, escape mappings, missing/stale maps, and unmapped synthetic regions |
| Output integrity | Missing files/edges, conflicting addresses, wrong payload/profile/ABI, rehashed untrusted code, altered MF2/AOT association, and no self-reference cycle |
| Readiness and Release | All eager dependencies checked before construction, unsupported locale, no partly ready binding, mixed-Release rejection, immutable interleaved contexts |
| Failure and reuse | Exact/first-over limits and overflow, cancellation/failure followed by fresh/reused work, retained old results, no partial file/text success |
| Execution pair | Actual Runtime and AOT with the same selected artifacts/arguments/context; exact 023 text, bidi controls, failure observations, and logical work |
| Workflow/performance | Zero build/render Provider or governance calls, no runtime source discovery, no hot MF2 parsing, and complete scoped 026 evidence |

For the primary fixture, verify the six selected definitions and twelve logical placements independently from the physical four locale payloads across two targets. Each target retains its application/bindings/dependencies as well. The unchanged brand exclusion has no message slot or translation entry.

Run `Ada` and `Kai` through both locale-bound instances and both physical paths. With `Ada`, 023's greeting output has 16 English and 28 Japanese UTF-8 bytes, including FSI/PDI. Use those exact characters and independent resource-bound expectations; do not strip invisible controls or compare only screenshots.

The AOT path must be observable without importing/calling a Runtime MF2 evaluator. Its code must consume the actual changed arguments. The Runtime path must consume actual selected MF2 and perform its real preparation. Shared argument/limit helpers do not excuse a shared incorrect expected result.

Completion of this minimum requires one admitted [026 Logical Render Equivalence](./026-intlify-conformance-and-measurement-design.md#logical-render-equivalence) relation and the applicable real-browser checks in 028. Pure export tests can complete their own scope earlier, but cannot mark 016 Phase 5 complete before Release admission and Runtime/browser execution exist. Broader MF2, library, native, and hydration cases remain explicitly unclaimed.

## Adoption with 016 and 028

| Adopting work | Minimum prerequisite and completed scope |
| --- | --- |
| Capability and lowering-plan core | Actual 016/019/020/023 checked inputs, bounded target profile, complete reference/semantic validators, and independent plan fixtures |
| Target encoding and emission | Adopt the needed 017 target/binding/payload/output representations and integrity rules; produce/validate actual data, functions, maps, and dependency inventories |
| Reference execution integration | 027 implements this exact Runtime data/preparation/ready-binding adapter and 023 semantics; AOT implements generated functions and typed host projection |
| Release and local host integration | 018/025/029 establish actual code-origin, immutable staging, group/Release compatibility, and execution-admission inputs before importing modules |
| 016 Phase 5 / 028 gate | Run the real upstream supply/selection, both complete outputs, admitted Release, browser/host traces, and applicable 026 conformance/measurement |

These are adoption dependencies, not a claim that their implementations or the broader owning designs are finished. 016 Phases 1–3 remain independently implementable. A concrete plan must place required shared-codec and Release/Runtime additions before the integration operations that consume them.

## Decision Log

| ID | Decision | Rationale |
| --- | --- | --- |
| 024-001 | Start with one application-only Web text profile and two explicitly chosen physical modes | Completes the finite 028 target surface without general platform or module transformation |
| 024-002 | Check every selected message against 023 before emitting either mode | Parsing and candidate validation alone cannot establish target support |
| 024-003 | Keep compact message/use slots separate from persistent identity and bound Release handles | Reduces repeated lookup while retaining provenance, use-site diagnostics, and compatibility |
| 024-004 | Keep original parameter objects and reusable declaration initialization in the bounded rewrite | Preserves host evaluation order, exceptions, and temporal-dead-zone behavior |
| 024-005 | Bind locale through a checked generated application factory, not authored message arguments | Supports isolated concurrent application contexts without a mutable global locale |
| 024-006 | Use exact MF2 JSON data for minimum Runtime readiness and semantic-derived functions for AOT | Keeps the initial transport small in scope while providing genuinely different execution paths |
| 024-007 | Retain complete output dependencies and build the descriptor/Release without backward digest links | Makes integrity and Release composition checkable without self-reference cycles |
| 024-008 | Admit generated executable bytes before import and keep message content inert | A checksum or an ESM export inspection alone does not establish trusted execution |
| 024-009 | Require source/evaluation maps and independent host/semantic observations | Output text equality cannot prove a correct source transform |
| 024-010 | Apply bounded generation and whole-output 026 evidence from the first slice | Avoids hiding preparation, helper, transfer, or loading costs behind one smaller file |

## Deferred Follow-Up Notes

### Required follow-up for minimum Web execution

- 017: encode the actual complete source/localized-message and plan inputs, Target Profile bodies, binding/use/selection tables, locale payload metadata, output descriptors, and required references/digest domains. Register exact closed tuples and validators; do not reuse an authoring integrity digest as a selected ArtifactDigest or extend ESM `0.1` implicitly.
- 018/025/029: define the minimum trusted local generated-code acquisition/staging/serving and execution-admission profile, including protection against verification/use substitution. Existing source/registry authorization does not supply these powers.
- 025: define the smallest complete group Release over both target output sets, independent publication/activation evidence, and exact mixed-output/handle rejection. This is the next owner specification.
- 027: adopt the MF2-data preparation adapter, ready locale-bound Localizer, private handle/value/result projection, cache/lifetime behavior, and synchronous 023 formatting for the chosen Web implementation.
- 028/029: wire the actual lowerer, loader, generated factory, explicit locale construction, and both browser paths; retain required maps, trust checks, and complete scoped 026 evidence.

### Broader extensions

- A compiled Locale Capsule, binary/prepared representations, additional physical engines, or an explicit current-ESM bridge with its own compatibility specification.
- General module/TypeScript/framework lowering, source-first library composition, conditional descriptor selection, bundlers, hot reload, and public APIs.
- Rich values, functions, selectors, markup/parts, locale-service data, fallback-materialized definitions, SSR/hydration, and other platform targets.
- Lazy and multi-unit delivery, independently reusable locale packages, localization-only releases, production remote trust, and deployment packaging.
- Measured minification, specialized escaping/copying, persistent incremental caches, bounded parallel emitters, and additional performance budgets under 026.

## Relationship to Existing Foundations

- [000](./000-intlify-overview-design.md#deterministic-build-link-and-export-area) owns the compiler transaction and complete target/Release composition. This design does not move selection or publication into a Web helper.
- [016](./016-intlify-source-authoring-and-intent-identity-design.md#source-evidence-and-consumer-handoff) and [020](./020-intlify-requirement-planning-and-linking-design.md#final-linking-placement-and-export-handoff) supply the actual source, evaluation, and selected-definition inputs consumed here.
- [023](./023-intlify-localization-execution-specification-design.md#resource-accounting-and-output-ownership) owns text and resource semantics; [027](./027-intlify-reference-runtime-design.md#artifact-model) consumes this target specification without making its internal IR a portable format.
- [028](./028-intlify-javascript-web-vertical-slice-design.md#source-lowering-and-host-behavior) supplies the real host and browser acceptance scenario; [026](./026-intlify-conformance-and-measurement-design.md#execution-web-and-reference-runtime-integration) supplies the applicable evidence requirements.
- Existing [export preparation](../crates/intlify_export/src/lib.rs), [bounded writers](../crates/intlify_export/src/writer.rs), and [ESM scalar emission](../crates/intlify_export/src/esm/source.rs) are reusable foundations. Their resource-key-oriented records, `dev.intlify/esm-module` `0.1`, and `MessageRuntime<Result>` accessor are not this source-first binding/output specification.
