# ox-mf2 Message Linker Design

## Purpose

This document defines the overview-level architecture for an Intlify message linker. The linker connects application message references with catalog definitions, resolves locale fallback, computes message reachability, reports consistency findings, and produces platform-neutral bundle plans.

The design makes data requirements, finite production locales, generated artifacts, and pre-runtime validation explicit. It also covers the application-specific concepts that message catalogs require: language-specific reference production, dynamic-key bounds, authored-catalog maintenance, delivery-unit reachability, and final-application composition.

This document owns the language-neutral linker boundary and its public artifacts. It builds on the resource extraction and validated write-back contracts in [013-ox-mf2-resource-catalog-adapter-design.md](./013-ox-mf2-resource-catalog-adapter-design.md), the CLI and configuration foundation in [006-ox-mf2-phase-3a-tooling-foundation-design.md](./006-ox-mf2-phase-3a-tooling-foundation-design.md), the linter presentation surface in [008-ox-mf2-phase-3c-linter-design.md](./008-ox-mf2-phase-3c-linter-design.md), and the editor integration boundary in [009-ox-mf2-phase-3d-lsp-editor-design.md](./009-ox-mf2-phase-3d-lsp-editor-design.md).

## Goals

- Define a programming-language-neutral contract for message references and catalog definitions.
- Resolve exact and bounded dynamic references without silently dropping a possibly used message.
- Detect unresolved references, translation coverage gaps, orphaned translations, and unused definitions before production runtime.
- Produce deterministic per-locale and per-delivery-unit bundle plans that avoid whole-catalog and all-locale eager delivery.
- Reuse one linker analysis across lint, generation, pruning, bundler integration, and future editor features.
- Support final-application composition across application code, libraries, plugins, and multiple producer languages.
- Keep catalog extraction, host-format parsing, and validated write-back owned by `intlify_resource`.
- Make public artifacts versioned, testable by third-party producers, and safe to reject when incomplete or incompatible.

## Non-Goals

- Replacing the message-level parser, formatter, linter, or runtime formatter.
- Introducing a second catalog extractor, key resolver, locale-binding implementation, or host-format write-back path.
- Making the linker core parse JS/TS, Rust, C/C++, WASM, or build-system-specific inputs directly.
- Defining runtime caching, retries, suspense, locale negotiation, or application loading policy beyond the generated loader-map contract.
- Shipping every producer, exporter, native embedding strategy, or delivery-unit granularity in the first milestone.
- Requiring code-first message authoring or replacing catalog-authored workflows.
- Treating open-world development or library builds as if they were closed final-application links.
- Finalizing evidence-gated payload containers, deduplication strategies, or baked native data before concrete consumers and benchmarks justify them.

## Ownership

The future `intlify_link_contract` boundary owns public artifact types, wire compatibility, version negotiation, and producer conformance tests. Language-specific producers own source, object, or binary analysis and emit that contract. The future `intlify_message_linker` core consumes only reference artifacts, definition artifacts, locale policy, and the delivery graph; it owns resolution, reachability, placement, findings, and bundle plans.

`intlify_resource` remains the sole owner of catalog assignment, host parsing, message entry extraction, catalog key domains, source spans, and validated write-back. `intlify_lint` presents linker findings through its rule and reporting contracts. Platform integrations own build-graph adaptation and exporter invocation, while runtimes own loading policy after consuming generated assets and loader maps.

This overview fixes those responsibility boundaries and the intended product direction. Detailed wire schemas, rule contracts, configuration validation, producer encodings, delivery-graph algorithms, and exporter formats land through the milestones and open questions below rather than being inferred by individual implementations.

## Problem Statement

i18n libraries and message resources repeatedly produce the same failures:

1. **Missing message**: application code references a message that no catalog supplies. Discovered at runtime, in production, in whichever locale a user happened to hit.
2. **Untranslated locale**: one or more locales lack a translation for an existing message.
3. **Unused messages**: entries no code can reference anymore accumulate in the catalog, costing translation, review, and bytes forever.
4. **Whole-catalog delivery**: importing the catalog ships every message — into the JS bundle, or compiled into the native binary — regardless of what the code can reference.
5. **All locales up front**: every supported locale enters the initial bundle instead of one asset per locale loaded on demand.
6. **Lazy features pay eagerly**: messages used only by a lazily loaded feature still land in the main delivery unit.
7. **Per-tool reimplementation**: lint rules, source scanners, and bundler-specific tree-shaking each re-implement the same reachability analysis.

Problems 1–3 are consistency problems between two things the project itself authors — its code and its catalogs — and their fixes are edits to authored files. Problems 4–6 are delivery problems and belong to packaging. Problem 7 is the architectural problem: every prior mitigation solved one surface at a time.

Solving these problems requires the project to state data requirements before generation, constrain production locales to a finite set, generate only what is reachable, and detect required-but-unavailable messages before production runtime. Message resources also need application-reference analysis, bounded dynamic-key handling, locale fallback, and final program or module composition. Intlify therefore gets a dedicated **message linker**.

## Design Overview: A Linker for Messages

The linker core recognizes no particular programming language or build system. JS/TS, Rust, C/C++, and others are **reference producers** that feed one common contract; catalogs feed the other side through the existing `intlify_resource` extraction path. The linker resolves references against definitions under a locale/fallback policy and a delivery-unit graph, and produces bundle plans and findings. One analysis, every surface:

| Problem | Linker behavior |
| --- | --- |
| referenced message key does not exist | symbol resolution error (`unresolved-message`) |
| a locale lacks a translation | locale/fallback resolution finding (`missing-translation`) |
| catalog contains unused messages | unreachable definition (`unused-message`) |
| whole catalog gets bundled | emit only reachable messages |
| all locales in the initial bundle | separate assets per locale |
| lazy-feature messages in the main unit | split messages per delivery unit |
| linter and build integration repeat the analysis | one shared linker core |

## Architecture

```text
Language-specific Reference Producer
  ├─ JS/TS AST transform
  ├─ Rust macro / binary scanner
  ├─ C/C++ macro / object scanner
  └─ WASM binary scanner
                │
                ↓
      Message Reference Artifact
                │
message catalog │  locale/fallback policy
      ↓         │          ↓
intlify_resource│    Delivery Unit Graph
      ↓         │          ↓
Message Definition Artifact
                │
                └──────────┐
                           ↓
             Language-neutral Message Linker
               ├─ reference resolution
               ├─ locale/fallback resolution
               ├─ reachability
               ├─ missing/unused findings
               └─ Message Bundle Plan
                           │
                 Platform Exporters
               ├─ ESM
               ├─ binary blob
               ├─ baked Rust
               └─ generated C/C++
```

The linker never parses JS/TS source, Rust crates, or C/C++ objects directly. Each language producer emits a common-format `MessageReferenceArtifact`; the catalog side converts `intlify_resource` extraction results into a common-format `MessageDefinitionArtifact`. The linker consumes only the two artifacts, the locale policy, and the delivery graph.

## Data Selection and Delivery Constraints

- State data requirements explicitly before generation.
- Constrain production locales to a finite, declared set.
- Generate data only after requirements are fixed.
- Detect required-but-unavailable data before production runtime.
- Separate source data from generated artifacts.
- Make runtime data requirements extractable from a binary.

The linker selection input is not the message key alone. It is the combination of reachable message references or bounded selectors produced per language, the locale policy, and the delivery graph.

### Mechanism decisions

| Mechanism | Decision |
| --- | --- |
| Native final-binary scanning | Required for the initial Rust/C/C++ native producer direction because the final binary identifies which references survived conditional compilation, dead-code elimination, and LTO. JS instead uses its bundler's module and chunk graph. |
| Packed container with a hashed key index | Deferred. Per-locale, per-unit assets over HTTP or the filesystem cover the initial delivery model; a packed container requires concrete evidence. |
| Cross-locale deduplication and locale families | Deferred. Applications enumerate locales explicitly; omitting values identical to fallback remains evidence-gated. |
| Runtime provider or adapter stack | Out of scope. Delivery ends at assets plus a minimal loader map; runtimes own loading policy. |
| Artifact versioning | Required for public linker artifacts. Catalog sources remain versioned by VCS, while each artifact carries an explicit format version and participates in compatibility negotiation. |

## Message-Domain Prior Art

- **`@intlify/eslint-plugin-vue-i18n`** (`no-missing-keys`, `no-unused-keys`): the right checks in an awkward home — per-file, JS-implemented, duplicated configuration, unconnected to delivery. The linker gives the same checks one core and connects them to what ships.
- **lingui** (`extract --clean`, `compile --strict`), **i18next-parser** (sync): reference-driven catalog maintenance precedent.
- **formatjs** (`refers/formatjs`): the inverse, code-first authoring model; out of scope, but the reference-producer half is reusable if ever wanted.
- **paraglide-js**: per-message accessor functions make bundler DCE prune per key — proof that per-key delivery works when the API is accessor-shaped, and a demonstration of its limit: string-keyed `t(variable)` defeats import-graph DCE, which is why pruning here is linker-driven.
- **Android resources / App Bundles**: per-locale splits delivered on demand — the shape of problem 5, in production for a decade.
- **gettext** (`msgfmt --statistics`, `.mo`): parity reporting and precompiled catalogs as ordinary build steps.

## Artifacts and Contracts

### Message Reference Artifact

Conceptual shape:

```rust
pub struct MessageReferenceArtifact {
    pub version: ArtifactVersion,
    pub producer: ProducerIdentity,
    pub delivery_unit: DeliveryUnitId,
    pub references: Vec<MessageReference>,
}

pub struct MessageReference {
    pub scope: CatalogScopeId,
    pub selector: MessageSelector,
    pub reason: Option<ReasonText>,
    pub origin: Option<SourceOrigin>,
}

pub enum MessageSelector {
    Exact(MessageKey),
    Prefix(MessageKeyPrefix),
    Pattern(MessageKeyPattern),
    AllInScope,
}
```

- `version` identifies wire-contract compatibility; `producer` identifies the emitting frontend or scanner and its version.
- `delivery_unit` identifies the output unit that owns the references.
- `selector` expresses a static key or a bounded dynamic set. **Non-`Exact` selectors should carry `reason`** (from the declaring API or config) so reviewers can see why a widening exists; producers that cannot supply one emit it absent, never fabricated.
- `origin` is diagnostic source location; linker correctness never depends on it. Producers that have locations should supply them (directly or via a debug sidecar artifact) so findings can point at reference sites.

**The artifact is a public, versioned contract from day one.** Libraries and plugins distribute artifacts through package registries, so this is an ecosystem contract, not an internal cache format: it needs explicit stability stages and v1 freeze criteria, reserved room for extension, a conformance test suite that any third-party producer can run, and explicit version negotiation at link time. A producer failure (unparsable configured source, scanner error) fails artifact production fail-complete, in the 013 tradition — a partial artifact is never emitted silently.

### Message Definition Artifact

Catalog entries extracted by `intlify_resource` convert into linker-facing definitions:

```rust
pub struct MessageDefinition {
    pub scope: CatalogScopeId,
    pub domain: CatalogKeyDomain,
    pub key: CatalogKey,
    pub locale: Locale,
    pub message: MessagePayload,
    pub source: EntryReference,
}
```

- `scope`, `domain`, and `key` define message identity; `locale` resolves from 013 locale binding; `source` keeps the original catalog path, entry identity, and span.
- Catalog-level checks and the linker share one extraction result. No second catalog parser or key resolver is introduced.

### Scope identity across packages

`CatalogScopeId` must compare correctly across independently published artifacts. Within one project, scopes are the 013 comparison scopes. For distributed artifacts, scope identity follows the precedent 013 sets for third-party `CatalogKeyDomain` issuance: a host-constructed opaque identity derived from package identity and a package-local scope name. Another package cannot create cross-package equivalence by making a unilateral name claim; the final application's explicit host-side mapping is the only way to introduce that equivalence. The exact package identity, compatibility, and trust contract remains an open question.

## Delivery Units

The earlier JS/TS-centric "bundler chunk" generalizes to an opaque `DeliveryUnitId` plus a dependency graph; the core has no platform enum.

| Platform      | Example delivery units                     |
| ------------- | ------------------------------------------ |
| Vite/Rolldown | JavaScript chunk                           |
| Rust          | executable, crate, feature module          |
| C/C++         | executable, shared library, static library |
| WASM          | WASM module                                |
| Mobile        | application feature, asset pack            |
| Plugin system | plugin package                             |

```text
entry
├─ checkout
│  └─ payment
└─ settings
```

For each delivery unit the linker computes reachable messages and produces per-locale `MessageBundlePlan`s. Whether a unit is a JavaScript chunk or a native shared library is irrelevant to the core.

**Granularity differs per platform and must be declared honestly.** A bundler integration can attribute references to chunks from its live graph. A final-binary scan can only produce a single-unit artifact — post-link, references cannot be attributed to sub-units; per-unit native granularity requires object-level scanning before link. Native v1 is therefore whole-program (one unit), which is correct, just coarse.

### Shared-message placement policy

When several units reach the same message (`common.ok` from both `checkout` and `settings`), the plan must place it deterministically. Placement is a linker-owned, per-target policy fixed together with the plan format:

- `hoist` (candidate default): assign to a loading-order-safe shared ancestor of all referencing units in the delivery graph.
- `duplicate`: each referencing leaf carries its own copy — larger totals, no cross-unit loading dependency.

Both policies must be deterministic; the default and the exact shared-ancestor algorithm for a delivery DAG remain open. Mixed per-scope policy is future work. Exporters consume placement results, never re-derive them.

## Linking Semantics

### Resolution and unresolved messages

Production linking is **closed-world over locales**: the supported locale set is finite and declared, and each locale resolves through its explicit fallback chain (byte-exact locale strings per 013):

```text
ja-JP → ja → en
```

If no locale in a requested chain defines a referenced message, runtime formatting cannot succeed; that is the build error `unresolved-message`.

### Coverage and the baseline locale

If `ja` lacks a message that `en` supplies, fallback makes runtime resolution succeed; that is not an unresolved reference but a translation-coverage gap: `missing-translation`, warning or error by project policy (a strict coverage policy may reject gaps even when fallback would save them).

Resolution semantics need no privileged locale — the chain replaces it. A **coverage baseline** locale per scope survives for exactly two jobs: as the yardstick for coverage-style reporting (`orphaned-translation`: defined in some locale but absent from the baseline) and as the source of typed-key generation. It has no effect on resolution.

### Reachability and unused messages

Reachability roots are: static references, bounded selectors, and roots declared in configuration — messages requested from outside scanned code (server-driven, feature flags), each declared as a selector with a reason. A definition reachable from none of them is `unused-message`:

```text
catalog definitions
  ├─ checkout.title  ← reachable
  ├─ checkout.total  ← reachable
  └─ checkout.old    ← unreachable
```

`checkout.old` can be excluded from production assets. Project-wide unused definitions are distinct from messages that are used by one delivery unit but absent from another's plan — the latter is normal slicing, not a finding. Exclusion from shipping does not remove the entry from the catalog; that is `prune`'s job below.

### Dynamic references

Dynamic keys are the safety boundary shared by every producer. When a full key is not statically known, the producer emits a bounded selector (`Prefix` / `Pattern` / `AllInScope`). An unbounded dynamic reference follows explicit build policy:

- **strict mode**: build error (`unbounded-dynamic-reference`).
- **compat mode**: retain the entire target scope and warn.
- In no mode does the linker guess a narrower set — a possibly-used message is never silently dropped.

**Analysis degradation is itself visible.** A wide selector (`AllInScope`, broad `Prefix`) makes `unused-message` vacuous within its range; the linker reports `degraded-analysis` naming the scope, the selector, and its producer, so a project can see _why_ unused reporting went quiet instead of trusting a silently weakened result. Selector `reason` fields feed this finding.

## Findings and Lint Integration

Linker finding categories:

| Finding | Meaning | Default severity |
| --- | --- | --- |
| `unresolved-message` | reference resolves in no locale of a requested chain | error |
| `missing-translation` | resolvable only via fallback; coverage gap in a requested locale | warn |
| `orphaned-translation` | defined in a locale but absent from the coverage baseline | warn |
| `unused-message` | definition unreachable from every root | warn |
| `unbounded-dynamic-reference` | dynamic site with no bounded selector (strict mode) | error |
| `degraded-analysis` | wide selector or open-world participant suppressed unused analysis in a scope | warn |

**`intlify lint` is the presentation surface**: each category ships one-to-one as a lint rule under the same id, configured, reported, counted, and exit-coded through the 008 contracts and the catalog-level linter addendum 013 already calls for (project-scope execution, typed subject identities, related-entry evidence). These rules are the toolchain successor of the ESLint plugin's `no-missing-keys` / `no-unused-keys`. Reference-anchored findings (`unresolved-message`, `unbounded-dynamic-reference`) anchor at the code site when `origin` is available, with the probed chain as evidence; definition-anchored findings (`unused-message`, coverage findings) anchor at the 013 entry span with reference sites or their absence as related evidence. The editor (009) presents both directions incrementally from whatever source-level reference information is available in the open development world. Producer failures surface as lint operational errors, not findings.

## Fixes and Generation

- **`prune`**: the linker makes shipping safe even with stale entries, but stale entries still cost translation and review. `intlify messages prune` turns `unused-message` findings into a deterministic deletion plan applied through 013 validated write-back (plan/dry-run by default, `--write` to apply). Deleting entries is a semantic decision, so it is a command, never a lint autofix. Stub scaffolding for missing/untranslated entries remains a deferred candidate pending translation-workflow conventions.
- **Typed keys**: generated from the coverage baseline's definitions — TypeScript key unions plus, where MF2 declarations allow, argument types:

```ts
type MessageKey = 'checkout.title' | 'checkout.total' | 'errors.network'

declare function t<K extends MessageKey>(key: K, args: MessageArgs<K>): string
```

Generated types give early, in-editor feedback; per-locale availability and fallback resolution remain the linker's final authority.

- **Determinism and freshness**: linker output (plans, exported assets, generated types) is byte-deterministic for identical inputs, and every generation surface has a `--check` mode that re-runs and diffs instead of writing — the CI freshness job.

## Reference Producers

### JS/TS

An AST transform recognizes existing static-key APIs (`t('checkout.title')` and the configured recognizer surface). Dynamic sets are declared with a bounding API rather than an annotation comment — the declaration is code: refactorable, type-checkable, and usable as a runtime value:

```ts
const errors = useMessageSet('errors.*')

errors.format(errorCode)
```

Integration with Vite/Rolldown:

```text
JS/TS frontend
    → module-level references
    → bundler's live chunk graph
    → delivery-unit references
    → MessageReferenceArtifact
```

The catalog object is never imported wholesale into JavaScript; the integration emits virtual modules or external assets from the linker's plan. A CLI source-scan mode (globs, no bundler) produces a single-unit artifact for projects without bundler integration.

### Rust

Macros or generated APIs are more reliable than source-text scanning:

```rust
let title = intlify::message!("checkout.title");
let errors = intlify::message_set!("errors.*");
```

The macro embeds a stable message reference ID into executed code (`leading tag + message reference hash`), so scanning the final binary collects exactly the references that survived `#[cfg]`, dead-code elimination, and LTO:

```text
Rust source
    ↓ macro expansion
message reference ID
    ↓ rustc / linker / DCE
final executable
    ↓ intlify reference scanner
MessageReferenceArtifact
```

The final binary is the authority for native reachability because it reflects conditional compilation, dead-code elimination, and LTO. Source locations are not embedded in binaries because of size and build-path leakage; the reference-ID-to-origin mapping is a debug sidecar artifact.

### C/C++

```cpp
INTLIFY_MESSAGE(CheckoutTitle, "checkout.title");
INTLIFY_MESSAGE_SET(ErrorMessages, "errors.*");
```

Macros embed reference IDs or records into objects/binaries, scanned after final link. How references survive LTO, COMDAT folding, and Mach-O/ELF/PE handling must be validated before implementation; where exact elimination cannot be guaranteed, the producer over-retains — it never guesses in the under-inclusive direction.

### WASM and other languages

WASM built from Rust or C/C++ reuses the same scanner if the tagged IDs survive into the module. Any other language participates via a compiler plugin, macro/annotation processor, semantic analyzer, object/binary scanner, or an explicit build-system manifest — all producing the same artifact. The linker core never grows language-specific analysis.

### Native build ordering

External message bundles need no cycle: build the executable, scan it, generate per-locale bundles, ship them alongside. Baking messages _into_ the binary is circular (selecting messages needs the binary; finishing the binary needs the data) and requires one of: a probe-build → datagen → final-build two-phase build, object-file scanning before final link, generated source/objects added at link time, or a conservative per-crate/library manifest. **Native v1 ships external bundles**; baked native data is a later milestone.

## Libraries and Plugins

A library cannot know which of its APIs a final application uses, so a library build must not be treated as a closed world:

```text
library
├─ MessageReferenceArtifact
└─ optional catalog definitions

final application
├─ application references
├─ linked libraries' references
└─ supported locales
        ↓
final Message Link
```

Library packages distribute reference artifacts (and optionally catalog definition artifacts); the final application's link composes them and decides reachability against the final locale set and delivery graph. Dynamically loaded plugins likewise supply their own artifacts; a plugin that can request arbitrary messages without an artifact forces its scope open-world — retained in full and reported as `degraded-analysis`.

## World Model

- **Production application link: closed world.** Supported locales are a finite declared set; static references come from producers; dynamic references are bounded selectors; unbounded dynamics are rejected or conservatively retained per policy; the final delivery graph fixes the reachability roots.
- **Development: open world.** Catalog additions, HMR, and not-yet-resolved references are tolerated; findings from partial information are advisory.
- **Library builds: open or partial world**, closed only at the final application link.

Closed world does not mean eager loading: it means the finite message × locale universe is known at build time, while runtime lazily loads exactly the delivery units and locales it needs.

### Adoption gradient

Existing applications are full of `t(dynamicKey)`. The entry point is compat mode: dynamic scopes are retained whole with warnings, while static references immediately power `unresolved-message` and `unused-message` (outside degraded scopes), per-locale splitting, and typed keys — value on day one with zero API changes. `useMessageSet` / `message_set!` are the opt-in tightening tools, applied scope by scope where teams want strict mode and maximal slicing. Nothing requires a flag-day migration.

## Platform Exporters

The linker's output is a plan, not a runtime format:

```rust
pub struct MessageBundlePlan {
    pub delivery_unit: DeliveryUnitId,
    pub locale: Locale,
    pub messages: Vec<ResolvedMessage>,
}
```

| Exporter    | Example output                          |
| ----------- | --------------------------------------- |
| JavaScript  | ESM virtual module, JavaScript asset    |
| Web runtime | binary message pack                     |
| Rust        | baked Rust module, external blob        |
| C/C++       | generated source, object, external blob |
| WASM        | sidecar asset, custom section           |

Payloads are pre-processed at export (message-compiler-style generated code; Binary AST snapshot (003) where codegen does not fit; raw-text escape hatch for debugging); representation defaults are benchmark-driven follow-up. Exporters also emit the locale → asset map with the fallback chains embedded, and runtimes must resolve with the same chains (for vue-i18n, aligning `fallbackLocale` with this config is the integration point). The ESM exporter additionally takes `eagerLocales`: the locales whose assets the entry delivery unit imports eagerly; every other supported locale loads lazily through the map — the default that answers problem 5. The loader contract stays minimal: the generated map plus a `loadLocale(unit, locale)` hook; loading policy belongs to runtimes. M3 currently selects ESM as the first exporter milestone; that product ordering does not make ESM part of the linker core contract.

## Component Boundaries

```text
intlify_link_contract
  └─ artifact types, wire format, conformance test suite

intlify_resource
  └─ catalog definitions, keys, messages, source spans (unchanged; 013)

language reference producers
  ├─ intlify_producer_js   (oxc-based JS/TS + Vue SFC frontend)
  └─ intlify_producer_bin  (tagged-ID scanner for native/WASM; later)

intlify_message_linker
  ├─ reference resolution, locale/fallback resolution
  ├─ reachability over the delivery graph, placement
  └─ bundle plans + findings

intlify_lint
  └─ presents linker findings as rules (008 + catalog-level addendum)

platform integrations
  ├─ Vite/Rolldown, Cargo, CMake, …
  └─ exporters emitting platform artifacts

LSP/editor integration (009)
  └─ incremental findings from available source-level reference info
```

013's deferred catalog-level and cross-locale checks, locale binding, unused-translation reporting, and application-source reference analysis all layer above the one shared catalog extraction path; no second resource extractor is introduced. `crates/intlify_resource` is unchanged.

## Configuration (sketch)

Additive sections under the 006 unified config contract; the sketch fixes intent only. It mirrors the linker's three inputs — link policy, reference producers, delivery targets — rather than the pre-linker "one CLI scanner" shape:

```jsonc
{
  "resources": {
    "catalogs": [
      /* 013, with locale binding + group */
    ]
  },
  "lint": {
    "rules": {
      "unresolved-message": "error",
      "unused-message": "warn" /* linker findings opt in as rules */
    }
  },
  "messages": {
    // link policy: the closed world
    "locales": ["en", "ja", "ja-JP"],
    "fallback": { "ja-JP": ["ja", "en"], "ja": ["en"] },
    "coverageBaseline": { "app": "en" },
    "dynamicReferences": "strict", // strict | compat
    "roots": [{ "scope": "app", "selector": "legal.**", "reason": "rendered server-side" }],
    // reference producers: who supplies MessageReferenceArtifacts
    "producers": {
      "js": {
        "include": ["src/**/*.{ts,tsx,vue}"],
        "recognizers": {
          /* defaults provided */
        }
      },
      "artifacts": [
        "services/mailer/intlify-references.json" /* externally produced, composed at link */
      ]
    },
    // delivery: exporters consuming bundle plans
    "delivery": {
      "targets": [
        {
          "name": "web",
          "exporter": "esm",
          "placement": "hoist",
          "eagerLocales": ["en"],
          "out": "src/generated/messages"
        },
        { "name": "native", "exporter": "blob", "out": "build/messages" }
      ]
    }
  }
}
```

`producers.js` configures the built-in CLI source-scan producer; `producers.artifacts` composes externally produced artifacts at link time. Bundler integrations and native binary scanners supply their artifacts through their build integrations (plugin options, build-script invocation, scan inputs per `emit` invocation), not through this file. `roots` are config-declared reachability roots: additional references in artifact vocabulary (scope + selector + reason).

## CLI Surface (proposal)

Findings ship inside `intlify lint`; the linker itself has no separate check command. Remaining commands are mutation and generation, reserved in the 006 style:

- `intlify messages prune [--write]` — deletion plan for `unused-message` findings via validated write-back.
- `intlify messages emit [--target <name>] [--check]` — run the link and exporters for named targets; `--check` re-runs and diffs as the CI freshness job. A plan-inspection output (dump of `MessageBundlePlan`s without exporting) is an open question.

Global options, reporters, operational error shaping, and exit codes inherit the Phase 3A contracts. `messages` vs `catalog` naming remains open.

## Milestones

Main track:

- **M0 — contract**: define the first versioned `MessageReferenceArtifact` contract (selector semantics, provenance, version negotiation) with its conformance suite; whether this milestone freezes v1 or publishes a pre-v1 contract is resolved by the artifact-freeze open question. Deliver a locale-blind linker core (resolution against scoped key unions; `unresolved-message`, `unused-message`, `unbounded-dynamic-reference`, `degraded-analysis` through lint) and a JS/TS source-scan producer without bundler integration. Depends on 013 Tier 1 extraction and the linter addendum's project-scope machinery.
- **M1 — typed keys**: key-union and argument-type generation from the coverage baseline.
- **M2 — locale-aware link**: chains, `missing-translation` vs `unresolved-message`, `orphaned-translation`. Gated on 013 locale binding.
- **M3 — exporter v1**: per-locale ESM assets + loader map + `--check`, single delivery unit.
- **M4 — bundler integration**: live chunk graph as delivery units, placement policy in effect, virtual modules, dev-mode findings.
- **M5 — prune** write-back.

Native track (parallel after M0):

- **N0**: Rust `message!` / `message_set!` + tagged reference-ID contract + final-binary scanner (single-unit artifact) + external per-locale bundles; debug sidecar for origins.
- **N1**: C/C++ macros and WASM after object/linker format-survival validation.
- **Deferred**: baked native data (two-phase build machinery), per-object unit granularity, binary container, stub scaffolding, identical-to-fallback omission.

## Validation

- Artifact conformance suite: golden artifacts per producer, version-negotiation cases, third-party-producer fixtures.
- Linker semantic goldens: resolution, chain fallback, reachability, placement — deterministic across runs and platforms.
- The constructive invariant, executed: every emitted target re-links clean — zero `unresolved-message`, zero `unused-message` when pruned.
- Degradation tests: wide selectors and artifact-less plugins produce `degraded-analysis` exactly where expected.
- Native format-survival fixtures: strip/LTO/COMDAT matrices proving tagged IDs survive (or over-retain, never under-report).
- Prune safety under 013 write-back invariants; byte-determinism with `--check`.
- Benchmarks measure the problems: initial-bundle bytes, per-locale and per-unit asset bytes before/after, scan and link wall time — `tools/messages-bench`.

## Relationship to Other Documents

| Document | Relationship |
| --- | --- |
| [013-ox-mf2-resource-catalog-adapter-design.md](./013-ox-mf2-resource-catalog-adapter-design.md) | definition side: extraction artifacts, entry identity/spans, locale binding, comparison scopes, validated write-back for `prune`, and the third-party identity precedent reused for scope identity |
| [006-ox-mf2-phase-3a-tooling-foundation-design.md](./006-ox-mf2-phase-3a-tooling-foundation-design.md) | config envelope, schema pipeline, reporters, operational errors, exit codes |
| [008-ox-mf2-phase-3c-linter-design.md](./008-ox-mf2-phase-3c-linter-design.md) | presentation surface: linker findings ship as rules through its contracts and the catalog-level addendum |
| [003-ox-mf2-phase-2-binary-ast-snapshot-design.md](./003-ox-mf2-phase-2-binary-ast-snapshot-design.md) | candidate payload representation; format-version precedent |
| [009-ox-mf2-phase-3d-lsp-editor-design.md](./009-ox-mf2-phase-3d-lsp-editor-design.md) | editor surfacing; incremental open-world findings |
| `refers/formatjs` | code-first reference-producer comparison recorded in Message-Domain Prior Art |

## Open Questions

1. **Placement and delivery-graph semantics**: `hoist` vs `duplicate` as the default, whether placement becomes per-scope configurable, the direction and validation rules of graph edges, and how a loading-order-safe shared placement is selected deterministically in a rooted DAG where a tree-style lowest common ancestor may not be unique — wants fixture evidence from real app graphs (M4).
2. **Artifact freeze timing**: freeze v1 at M0 (with reserved fields) versus after the native track validates binary-scan constraints; version negotiation mitigates either way.
3. **Plan inspection**: whether `emit` grows a plan-dump mode or plans stay internal until a concrete consumer appears.
4. **Naming**: `messages` vs `catalog` for the config section and command namespace.
5. **Typed-keys shape**: d.ts augmentation versus generated accessor modules, and how far `useMessageSet` converges with the accessor direction.
6. **Coverage baseline and typed-key completeness**: per-scope `coverageBaseline` naming and defaulting (largest catalog? explicit only?), and whether type generation rejects an `orphaned-translation`, generates from the union instead, or otherwise prevents a runtime-resolvable key from being absent from generated types.
7. **Stub scaffolding**: whether placeholder insertion for `unresolved-message` / `missing-translation` is wanted, under which translation-workflow conventions.
8. **Lint scan gating**: whether `intlify lint` produces or loads reference artifacts only when at least one linker-backed usage-aware rule is enabled, so projects that do not enable those rules pay no reference-analysis cost. `messages emit` and `messages prune` require linker analysis independently of lint-rule enablement; the remaining question is how lint-time production, caching, and reuse are gated without creating a second analysis path.
9. **Rule rollout**: whether linker-backed usage-aware rules are default-off initially, which presets eventually enable them, and how rollout interacts with project-scope operand handling, warning counts, and the catalog-level linter addendum.
10. **Reference-key canonicalization**: how source spellings such as `t('checkout.title')` become the domain-qualified canonical `CatalogKey` used by 013, including the distinction between a literal key containing `.` and a nested JSON Pointer path. Producers must not guess when the configured runtime-key syntax is ambiguous.
11. **Native reference-ID recovery**: whether native producers embed complete selector records or join scanned surviving IDs against a producer dictionary, including the hash/version scheme, collision rejection, unknown-ID handling, and the distinction between a library's available-reference dictionary and the final binary's surviving roots.
12. **Artifact completeness and prune eligibility**: how a final link proves that every configured producer participated, how artifacts declare closed, partial, or degraded analysis, and whether `messages prune` must refuse mutation unless the relevant scopes are closed and complete.
13. **Definition artifact envelope**: the version, producer/source identity, completeness marker, input fingerprint, resource limits, deterministic wire representation, and compatibility negotiation around the `MessageDefinition` records projected from 013 extraction.
14. **Selector grammar**: the exact byte-level grammar and matching semantics of `Prefix` and `Pattern` for each catalog-key domain, including escaping, separator behavior, normalization policy, and conformance fixtures.
15. **Distributed scope identity**: the canonical package identity across npm, Cargo, C/C++, and other ecosystems; compatible-version equality; host-issued namespacing; and the trust model behind accepting published scope identities.
