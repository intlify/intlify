# Intlify Compiler-Based Localization Overview Design

## Status

This document defines the high-level product architecture for Intlify as a compiler-based localization toolchain with a deterministic runtime.

It is one abstraction level above the component designs in this repository. In particular, [015](./015-ox-mf2-runtime-design.md) defines the runtime-side responsibility split, while this document explains how authoring, MF2 language services, localization synchronization, linking, target generation, editor and agent tooling, and runtime delivery form one Intlify system.

The source-first PoC in [PR #183](https://github.com/intlify/intlify/pull/183) and its review discussion are design evidence, not a frozen public API. This document fixes the overall direction and responsibility boundaries. It does not yet freeze:

- the exact JavaScript `intent()` signature or tagged-template API;
- automatic extraction rules for each host language and framework;
- the wire schemas of the source-first artifacts;
- the Translation Store storage protocol;
- Provider, TMS, review, or approval APIs;
- final CLI command names;
- Locale Capsule, Runtime Manifest, or generated-code ABIs; or
- package, crate, binding, plugin, or adapter names.

## Product Statement

> **Localization, Compiled.**
>
> **The Source-First Localization Compiler**
>
> Write messages naturally in your application. Compile validated localization across web, mobile, and native systems—without hand-maintained catalogs.

Intlify is not an application framework and does not own application rendering, routing, state management, or deployment. It is a composable localization toolchain and runtime that integrates with host languages, build systems, UI frameworks, localization services, and existing Translation Management Systems.

“Without hand-maintained catalogs” describes the application authoring model. It does not mean that target-locale messages are never stored. Intlify replaces catalogs as the developer-maintained source of application messages with compiler-managed, validated localization artifacts.

## Purpose

Traditional i18n application development usually has this shape:

```text
application code
  -> developer-authored message key
  -> manually maintained locale catalogs
  -> runtime key lookup and fallback
  -> parse and format
```

This model was reasonable when people translated and maintained every catalog entry directly. It also made message identity, translation storage, and runtime lookup the same developer-visible concept.

In an AI-agent and automation-friendly workflow, those concerns no longer need to be coupled. Application developers should be able to author the source-locale UI naturally. Localization Providers, TMS integrations, machine translation, AI, rules, and humans can all supply target-locale candidates through one checked workflow. A compiler can then resolve the finite localization requirements of the application, validate them, and generate target-specific code and immutable locale artifacts.

The intended model is:

```text
application-owned source messages
  -> statically discovered Message Intents and references
  -> explicit localization synchronization
  -> validated localized artifacts
  -> reachability, locale, fallback, and delivery linking
  -> generated application bindings and target locale artifacts
  -> deterministic locale-scoped runtime formatting
```

The resulting system keeps translation generation and remote services out of production rendering while removing hand-maintained keys and catalogs from the normal source-first application workflow.

## Problem Statement

### Authoring indirection

Message keys make application code describe storage locations rather than user-facing communication.

```ts
t('checkout.actions.pay')
```

The source text, parameter specification, UI usage, and target translations live elsewhere. Reviewers cannot understand the rendered UI from the application change alone, and a rename or refactor may require coordinated edits across code, catalogs, types, tests, and TMS state.

### Catalog maintenance

Hand-maintained catalogs accumulate stale entries, missing translations, duplicate meanings, inconsistent placeholders, and orphaned keys. Extraction and synchronization become recurring application-development tasks rather than compiler work.

### Late failure

Missing messages, invalid fallback, incompatible arguments, unsupported functions, and malformed translated syntax are often discovered only when a particular locale and route execute in production.

### Runtime over-responsibility

A conventional `t()` path may combine mutable locale state, catalog lookup, key fallback, loading, parsing, formatting, and error policy. This makes request-safe SSR, deterministic behavior, static optimization, and target-native output harder.

### Tool fragmentation

Extractors, formatters, linters, editors, TMS clients, build plugins, and AI coding agents often parse or infer the same message semantics independently. Their results drift because they do not share one parser, semantic model, artifact identity, or diagnostic specification.

### Delivery inefficiency

Applications frequently ship more messages, locales, and feature resources than a runtime path can reach. Dynamic catalogs prevent the build from proving completeness or pruning unreachable localization data.

### Uncontrolled automation

Calling an AI, MT service, or TMS implicitly during each build makes output depend on network availability, credentials, provider revisions, model behavior, and unreviewed candidates. The application build is no longer deterministic or reliably offline.

## Core Thesis

Intlify treats application localization as compilation.

A localizable message is not primarily a developer-authored storage key. It is a statically discoverable communication requirement with:

- source-locale MF2;
- parameter and selector specifications;
- source and UI evidence;
- stable compiler-owned identity and revision;
- locale, policy, glossary, review, and delivery requirements; and
- references from application or library code.

The toolchain turns those requirements and approved localized messages into target-specific application code and immutable runtime data.

This changes where responsibilities live:

| Concern | Traditional application responsibility | Compiler-based Intlify responsibility |
| --- | --- | --- |
| Message authoring | Choose a key and update a source catalog | Write source-locale UI or explicitly declare message semantics |
| Message identity | Developer-maintained string key | Generated, versioned identity and checked handle |
| Translation supply | Manually edit locale files | Provider, TMS, MT, AI, rule, or human adapter |
| Validation | Partial build checks or runtime errors | Shared MF2, parameter, policy, coverage, and target checks |
| Selection | Runtime key and fallback search | Link-time reachability, fallback, and delivery planning |
| Delivery | Catalog-oriented bundles | Target- and delivery-unit-specific artifacts |
| Runtime | Lookup, fallback, parse, and format | Load admitted artifacts and format an already selected message |

This is not `t()` renamed to `intent()`.

- `t()` is normally a runtime lookup operation over developer-authored identity.
- `intent()` is a candidate explicit authoring marker consumed by a producer at compile time.
- Simple, statically understandable UI text may require neither API.
- The compiler lowers all supported authoring forms to generated Message Handles and target runtime calls.
- The exact authoring syntax may differ across JavaScript, Vue, Swift, Kotlin, Rust, and other producers.

## Goals

- Let developers author ordinary static UI messages in source without maintaining message keys or source catalogs.
- Provide explicit, statically discoverable authoring for interpolation, selectors, reusable messages, headless messages, and advanced MF2.
- Use Unicode MessageFormat 2 as the message syntax and semantic foundation instead of defining an Intlify-specific message language.
- Make source discovery predictable: automatically compile only known UI surfaces and diagnose unsupported or ambiguous cases.
- Keep the shared compiler core independent of host-language ASTs and target-platform resource formats.
- Integrate AI, MT, TMS, rule-based, and human localization through provider-neutral interfaces and specifications.
- Separate remote synchronization from deterministic, offline-capable application builds.
- Validate syntax, parameters, policy, provenance, approval, coverage, fallback, reachability, and target capability before publication.
- Generate only the messages and locales required by the final application and its delivery units.
- Keep migration and compatibility decisions separate so that existing resource-oriented specifications do not constrain the source-first core.
- Provide structured compiler and semantic queries that can be reused by the CLI, editors, LSP adapters, build tools, and AI coding agents.
- Keep browser locale state application-scoped and server locale state request-scoped.
- Support Web, SSR, workers, iOS, Android, native applications, system languages, libraries, and CLIs without requiring one physical runtime implementation everywhere.

## Non-Goals

- Translating arbitrary runtime strings, user-generated content, logs, protocol values, or remote content automatically.
- Calling a Localization Provider, TMS, AI model, or machine-translation service from production formatting.
- Claiming that localized resources, translation storage, provenance, or human review disappear.
- Inferring localization semantics from every human-readable string in a general-purpose programming language.
- Making an AI model the source of truth for parsing, identity, validation, linking, approval, code generation, or runtime behavior.
- Deciding whether current key- and catalog-authoring workflows remain permanent public compatibility surfaces.
- Making JavaScript, Vue, one bundler, one UI framework, one Provider, or one TMS part of the language-neutral core.
- Moving host-language parsing, framework lifecycle, locale negotiation, application routing, or UI rendering into the MF2 Runtime Core.
- Freezing every artifact schema and public API in this overview.

## Product Design Principles

### Source-first is the authoring source of truth

Application-owned UI and messages are authored in application source, either as ordinary UI text on a recognized surface or through an explicit source declaration. The Translation Store, Locale Capsules, and target-native resources are managed outputs or integration artifacts, not developer-maintained authoring inputs.

The current resource-oriented implementation is evaluated separately as an implementation-reuse and compatibility follow-up. It does not appear in the primary source-first architecture and does not define the source-first compiler specifications.

### Automatic where provable, explicit where meaningful

A producer may automatically recognize compiler-owned or reliably known UI surfaces such as template text, static HTML text nodes, supported localizable attributes, or known DOM UI sinks.

When the producer cannot safely prove that a value is localizable or cannot follow its data flow, it reports a diagnostic. It does not silently translate every string or silently leave a known UI destination untranslated. Explicit `intent()`-like or `mf2`-like authoring makes the semantics unambiguous.

### Static messages, dynamic values

Message source remains statically discoverable. Runtime variation is expressed through typed parameters, MF2 declarations and selectors, or dynamic selection among statically declared messages.

An arbitrary runtime-generated source message is not sent to a translation service as a hidden fallback.

### MF2 is the message language

Intlify owns discovery, host-language integration, synchronization, validation orchestration, linking, target generation, and runtime integration. MF2 owns message syntax and message semantics.

The `ox-mf2` parser and semantic foundations are shared across compiler, formatter, linter, editor, agent, export, and runtime-preparation workflows.

### Synchronize remotely; build and run locally

`intlify sync` is the conceptual network and credential boundary. It discovers missing or stale requirements, communicates with Providers or TMS systems, validates candidates, and updates a Translation Store.

A normal application build reads stored, validated artifacts. It never silently calls a remote service. Production runtime receives only published immutable outputs.

### Providers propose; Intlify validates and policy approves

A Provider returns localization candidates. It does not gain authority to publish production artifacts merely because it is an AI model, TMS, MT engine, or human adapter.

MF2 validation, parameter compatibility, project policy, provenance, review requirements, target capability, and approval gates determine whether a candidate becomes a `LocalizedMessageArtifact`.

### Language-neutral core, producer-specific authoring

Each host language and framework uses the authoring surface natural to it. Producers lower those surfaces into common artifacts defined by versioned specifications. The shared compiler does not need to understand OXC nodes, Vue template nodes, SwiftSyntax, Kotlin compiler trees, Rust macros, or C++ ASTs.

### Target-specific generation

The same checked localization graph can generate Browser ESM, SSR modules, Locale Capsules, iOS resources, Android resources, baked native data, generated bindings, manifests, and source maps.

Target-native export is allowed only when it can preserve the required MF2 semantics or report an explicit capability failure.

### Internal identity is generated, not eliminated

Stable identity is still required for translation history, provenance, caching, review, linking, and runtime lookup. Intlify generates and versions that identity instead of requiring application developers to invent and maintain message keys.

### One semantic foundation for people and agents

Editors and AI coding agents need more than source strings or TypeScript declarations. They should consume the same structured parser, semantic, artifact, finding, reference, and suggested-edit data as the compiler.

LSP is one editor adapter over those services, not the core semantic protocol.

### Small, deterministic runtime

Fallback resolution, reachability, coverage, approval, and target generation finish before runtime. Runtime loads admitted artifacts, resolves generated handles, and synchronously formats selected messages after readiness.

No process-global mutable locale is required.

## Terminology

| Term | Meaning |
| --- | --- |
| **Intlify** | A composable, compiler-based localization toolchain and runtime spanning authoring, synchronization, validation, linking, target generation, and localized execution. |
| **Locale Compiler** | The compiler-based toolchain that converts checked application localization requirements and approved localized artifacts into generated application bindings and target locale outputs. It is a pipeline, not one parser-sized component. |
| **Authoring Surface** | Host-language or framework syntax through which a developer expresses localizable UI or message semantics. |
| **Intent Frontend / Producer** | Host-specific analyzer that recognizes authoring surfaces and emits language-neutral message and reference artifacts. |
| **Message Intent** | A statically discoverable communication requirement: source MF2, parameters, selectors, meaning or usage evidence, constraints, identity, and revision. |
| **Message Reference** | Evidence that application or library code may use a message in a scope and delivery unit. |
| **Localization Provider** | Adapter that returns target-locale candidates from AI, MT, TMS, rules, or human-authored sources. |
| **Localization Sync** | Explicit workflow that resolves missing or stale localization requirements through Providers or TMS systems and publishes checked artifacts. |
| **Localized Message Artifact** | Immutable, validated target-locale MF2 bound to an exact Message Intent revision, provenance, policy revision, and approval state. |
| **Translation Store** | Logical storage and query boundary for localized artifacts. It may be local, remote, TMS-backed, or hybrid. |
| **Message Linker** | Language-neutral tool that resolves references, locale fallback, coverage, reachability, and delivery placement before export. |
| **Target Exporter** | Generator that turns checked link results into target-specific code, manifests, locale assets, and native resources. |
| **Message Handle** | Compiler-generated checked identity used by generated application code and runtime artifacts. |
| **Localization Runtime** | Target-facing service that binds locale and artifacts, resolves handles, and invokes MF2 evaluation. |
| **MF2 Runtime Core** | Language-neutral evaluator for one already selected, checked MF2 message. |

## Architecture

![Intlify compiler-based localization architecture](./assets/000-intlify-architecture.svg)

The diagram contains seven labeled areas arranged across six numbered stages. Stage 4 is split into two sibling workflows so that remote localization synchronization and the deterministic application build are not mistaken for one build-time operation.

1. **1 — Application authoring surfaces** — source-first UI, explicit Message Intent declarations, and standalone MF2 messages.
2. **2 — Host-specific Intent Frontends and Producers** — recognizes host-language and framework syntax, then emits portable compiler inputs.
3. **3 — Language-neutral compiler model and shared tooling** — provides portable message artifacts, MF2 language services, the checked project localization graph, and structured tooling queries.
4. **4A — Explicit localization synchronization** — obtains target-locale candidates through Provider or TMS adapters, validates them, and publishes approved artifacts to the Translation Store.
5. **4B — Deterministic application build** — reads the checked project graph and stored approved artifacts, then performs coverage checks, linking, source lowering, and export without remote side effects.
6. **5 — Generated target outputs** — contains application bindings, Web/server artifacts, mobile/native resources, and runtime metadata.
7. **6 — Deterministic target runtime** — admits generated outputs, binds an application- or request-scoped locale, and formats selected messages.

`4A` and `4B` are connected by the Translation Store, but they are not sequential steps of every build. Synchronization updates stored compiler inputs explicitly; the normal application build consumes a stable snapshot of those inputs.

The word “compiler” describes the complete static transformation from application localization semantics and approved locale data to deployable code and artifacts. Localization synchronization supplies compiler inputs, but a reproducible build transaction does not run remote Providers.

## Ownership by Architectural Area

### Authoring and producer stages

Host-specific producers own:

- known UI sink and template recognition;
- explicit authoring marker recognition;
- host-language syntax, import, macro, plugin, and source-map behavior;
- safe bounded data-flow analysis;
- parameter-expression and source-span evidence;
- reference and delivery-unit discovery; and
- projection into common artifacts defined by versioned specifications.

They do not own target translation, cross-locale coverage, fallback resolution, approval, or runtime formatting.

Candidate producer families include:

```text
JavaScript / TypeScript -> OXC producer
Vue SFC                 -> Vue producer
HTML                    -> HTML producer
Swift / SwiftUI         -> Swift producer
Kotlin / Compose        -> Kotlin producer
Java / Android Views    -> JVM / Android producer
Rust                    -> macro and binary-evidence producer
C / C++                 -> compiler or object-evidence producer
Go / .NET               -> language-specific producers
```

### Shared semantic and tooling stage

This stage owns the common meaning of a message after host syntax has been lowered.

It includes:

- `ox-mf2` parsing and parser-owned semantic validation;
- formatter and linter behavior;
- Message Intent, Message Reference, and localized-artifact admission;
- parameter and selector specification derivation;
- stable identity and revision rules;
- typed findings and source evidence;
- source maps and suggested edits;
- structured queries for CLI, editor, LSP, build integrations, and AI agents; and
- common conformance fixtures for producers and targets.

Host-specific tooling projects these facts back to host syntax. It must not reimplement MF2 semantics independently.

### Localization synchronization stage

Synchronization owns networked and potentially non-deterministic candidate acquisition.

Conceptually:

```text
intlify sync
  -> inventory Message Intents and target locales
  -> find missing or stale Intent revision × locale requirements
  -> call configured Provider or TMS adapter
  -> parse and validate returned MF2
  -> validate parameters, policy, provenance, layout, and target capabilities
  -> apply automatic or human approval policy
  -> publish immutable LocalizedMessageArtifacts
  -> update the Translation Store
```

`intlify dev --sync` may provide an explicit watch-mode convenience, but it retains the same validation and approval rules. Normal `intlify dev`, test, and build workflows do not unexpectedly use credentials or network services.

### Deterministic build, link, and export stage

The normal application build owns:

- source and dependency inventory;
- producer execution or artifact consumption;
- Translation Store reads;
- stale, missing, incompatible, or unapproved artifact checks;
- finite requested-locale and delivery-unit requirements;
- message reference and definition resolution;
- locale fallback materialization;
- reachability and placement;
- MF2 export validation and target capability admission;
- source lowering to generated Message Handles;
- target code and locale-asset generation;
- Runtime Manifest, loader, binding, and source-map generation; and
- registration of output artifacts with the host build system.

The build is deterministic for the same checked inputs, resolved configuration, tool versions, and artifact revisions. A missing or stale localization emits a diagnostic that points to the explicit synchronization workflow; it does not trigger hidden remote generation.

### Runtime stage

The runtime owns only deployed execution concerns:

- immutable artifact and ABI admission;
- exact locale binding;
- delivery-unit loading;
- generated Message Handle lookup;
- prepared-message caching;
- MF2 evaluation;
- plain text or safe structured-parts output; and
- typed runtime diagnostics and bounded resource use.

Browser applications bind locale to an application tree. Servers bind locale to a request or task. Mobile and native adapters bind locale to an application, scene, view tree, job, or explicit operation.

The detailed split between Runtime Engine, locale-bound Localizer, application adapter, and MF2 Runtime Core is defined by [015](./015-ox-mf2-runtime-design.md).

## Authoring Model

### Ordinary static UI

A supported producer may recognize simple UI text without requiring an Intlify API.

```js
const button = document.querySelector('#pay')

button.textContent = 'Pay now'
```

The source message remains readable in application code. The producer proves that the assignment targets a supported UI sink, creates or references a Message Intent, and lets the compiler lower the expression to a generated runtime call.

Conceptually:

```js
button.textContent = __intlify_message(messageHandles.payNow)
```

The generated call is target-specific and not a proposed public API.

Automatic recognition is intentionally bounded. A producer can support:

- static template or markup text;
- statically known localizable attributes;
- known framework text expressions;
- known DOM or native UI setters; and
- values whose origin it can safely and predictably follow.

If a known UI destination receives a value the producer cannot understand, the preferred behavior is a diagnostic with an explicit-authoring suggestion.

### Explicit programmable messages

Interpolation, selectors, advanced MF2, non-UI output, reuse, or an ambiguous host-language location requires explicit semantics.

Conceptual JavaScript examples are:

```js
intent('Hello {$name}!', { name })
```

```js
const inboxMessage = mf2`
  .input {$count :number}
  .match $count
  one {{You have one message}}
  * {{You have {$count} messages}}
`
```

`intent()` already establishes a localizable context, so requiring `intent(mf2\`...\`)`for the same message would be redundant. A standalone`mf2` tagged template remains useful for reusable, headless, multiline, or advanced messages.

The exact API and multiline-whitespace behavior remain open. Both forms must use the same MF2 parser, semantic checks, parameter model, source mapping, and tooling.

Other languages may use macros, compiler plugins, annotations, compiler-recognized declarations, or generated typed functions instead of JavaScript-shaped APIs.

### Static selection instead of dynamic source

This is supported conceptually:

```js
const current = pending ? messages.loading : messages.done
```

Both messages are statically declared; runtime chooses between checked handles.

This is not a source-first compiler input:

```js
intent(createMessageAtRuntime())
```

An arbitrary dynamic source prevents extraction, translation coverage, parameter validation, AOT generation, and editor reasoning. The producer reports a compile-time diagnostic rather than silently invoking runtime translation.

## End-to-End Workflows

### Source-first synchronization

```text
application and library source
  -> host-specific producers
  -> MessageIntentArtifact + MessageReferenceArtifact
  -> required Intent revision × locale inventory
  -> intlify sync
  -> Provider / TMS candidate acquisition
  -> MF2 + specification + policy + approval validation
  -> LocalizedMessageArtifact
  -> Translation Store
```

Only missing, changed, stale, explicitly refreshed, or policy-invalid requirements need synchronization.

### Normal application build

```text
source/reference artifacts + project configuration
  + validated Translation Store snapshot
  -> coverage and freshness checks
  -> reference, locale fallback, reachability, and delivery linking
  -> MF2 export preparation
  -> target source lowering and Target Exporters
  -> generated application bindings
  + Locale Capsule / Browser ESM / native resource
  + Runtime Manifest / loader / source maps
```

The build does not contact a Provider or TMS. It either produces a complete compatible artifact set or fails without publishing a valid-looking partial release.

### Production runtime

```text
generated bindings + immutable locale outputs
  -> application- or request-scoped Localizer
  -> generated Message Handle resolution
  -> already selected MF2 message
  -> MF2 Runtime Core
  -> string or structured parts
```

The runtime does not rediscover source, synchronize translations, repeat linker fallback, or invent missing target messages.

## Compiler Pipeline Interpretation

The Locale Compiler is the whole static pipeline, not a box named “Intent Compiler.”

The closest programming-language analogy is:

| Compiler phase | Intlify responsibility |
| --- | --- |
| Host-language frontend | Recognize UI sinks, templates, explicit markers, macros, and application references |
| Parsing and semantic analysis | Parse MF2; derive parameters and selectors; validate Message Intents, localized messages, policies, and source evidence |
| Program analysis | Build localization requirements; resolve identity, scope, locales, coverage, and stale revisions |
| Linking | Resolve references, localized fallback selections, reachability, delivery units, and final bundle plans |
| Optimization | Prune unreachable messages, split locales and delivery units, reuse approved artifacts, and prepare target representations |
| Code generation | Lower host expressions; emit handles, bindings, manifests, Locale Capsules, ESM, and native resources |
| Runtime execution | Load admitted generated outputs and evaluate selected messages |

Localization Provider execution is not lexical or semantic compilation. It is an explicit supply workflow that produces validated inputs consumed by deterministic compiler transactions.

## Artifact and Identity Model

| Artifact or model | Produced by | Consumed by | Purpose |
| --- | --- | --- | --- |
| `MessageIntentArtifact` | Source-first producer | Sync inventory, validation, planning | Portable specification of localizable communication |
| `MessageReferenceArtifact` | Application or library producer | Linker | Portable reachability and delivery evidence |
| Required Intent × Locale Plan | Inventory/planning stage | Synchronization | Finite set of localization work |
| `LocalizedMessageArtifact` | Validation and approval pipeline | Translation Store and build | Checked localized MF2 for an exact intent revision and locale |
| Translation Store snapshot | Store adapter | Deterministic build | Complete selected set of approved localized artifacts |
| `MessageBundlePlan` | Message linker | Export preparation | Resolved requested-locale and delivery-unit selections |
| Generated Message Handle | Target code generator | Application binding and runtime | Checked internal runtime identity |
| Locale Capsule / target resource | Target Exporter | Runtime or platform | Immutable deployable localization data |
| Runtime Manifest / loader map | Target Exporter | Runtime and build host | Compatibility, locale, delivery, and loading metadata |

These artifacts and their specifications must be:

- explicitly versioned;
- language-neutral where shared;
- deterministic in canonical encoding and ordering;
- bounded and fail-complete on admission;
- linked to exact source, intent, policy, provider, and approval revisions where applicable; and
- testable through producer, store, linker, exporter, and runtime conformance fixtures.

An Intent ID and Intent revision serve different purposes. Identity retains history and references; revision changes when localization-relevant semantics change. Exact derivation, move/rename behavior, collision handling, and source-history association require a dedicated specification.

## Localization Provider and TMS Integration

Intlify does not replace every TMS or localization service. It provides a checked interoperability layer around them.

### Provider-neutral candidate acquisition

Candidate sources may include:

- AI localization;
- managed machine translation;
- an existing TMS;
- human-authored translations;
- deterministic rules or terminology templates; and
- previously approved local or remote artifacts.

Projects may select different Providers by locale, message surface, risk, policy, or target. AI is optional and is never implied by the name “Locale Compiler.”

### Translation Store topologies

The logical Translation Store may be implemented as:

1. local generated artifacts checked into or cached beside the project;
2. a local materialized snapshot pulled from a TMS;
3. a remote artifact store with an integrity-pinned local build input;
4. a TMS as system of record plus an Intlify validation/provenance layer; or
5. a hybrid with sparse human overrides over generated or machine-provided candidates.

The build must consume a stable, complete snapshot. It must not rely on eventually consistent remote reads during code generation.

### Human review

Human input uses the same artifact path as any other candidate. Approval is bound to the exact Intent revision, locale, MF2 bytes, policy revision, and relevant target constraints.

Changing source meaning, parameters, selectors, policy, or target requirements may mark an existing translation stale and require validation or review again.

### Authority

The Provider owns candidate generation or retrieval. Intlify owns candidate admission and deterministic validation. Project policy owns release approval. The Target Exporter owns target capability checks. Runtime owns neither candidate generation nor approval.

## Developer, Editor, and AI-Agent Tooling

The toolchain exposes structured facts instead of forcing every client to scrape diagnostics or understand every host grammar.

Potential shared capabilities include:

- MF2 completion and syntax diagnostics;
- parameter, selector, function, and markup information;
- missing and unexpected argument diagnostics;
- definition, reference, rename, and source mapping;
- Intent identity and stale-localization explanation;
- coverage, fallback, reachability, and delivery findings;
- Provider sync previews and approval status;
- target capability explanations;
- deterministic suggested edits; and
- machine-readable project and artifact inspection.

The same core queries can be adapted to:

- CLI text and JSON output;
- an LSP server;
- editor-native extensions;
- build-plugin diagnostics;
- CI reports;
- TMS review interfaces; and
- AI coding-agent tools.

Public names should communicate semantics clearly to both developers and coding agents. That consideration supports an explicit name such as `intent`, but naming alone is not a substitute for structured interfaces, specifications, and documentation.

## Runtime Model Summary

The runtime is downstream of all localization authority.

```text
build/runtime boundary
  -> immutable target artifacts
  -> Runtime Engine
  -> locale-bound Localizer
  -> generated Message Handle lookup
  -> MF2 Runtime Core
  -> text or structured parts
```

Required invariants are:

- no production Provider, TMS, model, credential, prompt, or approval connection;
- no process-global mutable current locale;
- browser localization is application-scoped;
- server localization is request- or task-scoped;
- immutable artifacts and compatible prepared messages may be shared;
- loading may be asynchronous, but formatting after readiness is synchronous;
- linker-materialized fallback is authoritative;
- translated markup remains inert until an allowlisted adapter projects it; and
- missing or incompatible deployed data is an explicit artifact/integration failure.

See [015](./015-ox-mf2-runtime-design.md) for the detailed runtime architecture and milestones.

## Language and Target Strategy

| Family | Candidate authoring and producer surface | Candidate target output | Runtime ownership |
| --- | --- | --- | --- |
| Browser JS/TS | Known DOM/UI sinks, `intent()`, `mf2`, generated handles | ESM, Locale Capsule, manifest, source lowering | Application-scoped Localizer and Web adapter |
| Vue and Web frameworks | Template extraction, compiler plugin, explicit script authoring | Client and SSR modules, generated render bindings | Framework context over client or request Localizer |
| SSR and server | Producer-generated handles and explicit headless formatting | Server modules and immutable locale artifacts | Request- or task-scoped Localizer |
| iOS | Swift macro/compiler plugin, SwiftUI/UIKit sink analysis | Locale Capsule, `.xcstrings`, generated Swift bindings | Application, scene, or view-tree adapter |
| Android | Kotlin/Java plugin, Compose and Views/XML analysis | Locale Capsule, `strings.xml`, generated Kotlin/Java bindings | Application, activity, composition, or task adapter |
| Rust and native | Macros, build integration, object/final-binary evidence | Baked Rust, capsule, native data, C ABI bindings | Explicit Localizer or application adapter |
| C/C++, Go, .NET, JVM services | Language/compiler-specific producers | Generated bindings, native artifact, capsule | Conforming binding or target-native runtime |

Every target must provide the same observable Intlify behavior: the same message semantics, parameter validation, locale negotiation, diagnostics, and output model. The code and platform services used to provide that behavior do not need to be identical on every target.

For example, Node.js may call the Rust reference implementation through N-API, a browser may use WASM or a conforming JavaScript implementation, and a mobile or native target may use a C ABI or locale services such as Foundation, ICU4J, ICU4X, or ICU. Platform-native resource formats may also be generated when they can represent the required behavior without changing its meaning.

Each implementation must pass the applicable Intlify conformance tests and report its supported capabilities. If a target runtime or native resource format cannot preserve a required Intlify or MF2 feature, the compiler must select a compatible runtime representation or report the unsupported feature instead of silently changing the result.

## Conceptual Product Surfaces

Exact commands and package names are deferred, but Intlify needs coherent surfaces for:

| Surface | Responsibility |
| --- | --- |
| `intlify fmt` | Format source-authored MF2 and supported localization interchange content |
| `intlify lint` / `check` | Report syntax, semantics, policy, localization coverage, and project findings |
| `intlify sync` | Explicitly communicate with Provider/TMS adapters and update validated localized artifacts |
| `intlify dev --sync` | Opt-in incremental development synchronization |
| Build integration | Run producers, read stored artifacts, link, lower source, export, and register outputs |
| Inspect/explain API | Expose identity, requirements, fallback, reachability, provenance, and stale reasons |
| Editor/agent service | Return structured semantic queries, diagnostics, references, and edits |
| Runtime API | Create locale-bound Localizers and format generated handles |

No command in this table is reserved merely by appearing here. The fixed property is the separation between explicit synchronization, deterministic build, and production runtime.

## Security, Trust, and Reproducibility

- Treat source comments, external documents, TMS content, Provider output, and imported localization data as untrusted data.
- Never forward credentials, production requests, secrets, or unrelated user-generated content into Provider requests implicitly.
- AI prompts and responses are candidate-generation inputs, not executable compiler instructions.
- Parse and validate all returned MF2 with the shared implementation.
- Check declared parameters, selectors, functions, markup, policy, and target capabilities before publication.
- Bind provenance and approval to exact revisions and content digests.
- Canonicalize artifact field ordering, message ordering, locale ordering, and digest inputs.
- Pin Provider configuration and revision when reproducibility requires it; do not regenerate approved output implicitly.
- Make translated markup inert and allowlist its projection at the target adapter.
- Reject incompatible, oversized, incomplete, or integrity-invalid artifact sets fail-completely.
- Keep network, credentials, Provider SDKs, model clients, and TMS connections outside production runtime artifacts.

## Failure and Diagnostic Model

Intlify should make localization failures early and actionable.

| Situation | Required behavior |
| --- | --- |
| Known UI sink with unanalyzable value | Compile diagnostic with an explicit-authoring suggestion |
| Dynamic message source | Compile diagnostic; no hidden runtime translation |
| Invalid source or target MF2 | Parser/semantic finding with mapped source or artifact evidence |
| Missing or unexpected parameter | Compile/export diagnostic where provable; typed runtime failure only for unchecked external calls |
| Missing or stale localized artifact | Build diagnostic that identifies the requirement and synchronization action |
| Unapproved high-risk candidate | Block release according to project policy |
| Missing locale definition with configured fallback | Linker materializes the selected fallback and retains provenance |
| Unresolved required message | Blocking linker/build finding |
| Unsupported target feature | Export capability error before publishing partial outputs |
| Missing deployed handle or incompatible artifact | Runtime integration failure; no Provider call or invented fallback |

Diagnostics should explain not only what failed, but also which producer, Intent revision, locale requirement, Provider/store artifact, fallback decision, delivery unit, target capability, or runtime compatibility edge caused it.

## Current Foundation and Gaps

As of this overview:

| Area | Current foundation | Main gap to the New Concept |
| --- | --- | --- |
| MF2 syntax and semantics | `ox_mf2_parser`, semantic validation design and implementation foundations, N-API/WASM bindings | Complete shared semantic/query surfaces needed by every compiler and tooling client |
| Formatter | Initial Rust formatter and bindings | Integration with all source-first embedded authoring surfaces |
| Linter | Initial detailed design | Product implementation and project/source-first rule integration |
| CLI and transport | Shared CLI/tooling designs and implementation foundations | Final product command model for sync, build, inspect, editor, and agent clients |
| Existing resource implementation | `intlify_resource` host-format parsing, extraction, source mapping, and validated write-back | Decide which implementation capabilities are reusable and whether any catalog compatibility remains outside the source-first core |
| Message linker and exporter | Language-neutral artifact specifications, initial implementation, ESM export path | Accept formal source-first Intent/localized-artifact inputs without making catalog projection part of the new specification |
| JavaScript producer | OXC-based producer direction and implementation foundations | Hybrid UI-sink, `intent()`, and `mf2` source-first production specification |
| Source-first flow | Isolated end-to-end PoC in PR #183 | Production MF2, explicit sync/store separation, versioned artifacts, and target integration |
| Editor and AI-agent tooling | LSP/editor and agent integration designs | One shared structured semantic/query service and host-language projections |
| Localization synchronization | Provider concepts proven by PoC and design discussion | Provider/TMS/store APIs, validation gates, provenance, approval, and CLI workflow |
| Runtime | Overview architecture in 015 | Production MF2 Runtime Core, Localization Runtime, target adapters, and conformance |
| Mobile and native | Architecture direction and target strategy | Producers, exporters, native resources/bindings, and runtime adapters |

The New Concept is therefore an integration direction over foundations already present in the repository, not a claim that every architectural area is implemented.

## Existing Resource Implementation and Compatibility Follow-Up

The target architecture in this document is source-first. The current `intlify_resource` implementation, catalog assignment model, key-based `MessageDefinitionArtifact`, and `t()`-style authoring path are not primary authoring surfaces or source-first core artifacts.

They are existing implementation assets whose future disposition requires a separate design decision. This overview does not promise that catalog authoring remains a permanent compatibility mode, and it does not require removing the current implementation before the source-first path can be built.

### Candidate implementation reuse

Parts of `intlify_resource` may be reusable without carrying its resource-first authoring model into the New Concept:

- host-format detection and parser adapters;
- string decoding, escaping, and re-escaping;
- source ownership, raw-offset mapping, and diagnostic projection;
- bounded input admission and format-specific validation;
- fail-complete validated write-back transactions; and
- JSON, YAML, XLIFF, Vue SFC, or other interchange knowledge useful for Provider/TMS import, migration tooling, review workflows, and target integration.

Reuse should happen behind source-first interfaces and artifact specifications. A reused parser or mapping utility does not make a locale catalog an application authoring source.

### Compatibility decisions still required

A follow-up design must decide:

- whether existing catalogs are supported only as one-time import and migration inputs;
- whether a legacy adapter remains outside the source-first core for existing applications;
- whether any bidirectional TMS or interchange workflow needs validated resource write-back;
- how existing keys and translation history associate with generated Message Intent identity;
- whether the current `MessageDefinitionArtifact` is adapted internally, replaced, or retained only for the existing implementation track;
- which `intlify_resource` modules should be generalized rather than depended on as a catalog-oriented package; and
- how long any compatibility surface is maintained and tested.

Until that decision is made, the current resource and catalog path is implementation context, not part of the New Concept’s normative authoring architecture. Temporary reuse of current linker or exporter inputs is an internal migration technique and must not leak into the public source-first interfaces or artifact specifications.

## Roadmap

### I0: Shared interfaces and artifact specifications

- Ratify this product boundary and glossary.
- Define versioned `MessageIntentArtifact` and source-first `LocalizedMessageArtifact` specifications.
- Define Intent identity, revision, parameter, source-evidence, and requirement rules.
- Define the Translation Store query/snapshot boundary.
- Define Provider candidate, validation, provenance, approval, and stale-result specifications.
- Define how source-first requirements adapt or replace the current linker/export inputs without making catalog-oriented artifacts normative.
- Establish conformance fixtures shared by producers, stores, linkers, exporters, and runtimes.

### I1: JavaScript/Web vertical slice

- Replace the PoC placeholder parser with `ox-mf2`.
- Implement bounded JavaScript/TypeScript UI-sink recognition plus explicit `intent()` and standalone `mf2` authoring.
- Add explicit local or fixture-backed `intlify sync`.
- Materialize validated localized artifacts in a deterministic local Translation Store snapshot.
- Reuse the current linker and ESM exporter through a temporary internal adapter where necessary.
- Implement the R1 application-scoped/request-scoped runtime path from 015.
- Prove source maps, parameters, missing/stale diagnostics, offline build, and browser rendering end to end.

### I2: Vue, SSR, editor, and agent integration

- Add Vue template and script producers without putting Vue types in shared specifications.
- Add client and SSR target lowering with request-safe Localizers.
- Expose shared MF2 and Intent queries to editor and AI-agent adapters.
- Add incremental project inventory, sync preview, coverage, and stale-artifact diagnostics.
- Prove hydration consistency and concurrent requests with different locales.

### I3: TMS and production synchronization

- Add at least one real TMS/Localization Provider adapter.
- Define pull, push, conflict, refresh, retry, rate-limit, and approval workflows.
- Support local, TMS-backed, and hybrid Translation Store topologies.
- Add integrity-pinned CI synchronization and deterministic build fixtures.
- Prove sparse human review and override without returning to hand-maintained full catalogs.

### I4: Mobile targets

- Add Swift/SwiftUI and Kotlin/Compose producer experiments.
- Generate `.xcstrings`, Android resources, or portable Locale Capsules through capability-checked exporters.
- Add application/scene/request-equivalent runtime adapters.
- Reuse shared artifact and MF2 conformance fixtures across Web, iOS, and Android.

### I5: Native and system-language composition

- Add Rust and at least one additional system-language producer.
- Define library artifact composition and final-application linking.
- Add C ABI or conforming native runtime bindings.
- Generate native or baked target artifacts without changing shared message semantics.
- Prove bounded dynamic references, final-binary reachability evidence, and offline deployment.

## Expected Outcomes

The architecture succeeds when:

- a developer can add ordinary static UI text without creating a message key or editing locale catalogs;
- advanced or headless messages have an explicit, predictable, MF2-based authoring path;
- target translations can come from AI, MT, TMS, rules, or humans without changing compiler and runtime specifications;
- all release messages are stored, validated, traceable, and approved according to policy;
- normal builds and production formatting work without Provider/TMS network access;
- missing, stale, invalid, unreachable, and unsupported localization states are diagnosed before deployment;
- only reachable messages, locales, and delivery units are emitted;
- browser and SSR rendering never depend on process-global mutable locale;
- editor and AI-agent tools can query the same semantics and findings as the compiler;
- reuse or compatibility work remains isolated from the source-first specifications; and
- Web, mobile, and native targets operate on artifacts defined by the same language-neutral localization specifications.

## Deferred Follow-Up Notes

The following need dedicated designs and do not block this overview:

- Message Intent identity, revision, movement, duplication, and merge semantics;
- exact source-first artifact wire schemas and limits;
- host-specific automatic UI-sink and bounded data-flow policies;
- JavaScript `intent()` and `mf2` APIs;
- multiline embedded-MF2 whitespace and source mapping;
- Translation Store protocol, snapshots, history, conflict, and garbage collection;
- Provider registry, batching, cancellation, retry, rate limiting, refresh, and credentials;
- human review, approval, risk, glossary, policy, and layout-test specifications;
- source-first requirement planning and its exact relationship to `intlify_linker`;
- generated source-lowering ABI and bundler integration;
- Locale Capsule, Runtime Manifest, and Message Handle formats;
- runtime public APIs, parts model, functions, locale services, and bindings;
- LSP/editor/agent shared query protocol;
- existing key/catalog import, migration, and translation-memory association;
- native target capability matrices and semantic-loss policy;
- library publication, open-world requirements, and final-application composition; and
- product packaging, command names, configuration layout, and release sequencing.

No dormant field, package, command, API, wire tag, or format name is reserved merely by appearing as a candidate in this overview.

## Relationship to Other Documents

| Document | Relationship |
| --- | --- |
| [001-ox-mf2-toolchain-foundation.md](./001-ox-mf2-toolchain-foundation.md) | Defines `ox-mf2` as the shared parser and semantic foundation. This overview places that foundation inside the broader Intlify product. |
| [005-ox-mf2-phase-3-tooling-transport-design.md](./005-ox-mf2-phase-3-tooling-transport-design.md) | Defines shared tooling transport direction used by CLI, editor, and long-lived clients. |
| [006-ox-mf2-phase-3a-tooling-foundation-design.md](./006-ox-mf2-phase-3a-tooling-foundation-design.md) | Defines CLI and project-tooling foundations on which future sync, inspect, and build surfaces can compose. |
| [007-ox-mf2-phase-3b-formatter-design.md](./007-ox-mf2-phase-3b-formatter-design.md) | Owns formatter product behavior and specifications. |
| [008-ox-mf2-phase-3c-linter-design.md](./008-ox-mf2-phase-3c-linter-design.md) | Owns linter rules, results, reporting, and configurable lint behavior. |
| [009-ox-mf2-phase-3d-lsp-editor-design.md](./009-ox-mf2-phase-3d-lsp-editor-design.md) | Owns editor lifecycle and LSP projection. This overview requires the underlying semantics to remain reusable by non-LSP clients. |
| [010-ox-mf2-phase-3e-agent-integration-design.md](./010-ox-mf2-phase-3e-agent-integration-design.md) | Defines the current agent-as-tooling-client direction. Future source-first queries extend it through an explicit design. |
| [012-ox-mf2-parser-semantic-validation-design.md](./012-ox-mf2-parser-semantic-validation-design.md) | Owns parser-backed MF2 semantic validation reused across authoring, sync, export, and runtime preparation. |
| [013-ox-mf2-resource-catalog-adapter-design.md](./013-ox-mf2-resource-catalog-adapter-design.md) | Documents the current resource/catalog implementation. Its format parsing, source mapping, validation, and write-back capabilities are reuse candidates; it does not define the target source-first authoring model. |
| [014-ox-mf2-message-linker-design.md](./014-ox-mf2-message-linker-design.md) | Owns current reference/definition resolution, locale fallback, reachability, delivery planning, export preparation, and current ESM output. Source-first integration must explicitly adapt or evolve its catalog-oriented definition input. |
| [015-ox-mf2-runtime-design.md](./015-ox-mf2-runtime-design.md) | Owns the detailed runtime-side architecture below the build/runtime boundary. |
| [PR #183](https://github.com/intlify/intlify/pull/183) | Provides the isolated source-first PoC and review feedback used to clarify this overview; it does not freeze production APIs. |
