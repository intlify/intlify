# Intlify Compiler-Based Localization Overview Design

## Status

This document defines the high-level product architecture for Intlify as a compiler-based localization toolchain with an offline, artifact-driven runtime.

It is one abstraction level above the component designs in this repository. In particular, [015](./015-ox-mf2-runtime-design.md) defines the runtime-side responsibility split, while this document explains how authoring, MF2 language services, localization synchronization, linking, target generation, editor and agent tooling, and runtime delivery form one Intlify system.

The source-first PoC in [PR #183](https://github.com/intlify/intlify/pull/183) and its review discussion are design evidence, not a frozen public API. This document fixes the overall direction and responsibility boundaries. It does not yet freeze:

- the exact JavaScript `intent()` signature or tagged-template API;
- automatic extraction rules for each host language and framework;
- the wire schemas of the source-first artifacts;
- the user-facing configuration syntax or resolved-project-profile wire schema;
- the persistent Intent identity registry encoding and reconciliation protocol;
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

“Without hand-maintained catalogs” describes the application authoring model. It does not mean that requested-locale messages are never stored. Intlify replaces catalogs as the developer-maintained source of application messages with compiler-managed, validated localization artifacts.

The initial product scope is application- and library-owned, user-facing messages: static UI, accessibility text, explicit headless messages, MF2 interpolation and selection, and locale-aware formatting inside those messages. It is not a general engine for locale-dependent routing, input parsing, collation, regional business rules, remote content, or localized non-message media.

“Validated localization” means that an artifact has passed the applicable deterministic checks, project policies, and required approval gates. It does not claim that a compiler can prove linguistic, cultural, legal, or product correctness.

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

In an AI-agent and automation-friendly workflow, those concerns no longer need to be coupled. Application developers should be able to author the source-locale UI naturally. Localization Providers, TMS integrations, machine translation, AI, rules, and humans can all supply requested-locale candidates through one checked workflow. A compiler can then resolve the finite localization requirements of the application, validate them, and generate target-specific code and immutable locale artifacts.

The intended model is:

```text
application-owned source messages
  -> statically discovered Message Intents and references
  -> explicit localization synchronization
  -> validated localized artifacts
  -> reachability, requested-locale, message-locale-fallback, and delivery linking
  -> generated application bindings and requested-locale artifacts
  -> deterministic locale-scoped runtime formatting
```

The resulting system keeps translation generation and remote services out of production rendering while removing hand-maintained keys and catalogs from the normal source-first application workflow. Target execution remains driven by admitted artifacts even when the physical engine is implemented through a platform-native service.

## Problem Statement

### Authoring indirection

Message keys make application code describe storage locations rather than user-facing communication.

```ts
t('checkout.actions.pay')
```

The source text, parameter specification, UI usage, and requested-locale translations live elsewhere. Reviewers cannot understand the rendered UI from the application change alone, and a rename or refactor may require coordinated edits across code, catalogs, types, tests, and TMS state.

### Catalog maintenance

Hand-maintained catalogs accumulate stale entries, missing translations, duplicate meanings, inconsistent placeholders, and orphaned keys. Extraction and synchronization become recurring application-development tasks rather than compiler work.

### Late failure

Missing messages, invalid message locale fallback, incompatible arguments, unsupported functions, and malformed translated syntax are often discovered only when a particular locale and route execute in production.

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

The toolchain resolves user-facing configuration into one language-neutral project profile, turns reachable requirements and approved localized messages into target-specific application code, and publishes immutable Store and Release snapshots. Host-language values are lowered through a shared parameter and value specification before a conforming localization execution layer evaluates the selected message.

This changes where responsibilities live:

| Concern | Traditional application responsibility | Compiler-based Intlify responsibility |
| --- | --- | --- |
| Message authoring | Choose a key and update a source catalog | Write source-locale UI or explicitly declare message semantics |
| Message identity | Developer-maintained string key | Generated, versioned identity and checked handle |
| Translation supply | Manually edit locale files | Provider, TMS, MT, AI, rule, or human adapter |
| Validation | Partial build checks or runtime errors | Shared MF2, parameter, policy, coverage, and target checks |
| Selection | Runtime key and locale-fallback search | Link-time reachability, message locale fallback, and delivery planning |
| Delivery | Catalog-oriented bundles | Target- and delivery-unit-specific artifacts |
| Target execution | Lookup, message locale fallback, parse, and format | Admit generated artifacts and evaluate an already selected message through a conforming engine |

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
- Validate syntax, parameters, policy, provenance, approval, coverage, message locale fallback, reachability, and target capability before publication.
- Publish Translation Store and deployable release views atomically so builds and target execution never observe mixed revisions.
- Make invalidation content-addressed, incremental, and explainable instead of rerunning all localization work after every edit.
- Generate only the messages and locales required by the final application and its delivery units.
- Define one localized message per Message Intent revision and requested locale; target-specific wording is represented as a distinct Intent rather than an implicit target variant.
- Compose application and library Message Intents before final requirement planning and linking.
- Keep migration and compatibility decisions separate so that existing resource-oriented specifications do not constrain the source-first core.
- Provide structured compiler and semantic queries that can be reused by the CLI, editors, LSP adapters, build tools, and AI coding agents.
- Keep browser locale state application-scoped and server locale state request-scoped.
- Support Web, SSR, workers, iOS, Android, native applications, system languages, libraries, and CLIs without requiring one physical execution engine everywhere.

## Non-Goals

- Translating arbitrary runtime strings, user-generated content, logs, protocol values, or remote content automatically.
- Calling a Localization Provider, TMS, AI model, or machine-translation service from production formatting.
- Claiming that localized resources, translation storage, provenance, or human review disappear.
- Inferring localization semantics from every human-readable string in a general-purpose programming language.
- Making an AI model the source of truth for parsing, identity, validation, linking, approval, code generation, or runtime behavior.
- Deciding whether current key- and catalog-authoring workflows remain permanent public compatibility surfaces.
- Making JavaScript, Vue, one bundler, one UI framework, one Provider, or one TMS part of the language-neutral core.
- Moving host-language parsing, framework lifecycle, locale negotiation, application routing, or UI rendering into the MF2 Runtime Core.
- Providing general locale-aware routing, input parsing, collation, regional business rules, remote-content translation, or non-message media localization in the initial message-localization core.
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

`intlify sync` is the conceptual network and credential boundary. It consumes a finite `LocalizationRequirementPlan`, communicates with Providers or TMS systems for missing or stale requirements, validates candidates, and atomically publishes a new Translation Store snapshot.

A normal application build pins one stored, validated snapshot. It never silently calls a remote service. Production localization execution receives only outputs named by one immutable `ReleaseSnapshot`.

### Providers propose; Intlify validates and policy approves

A Provider returns localization candidates. It does not gain authority to publish production artifacts merely because it is an AI model, TMS, MT engine, or human adapter.

MF2 and parameter validation determine whether a candidate can become a stored `LocalizedMessageArtifact`. Policy validation and approval are separate evidence bound to the exact localized-message digest. Only an artifact with all evidence required by the pinned project policy is selectable by a build. Synchronization may preflight configured targets, while the Target Exporter owns final target-capability admission.

Validation produces evidence; policy defines which evidence is required; an authorized approval makes the artifact selectable. Linguistic, cultural, legal, and product judgment remains an explicit human or organizational responsibility when policy requires it.

### Language-neutral core, producer-specific authoring

Each host language and framework uses the authoring surface natural to it. Producers lower those surfaces into common artifacts defined by versioned specifications. The shared compiler does not need to understand OXC nodes, Vue template nodes, SwiftSyntax, Kotlin compiler trees, Rust macros, or C++ ASTs.

### Target-specific generation

The same checked localization graph can generate Browser ESM, SSR modules, Locale Capsules, iOS resources, Android resources, baked native data, generated bindings, manifests, and source maps.

Target-native export is allowed only when it can preserve the required MF2 semantics or report an explicit capability failure. Every target implements the same logical Localization Execution Layer, but it may use the Intlify MF2 Runtime Core, a conforming language implementation, native bindings, or a capability-checked platform resource engine as its physical implementation.

### Internal identity is generated, not eliminated

Stable identity is still required for translation history, provenance, caching, review, linking, and runtime lookup. Intlify generates and versions that identity instead of requiring application developers to invent and maintain message keys.

A production `MessageIntentId` is opaque and independent of source text, file path, and occurrence order. Compiler-managed identity metadata, such as an `intent.lock`, preserves that association across edits and moves without becoming a translation catalog. An `IntentRevision` changes only when localization-relevant semantics change. Generated target code may lower the persistent identity to a compact, release-local Message Handle.

### Immutable snapshots make publication explicit

Candidate acquisition, validation, approval, build generation, and deployment do not mutate one live catalog in place. Intlify stages immutable content and makes it visible by publishing a complete Translation Store or Release snapshot. A failed synchronization or export leaves the previously visible snapshot unchanged.

Store publication, build selection, and release admission are distinct states. A Store snapshot may contain validated but unapproved artifacts; a build selects only artifacts whose validation and approval evidence satisfy the pinned policy; target admission and Release Assembly then bind selected content to compatible deployable outputs.

### Versioned specifications admit compatibility explicitly

Every shared artifact declares its specification version, required capabilities, and integrity identity. Consumers admit only declared compatible versions and never infer unknown required semantics or silently downgrade them. Deterministic migration belongs to the toolchain, creates new immutable artifacts or snapshots, and preserves provenance; production execution admits only the versions fixed by its Release snapshot.

### One semantic foundation for people and agents

Editors and AI coding agents need more than source strings or TypeScript declarations. They should consume the same structured parser, semantic, artifact, finding, reference, and suggested-edit data as the compiler.

LSP is one editor adapter over those services, not the core semantic protocol.

### Small, artifact-driven runtime

Message locale fallback, reachability, coverage, approval, and target generation finish before target execution. The logical Localization Execution Layer loads admitted artifacts, resolves generated handles, and synchronously formats selected messages after readiness.

No Provider, TMS, hidden message fallback, or process-global mutable locale is required. Exact locale-service output is reproducible under a pinned Locale Service Profile; a platform-managed profile permits only its explicitly declared locale-dependent variation.

## Terminology

| Term | Meaning |
| --- | --- |
| **Intlify** | A composable, compiler-based localization toolchain and runtime spanning authoring, synchronization, validation, linking, target generation, and localized execution. |
| **Locale Compiler** | The compiler-based toolchain that converts checked application localization requirements and approved localized artifacts into generated application bindings and requested-locale outputs. It is a pipeline, not one parser-sized component. |
| **Authoring Surface** | Host-language or framework syntax through which a developer expresses localizable UI or message semantics. |
| **Intent Frontend / Producer** | Host-specific analyzer that recognizes authoring surfaces and emits language-neutral message and reference artifacts. |
| **Message Intent** | A statically discoverable communication requirement: source MF2, parameters, selectors, meaning or usage evidence, constraints, identity, and revision. |
| **Message Reference** | Evidence that application or library code may use a message in a scope and delivery unit. |
| **Localization Project Profile** | Language-neutral, resolved project configuration consumed by shared compiler stages. It references locale negotiation, message locale fallback, coverage, Provider-routing, approval, glossary, target, delivery, and resource-limit policies by explicit revision. |
| **Localization Requirement Plan** | Deterministic finite set of reachable Intent revision × requested-locale work required by synchronization. |
| **Localization Provider** | Adapter that returns requested-locale candidates from AI, MT, TMS, rules, or human-authored sources. |
| **Localization Sync** | Explicit workflow that resolves missing or stale localization requirements through Providers or TMS systems and publishes checked artifacts. |
| **Source-Locale Message Artifact** | Compiler-derived source-locale message for an exact Intent revision. It is regenerated from application or library source rather than synchronized through a Provider. |
| **Localized Message Artifact** | Immutable localized MF2 bound to one exact Message Intent revision and definition locale, plus its parameter specification, provenance, content digest, and required capabilities. One Intent revision and requested locale selects at most one such message. |
| **Approval Record** | Immutable decision that approves or rejects an exact localized-message digest under explicit policy and, where applicable, target revisions. |
| **Translation Store** | Logical storage and query system for localized artifacts and decision evidence. It may be local, remote, TMS-backed, or hybrid. |
| **Translation Store Snapshot** | Atomically published immutable view of validated localized messages and their applicable evidence. It may contain artifacts that are stored but not yet selectable under the current policy. |
| **Stored / Selectable / Release-admitted** | Successive eligibility states: visible as a validated Store artifact; eligible under pinned policy and approval evidence; then bound to a capability-admitted target output in one Release snapshot. |
| **Message Linker** | Language-neutral tool that resolves references, message locale fallback, coverage, reachability, and delivery placement before export. |
| **Target Profile** | Versioned deployment-target requirements including semantic specification, Runtime ABI, locale-service profile, supported capabilities, and output model. |
| **Locale Negotiation Profile** | Versioned rules for choosing one supported requested locale from application-supplied preferences. It is separate from message locale fallback. |
| **Locale Service Profile** | Versioned identity and reproducibility class of the locale-data and formatting services used by one target runtime. |
| **Target Exporter** | Generator that turns checked link results into target-specific code, manifests, locale assets, and native resources. |
| **Message Handle** | Compiler-generated checked identity used by generated application code and runtime artifacts. |
| **Source Locale** | Locale in which one Message Intent's source message is authored. A project default applies only when the Intent does not declare one; libraries retain their own source locales. |
| **Requested Locale** | Supported locale selected for a user or operation and used as the requirement, coverage, and emitted-artifact unit. |
| **Default Requested Locale** | Locale selected when negotiation cannot match application-supplied preferences. It is independent of the default source locale. |
| **Fallback Locale** | Locale considered by the Message Linker when the requested locale has no eligible definition. Runtime does not search this chain. |
| **Definition Locale** | Locale of the message definition selected by the Linker. It may differ from the requested locale and supplies the language context for MF2 evaluation. |
| **Release Snapshot** | Immutable localization release manifest binding one compatibility group of generated bindings and one or more Target Profile output sets to their exact project, Store, source-message, bundle-plan, manifest, specification, and Runtime ABI identities. |
| **Localization Execution Layer** | Logical target-side responsibility that admits one release, binds locale and artifacts, resolves handles, and evaluates selected messages through a conforming physical engine. |
| **Localization Runtime** | One physical target-facing implementation of the Localization Execution Layer. |
| **MF2 Runtime Core** | Language-neutral physical evaluator for one already selected, checked MF2 message; a conforming target-native engine may fulfill the same semantic role. |

## Architecture

![Intlify compiler-based localization architecture](./assets/000-intlify-architecture.svg)

The diagram contains seven labeled areas arranged across six numbered stages. Stage 4 is split into two sibling workflows so that remote localization synchronization and the deterministic application build are not mistaken for one build-time operation.

1. **1 — Application authoring surfaces** — source-first UI, explicit Message Intent declarations, and standalone MF2 messages.
2. **2 — Host-specific Intent Frontends and Producers** — recognizes host-language and framework syntax, then emits portable compiler inputs.
3. **3 — Language-neutral compiler model and shared tooling** — resolves the project profile, provides portable message artifacts and MF2 language services, plans finite localization requirements, and exposes structured tooling queries.
4. **4A — Explicit localization synchronization** — consumes a requirement plan, obtains requested-locale candidates through Provider or TMS adapters, validates them, records approval separately, and atomically publishes a Translation Store snapshot.
5. **4B — Deterministic application build** — recomputes the requirement plan, pins a Store snapshot, performs final linking, source lowering, and authoritative target-capability admission, then passes complete target output sets to Release Assembly without remote side effects.
6. **5 — Generated target outputs and release assembly** — contains application bindings, Web/server artifacts, mobile/native resources, Runtime metadata, and one Release snapshot binding the output sets in a deployment compatibility group.
7. **6 — Conforming localization execution** — admits one compatible release, binds a locale selected by the application or optional Locale Negotiator, and formats selected messages through an Intlify runtime or a capability-checked conforming target-native engine.

`4A` and `4B` are connected by a Translation Store snapshot, but they are not sequential steps of every build. Synchronization updates stored compiler inputs explicitly; the normal application build pins one exact snapshot and never follows a changing `latest` view while code generation is running.

The word “compiler” describes the complete static transformation from application localization semantics and approved locale data to deployable code and artifacts. Localization synchronization supplies compiler inputs, but a reproducible build transaction does not run remote Providers. The Localization Execution Layer is a logical responsibility; it does not require one identical physical engine on every target.

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

They do not own requested-locale localization, cross-locale coverage, message locale fallback resolution, approval, or target formatting.

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
- project-profile resolution plus parameter, selector, and portable-value specification derivation;
- stable identity and revision rules;
- dependency-digest tracking and typed invalidation reasons;
- typed findings and source evidence;
- source maps and suggested edits;
- structured queries for CLI, editor, LSP, build integrations, and AI agents; and
- common conformance fixtures for producers and targets.

Host-specific tooling projects these facts back to host syntax. It must not reimplement MF2 semantics independently.

### Localization synchronization stage

Synchronization owns networked and potentially non-deterministic candidate acquisition.

Requirement planning happens before remote synchronization. The Message Linker core therefore has two deterministic operations: conceptual `plan_requirements` before synchronization and `link_outputs` after a Store snapshot exists. Conceptually:

```text
source and reference artifacts + Localization Project Profile
  -> plan localization requirements
  -> LocalizationRequirementPlan
  -> intlify sync
  -> find missing or stale reachable Intent revision × locale requirements
  -> call configured Provider or TMS adapter
  -> parse and validate returned MF2
  -> validate parameters, provenance, and machine-checkable policy
  -> derive required capabilities and optionally preflight Target Profiles
  -> stage immutable LocalizedMessageArtifacts
  -> attach automatic or human approval evidence when available
  -> atomically publish a TranslationStoreSnapshot
```

`intlify dev --sync` may provide an explicit watch-mode convenience, but it retains the same validation and approval rules. Normal `intlify dev`, test, and build workflows do not unexpectedly use credentials or network services.

Synchronization processes only requirements in the plan. It does not decide final message locale fallback selection, silently broaden the reachable application graph, or make one target-specific wording variant for the same Intent revision and requested locale.

The plan identifies the reachable Intent revisions, requested locales, delivery units, and applicable policy inputs needed for explicit synchronization. A normal build recomputes and validates it against current source and profile inputs. A stale plan never triggers implicit synchronization.

Validated artifacts may be published before human approval so that review can be asynchronous. Store publication makes an artifact visible; applicable validation and approval evidence makes it selectable. Automatic approval may publish both in one snapshot, while later human approval produces a new immutable snapshot.

### Deterministic build, link, and export stage

The normal application build owns:

- source and dependency inventory;
- producer execution or artifact consumption;
- resolved `LocalizationProjectProfile` and pinned Translation Store snapshot reads;
- recomputation and freshness validation of the `LocalizationRequirementPlan`;
- stale, missing, incompatible, or unapproved artifact checks;
- finite requested-locale and delivery-unit requirements;
- message reference and definition resolution;
- message locale fallback materialization;
- reachability and placement;
- MF2 export validation and authoritative Target Profile capability admission;
- source lowering to generated Message Handles;
- target code and locale-asset generation;
- Runtime Manifest, loader, binding, and source-map generation;
- handoff of one or more complete Target Profile output sets to Release Assembly;
- `ReleaseSnapshot` generation over the complete deployment compatibility group; and
- registration of output artifacts with the host build system.

The build is deterministic for the same checked inputs, resolved configuration, tool versions, and artifact revisions. A missing or stale localization emits a diagnostic that points to the explicit synchronization workflow; it does not trigger hidden remote generation.

### Localization execution stage

The logical Localization Execution Layer owns only deployed execution concerns:

- immutable artifact and ABI admission;
- exact requested-locale binding after application-owned or optional Intlify locale negotiation;
- delivery-unit loading;
- generated Message Handle lookup;
- prepared-message caching;
- MF2 evaluation;
- plain text or safe structured-parts output; and
- typed runtime diagnostics and bounded resource use.

Browser applications bind locale to an application tree. Servers bind locale to a request or task. Mobile and native adapters bind locale to an application, scene, view tree, job, or explicit operation.

The physical path may use the Intlify Localization Runtime and MF2 Runtime Core or a conforming target-native implementation admitted through the same target capability and release checks. The detailed split between Runtime Engine, locale-bound Localizer, application adapter, and MF2 Runtime Core is defined by [015](./015-ox-mf2-runtime-design.md).

## Localization Project Profile

User-facing configuration may be JavaScript, TOML, YAML, framework configuration, workspace metadata, or another host-specific format. Before shared compilation begins, host tooling resolves it into one language-neutral `LocalizationProjectProfile`. Shared compiler stages consume only that resolved profile, not host configuration objects.

The profile identifies the project, requested locales, default source and requested locales, locale-negotiation and message-locale-fallback policies, coverage, Provider routing, approval and glossary policy, Target Profiles, delivery topology, and resource limits. It references mutable external policy data by immutable revision and never carries Provider credentials into a normal build or production execution path.

Each Message Intent has one source locale. The project default source locale applies only when authoring omits it, and a library retains the source locale of each published Intent. Requested locale is the requirement and emitted-artifact unit; the Linker selects a definition locale through message locale fallback. The default requested locale belongs to locale negotiation and is independent of the default source locale.

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

### UI context and disambiguation

Automatic authoring does not make identical source text one shared message. Two occurrences of `Open` are distinct Message Intents by default, even if translation memory or a Provider can reuse one as a suggestion. Identity is never merged solely from matching text or inferred context.

Producer context has three layers:

1. **Source evidence** — file, span, syntax node, component, and producer information used for diagnostics and navigation.
2. **Derived UI usage** — a bounded classification such as button label, heading, accessibility label, placeholder, notification, or headless output, plus the applicable element role, attribute, component, and nearby static text.
3. **Explicit semantic context** — source-authored information such as developer note, audience, tone, subject, character limit, terminology domain, or accessibility purpose.

Source evidence such as a file path or span does not by itself change the Intent revision. Derived usage that changes the communication purpose, explicit semantic context, and localization constraints do change the revision. A coding agent may propose structured context, but that proposal becomes authoritative only after it is represented explicitly in source and recompiled.

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
  + resolved LocalizationProjectProfile
  -> host-specific producers
  -> MessageIntentArtifact + MessageReferenceArtifact
  -> compiler-derived SourceLocaleMessageArtifact
  -> LocalizationRequirementPlan
  -> intlify sync
  -> Provider / TMS candidate acquisition
  -> MF2 + parameter + integrity validation
  -> stored LocalizedMessageArtifact
  -> separate policy validation and approval evidence
  -> atomic TranslationStoreSnapshot publication
```

Only missing, changed, stale, explicitly refreshed, or policy-invalid requirements need synchronization. A validated artifact may be stored before approval; it becomes selectable only when the pinned project policy finds all required evidence in the snapshot.

### Normal application build

```text
source/reference/source-locale artifacts
  + resolved LocalizationProjectProfile
  + pinned TranslationStoreSnapshot
  -> recompute and verify LocalizationRequirementPlan
  -> coverage, freshness, validation, and approval checks
  -> final reference, message locale fallback, reachability, and delivery linking
  -> MF2 export preparation
  -> target source lowering and authoritative Target Profile admission
  -> generated application bindings and target output sets
  -> Release Assembly over one deployment compatibility group
  -> ReleaseSnapshot
```

The build does not contact a Provider or TMS. It either produces a complete compatible artifact set or fails without publishing a valid-looking partial release.

### Production localization execution

```text
generated bindings + one admitted ReleaseSnapshot
  -> application preference resolution
  -> optional Locale Negotiator or directly selected supported locale
  -> application-, request-, or operation-scoped localization context
  -> generated Message Handle resolution
  -> already selected message
  -> conforming Localization Execution Layer
  -> string or structured parts
```

Target execution does not rediscover source, synchronize translations, repeat message locale fallback, or invent missing requested-locale messages. An Intlify MF2 Runtime Core is one conforming physical path, not a mandatory binary component on every target.

## Compiler Pipeline Interpretation

The Locale Compiler is the whole static pipeline, not a box named “Intent Compiler.”

The closest programming-language analogy is:

| Compiler phase | Intlify responsibility |
| --- | --- |
| Host-language frontend | Recognize UI sinks, templates, explicit markers, macros, and application references |
| Parsing and semantic analysis | Parse MF2; derive parameters and selectors; validate Message Intents, localized messages, policies, and source evidence |
| Program analysis | Build localization requirements; resolve identity, scope, locales, coverage, and stale revisions |
| Linking | Resolve references, message locale fallback selections, reachability, delivery units, and final bundle plans |
| Optimization | Prune unreachable messages, split locales and delivery units, reuse approved artifacts, and prepare target representations |
| Code generation | Lower host expressions; emit handles, bindings, manifests, Locale Capsules, ESM, and native resources |
| Target execution | Load admitted generated outputs and evaluate selected messages through a conforming physical engine |

Localization Provider execution is not lexical or semantic compilation. It is an explicit supply workflow that produces validated inputs consumed by deterministic compiler transactions.

## Artifact and Identity Model

| Artifact or model | Produced by | Consumed by | Purpose |
| --- | --- | --- | --- |
| `LocalizationProjectProfile` | Host configuration resolver | Shared compiler stages | Canonical project, locale, policy, target, and delivery inputs |
| `MessageIntentArtifact` | Source-first producer | Sync inventory, validation, planning | Portable specification of localizable communication |
| `SourceLocaleMessageArtifact` | Compiler from one Message Intent | Linker, export, Release snapshot | Deterministic source-locale definition without Provider synchronization |
| `MessageReferenceArtifact` | Application or library producer | Linker | Portable reachability and delivery evidence |
| `LocalizationRequirementPlan` | Requirement-planning operation | Synchronization and build verification | Finite reachable set of Intent revision × requested-locale work, with one localized message requirement per pair |
| `LocalizedMessageArtifact` | Candidate-validation pipeline | Translation Store and build | Checked localized MF2 payload for an exact Intent revision and locale |
| Validation and approval evidence | Validator, policy engine, or reviewer | Store publication and build | Separate decisions that can make a stored artifact selectable under pinned policy |
| `TranslationStoreSnapshot` | Store publication transaction | Deterministic build | Atomic immutable view of validated artifacts and applicable evidence, including artifacts not yet selectable |
| `MessageBundlePlan` | Message linker | Export preparation | Resolved requested-locale and delivery-unit selections |
| `TargetProfile` | Project-profile resolution | Sync preflight and Target Exporter | Target semantics, Runtime ABI, locale services, capabilities, and output model |
| Generated Message Handle | Target code generator | Application binding and runtime | Checked internal runtime identity |
| Locale Capsule / target resource | Target Exporter | Localization Execution Layer | Immutable deployable localization data for one conforming physical path |
| Runtime Manifest / loader map | Target Exporter | Runtime and build host | Compatibility, locale, delivery, and loading metadata |
| `ReleaseSnapshot` | Release Assembly | Deployment and target admission | Atomic identity of one compatibility group containing one or more Target Profile output sets |

These artifacts and their specifications must be:

- explicitly versioned;
- language-neutral where shared;
- deterministic in canonical encoding and ordering;
- bounded and fail-complete on admission;
- admitted only through declared specification and capability compatibility, without silent downgrade;
- linked to exact source, intent, policy, provider, and approval revisions where applicable; and
- testable through producer, store, linker, exporter, and runtime conformance fixtures.

### Source-locale lifecycle

`MessageIntentArtifact` is the source of truth for the source-locale message. The compiler deterministically derives a `SourceLocaleMessageArtifact` for the Intent revision and source locale.

The source-locale artifact is not translated through a Provider or manually maintained in the Translation Store. It is regenerated from source and participates in the same checked message-locale-fallback and release path as localized artifacts without pretending that source text came from a Provider.

Source code admission acts as the default source approval. A project may require separate source review by publishing an Approval Record bound to the source artifact.

### Intent identity and revision

`MessageIntentId` is opaque persistent identity for history and references; `IntentRevision` identifies localization-relevant semantics. Source text, parameter or selector specifications, semantic UI usage, explicit context, and localization constraints affect revision, while source location and formatting do not. Policy, glossary, Provider, target, and locale-service revisions remain separate dependency inputs.

Compiler-managed identity metadata preserves IDs across ordinary edits and unambiguous moves without becoming a translation catalog. Ambiguous copy, split, merge, or identity conflict requires explicit reconciliation rather than silent history reuse. Persistent Intent identity remains separate from the compact, release-bound Message Handle generated for a target. Exact registry and reconciliation mechanics belong to a dedicated producer and identity design.

### Dependency and stale-state model

Generated artifacts retain typed dependency identities and digests so source, semantic, policy, approval, reachability, target, and runtime changes invalidate only affected work. Invalidation never deletes immutable history or calls a Provider; it makes an old artifact or evidence item ineligible until the applicable explicit workflow supplies a replacement. Exact dependency schemas and cache algorithms belong to component designs.

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

Projects may select different Providers by locale, message surface, semantic context, risk, or policy. Provider routing does not create different target-specific wording for the same Intent revision and requested locale; a target-specific communication requirement is authored as a distinct Message Intent. AI is optional and is never implied by the name “Locale Compiler.”

### Translation Store topologies

The logical Translation Store may be implemented as:

1. local generated artifacts checked into or cached beside the project;
2. a local materialized snapshot pulled from a TMS;
3. a remote artifact store with an integrity-pinned local build input;
4. a TMS as system of record plus an Intlify validation/provenance layer; or
5. a hybrid with sparse human overrides over generated or machine-provided candidates.

The build must consume a stable, complete snapshot. It must not rely on eventually consistent remote reads during code generation.

### Atomic Store publication

Synchronization publishes one immutable `TranslationStoreSnapshot` atomically. A failed publication leaves the previously visible snapshot unchanged, and a build pins one exact snapshot rather than following a changing view during compilation. Local, remote, and TMS-backed stores may use different physical protocols as long as readers never observe a partial snapshot or silent last-write-wins result.

A snapshot may contain validated artifacts without approval. Those artifacts are stored for inspection and review but remain unselectable until applicable evidence is published in the same or a later snapshot. Exact transaction, conflict, retention, and partitioning protocols belong to the Store design.

### Human review

Human-authored messages use the same candidate and validation path as any other source. Approval remains separate immutable evidence bound to exact content and applicable policy inputs. An authorized reviewer can therefore approve a stored artifact without mutating it, and a later policy change can stale approval evidence without pretending that the message bytes or Intent semantics changed.

### Authority and permissions

Candidate supply, deterministic validation, approval, Store publication, release assembly, and deployment are separate powers. A Provider cannot approve merely by supplying a candidate, and an AI agent acts only with the permissions of its authenticated automation identity. Policy may allow low-risk automatic approval and require a distinct human reviewer for high-risk content.

Intlify consumes actor identity and authorization from the surrounding development, CI, TMS, or organizational system rather than becoming an identity provider. Store adapters enforce publication authority, builds verify applicable evidence, and the production Localization Execution Layer receives neither credentials nor approval power. Exact roles, scopes, signatures, revocation, and evidence schemas belong to the synchronization and governance design.

## Developer, Editor, and AI-Agent Tooling

The toolchain exposes structured facts instead of forcing every client to scrape diagnostics or understand every host grammar.

Potential shared capabilities include:

- MF2 completion and syntax diagnostics;
- parameter, selector, function, and markup information;
- missing and unexpected argument diagnostics;
- definition, reference, rename, and source mapping;
- Intent identity and stale-localization explanation;
- coverage, message locale fallback, reachability, and delivery findings;
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

### Development workflow

Normal development remains remote-side-effect free. The compiler derives a `DevelopmentBundlePlan` from the current source and pinned Store snapshot:

- source locale renders from the compiler-derived source-locale artifact;
- a valid approved target artifact renders normally;
- missing, stale, or unapproved requested-locale data renders a Linker-selected approved fallback definition or source artifact and emits a typed development diagnostic;
- a UI adapter may highlight the affected surface and link it to source, status, and the applicable synchronization action; and
- headless, SSR, CLI, notification, and email paths expose the same status through terminal, editor, or structured diagnostics.

`intlify dev` does not call a Provider. `intlify dev --sync` or an equivalent explicit watch command may run synchronization through a trusted development server, keeping credentials out of browser code and applying the same validation and approval policy. A strict development mode uses production coverage and freshness rules.

Stale requested-locale text is not presented as current by default. Tooling may offer an explicitly labeled stale-candidate preview without changing production message-locale-fallback semantics.

### Incremental and explainable processing

Parser, producer, planning, synchronization, validation, linking, and export caches use complete typed dependency digests. Editing one Intent invalidates only that Intent's affected locale requirements and dependent delivery units. Adding a locale creates only the newly required Intent × locale work. A Provider revision does not invalidate an already approved artifact unless policy explicitly requires regeneration or review.

An inspect or explain query reports the dependency path responsible for work, for example source semantic change → new Intent revision → stale Japanese approval → affected checkout delivery unit. Cache presence, concurrency, and execution order cannot change the selected artifacts or findings.

## Portable Parameter, Value, and Function Model

Host types are not the cross-platform semantic model. Generated bindings lower host-language values into a versioned language-neutral `MessageValue` and `ParameterSpecification` model before MF2 evaluation. The portable model is closed and explicit enough to preserve numeric, temporal, selector, missing-value, and function behavior across languages without relying on implicit host coercion.

Functions have versioned identities, checked inputs and options, declared locale-service requirements, and defined failure classifications. Localized MF2 can select only admitted functions and never supplies implementation code. Platform-specific values or functions are explicit Target Profile capabilities; a message that depends on one is not portable to an incompatible target. Exact value families, ranges, encodings, function ABI, and extension rules belong to the Runtime Core specification.

## Localization Execution Model Summary

The Localization Execution Layer is downstream of all localization authority.

```text
build/runtime boundary
  -> one admitted ReleaseSnapshot and immutable target artifacts
  -> conforming Localization Execution Layer
  -> application-selected locale or optional Locale Negotiator
  -> locale-bound execution context
  -> generated Message Handle lookup
  -> selected-message evaluation
  -> text or structured parts
```

Required invariants are:

- no production Provider, TMS, model, credential, prompt, or approval connection;
- no process-global mutable current locale;
- locale negotiation is separate from message locale fallback and single-message evaluation;
- browser localization is application-scoped;
- server localization is request- or task-scoped;
- immutable artifacts and compatible prepared messages may be shared;
- loading may be asynchronous, but formatting after readiness is synchronous;
- linker-materialized message locale fallback is authoritative;
- translated markup remains inert until an allowlisted adapter projects it; and
- missing or incompatible deployed data is an explicit artifact/integration failure.

The application, framework, HTTP layer, or platform obtains user or request locale preferences. It may select a supported requested locale directly or pass those preferences to an optional Intlify Locale Negotiator. Negotiation chooses one requested locale before formatting; it is independent of the source locale, Linker-selected definition locale, and message locale fallback policy.

The Runtime Manifest records supported requested locales, the locale-negotiation profile revision, and the locale-artifact map. Once a locale is selected, execution loads only that requested-locale artifact. The selected record retains its definition locale for language-sensitive MF2 evaluation. The execution layer never searches another locale artifact.

An Intlify Runtime Engine and locale-bound Localizer are the reference physical model described in [015](./015-ox-mf2-runtime-design.md). A target-native path may replace that physical engine only when its exporter, adapter, capabilities, and conformance evidence preserve the same logical responsibilities.

## Language and Target Strategy

| Family | Candidate authoring and producer surface | Candidate target output | Execution ownership |
| --- | --- | --- | --- |
| Browser JS/TS | Known DOM/UI sinks, `intent()`, `mf2`, generated handles | ESM, Locale Capsule, manifest, source lowering | Application-scoped Localizer and Web adapter |
| Vue and Web frameworks | Template extraction, compiler plugin, explicit script authoring | Client and SSR modules, generated render bindings | Framework context over client or request Localizer |
| SSR and server | Producer-generated handles and explicit headless formatting | Server modules and immutable locale artifacts | Request- or task-scoped Localizer |
| iOS | Swift macro/compiler plugin, SwiftUI/UIKit sink analysis | Locale Capsule, `.xcstrings`, generated Swift bindings | Application, scene, or view-tree adapter |
| Android | Kotlin/Java plugin, Compose and Views/XML analysis | Locale Capsule, `strings.xml`, generated Kotlin/Java bindings | Application, activity, composition, or task adapter |
| Rust and native | Macros, build integration, object/final-binary evidence | Baked Rust, capsule, native data, C ABI bindings | Explicit Localizer or application adapter |
| C/C++, Go, .NET, JVM services | Language/compiler-specific producers | Generated bindings, native artifact, capsule | Conforming binding or target-native execution |

Every target must provide the same observable Intlify semantics for MF2 declaration and selection, parameter validation, MF2 fallback values, markup and parts ordering, bidi behavior, failure classification, diagnostic and evidence schemas, resource-limit meaning, handle/artifact compatibility, and output model. The physical code and platform services do not need to be identical.

For example, Node.js may call the Rust reference implementation through N-API, a browser may use WASM or a conforming JavaScript implementation, and a mobile or native target may use a C ABI or locale services such as Foundation, ICU4J, ICU4X, or ICU. Platform-native resource formats may also be generated when they can represent the required behavior without changing its meaning.

Locale-dependent number, date, time, plural, and similar operations run through an explicit `LocaleServiceProfile` containing the provider kind and revision, locale-data and timezone-data revisions, supported functions, and reproducibility class.

- A `pinned` profile must produce the same locale-dependent parts and diagnostics for the same complete semantic input and profile revision.
- A `platform-managed` profile preserves artifact selection, MF2 semantics, result schemas, and failure classification but permits only explicitly declared locale-dependent output variation from the platform service.

Each `TargetProfile` records the semantic-specification version, Runtime ABI version, Locale Service Profile, supported capabilities, and output model. Each physical implementation must pass the applicable conformance tests and report its capabilities. If a target runtime or native resource format cannot preserve a required Intlify or MF2 feature, the Target Exporter must select a compatible representation or report the unsupported feature instead of silently changing the result.

## Library and Open-World Composition

A library cannot know the final application's requested locales, Provider routing, message locale fallback, approval policy, Target Profiles, or delivery topology. A source-first library therefore publishes a versioned language-neutral manifest containing its Message Intents, source-locale artifacts, references, requirements, and package identity. It does not precompute the final application bundle plan.

The final application composes direct and transitive library manifests with application artifacts before requirement planning. The library owns source message semantics; the application owns requested locales, message locale fallback, coverage, Provider routing, trust, final reachability, and release approval. A library may include localized artifacts as optional candidates, but application policy decides whether their provenance and approval evidence are admissible.

Package and Intent identity prevent collisions when multiple library versions coexist, and each library Intent retains its own source locale. The final application admits supported manifest specification versions and never silently rewrites incompatible semantics. Initial implementation targets build-known static composition; admission of modules unknown at application-build time remains a separate later design.

## Release Assembly and Deployment

Each Target Exporter produces a complete, capability-admitted output set for one Target Profile. Host build integration then invokes Release Assembly after all outputs in one deployment compatibility group are available. Release Assembly creates one immutable `ReleaseSnapshot` over the pinned project and Store inputs, source-locale artifacts, Message Bundle Plan, generated bindings, one or more Target Profile output sets, manifests, semantic specifications, and Runtime ABIs.

The compatibility group follows deployment coupling rather than product family: Browser and SSR outputs that must hydrate consistently may share a Release snapshot, while independently built Web and mobile applications may use separate snapshots. A Release snapshot is a localization release manifest, not the application's complete deployment manifest.

Intlify does not own a project's CDN, application store, or deployment orchestrator. It provides immutable artifact naming, integrity data, compatibility admission, and the Release snapshot needed for a consistent rollout. A deployment uploads and verifies versioned artifacts before activating the corresponding manifest or release pointer. Target execution rejects handles, manifests, or locale artifacts from another release instead of rendering a mixed revision.

Packaged mobile and native applications may obtain this atomicity from the application package itself. Web, server, OTA, and remotely loaded locale deployments use versioned release namespaces and activate the manifest last. Previous releases remain addressable long enough for rollback and already-running clients, then may be garbage-collected by deployment policy.

## Conceptual Product Surfaces

Exact commands and package names are deferred, but Intlify needs coherent surfaces for:

| Surface | Responsibility |
| --- | --- |
| `intlify fmt` | Format source-authored MF2 and supported localization interchange content |
| `intlify lint` / `check` | Report syntax, semantics, policy, localization coverage, and project findings |
| `intlify sync` | Execute a finite requirement plan and atomically publish validated localized artifacts and evidence |
| `intlify dev` / `intlify dev --sync` | Run side-effect-free development diagnostics or opt into trusted incremental synchronization |
| Build integration | Run producers, pin profiles and Store snapshots, link, lower source, export, and assemble a Release snapshot |
| Inspect/explain API | Expose identity, dependency invalidation, requirements, message locale fallback, reachability, provenance, and stale reasons |
| Editor/agent service | Return structured semantic queries, diagnostics, references, and edits |
| Runtime API | Create locale-bound execution contexts and format generated handles through the reference runtime path |

No command in this table is reserved merely by appearing here. The fixed property is the separation between explicit synchronization, deterministic build, and production localization execution.

## Security, Trust, and Reproducibility

- Treat source comments, external documents, TMS content, Provider output, and imported localization data as untrusted data.
- Never forward credentials, production requests, secrets, or unrelated user-generated content into Provider requests implicitly.
- AI prompts and responses are candidate-generation inputs, not executable compiler instructions.
- Parse and validate all returned MF2 with the shared implementation.
- Check declared parameters, selectors, functions, markup, and policy before Store publication; derive required capabilities for preflight and perform authoritative Target Profile admission before target or Release publication.
- Bind provenance and approval to exact revisions and content digests.
- Keep localized-message payloads logically separate from validation, approval, and revocation evidence.
- Canonicalize artifact field ordering, message ordering, locale ordering, and digest inputs.
- Pin Provider configuration and revision when reproducibility requires it; do not regenerate approved output implicitly.
- Make translated markup inert and allowlist its projection at the target adapter.
- Reject incompatible, oversized, incomplete, or integrity-invalid artifact sets fail-completely.
- Publish Store and Release manifests atomically and reject concurrent publication conflicts rather than silently overwriting them.
- Give source authors, synchronization operators, Providers, reviewers, publishers, builders, deployers, and runtimes only their required powers.
- Keep network, credentials, Provider SDKs, model clients, and TMS connections outside production execution artifacts.

## Failure and Diagnostic Model

Intlify should make localization failures early and actionable.

| Situation | Required behavior |
| --- | --- |
| Known UI sink with unanalyzable value | Compile diagnostic with an explicit-authoring suggestion |
| Dynamic message source | Compile diagnostic; no hidden runtime translation |
| Invalid source or target MF2 | Parser/semantic finding with mapped source or artifact evidence |
| Missing or unexpected parameter | Compile/export diagnostic where provable; typed runtime failure only for unchecked external calls |
| Missing or stale localized artifact | Build diagnostic that identifies the requirement and synchronization action |
| Store publication conflict or incomplete transaction | Keep the previous snapshot visible and require retry, replanning, or explicit merge |
| Unapproved high-risk candidate | Block release according to project policy |
| Missing requested-locale definition with configured message locale fallback | Linker materializes the selected definition and retains its definition locale |
| Unresolved required message | Blocking linker/build finding |
| Unsupported target feature | Export capability error before publishing partial outputs |
| Mixed-release handle, manifest, or locale artifact | Target execution admission failure; never combine releases |
| Missing deployed handle or incompatible artifact | Target integration failure; no Provider call or invented message locale fallback |

Diagnostics should explain not only what failed, but also which producer, Intent revision, locale requirement, Provider/store artifact, policy or approval revision, dependency edge, message locale fallback decision, delivery unit, Target Profile capability, Store snapshot, Release snapshot, or execution compatibility edge caused it.

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
| Source-first flow | Isolated end-to-end PoC in PR #183 | Resolved project profile, stable identity, production MF2, requirement planning, versioned artifacts, and target integration |
| Editor and AI-agent tooling | LSP/editor and agent integration designs | One shared structured semantic/query service and host-language projections |
| Localization synchronization | Provider concepts proven by PoC and design discussion | Provider/TMS/store APIs, atomic snapshots, validation gates, provenance, scoped approval, and CLI workflow |
| Localization execution | Reference runtime architecture in 015 | Portable values and functions, locale profiles and negotiation, production MF2 Runtime Core, target-native conformance, and adapters |
| Libraries | Existing package and linker foundations | Source-first library manifests, final-application composition, trust, and optional future dynamic-module admission |
| Mobile and native | Architecture direction and target strategy | Producers, capability-checked exporters, native resources/bindings, Runtime adapters, and Release snapshot integration |

The New Concept is therefore an integration direction over foundations already present in the repository, not a claim that every architectural area is implemented.

## Existing Resource Implementation and Compatibility Follow-Up

The target architecture in this document is source-first. The current `intlify_resource` implementation, catalog assignment model, key-based `MessageDefinitionArtifact`, and `t()`-style authoring path are not primary authoring surfaces or source-first core artifacts.

They are existing implementation assets whose future disposition requires a separate design decision. This overview does not promise that catalog authoring remains a permanent compatibility mode, and it does not require removing the current implementation before the source-first path can be built.

Reusable implementation knowledge may include host-format parsing, escaping, source mapping, bounded admission, validated write-back, and JSON, YAML, XLIFF, or Vue SFC interchange behavior. Reuse remains behind source-first interfaces; a reused parser or mapping utility does not make a catalog an authoring source.

A dedicated follow-up decides whether existing catalogs are migration-only inputs or remain behind a legacy adapter, whether bidirectional interchange needs write-back, how existing keys and translation history associate with generated Intent identity, which modules are generalized, and how long compatibility is maintained. Until then, the current resource path is implementation context rather than normative source-first architecture.

## Expected Outcomes

The architecture succeeds when:

- **O1 — Natural source authoring:** a developer can add ordinary static UI text without creating a message key or editing locale catalogs.
- **O2 — Explicit power when needed:** advanced, parameterized, reusable, or headless messages have a predictable MF2-based authoring path.
- **O3 — Provider-neutral supply:** requested-locale messages can come from AI, MT, TMS, rules, or humans without changing compiler or execution specifications.
- **O4 — Governed localization:** stored, selectable, and release-admitted states are distinct, and every selected message is validated, traceable, and approved according to pinned policy.
- **O5 — Offline, artifact-driven delivery:** normal builds and production formatting work from pinned artifacts without Provider or TMS network access.
- **O6 — Early, explainable failure:** missing, stale, invalid, unapproved, unreachable, and unsupported states are diagnosed before deployment with their dependency cause.
- **O7 — Finite delivery:** only reachable messages, requested locales, and required delivery units are emitted.
- **O8 — Scoped locale state:** browser and SSR rendering never depend on a process-global mutable locale.
- **O9 — Shared tooling semantics:** editors and AI-agent tools query the same semantics, evidence, and findings as the compiler.
- **O10 — Source-first integrity:** reuse and compatibility work remains isolated from source-first interfaces and artifact specifications.
- **O11 — Cross-platform meaning:** Web, mobile, and native targets use the same language-neutral semantic, value, function, artifact, and logical execution specifications while allowing conforming physical engines.
- **O12 — Consistent releases:** generated bindings, locale outputs, manifests, specifications, and Runtime ABIs for one deployment compatibility group are deployed and admitted as one Release snapshot.
- **O13 — Library composition:** application and transitive library localization requirements compose before final synchronization, linking, and release policy.
- **O14 — Incremental operation:** source, policy, locale, target, and dependency changes invalidate only affected work and are explainable through typed dependency edges.

## Roadmap

The Roadmap is ordered by implementation dependencies. It records product-level direction rather than component exit criteria; implementation plans own exact tests, schedules, and completion gates. The traceability table distinguishes specification foundations from the first observable product evidence.

### I0: Shared interfaces and artifact specifications

- Ratify this product boundary and glossary.
- Define the shared project, locale, Message Intent, requirement, artifact-state, Store, target-output, Release, and execution specifications.
- Define explicit specification-version and capability admission plus deterministic toolchain migration.
- Define requirement planning, final output linking, and static library composition boundaries.
- Establish language-neutral conformance fixtures shared across producers, stores, linkers, exporters, locale services, and execution engines.

### I1: JavaScript/Web vertical slice

- Replace the PoC placeholder parser with `ox-mf2`.
- Implement bounded JavaScript/TypeScript UI-sink recognition plus explicit `intent()` and standalone `mf2` authoring.
- Generate stable Intent identity metadata and compiler-derived source-locale artifacts.
- Add explicit local or fixture-backed `intlify sync` over a finite Requirement Plan.
- Materialize stored and selectable artifacts through an atomic local Store snapshot.
- Reuse or adapt the current linker and ESM exporter while keeping source-first interfaces normative.
- Implement the reference Web execution path from 015 and assemble a local Web Release snapshot.
- Add development message-locale-fallback diagnostics and dependency-digest incremental processing.

### I2: Vue, SSR, editor, and agent integration

- Add Vue template and script producers without putting Vue types in shared specifications.
- Add client and SSR target lowering with request-safe Localizers.
- Expose shared MF2, Intent, dependency, and planning queries to editor and AI-agent adapters.
- Add incremental project inventory, sync preview, coverage, and stale-artifact diagnostics.
- Prove hydration consistency and concurrent requests with different locales.

### I3: TMS and production synchronization

- Add at least one real TMS/Localization Provider adapter.
- Define pull, push, conflict, refresh, retry, rate-limit, scoped-actor, and approval workflows.
- Support local, TMS-backed, and hybrid Translation Store topologies with atomic snapshot publication.
- Add integrity-pinned CI synchronization, concurrent-publication checks, and deterministic build fixtures.
- Prove sparse human review and override without returning to hand-maintained full catalogs.
- Prove versioned Release publication and rollback through a deployment integration fixture.

### I4: Mobile targets

- Add Swift/SwiftUI and Kotlin/Compose producer experiments.
- Generate `.xcstrings`, Android resources, or portable Locale Capsules through authoritative capability-checked exporters.
- Add application/scene-equivalent execution adapters and portable value bindings.
- Reuse shared artifact, locale-service-profile, and MF2 conformance fixtures across Web, iOS, and Android.

### I5: Native and system-language composition

- Add Rust and at least one additional system-language producer.
- Implement static library-manifest composition and final-application linking.
- Add C ABI or conforming native execution bindings.
- Generate native or baked target artifacts without changing shared message semantics.
- Prove bounded dynamic references, final-binary reachability evidence, and offline deployment.
- Consider self-contained runtime module admission only after static composition is stable.

### Outcome traceability

| Outcome | Foundation | First observable evidence | Expanded evidence |
| --- | --- | --- | --- |
| O1 | I0 authoring specification | I1 Web | I4–I5 mobile and native |
| O2 | I0 message specification | I1 explicit MF2 | I4–I5 host-language authoring |
| O3 | I0 Provider specification | I1 fixture Provider | I3 real Provider/TMS |
| O4 | I0 state and evidence specifications | I1 local Store | I3 production workflow |
| O5 | I0 build/execution boundary | I1 offline Web | I3–I5 production and native paths |
| O6 | I0 finding specification | I1 Web diagnostics | I2–I5 integrations and targets |
| O7 | I0 planning/linking specification | I1 Web pruning | I5 final-binary evidence |
| O8 | I0 execution specification | I1 scoped Web execution | I2 concurrent SSR |
| O9 | I0 query specification | I2 editor and agent integration | Later host projections |
| O10 | I0 architecture | I1 source-first slice | Continuous across milestones |
| O11 | I0 semantic and conformance foundation | I4 Web/mobile conformance | I5 native conformance |
| O12 | I0 Release specification | I1 local Web release | I3–I5 deployment groups |
| O13 | I0 library composition specification | I5 static composition | Later dynamic-module work |
| O14 | I0 dependency specification | I1 local incremental flow | I2–I3 project and Store workflows |

## Deferred Follow-Up Notes

The following need dedicated designs and do not block this overview:

- source authoring and identity: producer recognition, `intent()` and `mf2` APIs, source mapping, persistent-ID encoding, and reconciliation;
- shared inputs: exact project-profile, artifact, dependency, capability, version-admission, and migration schemas;
- synchronization and governance: Provider/TMS transport, Store protocol, review, policy, approval, actor scope, conflict, retention, and credentials;
- linking and generation: requirement-plan and final-link APIs, source lowering, bundler integration, and target capability matrices;
- target execution: Locale Capsule, Runtime Manifest, Message Handle, portable values, parts, function ABI, locale services, bindings, and conformance;
- tooling: LSP, editor, agent, inspect, and suggested-edit query protocols;
- composition and release: library manifests, package identity, optional dynamic modules, Release wire format, signing, deployment adapters, retention, and rollback; and
- migration and packaging: existing catalog import and translation-memory association, commands, configuration layout, packages, and release sequencing.

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
| [014-ox-mf2-message-linker-design.md](./014-ox-mf2-message-linker-design.md) | Owns current reference/definition resolution, message locale fallback, reachability, delivery planning, export preparation, and current ESM output. Source-first integration must explicitly adapt or evolve its catalog-oriented definition input. |
| [015-ox-mf2-runtime-design.md](./015-ox-mf2-runtime-design.md) | Owns the detailed runtime-side architecture below the build/runtime boundary. |
| [PR #183](https://github.com/intlify/intlify/pull/183) | Provides the isolated source-first PoC and review feedback used to clarify this overview; it does not freeze production APIs. |
