# ox-mf2 Message Linker Design

## Purpose

This document defines the overview-level architecture for an Intlify message linker. The linker connects application message references with catalog definitions, resolves locale fallback, computes message reachability, reports consistency findings, and produces platform-neutral bundle plans.

The design makes data requirements, finite production locales, generated artifacts, and pre-runtime validation explicit. It also covers the application-specific concepts that message catalogs require: language-specific reference production, dynamic-key bounds, authored-catalog maintenance, delivery-unit reachability, and final-application composition.

This document owns the language-neutral linker boundary and its public artifacts.

It builds on four related designs:

- resource extraction and validated write-back in [013-ox-mf2-resource-catalog-adapter-design.md](./013-ox-mf2-resource-catalog-adapter-design.md);
- the CLI and configuration foundation in [006-ox-mf2-phase-3a-tooling-foundation-design.md](./006-ox-mf2-phase-3a-tooling-foundation-design.md);
- the later linter presentation surface in [008-ox-mf2-phase-3c-linter-design.md](./008-ox-mf2-phase-3c-linter-design.md); and
- the editor integration boundary in [009-ox-mf2-phase-3d-lsp-editor-design.md](./009-ox-mf2-phase-3d-lsp-editor-design.md).

## Goals

- Define a programming-language-neutral contract for message references and catalog definitions.
- Resolve exact and bounded dynamic references without silently dropping a possibly used message.
- Detect ambiguous definitions, unresolved references, translation coverage gaps, orphaned translations, and unused definitions before production runtime.
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

The future `intlify_contract` boundary owns public artifact types, wire compatibility, version negotiation, selector wire grammar, the normative selector semantics for each catalog-key domain, and producer/linker conformance fixtures.

Language-specific producers own source, object, or binary analysis and emit that contract.

The future `intlify_linker` core consumes only reference artifacts, definition artifacts, checked scope mapping and completeness inputs, a resolved link policy, and the delivery graph; it owns execution of the contract-defined selector matching, resolution, reachability, placement, findings, bundle plans, and linker-result identity composites such as `DefinitionLocation`.

`intlify_resource` remains the sole owner of catalog assignment, host parsing, message entry extraction, catalog-key production and domain issuance, source spans, and validated write-back.

It emits canonical domain-qualified keys but has no selector-matching API and never expands selectors. `intlify_linker` returns linker-owned findings without depending on `intlify_lint`; a later lint integration presents those findings through its rule and reporting contracts.

The workspace-internal `intlify_export` crate owns shared MF2 export preparation, mapped export diagnostics, `ValidatedExportBatch`, the object-safe exporter trait, common exporter output/error contracts, and built-in exporters. It depends on `intlify_linker`, `intlify_contract`, and `ox_mf2_parser`; `intlify_linker` never gains a parser or exporter dependency.

Platform integrations own build-graph adaptation, checked exporter construction and selection, invocation orchestration, destination mapping, and final registration. The initial `intlify_cli` integration owns the built-in target registry and typed factory wiring but not the shared preparation, exporter, or output contracts. Runtimes own loading policy after consuming generated assets and loader maps.

This overview fixes those responsibility boundaries and the intended product direction.

Detailed wire schemas, rule contracts, configuration validation, producer encodings, delivery-graph algorithms, and exporter formats live in their owning sections below or in explicitly named milestone implementation addenda rather than being inferred by individual implementations.

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

Solving these problems requires the project to state data requirements before generation, constrain production locales to a finite set, generate only what is reachable, and detect required-but-unavailable messages before production runtime.

Message resources also need application-reference analysis, bounded dynamic-key handling, locale fallback, and final program or module composition. Intlify therefore gets a dedicated **message linker**.

## Design Overview: A Linker for Messages

The linker core recognizes no particular programming language or build system. JS/TS, Rust, C/C++, and others are **reference producers** that feed one common contract; catalogs feed the other side through the existing `intlify_resource` extraction path.

The linker first applies one checked host scope-mapping table, then resolves references against definitions under a resolved link policy — locale/fallback, dynamic-reference, coverage, and placement policy — and a delivery-unit graph, and produces bundle plans and findings. One analysis, every surface:

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

![ox-mf2 message linker architecture including platform build integrations](./assets/014-ox-mf2-message-linker-architecture.svg)

The linker never parses JS/TS source, Rust crates, or C/C++ objects directly. Each language producer emits a common-format `MessageReferenceArtifact`; on the catalog side, a host-owned definition projection outside `intlify_resource` combines each complete extraction with its resolved catalog assignment and constructs one common-format `MessageDefinitionArtifact`.

The linker consumes only the two artifacts, a checked `ScopeMappingTable`, a checked `ScopeCompletenessTable`, the resolved link policy, and the delivery graph.

A platform build integration orchestrates the surrounding workflow: it runs the relevant producer or scanner, resolves policy, supplies any checked host-owned scope mapping, derives scope completeness from configured inputs and their execution results, supplies the delivery graph, invokes the linker and selected exporter, and registers emitted artifacts with the build system. The M0-owned built-in CLI/editor orchestration contract always supplies the checked empty mapping table when a later M3/M5/L0 surface invokes it; M0 alone exposes no such user-facing surface.

### Architecture Components

The diagram separates language- and host-specific input production from language-neutral linking and platform-specific export. Each box has one responsibility:

#### Reference producer path

Runs outside the linker core and recognizes message usage in one source or build artifact format. A JS/TS producer may inspect an AST and bundler graph, while native producers may use macros plus object or final-binary scanning.

The producer owns language-specific recognition, provenance, and delivery-unit attribution; it does not resolve references against catalogs.

#### Message Reference Artifact

Carries the producer's versioned, language-neutral output: one portable artifact identity plus an ordered array of scope- and domain-qualified exact references, bounded selectors, or explicit `UnboundedDynamic` evidence, together with their owning delivery unit and optional source origins.

The artifact identity plus each array ordinal gives every reference record a deterministic portable identity. It is the only application-reference input accepted by the linker core.

#### Catalog definition path

Uses the configured `intlify_resource` registry and host-format adapters to extract messages from catalog files. The resource boundary owns host parsing, canonical catalog keys, message text, source spans, and validated write-back; it does not import the linker contract, inspect application code, aggregate files, or decide reachability.

#### Host-owned definition projection

Consumes one successful `intlify_resource` extraction plus its resolved project catalog assignment. It validates the physical-alias group's equal namespace, host-format, scope, and locale binding; selects the canonical primary source identity and ordered aliases; and constructs the `MessageDefinitionArtifact` through `intlify_contract`.

The initial implementation is a shared module in the `intlify_cli` project-inventory layer rather than a new crate. It owns no host parser, catalog-key semantics, selector matching, reachability, or linker analysis. A later non-CLI host must reuse this exact boundary or extract it into a shared crate before implementing another projection path; it must not duplicate the rules independently or move them into `intlify_resource`.

#### Message Definition Artifact

Projects exactly one completely extracted, selected catalog source document into ordered language-neutral definitions containing a portable namespaced source identity, scope, key domain, canonical key, locale, message payload, and source-entry evidence.

It is the only catalog-definition input accepted by the linker core; several source artifacts compose through `LinkRequest`.

#### Scope Mapping Table

Carries the final application's checked, exact, one-hop structural-scope mappings. It permits explicit many-to-one equivalence, leaves unmapped scopes unchanged, and forbids artifact rewriting, chains, cycles, namespace inference, and name-only fallback.

The host constructs it through the checked typed API and the linker applies it uniformly before semantic indexing. The M0-owned built-in CLI/editor configuration contract exposes no mapping field and supplies the empty table when activated by a later surface; non-empty M0 tables are available only to custom in-process build integrations.

#### Scope Completeness Table

Records, for each application-owned scope targeted by this link, whether its configured definition sources and reference producers completed as a closed input set or remained partial for one typed reason. The integration derives this table from resolved configuration plus execution results; an artifact cannot claim that its project scope is complete.

#### Resolved link policy

At M0, declares the finite production locale set, dynamic-reference mode, configured roots, and `duplicate` placement policy. M1 adds coverage baselines, and M2 adds explicit fallback chains together with locale-aware resolution. The linker uses only the policy fields admitted by the active milestone; runtime locale negotiation and asset-loading policy remain outside the linker.

#### Delivery Unit Graph

Describes project-contextual delivery nodes identified by the shared non-empty logical segment-array `DeliveryUnitId` and their dependency or loading-order relationships.

A bundler or build integration supplies the graph and binds every reference artifact to one existing node; the linker uses it to calculate per-unit reachability and shared-message placement without interpreting platform-specific labels or unit types.

#### Language-neutral `intlify_linker`

Consumes only the two artifacts, scope mapping table, scope completeness table, resolved link policy, and delivery graph as one immutable `LinkRequest`.

It performs deterministic scope resolution, completeness gating, reference resolution, reachability, placement, and finding generation without parsing source languages, binaries, catalog host formats, or build-system configuration directly, and returns one `LinkOutcome`.

#### Reference resolution

Matches exact and bounded selectors to domain-qualified canonical catalog keys within their scopes and applies strict/compat policy to `UnboundedDynamic`. A reference that cannot resolve under the relevant locale chains becomes an `unresolved-message` finding.

#### Locale / fallback resolution

Resolves each reachable key for every requested locale through its explicit fallback chain. It distinguishes a resolvable coverage gap (`missing-translation`) from a reference with no definition anywhere in the chain (`unresolved-message`).

#### Reachability and placement

Computes which definitions are reachable from static references, bounded selectors, configured roots, and compat-mode conservative widening, then assigns shared messages to delivery units under the selected placement policy. Definitions outside the closed and complete root set are candidates for `unused-message`.

#### Finding generation

Produces typed linker findings and related evidence for ambiguous, unresolved, missing, orphaned, unused, unbounded, or degraded cases. It does not define a second reporting surface or decide lint severity and preset membership.

#### Linker findings

Feed `intlify lint` and future editor integration through the diagnostic, rule, and catalog-finding contracts owned by 008 and 013. Consumers present the findings but do not rerun link analysis.

#### `MessageBundlePlan`

Records the deterministic resolved-message selection and placement for one requested locale and delivery unit.

Each selected `ResolvedMessage` is a fully owned immutable snapshot of the exact resolved scope, domain-qualified key, selected definition locale, decoded message payload, and definition location. It is an input to shared export preparation, not a directly callable exporter argument, runtime serialization format, or loading policy.

#### Platform build integrations

Orchestrate the platform build around the neutral contracts.

A Vite/Rolldown plugin, Cargo task or `build.rs`, CMake integration, or another toolchain adapter runs producers or scanners, supplies the checked host-owned scope mapping table and execution-derived completeness table, supplies the delivery graph, invokes the linker, and asks `intlify_export` to create one opaque `ValidatedExportBatch` before invoking exactly one selected exporter for the export transaction. Built-in M0 integrations use the empty mapping table; a custom in-process integration may construct a non-empty table through the same checked API.

It maps each generic returned artifact's portable relative logical path, kind, payload, and metadata into platform-specific registration and then registers the one returned `ExportArtifactSet` in the final build. A parser or semantic diagnostic produces no batch, invokes no exporter, and publishes no partial asset or manifest.

The integration does not reimplement linking or message parsing semantics. Integrations and exporters are selectable rather than fixed one-to-one pairs: integrations may share one exporter, and one integration may reuse the same borrowed batch in separate independent transactions with different exporters.

One exporter may return multiple artifacts and artifact kinds in its one set.

#### Platform exporters

Convert the message-validated plans and their selected definition snapshots exposed read-only through `ValidatedExportBatch` into an ordered list of generic `ExportArtifact` records representing ESM, binary blobs, baked Rust, generated C/C++, or outputs for other languages and platforms.

Each record uses an open namespaced semantic `ExportArtifactKind` plus an explicit kind-specific `ExportArtifactFormatVersion`; loader maps and manifests are ordinary artifacts with distinct kinds in the same list rather than side-channel result fields.

The set is extensible through the shared object-safe `PlatformExporter` trait, whose only plan-bearing argument is the opaque batch and whose result uses the concrete `ExportArtifactSet` / `ExportError` contracts.

Exporters choose representation and emit deterministic metadata, but they neither accept a raw `MessageBundlePlan`, return a closed platform-specific enum or opaque `Any` payload, repeat reference resolution, fallback resolution, reachability, or placement, nor substitute target-specific MF2 parser or semantic validation for the shared gate.

These shared types, the preparation implementation, and the built-in ESM exporter live in `crates/intlify_export`. An external Rust build integration can depend on that workspace crate without importing `intlify_cli`; crates.io publication is not implied.

## Data Selection and Delivery Constraints

- State data requirements explicitly before generation.
- Constrain production locales to a finite, declared set.
- Generate data only after requirements are fixed.
- Detect required-but-unavailable data before production runtime.
- Separate source data from generated artifacts.
- Make runtime data requirements extractable from a binary.

The linker selection input is not the message key alone. It is the combination of exact or bounded references and explicit unbounded-dynamic evidence produced per language, the resolved link policy, and the delivery graph.

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
pub struct ArtifactVersion {
    major: u16,
    minor: u16,
}

pub enum ArtifactNamespace {
    Project,
}

pub struct CatalogScopeId {
    namespace: ArtifactNamespace,
    name: CatalogScopeName,
}

pub struct CatalogScopeName(String);

pub struct Locale(String);

pub struct MessageReferenceArtifact {
    version: ArtifactVersion,
    producer: ProducerIdentity,
    identity: ReferenceArtifactIdentity,
    delivery_unit: DeliveryUnitId,
    references: Vec<MessageReference>,
}

pub struct ReferenceArtifactIdentity {
    namespace: ArtifactNamespace,
    segments: ReferenceArtifactSegments,
}

pub struct ReferenceArtifactSegments(Vec<ReferenceArtifactSegment>);

pub struct ReferenceArtifactSegment(String);

pub struct DeliveryUnitId(Vec<DeliveryUnitSegment>);

pub struct DeliveryUnitSegment(String);

pub struct ReferenceRecordIdentity {
    artifact: ReferenceArtifactIdentity,
    ordinal: u32,
}

pub struct MessageReference {
    scope: CatalogScopeId,
    domain: CatalogKeyDomain,
    selector: MessageSelector,
    reason: Option<ReasonText>,
    origin: Option<SourceOrigin>,
}

pub struct ReasonText(String);

pub struct SourceOrigin {
    source: SourceDocumentIdentity,
    span: SourceUtf8Span,
}

pub struct SourceUtf8Span {
    start: u32,
    end: u32,
}

pub enum MessageSelector {
    Exact(CatalogKey),
    Prefix(CatalogKeyPrefix),
    Pattern(CatalogKeyPattern),
    AllInScope,
    UnboundedDynamic,
}

impl MessageReferenceArtifact {
    pub fn try_new(
        version: ArtifactVersion,
        producer: ProducerIdentity,
        identity: ReferenceArtifactIdentity,
        delivery_unit: DeliveryUnitId,
        references: Vec<MessageReference>,
        limits: &LinkLimits,
    ) -> Result<Self, ArtifactContractError>;

    pub fn version(&self) -> &ArtifactVersion;
    pub fn producer(&self) -> &ProducerIdentity;
    pub fn identity(&self) -> &ReferenceArtifactIdentity;
    pub fn delivery_unit(&self) -> &DeliveryUnitId;
    pub fn references(&self) -> &[MessageReference];
}
```

Every reference/definition artifact wire value, nested record, identity, and scalar newtype in this artifact input contract has private state and read-only accessors.

Each scalar/identity constructor validates its complete value before exposure; each record constructor accepts its complete field tuple once and is infallible only when the checked field types leave no cross-field invariant, otherwise returning a closed type-local construction error.

No public struct literal, mutable field, setter, unchecked `From`, or partially initialized builder can create one of these values.

The artifact-level `try_new` is the one direct-construction admission boundary: it preserves the submitted record order, applies the same canonical structural, cross-record, decoded-accounting, and current-lower-limit phases as decoding, and exposes no partial artifact.

Decoder and producer paths feed the same internal validator rather than reconstructing the rules independently. The private sequence storage is owned by the artifact and exposed only as a slice; whether the implementation compacts a consumed `Vec` into a boxed slice is not public identity or ABI.

A cache that needs shared ownership wraps the complete immutable artifact externally in `Arc`; the artifact contract itself contains no `Arc`, borrow from producer scratch storage, or mutable copy-on-write state.

Link-request, finding, plan, and export-result construction remain governed by their owning sections and are not implicitly changed by this artifact-input rule.

- `ArtifactVersion` is shared by reference and definition artifacts and contains only unsigned `major` and `minor` components.
  - A major increment denotes a breaking contract change; a minor increment denotes compatible additive evolution under the negotiation rules fixed below.
  - Patch and build identity do not belong here and remain producer-revision concerns.
  - The canonical fingerprint payload is exactly `major:u16be || minor:u16be`, with no text, separator, sign, variable-width integer, or patch component.
- While the repository remains WIP, the initial writer emits `{ major: 0, minor: 1 }`.
  - As with the existing v0.1 binary contract precedent, coordinated writer, reader, schema, fixture, and document changes may revise this draft without promising compatibility across producer revisions.
  - M0 intentionally remains at this mutable v0.1 point and does not freeze v1.
  - Declaring the first stable artifact compatibility point ends that exception; it is reconsidered only after N0 validates the native binary-scan and reference-ID constraints, and subsequent breaking and additive changes then follow the major/minor meanings above.
  - Reader/writer negotiation uses the exact-draft and stable-range rules fixed in [Artifact version negotiation](#artifact-version-negotiation).
- `version` identifies wire-contract compatibility; `producer` identifies the emitting frontend or scanner and its provenance revision.
- `identity` is exactly one `ArtifactNamespace` plus one non-empty logical segment array. It names this reference artifact independently of its content, delivery unit, producer identity or revision, input position, and host filesystem spelling; together with a record ordinal it forms the reference-record identity used by findings.
- `delivery_unit` identifies the output unit that owns the references.
- `domain` identifies the 013 catalog-key comparison domain. A reference matches definitions only within the same `scope` and `domain`; `AllInScope` means every key in that exact scope-domain pair, not every domain carrying the same scope name. `UnboundedDynamic` is likewise contained by the already resolved scope-domain pair, but records that the producer could not derive a narrower key set.
- `selector` expresses a static key, an intentionally bounded dynamic set, or the explicit fact that a recognized dynamic lookup could not be bounded. **Non-`Exact` selectors should carry `reason`** (from the declaring API, config, or producer analysis) so reviewers can see why a widening exists; producers that cannot supply one emit it absent, never fabricated.
- `origin` is diagnostic source location; linker correctness never depends on it. Producers that have locations should supply them (directly or via a debug sidecar artifact) so findings can point at reference sites.

#### Reference artifact identity

`ArtifactNamespace` is shared by reference-artifact identity, portable source-document identity, and catalog-scope identity.

M0 admits only `Project`, contextual to the one consuming application represented by a `LinkRequest`; the host must bind it to the current project root before admitting a locally produced or cached value.

The explicit enum and wire tag reserve a versioned extension point without making published-package resources part of M0. A published artifact cannot make a portable `Project` claim, and a namespace kind other than exact `project` is unsupported under v0.1 rather than inferred from package-manager metadata.

Package-owned resource artifacts are deferred below.

`ReferenceArtifactSegments` is a non-empty ordered array of logical labels, not a host or output path. Each `ReferenceArtifactSegment` uses the same scalar grammar as one 013-derived `PortablePathSegment`: a non-empty Unicode scalar sequence other than exact `.` or `..`, containing neither `U+0000` nor `/`.

The bytes and segment boundaries are retained exactly without Unicode normalization, case folding, trimming, percent decoding, or separator rewriting. Backslash is an ordinary character.

The types remain distinct because a reference artifact may represent an aggregate module, generated unit, object set, or final binary rather than one filesystem object; no implicit conversion from a path is part of the contract.

Reference-artifact logical identity uses three independent inclusive protocol ceilings:

| `LinkLimits` counter | Accounting rule | Initial protocol ceiling |
| --- | --- | --: |
| `reference_identity_segment_bytes` | Exact decoded UTF-8 bytes in one `ReferenceArtifactSegment`. | 255 bytes |
| `reference_identity_segments` | Number of elements in one non-empty `ReferenceArtifactSegments` array. | `64` |
| `reference_identity_bytes` | Sum of decoded UTF-8 byte lengths of all segments in one identity. | 4 KiB (`4,096` bytes) |

The segment-count limit is checked before retaining the array; known structured lengths are preflighted, and streaming adapters stop admitting elements at the first value above the limit. After that preflight, segments are processed in semantic array order.

Each decoded segment is checked against its individual byte ceiling before retention and contributes through checked `u64` addition to the per-identity total before the identity is exposed. Empty arrays remain structurally invalid rather than a zero-sized identity.

Exactly 255 bytes in one otherwise valid segment, exactly 64 segments, and exactly 4,096 total segment bytes are accepted when all other limits pass; the first byte or element above any applicable ceiling rejects the complete reference artifact.

`reference_identity_bytes` charges every segment occurrence independently. Equal segment values, shared prefixes, interning, zero-copy storage, and a slash-joined display form provide no deduction.

It excludes array/object framing, display separators, allocation capacity, and the fixed `Project` namespace discriminant; actual serialized bytes remain subject to `reference_artifact_wire_bytes`, while every segment occurrence also contributes to `reference_artifact_decoded_bytes`.

Producers, decoders, direct checked constructors, and defensive link admission enforce identical accounting without truncating, merging, hashing, shortening, or replacing an identity.

The fixed ceilings follow the common `LinkLimits` model: callers may select lower immutable values, including zero, but cannot raise a protocol ceiling; the lower values do not change identity equality, ordering, or serialization for an admitted artifact.

The containing `LinkRequest` adds two independent inclusive aggregate ceilings:

| `LinkLimits` counter | Accounting rule | Initial protocol ceiling |
| --- | --- | --: |
| `reference_artifacts` | Number of submitted `MessageReferenceArtifact` occurrences in one request. | `65,536` |
| `reference_identity_bytes_total` | Sum of `reference_identity_bytes` across every decoded reference-artifact identity submitted to one request. | 64 MiB (`67,108,864` bytes) |

The linker checks `reference_artifacts` from the input slice length before per-artifact validation, sorting, duplicate detection, index allocation, or semantic analysis.

A structured integration with a known collection length performs the same preflight before allocating artifact storage; a streaming integration stops at the 65,537th submitted occurrence. Empty input and exactly 65,536 artifacts are accepted when every other constraint passes.

Every submitted occurrence counts, including a duplicate or an artifact later rejected for another reason; sorting, filtering, deduplication, cache reuse, and worker partitioning never reduce the count.

After every submitted artifact has passed its complete structural and effective per-artifact limits, each exact `reference_identity_bytes` value contributes once to the first request aggregate pass, `reference_identity_bytes_total`, with checked `u64` addition.

A duplicate identity or an artifact rejected later by a cross-artifact rule still receives no deduction. The fixed `Project` namespace discriminant remains excluded from this specific counter, exactly as above.

Zero and exactly 64 MiB are accepted; conversion or addition overflow and the first byte above the ceiling reject the complete request. The artifact-count preflight and every per-artifact failure precede this aggregate check; `reference_records_total` and `reference_artifact_decoded_bytes_total` follow it.

The shared canonical identity-group reduction fixed below makes input order, parallel decoding, interning, equal identities, and shared prefixes unable to change the accepted total or selected error.

Both aggregate counters use the same immutable lower-budget rule as the per-identity counters. A zero artifact limit admits only an empty reference-artifact collection, and a zero total-byte limit likewise admits no non-empty logical identity.

An over-limit request returns one `LinkOperationalError` with no findings or plans; the caller must submit one new complete admissible request and may not make the linker silently drop artifacts, split global analysis across requests, truncate identities, or retry with relaxed limits.

Exact aggregate definition, policy, graph, and output counters remain separate decisions.

The initial JSON identity value is exactly an object with `namespace` and `segments`, for example `"identity":{"namespace":{"kind":"project"},"segments":["js","module","src","checkout.ts"]}`. The namespace uses the same exact tagged-object codec as definition sources: `{"kind":"project"}`.

Input member order is non-semantic; canonical emission orders `namespace` before `segments`. The segment array is non-empty and order-sensitive.

A slash-joined rendering is presentation-only and cannot be parsed back into identity. Missing, duplicate, unknown, mistyped, or invalid members, including a `"package"` kind or extra `package` member, reject the complete reference artifact rather than being normalized.

The build integration owns namespace binding and assigns the portable logical segment array to the producer.

For a source-level artifact it may map an already root-bound logical module name, such as `["js", "module", "src", "checkout.ts"]`; for an aggregate or native artifact it may assign a configured build identity such as `["native", "binary", "app"]`.

Neither the integration nor producer derives identity from an absolute path, current directory, temporary/output path, content-addressed filename, content hash, `ProducerId`, producer revision, delivery-unit ID, artifact input order, or random value.

Changing one of those facts alone therefore preserves identity; changing the assigned namespace or any exact segment changes it. Producer families operating in the same namespace must coordinate their assigned logical IDs, and a duplicate is rejected rather than namespaced implicitly by provenance.

Identity equality compares the exact segment sequence under the single M0 `Project` namespace. Canonical ordering compares segments lexicographically by exact decoded UTF-8 bytes with a shorter equal-prefix sequence first.

Host path rules, locale collation, normalized display strings, producer data, and discovery order never participate. This order is reused wherever `ReferenceArtifactIdentity` contributes to a finding subject, cache key, structured output, or deterministic sort.

A project-scoped persistent cache additionally carries the host's project binding outside the artifact so equal contextual identities from unrelated projects are never confused. Any future namespace variant must extend this ordering explicitly through the artifact-version contract.

#### Delivery unit identity

`DeliveryUnitId` is a project-contextual, non-empty ordered array of logical segments. It names one node in the exact `DeliveryUnitGraph` supplied by the same `LinkRequest`; it is not a globally published package identity, platform enum, host/output path, filename, URL, MIME value, or artifact identity.

The linker assigns no meaning to labels such as `js`, `chunk`, `native`, or `binary`.

Multiple reference artifacts may name the same delivery unit, one graph node may have no reference artifact, and changing an artifact's delivery unit changes reachability and placement inputs without changing its `ReferenceArtifactIdentity` or existing record ordinals.

`DeliveryUnitSegment` uses the same scalar grammar as `ReferenceArtifactSegment`: a non-empty Unicode scalar sequence other than exact `.` or `..`, containing neither `U+0000` nor `/`, retained exactly without normalization, case folding, trimming, percent decoding, or separator rewriting. Backslash is ordinary.

The types are distinct and have no implicit conversion because an artifact logical identity and a graph-node identity have different ownership and lifecycle even when their segment bytes happen to match.

The required JSON value is the segment array itself, for example `"deliveryUnit":["js","chunk","checkout"]`. It is never a slash-joined string, namespace object, platform-tagged union, integer index, or `null`.

Array order and boundaries are semantic and preserved exactly; an empty array or invalid segment rejects the complete reference artifact. Canonical output uses the decoded segment sequence in order with the shared JSON string-escaping rules.

Human-facing slash joining is display-only and cannot be parsed back into an ID.

Delivery-unit identity uses three independent inclusive protocol ceilings:

| `LinkLimits` counter | Accounting rule | Initial protocol ceiling |
| --- | --- | --: |
| `delivery_unit_segment_bytes` | Exact decoded UTF-8 bytes in one `DeliveryUnitSegment`. | 255 bytes |
| `delivery_unit_segments` | Number of elements in one non-empty `DeliveryUnitId`. | `64` |
| `delivery_unit_bytes` | Sum of decoded UTF-8 byte lengths of every segment in one `DeliveryUnitId`. | 4 KiB (`4,096` bytes) |

Known array lengths are preflighted before retaining segments; adapters then validate and charge each decoded segment in array order using checked `u64` arithmetic before exposing the ID.

Exactly 255 bytes in one valid segment, 64 segments, and 4,096 total bytes are accepted when all other checks pass; the first value over any ceiling rejects the complete artifact or graph. Repeated segment values and shared prefixes are charged per occurrence without deductions for interning or zero-copy storage.

Framing, display separators, and allocation capacity are excluded, while the actual JSON spelling remains part of the enclosing wire-byte counter.

Producers, graph constructors, artifact decoders, and direct checked constructors apply the same fixed ceilings and caller-selected lower immutable budgets without truncation, hashing, replacement, or fallback to a string form.

Aggregate graph-node, edge, and ID-byte limits are fixed in the Delivery Units section below.

The build integration deterministically assigns IDs from its pre-output logical delivery graph and supplies the exact same checked values to reference producers and `DeliveryUnitGraph`.

It does not derive them from an absolute/current/temporary path, a content-addressed output filename, array position, worker completion, random value, or final registration destination.

The shared built-in single-unit identity has the exact one-segment wire value `["main"]`. Built-in CLI, editor analysis, and N0 whole-program integrations construct that same checked `DeliveryUnitId` under the effective limits; they never substitute a project name, target name, path, hash, process value, or platform label. This contextual constant is not a globally unique package identity.

M0 assigns IDs only for reference artifacts produced within the current application; binding reusable-package references into final contextual graph nodes belongs to the deferred package-resource design.

The checked graph constructor requires every node ID to be unique by exact segment equality. During `LinkRequest` validation, each reference artifact's `delivery_unit` must equal exactly one existing graph node; a missing node is an operational error and the linker never creates one implicitly.

Equality and canonical ordering compare segment-by-segment exact decoded UTF-8 bytes, with a shorter equal-prefix sequence first. Locale collation, host path behavior, display rendering, artifact order, and platform conventions do not participate.

The value participates in artifact equality, cache identity, structured output, finding evidence, plan placement, and deterministic ordering.

`ReferenceRecordIdentity` is exactly `(artifact, ordinal)`. `ordinal` is the zero-based position in the artifact's semantic `references` array and is derived rather than redundantly serialized on each `MessageReference`. The array order is therefore preserved by every wire adapter and checked typed constructor.

An empty vector is valid; a non-empty vector's first record is `0`.

Reference-record collections use two independent inclusive protocol ceilings:

| `LinkLimits` counter | Accounting rule | Initial protocol ceiling |
| --- | --- | --: |
| `reference_records` | Number of submitted `MessageReference` occurrences in one artifact's semantic `references` array. | `1,000,000` |
| `reference_records_total` | Sum of submitted reference-record counts across every reference artifact in one `LinkRequest`. | `4,000,000` |

The producer and reference-artifact decoder preflight `reference_records` from the array length before allocating or decoding individual records, and a direct checked constructor verifies the supplied vector length before exposing the artifact.

Empty and exactly 1,000,000-record arrays are valid when their contents pass every other check; the latter assigns exact ordinals `0.. =999,999`.

Length conversion is checked even though the protocol ceiling is below `u32::MAX`; the 1,000,001st submitted occurrence or any conversion failure rejects the complete artifact before a `ReferenceRecordIdentity` is exposed.

Every submitted occurrence counts, including equal, duplicate, or later-invalid records, without deductions for sorting, grouping, interning, or presentation aggregation.

After phase-zero wire-byte admission succeeds, complete reference-artifact validation first validates the root shape, version, producer, checked `ReferenceArtifactIdentity`, and checked `DeliveryUnitId`, then performs that `reference_records` length preflight.

It next runs one complete `catalog_scope_name_bytes` pass over the preserved semantic reference array in ordinal order before domain, selector, reason, origin, or remaining record validation.

An overrun uses `CatalogScopeNameBytes`, the already established `ReferenceArtifactGroup(identity)`, and `Exact(effective_limit + 1)` without retaining the raw scope, a `CatalogScopeId`, or record ordinal. A scope-name failure in this pass wins even when a later field or semantic failure occurs at an earlier ordinal.

JSON member order, decoder strategy, cache reuse, partitioning, and worker completion cannot interleave or reorder this pass.

After the complete scope-name pass succeeds, record admission uses one fixed hybrid precedence. First, one complete `selector_path_bytes` pass visits every `Exact` and `Prefix` payload in record-ordinal order; other selector variants contribute nothing to this pass.

Second, one complete `selector_pattern_bytes` pass visits every `Pattern` payload in record-ordinal order.

Both byte passes operate on the decoded strings before domain-path grammar or pattern parsing, and an overrun in an earlier complete pass wins over every failure in a later pass even when the later failure belongs to an earlier ordinal.

Third, a pattern parse/token phase visits `Pattern` records in ordinal order and each pattern segment from left to right, performing slash segmentation, escape decoding, operator/literal disambiguation, and `selector_pattern_tokens` admission together.

Because a token exists only after those structural steps succeed, this third phase does not fabricate a token count for malformed input or defer a structural parse error merely to search later records for a token overrun: the first parse error or token overrun in that canonical record-and-segment order wins.

Fourth, one complete `reason_bytes` pass visits every present reason in record-ordinal order before empty/control-scalar grammar; omission contributes nothing. Fifth, one complete `path_segments` pass preflights the source path of every present origin in record-ordinal order.

Sixth, one complete `path_segment_bytes` pass visits those origins in record-ordinal order and each decoded segment in segment order. Seventh, one complete `path_bytes` pass visits the same origins in record-ordinal order and adds each already admitted complete segment to a counter reset for that path.

All four passes use the established `ReferenceArtifactGroup(identity)` subject. `ReasonBytes`, `PathSegments`, and `PathSegmentBytes` return exactly `Exact(effective_limit + 1)`; `PathBytes` returns the exact attempted per-path running total.

An earlier complete pass wins over every later failure regardless of record ordinal. Only after all seven admission phases succeed does validation continue with remaining domain-specific path/literal rules, selector semantics, reason grammar, origin path grammar and span validation, and other record fields.

Sequential, cached, partitioned, and parallel implementations must expose the same phase and canonical-order result without retaining a reason, path, segment, or ordinal in limit evidence.

Each per-artifact record count is checked during the complete per-artifact phase.

After that phase and a successful `reference_identity_bytes_total` pass, the integration and linker run the second request aggregate pass and sum the counts into `reference_records_total` with checked `u64` arithmetic before `reference_artifact_decoded_bytes_total`, aggregate record-index allocation, or semantic analysis.

Zero and exactly 4,000,000 records are accepted; conversion or addition overflow and the 4,000,001st record reject the complete request.

The shared canonical identity-group reduction makes artifact input order, parallel decoding, worker partitioning, identical records, cache reuse, and records later suppressed as secondary findings unable to change the charged total or selected error.

This count is independent of record payload bytes, which remain subject to the reference artifact's decoded-byte and field-specific limits.

Both record counters follow the common immutable lower-budget rule. A per-artifact lower limit of zero admits only empty `references` arrays, and a total lower limit of zero requires every admitted reference artifact to be empty.

Exceeding either counter produces no truncated artifact, partial identity set, sampled findings, partial plans, automatic artifact sharding, or relaxed-limit retry.

A producer may intentionally define several separately identified logical artifacts, but neither the decoder nor linker invents shards to make an over-limit artifact admissible.

The containing `LinkRequest` rejects duplicate `ReferenceArtifactIdentity` values before semantic analysis, because they would make record identity ambiguous; it never disambiguates them with producer revision, delivery unit, artifact input order, or a content hash.

Within one admitted artifact, every ordinal occurs exactly once by construction. Reordering, inserting, or deleting reference records changes affected ordinals and therefore their identities in the new artifact.

The contract guarantees portability and determinism for one exact artifact, not cross-revision identity stability after its semantic array changes.

`origin`, selector, reason, scope, domain, delivery unit, producer identity/revision, record content, and hashes are not fallback identity inputs. They remain semantic fields, provenance, or evidence attached to the record.

`ReferenceRecordIdentity` participates in finding subjects, equality, cache keys, structured output, and deterministic within-kind ordering; identical record fields at two ordinals remain distinct, while changing only optional reason or origin does not create a new identity for the same artifact and ordinal.

#### M0 reference-artifact transport and limits

##### Encoding contract

The M0 serialized `MessageReferenceArtifact` wire is exactly one JSON object encoded as valid UTF-8 without a byte-order mark. It uses the same JSON transport contract as `MessageDefinitionArtifact`.

CBOR, MessagePack, a custom binary envelope, and content-sniffed alternatives cannot claim the same artifact contract or `ArtifactVersion`; another encoding requires an explicitly identified transport contract and compatibility decision.

The decoder never guesses from leading bytes, a filename extension, MIME metadata, or content.

##### Per-artifact byte budgets

Reference-artifact admission uses two independent, inclusive per-artifact byte counters:

| `LinkLimits` counter | Accounting rule | Initial protocol ceiling |
| --- | --- | --: |
| `reference_artifact_wire_bytes` | Exact bytes in one uncompressed serialized `MessageReferenceArtifact` JSON document supplied to the `intlify_contract` decoder, including member names, punctuation, number tokens, string escaping, whitespace, and trailing or otherwise invalid input bytes that the decoder receives. External transport or container compression is not artifact wire; integrations feed its decompressed output through the bounded decoder without first materializing an unbounded buffer. | 512 MiB (`536,870,912` bytes) |
| `reference_artifact_decoded_bytes` | Sum of the exact decoded UTF-8 byte lengths of every variable-width scalar payload in the final logical typed reference artifact. Each logical occurrence is charged independently after wire unescaping and any artifact-local expansion. | 256 MiB (`268,435,456` bytes) |

These values intentionally match the definition-artifact byte ceilings, but the four counters are distinct: no artifact kind borrows, transfers, pools, or offsets another kind's budget.

A reference artifact must pass both counters independently, and neither counter estimates or substitutes for the other; no required two-to-one wire/decoded ratio exists. `reference_artifact_decoded_bytes` includes producer id and revision, reference-artifact identity segments, delivery-unit segments, every catalog-scope name, every `Exact`, `Prefix`, or `Pattern` payload, each present reason, and every present origin source-path segment.

A more specific field or path limit overlaps rather than replaces this total. Fixed-width integers, closed enum and namespace discriminants, wire-only kind strings after validation, JSON framing, and member names add zero decoded bytes.

Repeated equal values, shared strings, zero-copy slices, and interning provide no deduction.

##### Wire-byte admission

Wire-byte admission is phase zero before root syntax and every decoded or semantic check. When the uncompressed document length is known, the decoder compares it before parsing.

For an unknown-length stream, it counts through EOF or the first byte at `effective_limit + 1`; a syntax error discovered earlier remains provisional until that bounded wire admission finishes.

Reaching the first excess byte selects `ReferenceArtifactWireBytes` over every syntax or decoded failure and stops without reading the remainder; reaching EOF at or below the limit permits the previously selected canonical syntax result.

Chunking, parser lookahead, and known-versus-streaming ingestion therefore cannot change the public failure. Direct typed construction has no wire phase or synthetic wire charge.

A producer validates the decoded total over the complete logical artifact and validates wire bytes with a bounded counting encoder before publication, exposing no partial artifact.

A decoder counts exact wire bytes incrementally, including invalid trailing input, and charges each decoded scalar before allocation or typed admission.

Direct checked construction enforces the decoded and structural limits. `intlify_linker` may defensively recompute the observable decoded total from a typed artifact but never invents a wire length after decoding.

Caller-selected lower immutable values for the two counters are independent, and every overrun follows the common fail-complete, cache-revalidation, and versioned-ceiling rules.

##### Decoded-byte accounting

Decoded-budget failure selection follows the canonical reference-artifact validation phases above, never input object-member order or the point at which a streaming parser happens to encounter bytes.

Within each phase, the applicable shape and field-specific limit checks run first; only an admitted complete variable-width payload is then added once, with checked arithmetic, to the artifact's decoded running total.

If that addition exceeds the effective `reference_artifact_decoded_bytes` ceiling, the per-artifact decoded-budget failure wins over every later phase but never over an unfinished earlier phase.

A decoder may measure and mark provisional charges while scanning wire order, but it exposes that failure only after all logically preceding phases have passed.

It uses bounded staging, skips retention of payload beyond the effective budget, and may continue the bounded syntax scan needed to resolve earlier-phase validity; it never allocates decoded storage past the effective ceiling merely to preserve precedence.

Producers, direct checked constructors, cache revalidation, defensive linker validation, partitioned implementations, and parallel implementations select the same canonical phase result.

##### Request aggregate decoded-byte budget

At request admission, `intlify_linker` owns one additional inclusive aggregate `LinkLimits` counter named `reference_artifact_decoded_bytes_total`, with a fixed protocol ceiling of 1 GiB (`1,073,741,824` bytes).

It is the checked `u64` sum of the exact `reference_artifact_decoded_bytes` charge for every submitted reference artifact that reaches aggregate admission.

Each artifact occurrence contributes once; duplicate identities, later cross-artifact validation or semantic rejection, equal payloads, interning, zero-copy storage, cache reuse, worker partitioning, and parallel execution provide no deduction.

The core recomputes the observable charge from typed artifacts, so decoded artifacts and direct checked construction receive identical aggregate treatment without depending on their ingestion route.

Zero and exactly 1 GiB are accepted; the first byte above the effective ceiling or checked-addition / host-size conversion overflow rejects the complete request without findings or plans.

At protocol-default limits, four artifacts each charged at the exact 256 MiB per-artifact ceiling fit exactly, while any positive charge from a fifth artifact exceeds the aggregate ceiling. This is a boundary illustration, not a required artifact size or sharding policy.

A caller may select an independent lower immutable aggregate value, including zero; because every structurally valid reference artifact contains non-empty variable-width identity and provenance payloads, a zero aggregate value admits only an empty reference-artifact collection.

##### Request admission precedence

Reference-artifact request admission uses this exact, non-interleaved precedence:

1. preflight the `reference_artifacts` collection length;
2. complete structural and effective per-artifact validation for every submitted artifact;
3. complete the `reference_identity_bytes_total` aggregate pass;
4. complete the `reference_records_total` aggregate pass;
5. complete the `reference_artifact_decoded_bytes_total` aggregate pass; and
6. perform duplicate-identity detection, other cross-artifact validation, index construction, and semantic analysis.

A failure in an earlier phase or aggregate counter always wins even if a later counter would exceed its ceiling at an earlier artifact identity. Implementations may compute provisional charges early, but they may not expose, cache-admit, or select a later-phase failure before every preceding phase has passed.

Each of the three aggregate passes orders admitted artifacts by canonical `ReferenceArtifactIdentity`.

Occurrences with one equal identity form one contiguous group: the pass adds each occurrence's charge with checked arithmetic into a counter-specific group subtotal and then adds that subtotal once to the request counter, without deduplicating any occurrence.

It stops at the first canonical group whose addition exceeds the effective ceiling; it does not scan later groups merely to report a larger value. Grouping makes the selected aggregate overrun independent of input order within an invalid duplicate group.

Sequential, partitioned, and parallel implementations must select the same counter, canonical identity group, and attempted value as this serial reduction and may not return a racing worker's overrun.

##### Aggregate error evidence

An aggregate-limit `LinkOperationalError` from any of those three passes retains exactly the one `ReferenceArtifactIdentity` naming the canonical group whose checked addition selected the failure.

The subject denotes the whole equal-identity group rather than an arbitrary occurrence, so it remains well-defined even when duplicate artifacts have different producer, delivery-unit, record, or payload data.

It never carries every contributing artifact, an input index, worker identity, producer value, delivery unit, record ordinal, selector, origin, payload excerpt, or a second representative occurrence.

A `reference_artifacts` collection-count preflight failure is request-level and therefore has no fabricated artifact-group subject.

#### Shared artifact limit and error contracts

##### Link limit evidence

The common checked operational-limit evidence has this conceptual shape:

```rust
pub enum LinkLimitCounter {
    ReferenceArtifacts,
    ReferenceIdentityBytesTotal,
    ReferenceRecordsTotal,
    ReferenceArtifactDecodedBytesTotal,
    DefinitionArtifacts,
    DefinitionsTotal,
    DefinitionArtifactDecodedBytesTotal,
    DeliveryGraphNodes,
    DeliveryGraphEdges,
    DeliveryGraphIdBytes,
    ProductionLocales,
    FallbackSources,
    ConfiguredRoots,
    FallbackTargetsPerSource,
    LocaleBytes,
    EntryStructuralPathBytes,
    CatalogKeyBytes,
    MessageBytes,
    TotalMessageBytes,
    CatalogScopeNameBytes,
    ScopeMappingEntries,
    SelectorPathBytes,
    SelectorPatternBytes,
    SelectorPatternTokens,
    PatternMatchStatesTotal,
    ReasonBytes,
    PathSegments,
    PathSegmentBytes,
    PathBytes,
    LogicalAliases,
    SourcePathBytes,
    ReferenceArtifactWireBytes,
    ReferenceArtifactDecodedBytes,
    DefinitionArtifactWireBytes,
    DefinitionArtifactDecodedBytes,
    FindingsTotal,
    FindingBytesTotal,
    BundlePlansTotal,
    ResolvedMessagesTotal,
    BundlePlanBytesTotal,
    // Later owning decisions add explicit variants; there is no custom-string case.
}

pub enum LinkLimitObservation {
    Exact(u64),
    ArithmeticOverflow,
}

pub enum LinkLimitSubject {
    Request,
    DefinitionArtifactEnvelope,
    ReferenceArtifactGroup(ReferenceArtifactIdentity),
    DefinitionArtifactGroup(SourceDocumentIdentity),
    DeliveryGraph,
    DeliveryUnitGroup(DeliveryUnitId),
    ResolvedPolicy,
    FallbackSource(Locale),
    ScopeMappings,
}

pub struct LinkLimitEvidence {
    counter: LinkLimitCounter,
    subject: LinkLimitSubject,
    effective_limit: u64,
    observation: LinkLimitObservation,
}
```

The fields are private and exposed read-only through a checked constructor. The forty currently fixed counter variants form a closed set.

Within the reference-request, definition-request, delivery-graph, and resolved-policy groups, declaration order matches the non-interleaved local precedence fixed in their owning sections.

`ScopeMappingEntries` is appended as the twenty-first variant to preserve the already fixed ordinals 1 through 20; its owning mapping phase explicitly runs before the shared `CatalogScopeNameBytes` phase despite their enum declaration positions.

`SelectorPathBytes`, `SelectorPatternBytes`, and `SelectorPatternTokens` are appended as variants 22 through 24 in their selector-contract order while preserving ordinals 1 through 21; their exact cross-record validation precedence is fixed by the complete reference-artifact validation phase below rather than inferred from enum order.

`PatternMatchStatesTotal` is appended as the twenty-fifth variant without changing ordinals 1 through 24; its request-wide pattern-evaluation phase occurs after admitted references and definitions have formed canonical candidate sets, not at its enum declaration position. `ReasonBytes` is the twenty-sixth variant.

The shared portable-path variants follow their local admission order as `PathSegments`, `PathSegmentBytes`, and `PathBytes` at ordinals 27 through 29. The definition-only set counters follow as `LogicalAliases` and `SourcePathBytes` at ordinals 30 and 31.

The per-artifact transport/accounting counters follow without renumbering those variants: `ReferenceArtifactWireBytes`, `ReferenceArtifactDecodedBytes`, `DefinitionArtifactWireBytes`, and `DefinitionArtifactDecodedBytes` occupy ordinals 32 through 35.

The linker-result counters are appended without renumbering any input counter. `FindingsTotal` and `FindingBytesTotal` occupy ordinals 36 and 37. `BundlePlansTotal`, `ResolvedMessagesTotal`, and `BundlePlanBytesTotal` occupy ordinals 38 through 40.

The finding counters run only after semantic suppression and canonical finding construction rules have selected the complete final finding candidate set. The plan counters run only when that set contains no blocking finding and therefore admits plan construction.

Their declaration order groups one artifact kind's wire and decoded boundary and does not create cross-artifact-kind precedence. Field-, path-, and artifact-level variants follow their owning contexts' local phases rather than creating a cross-input group by declaration position.

Declaration order does not silently decide precedence between otherwise independent request-input groups.

Structured adapters use these exact spellings. The common v0.1 counter registry fixes `FallbackSources` and `FallbackTargetsPerSource` ahead of activation so the already documented ordinals of later artifact and selector counters do not shift. They are reserved, unreachable evidence variants in M0/M1: raw configuration cannot name them, `LinkPolicy` has no fallback values, no caller can select a lower value for them, and no M0/M1 operation emits them. M2 activates both counters atomically with the fallback-bearing typed policy, validation, and analysis. This protocol-level ordinal reservation is not acceptance of a dormant configuration field.

| `LinkLimitCounter` variant            | Structured spelling                       |
| ------------------------------------- | ----------------------------------------- |
| `ReferenceArtifacts`                  | `reference_artifacts`                     |
| `ReferenceIdentityBytesTotal`         | `reference_identity_bytes_total`          |
| `ReferenceRecordsTotal`               | `reference_records_total`                 |
| `ReferenceArtifactDecodedBytesTotal`  | `reference_artifact_decoded_bytes_total`  |
| `DefinitionArtifacts`                 | `definition_artifacts`                    |
| `DefinitionsTotal`                    | `definitions_total`                       |
| `DefinitionArtifactDecodedBytesTotal` | `definition_artifact_decoded_bytes_total` |
| `DeliveryGraphNodes`                  | `delivery_graph_nodes`                    |
| `DeliveryGraphEdges`                  | `delivery_graph_edges`                    |
| `DeliveryGraphIdBytes`                | `delivery_graph_id_bytes`                 |
| `ProductionLocales`                   | `production_locales`                      |
| `FallbackSources`                     | `fallback_sources`                        |
| `ConfiguredRoots`                     | `configured_roots`                        |
| `FallbackTargetsPerSource`            | `fallback_targets_per_source`             |
| `LocaleBytes`                         | `locale_bytes`                            |
| `EntryStructuralPathBytes`            | `entry_structural_path_bytes`             |
| `CatalogKeyBytes`                     | `catalog_key_bytes`                       |
| `MessageBytes`                        | `message_bytes`                           |
| `TotalMessageBytes`                   | `total_message_bytes`                     |
| `CatalogScopeNameBytes`               | `catalog_scope_name_bytes`                |
| `ScopeMappingEntries`                 | `scope_mapping_entries`                   |
| `SelectorPathBytes`                   | `selector_path_bytes`                     |
| `SelectorPatternBytes`                | `selector_pattern_bytes`                  |
| `SelectorPatternTokens`               | `selector_pattern_tokens`                 |
| `PatternMatchStatesTotal`             | `pattern_match_states_total`              |
| `ReasonBytes`                         | `reason_bytes`                            |
| `PathSegments`                        | `path_segments`                           |
| `PathSegmentBytes`                    | `path_segment_bytes`                      |
| `PathBytes`                           | `path_bytes`                              |
| `LogicalAliases`                      | `logical_aliases`                         |
| `SourcePathBytes`                     | `source_path_bytes`                       |
| `ReferenceArtifactWireBytes`          | `reference_artifact_wire_bytes`           |
| `ReferenceArtifactDecodedBytes`       | `reference_artifact_decoded_bytes`        |
| `DefinitionArtifactWireBytes`         | `definition_artifact_wire_bytes`          |
| `DefinitionArtifactDecodedBytes`      | `definition_artifact_decoded_bytes`       |
| `FindingsTotal`                       | `findings_total`                          |
| `FindingBytesTotal`                   | `finding_bytes_total`                     |
| `BundlePlansTotal`                    | `bundle_plans_total`                      |
| `ResolvedMessagesTotal`               | `resolved_messages_total`                 |
| `BundlePlanBytesTotal`                | `bundle_plan_bytes_total`                 |

There is no unknown, other, custom, or raw-string counter. Adding a later policy or output counter adds one explicit variant and its ordering/ceiling/subject invariants through the owning compatibility decision rather than accepting an extension string.

##### Artifact contract errors

###### Boundary and evidence ownership

`ArtifactContractError::Limit` is the producer/decoder/checked-constructor boundary for one artifact and carries exactly the same closed `LinkLimitCounter`, effective limit, and `LinkLimitObservation`, but no `LinkLimitSubject`, artifact index, fabricated identity, raw field, or payload excerpt.

The counter itself identifies the artifact kind and wire-versus-decoded budget. `ReferenceArtifactWireBytes` and `DefinitionArtifactWireBytes` occur only at serialized production/ingestion and never in `LinkLimitEvidence`, because `link` receives typed artifacts and cannot reconstruct an honest wire length.

`ReferenceArtifactDecodedBytes` and `DefinitionArtifactDecodedBytes` may occur both in subject-free `ArtifactContractError::Limit` and, when `link` revalidates a checked artifact under a lower request budget, in `LinkOperationalError` with the already established exact `ReferenceArtifactGroup(identity)` or `DefinitionArtifactGroup(source)`.

This split preserves one counter vocabulary without forcing an identity to exist before contract admission.

###### Top-level error model

The complete M0 top-level contract-error shape is closed to three variants:

```rust
pub enum ArtifactContractError {
    InvalidArtifact(ArtifactViolation),
    UnsupportedVersion(ArtifactVersionEvidence),
    Limit(ArtifactLimitEvidence),
}

pub struct ArtifactViolation {
    code: ArtifactViolationCode,
    location: ArtifactViolationLocation,
}

pub struct ArtifactVersionEvidence {
    observed: ArtifactVersion,
    supported: ArtifactVersionSupport,
}

pub enum ArtifactVersionSupport {
    Exact(ArtifactVersion),
    StableRange { major: u16, max_minor: u16 },
}

pub struct ArtifactLimitEvidence {
    counter: LinkLimitCounter,
    effective_limit: u64,
    observation: LinkLimitObservation,
}
```

###### Artifact violation model

```rust

pub enum ArtifactViolationCode {
    InvalidUtf8,
    InvalidJsonSyntax,
    TrailingData,
    MissingMember,
    DuplicateMember,
    UnknownMember,
    NullNotAllowed,
    TypeMismatch,
    InvalidInteger,
    KindMismatch,
    UnknownTag,
    InvalidValueGrammar,
    NonCanonicalOrder,
    DuplicateValue,
    InconsistentValue,
    DiscontinuousOccurrence,
}

pub enum ArtifactViolationLocation {
    Root,
    Envelope(ArtifactEnvelopeLocation),
    EnvelopePathSegment {
        role: ArtifactPathRole,
        segment_ordinal: u32,
    },
    LogicalAlias {
        alias_ordinal: u32,
        segment_ordinal: Option<u32>,
    },
    Definition {
        ordinal: u32,
        field: Option<DefinitionField>,
    },
    Reference {
        ordinal: u32,
        field: Option<ReferenceField>,
    },
    ReferenceOriginSegment {
        reference_ordinal: u32,
        segment_ordinal: u32,
    },
}

pub enum ArtifactEnvelopeLocation {
    Reference {
        field: Option<ReferenceEnvelopeField>,
    },
    Definition {
        field: Option<DefinitionEnvelopeField>,
    },
}

pub enum ReferenceEnvelopeField {
    Kind,
    Version,
    VersionMajor,
    VersionMinor,
    Producer,
    ProducerId,
    ProducerRevision,
    Identity,
    IdentityNamespace,
    IdentitySegments,
    DeliveryUnit,
    References,
}

pub enum DefinitionEnvelopeField {
    Kind,
    Version,
    VersionMajor,
    VersionMinor,
    Producer,
    ProducerId,
    ProducerRevision,
    Source,
    SourceNamespace,
    SourcePath,
    LogicalAliases,
    InputFingerprint,
    FingerprintAlgorithm,
    FingerprintDigest,
    Definitions,
}

pub enum ArtifactPathRole {
    DefinitionSource,
    ReferenceIdentity,
    DeliveryUnit,
}

pub enum DefinitionField {
    Scope,
    ScopeNamespace,
    ScopeName,
    Domain,
    Key,
    Locale,
    Message,
    Source,
    SourceStructuralPath,
    SourceOccurrence,
}

pub enum ReferenceField {
    Scope,
    ScopeNamespace,
    ScopeName,
    Domain,
    Selector,
    SelectorKind,
    SelectorKey,
    SelectorPrefix,
    SelectorPattern,
    Reason,
    Origin,
    OriginSource,
    OriginSourceNamespace,
    OriginSourcePath,
    OriginSpan,
    OriginSpanStart,
    OriginSpanEnd,
}
```

`InvalidArtifact` covers malformed UTF-8/JSON, the selected schema, exact kind/tag/value grammar, canonical-order/duplicate rules, and cross-field consistency.

A structurally valid `ArtifactVersion` that fails the negotiated exact pair or stable range uses `UnsupportedVersion` with the already fixed observed/supported evidence; a malformed version object or integer remains `InvalidArtifact`. Resource exhaustion alone uses `Limit`.

No variant accepts a free-form message, arbitrary code, source chain, parser error object, `Other`, or `Custom`; presentation derives text from the closed code and typed location.

An explicitly detected implementation invariant is not forged from untrusted bytes as an artifact violation: linker-side detection remains `LinkOperationalError::InternalInvariant`, while producer/ingestion implementation failures remain their owning integration error.

##### Artifact violation classification

The sixteen violation codes have these exact classification boundaries:

| `ArtifactViolationCode` | Exact use |
| --- | --- |
| `InvalidUtf8` | The input byte sequence cannot decode as UTF-8. |
| `InvalidJsonSyntax` | The bytes are valid UTF-8 but violate the selected JSON lexical/grammar contract, including a BOM, comment, trailing comma, malformed escape, or unpaired surrogate. |
| `TrailingData` | One complete root value is followed by non-whitespace or another root value; permitted trailing JSON whitespace is not a violation. |
| `MissingMember` | A required member is absent from an otherwise identified containing object. |
| `DuplicateMember` | One object submits the same decoded member name more than once, before map-style overwrite. |
| `UnknownMember` | A member is not declared by the selected supported schema. |
| `NullNotAllowed` | An explicitly present `null` occurs where omission or a non-null value is required; this is selected instead of `TypeMismatch`. |
| `TypeMismatch` | A present value has the wrong JSON category, such as an object where a string is required. |
| `InvalidInteger` | An integer token has a sign, fraction, exponent, forbidden spelling, or value outside its declared unsigned width. |
| `KindMismatch` | A decoded string in `kind` is not the exact artifact kind required by the typed decoder/generic dispatch; a non-string still uses `TypeMismatch`. |
| `UnknownTag` | A decoded string tag inside an otherwise supported schema, such as a domain, selector kind, namespace, or fingerprint algorithm, is not one of that schema's exact closed tags. |
| `InvalidValueGrammar` | A correctly typed scalar or compound value violates its field grammar, including producer, locale, path, span, selector literal, or reason rules not represented by a more specific code. |
| `NonCanonicalOrder` | A submitted collection required to be strictly increasing contains a descending adjacent pair; non-semantic JSON object-member order never uses this code. |
| `DuplicateValue` | Exact equality creates a forbidden repeated alias, identity, mapping key, or other value-level duplicate; duplicate object members remain `DuplicateMember`. |
| `InconsistentValue` | Individually well-shaped fields conflict across a required binding, namespace, scope/domain, physical group, or other cross-field invariant. |
| `DiscontinuousOccurrence` | `EntryReference.occurrence` skips, decreases, repeats improperly, or cannot advance within its `u32` sequence. |

This table classifies a failure after the owning validation phase has selected it; declaration order is not a global error precedence. Wire admission still precedes UTF-8/JSON, root syntax and duplicate-member checks precede schema phases, and every envelope/record phase retains its already fixed ordering.

A malformed version shape or integer uses the applicable invalid-artifact code, while a structurally valid but incompatible version alone uses `UnsupportedVersion`. Parser-native categories are translated into this table and never leak into the public contract.

##### Artifact violation locations

The location is semantic and bounded, not a reconstruction of input JSON syntax. The closed `ArtifactEnvelopeLocation`, `ArtifactPathRole`, `DefinitionField`, and `ReferenceField` enums name only versioned contract fields; the path role is exactly definition source, reference identity, or delivery unit.

An envelope location always retains `Reference` or `Definition`, including its container form. `None` identifies that containing known object when a missing/unknown member has no valid field identity.

Every ordinal is bounded by the already admitted collection/path ceilings and is stored as `u32`. Invalid UTF-8/JSON with no typed position uses `Root`; a primary path segment, logical alias/segment, definition field, reference field, and reference-origin segment use their corresponding variant.

Unknown raw member names, rejected values, JSON Pointer strings, byte offsets, line/column, excerpts, and host paths are absent.

A decoder may retain byte-oriented detail in implementation-local tracing, but decoder, checked constructor, cache revalidation, and defensive validation expose the same public semantic location regardless of member order, escaping, whitespace, or ingestion route.

`ArtifactEnvelopeLocation` is intrinsically tagged by artifact kind, so a reference-only field such as `DeliveryUnit` and a definition-only field such as `InputFingerprint` cannot form a cross-kind location, and a container-level failure cannot lose its artifact kind.

The nested envelope enums name both a containing object and its known leaves; an object-shape failure selects the kind-tagged container, while a known leaf failure selects that leaf.

`EnvelopePathSegment` is used only after the applicable definition-source, reference-identity, or delivery-unit path has exposed a submitted segment ordinal; a whole-path/count/shape failure remains at its envelope field.

`LogicalAlias` similarly uses `segment_ordinal: None` for the submitted alias as a whole and `Some` only for one exposed segment. Definition/reference object-shape or unknown-member failure uses the record ordinal with `field: None`; a known member or nested leaf uses `Some`.

Reference-origin source-path segments use the dedicated variant, while whole source/span failures use the applicable `ReferenceField`. Enum declaration order is a stable adapter order only and never replaces the owning canonical validation precedence.

##### Structured error adapters

The structured adapter for `ArtifactContractError` is an internally tagged, closed JSON object contract. Its exact top-level shapes and canonical member orders are:

```json
{"kind":"invalid_artifact","violation":{"code":"missing_member","location":{"kind":"envelope","artifact":"definition","field":"producer_revision"}}}
{"kind":"unsupported_version","observed":{"major":0,"minor":2},"supported":{"kind":"exact","version":{"major":0,"minor":1}}}
{"kind":"limit","counter":"definition_artifact_wire_bytes","effectiveLimit":536870912,"observation":{"kind":"exact","attempted":536870913}}
```

The top-level order is `kind`, then `violation`; `kind`, `observed`, then `supported`; or `kind`, `counter`, `effectiveLimit`, then `observation`, respectively. A violation orders `code` before `location`; a version orders `major` before `minor`.

`ArtifactVersionSupport::Exact` is exactly `{"kind":"exact","version":...}` and `StableRange` is exactly `{"kind":"stable_range","major":N,"maxMinor":N}` in the shown member order.

`LinkLimitObservation::Exact` is exactly `{"kind":"exact","attempted":N}` and `ArithmeticOverflow` is exactly `{"kind":"arithmetic_overflow"}`. Limit counters use the exact snake-case tokens fixed above.

JSON input member order is non-semantic, while canonical output always uses these orders. Missing, duplicate, unknown, or required `null` members are rejected; no Rust enum ordinal, display message, parser error, or source chain is serialized.

The sixteen exact violation-code tokens are `invalid_utf8`, `invalid_json_syntax`, `trailing_data`, `missing_member`, `duplicate_member`, `unknown_member`, `null_not_allowed`, `type_mismatch`, `invalid_integer`, `kind_mismatch`, `unknown_tag`, `invalid_value_grammar`, `non_canonical_order`, `duplicate_value`, `inconsistent_value`, and `discontinuous_occurrence`.

Their Rust declaration ordinals are not part of the representation.

The seven exact location shapes and their canonical member orders are:

- root: `{"kind":"root"}`;
- an envelope container or leaf: `{"kind":"envelope","artifact":"reference"}` or `{"kind":"envelope","artifact":"definition","field":"producer_revision"}`;
- an envelope path segment: `{"kind":"envelope_path_segment","role":"definition_source","segmentOrdinal":1}`;
- a logical alias or its segment: `{"kind":"logical_alias","aliasOrdinal":3}` or the same object followed by `"segmentOrdinal":1`;
- a definition container or leaf: `{"kind":"definition","ordinal":2}` or the same object followed by `"field":"message"`;
- a reference container or leaf: `{"kind":"reference","ordinal":4}` or the same object followed by `"field":"selector_pattern"`;
- a reference-origin segment: `{"kind":"reference_origin_segment","referenceOrdinal":4,"segmentOrdinal":1}`.

The envelope `artifact` token is exactly `reference` or `definition`. The path-role tokens are exactly `definition_source`, `reference_identity`, and `delivery_unit`.

Reference-envelope field tokens are exactly `kind`, `version`, `version_major`, `version_minor`, `producer`, `producer_id`, `producer_revision`, `identity`, `identity_namespace`, `identity_segments`, `delivery_unit`, and `references`.

Definition-envelope field tokens are exactly `kind`, `version`, `version_major`, `version_minor`, `producer`, `producer_id`, `producer_revision`, `source`, `source_namespace`, `source_path`, `logical_aliases`, `input_fingerprint`, `fingerprint_algorithm`, `fingerprint_digest`, and `definitions`.

Definition-field tokens are exactly `scope`, `scope_namespace`, `scope_name`, `domain`, `key`, `locale`, `message`, `source`, `source_structural_path`, and `source_occurrence`.

Reference-field tokens are exactly `scope`, `scope_namespace`, `scope_name`, `domain`, `selector`, `selector_kind`, `selector_key`, `selector_prefix`, `selector_pattern`, `reason`, `origin`, `origin_source`, `origin_source_namespace`, `origin_source_path`, `origin_span`, `origin_span_start`, and `origin_span_end`.

Optional `field` and `segmentOrdinal` members are omitted when absent and are never encoded as `null`; all other members shown for a location shape are required. Unknown raw member names still map only to the applicable kind-tagged container location and are never copied into this representation.

##### Schema-error precedence

Schema-error selection is deterministic and independent of parser discovery or JSON member order.

For one root document, schema errors use this fixed precedence:

1. Phase-zero wire-byte admission.
2. UTF-8, JSON syntax, and trailing-data validation.
3. Duplicate root-member selection in canonical root-field order.
4. `kind` presence, followed by `null`, JSON type, and exact-value checks.
5. `version` presence, followed by version-object duplicate/shape checks and then `major` and `minor` integer checks.
6. Version compatibility.
7. Supported-version required-root-member presence preflight in canonical emission order, excluding the already checked `kind` and `version`.
8. Root unknown-member rejection.
9. The remaining fields in their already fixed canonical envelope phases.

The reference presence preflight orders `producer`, `identity`, `deliveryUnit`, then `references`; the definition preflight orders `producer`, `source`, `logicalAliases`, `inputFingerprint`, then `definitions`.

Thus a structurally valid newer unsupported version wins over an unknown member from that unselected schema, while malformed bootstrap version data remains `InvalidArtifact`.

When an owning phase enters any nested object, its local schema precedence is: (1) duplicate-member selection in that object's canonical field order; (2) required-member presence preflight in canonical emission order; (3) unknown-member rejection; and (4) present-member validation in canonical field order, applying `null`, JSON type, scalar/compound value grammar, then cross-field consistency as applicable.

Optional members do not participate in the presence preflight. Duplicate selection is semantic and container-scoped: buffering may discover candidates in any order, but the public result selects the canonical field and location.

Collection counts, byte budgets, canonical-set checks, and record phases retain their more specific precedence already fixed by their owning sections; this rule orders schema failures within a phase and does not reorder those phase boundaries.

##### Decoder and encoder APIs

M0 provides both known-length slice entry points and synchronous pull-based reader entry points over one incremental decoder:

```rust
pub fn decode_reference_artifact(
    input: &[u8],
    limits: &LinkLimits,
) -> Result<MessageReferenceArtifact, ArtifactContractError>;

pub fn decode_reference_artifact_from_reader<R: std::io::Read>(
    reader: &mut R,
    limits: &LinkLimits,
) -> Result<MessageReferenceArtifact, ArtifactReadError>;

pub fn decode_definition_artifact(
    input: &[u8],
    limits: &LinkLimits,
) -> Result<MessageDefinitionArtifact, ArtifactContractError>;

pub fn decode_definition_artifact_from_reader<R: std::io::Read>(
    reader: &mut R,
    limits: &LinkLimits,
) -> Result<MessageDefinitionArtifact, ArtifactReadError>;

pub fn encode_reference_artifact(
    artifact: &MessageReferenceArtifact,
    limits: &LinkLimits,
) -> Result<Box<[u8]>, ArtifactContractError>;

pub fn encode_definition_artifact(
    artifact: &MessageDefinitionArtifact,
    limits: &LinkLimits,
) -> Result<Box<[u8]>, ArtifactContractError>;

pub enum ArtifactReadError {
    Transport(std::io::Error),
    Contract(ArtifactContractError),
}
```

The two encode functions are the complete M0 canonical-writer surface. They accept only immutable checked artifacts and already valid limits, select an explicitly supported writer for the artifact version, and return exactly one owned canonical JSON document with no final newline.

Within that selected writer, a bounded counting pass applies the matching artifact wire counter as phase zero and stops at the first attempted byte above the current effective limit without allocating or emitting a partial document.

If the count fits, the encoder runs the same current-lower decoded and structural revalidation as direct construction, allocates only the exact bounded output length, and emits the canonical spelling fixed by the selected artifact schema. The resulting box length is therefore at or below the effective wire limit.

A count/emission disagreement is an implementation invariant, not fabricated untrusted-input evidence.

M0 exposes no generic `std::io::Write` encoder, callback sink, caller-provided mutable buffer, streaming iterator, partial-success count, or resumable writer state.

Filesystem writes, temporary files, atomic rename, socket/backpressure handling, compression, framing, and publication remain integration responsibilities performed only after a complete `Box<[u8]>` is returned.

An I/O failure can therefore leave integration-owned transport state but never a partially successful contract encode result.

A future transactional or streaming encoder requires concrete memory/throughput evidence and a separate API addition that preserves the same canonical bytes, preflight result, limits, and fail-complete contract; it cannot silently replace these functions.

The encoder has no `ArtifactWriteError` because it performs no external I/O. Unsupported writer versions use `UnsupportedVersion`, current limit failures use `Limit`, and a checked artifact never regresses into parser/schema-shaped `InvalidArtifact` merely because it is being re-encoded.

##### Reader semantics

The slice path supplies its exact length hint to the same state machine; the reader path neither seeks nor closes the borrowed reader and consumes through artifact EOF unless phase-zero wire admission stops at the first excess byte.

An I/O or decompression adapter error before EOF means that no complete artifact was supplied and therefore returns `ArtifactReadError::Transport`, winning over every provisional syntax/schema/value result.

If the wire ceiling is reached first, the decoder returns `Contract(ArtifactContractError::Limit(...))` without another read, so a later hypothetical transport failure cannot replace it. At bounded EOF the normal contract precedence selects the result.

`ArtifactReadError` is an ingestion wrapper, not serialized artifact evidence or a linker error, and its transport payload never enters equality, cache identity, structured contract output, or conformance comparisons.

One reader invocation accepts exactly one EOF-delimited artifact document.

Completing the first JSON root is never early success: the decoder continues through permitted trailing whitespace until the reader returns EOF, classifies any non-whitespace or second root as `TrailingData`, and includes every consumed byte in wire accounting. A successful call leaves the borrowed reader at EOF.

An integration carrying several artifacts owns their framing and passes each exact frame through a separately bounded reader such as `std::io::Take`; reaching that frame boundary is the decoder's EOF and leaves the outer reader positioned immediately after the frame.

M0 does not interpret concatenated JSON, NDJSON, a root-array batch, a length prefix, or a transport container. On wire-limit failure it has consumed exactly `effective_limit + 1` document bytes and leaves the remainder unread; on transport failure it leaves the reader at the position of the failing read.

The slice entry point applies the same single-complete-document rule to the entire slice.

The synchronous decoder never calls `Read::read` with an empty destination buffer. `Ok(n)` for any positive `n` consumes exactly those `n` bytes even when the read is shorter than the requested buffer; a positive short read is ordinary chunking and never EOF.

`Ok(0)` therefore means EOF for this API and triggers the bounded contract-result selection described above.

`ErrorKind::Interrupted` is retried internally without advancing wire accounting, parser state, provisional failure, or reader position; any number of consecutive interruptions remains semantically invisible, although the caller remains responsible for a reader that eventually progresses.

Every other error, including `WouldBlock` and `TimedOut`, returns `ArtifactReadError::Transport` immediately.

The synchronous API never spins, sleeps, polls, or installs readiness handling for `WouldBlock`; an asynchronous or nonblocking integration must perform that work outside the contract boundary and present a blocking `Read` adapter or a complete bounded slice.

Once a wire overrun has been selected, no further call—including an interruption probe—is made. Reader chunk sizes, positive short reads, and inserted interruptions cannot change the successful artifact or public contract failure selected from the same byte sequence.

##### Async and transport boundary

`intlify_contract` owns no async runtime, executor, worker pool, cancellation mechanism, or decompressor.

An async integration adapts its stream to the synchronous reader on its own blocking worker or performs an equivalently bounded chunk pump outside the contract crate; transport-wide buffering, cancellation, and decompression policy remain integration-owned.

A future no-`std` or native async source requires a separate evidenced API addition and must preserve exact equivalence with these slice/reader semantics rather than introducing another decoder contract.

A generic artifact dispatcher may wrap the two typed decoders after exact kind selection but cannot use different limits, precedence, error codes, locations, or buffering semantics.

##### Limit subjects

In the subject rules below, “the other three reference variants” means only the three request-aggregate variants already declared next to `ReferenceArtifacts`; it does not include the newly appended per-artifact variants.

In `LinkLimitEvidence`, `ReferenceArtifactDecodedBytes` requires exactly the checked `ReferenceArtifactGroup(identity)` and `DefinitionArtifactDecodedBytes` requires exactly the checked `DefinitionArtifactGroup(source)`.

Their wire counterparts are unconstructible in that wrapper, and all four per-artifact variants are valid in subject-free `ArtifactContractError::Limit` only for their matching artifact kind and boundary.

`ReferenceArtifacts`, `DefinitionArtifacts`, `PatternMatchStatesTotal`, `FindingsTotal`, `FindingBytesTotal`, `BundlePlansTotal`, `ResolvedMessagesTotal`, and `BundlePlanBytesTotal` require `LinkLimitSubject::Request`.

`PatternMatchStatesTotal` never accepts a reference-artifact group, reference-record identity, candidate key, pattern, matcher state, worker identity, or another subject.

The two finding-result counters never retain a finding kind, subject, evidence, canonical position, blocking disposition, target, reporter, or another result fragment. The three plan-result counters never retain a delivery unit, locale, resolved message, definition location, payload, plan position, exporter, target, worker, or another plan fragment.

Each of the other three reference variants requires the exact selected `ReferenceArtifactGroup`; `DefinitionsTotal` and `DefinitionArtifactDecodedBytesTotal` require the exact selected `DefinitionArtifactGroup`.

`EntryStructuralPathBytes`, `CatalogKeyBytes`, `MessageBytes`, and `TotalMessageBytes` also require exactly `DefinitionArtifactGroup(source)` and never retain a raw field, `EntryReference`, definition index, occurrence index, or an arbitrary duplicate artifact occurrence.

`CatalogScopeNameBytes` requires the exact owning `ReferenceArtifactGroup(identity)`, `DefinitionArtifactGroup(source)`, `ResolvedPolicy`, or payload-free `ScopeMappings` subject according to whether the occurrence belongs to a reference artifact, definition artifact, policy/completeness input, or scope-mapping endpoint; it never accepts `Request`, a raw `CatalogScopeName` or `CatalogScopeId`, a record/mapping index, or another subject.

`ScopeMappingEntries` also requires exactly the payload-free `ScopeMappings` subject; it never accepts `Request`, an individual mapping, an endpoint, or an entry index.

`SelectorPathBytes`, `SelectorPatternBytes`, `SelectorPatternTokens`, and `ReasonBytes` each require exactly the owning `ReferenceArtifactGroup(identity)` established before record validation; they never accept `Request`, a raw selector, pattern, token, or reason, a record ordinal, or another subject.

`PathSegments`, `PathSegmentBytes`, and `PathBytes` use exactly the payload-free `DefinitionArtifactEnvelope` when validating a definition artifact's primary source path, exactly `DefinitionArtifactGroup(source)` for a definition alias after that primary identity is checked, and exactly `ReferenceArtifactGroup(identity)` for a reference origin.

Primary-path validation keeps `DefinitionArtifactEnvelope` even when a cache or defensive revalidation route already knows the eventual source, so every route selects identical evidence.

These path counters never accept `Request`, a raw or partial path or segment, an alias or record index, or another subject; no other currently fixed counter accepts `DefinitionArtifactEnvelope`.

`LogicalAliases` and `SourcePathBytes` require exactly the `DefinitionArtifactGroup(source)` established by the checked primary path. They never accept `Request`, `DefinitionArtifactEnvelope`, `ReferenceArtifactGroup`, a raw alias or path, an alias index, or another subject.

`DeliveryGraphNodes` and `DeliveryGraphEdges` require `LinkLimitSubject::DeliveryGraph`, while `DeliveryGraphIdBytes` requires the exact selected `DeliveryUnitGroup`.

`ProductionLocales`, `FallbackSources`, and `ConfiguredRoots` require `LinkLimitSubject::ResolvedPolicy`; `FallbackTargetsPerSource` requires the exact checked and unique `FallbackSource(locale)` selected by the canonical source-order pass.

`LocaleBytes` requires `DefinitionArtifactGroup(source)` for a definition occurrence and `ResolvedPolicy` for any production, fallback-source, or fallback-target occurrence; it never accepts `Request`, `FallbackSource`, a raw locale, or an occurrence index.

A later locale- or scope-bearing context must add its one bounded owning subject explicitly rather than reuse an unrelated subject. Every other mismatched counter/subject pair is unconstructible.

`Exact(attempted)` stores the exact first attempted total and is constructible only when `attempted > effective_limit`.

##### Limit observations

For `LocaleBytes`, `EntryStructuralPathBytes`, `CatalogKeyBytes`, `MessageBytes`, `CatalogScopeNameBytes`, `ReasonBytes`, and `PathSegmentBytes`, `attempted` is always exactly `effective_limit + 1`; every valid effective limit is at most `67,108,864`, so `ArithmeticOverflow` is unconstructible for those counters.

Streaming decode stops field-byte accounting at that first attempted byte and does not retain, re-decode, or scan the remainder merely to compute a complete submitted length; direct construction and cache revalidation report the same canonical observation even when a complete length is already available.

For `FindingsTotal`, `BundlePlansTotal`, and `ResolvedMessagesTotal`, `attempted` is always exactly `effective_limit + 1`. Each final candidate count is checked against its bounded effective limit before proportional result allocation, so a known larger count is not exposed and `ArithmeticOverflow` is unconstructible.

For `FindingBytesTotal` and `BundlePlanBytesTotal`, `attempted` is the exact checked running total immediately after adding the complete variable-length semantic payload that first crosses the effective limit. Accounting stops there and never scans later finding or plan payloads merely to report a larger total.

Every individual addend has already passed its owning field or identity ceiling. The prior accepted finding total is at most 256 MiB, and the prior accepted plan total is at most 1 GiB. The first attempted total is therefore bounded; host-size conversion failure and `ArithmeticOverflow` are unconstructible for all five linker-result counters.

`ScopeMappingEntries`, `PathSegments`, and `LogicalAliases` follow the same canonical first-over rule: known-length construction compares the bounded effective limit without converting or reporting the complete collection length, streaming construction stops before retaining the first excess entry, segment, or alias, and all three return exactly `Exact(effective_limit + 1)`.

Their effective limits are at most 4,096, 1,024, and 4,096 respectively, so `ArithmeticOverflow` is unconstructible for all three.

`SelectorPathBytes`, `SelectorPatternBytes`, and `SelectorPatternTokens` also always return exactly `Exact(effective_limit + 1)`: selector-string decoding stops at the first excess decoded byte, token parsing stops before retaining the first excess token, and direct construction or cache revalidation does not substitute a complete value length or token count.

Their effective limits are at most 67,108,864 bytes, 134,217,728 bytes, and 513 tokens respectively, so `ArithmeticOverflow` is unconstructible for all three. `PathBytes` records the exact checked per-path running total after adding the complete current segment; it never substitutes `effective_limit + 1`.

Because `PathSegmentBytes` has already limited the addend to 4,096 bytes and the prior accepted total is at most 262,144 bytes, the first rejected attempted total is at most `266,240` and `ArithmeticOverflow` is unconstructible.

`SourcePathBytes` records the exact checked definition-artifact running total after adding one complete primary or alias path; it also never substitutes `effective_limit + 1`.

Because `PathBytes` has already limited each addend to 262,144 bytes and the prior accepted total is at most 67,108,864 bytes, the first rejected attempted total is at most `67,371,008` and `ArithmeticOverflow` is unconstructible.

`TotalMessageBytes` similarly records the exact checked running total after adding the complete current message.

Because the per-message pass has already limited that addend to 1 MiB and the prior accepted total is at most 64 MiB, its first rejected attempted total is at most `68,157,440`; `ArithmeticOverflow` is therefore also unconstructible for `TotalMessageBytes`.

`PatternMatchStatesTotal` likewise records the exact checked request total after atomically adding the complete reachable-state count of the current canonical evaluation; it never substitutes `effective_limit + 1`.

The prior accepted total is at most 100,000,000 and one evaluation contributes at most 132,098, so the first rejected attempted total is at most `100,132,098` and `ArithmeticOverflow` is unconstructible for this counter.

For other counters, `ArithmeticOverflow` is used only when checked length conversion, group-subtotal addition, or request-total addition cannot produce an exact `u64` attempted value.

It never substitutes `u64::MAX`, saturation, wrapping, a decimal string, or an arbitrary-precision value.

`effective_limit` is the exact immutable limit applied to this invocation. It is either a valid caller-selected lower value or the protocol ceiling when no lower value was selected.

The evidence does not redundantly store protocol ceiling, configured override, remaining budget, allocation size, or a second attempted representation; the protocol ceiling is derived from the closed counter.

For `ReferenceArtifactWireBytes` and `DefinitionArtifactWireBytes`, `observation` is always exactly `Exact(effective_limit + 1)`. A known larger document length is not substituted, and a streaming decoder stops after reading the first excess byte.

For `ReferenceArtifactDecodedBytes` and `DefinitionArtifactDecodedBytes`, `observation` is instead the exact checked running total after adding the complete admitted payload that first crosses the effective ceiling; it is never replaced by `effective_limit + 1`.

Every addend has already passed a fixed field/shape ceiling no greater than 128 MiB and the prior accepted total is at most 256 MiB, so the first attempted total is bounded and `ArithmeticOverflow` is unconstructible for all four per-artifact counters.

Implementations compare a known or host-sized length against the bounded ceiling before conversion and report the canonical first-over observation rather than using conversion overflow to expose a route-specific result.

There is deliberately no linker-owned `reference_artifact_wire_bytes_total` counter. Wire bytes exist only at serialized transport or ingestion boundaries, and direct typed construction has no honest wire length to contribute.

An integration that accepts a serialized batch or stream must bound its total decompressed input, buffering, concurrency, and cancellation under that integration's transport contract while still enforcing `reference_artifact_wire_bytes` for every individual document.

Such a transport budget is not part of `LinkLimits`, artifact identity or versioning, `InputFingerprint`, link semantics, or linker cache admission, and the core never synthesizes it from canonical re-encoding.

#### M0 reference-artifact JSON schema

##### Root envelope

The root object has the required wire-only discriminator `"kind":"message-reference"`.

A typed reference decoder compares the decoded string exactly and rejects a missing, duplicate, non-string, differently cased, aliased, unknown, or `"message-definition"` value rather than normalizing or dispatching it as a reference.

After root syntax and duplicate-member checks, it validates `kind` before `ArtifactVersion` compatibility and the remaining schema. A generic artifact decoder may use the same exact discriminator to select the typed decoder, and generic and direct decoding must agree.

Successful decoding returns `MessageReferenceArtifact` without storing a redundant kind field.

For v0.1 the root contains exactly six required, non-null members. Their canonical emission order is `kind`, `version`, `producer`, `identity`, `deliveryUnit`, and `references`; the first bytes after `{` are therefore exactly `"kind":"message-reference"`.

Input object-member order is non-semantic. `references` is an array whose order remains semantic and is emitted without sorting because its zero-based positions define `ReferenceRecordIdentity.ordinal`; an empty array is valid. No record carries or accepts a redundant serialized `ordinal`.

A missing, duplicate, unknown, mistyped, flattened, or `null` root member rejects the complete artifact.

`version` uses the shared exact `{"major":0,"minor":1}` bootstrap codec, `producer` uses the shared exact `{"id":...,"revision":...} ` `ProducerIdentity` codec, and `identity` uses the `{"namespace":...,"segments":[...]}` codec fixed above.

Canonical nested member order is respectively `major` then `minor`, `id` then `revision`, and `namespace` then `segments`. `deliveryUnit` uses the required non-empty logical segment-array `DeliveryUnitId` codec fixed above.

##### Reference record envelope

Each `references` element is exactly one `MessageReference` object. `scope`, `domain`, and `selector` are required, non-null members carrying the checked `CatalogScopeId`, `CatalogKeyDomain`, and `MessageSelector` codecs. `reason` and `origin` are optional members carrying checked `ReasonText` and `SourceOrigin` values.

Absence is represented only by omitting the member; a present `null` is invalid and is never treated as `None`. The object contains no `ordinal`, `deliveryUnit`, producer, artifact identity, `metadata` wrapper, flattened extension, or unknown member.

Input member order is non-semantic. Canonical emission always begins `scope`, `domain`, `selector`, then emits `reason` when present and `origin` when present.

The four exact canonical member sequences are therefore `(scope, domain, selector)`, `(scope, domain, selector, reason)`, `(scope, domain, selector, origin)`, and `(scope, domain, selector, reason, origin)`.

A conforming writer never emits an absent optional member as `null`, an empty placeholder object, or a default string. Missing required members, duplicate members, unknown members, a mistyped required or present optional value, and `null` at any record member reject the complete reference artifact.

`reason` may be present with any selector kind. A non-`Exact` producer provides it when it has a real declaring source and never fabricates one. `origin` is independently optional, including when a producer supplies coordinates through a separate debug sidecar.

Neither optional field is inferred from the other, the selector, producer, delivery unit, or array position.

Presence and exact checked value participate in typed artifact equality, canonical bytes, decoded/wire accounting, cache input, and finding evidence, but not in `ReferenceRecordIdentity`; changing either at an unchanged artifact identity and ordinal changes record evidence without minting a new record identity.

##### Reason text

`ReasonText` is human-readable producer evidence represented by one JSON string and retained as an exact Unicode scalar sequence. Its decoded UTF-8 length is from 1 through 4,096 bytes inclusive.

Empty is invalid rather than another spelling of absence; absence remains omission of the `reason` member. Length is measured after JSON unescaping and before constructing the checked newtype, in bytes rather than scalars, grapheme clusters, display columns, or serialized escape bytes.

Horizontal tab (`U+0009`), line feed (`U+000A`), and carriage return (`U+000D`) are the only permitted C0/C1 control characters.

The decoder rejects `U+0000..U+0008`, `U+000B`, `U+000C`, `U+000E..U+001F`, `U+007F`, and `U+0080..U+009F` wherever they occur, including through JSON escapes. All other Unicode scalar values are allowed.

The value is not trimmed, Unicode-normalized, case-folded, newline-normalized, localized, Markdown-parsed, or otherwise rewritten; a whitespace-only but non-empty value remains syntactically valid, and CRLF, LF, and CR spellings remain distinct exact values.

Presentation layers escape unsafe display contexts without mutating the stored evidence.

`reason_bytes` is the field-specific inclusive `LinkLimits` counter with a fixed protocol ceiling of 4 KiB (`4,096` decoded UTF-8 bytes) per present reason. Its public evidence variant is the twenty-sixth common counter, `ReasonBytes`.

Exactly 1 and 4,096 bytes are accepted; the first decoded byte above rejects the complete reference artifact with the established `ReferenceArtifactGroup(identity)` and exactly `Exact(effective_limit + 1)`.

Bounded first-over admission makes length-conversion failure and `ArithmeticOverflow` unconstructible for this counter. A decoded empty string, invalid scalar/control, or non-string JSON value instead fails the later applicable grammar or shape rule.

Streaming decoders count decoded bytes and reject before retaining an over-limit value; direct construction, cache revalidation, and producer paths select the same evidence without a raw reason, complete length, or record ordinal.

Repeated equal reasons are charged independently to `reference_artifact_decoded_bytes` without interning deductions. A caller-selected lower value follows the common immutable-budget rule; zero permits only omission and never coerces a present reason to absence.

No layer truncates, replaces, hashes, sanitizes, or drops an invalid reason to recover.

The linker never parses `ReasonText` as a policy, selector, code, locale, or control instruction. It carries the exact value into applicable finding evidence and structured output.

Presence and bytes affect artifact equality and cache input but not selector matching, reachability, record identity, or finding disposition. A future machine-readable reason code would be a separately versioned typed field rather than an overloaded text convention.

##### Source origin

`SourceOrigin` is optional diagnostic evidence represented by exactly one object containing `source` and `span`. Its canonical JSON shape is, for example, `"origin":{"source":{"namespace":{"kind":"project"},"path":["src","checkout.ts"]},"span":{"start":128,"end":146}}`.

Input member order is non-semantic; canonical emission orders `source` before `span`, the nested `SourceDocumentIdentity` orders `namespace` before `path`, and the span orders `start` before `end`. Missing, duplicate, unknown, mistyped, or `null` members reject the complete reference artifact.

`source` uses exactly the shared `SourceDocumentIdentity` type and codec used by a definition artifact's catalog document: the explicit M0 `Project` namespace plus one non-empty `PortableRelativePath`.

It does not inherit the reference artifact's namespace; an aggregate producer supplies the exact project-relative identity for each origin independently. The same current-project binding applies to the origin.

Reusing one source identity across any number of records is valid evidence and does not conflict with the rule that definition artifacts have unique envelope source identities. Published-package origins are unavailable until the deferred namespace extension lands.

`span` is one exact object containing unsigned lexical JSON integers `start` and `end`, both represented as `u32`. It denotes the half-open UTF-8 byte range `[start, end)` in the exact valid-UTF-8 source document analyzed by the producer.

The invariant is `start <= end`; an empty range is valid, including `0..0` and an end-of-document caret. Offsets count source bytes, not Unicode scalars, UTF-16 code units, grapheme clusters, lines, display columns, AST nodes, instructions, or bytes in an object file.

Signed, fractional, exponent, string, overflowing, reversed, missing, duplicate, unknown, and `null` forms are invalid.

The producer validates both endpoints against the exact analyzed source length and at UTF-8 scalar boundaries before constructing the origin.

A standalone artifact decoder can validate integer width and ordering but cannot revalidate those external-source facts because source bytes and a source digest are intentionally absent.

When a host resolves the identity to source bytes, it may use the location only if those bytes make the range valid; a stale, unavailable, out-of-bounds, or split-scalar mapping remains unavailable diagnostic evidence and is never clamped, shifted, widened, or converted into a linker failure.

A source location whose endpoint cannot be represented by `u32`, or a producer with only binary/object offsets and no exact UTF-8 source mapping, normally omits `origin` rather than encoding another coordinate system in this type. A producer-specific profile may instead require origin for every locally emitted record; that producer fails its complete operation when it cannot construct the required checked range.

No host path, URI, line/column pair, source excerpt, source digest, language id, symbol, or producer-specific payload is serialized.

A presentation layer derives line and column from the resolved exact source bytes and chooses display-base conventions outside the artifact contract; line/column never participates in equality, ordering, cache input, or record identity.

`SourceOrigin` remains evidence only: changing it changes typed artifact equality, canonical bytes, cache input, and applicable finding evidence, but never selector matching, reachability, disposition, or `ReferenceRecordIdentity`.

Every origin path independently obeys the shared `path_segment_bytes`, `path_segments`, and `path_bytes` ceilings and any caller-selected lower values.

Its path payload is charged per occurrence to `reference_artifact_decoded_bytes` without deductions for repeated identities, shared segments, interning, or zero-copy storage; actual JSON spelling contributes to `reference_artifact_wire_bytes`. The two fixed-width `u32` values add no variable-width decoded scalar bytes.

There is no separate free-form `origin_bytes` field or source-origin string budget. A lower path budget of zero therefore permits only omission of `origin`, never a truncated or pathless coordinate.

The M0 `CatalogScopeId` object shape, host mapping semantics, `CatalogScopeName` byte ceiling, `CatalogKeyDomain` codec, and `MessageSelector` tagged-object envelope are fixed below. The `Prefix` representation, inclusive root semantics, and empty-root rejection are also fixed below.

`Exact.key` and `Prefix.prefix` share the fixed 64 MiB `selector_path_bytes` ceiling defined below.

`Pattern` matching is fixed over complete parsed structural token sequences with a non-empty payload, the complete-token `*` and `**` operators, the pattern-only `~2` literal-asterisk escape, and rejection of adjacent `**` operators as defined below.

It uses the separate per-value `selector_pattern_bytes` counter with a fixed 128 MiB ceiling, a fixed 513-token ceiling, and the iterative NFA/DP evaluation model below, with one logical work unit per distinct reachable state pair and a derived per-evaluation maximum of 132,098 units.

Its inclusive request-level `pattern_match_states_total` ceiling is fixed at 100,000,000 logical states, using the canonical request-wide accounting defined below.

A future compatible minor may add an optional member only with omission as an unambiguous older-version default and a declared canonical position; v0.1 decoders reject it as unknown.

Making an optional member required, changing omission semantics, accepting `null` as equivalent to absence, or moving existing data behind a wrapper is breaking.

The strict decoder acceptance and canonical-emission rules stated for the definition JSON wire below apply verbatim:

- one root followed only by permitted JSON whitespace and EOF;
- no BOM, comments, trailing comma, or extra root value;
- duplicate rejection at every object depth;
- no unknown field under v0.1;
- Unicode-scalar strings without normalization;
- lexical, width-checked unsigned integers;
- non-semantic object-member order;
- semantic array order;
- no insignificant whitespace or final newline in canonical output;
- shortest integer spelling; and
- the same exact string escaping.

Accepted noncanonical member placement, whitespace, or equivalent string escapes re-emit as the single canonical spelling and decode to the same typed artifact.

The discriminator and every other actual JSON byte contribute to `reference_artifact_wire_bytes`. Because `kind` is not retained in the typed artifact, its fixed decoded constant contributes zero to `reference_artifact_decoded_bytes`.

It is not producer provenance, artifact or record identity, a semantic link input, a finding field, or a plan field.

Changing the root member set, requiredness, field meaning, or canonical name follows `ArtifactVersion` compatibility rules; an alternative transport cannot obtain compatibility merely by emitting equivalent values.

#### Message key domains and selector semantics

A message key is **not restricted to an object path** and has no universal dot-separated format. The linker-facing identity is the opaque, canonical `CatalogKey` defined by 013 and interpreted only together with its `CatalogKeyDomain`.

For example, JSON uses RFC 6901 JSON Pointer (`/checkout/title`), YAML uses a typed path (`/k:str:checkout/k:str:title`), and XLIFF uses its unit hierarchy. A literal JSON member named `checkout.title` canonicalizes to `/checkout.title`, which is distinct from the nested JSON path `/checkout/title`.

M0 encodes `CatalogKeyDomain` as exactly one of four closed, lowercase ASCII JSON strings:

| Canonical order | `CatalogKeyDomain` variant | Exact JSON string   |
| --------------: | -------------------------- | ------------------- |
|               0 | `JsonPointer`              | `"json-pointer"`    |
|               1 | `YamlTypedPath`            | `"yaml-typed-path"` |
|               2 | `Xliff12`                  | `"xliff-1.2"`       |
|               3 | `Xliff2`                   | `"xliff-2"`         |

The decoder compares the decoded string exactly and rejects `StandaloneMf2`, including a hypothetical `"standalone-mf2"` spelling, because 013 defines it as non-comparable metadata that never enters catalog-level grouping.

It likewise rejects an empty string, `null`, case variants, surrounding whitespace, aliases such as `"json_pointer"`, unknown strings, numbers, and object forms without trimming or normalization. Canonical emission uses the table's exact token.

Equality uses the typed variant, canonical ordering uses the table order, and the canonical fingerprint payload is the exact ASCII token bytes.

The actual escaped JSON token contributes to `reference_artifact_wire_bytes`; the checked enum has no variable decoded scalar payload and therefore adds zero to `reference_artifact_decoded_bytes`.

Because the accepted set is closed and its longest token is 15 bytes, M0 has no caller-adjustable `catalog_key_domain_bytes` limit. Adding a built-in domain requires a coordinated 013 semantic contract, artifact schema/version decision, canonical ordering and fingerprint update, and conformance fixtures.

A third-party domain cannot claim an arbitrary token under v0.1.

Source spellings such as `t('checkout.title')`, `message!(" checkout.title")`, or `useMessageSet('errors.*')` belong to a configured runtime API or producer recognizer; they are not artifact key syntax.

Before emitting an artifact, the producer must resolve the target `scope` and `domain` and canonicalize the source spelling under that runtime-key contract.

If the spelling cannot be mapped unambiguously — for example, whether `.` is a literal character or a path separator is unspecified — artifact production fails instead of guessing.

##### Message selector wire envelope

M0 encodes every `MessageSelector` variant as one internally tagged JSON object. It has no bare-string shortcut, externally tagged form, or generic payload member:

| Canonical order | Variant            | Exact member set and canonical emission           |
| --------------: | ------------------ | ------------------------------------------------- |
|               0 | `Exact`            | `{"kind":"exact","key":"/checkout/title"}`        |
|               1 | `Prefix`           | `{"kind":"prefix","prefix":"/checkout"}`          |
|               2 | `Pattern`          | `{"kind":"pattern","pattern":"<domain-pattern>"}` |
|               3 | `AllInScope`       | `{"kind":"all-in-scope"}`                         |
|               4 | `UnboundedDynamic` | `{"kind":"unbounded-dynamic"}`                    |

Input object-member order is non-semantic; canonical emission always writes `kind` first and then the variant-specific payload when one exists. `kind` is required, non-null, and compared as the exact lowercase ASCII token shown above.

`Exact` requires exactly one additional non-null string member `key`, `Prefix` requires exactly `prefix`, and `Pattern` requires exactly `pattern`. `AllInScope` and `UnboundedDynamic` contain no payload member.

A missing, duplicate, unknown, mistyped, or `null` member rejects the complete artifact. A payload member belonging to another variant is unknown rather than ignored.

A bare key string, `{"exact":"..."}`, `{"kind":"exact","value":"..."}`, an array, an integer tag, and placing `reason` inside the selector are invalid alternate forms. `reason` remains the independent optional `MessageReference` member.

The illustrative `Pattern` string in the table fixes only its object envelope and field name; the `Prefix` representation is fixed below.

Selector equality compares the typed variant and then its exact validated payload when present. Canonical ordering uses the table order and then the exact canonical payload bytes; the `Prefix` representation and `Pattern` grammar/order are fixed below.

The fixed kind token adds zero variable decoded scalar bytes. `key` and `prefix` contribute their decoded UTF-8 bytes to the shared `selector_path_bytes` check and `reference_artifact_decoded_bytes`; `pattern` contributes to `selector_pattern_bytes` and `reference_artifact_decoded_bytes`.

The actual JSON spelling contributes to `reference_artifact_wire_bytes`. `AllInScope` and `UnboundedDynamic` add no variable decoded payload.

Because the five kind tokens are a closed set and the longest is 17 bytes, M0 has no caller-adjustable `selector_kind_bytes` limit. Adding a selector variant requires coordinated artifact schema/version, ordering, accounting, and conformance changes.

`Exact.key` carries one canonical `CatalogKey`.

`Prefix.prefix` and `Pattern.pattern` are also domain-qualified and match domain-defined key or token boundaries. They are not raw prefix or glob operations over the serialized `CatalogKey` string.

`intlify_contract` owns their normative specification for every supported `CatalogKeyDomain`. That contract covers tokenization, equality and boundary behavior, escaping, pattern operators, normalization (none unless the domain explicitly specifies it), invalid-selector rejection, and conformance fixtures.

##### Prefix payload representation

`Prefix.prefix` is one canonical string interpreted under the `CatalogKeyDomain` already selected by its enclosing reference.

It reuses that domain's `CatalogKey` path serialization, escaping, and structural-token vocabulary, but may terminate at any valid token boundary that can be an ancestor of a complete `CatalogKey`.

It therefore does not introduce a token-array wire form, and it is not a display key, host path, source-language key spelling, or arbitrary byte prefix. A structurally valid prefix need not itself identify an existing message definition.

The decoder dispatches through the selected domain contract, validates the complete string as a canonical structural prefix path, and retains its exact UTF-8 bytes. It rejects a noncanonical spelling rather than normalizing it.

For matching, `intlify_linker` parses both the prefix and candidate `CatalogKey` into the domain's structural token sequence and performs an inclusive sequence-prefix comparison at token boundaries: a candidate matches when its token sequence equals the prefix token sequence or has that sequence as a proper structural prefix.

It never applies a string `starts_with` operation to their serialized bytes. `Exact` continues to select only one complete key, whereas `Prefix` selects that root key plus its structural descendants.

For example:

- JSON prefix `/checkout` matches both `/checkout` and `/checkout/title` but not `/checkout2`; RFC 6901 `~0` and `~1` are decoded as part of token parsing before comparison.
- YAML prefix `/k:str:checkout` is structurally ancestral to `/k:str:checkout/k:str:title` but not `/k:str:checkout2`; typed segments keep mapping keys and sequence indices distinct.
- XLIFF prefix `/file:original:app.json/group:id:menu` is structurally ancestral to `/file:original:app.json/group:id:menu/unit:id:welcome`; the XLIFF domain's typed hierarchy and escaping remain authoritative.

`Prefix.prefix` must be non-empty in every domain. The decoder rejects `{"kind":"prefix","prefix":""}` even where an empty string is the canonical catalog root, because an inclusive empty token sequence would select every key and duplicate `AllInScope`.

Callers use `AllInScope` to state that broad intent explicitly. Where a domain admits an actual root message with the empty `CatalogKey`, `Exact.key` remains able to select only that root; `Prefix` does not provide an alias for either behavior.

`Exact.key` and `Prefix.prefix` share the twenty-second common `LinkLimitCounter` variant, `SelectorPathBytes`, whose exact structured spelling is `selector_path_bytes`. Its inclusive fixed protocol ceiling is 64 MiB (`67,108,864` decoded UTF-8 bytes) per payload.

It checks the decoded UTF-8 length of each selector payload independently after JSON unescaping and before retaining or structurally parsing the value. There is no separate `exact_key_bytes` or `prefix_bytes` limit: both canonical path forms have the same effective protocol ceiling and caller-selected lower value.

A syntactically valid path exactly at the effective ceiling is admitted; the first decoded byte above it rejects the complete reference artifact with `SelectorPathBytes`, the established `ReferenceArtifactGroup(identity)`, and exactly `Exact(effective_limit + 1)`.

It never retains the raw selector or record ordinal, scans for a complete length, or produces `ArithmeticOverflow`.

Their bytes also contribute per occurrence to `reference_artifact_decoded_bytes`, without deductions for equal values, interning, shared prefixes, or zero-copy storage; serialized escaping remains part of `reference_artifact_wire_bytes` instead.

Streaming decoders, direct checked constructors, and producers apply the same check without truncation, hashing, replacement, or selector widening. A lower value of zero admits only a valid empty `Exact.key` in a domain whose `CatalogKey` grammar permits it and rejects every `Prefix`.

Under protocol-default limits, the 64 MiB ceiling aligns with 013's artifact-wide `identity_bytes` ceiling, so any single `CatalogKey` admitted by 013 fits this field check; `reference_artifact_decoded_bytes` and any caller-selected lower limit remain independent admission checks.

##### Pattern structural matching domain

###### Structural tokens and operators

`Pattern.pattern` is evaluated against the parsed structural token sequence of a candidate `CatalogKey` in the reference's selected `CatalogKeyDomain`. It is never a glob, regular expression, or substring operation over the serialized canonical path bytes.

Pattern operators may occupy only complete structural-token positions; no operator can consume a byte, Unicode scalar, or substring inside one literal token.

Consequently, a pattern that selects the JSON token `checkout` does not partially select `checkout2`, YAML typed key tokens remain distinct from sequence-index tokens, and XLIFF hierarchy tokens retain their typed boundaries.

M0 has exactly two pattern operators:

| Operator token | Structural consumption                                                   |
| -------------- | ------------------------------------------------------------------------ |
| `*`            | Exactly one complete candidate token of any domain-defined token kind.   |
| `**`           | Zero or more complete candidate tokens of any domain-defined token kind. |

An operator is recognized only when its spelling occupies one complete pattern-token position after domain path segmentation. `*` never matches zero or two tokens, and `**` may consume an empty sequence. Neither operator inspects or partially matches a token payload.

No question mark, character class, alternation, brace expansion, capture, backreference, lookaround, or regular-expression construct has operator semantics in M0; whether such characters are valid literal payload is determined only by the domain literal grammar.

Adding another operator requires an artifact compatibility, canonical grammar, complexity, and conformance decision rather than treating a formerly literal or invalid spelling as an implicit extension.

###### Canonical syntax and escaping

Pattern strings reuse the selected domain's slash-separated canonical path and structural-token grammar, with one pattern-only extension to the token escape table:

| Raw character in a literal structural token | Canonical pattern escape |
| ------------------------------------------- | ------------------------ |
| `~`                                         | `~0`                     |
| `/`                                         | `~1`                     |
| `*`                                         | `~2`                     |

Encoding applies this table directly to raw token characters; it does not escape a previously serialized string in a second pass. Decoding scans left to right and accepts only `~0`, `~1`, and `~2`; a dangling `~`, any other `~X`, percent escape, or backslash escape is invalid.

JSON string escaping remains a separate outer wire concern and does not introduce pattern semantics.

After splitting at unescaped `/`, a segment spelled exactly `*` is the one-token operator and a segment spelled exactly `**` is the zero-or-more operator.

Every asterisk in a literal segment must be `~2`; consequently, any other segment containing a raw `*`, such as `a*` or `***`, is rejected rather than interpreted as a partial wildcard or noncanonical literal. `~2` is `Literal("*")`, and `~2~2` is `Literal("**")`.

A raw token `~2` canonicalizes to `~02`, so it remains distinct from a literal asterisk. The decoder then validates each decoded literal as one token in the selected domain grammar.

For example, the checked pattern tokens `[Literal("checkout"), *]` canonically serialize as `/checkout/*` in the JSON Pointer domain. JSON literal key `*` serializes as `/~2` in a pattern, and `a/b~c*` serializes as `/a~1b~0c~2`.

A YAML literal string-key token for `*` is `/k:str:~2`, while `/*` means any one complete YAML typed token. XLIFF uses the same escape inside its typed hierarchy segments.

This `~2` extension belongs only to `Pattern.pattern`: an `Exact.key` or `Prefix.prefix` continues to use the unmodified domain `CatalogKey` spelling, so the JSON literal key `*` remains `/*` there.

###### Empty patterns and adjacent globstars

`Pattern.pattern` must be non-empty in every domain. The decoder rejects `{"kind":"pattern","pattern":""}` rather than interpreting the empty token sequence as a root-only selector, an implicit `**`, or `AllInScope`.

Where a domain admits an actual root message with the empty `CatalogKey`, callers use `Exact.key` with that domain-valid empty key to select only the root. Callers use `AllInScope` for an intentional complete scope-domain selection.

No decoder rewrites an empty pattern into either form.

Two `**` operator tokens may not be adjacent. Any parsed run of two or more consecutive `**` tokens is noncanonical and rejects the complete artifact; the decoder does not silently collapse the run.

For example, `/**/**/title` is rejected and a producer emits the single equivalent `/**/title` instead.

This rule applies after operator/literal disambiguation, so `/~2~2/~2~2` contains two literal `**` keys and remains valid when the domain admits those tokens. `*/*` remains valid because it consumes exactly two tokens, and `**/*/**` remains valid because the two multi-token operators are not adjacent and the middle `*` requires at least one candidate token.

###### Pattern byte limits and accounting

`Pattern.pattern` has the twenty-third common `LinkLimitCounter` variant, `SelectorPatternBytes`, whose exact structured spelling is `selector_pattern_bytes`. Its inclusive fixed protocol ceiling is 128 MiB (`134,217,728` decoded UTF-8 bytes) per payload.

It measures the decoded UTF-8 byte length of the complete canonical pattern string after JSON unescaping and before path segmentation, pattern-escape decoding, domain-literal validation, or matcher compilation.

The bytes of `/`, `*`, `**`, `~0`, `~1`, and `~2` therefore count exactly as they appear in that decoded canonical string. JSON quotes, JSON escape expansion, and other serialized spelling belong only to `reference_artifact_wire_bytes`.

A valid canonical pattern exactly at the effective ceiling is admitted; the first decoded byte above it rejects the complete reference artifact with `SelectorPatternBytes`, the established `ReferenceArtifactGroup(identity)`, and exactly `Exact(effective_limit + 1)`.

It never retains the raw pattern or record ordinal, scans for a complete length, or produces `ArithmeticOverflow`.

Each pattern occurrence is checked independently and contributes the same canonical-string length to `reference_artifact_decoded_bytes`. Equal patterns, shared literal tokens, interning, compiled-matcher reuse, and zero-copy storage provide no deduction.

A streaming decoder counts decoded JSON-string bytes with checked arithmetic before retaining an over-limit value; a producer and direct checked constructor apply the same check before pattern parsing or compilation. No layer truncates, hashes, replaces, splits, drops, or widens an over-limit pattern.

A caller-selected lower immutable value follows the common `LinkLimits` rule; because `Pattern.pattern` is non-empty, a lower value of zero rejects every `Pattern` while leaving the other selector variants unaffected.

The 128 MiB ceiling preserves literal-pattern representability for every single `CatalogKey` admitted by 013's 64 MiB artifact-wide `identity_bytes` ceiling.

Re-encoding one canonical domain key as a literal-only pattern leaves separators and the existing `~0` / `~1` escapes unchanged and can at most replace each one-byte literal `*` with the two-byte `~2`; therefore its canonical pattern is strictly less than or equal to twice the key's canonical byte length.

This alignment does not bypass `reference_artifact_decoded_bytes`, pattern-complexity limits, or a caller-selected lower value.

There is no `pattern_literal_bytes`, `pattern_operator_bytes`, or shared use of `selector_path_bytes`. Token and operator counts required to bound matching work are independent complexity counters rather than deductions from this byte accounting.

Selector equality and canonical ordering use the exact validated canonical pattern-string bytes after the fixed variant order.

###### Matching execution model

Pattern evaluation uses an iterative finite-state NFA/DP model over checked token arrays. For a pattern with `m` tokens and candidate with `n` tokens, a state is one pair `(pattern_position, candidate_position)` in `0..=m × 0..=n`. Evaluation starts at `(0, 0)` and accepts only `(m, n)`. Transitions are exactly:

- a matching literal advances to `(i + 1, j + 1)`;
- `*` advances to `(i + 1, j + 1)` when one candidate token exists;
- `**` has an epsilon transition to `(i + 1, j)` for zero tokens and a consuming transition to `(i, j + 1)` when one candidate token exists.

An iterative worklist or row-wise active-state implementation marks each reachable state pair at most once.

Candidate positions are processed in ascending order and pattern positions in ascending order within one candidate position, making work accounting and the first limit failure independent of hash iteration, thread scheduling, or transition insertion order.

The matcher performs no recursive descent, path enumeration, speculative capture, or exponential backtracking. Its worst-case logical time is `O((m + 1) × (n + 1))`; a row-wise implementation retains `O(m + 1)` active/visited state storage rather than an unbounded search tree.

Checked addition and multiplication are required before allocating state storage or deriving a work bound.

One distinct reachable state pair is exactly one logical pattern-match work unit. `(0, 0)` always contributes the first unit.

A pair contributes when it is first admitted to the canonical row-ordered worklist, before its outgoing transitions are evaluated; several incoming transitions to the same pair add no further units.

The accepting `(m, n)` pair contributes when reachable, while every unreachable cell in the theoretical state rectangle contributes zero. A mismatch therefore charges only the reachable prefix of the computation, not the full `(m + 1) × (n + 1)` rectangle.

Checked `u64` addition records the complete exact per-evaluation count. M0 has no independent effective work budget inside one evaluation; request-level admission atomically attempts to add that complete count only after the evaluation finishes.

The unit covers the fixed control decision and at most one exact domain-token equality test associated with that state.

Transition fan-in, epsilon-edge discovery, queue operations, allocation capacity, wall-clock time, machine instructions, thread count, and compiled representation do not create additional logical units.

Artifact byte and token limits independently bound the data compared and compiled; future index-construction limits remain separate from pattern-state accounting rather than being hidden in this unit.

Literal transitions use exact decoded domain-token equality. `intlify_linker` may reuse an immutable compiled pattern or use an equivalent trie/batched traversal, but the observable match result and logical-work accounting must be identical to this model.

After the logical evaluations defined below have been formed, every evaluation is charged its conceptual reachable-state count independently.

A cache hit, an equal pattern in another reference record, a repeated evaluation of the same key by another reference record, interning, shared trie prefixes, SIMD/bitset processing, and parallel execution provide no deduction. Optimization cannot change admission, findings, plans, or the selected limit failure.

Exceeding a complexity budget is a fail-complete `LinkOperationalError`, never a non-match, partial candidate set, pattern downgrade, or fallback to `AllInScope`.

###### Token limit and derived maximum

`selector_pattern_tokens` is the exact structured spelling of the twenty-fourth common `LinkLimitCounter` variant, `SelectorPatternTokens`. It is an inclusive per-pattern counter with a fixed protocol ceiling of `513` parsed pattern tokens.

Every literal, `*`, and `**` occurrence counts as one after slash segmentation and operator/literal disambiguation, independently of its encoded byte length or how many candidate tokens `**` later consumes.

The parser walks segments from left to right and checks the incremented count before retaining the next parsed token; the first token above the effective limit rejects the complete reference artifact with `SelectorPatternTokens`, the established `ReferenceArtifactGroup(identity)`, and exactly `Exact(effective_limit + 1)`.

It never retains the raw pattern, rejected token, or record ordinal, reports a complete token count, or produces `ArithmeticOverflow`; it also never truncates, drops a suffix, splits the pattern, or converts it to another selector.

A caller-selected lower immutable value follows the common limits rule, and zero rejects every non-empty `Pattern`.

The value is derived from 013's fixed maximum candidate structural-path depth of 256 tokens. A literal or `*` consumes exactly one candidate token, so a pattern that can match an admitted candidate has at most 256 consuming tokens.

Because adjacent `**` operators are forbidden, at most one `**` can occupy each of the 257 gaps before, between, and after those consuming tokens. The largest potentially matchable pattern therefore has `256 + 257 = 513` tokens.

Any larger admitted spelling would necessarily require at least 257 consuming tokens and could never match a 013-admitted candidate. `*/*` and the alternating maximum remain expressible; the token ceiling does not count decoded token bytes or NFA state pairs.

This derivation is part of the coordinated 013/014 contract rather than a universal glob constant. Changing 013's maximum structural depth requires re-evaluating the formula `2 × candidate_token_ceiling + 1`, its protocol value, artifact compatibility, work ceilings, and fixtures.

An implementation must not infer a larger local value from a parser setting or accept an otherwise unmatchable oversized pattern as a harmless non-match.

M0 has no independent caller-adjustable `pattern_match_states` limit for one pattern-candidate evaluation. The structural ceilings already derive the inclusive absolute maximum as `(513 + 1) × (256 + 1) = 132,098` possible state pairs, and reachable-state accounting can never exceed that rectangle.

The implementation computes the exact reachable count with checked `u64` arithmetic and treats a value above the derived maximum as an internal contract invariant failure, not as another configurable resource outcome. There is no separate protocol value to clamp, serialize, or negotiate.

A caller that needs less per-evaluation work lowers `selector_pattern_tokens`; the candidate ceiling remains inherited from the admitted 013 definition contract. Aggregate logical work uses the inclusive request-level `LinkLimits` counter `pattern_match_states_total`.

Thus a single evaluation cannot bypass the aggregate budget, but M0 does not add a redundant per-evaluation knob whose hard ceiling is already completely implied by two existing structural limits.

###### Candidate-set construction and order

For one `Pattern` reference record, the logical candidate set is the set of distinct canonical `CatalogKey` values present among admitted definitions in that record's exact resolved scope-domain pair.

Exact domain-key equality removes duplicate definition occurrences before matching, so one key is evaluated exactly once for that reference record even when it appears in several locales, in several source artifacts, or in an ambiguous-definition collision.

Locale bindings, fallback edges, message payload equality, source/entry identity, and collision-evidence cardinality do not create additional pattern candidates. A scope-domain pair containing no definition keys produces zero pattern-candidate evaluations.

The match result is key-only and may be reused by later locale reachability, coverage, ambiguity, and bundle-plan processing. Each different `Pattern` reference record forms and charges its own candidate set, however, even when two records carry an equal scope, domain, and pattern.

Reference-record duplication is therefore not a work-accounting deduction, while definition-occurrence duplication is not part of the candidate set in the first place.

Pattern-candidate evaluations use one canonical nested order.

`Pattern` reference records are ordered first by their canonical `ReferenceRecordIdentity` ordering — `ReferenceArtifactIdentity`, then unsigned ordinal — and the distinct candidates for one record are ordered by the exact canonical `CatalogKey` bytes under that record's already-fixed domain.

All evaluations for one record precede those for the next record in this logical order. Input artifact order, source or definition order, locale order, configuration or discovery order, hash-map iteration, worker partitioning, and worker completion do not participate.

An implementation may execute or precompute matches concurrently, but aggregate accounting, the first attempted excess, and any evidence that identifies that failure must be observationally identical to this serial canonical order.

It may buffer out-of-order results for canonical admission; cancellation after a deterministic failure may avoid unnecessary physical work but never reduces the logical work charged before that failure or changes which evaluation fails first.

###### Request-wide pattern-work budget

`pattern_match_states_total` is the exact structured spelling of the twenty-fifth common `LinkLimitCounter` variant, `PatternMatchStatesTotal`.

It starts at zero for each checked `LinkRequest` and atomically adds the complete exact reachable-state count of every logical pattern-candidate evaluation in that canonical nested order with checked `u64` arithmetic. It never resets per artifact, record, scope, domain, locale, or delivery unit.

`Exact`, `Prefix`, `AllInScope`, and `UnboundedDynamic` add zero, as does a `Pattern` record whose candidate set is empty. Its inclusive fixed protocol ceiling is `100,000,000` logical states; any caller-selected lower immutable value follows the common `LinkLimits` rule.

An effective value of zero therefore admits requests with no pattern-candidate evaluation and rejects the first non-empty evaluation after its complete reachable-state count has been computed.

At the derived per-evaluation maximum of 132,098 states, the protocol ceiling admits 757 complete maximum-cost evaluations (`99,998,186` states) and rejects a 758th if it also reaches that maximum (`100,130,284` attempted states).

This is not a 757-evaluation count limit: any number of evaluations admitted by the separate artifact, record, and key limits remains valid when their exact reachable-state sum stays within 100,000,000.

The ceiling is a deterministic logical-work bound, not a wall-clock, instruction-count, throughput, or memory guarantee.

Protocol evolution follows the common versioned-ceiling rule. A protocol-ceiling increase requires a newer compatible minor contract; while `major == 0`, exact version matching means both producer/integration and consumer must explicitly select that new minor rather than treating it as an automatic backward read.

A ceiling reduction requires a breaking major.

Changing the `pattern_match_states_total` name, logical unit, candidate-set construction, canonical evaluation/state order, reset boundary, or deduction rules is also breaking because it can change deterministic admission and the first limit failure for an unchanged request.

No implementation release silently changes any of these under the same contract version.

A caller-selected lower immutable value is request policy, not protocol evolution, and requires no artifact-version change. It participates in link admission and cache input under the common `LinkLimits` rule but never changes artifact bytes, the 100,000,000-state protocol ceiling, or another caller's result.

The implementation attempts one atomic request-total addition after completing each canonical evaluation.

The first evaluation whose addition exceeds the effective ceiling returns one fail-complete `LinkOperationalError` carrying `PatternMatchStatesTotal`, `LinkLimitSubject::Request`, and `Exact(previous_total + current_evaluation_states)`, with no partial findings, plans, match set, or relaxed-limit retry.

It never substitutes `effective_limit + 1` or retains the selected artifact, reference record, candidate key, pattern, matcher-state position, or worker. Because the attempted total is at most `100,132,098`, `ArithmeticOverflow` is unconstructible.

Repeated patterns, records, or cross-record keys, cache reuse, compiled matcher reuse, and physical parallelism do not reduce this request total; duplicate definition occurrences were already excluded by candidate-set construction and therefore never create units to deduct.

###### Anchoring and responsibility boundaries

Matching is anchored at both ends of the structural token sequence. Evaluation begins at the candidate's first token and succeeds only when one pattern evaluation consumes the complete pattern and the complete candidate sequence.

There is no implicit leading or trailing `**`, substring search, suffix search, or prefix completion.

Conceptually, `[Literal("checkout"), *]` matches `["checkout", "title"]` but not `["checkout"]` or `["checkout", "form", "title"]`; `[Literal("checkout"), **]` can match the root sequence `["checkout"]` and any deeper sequence below it.

These examples describe checked tokens; their canonical strings use the fixed domain-pattern spelling above.

`intlify_contract` owns pattern parsing into a checked sequence of literal and operator tokens, including domain path separators, literal escaping, canonical emission, and invalid-input rejection. `intlify_linker` evaluates only that checked token pattern against candidate token sequences after the exact scope-domain filter.

Producers canonicalize source-language patterns into this representation but do not enumerate definitions or pre-expand matches. No layer applies Unicode normalization, case folding, percent decoding, or host-path rules unless a future version explicitly adds an operator with such semantics.

The M0 `pattern_match_states_total` contract, including compatible evolution, is fixed above and completes the selector-domain contract for this milestone.

`intlify_linker` filters candidates by the exact scope-domain pair and is the sole layer that evaluates a selector against definition keys.

`Exact`, `Prefix`, and `Pattern` dispatch through the selected domain contract; `AllInScope` selects all keys only after that scope-domain filter; and `UnboundedDynamic` carries no matcher payload and enters strict/compat policy instead of the matcher.

A producer canonicalizes its source-language spelling into the contract representation but neither enumerates catalog definitions nor pre-expands a selector. `intlify_resource` supplies only the canonical `CatalogKeyDomain` and `CatalogKey` values on the definition side and has no dependency on selector grammar or linker execution.

An unsupported selector/domain-contract version or an invalid selector is rejected during artifact compatibility validation before semantic linking, not approximated as a broader match.

`AllInScope` is an intentional, explicit bound selected by an API or configuration contract. `UnboundedDynamic` is never a spelling or fallback encoding of `AllInScope`; it preserves the producer's inability to prove a bound so link policy can decide the outcome.

These boundaries keep source syntax in producers, canonical catalog identity in `intlify_resource`, portable semantics in `intlify_contract`, and catalog enumeration and matching in `intlify_linker`.

#### Public artifact stability

**The artifact is a public, versioned contract from day one.** Independent producers, build integrations, caches, and consumers exchange it across crate and process boundaries, so it is not merely a linker-internal struct.

It needs explicit stability stages and v1 freeze criteria, reserved room for extension, a conformance test suite that any third-party producer can run, and explicit version negotiation at link time. M0 artifacts remain contextual to the current project and are not package-published resource artifacts.

A producer failure (unparsable configured source, scanner error) fails artifact production fail-complete, in the 013 tradition — a partial artifact is never emitted silently.

Recognizing an otherwise representable unbounded dynamic lookup is not such a failure: the producer emits `UnboundedDynamic`, preserving the same artifact independently of the consuming project's strict or compat policy.

#### Artifact version negotiation

Version support is consumer-defined and independent of producer identity:

```rust
pub enum ArtifactVersionSupport {
    Exact(ArtifactVersion),
    StableRange {
        major: u16,
        max_minor: u16,
    },
}
```

`Exact` and `StableRange` are the only Rust/API names; `DraftExact`, `Stable`, `Compatible`, and aliases do not exist. Their exact structured tags remain `exact` and `stable_range`.

The checked `ArtifactVersionEvidence` construction boundary admits `Exact` only when its version has `major == 0`, and admits `StableRange` only when `major >= 1`; an invalid mode/major combination cannot enter an `UnsupportedVersion` error or consumer support table.

For `major == 0`, a consumer uses `Exact` and accepts only the configured major/minor pair. The current reader and writer pair is `0.1`; `0.0`, `0.2`, and every other draft pair are incompatible.

Exact v0 matching remains only a gate, not a cross-revision compatibility promise: the mutable-draft constraints above still apply, and complete structural/semantic validation is required.

For a stable `major >= 1`, a consumer uses `StableRange` and accepts an artifact exactly when its major equals the supported major and its minor is in `0..=max_minor`. A newer reader decodes every accepted older minor using that minor's defaults and normalizes it into the current in-memory contract before linking.

Removing a field or variant, changing an existing meaning/default, tightening previously valid input, or otherwise preventing that normalization requires a major increment. An additive minor may add only contract elements for which the newer reader has an unambiguous older-version default.

Each reference and definition artifact is checked independently. One `LinkRequest` may contain different accepted stable minor versions; equality of artifact minor versions is not required after each has been normalized.

A different major, a newer minor, or any unsupported v0 pair fails compatibility validation before semantic linking and returns an operational error with the observed version and supported exact pair or range.

The consumer never guesses, downgrades, rewrites the claimed version, or treats a producer id/revision as an override.

M0 has no `required_features` list. All required semantics are represented by `ArtifactVersion`; an older consumer rejects a newer minor even if it could skip the unfamiliar outer fields mechanically.

Introducing orthogonal feature negotiation later requires concrete evidence and its own versioned contract change. A writer may claim an older supported minor only when it emits exactly that minor's schema and semantics, not merely by lowering the version numbers.

### Message Definition Artifact

One `MessageDefinitionArtifact` represents exactly one selected catalog source document after complete `intlify_resource` extraction and scope/locale binding. Its conceptual envelope and entry shape are:

```rust
pub struct MessageDefinitionArtifact {
    version: ArtifactVersion,
    producer: ProducerIdentity,
    source: SourceDocumentIdentity,
    logical_aliases: Vec<PortableRelativePath>,
    input_fingerprint: InputFingerprint,
    definitions: Vec<MessageDefinition>,
}

pub struct SourceDocumentIdentity {
    namespace: ArtifactNamespace,
    path: PortableRelativePath,
}

pub struct ProducerIdentity {
    id: ProducerId,
    revision: ProducerRevision,
}

pub struct ProducerId(String);

pub struct ProducerRevision(String);

pub struct InputFingerprint {
    algorithm: FingerprintAlgorithm,
    digest: FingerprintDigest,
}

pub enum FingerprintAlgorithm {
    Blake3_256,
}

pub struct FingerprintDigest([u8; 32]);

pub struct PortableRelativePath(Vec<PortablePathSegment>);

pub struct PortablePathSegment(String);

pub struct MessagePayload(String);

pub struct EntryReference {
    structural_path: EntryStructuralPath,
    occurrence: u32,
}

pub struct EntryStructuralPath(String);

pub struct MessageDefinition {
    scope: CatalogScopeId,
    domain: CatalogKeyDomain,
    key: CatalogKey,
    locale: Locale,
    message: MessagePayload,
    source: EntryReference,
}

impl MessageDefinitionArtifact {
    pub fn try_new(
        version: ArtifactVersion,
        producer: ProducerIdentity,
        source: SourceDocumentIdentity,
        logical_aliases: Vec<PortableRelativePath>,
        input_fingerprint: InputFingerprint,
        definitions: Vec<MessageDefinition>,
        limits: &LinkLimits,
    ) -> Result<Self, ArtifactContractError>;

    pub fn version(&self) -> &ArtifactVersion;
    pub fn producer(&self) -> &ProducerIdentity;
    pub fn source(&self) -> &SourceDocumentIdentity;
    pub fn logical_aliases(&self) -> &[PortableRelativePath];
    pub fn input_fingerprint(&self) -> &InputFingerprint;
    pub fn definitions(&self) -> &[MessageDefinition];
}
```

The same private-state and complete-construction rule applies to the definition side.

`MessageDefinitionArtifact::try_new` takes ownership of the complete alias and definition sequences, retains their submitted semantic order, and performs the definition envelope, alias-set, fingerprint-shape, record, occurrence-continuity, decoded-accounting, and current-lower-limit phases before exposing the artifact.

It does not perform a wire-byte check, freshness recomputation, host parsing, or MF2 parsing. A failed call returns no artifact and leaves no builder state that can later be completed under a different validation order.

#### M0 definition-artifact JSON schema

The M0 serialized `MessageDefinitionArtifact` wire is exactly one JSON object encoded as valid UTF-8 without a byte-order mark.

CBOR, MessagePack, a custom binary envelope, and content-sniffed alternatives cannot claim the same artifact contract or `ArtifactVersion`; introducing another encoding requires an explicitly identified transport contract and a compatibility decision.

The artifact decoder never guesses an encoding from leading bytes, a file extension, or content.

##### Root and identity envelopes

The root object has the required wire discriminator `"kind":"message-definition"`. A decoder compares the decoded string exactly; case variants, aliases, unknown values, non-string values, and a reference-artifact kind are invalid rather than normalized or dispatched as a definition.

After root syntax and duplicate-member checks, the typed definition decoder validates `kind` before `ArtifactVersion` compatibility and the remaining schema.

A future generic decoder may use the same value to select a typed decoder, but successful decoding returns `MessageDefinitionArtifact` without storing a redundant kind field. The constant is not provenance, source or message identity, semantic link input, or a fingerprint-tuple component.

`ArtifactVersion` has the required JSON object shape `"version":{"major":0,"minor":1}` for the current WIP writer. `major` and `minor` are separate unsigned `u16` JSON numbers under the lexical integer rules below; a dotted string, array, single combined integer, signed value, or additional version component is invalid.

The object contains exactly the required `major` and `minor` members. Their input order is non-semantic, while canonical emission orders `major` before `minor`.

This object is part of the bootstrap JSON envelope used to select the remaining schema, so every version of this JSON transport retains its shape; changing it requires a separately identified transport-contract change rather than an artifact minor bump.

`SourceDocumentIdentity` has the required JSON object shape `"source":{"namespace":{...},"path":[...]} `.

It contains exactly `namespace` and `path`; input member order is non-semantic, while canonical emission orders `namespace` before `path`. `path` uses the non-empty portable segment-array codec fixed below.

The same checked type and codec identify a definition artifact's catalog document and a reference record's optional source origin; the type name therefore does not assign ownership to either artifact family.

The required `logicalAliases` member applies that same `PortableRelativePath` codec directly to every array element. Its exact shape is therefore an array of zero or more non-empty arrays of non-null JSON strings, for example `"logicalAliases":[["locales","ja.json"],["resources","ja.json"]]`.

An alias never repeats the enclosing `source.namespace` and is not wrapped in a `{ "path": ... }` object. A slash-separated string, object, `null` element or segment, non-string segment, flattened path, host path, or namespace-bearing alias is invalid rather than converted.

The empty array is the one no-alias value fixed above.

`ArtifactNamespace` uses one uniform tagged-object codec for source-document, reference-artifact, and catalog-scope identity. M0 accepts only `"namespace":{"kind":"project"}` with exactly one `kind` member.

The decoded value is compared exactly. Case variants, aliases, unknown values including `"package"`, missing or duplicate discriminators, non-string discriminators, and any additional `package` or other member are invalid rather than normalized or inferred.

Canonical output is exactly `{"kind":"project"}`.

Unlike the root wire-only discriminator, this nested discriminator is retained as the explicit contextual namespace and participates in source equality and the fingerprint input through `SourceDocumentIdentity`. Its JSON bytes count toward the wire limit; the fixed typed discriminant adds no decoded scalar bytes.

A future package variant requires the deferred design, coordinated artifact-version and schema work, a canonical fingerprint payload, limits, and host trust binding. An M0 decoder never accepts package coordinates speculatively.

##### Entry reference

`EntryReference` is the portable projection of the 013 `EntryKey`, not a catalog key or an artifact-local handle. Its required JSON object is `"source":{"structuralPath":"...","occurrence":0}` and contains exactly `structuralPath` and `occurrence`.

Input member order is non-semantic; canonical emission orders `structuralPath` before `occurrence`. `structuralPath` is the exact decoded UTF-8 serialization returned by the selected 013 host-format contract and may be empty for a valid root entry.

It is retained without Unicode normalization, case folding, display-key flattening, or conversion to a host path. `occurrence` is the zero-based `u32` occurrence among entries with the same complete structural path and is always present, including `0` for a path that occurs only once.

Within one definition artifact, the pair `(structural_path, occurrence)` is unique.

Reading `definitions` in its preserved 013 raw-entry order, occurrences for each equal structural path must begin at `0` and advance contiguously by one whenever that path appears again; a duplicate, skipped, decreasing, or overflowing occurrence rejects the complete artifact.

The complete portable entry identity is the envelope's `SourceDocumentIdentity` together with this `EntryReference`. It deliberately excludes `EntryHandle`, `CatalogKey`, display key, host span, message offset map, vector index, and generated numeric ID.

The structural-path bytes contribute once per definition to `definition_artifact_decoded_bytes` and their actual JSON spelling contributes to `definition_artifact_wire_bytes`; the fixed-width occurrence contributes no decoded scalar bytes.

Its canonical semantic payload is a two-field tagged record with structural-path UTF-8 bytes at tag `0x01` and `occurrence:u32be` at tag `0x02`.

##### Definition identity limits

Every `EntryStructuralPath` independently uses the inclusive field-specific `LinkLimits` counter `entry_structural_path_bytes`, with a fixed protocol ceiling of 64 MiB (`67,108,864` decoded UTF-8 bytes) per value. Its public evidence variant is the sixteenth common counter, `EntryStructuralPathBytes`.

The check measures the exact decoded serialization supplied by the selected 013 host-format contract; an empty root path contributes zero and remains valid.

Its bytes still contribute independently per definition occurrence to `definition_artifact_decoded_bytes`, without a deduction for equal values, interning, shared storage, or repeated occurrences.

The default ceiling deliberately matches 013's complete source-local `identity_bytes` ceiling, so one structural path admitted by 013 is not rejected merely by definition projection. A caller-selected lower value may intentionally reject it; no layer truncates, hashes, replaces, or normalizes an over-limit path.

The definition-side `MessageDefinition.key` carries the exact canonical `CatalogKey` emitted by 013 and is interpreted only with the record's `CatalogKeyDomain`.

Every value independently uses the inclusive field-specific `LinkLimits` counter `catalog_key_bytes`, also with a fixed protocol ceiling of 64 MiB (`67,108,864` decoded UTF-8 bytes). Its public evidence variant is the seventeenth common counter, `CatalogKeyBytes`.

A domain-valid empty root key contributes zero and remains valid.

Key bytes are charged independently per definition occurrence to `definition_artifact_decoded_bytes`, including repeated equal keys. `catalog_key_bytes` is distinct from the reference-side `selector_path_bytes`: the former validates a catalog definition produced by resource extraction, while the latter validates an `Exact` or `Prefix` reference selector, and neither counter borrows, pools, or offsets the other.

The matching numeric ceiling preserves the same single-value compatibility with 013; a caller may select independent lower values for the two counters.

There is no additional `definition_identity_bytes_total` counter. The 013 `identity_bytes` counter remains a source-local producer fact over distinct interned structural-path, catalog-key, and display-key payloads and is enforced before projection.

A published artifact does not expose enough trusted interner state to reconstruct that accounting.

On the consumer side, the two per-value checks above, `definition_artifact_decoded_bytes`, and `definition_artifact_decoded_bytes_total` already bound both individual identities and every serialized occurrence across an artifact and request.

Implementations therefore do not re-intern definition identities merely to create a second aggregate limit.

##### Message payload

M0 `MessagePayload` is one required JSON string, for example `"message":"Hello, {$name}"`.

Its decoded value is exactly the Unicode-scalar `message_text` supplied by the admitted 013 entry, including an empty string, and is retained without Unicode normalization, newline conversion, trimming, placeholder rewriting, or re-escaping into the host format.

It contains neither raw host-string syntax nor an AST, Binary AST snapshot, compiled runtime representation, fallback value, or format tag.

JSON escaping is only the outer wire representation and follows the canonical/accepted spelling rules below; after decoding, equivalent JSON escapes produce the same `MessagePayload` bytes.

The JSON codec, definition-artifact conformance boundary, and `intlify_linker` treat the string as an opaque MF2 source payload. Structural decoding validates its scalar/string and resource-limit contract but does not parse MF2 syntax.

A structurally conforming artifact may therefore retain syntax-invalid or syntax-valid-but-semantically-invalid message text, and that definition still participates in key resolution and usage findings. `intlify lint` can present the parser-owned diagnostic together with linker-backed key findings instead of losing the complete source definition set.

The producer must copy the accepted extraction value exactly rather than substituting a formatted or reparsed spelling.

Every occurrence independently uses the inclusive `message_bytes` counter inherited from 013, with a fixed protocol ceiling of 1 MiB (`1,048,576` decoded UTF-8 bytes). Its public evidence variant is the eighteenth common counter, `MessageBytes`.

An empty payload is valid and contributes zero, so an effective lower value of zero admits only empty messages. A non-empty overrun stops at the first attempted byte and returns `Exact(effective_limit + 1)` without retaining or scanning the remainder merely to compute a larger value.

The complete source artifact also uses the inclusive running `total_message_bytes` counter inherited from 013, with a fixed protocol ceiling of 64 MiB (`67,108,864` bytes) and the nineteenth public variant `TotalMessageBytes`.

Each already admitted message contributes its complete decoded length in raw-entry order, including empty and repeated equal values; the first rejected addition records the exact checked running sum rather than a `limit + 1` sentinel.

Both variants require `DefinitionArtifactGroup(source)`, expose no message, definition index, or `EntryReference`, and permit independent caller-selected lower values.

Each occurrence also contributes its complete decoded length to `definition_artifact_decoded_bytes`, while its actual escaped JSON spelling contributes to `definition_artifact_wire_bytes`. Those enclosing counters overlap rather than replace either inherited message counter.

Equal strings, interning, zero-copy storage, cache reuse, or repeated locale placement provide no deduction from any applicable total. Deployable-output safety is enforced by the build/export integration gate below, never by adding an MF2 parser to the linker core.

An object, array, number, boolean, or `null` in `message` is invalid in M0. A future AST, snapshot, or compiled payload must use an explicitly versioned additional representation; it cannot reinterpret an M0 string.

Keeping the string while adding an optional understood field requires the applicable compatible minor-version decision, while replacing the string shape is a breaking major-version change.

##### Accepted and canonical JSON

Decoder acceptance and canonical producer emission are intentionally distinct. The decoder accepts object-member order and insignificant JSON whitespace as non-semantic, but otherwise applies a strict grammar and the schema selected by `ArtifactVersion`:

- The input contains one root object followed only by optional JSON whitespace and EOF. JSON whitespace is limited to `U+0020`, `U+0009`, `U+000A`, and `U+000D`. A BOM, comments, trailing commas, multiple root values, and non-whitespace trailing data are invalid.
- Duplicate member names are rejected at every object depth before any map-like representation could overwrite them. Unknown members, missing required members, and `null` where the selected version does not explicitly permit it are invalid. An older consumer rejects a newer artifact version rather than accepting its unfamiliar members.
- JSON strings decode to Unicode scalar sequences. Unpaired surrogate escapes are invalid; decoded values are not Unicode-normalized, case-folded, or otherwise rewritten. The decoder accepts semantically equivalent valid JSON escape spellings, and the decoded scalar sequence is the contract value.
- Every M0 numeric field is an unsigned integer with a contract-defined width. Its JSON token is `0` or an unsigned base-10 sequence beginning with `[1-9]`; a sign, fraction, exponent, out-of-range value, or conversion through an IEEE-754 value is invalid. Implementations parse the token lexically into the target integer type. M0 does not encode a wider integer as an imprecise JSON number or an ad hoc decimal string.
- Object-member order does not change the typed artifact. Array order remains semantic: `logicalAliases` uses its canonical alias order, and `definitions` preserves 013 raw-entry order.

A conforming producer emits one canonical JSON spelling.

The root contains exactly seven required, non-null members in this exact order: `kind`, `version`, `producer`, `source`, `logicalAliases`, `inputFingerprint`, `definitions`. `logicalAliases` is always an array and is emitted as `[]` when there are no aliases; omission is not an alternate empty spelling.

A missing, duplicate, unknown, mistyped, conditionally omitted, or `null` root member rejects the complete artifact. Input object-member order remains non-semantic, but canonical re-emission always includes all seven members, so the first bytes after `{` are exactly `"kind":"message-definition"`.

Nested record members follow the order declared by their versioned contract; maps with unconstrained user keys are not part of the M0 definition envelope.

The encoder emits no insignificant whitespace or final newline and uses the shortest unsigned decimal integer spelling.

For strings, `\"`, `\\`, and control scalars are escaped. It uses `\b`, `\t`, `\n`, `\f`, and `\r` where applicable, and lowercase `\u00xx` for the remaining `U+0000..U+001F` scalars. `/` and all other Unicode scalars are emitted directly as UTF-8.

A consumer may accept a noncanonical but otherwise conforming member order, whitespace layout, or equivalent string escape. Canonical re-emission produces the one spelling above.

This JSON canonicalization governs reproducible artifact bytes only. It is not the semantic fingerprint framing and does not replace the tagged, typed fingerprint byte stream.

Equivalent accepted JSON spellings decode to the same typed artifact and fingerprint inputs, while `definition_artifact_wire_bytes` always charges the exact input bytes actually supplied to the decoder.

The `kind` member's actual JSON bytes contribute to `definition_artifact_wire_bytes`. Its fixed decoded constant contributes zero to `definition_artifact_decoded_bytes` because the typed artifact does not retain it; this cannot create data-dependent expansion.

##### Producer identity and provenance

- For both reference and definition artifacts, `producer` is the required wire object `{ id, revision }` represented by the shared `ProducerIdentity` type. `id` is a stable, cross-ecosystem namespaced identifier for one producer implementation family; it remains unchanged across releases, installation paths, package managers, and host processes. `revision` is the exact implementation/build revision that emitted the artifact.
  - It is an opaque string rather than a SemVer value: consumers compare its decoded bytes for equality and never parse ordering, compatibility ranges, or release precedence from it.
- `ProducerId` uses the canonical lowercase-ASCII grammar `<reverse-dns>/<producer-slug>` and is at most 255 decoded bytes in total.
  - The reverse-DNS part contains at least two `.`-separated labels; each label is 1–63 bytes, starts and ends with `[a-z0-9]`, and otherwise contains only `[a-z0-9-]`.
  - An internationalized domain uses its lowercase ASCII IDNA spelling rather than Unicode.
  - The slug is one kebab-case component matching `[a-z0-9]+(?:-[a-z0-9]+)*`.
  - There is exactly one `/`; empty components, uppercase, `_`, whitespace, URI schemes, percent encoding, query/fragment text, and additional `/` are invalid rather than normalized.
  - The built-in 013 definition projection uses `dev.intlify/resource-definition`; a third party uses a namespace conventionally controlled by that producer, such as `com.example/custom-json`.
  - The built-in JS/TS/Vue reference producer uses `dev.intlify/js-reference`. JS, JSX, TS, TSX, and Vue are frontend profiles of that one implementation family rather than separate producer IDs.
  - A release or build identifier belongs in `revision`; a producer does not mint a new id merely because its implementation version changes.
- `ProducerRevision` is 1–128 decoded ASCII bytes and matches `[A-Za-z0-9][A-Za-z0-9._+-]*`.
  - Uppercase is valid in a revision even though it is forbidden in a producer ID.
  - Reject leading or trailing whitespace, control characters, Unicode, `/`, `:`, and every other character outside the grammar.
  - Never trim, case-fold, Unicode-normalize, or substitute an empty value.
  - Apply length checks to decoded wire strings before constructing the validated newtypes.
- The 255-byte id ceiling and 128-byte revision ceiling are fixed structural limits of the artifact contract, not separate `LinkLimitCounter` variants and not caller-lowerable budgets.
  - In every reference or definition artifact, the exact decoded bytes of `id` and `revision` contribute once each to that artifact's `*_artifact_decoded_bytes` total after JSON unescaping and before checked newtype construction.
  - Their actual escaped JSON spellings contribute only to `*_artifact_wire_bytes`; object framing and member names contribute zero decoded bytes.
  - Grammar and structural-length failures are selected within the complete `ProducerIdentity` phase before source validation.
  - Provenance comparison, cache partitioning, interning, or reuse never adds a second charge or provides a deduction.
- Reverse-DNS spelling provides collision-resistant allocation, not authority or network discovery. A consumer performs no DNS request, ownership lookup, redirect, case fold, or alias resolution, and two distinct valid byte strings are distinct producer ids. Once published, an id remains stable even if implementation packaging or domain ownership changes; provenance authentication remains external as described below.
- `ProducerRevision` identifies the artifact-producing build, not merely its marketing release.
  - A producer must advance it whenever a code, build-feature, generated-table, output-affecting dependency, or other implementation change could produce a different artifact from the same effective source/configuration inputs under the same `ArtifactVersion`.
  - It may remain unchanged only when the producer can establish that a change cannot affect artifact production, such as a documentation-only change.
  - An immutable released build may use its exact release version when that value uniquely identifies the artifact-producing implementation; otherwise it adds a build/source fingerprint, for example `0.4.2+sha.0123abcd`.
  - A development or locally modified build must not claim an unchanged released revision when its artifact-producing implementation differs.
- Revision generation must be deterministic for equivalent producer builds.
  - Wall-clock timestamps, random identifiers, installation paths, process-local state, and other per-invocation values are forbidden because they would defeat cache reuse without identifying an output-affecting change.
  - The revision value remains opaque to consumers: a linker accepts valid artifacts from different revisions of the same producer in one `LinkRequest`, does not require revision uniformity, and does not infer that one revision is newer or safer than another.
- The complete `{ id, revision }` value is provenance and a producer-specific cache namespace only.
  - A change prevents reuse of a cached artifact.
  - It does not change source identity, message identity, finding semantics, plan ordering, or any other `LinkOutcome` behavior for otherwise equivalent inputs.
  - Neither field is inferred from an executable path or local package-manager installation location.
  - `ArtifactVersion`, not producer identity, controls wire compatibility and version negotiation.
- `intlify_linker` does not maintain a producer allowlist and does not treat a familiar producer id as proof of authenticity or correctness.
  - An unknown third-party producer is accepted when its artifact version is supported and the complete artifact passes contract validation; a malformed or incompatible artifact is rejected even when it claims the built-in producer identity.
  - Operational validation evidence may include producer provenance, but rule subjects and semantic findings do not depend on it.
  - Authenticating an artifact remains a host/integration concern rather than a meaning assigned to this field.
- The artifact is source-complete by construction.
  - It contains every entry admitted by one successful extraction in 013 raw-entry order, including an empty `definitions` vector for a valid zero-entry catalog.
  - A host parse, extraction, binding, domain-consistency, or projection failure emits no artifact for that source; partial entry subsets are invalid rather than a degraded form.
  - The initial envelope therefore has no source-level `complete` / `partial` discriminator: the presence of a valid artifact means complete for that source, while set-level completeness is separate.
- `source` names the selected logical catalog document and is the base identity for each definition's `source` evidence.
  - Equality is the tuple of namespace and exact portable relative path.
  - Source bytes, `input_fingerprint`, an absolute or canonical filesystem path, device or inode identity, symlink target, and adapter identity never participate in source equality.
  - Equal bytes at different source identities remain different sources, while editing bytes preserves source identity.
- `ArtifactNamespace::Project` is contextual to the one consuming application represented by a `LinkRequest`. It is valid for locally produced or locally cached artifacts only after the host binds the artifact to the current `projectRoot`; a published artifact cannot make a portable `Project` claim. M0 has no second namespace variant and therefore no portable package-root source identity.
- `PortableRelativePath` is serialized as a non-empty array of UTF-8 string segments, for example `["locales", "ja", "messages.json"]`; it is never serialized as one slash-separated path string.
  - Each decoded segment must be a non-empty Unicode scalar sequence other than exact `.` or `..`, must not contain `U+0000` or `/`, and is retained byte-for-byte in UTF-8 without Unicode normalization or case folding.
  - A backslash is an ordinary segment character, not a separator, so a Unicode-representable Unix filename is not rejected merely because another host treats that spelling specially.
  - The producer first binds an exactly Unicode-representable host path beneath the project root, splits host separators at that boundary, and only then constructs the segment array; an absolute, drive-prefixed, UNC-prefixed, or root-escaping host path therefore has no valid wire representation.
  - Equality compares the namespace and segment arrays exactly.
  - Human-readable paths may join reporter-escaped segments with `/`, but that rendering is display-only and must never be parsed back into identity.
- `logical_aliases` is sorted, duplicate-free source evidence under the same namespace and never changes `SourceDocumentIdentity` or creates another set of definitions. Host physical identity helps the local integration discover aliases, but it is never serialized.
- `input_fingerprint` is exactly one wire object with the two required, non-null members `algorithm` and `digest`.
  - A missing, duplicate, unknown, mistyped, or `null` member rejects the complete artifact.
  - Input member order is non-semantic, but the canonical encoder always emits `algorithm` followed by `digest`; M0 never infers a default algorithm from an omitted member or accepts a bare/external-tagged digest as the same contract.
  - M0 emits the exact algorithm tag `"blake3-256"` and computes the standard unkeyed BLAKE3 hash with its 32-byte output over the domain-separated canonical fingerprint byte stream.
  - No secret key, salt, random value, machine identity, or process-specific input participates.
  - The semantic input tuple and framing are fixed below.
- `digest` serializes those 32 bytes as exactly 64 lowercase ASCII hexadecimal characters, with no `0x` prefix, separators, padding, or alternate base64 form.
  - Uppercase hex, a wrong length, a non-hex character, a missing field, an extra field, and every algorithm tag other than the negotiated contract's supported exact spelling are artifact-validation failures rather than values to normalize or guess.
  - A future algorithm receives a distinct tag and compatibility treatment; it never reuses `"blake3-256"` with different hash semantics.
  - The actual 64-character spelling and JSON syntax contribute to `definition_artifact_wire_bytes`.
  - After successful hex decoding, the retained opaque digest contributes exactly 32 bytes once to `definition_artifact_decoded_bytes`; the fixed `FingerprintAlgorithm::Blake3_256` discriminant contributes zero.
  - The decoder never charges both the wire hex and decoded digest to the decoded total or treats the digest as zero merely because its Rust storage has a fixed array length.
- The fingerprint is freshness/cache evidence, never source identity, artifact authenticity, semantic link input, or a substitute for complete artifact validation.
  - A cache owner that has the complete canonical inputs recomputes it and treats a mismatch as a cache miss or stale artifact, not as a linker finding.
  - A consumer without the complete canonical inputs may validate the structural envelope but cannot claim to have verified the producer's digest.
  - Changing only a valid fingerprint cannot change findings or plans for otherwise equal artifacts.

#### Fingerprint input tuple

The producer hashes exactly one resolved semantic tuple for the complete artifact:

```rust
struct DefinitionFingerprintInputs<'a> {
    artifact_version: ArtifactVersion,
    source: &'a SourceDocumentIdentity,
    logical_aliases: &'a [PortableRelativePath],
    host_format: CanonicalHostFormatId,
    scope: CatalogScopeId,
    locale: Locale,
    domain: CatalogKeyDomain,
    projection_options: CanonicalProjectionOptions,
    source_bytes: &'a [u8],
}
```

The fingerprint tuple uses the following resolved values:

- `source_bytes` is the complete exact selected host document, including any retained BOM, original line endings, whitespace, comments, and bytes outside message entries.
- `source` includes its explicit fixed `Project` namespace.
- `logical_aliases` uses the already validated and sorted segment arrays.
- `host_format` is the resolved canonical registry id, independent of whether config or extension classification selected it.
- `scope`, `locale`, and `domain` are the exact resolved values serialized into or governing the definitions.
- `projection_options` is the canonical typed collection of every remaining resolved option that can alter entry admission, key construction, message extraction, or definition projection; it is empty when no such option exists.

An implementation-dependent environment value may affect output only after it is represented in this collection or covered by a changed producer revision.

The tuple deliberately excludes raw config bytes and formatting, catalog-definition/glob spelling and order, match provenance, CLI operand and filesystem enumeration order, the project absolute root, canonical host paths, mtime, size metadata, device/inode and symlink targets, reporter or command-mode settings, and producer id/revision.

Producer identity already partitions the cache separately. The emitted `definitions`, extraction artifact, and fingerprint itself are outputs and are not recursively hashed.

Two configurations that resolve to the same tuple must produce the same fingerprint under the same producer identity; changing any included component must change the fingerprint input even when the resulting message text happens to remain equal.

#### Fingerprint byte framing

The BLAKE3 input starts with these exact bytes followed by the unsigned big-endian framing version `1`:

```text
69 6e 74 6c 69 66 79 00                         # "intlify\0"
64 65 66 69 6e 69 74 69 6f 6e 2d 66 69 6e 67 65
72 70 72 69 6e 74 00                            # together: "definition-fingerprint\0"
62 6c 61 6b 65 33 2d 32 35 36 00                # "blake3-256\0"
00 01                                           # framing version: u16be(1)
```

The remaining stream contains exactly these fields in ascending tag order:

| Tag    | Tuple field          |
| ------ | -------------------- |
| `0x01` | `artifact_version`   |
| `0x02` | `source`             |
| `0x03` | `logical_aliases`    |
| `0x04` | `host_format`        |
| `0x05` | `scope`              |
| `0x06` | `locale`             |
| `0x07` | `domain`             |
| `0x08` | `projection_options` |
| `0x09` | `source_bytes`       |

Each field is `tag:u8 || payload_len:u64be || payload`. Every field appears exactly once; there are no unknown or duplicate tags, alternate order, padding, alignment bytes, terminator, or trailing data.

Lengths count payload bytes only. A sequence payload is `item_count:u64be` followed by `item_len:u64be || item_payload` for each item in semantic order.

Records nested inside a payload use the same ascending-tag field framing. Validated UTF-8 or ASCII scalar newtypes contribute their decoded bytes exactly, while fixed numeric scalars use their contract-defined unsigned big-endian width.

Enums start with their contract-defined `u8` discriminant followed by variant payload when present. They are semantic encodings: JSON key order, escaping, number spelling, and other outer artifact-wire syntax never enter the hash.

`PortableRelativePath` is a sequence of its UTF-8 segment payloads. `logical_aliases` is a sequence of complete path payloads in the already fixed order. `SourceDocumentIdentity` is a two-field nested record: namespace at tag `0x01`, path at tag `0x02`.

M0 `Project` has namespace discriminant `0x00` and no variant payload; no other discriminant is valid. `projection_options` is a nested record ordered by stable option tags and is an empty record payload when M0 has no format-specific options.

Adding or changing an output-affecting option requires a stable option tag and the applicable artifact/framing compatibility change; it cannot be inserted as untagged bytes.

The M0 `CatalogScopeId` record framing is fixed below; canonical payload codecs for other public inner types that remain open are fixed by those owning decisions and then used unchanged here. `ArtifactVersion` already contributes its exact four-byte major/minor payload above.

These type decisions do not change the framing itself. Until every type present in an input has such a codec, a producer cannot claim a conformant fingerprint for that input.

Because the final `source_bytes` field supplies its known byte length before its payload, the producer can feed the exact source directly to BLAKE3 in chunks without constructing the complete framed stream or making a second source-sized copy.

#### Definition projection semantics

- Source completeness does not claim that every source in a project, scope, or producer participated. Closed/partial world and prune eligibility come only from the execution-derived `ScopeCompletenessTable` above. A linker therefore never infers project completeness merely because every supplied source artifact is individually complete.
- The projection layer is outside `intlify_resource`: it consumes a successful extraction plus resolved catalog assignment and serializes the public contract. `intlify_resource` neither imports the linker contract nor aggregates files.
- `scope`, `domain`, and `key` define message identity. `scope` resolves from the explicit 013 `resources.catalogs[].scope`, `locale` resolves from its paired locale binding, and the entry-level `source` keeps entry identity and span under the envelope's catalog-source identity.
- `locale` is mandatory from M0.
  - M0 is fallback-blind, not locale-absent: it retains the exact production-set locale on every definition but resolves references against a locale-agnostic union keyed by `(scope, domain, key)`.
  - An M0 `unresolved-message` therefore means that no declared production locale defines the referenced key; a reachable union member marks all of its production-locale-specific definitions reachable, so M0 reports `unused-message` only for keys unreachable in every production locale.
  - M2 activates per-requested-locale fallback and coverage semantics without changing the definition artifact shape.
- The general resource schema keeps the `CatalogConfig.scope` / `locale` pair optional because entry-level formatting does not need it.
  - A catalog participating in linker definition input must resolve one exact scope and locale for every selected entry.
  - M0 never emits a definition with missing or optional identity metadata.
  - An entry-level-only catalog may still be formatted, but a linker invocation that selects it fails before linking.
- Catalog-level checks and the linker share one extraction result. No second catalog parser or key resolver is introduced.
- A `LinkRequest` rejects duplicate definition-artifact `SourceDocumentIdentity` values as an operational contract error rather than silently choosing, merging, or de-duplicating artifacts, even when their fingerprints and definitions are byte-identical. Repeated use of one source identity in independent optional reference origins is evidence and is not subject to that definition-artifact uniqueness rule.

#### Logical alias grouping and primary identity

The local definition producer resolves catalog membership and binding independently for every selected logical target, then groups targets that the shared 005/013 host inspection identifies as the same physical source. This grouping is only a local production step; it does not add physical identity to the artifact contract.

Every participating alias in one physical group must resolve to the same binding tuple: `ArtifactNamespace`, canonical host-format id, `CatalogScopeId`, and exact locale. A mismatch is a configuration operational error for the complete group.

The producer emits no artifact for it and does not pick one binding by config order, discovery order, or path order. Excluded or otherwise non-participating logical targets are absent from the group and from alias evidence.

For a valid group, paths are ordered segment-by-segment by exact UTF-8 bytes, with a shorter equal-prefix path first. The first path becomes `SourceDocumentIdentity.path`; the remaining paths become `logical_aliases` in that same order.

The producer performs one complete extraction and emits one definition artifact whose entries use the primary source identity. Input enumeration order, filesystem enumeration order, symlink traversal order, and command operand order cannot affect the artifact.

If two distinct host paths collapse to the same `PortableRelativePath`, production fails as a duplicate logical identity rather than retaining one arbitrarily.

This read-only linker projection intentionally differs from 013's formatter/linter CLI alias behavior, where every logical alias retains a separate result and writes require serialized rereads.

Definition linking needs one semantic definition set per source; aliases remain evidence for diagnostics and local source lookup, not duplicate definitions. The artifact consumer validates the portable identities and ordering but neither reconstructs nor trusts a serialized device/inode claim.

### Locale identity

`Locale` is one shared checked, opaque identity used by 013 catalog binding, every `MessageDefinition`, and resolved policy or output fields that name a locale. Its M0 scalar grammar is exactly one or more Unicode scalar values.

Equality and canonical ordering use the exact decoded UTF-8 bytes and are case-sensitive. No layer trims, Unicode-normalizes, case-folds, replaces `_` with `-`, canonicalizes deprecated subtags, or requires BCP 47, Unicode Locale Identifier, ICU, or POSIX syntax.

Standards advice and project-specific naming conventions belong to locale-aware lint rules rather than artifact admission.

The opaque grammar intentionally preserves all non-empty Unicode-scalar sequences, including leading or trailing whitespace, whitespace-only values, and control scalars. Those values remain distinct identities rather than being repaired or treated as omission.

JSON writers, reporters, diagnostics, and exporters must escape them for their output context.

A locale value is never concatenated directly into a host path, output path, shell token, source literal, or C string; an integration that maps locale identity into such a namespace validates and encodes that destination independently.

An unpaired surrogate is not a Unicode scalar and remains invalid at the JSON boundary.

`locale_bytes` is the field-specific inclusive `LinkLimits` counter for one submitted `Locale` occurrence, with a fixed protocol ceiling of 255 decoded UTF-8 bytes. The accepted protocol-default range is therefore 1 through 255 bytes.

Decoders count after JSON unescaping, 013 path binding counts the exact captured scalar bytes, fixed and future host bindings count the exact resolved value, and direct constructors check before retaining the newtype.

Exactly 255 bytes is valid; an empty value, the first byte above the effective limit, checked conversion failure, or a non-string wire value rejects the complete owning configuration, artifact, policy, or output input.

A caller-selected lower immutable value applies wherever that invocation consumes `Locale`; zero admits no locale occurrence and never turns a required value into omission.

Every occurrence is checked and contributes its exact bytes independently to any enclosing decoded-byte counter. Equal values, production-set membership, fallback reuse, sorting, deduplication, interning, cache reuse, and parallel execution provide no deduction.

`MessageDefinition.locale` is one required JSON string carrying this exact codec, contributes once to `definition_artifact_decoded_bytes`, and uses its exact decoded bytes as the canonical `locale` fingerprint payload.

The coordinated 013 validator applies the same protocol ceiling before definition projection, so a producer cannot emit a locale that the shared contract rejects.

The M0/M1 production-locale portion needs no independent `policy_locale_bytes_total` counter. Its one collection ceiling admits at most 1,024 submitted locale occurrences and therefore at most `1,024 × 255 = 261,120` locale bytes.

Beginning with M2, the added fallback collection ceilings admit at most 1,024 fallback-source occurrences and `1,024 × 64 = 65,536` fallback-target occurrences. Together with production locales, the post-M2 policy therefore admits at most 67,584 submitted locale occurrences and `67,584 × 255 = 17,233,920` locale bytes, still without a separate aggregate counter.

Duplicate, malformed, out-of-set, and later-rejected occurrences receive no accounting deduction.

Each derivation covers only the locale-bearing policy collections admitted by that milestone. Every later locale-bearing coverage or output collection must receive its own bounded occurrence count in its owning decision and reuse `locale_bytes`, rather than silently expanding either derivation or introducing one generic aggregate counter.

The explicit `LocaleBytes` variant is the fifteenth member of the closed common `LinkLimitCounter`; the two earlier fallback-counter positions remain reserved and unreachable until M2 so later ordinals do not shift. A definition occurrence uses `DefinitionArtifactGroup(source)` and a resolved-policy occurrence uses `ResolvedPolicy`; neither subject retains the raw locale or an occurrence index.

Decoded-byte admission stops accounting at the first attempted byte above the effective limit and records exactly `Exact(effective_limit + 1)`, never the full submitted scalar length or `ArithmeticOverflow`.

A transport parser may still skip and validate bounded wire syntax needed to finish the enclosing object and establish its source identity, but it does not retain, re-decode, or scan the remaining scalar merely to compute a larger observation.

`LocaleBytes` creates no counter-global priority between definition and policy inputs; the eventual request-wide input-group order selects between independently failing groups.

### Project scope identity

M0 supports application-owned resources only. `ArtifactNamespace` therefore has exactly one admitted value, `Project`, bound by the host to the current application/project root.

Package-provided resources, package namespaces, and portable artifacts published by libraries or resource-only packages are outside M0 and are retained in [Package-provided resources and published artifacts](#package-provided-resources-and-published-artifacts).

`CatalogScopeId` is the portable structural pair `(ArtifactNamespace, CatalogScopeName)`. Its required JSON value is exactly an object with `namespace` and `name`, for example `"scope":{"namespace":{"kind":"project"},"name":"app"}`.

Input member order is non-semantic; canonical emission orders `namespace` before `name`, and the nested namespace is exactly `{"kind":"project"}`. Missing, duplicate, unknown, mistyped, flattened, or `null` members reject the complete artifact.

A `"package"` kind or a `package` member is invalid in v0.1 rather than a partially supported alternate form.

`CatalogScopeName` is exactly the 013 validated non-empty Unicode-scalar name. Its decoded UTF-8 bytes are retained and compared case-sensitively without trimming, Unicode normalization, case folding, separator rewriting, percent decoding, or locale-sensitive comparison.

A bare string such as `"app"`, a combined string such as `"project/app"` or `"com.example/app"`, an array, a request-local integer, a hash, and a producer-defined opaque object are not alternate wire spellings and are never normalized into this object.

`catalog_scope_name_bytes` is the field-specific inclusive `LinkLimits` counter with a fixed protocol ceiling of 255 decoded UTF-8 bytes for each `CatalogScopeName` occurrence. Its public evidence variant is the twentieth common counter, `CatalogScopeNameBytes`.

Because the scalar grammar is non-empty, the accepted default range is 1 through 255 bytes: exactly 255 is valid and the first byte above is rejected.

JSON decoders count after string unescaping and before constructing the checked newtype; direct constructors and the coordinated 013 configuration validator apply the same byte boundary.

A caller-selected lower value follows the common immutable-budget rule, so zero admits no scope name rather than creating an empty or implicit scope.

An overrun always records `Exact(effective_limit + 1)` and never scans the remaining scalar merely to recover its complete length or produces `ArithmeticOverflow`.

The subject is selected only from the occurrence's owning bounded context: `ReferenceArtifactGroup(identity)`, `DefinitionArtifactGroup(source)`, `ResolvedPolicy`, or `ScopeMappings`.

Evidence never retains the raw name, a partially constructed `CatalogScopeId`, a record or mapping index, or an arbitrary duplicate artifact occurrence. Context-specific validation phases decide precedence without changing the counter, ceiling, lower value, or observation contract.

The counter excludes the namespace payload, JSON quotes and escaping, object framing, and allocation capacity. Those bytes remain subject to their enclosing decoded or wire counters.

Equal names and interned storage do not bypass the per-occurrence validation or reduce enclosing aggregate accounting. No producer, decoder, configuration layer, mapping constructor, or linker truncates, hashes, aliases, replaces, or normalizes an over-limit value to recover.

Each linker-participating 013 catalog definition's `scope: "app"` resolves to `CatalogScopeId { namespace: Project, name: "app" }`.

The resolved configuration may intern equal complete structural values for memory and lookup efficiency, but an interner handle is process-local implementation state and never enters an artifact, fingerprint, cache key, finding, or structured output.

Definitions, producer recognizers, configured roots, and coverage-baseline entries resolve through the same host registry. Definition order, include patterns, matched paths, config location, source identity, reference-artifact identity, and producer identity never supply or modify the scope.

The scope namespace is carried independently rather than inherited from a surrounding definition source, reference artifact, producer id, delivery unit, or input position.

Producers and decoders preserve the explicit `Project` claim exactly; only the checked host mapping stage below may introduce semantic equivalence between declared project scopes. For example, `Project/vendor-checkout` and `Project/checkout` remain distinct until the consuming application maps the former to the latter.

Equality compares the fixed `Project` namespace and then the exact name bytes; canonical ordering therefore compares catalog scope names by decoded UTF-8 bytes.

The canonical fingerprint payload is a two-field nested record with namespace at tag `0x01` and exact scope-name bytes at tag `0x02`; internal interner handles, JSON spelling, and any future host mapping are absent.

The actual JSON bytes contribute to the enclosing artifact wire counter, and each scope-name payload contributes per occurrence to its decoded counter without interning deductions.

An unknown project-local name or a name bound only to entry-level catalogs fails configuration or artifact production before linking rather than creating an implicit scope. No producer, decoder, mapping table, or linker silently falls back from an unknown identity to an equal local name.

#### Scope mapping and semantic resolution

The final application may explicitly make independently named structural scopes participate in one semantic scope through an immutable host-owned mapping table:

```rust
pub struct ScopeMapping {
    pub source: CatalogScopeId,
    pub target: CatalogScopeId,
}

pub struct ScopeMappingTable(Vec<ScopeMapping>);

pub struct ResolvedCatalogScopeId(CatalogScopeId);
```

`ScopeMappingTable` is a link-invocation input, not a member of either artifact and not a mechanism for rewriting an artifact. A host builds it through the checked typed API after project binding. The M0-owned built-in CLI/editor orchestration used by later surfaces has no raw configuration spelling for this input and always constructs the canonical empty table; custom in-process build integrations may supply a non-empty checked table.

Its checked form is sorted by exact canonical `source` order and has at most one entry for each source. A duplicate source is invalid even when both targets are equal.

A self-map is invalid because omission already preserves identity. Every source and target must be a declared, host-validated scope in the same consuming-project registry; unknown or entry-level-only scopes are host-input failures rather than new declarations. For the M0-owned built-in workflow contract, only the empty table is constructible from product configuration.

Mapping is deliberately one hop. No target may also occur as a source anywhere in the same table, which rejects chains and cycles before scope resolution and makes entry order irrelevant. Multiple distinct admitted sources, up to the table limit below, may name one equal target, so explicit many-to-one equivalence is valid. An empty table is valid.

`scope_mapping_entries` is the exact structured spelling of the twenty-first common `LinkLimitCounter` variant, `ScopeMappingEntries`. It counts submitted `ScopeMapping` occurrences under a fixed protocol ceiling of `4,096` entries per link request.

Empty and exact-boundary tables are accepted when otherwise valid; the first occurrence above the effective limit rejects the complete table with `ScopeMappingEntries`, the payload-free `ScopeMappings` subject, and exactly `Exact(effective_limit + 1)`.

It never uses `Request`, an individual entry or endpoint, an entry index, a complete submitted count, or `ArithmeticOverflow`.

A structured adapter preflights a known collection length before retaining or validating entries by comparing it with the bounded effective limit without converting or reporting the complete length.

A streaming adapter stops before retaining the first excess entry, and direct typed construction applies the same preflight before sorting or semantic validation. Thus the default first-over case is the 4,097th occurrence and every ingestion path exposes identical evidence.

Every submitted occurrence counts, including a later-invalid unknown endpoint, duplicate source, self-map, chain, or cycle participant. Sorting, duplicate rejection, equal source or target values, many-to-one mapping, interning, and cache reuse provide no deduction.

A caller-selected lower value follows the common immutable-budget rule; zero admits only the empty table. Count preflight precedes endpoint validation, derived-byte accounting, canonical sorting, and semantic mapping validation.

Immediately after the count preflight, checked construction runs one complete, non-interleaved `catalog_scope_name_bytes` pass over the occurrence-preserving table in submitted entry order, visiting each source before its target.

Only after that complete pass succeeds does it validate the remaining endpoint namespace/name grammar and declared-registry membership in the same source-then-target order, reject duplicate sources, reject self-maps, reject every target that is also a source, and finally sort and construct the canonical table.

Decoder-level object/array shape validation may establish the typed endpoint envelopes needed to locate those names, but it cannot perform a later semantic check ahead of this local precedence. Parallel implementations may detect later failures provisionally and must expose only the serial winner.

M0 has no independent `scope_mapping_bytes` admission counter. A mapping payload contains exactly two `CatalogScopeName` values per entry and no variable-width namespace payload.

The existing ceilings therefore derive an inclusive maximum of `4,096 * 2 * 255 = 2,088,960` decoded name bytes for a table.

The decoder or constructor still validates and accounts for each source and then target in submitted order with checked `u64` addition before exposing the complete table; integer overflow or host-size conversion failure is an operational error.

A per-name overrun uses `CatalogScopeNameBytes` with the payload-free `ScopeMappings` subject and no raw endpoint or entry index. This accounting proves the derived bound and preserves deterministic validation order but does not introduce another caller-adjustable limit.

Equal endpoints, later-invalid entries, many-to-one targets, sorting, interning, and cache reuse provide no accounting deduction. A future package namespace must re-evaluate whether its additional variable payload warrants an independent aggregate byte counter.

Resolution is the exact function `resolve(scope) = target` when the table contains that structural source and `resolve(scope) = scope` otherwise.

It performs no transitive traversal, reverse lookup, name-only comparison, namespace inheritance, wildcard, prefix match, case fold, Unicode normalization, or closest-scope fallback. The target is the canonical semantic identity for that invocation; an unmapped scope remains its own canonical target.

The result is wrapped as `ResolvedCatalogScopeId` so implementation code cannot accidentally mix artifact identity with post-mapping semantic identity.

After complete artifact, trust, resolved-policy, and table validation but before definition ambiguity grouping, domain-consistency checks, reference matching, coverage, reachability, or placement, the linker resolves every `MessageReference.scope`, every `MessageDefinition.scope`, and every scope-keyed policy/completeness input through the same table.

A mapping that brings incompatible domains into one resolved scope fails the resolved input contract before semantic linking rather than silently partitioning that scope.

A mapping that brings two definitions onto the same resolved `(scope, domain, key, locale)` intentionally exposes the ordinary `ambiguous-message-definition` semantics instead of selecting a winner.

All semantic indexes, finding subjects, coverage identities, and plan selections use `ResolvedCatalogScopeId`.

The original `CatalogScopeId` remains byte-for-byte in its immutable artifact and is recoverable through reference-record or definition-location provenance; neither the linker nor host writes the target back into an artifact.

Artifact equality, canonical bytes, producer fingerprints, and artifact-cache keys therefore remain unchanged by project mapping.

Link-request/incremental-result cache identity includes the complete canonical mapping table, so changing only a mapping invalidates semantic results without invalidating extraction or producer artifacts.

The host constructs the table through a checked API before calling `link`, and `intlify_linker` defensively rejects any directly supplied table that violates the same invariants. Invalid mapping input is one fail-complete `LinkOperationalError` before semantic findings, plans, or cache admission.

No M0 `intlify.config` spelling produces this typed table. A future package-resource/composition addendum must define the one raw configuration field, schema, validation order, error evidence, and host-construction path together; it cannot weaken these semantics or introduce a second mapping path.

### Scope-level input completeness

Completeness is an explicit request input, not a property inferred from the artifacts that happened to arrive. One valid `MessageDefinitionArtifact` proves only that its one source was extracted completely, and one valid `MessageReferenceArtifact` proves only that its producer output is structurally complete.

Neither artifact can prove that every source or producer configured for its project scope participated in the current link.

The integration constructs the following checked, request-local model from the resolved target-scope inventory and the execution result of every configured catalog source and reference producer:

```rust
pub struct ScopeCompletenessTable {
    entries: Vec<ScopeCompleteness>,
}

pub struct ScopeCompleteness {
    scope: CatalogScopeId,
    definitions: InputCompleteness,
    references: InputCompleteness,
}

pub enum InputCompleteness {
    Closed,
    Partial(PartialReason),
}

pub enum PartialReason {
    OpenEditorWorld,
    SourceOmitted,
    SourceFailed,
    ProducerOmitted,
    ProducerFailed,
    ExternalArtifactUnverified,
}

pub enum CompletenessSide {
    Definitions,
    References,
}
```

All fields are private and exposed through read-only accessors. Checked construction requires exactly one entry for every application-owned scope targeted by the current link, rejects an extra or missing scope and an equal duplicate, and stores entries in canonical `CatalogScopeId` order.

The target inventory comes from resolved configuration and can include a scope with no artifact because a configured input was omitted or failed.

It is admitted and bounded by its owning configuration contract before table construction; the completeness table cannot add another scope or bypass that admission, so M0 adds no independent `LinkLimitCounter` or artifact wire member for it.

`Closed` has a positive meaning. On the definition side it means every catalog source selected for that exact scope completed extraction, binding, freshness checks, and artifact projection.

On the reference side it means every enabled producer selected for that scope completed its closed-world scan, including the valid case where all of them produced zero references. An exact M0 `producers.artifacts` declaration is a project-global producer participant: successful bounded decoding of its complete authoritative selected snapshot completes that participant for every target scope, while its individual records still retain their explicit scopes. The CLI selects the declared file bytes; the editor may instead select one unambiguous current open buffer for that same configured source under 009. `Closed` is never copied from an artifact field; for either snapshot it follows from the explicit inventory declaration plus successful processing of the complete selected bytes.

An integration derives it only after comparing the resolved inventory with complete successful execution results.

`Partial` records why the integration cannot make that proof. `SourceOmitted` and `SourceFailed` are valid only for `definitions`; `ProducerOmitted` and `ProducerFailed` are valid only for `references`; `OpenEditorWorld` and `ExternalArtifactUnverified` are valid for either side. A successfully decoded exact `producers.artifacts` disk snapshot, or one unambiguous configured open-buffer replacement selected under 009, is authoritative for that invocation and never receives `ExternalArtifactUnverified`. That reason is reserved for a cache, ad hoc editor overlay, package artifact, or another integration-supplied external value that is not tied to an authoritative current-invocation declaration. Invalid selected configured bytes are `ProducerFailed`; ambiguous live-source ownership is `OpenEditorWorld`, and neither case falls back to another snapshot.

A side-inapplicable reason rejects checked construction. When several causes affect one side, the integration chooses the deterministic most-specific reason in this order: failed input, omitted input, unverified external artifact, then intentionally open editor world.

The complete execution report remains integration-owned operational evidence; the table carries only the semantic fact needed by the linker.

Canonical `PartialReason` comparison uses declaration order above, while `CompletenessSide` orders `Definitions` before `References`; neither comparison order changes cause-selection priority.

Scope mapping is applied to completeness through the same one-hop function used for artifacts and policy. A resolved scope is definition-closed only when every original completeness entry that maps to it is definition-closed, and is reference-closed only when every such entry is reference-closed.

Mapping can therefore preserve or weaken closure but can never upgrade a partial source scope to `Closed`. When several partial entries merge, finding evidence retains every contributing original scope and reason in canonical scope/side/reason order even though the effective resolved status is simply partial.

Completeness-table validation runs after complete artifact, resolved-policy, scope-mapping, and delivery-graph validation and after the original target-scope inventory is known, but before resolved semantic indexes, ambiguity grouping, reference matching, coverage, reachability, or placement.

A malformed, noncanonical, incomplete, or inventory-mismatched table is one fail-complete `LinkOperationalError`. A well-formed `Partial` value is not an operational error: it deliberately permits useful partial lint and editor analysis.

The semantic gates are per resolved scope:

- Definition-partial scopes do not emit absence-dependent `unresolved-message`, `missing-translation`, or `orphaned-translation` findings. Present-definition ambiguity remains reportable.
- Reference-partial scopes do not emit `unused-message`. `unused-message` is emitted only when both definitions and references are closed for that scope.
- Each partial side emits one `degraded-analysis` finding whose typed subject is `(ResolvedCatalogScopeId, CompletenessSide)` and whose evidence is the complete non-empty canonical vector of contributing `(CatalogScopeId, PartialReason)` values. Completeness-derived degradation is linker-blocking for generation even though its proposed lint presentation remains `warn`; other degradation cases retain their independently specified disposition.
- `messages emit` and `MessageBundlePlan` generation require both sides to be `Closed` for every scope targeted by the link. Because `LinkOutcome` carries one all-or-nothing `bundle_plans` option, any targeted partial scope returns `bundle_plans: None` with the complete finding set.
- `messages prune` additionally requires no unbounded-dynamic degradation in the affected scope. Partial inputs and any inability to prove a bounded closed reference world forbid deletion rather than producing a partial deletion plan.

The complete canonical `ScopeCompletenessTable` participates in link-request and incremental-result cache identity. Changing only a side from `Closed` to `Partial`, changing its reason, or changing the target inventory invalidates semantic results without invalidating already verified extraction or producer artifacts.

### Linker core API and result boundary

`intlify_linker` exposes one immutable, stateless core operation. The conceptual Rust boundary is:

```rust
pub struct LinkRequest<'a> {
    pub reference_artifacts: &'a [MessageReferenceArtifact],
    pub definition_artifacts: &'a [MessageDefinitionArtifact],
    pub policy: &'a LinkPolicy,
    pub scope_mappings: &'a ScopeMappingTable,
    pub scope_completeness: &'a ScopeCompletenessTable,
    pub delivery_graph: &'a DeliveryUnitGraph,
    pub limits: &'a LinkLimits,
}

pub struct LinkOutcome {
    findings: Vec<LinkFinding>,
    bundle_plans: Option<Vec<MessageBundlePlan>>,
}

impl LinkOutcome {
    pub fn findings(&self) -> &[LinkFinding];
    pub fn bundle_plans(&self) -> Option<&[MessageBundlePlan]>;
    pub fn generation_blocked(&self) -> bool;
}

pub fn link(
    request: &LinkRequest<'_>,
) -> Result<LinkOutcome, LinkOperationalError>;
```

Artifact inputs use the private owned values and read-only slices fixed above, while `LinkRequest` borrows those complete immutable values for one call; exact private `Vec`-versus-boxed-slice storage remains an implementation detail.

`LinkOutcome` is a fully owned immutable result with no request lifetime. It retains the checked semantic values needed by every finding and plan, including each selected definition's exact decoded `MessagePayload` and `DefinitionLocation`, rather than borrowing an artifact, policy, mapping table, completeness table, graph, temporary index, or worker arena.

Its fields are private. Only `link` constructs it after complete semantic, result-limit, blocking, and plan invariants pass; there is no public constructor, struct literal, setter, mutable slice, deserializer, or post-construction revalidation route.

`findings()` returns the complete canonical read-only slice. `bundle_plans()` preserves the semantic distinction between `None`, `Some(empty)`, and `Some(non-empty)`. `generation_blocked()` is exactly `bundle_plans().is_none()` and is never stored independently.

The private implementation may use owned checked values, immutable arenas, interners, `Arc<str>`, boxed slices, or another equivalent representation. Repeated placements of one selected definition may therefore share immutable payload storage without changing the fully owned public result. Pointer identity, arena index, sharing topology, allocation capacity, and `Vec`-versus-boxed-slice choice are not public identity, ordering, result accounting, cache keys, serialization, or ABI promises.

Semantic finding-byte accounting remains per occurrence even when storage is shared. The public outcome and every value reachable through its read-only accessors have no interior mutability or process-global dependency and are `Send + Sync`.

The API does not require `LinkOutcome: Clone`. A caller that needs shared ownership wraps the complete result externally in `Arc<LinkOutcome>`; cross-call cache lifetime and eviction remain caller-owned.

The request/result shape and the one-source-per-definition-artifact granularity are fixed.

`LinkPolicy` is the fully resolved, milestone-specific typed policy input. M0 contains finite production locales, dynamic-reference decisions, configured roots, and placement. M1 adds its coverage contract, and M2 adds fallback chains. A pre-M2 type has no dormant fallback member or accepted fallback constructor input. The core never reads raw configuration.

`scope_mappings` is the checked one-hop semantic mapping above. It is applied uniformly to artifact, policy, and completeness scopes without mutating any input.

`scope_completeness` is the checked execution-derived table above and cannot be inferred from artifact presence.

`LinkLimits` is a separate immutable operational budget and never changes linking semantics for an admitted request.

A caller resolves configuration, runs producers and resource extraction, constructs the mapping, completeness, and delivery-graph inputs, and chooses its effective limits before invoking `link`; the core performs no filesystem I/O, source parsing, provider execution, exporter invocation, environment lookup, or mutation of caller data.

It keeps no process-global mutable state. Independent calls are reentrant and safe to run concurrently, while worker scheduling and cross-call caching belong to the caller.

### Resource-limit ownership

#### Protocol ceilings and construction

`intlify_contract` owns one platform-independent hard ceiling for every public artifact and aggregate-link counter. These ceilings define the largest interoperable request; they are not suggestions and do not vary by pointer width, available memory, producer, reporter, command, or trust level.

The caller constructs `LinkLimits` by selecting values less than or equal to every corresponding ceiling. Omitting an override uses that ceiling.

The checked constructor rejects a value above its ceiling rather than creating an invalid or silently clamped object, and no artifact field, producer claim, project configuration, or command option can raise it.

`LinkLimits` has private fields and no unchecked aggregate literal, public mutable field, lossy deserializer, or post-construction setter. Its minimum public construction boundary is:

```rust
pub struct LinkLimits {
    // private effective values for the closed LinkLimitCounter set
}

pub struct LinkLimitConfigurationError {
    counter: LinkLimitCounter,
    submitted: u64,
}

impl LinkLimits {
    pub fn protocol_defaults() -> Self;

    pub fn try_with_limit(
        self,
        counter: LinkLimitCounter,
        value: u64,
    ) -> Result<Self, LinkLimitConfigurationError>;

    pub fn effective_limit(&self, counter: LinkLimitCounter) -> u64;
}
```

`Default`, if implemented, is exactly `protocol_defaults()`. `try_with_limit` accepts zero and every value through that counter's inclusive protocol ceiling, returns a new complete immutable value, and rejects the first submitted value above the ceiling without clamping or changing the original value.

The error stores only the closed counter and submitted value; the ceiling and presentation text are derived from the counter. Configuration decoding must finish through this checked boundary before artifact production, decoding, cache admission, or `link` starts.

Consequently every public operation receives an already valid `&LinkLimits`: an invalid limits object never competes with wire, syntax, contract, or operational failures, and no artifact bytes are read to report a configuration mistake.

Duplicate or otherwise malformed raw configuration remains configuration-schema evidence outside this type; successive programmatic calls deliberately replace the selected effective value before the immutable object is shared.

The same counter vocabulary is shared across production, decoding, and linking. Producers enforce per-artifact ceilings and any lower caller budget before emitting an artifact.

Where the transport exposes a length, the `intlify_contract` decoder bounds raw wire bytes before complete decoding. It checks individual lengths and counts before allocating their payloads and validates cumulative decoded budgets incrementally.

Before constructing indexes or result storage, `intlify_linker` defensively applies the relevant checked contracts to already typed artifacts, policy, scope mappings, scope completeness, and the delivery graph.

All additions and host-size conversions are checked; integer overflow is an operational limit failure, never wrapping, saturation, or allocation-dependent behavior.

The inherited definition-source counters and the definition-request counters fixed below are normative; counters not fixed here follow in later envelope decisions.

#### Definition extraction limits

The definition producer inherits these inclusive source-local ceilings directly from 013:

| Counter | Initial protocol ceiling |
| --- | --: |
| `host_bytes` | 64 MiB (`67,108,864` bytes) |
| `entries` / emitted `definitions` | `100,000` |
| `message_bytes` for one definition's extracted UTF-8 message text | 1 MiB (`1,048,576` bytes) |
| `total_message_bytes` across one source artifact | 64 MiB (`67,108,864` bytes) |
| `identity_bytes` for distinct interned structural-path, catalog-key, and present display-key payloads | 64 MiB (`67,108,864` bytes) |

All five counters retain 013's exact admission order, distinct-string accounting, and inclusive-boundary semantics. Under protocol-default limits, every successful 013 extraction is therefore eligible for definition projection without a stricter duplicate source budget.

A caller-selected lower value may intentionally reject it. The built-in producer consumes the accepted extraction's counters and exact messages without reparsing the host document or maintaining a second interpretation of entry identity.

It emits one definition for every admitted entry; a payload that cannot represent one admitted message makes production fail-complete rather than reducing the definition count.

`host_bytes`, raw extracted message lengths, and 013-only display-key accounting are producer-side facts and are not added redundantly to the definition artifact merely to make a published consumer trust a claim. A local cache owner with the source can recheck them while recomputing the fingerprint.

A remote consumer validates every observable projected count and payload under the artifact-wire and decoded-budget counters fixed separately; it does not claim to have verified unavailable host bytes.

The open limits work therefore covers reference artifacts, serialized/decoded definition expansion, aliases and paths, aggregate request counts, the delivery graph, and bounded output construction — not replacement values for these five source-local ceilings.

#### Definition artifact byte budgets

Definition-artifact admission uses two independent, inclusive byte counters:

| `LinkLimits` counter | Accounting rule | Initial protocol ceiling |
| --- | --- | --: |
| `definition_artifact_wire_bytes` | Exact bytes in one uncompressed serialized `MessageDefinitionArtifact` JSON document supplied to the `intlify_contract` decoder, including member names, punctuation, number tokens, string escaping, whitespace, and trailing or otherwise invalid input bytes that the decoder receives. External transport or container compression is not artifact wire; integrations must feed its decompressed output through the bounded decoder without first materializing an unbounded buffer. | 512 MiB (`536,870,912` bytes) |
| `definition_artifact_decoded_bytes` | Sum of the exact byte lengths of every variable-width scalar payload in the final logical typed artifact. This includes decoded UTF-8 strings and opaque byte strings after wire unescaping, binary decoding, and any artifact-local table or reference expansion. Each logical occurrence is charged independently. | 256 MiB (`268,435,456` bytes) |

An artifact must pass both counters; neither is an estimate of or substitute for the other.

The 256 MiB decoded ceiling leaves bounded composition headroom above the 64 MiB source-local message, identity, and source-path classes, while the 512 MiB wire ceiling permits additional framing and escaping growth without defining a required two-to-one relationship.

An artifact may independently exceed either counter. `definition_artifact_decoded_bytes` includes message, locale, identity, source-path, alias, producer, and other variable-width payloads even where a more specific counter also applies, so all overlapping limits must pass.

Every decoded segment occurrence in the primary path and every submitted alias contributes its UTF-8 bytes exactly once to this enclosing total, including a repeated alias that later fails canonical-set validation.

`PathSegmentBytes`, `PathBytes`, and `SourcePathBytes` are overlapping admission views of those same payload bytes; their observed or derived counter values are not added again, so no segment is double- or triple-charged.

Array framing, path/segment counts, and the fixed namespace discriminant add zero decoded bytes, while their actual JSON syntax contributes only to `definition_artifact_wire_bytes`. Repeated values, shared string-table entries, zero-copy slices, and interner reuse do not reduce the decoded charge.

Container framing, fixed-width integers, and enum discriminants add no decoded bytes; record and collection growth is bounded by separate count limits, while linker indexes, graph state, and output buffers belong to later aggregate and output counters.

Wire-byte admission is phase zero before root syntax and every decoded or semantic check. When the uncompressed document length is known, the decoder compares it before parsing.

For an unknown-length stream, it counts through EOF or the first byte at `effective_limit + 1`; a syntax error discovered earlier remains provisional until that bounded wire admission finishes.

Reaching the first excess byte selects `DefinitionArtifactWireBytes` over every syntax or decoded failure and stops without reading the remainder; reaching EOF at or below the limit permits the previously selected canonical syntax result.

Chunking, parser lookahead, and known-versus-streaming ingestion therefore cannot change the public failure. Direct typed construction has no wire phase or synthetic wire charge.

A producer validates the decoded counter over the logical artifact and validates the wire counter through a bounded counting encoder before publication, without exposing partial output. A decoder counts exact bytes incrementally and charges each decoded scalar before allocating or admitting that value.

Direct typed construction through `intlify_contract` has no synthetic wire-byte charge, but its checked constructors enforce the decoded and structural limits. `intlify_linker` can therefore defensively recompute observable decoded charges from typed artifacts but does not invent a wire length after decoding.

Caller-selected lower values for the two counters remain independent.

Decoded-budget failure selection follows the canonical definition-artifact envelope and field phases below, never input object-member order or the point at which a streaming parser encounters bytes.

Within each phase, the applicable shape and field-specific limit checks run first; only an admitted complete variable-width payload is then added once, with checked arithmetic, to the artifact's decoded running total.

If that addition exceeds the effective `definition_artifact_decoded_bytes` ceiling, the per-artifact decoded-budget failure wins over every later phase but never over an unfinished earlier phase.

A decoder may measure and mark provisional charges in wire order, but it exposes that failure only after all logically preceding phases have passed.

It uses bounded staging, skips retention of payload beyond the effective budget, and may continue the bounded syntax scan needed to resolve earlier-phase validity; it never allocates decoded storage past the effective ceiling merely to preserve precedence.

Producers, direct checked constructors, cache revalidation, defensive linker validation, partitioned implementations, and parallel implementations select the same canonical phase result.

#### Definition artifact validation precedence

Within complete definition-artifact validation, the decoder uses one member-order-independent envelope precedence after root syntax and duplicate-member checks: exact `kind`, supported `ArtifactVersion`, complete `ProducerIdentity`, checked primary `SourceDocumentIdentity`, the fixed alias count/per-path numeric/cumulative-byte/grammar/canonical-set phases, and then structural `InputFingerprint` validation.

It only then preflights the submitted `definitions` length. Structural fingerprint validation checks the exact algorithm/digest object and decoded digest but does not recompute freshness.

The decoder subsequently runs four complete, non-interleaved field-byte passes over definitions in preserved 013 raw-entry order: first `locale_bytes`, then `catalog_scope_name_bytes`, then `entry_structural_path_bytes`, then `catalog_key_bytes`.

It next runs one complete message-admission pass in the same raw-entry order. For each definition, that pass checks `message_bytes` first and, only after the per-value check succeeds, adds the complete message length to the one artifact-local `total_message_bytes` running counter.

It stops at the first failing record, so an earlier record's total overrun wins over a later record's per-message overrun, while the same record's per-message overrun wins before its prospective total addition.

A producer failure wins over every source-envelope failure, an alias-set failure wins over malformed fingerprint or definition payload, and a malformed fingerprint wins over a `definitions` length or record failure even when the latter is discoverable from a known array length.

This preserves source-envelope validation as one complete phase, the already fixed locale-byte precedence, scope-namespace validation before key-bearing identity fields, and 013's per-entry message-then-running-total order.

A cache owner with the complete canonical fingerprint inputs may recompute only after structural admission and treats mismatch as stale/cache-miss evidence; `intlify_linker` and a remote consumer never fabricate unavailable inputs or turn mismatch into a semantic finding.

Only after all five passes succeed does validation apply locale and scope-name grammar, domain-specific catalog-key grammar, entry-reference continuity, and the remaining definition-record semantics.

A failure in an earlier pass wins even when a later counter would fail at an earlier definition occurrence; decoder strategy, JSON member order, partitioning, and worker completion cannot interleave or reorder the passes.

A zero-length locale or scope name passes its maximum-byte comparison and is then rejected by its non-empty scalar grammar; any non-empty value under an effective lower value of zero fails at its first byte.

Empty structural paths, domain-valid empty root catalog keys, and empty messages pass their respective byte checks.

A protocol or lower-budget per-value overrun returns its exact `LocaleBytes`, `CatalogScopeNameBytes`, `EntryStructuralPathBytes`, `CatalogKeyBytes`, or `MessageBytes` variant with `DefinitionArtifactGroup(source)` and `Exact(effective_limit + 1)`.

A running message-total overrun instead returns `TotalMessageBytes` with the same subject and the exact attempted sum. Neither form retains a submitted scalar, `CatalogScopeId`, `EntryReference`, definition index, or occurrence index.

When linker-side lower-budget revalidation sees several artifact groups, canonical `SourceDocumentIdentity` order selects the first failing group for the current pass; equal-source occurrences retain the same group subject and observation, so their input or worker order cannot change public evidence.

These local passes remain inside definition-request phase 2 and do not move ahead of the request's `definition_artifacts` preflight.

#### Definition request aggregate limits

One checked `LinkRequest` applies three independent, inclusive definition-set counters:

| `LinkLimits` counter | Accounting rule | Initial protocol ceiling |
| --- | --- | --: |
| `definition_artifacts` | Number of submitted `MessageDefinitionArtifact` occurrences in the request. | `65,536` |
| `definitions_total` | Number of submitted `MessageDefinition` record occurrences across all definition artifacts in the request. | `4,000,000` |
| `definition_artifact_decoded_bytes_total` | Sum of the defensively recomputed `definition_artifact_decoded_bytes` charge for every submitted typed definition-artifact occurrence. | 1 GiB (`1,073,741,824` bytes) |

An empty definition-artifact collection and every exact boundary are valid; the first occurrence or byte above an effective limit rejects the complete request.

At the protocol defaults, four artifacts charged at exactly 256 MiB each fit the decoded total. Any positive decoded charge from a fifth artifact exceeds it.

`definition_artifacts = 0` admits only an empty collection.

`definitions_total = 0` may admit structurally valid artifacts only when every `definitions` array is empty.

`definition_artifact_decoded_bytes_total = 0` admits only an empty collection because every structurally valid definition artifact retains non-empty variable-width source and producer identity payloads.

A caller may independently select a lower value for any of the three counters.

Every submitted occurrence is charged even when a later duplicate-identity, cross-artifact, or semantic check rejects it. Sorting, filtering, deduplication, interning, cache reuse, partitioning, and physical parallelism never reduce the count or decoded-byte charge.

Direct typed construction receives no synthetic wire charge, but the linker recomputes each observable decoded charge and adds it exactly once with checked arithmetic before constructing definition indexes or result storage.

The linker core deliberately has no `definition_artifact_wire_bytes_total` counter.

Each serialized document must still pass `definition_artifact_wire_bytes`, while the integration transport or ingestion contract owns a bounded total for decompressed batch/stream bytes, buffering, concurrency, and cancellation before checked artifacts reach `link`.

That transport total is not stored in `LinkLimits`, artifact identity, `ArtifactVersion`, `InputFingerprint`, or linker cache inputs, and the core never synthesizes it by canonical re-encoding.

Definition-artifact request admission uses this exact, non-interleaved precedence:

1. preflight the `definition_artifacts` collection length;
2. complete structural and effective per-artifact validation for every submitted artifact;
3. complete the `definitions_total` aggregate pass;
4. complete the `definition_artifact_decoded_bytes_total` aggregate pass; and
5. perform duplicate-source-identity detection, other cross-artifact validation, definition-index construction, and semantic analysis.

A failure in an earlier phase or aggregate counter always wins even if a later counter would exceed its ceiling at an earlier source identity. Implementations may compute provisional charges early, but they may not expose, cache-admit, or select a later-phase failure before every preceding phase has passed.

Both aggregate passes group admitted artifacts by the exact `SourceDocumentIdentity` contract ordering and visit those groups canonically.

That ordering compares the namespace discriminant first and then compares `PortableRelativePath` segments one by one by exact UTF-8 bytes, with a shorter equal-prefix path first; M0's only valid namespace is `Project`. Occurrences with one equal source identity form one contiguous group.

The `definitions_total` pass adds every occurrence's submitted `definitions.len()` charge into a checked group subtotal; the decoded-total pass likewise adds every occurrence's recomputed `definition_artifact_decoded_bytes` charge.

Each pass then adds the group subtotal once to its request counter without deduplicating any occurrence and stops at the first canonical group whose addition exceeds the effective limit.

Equal-group input permutations, hash-map iteration, worker partitioning, and racing completion cannot change the selected counter, source-identity group, or attempted value from the serial canonical reduction.

A later decoded-byte crossing at an earlier source identity never beats an earlier `definitions_total` failure.

A `definition_artifacts` collection-count failure uses `LinkLimitSubject::Request`, because the preflight has no validated individual artifact to blame.

A failure from either aggregate pass instead retains exactly `LinkLimitSubject::DefinitionArtifactGroup(source)` for the canonical equal-source group whose checked addition selected it.

It never chooses an arbitrary duplicate occurrence or carries every contributing artifact, an input index, producer, fingerprint, definition record, message payload, or worker identity.

The three counters are the closed `DefinitionArtifacts`, `DefinitionsTotal`, and `DefinitionArtifactDecodedBytesTotal` variants and use the exact structured-adapter spellings fixed in the common evidence contract above.

#### Portable path and alias limits

Portable source identity uses five independent, inclusive counters. The first three apply to every `PortableRelativePath` carried by a `SourceDocumentIdentity`, including a reference origin; the final two apply only to the primary-plus-alias set in one definition artifact:

| `LinkLimits` counter | Accounting rule | Initial protocol ceiling |
| --- | --- | --: |
| `path_segments` | Number of segments in one non-empty source path or definition alias. | `1,024` |
| `path_segment_bytes` | Exact decoded UTF-8 bytes in one validated segment of any source path or definition alias. | 4 KiB (`4,096` bytes) |
| `path_bytes` | Sum of segment UTF-8 byte lengths in one source path or definition alias; array framing and display separators add zero. | 256 KiB (`262,144` bytes) |
| `logical_aliases` | Number of alias paths in one definition artifact; the primary path is not an alias. | `4,096` |
| `source_path_bytes` | Sum of `path_bytes` for the primary path followed by every alias in canonical alias order. | 64 MiB (`67,108,864` bytes) |

These counters are platform-independent and apply after wire strings are decoded but before path newtypes or alias storage are admitted. They do not consult `PATH_MAX`, `NAME_MAX`, Windows extended-path rules, the current filesystem, or display rendering.

The first three use the twenty-seventh through twenty-ninth common variants `PathSegments`, `PathSegmentBytes`, and `PathBytes`, with exact structured spellings `path_segments`, `path_segment_bytes`, and `path_bytes`.

The final two use the thirtieth and thirty-first common variants `LogicalAliases` and `SourcePathBytes`, with exact spellings `logical_aliases` and `source_path_bytes`.

Within one path, admission first preflights the submitted segment count, then performs the complete segment-byte checks in segment order, then adds each admitted complete segment in the same order to the per-path byte total.

`PathSegments`, `PathSegmentBytes`, and `LogicalAliases` return exactly `Exact(effective_limit + 1)`; `PathBytes` and `SourcePathBytes` return their exact attempted running totals after the current complete segment or path. None can produce `ArithmeticOverflow`.

A definition primary path always uses the payload-free `DefinitionArtifactEnvelope` subject, including direct, cached, and lower-budget revalidation routes that already possess an eventual checked source.

Its segment-count, complete segment-byte, and per-path-total phases run first, followed by the remaining primary-path grammar needed to establish the checked `SourceDocumentIdentity`; a primary failure therefore wins over every alias failure.

After that identity exists, `LogicalAliases`, every alias-path counter, and `SourcePathBytes` use exactly `DefinitionArtifactGroup(source)`. A reference origin uses its already established `ReferenceArtifactGroup(identity)`.

No path or alias-set failure retains a raw or partial path, segment, alias index, record ordinal, complete submitted alias count, or full submitted byte length.

Definition alias admission then uses the following non-interleaved phases. First, one `logical_aliases` preflight counts every submitted alias occurrence before validating any alias or checking canonical order or duplicates; the primary path is excluded.

It accepts an empty list and exactly 4,096 aliases, stops before retaining the first excess alias, and reports exactly `Exact(effective_limit + 1)`. A caller-selected lower limit of zero therefore permits only an empty alias list.

Second, one complete `path_segments` pass visits all aliases in submitted array order, which a valid artifact must already make canonical. Third, one complete `path_segment_bytes` pass visits aliases in that order and decoded segments in segment order.

Fourth, one complete `path_bytes` pass visits aliases in the same order and resets its running total for each alias. An earlier complete counter pass wins over every later pass regardless of which alias would fail first within the later pass.

Fifth, one complete `source_path_bytes` pass adds the already admitted `path_bytes` for the primary path and then every alias in submitted array order. It charges every submitted complete path occurrence independently: shared prefixes, repeated segment strings, and interner reuse do not reduce it.

The exact attempted running total after adding the current complete path is retained, not `effective_limit + 1`; the maximum first rejected value is `67,371,008`, so `ArithmeticOverflow` is unconstructible. A lower limit of zero rejects every otherwise valid definition artifact because its primary path is non-empty.

Only after all five numeric alias/set phases succeed does one complete alias-grammar pass visit every alias in submitted array order and every segment in segment order. It applies the shared `PortableRelativePath` rules, including non-empty path and segment requirements and rejection of `.

`, `.. `, `U+0000`, and `/`, before any alias-order comparison.

Consequently, a grammar failure in a later alias wins over an ordering or duplicate failure near the front of the array. After all aliases are checked paths, one canonical-set pass compares adjacent paths in `[source.path] + logicalAliases` using the exact existing `PortableRelativePath` ordering.

Every adjacent pair must be strictly increasing: equality is a duplicate logical identity, descending order is a noncanonical alias-order error, and the first violating adjacent pair in submitted sequence selects the failure. The decoder never sorts, deduplicates, drops an alias, or selects a different primary.

Remaining physical binding and artifact semantics follow. Sequential, cached, partitioned, and parallel implementations must reproduce these phases, submitted ordering, and the same selected failure.

Empty `logical_aliases` remains valid; an empty source path is structurally invalid regardless of limits. A definition artifact can carry at most `4,097` total primary-plus-alias paths only when their combined bytes also remain within 64 MiB.

A reference origin applies the three per-path counters to its one path and does not consume the definition-only alias or cumulative counter.

The definition producer applies all five counters to the complete participating physical-source group before extraction. If any ceiling or lower caller budget is exceeded, it rejects the group without extracting, truncating aliases, selecting a smaller subset, or changing the primary path.

A reference producer applies the first three before admitting each present origin. The published consumer repeats all observable checks and additionally requires a definition alias sequence to remain sorted and duplicate-free.

Invalid path syntax, duplicate logical identity, and resource overrun remain distinct validation causes; their exact deterministic precedence is fixed with the numeric ceilings and operational-error contract.

#### Resolved policy limits

The resolved link-policy input uses milestone-gated inclusive numeric collection ceilings. M0 admits `production_locales` and `configured_roots`. The common counter registry reserves the two fallback rows, but only M2 activates their limits and typed inputs. The active post-M2 set is:

| `LinkLimits` counter | Counted unit | Initial protocol ceiling |
| --- | --- | --: |
| `production_locales` | One submitted production-locale occurrence. | `1,024` |
| `fallback_sources` | One submitted fallback-map source occurrence. | `1,024` |
| `configured_roots` | One submitted root occurrence to be resolved into the common scope, domain, selector, and optional reason vocabulary. | `4,096` |
| `fallback_targets_per_source` | One submitted ordered target-locale occurrence in one source locale's fallback sequence. | `64` |

Every submitted locale occurrence counted by an admitted locale-bearing row independently passes the shared 1-through-255-byte `locale_bytes` field contract before a checked `Locale` is retained.

Collection counts and per-value bytes are overlapping limits: passing one does not bypass the other, and equal locale spelling or later membership in the production set gives no deduction.

Each exact numeric boundary is admissible and the first count above it rejects the complete policy/request before work or storage proportional to that excess is admitted.

A valid M0 resolved policy has at least one and at most 1,024 production locales; the linker has no command-dependent empty-policy exception for lint or another analysis-only caller.

Beginning with M2, every fallback-map source must be one of those production locales, and omitting a production locale from the map means that locale has no fallback sequence. Every target retained in a fallback sequence must also be a member of the same production-locale set.

An out-of-set source or target rejects the complete policy rather than being ignored, inferred from catalog definitions, or retained as an implicit resolution-only locale.

A future distinction between emitted and resolution-only locales requires an explicit versioned policy contract rather than widening this set silently.

After construction of checked `Locale` values, the production-locale collection must be duplicate-free. Beginning with M2, the fallback mapping may contain at most one source entry for each equal locale.

An equal duplicate rejects the complete policy; first-wins, last-wins, source-order override, sorting-based retention, and silent set/map normalization are forbidden. This semantic rejection occurs after the submitted collection counters have already charged every occurrence.

A configured root's semantic identity is exactly its checked `(CatalogScopeId, CatalogKeyDomain, MessageSelector)` tuple. `reason` is optional declaration evidence and does not participate in identity or reachability.

Two roots with the same tuple reject the complete policy even when their reasons differ; the resolver never runs both, chooses one reason, or merges reasons into a new multi-reason representation. Both submitted roots remain charged to the collection limit before duplicate detection.

Beginning with M2, each fallback array is the complete ordered fallback sequence for its one source locale, not an adjacency list. Resolution for source `S` considers `S` first and then the array's targets exactly once in stored order.

It never recursively splices in a target locale's own configured sequence; that other sequence applies only when the target locale itself is the resolution source. The source locale cannot appear in its own array, and one target locale cannot appear twice in the same array.

Either case rejects the complete policy after accounting. Reciprocal references across different source arrays are finite and valid under this non-recursive model rather than being a graph cycle: for example, `en: [fr]` and `fr: [en]` each define a separate two-locale resolution order.

M2 performs no global fallback-DAG expansion, cycle breaking, visited-set truncation, or inferred transitive suffix. Omission is the only canonical no-fallback form.

A submitted explicit empty fallback array first charges its source occurrence and passes the zero target-count boundary, then rejects the complete policy; it is never accepted, normalized to omission, or retained as a second representation of the same semantics.

Input order is non-semantic for the production-locale set and configured-root set and, beginning with M2, for the fallback-source mapping.

After their submitted limits, scalar checks, membership checks, and duplicate rules pass, checked construction sorts production locales by the exact UTF-8 bytes of their checked `Locale` spelling; sorts fallback entries by the same source-locale order; and sorts roots by the exact contract order of `(CatalogScopeId, CatalogKeyDomain, MessageSelector)`, carrying each root's optional reason with it.

It applies no host collation, case folding, Unicode normalization, configuration-enumeration order, or hash-map order. In contrast, each fallback target array's order is semantic fallback priority and is retained exactly; it is never sorted or inferred from the ordering of another source's chain.

Equivalent collection permutations therefore produce the same checked policy while different target orders remain different policies.

Every collection ceiling charges submitted occurrences before scalar validation, membership checks, sorting, canonicalization, duplicate rejection, or any semantic use.

A duplicate, malformed, out-of-set, or otherwise later-rejected locale, fallback source, fallback target, or root therefore consumes one unit and receives no deduction from map overwrite, filtering, deduplication, equality, interning, cache reuse, partitioning, or parallel validation.

Configuration decoders must detect and count duplicate object members before a map-like representation could overwrite them, and direct checked construction must expose an occurrence-preserving bounded input rather than accepting an already lossy map.

Known lengths are preflighted before retaining proportional values; streaming adapters stop before admitting the first occurrence above the applicable effective limit. Each collection supports the common caller-selected lower immutable budget rule.

M0 resolved-policy admission uses this exact, non-interleaved precedence:

1. Preflight `production_locales`.
2. Preflight `configured_roots`.
3. Run a complete `locale_bytes` pass over submitted production-locale occurrences.
4. Validate the remaining production-locale grammar, reject the empty set and equal checked duplicates, and construct the canonical production set.
5. Run one complete `catalog_scope_name_bytes` pass over configured roots in submitted root order.
6. Validate the remaining root fields, reject equal checked `(scope, domain, selector)` duplicates, and canonically order roots.
7. Perform the remaining M0 policy-dependent link semantics.

M1 inserts its coverage-baseline admission only at the explicit position fixed by the M1 addendum. Beginning with M2, resolved-policy admission uses this exact, non-interleaved precedence:

1. Preflight `production_locales`.
2. Preflight `fallback_sources`.
3. Preflight `configured_roots`.
4. Run a complete `locale_bytes` pass over submitted production-locale occurrences.
5. Validate the remaining production-locale grammar, reject the empty set and equal checked duplicates, and construct the canonical production set.
6. Run a complete `locale_bytes` pass over submitted fallback-source occurrences.
7. Validate the remaining fallback-source grammar and production-set membership, reject equal checked source duplicates, and establish canonical source order.
8. Run one complete `fallback_targets_per_source` count pass in that source order.
9. Run one complete `locale_bytes` pass in canonical source order and declared target-priority order within each sequence.
10. Validate the remaining target grammar, membership, non-empty-chain, self-reference, repeated-target, and ordered-chain semantics.
11. Run one complete `catalog_scope_name_bytes` pass over configured roots in submitted root order.
12. Validate the remaining root fields, reject equal checked `(scope, domain, selector)` duplicates, and canonically order roots.
13. Perform the remaining policy-dependent link semantics.

An earlier phase always wins even when a later failure occurs in an earlier submitted member. Implementations may provisionally detect later failures concurrently, but they expose only the serial winner.

Every policy `LocaleBytes` or `CatalogScopeNameBytes` failure uses `ResolvedPolicy` and `Exact(effective_limit + 1)`, so production, source, or root collection permutation and racing workers cannot expose a raw value or change the evidence; target traversal and the root byte pass nevertheless follow their fixed serial orders.

The M2 phase 11 covers configured roots because their shape and 4,096-occurrence bound are fixed.

A future locale-bearing coverage-baseline mapping or another scope-bearing policy collection does not silently join that pass: its owning design first fixes an occurrence-preserving representation, its own count preflight and ceiling, and an explicit position in resolved-policy precedence, then reuses `CatalogScopeNameBytes` with `ResolvedPolicy`.

Rejecting equal checked fallback sources in phase 7 deliberately precedes the per-source target limit in phase 8. Until the source is valid and unique, no single target sequence or `FallbackSource(locale)` subject can be selected without depending on an arbitrary duplicate occurrence.

This precedence does not authorize unbounded ingestion: a decoder or checked constructor must detect a sequence overrun with bounded state while reading it and may defer only selection of the public error.

Once sources are unique, the target-count pass selects the first over-limit source in canonical checked-locale order, and sequential, partitioned, and parallel implementations must return the same source and observation.

`ProductionLocales`, `FallbackSources`, and `ConfiguredRoots` failures use `LinkLimitSubject::ResolvedPolicy`, because their collection-length preflights have no validated individual member to blame.

A `FallbackTargetsPerSource` failure instead retains exactly `LinkLimitSubject::FallbackSource(locale)` for the canonical checked source selected in phase 8.

Every policy-owned `LocaleBytes` failure uses `ResolvedPolicy`, including one selected while traversing a target sequence; it never switches to `FallbackSource` or retains the invalid locale. Every configured-root `CatalogScopeNameBytes` failure likewise uses `ResolvedPolicy` and retains neither the root nor its scope.

None of these subjects carries a source occurrence index, complete target list, target locale, configuration-member order, or worker identity.

`fallback_sources` is a distinct counter with a 1,024 hard ceiling rather than an alias for `production_locales`: the two collections have different units and may have independent lower budgets and failures.

Because a valid fallback map has at most one sequence for each production locale and each sequence has at most 64 targets, the current ceilings derive a maximum of 65,536 fallback targets.

M2 deliberately adds no independent `fallback_entries_total` counter, `LinkLimits` field, lower override, or operational-evidence variant: the source and per-sequence limits already bound storage and work. A future aggregate budget requires a concrete need and an explicit compatibility decision.

M0's emittable policy-counter subset contains only `ProductionLocales` and `ConfiguredRoots`. The registry also contains the reserved `FallbackSources` and `FallbackTargetsPerSource` variants to preserve later ordinals, but no M0/M1 policy, limit override, or error path can construct them. M2 activates those variants in their local admission order. Each active variant uses the exact structured spelling in the table.

The 4,096-root ceiling treats configuration roots as bounded exceptional reachability declarations; a larger generated reference corpus should normally be supplied by a reference producer artifact so it retains producer, source, and delivery-unit evidence. The numeric ceiling does not otherwise change root semantics.

#### Failure and retry behavior

Any overrun is fail-complete. Artifact production emits no partial artifact; decoding returns no partially trusted value; `link` returns one `LinkOperationalError` with neither findings nor plans.

Validation uses a documented deterministic counter and input order when several limits could fail, and stops once the winning overrun is established without scanning an unbounded remainder merely to report a larger `actual`.

A caller may retry the same immutable inputs with a larger valid lower limit, but the linker never retries or relaxes limits internally.

#### Lower-limit cache revalidation

Effective lower limits are invocation/cache-admission policy, not artifact semantics.

A checked `MessageReferenceArtifact` or `MessageDefinitionArtifact` is always structurally valid under the protocol hard ceilings, but it does not store, adopt, or expose the lower `LinkLimits` value that happened to admit one earlier invocation.

That value is not serialized, included in `InputFingerprint`, used in artifact/source/message identity or equality, or made part of canonical bytes.

Raising a protocol hard ceiling so a newer writer may emit formerly invalid artifacts requires at least an artifact minor-version compatibility decision, while lowering a ceiling within the same major is forbidden because it would invalidate previously conformant artifacts.

Every producer emission, decoder return, checked-constructor call, cache admission, and `link` invocation compares the artifact against that operation's current immutable effective limits. A previous acceptance never bypasses this comparison.

Revalidation follows the same canonical counter and field phases and derives failure evidence from the current effective limit, so moving to a stricter budget may reject a cached artifact while moving to a more permissive valid budget may admit it without changing the artifact.

An implementation may cache a complete contract-revision-tagged accounting summary or recompute usage from the immutable semantic value; either route must retain enough canonical per-phase information to reproduce the same first failing counter and attempted value under every lower budget.

A lone prior pass/fail bit, the previous effective limit, or only one artifact-wide total is insufficient and cannot be used as proof for another budget.

The summary is implementation/cache metadata only: it contributes to neither artifact equality nor fingerprinting, and an absent or revision-mismatched summary causes deterministic recomputation rather than trust or rejection.

Wire-byte limits remain unrecoverable from a typed artifact and are checked only during production or serialized ingestion; typed reuse revalidates every observable decoded and structural counter but never invents a wire charge.

### Link outcome semantics

Semantic findings are successful analysis data, not operational failures. `link` completes deterministic analysis and returns `Ok(LinkOutcome)` even when one or more findings have linker-owned blocking disposition.

If no finding blocks generation, `bundle_plans` is `Some`, including `Some(Vec::new())` for a valid link with no output plans. If any finding blocks generation, it is `None`; the outcome still contains the complete deterministically ordered finding set.

Examples include `ambiguous-message-definition`, `unresolved-message`, strict-mode `unbounded-dynamic-reference`, and completeness-derived `degraded-analysis`. Initial coverage findings remain non-blocking under the fixed matrix below. This blocking disposition governs plan validity and is independent of configurable lint severity.

`Err(LinkOperationalError)` is reserved for a request that cannot be analyzed under the contract: malformed or incompatible artifact data, an unsupported selector/domain-contract version, an invalid resolved policy, scope mapping table, scope completeness table, or delivery graph, a resource-limit failure, or an internal invariant failure.

It returns neither partial findings nor partial plans. Producer, resource-extraction, configuration-I/O, and exporter failures occur outside this call and remain owned by their respective layers; an integration maps them together with `LinkOperationalError` into its user-facing operational-error surface.

### Finding result limits

Input record limits do not bound the semantic result. One bounded selector can match many keys, each key can generate locale-specific coverage findings, and one ambiguity finding can retain many definition locations.

M0 therefore adds two independent inclusive request-result counters:

| `LinkLimits` counter | Accounting rule | Initial protocol ceiling |
| --- | --- | --: |
| `findings_total` | Number of final retained `LinkFinding` records after all semantic suppression. | `1,000,000` |
| `finding_bytes_total` | Sum of variable-length semantic payload bytes retained by those final subjects and evidence values. | 256 MiB (`268,435,456` bytes) |

The protocol ceilings are independent of the higher artifact-input record and decoded-byte ceilings. They deliberately prevent one valid large input from multiplying into an unbounded result.

A caller may select either lower immutable effective value through `LinkLimits`, including zero, but cannot raise either ceiling. The initial CLI exposes no config field, command option, environment override, reporter override, or target-specific value and uses both protocol defaults.

#### Final-candidate accounting

Finding detection first applies every semantic suppression rule, including ambiguity suppression and completeness gating. Only findings that would occur in the complete canonical `LinkOutcome::findings()` slice are result candidates.

Suppressed intermediate facts do not count. An implementation cannot change accounting by eagerly constructing a finding that another conforming implementation suppresses before construction.

The result phase is exactly:

1. determine the complete final candidate set and its canonical kind/within-kind order without materializing a machine DTO;
2. preflight `findings_total`;
3. if the count is admissible, visit every candidate in canonical order and account `finding_bytes_total`; and
4. only after both complete checks succeed, admit the canonical finding set and derive whether any finding blocks plan generation.

`findings_total` counts each retained record once, including equal-looking findings with distinct typed identity, non-blocking findings, and separate locale-definition findings. It takes no deductions for presentation grouping, shared evidence, repeated references, interning, cache reuse, worker partitioning, or target count.

A known candidate count above the effective ceiling is reported as exactly `effective_limit + 1`; the linker does not enumerate later candidates merely to expose their complete count. Count admission precedes all finding-result byte accounting, allocation proportional to the final finding vector, plan construction, and machine adaptation.

#### Semantic byte accounting

`finding_bytes_total` counts the exact decoded UTF-8 payload bytes of every variable-length checked value each time that value occurs in a retained subject or evidence object.

It includes, as applicable:

- catalog scope names, canonical catalog keys, locales, selector payloads, and free-form `ReasonText`;
- reference-artifact and delivery-unit segment payloads;
- source-document path segments and entry structural paths;
- origin source paths;
- every requested, probed, selected, baseline, and definition locale occurrence;
- every ambiguity location occurrence; and
- every partial-completeness contributor scope occurrence.

Equal values are charged independently across findings and independently when one finding repeats the value in different semantic fields. For example, a selected locale repeated as the last probed locale contributes twice because both fields are retained.

The counter excludes:

- JSON framing, member names, quotes, escapes, commas, whitespace, and other reporter serialization;
- fixed finding/evidence kind tokens, namespace discriminants, catalog-domain variants, completeness-side and partial-reason variants, dynamic-mode variants, and `blocking`;
- fixed-width ordinals, occurrences, and UTF-8 span integers;
- vector, enum, struct, pointer, allocator, interner, or `Arc` storage overhead; and
- human presentation text not retained by `LinkFinding`.

Together, `findings_total` bounds fixed per-record storage while `finding_bytes_total` bounds retained variable payload independently of the implementation's sharing strategy.

The byte pass visits findings in their final canonical order and each finding's fields in its exact subject-then-evidence codec order. Within a composite value it visits array elements canonically and scalar payloads in member order.

It adds every complete scalar payload with checked `u64` arithmetic. The first addend that crosses the effective limit produces its exact attempted running total; no later finding or field is scanned merely to obtain a larger observation.

#### Failure result and retry

An overrun of either result counter is `Err(LinkOperationalError::Limit(...))` with `LinkLimitSubject::Request`. `FindingsTotal` wins whenever both counters would fail because its complete preflight precedes the byte pass.

The failed operation returns no `LinkOutcome`, findings prefix, truncation marker, blocking subset, or bundle plans. It never continues export, registration, check comparison, prune planning, or lint adaptation from a prefix that might have omitted a later blocking finding.

The CLI maps this error to top-level `message_link_failed` with `details.kind: "limit"` and exit code `2`; `analysis` is absent because no checked outcome exists. No target exporter is invoked.

A caller that selected a lower effective value may retry through one new complete request with a larger value not exceeding the protocol ceiling. The linker never raises a limit internally, splits one semantic link into independent shards, drops findings, or retries automatically.

### Bundle-plan result limits

Individually bounded delivery nodes, production locales, and definitions can still multiply into an unbounded plan result. At the protocol defaults, the admitted node and locale ceilings alone could otherwise create `65,536 × 1,024 = 67,108,864` empty plans before any message placement.

When the admitted finding set has no blocking record, plan construction therefore applies three independent inclusive request-result counters:

| `LinkLimits` counter | Accounting rule | Initial protocol ceiling |
| --- | --- | --: |
| `bundle_plans_total` | Number of canonical `(DeliveryUnitId, requested Locale)` plans, including empty plans. | `1,048,576` |
| `resolved_messages_total` | Number of final deduplicated `ResolvedMessage` placements across every plan. | `4,000,000` |
| `bundle_plan_bytes_total` | Sum of variable-length semantic payload bytes retained by all plans and resolved-message placements. | 1 GiB (`1,073,741,824` bytes) |

The counters are independent of artifact-input, finding-result, export-diagnostic, and exporter-output limits. A caller may select a lower immutable effective value for each counter, including zero, but cannot raise its protocol ceiling. The initial CLI exposes no corresponding option, config field, environment variable, reporter override, or target-specific value and uses all three protocol defaults.

#### Plan candidate and count accounting

The plan-result phase runs only after both finding-result counters have admitted the complete canonical finding set.

If any admitted finding blocks generation, the linker constructs `LinkOutcome { findings, bundle_plans: None }` without evaluating any plan-result counter. Plan limits never replace or hide a semantic blocking result.

Otherwise the phase is exactly:

1. derive the complete delivery-node × production-locale pair count and preflight `bundle_plans_total`;
2. visit those pairs in canonical plan order, derive each final logical-message set without materializing the public plan vector, and preflight the request-wide `resolved_messages_total`;
3. visit every admitted plan and resolved-message candidate in final canonical order and account `bundle_plan_bytes_total`; and
4. only after all three checks succeed, construct the private `MessageBundlePlan` and `ResolvedMessage` values and return the complete `LinkOutcome`.

The plan-count product uses checked `u64` arithmetic. Its two admitted input ceilings make the exact maximum `67,108,864`, so host-size conversion failure and arithmetic overflow cannot become public count evidence.

`resolved_messages_total` counts each placement once after same-plan logical-identity deduplication. Equal definitions selected into another requested locale or delivery unit count again. Repeated references that collapse to one logical identity in one plan count once, while storage interning, fallback reuse, cache reuse, worker partitioning, or exporter count gives no deduction.

A count above either effective ceiling is reported as exactly `effective_limit + 1`. The linker stops the applicable count admission without enumerating a larger public total or allocating a partial result vector.

#### Plan semantic byte accounting

`bundle_plan_bytes_total` counts decoded UTF-8 payload bytes in the exact public semantic occurrence where they are retained.

For each plan, it counts:

- every `DeliveryUnitId` segment payload; and
- the requested `Locale` payload.

For each resolved-message placement, it then counts:

- the resolved scope name;
- the canonical catalog key;
- the selected definition locale;
- the exact decoded `MessagePayload`;
- every selected definition source-path segment; and
- the selected `EntryStructuralPath`.

The fixed project-namespace, catalog-domain, and other enum discriminants; the `EntryReference.occurrence` integer; vector lengths and framing; pointers, capacities, `Arc` headers, and allocator overhead; and exporter or reporter serialization contribute zero.

Equal values are charged for every semantic occurrence. In particular, a fallback-selected payload placed in several requested locales or a shared definition placed in several delivery units contributes its complete fields each time even when the private implementation shares immutable storage.

The byte pass follows canonical plan order and then canonical logical-message order. Within each value it follows the field order above and adds each complete scalar payload with checked `u64` arithmetic.

The first addition that crosses the effective ceiling reports its exact attempted running total and stops accounting. Every addend has already passed its field-specific protocol ceiling, so the first attempted total is bounded and `ArithmeticOverflow` is unconstructible.

#### Zero limits, failure, and retry

`bundle_plans_total = 0` admits only the valid empty-graph `Some(Vec::new())` plan set. `resolved_messages_total = 0` may admit non-empty plans only when all are empty.

Because every non-empty plan has a non-empty delivery-unit identity and requested locale, `bundle_plan_bytes_total = 0` likewise admits only `Some(Vec::new())`.

An overrun of any plan-result counter is `Err(LinkOperationalError::Limit(...))` with `LinkLimitSubject::Request`. It returns no `LinkOutcome`, finding vector, plan prefix, omitted-count marker, validated batch, or export result.

The three counters use the fixed precedence `BundlePlansTotal`, then `ResolvedMessagesTotal`, then `BundlePlanBytesTotal`. Finding-result limit failures always precede them. Input order, hash-map iteration, storage sharing, cache reuse, target enumeration, and worker completion cannot change that result.

A caller may retry the same immutable logical inputs with larger valid lower limits. The linker never changes the plan universe, drops empty plans or messages, partitions one result, raises a limit, or retries internally to recover.

### Export preparation handoff

Shared export preparation considers only concrete `MessageBundlePlan` values from `Some` and exposes them to exporters only through `ValidatedExportBatch`; exporters never inspect findings or attempt to repair a blocked outcome.

A future incremental editor or dev-server session may cache indexes and invalidation state around this operation, but it must preserve equivalence with a full `link` call for the same logical request.

The incremental wrapper does not become a second source of linking semantics or a way to construct a validated batch without the current export gate.

## Delivery Units

A JS/TS bundler chunk is one platform-specific example of the project-contextual segment-array `DeliveryUnitId` fixed above. The linker combines those identifiers with a dependency graph and has no platform enum.

Reference artifacts and graph nodes share that exact checked type, and an admitted artifact must name one existing node without implicit creation or platform-specific conversion.

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

**Granularity differs per platform and must be declared honestly.** A bundler integration can attribute references to chunks from its live graph.

A final-binary scan can only produce a single-unit artifact — post-link, references cannot be attributed to sub-units; per-unit native granularity requires object-level scanning before link. Native v1 is therefore whole-program (one unit), which is correct, just coarse.

### Built-in single-unit graph

Before M4 supplies a live bundler graph, every link request made through the M0-owned built-in CLI/editor orchestration uses exactly one submitted node with `DeliveryUnitId` wire value `["main"]`, zero edges, and therefore that same one real root. M3/M5 CLI leaves and the L0 editor adapter activate that orchestration; M0-only builds exercise it through in-process tests rather than a user-facing command or diagnostic session. N0 final-binary scanning uses the identical graph. The checked graph is constructed normally under the request's effective limits; this rule is not an unchecked bypass.

The built-in JS producer assigns `["main"]` to every artifact it emits. An exact `producers.artifacts` snapshot consumed by the built-in CLI must also name `["main"]`; another structurally valid unit ID fails ordinary request validation because that node is absent. Configured roots are placed once in `["main"]` under `duplicate`.

Every M3 delivery target consumes plans from this same graph. Target names, exporter IDs, `out`, `eagerLocales`, `--target`, and target ordering never change the node, reference artifacts, reachability, or plan identities. A custom in-process integration and the M4 bundler integration may instead supply another fully checked graph and matching artifact IDs explicitly; no raw `messages.deliveryUnit` configuration field exists.

### Delivery-graph resource limits

`DeliveryUnitGraph` uses three independent inclusive `LinkLimits` counters:

| `LinkLimits` counter | Accounting rule | Initial protocol ceiling |
| --- | --- | --: |
| `delivery_graph_nodes` | Number of submitted delivery-node occurrences in one graph. | `65,536` |
| `delivery_graph_edges` | Number of submitted directed edge occurrences in one graph. | `1,048,576` |
| `delivery_graph_id_bytes` | Sum of each submitted node occurrence's exact `delivery_unit_bytes` charge. | 64 MiB (`67,108,864` bytes) |

Empty graphs, edgeless graphs, and exact boundaries are admitted when every other invariant passes; the 65,537th node, 1,048,577th edge, or first aggregate ID byte above 64 MiB rejects the complete graph and request.

The edge ceiling permits an average of sixteen submitted edges per node at the maximum node count, but it is not a per-node degree limit: a valid graph may distribute edges unevenly while remaining under the request-wide ceiling.

Each node ID must also pass the independent 255-byte segment, 64-segment, and 4 KiB per-ID ceilings above. Consequently, 65,536 nodes can coexist only when their combined ID payload stays within 64 MiB; the theoretical 256 MiB product of maximum count and maximum per-ID bytes is intentionally not admissible.

Every submitted occurrence is charged, including a duplicate node ID or an edge later rejected for a duplicate, unknown endpoint, disallowed self-reference, cycle, direction, or other graph-semantic reason.

Sorting, validation, deduplication, pruning unreferenced nodes, interning, shared prefixes, adjacency compression, cache reuse, and worker partitioning provide no deduction. `delivery_graph_id_bytes` counts each node's segment payload once and excludes array/record framing, display separators, allocation capacity, and edge records.

Logical edges reference nodes already present in the graph's checked node table and do not create another node-ID payload occurrence merely because one node participates in many edges.

All three counters use checked `u64` conversion and addition, the common fail-complete rule, and independent caller-selected lower immutable values. A zero node limit or zero ID-byte total admits only an empty graph because every valid node ID is non-empty; a zero edge limit admits any otherwise valid edgeless graph.

The linker never drops nodes or edges, partitions one graph into requests, truncates IDs, widens a lower limit, or retries internally. The numeric ceilings and accounting units are independent of the fixed edge direction and M0–M3 duplicate-placement semantics and remain unchanged by the deferred M4 hoist design.

`DeliveryUnitGraph` admission uses this exact, non-interleaved precedence:

1. preflight `delivery_graph_nodes` from the submitted node collection length;
2. preflight `delivery_graph_edges` from the submitted edge collection length;
3. complete structural and effective per-ID validation for every submitted node ID;
4. run the complete `delivery_graph_id_bytes` aggregate pass; and
5. detect duplicate node IDs, resolve edge endpoints, and apply duplicate-edge, self-reference, cycle, direction, and other graph-semantic validation.

A failure in an earlier phase always wins even when a later failure appears in an earlier submitted record. Implementations may compute provisional values concurrently, but they may not expose or select a later-phase failure before all preceding phases pass.

The ID-byte aggregate pass orders nodes by canonical `DeliveryUnitId`: segment-by-segment exact decoded UTF-8 byte order with the shorter equal-prefix sequence first. Equal IDs form one contiguous group.

Every submitted occurrence contributes its complete `delivery_unit_bytes` charge to a checked group subtotal, and that subtotal is added once to the request counter without deduplication.

Reduction stops at the first canonical group whose addition exceeds the effective limit; later groups are not scanned merely to report a larger value.

Sequential, partitioned, and parallel implementations must select the same group and attempted value as this serial canonical reduction, independently of submitted order or worker completion.

A `DeliveryGraphNodes` or `DeliveryGraphEdges` limit failure uses `LinkLimitSubject::DeliveryGraph`, because a collection-length preflight has no validated individual node or edge to blame.

A `DeliveryGraphIdBytes` failure instead uses exactly `LinkLimitSubject::DeliveryUnitGroup(id)` for the canonical equal-ID group whose addition selected the overrun. It never chooses an arbitrary duplicate occurrence or retains every contributing node.

These subject rules identify the narrowest validated cause without making evidence proportional to graph size.

### Delivery-graph semantics

M0 through M3 use one exact graph contract even when a particular milestone has only one delivery unit. A directed edge `parent -> child` means the child may become loadable only after the parent is loadable; it is a loading-order/dependency edge, not a reference-flow or message-copy edge.

A **real graph root** is a submitted node with indegree zero. Multiple roots and disconnected DAG components are valid.

The checked graph is finite and acyclic. Node identities and directed edge pairs must be exact and duplicate-free; every edge endpoint must name one submitted node; self-edges and cycles are invalid.

Input order is non-semantic: after validation the checked graph stores nodes in canonical `DeliveryUnitId` order, edges lexicographically by `(parent, child)`, and its derived roots in canonical node order. These checks run in the graph-semantic phase after the resource-limit admission order above.

Every admitted reference artifact names exactly one existing node. The linker never creates an implicit node, reverses an edge, infers an edge from artifact order, or chooses a platform-specific root.

An empty graph is valid only when the current request has no reference artifact, no configured root, and no output that requires a delivery unit. A configured root intentionally has no owning delivery unit, so under M0–M3 `duplicate` placement it is assigned to every real graph root.

Supplying one or more configured roots with an empty graph is therefore an invalid resolved request and returns `LinkOperationalError`; it does not produce `bundle_plans: None`, invent a synthetic output unit, or drop the roots. Every non-empty admitted DAG has at least one real root.

### Shared-message placement policy

When several units reach the same message (`common.ok` from both `checkout` and `settings`), the plan must place it deterministically. M0 through M3 support exactly one placement policy, `duplicate`, and it is the default.

A resolved policy or configuration that spells `hoist` is rejected as unsupported; it is never silently normalized or mapped to `duplicate`.

```rust
pub enum PlacementPolicy {
    Duplicate,
}
```

The closed enum is the only placement value stored in an M0–M3 `LinkPolicy`; omission in raw target configuration is resolved to `Duplicate` before construction.

Under `duplicate`, a message selected several times by references in the same delivery unit is stored once in that unit's plan for each locale. If references in several units select the same message, it is stored once per referencing unit for each locale.

Configured roots are stored once in every real graph root. Canonical plan construction deduplicates only equal resolved message identity within the same `(delivery unit, locale)` output; it never coalesces copies across units.

When plan generation is admitted, the plan set is the complete Cartesian product of every checked delivery-graph node and every production locale. A pair remains present with an empty `messages` slice when no message is selected for it.

An empty checked graph therefore produces `Some(Vec::new())`. A non-empty graph never uses omitted pairs as an alternate spelling for an empty plan, and an exporter never infers a missing pair from another plan, the policy, target options, or message presence.

Plans are ordered first by canonical `DeliveryUnitId` and then by the requested `Locale`'s exact UTF-8 bytes. Exactly one plan exists for each pair.

Within one plan, resolved message identity is exactly `(ResolvedCatalogScopeId, CatalogKeyDomain, CatalogKey)`. Repeated references or configured reachability selecting the same logical identity and exact definition snapshot produce one `ResolvedMessage`.

`definition_locale`, `DefinitionLocation`, and `MessagePayload` are attributes of that one selection, not additional identity dimensions. If an equal logical identity would retain different selected snapshot attributes, the linker reports an internal invariant failure and returns no outcome; it never keeps both, chooses first or last, or lets an exporter resolve the collision.

Messages are ordered by resolved scope, then catalog-domain contract order, then exact canonical-key bytes. Because the logical identity is unique inside the plan, no location, payload, definition-locale, reference order, or worker-completion tie-breaker is needed.

The duplicate algorithm uses the owning unit attached to each reference and does not traverse ancestors, descendants, or siblings to relocate that selection. Graph edges still validate loading topology and bind roots, but shared ancestry has no effect on M0–M3 placement.

Exporters consume these fixed placement results and never re-derive, merge, or hoist them.

`hoist` and mixed per-scope placement are deferred. The M4 candidate is a dominator-tree algorithm over a virtual super-root connected to every real graph root: a message may move only to a real node that dominates all referencing units and is loading-order-safe.

The virtual super-root is analysis-only and can never become a plan delivery unit. M4 must validate the exact algorithm, tie-breaking, no-common-real-dominator behavior, and output-size/load-order trade-offs against real bundler graph fixtures before `hoist` becomes an admitted policy.

## Linking Semantics

### Definition location identity

`DefinitionLocation` is the linker-owned portable identity of one selected or reported definition record:

```rust
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DefinitionLocation {
    source: SourceDocumentIdentity,
    entry: EntryReference,
}

impl DefinitionLocation {
    pub fn source(&self) -> &SourceDocumentIdentity;
    pub fn entry(&self) -> &EntryReference;
}
```

`intlify_contract` continues to own the checked `SourceDocumentIdentity` and `EntryReference` components. `intlify_linker` owns only their result-level composition and uses the same type in findings, `ResolvedMessage`, export diagnostic mapping, and prune selection.

Those two contract-owned component types provide the same `Clone`, equality, ordering, and hashing capabilities required by this composition. This requirement does not expose their fields or broaden their checked construction boundaries.

The fields are private and exposed through the two exact read-only accessors. No public constructor, struct literal, setter, mutable reference, deserializer, default, unchecked conversion, or partial builder exists; the linker constructs the value only from an admitted definition record.

Equality compares the complete source and entry components. Canonical `Ord` compares `SourceDocumentIdentity` first and `EntryReference` second, using their already fixed exact orders.

`Hash` is consistent with that equality and exists only for process-local checked collections. A Rust hasher, hash seed, or resulting hash value is never a stable artifact identity, canonical order, cache key, wire value, fingerprint input, finding codec, or cross-process protocol.

`Clone` duplicates one already checked identity so an owned finding, mapped diagnostic, or prune selection can outlive the `LinkOutcome` value from which it was projected. It does not expose component construction, create a new semantic identity, imply `Copy`, or promise a private allocation or sharing strategy.

The type does not retain producer identity, artifact version or fingerprint, alias, definition index, host span, payload, resolved scope, domain, key, or locale. Those values remain in their owning result or snapshot rather than changing record location identity.

### Ambiguous message definitions

After scope mapping and before fallback or reference resolution, the linker groups every admitted definition by the exact logical identity `(resolved_scope, domain, key, locale)`. A group containing two or more definitions produces exactly one `ambiguous-message-definition` finding.

Definitions in different exact locales do not collide. A physical source exposed through several logical aliases has already become one definition artifact and does not create a collision merely because it has aliases.

The finding's typed subject is the complete `(ResolvedCatalogScopeId, CatalogKeyDomain, CatalogKey, Locale)` tuple. Its evidence contains every colliding location as `DefinitionLocation { source: SourceDocumentIdentity, entry: EntryReference }`.

Evidence is canonicalized by `SourceDocumentIdentity` and then `EntryReference`, using their exact contract ordering, independently of artifact input, configuration, filesystem discovery, hash-map, or worker-completion order.

The vector is never truncated and is bounded by the request's admitted aggregate definition count; a request that exceeds that operational bound fails before semantic analysis rather than returning incomplete ambiguity evidence.

Ambiguity is always linker-blocking and cannot be made non-blocking by lint severity, coverage policy, dynamic-reference policy, source order, or exporter selection. Equal message payloads, equal fingerprints, equal producer identities, or equal source bytes do not coalesce definitions and do not select a winner.

The linker likewise never chooses by artifact order, configuration order, lexical source path, locale fallback order, or first/last occurrence.

It reports every collision group deterministically and returns `bundle_plans: None`; the evidence preserves both duplicate entries within one source and collisions across distinct sources.

After collecting collision groups, the linker derives the exact set of ambiguous logical keys `(scope, domain, key)`. It continues complete semantic analysis for every key outside that set and continues reference-owned `unbounded-dynamic-reference` and `degraded-analysis` detection.

For a key inside the set, it emits no derived `unresolved-message`, `unused-message`, `missing-translation`, or `orphaned-translation`: an ambiguous definition is neither a successful resolution candidate nor evidence of absence, and those secondary findings would be consequences of an identity that must first be repaired.

This suppression covers every locale of that logical key even when only one exact locale has a collision. It does not remove any ambiguity evidence, make the collision non-blocking, coalesce definitions for analysis, or suppress findings for another exact key.

### Resolution and unresolved messages

The artifact contract is locale-bearing from M0. For M0's limited resolution and reachability analysis, definitions are projected into the locale-agnostic `(scope, domain, key)` union described above; exact locale metadata is preserved but does not yet affect resolution. M2 activates the following production semantics.

Production linking is **closed-world over locales**: the supported locale set is finite and declared, and each locale resolves through its explicit fallback chain (byte-exact locale strings per 013):

```text
ja-JP → ja → en
```

If no locale in a requested chain defines a referenced message, runtime formatting cannot succeed; that is the build error `unresolved-message`.

### Reference-finding granularity

Every admitted `MessageReference` record is one distinct semantic observation. The linker emits at most one `unresolved-message` and at most one `unbounded-dynamic-reference` for that record; records are never grouped by equal scope, domain, selector, reason, origin, delivery unit, or resulting missing key.

Two call sites that make the same lookup therefore remain two findings. The same rule applies when an origin is absent, as for a stripped native artifact: missing presentation coordinates do not make distinct records equal.

In M2, one reference may fail resolution for several requested locales. It still produces one `unresolved-message`; its typed evidence contains the complete non-empty set of failing requested locales and each exact probed fallback chain in canonical locale order.

A locale whose chain resolves is absent from that evidence. The vector is never truncated and is bounded by the admitted finite locale policy.

M0 uses the same one-record/one-finding shape with its one fallback-blind resolution result, so activating M2 does not change finding identity or multiply one call site into locale-specific finding records.

The finding retains the containing artifact's delivery-unit identity and the post-mapping `ResolvedCatalogScopeId`, domain, selector, optional reason, and optional origin together with its exact `ReferenceRecordIdentity { artifact, ordinal }`.

The original declared `CatalogScopeId` remains recoverable from the immutable reference record addressed by that identity. The machine evidence reports the resolved semantic scope so it cannot disagree with the scope against which lookup actually ran.

That record identity is the reference-owned part of the typed subject and the first reference-record tie-break within the kind-specific canonical subject order. `origin` remains diagnostic evidence rather than identity or a sorting fallback.

An `AllInScope` reference's `degraded-analysis` remains attached to that one record. Completeness-derived degradation uses the `(resolved scope, side)` subject and contributing evidence fixed above.

Presentation may visually group equal findings, but the linker outcome and machine-facing adapter preserve every per-record finding and its count.

### Coverage and the baseline locale

If `ja` lacks a message that `en` supplies, fallback makes runtime resolution succeed; that is not an unresolved reference but the non-blocking M2/M3 translation-coverage finding `missing-translation`. No initial raw configuration or checked `LinkPolicy` member makes that finding generation-blocking.

Resolution semantics need no privileged locale — the chain replaces it. A **coverage baseline** locale per scope survives for exactly two jobs: as the yardstick for coverage-style reporting (`orphaned-translation`: defined in some locale but absent from the baseline) and as the source of typed-key generation.

It has no effect on resolution. The `coverageBaseline` mapping is optional per scope, but a scope selected for typed-key generation must have one explicit entry.

The integration never infers a baseline from the largest catalog, the first discovered locale, fallback order, or definition order. The selected locale must belong to `messages.locales` and have a definition-closed baseline inventory for that scope.

For typed-key generation, the baseline's canonical key set must contain the union of keys defined by every admitted production locale in the same resolved scope-domain pair.

If another locale contains a key absent from the baseline, generation for that scope fails complete and emits no stale or partial accessor module; it never silently omits the runtime-resolvable key or widens the generated surface from the union.

M1 performs this deterministic baseline-versus-union preflight as a generation gate without publishing an `orphaned-translation` linker finding. M2 reuses the same canonical difference as the subject set for that finding.

Missing keys in a non-baseline locale do not invalidate the baseline key surface; M2 reports them separately as non-blocking `missing-translation` findings under the fixed fallback and baseline analysis.

For each key in the baseline-versus-union difference, M2 emits one `orphaned-translation` for every exact non-baseline locale definition of that key. It does not aggregate several locale entries into one finding.

Each finding therefore identifies one independently repairable definition and retains its one exact `DefinitionLocation`. Presentation may group equal scope-domain-key subjects, but the linker result, machine count, and lint accounting remain definition-local.

### Reachability and unused messages

Reachability roots are: static references, bounded selectors, and roots declared in configuration — messages requested from outside scanned code (server-driven, feature flags), each declared as a selector with a reason. A definition reachable from none of them is `unused-message`:

```text
catalog definitions
  ├─ checkout.title  ← reachable
  ├─ checkout.total  ← reachable
  └─ checkout.old    ← unreachable
```

`checkout.old` can be excluded from production assets. Project-wide unused definitions are distinct from messages that are used by one delivery unit but absent from another's plan — the latter is normal slicing, not a finding. Exclusion from shipping does not remove the entry from the catalog; that is `prune`'s job below.

The linker emits one `unused-message` for every locale-bearing definition entry of an unreachable logical key. It does not aggregate the key's locale definitions into one finding.

The finding carries the exact definition identity and `DefinitionLocation`, so lint presentation and the M5 prune planner do not repeat semantic lookup merely to recover the affected entry. Closed completeness and the absence of a matching reference or configured root are generation preconditions, not serialized empty vectors or repeated constant evidence.

### Dynamic references

Dynamic keys are the safety boundary shared by every producer. When a full key is not statically known but a finite or scope-level bound is intentional and provable, the producer emits `Prefix`, `Pattern`, or `AllInScope`.

When it recognizes a lookup under a known scope-domain pair but cannot prove any bound on the key expression, it emits `UnboundedDynamic`. Producers do not read or apply `messages.dynamicReferences`; the same artifact can be consumed under either policy.

- **strict mode**: emit a blocking `unbounded-dynamic-reference` finding and produce no valid bundle plan.
- **compat mode**: emit a non-blocking `unbounded-dynamic-reference` finding and conservatively treat the record as reaching every definition in its exact scope-domain pair.
- In no mode does the linker guess a narrower set — a possibly-used message is never silently dropped.

`UnboundedDynamic` always produces the dedicated finding above and never produces `unresolved-message` for the same record: strict mode does not pretend to resolve an unbounded selector, while compat mode conservatively widens it.

Compat mode does not additionally duplicate it as `degraded-analysis`. In M0 through M3 that category is limited to reference-record `AllInScope` selectors and the explicit partial-completeness states above that suppress absence-dependent analysis.

The `unbounded-dynamic-reference` evidence retains the exact checked `dynamicReferences` mode in addition to the resolved scope-domain pair, delivery unit, and optional reference reason/origin. `blocking` communicates plan availability; the mode separately communicates whether compat analysis conservatively retained the complete scope-domain definition set.

Every reference-record `AllInScope` emits one non-blocking `degraded-analysis` because it intentionally makes the complete resolved scope-domain pair reachable. `Prefix` and `Pattern` remain exact bounded selectors regardless of how many current definitions they match and never become degraded through an implicit match-count threshold.

A configured root, including `AllInScope`, is an explicit reachability policy rather than an imprecise producer observation and emits no degradation finding. A future configurable selector-breadth warning requires the deferred contract below rather than deriving policy from current catalog size.

The initial finding names the exact reference record or completeness side and reason so a project can see _why_ analysis was weakened instead of trusting a silent suppression. Selector `reason` fields feed the reference-owned variant.

## Linker Findings and Later Lint Integration

M0 returns findings as deterministic, language-neutral `intlify_linker` result data. Finding kinds, typed subjects, and evidence belong to the linker contract; lint rule configuration, severity, preset membership, reporter serialization, warning counts, and lint exit status do not.

Core tests and non-lint consumers inspect these linker-owned records directly, so implementing or invoking the linker never requires `intlify_lint`.

Linker finding categories, their suggested severity when explicitly enabled as lint rules, and their initial rollout state are:

| Finding | Meaning | Suggested explicit severity | Initial lint default |
| --- | --- | --- | --- |
| `ambiguous-message-definition` | two or more definitions claim the same exact scope, domain, key, and locale | error | off |
| `unresolved-message` | reference resolves in no locale of a requested chain | error | off |
| `missing-translation` | resolvable only via fallback; coverage gap in a requested locale | warn | off |
| `orphaned-translation` | defined in a locale but absent from the coverage baseline | warn | off |
| `unused-message` | definition unreachable from every root | warn | off |
| `unbounded-dynamic-reference` | `UnboundedDynamic` site; blocks in strict mode, retains its scope-domain pair in compat mode | error in strict, warn in compat | off |
| `degraded-analysis` | a reference-record `AllInScope` selector or partial scope input weakened analysis; completeness-derived cases block plans | warn | off |

### Rust finding union

The core Rust API represents one finding with a closed typed record union:

```rust
pub struct LinkFinding {
    record: LinkFindingRecord,
}

pub enum LinkFindingRecord {
    AmbiguousMessageDefinition(AmbiguousMessageDefinitionFinding),
    UnresolvedMessage(UnresolvedMessageFinding),
    MissingTranslation(MissingTranslationFinding),
    OrphanedTranslation(OrphanedTranslationFinding),
    UnusedMessage(UnusedMessageFinding),
    UnboundedDynamicReference(UnboundedDynamicReferenceFinding),
    DegradedAnalysis(DegradedAnalysisFinding),
}

pub enum DegradedAnalysisFinding {
    WideSelector(WideSelectorDegradation),
    PartialCompleteness(PartialCompletenessDegradation),
}

pub enum LinkFindingKind {
    AmbiguousMessageDefinition,
    UnresolvedMessage,
    MissingTranslation,
    OrphanedTranslation,
    UnusedMessage,
    UnboundedDynamicReference,
    DegradedAnalysis,
}
```

Each concrete finding type contains exactly one private typed subject and one private typed evidence value:

```rust
pub struct MissingTranslationFinding {
    subject: MissingTranslationSubject,
    evidence: MissingTranslationEvidence,
}
```

The other six concrete types follow the same paired pattern. Their fields and collection storage are private and exposed through read-only accessors.

`LinkFinding` exposes:

```rust
impl LinkFinding {
    pub fn kind(&self) -> LinkFindingKind;
    pub fn blocking(&self) -> bool;
    pub fn record(&self) -> &LinkFindingRecord;
}
```

`kind()` is derived from `record`; no independent kind field exists. The linker-owned semantic phases are the only constructors and can construct a record only after its complete subject/evidence invariants and result budgets have passed.

There is no public struct literal, setter, unchecked constructor, generic subject/evidence map, `serde_json::Value`, `Any`, custom/unknown variant, independent kind-plus-record constructor, or deserialization path into the core type.

`LinkFindingKind`, `LinkFindingRecord`, and `DegradedAnalysisFinding` are exhaustive public closed enums. They do not use `#[non_exhaustive]` and contain no `Unknown`, `Other`, or `Custom` escape variant.

Consumers may match them exhaustively without a wildcard. Adding a variant intentionally breaks such matches and requires the coordinated compatibility work below, so a new potentially blocking finding cannot be silently ignored by an old catch-all branch.

The future stable-v1 freeze may revisit the Rust evolution strategy explicitly, but no pre-v1 implementation may add `#[non_exhaustive]` or a catch-all as an undocumented compatibility shortcut.

The core union is not laid out around the machine JSON schema. The M3 adapter pattern-matches the typed record and projects its exact `subject` and `evidence` objects; changing JSON framing cannot change core identity or create a mismatched kind/evidence pair.

Adding, removing, or changing a record variant is a coordinated public compatibility decision across the typed API, kind precedence, blocking derivation, machine codec, lint adapter, result accounting, and fixtures.

### Canonical finding order

`LinkOutcome::findings()` uses this fixed primary kind precedence, from lowest comparison value to highest: `ambiguous-message-definition` (`0`), `unresolved-message` (`1`), `missing-translation` (`2`), `orphaned-translation` (`3`), `unused-message` (`4`), `unbounded-dynamic-reference` (`5`), and `degraded-analysis` (`6`).

A read-only `LinkFindingKind::precedence() -> u8` exposes those values. They are explicit comparison data, not Rust enum discriminants, wire tags, diagnostic codes, severities, exit codes, blocking ranks, or an instruction to run semantic phases in that order; enum declaration order does not define them.

Within one kind, findings compare by the following exact typed subject tuple and then exact evidence tuple:

| Kind | Subject tuple | Evidence tuple |
| --- | --- | --- |
| `ambiguous-message-definition` | resolved scope, domain, key, locale | canonical definition-location vector |
| `unresolved-message` | reference-artifact identity, ordinal | delivery unit, resolved scope, domain, selector, optional reason, optional origin, failures |
| `missing-translation` | reference-artifact identity, ordinal, requested locale, key | delivery unit, resolved scope, domain, probed locales, selected locale, definition location |
| `orphaned-translation` | resolved scope, domain, key, definition locale | baseline locale, definition location |
| `unused-message` | resolved scope, domain, key, definition locale | definition location |
| `unbounded-dynamic-reference` | reference-artifact identity, ordinal | delivery unit, resolved scope, domain, optional reason, optional origin, dynamic mode |
| `degraded-analysis` | variant precedence, then variant subject | variant evidence |

The common component rules are:

- checked text compares by exact decoded UTF-8 bytes without normalization, collation, rendering, or host-path behavior;
- a structured identity compares its already fixed semantic components in contract order;
- vectors compare lexicographically by element, with a shorter equal-prefix vector first;
- `None` compares before `Some`, and a present value then uses its typed comparison;
- `SourceOrigin` compares source identity, then span start, then span end;
- `DefinitionLocation` compares source-document identity, then entry structural path, then occurrence;
- locales compare exact UTF-8 bytes;
- catalog domains and selector variants use their explicit contract precedence before a variant payload;
- dynamic mode orders `compat` before `strict`;
- completeness side orders definitions before references; and
- partial reasons retain their explicit comparison order inside contributor vectors.

`degraded-analysis` first compares `wide-selector` (`0`) before `partial-completeness` (`1`), then applies the matching subject/evidence codec fixed below.

`LinkFinding` equality compares the complete typed record. Its canonical comparison is implemented explicitly from this contract; Rust enum declaration order, derived field order, memory layout, discriminants, or serialized JSON bytes never define it.

Optional reason and origin participate only in the evidence tuple after the complete subject tuple. No final sort consults human-rendered text, rendered/host paths, derived line/column, lint severity, blocking disposition, configuration order, artifact order, discovery order, hash-map iteration, analysis scheduling, worker completion, or machine JSON escaping/member bytes.

Suppressed findings are removed before this sort, and sorting never deduplicates two distinct typed subjects or evidence values.

Adding a finding kind or changing an existing kind's precedence or canonical tie-break is an explicit public compatibility change with corresponding conformance fixtures rather than an incidental implementation reorder.

### Finding blocking disposition

M0 through M3 use only blocking policy that exists in the checked `LinkPolicy`. They do not infer an unimplemented strict-coverage mode from lint severity, baseline configuration, reporter choice, exporter, target, or command environment.

| Finding | Initial M0–M3 blocking disposition |
| --- | --- |
| `ambiguous-message-definition` | Always blocking. |
| `unresolved-message` | Always blocking. |
| `missing-translation` | Non-blocking. |
| `orphaned-translation` | Non-blocking. |
| `unused-message` | Non-blocking. |
| `unbounded-dynamic-reference` | Blocking in strict dynamic-reference mode; non-blocking in compat mode. |
| completeness-derived `degraded-analysis` | Blocking. |
| reference-record `AllInScope` `degraded-analysis` | Non-blocking. |

`LinkFinding` stores no independent `blocking: bool`. Its `blocking()` method matches the typed record:

- ambiguity and unresolved records return `true`;
- missing-translation, orphaned-translation, and unused records return `false`;
- unbounded-dynamic records return `false` for their checked `compat` evidence and `true` for `strict`;
- wide-selector degradation returns `false`; and
- partial-completeness degradation returns `true`.

This makes a state such as strict dynamic evidence paired with non-blocking disposition unconstructible. A future policy-dependent disposition must retain the exact checked policy fact in its typed finding record and update this derivation; it cannot add an unrelated mutable boolean or let a presentation adapter reinterpret the record.

`LinkOutcome::bundle_plans()` is `None` if and only if at least one retained finding has blocking disposition under this matrix. Because the outcome exposes one all-or-nothing plans option, one blocking finding blocks every selected M3 target; the linker does not emit a partial plan subset for apparently unrelated targets or scopes.

Non-blocking findings remain in the complete canonical outcome and machine analysis but do not prevent export, turn a target into `blocked`, change `summary.status`, or produce exit code `1` by themselves. Lint adapters may independently assign warning or error severity, but that presentation cannot alter the linker's `blocking` value or plan availability.

The machine-facing `blocking` boolean is exactly `LinkFinding::blocking()`. It is not stored a second time or recomputed by the CLI, reporter, linter, exporter, or target integration.

### Machine-facing finding union

The `analysis.findings` adapter is a closed kind-discriminated union rather than one nullable superset or opaque JSON payload. Every record emits exact `kind`, `blocking`, `subject`, and `evidence` in that order. The outer `kind` selects one exact subject object and one exact evidence object; those nested objects do not repeat a generic type tag unless the finding itself has several typed evidence variants.

The seven initial semantic families are:

| `kind` | Subject family | Evidence family |
| --- | --- | --- |
| `ambiguous-message-definition` | resolved scope, domain, canonical key, locale | complete canonical colliding definition locations |
| `unresolved-message` | reference-record identity | delivery unit, resolved scope, domain, selector, optional reason/origin, failing requested locales and probed fallback chains |
| `missing-translation` | reference-record identity, requested locale, and resolved canonical key | delivery unit, resolved scope, domain, probed locales, selected definition locale, and location |
| `orphaned-translation` | resolved scope, domain, key, and non-baseline definition locale | coverage baseline locale and one definition location |
| `unused-message` | resolved scope, domain, key, and definition locale | one definition location |
| `unbounded-dynamic-reference` | reference-record identity | delivery unit, resolved scope, domain, optional reason/origin, and checked dynamic-reference mode |
| `degraded-analysis` | reference-record identity or resolved-scope/completeness-side | closed `wide-selector` or `partial-completeness` evidence |

#### `ambiguous-message-definition` machine codec

The exact subject member order is `scope`, `domain`, `key`, then `locale`:

```json
{
  "scope": {
    "namespace": { "kind": "project" },
    "name": "app"
  },
  "domain": "json-pointer",
  "key": "/checkout/title",
  "locale": "ja"
}
```

`scope` is the post-mapping `ResolvedCatalogScopeId` encoded through the exact `CatalogScopeId` object codec. `domain`, `key`, and `locale` reuse their checked semantic values without rendering or normalization.

The exact evidence object contains only `definitions`. It is a complete array of at least two objects, each ordered as `source` then `entry`:

```json
{
  "definitions": [
    {
      "source": {
        "namespace": { "kind": "project" },
        "path": ["locales", "ja.json"]
      },
      "entry": {
        "structuralPath": "/checkout/title",
        "occurrence": 0
      }
    }
  ]
}
```

Each element is the existing `DefinitionLocation { source: SourceDocumentIdentity, entry: EntryReference }` projection. The array uses the canonical source-then-entry order already fixed for ambiguity analysis, contains every collider, and is never truncated, deduplicated, or reordered for presentation.

The codec includes no message payload, fingerprint, producer identity, alias, host path, span, source excerpt, or chosen winner. `blocking` is always `true`.

#### `unresolved-message` machine codec

The exact subject is the `ReferenceRecordIdentity` object ordered as `artifact` then `ordinal`:

```json
{
  "artifact": {
    "namespace": { "kind": "project" },
    "segments": ["js", "module", "src", "checkout.ts"]
  },
  "ordinal": 3
}
```

The artifact member reuses the exact `ReferenceArtifactIdentity` codec. `ordinal` is the checked zero-based `u32` semantic array position and is serialized here because the finding refers to one record rather than serializing that record inside its parent artifact.

The M3 machine evidence has exact member order `deliveryUnit`, `scope`, `domain`, `selector`, optional `reason`, optional `origin`, then `failures`:

```json
{
  "deliveryUnit": ["main"],
  "scope": {
    "namespace": { "kind": "project" },
    "name": "app"
  },
  "domain": "json-pointer",
  "selector": {
    "kind": "exact",
    "key": "/checkout/title"
  },
  "failures": [
    {
      "requestedLocale": "ja",
      "probedLocales": ["ja", "en"]
    }
  ]
}
```

`scope` is the post-mapping resolved semantic scope. `selector`, and present `reason` or `origin`, reuse the exact checked reference codecs. Absence of either optional is omission, never `null`.

`failures` is non-empty. Its elements are ordered by canonical requested-locale order and contain exactly `requestedLocale` followed by `probedLocales`. Each `probedLocales` array is the complete non-empty fallback chain actually probed, begins with its requested locale, preserves configured fallback priority, and contains no locale that was not probed. Because no definition resolved, it contains no selected definition or location.

One reference record produces at most one such finding even when several requested locales fail. The vector is complete and never split, truncated, or regrouped by presentation. `blocking` is always `true`.

#### `missing-translation` machine codec

One finding represents exactly one `(ReferenceRecordIdentity, requested locale, resolved canonical key)` coverage gap. A bounded selector that resolves several keys therefore produces separate findings for the affected keys rather than one evidence array that hides independently repairable translations.

The exact subject order is `reference`, `requestedLocale`, then `key`:

```json
{
  "reference": {
    "artifact": {
      "namespace": { "kind": "project" },
      "segments": ["js", "module", "src", "checkout.ts"]
    },
    "ordinal": 3
  },
  "requestedLocale": "ja",
  "key": "/checkout/title"
}
```

`reference` nests the same exact `ReferenceRecordIdentity` codec. The selected reference fixes the domain used to interpret `key`; the evidence repeats that domain explicitly so the machine record remains self-describing.

The exact evidence order is `deliveryUnit`, `scope`, `domain`, `probedLocales`, `selectedLocale`, then `definition`:

```json
{
  "deliveryUnit": ["main"],
  "scope": {
    "namespace": { "kind": "project" },
    "name": "app"
  },
  "domain": "json-pointer",
  "probedLocales": ["ja", "en"],
  "selectedLocale": "en",
  "definition": {
    "source": {
      "namespace": { "kind": "project" },
      "path": ["locales", "en.json"]
    },
    "entry": {
      "structuralPath": "/checkout/title",
      "occurrence": 0
    }
  }
}
```

`scope` is the post-mapping resolved semantic scope. `probedLocales` is non-empty, begins with `subject.requestedLocale`, preserves fallback priority, and ends at the first locale that defines the exact subject key. `selectedLocale` equals that last element and must differ from the requested locale; otherwise no translation gap exists.

`definition` is the one selected `DefinitionLocation`, ordered as `source` then `entry`, whose definition locale equals `selectedLocale`. Ambiguous keys are suppressed before this phase, so the codec never carries several candidates or a winner-selection explanation.

The finding contains no selector, reason, origin, payload, or complete fallback suffix after the selected locale. Those remain available through the reference identity or are irrelevant after first-success resolution. `blocking` is always `false` in M2 and M3.

#### `orphaned-translation` machine codec

One finding represents exactly one non-baseline locale definition whose resolved canonical key is absent from that scope's explicit coverage baseline.

The exact subject order is `scope`, `domain`, `key`, then `locale`:

```json
{
  "scope": {
    "namespace": { "kind": "project" },
    "name": "app"
  },
  "domain": "json-pointer",
  "key": "/checkout/title",
  "locale": "ja"
}
```

`scope` is the post-mapping resolved semantic scope. `locale` is the exact locale of the affected definition, not the baseline locale.

The exact evidence order is `baselineLocale` then `definition`:

```json
{
  "baselineLocale": "en",
  "definition": {
    "source": {
      "namespace": { "kind": "project" },
      "path": ["locales", "ja.json"]
    },
    "entry": {
      "structuralPath": "/checkout/title",
      "occurrence": 0
    }
  }
}
```

`baselineLocale` is the explicit checked coverage baseline for the resolved scope and differs from `subject.locale`. The subject key is absent from that baseline inventory, while `definition` is the one exact `DefinitionLocation` whose scope, domain, key, and locale equal the subject.

If the same missing baseline key is defined in several non-baseline locales, each definition produces a separate finding. Ambiguous keys and definition-partial scopes are suppressed before this phase. A scope without an explicit baseline produces no orphaned finding. `blocking` is always `false`.

#### `unused-message` machine codec

One finding represents exactly one locale-bearing definition entry belonging to a logical key that no admitted static reference, bounded selector, compat-widened dynamic reference, or configured root reaches.

The exact subject order is `scope`, `domain`, `key`, then `locale`:

```json
{
  "scope": {
    "namespace": { "kind": "project" },
    "name": "app"
  },
  "domain": "json-pointer",
  "key": "/checkout/old",
  "locale": "ja"
}
```

The exact evidence object contains only `definition`:

```json
{
  "definition": {
    "source": {
      "namespace": { "kind": "project" },
      "path": ["locales", "ja.json"]
    },
    "entry": {
      "structuralPath": "/checkout/old",
      "occurrence": 0
    }
  }
}
```

`scope` is resolved and the one `DefinitionLocation` must identify the exact subject definition. A logical key defined in three locales produces three findings. The machine result does not aggregate locale locations or serialize empty matching-reference/root vectors, evaluated-selector counts, or redundant closed-completeness constants.

Both definition and reference sides must already be closed for the resolved scope and ambiguity suppression must already have run. Reachability follows the checked dynamic mode: a compat-widened scope-domain pair cannot produce an unused finding there, while M5 independently refuses prune mutation for any affected unbounded-dynamic scope. Presentation may group records, but machine counting and the M5 prune mapping remain definition-local. `blocking` is always `false`.

#### `unbounded-dynamic-reference` machine codec

The exact subject is the same artifact-then-ordinal `ReferenceRecordIdentity` object used by `unresolved-message`:

```json
{
  "artifact": {
    "namespace": { "kind": "project" },
    "segments": ["js", "module", "src", "checkout.ts"]
  },
  "ordinal": 8
}
```

The exact evidence order is `deliveryUnit`, `scope`, `domain`, optional `reason`, optional `origin`, then `dynamicMode`:

```json
{
  "deliveryUnit": ["main"],
  "scope": {
    "namespace": { "kind": "project" },
    "name": "app"
  },
  "domain": "json-pointer",
  "reason": "runtime-provided key",
  "origin": {
    "source": {
      "namespace": { "kind": "project" },
      "path": ["src", "checkout.ts"]
    },
    "span": {
      "start": 128,
      "end": 146
    }
  },
  "dynamicMode": "compat"
}
```

`scope` is the post-mapping resolved semantic scope. Present `reason` and `origin` reuse the exact reference codecs; absent values are omitted rather than emitted as `null`.

`dynamicMode` is exactly `"compat"` or `"strict"` and equals the checked immutable policy used for this link. Compat requires `blocking: false` and conservative reachability of every definition in the exact scope-domain pair. Strict requires `blocking: true` and no bundle plans.

The selector is not repeated because the finding kind is emitted only for `MessageSelector::UnboundedDynamic`. The evidence contains no retained-definition vector, guessed key set, producer policy, lint severity, or reporter choice.

#### `degraded-analysis` machine codec

The initial codec is a closed two-variant union. Evidence `kind` selects the exact subject and evidence shape:

| Variant precedence | Exact evidence `kind`    | Subject                              | Blocking |
| -----------------: | ------------------------ | ------------------------------------ | -------- |
|                `0` | `"wide-selector"`        | `ReferenceRecordIdentity`            | `false`  |
|                `1` | `"partial-completeness"` | resolved scope and completeness side | `true`   |

No initial `"open-world-participant"` or generic custom variant exists. Adding another cause requires a coordinated typed-core, ordering, machine-schema, resource-accounting, and compatibility change.

##### Wide selector

The exact subject is the ordinary artifact-then-ordinal `ReferenceRecordIdentity`. The evidence order is `kind`, `deliveryUnit`, `scope`, `domain`, optional `reason`, then optional `origin`:

```json
{
  "kind": "degraded-analysis",
  "blocking": false,
  "subject": {
    "artifact": {
      "namespace": { "kind": "project" },
      "segments": ["js", "module", "src", "checkout.ts"]
    },
    "ordinal": 12
  },
  "evidence": {
    "kind": "wide-selector",
    "deliveryUnit": ["main"],
    "scope": {
      "namespace": { "kind": "project" },
      "name": "app"
    },
    "domain": "json-pointer",
    "reason": "all checkout messages are loaded together"
  }
}
```

This variant is emitted exactly once for each admitted reference record whose selector is `AllInScope`. `scope` is the post-mapping resolved semantic scope. Present reason/origin reuse the reference codecs and absent values are omitted rather than emitted as `null`.

The evidence does not repeat the selector because only `AllInScope` admits this variant. `Prefix` and `Pattern` never enter it through current match count, and configured roots never enter it at all. The linker marks every definition in the exact scope-domain pair reachable, so the result remains safe and non-blocking.

##### Partial completeness

The exact subject order is `scope` then `side`:

```json
{
  "scope": {
    "namespace": { "kind": "project" },
    "name": "app"
  },
  "side": "references"
}
```

`scope` is the post-mapping `ResolvedCatalogScopeId`. `side` is exactly `"definitions"` or `"references"` and uses definitions-before-references canonical order.

The exact evidence order is `kind` then `contributors`:

```json
{
  "kind": "partial-completeness",
  "contributors": [
    {
      "scope": {
        "namespace": { "kind": "project" },
        "name": "app"
      },
      "reason": "producer-failed"
    }
  ]
}
```

`contributors` is the complete non-empty vector of original pre-mapping scopes whose selected side is partial. Each element is ordered as `scope` then `reason`; the vector is canonical by original `CatalogScopeId`, then `PartialReason`, and contains no duplicate pair.

The exact reason tokens are:

| Canonical order | Evidence token                   | Applicable side           |
| --------------: | -------------------------------- | ------------------------- |
|             `0` | `"open-editor-world"`            | definitions or references |
|             `1` | `"source-omitted"`               | definitions only          |
|             `2` | `"source-failed"`                | definitions only          |
|             `3` | `"producer-omitted"`             | references only           |
|             `4` | `"producer-failed"`              | references only           |
|             `5` | `"external-artifact-unverified"` | definitions or references |

These token ordinals follow the existing `PartialReason` comparison order; integration cause-selection priority remains the separate failed-before-omitted-before-unverified-before-open-world rule.

One resolved scope with both sides partial emits two findings rather than combining them. The codec contains no execution report, glob, failed source or producer list, arbitrary message, underlying error, cache status, or inferred missing definition/reference. `blocking` is always `true`.

Within `degraded-analysis`, canonical ordering compares the explicit variant precedence first, then that variant's typed subject tuple, then its evidence tuple. Presentation grouping, blocking rank, optional origin availability, or enum declaration order never changes it.

Each per-kind codec fixes its member names, order, nested identity codecs, optional-member admission, resource accounting, and canonical vector order. Missing, duplicate, unknown, mistyped, cross-kind, and disallowed `null` members are invalid; optional values are omitted rather than replaced with `null`.

The CLI envelope's top-level `schemaVersion: "0"` owns this JSON adapter's pre-stable compatibility. A finding record carries no redundant schema, kind-version, artifact-version, or lint-rule version. Changing a subject/evidence meaning or shape requires the coordinated finding compatibility and CLI schema decision; an implementation never exposes an internal Rust enum or debug serialization as the wire format.

**The later `intlify lint` integration is a presentation adapter, not an M0 dependency.**

It maps each category one-to-one to a lint rule under the same ID. Configuration, reporting, counting, and exit status remain governed by the 008 contracts and the catalog-level linter addendum already required by 013: project-scope execution, typed subject identities, and related-entry evidence.

The adapter does not rerun resolution or create a second finding model. These rules are the toolchain successor of the ESLint plugin's `no-missing-keys` / `no-unused-keys`.

Reference-anchored findings (`unresolved-message`, `unbounded-dynamic-reference`) anchor at the code site when `origin` is available, with the probed chain as evidence; definition-anchored findings (`ambiguous-message-definition`, `unused-message`, coverage findings) anchor at 013 entries.

The ambiguity adapter derives one deterministic primary entry and the remaining related entries from the already canonical complete evidence vector without dropping any collider. Other definition findings use reference sites or their absence as related evidence.

One linker finding counts exactly once under the 008 warning/error accounting regardless of the number of related entries. The editor (009) presents both directions incrementally from whatever source-level reference information is available in the open development world.

Producer failures are producer-owned operational errors, not linker findings; when a later lint adapter invokes production, it maps those failures into the existing lint operational-error surface.

The lint integration first resolves rule configuration, preset membership, and severity, then unions the declared input capabilities of every enabled linker-backed rule. If no linker-backed rule is enabled, it performs no linker orchestration.

If at least one is enabled but none requires application-reference analysis, it invokes the shared linker orchestration once with the required definition/policy inputs and an integration-derived non-closed reference side, but neither loads nor produces reference artifacts and pays no source-scan cost.

If reference input is required, the same orchestration additionally uses the shared producer and artifact cache. In both enabled cases all adapters consume the resulting single `LinkOutcome`; a rule is never allowed to scan or link independently.

Disabled findings are filtered only by the presentation adapter and do not change producer output, linker semantics, or cache identity.

Locally produced reference-artifact cache identity depends on the actual producer inputs — artifact version, producer identity/revision, resolved producer and recognizer configuration, source identity, and source-input fingerprint — never on lint severity, preset membership, reporter choice, or enabled-rule order.

An exact configured external-artifact snapshot uses a separate consumer-side decode-cache identity: its normalized declared path, exact byte length, BLAKE3-256 digest of the complete file bytes, decoder contract revision, and compatible effective limits. This digest is cache metadata, not a new `MessageReferenceArtifact` wire member or a claim about upstream source freshness. Equal cached bytes may reuse bounded decoding only when every component matches; changed bytes naturally miss.

Compatible fresh locally produced artifacts and byte-identical authoritative external snapshots from another invocation may be reused under their respective identities. `messages emit` and `messages prune` invoke the same orchestration whenever their own contracts require it, independently of lint rules.

A narrow lint operand that cannot prove the project-wide reference or definition world derives the applicable `Partial` completeness state; absence-dependent findings are suppressed and the existing completeness-derived `degraded-analysis` evidence is adapted instead of guessing from the subset.

All L0/L1 linker-backed lint rules are initially default-off and excluded from `recommended`, even when the table suggests an error or warning severity for explicit opt-in.

A later catalog-level lint addendum may promote individual rules only after measuring scan cost, cache behavior, project-scope operand semantics, completeness UX, and false positives.

Promotion is per rule and is a deliberate preset/default change; merely configuring `resources.catalogs`, invoking another linker consumer, or warming a cache never enables a lint rule.

## Fixes and Generation

- **`prune`**: the linker makes shipping safe even with stale entries, but stale entries still cost translation and review. `intlify messages prune` turns `unused-message` findings into a deterministic logical deletion plan (plan/dry-run by default, `--write` to apply).
  - It requires definition-closed and reference-closed completeness plus no unbounded-dynamic degradation for every affected scope; otherwise it refuses mutation rather than deleting from partial evidence.
  - Deleting entries is a semantic decision, so it is a command, never a lint autofix.
  - Linker orchestration maps each selected definition back to the originating 013 extraction artifact and artifact-local entry handle. It then submits the complete per-document deletion set to the separate resource-owned structural-mutation boundary planned for M5.
  - The formatter-facing `build_and_validate_write_back` API is not used for deletion and retains its fixed entry-count and identity-preservation invariants. `intlify_resource` alone owns host-structural edits, candidate reparse and re-extraction, and proof that exactly the requested definitions were removed without changing surviving definitions.
  - An unsupported or invalid deletion fails the affected document completely and produces no partial edit set. Concrete JSON member/array rules, occurrence renumbering, empty-ancestor behavior, API types, and error mapping are fixed by the coordinated 013 M5 addendum before implementation.
  - Initial milestones provide no missing-message or missing-translation stub insertion; that mutation is deferred below.
- **Typed keys**: generated from each explicitly configured coverage baseline after the baseline-versus-union preflight.
  - The language-neutral generation model carries resolved scope, canonical domain-qualified keys, and MF2 argument information.
  - Each platform integration renders that model in its native form.
  - The initial JS/TS form is an explicit scope-bound generated accessor module containing TypeScript key unions and, where MF2 declarations allow, argument types.
  - It does not use global or ambient `.d.ts` augmentation:

```ts
// Example for a runtime configured with dot-path key syntax.
type MessageKey = 'checkout.title' | 'checkout.total' | 'errors.network'

declare function t<K extends MessageKey>(key: K, args: MessageArgs<K>): string
```

Generated types give early, in-editor feedback; per-locale availability and fallback resolution remain the linker's final authority. The generated strings use the target runtime API's configured key spelling and do not replace the domain-qualified canonical keys used internally by the linker.

Explicit module imports keep scopes and generated revisions visible and avoid cross-project ambient-type collisions.

They also allow later Rust, C/C++, and other platform generators to consume the same language-neutral model without treating TypeScript augmentation as the contract.

`useMessageSet` remains a producer-side bounded-selector API. It may consume generated key or pattern types in a later JS integration, but M1 does not replace it with, or make it an implicit entry point into, the generated accessor module.

- **Determinism and freshness**: linker output (plans, exported assets, generated types) is byte-deterministic for identical inputs, and every generation surface has a `--check` mode that re-runs and diffs instead of writing — the CI freshness job.

## Reference Producers

Each producer owns the conversion from its source-facing runtime key syntax to `(CatalogScopeId, CatalogKeyDomain, MessageSelector)`.

Every M0 recognizer or native build binding names an explicit project scope and domain. The pre-discovery configuration pass resolves the declared scope through `ResolvedResources` and validates the domain token plus syntax/domain compatibility; a producer never infers either value from an API name, source path, catalog glob, definition order, or source spelling. Multiple recognizers may bind to different declared scopes.

The examples below use an explicitly configured dot-path spelling; they do not make dot paths the linker key format. With JSON definitions, for example, `keySyntax: "literal"` maps `t('checkout.title')` to `/checkout.title`, while `keySyntax: "dot-path"` maps it to `/checkout/title`.

### JS/TS

An AST transform recognizes existing static-key APIs (`t('checkout.title')` and the configured recognizer surface). Dynamic sets are declared with a bounding API rather than an annotation comment — the declaration is code: refactorable, type-checkable, and usable as a runtime value:

```ts
const errors = useMessageSet('errors.*')

errors.format(errorCode)
```

#### M0 built-in source artifact partition

The built-in JS/Vue producer emits exactly one `MessageReferenceArtifact` for each selected physical source group, including a successfully scanned group with zero references. It uses the shared host inspection and grouping rules for equal regular-file identity, so repeated glob matches, symlink aliases, and hard-link aliases do not cause the same physical bytes to be scanned or emitted more than once.

Every such artifact carries exact producer ID `dev.intlify/js-reference`. CLI, editor, and in-process M0 orchestration obtain its one immutable `ProducerRevision` from the same build-provenance value compiled into the artifact-producing implementation; project configuration, command options, source contents, worker identity, and invocation state cannot replace it.

That revision covers every output-affecting JS parser, TypeScript grammar, Vue SFC/template frontend, recognizer, static evaluator, source mapping, ordering, and artifact-writer component. A uniquely identifying immutable release version may be used directly. Otherwise the build supplies a deterministic release-plus-source/build fingerprint accepted by the shared `ProducerRevision` grammar; a locally modified artifact-producing build cannot retain an unchanged release revision. CLI and editor builds with different effective producer implementations therefore occupy different cache provenance even when they scan equal source bytes.

Participating logical paths in a group are ordered segment-by-segment by exact UTF-8 bytes, with the shorter equal-prefix path first. The first path is the primary path. The artifact identity is the M0 `Project` namespace plus the exact segment sequence `["js", "module", ...primary_path_segments]`; an ordinary JS, JSX, TS, TSX, or Vue source uses this same producer-family prefix.

The complete prefixed sequence passes through the ordinary checked `ReferenceArtifactIdentity` constructor. Its two fixed prefix segments consume the same segment and byte ceilings as every other segment. If the resulting identity is not admissible, the producer neither truncates nor hashes the path, emits no artifact for that group, and derives `ProducerFailed`.

Every record in a successfully emitted source artifact uses the primary project-relative path as `SourceOrigin.source`. Alias paths do not create duplicate records or additional artifacts. The producer scans the complete selected bytes first, then orders records by exact host-source `(origin.span.start, origin.span.end)`; each recognized call-expression node contributes at most one record. Distinct call sites remain distinct even when scope, domain, selector, reason, and every other semantic field except origin are equal.

Two distinct recognized call sites cannot have the same complete first-argument source range under the admitted parser profile. Encountering such an invalid duplicate range is a frontend invariant failure for the complete group rather than an input-order tie-break. Successfully emitted artifacts are handed to orchestration in canonical `ReferenceArtifactIdentity` order; worker completion, glob enumeration, alias discovery, and AST visitation order are non-semantic.

This per-group partition keeps cache invalidation and editor recomputation local to one physical source. Adding or removing a record in one source changes ordinals only in that artifact, not in artifacts belonging to other sources. M0 never aggregates all sources into one project artifact or partitions them by a delivery graph; every such artifact still names the built-in `["main"]` delivery unit fixed above.

#### M0 selected source profiles

Every logical file matched by `producers.js.include` must have one exact lowercase, case-sensitive supported suffix. Matching is filename-based and tests the longer declaration suffixes first:

| Exact suffix | Frontend grammar profile      | Source goal                            |
| ------------ | ----------------------------- | -------------------------------------- |
| `.js`        | JavaScript                    | bounded unambiguous                    |
| `.jsx`       | JavaScript with JSX           | bounded unambiguous                    |
| `.mjs`       | JavaScript                    | module                                 |
| `.cjs`       | JavaScript                    | CommonJS                               |
| `.ts`        | TypeScript                    | bounded unambiguous                    |
| `.tsx`       | TypeScript with JSX           | bounded unambiguous                    |
| `.mts`       | TypeScript                    | module                                 |
| `.cts`       | TypeScript                    | CommonJS                               |
| `.d.ts`      | TypeScript declaration source | bounded unambiguous                    |
| `.d.mts`     | TypeScript declaration source | module                                 |
| `.d.cts`     | TypeScript declaration source | CommonJS                               |
| `.vue`       | Vue SFC profile fixed below   | module for every admitted script block |

An extensionless file, an uppercase or mixed-case spelling, and every unlisted suffix are unsupported. A selected unsupported source fails its complete physical source group and derives `ProducerFailed`; it is never silently removed from the configured closed inventory. A valid include set that matches no files remains the separately specified successful empty scan.

The producer does not inspect MIME metadata, shebang text, `package.json`, `tsconfig.json`, editor language IDs, or another tool's loader configuration to select or repair a grammar profile or fixed source goal. A future suffix requires an explicit contract and producer-revision change; `.mjsx`, `.cjsx`, arbitrary preprocessor suffixes, and files merely containing familiar JS text receive no implicit compatibility.

The bounded-unambiguous goal is one closed parsing algorithm, not project inference. The frontend first parses the complete source with the table's grammar profile and module goal. Only an ordinary module-goal grammar rejection permits one complete retry of the same bytes and grammar profile with script goal. If module parsing succeeds, its AST is authoritative and no script parse runs. If module rejects and script succeeds, the script AST is authoritative. If both reject, the complete source fails and derives `ProducerFailed`.

A resource-limit failure, invalid UTF-8 input, frontend invariant or internal failure, cancellation, or any other non-grammar failure during the module attempt does not trigger the script retry. No route merges two ASTs, retains records from a rejected attempt, selects a goal per statement or Vue expression, or retries after scanning has begun. The maximum parse-attempt count is therefore one for fixed-goal sources and two for bounded-unambiguous sources.

For this contract, parser success means that the complete selected grammar/source-goal parse reports zero syntax-error diagnostics. A parser may internally recover and return an AST beside one or more syntax errors, but that AST is rejected-attempt state: the producer never scans it, caches it, combines it with another attempt, or emits any record obtained from it. Parser warnings and later lint or semantic diagnostics are outside this syntax-success test and do not make an otherwise valid source fail production.

For a bounded-unambiguous source, any module-attempt syntax error makes that attempt the ordinary grammar rejection eligible for the one script retry. The script attempt succeeds only with zero syntax errors. If it also reports an error, the source fails as `parse` / `syntax_invalid`; the error evidence uses the smallest exact safe source span available under the common producer-failure ordering and never exposes the recovery AST or dependency diagnostic text.

Every participating logical alias in one physical source group derives its profile independently before the group is scanned. All aliases must resolve to the same complete grammar/source-goal profile. An unsupported alias or disagreement such as one physical source selected through both `.ts` and `.vue` names fails the group as `ProducerFailed`; primary-path order never chooses one interpretation and the producer emits neither a partial nor duplicate artifact.

Declaration sources use the selected parser's declaration grammar and still pass through the same complete scan and artifact-production path. A valid declaration source with no recognized runtime call emits the ordinary empty source artifact; declaration spelling does not cause the source to disappear from completeness accounting.

#### M0 source snapshot and encoding

The built-in producer analyzes one complete exact UTF-8 source snapshot. A disk source uses the bytes selected by the shared input-snapshot contract. An authoritative editor buffer uses its exact Unicode scalar sequence and protocol version encoded as UTF-8 under 009; neither route falls back to another snapshot after analysis begins.

The snapshot must be valid UTF-8. Exactly one optional leading UTF-8 signature byte sequence `EF BB BF` is admitted as a BOM. A frontend may consume that signature before invoking its parser, but it retains a checked mapping to the original snapshot so every `SourceOrigin` offset includes those three bytes. Any following `U+FEFF` is ordinary source text interpreted by the selected language grammar rather than another transport instruction.

No layer converts UTF-16, Shift_JIS, Latin-1, or another encoding; applies Unicode normalization; rewrites CRLF to LF; expands tabs; strips trailing whitespace; or replaces malformed input. Invalid UTF-8 or an input that requires transcoding fails the complete physical source group and derives `ProducerFailed`. For a bounded-unambiguous source this is a pre-parse failure and never activates the script retry.

The exact retained bytes are the source-length and source-origin coordinate space. They are also an indivisible input to any built-in producer cache key, along with the separately required producer revision and effective semantic configuration; equal parsed ASTs, equal decoded selector values, or normalized line endings do not make different byte snapshots cache-equivalent.

Vue SFC parsing and every embedded-expression mapping use this same host snapshot. A BOM consumed by the SFC frontend, entity decoding, parser-local UTF-16 coordinates, or any temporary embedded buffer must be mapped back to the exact original `.vue` UTF-8 byte range before a record can be emitted.

#### M0 built-in source admission limits

Built-in JS/Vue source admission has three independent inclusive fixed ceilings:

| Producer counter | Accounting rule | M0 ceiling |
| --- | --- | --: |
| `source_groups` | Number of selected physical source-group occurrences after logical alias grouping. | `65,536` |
| `source_bytes` | Exact bytes in one selected source snapshot, including an admitted UTF-8 BOM and all bytes outside recognized calls. | 64 MiB (`67,108,864` bytes) |
| `source_bytes_total` | Checked sum of exact snapshot bytes across all submitted physical source groups in the built-in producer invocation. | 1 GiB (`1,073,741,824` bytes) |

These are producer-input limits, not `LinkLimits`, reference-artifact decoded/wire limits, parser settings, or resource-catalog `host_bytes`. M0 exposes no configuration or command option that raises, lowers, pools, or transfers them. A later configurable resource policy requires an explicit design; environment variables, worker count, available memory, and platform word size do not silently change the contract.

Alias occurrences in one physical group receive no additional charge, while every distinct selected physical group contributes once even when its bytes equal another group or a cache already contains an equal parse. Empty snapshots contribute zero bytes but still count as groups. BOM stripping, line-ending normalization, zero-copy storage, interning, memory mapping, compression, and parser buffer reuse never reduce either byte counter.

After complete inventory enumeration and physical grouping, the producer preflights `source_groups` before opening source contents, validating source profiles, parsing, cache lookup, or emitting artifacts. Because M0 needs no exact count beyond failure, encountering the 65,537th group records `limit: 65_536` and `observed: 65_537`; it does not continue enumeration merely to report a larger total.

Each selected snapshot then passes `source_bytes` before UTF-8 decoding or parser allocation. A known length and streaming/editor input use the same first-over evidence: the first byte beyond the ceiling records `limit: 67_108_864` and `observed: 67_108_865`. The producer neither retains nor scans the remainder merely to discover its final length.

Only a completely admitted per-source snapshot contributes its exact full length through checked `u64` addition to `source_bytes_total`. Groups are charged in canonical primary `SourceDocumentIdentity` order, independent of acquisition or worker completion. The first addition above 1 GiB records `limit: 1_073_741_824` and the exact checked prospective running sum as `observed`; addition or host-size conversion overflow selects the same failure boundary without wrapped arithmetic.

A per-source `source_bytes` failure emits no artifact for that group, uses `snapshot` / `source_bytes_limit`, and derives ordinary source-attributable `ProducerFailed`; other admissible groups may still produce safe present-world input. A `source_groups` or `source_bytes_total` failure is producer-invocation-wide: it emits one error whose source is the canonical first-over group, discards every built-in artifact and cache-publication candidate from that invocation, and derives `ProducerFailed` for every scope bound by its enabled recognizers. It never links a canonical-order prefix, shards the inventory, truncates a source, retries with a relaxed limit, or changes the selected participant set.

Configured external reference artifacts remain separate inventory participants and retain their own transport, artifact, and request-aggregate limits. They cannot donate budget to this producer, and their presence does not reduce these three source ceilings; the later complete `LinkRequest` still independently enforces its total reference-artifact count and bytes.

#### M0 built-in source artifact cache

The built-in producer cache is an optional performance layer over one successfully admitted physical source group. Its semantic key is the complete tuple below, in this order:

1. the cache-schema revision owned by this producer;
2. the emitted `ArtifactVersion`;
3. the complete `ProducerIdentity`;
4. the primary `SourceDocumentIdentity` followed by every participating logical alias in canonical alias order;
5. the selected grammar-profile and source-goal enum values;
6. every resolved recognizer binding in canonical callee-key order, including exact callee key, call kind, scope, domain, and key syntax;
7. the exact source byte length and standard unkeyed BLAKE3-256 digest of the complete admitted snapshot;
8. the exact built-in delivery-unit ID `["main"]`.

The initial internal cache-schema revision is `1`. It is independent of `ArtifactVersion` and `ProducerRevision`: a cache-storage or key-framing change increments the cache revision, while an output-affecting producer implementation change advances `ProducerRevision` under the provenance contract. Persistent implementations encode the tuple with the repository's checked domain-separated, length-prefixed canonical cache framing before deriving a physical lookup digest; they never concatenate display paths or ambiguous strings. Cache equality is the semantic tuple above, not a filesystem cache filename.

The include-pattern spelling does not enter the key after it has selected the same physical group and aliases. `dynamicReferences`, configured roots, production locales, fallback, coverage baseline, delivery targets, placement, eager locales, and output roots likewise remain linker/export policy and do not affect producer output. Changing any actual recognizer field, primary/alias identity, source bytes, grammar/goal, artifact contract, producer build, or delivery unit produces a different semantic key.

Only a completely successful checked `MessageReferenceArtifact`, including an empty one, is a cacheable value. A syntax-recovery AST, partial record vector, producer error, canceled operation, or failed checked output is never cached. A count or aggregate source-limit failure discards every pending cache-publication candidate together with the invocation's built-in artifacts; completion of one earlier worker does not publish a canonical-prefix cache state.

A hit is admitted only after the current selected snapshot has independently passed `source_bytes` and contributed its full bytes to `source_bytes_total`; cache reuse never deducts producer input work. The consumer then revalidates artifact version, producer identity, expected reference-artifact identity, `["main"]`, record ordering, required origins against the current exact bytes, and all current structural and byte limits before treating the participant as complete.

A missing entry, old cache-schema revision, mismatched key component, malformed or corrupted value, failed revalidation, cache read error, or cache write error is an optimization miss. The implementation discards that value and regenerates from the authoritative current snapshot; it does not emit `ProducerFailed`, weaken completeness, or use a stale value merely because regeneration costs more. A successfully regenerated artifact alone may replace the entry after the complete producer invocation passes its global admission gates.

For an editor buffer, the key depends on its exact current UTF-8 bytes but not its protocol version. Equal bytes may reuse equal production safely; 009's captured document version remains outside the cache as the mandatory publication-freshness gate. A cache hit never authorizes publication for a superseded buffer version and never claims that disk bytes, another open buffer, or an upstream external producer are current.

#### M0 recognizer and key-canonicalization contract

Every JS/TS recognizer binding requires all four fields `kind`, `scope`, `domain`, and `keySyntax`; none has a default and none is inferred. The exact M0 call-kind tokens are `lookup` and `set`, and the exact M0 `keySyntax` tokens are `canonical`, `literal`, and `dot-path`.

```rust
pub enum JsRecognizerCallKind {
    Lookup,
    Set,
}
```

The resolved binding stores this closed enum rather than its raw string. `Lookup` recognizes one ordinary message lookup: a statically known valid string emits `Exact`, while a key expression that cannot be proven static emits `UnboundedDynamic`. `Set` recognizes the explicit finite-set API: its argument must be a statically known valid canonical pattern under the configured syntax and emits `Pattern`; a dynamic or invalid value fails artifact production and is never widened to `UnboundedDynamic`.

The producer never infers call kind from callee spelling, return use, argument contents, imported symbol name, or type information. One configured callee has exactly one kind in M0; overload-dependent or argument-dependent kind selection is not supported.

Every matched call uses its first argument as the only selector input. That argument must exist and must be an ordinary non-spread expression. Later arguments may carry interpolation values, formatting options, or other runtime data and have no linker meaning; the producer parses them as part of the source but does not inspect or constrain their values, count, or spread form.

A matched zero-argument call or a call whose first argument is a `SpreadElement` fails the complete source's producer operation and derives `ProducerFailed`. It is never ignored, reconstructed from later arguments, or represented as `UnboundedDynamic`: after configuration has declared the exact callee to be a message API, silently dropping an unrepresentable invocation would make a closed reference world unsound. A non-spread dynamic first argument to `Lookup` remains representable as `UnboundedDynamic`; the corresponding dynamic first argument to `Set` remains a producer failure under the finite-pattern rule.

Every reference record emitted by the built-in JS/TS producer carries `SourceOrigin`. Its span is the first argument expression's exact half-open source range as reported by the selected parser. It includes parentheses and admitted TypeScript wrappers that belong to that argument, but excludes the call's opening and closing parentheses, the following comma, and every later argument. The same rule applies to a static selector and `UnboundedDynamic`; the producer never substitutes a decoded-string-content range or the complete call range.

For an ordinary JS/TS file, that range addresses the exact selected file bytes. For a Vue embedded expression, the SFC frontend maps the same argument-expression range to the exact host `.vue` UTF-8 range. An endpoint beyond `u32`, a non-scalar boundary, or a missing, ambiguous, or invalid embedded-to-host mapping fails the complete source's producer operation and derives `ProducerFailed`; this built-in profile does not recover by omitting origin.

The key of each `recognizers` member is one static callee chain. It denotes either a direct `Identifier` call or a non-computed, non-optional `MemberExpression` chain whose root is an `Identifier` or exact `ThisExpression` and whose remaining properties are static identifier names. Thus configured `t`, `i18n.t`, `i18n.global.t`, and `this.$t` match those exact call-expression callee shapes.

The decoded config key contains 1 through 255 ASCII bytes inclusive, including its `.` separators, and splits into 1 through 64 segments. Every segment has the exact grammar `[A-Za-z_$][A-Za-z0-9_$]*`; an empty segment, leading/trailing/consecutive `.`, non-ASCII scalar, or first count/byte above a ceiling is invalid.

The first segment is either exact `this` or an ASCII binding identifier. For a stable conservative M0 contract, the root rejects `await`, `break`, `case`, `catch`, `class`, `const`, `continue`, `debugger`, `default`, `delete`, `do`, `else`, `enum`, `export`, `extends`, `false`, `finally`, `for`, `function`, `if`, `implements`, `import`, `in`, `instanceof`, `interface`, `let`, `new`, `null`, `package`, `private`, `protected`, `public`, `return`, `static`, `super`, `switch`, `throw`, `true`, `try`, `typeof`, `var`, `void`, `while`, `with`, and `yield`; exact `this` is admitted only through the special `ThisExpression` case. A later member may use any ASCII segment accepted by the grammar because a non-computed property is an `IdentifierName`.

Validation and matching are case-sensitive and perform no trimming, escape interpretation beyond JSON decoding, case folding, Unicode normalization, or aliasing. After complete validation and duplicate-member rejection by the 006 parser, resolved recognizers are stored in exact callee-key ASCII-byte order; source/config order and map iteration are non-semantic.

M0 matching is intentionally syntactic. Once a call's callee has the exact configured AST chain, it matches regardless of where the root identifier was imported or declared, whether another lexical scope shadows an equal spelling, what type a checker would assign, or which runtime value it denotes. The producer does not load a TypeScript project, resolve modules or symbols, inspect declaration provenance, or infer that two differently spelled bindings are aliases.

Consequently, a configured `t` matches every exact `t(...)` call in the configured include inventory, including an unrelated shadowed binding; `this.$t` matches that syntax in every applicable `this` context. Projects limit false positives with the producer include set and distinctive explicit callee chains. Every match still retains its exact source origin, and changing lexical bindings without changing the recognized syntax does not alter producer output.

M0 does not match computed properties such as `i18n["t"]`, optional calls or member chains such as `i18n?.t`, a call/new/tagged/private/super-derived receiver such as `getI18n().t`, `super.t`, or any dynamic property. It does not evaluate the config key as JavaScript, execute or fold a receiver expression, rewrite one AST shape into another, or fall back from a rejected shape to a shorter suffix match.

An unknown scope, invalid domain token, or unsupported syntax/domain pair is a structural configuration error before inventory discovery. Equality with the scope's concrete catalog-key domain is checked later, after the complete definition inventory has been attempted and 013 has aggregated domains from successfully extracted entries, but before the built-in producer scans reference sources or emits artifacts.

When one observed domain exists, a different recognizer or configured-root domain is `config_validation_failed` with `details.reason: "scope_key_domain_mismatch"`, the exact pointer to that `/domain` member, and stable `scope`, `domain`, and `observedDomain` fields. Recognizers are checked in their canonical configuration-key order and roots in array order under the messages-local validation sequence. A scope with zero successful definition entries has no observed-domain constraint and accepts any otherwise supported explicit domain. A source failure or omission makes the definition side partial and never manufactures this mismatch. A proven 013 `catalog_scope_key_domain_conflict` wins before any recognizer/root mismatch because no single observed domain exists to compare.

`canonical` means a normal lookup string is already the canonical `CatalogKey` for the selected domain, while a bounding-API string is already the canonical pattern payload for that domain. `literal` treats the entire source string as one literal structural key segment. `dot-path` parses the source string into structural segments as specified below.

M0 supports `literal` and `dot-path` only with `json-pointer`; the YAML and XLIFF domains accept only `canonical`. No producer retries a rejected spelling under another syntax.

The M0 `dot-path` grammar is exact:

- `.` separates segments.
- `\.` encodes a literal dot in one segment, and `\\` encodes one literal backslash.
- Any other backslash escape, including a trailing backslash, is invalid.
- An empty string and a leading, trailing, or consecutive separator are invalid because they would create an empty segment. A caller that needs a domain-valid empty key or segment uses `canonical` or `literal` instead.

For a normal lookup call such as `t(...)`, every parsed segment is literal. The producer JSON-Pointer-escapes each segment by replacing `~` with `~0` and `/` with `~1`, joins the segments with `/`, prefixes the result with `/`, validates the resulting canonical key, and emits `Exact`.

A segment spelled exactly `*` or `**` in a normal lookup remains an ordinary literal asterisk spelling; the producer never infers a `Pattern` from characters in the string.

For a recognizer whose configured kind is `set`, call kind — not string contents — selects `Pattern`. With `dot-path`, only a complete segment spelled `*` or `**` becomes the corresponding pattern operator.

Every other segment is literal and is escaped with `~0`, `~1`, and pattern-only `~2` for `~`, `/`, and `*` before joining into the canonical pattern payload.

Thus `useMessageSet('errors.*')` emits a bounded pattern, while `t('errors.*')` emits one exact key whose final segment is the literal `*`. `literal` likewise escapes every asterisk as a literal; `canonical` validates against the selected call kind's canonical key or pattern grammar without rewriting.

M0's static-string evaluator is deliberately expression-local. It accepts an ECMAScript string literal or a template literal with no substitutions, after recursively removing only parentheses and the value-preserving TypeScript `as` (including `as const`), `satisfies`, non-null, and type-assertion wrappers admitted by the selected JS/TS grammar. The frontend supplies the exact ECMAScript-decoded string value to the configured key-syntax conversion; source escape spelling and wrapper spelling never alter selector identity.

The evaluator does not resolve an identifier or import, inspect a property, follow a binding, or fold a binary, conditional, call, tagged-template, or template-substitution expression. Thus `t(KEY)`, `t('checkout.' + section)`, and ``t(`checkout.${section}`)`` are dynamic even if another analysis could prove their runtime value. `useMessageSet(PATTERN)` likewise fails unless its first argument is one of the admitted expression-local static forms. Static evaluation never changes the configured scope, domain, key syntax, or call kind.

For `Lookup`, only failure to prove the first argument's value statically produces `UnboundedDynamic`. Once the evaluator has produced an exact string, failure of the configured `canonical`, `literal`, or `dot-path` conversion, the selected domain grammar, or any applicable selector size ceiling is a source-attributable `ProducerFailed` with stage `selector`, reason `lookup_selector_invalid`, and the first-argument span. The producer does not retry another key syntax, reinterpret the known invalid value as dynamic, omit the call, or conservatively retain the complete scope.

For `Set`, either inability to prove one admitted static string or failure to convert and validate it as one finite pattern remains a producer failure under the separately fixed `set_selector_dynamic` or `set_selector_invalid` reason. No invalid known selector of either call kind reaches the artifact constructor.

For a recognized normal lookup such as `t(runtimeKey)`, the producer emits `UnboundedDynamic` when constant propagation cannot prove an exact string.

It still records the known scope, domain, producer, and source origin and supplies exact `ReasonText` value `lookup argument is not statically known`. `useMessageSet('errors.*')`, by contrast, emits the corresponding bounded `Pattern` with exact reason `bounded set declared by configured recognizer`; producer policy never rewrites one call kind into the other.

These two fixed English strings are the only `ReasonText` values emitted by the M0 built-in JS/Vue producer. Every built-in `UnboundedDynamic` and `Pattern` record includes its applicable value, while every built-in `Exact` record omits `reason`. Project configuration, source comments, callee spelling, selector spelling, parser diagnostics, and reporter locale cannot replace, prefix, suffix, or localize either artifact value.

The strings are human-readable provenance only. The linker never parses them as a reason code, policy switch, selector, rule identity, or compatibility signal; call kind and `MessageSelector` remain the typed semantic authority. Their presence and exact bytes participate in artifact equality, canonical emission, accounting, and cache values under the ordinary `ReasonText` contract without changing reachability.

#### M0 Vue SFC source profile

An included `.vue` source is one Vue SFC producer participant. The frontend parses the complete SFC, then scans both inline `<script>` / `<script setup>` blocks and every JavaScript expression exposed by the standard `<template>` AST, including interpolations, directive values and dynamic arguments, event handlers, and slot expressions. Both script blocks participate when the SFC validly contains both; reference-record order follows exact host-source start position with the ordinary record tie-breakers.

An absent script `lang` means JavaScript. The exact admitted script tokens are `js`, `jsx`, `ts`, and `tsx`, and every admitted inline script block uses module goal without an unambiguous/script retry. The initial template profile requires the standard template syntax with no custom `lang`; Pug and every other preprocessor-owned template language are unsupported. A SFC syntax failure, an unsupported script/template language, an embedded JS/TS parse failure, or inability to construct the required exact host-span mapping fails that source's complete producer operation and derives `ProducerFailed`; no partial artifact silently omits the failed block or expression.

The zero-syntax-error success rule applies independently to the SFC parse, every admitted script block, and every standard-template embedded expression. A recovery AST or partially decoded template AST from any failed component is discarded, and previously scanned blocks or expressions contribute no records or cache value for that source. Parser recovery order never determines which subset survives because no subset survives.

The frontend maps every locally produced `SourceOrigin` back to the original `.vue` source's UTF-8 byte coordinates through one checked monotonic embedded-expression map. It never reports block-local offsets as host offsets, splits a source scalar or decoded template escape, guesses a location after preprocessing, or drops origin merely to retain a reference.

An inline `<script src>` is not followed by the SFC participant and does not expand configured inventory. The referenced external file participates only when it independently matches `producers.js.include`, using its own project source identity and ordinary JS/TS profile. Style blocks and custom blocks are not JavaScript-reference sources under this profile.

Declarative template usage that is not a call expression — for example an `<i18n-t keypath="...">` prop — is not matched by the M0 callee recognizer. A project using such an API declares the affected exact or bounded configured roots, or supplies an external artifact, until the deferred framework-declarative producer contract is promoted; the built-in producer never guesses a component's message semantics from its tag or prop spelling.

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

The macro records one complete native reference entry in a build-time producer dictionary and embeds only its tagged, versioned fixed-width ID into executed code. Scanning the final binary therefore collects the IDs that survived `#[cfg]`, dead-code elimination, and LTO without embedding complete selectors or source paths in the shipped executable:

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

#### N0 reference-ID and producer-dictionary contract

N0 uses dictionary join rather than embedding a complete `MessageReference` record in each binary marker. A marker contains only the exact native-reference magic/tag, an ID-format major/minor, and a 32-byte `NativeReferenceId`.

The ID is an unkeyed BLAKE3-256 digest over a domain-separated, versioned, typed length-prefixed input containing the artifact version, producer identity, producer-stable logical site identity, resolved scope, domain, selector, and optional reason.

Origin is excluded from the digest and binary marker; it remains optional debug-sidecar data keyed by the same ID. The logical site identity makes equal selectors at distinct source sites distinct without exposing a source path in the marker.

Its exact portable codec, bounds, marker bytes, and object-section representation are fixed by the N0 implementation addendum before a producer ships; changing any digest input, framing rule, tag meaning, or ID width is an ID-format compatibility change.

The producer dictionary stores every available ID together with the complete checked digest input and the full reference record needed to reconstruct the artifact.

Dictionary construction recomputes every ID, permits repeated emission of one byte-identical site entry to collapse to one available entry, and rejects one ID associated with different canonical input as a collision. It is build metadata, not a reachability-root list.

The debug sidecar may add the origin for the same site ID but cannot change its selector or make an unavailable entry reachable.

The final scanner validates marker and ID-format compatibility, collects distinct surviving IDs, orders them by exact ID bytes, and joins each against the matching dictionary from the same producer/build.

It recomputes the digest from the dictionary entry before constructing one complete single-delivery-unit `MessageReferenceArtifact`.

An unknown surviving ID, missing dictionary, incompatible version, malformed marker, digest mismatch, or conflicting dictionary entry is a fail-complete native build error; the scanner never skips it, substitutes an unversioned selector, or treats the remaining subset as closed.

Dictionary entries not observed in the final binary remain available references only and do not become surviving roots.

Repeated occurrences of one exact ID represent the same source site and produce one reference record; final artifact ordinals follow canonical surviving-ID order rather than scan offset or section order.

### C/C++

```cpp
INTLIFY_MESSAGE(CheckoutTitle, "checkout.title");
INTLIFY_MESSAGE_SET(ErrorMessages, "errors.*");
```

Macros use the same tagged ID plus producer-dictionary contract and are scanned after final link. They do not select an alternative full-record binary encoding.

How IDs survive LTO, COMDAT folding, and Mach-O/ELF/PE handling must be validated before implementation; where exact elimination cannot be guaranteed, the producer over-retains a proven available set and reports degradation — it never guesses in the under-inclusive direction or ignores an unknown surviving ID.

### WASM and other languages

WASM built from Rust or C/C++ reuses the same scanner if the tagged IDs survive into the module. Any other language participates via a compiler plugin, macro/annotation processor, semantic analyzer, object/binary scanner, or an explicit build-system manifest — all producing the same artifact. The linker core never grows language-specific analysis.

### Native build ordering

External message bundles need no cycle: build the executable, scan it, generate per-locale bundles, ship them alongside.

Baking messages _into_ the binary is circular (selecting messages needs the binary; finishing the binary needs the data) and requires one of: a probe-build → datagen → final-build two-phase build, object-file scanning before final link, generated source/objects added at link time, or a conservative per-crate/library manifest.

**Native v1 ships external bundles**; baked native data is a later milestone.

## Future reusable libraries and plugins

This section records the intended future reachability model only. M0 does not admit package-published reference or definition artifacts; the identity, trust, and final-build binding needed to activate this model are deferred in [Package-provided resources and published artifacts](#package-provided-resources-and-published-artifacts).

A reusable package knows which messages it _may_ reference, but only the final application build knows which package modules, native symbols, or plugins actually survive and must ship. The integration therefore distinguishes two sets:

- **Available references** are every reference that a published library or plugin can contribute. They are capability metadata, not reachability roots by themselves.
- **Surviving roots** are the references selected by the final bundler graph, executable or shared-library scan, or declared plugin delivery graph. Only these roots seed the final reachability calculation when exact selection is available.

| Use case | Reusable package supplies | Final build supplies |
| --- | --- | --- |
| Bundled JS/TS library | module-addressable references and optional definitions | live modules/chunks selected by the bundler graph |
| Native library | reference-ID dictionary and optional definitions | IDs surviving in the final executable or shared library |
| Build-time-known plugin | references, optional definitions, and plugin delivery unit | enabled plugin set and its place in the final delivery graph |
| Runtime-discovered plugin | independently linked bundles and loader metadata | no host-side roots unless the plugin exposes a bounded host contract |

### Bundled JS/TS libraries

For example, `@acme/checkout-ui` may contain a payment module referencing `checkout.pay` and a receipt module referencing `checkout.receipt`. An application that imports only the payment module must not retain the receipt message merely because both references were published in one package.

The bundler integration selects references from its live module graph and attributes them to final delivery units. A published flat artifact containing every library reference is a candidate conservative fallback only after the deferred open-world-participant contract can prove and represent that selection; that future route may over-retain messages and report its dedicated `degraded-analysis`.

The exact module-addressable packaging and selection contract remains part of the deferred package-artifact design; it must not require the linker core to understand JavaScript modules.

### Native libraries

A Rust or C/C++ library can publish a dictionary that maps embedded reference IDs to selectors, but that dictionary describes available references. The final executable or shared-library scan determines which IDs survived dead-code elimination, LTO, and linking; those IDs become the surviving roots.

If the toolchain cannot recover them exactly, the initial contract marks the affected reference side partial. A future promoted conservative-participant contract may instead prove the complete available set, report its dedicated `degraded-analysis`, and retain too much — never too little.

The tagged BLAKE3-256 ID, checked dictionary join, collision rejection, and unknown-ID failure rules are the same N0 contract above; package publication and trust remain deferred separately.

### Build-time-known plugins

A plugin enabled before the final application link participates like another delivery unit. Its reference artifact and optional catalog definitions are composed with the application's inputs, while the host integration declares the plugin node and loading edges in the delivery graph.

This supports separately loaded plugin assets without pretending that all plugin messages belong to the application entry unit.

### Runtime-discovered plugins

A plugin that is unknown until runtime cannot participate in the host application's closed-world link. It is an independent link target and ships its own message bundles and loader metadata.

If such a plugin accesses messages owned by the host, it must expose an exact or bounded selector contract that the host can include at build time.

Without that contract, the affected host scope is open-world. The initial integration marks its reference side partial, produces blocking `partial-completeness`, and refuses output or destructive pruning that depends on a closed scope. Whole-scope retention with a non-blocking participant finding remains deferred.

### Composition invariants

- Merely discovering or installing a package artifact does not make all of its available references reachable.
- When an integration cannot derive surviving roots, the initial contract marks the affected side partial. Only the deferred checked conservative-participant contract may widen to a proven available set and report non-blocking degradation; no version guesses an under-inclusive subset.
- Optional catalog definitions provide candidates for resolution. Their presence does not create message reachability.
- A production link is closed-world only when every configured source, producer, and build-time-known plugin has participated and the relevant `ScopeCompletenessTable` sides are `Closed`. `prune` additionally requires no unbounded-dynamic degradation for the affected scope.

## World Model

- **Production application link: closed world.** Supported locales are a finite declared set; static references come from producers; dynamic references are bounded selectors; unbounded dynamics are rejected or conservatively retained per policy; the final delivery graph fixes the reachability roots; and every target scope is definition-closed and reference-closed before plans are generated.
- **Development: open world.** Catalog additions, HMR, and not-yet-resolved references are tolerated. The integration marks affected sides `Partial(OpenEditorWorld)`; absence-dependent findings are suppressed as specified above and completeness-derived degradation blocks deployable plans, while the available lint/editor findings remain advisory.
- **Future library builds: open or partial world**, closed only at the final application link after the deferred package-artifact contract exists.

Closed world does not mean eager loading: it means the finite message × locale universe is known at build time, while runtime lazily loads exactly the delivery units and locales it needs.

### Adoption gradient

Existing applications are full of `t(dynamicKey)`.

The entry point is compat mode: dynamic scopes are retained whole with warnings, while static references immediately power `unresolved-message` and `unused-message` (outside degraded scopes), per-locale splitting, and typed keys — value on day one with zero API changes. `useMessageSet` / `message_set!

` are the opt-in tightening tools, applied scope by scope where teams want strict mode and maximal slicing. Nothing requires a flag-day migration.

## Platform Exporters

### Output model

The linker's output is a plan, not a runtime format:

```rust
pub struct MessageBundlePlan {
    delivery_unit: DeliveryUnitId,
    locale: Locale,
    messages: Vec<ResolvedMessage>,
}

pub struct ResolvedMessage {
    resolved_scope: ResolvedCatalogScopeId,
    domain: CatalogKeyDomain,
    key: CatalogKey,
    definition_locale: Locale,
    message: MessagePayload,
    definition: DefinitionLocation,
}

impl MessageBundlePlan {
    pub fn delivery_unit(&self) -> &DeliveryUnitId;
    pub fn locale(&self) -> &Locale;
    pub fn messages(&self) -> &[ResolvedMessage];
}

impl ResolvedMessage {
    pub fn resolved_scope(&self) -> &ResolvedCatalogScopeId;
    pub fn domain(&self) -> &CatalogKeyDomain;
    pub fn key(&self) -> &CatalogKey;
    pub fn definition_locale(&self) -> &Locale;
    pub fn message(&self) -> &MessagePayload;
    pub fn definition(&self) -> &DefinitionLocation;
}
```

The plan's `locale` is the requested production locale. `ResolvedMessage::definition_locale` is the locale of the definition selected directly or through fallback; it may therefore differ from the enclosing plan locale.

`ResolvedMessage::definition` is exactly the selected record's `DefinitionLocation { source, entry }`. It supplies portable source evidence for export validation and presentation without retaining the definition artifact envelope.

The resolved message deliberately omits fallback-chain position, producer identity, artifact fingerprint, logical aliases, host spans, and reporter data. Those values neither change the selected deployable message nor authorize an exporter to repeat resolution.

Both structs have private fields and read-only accessors. Only `intlify_linker::link` constructs them after complete resolution, placement, canonical ordering, and deduplication. Neither type exposes a public constructor, struct literal, setter, mutable slice, deserializer, or post-construction normalization route.

They are fully owned immutable values reachable through `LinkOutcome`. Implementations may share equal checked strings and payloads internally, but callers cannot observe or depend on that sharing.

The complete plan set, its outer order, each plan's logical-message uniqueness, and its inner order follow [Shared-message placement policy](#shared-message-placement-policy). Exporters preserve that selection and never interpret absence as an implicit empty pair.

| Exporter                    | Example output                                     |
| --------------------------- | -------------------------------------------------- |
| JavaScript                  | ESM virtual module, JavaScript asset               |
| Web runtime                 | binary message pack                                |
| Rust                        | baked Rust module, external blob                   |
| C/C++                       | generated source, object, external blob            |
| WASM                        | sidecar asset, custom section                      |
| Other languages / platforms | generated modules, source, objects, or data assets |

The table is illustrative, not exhaustive. A language or platform not listed above participates by providing an exporter and build integration that consume the same opaque `ValidatedExportBatch` and return the common `ExportArtifactSet` / `ExportError` result.

Every target uses the same generic `ExportArtifact` envelope; adding an exporter does not add a platform variant to the common contract or require language-specific logic in `intlify_linker`, provided it preserves the plan's resolved messages, locale, delivery-unit identity, and placement.

The exporter owns its target-native representation, packaging, and loader metadata.

### Export transaction model

The build integration and exporter are separate responsibilities. In M3, one export transaction consists of one prepared batch, exactly one selected exporter invocation, one complete in-memory artifact set, and registration of that set.

The integration supplies build context, selects and invokes the exporter, and registers the returned set; the exporter performs no final-build registration itself.

For one M3 CLI command, export preparation is project-shared rather than repeated per selected target. After one successful `LinkOutcome` with plans, orchestration invokes `prepare_export` exactly once over the outcome's common selected definition snapshots. A message-validation failure blocks every selected target before any exporter instance, destination mapping, lock, or registration work; one successful borrowed `ValidatedExportBatch` is reused by every independent target transaction.

Target options such as eager locales change exporter representation but do not change the linker's selected definition records or the MF2 syntax/semantic validity of their exact payloads. A future target type that genuinely selects a different message set requires an explicit preparation-partition contract rather than silently rerunning and duplicating the M3 diagnostic result.

Integrations and exporters need not be paired one-to-one: Vite and Rolldown integrations can share an ESM exporter, and the same integration may reuse a borrowed batch in separate independent transactions with different exporters.

Multiple output records do not require multiple exporters: one Rust exporter can return a baked module and companion blob, for example, in the same `ExportArtifactSet`.

### MF2 syntax and semantic validation export gate

#### Validation scope

`MessageDefinitionArtifact` structural conformance and `intlify_linker` intentionally treat `MessagePayload` as opaque MF2 source. A `MessageBundlePlan` can therefore be computed while parser or parser-owned semantic diagnostics exist, which lets lint and editor consumers preserve key-resolution, missing, and unused analysis over the complete definition set.

Any integration that turns plans into deployable output must pass the complete `LinkOutcome` through one shared export-preparation pipeline before invoking the transaction's selected exporter. The pipeline reads the exact decoded `MessagePayload` retained by every selected `ResolvedMessage`; it never reloads or re-resolves a definition artifact. For each unique selected definition, that pipeline:

1. parses the payload through `ox_mf2_parser`;
2. maps parser diagnostics and skips semantic construction when any parser diagnostic exists;
3. otherwise calls `build_semantic_model` with the exact source owner and parse result; and
4. calls `validate_semantics` over that model and maps every parser-owned semantic diagnostic.

The pipeline runs no configurable `intlify_lint` rule. Parser-owned MF2 syntax and Data Model validation are deployment correctness gates; configurable lint policy remains independent.

Definitions absent from all plans, including unused definitions that will not ship, are outside this export gate; their parser and semantic diagnostics remain visible through ordinary lint/editor diagnostics but do not block deployment of a disjoint valid output set. The gate does not format, normalize, repair, re-escape, or run configurable lint rules over a payload.

#### Definition selection and deduplication

The preparation boundary unions selections from the complete `Some(bundle_plans)` value and validates each stable definition record identity exactly once across repeated placement, locales, and delivery units.

Dedupe is by `ResolvedMessage::definition` — the selected record's `SourceDocumentIdentity` plus its entry-level `EntryReference` under the canonical codec above — never by message text, logical message identity, or plan position. An M3 transaction selects exactly one exporter, so exporter identity is not a dedupe dimension.

Every occurrence with an equal `DefinitionLocation` is linker-guaranteed to carry equal resolved scope, domain, key, definition locale, and exact payload. A disagreement is an internal `LinkOutcome` invariant violation, not a second definition, first/last-wins choice, or request to inspect an external artifact.

Within one `prepare_export` call, implementations may reuse parse and semantic computation for byte-identical payloads only if each selected record retains the same diagnostic mapping and observable result as its own complete validation. The initial contract admits no reuse from an earlier call.

#### Failure behavior

If any linker-owned finding blocks plans and `bundle_plans` is `None`, there is no export transaction and the message-validation gate is not run merely to manufacture export errors.

`Some(Vec::new())` has an empty validation set, passes the gate, and produces a valid empty `ValidatedExportBatch`. The integration still invokes the transaction's one selected exporter exactly once.

That exporter may return an empty checked `ExportArtifactSet` or target-native bootstrap, loader, or metadata artifacts that do not require a message plan. The ordinary output contract validates either result; orchestration never substitutes an automatic empty set or rejects the batch merely because `plans().is_empty()`.

For a non-empty set, one parser or semantic diagnostic fails the complete export transaction: no exporter is invoked, no asset, generated source, blob, loader map, or manifest is registered, and `--check` also reports failure.

The failure belongs to the build/export integration surface, not `LinkFinding` or `LinkOperationalError`, because linking itself completed over a structurally valid request.

#### Diagnostic ordering and retention

The gate validates every unique selected definition in stable definition-record identity order even after the diagnostic reporting limit is reached.

Within one record, parser diagnostics preserve `ox_mf2_parser` order. When that list is empty, semantic diagnostics preserve the deterministic order owned by `validate_semantics`. The gate checks the portable mapping contract for every diagnostic and maps each retained diagnostic to that record's source evidence until the effective bounded limit is full. A record never contributes both categories because semantic construction is forbidden after parser diagnostics.

Later diagnostics are still mapping-validated and counted but are not retained, so the final failure can report the exact total and whether its returned list is truncated without making memory proportional to all validation output. Checked arithmetic applies to the total.

The shared export-validation contract sets an inclusive hard ceiling of `10,000` retained mapped message diagnostics per export transaction. An integration uses `1,000` when the caller omits an explicit retention limit; the caller may select any value from zero through `10,000`.

Zero deliberately retains no diagnostic records while still validating the complete selected set and reporting the exact nonzero total and truncation state. A value above `10,000` is rejected before parsing rather than clamped.

This retention budget is separate from linker-core `LinkLimits`: it neither changes `LinkOutcome` nor limits how many selected definitions, parser diagnostics, or semantic diagnostics are examined.

The initial M3 CLI always supplies the exact effective retention limit `1,000`. It exposes no `--diagnostic-limit`, config field, environment variable, reporter override, target option, or worker-dependent adjustment. JSON emits the retained canonical prefix plus exact total and truncation state; the text reporter presents that same retained prefix and the exact omitted count when nonzero.

Programmatic integrations retain the checked `0..=10,000` API choice. A future CLI control requires an explicit argument/config and reporter contract; it cannot appear as an undocumented pass-through to the programmatic option or change cache/link identity.

`ExportValidationLimits` is the only programmatic preparation-limit input. It has one private immutable `u32` field and this checked construction boundary:

```rust
pub struct ExportValidationLimits {
    diagnostic_retention: u32,
}

pub struct ExportValidationLimitConfigurationError {
    submitted: u32,
}

impl ExportValidationLimits {
    pub const MAX_DIAGNOSTIC_RETENTION: u32 = 10_000;

    pub fn protocol_defaults() -> Self;

    pub fn try_with_diagnostic_retention(
        self,
        value: u32,
    ) -> Result<Self, ExportValidationLimitConfigurationError>;

    pub fn diagnostic_retention(&self) -> u32;
}

impl ExportValidationLimitConfigurationError {
    pub fn submitted(&self) -> u32;
}

impl Default for ExportValidationLimits {
    fn default() -> Self {
        Self::protocol_defaults()
    }
}
```

`protocol_defaults()` selects exactly `1,000`. `Default` is part of the public API and is exactly identical to `protocol_defaults()`; it never selects zero, reads configuration or environment state, or changes according to the caller.

The initial M3 CLI still supplies its resolved value explicitly. `Default` is a safe programmatic convenience, not a replacement for external configuration decoding or precedence.

`try_with_diagnostic_retention` accepts every value from zero through the inclusive `10,000` ceiling and returns a new complete value. The first value above that ceiling returns `ExportValidationLimitConfigurationError` without clamping or changing the original limits value.

The configuration error stores only the submitted value and exposes it through `submitted()`. It has no public constructor, setter, deserializer, maximum field, presentation string, or independently supplied counter.

The fixed ceiling comes from `ExportValidationLimits::MAX_DIAGNOSTIC_RETENTION`; presentation text is derived outside the error.

A direct Rust caller that supplies an admitted `u32` above the preparation ceiling receives `ExportValidationLimitConfigurationError` before `prepare_export` is called. The initial M3 CLI has no raw retention option or configuration surface and always supplies `1,000`, so it has no `invalid_options` emission path for this error.

A future CLI control or custom raw-input adapter must define its own decoding and error-mapping contract before exposing the value. A numeric value outside `u32` belongs to that adapter's raw-input boundary rather than to `ExportValidationLimits`; this shared type does not prescribe a blanket `invalid_options` mapping.

#### Message-validation failure contract

The checked failure value has exactly this result-level shape:

```rust
pub struct ExportMessageValidationFailure {
    diagnostics: Vec<MappedMessageDiagnostic>,
    total_diagnostics: u64,
    truncated: bool,
}

impl ExportMessageValidationFailure {
    pub fn diagnostics(&self) -> &[MappedMessageDiagnostic];
    pub fn total_diagnostics(&self) -> u64;
    pub fn truncated(&self) -> bool;
}
```

It exists only when `total_diagnostics > 0`. `diagnostics` is the retained deterministic prefix and its length never exceeds the effective retention limit.

Its fields are private. Only export preparation's crate-private checked constructor creates it; there is no public constructor, struct literal, setter, mutable diagnostic slice, deserializer, default, or unchecked conversion.

The checked constructor requires `total_diagnostics >= diagnostics.len()` after a checked length conversion and sets `truncated` if and only if `total_diagnostics > diagnostics.len()`. The three public accessors expose only the immutable diagnostic prefix, exact total, and checked truncation state.

The omitted count is therefore the exact total minus the checked `u64` conversion of `diagnostics().len()` and is not stored redundantly; the effective limit is request context and is also not copied into the failure. Structured camel-case adapters read the accessors and expose the same three fields as `diagnostics`, `totalDiagnostics`, and `truncated`.

#### Portable diagnostic mapping

Every `MappedMessageDiagnostic` is anchored by the selected `ResolvedMessage`'s exact `DefinitionLocation`: its `SourceDocumentIdentity` and `EntryReference`.

Its primary span and every label span use half-open UTF-8 byte offsets into that definition's exact decoded `MessagePayload`; they retain the parser's message-local values without shifting them by an entry or host-file offset.

The shared record does not claim that these are host-document coordinates and does not serialize a `MessageOffsetMap`, raw-value span, host line/column, or a second host-coordinate span solely for export validation.

This makes the result complete for a published or third-party definition artifact even when the original host document and 013 extraction artifact are unavailable.

A local integration that still owns the matching 013 extraction artifact may map the message-local spans through its validated `MessageOffsetMap` when constructing CLI, editor, or build-tool presentation.

Those derived host coordinates belong only to that presentation DTO; they do not replace or mutate the shared failure, alter diagnostic ordering or counting, or become required evidence for invoking an exporter.

An integration without that local map reports the portable source identity, entry evidence, and message-local coordinates.

The mapped record is self-contained after it copies the selected resolved-message evidence. Its conceptual read-only Rust shape is:

```rust
pub struct MappedMessageDiagnostic {
    definition: DefinitionLocation,
    kind: MappedMessageDiagnosticKind,
    severity: DiagnosticSeverity,
    message: &'static str,
    span: MessageUtf8Span,
    labels: Vec<MappedMessageLabel>,
}

pub enum MappedMessageDiagnosticKind {
    Parser(DiagnosticCode),
    Semantic(SemanticDiagnosticCode),
}

pub struct MappedMessageLabel {
    span: MessageUtf8Span,
    message: &'static str,
}

pub struct MessageUtf8Span {
    start: u32,
    end: u32,
}

impl MappedMessageDiagnostic {
    pub fn definition(&self) -> &DefinitionLocation;
    pub fn kind(&self) -> &MappedMessageDiagnosticKind;
    pub fn severity(&self) -> DiagnosticSeverity;
    pub fn message(&self) -> &'static str;
    pub fn span(&self) -> &MessageUtf8Span;
    pub fn labels(&self) -> &[MappedMessageLabel];
}

impl MappedMessageDiagnosticKind {
    pub fn category(&self) -> &'static str;
    pub fn code(&self) -> &'static str;
}

impl MappedMessageLabel {
    pub fn span(&self) -> &MessageUtf8Span;
    pub fn message(&self) -> &'static str;
}

impl MessageUtf8Span {
    pub fn start(&self) -> u32;
    pub fn end(&self) -> u32;
}
```

The Rust fields are private and exposed only through the exact read-only accessors above. There is no public constructor, struct literal, setter, mutable label slice, deserializer, default, unchecked conversion, or partial builder for any of the three record/span structs.

Construction is mapper-owned and accepts one existing selected `DefinitionLocation`, the exact decoded payload, and either an `ox_mf2_parser` parser diagnostic or parser-owned `SemanticDiagnostic` rather than independently supplied field values. It clones the complete linker-owned definition identity from `ResolvedMessage::definition()` and never reconstructs a source/entry pair through another public path.

`MappedMessageDiagnosticKind` keeps category and code inseparable. `category()` returns exactly `"parser"` or `"semantic"`, and `code()` returns the corresponding parser-owned kebab-case JSON code. Rust consumers that need the typed code match the public enum rather than reparsing that string.

The mapper drops parse-workspace-local `SourceId` and computed `location`, retains severity, borrows the exact `&'static str` message from the applicable parser-owned diagnostic catalog without localization or rewriting, and converts the primary span and labels in their category-owned order.

`MessageUtf8Span` retains the parser's `u32` half-open byte offsets and does not require an additional Unicode-scalar-boundary rule.

Static catalog references make the failure independent of the parser workspace without allocating one duplicate message string per record; their exact text is presentation, not diagnostic identity.

#### Diagnostic mapper invariants

Each mapped diagnostic has an inclusive hard ceiling of `32` labels with no caller override. The checked mapper validates the complete parser or semantic label count before allocating or admitting the mapped record, then validates every span and requires every label to refer to the same parser source as the primary diagnostic.

An inverted or out-of-payload span, foreign label source, or thirty-third label is an integration invariant failure: the integration discards the partial message-validation failure, invokes no exporter, and reports an internal operational failure rather than silently dropping labels or setting the diagnostic-list `truncated` flag.

An error from `build_semantic_model` or `validate_semantics` is likewise an operational preparation failure, not an ordinary message diagnostic. Export preparation preserves the 012-owned distinction. Passing a parse result that still has parser diagnostics or a detectably mismatched `SourceStore` / `ParseResult` pair uses `semantic_api_misuse` with no additional required details field. A valid pair that fails model construction uses `semantic_invariant_failed` with required stage `semantic_model_construction`; semantic validation failure uses the same reason with required stage `semantic_validation`. Every case discards any partially accumulated diagnostic failure and invokes no exporter. A conforming preparation path checks parser diagnostics and passes the original owner/result pair, so misuse is an implementation bug, but it is never relabeled or converted into a panic.

These invariant failures may stop the gate; complete-set validation applies to ordinary parser and semantic diagnostics, not trusted parser/integration contract violations.

Preparation-owned invariant failures use this closed hierarchy:

```rust
pub enum ExportPreparationInvariant {
    DiagnosticCountOverflow,
    DiagnosticMapping(DiagnosticMappingInvariant),
    OutcomeContract(OutcomeContractInvariant),
}

pub enum DiagnosticMappingInvariant {
    LabelCountExceeded,
    InvalidPrimarySpan,
    InvalidLabelSpan,
    ForeignLabelSource,
}

pub enum OutcomeContractInvariant {
    DuplicatePlanCoordinate,
    NonCanonicalPlanOrder,
    DuplicateLogicalMessage,
    NonCanonicalMessageOrder,
    DefinitionSnapshotMismatch,
}
```

`DiagnosticCountOverflow` means the exact `u64` diagnostic total could not admit the next ordinary diagnostic. It is not a configurable limit and never truncates, saturates, or wraps the reported total.

`DiagnosticMappingInvariant` is limited to contradictions between a parser-owned diagnostic and the portable mapping contract. Its variants respectively mean more than `32` labels, an inverted or out-of-payload primary span, an inverted or out-of-payload label span, or a label whose parser source differs from the primary diagnostic's parser source.

`OutcomeContractInvariant` is limited to a state that the private checked `LinkOutcome` construction forbids. Its variants respectively mean a repeated delivery-unit/requested-locale coordinate, plan order that contradicts the canonical coordinate order, a repeated logical-message identity within one plan, message order that contradicts the canonical logical-message order, or unequal resolved snapshots attached to one equal `DefinitionLocation`.

Preparation selects failures deterministically:

1. It preflights the complete outcome contract before parsing. If several outcome contradictions are detectable, `OutcomeContractInvariant` declaration order wins, followed by the smallest affected canonical plan, message, or definition identity for that variant.
2. After a clean preflight, it visits unique definitions in stable definition-record identity order. The first record with a semantic API failure or preparation-owned invariant determines the result; later records do not run merely to collect another operational error.
3. For each ordinary diagnostic, it checks label count, primary span, label spans in parser-owned label order, and label-source equality in that order. These checks run even after the retention budget is full.
4. Only after that diagnostic's mapping contract succeeds does preparation checked-add one to `total_diagnostics`. A mapping invariant on that diagnostic therefore precedes `DiagnosticCountOverflow`.

An operational failure discards every ordinary diagnostic accumulated from earlier records and returns no batch. Input order, hash-map order, optional within-call computation reuse, worker completion, and retention-limit fullness cannot change the selected failure.

Structured adapters project `definition().source()` and `definition().entry()` and expose fields in this order: `source`, `entry`, `category`, `code`, `severity`, `message`, `span`, and `labels`; labels contain `span` then `message`. The wire shape does not add a nested `definition` object merely because the Rust representation reuses `DefinitionLocation`.

Parser and semantic `code` values use their shared kebab-case tooling spellings, `category` uses the shared 008 spellings, and `severity` uses the shared lowercase spelling. An export reporter does not define a second diagnostic vocabulary.

Neither `SourceId`, `location`, nor host coordinates are added. `category` is required because one failure may contain parser diagnostics from one selected definition and semantic diagnostics from another.

Adapter serialization reads the static catalog strings directly and is governed by its ordinary bounded reporter/output writer; the in-memory failure has no separate diagnostic-message-byte counter.

A future parser or semantic diagnostic with input-dependent or otherwise owned message text requires an explicit bounded owned-message contract and cannot silently change these fields from `&'static str` to `String`. The concrete checked wrapper supplied to exporters remains separate.

All platform integrations use the same parser, SemanticModel, and semantic-validation contracts rather than implementing target-specific MF2 validity rules. They map each ordinary diagnostic back through the selected `ResolvedMessage::definition`.

The initial implementation performs a new complete export-validation operation for every `prepare_export` call. Only duplicate work inside that call may be shared under the rule above; cross-call or persistent export-validation caching is deferred.

A third-party artifact, direct exporter selection, or custom platform integration cannot opt out of this gate when claiming deployable output. Parsing and semantic validation remain outside `intlify_linker`; `intlify_export` owns the shared export-preparation boundary and the exporter trait.

#### Export preparation API

##### Preparation entry point

`crates/intlify_export` exposes this conceptual Rust API:

```rust
pub fn prepare_export<'a>(
    outcome: &'a LinkOutcome,
    limits: ExportValidationLimits,
) -> Result<Option<ValidatedExportBatch<'a>>, ExportPreparationError>;

pub enum ExportPreparationError {
    MessageValidation(ExportMessageValidationFailure),
    SemanticModelConstruction(SemanticInvariantError),
    SemanticValidation(SemanticInvariantError),
    InternalInvariant(ExportPreparationInvariant),
}

pub struct ValidatedExportBatch<'a> {
    outcome: &'a LinkOutcome,
}

impl ValidatedExportBatch<'_> {
    pub fn plans(&self) -> &[MessageBundlePlan];
}
```

##### Output artifact types

```rust

pub struct ExportArtifactSet {
    artifacts: Vec<ExportArtifact>,
}

pub struct ExportArtifact {
    logical_path: ExportArtifactPath,
    kind: ExportArtifactKind,
    format_version: ExportArtifactFormatVersion,
    payload: ExportArtifactPayload,
    metadata: ExportArtifactMetadata,
}

pub struct ExportArtifactPath {
    segments: Vec<ExportArtifactPathSegment>,
}

pub struct ExportArtifactPathSegment(String);

pub struct ExportArtifactKind(String);

pub struct ExportArtifactFormatVersion {
    major: u16,
    minor: u16,
}

pub struct ExportArtifactPayload(Box<[u8]>);

pub struct ExportArtifactMetadata {
    media_type: Option<ExportMediaType>,
    relationships: Vec<ExportArtifactRelationship>,
}

pub struct ExportMediaType(String);

pub struct ExportArtifactRelationship {
    kind: ExportArtifactRelationshipKind,
    target: ExportArtifactPath,
}

pub enum ExportArtifactRelationshipKind {
    EagerLoad,
    LazyLoad,
}
```

##### Export error types

```rust

pub struct ExportError {
    evidence: ExportErrorEvidence,
}

pub enum ExportErrorKind {
    UnsupportedBatch,
    GenerationFailed,
    OutputLimitExceeded,
    InvalidOutput,
    InternalInvariant,
}

pub enum ExportErrorEvidence {
    UnsupportedBatch(UnsupportedBatchEvidence),
    GenerationFailed(GenerationFailedEvidence),
    OutputLimitExceeded(OutputLimitExceededEvidence),
    InvalidOutput(InvalidOutputEvidence),
    InternalInvariant(InternalInvariantEvidence),
}

pub struct UnsupportedBatchEvidence {
    feature: UnsupportedBatchFeature,
    location: UnsupportedBatchLocation,
}

pub enum UnsupportedBatchFeature {
    BatchComposition,
    DeliveryUnitPartitioning,
    LocalePartitioning,
    FallbackSemantics,
    MessageSemantics,
}

pub enum UnsupportedBatchLocation {
    Batch,
    Plan {
        delivery_unit: DeliveryUnitId,
        locale: Locale,
    },
    Definition {
        delivery_unit: DeliveryUnitId,
        locale: Locale,
        source: SourceDocumentIdentity,
        entry: EntryReference,
    },
}

pub struct GenerationFailedEvidence {
    stage: GenerationStage,
    location: GenerationLocation,
}

pub enum GenerationStage {
    MessageCompilation,
    PayloadEncoding,
    MetadataGeneration,
}

pub enum GenerationLocation {
    Batch,
    Plan {
        delivery_unit: DeliveryUnitId,
        locale: Locale,
    },
    Definition {
        delivery_unit: DeliveryUnitId,
        locale: Locale,
        source: SourceDocumentIdentity,
        entry: EntryReference,
    },
}

pub struct OutputLimitExceededEvidence {
    counter: OutputLimitCounter,
    observation: OutputLimitObservation,
}

pub enum OutputLimitCounter {
    ArtifactRecordsPerSet,
    ArtifactPathSegmentsPerPath,
    ArtifactPathSegmentBytes,
    ArtifactPathBytesPerPath,
    ArtifactPathBytesPerSet,
    ArtifactKindBytes,
    MediaTypeComponentBytes,
    MediaTypeBytes,
    RelationshipRecordsPerArtifact,
    RelationshipRecordsPerSet,
    RelationshipTargetBytesPerSet,
    PayloadBytesPerArtifact,
    PayloadBytesPerSet,
}

pub enum OutputLimitObservation {
    Exact(u64),
    ArithmeticOverflow,
}

pub struct InvalidOutputEvidence {
    violation: InvalidOutputViolation,
}

pub enum InvalidOutputViolation {
    ArtifactEnvelope,
    ArtifactPath,
    DuplicateArtifactPath,
    ArtifactKind,
    FormatVersion,
    MetadataEnvelope,
    MediaType,
    RelationshipEnvelope,
    RelationshipKind,
    RelationshipTarget,
    DuplicateRelationship,
    ConflictingRelationshipClassification,
    RelationshipSelfEdge,
    LazyTargetInEagerClosure,
}

pub struct InternalInvariantEvidence {
    invariant: InternalInvariantViolation,
}

pub enum InternalInvariantViolation {
    ValidatedBatchContract,
    CapabilityPreflightContract,
    GenerationState,
    OutputBudgetAccounting,
    ArtifactAssemblyState,
    SharedConstructorState,
}
```

##### Exporter trait

```rust

pub trait PlatformExporter: Send {
    fn export(
        &self,
        batch: &ValidatedExportBatch<'_>,
    ) -> Result<ExportArtifactSet, ExportError>;
}
```

##### Configured exporter construction

`PlatformExporter` deliberately receives no target context or untyped option bag. Before one target transaction begins, the owning registry resolves the exact exporter ID and constructs one immutable exporter instance from that exporter's checked typed options.

The M3 built-in registry in `intlify_cli` maps exact ID `"esm"` to a private typed factory for the built-in exporter supplied by `intlify_export`. Conceptually, it constructs:

```rust
pub struct EsmExporterOptions {
    production_locales: ProductionLocales,
    fallback: FallbackSources,
    eager_locales: EagerLocales,
}

pub struct EagerLocales(Vec<Locale>);

pub struct EsmExporter {
    options: EsmExporterOptions,
}
```

The concrete fields are private checked values with read-only accessors. The factory receives the production-locale set and fallback table from the exact immutable `LinkPolicy` used to create the current `LinkRequest`, plus the selected target's resolved eager-locale set. It revalidates every eager and fallback locale against that production set before exposing the exporter.

The built-in constructor and factory are not a generic map-valued plugin API. An in-process third-party exporter registers its own typed factory and validation boundary before yielding `Box<dyn PlatformExporter>`; the trait's `Send` supertrait makes that instance movable into one integration-owned worker. No common `Any`, JSON object, string map, environment lookup, downcast, or exporter-specific union is added to the object-safe trait.

One configured target creates one exporter instance and exactly one invocation. Multiple targets receive independent immutable instances even when their exporter ID and options are equal. A `ValidatedExportBatch` may still be borrowed by several separate transactions, including concurrent transactions, but no exporter instance combines target results or carries mutable cross-transaction state.

`PlatformExporter` deliberately does not require `Sync`. The initial contract never shares one exporter instance across workers or invokes it more than once; requiring thread-safe shared instance state would constrain third-party implementations without enabling an admitted transaction shape.

`target.name`, `out`, the project root, command flags, reporter, worker identity, filesystem handles, virtual-module registry, and platform destination capability remain integration context and are never stored in `EsmExporterOptions` or passed through the exporter trait. The integration attaches target identity to result/error placement and registers the returned logical paths only after export succeeds.

The built-in orchestration constructs the linker request and ESM options from one resolved configuration snapshot. A detected mismatch between that batch's production-locale/fallback semantics and the privately constructed options is `InternalInvariantViolation::CapabilityPreflightContract`, not a request to consult another config snapshot or repair the batch. Configuration validation remains the user-facing boundary for invalid raw target options.

#### Validated batch lifecycle

`prepare_export` returns `Ok(None)` for `bundle_plans: None` without running the message-validation gate. It returns `Ok(Some(batch))` for a parser-and-semantically clean `Some`, including one valid empty batch for `Some(Vec::new())`.

Ordinary parser or semantic diagnostics return `ExportPreparationError::MessageValidation(ExportMessageValidationFailure)`. A `build_semantic_model` error returns `SemanticModelConstruction`; a `validate_semantics` error returns `SemanticValidation`; and checked-count overflow, diagnostic-mapping contradiction, or invalid outcome/batch state returns `InternalInvariant`. Every error returns no batch.

The two semantic variants retain the exact parser-owned `SemanticInvariantError`. The integration combines their call-site stage with its 012-owned `kind()` classification: `ApiMisuse` remains `semantic_api_misuse`, while `InvariantViolation` remains `semantic_invariant_failed` at `semantic_model_construction` or `semantic_validation`. It does not copy those conditions into `ExportPreparationInvariant` or relabel them with an 014-owned reason.

Platform integrations map these typed distinctions to their own 006-compatible user-facing surface, but no variant is a `LinkFinding` or `LinkOperationalError`.

`ValidatedExportBatch` has private fields, no public or unsafe constructor, no deserializer, and no persisted proof-token format.

Successful private construction by `prepare_export` is the complete validation proof. The batch stores the immutable `outcome` borrow and no per-message validated flag, status map, definition digest list, location proof vector, independently editable marker, or serialized capability.

It borrows the exact `LinkOutcome`. Its only public data accessor is `plans()`, which returns the outcome's complete canonical read-only `MessageBundlePlan` slice; selected message-validated `ResolvedMessage` values are reached only through each plan's `messages()` accessor.

The batch exposes no finding access, outcome accessor, flattened or unique-definition view, definition-artifact collection, parser workspace, parse result, semantic model, mutable cache handle, target option, or exporter-specific data. Slice `len`, `is_empty`, and iteration provide the ordinary collection operations without duplicate batch methods.

Preparation groups the outcome's selections by exact `DefinitionLocation`, verifies the linker-owned equal-snapshot invariant for repeated placements, and validates each unique selected payload exactly once.

The immutable outcome borrow prevents mutation or replacement of plans or selected payload snapshots while the batch lives. Another `LinkOutcome`, including one produced from changed definition artifacts, requires a new complete call to `prepare_export`; an earlier batch or private implementation state never validates it.

The batch and every value reachable through `plans()` have no interior mutability or process-global dependency and are `Send + Sync`. Preparation retains no parser workspace, non-thread-safe AST, mutable semantic state, or cache guard in the successful batch.

The batch is borrowed by exporter calls rather than consumed, so one successful preparation can feed separate independent exporter invocations without repeating message validation. Each M3 export transaction nevertheless passes the batch to exactly one independently constructed exporter; reusing the batch does not combine those invocations into one transaction.

An empty batch follows that same rule. `Ok(Some(batch))` always permits and requires the selected transaction to invoke its exporter once; only `Ok(None)` means that no exporter transaction exists.

An integration may move those separate `Send` exporter instances to different workers and share the same immutable batch through scoped concurrency. Worker count, scheduling, runtime, cancellation, join barrier, deterministic target-result ordering, and error aggregation remain integration-owned. Neither `prepare_export` nor `PlatformExporter::export` creates a thread or chooses a runtime.

This capability does not activate a target worker pool in the initial M3 CLI, whose selected-target execution remains sequential until the shared CLI scheduler follow-up is promoted.

`PlatformExporter` has no associated output or error types: every implementation returns the same concrete `ExportArtifactSet` / `ExportError` result, so a heterogeneous runtime registry can store trait objects without an exporter-specific type-erasure adapter.

`ExportArtifactSet` is the complete in-memory result of one exporter invocation and contains a private `Vec<ExportArtifact>` exposed read-only in its checked canonical order. It does not itself register files or mutate the final build; `ExportError` returns no partial set.

### Exporter error contract

#### Boundary and evidence model

`ExportError` begins only after a `PlatformExporter` is invoked with a valid `ValidatedExportBatch`.

It owns failures while the exporter interprets that batch for its selected representation, generates in-memory bytes and metadata, or submits its candidate result through the shared checked `ExportArtifactSet` constructor.

A message-validation-gate or batch-preparation failure remains `ExportPreparationError`; a linker finding or linker operation remains in `LinkOutcome` / `LinkOperationalError`; and unsupported platform capabilities, destination mapping, collision under platform rules, and final registration remain integration operational errors after a successful export.

An exporter performs no final-build mutation or platform registration, so it cannot wrap those integration failures as `ExportError`.

The common error is a checked value with one private `evidence: ExportErrorEvidence` field. `ExportErrorKind` is a derived read-only classification, not independently stored data.

It is not an arbitrary message string, `anyhow::Error`, `Box<dyn Error>`, `Any`, platform error, opaque source chain, or exporter-specific payload. Human-readable messages and platform-specific presentation are derived from the common kind and evidence outside the contract.

Built-in and third-party exporters map their internal failures into this same bounded deterministic contract and return no candidate or partial set. The closed kinds are:

| `ExportErrorKind` | Exact responsibility |
| --- | --- |
| `UnsupportedBatch` | The batch is valid under the common contract, but the selected exporter cannot represent one of its required plan or definition features in that exporter's output representation. It does not mean that the eventual platform cannot register an otherwise valid exported artifact. |
| `GenerationFailed` | The exporter supports the batch feature but its deterministic representation generation, message compilation, or byte encoding operation fails before a valid candidate set exists. It does not wrap final-build I/O. |
| `OutputLimitExceeded` | A shared artifact-count, logical-path, relationship, payload, or other common output resource ceiling is exceeded either during bounded generation or final checked construction. A stricter platform destination limit remains an integration error. |
| `InvalidOutput` | The candidate violates a non-limit common output contract such as path grammar or uniqueness, kind/version/metadata conformance, relationship resolution or semantics, or canonical set invariants. |
| `InternalInvariant` | Exporter logic detects an impossible internal state that is neither an unsupported valid batch, an ordinary generation failure, nor a candidate output-contract rejection. |

There is no `Other`, `Custom`, `Unknown`, `Io`, `Platform`, `Cancelled`, or open namespaced variant. Allocation failure is not modeled as a recoverable `ExportError`.

The kind describes the common failure class rather than exporter identity or presentation severity. Exact evidence records and their bounds are fixed next; no evidence may change the meaning of its enclosing kind.

#### Unsupported batches

`UnsupportedBatch` carries exactly one `UnsupportedBatchEvidence { feature, location }`, not an unbounded or truncated collection.

The exporter completes a read-only capability preflight before payload or metadata generation and selects the first unsupported requirement in canonical plan order, then stable definition-record identity order.

When more than one unsupported feature applies at the same location, declaration order of the closed `UnsupportedBatchFeature` enum breaks the tie: `BatchComposition`, `DeliveryUnitPartitioning`, `LocalePartitioning`, `FallbackSemantics`, then `MessageSemantics`. Existing variants are never reordered.

The same valid batch and exporter implementation therefore produce the same evidence regardless of input enumeration, hash-map iteration, or worker scheduling.

`UnsupportedBatchLocation::Batch` is used only when the unsupported requirement belongs to the complete batch or combination of plans and has no narrower valid coordinate. `Plan { delivery_unit, locale }` identifies a plan-level requirement by its stable semantic identity.

`Definition { delivery_unit, locale, source, entry }` identifies a particular definition use within a plan; it retains the plan coordinate because the same stable definition may be representable in one plan context but not another. `source` is the exact `SourceDocumentIdentity`, and `entry` is the stable `EntryReference` already used by batch preparation and syntax evidence.

No variant stores a vector index, display path, host path, source span, payload excerpt, or generated-output path.

The selected exporter identity remains in the invoking registry/integration context and is not duplicated inside the evidence. Presentation joins that context with the typed evidence when rendering a diagnostic.

`UnsupportedBatchEvidence` has private fields, checked construction, and read-only accessors; callers and third-party exporters cannot attach a free-form reason or invent another location variant.

The five features have exact common meanings and checked location combinations:

| `UnsupportedBatchFeature` | Required location | Meaning |
| --- | --- | --- |
| `BatchComposition` | `Batch` | The exporter cannot represent the otherwise valid combination of plans as one export invocation. This is a semantic capability mismatch, not the artifact-count or another resource ceiling. |
| `DeliveryUnitPartitioning` | `Plan` | The exporter cannot preserve the plan's required delivery-unit partition or placement. |
| `LocalePartitioning` | `Plan` | The exporter cannot preserve the plan's required per-locale separation and locale identity. |
| `FallbackSemantics` | `Plan` | The exporter cannot carry the fallback chain required for that plan without changing its resolved loading semantics. |
| `MessageSemantics` | `Definition` | The message is syntax-valid, but the exporter knows before generation that its target representation cannot preserve that message's MF2 semantics losslessly. |

Checked construction rejects every feature/location mismatch. A known unsupported message construct is `MessageSemantics`; if an exporter declares that construct supported and its compiler or encoder then fails, the failure is `GenerationFailed` instead.

Output configuration and platform capabilities are not batch features. There is no generic representation, target-option, or custom-string feature.

A future language-neutral plan requirement adds a new closed variant through the public contract compatibility process and conformance fixtures; it does not reuse an existing variant with a changed meaning or bypass the enum.

#### Generation failures

`GenerationFailed` carries exactly one private checked `GenerationFailedEvidence { stage, location }`. `MessageCompilation` means a target message compiler failed after capability preflight had declared the syntax-valid definition representable; its location must be `Definition`.

`PayloadEncoding` means generation or encoding of representation bytes failed and uses the narrowest stable input location: `Batch` for a batch-wide payload such as a loader map, `Plan` for a locale/delivery-unit asset, or `Definition` for independently generated message output.

`MetadataGeneration` means deterministic common metadata generation failed and uses the same narrowest-location rule. A failure in the final shared output constructor is instead `OutputLimitExceeded` or `InvalidOutput`, and a detected impossible exporter state is `InternalInvariant`.

When several generation operations fail, the observable error is the minimum by stage declaration order — `MessageCompilation`, `PayloadEncoding`, then `MetadataGeneration` — followed by canonical location order.

Within one stage, `Batch` precedes `Plan`, and plans use the canonical `(delivery_unit, locale)` order fixed for bundle plans; `Definition` locations follow their plan and then stable definition-record identity order.

Concurrent implementations may collect or coordinate bounded failure candidates internally, but completion, cancellation, or worker scheduling cannot choose a different public error. They return no payload, metadata, candidate artifact, or partial set.

`GenerationFailedEvidence` stores no generated artifact path or kind because no valid candidate artifact is required to exist at this boundary. It also stores no compiler-owned diagnostic object, numeric vector index, payload excerpt, free-form reason, arbitrary error code, source chain, or platform/exporter error.

The invoking exporter context, stage, and stable input location are sufficient for deterministic common presentation.

Exact source-language or target-compiler debugging may be emitted only through an implementation-local debug channel that is not part of `ExportError`, structured output, cache identity, or conformance behavior.

#### Output limit failures

`OutputLimitExceeded` carries exactly one private checked `OutputLimitExceededEvidence { counter, observation }`.

`OutputLimitObservation::Exact(value)` is the exact attempted count produced by checked measurement and is valid only when `value` is strictly greater than the fixed inclusive ceiling returned by `counter.ceiling()`.

It may be more than ceiling plus one when one bounded append, declared collection length, or complete payload length attempts a larger increment.

`ArithmeticOverflow` is used only when a source length cannot convert to `u64` or checked `u64` addition cannot represent the attempted total; implementations never saturate, wrap, clamp, or substitute `u64::MAX` as an exact observation.

The ceiling is a normative property of `OutputLimitCounter` and is derived through the read-only `ceiling() -> u64` API rather than copied into the evidence.

The evidence likewise omits remaining capacity, accepted prefix, caller/configured limit, lower budget, allocation capacity, physical memory use, compressed size, and partial totals. All common output ceilings are fixed and have no lower-budget override, so no request-specific value is needed to interpret the error.

No artifact path, relationship path, plan/definition coordinate, vector index, or optional location is stored.

Limit enforcement can occur before a submitted path or candidate artifact has a valid common identity; attaching a location only in later validation paths would make equivalent failures expose different shapes.

The invoking exporter and the closed counter provide presentation context, while implementation-local tracing may carry non-contract debugging detail.

`OutputLimitCounter` has exactly the following declaration order and inclusive `ceiling()` values:

| `OutputLimitCounter`             |     `ceiling()` |
| -------------------------------- | --------------: |
| `ArtifactRecordsPerSet`          |        `65,536` |
| `ArtifactPathSegmentsPerPath`    |            `64` |
| `ArtifactPathSegmentBytes`       |           `255` |
| `ArtifactPathBytesPerPath`       |         `4,096` |
| `ArtifactPathBytesPerSet`        |    `67,108,864` |
| `ArtifactKindBytes`              |           `255` |
| `MediaTypeComponentBytes`        |           `127` |
| `MediaTypeBytes`                 |           `255` |
| `RelationshipRecordsPerArtifact` |         `4,096` |
| `RelationshipRecordsPerSet`      |        `65,536` |
| `RelationshipTargetBytesPerSet`  |    `67,108,864` |
| `PayloadBytesPerArtifact`        |   `268,435,456` |
| `PayloadBytesPerSet`             | `1,073,741,824` |

Declaration order is also the normative precedence when one candidate result violates more than one counter; the first counter wins regardless of discovery or worker completion order. Within one counter, `ArithmeticOverflow` wins over every `Exact` observation.

Otherwise the greatest exact attempted value wins, so record enumeration and parallel completion cannot select a smaller violation. A validator may stop after establishing that no lower-precedence result can replace the selected evidence, but it cannot expose the first racing failure.

Existing counters are never reordered, removed, or reinterpreted; a future common output ceiling requires a public contract compatibility update, a new closed variant, a fixed insertion/evolution rule, and conformance fixtures.

`ArtifactPathSegmentsPerPath`, `ArtifactPathSegmentBytes`, and `ArtifactPathBytesPerPath` apply to every `ExportArtifactPath`, including each relationship target.

`ArtifactPathBytesPerSet` counts only submitted artifact `logical_path` values, while relationship target occurrences use `RelationshipTargetBytesPerSet`; the two cumulative counters never substitute for or deduct from each other.

`ArtifactKindBytes`, `MediaTypeComponentBytes`, and `MediaTypeBytes` classify only an otherwise countable overlength spelling. An invalid character, delimiter, case, component shape, or other lexical/semantic violation is `InvalidOutput`.

Likewise a malformed or out-of-range format-version representation is `InvalidOutput`, not a resource-limit counter. No unknown, generic-byte, configured-limit, platform-limit, or custom counter is admitted.

#### Invalid output

`InvalidOutput` carries exactly one private checked `InvalidOutputEvidence { violation }`. The closed violation identifies a non-limit common output-contract failure and is sufficient for the presentation layer to select one static human-readable message.

It never stores a raw or display path, invalid character or substring, media/kind spelling, payload or metadata excerpt, relationship endpoint, vector index, source span, free-form reason, arbitrary code, nested error, or exporter/platform detail.

There is deliberately no required or optional location.

Some violations occur before an artifact path, kind, relationship endpoint, or complete candidate record can construct a valid common identity; attaching a location only to later failures would make equivalent structured-adapter, typed-constructor, built-in, and third-party failures expose different shapes.

The invoking exporter context and implementation-local tracing remain available outside the common contract for debugging.

If one candidate has several non-limit contract violations, the checked boundary selects one by the fixed `InvalidOutputViolation` declaration order below, never by input enumeration, validation discovery, hash-map iteration, or worker completion order. It exposes no rejected value and no secondary violation vector.

An implementation may validate in resource-safe stages, but it must produce the same selected violation whenever the same complete candidate can be examined under the fixed limits.

| `InvalidOutputViolation` | Exact meaning |
| --- | --- |
| `ArtifactEnvelope` | The common artifact root has a missing, duplicate, unknown, or wrong-shaped field, including an absent or incorrectly represented payload container. A present nested value with a more specific violation uses that specific variant. |
| `ArtifactPath` | An artifact logical path or relationship target is not the required non-empty segment array, or a segment is empty, exact `.` / `..`, or contains `/`, `\`, `U+0000`, or another `Cc` scalar. Path byte/count excess uses `OutputLimitExceeded`. |
| `DuplicateArtifactPath` | Two submitted artifact records have the same exact validated logical segment sequence. |
| `ArtifactKind` | A present kind violates the canonical namespace/slug grammar for a reason other than its byte ceiling. A syntactically valid unknown kind remains valid common output. |
| `FormatVersion` | A present version has a malformed shape or component outside `u16`; a well-formed but integration-unsupported kind/version pair remains an integration error. |
| `MetadataEnvelope` | The metadata object or one of its required fields is missing, duplicate, unknown, flattened, or wrong-shaped. The canonical `{ media_type: None, relationships: empty }` value is valid. |
| `MediaType` | A present media type violates canonical parameter-free grammar for a reason other than its component or total byte ceiling. `None` and a syntactically valid unregistered name remain valid. |
| `RelationshipEnvelope` | A relationship record has a missing, duplicate, unknown, or wrong-shaped field. A malformed target path uses `ArtifactPath`, and an unknown kind tag uses `RelationshipKind`. |
| `RelationshipKind` | A relationship tag is not the exact supported `EagerLoad` or `LazyLoad` contract value. |
| `RelationshipTarget` | A syntactically valid exact target path does not resolve to exactly one artifact in the same set after artifact-path uniqueness is established. |
| `DuplicateRelationship` | One source artifact submits the same exact `(kind, target)` pair more than once. |
| `ConflictingRelationshipClassification` | One source artifact submits both eager and lazy classifications for the same exact target. |
| `RelationshipSelfEdge` | An eager or lazy relationship resolves from an artifact to that same artifact. |
| `LazyTargetInEagerClosure` | A lazy target is already reachable from its source through a non-empty eager-only path. |

This declaration order is the normative precedence among non-limit candidate violations and existing variants are never reordered or reinterpreted. Common construction canonicalizes valid artifact and relationship order, so unsorted input is not a separate violation.

Valid multi-artifact cycles remain common output unless they violate `LazyTargetInEagerClosure`; a platform that cannot preserve an otherwise valid cycle reports an integration operational error. Destination collision, unsupported valid kind/version, and final registration also remain integration errors.

A shared output ceiling is always `OutputLimitExceeded`, while an impossible state originating inside exporter logic rather than rejected candidate data is `InternalInvariant`.

A future common structural rule requires a public contract compatibility update and conformance fixtures rather than `Other`, `Unknown`, or reuse of an existing meaning.

#### Internal invariants

`InternalInvariant` carries exactly one private checked `InternalInvariantEvidence { invariant }`. It records an impossible state that exporter or shared-constructor code explicitly detects after its public preconditions have been satisfied.

Candidate data that a checked constructor can reject remains `InvalidOutput` or `OutputLimitExceeded`; a supported operation that reports an ordinary compiler or encoder failure remains `GenerationFailed`; and an unsupported valid requirement remains `UnsupportedBatch`.

Third-party implementations use `InternalInvariant` only for the same internal-contract meaning, not as a generic error escape hatch.

The common evidence stores no arbitrary message or code, source/error chain, panic payload, exception object, backtrace, thread ID, source/output location, internal index, rejected value, or implementation state. Presentation chooses one static message from the closed invariant.

Implementation-local logs or tracing may retain debugging details but do not enter structured output, equality, cache identity, or conformance behavior. The error returns no candidate or partial set.

This variant represents only an explicit recoverable detection path. The contract does not require catching a Rust panic, allocation failure, process abort, or foreign exception and converting it into `InternalInvariant`; unwind/exception containment at an FFI or plugin boundary is separate runtime-safety policy.

If several invariants are explicitly detected for one invocation, `InternalInvariantViolation` declaration order selects one independently of discovery or worker order.

| `InternalInvariantViolation` | Exact meaning |
| --- | --- |
| `ValidatedBatchContract` | Exporter code observes a state forbidden by `ValidatedExportBatch`, such as duplicate plan coordinates, a same-plan logical-message collision, unequal snapshots for one equal `DefinitionLocation`, or plan/message order that contradicts the checked canonical order. The batch never re-resolves an external definition artifact, so zero/multiple artifact lookup is not this invariant. It does not reclassify a batch-preparation failure. |
| `CapabilityPreflightContract` | The exporter's internal capability decision or checked feature/location correspondence contradicts itself, including a support result changing without an input change between preflight and generation. An ordinary known unsupported requirement is `UnsupportedBatch`. |
| `GenerationState` | The internal generation state machine takes an impossible transition or produces mutually contradictory stage state. An ordinary reported compiler or encoder failure is `GenerationFailed`. |
| `OutputBudgetAccounting` | Shared bounded-writer or set-budget state underflows, is released or charged twice, or disagrees with its own checked accounting. An actual ceiling excess, failed length conversion, or checked-addition overflow is `OutputLimitExceeded`. |
| `ArtifactAssemblyState` | Generated parts and the exporter's planned artifact records cannot be paired or completed despite individually consistent stage results. Once a candidate record or set exists and violates the common contract, the failure is `InvalidOutput` instead. |
| `SharedConstructorState` | The common checked constructor's own sorting, indexing, graph, or validation state contradicts itself after ordinary candidate validation. Rejecting invalid candidate data is not an internal invariant. |

This declaration order follows the export pipeline and is the normative precedence for simultaneous explicit detections. Existing variants are never reordered or reinterpreted.

There is no generic exporter-bug, adapter-bug, assertion, unreachable, or custom invariant; a newly observable common invariant class requires a public contract compatibility update and conformance fixtures. Implementation-specific assertions may map to one of these variants only when its exact common meaning applies.

`ExportErrorEvidence` is the sole stored discriminated union and maps one-to-one to `ExportErrorKind`: each evidence variant maps to the same-named kind. `ExportError::kind()` derives that mapping on every call, and `ExportError::evidence()` returns only `&ExportErrorEvidence`.

There is no stored duplicate discriminant, optional evidence, empty error, multiple-evidence collection, or state in which a kind and evidence disagree.

#### Error construction and precedence

The public construction surface provides exactly one checked constructor per evidence variant: `unsupported_batch`, `generation_failed`, `output_limit_exceeded`, `invalid_output`, and `internal_invariant`. Each accepts only its matching already-checked evidence type.

There is no public or unsafe generic `new(kind, evidence)`, mutable setter, default constructor, tuple constructor, deserializer that accepts the pair independently, `From<(ExportErrorKind, _)>`, or custom/unknown evidence constructor.

Third-party Rust exporters use those same variant constructors; non-Rust adapters select one corresponding checked operation rather than sending an independently editable kind field.

Adding, removing, or changing a kind requires the matching `ExportErrorEvidence` variant, derived mapping, constructor, presentation, adapter, and conformance fixtures in the same public compatibility change.

An implementation never preserves an unknown evidence payload under a known kind or rewrites one variant into another merely to cross an adapter boundary.

Within one `PlatformExporter::export` invocation, simultaneous explicitly detected error kinds use this fixed precedence, from highest to lowest: `InternalInvariant`, `UnsupportedBatch`, `GenerationFailed`, `OutputLimitExceeded`, then `InvalidOutput`.

`ExportErrorKind::precedence() -> u8` returns `0` through `4` in that order, and the lower value wins.

This value is a comparison contract only: it is not a Rust discriminant, wire tag, serialized field, process exit code, diagnostic severity, or compatibility shortcut, and declaration order of `ExportErrorKind` does not define it.

`InternalInvariant` wins because a detected broken internal guarantee makes ordinary results from the affected invocation unreliable. Capability preflight completes before generation, so `UnsupportedBatch` normally prevents later work.

`GenerationFailed` precedes failures from a candidate that could not be completed. `OutputLimitExceeded` precedes `InvalidOutput` because resource accounting deliberately charges records that later fail semantic validation and may reject before that validation runs.

Each winning kind then applies its own already-defined evidence precedence. Discovery order, validation order chosen for efficiency, hash-map iteration, cancellation, and worker completion never alter the observable selection.

This precedence applies to the one exporter invocation in an M3 export transaction and therefore selects the transaction's one `ExportError`. Separate invocations may reuse the same borrowed batch but remain independent transactions.

Cross-exporter scheduling, atomicity, error aggregation, and presentation are deferred; M3 neither combines their results nor manufactures a synthetic `ExportError` by merging their evidence.

### Export artifact set

#### Set size and generic envelope

One `ExportArtifactSet` has a fixed inclusive hard ceiling of 65,536 submitted artifact records. An empty set and exactly 65,536 records are accepted when otherwise valid; length conversion failure or the 65,537th record rejects the complete set.

Every submitted record is charged once before semantic validation, including a record later rejected for a duplicate logical path or another invalid field. Canonical sorting, duplicate rejection, equal payload bytes, relationship targets, and platform destination mapping never reduce or increase this record count.

Structured adapters preflight a known collection length and otherwise increment the checked counter before retaining each decoded record. Direct typed construction converts and checks the input `Vec` length before sorting, per-record semantic validation, relationship graph work, or cumulative payload accounting.

Built-in exporters collect through the same bounded set builder and never materialize a conforming result above the ceiling.

The ceiling has no caller, configuration, exporter, kind, integration, platform, command-mode, or lower-budget override; exceeding it never drops an artifact, partitions the set, or returns partial output.

`ExportArtifact` is one generic output envelope rather than a built-in-target enum. Its fields have distinct responsibilities:

- `ExportArtifactPath` is a portable relative logical path that identifies the emitted item and proposes its registration name.
- The open, namespaced semantic `ExportArtifactKind` identifies the representation and role.
- The required `ExportArtifactFormatVersion` identifies the contract revision of that kind's payload and metadata.
- The payload is an owned, bounded byte sequence, including UTF-8 generated text.
- Metadata carries only deterministic registration information.

A loader map, manifest, generated module, source file, object, and data blob all use this same record. There is no exporter-defined `Any`, callback, trait object, or typed side channel inside the set.

Checked construction rejects an unbounded payload and any set that is not valid under the shared path, ordering, uniqueness, kind, format-version, metadata, and cumulative-size contracts.

#### Artifact paths

`ExportArtifactPath` is a non-empty ordered `Vec<ExportArtifactPathSegment>`, not a host `Path` / `PathBuf`, absolute pathname, filesystem capability, or command to write a file. A segment is an owned non-empty sequence of Unicode scalar values encoded as UTF-8.

It rejects the exact navigation segments `.` and `..`, `/`, `\`, `U+0000`, and every Unicode General Category `Control` (`Cc`) scalar.

Dots inside any other segment, including an extension such as `en.json`, remain ordinary data.

Construction performs no Unicode normalization, case folding, trimming, percent decoding, separator rewriting, or host-filesystem canonicalization; equality retains the exact decoded scalar sequence and therefore the exact UTF-8 bytes.

Structured adapters and any future wire form preserve the ordered string array — for example `["assets", "messages", "en.json"]` — and never accept a slash-delimited string as the identity form.

Human-readable display may join segments with `/`, but that display is not parsed back into a path and is never accepted as equality, cache, or `--check` input. Canonically equivalent but differently encoded Unicode strings remain distinct logical segments rather than being silently collapsed.

The path is never resolved against the current directory, project root, output directory, or symlink state, and common validation does not test whether it exists.

A build integration maps the segment sequence to an emitted relative file, virtual-module identifier, object/custom-section registration, in-memory asset key, or another platform-native destination. That mapping must be injective over every artifact set it registers.

If distinct exact logical paths collapse under a platform's Unicode normalization, case behavior, escaping, reserved-name handling, or other destination rules, the integration fails the complete registration instead of selecting one, overwriting, or inventing renamed output.

The platform mapping does not alter the original segment sequence used for set equality, duplicate detection, deterministic ordering, cache keys, or `--check`; any mapping failure is an integration operational error and publishes no substitute.

The checked `ExportArtifactSet` constructor canonicalizes caller order rather than requiring exporters to pre-sort. It compares paths segment by segment; each segment uses exact UTF-8 byte lexicographic order, and when every compared segment is equal, the path with fewer segments comes first.

After sorting, two equal segment sequences are a duplicate-path error for the complete set. Input enumeration, exporter construction, parallel completion, hash-map iteration, and platform registration order therefore cannot affect the exposed artifact sequence.

This canonical sequence exists only for equality, fingerprints, cache keys, serialization, reporting, and `--check`; it is not dependency, load, write, or registration order.

Portable relationships are represented explicitly by the checked metadata edges below, and integrations may schedule registration differently while preserving those semantics and fail-complete behavior.

The common path contract has these fixed inclusive hard ceilings:

| Counter                                                           |                     Ceiling |
| ----------------------------------------------------------------- | --------------------------: |
| Decoded UTF-8 bytes in one segment                                |                   255 bytes |
| Segments in one `ExportArtifactPath`                              |                          64 |
| Sum of decoded UTF-8 segment bytes in one path                    |       4 KiB (`4,096` bytes) |
| Decoded segment bytes across all submitted artifact logical paths | 64 MiB (`67,108,864` bytes) |

All four ceilings apply independently. The per-path byte counter charges every segment's exact decoded UTF-8 bytes once within that path.

The set counter charges the same bytes again as resource accounting for every submitted artifact record, including a logical path later rejected as invalid or duplicate. Repeated paths, equal segments, shared prefixes, sorting, duplicate rejection, and interner reuse never reduce that cumulative charge.

Both byte counters exclude segment-array and record framing, presentation `/` separators, allocation capacity, artifact kind, payload, metadata, and relationship targets; target-path occurrences are charged only by the separate relationship-target byte counter below.

Structured adapters preflight available collection and string lengths and otherwise accumulate decoded segment bytes with checked `u64` conversion and addition before retaining an unbounded record collection.

Direct typed construction performs the same accumulation after the artifact-count preflight but before path sorting, duplicate detection, metadata validation, relationship graph work, or cumulative payload accounting.

Zero and exact-boundary values are accepted; conversion or addition overflow, or the first value over any ceiling, rejects the complete `ExportArtifactSet` without truncating a segment, dropping an artifact, partitioning the set, or returning partial output.

These ceilings have no caller, configuration, exporter, artifact, integration, platform, command-mode, or lower-budget override and are separate from `LinkLimits`, syntax-diagnostic retention, metadata, payload, relationship, and set-size budgets.

A platform integration may have a stricter destination limit, but that is an integration mapping failure and does not redefine, shorten, or rename the common logical path.

#### Artifact kinds

`ExportArtifactKind` is an open, namespaced semantic identifier carried as a validated string, not a MIME media type, filename extension, platform destination, Rust enum discriminant, or content-sniffing hint.

It reuses the `ProducerId` lexical contract `<reverse-dns>/<kebab-slug>` and its validator, with a fixed inclusive maximum of 255 decoded ASCII bytes, but remains a distinct semantic newtype with no implicit conversion, shared equality, or interchangeability with `ProducerId`.

The reverse-DNS part has at least two `.`-separated labels. Each label is 1–63 bytes, begins and ends with `[a-z0-9]`, and otherwise contains only `[a-z0-9-]`; an internationalized domain uses its lowercase ASCII IDNA spelling.

The kind slug is one kebab-case component matching `[a-z0-9]+(?:-[a-z0-9]+)*`. Exactly one `/` separates them.

Empty components, uppercase, `_`, whitespace, Unicode, URI schemes, percent encoding, query or fragment text, and additional `/` are invalid rather than normalized.

The exact 255-byte boundary is accepted and the first byte over it is rejected; construction does not trim, case-fold, decode aliases, or perform network or ownership lookup.

M3 initially defines the exact built-in kinds `dev.intlify/esm-module` and `dev.intlify/loader-map`; both initially emit format version `{ major: 0, minor: 1 }`. Later built-in representations allocate additional stable IDs without adding variants to a closed common enum.

A third-party exporter uses a namespace it conventionally controls, such as `com.example/custom-bundle`. The ID denotes representation semantics and role: two JSON payloads may use different kinds for a loader map and manifest, while optional MIME metadata never replaces the semantic kind.

Common construction accepts every syntactically valid kind without a built-in allowlist or inference from path, extension, MIME type, or payload bytes. An integration selected for the transaction declares the kinds it can register and an independent format-version support entry for each kind.

Encountering a valid but unsupported kind is one fail-complete integration operational error; it is not rewritten to a generic blob, content-sniffed, or silently dropped. Kind equality uses the exact validated ASCII bytes.

An exporter release or implementation revision does not mint a new kind merely because its version changes.

#### Artifact format versions

Every artifact carries a mandatory `ExportArtifactFormatVersion { major: u16, minor: u16 }`; patch, prerelease, build, exporter revision, and variable-width/string forms are not part of this field.

It intentionally remains a separate semantic newtype from the reference/definition `ArtifactVersion`, with no implicit conversion or shared compatibility table. The pair `(kind, format_version)` selects the representation contract.

It participates in artifact equality, fingerprints, cache keys, structured output, and `--check`, while path and MIME type never supply or override it.

Format-version compatibility is evaluated separately for each kind. While an output kind has `major == 0`, its support entry names one exact draft pair and an integration accepts only that pair; the initial built-in writer and integration pair is `0.1`, so `0.0`, `0.2`, and every other draft pair are incompatible.

Draft matching is only an admission gate and does not promise compatibility across coordinated exporter, integration, fixture, and document revisions while the repository remains WIP.

For a stable `major >= 1`, a support entry names a supported major and `max_minor`. An integration accepts an artifact exactly when its major equals that supported major and its minor is in `0..=max_minor`.

It interprets each accepted older minor using the defaults defined by that kind/version contract and normalizes it into its current in-memory registration model.

Removing a field or representation, changing an existing meaning or default, tightening previously valid input, or otherwise preventing that normalization requires a major increment. An additive minor may add only elements for which a newer integration has an unambiguous older-version default.

These rules deliberately parallel the artifact-version policy, but `ExportArtifactFormatVersion` retains its own per-kind support entries and tests; it does not reuse the `ArtifactVersion` compatibility table.

Before inspecting or interpreting payload content or metadata, an integration validates every canonical artifact's kind/version pair against the support declared for that kind.

Any unknown kind, unsupported version, malformed version adapter value, or mixed-set incompatibility fails the complete integration before registration; no payload is speculatively parsed, downgraded, version-rewritten, content-sniffed, or partially published.

#### Artifact metadata

`ExportArtifactMetadata` is a closed common typed struct with private fields, checked construction, and read-only accessors. It contains only bounded, deterministic, portable registration facts whose meaning is shared by build integrations without parsing the payload.

It has no arbitrary JSON/object/map, stringly typed key/value entry, namespaced extension bag, `Any`, callback, trait object, or exporter-defined side channel.

Kind-specific representation data remains in `ExportArtifactPayload` under the `(kind, format_version)` contract; platform-only policy remains an input to the selected build integration rather than artifact metadata.

A third-party kind uses those same boundaries and cannot smuggle a private registration protocol through common metadata.

##### Media type

The first common field is `media_type: Option<ExportMediaType>`. `Some` is an explicit checked MIME claim for that artifact under its `(kind, format_version)` contract.

`None` means that the artifact makes no MIME claim; neither the common layer nor an integration may fill it from the logical-path extension, kind ID, payload bytes, content sniffing, or a platform default.

An integration that requires a MIME type for the selected destination rejects a missing or unsupported claim for the complete artifact set before registration. It does not guess, rewrite the metadata, or publish a partial set.

The complete metadata value participates in artifact equality, fingerprints, cache keys, structured output, and `--check`.

`ExportMediaType` stores only the parameter-free MIME essence `type/subtype`. Its grammar is the [RFC 6838 Section 4.2](https://www.rfc-editor.org/rfc/rfc6838.html#section-4.2) registered-name grammar narrowed to one canonical lowercase ASCII spelling:

```text
media-type      = restricted-name "/" restricted-name
restricted-name = name-first *126name-rest
name-first      = %x61-7A / DIGIT
name-rest       = name-first / "!" / "#" / "$" / "&" / "-" /
                  "^" / "_" / "." / "+"
```

Each type and subtype is therefore 1–127 ASCII bytes, and the complete value is 3–255 bytes including the one required `/`. Exact component and total ceilings are accepted; the first byte over either component ceiling rejects construction without truncation.

A structured suffix such as `+json`, vendor and personal tree spelling, and a syntactically conforming unregistered name are ordinary accepted names. Common validation performs no IANA registry, suffix-registry, alias, ownership, or network lookup.

Uppercase, Unicode, empty components, a missing or additional `/`, wildcard `*`, whitespace, comments, percent encoding, URI-like text, and every character outside the grammar are invalid rather than normalized.

The semicolon and all [RFC 9110 Section 8.3.1](https://www.rfc-editor.org/rfc/rfc9110.html#section-8.3.1) media-type parameters, including `charset`, are outside this initial field.

Equality compares the exact validated ASCII bytes; aliases and case-insensitive MIME comparison are not applied because noncanonical spellings never construct a value.

If parameters become necessary, they are added as separate bounded typed metadata with an unambiguous absent default and the affected per-kind minor-version update. The only other initial common metadata field is `relationships`, defined below; payload limits remain a separate decision.

##### Relationships

The second common field is a checked, bounded `relationships: Vec<ExportArtifactRelationship>`. Each record contains a typed `ExportArtifactRelationshipKind` and one exact `ExportArtifactPath` target.

The target must resolve to exactly one artifact in the same completed `ExportArtifactSet`; it is never an external package, runtime module, URL, host path, virtual-module spelling, or platform destination.

Resolution compares the original logical segment arrays, never slash-joined display, extension inference, filesystem state, or the integration's mapped destination.

The checked set constructor validates targets after artifact-path uniqueness is established, canonicalizes relationship order by the contract kind tag and then the exact target-path order, and rejects a duplicate `(kind, target)` pair rather than silently deduplicating it.

An empty relationship vector is valid and is the older-format default. Missing or ambiguous targets, invalid relation values, noncanonical input that cannot be checked, and checked-count overflow reject the complete set without dropping an edge or artifact.

Relationship order itself has no dependency, scheduling, load, write, or registration semantics. Relationships participate in artifact equality, fingerprints, cache keys, structured output, and `--check`.

The initial portable kind set is closed to `EagerLoad` with contract tag `0`, followed by `LazyLoad` with contract tag `1`. The tags define validation, canonical ordering, fingerprints, cache keys, and structured-adapter discriminants; they do not declare the Rust ABI or memory representation.

Every edge is directed from the artifact whose metadata contains it to `target`.

`EagerLoad` means the target belongs to the source artifact's initial load closure and no deferred load boundary may be required between making the source usable and making the target available.

`LazyLoad` means the target is excluded from that initial closure and remains addressable through the source's deferred load path. Neither kind prescribes ESM static/dynamic import syntax, a filesystem write sequence, or platform registration order.

An integration that cannot preserve the declared load classification rejects the complete set before registration instead of promoting, demoting, dropping, or rewriting the edge.

For one source artifact and one exact logical target path, the load classification is single-valued. Therefore `EagerLoad` and `LazyLoad` edges to the same target are mutually exclusive even though their `(kind, target)` pairs differ.

Checked set construction rejects that conflict for the complete set without choosing precedence, promoting lazy to eager, demoting eager to lazy, merging the records, or retaining only the first/last edge.

Separate source artifacts may each point to the same target, and one source may point to multiple distinct targets; those cases do not conflict. Conflict identity uses the original source and target logical paths, not mapped platform destinations.

A relationship whose target path exactly equals its source artifact path is invalid for both initial kinds. An `EagerLoad` self-edge is a non-informative recursive assertion, while a `LazyLoad` self-edge contradicts the requirement that the source itself is available before its deferred path can be used.

Checked construction detects self-edges after exact target resolution and before graph-cycle analysis, and rejects the complete set without deleting the edge, reducing it to a warning, or treating it as an empty relationship.

Destination mapping, case behavior, and filesystem aliases neither create nor excuse a common-layer self-edge.

Multi-artifact cycles are valid common graph shapes rather than construction errors. The constructor forms the directed subgraph containing only `EagerLoad` edges, computes its strongly connected components, and computes reachability over the resulting condensation DAG.

Every eager SCC belongs to one inseparable initial load closure, without requiring a topological registration order inside the component. Cycles containing one or more `LazyLoad` edges are also accepted when every lazy edge preserves its meaning.

Graph identity and traversal use exact logical paths and canonical edge order, not caller order or mapped destinations.

For every `LazyLoad(source, target)`, `target` must not already be reachable from `source` through a non-empty path of only `EagerLoad` edges.

Such eager reachability would place the target in the source's initial closure while the lazy edge simultaneously claims it is excluded, so checked construction rejects the complete set.

This transitive rule covers an indirect eager path, an eager SCC, and a lazy edge between two members of the same eager SCC; it does not reject a pure lazy cycle or a mixed cycle whose individual lazy edges all target artifacts outside their respective source eager closures.

Self-edges remain rejected separately before this analysis.

The common layer does not require every platform to implement cyclic loading. After common graph validation and before any registration, a selected integration preflights the complete relationship graph against its capabilities.

An integration that cannot preserve a valid eager, lazy, or mixed cycle returns one fail-complete operational error without breaking the cycle, duplicating an artifact, changing an edge kind, or partially registering an acyclic subset.

###### Relationship resource limits

One artifact may submit at most 4,096 relationship records across `EagerLoad` and `LazyLoad` combined. This is a fixed inclusive hard ceiling: zero and exactly 4,096 records are accepted when otherwise valid, and the 4,097th rejects the complete `ExportArtifactSet`.

Counting uses the input vector length before sorting, duplicate/conflict detection, target resolution, self-edge rejection, or graph analysis, so invalid or repeated records cannot reduce the charged count.

Structured adapters preflight the declared count before allocating or decoding individual relationship records; direct typed construction checks the supplied vector length with checked conversion before graph work.

The complete set may submit at most 65,536 relationship records across all artifacts and both kinds. The constructor converts each raw input-vector length to `u64` and accumulates it with checked addition after the per-artifact length check but before relationship sorting, graph allocation, or semantic validation.

Zero and exactly 65,536 cumulative records are accepted when otherwise valid; the 65,537th, integer-conversion failure, or addition overflow rejects the complete set. Charging every submitted occurrence prevents many low-fan-out artifacts, duplicate edges, or later-invalid records from bypassing the cumulative bound.

Where structured framing exposes nested counts, adapters validate the per-artifact count and checked cumulative declared count before allocating or decoding the corresponding records.

The relationship-metadata ceilings are:

| Counter                                                       |                     Ceiling |
| ------------------------------------------------------------- | --------------------------: |
| Submitted relationship records in one artifact                |                       4,096 |
| Submitted relationship records in the entire set              |                      65,536 |
| Decoded target-segment bytes across all submitted occurrences | 64 MiB (`67,108,864` bytes) |

For the byte ceiling, every submitted relationship occurrence contributes the exact decoded UTF-8 byte length of every segment in its target `ExportArtifactPath`. The counter excludes the source artifact path, kind tag, segment-array/object framing, presentation `/` separators, allocation capacity, and graph indices.

Repeated or equal target paths are charged again for every occurrence; string interning, target resolution, duplicate/conflict rejection, and shared prefixes never reduce the charge. No Unicode normalization, case folding, destination mapping, or serialized escape spelling changes the counted decoded bytes.

Adapters accumulate target bytes with checked `u64` conversion and addition as segments are decoded, after count preflight but before retaining an unbounded relationship collection or allocating graph state.

Direct typed construction traverses the submitted targets and performs the same checked accumulation before sorting or semantic graph validation. Zero and exactly 64 MiB (`67,108,864` bytes) are accepted when otherwise valid; conversion/addition overflow or the first byte above the ceiling rejects the complete set.

All three ceilings have no caller, configuration, exporter, kind, integration, platform, command-mode, or lower-budget override and apply independently. Exceeding any ceiling never truncates a path or vector, splits an artifact or set, drops lazy edges, applies interning deductions, or returns partial output.

For the M3 built-in loader map, the loader-map artifact is the source of one edge to each emitted locale ESM artifact. A locale listed in `eagerLocales` receives `EagerLoad`; every other supported locale receives `LazyLoad`.

Locale assets need no reverse edge merely because the loader map names them. Runtime-specific loading mechanisms remain runtime/integration policy, but they must preserve this exporter-selected initial-versus-deferred boundary.

Only portable relationships that a common build integration can interpret belong in this vector. External imports and platform-specific dependencies remain in the versioned payload or selected integration inputs; exporters cannot encode them as invented paths or private relationship variants.

Adding another common kind requires an affected per-kind format minor and an unambiguous older-format default; implementations never accept an unknown tag under `0.1`. These count and byte ceilings complete the initial relationship resource contract.

##### Metadata evolution

For output format `0.1`, `ExportArtifactMetadata` has exactly the two fields shown in the API: `media_type` and `relationships`. The metadata object itself is mandatory on every artifact; `{ media_type: None, relationships: Vec::new() }` is its valid canonical empty semantic value rather than an absent metadata object.

Checked construction and structured adapters reject missing required fields, unknown or duplicate fields, flattened alternatives, and any attempt to preserve an extra value for round-trip output.

The initial contract deliberately has no integrity hash, source-map pointer, file permission or executable bit, compression/content-encoding field, locale, delivery-unit ID, or platform registration option. Payload bytes can be hashed by cache/build owners without making a supplied digest authoritative.

A source map, if required, is a separate artifact plus a future typed relationship kind. Permissions, compression, and registration options are platform integration inputs; locale and delivery placement remain in plans or kind-specific versioned representations.

None is encoded into an arbitrary metadata extension. A concrete cross-platform consumer can justify a later bounded typed field or relationship kind through the format-version evolution rule below.

Adding another common metadata field requires an unambiguous default for every older compatible format and the corresponding per-kind minor-version and fixture updates.

Removing a field, changing its meaning or default, or making an optional field required is breaking for every affected kind and requires its format major to advance. All envelope members are concrete shared types rather than target-specific associated types.

#### Artifact payloads

`ExportArtifactPayload` is an owned immutable byte sequence stored by the concrete private newtype `Box<[u8]>` and exposed only as a read-only `&[u8]`. Every artifact contains exactly one payload; an empty box is valid and is distinct from an absent payload, which the envelope cannot represent.

Construction takes ownership of the final bytes before the checked `ExportArtifactSet` is exposed, so the payload does not borrow exporter scratch memory or retain a mutable `Vec`, file/mmap handle, reader, stream, callback, trait object, `Any`, or deferred producer closure.

Text and binary outputs use the same byte type. The `(kind, format_version)` contract alone defines whether those bytes are UTF-8 source, JSON, an object, a snapshot, a compressed representation, or another format.

The common envelope performs no UTF-8 validation, decoding or encoding, BOM handling, newline conversion, Unicode normalization, transcoding, compression/decompression, content sniffing, media-type inference, or semantic parsing.

Thus an arbitrary sequence containing any byte value from `0x00` through `0xff` is structurally representable; a selected integration interprets it only after the complete kind/version preflight described above.

Payload equality is exact byte-for-byte equality, including empty content, BOM-like prefixes, line endings, and bytes that are invalid UTF-8. The exact bytes participate in artifact fingerprints, cache keys, lossless structured output, and `--check`; allocation capacity and the exporter's source buffer do not.

A structured adapter must preserve the complete byte sequence through its eventual versioned byte representation rather than substitute a text string or lossy escape form.

##### Payload resource limits

One `ExportArtifactPayload` has a fixed inclusive hard ceiling of 256 MiB (`268,435,456` bytes).

Empty and exact-boundary payloads are accepted; conversion failure or the first byte over the ceiling rejects the complete `ExportArtifactSet` without truncating bytes, splitting the artifact, changing its representation, or returning a partial set.

Checked construction converts the exact `Box<[u8]>::len()` to `u64` and compares that stored length; allocation capacity, textual character count, encoded/decoded expansion, compression ratio, and kind-specific meaning neither add to nor reduce this counter.

The ceiling has no caller, configuration, exporter, kind, integration, platform, command-mode, or lower-budget override.

Across one complete `ExportArtifactSet`, the cumulative exact stored payload length has a fixed inclusive hard ceiling of 1 GiB (`1,073,741,824` bytes). Every artifact contributes its exact payload length once, including artifacts whose payload bytes are equal; an empty payload contributes zero.

Checked construction converts every length to `u64` and uses checked addition. Allocation capacity, deduplication, interning, shared prefixes, compression ratio, encoded or decoded expansion, artifact kind, and relationship structure neither add to nor reduce the charge.

An empty set and an exact-boundary total are accepted; length conversion failure, addition overflow, or the first byte above the ceiling rejects the complete set without dropping an artifact, truncating or splitting a payload, changing a representation, or returning a partial set.

This ceiling has the same no-override rule as the per-artifact ceiling.

Built-in exporters must produce each payload through a shared bounded byte writer and place all writers for one export invocation under one set-scoped cumulative budget.

Before every reserve, append, formatting write, encoder emission, or final conversion to `Box<[u8]>`, a writer uses checked `u64` arithmetic against both the 256 MiB per-artifact ceiling and the shared 1 GiB set ceiling and refuses the operation that would cross either one.

Concurrent generation may coordinate that shared budget internally, but worker scheduling cannot change whether the exact final byte total is accepted.

A refusal returns one `ExportError` with no `ExportArtifactSet`; it never constructs an over-limit final set and never retries through truncation or a different representation. Non-Rust adapters use the same bounded buffering and set-scoped accounting rules.

The checked result constructor revalidates every final box length and their cumulative sum so a custom or third-party `PlatformExporter` cannot bypass either contract, although the common type does not pretend to prevent arbitrary out-of-contract code from allocating its own temporary memory.

### Exporter registration boundary

Built-in exporters keep any raw-plan helper private, and exporter registries accept only `PlatformExporter` implementations.

A third-party Rust exporter implements that trait and uses the same concrete result boundary; non-Rust bindings expose a validate-and-export orchestration operation or an opaque batch handle, never the batch constructor or a serialized capability.

Code may define an unrelated raw-plan function, but it is outside the conforming exporter contract and cannot be registered as a `PlatformExporter`. This boundary prevents accidental bypass in supported integrations without pretending that a Rust type can prohibit arbitrary out-of-contract code.

### Initial ESM exporter behavior

#### M3 v0.1 representation boundary

The initial `dev.intlify/esm-module` format `0.1` is a data-only ESM representation of exact validated MF2 source. Each locale module carries the selected records' scope, catalog-key domain, canonical message key, and exact decoded `MessagePayload`; it does not compile, normalize, format, or otherwise rewrite the MF2 text.

The shared export-preparation gate has already parsed and semantically validated every selected definition before the ESM exporter runs. That proof makes the raw payload deployable source data but does not turn the ESM module into an MF2 runtime: it exports no formatter function, locale negotiation, function registry, ICU data provider, retry policy, cache, suspense primitive, or application provider.

An application/runtime adapter loads the module and passes its exact source records to the selected MF2 runtime. Runtime formatter semantics remain outside 014 and cannot be inferred from this data-only ABI.

M3 `0.1` never emits message-compiler-generated JavaScript functions or a 003 Binary AST snapshot. `GenerationStage::MessageCompilation` remains available to another exporter or future representation but is unreachable for an ordinary built-in `0.1` ESM transaction. Introducing compiled functions, a snapshot, or another executable/runtime-coupled payload requires an explicit new artifact kind or coordinated format-version decision with its runtime and integration contract; it cannot silently replace the raw records under the existing pair.

#### Linker-resolved fallback materialization

One locale module represents one requested production locale from its `MessageBundlePlan`. It contains every exact `ResolvedMessage` selected by the linker for that requested locale, including a definition selected from a configured fallback locale. The exporter never repeats fallback search or repartitions a selected definition back into its definition locale's asset.

The module-level `locale` is the requested locale. Each message record additionally carries the exact `definitionLocale` from the selected `MessageDefinition`, so provenance is not lost when, for example, the `fr` plan materializes an `en` definition. `definitionLocale` is evidence about the selected source record, not an instruction to perform another message-key lookup or to load a second locale asset.

Every production locale receives one locale artifact even when its resolved plan is empty. Equal fallback-selected source text may therefore occur in several requested-locale artifacts and is charged independently; the exporter does not share one record through a runtime indirection, omit an empty locale asset, or replace requested-locale plans with source-locale buckets.

The loader map exposes the exact configured fallback table for integration consistency and locale negotiation, but `loadLocale(locale)` loads only that exact requested production-locale asset. Neither the generated loader nor the locale module performs message-key fallback. A runtime adapter consumes the already materialized selection and cannot treat the published fallback table as permission to override the plan.

#### Built-in plan cardinality

For the built-in single-node graph, successful plan construction emits exactly one `MessageBundlePlan` for every production locale, all with delivery unit `["main"]`, in canonical locale UTF-8 byte order. A plan remains present when its resolved message vector is empty.

Because `ProductionLocales` is non-empty, a successful built-in outcome therefore cannot contain `Some(Vec::new())`. That generic result remains valid for another checked graph or in-process integration, but it is unreachable through the M0-owned graph and M3 built-in ESM route.

The ESM exporter requires the exact duplicate-free production-locale plan set captured by its private options. A missing, repeated, or extra locale, or any unit other than exact `["main"]`, is `InternalInvariantViolation::CapabilityPreflightContract` for the built-in orchestration; it never synthesizes a missing plan from configuration or drops an unexpected one. A future custom integration that invokes an ESM exporter explicitly designed for M3 against a valid multi-unit batch receives `UnsupportedBatchFeature::DeliveryUnitPartitioning` rather than implicit flattening.

Consequently, one successful M3 ESM transaction returns exactly `production_locales.len() + 1` artifacts: one locale module per production locale plus one loader map. With the 1,024-locale configuration ceiling this is at most 1,025 artifacts, and the loader map carries exactly `production_locales.len()` relationships. Empty message sets do not alter either count.

#### Locale module ABI

Each `dev.intlify/esm-module` `0.1` payload has exactly four named exports and no default export:

```js
export const formatVersion = [0, 1]
export const deliveryUnit = ['main']
export const locale = 'fr'

export const messages = [[['project', 'app'], 'json-pointer', '/checkout/title', 'en', 'Hello']]
```

Their exact meanings are:

1. `formatVersion` is the two-element numeric tuple `[0, 1]` matching the artifact envelope's kind-specific format version.
2. `deliveryUnit` is the exact segment array from the plan; M3 therefore emits `["main"]`.
3. `locale` is the plan's exact requested `Locale`.
4. `messages` is an array of message tuples.

Every message tuple has exactly five positional values in this order:

```text
[
  [scopeNamespace, scopeName],
  catalogKeyDomain,
  canonicalKey,
  definitionLocale,
  exactMf2Source
]
```

M3 writes scope namespace token `"project"` explicitly even though it is the only admitted namespace. It writes the checked domain token, canonical key, selected definition locale, and exact decoded `MessagePayload` without path conversion, normalization, formatting, or source re-extraction.

Records are ordered by exact resolved-scope identity, domain contract order, and canonical-key byte order, exactly matching the plan's logical-message order. A valid plan cannot contain two records with the same three identity components for one requested locale; detecting such a duplicate during generation is `InternalInvariantViolation::ValidatedBatchContract`, not a first/last-wins rule.

The module omits source-document identity, `EntryReference`, host spans, definition-artifact provenance, fallback-chain position, and reporter data because they are not required by the runtime data ABI. Those values remain available to export preparation and diagnostics but do not enlarge shipped message records.

The exported arrays and tuples are read-only by ABI contract but are ordinary JavaScript literals. M3 emits no `Object.freeze`, proxy, class, map, initialization helper, or defensive clone. A consumer that mutates imported nested data violates the runtime adapter contract; the exporter does not add executable mutation policing to a data-only module.

#### Locale asset logical paths

M3 has the fixed built-in `["main"]` delivery unit and emits one locale ESM artifact at logical path:

```text
["locales", "locale-<digest>.mjs"]
```

`<digest>` is the 64-character lowercase hexadecimal spelling of standard unkeyed BLAKE3-256 over this exact framed byte sequence:

```text
"dev.intlify/esm-locale-path" || 0x00
|| 0x0000_u16be || 0x0001_u16be
|| locale_utf8_length_u32be
|| exact_locale_utf8_bytes
```

The two `u16` values are the `dev.intlify/esm-module` format major and minor. The locale length is checked even though the shared locale ceiling is 255 bytes. The domain terminator, fixed-width version and length make the preimage unambiguous and reserve coordinated path evolution.

The resulting filename segment is always 75 lowercase ASCII bytes, including `locale-` and `.mjs`, and therefore fits the portable segment limit independently of locale spelling. Locale bytes are never placed directly in a path, percent-encoded, normalized, case-folded, truncated, or replaced with a declaration ordinal.

The loader map retains each exact `Locale` as its source-level key and maps it to this logical artifact. The filename is not a reversible locale representation and no consumer parses it back into a locale.

Before artifact-set construction, the exporter compares every generated digest/path association. If two unequal exact locale identities produce one equal path, the complete transaction fails through `InvalidOutputViolation::DuplicateArtifactPath`; it never selects one locale, adds an ordinal or suffix, lengthens the digest ad hoc, or relies on relationship order. Equal locale identities cannot reach this check twice because the checked production set is duplicate-free.

#### Loader map ABI

The one M3 loader-map artifact has logical path `["loader.mjs"]`, kind `dev.intlify/loader-map`, and format version `0.1`. Its payload is an ESM module with no default export.

Static imports appear first in canonical eager-locale order. Aliases are assigned densely within that ordered eager subset as `eager0`, `eager1`, and so on:

```js
import * as eager0 from './locales/locale-<digest>.mjs'
```

The module then exports:

```js
export const formatVersion = [0, 1]
export const locales = ['en', 'fr', 'ja']
export const fallbacks = [['fr', ['en']]]

export function loadLocale(locale) {
  switch (locale) {
    case 'en':
      return Promise.resolve(eager0)
    case 'fr':
      return import('./locales/locale-<digest>.mjs')
    case 'ja':
      return import('./locales/locale-<digest>.mjs')
    default:
      return Promise.reject(new RangeError('unsupported locale'))
  }
}
```

`locales` contains every exact production locale in canonical UTF-8 byte order. `fallbacks` contains only sources with an explicitly configured non-empty chain, ordered by source locale; each nested target array preserves configured priority. Tuple arrays avoid using opaque locale identities as JavaScript object properties.

The switch contains one case for every production locale in the same canonical order. An eager case returns `Promise.resolve()` of its statically imported module namespace. A lazy case returns the direct dynamic `import()` promise for the exact relative locale-asset path. Thus every successful branch has one uniform promise-shaped result without wrapping or re-exporting the locale module.

The default branch never throws synchronously, coerces an input, normalizes a locale, consults fallback policy, or embeds the submitted value in an error. It returns `Promise.reject(new RangeError("unsupported locale"))` with that exact fixed message.

`loadLocale` performs exact JavaScript string equality against the opaque configured locale spellings and loads only the matching requested-locale artifact. The exported `fallbacks` value supports configuration alignment and locale negotiation outside message-key selection; the generated function never walks it.

The loader-map metadata contains exactly one relationship to every emitted locale artifact. A statically imported eager locale uses `EagerLoad`; a dynamically imported locale uses `LazyLoad`. Locale artifacts have no reverse relationship merely because their paths appear in the loader payload.

#### Canonical ESM source bytes

Both built-in ESM kinds emit ECMAScript 2020 module source as valid UTF-8 without a BOM. The canonical writer is part of the `0.1` payload contract and is not Oxfmt, a generic pretty-printer, or a host JavaScript serializer.

Every physical line ends with LF, no CR is emitted, and the payload ends with exactly one LF after its final statement or closing brace. Import declarations, `export const` declarations, and `return` statements end in semicolons. Function declarations do not receive a trailing semicolon. Indentation is exactly two ASCII spaces per block level, with one message tuple, fallback tuple, import, or switch case/return pair in canonical semantic order.

The locale module has no blank line among its first three scalar exports and exactly one blank line before `messages`. The loader module has exactly one blank line after a non-empty static-import block, no blank line when that block is empty, no blank lines among the three data exports, and exactly one blank line before `loadLocale`. No trailing comma appears in any array, tuple, import, or switch construct; an empty collection uses `[]` on the declaration line.

Every string literal uses double quotes. The writer escapes exact scalar values as follows:

- `"` and `\` become `\"` and `\\`.
- backspace, tab, LF, form feed, and CR become `\b`, `\t`, `\n`, `\f`, and `\r`.
- every other scalar in `U+0000..U+001F` becomes lowercase `\u00xx`.
- `U+2028` and `U+2029` become exact `\u2028` and `\u2029`.
- every other Unicode scalar is emitted directly as UTF-8.

It does not escape `/`, `<`, `>`, `&`, or `'`, HTML-protect text, ASCII-escape non-ASCII scalars, normalize Unicode, or rewrite MF2 source contents. Unpaired surrogates cannot reach the writer because every input is already a Unicode scalar sequence.

The writer emits no comments, timestamp, source path, generator banner, source map, source-map URL, environment-dependent line, or random data. It does not pass generated bytes through Oxfmt or another formatter. Any coordinated change to these observable `0.1` bytes follows the output format-version and producer-revision rules rather than being treated as insignificant presentation.

#### ESM artifact metadata

Every locale artifact has kind `dev.intlify/esm-module`, format version `0.1`, media type `Some("text/javascript")`, and an empty relationship vector. The one loader artifact has kind `dev.intlify/loader-map`, format version `0.1`, the same exact media type, and the one relationship per locale fixed above.

Neither artifact emits a MIME parameter or `charset`. No source map, integrity digest, locale, delivery-unit ID, file permission, compression setting, or platform destination is added to common metadata. The payload-level locale and delivery-unit exports do not become duplicate metadata fields.

The selected integration preflights support for both exact kind/version pairs and exact `text/javascript` before interpreting payloads or registering any item. It never infers media type from `.mjs`, fills a missing value, or rewrites the claim to `application/javascript`.

For the built-in exporter, the canonical payload's static/dynamic import classification and the loader metadata's eager/lazy relationships must agree exactly for every locale path. A missing, extra, reversed, or differently classified relationship, a non-empty locale-module relationship, a payload/envelope format-version mismatch, or a non-`text/javascript` built-in claim is `InternalInvariantViolation::ArtifactAssemblyState`; it is not repaired during checked set construction or registration.

The ESM exporter additionally takes `eagerLocales`: the locales whose assets the entry delivery unit imports eagerly; every other supported locale loads lazily through the map — the default that answers problem 5.

The loader-map relationship metadata records the same eager/lazy classification realized by the generated static and dynamic imports. The M3 loader contract stays limited to the exported policy data plus `loadLocale(locale)` for fixed unit `["main"]`; application caching, retries, suspense, prefetch timing, and locale negotiation remain runtime/integration concerns.

M3 currently selects ESM as the first exporter milestone; that product ordering does not make ESM part of the linker core contract.

## Component Boundaries

```text
intlify_contract
  └─ artifact types, selector/domain contracts, wire format, conformance fixtures

intlify_resource
  └─ catalog definitions, canonical domain-qualified keys, messages, source spans (013)

host-owned definition projection (initially an intlify_cli project-inventory module)
  └─ combines one complete extraction with its resolved catalog assignment,
     validates and groups physical aliases, and constructs MessageDefinitionArtifact

intlify_cli project inventory
  └─ enumerates configured definition/reference inputs, groups physical aliases,
     invokes the shared definition projection, records execution outcomes,
     and derives scope completeness

language reference producers
  ├─ intlify_producer_js   (oxc-based JS/TS + Vue SFC frontend)
  └─ intlify_producer_bin  (tagged-ID scanner for native/WASM; later)

intlify_linker
  ├─ domain-dispatched selector matching, reference resolution
  ├─ locale/fallback resolution
  ├─ reachability over the delivery graph, placement
  ├─ bundle plans + findings
  └─ stateless link(LinkRequest) -> LinkOutcome

intlify_lint integration (later; not an M0 dependency)
  └─ maps linker findings to rules (008 + catalog-level addendum)

intlify_export
  ├─ shared parser + SemanticModel validation gate
  ├─ opaque ValidatedExportBatch tied to exact plans + definitions
  ├─ PlatformExporter accepts only &ValidatedExportBatch
  ├─ common ExportArtifactSet { Vec<ExportArtifact> } / ExportError result contract
  └─ built-in ESM exporter

platform build integrations
  └─ orchestrate producers, graph, linker, exporter construction/selection,
     invocation, destination mapping, and output registration

platform exporters
  └─ convert the batch's message-validated plans into ordered generic byte artifacts

LSP/editor integration (009)
  └─ incremental findings from available source-level reference info
```

013's deferred catalog-level and cross-locale checks, unused-translation reporting, and application-source reference analysis all layer above the one shared catalog extraction path; no second resource extractor is introduced.

Linker M0 brings forward 013's explicit `scope` plus `path` and `fixed` locale-binding configuration as a coordinated prerequisite and projects their resolved values into mandatory `MessageDefinition.scope` and `MessageDefinition.locale`.

The resource/configuration integration owns binding resolution; `intlify_linker` only consumes the resulting definition metadata.

## Configuration

### Configuration ownership

This is an additive section under the 006 unified config contract. `messages` is the normative section name for linker policy, reference producers, and delivery targets; `catalog` is not accepted as an alias. `resources.catalogs` remains the separately owned resource-source selection section from 013.

The 006 loader owns JSON/JSONC syntax, duplicate-member and known-root-field admission, config discovery, schema composition, config-path attachment, the command-independent cross-section validation order, and the final CLI error envelope. It validates and compiles `resources` before passing the raw optional `messages` section, `ResolvedResources`, and project-root context to the 014-owned validator. This document owns only validation within `messages`: all of its fields, cross-section scope references, immutable policy/producer/target construction, section-local ordering, and path-independent violation evidence. The generated 014 schema fragment is composed as `definitions.messages` in the one published 006 schema. Omission remains distinct from a present section and enables no linker consumer implicitly.

Configuration admission is milestone-gated. A release accepts exactly the fields whose owning milestone is implemented and included in that release; a field shown for a later milestone is an unknown field until that milestone lands. The validator never accepts a dormant placeholder, silently ignores a recognized future field, or stores an unimplemented value for later activation. Schema generation, strict object validation, resolved Rust types, and fixtures add each field atomically with its owning behavior.

The composite example below shows the shape after M3 and is not evidence that every member is accepted at M0. The milestone-specific contracts below identify when each concern becomes admissible. Within an admitted milestone, the project-inventory path/pattern, recognizer, policy, and placement leaf contracts in this section are normative. The enclosing shape mirrors the linker's three configured concerns — link policy, reference producers, and delivery targets — rather than the pre-linker "one CLI scanner" shape.

Scope completeness is deliberately absent because integrations derive it from resolved inventory and execution results rather than trusting a user-authored closed-world claim.

Scope mapping is likewise absent from the M0 built-in config. Later CLI/editor surfaces using the M0-owned orchestration pass `ScopeMappingTable::empty()`; the typed linker input remains available to custom in-process integrations, and a public config spelling is deferred to package-resource composition.

### Message-section validation result

The 014-owned section validator returns only the first `config_validation_failed` violation under one fixed section-local order. It never aggregates independent violations, selects by object/map iteration, races validators, or exposes worker-completion order. Sequential, cached, and parallel implementations must return the same stable `details.reason`, narrowest applicable JSON Pointer, and bounded reason-specific evidence.

The 006 loader still owns all earlier syntax and duplicate-object-member failures, root known-field admission, and the global `fmt` → `lint` → `resources` → `messages` section order. A duplicate member therefore remains `config_parse_failed` and never reaches this order. Once 013 succeeds and invokes 014, one earlier 014 validation failure prevents discovery, artifact production, policy construction, linking, exporting, checking, or mutation; no partially resolved messages configuration is returned.

Within 014, validation is append-only by owning milestone:

1. When present, `messages` must be an object.
2. Unknown `messages` members under the active milestone are checked by exact decoded member name in ascending ASCII order.
3. M0 validates `locales` to completion.
4. M0 validates `dynamicReferences` to completion when present.
5. M0 validates `roots` to completion when present.
6. M0 validates `producers` to completion when present.
7. M1 appends complete `coverageBaseline` validation when present.
8. M2 appends complete `fallback` validation when present.
9. M3 appends complete `delivery` validation when present.

Each field's shape, submitted count, scalar values, duplicates, canonical construction, and field-owned cross-value checks finish before the next field begins. A later field may refer only to already checked dependencies, such as production-locale membership or the 013 scope registry; it cannot retroactively outrank the earlier field.

When a later milestone makes a member known, that release removes its earlier `unknown_field` result and validates the member only at the newly appended step. It does not insert the new step into a semantic grouping or change the relative precedence of any two pre-existing admitted fields. JSON source member order, schema-property order, Rust field declaration order, hash-map order, cache state, and worker scheduling are non-semantic.

Stable `details.reason` values are grouped by the invalid field or contract layer, while `details.pointer` identifies the narrowest applicable object, member, or array occurrence. A field-family reason may therefore cover its missing value, wrong JSON shape, forbidden empty form, invalid scalar, limit violation, membership failure, or duplicate; those conditions do not each create another public reason.

A relation spanning multiple fields, scopes, or targets receives a dedicated relation-level reason and points to the narrowest common owning object. Existing 013-owned catalog assignment/domain failures and the post-extraction `scope_key_domain_mismatch` remain separate because they cross the resource/linker boundary at later stages. The validator never collapses every failure into one generic messages-config reason, and it never exposes an unbounded rejected value merely to distinguish conditions within one reason family.

The closed 014 configuration reason vocabulary is:

| First admitted | `details.reason` | Owned contract |
| --- | --- | --- |
| M0 | `invalid_messages_section_shape` | A present `/messages` value is not an object. |
| M0 | `unknown_field` | A member is unknown under the active milestone; this reuses the 006 spelling and exact-field evidence. |
| M0 | `invalid_message_locales` | Required `locales` field shape, count, scalar, duplicate, and production-set construction. |
| M0 | `invalid_message_dynamic_references` | Optional `dynamicReferences` token and defaultable scalar contract. |
| M0 | `invalid_message_roots` | Optional `roots` collection, entries, selectors, identities, and scope/domain references. |
| M0 | `invalid_message_producers` | Optional `producers`, built-in JS configuration, recognizers, and external artifact declarations. |
| M1 | `invalid_message_coverage_baseline` | Optional non-empty `coverageBaseline` mapping and its scope/locale references. |
| M2 | `invalid_message_fallback` | Optional fallback occurrence mapping, chains, membership, and canonical construction. |
| M3 | `invalid_message_delivery` | Optional `delivery`, target count, target-local identity/exporter/output/eager-locale fields, and canonical construction. |
| M3 | `delivery_output_conflict` | Equal or ancestor/descendant `out` roots across configured targets. |
| M0 post-extraction | `scope_key_domain_mismatch` | One recognizer or configured-root domain differs from the successfully observed domain for its scope. |

For a field-family reason, a missing or collection-wide violation points to the field, while an entry-local violation points to its exact array occurrence or object member. `delivery_output_conflict` points to `/messages/delivery/targets`, the narrowest common owner of both targets. The existing 013-owned `catalog_locale_not_production` and `catalog_scope_key_domain_conflict` reasons retain their resource-owned stages and evidence; 014 does not alias them into this table.

#### Configuration violation evidence

`details.value` is present only for a rejected JSON string, number, or boolean whose exact decoded or canonical scalar representation fits the owning field's already-fixed per-value byte ceiling. When that field has no smaller scalar ceiling, the config-evidence ceiling is 255 UTF-8 bytes. Missing values, `null`, arrays, objects, and a scalar rejected for exceeding that ceiling omit `value`; evidence never truncates, hashes, normalizes, or substitutes a display spelling.

A count or byte limit failure instead includes exact non-negative JSON integers `limit` and `observed`. `observed` is the submitted collection length or decoded UTF-8 byte length that first exceeds the applicable inclusive limit, not a saturated value, remaining budget, retained count, or `limit + 1` unless that is the actual observation.

A semantic duplicate points `details.pointer` to the later submitted occurrence and includes `details.firstPointer` for the earlier equal checked occurrence. It does not copy the repeated locale, path, selector, target, or other raw value. Config syntax duplicate object members remain the earlier 006-owned parse error and never use this shape.

`delivery_output_conflict` contains only `firstTarget`, `secondTarget`, `firstOutPointer`, and `secondOutPointer` in addition to the common reason and pointer. Target values have already passed the 255-byte checked-name contract, and the two pointers identify `/out` members without copying either path. `scope_key_domain_mismatch` retains its separately fixed checked scope/domain evidence. The 006-owned `unknown_field` and 013-owned failures retain their existing evidence rather than being reshaped here.

```jsonc
{
  "resources": {
    "catalogs": [
      {
        "scope": "app",
        "include": ["locales/*.json"],
        "locale": { "from": "path", "pattern": "locales/{locale}.json" }
      }
    ]
  },
  "lint": {
    "rules": {
      "ambiguous-message-definition": "error",
      "unresolved-message": "error",
      "unused-message": "warn" /* later L0/L1 adapters expose linker findings as rules */
    }
  },
  "messages": {
    // link policy: the closed world
    "locales": ["en", "ja", "ja-JP"],
    "fallback": { "ja-JP": ["ja", "en"], "ja": ["en"] }, // M2+
    "coverageBaseline": { "app": "en" }, // M1+ optional; non-empty when present
    "dynamicReferences": "strict", // M0 optional; strict | compat; default compat
    "roots": [
      // M0 optional; default []
      {
        "scope": "app",
        "domain": "json-pointer",
        "selector": { "kind": "exact", "key": "/legal/notice" },
        "reason": "rendered server-side"
      }
    ],
    // reference producers: who supplies MessageReferenceArtifacts
    "producers": {
      // M0 optional; default {}
      "js": {
        "include": ["src/**/*.ts", "src/**/*.tsx", "src/**/*.vue"],
        "recognizers": {
          "t": {
            "kind": "lookup",
            "scope": "app",
            "domain": "json-pointer",
            "keySyntax": "dot-path"
          },
          "i18n.t": {
            "kind": "lookup",
            "scope": "app",
            "domain": "json-pointer",
            "keySyntax": "dot-path"
          },
          "useMessageSet": {
            "kind": "set",
            "scope": "app",
            "domain": "json-pointer",
            "keySyntax": "dot-path"
          }
        }
      },
      "artifacts": [
        "services/mailer/intlify-references.json" /* externally produced, composed at link */
      ]
    },
    // delivery: exporters consuming bundle plans
    "delivery": {
      // M3+ optional in config; required by emit
      "targets": [
        {
          "name": "web",
          "exporter": "esm",
          "placement": "duplicate",
          "eagerLocales": ["en"],
          "out": "src/generated/messages"
        }
      ]
    }
  }
}
```

This M3 configuration example contains only the initial supported ESM exporter. Binary blobs, baked Rust, generated C/C++, and other outputs in the architecture examples describe future platform exporters or additional artifacts returned by one such exporter; `"blob"` is not an M3 built-in exporter id.

### Locale policy and M2 fallback policy

Whenever the `messages` section is present, `locales` is a required non-empty array and resolves to the one M0 production-locale set. Its accepted count is 1 through 1,024 entries inclusive. There is no omitted default, empty analysis-only mode, catalog-derived inference, or command-specific exception. A project that does not enable the linker omits the complete `messages` section rather than supplying it without `locales`.

M0 and M1 reject `fallback` as an unknown field and their resolved `LinkPolicy` contains no fallback member. M2 adds the field, its occurrence-preserving normalized representation, two fallback-specific counters, validation, cache identity, and fallback-aware analysis atomically.

Beginning with M2, the production-locale set bounds both emitted locales and every locale that may participate in fallback resolution. A `fallback` member may be present only for a locale in that set, and every locale in its ordered array must also be in the set.

Omitting a member means no fallback for that source locale. Configuration resolution rejects an out-of-set source or target before constructing `LinkPolicy`; it never ignores one or admits it conditionally because a matching catalog definition exists.

The same exact set bounds linker-participating catalog definitions. After both sections pass their local structural validators, the cross-section validator rejects a linkable fixed catalog locale outside `messages.locales` even when its patterns match no file. A path-captured locale is checked after concrete catalog assignment and before extraction. Both use the 013-owned `catalog_locale_not_production` evidence; out-of-set definitions are never silently filtered into an analysis-only side channel or admitted to the M0 union. Entry-level-only resource definitions remain outside this rule.

At M0, collection limits count submitted `locales` and `roots` before duplicate or semantic validation. Beginning with M2, they additionally count fallback-source members and each fallback target array.

Beginning with M2, a JSON/JSONC configuration decoder rejects and charges duplicate `fallback` object members before any ordinary map could overwrite them; another adapter must preserve the same occurrence information through its bounded construction boundary.

Every locale spelling in `locales` and, beginning with M2, every fallback source member or fallback target is decoded directly into the shared opaque `Locale` identity and must contain 1 through 255 decoded UTF-8 bytes. Configuration does not trim, canonicalize, validate BCP 47, or rewrite the value before equality and membership checks.

M0/M1 use the 1,024-occurrence and 261,120-byte production-only derived maxima above. Beginning with M2, the 67,584-occurrence and 17,233,920-byte production-plus-fallback maxima apply. Neither stage introduces a separate aggregate locale-byte setting.

Every milestone rejects an equal checked locale repeated in `locales`. Beginning with M2, the resolver also rejects a repeated fallback source member, an explicit empty fallback array, a source locale repeated in its own target array, and a target repeated within one array; it never selects, deduplicates, or normalizes one occurrence to omission.

Beginning with M2, each fallback array is already the complete sequence used after its source locale and is not recursively expanded through another member's configuration. Reciprocal arrays are consequently finite and valid.

Roots are duplicate when their checked `(scope, domain, selector)` tuples are equal, regardless of `reason`, and duplicate roots are rejected rather than merged.

Checked M0 policy construction canonically sorts locale-set values and roots by their semantic identity. Beginning with M2, it also sorts fallback entries by source locale while preserving each fallback target array's exact declared priority order.

### Configured root policy

`roots` is an optional M0 field. Omission and an explicit empty array both resolve to the same empty configured-root set and therefore produce the same immutable `LinkPolicy`, semantic fingerprint, cache identity, findings, and plans. The resolver never infers roots from catalog definitions, producer configuration, or the absence of reference artifacts.

Configured roots are exceptional reachability declarations for messages intentionally consumed outside the available reference-producer world, such as a server-driven key. Ordinary code-derived reachability continues to come from `MessageReferenceArtifact`; projects with no such exception do not need a placeholder root. This preserves meaningful `unused-message` findings and pruning instead of making every definition implicitly reachable.

### Dynamic reference policy

`dynamicReferences` is an optional M0 field with the exact accepted tokens `"compat"` and `"strict"`. Omission resolves to `compat`; explicit `"compat"` produces the same immutable `LinkPolicy`, semantic fingerprint, cache identity, findings, and plans as omission. `null`, booleans, numbers, arrays, objects, case variants, whitespace-padded strings, and every other token are invalid rather than being coerced or treated as omission.

`compat` is the safe adoption default: one `UnboundedDynamic` record remains visible as the non-blocking finding fixed above and conservatively retains every definition in its exact scope-domain pair. `strict` is an explicit project opt-in that makes the same finding blocking and withholds bundle plans. Neither mode changes artifact production, and a producer never reads this field.

### Coverage baseline

`coverageBaseline` is an optional M1 exact mapping from declared scope name to one locale in `messages.locales`. When present, it must contain at least one scope member; an explicit empty object is a configuration error rather than a second spelling of omission. It has no project-wide default and no inference rule.

Omitting a scope disables coverage-baseline reporting and typed-key generation for that scope without changing reference resolution or ordinary bundle emission; requesting typed-key generation for an omitted scope is a configuration error rather than a request to guess.

An explicit entry whose scope is unknown, whose locale is outside `messages.locales`, or whose selected scope has no definition-closed inventory for that locale is rejected before generation.

M1 adds this field together with its occurrence-preserving bounded configuration representation, count/byte ceilings, admission precedence, schema, and exact error contract; it cannot borrow the configured-root pass or silently collapse duplicate object members.

### Reference producers and roots

`producers` is an optional M0 field. Omission and an explicit empty object both resolve to the same empty configured-producer set, project inventory, cache identity, completeness result, findings, and plans. They never trigger an implicit source scan or artifact search.

When no producer is configured, the reference inventory is the intentionally empty closed world: every enabled producer has completed vacuously because there are none. Target scopes therefore receive reference-side `Closed`, not `Partial(ProducerOmitted)`. Subject to definition-side closure, definitions reached by neither an artifact nor a configured root remain eligible for `unused-message`; an explicit prune request may select them for deletion.

`ProducerOmitted` is reserved for a producer that exists in the resolved inventory but was not executed or included in the current integration request. It never represents absence of producer configuration.

`producers.js` configures the built-in CLI source-scan producer and is itself optional. When present, it requires both a non-empty `include` array and a non-empty `recognizers` object. An empty array, an empty object, or omission of either member is a configuration error; a project that does not use the built-in producer omits `js` itself.

Each recognizer binding requires exact `kind`, `scope`, `domain`, and `keySyntax` members and follows the M0 canonicalization contract above. The exact `kind` tokens are `"lookup"` and `"set"`. There is no default recognizer, call kind, include pattern, scope, domain, or key syntax, so names such as `t`, `i18n.t`, and `useMessageSet` in the example have no effect unless explicitly declared.

Recognizer object keys use the static callee-chain contract in the JS/TS producer section. They are not arbitrary expressions, regular expressions, suffixes, or runtime property paths.

The scope must be declared by at least one linkable resource definition. The pre-discovery pass validates the explicit domain token and key-syntax compatibility; when successful definition extraction observes one domain for that scope, the later binding pass requires the declared domain to match it under the contract above.

`producers.artifacts` is optional and composes externally produced artifacts at link time. When present, it must be a non-empty array; an empty array is a configuration error rather than a second spelling of omission. A project with no external artifact declaration omits this member.

Bundler integrations and native binary scanners supply their artifacts through their build integrations (plugin options, build-script invocation, scan inputs per `emit` invocation), not through this file. `roots` are optional config-declared reachability exceptions in artifact vocabulary (scope + domain + selector + reason); omission does not add any implicit reference.

They accept only intentional `Exact`, `Prefix`, `Pattern`, or `AllInScope` selectors; `UnboundedDynamic` is producer evidence and is invalid as a configured root. Scope names in recognizers, roots, and `coverageBaseline` resolve through the same project registry and are never created by first use.

Domain IDs use the exact `CatalogKeyDomain` tokens above, and selector objects use the fixed artifact selector envelope without a bare-string shortcut.

### Delivery configuration

`delivery` is an optional M3 field so link analysis, lint adaptation, typed-key generation, and prune analysis do not require an exporter destination. When present, it requires a `targets` array containing 1 through 64 submitted targets inclusive; an empty object, an omitted `targets` member, and an explicit empty array are configuration errors rather than no-op delivery plans.

The M3 configuration validator preflights the submitted target count before target-name validation, duplicate detection, exporter lookup, option validation, inventory work, or allocation proportional to target contents. The 65th submitted target rejects the complete delivery configuration; invalid and duplicate targets receive no count deduction. The validator never truncates, partitions, or executes a valid prefix.

`intlify messages emit` requires a resolved non-empty delivery configuration. If the field is absent, the command fails before project inventory, linking, export preparation, or output comparison; it never treats the request as a successful zero-target emission. `--target` can select an existing configured target but cannot supply or synthesize a missing delivery configuration.

Omitting `--target` selects every configured target. Supplying `--target <name>` selects exactly one target by exact checked-name equality; there is no implicit or configured default target. Equal checked target names reject the complete delivery configuration rather than using first-wins, last-wins, or merging their exporter options.

Each target `name` is a machine-facing identifier with the exact grammar `[a-z0-9][a-z0-9._-]{0,254}`. Its accepted length is therefore 1 through 255 ASCII bytes inclusive. Uppercase letters, non-ASCII text, whitespace, control characters, an empty value, a leading punctuation character, and the first byte beyond the ceiling are invalid. Validation performs no trimming, case conversion, Unicode normalization, aliasing, or filesystem-name rewriting before identity comparison.

The `--target` operand passes the same grammar and length check before exact identity lookup. A spelling that differs in case or punctuation never selects a near match. The target name is not a display label or an output path; an exporter destination remains the separately validated `out` field.

Target declaration order is non-semantic. After complete target validation and duplicate rejection, resolved delivery construction sorts targets by exact checked-name ASCII bytes and carries each target's exporter configuration with it. The same canonical order drives all-target selection, fingerprints, cache identity, result DTOs, reports, and `--check`; sequential and parallel execution reassemble results in that order rather than completion order.

After canonical target construction, the configuration validator compares every `out` as its exact portable segment sequence. Two equal roots, or a pair in which either sequence is a proper segment-prefix of the other, reject the complete delivery configuration. The validator never merges ownership, relies on generated artifact paths being disjoint, or gives one target cleanup or overwrite priority over another.

When several pairs conflict, validation enumerates `(firstTarget, secondTarget)` pairs lexicographically in checked target-name order with the first name strictly less than the second and returns the first conflicting pair. The evidence names follow that order, while `firstOutPointer` and `secondOutPointer` retain each target's original submitted array index. Configuration-array order, path length, prefix direction, and parallel discovery do not select the winner.

This cross-target rule applies before `--target` selection, so selecting one target cannot make an otherwise invalid shared or nested output topology valid. Distinct exact portable roots that later collide under a host filesystem's case, normalization, reserved-name, or other mapping rules remain a target-registration failure under the platform mapping contract; exact configuration validation does not guess those host rules.

Every M3 configured target requires exactly one string-valued `exporter` member, and the only accepted built-in ID is the exact token `"esm"`. Omission, `null`, non-string values, case variants, whitespace-padded values, aliases, and every other token are configuration errors. There is no default and no inference from `name`, `out`, a file extension, the host platform, or another target.

The resolved target therefore selects exactly one exporter and one export transaction. A custom in-process integration may select another registered exporter through its checked API, but an M3 CLI configuration cannot smuggle an unknown plugin ID through the built-in schema. A later milestone that exposes another configured exporter adds its exact ID, options contract, registry admission, and fixtures atomically.

Every M3 configured target also requires exactly one string-valued `out` member. It has no default and is never inferred from the target name, exporter ID, project layout, current directory, or another target. The checked value is the CLI integration's registration root for the exporter's portable artifact paths; it is not passed to the exporter as authority to perform filesystem I/O.

The string is a portable project-root-relative path. It is split only on `/` and resolved lexically beneath the 006-owned resolved `projectRoot`. The validator rejects an empty string, leading or trailing `/`, repeated `/`, empty segments, exact `.` or `..` segments, POSIX absolute paths, Windows drive-prefixed or UNC forms, and every `\`. It never uses the process current directory, the declaring config file's directory, or host-dependent separator rewriting as the base.

After splitting, `out` reuses the portable `ExportArtifactPathSegment` scalar contract: each segment contains 1 through 255 decoded UTF-8 bytes, the path contains at most 64 segments, and the sum of decoded segment bytes is at most 4,096. It accepts Unicode scalar values but rejects `U+0000` and every Unicode General Category `Control` (`Cc`) scalar. It performs no trimming, case folding, Unicode normalization, percent decoding, or host-name canonicalization; exact segment bytes define configuration identity.

Filesystem registration does not follow a directory symlink or Windows reparse point below the trusted 006 project root. Before registration, the CLI establishes the output root by checking each existing ancestor without following it; write mode creates each missing directory as a real directory, and check mode creates nothing. An existing destination leaf that is a symlink or reparse point is likewise rejected rather than overwritten through its target.

Every portable artifact path is then registered relative to that verified output root under the same no-follow rule. A platform that cannot establish the required identity and no-follow behavior fails the complete target registration instead of falling back to path-text containment or unrestricted host resolution. Exporters remain unaware of symlinks and receive no filesystem capability.

#### Filesystem output ownership

The M3 CLI treats each configured `out` as a dedicated generated-output root. It does not mix linker output with caller-owned files merely because all generated artifact paths currently appear disjoint.

The CLI integration owns the reserved root-relative control path `[".intlify-output-manifest.json"]`. The manifest is not an `ExportArtifact`, is not visible to the exporter, and does not participate in artifact-set count, path, metadata, relationship, or payload limits. An exporter artifact that maps to that exact path is a complete `destination_collision` / `reserved_control_path` failure before any registration mutation.

##### Manifest schema

The manifest has its own integration contract and version. It is not an accidental serialization of Rust registration structs. The initial exact semantic shape is:

```json
{
  "schemaVersion": {
    "major": 0,
    "minor": 1
  },
  "exporter": "esm",
  "artifacts": [
    {
      "path": ["loader.mjs"],
      "kind": "dev.intlify/loader-map",
      "formatVersion": {
        "major": 0,
        "minor": 1
      },
      "metadata": {
        "mediaType": "text/javascript",
        "relationships": []
      },
      "payload": {
        "bytes": 1234,
        "fingerprint": {
          "algorithm": "blake3-256",
          "digest": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        }
      }
    }
  ]
}
```

The root contains exactly the required `schemaVersion`, `exporter`, and `artifacts` members. `schemaVersion` reuses the exact unsigned `u16` `{ major, minor }` value shape, but remains a manifest-schema version rather than an artifact or payload format version. While its major is `0`, M3 accepts only exact `0.1`.

`exporter` is the selected checked exporter ID. For M3 it is exact `"esm"`; a manifest with another syntactically valid value does not own an `"esm"` target merely because its artifact records appear compatible.

`artifacts` preserves the checked `ExportArtifactSet` canonical order. Every element contains exactly:

1. `path`, using the portable segment-array codec;
2. `kind`, using the checked `ExportArtifactKind` spelling;
3. `formatVersion`, using the exact unsigned `u16` `{ major, minor }` shape;
4. `metadata`;
5. `payload`.

`metadata` contains exactly `mediaType` followed by `relationships`. `mediaType` is either the checked MIME essence string or explicit JSON `null`; absence is not a second spelling of `None`. `relationships` contains the complete canonical common relationship vector. Every element contains exact `kind` followed by `target`; the initial kind tokens are `"eager_load"` and `"lazy_load"`, and `target` uses the same portable segment-array codec.

`payload` contains exact non-negative integer `bytes` followed by `fingerprint`. `bytes` is the registered payload's exact byte length and must satisfy the common payload ceiling. `fingerprint` reuses the exact `{ algorithm, digest }` object shape: M3 supports only `"blake3-256"` and exactly 64 lowercase hexadecimal characters.

The payload fingerprint is standard unkeyed BLAKE3-256 over the complete exact payload byte sequence, with no domain prefix, text decoding, normalization, or host metadata. Equal fingerprints do not replace byte equality, and a manifest is never content-addressed authority for a missing or unreadable payload.

Every object rejects a missing, duplicate, unknown, mistyped, or disallowed `null` member. Every integer uses the existing unsigned plain-decimal JSON lexical rules. Artifact paths, kinds, versions, media types, relationship vectors and targets, payload lengths, ordering, uniqueness, and graph consistency are reconstructed through their ordinary checked types rather than trusted because they came from a manifest.

##### Canonical manifest encoding

Input object member order and insignificant JSON whitespace are non-semantic after successful strict decoding. The integration's canonical writer emits the fixed member orders above as UTF-8 without a BOM, with:

- two ASCII spaces per nesting level;
- each member or array element of a non-empty object or array on its own line;
- one ASCII space after `:`;
- a comma after every non-final member or element;
- exact `[]` for an empty array;
- LF-only line endings and exactly one final LF; and
- no trailing spaces, comments, trailing commas, timestamp, absolute path, environment value, or random data.

Canonical strings escape `"` and `\` as `\"` and `\\`; use `\b`, `\t`, `\n`, `\f`, and `\r` for those five control scalars; encode every other `U+0000` through `U+001F` scalar as lowercase `\u00xx`; and preserve every other Unicode scalar literally, including `/`, `U+2028`, and `U+2029`. Canonical non-negative integers use the shortest base-10 spelling, with exact `0` for zero and no leading zero, sign, fraction, or exponent.

A semantically valid supported manifest with different member order, escaping, or whitespace can still establish ownership after checked decoding. Write mode rewrites it canonically, while `--check` classifies its byte difference as safely writable staleness.

##### Manifest resource limit

The complete manifest input and canonical output each have a fixed inclusive 16 MiB (`16,777,216` byte) UTF-8 wire ceiling. The first byte above it, length conversion failure, invalid UTF-8, or a BOM rejects the manifest before proportional parsing or ownership is inferred.

This bound is independent of artifact payload bytes, which the manifest does not embed. The decoder additionally enforces all submitted artifact, path, metadata, and relationship count and decoded-byte ceilings from `ExportArtifactSet`; manifest framing or pretty-print whitespace provides no deduction from those ordinary counters.

The M3 built-in ESM route remains bounded more tightly by at most 1,024 locale artifacts, one loader artifact, and one loader relationship per locale. The 16 MiB wire ceiling is not a way to admit a plan or artifact count that the built-in route otherwise forbids.

Write mode may adopt a root only when it does not yet exist or is an existing empty real directory. A non-empty root without a valid supported manifest is unowned and fails the target with `message_output_registration_failed` / `registration_failed`; the CLI neither inventories it as generated output nor deletes or overwrites any entry.

Once adopted, a valid manifest identifies the prior generated files. The only admitted entries below the root are:

- the manifest itself;
- one regular file for every logical path recorded by that manifest; and
- the exact real ancestor directories required by those files.

Any other file, directory, symlink, reparse point, or special entry is an unowned collision. Registration fails without deleting, moving, renaming, or overwriting that entry. An empty directory is admitted only when it is an exact required ancestor; an unrelated empty directory is not silently removed.

A manifest-recorded file may be missing or have different bytes because generated output is stale or damaged. Write mode may repair or replace that owned file. A path recorded by the previous manifest but absent from the new artifact set is a stale owned output and is removed only as part of a successful complete transaction. A valid manifest therefore permits deterministic cleanup without granting authority over entries it did not record.

Malformed, over-limit, unsupported-version, or non-regular manifest input establishes no ownership. It fails before mutation rather than being ignored, repaired heuristically, or treated as permission to replace the root. The manifest path and all recorded paths remain subject to the same verified-root-relative no-follow checks before any prior record is trusted for cleanup.

#### Filesystem registration transaction

One selected filesystem target is one registration transaction. The integration first completes destination mapping, kind/version and relationship capability preflight, manifest construction, and all ownership checks without mutating the output root.

##### Target control identity and lock

Every filesystem target derives one 32-byte output-root control ID with standard unkeyed BLAKE3-256 over:

```text
"dev.intlify/output-root-control" || 0x00
|| 0x0000_u16be || 0x0001_u16be
|| segment_count_u16be
|| for each project-root-relative out segment:
     segment_utf8_length_u16be
     || exact_segment_utf8_bytes
```

The two version values are the control-protocol major and minor. The count and lengths are checked even though the output-root contract already limits them to 64 segments and 255 bytes per segment. Exact segment bytes are used without slash joining, normalization, case folding, or host canonicalization.

The ID's 64-character lowercase hexadecimal spelling is `<output-root-id>` in the persistent real sibling lock filename:

```text
.intlify-output-<output-root-id>.lock
```

The lock file is integration-owned control state outside `out`, not an exporter artifact or output-manifest entry. Its complete canonical JSON contains exactly `schemaVersion` followed by `out`; the version is exact `{ "major": 0, "minor": 1 }`, and `out` is the complete project-root-relative portable segment array. It uses the manifest canonical JSON writer and has a fixed inclusive 64 KiB (`65,536` byte) wire ceiling.

On first write, the integration creates the regular lock file without replacement, acquires its exclusive OS advisory lock before initializing its checked content, writes and durably flushes the canonical record, and retains the file after transaction completion. Later writers open the existing no-follow regular file, acquire its exclusive lock, and only then validate its content and output-root identity.

A second writer that races first creation opens the winning path and waits for that same exclusive lock rather than creating another authority. A different decoded `out` under the same digest is a destination collision. Invalid, over-limit, incomplete, unsupported, non-regular, symlinked, or reparse lock state is `registration_failed` and is never deleted or reinitialized by assumption.

`--check` opens an existing lock read-only, acquires a shared advisory lock for its complete filesystem snapshot, and validates the same record without changing its contents. If the lock is absent at the initial no-follow observation, check mode creates nothing, records absence, and requires it to remain absent at the final observation. Because a first writer creates and retains the lock before modifying registration state, appearance of the lock during that interval is a concurrent-state operational failure rather than a stale-output difference.

The platform must provide process-releasing shared/exclusive advisory locks whose identity remains attached to the opened no-follow regular file. PID files, elapsed-time leases, clock-based stale detection, and an in-process mutex are not conforming substitutes. A platform that cannot provide this boundary returns `unsupported_capability` before output mutation.

##### Transaction journal

After lock acquisition and any prior recovery, write mode generates one cryptographically random 128-bit transaction ID, encoded as exactly 32 lowercase hexadecimal characters. Failure to obtain the required randomness is `unsupported_capability`; a timestamp, PID, counter, weak PRNG, or overwrite retry is not substituted.

The transaction uses these exact real sibling basenames under the verified output parent:

```text
.intlify-output-<output-root-id>.staging-<transaction-id>
.intlify-output-<output-root-id>.backup-<transaction-id>
.intlify-output-<output-root-id>.transaction.json
```

Staging and backup are newly created or renamed real directories. The one journal path is a no-follow regular file created without replacing an unrecognized entry. These basenames are recorded and compared as basenames; `/`, `\`, navigation, alternate case, Unicode aliases, and external paths are not accepted.

The journal is canonical JSON with exact `0.1` schema and these root members in order:

1. `schemaVersion`;
2. `transactionId`;
3. `out`;
4. `staging`;
5. `backup`;
6. `oldRoot`;
7. `newManifestFingerprint`;
8. `phase`.

`schemaVersion`, `out`, and fingerprint values reuse the checked shapes above. `transactionId`, `staging`, and `backup` must reproduce the names derived from the output-root ID and transaction ID.

`oldRoot` is one closed object:

- `{ "kind": "absent" }` when `out` did not exist;
- `{ "kind": "empty" }` for an adopted empty real root; or
- `{ "kind": "managed", "manifestFingerprint": ... }` for a previously managed root.

The managed fingerprint and `newManifestFingerprint` are standard unkeyed BLAKE3-256 over the complete exact canonical old or new manifest bytes. They use the same `{ algorithm, digest }` shape and exact `"blake3-256"` token as payload fingerprints.

`phase` is exactly `"staged"`, `"old_moved"`, or `"new_installed"`. The journal uses the manifest canonical JSON writer and has a fixed inclusive 64 KiB (`65,536` byte) wire ceiling. A journal with invalid UTF-8, BOM, excess bytes, unsupported version, unknown or duplicate members, invalid derived names, or another phase is not recovery authority.

##### Commit protocol

After lock, recovery, and ownership validation, write mode performs the same exact manifest, metadata, path-set, and actual payload-byte comparison used by check mode. If the existing managed root is already equal to the complete expected output, the target succeeds as `unchanged` without creating staging, backup, journal, or another output-root mutation. A semantically valid but noncanonical manifest, stale recorded path, changed payload, or any other safe difference proceeds to a real transaction rather than being called unchanged.

Otherwise write mode constructs the complete staging root under the same verified parent and filesystem. It writes every expected artifact plus the canonical new manifest under the same no-follow rules. Nothing inside the current output root is modified while staging is incomplete.

After all staging files and directories are durably flushed, commit proceeds:

1. create and durably publish the journal at phase `staged`;
2. move the current owned root, when present, to the journal's backup path and durably publish that directory-entry change;
3. durably replace the journal with phase `old_moved`;
4. move the complete staging root to the configured `out` and durably publish that directory-entry change;
5. durably replace the journal with phase `new_installed`;
6. remove the backup, when present, and durably publish its removal; and
7. remove the journal and durably publish its removal.

If an ordinary commit step fails, the integration restores the prior owned root before returning the target-local `message_output_registration_failed` / `registration_failed` error. It never falls back to direct file-by-file overwrite, publishes the successfully written prefix, or reports a successful partial artifact set.

Journal replacement itself writes a complete same-parent temporary regular file, durably flushes it, atomically replaces the recognized journal entry, and durably flushes the parent. A partially written file never becomes the supported journal path. An unrecognized existing journal or derived staging/backup collision fails without replacement.

##### Recovery state machine

A process or machine interruption may leave integration-owned staging, backup, or journal entries. The next write invocation acquires the exclusive target lock and validates the complete journal before ordinary root ownership admission.

Recovery compares the actual safe root states and exact manifest fingerprints recorded by the journal rather than trusting `phase` alone:

- If the old root still occupies `out` and staging is the complete recorded new root, recovery removes staging and the journal and retains the old root.
- If `out` is absent, backup is the complete recorded old root, and staging is the complete recorded new root, recovery restores backup to `out`, removes staging and the journal, and retains the old root.
- If `out` is the complete recorded new root and backup is the complete recorded old root, recovery retains the new root and removes backup, any remaining staging entry, and the journal.
- If `out` is the complete recorded new root and no backup is required or remains, recovery retains the new root and removes any remaining staging entry and journal.
- If a journal marked `new_installed` has no valid new root but still has the complete recorded old backup, recovery restores the old root and removes the incomplete transaction state.

These observations cover an interruption between a rename and the following journal phase update: an exact new root at `out` is recognized even when the durable phase still says `old_moved`, while an absent `out` with an exact old backup is rolled back.

Any other combination — including a digest mismatch, an unexpected entry, two candidate new roots, a missing required old backup, an invalid derived name, or an unrecognized special file — is ambiguous or externally modified state. Recovery returns `registration_failed` without deleting, moving, overwriting, or relabeling any participant. A similarly named sibling without the one valid journal is never guessed to be stale integration output.

##### Durability and publication guarantee

Before each state transition, the implementation durably flushes the affected regular-file contents and every required directory entry in dependency order. A successful flush call is required for payload files, manifests, journal replacements, staging directories, the output parent after rename/removal, and the restored root during rollback or recovery.

If the platform or selected filesystem cannot establish same-filesystem sibling placement, cryptographically random transaction IDs, durable file and directory flushes, safe rename and rollback behavior, target-writer serialization, or deterministic recovery, the complete target fails as `unsupported_capability`; it does not weaken the transaction contract or silently downgrade to process-crash-only recovery.

The successful-return guarantee is that `out` contains exactly one complete new manifest and artifact set. M3 does not claim that arbitrary external processes reading the directory during the short backup-and-rename commit window observe an instantaneous cross-platform directory swap. Build integrations consume the target only after successful registration; stronger concurrent-reader publication requires a future versioned indirection protocol.

#### Check-mode comparison

`intlify messages emit --check` performs the same complete inventory, link, export preparation, exporter invocation, checked artifact-set construction, destination mapping, and capability preflight as write mode. It then compares the expected target with the existing filesystem state without creating or updating the output root, manifest, lock, staging root, backup, or transaction state.

The comparison covers:

- the exact canonical artifact logical-path set;
- each artifact's kind, format version, media type, and relationships;
- every payload byte;
- the manifest's canonical representation; and
- every prior manifest-recorded stale artifact.

The CLI reads and compares the actual regular-file bytes. It never accepts a manifest payload digest as proof that an unread, missing, or different file matches. Modification time, owner, and ordinary permission bits are not output metadata and do not affect equality; inability to establish or read the required safe regular-file snapshot remains an operational failure.

An absent root, an empty adoptable root, a missing recorded artifact, different artifact or manifest bytes, noncanonical but otherwise valid supported manifest bytes, or a prior manifest-recorded stale artifact means that a safe write would change owned output. That target reports a check difference and contributes exit code `1`; it is not an operational error.

A malformed or unsupported manifest, a symlink or reparse point, an unreadable required entry, a platform capability failure, or any entry not owned by a valid manifest cannot be repaired safely under the ownership contract. It produces target-local `message_output_registration_failed` and contributes operational exit code `2`, not a check difference.

A target contributes exit code `0` only when its manifest and complete actual artifact set match exactly. Across selected targets, any operational failure makes the command exit `2`; otherwise any difference makes it exit `1`; only complete equality for every selected target exits `0`. Canonical target order, worker completion, and reporter selection do not change this precedence.

An in-process build integration that registers virtual modules, asset keys, object sections, or another non-filesystem destination supplies that destination through its own integration context rather than omitting `out` from this CLI configuration shape. Exporter output remains the same portable `ExportArtifactSet` in either case.

Other consumers have no delivery work to select, while continuing to use the same policy, definition inventory, reference inventory, completeness, and linker analysis. M0 through M2 reject the raw `delivery` field under the milestone-gated admission rule rather than retaining it for M3.

### M3 ESM target options

`eagerLocales` is optional for the M3 `"esm"` exporter and defaults to the empty locale set. Omission and an explicit empty array produce the same resolved target, fingerprint, cache identity, artifact set, relationships, and `--check` result. The resolver never infers an eager locale from production-locale declaration order, fallback chains, coverage baselines, host locale, environment variables, or another target.

The submitted array contains 0 through 1,024 locale occurrences inclusive. Count preflight occurs before locale scalar validation, production-set membership, or duplicate detection; invalid and duplicate occurrences receive no deduction, and the 1,025th rejects the complete target without truncation.

Every occurrence uses the shared exact `Locale` identity and 1-through-255 decoded UTF-8 byte contract. It must be a member of `messages.locales`; an out-of-set value and an equal duplicate are configuration errors rather than being ignored or used to expand the production universe. After validation, resolved target construction sorts the set by exact locale UTF-8 bytes. Submitted order never expresses import, fallback, or runtime priority.

An empty eager set does not omit locale assets: the exporter still emits every selected production-locale asset and its loader-map entry, with every loader-map relationship marked `LazyLoad`. Each explicitly eager locale changes only its corresponding relationship to `EagerLoad` under the fixed exporter contract.

After this validation, the M3 orchestration moves the canonical checked eager set together with the exact resolved production locales and fallback table into the private typed ESM factory described above. No later exporter step re-reads raw configuration, environment state, or another target to reconstruct those options.

### Delivery placement

For every target supplied to the M0 core contract and every configured delivery target beginning with M3, omitting `placement` resolves to `duplicate` and explicitly spelling `"duplicate"` is equivalent. Any other token, including `"hoist"`, is an unsupported-configuration error before `LinkPolicy` construction. Placement is target-wide; a scope-level override is not accepted.

## CLI Surface

### Project inventory discovery

The initial `intlify messages emit` and `intlify messages prune` commands are project-wide closed-world consumers. They accept no positional file, directory, glob, or stdin operands and do not treat an empty operand list as directory `.`. The discovered 006 `projectRoot`, the resolved linker-participating `resources.catalogs` assignments, and the resolved `messages` configuration are the only initial inventory authority.

The definition inventory contains every concrete logical file selected by a resource catalog definition with the coordinated M0 `scope` and locale binding. Resource definitions without that pair remain entry-level `fmt` / `lint` inputs and are not omitted linker sources. `PolicyAbsent`, `PolicyEmpty`, and the config-free direct-file exception never trigger an implicit project scan for a linker command.

The reference inventory contains every source file matched by each enabled built-in producer plus every exact external artifact declaration. For M0, a present `producers.js` object requires the non-empty `include` and `recognizers` members fixed above. Include patterns use exactly the 013 resource-membership glob syntax and matching semantics, including its rejection of brace expansion, character classes, extglobs, negation, absolute paths, and `..`. The producer validator owns its pointers and error evidence; reusing the grammar does not move producer configuration into `intlify_resource`.

Every artifact emitted by this built-in producer carries the exact single-unit ID `["main"]`.

A structurally valid non-empty `include` array that matches zero files is a successful completed scan and may produce a closed empty reference inventory. It is not equivalent to an omitted or empty `include` member: those forms fail configuration validation before discovery.

When present, the non-empty `producers.artifacts` array contains exact non-empty project-root-relative file paths, not globs or directories. Its entries use slash-separated Unicode segments, reject empty, `.` and `..` segments plus absolute, drive-prefixed, and UNC forms, and resolve lexically beneath `projectRoot`. The ASCII characters `*`, `?`, `[`, `]`, `{`, `}`, and `\` are rejected rather than expanded or treated as literal filename characters. The configuration validator rejects an empty array and equal duplicate paths after this exact structural resolution rather than reading one twice.

Each declaration makes the complete file bytes the authoritative external reference snapshot for that invocation. The CLI reads the complete bounded input, fingerprints those exact bytes for decode caching, and validates version, structure, identities, records, and limits through the ordinary published decoder. Successful decode accepts the producer's semantic claim that this file is its complete current output; the CLI cannot and does not infer or re-scan the producer's upstream language sources to prove freshness. Build integrations that require source-level freshness regenerate or verify the artifact before invoking the linker. A future signed or source-manifest protocol may strengthen that build boundary without changing M0 acceptance.

After successful decode, built-in single-unit request validation requires the artifact's checked `delivery_unit` to equal `["main"]`. Another valid ID is not rewritten and does not create a graph node; it fails through the ordinary missing-node `LinkOperationalError`.

For project-backed editor diagnostics, 009 applies the open-document override before this read/decode boundary. One unambiguous current buffer for the exact configured artifact source replaces the disk bytes and, after complete successful bounded decode, is authoritative for that editor invocation. Invalid buffer bytes are `ProducerFailed` without disk fallback; ambiguous source ownership is `Partial(OpenEditorWorld)`; closing the override selects the then-current disk snapshot. The editor does not label a successfully decoded configured override `ExternalArtifactUnverified` or infer upstream-source freshness.

The declaration is project-global rather than scope-qualified. A successfully decoded artifact containing zero references is still one completed external producer participant for every target scope. I/O or contract failure conservatively makes every target reference side partial because the failed complete snapshot cannot reveal a trustworthy narrower scope set.

`crates/intlify_cli` owns enumeration and the execution report. It may prune a directory only when the complete compiled include set proves that no descendant can match. It follows selected file symlinks, never follows directory symlinks, and inspects selected file-symlink and hard-link aliases through the 005 physical identity boundary.

A catalog physical group produces one definition artifact under 013/014's primary/alias rules. The built-in JS/Vue producer likewise collapses its selected logical aliases into the one physical-group participant and one primary-path artifact fixed in the JS/TS section; it does not reconstruct one artifact per alias. This grouping does not cross producer sides: a source file that belongs to both definition and reference inventories participates independently on both sides, and one side never suppresses the other.

The explicit project patterns are authoritative for closed-world selection. Root `.gitignore`, `--ignore-path`, `fmt.ignorePatterns`, `lint.ignorePatterns`, hidden-name filtering, and default VCS, dependency, or output-directory exclusions do not subtract a matching source. Projects exclude such content through `resources.catalogs[].exclude` or by narrowing the corresponding producer include set. A valid include pattern that matches no file is a successfully enumerated empty set, not an error, warning, fallback scan, or partial-world claim.

Inventory and execution order are deterministic. Representable definition and built-in producer paths use their exact slash-normalized project-relative logical identities and are ordered by those identities, independently within each side. External artifact declarations are admitted in canonical resolved-path order after duplicate rejection. Native directory-entry order is used only to select and order filesystem-discovery failures for names that cannot enter the Unicode logical-path order. Worker completion, filesystem iteration, config member order, glob order, and physical alias discovery cannot change artifacts, completeness, findings, plans, errors, or output order.

The complete resolved inventory is fixed before constructing a `LinkRequest`. A known definition-source enumeration, metadata, read, extraction, binding, freshness, or projection failure records `SourceFailed`; a known configured definition deliberately not executed records `SourceOmitted`. A built-in source-scan failure records `ProducerFailed` for every scope bound by that producer's enabled recognizers, while a deliberately unexecuted matched source records `ProducerOmitted`. An exact configured external artifact becomes a complete authoritative participant after successful bounded read and contract decoding; it needs no embedded source fingerprint, sidecar, or independent upstream-source proof. Its I/O or contract-decoding failure is `ProducerFailed` for every target scope. `ExternalArtifactUnverified` applies only when another integration submits an external artifact or cached value without the authoritative configured-snapshot evidence required by that integration.

If project-root or configuration setup fails before a resolved inventory exists, the command returns the ordinary operational error and does not invoke the linker. Once the inventory exists, source-attributable discovery and execution failures remain operational evidence and also derive the affected partial side; the linker may still return safe present-world findings, but no command with an operational failure or partial target scope generates, registers, diffs, or mutates output.

Omitting `--target` selects every configured delivery target; `--target <name>` selects the one exact checked-name match. Selection changes delivery targets and exporters only after the same complete definition and reference inventories have been processed. It never narrows catalog files, producer source files, external artifacts, scopes, findings evidence, or completeness. `emit --check` performs the identical full inventory and selected export transactions before comparing expected artifacts. `prune` always analyzes the complete configured project inventory because a delivery-target subset cannot prove project-wide unreachability.

### Multi-target execution

Configuration loading and validation, project inventory, artifact production, completeness construction, linking, and every other project-global gate complete before any selected delivery target starts. A failure in that shared prefix produces its existing top-level operational error, returns no target result, and performs no export or registration mutation.

The one shared `prepare_export` call belongs to that project-global prefix after a checked `LinkOutcome` exists. `SemanticModelConstruction`, `SemanticValidation`, and `InternalInvariant` therefore produce exactly one top-level operational error and an empty `results` array; they are never copied into every target or assigned to the first canonical target.

`MessageValidation` is the deliberate non-operational exception. It retains the checked command-level diagnostic result and creates one `blocked` result for every selected target without entering any exporter transaction.

After a successful shared prefix, selected targets are independent transactions. The initial M3 CLI visits them sequentially in exact checked target-name order; it does not introduce a target worker pool ahead of the deferred shared CLI scheduler. A later scheduler may overlap independent target preparation only if it preserves the same per-target transaction, result, failure, and side-effect semantics.

Every selected target receives exactly one result entry. A target-local export, destination-mapping, registration, check, or target-scoped internal failure completes that entry as an error and does not suppress entries for later targets. The CLI continues with every remaining target because configuration has already rejected equal or nested roots and one target holds no authority over another target's output or lock.

A completed successful write remains committed when a later target fails. The CLI does not acquire all target locks as one unit, retain cross-target backups, or roll an earlier target back to approximate command-wide atomicity. A failed target publishes no partial artifact prefix under its own transaction rules, but target A's complete new output may therefore coexist with target B's restored old output after B fails.

The same rule applies after a target-local recovery or rollback failure: its result reports that operational state, and later disjoint targets continue. A project-global or pathless internal failure that occurs outside one target transaction still aborts the shared command rather than being attached to an arbitrary target.

Results are assembled in canonical target-name order independently of execution completion. Any target-local or top-level operational error makes `summary.status` `error` and exit code `2`; otherwise any `--check` difference makes the summary `failure` and exit code `1`; only complete success or complete check equality produces `success` and exit code `0`.

Target-local errors are emitted only in that target's `results[].errors` and are never duplicated at top level. A target success, difference, or error reports only its own transaction outcome; there is no synthetic cross-target operational error and no merging of several target evidence objects.

### M3 emit result contract

The JSON reporter uses the shared 006 envelope with exact command `"messages.emit"`. After the shared prefix succeeds or returns the checked non-operational `MessageValidation` result, `results` contains the selected target entries described below in canonical target-name order. A shared-prefix operational error leaves `results` empty. Top-level `errors` contains only project-global operational errors.

The initial text reporter presents the same typed outcomes without making its prose a second machine contract. JSON field names, status tokens, counters, ordering, and omission rules are normative.

#### Command analysis

When a checked `LinkOutcome` exists, the `messages.emit` envelope inserts required command-specific `analysis` after `summary` and before `results`:

```json
{
  "generationBlocked": true,
  "findings": [
    {
      "kind": "unresolved-message",
      "blocking": true,
      "subject": {},
      "evidence": {}
    }
  ]
}
```

If no `LinkOutcome` exists because setup, artifact production, request admission, or link execution failed operationally before a checked outcome could be returned, `analysis` is omitted rather than emitted as `null` or a misleading empty result. Other commands do not acquire an empty `analysis` field.

`generationBlocked` is a required boolean. It is true when the link outcome has no bundle plans, shared message validation fails, or another already-known project-global gate forbids invoking every selected exporter. It is false only when the common validated batch may proceed to target transactions.

`findings` is always present and equals `LinkOutcome::findings()` in its fixed canonical order. Each finding has exact `kind`, `blocking`, kind-specific typed `subject`, and kind-specific typed `evidence` in that order:

- `kind` uses the linker finding token, not a lint diagnostic or operational-error code;
- `blocking` is the finding's disposition under the exact immutable link policy used by this command;
- `subject` preserves the finding kind's canonical language-neutral typed subject identity; and
- `evidence` preserves its complete canonical typed evidence.

The structured adapter adds no lint rule ID, severity, preset state, human message, rendered path, source excerpt, or target name. One finding appears once regardless of selected target count or how many related entries a later lint adapter presents. `summary.findingCount` equals the complete findings length and `blockingFindingCount` equals the number with `blocking: true`.

When shared export preparation returns `ExportMessageValidationFailure`, `analysis` appends required `messageValidation` after `findings`:

```json
{
  "generationBlocked": true,
  "findings": [],
  "messageValidation": {
    "diagnostics": [],
    "totalDiagnostics": 12,
    "truncated": true
  }
}
```

`messageValidation` uses the existing exact structured adapter shape and exists only when `totalDiagnostics > 0`. `diagnostics` is the retained canonical prefix, `totalDiagnostics` is the exact count across the one complete shared validation pass, and `truncated` is true exactly when that count exceeds the retained vector length. A retention limit of zero therefore emits an empty vector, positive total, and `truncated: true`.

Every selected target is `blocked` after shared message-validation failure. Its target-local `diagnostics` remains empty, and no exporter or registration step runs. `summary.diagnosticCount` is `totalDiagnostics`, not the retained vector length; `preparedTargets`, `preparedArtifacts`, and `preparedPayloadBytes` are zero.

When shared export preparation instead returns `SemanticModelConstruction`, `SemanticValidation`, or `InternalInvariant`, `analysis` remains present because the checked `LinkOutcome` already exists. It retains the complete canonical `findings`, sets `generationBlocked` to `true`, and omits `messageValidation` because no complete ordinary diagnostic result exists.

That operational path appends exactly one error to top-level `errors`, leaves `results` empty, invokes no exporter, and performs no destination mapping, registration, comparison, or mutation. Its prepared counters and `diagnosticCount` are zero; `findingCount` and `blockingFindingCount` continue to describe the retained checked outcome.

On validation success, `messageValidation` is omitted and the one borrowed `ValidatedExportBatch` feeds every target. The command never emits an empty success-shaped `messageValidation`, duplicates its diagnostics per target, reparses for each exporter, or stops at the first diagnostic.

#### Target result

Every entered selected target has this write-mode typed field order:

```json
{
  "target": "web",
  "exporter": "esm",
  "out": "dist/messages",
  "status": "written",
  "outputState": "updated",
  "artifactCount": 4,
  "payloadBytes": 8192,
  "diagnostics": [],
  "errors": []
}
```

Check mode inserts required `differences` after the optional paired output metrics and before `diagnostics`. Write mode omits that field entirely.

`target`, `exporter`, and `out` are respectively the exact checked target name, selected exporter ID, and configured project-root-relative slash-separated output root. They are configuration identities, not values inferred from generated artifacts, mapped host paths, or control records.

`status` is one closed operation-sensitive token:

| Operation | Status      | Meaning                                                              |
| --------- | ----------- | -------------------------------------------------------------------- |
| write     | `written`   | The complete expected artifact set was committed.                    |
| write     | `unchanged` | The existing output was exactly equal and commit was skipped.        |
| check     | `matched`   | Existing output exactly matched the expected set.                    |
| check     | `different` | A safe write would change owned output.                              |
| either    | `blocked`   | A non-operational linker or export-validation gate prevented output. |
| either    | `error`     | At least one target-local operational error occurred.                |

`written` and `unchanged` are invalid in check mode; `matched` and `different` are invalid in write mode. `written`, `unchanged`, `matched`, and `different` have empty `diagnostics` and `errors`.

`blocked` has empty target-local `diagnostics` and `errors` arrays and is justified by at least one command-level blocking linker finding applicable to the target or the one shared `analysis.messageValidation` failure. It never duplicates that evidence or converts it into an operational error merely to fill the result.

`error` has a non-empty `errors` array. A boundary that cannot return a complete valid result does not retain incomplete diagnostics beside that error; ordinary parser or semantic diagnostics use `blocked`, while a diagnostic-mapping invariant uses its operational internal-error boundary.

`diagnostics` and `errors` are always present arrays. Initial M3 keeps target-local `diagnostics` empty because all MF2 syntax/semantic validation is shared in `analysis.messageValidation`; the field reserves typed placement for a future target-specific generation diagnostic contract without overloading `errors`. Adding such a diagnostic requires an explicit addendum with ordering, retention, blocking, and count semantics. Errors use the shared operational shape and the placement contract below.

`artifactCount` and `payloadBytes` are present together only after one complete valid `ExportArtifactSet` exists. They are required for `written`, `unchanged`, `matched`, and `different`, and may remain present on a later destination or registration `error`. They are omitted for `blocked` and for an exporter `error` that produced no set; `0`, `null`, a submitted prevalidation length, and a partial prefix are not substitutes.

`artifactCount` is the exact set record count and excludes the ownership manifest, lock, journal, staging, and backup. `payloadBytes` is the checked exact sum of artifact payload lengths and excludes all control files and filesystem allocation. Equal payloads in separate artifacts are charged again.

#### Check differences

Every check-mode target contains `differences`. It is empty for `matched`, `blocked`, and `error`, and non-empty for `different`. An operationally unsafe state is an error rather than a best-effort difference list.

Each difference is one closed object:

| `kind` | Exact remaining fields | Meaning |
| --- | --- | --- |
| `output_missing` | none | `out` is absent or an adoptable empty real root. |
| `manifest_noncanonical` | none | The supported checked manifest is semantically valid but its bytes are not canonical. |
| `artifact_missing` | `path` | An expected manifest record or its required regular payload file is absent. |
| `artifact_changed` | `path`, `components` | The same expected path has different manifest metadata or actual payload bytes. |
| `artifact_stale` | `path` | A prior owned manifest path is absent from the expected set. |

Every object emits `kind` first. `path` is the exact portable artifact segment array. `components` is a non-empty canonical subset of:

1. `kind`;
2. `format_version`;
3. `media_type`;
4. `relationships`;
5. `payload`.

One `artifact_changed` record combines every differing component for that path. A payload component means the manifest payload length/fingerprint or complete actual regular-file bytes differ from expected; it does not expose either byte sequence or digest.

Difference order is:

1. the sole `output_missing`, when applicable, with no per-artifact expansion;
2. `manifest_noncanonical`, when applicable;
3. `artifact_missing` and `artifact_changed` in expected canonical path order, with missing before changed only when an impossible equal path comparison would otherwise tie; and
4. `artifact_stale` in prior-manifest canonical path order.

A present valid but noncanonical manifest may therefore produce `manifest_noncanonical` together with semantic artifact differences. A non-empty root without ownership, an invalid/unsupported manifest, a special or unsafe entry, or unreadable state produces `error` with no differences instead.

The initial M3 ESM comparison has a fixed inclusive ceiling of 2,051 records: one manifest record plus at most 1,025 expected-path and 1,025 prior-path observations. The ordinary valid ESM shapes make the reachable total no larger, but the conservative bound keeps construction independent of overlap assumptions. The first attempted excess is a registration invariant rather than truncation or an unreported check difference.

Check results never include generated source, MF2 payload, source excerpt, actual/expected digest, unified diff, timestamp, permission, or owner. `summary.differenceCount` is the checked sum of complete target difference-vector lengths.

#### Output state

Required `outputState` describes the output-root side effect that the CLI can prove at target-result emission. It is independent of `status`:

| `outputState` | Proven state |
| --- | --- |
| `unchanged` | This invocation did not change `out`. |
| `updated` | The complete current expected artifact set is installed at `out`. |
| `restored` | A complete previously validated root was restored after commit or prior-journal recovery work. |
| `indeterminate` | Rollback or recovery failed after mutation and neither complete prior nor expected state can be proved. |

`unchanged` covers check mode, a blocked target, exporter or pre-registration failure, exact-equality write skip, and a staging-only failure whose staging cleanup does not alter `out`. It describes this invocation's effect; on an error it does not independently assert that pre-existing output was valid.

`updated` is required for a successful `written` target. It also applies when the complete new root was installed but later journal or backup cleanup failed, so `status` is `error` even though expected output is visible.

`restored` applies only when the integration proves the exact prior root after current rollback or prior-journal recovery and performs no later successful commit. `indeterminate` applies only after an attempted mutation whose safe final root cannot be established; merely observing malformed pre-existing state in non-mutating check mode remains `unchanged` plus the applicable operational error.

Check mode always reports `outputState: "unchanged"` because it creates, moves, repairs, or removes no registration or control entry. A successful recovery followed by a later write reports the final current-transaction state: `updated` after commit, `restored` when the recovered/prior root remains after rollback, or `indeterminate` when proof is lost.

The field concerns the configured `out` root. A proven unchanged root may coexist with an operationally reported staging, journal, or backup cleanup problem; the error evidence identifies that control subject without redefining `outputState`.

#### Summary

When the shared project prefix completes far enough to construct selected target results, `summary` uses one of these exact field orders.

Write mode:

```text
status, operation, selectedTargets, preparedTargets,
preparedArtifacts, preparedPayloadBytes, blockedTargets,
diagnosticCount, findingCount, blockingFindingCount,
errorTargets, errorCount, writtenTargets, unchangedTargets
```

Check mode:

```text
status, operation, selectedTargets, preparedTargets,
preparedArtifacts, preparedPayloadBytes, blockedTargets,
diagnosticCount, findingCount, blockingFindingCount,
errorTargets, errorCount, matchedTargets, differentTargets,
differenceCount
```

When a checked `LinkOutcome` exists but shared export preparation fails operationally before target-result construction, both modes instead use this exact reduced field order:

```text
status, operation, selectedTargets, preparedTargets,
preparedArtifacts, preparedPayloadBytes, diagnosticCount,
findingCount, blockingFindingCount, errorCount
```

For that reduced shape, `status` is `error`; `selectedTargets` remains the already resolved selected count; `preparedTargets`, `preparedArtifacts`, `preparedPayloadBytes`, and `diagnosticCount` are zero; the two finding counters come from retained `analysis`; and `errorCount` is exactly one. Target-result-derived partition, output-state, and mode-specific counters are omitted because `results` is empty and no target transaction was entered.

`operation` is exact `"write"` or `"check"`. Every counter is a non-negative checked `u64` serialized with the shared shortest plain-decimal JSON rule.

`selectedTargets` is the resolved selected target count. The mode-specific status counters and `blockedTargets` plus `errorTargets` partition it exactly:

```text
write: writtenTargets + unchangedTargets + blockedTargets + errorTargets
       == selectedTargets

check: matchedTargets + differentTargets + blockedTargets + errorTargets
       == selectedTargets
```

`preparedTargets` counts target results for which a complete valid artifact set exists, including a target that later encounters a destination or registration error. `preparedArtifacts` and `preparedPayloadBytes` sum those target-local metrics with checked addition. Repeated artifacts or payloads across targets receive no deduction.

`diagnosticCount` is zero after successful shared message validation and otherwise equals `analysis.messageValidation.totalDiagnostics`, including diagnostics omitted by retention. Initial M3 target-local diagnostic arrays are empty and add no second count. `findingCount` is the complete `analysis.findings` length, counted once rather than once per blocked target; `blockingFindingCount` is its `blocking: true` subset. None enters `errorCount`.

`errorTargets` counts result entries with non-empty target-local `errors`. `errorCount` counts every top-level plus target-local operational error occurrence. A target with one operational error contributes once to both counters; a top-level error contributes only to `errorCount`.

Check-only `differenceCount` is the checked sum of every target `differences` vector length. It is zero for matched, blocked, and error targets and does not count operational ownership problems as differences. It is absent in write mode.

When target execution has begun, all fields for the selected operation are present even when their values are zero. The post-link shared-preparation error uses the reduced known-value shape above. If an earlier CLI/config/project-global setup fails before both a checked outcome and target result construction, `results` is empty and unresolved command-specific counters are omitted rather than filled with zero. `summary.status` remains required, and `operation` is retained only when argument/config processing resolved it safely.

Status and exit precedence remains:

- any non-empty top-level or target-local operational error: `status: "error"`, exit `2`;
- otherwise any blocking evidence, target `blocked`, or check `different`: `status: "failure"`, exit `1`;
- otherwise: `status: "success"`, exit `0`.

A successful write remains `success` when one or more targets are `written`; write mutation itself is not a check failure.

### CLI operational error mapping

The shared registry in [appendix-ox-mf2-error-code.md](./appendix-ox-mf2-error-code.md#message-linker-and-export-workflow) reserves four boundary-level operational codes for the 014 workflow:

| Boundary | CLI operational code | Required `details.kind` values |
| --- | --- | --- |
| Artifact production or contract ingestion | `message_artifact_failed` | `invalid_artifact`, `unsupported_version`, `limit`, `producer_failed` |
| Stateless linker operation | `message_link_failed` | `invalid_request`, `unsupported_contract`, `limit` |
| Exporter invocation and checked common output | `message_export_failed` | `unsupported_batch`, `generation_failed`, `output_limit_exceeded`, `invalid_output` |
| Platform destination mapping and registration | `message_output_registration_failed` | `unsupported_capability`, `destination_mapping_failed`, `destination_collision`, `registration_failed` |

These codes preserve the public boundary rather than allocating one top-level string for every typed enum variant. The selected `details.kind` is required and stable. When the originating error has checked structured evidence, `details.evidence` is its canonical bounded adapter shape; a reporter does not replace it with `Display`, `Debug`, a dependency error name, a source chain, source text, or an arbitrary message.

`ArtifactReadError::Transport` and ordinary configured source-file I/O reuse `input_read_failed` with the existing normalized I/O details. External artifact decoding, caller-owned direct construction, cache admission, and defensive contract admission map `ArtifactContractError` one-to-one to `invalid_artifact`, `unsupported_version`, or `limit`.

The built-in JS/Vue source producer instead uses `message_artifact_failed` with `details.kind: "producer_failed"` once one resolved inventory participant cannot produce its complete artifact. This includes a source/profile failure discovered before bytes need to be read and a producer-owned checked output failure after scanning; an ordinary attempt to read selected disk bytes still uses `input_read_failed`, and a logical host path that cannot become the shared portable source identity still uses `input_path_unrepresentable`.

The canonical `details.evidence` object for `producer_failed` contains the members below in this exact emission order:

1. `producer`: exact `"dev.intlify/js-reference"`;
2. `stage`: one closed stage token;
3. `reason`: one stage-owned closed reason token;
4. `source`: the primary project `SourceDocumentIdentity`;
5. optional `span`: one checked `SourceUtf8Span`, only when the failure has an exact safe source range;
6. optional `limit` and `observed`: both present together only for a checked limit failure.

The primary source identity is available independently of the prefixed reference-artifact identity. If it cannot be constructed, processing fails at the earlier shared path-representation boundary and never fabricates producer evidence. Optional members are omitted rather than emitted as `null`. `limit` and `observed` are unsigned bounded integers; `observed` is the exact checked attempted value or the owning counter's already specified first-over sentinel, never a guessed final size.

The stage and reason vocabulary is:

| Stage | Admitted reasons |
| --- | --- |
| `identity` | `artifact_identity_limit` |
| `profile` | `unsupported_source_suffix`, `conflicting_alias_profile`, `unsupported_vue_script_lang`, `unsupported_vue_template_lang` |
| `snapshot` | `invalid_utf8`, `source_count_limit`, `source_bytes_limit`, `source_bytes_total_limit` |
| `parse` | `syntax_invalid` |
| `source_map` | `host_span_unavailable`, `host_span_ambiguous`, `host_span_unrepresentable` |
| `recognizer` | `selector_argument_missing`, `selector_argument_spread` |
| `selector` | `lookup_selector_invalid`, `set_selector_dynamic`, `set_selector_invalid` |
| `artifact` | `output_contract_invalid`, `output_limit`, `canonical_encoding_failed` |

No other stage/reason pairing is valid. `span` is permitted only for `parse`, `source_map`, `recognizer`, and `selector`; `limit` plus `observed` is required only for `artifact_identity_limit`, all three `*_limit` snapshot reasons, and `output_limit`, and forbidden for every other reason. A parse frontend may use a bounded dependency diagnostic internally for display, but no parser code, dependency type name, source excerpt, selector spelling, arbitrary message, debug string, or error chain enters this stable evidence.

Within one source group, failure selection completes stages in the table's declaration order. For stages that can observe several source sites, the smallest checked `(span.start, span.end)` wins, followed by the reason order shown in that stage. Across groups, errors are ordered by the primary `SourceDocumentIdentity` canonical order. Glob order, alias discovery, AST traversal, parser recovery order, cache hits, worker completion, and reporter choice cannot change selection or ordering.

The producer emits at most one such operational error for one failed physical source group, emits no artifact for that group, and derives `ProducerFailed` for every scope bound by the enabled recognizers. Other successfully completed groups may still enter the safe present-world link request, but any operational producer error or affected partial scope retains the existing no-export/no-prune gate.

An impossible AST duplication, invalid checked parser span for valid UTF-8, contradiction between already validated state, or other producer invariant uses `internal_error` with `details.reason: "message_producer_invariant_failed"` instead. It never consumes an input-facing stage/reason token merely to avoid the invariant boundary. For a built-in output constructor, an ordinary checked, source-dependent rejection uses the `artifact` stage above; an impossible rejection after all equivalent checks have succeeded is an invariant.

A non-invariant `LinkOperationalError` uses `message_link_failed`. Invalid checked policy, scope mapping, completeness, or delivery-graph input maps to `invalid_request`; an unsupported selector, domain, artifact, or contract revision maps to `unsupported_contract`; and checked operational budget evidence maps to `limit`. `LinkOperationalError::InternalInvariant` is never disguised as one of those input kinds.

The four non-invariant `ExportErrorKind` values map mechanically to the first four snake-case `message_export_failed` kinds above and retain the matching typed `ExportErrorEvidence`. `ExportErrorKind::InternalInvariant` instead crosses the internal boundary below. Export preparation does not use `message_export_failed`. The initial M3 CLI has no caller-supplied retention option and therefore no `invalid_options` path for `ExportValidationLimitConfigurationError`; an ordinary `ExportMessageValidationFailure` remains result-level parser/semantic diagnostics.

`message_output_registration_failed` begins only after a complete valid `ExportArtifactSet` exists. It covers a valid common artifact kind or relationship unsupported by the selected platform, failure to map a portable logical path into the platform destination namespace, collision introduced by platform path rules, or failure to register/write the complete selected target. Exporters cannot return it.

#### Registration evidence envelope

Every `message_output_registration_failed` contains required `details.kind` followed by required `details.evidence`. Evidence is one checked closed object selected by the kind; it is not a free-form map. Optional members described below are omitted rather than emitted as `null`.

The operational error's top-level `path` is the configured project-root-relative slash-normalized `out`. Inner artifact or control subjects remain in evidence and never replace that target-root path. Human-readable `message` may explain the platform condition but is not a stable discriminator.

All logical artifact and output-root paths in evidence use their original portable segment arrays. An optional mapped or unowned relative path is emitted only when it can be represented as a checked project-root-relative segment array with at most 128 segments and 8 KiB (`8,192` bytes) of decoded segment data; otherwise it is omitted and the closed reason carries the portable classification. No lossy path, absolute temporary path, random transaction basename, source text, dependency type, `Display`, `Debug`, or error chain enters stable evidence.

##### Unsupported capability

For `details.kind: "unsupported_capability"`, evidence contains required `capability` first and then only the optional artifact-specific fields admitted below:

| `capability` | Required additional evidence |
| --- | --- |
| `artifact_kind` | `artifactPath`, `artifactKind` |
| `artifact_format_version` | `artifactPath`, `artifactKind`, `formatVersion` |
| `media_type` | `artifactPath`, `artifactKind` |
| `relationship_kind` | source `artifactPath`, `relationshipKind` |
| `relationship_graph` | none; optional source `artifactPath` when one canonical source identifies the unsupported shape |
| `filesystem_no_follow` | none |
| `process_locking` | none |
| `same_filesystem_staging` | none |
| `durable_flush` | none |
| `safe_rename` | none |
| `secure_random` | none |
| `deterministic_recovery` | none |

`formatVersion` is the exact `{ major, minor }` object and `relationshipKind` is `"eager_load"` or `"lazy_load"`. An optional member not admitted by the selected capability is invalid evidence. Platform library names, OS feature probes, and guessed fallback capabilities are not emitted.

##### Destination mapping failure

For `details.kind: "destination_mapping_failed"`, evidence contains required `reason` then required `artifactPath`, followed only by optional `segmentIndex` and `mappedDestination`:

| `reason` | Meaning |
| --- | --- |
| `host_name_unrepresentable` | One logical segment cannot be represented exactly by the destination platform. |
| `host_name_reserved` | One exactly represented segment is forbidden in that destination namespace. |
| `host_path_limit` | The mapped complete path exceeds a platform path or component limit. |
| `host_destination_unsupported` | The platform cannot represent this otherwise valid portable destination shape. |

`segmentIndex` is a zero-based unsigned `u16` and is present only when one exact artifact-path segment owns the failure. `mappedDestination`, when safely representable within the evidence bound, is the project-root-relative segment sequence the integration had constructed before rejection. Mapping never shortens, escapes, suffixes, or silently drops a valid logical path merely to avoid this error.

##### Destination collision

For `details.kind: "destination_collision"`, evidence begins with required `reason` and uses exactly one of these shapes:

| `reason` | Required evidence |
| --- | --- |
| `mapped_path_collision` | canonical `firstArtifactPath`, canonical `secondArtifactPath`; optional `mappedDestination` |
| `host_equivalence_collision` | canonical `firstArtifactPath`, canonical `secondArtifactPath`; optional `mappedDestination` |
| `reserved_control_path` | `firstArtifactPath`, `controlPath` |
| `output_root_control_id_collision` | `firstOutputRoot`, `secondOutputRoot`, `controlPath: "lock"` |

`mapped_path_collision` means integration mapping produced one exact destination key from two unequal logical paths. `host_equivalence_collision` means two unequal mapped spellings collapse under host case, Unicode, alias, or equivalent-name behavior. The two artifact paths are ordered by the ordinary canonical logical-path order, never discovery order.

`reserved_control_path` initially permits only `controlPath: "manifest"` and identifies the exact exporter path that equals `[".intlify-output-manifest.json"]`. `output_root_control_id_collision` compares the current checked output root with the unequal root decoded from the persistent lock record; the roots are ordered as current target first and observed lock owner second rather than lexicographically.

##### Registration failure

For `details.kind: "registration_failed"`, evidence contains required `stage`, `reason`, and `subject` in that order. The closed stage/reason pairs are:

| `stage` | Admitted `reason` values |
| --- | --- |
| `ownership` | `root_not_directory`, `manifest_missing`, `manifest_invalid`, `manifest_unsupported`, `manifest_limit`, `unowned_entry`, `unsafe_entry`, `entry_unrepresentable` |
| `lock` | `control_conflict`, `control_invalid`, `acquire_failed`, `concurrent_state_changed` |
| `staging` | `control_conflict`, `create_failed`, `write_failed`, `flush_failed` |
| `commit` | `journal_conflict`, `journal_write_failed`, `old_root_move_failed`, `new_root_install_failed`, `cleanup_failed`, `flush_failed` |
| `rollback` | `old_root_restore_failed`, `cleanup_failed`, `flush_failed` |
| `recovery` | `journal_invalid`, `state_ambiguous`, `old_root_restore_failed`, `cleanup_failed`, `flush_failed` |
| `check` | `snapshot_unreadable`, `snapshot_changed` |

`subject` is exactly `output_root`, `manifest`, `lock`, `journal`, `staging`, `backup`, or `artifact`. Evidence may then contain, in order, only the applicable optional `control`, `artifactPath`, `relativePath`, `operation`, `ioKind`, and `rawOsError`.

`control` uses `manifest`, `lock`, `journal`, `staging`, or `backup`. `artifactPath` is used only for an expected exporter artifact; `relativePath` is used only for a safely representable actual entry below `out`. An entry that cannot enter that bound uses `entry_unrepresentable` and omits `relativePath`.

`operation` is one closed token: `inspect`, `open`, `read`, `create`, `write`, `flush`, `lock`, `replace`, `rename`, or `remove`. When an underlying operating-system error exists, `ioKind` is required and normalizes to `not_found`, `permission_denied`, `not_file`, `not_directory`, or `unknown`. `rawOsError` is included only when exposed by the OS and is serialized unchanged as a signed JSON integer. Portable consumers branch on stage/reason and never on that optional platform number.

Manifest syntax, version and limit failure uses the `ownership` stage even during `--check`, because ownership must be established before comparison. A lock that appears during an initially lock-free check uses `check` / `snapshot_changed`; it is not a manifest difference. An ambiguous journal/root combination uses `recovery` / `state_ambiguous`, retains every participant, and exposes no random staging or backup basename.

#### Registration failure selection

One target transaction emits at most one `message_output_registration_failed`. It selects that error by this fixed complete-stage order:

1. preflight supported artifact kind, format version, media type, relationship kind/graph, static platform capability, and canonical manifest capacity;
2. map artifacts in canonical logical-path order;
3. reject the reserved manifest path, then mapped collisions in canonical logical-path pair order;
4. establish the verified output parent and output-root control ID;
5. acquire and validate the target lock;
6. perform journal recovery;
7. validate the current root and ownership manifest;
8. compare check state or construct complete staging output; and
9. commit, then rollback when required.

For one artifact during capability preflight, kind precedes format version, media type, and canonical relationship order. A static platform capability failure precedes filesystem inspection; a capability that becomes unknowable until a concrete operation remains at that owning later stage rather than being guessed early.

Within ownership, check, staging, and recovery, checked expected paths are visited canonically. Representable actual entries follow exact UTF-8 segment-array order. A native entry that cannot become such an identity follows the platform's exact native component order — raw unsigned bytes on POSIX and unsigned UTF-16 code units on Windows — and reports the first `entry_unrepresentable` without lossy display.

Worker completion, directory enumeration, hash-map order, OS error arrival, human reporter choice, and prior cache state never choose the public failure. Once a stage selects its error, later ordinary work does not run merely to collect another error.

If commit fails and rollback succeeds, the original commit-stage error is returned. If rollback or recovery needed to re-establish a complete root also fails, that later `rollback` or `recovery` error replaces the original error because the current publication state is the safety-critical outcome. The original failure may remain in bounded human-facing tracing but is not merged into stable evidence or emitted as a second target error.

Internal invariants reuse the existing `internal_error` code and one globally unique `details.reason`:

- `message_artifact_invariant_failed` for artifact producer/codec/accounting invariants;
- `message_producer_invariant_failed` for an impossible built-in source recognizer state;
- `message_linker_invariant_failed` for `LinkOperationalError::InternalInvariant`;
- `message_export_preparation_invariant_failed` for selection, diagnostic mapping, or batch-construction invariants not already owned by parser semantic validation;
- `message_exporter_invariant_failed` for `ExportErrorKind::InternalInvariant`, with its exact `InternalInvariantViolation` spelling in `details.invariant`; and
- `message_output_registration_invariant_failed` for an impossible platform-integration registration state.

`message_export_preparation_invariant_failed` requires `details.invariant` immediately after `details.reason`. It is one closed flat snake-case token derived mechanically from the exact `ExportPreparationInvariant` value:

| Rust invariant                                | `details.invariant`                    |
| --------------------------------------------- | -------------------------------------- |
| `DiagnosticCountOverflow`                     | `diagnostic_count_overflow`            |
| `DiagnosticMapping(LabelCountExceeded)`       | `diagnostic_label_count_exceeded`      |
| `DiagnosticMapping(InvalidPrimarySpan)`       | `diagnostic_primary_span_invalid`      |
| `DiagnosticMapping(InvalidLabelSpan)`         | `diagnostic_label_span_invalid`        |
| `DiagnosticMapping(ForeignLabelSource)`       | `diagnostic_foreign_label_source`      |
| `OutcomeContract(DuplicatePlanCoordinate)`    | `outcome_duplicate_plan_coordinate`    |
| `OutcomeContract(NonCanonicalPlanOrder)`      | `outcome_noncanonical_plan_order`      |
| `OutcomeContract(DuplicateLogicalMessage)`    | `outcome_duplicate_logical_message`    |
| `OutcomeContract(NonCanonicalMessageOrder)`   | `outcome_noncanonical_message_order`   |
| `OutcomeContract(DefinitionSnapshotMismatch)` | `outcome_definition_snapshot_mismatch` |

This reason variant contains no category, second violation field, source or entry identity, plan or message index, path, span, payload excerpt, nested error, or additional invariant vector. Human-facing tracing may retain bounded implementation-local context, but it cannot change the selected stable token or structured error shape.

Parser semantic API failure during export preparation continues to use the 012-owned classification: `semantic_api_misuse` with no additional required details field for caller misuse, or `semantic_invariant_failed` with the exact required construction/validation stage for a valid call's invariant failure. A boundary does not collapse or relabel another boundary's reason merely to make every reason start with `message_`.

`LinkFinding` values, including blocking findings, are successful analysis results and never enter `errors[]`. Ordinary export-gate parser and semantic diagnostics retain their existing kebab-case diagnostic codes, category, severity, evidence, and bounded result shape; they likewise do not acquire an operational alias. Both cases may prevent plans or output and produce a non-success command result, but only an actual operational error makes `summary.status` equal to `error` under the 006 envelope.

`message_artifact_failed`, `message_link_failed`, and every operational `ExportPreparationError` are project-transaction errors and use top-level `errors[]`. Preparation `InternalInvariant` maps to `message_export_preparation_invariant_failed`; its two semantic variants map to the 012-owned `semantic_api_misuse` or `semantic_invariant_failed` reason without changing that top-level placement.

`message_export_failed` and `message_output_registration_failed` are target-transaction errors and use the selected M3 target's `results[].errors`. All operational paths exit with the shared operational exit code `2`, expose no partial typed result from their failed boundary, and permit no prune mutation. The M0 artifact/link placement and M3 emit target DTO, counters, independent-target behavior, output-state reporting, and registration evidence are complete here. M5 prune retains its own future result and mutation-counter addendum.

### Commands

At M0, linker-native findings are a library result validated directly by the core conformance and semantic suites; no standalone linker check command is introduced. The later `intlify lint` adapter becomes the general user-facing check surface, while generation and mutation commands consume the same linker findings independently of lint-rule enablement:

- `intlify messages prune [--write]` — logical deletion plan for `unused-message` findings, applied through the separate 013 structural-mutation boundary when `--write` is present; refuses mutation unless every affected scope is definition-closed, reference-closed, and free of unbounded-dynamic degradation.
- `intlify messages emit [--target <name>] [--check]` — require a resolved non-empty M3 `delivery.targets`, then run the link and exporters for named targets; requires both completeness sides to be closed for every target scope, and `--check` re-runs and diffs as the CI freshness job. Missing delivery configuration is an error before inventory or linking, not a successful no-op. M0–M3 expose no plan-dump mode: tests and in-process integrations inspect the typed `MessageBundlePlan`, while the CLI proceeds directly to the selected exporter in M3.

Global options, reporters, operational error shaping, and exit codes inherit the Phase 3A contracts. `messages` is the canonical command namespace; `catalog` is not a command alias.

The shared JSON envelope uses exact resolved command identities `messages.emit` and `messages.prune`; these dotted strings are not shell aliases. Bare `intlify messages` is namespace help and does not run linking. This document's M3 emit contract owns its typed `summary` and `results` DTOs, M5 owns the future prune DTO, and 006 owns the surrounding envelope and unresolved-command behavior.

## Milestones

### Main track

#### M0 — contract

Define the mutable WIP v0.1 `MessageReferenceArtifact` contract (selector semantics, provenance, version negotiation) with its conformance suite and the one-complete-source `MessageDefinitionArtifact` envelope. M0 does not freeze v1 or introduce a required-feature list.

Deliver a fallback-blind linker core over locale-bearing definitions through the stateless `link(&LinkRequest) -> Result<LinkOutcome, LinkOperationalError>` API: references resolve against the locale-agnostic `(scope, domain, key)` union, while every definition retains its mandatory exact locale.

M0 also fixes the execution-derived `ScopeCompletenessTable`, the explicit JS recognizer `kind` / `scope` / `domain` / `keySyntax` contract, the delivery-DAG direction and validation rules, the exact built-in one-node `["main"]` graph, and target-wide `duplicate` placement. The checked `ScopeMappingTable` remains part of the core request contract, but built-in CLI/editor orchestration supplies only its canonical empty value and exposes no mapping configuration field.

It also fixes the resolved project-inventory and failure-accounting model used by the later M3 and M5 CLI leaves. Fixing that model in M0 makes source-scan and completeness conformance testable without exposing an early generation or mutation command.

M0 implements the reusable `crates/intlify_cli` project-inventory, built-in producer, single-unit graph, completeness construction, and in-process integration-test path, but publishes no new executable CLI leaf and activates no editor link session. M3 exposes `intlify messages emit`, M5 exposes `intlify messages prune`, and L0 enables linker-backed editor/lint diagnostics; each reuses this M0-owned orchestration rather than defining another inventory or link path.

It returns deterministic linker-owned `ambiguous-message-definition`, `unresolved-message`, `unused-message`, `unbounded-dynamic-reference`, and `degraded-analysis` findings and produces basic plans only when no finding or completeness gate blocks them; no exporter consumes those plans until M3.

M0 includes a JS/TS source-scan producer without bundler integration and has no dependency on `intlify_lint`. It depends on 013 Tier 1 extraction, the coordinated promotion of explicit 013 `scope` plus `path` / `fixed` locale binding, and explicit producer-to-scope/domain bindings.

#### M1 — typed keys

Derive the language-neutral typed-key model from each explicit per-scope coverage baseline. Fail generation when any production-locale key is absent from that baseline, and emit the initial JS/TS scope-bound generated accessor module without ambient `.d.ts` augmentation.

The M1 implementation addendum fixes the bounded `coverageBaseline` config admission contract plus exact module ABI, file naming, runtime binding, argument-type derivation, and freshness behavior. `useMessageSet` remains a producer-side bounded-selector API.

#### M2 — fallback-aware link

Add the `messages.fallback` field, fallback-bearing resolved `LinkPolicy` member, activation of the two reserved fallback counters, exact validation and cache identity, and per-requested-locale chain analysis atomically. M0 and M1 reject the raw field and expose no fallback-bearing policy constructor.

Activate `missing-translation` versus `unresolved-message` and `orphaned-translation` over the locale-bearing M0 artifacts. The orphaned finding reuses M1's canonical baseline-versus-union difference rather than defining a second comparison path.

#### M3 — initial exporter

Establish the initial exporter contract:

- shared `prepare_export` and `ValidatedExportBatch` boundaries;
- an object-safe `PlatformExporter`;
- the concrete, versioned `ExportArtifactSet { Vec<ExportArtifact> }` and `ExportError` boundary;
- exactly one exporter per export transaction, with multiple artifact records allowed in its one set;
- initial built-in output format `0.1`;
- per-locale ESM assets and a loader map represented through the generic artifact envelope;
- `--check`; and
- one delivery unit under the same `duplicate` policy.

#### M4 — bundler integration

Use the live chunk DAG as delivery units, with `duplicate` placement over real graphs, virtual modules, and dev-mode findings. Design and fixture validation for optional `hoist` via a virtual-super-root dominator tree begins here but does not become supported merely by entering M4.

#### M5 — prune

Add the coordinated 013 structural-deletion contract and its format-specific capability rules, map linker definition evidence to artifact-local deletion targets, and expose dry-run plus explicitly requested fail-complete file mutation. This milestone does not broaden formatter value write-back into a structural editor.

### Lint integration track (not a gate on the main track)

#### L0 — M0 finding adapter

After the Phase 3C contracts and the dedicated catalog-level addendum are available, map the five M0 linker finding categories into initially default-off `intlify lint` rules without changing linker analysis.

Resolve enabled-rule input capabilities before work, skip reference production when none requires it, and otherwise reuse one shared producer/cache/linker orchestration and one `LinkOutcome`.

#### L1 — locale finding adapter

After M2, extend the same initially default-off adapter with `missing-translation` and `orphaned-translation`. Preset promotion remains evidence-gated per rule.

### Native track (parallel after M0)

#### N0

Rust `message!` / `message_set!` + tagged/versioned BLAKE3-256 reference IDs + complete producer dictionary + checked final-binary dictionary join (single-unit artifact) + external per-locale bundles; producer-stable logical site identity and a separate debug sidecar preserve per-site origins without embedding paths.

#### N1

C/C++ macros and WASM after object/linker format-survival validation.

#### Deferred

baked native data (two-phase build machinery), per-object unit granularity, binary container, stub scaffolding, identical-to-fallback omission.

## Validation

### Configuration, CLI, and lint integration

- Configuration and CLI naming fixtures accept the exact `messages` section and `intlify messages ...` namespace, keep `resources.catalogs` valid as the independent resource-source section, and reject `catalog` as a config-section or command alias.
- Message-config result fixtures return exactly one `config_validation_failed` under the fixed 014 section-local order with stable reason, narrowest applicable JSON Pointer, and bounded evidence; require sequential, cached, and parallel validators to select the same violation; and prove that no partial messages configuration or downstream work survives. They leave syntax, duplicate-member, root-field, and earlier product-section failures to the fixed 006 boundary.
  - Milestone-order fixtures require section shape, ASCII-ordered unknown members, M0 `locales` → `dynamicReferences` → `roots` → `producers`, appended M1 `coverageBaseline`, appended M2 `fallback`, and appended M3 `delivery`. They permute JSON member, schema, Rust declaration, map, cache, and worker order; prove each field completes before the next; and prove a newly admitted field changes only its former unknown-field result without reordering pre-existing fields.
  - Reason-granularity fixtures reuse one stable field-family reason across missing, shape, empty, scalar, limit, membership, and duplicate conditions while changing the pointer to the narrowest applicable location; require dedicated relation-level reasons only for cross-field/scope/target contradictions; and reject both per-condition reason proliferation and one generic messages-config reason.
  - Reason-vocabulary fixtures admit exactly the milestone-gated closed table above, preserve the existing 006 `unknown_field`, later-stage `scope_key_domain_mismatch`, and 013-owned catalog reasons without aliases, and reject spelling variants, generic/custom reasons, or use before the owning milestone.
  - Config-evidence fixtures include exact scalar `value` only within the owning per-value or 255-byte fallback evidence ceiling; omit missing, `null`, collection, object, and over-ceiling values without truncation or substitution; expose exact `limit` and `observed` for count/byte overruns; and use later `pointer` plus earlier `firstPointer` for semantic duplicates without copying the value.
  - Output-conflict evidence fixtures expose only the validated `firstTarget`, `secondTarget`, `firstOutPointer`, and `secondOutPointer` fields in addition to common reason/pointer data, never either raw output root. They preserve the independently owned unknown-field, scope/domain-mismatch, and 013 catalog evidence shapes.
- Project-inventory CLI fixtures reject every positional file, directory, glob, stdin, and post-`--` operand before project processing; prove that no operands do not imply `.`; and preserve the same resolved command identities for valid option-only invocations.
- Inventory-selection fixtures enumerate only linker-participating resource definitions, built-in producer include matches, and exact external artifact declarations; accept a valid zero-match pattern as a complete empty set; and reject config-free direct-extension, content-sniffing, or standalone `.mf2` fallback.
- Empty-producer fixtures require omitted `producers` and an explicit empty object to resolve to the same empty producer inventory, cache identity, scope completeness, findings, and plans without scanning project files. They mark each target scope's reference side `Closed`, permit ordinary `unused-message` and explicit prune selection when the definition side is also closed, and never synthesize `ProducerOmitted`.
- Built-in-producer shape fixtures require both non-empty `include` and non-empty `recognizers` whenever `producers.js` is present; reject an omitted or empty member without supplying defaults; and accept omission of `js` itself.
- External-producer shape fixtures accept omission of `producers.artifacts`, require a non-empty array when the member is present, and reject an explicit empty array rather than normalizing it to omission.
- Producer-include fixtures use the exact 013 glob grammar, accept the example's separate `.ts`, `.tsx`, and `.vue` patterns, accept a valid non-empty pattern set that matches zero files as one completed closed scan, and reject brace expansion or a glob-shaped external artifact path rather than delegating syntax to a filesystem dependency.
  - Source-profile fixtures admit only exact lowercase `.js`, `.jsx`, `.mjs`, `.cjs`, `.ts`, `.tsx`, `.mts`, `.cts`, `.d.ts`, `.d.mts`, `.d.cts`, and `.vue` suffixes with longest-suffix matching; parse declaration suffixes through the declaration profile; and reject every unsupported, extensionless, or case-varied selected source as `ProducerFailed` without metadata, editor-language, or project-config inference.
  - Source-goal fixtures use fixed module for `.mjs`/`.mts`/`.d.mts`, fixed CommonJS for `.cjs`/`.cts`/`.d.cts`, and fixed module for every Vue script block. Neutral suffixes parse module first and retry the complete source exactly once as script only after an ordinary grammar rejection; they keep only the successful authoritative AST, prefer module when it succeeds, and never retry a limit, UTF-8, cancellation, invariant, internal, or post-scan failure.
  - Parser-recovery fixtures require zero syntax errors for one successful complete parse; discard every recovered AST and record from a failed attempt; allow only the specified neutral-source module-to-script retry; and map two failed attempts to one `parse` / `syntax_invalid` result with the smallest safe span independently of dependency recovery order. Parser warnings and later lint/semantic diagnostics do not fail producer syntax admission.
  - Source-snapshot fixtures require exact valid UTF-8 bytes, admit an optional leading UTF-8 BOM while retaining its three bytes in every host offset and cache input, preserve line endings and scalar spelling, and reject malformed or transcoding-dependent input as `ProducerFailed` without an unambiguous-goal retry.
  - Producer-source-limit fixtures accept zero and exact `65,536` physical groups, 64 MiB in one snapshot, and 1 GiB across admitted snapshots; reject the first-over group or byte; charge empty and equal-byte groups independently; charge aliases once; and provide no deductions for BOM removal, normalization, cache reuse, compression, interning, memory mapping, or worker partitioning.
  - They select group-count failure before source-content I/O, always report the per-source first-over byte sentinel, charge aggregate bytes in canonical primary-source order with the exact prospective checked sum, and keep the three producer counters independent from resource, artifact, `LinkLimits`, and external-artifact budgets.
  - Per-source excess fails only that group and permits other present-world artifacts. Count or aggregate excess emits one error at the canonical first-over source, discards every built-in artifact and cache-publication candidate, weakens every recognizer-bound scope, and never links a prefix, truncates, shards, relaxes, or changes behavior with enumeration and worker order.
  - Built-in cache-key fixtures distinguish cache-schema revision, artifact version, complete producer identity, primary and canonical aliases, grammar and goal, every canonical recognizer field, exact source length and BLAKE3-256 digest, and `["main"]`; they prove include spelling and linker/export-only policy cannot cause a miss when the semantic producer inputs are equal.
  - Cache-value fixtures store only a complete checked artifact, including empty output; never store recovery ASTs, failures, cancellation, or partial records; reapply current source and artifact limits plus expected identity/provenance/origin/order validation on hit; and treat missing, stale, corrupt, invalid, and cache-I/O outcomes as regeneration misses without changing completeness.
  - Cache-publication fixtures count hit bytes normally, discard every pending entry after producer-wide count/aggregate failure, publish only after global admission, and keep editor protocol version in the external publication gate so equal-byte reuse cannot publish against a superseded buffer.
  - Vue snapshot fixtures map every parser-local or embedded coordinate back through BOM and template decoding to the exact original `.vue` UTF-8 snapshot; no normalized, block-local, UTF-16, or decoded-expression coordinate may escape as `SourceOrigin`.
  - Physical-profile fixtures require every participating alias to derive the same complete grammar/source-goal profile and fail the group for an unsupported or conflicting alias rather than selecting by primary path; a valid zero-reference declaration source still emits one empty artifact.
- Built-in source-partition fixtures emit one artifact per selected physical source group, including an empty artifact for a successful zero-reference scan; choose the exact UTF-8-ordered primary logical path; construct `Project` plus `["js", "module", ...primary_path_segments]` through ordinary identity limits; and map an inadmissible prefixed identity to `ProducerFailed` without truncation, hashing, or an alternate ID.
  - They use the primary path for every record origin, suppress alias artifacts and alias-induced duplicate records, retain equal selectors at distinct source ranges, reject a duplicate complete first-argument range as a frontend invariant failure, order records by exact host `(start, end)`, and order completed artifacts by canonical identity independently of glob, alias, AST, and worker order.
  - Built-in provenance fixtures require exact ID `dev.intlify/js-reference` for every JS, JSX, TS, TSX, and Vue artifact; use one immutable build-supplied revision across CLI, editor, empty artifacts, and worker partitions; reject configuration or invocation overrides; and change cache provenance whenever the effective output-affecting producer build changes.
- JS-call argument fixtures consume only an ordinary non-spread first argument, ignore every later argument for selector semantics, emit `UnboundedDynamic` for a dynamic lookup first argument, and fail the complete source as `ProducerFailed` for a matched zero-argument call, a spread first argument, or a dynamic set first argument.
  - Static-string fixtures accept exact decoded string literals and substitution-free template literals through only the admitted parenthesized and TypeScript value-preserving wrappers. They classify identifiers, imports, properties, binary and conditional expressions, calls, tagged templates, and templates with substitutions as dynamic without attempting lexical or module resolution; lookup emits `UnboundedDynamic`, while set fails production.
  - Static-selector failure fixtures map a statically known lookup rejected by key-syntax conversion, domain grammar, or size limits to `selector` / `lookup_selector_invalid` at the first-argument span; never retry another syntax, widen it to `UnboundedDynamic`, retain the scope, or omit the call; and keep only genuinely unknown lookup values on the dynamic path. They distinguish a set's unknown value as `set_selector_dynamic` from its known invalid finite pattern as `set_selector_invalid`.
  - Built-in reason fixtures require exact `lookup argument is not statically known` on every `UnboundedDynamic`, exact `bounded set declared by configured recognizer` on every `Pattern`, and omission on every `Exact`; reject configuration, source, callee, selector, diagnostic, or reporter-derived changes; and prove reason bytes remain provenance rather than semantic control.
  - Built-in-origin fixtures require every emitted JS/TS record to span exactly the complete first argument expression, including its own parentheses and admitted TypeScript wrappers but excluding call delimiters and later arguments. They apply the same rule to static and dynamic lookup records, map Vue ranges to exact host bytes, and fail the complete source rather than omit origin when the checked `u32` UTF-8 range cannot be constructed.
- Vue SFC producer fixtures scan both valid inline script forms and every standard-template embedded expression in canonical host-source order; map origins to exact original `.vue` UTF-8 spans through checked monotonic maps; accept only absent/`js`/`jsx`/`ts`/`tsx` script language and no custom template language; and fail the complete source as `ProducerFailed` for SFC, language, embedded parse, or mapping failure without a partial artifact.
  - They never follow `<script src>` or scan style/custom blocks, scan a referenced external script only through its own include match and identity, match template calls through the same callee contract, and require declarative non-call APIs such as `<i18n-t keypath>` to be represented by configured roots or an external artifact rather than guessed.
- Closed-world fixtures prove that `.gitignore`, `--ignore-path`, formatter/linter ignore patterns, hidden-name filtering, and default VCS/dependency/output exclusions cannot remove configured inputs; directory symlinks are not traversed, selected file symlinks and hard links use physical grouping, and a dual-role Vue file participates once on each applicable side.
- Completeness fixtures map every known source and producer outcome to the fixed partial reason; treat a successfully decoded exact configured external artifact, including an empty one selected from CLI disk or one unambiguous editor buffer, as an authoritative project-global participant; conservatively weaken every target reference scope on its I/O or contract failure; reserve `ExternalArtifactUnverified` for non-authoritative integration/cache inputs; and prove that setup failure creates no `LinkRequest` while post-inventory failure produces no export, check diff, registration, or mutation.
- Editor external-artifact override fixtures replace configured disk bytes with the exact current uniquely owned buffer, include its version in the captured editor snapshot, accept complete successful decode as closed configured participation without `ExternalArtifactUnverified`, map invalid selected bytes to `ProducerFailed` without disk fallback, map multiple claiming documents or unresolved ownership to `OpenEditorWorld`, and return to the current disk snapshot after close.
- External-artifact cache fixtures hash the exact complete file bytes with BLAKE3-256, include normalized declared path, byte length, decoder revision, and effective-limit compatibility, miss on any byte change, and prove that this consumer-side fingerprint neither enters the artifact wire contract nor claims upstream source freshness.
- Target-selection fixtures require omission of `--target` to select every configured target and one supplied exact name to select exactly one; expose no default-target setting; reject equal checked target names without first/last-wins or merging; and prove that selection changes only delivery/export work after full analysis and never changes inventories, artifact identities, completeness, findings, or prune reachability.
  - Target-name fixtures accept the exact one-byte and 255-byte lowercase ASCII-slug boundaries; accept internal `.`, `_`, and `-`; and reject empty, 256-byte, leading-punctuation, uppercase, non-ASCII, whitespace, and control-containing values. Configuration and CLI operands use identical validation and exact equality without trimming, case folding, normalization, aliasing, or path conversion.
  - Target-order fixtures permute declaration and worker-completion order and require identical resolved delivery, fingerprints, cache identity, result DTOs, reports, and check comparisons in exact checked-name ASCII-byte order.
  - Output-topology fixtures reject equal and either-direction proper segment-prefix `out` roots across every configured target before CLI selection, regardless of target declaration order or a selecting `--target`; never inspect generated artifact paths to excuse an overlap; and leave distinct exact roots that collapse only under host mapping to the registration collision contract.
    - Multi-conflict fixtures enumerate canonical checked-name pairs lexicographically, return the first conflicting pair with names in that order, retain both original submitted `/out` pointers, and remain invariant under config-array, path-length, prefix-direction, cache, and worker permutations.
  - Exporter-selection fixtures require exactly one string-valued `exporter` per M3 configured target, accept only exact `"esm"`, and reject omission, `null`, other JSON types, case or whitespace variants, aliases, unknown IDs, and inference from target or output fields. They prove one selected ID creates exactly one exporter transaction while custom in-process registry selection remains outside raw CLI configuration.
  - Configured-exporter fixtures construct one immutable typed ESM instance per selected target from the exact link policy's production locales/fallback and that target's canonical eager set; preserve the object-safe batch-only `PlatformExporter` trait; and reject global/environment/config rereads, untyped option bags, target/output/filesystem authority, shared mutable cross-target state, and mismatched policy/batch state. A detected built-in mismatch remains `CapabilityPreflightContract`.
  - Output-root shape fixtures require exactly one string-valued `out` per M3 configured target and reject omission, `null`, every other JSON type, defaults, and inference. They prove that the resolved value belongs to CLI registration rather than exporter I/O and that non-filesystem integrations supply their destination outside this CLI target shape.
  - Output-root namespace fixtures accept only non-empty slash-separated project-root-relative paths; resolve them against the same 006 `projectRoot` independently of current directory and config-file location; and reject leading or trailing separators, repeated separators, empty or navigation segments, absolute, drive-prefixed, UNC, and backslash-containing forms without host-dependent rewriting.
  - Output-root limit fixtures reuse the portable artifact-path segment validator; accept exact 1- and 255-byte segment, 64-segment, and 4,096-total-byte boundaries; and reject the first value above each ceiling, NUL, and every `Cc` scalar. They preserve exact Unicode bytes and reject trimming, case folding, normalization, percent decoding, or host-dependent canonicalization.
  - Output-root filesystem fixtures reject every existing directory symlink, Windows reparse point, and symlinked destination leaf below the trusted project root without following it; create missing real directories only in write mode; create nothing in check mode; and fail the complete target when the platform cannot preserve verified-root-relative no-follow registration. They prove exporters receive no host path or filesystem capability.
  - Output-ownership fixtures reserve exact root path `[".intlify-output-manifest.json"]` for integration metadata and reject an exporter collision before mutation; adopt only an absent or empty real root; reject a non-empty unmanifested root, invalid or unsupported manifest, and every unrecorded file, directory, symlink, reparse point, or special entry without deleting it; and admit only the manifest, its recorded regular files, and their exact real ancestor directories.
    - They prove that a valid manifest records the selected exporter and every canonical artifact path, kind, format version, complete common metadata, payload byte length, and payload fingerprint without entering `ExportArtifactSet` limits; permits repair of a missing or changed owned file; removes a prior recorded path absent from the new set only on successful commit; and never treats a digest as payload or cleanup authority for an unrecorded entry.
  - Output-manifest schema fixtures require exact root `schemaVersion`, `exporter`, and `artifacts`; exact artifact `path`, `kind`, `formatVersion`, `metadata`, and `payload`; exact metadata `mediaType` and `relationships`; exact relationship `kind` then `target`; exact payload `bytes` then `fingerprint`; and exact fingerprint `algorithm` then `digest`.
    - They accept only manifest draft version `0.1`, selected exporter `"esm"`, explicit `null` for absent media type, `"eager_load"` and `"lazy_load"` relationship tags, common plain-decimal counts and versions, exact `"blake3-256"`, and 64 lowercase digest hex. They reject every missing, duplicate, unknown, mistyped, disallowed-null, unordered collection, invalid checked field, reserved manifest artifact path, unsupported version/exporter, incorrect payload length or digest, and inconsistent relationship graph without inferring ownership.
  - Output-manifest canonical-writer fixtures emit UTF-8 without BOM using two-space nesting, one non-empty member or element per line, one space after colon, commas only after non-final values, exact `[]`, LF-only lines, and one final LF; fix every object member and canonical collection order; and emit no trailing whitespace, comments, timestamp, absolute path, environment value, or random data.
    - String fixtures use the exact quote, backslash, five short control escapes, lowercase `\u00xx` remaining-control escapes, and literal remaining-scalar rules. Integer fixtures use shortest unsigned base-10 spelling. Valid noncanonical JSON establishes checked ownership but is rewritten in write mode and differs in check mode.
  - Output-manifest limit fixtures accept exactly 16 MiB of valid UTF-8 wire input/output and reject the first excess, conversion failure, BOM, or invalid UTF-8 before proportional parsing; reapply every artifact/path/metadata/relationship decoded limit without counting payload bytes as embedded; and prove the built-in 1,025-artifact/1,024-relationship cardinality remains independently enforced.
    - Fingerprint fixtures hash the complete exact payload bytes with standard unkeyed BLAKE3-256, preserve the exact byte count, and require actual payload comparison despite equal supplied fingerprints. Manifest-fingerprint fixtures separately hash complete canonical manifest bytes for transaction evidence without confusing either digest domain.
  - Output-control-ID fixtures hash the exact `dev.intlify/output-root-control` NUL-terminated domain, `0.1` big-endian version, checked big-endian segment count, and every checked big-endian length plus exact project-root-relative output segment; preserve case and Unicode distinctions; and emit one 64-character lowercase ID without slash joining, host canonicalization, target name, exporter ID, or current-directory input.
  - Output-lock fixtures use only the exact `.intlify-output-<output-root-id>.lock` real sibling; require canonical exact-`0.1` `{ schemaVersion, out }` content within 64 KiB; create without replacement; lock before initialization or validation; flush first initialization; retain the file after completion; and reject invalid content, special files, symlinks, reparse points, unsupported versions, and unequal decoded output identities.
    - Concurrency fixtures serialize two first or later writers through one exclusive advisory lock, allow check to hold a shared read-only lock, and make an initially absent check detect a persistently appearing lock at its final observation. They reject PID/timeout stale inference and in-process-only substitutes, detect an unequal-output digest collision without reusing the lock, and return `unsupported_capability` when process-releasing no-follow advisory locking cannot be established.
  - Filesystem-transaction fixtures complete mapping, capability preflight, ownership validation, and a same-parent/same-filesystem staging root before commit; serialize target writers; commit through recorded backup-and-rename state; and expose exactly the old or new complete managed root after every ordinary injected failure.
    - Transaction-name fixtures require a cryptographically random 128-bit ID as 32 lowercase hexadecimal characters and derive only the exact output-ID-qualified staging, backup, and journal sibling basenames. They reject timestamps, PIDs, counters, weak randomness, separators, navigation, alternate case, replacement of an existing unrecognized entry, and external paths.
  - Transaction-journal fixtures require exact root `schemaVersion`, `transactionId`, `out`, `staging`, `backup`, `oldRoot`, `newManifestFingerprint`, and `phase` in canonical form within 64 KiB; accept only exact version `0.1`, names recomputed from the two IDs, the three closed old-root shapes, exact canonical-manifest BLAKE3 fingerprints, and phases `staged`, `old_moved`, or `new_installed`.
    - Commit fixtures flush complete staging before durably publishing `staged`; durably publish each old-root rename, `old_moved` journal replacement, new-root rename, `new_installed` replacement, backup removal, and journal removal in order. Journal replacement uses a flushed complete temporary regular file and atomic replacement, so a partial record never becomes recovery authority.
  - Transaction-recovery fixtures inject interruption before and after every flush, rename, journal replacement, and cleanup; compare actual old/new root and exact manifest fingerprints rather than phase alone; roll an intact staged transaction back to old; restore an exact backup when `out` is absent; and retain an exact installed new root even when the durable phase remains `old_moved`.
    - They retain a valid new root and clean its old backup after `new_installed`, restore a complete old backup if the claimed new root is absent, and reject every digest mismatch, unexpected entry, duplicate candidate, missing required backup, invalid derived name, or special file without mutation. A similarly named unjournaled sibling is never cleanup authority.
  - Transaction-durability fixtures require successful content and directory-entry flushes for payloads, manifests, journals, staging, output-parent renames/removals, rollback, and recovery. They restore the prior root before returning `registration_failed`, reject direct-overwrite, partial-prefix, or process-crash-only fallback, and return `unsupported_capability` for missing CSPRNG, same-filesystem placement, durable flush, safe rename/rollback, serialization, or recovery. Concurrent external-reader atomic visibility remains outside the M3 guarantee.
  - Check-mode fixtures execute the same inventory-through-capability-preflight path without creating or updating a root, manifest, lock, staging, backup, or transaction state; compare exact path sets, common metadata, payload bytes, canonical manifest bytes, and prior recorded stale paths; and ignore modification time, owner, and ordinary permissions.
    - They return `0` only for complete equality, `1` for an absent or empty adoptable root and every safely writable owned difference, and `2` for malformed or unsupported ownership state, unsafe/unreadable entries, unowned collisions, or capability failure. Multi-target fixtures give operational `2` precedence over difference `1`, and difference `1` precedence over equality `0`, independently of declaration and completion order.
  - Write-equality fixtures perform the same complete exact comparison after lock, recovery, and ownership validation; report `unchanged` and create no staging, backup, journal, or root mutation only for exact equality; and require a real transaction for noncanonical manifest bytes, stale owned paths, payload or metadata differences, and every other safely writable mismatch.
  - Registration-evidence envelope fixtures require `message_output_registration_failed` only after one valid complete artifact set; require exact `details.kind` followed by one kind-selected closed `details.evidence`; use the configured project-relative output root as top-level `path`; omit absent optionals rather than emitting `null`; and reject absolute/temp paths, random transaction names, lossy names, arbitrary maps, dependency/debug text, source text, and error chains.
    - Evidence-path fixtures use original portable segment arrays, admit mapped or actual relative paths only through the 128-segment/8-KiB evidence bound, and classify an unrepresentable actual entry without fabricating a path.
  - Unsupported-capability evidence fixtures exercise every closed capability token and its exact required artifact-specific fields; reject missing, extra, or mismatched artifact path/kind/version/relationship evidence; and prove static capability preflight does not expose platform probe or library names.
  - Destination-mapping evidence fixtures require one of the four closed mapping reasons plus canonical `artifactPath`; admit `segmentIndex` only for an exact zero-based failing segment and `mappedDestination` only when safely representable; and reject shortening, escaping, suffixing, dropping, or normalizing the artifact to avoid failure.
  - Destination-collision evidence fixtures exercise the exact mapped-path, host-equivalence, reserved-control-path, and output-root-control-ID shapes; canonically order two artifact paths; require only `controlPath: "manifest"` for the initial reserved artifact collision and `"lock"` for an output-root-ID collision; and distinguish the current output root from the unequal decoded lock owner without lexicographic reversal.
  - Registration-failure evidence fixtures accept only the documented stage/reason pairs and subject tokens; admit `control`, expected `artifactPath`, actual `relativePath`, closed `operation`, normalized `ioKind`, and signed `rawOsError` only in their applicable order and combination; and reject null optionals, random control basenames, Rust error-kind names, and branching on `rawOsError`.
    - They keep manifest admission on the ownership stage in check mode, classify appearance of an initially absent lock as check/snapshot change, and classify an ambiguous recovery state without exposing or deleting its participants.
  - Registration-precedence fixtures place failures at every adjacent stage and require capability/manifest-capacity, canonical mapping, collision pair, control identity, lock, recovery, ownership, check-or-staging, then commit order; require kind, format version, media type, then canonical relationships within one artifact; and remain invariant under artifact, directory, worker, hash-map, cache, OS-completion, and reporter permutations.
    - Representable actual entries use exact portable order; non-representable native entries use raw unsigned POSIX bytes or unsigned Windows UTF-16 units. One target emits at most one registration error, stops ordinary later stages, returns the original commit error after successful rollback, and replaces it with the rollback/recovery error when complete-state restoration itself fails.
  - ESM eager-default fixtures require omitted `eagerLocales` and an explicit empty array to produce the same resolved target, fingerprint, cache identity, complete locale-asset set, all-lazy relationships, and check result. They reject inference from locale order, fallback, coverage baseline, host state, environment, or another target.
  - ESM eager-set fixtures accept exact 0- and 1,024-occurrence boundaries; reject the 1,025th occurrence before scalar, membership, or duplicate work; apply the shared 1-through-255-byte locale contract; reject out-of-production-set and duplicate values without deductions or silent filtering; and canonicalize valid permutations by exact locale UTF-8 bytes without treating order as priority.
  - ESM locale-path fixtures hash the exact domain/version/length/locale framing with standard unkeyed BLAKE3-256; emit only `["locales", "locale-" + lowercase_hex + ".mjs"]`; cover empty-forbidden plus 1- and 255-byte locale boundaries, Unicode, whitespace, controls, case distinctions, and normalization distinctions; and never expose, encode, truncate, ordinalize, or reverse-parse locale text in the path.
  - Locale-path collision fixtures retain exact locale keys in the loader map, detect one equal logical path for unequal locales before publication, and fail the complete set as `DuplicateArtifactPath` without winner selection, suffixing, digest extension, or relationship-order dependence.
  - ESM v0.1 representation fixtures preserve exact validated MF2 source together with scope, domain, and canonical key in data-only modules; reject formatting, normalization, generated formatter functions, Binary AST snapshots, embedded runtime/provider behavior, and invocation of `MessageCompilation`; and require an explicit kind/version decision before another representation replaces the `0.1` payload.
  - Fallback-materialization fixtures place every linker-selected resolved message in its requested-locale artifact, retain its exact definition locale as provenance, emit an artifact for an empty plan, charge equal fallback materialization independently, and reject source-locale repartitioning, exporter/runtime fallback search, omitted fallback selections, and loader-driven message-key reselection.
  - Built-in plan-cardinality fixtures produce one canonical `["main"]` plan per production locale even for empty messages, reject empty/missing/duplicate/extra/wrong-unit plan sets as a built-in capability-preflight invariant, retain generic empty-batch validity outside that route, classify an explicitly custom multi-unit use as unsupported rather than flattening it, and require exactly `locales + 1` artifacts plus `locales` loader relationships.
  - Locale-module ABI fixtures emit exactly named `formatVersion`, `deliveryUnit`, `locale`, and `messages` exports with no default; use exact five-position tuples containing explicit project scope namespace/name, domain, canonical key, definition locale, and MF2 source; preserve the fixed canonical record order; and reject duplicate resolved identities, source/debug evidence, object-key catalogs, mutation helpers, or a second payload API.
  - Loader-module ABI fixtures emit one `["loader.mjs"]` `dev.intlify/loader-map` `0.1` artifact with no default; order static eager imports and aliases canonically; emit exact `formatVersion`, canonical `locales`, priority-preserving `fallbacks`, and one `loadLocale` switch case per locale; and return a promise for every branch.
  - Loader behavior fixtures resolve eager modules through `Promise.resolve`, lazy modules through direct dynamic import, and unsupported inputs through a rejected `RangeError("unsupported locale")`; perform no coercion, normalization, input interpolation, or fallback traversal; and require one matching eager/lazy relationship per locale with no reverse edge.
  - Canonical ESM-writer fixtures emit exact ECMAScript 2020 UTF-8 bytes with no BOM, LF-only lines, one final LF, fixed semicolons, two-space indentation, blank-line and no-trailing-comma templates, and double-quoted scalar escaping including control, `U+2028`, and `U+2029` boundaries. They preserve all other scalars and reject formatter, comment, banner, timestamp, source-map, environment, and host-newline influence.
  - ESM metadata fixtures require `text/javascript` without parameters on both exact `0.1` kinds, no relationships on locale modules, and one payload-consistent eager/lazy loader relationship per locale; reject extension/MIME inference, `application/javascript` rewriting, extra metadata, payload/envelope version mismatch, and every relationship/payload classification mismatch as `ArtifactAssemblyState`.
- Delivery-shape fixtures accept omission for non-emit consumers, require 1 through 64 submitted targets whenever `delivery` is present, and reject an empty object, an empty array, and the 65th occurrence without normalizing, truncating, partitioning, or executing a prefix. Count preflight precedes target validation, duplicate detection, exporter resolution, option validation, and proportional work; invalid and duplicate occurrences receive no deduction. Emit fixtures reject absent delivery before inventory and prove that neither `--target` nor `--check` turns it into a zero-target success.
- Multi-target execution fixtures fail every project-global setup stage before target work; then visit every selected target sequentially in canonical checked-name order; create exactly one ordered result entry per target; continue after each target-local export, mapping, registration, check, recovery, rollback, or target-scoped internal error; and never duplicate those errors at top level.
  - Side-effect fixtures retain each earlier complete commit when a later target fails, keep the failed target on its own old or otherwise explicitly reported recovery state, continue later disjoint targets, and never acquire cross-target locks/backups or synthesize command-wide rollback. They give any global or target operational error exit `2`, otherwise any check difference exit `1`, and only complete success/equality exit `0`, independently of future parallel completion.
- M3 command-analysis fixtures insert `analysis` only after a checked `LinkOutcome`, after `summary` and before `results`; require exact `generationBlocked` then canonical `findings` and optional `messageValidation`; omit the complete field rather than emit null or false evidence when no outcome exists; and never add it to another command implicitly.
  - Finding-adapter fixtures emit each linker finding once as exact derived `kind`, exact `LinkFinding::blocking()`, typed canonical `subject`, then typed canonical `evidence`; match summary complete/blocking counts; and reject per-target duplication, lint rule/severity/preset data, human messages, rendered paths, source excerpts, and target identity.
  - Finding-union fixtures require the exact common field order `kind`, `blocking`, `subject`, and `evidence`; use the outer kind to select one exact closed subject/evidence pair; and reject a nullable superset, opaque maps, redundant nested type or version members, internal Rust serialization, and per-record schema versions.
    - They exercise every per-kind codec with omitted allowed optionals and reject missing, duplicate, unknown, mistyped, cross-kind, and disallowed `null` members. Compatibility is owned only by the command envelope's top-level `schemaVersion`.
    - Ambiguity-codec fixtures require exact resolved `scope`, `domain`, `key`, and `locale` subject order; one evidence member containing at least two complete canonical source-then-entry locations; and `blocking: true`. They reject incomplete or truncated collider sets, a selected winner, payload, fingerprint, producer, alias, host path, span, excerpt, and noncanonical location order.
    - Unresolved-codec fixtures require exact artifact-then-ordinal reference identity; exact delivery unit, resolved scope, domain, selector, optional reason/origin, then a non-empty failures array; and `blocking: true`. They require one complete canonical failure vector per reference record, requested locale followed by the exact complete probed chain, omission of successful locales, and omission rather than null for absent optionals.
    - Missing-translation-codec fixtures require one record per reference/requested-locale/resolved-key tuple; exact nested reference, requested locale, and key subject order; exact delivery unit, resolved scope, domain, probed locales, selected locale, and one definition location evidence order; and `blocking: false`.
      - They require the probed vector to begin with the requested locale and end at its unequal first successful locale, require the location's definition locale to equal that selected locale, and reject per-reference aggregation, selector/reason/origin duplication, unprobed fallback suffixes, ambiguous candidate vectors, payload, and a same-locale success.
    - Orphaned-translation-codec fixtures require one record per non-baseline locale definition, exact resolved scope/domain/key/locale subject order, exact baseline locale then one definition location evidence order, unequal subject and baseline locales, and `blocking: false`.
      - They keep equal keys in several non-baseline locales separate; require the subject key to be absent from the complete explicit baseline and the location to equal the subject definition; and reject baseline omission, partial-definition analysis, ambiguity, location aggregation, inferred baselines, payload, and a baseline-locale subject.
    - Unused-message-codec fixtures require one record per locale-bearing definition, exact resolved scope/domain/key/locale subject order, one exact definition-location evidence member, and `blocking: false`.
      - They keep equal unused keys in several locales separate; require both completeness sides closed and ordinary checked-policy unreachability; suppress the finding in a compat-widened scope-domain pair; and reject aggregated locations, empty reference/root vectors, evaluated counts, redundant closure constants, payload, and presentation grouping in the core result.
    - Unbounded-dynamic-codec fixtures require exact artifact-then-ordinal subject order; exact delivery unit, resolved scope, domain, optional reason/origin, then dynamic mode evidence order; and omission rather than null for absent optionals.
      - They admit only `compat` paired with `blocking: false` and complete conservative scope-domain reachability, or `strict` paired with `blocking: true` and no plans. They reject a repeated unbounded selector, inferred mode, retained-definition vectors, guessed keys, producer policy, lint data, and any mode/blocking mismatch.
    - Degraded-analysis-codec fixtures admit only evidence kinds `wide-selector` and `partial-completeness`, require their fixed `0` then `1` variant precedence, select the exact subject shape from that evidence kind, and reject an open-world-participant, generic/custom, unknown, or mismatched variant.
      - Wide-selector fixtures emit one non-blocking record for every reference-record `AllInScope`, require exact reference identity plus kind/delivery-unit/resolved-scope/domain/optional-reason/origin shape, and mark the complete scope-domain pair reachable. They reject configured roots, `Prefix`, `Pattern`, match-count thresholds, repeated selectors, null optionals, and blocking disposition.
      - Partial-completeness fixtures emit one blocking record per resolved scope and partial side, require exact resolved scope then side subject and kind then complete non-empty contributor evidence, and preserve every original pre-mapping scope/reason pair in canonical order.
      - They admit only side-applicable exact reason tokens; keep two partial sides as two findings; and reject empty, duplicate, truncated, or post-mapping-only contributors, operational reports, arbitrary messages, source/producer lists, caches, missing-fact guesses, and non-blocking disposition.
  - Finding-blocking fixtures exercise every row of the initial disposition matrix in both applicable dynamic-reference modes, reference-record `AllInScope`, and both partial-completeness sides.
    - They require `bundle_plans: None` if and only if at least one retained finding is blocking, make one blocking finding block every selected target, and allow only non-blocking findings to retain plans without changing target status, summary status, or exit code by themselves.
    - They prove no independent stored bool, setter, kind-plus-bool constructor, adapter override, or side table exists; reject every conceptual mode/variant mismatch; and require machine output to equal the core method.
    - They prove lint severity, preset, reporter, exporter, target, coverage-baseline selection, environment, and command presentation cannot change `blocking`; no unimplemented strict-coverage switch or dormant alias is accepted.
  - Shared-message-validation fixtures call `prepare_export` exactly once before target exporter construction, reuse one successful borrowed batch across targets, and on failure emit exact retained diagnostics, total count, and truncation once in analysis while blocking every target with empty target diagnostics and zero prepared metrics. They cover retention zero and reject per-target parsing, duplicated counts, empty success objects, and first-diagnostic stopping.
- M3 target-result fixtures emit exact `target`, `exporter`, `out`, `status`, `outputState`, optional paired `artifactCount`/`payloadBytes`, check-only `differences`, `diagnostics`, then `errors` in canonical target-name order; retain configured identities rather than inferred paths; always emit diagnostics/errors arrays; and emit differences only in check mode.
  - Status fixtures admit `written`/`unchanged` only in write mode, `matched`/`different` only in check mode, and `blocked`/`error` in either; require empty diagnostics/errors on ordinary success/difference, no target-local evidence on a command-analysis-blocked target, at least one applicable shared blocking finding or validation failure for `blocked`, and a non-empty error array on `error`.
  - Prepared-metric fixtures require paired exact set count and cumulative payload bytes for written, unchanged, matched, and different results; retain them for a later mapping/registration error; omit both for blocked and pre-set export failure; exclude manifest/control data; charge equal artifacts independently; and reject null, guessed zero, submitted prevalidation lengths, and partial-prefix metrics.
  - Output-state fixtures require one of `unchanged`, `updated`, `restored`, or `indeterminate` on every target result. They classify every non-mutating check/block/pre-registration/staging-only path and exact-write skip as unchanged; classify installed complete expected output as updated even after cleanup error; classify a proven prior-root restoration as restored; and reserve indeterminate for failed rollback/recovery after mutation.
    - They prove output state reports the configured root rather than sibling-control cleanup, allow status error with updated/restored/indeterminate as applicable, never treat unchanged-on-error as proof that pre-existing output was valid, and require every check result to remain unchanged because check mode mutates no registration state.
- Check-difference fixtures emit only the five closed record shapes; collapse absent/empty output to one `output_missing`; combine one path's canonical changed-component subset; order manifest, expected paths, then stale prior paths; and return an empty vector for matched, blocked, and operational-error results.
  - They accept at most 2,051 complete M3 records without truncation, map an impossible excess to the registration invariant, keep unsafe ownership state operational, and expose no payload, generated source, source excerpt, digest, unified diff, timestamp, permission, or owner. The checked summary count equals the sum of complete vectors.
- M3 summary fixtures require the exact operation-specific field orders and shortest unsigned `u64` counters; partition selected targets exactly across write or check status counters plus blocked/error; count prepared targets only after a complete valid artifact set; sum artifact records and payload bytes per target without deduplication; and count shared total diagnostics, command findings, blocking findings, check differences, error targets, and all operational errors in their separate namespaces.
  - They emit every operation-specific counter including zero once target result construction starts, omit unresolved counters rather than inventing zero after earlier project-global failure, retain only safely resolved operation, and enforce error/exit `2` over blocking-or-different failure/exit `1` over success/exit `0`. A successful mutating write remains success.
- Operational-mapping fixtures cover every typed artifact, linker, export-preparation, exporter, and registration branch; require the exact boundary-level code plus `details.kind` and canonical evidence; keep transport I/O on `input_read_failed`; and map every internal variant to its registered unique `internal_error` reason.
  - Semantic-error fixtures preserve the 012-owned kind: `semantic_api_misuse` contains no `stage`, while `semantic_invariant_failed` requires exactly `semantic_model_construction` or `semantic_validation` from the owning call site.
  - Built-in producer-failure fixtures require exact `message_artifact_failed` / `producer_failed` evidence with `producer`, `stage`, `reason`, and primary `source` followed only by permitted optional `span` or paired `limit`/`observed`; accept exactly the closed stage/reason pairings; and reject null optionals, arbitrary parser text, source excerpts, selector spellings, dependency codes, and cross-stage evidence fields.
  - Failure-selection fixtures complete producer stages in declaration order, choose the smallest safe `(span.start, span.end)` and then declared reason order within one source, order failed groups by canonical primary source identity, emit at most one producer error per group, emit no failed artifact, and derive `ProducerFailed` for every recognizer-bound scope independently of glob, alias, recovery, cache, traversal, reporter, and worker order.
  - Producer-invariant fixtures keep impossible AST duplication, invalid parser spans over valid UTF-8, validated-state contradictions, and equivalent impossible output rejection on `internal_error` / `message_producer_invariant_failed`; they never relabel an invariant with an input-facing producer stage.
- Namespace-separation fixtures prove that blocking `LinkFinding` values and ordinary export-gate parser/semantic diagnostics never enter `errors[]`, while an operational failure never becomes a linker or source diagnostic merely to obtain a severity.
- Placement fixtures keep artifact/link errors at the project-transaction top level and exporter/registration errors on the selected target result, require exit `2` for every operational code, and reject first emission before the owning M0, M3, or M5 addendum fixes its remaining bounded evidence and transaction fields.
- M0–M3 CLI-surface fixtures reject plan-dump options and serialized-plan output.
  - Core tests and in-process integration tests inspect the typed `MessageBundlePlan` directly, while M3 `emit` passes it through export preparation without adding a public inspection wire format.
- M0 exposure fixtures exercise the complete internal project-inventory, built-in JS producer, `["main"]` graph, completeness, and linker orchestration in process while proving that an M0-only product publishes no new CLI leaf, starts no editor link session, performs no export or prune, and defers activation unchanged to M3, M5, and L0.
- Coverage-baseline semantic fixtures require an explicit entry for every typed-key generation scope, accept omission of the field or omission of scopes not requesting coverage or types, require at least one scope when the field is present, reject an explicit empty object, reject largest-catalog, discovery-order, fallback-order, and definition-order inference, and reject an unknown scope, an out-of-policy locale, or an incomplete baseline inventory.
  - Baseline-versus-union fixtures accept equal sets and baseline-only keys, reject every non-baseline-locale-only key fail-complete before writing generated output, preserve canonical offending-key order under input and worker permutations, and make M2 expose the same difference as `orphaned-translation` without changing M1 output semantics.
- Typed-key generation fixtures emit explicit scope-bound JS/TS modules and require explicit imports; reject global or ambient `.d.ts` augmentation, cross-scope merging, partial output after baseline failure, and treatment of `useMessageSet` as a generated accessor.
  - Cross-platform fixtures prove the language-neutral model contains no TypeScript module, declaration-merging, path, or runtime-binding assumptions.
- Stub-absence fixtures expose no initial config field, CLI option, lint autofix, placeholder default, destination inference, or partial write for `unresolved-message` or `missing-translation`; the only initial catalog mutation remains the explicitly requested, eligibility-checked `prune` path.
- Lint-gating fixtures resolve rules and severities before producer work; perform no linker invocation when no linker-backed rule is enabled; skip reference-artifact load, source scan, and reference-cache lookup when enabled rules require only definition/policy inputs; and invoke exactly one shared linker orchestration, adding the shared producer/cache path only when one or several enabled rules require references.
  - They prove rule order, severity, preset, reporter, and disabled findings do not alter artifact/cache identity or linker output; `emit` and `prune` remain independent consumers; absent reference production and narrow operands derive typed `Partial` completeness and adapt `degraded-analysis` instead of fabricating absence-dependent results.
- Lint-rollout fixtures make every L0/L1 linker-backed rule `off` and absent from `recommended` initially, require explicit configuration to enable it, apply ordinary 008 severity/quiet/max-warning/exit behavior, and count one linker finding once regardless of related-entry count.
  - They reject implicit enablement from resource config, cache presence, or another linker command and require a deliberate per-rule contract change for later preset promotion.

### Native reference production

- Native-ID fixtures use an exact versioned marker plus one 32-byte BLAKE3 ID and no embedded selector or origin; distinguish equal selectors at distinct logical sites; recompute every dictionary ID; collapse only byte-identical repeated site entries; and reject different canonical inputs with one ID as a collision.
  - Scanner fixtures canonicalize distinct surviving IDs by exact bytes, join every ID against the same-build dictionary, construct one record per ID in that order, leave unobserved entries available but unreachable, and fail complete on unknown ID, missing dictionary, incompatible version, malformed marker, digest mismatch, or conflicting entry without subset artifacts.
  - Object-format fixtures must validate DCE/LTO/COMDAT behavior before N0/N1 promotion and may over-retain only through the explicit degraded available-set fallback, never through unknown-ID skipping.

### Definition artifact envelope and accounting

- Definition-root member fixtures require exactly seven required, non-null M0 members; accept every input member permutation; and canonically emit `kind`, `version`, `producer`, `source`, `logicalAliases`, `inputFingerprint`, then `definitions`.
  - They require `logicalAliases` to be an array even when empty, emit exact `[]` for no aliases, and reject its omission, `null`, mistyped shape, conditional omission, duplicate member, or unknown replacement rather than defaulting an absent member to an empty sequence.
  - Alias-element fixtures accept only the direct `PortableRelativePath` segment-array codec, including literal backslashes and non-normalized Unicode; reject path wrappers, slash/host strings, namespace repetition, `null`, flattened arrays, and non-string segments; and prove primary and alias paths use one identical codec.
- Definition-local precedence fixtures require exact `kind`, supported version, complete producer validation, complete primary identity construction, every alias count/numeric/cumulative/grammar/canonical-set phase, structural fingerprint validation, and only then the `definitions` length preflight and fixed definition field passes after root syntax and duplicate-member checks.
  - They exercise conflicting failures at every adjacent phase: producer wins over source, alias wins over fingerprint, and malformed fingerprint wins over an already-known excessive definitions length.
  - Decoder member order, direct construction, cache revalidation, partitioning, or worker completion cannot reverse that result, and a remote consumer never performs unverifiable digest recomputation.
- Producer-identity accounting fixtures accept the exact 255-byte id and 128-byte revision boundaries and reject the first excess as structural `ProducerIdentity` failures without `LinkLimitCounter` evidence or caller-lowerable budgets.
  - For both artifact kinds they charge the exact decoded id and revision once each to the enclosing decoded total, charge only actual JSON spelling to wire bytes, add zero decoded bytes for object framing/member names, and reject double charging or interning/cache deductions across decoder, producer, direct construction, cache revalidation, and defensive-link paths.
- Definition source-path decoded-accounting fixtures charge every primary and submitted-alias segment occurrence exactly once to `definition_artifact_decoded_bytes`, including occurrences later rejected as duplicate or noncanonical.
  - They prove that per-segment, per-path, and cumulative path-limit observations add no second charge; framing, counts, and namespace discriminants add zero; actual JSON syntax belongs only to wire accounting; repeated storage has no deduction; and decoder, producer, direct-construction, cache, and defensive-link recomputation produce the same total.
- Fingerprint-codec and accounting fixtures require exactly the two non-null `algorithm` and `digest` members, accept both input permutations, and canonically emit `algorithm` then `digest`.
  - They reject missing, duplicate, unknown, mistyped, or `null` members, omitted-algorithm defaulting, a bare digest, and an externally tagged digest.
  - They charge exact input hex spelling and object syntax only to `definition_artifact_wire_bytes`, decode every accepted digest to exactly 32 opaque bytes charged once to `definition_artifact_decoded_bytes`, and charge zero decoded bytes for the fixed algorithm discriminant.
  - They reject 64-byte decoded charging, zero-byte digest charging, dual wire/decoded charging, uppercase or malformed hex, and any accounting difference among decoder, producer, direct construction, cache, and defensive validation.
  - Freshness fixtures permit recomputation only to an owner with the complete canonical inputs and map mismatch to stale/cache-miss handling rather than a linker finding or semantic difference.
- Per-artifact decoded-budget precedence fixtures place one field-specific or shape failure against a decoded-total overrun at every adjacent canonical phase for both artifact kinds.
  - They require the current phase's field check before its decoded addition, an admitted payload's decoded overrun before every later phase, and every unfinished earlier phase before that overrun, independently of JSON member order.
  - Streaming fixtures provisionally observe an overrun, retain no payload beyond the effective budget, continue only the bounded syntax/earlier-phase scan needed to select the public result, and produce the same failure as producer, direct-construction, cache, defensive, partitioned, and parallel routes without allocating past the decoded ceiling.
- Per-artifact limit-contract fixtures require the forty-variant closed set with `ReferenceArtifactWireBytes`, `ReferenceArtifactDecodedBytes`, `DefinitionArtifactWireBytes`, and `DefinitionArtifactDecodedBytes` unchanged at ordinals 32 through 35, followed by the linker-result counters at ordinals 36 through 40, all with their exact snake-case spellings.
  - Contract-boundary failures use subject-free `ArtifactContractError::Limit`; linker lower-budget revalidation permits only the matching decoded counter with its established artifact-group subject, wire counters are unconstructible in `LinkLimitEvidence`, and every finding- or plan-result counter is unconstructible at every artifact boundary.
  - Wire observations are exactly `Exact(effective_limit + 1)` and decoded observations are the exact attempted running total after one complete admitted payload; none of the four permits `ArithmeticOverflow`.
  - Wire-precedence fixtures make known-length and every stream chunking return wire overrun before syntax, stop at the first excess byte, return syntax only after bounded at-or-below-limit EOF, and give direct typed construction no wire phase.

### Shared artifact errors and codecs

- Artifact-contract error fixtures admit exactly `InvalidArtifact`, `UnsupportedVersion`, and `Limit`.
  - Violation-code fixtures admit exactly the sixteen declared codes and exercise every row of the normative classification table, including valid-UTF-8 forbidden JSON forms, root-complete trailing data, `null` before generic type mismatch, non-string versus wrong-string kind, unknown closed tags, semantic order versus member order, object-member versus value duplicates, cross-field inconsistency, and occurrence discontinuity.
  - They preserve the owning canonical validation precedence under conflicting failures and reject parser-native categories, free-form messages, arbitrary codes, `Other`, `Custom`, source chains, and input-derived internal-invariant errors.
  - Version fixtures distinguish a malformed version as `InvalidArtifact` from a structurally valid unsupported pair/range as `UnsupportedVersion`; resource fixtures use only `Limit`.
- Artifact-violation location fixtures admit exactly the declared artifact-kind-tagged envelope leaf enums, three path roles, ten definition fields, seventeen reference fields, and the seven location shapes with ceiling-bounded `u32` ordinals.
  - They reject every cross-artifact envelope field, undeclared path role or field, whole-path failure mislabeled as a segment, whole-alias failure with a fabricated segment, and known-leaf failure collapsed to its container.
  - They map missing or unknown fields without a valid identity to the containing object, map unpositioned UTF-8/JSON failures to root, and reject raw unknown names, rejected values, JSON Pointers, byte offsets, line/column, excerpts, host paths, unbounded vectors, and route-specific optional detail.
  - Decoder, constructor, cache, defensive, member-permutation, escape, whitespace, slice, and reader fixtures select identical semantic code/location pairs, and enum order never changes failure precedence.
- Structured artifact-error adapter fixtures require the exact internally tagged top-level shapes, canonical member orders, three error-kind tokens, sixteen violation-code tokens, exact/stable-range version-support shapes, all forty counter tokens, and exact/arithmetic-overflow observation shapes.
  - They accept every input object-member permutation but reject missing, duplicate, unknown, or required `null` members, Rust enum ordinals, alternate casing, display messages, parser errors, and source chains; canonical round trips reproduce the exact examples above.
- Artifact-version-support fixtures admit only the Rust/API names `Exact` and `StableRange` and the structured tags `exact` and `stable_range`; reject `DraftExact`, `Stable`, `Compatible`, aliases, alternate casing, and raw strings; and require the checked evidence/support-table boundary to pair `Exact` only with major zero and `StableRange` only with a nonzero stable major.
  - Exact draft and stable range acceptance follow the same negotiation result in direct construction, both typed decoders, both encoders, cache revalidation, and linker admission.
- Structured location fixtures require every envelope container and leaf to carry exactly one `reference` or `definition` artifact tag.
  - They cover all seven exact location objects, three role tokens, both envelope-field vocabularies, ten definition-field tokens, and seventeen reference-field tokens; require the declared camel-case ordinal member names; and require absent optional `field` or `segmentOrdinal` members to be omitted rather than encoded as `null`.
  - They reject a kindless envelope container, cross-kind field, raw unknown member name, alternate tag/case, additional detail, and any canonical key order different from the one fixed above.
- Artifact-schema precedence fixtures permute root and nested members and combine failures at every adjacent phase.
  - They require wire admission; complete syntax/trailing-data; canonical root duplicates; `kind`; bootstrap version shape/integer; version compatibility; canonical required-root presence preflight; root unknown members; and remaining envelope phases in exactly that order.
  - They prove a structurally valid unsupported newer version wins over fields unknown to the unselected schema.
  - For every nested object they require canonical duplicate selection, canonical required presence, unknown members, then canonical present-member `null`/type/value/consistency checks; optional members are excluded from presence preflight, and parser discovery, hash-map order, streaming chunks, constructors, caches, and worker completion cannot alter the public code/location pair.
- Artifact-construction fixtures require private fields and read-only accessors for artifacts, records, identities, and scalar newtypes; compile-fail for public literals, mutation, setters, unchecked `From`, borrowed scratch storage, and partial builders; and exercise the complete `try_new` boundaries at every structural, cross-record, decoded-accounting, and lower-limit phase.
  - Reference and definition sequence order is preserved, failed construction exposes no artifact or resumable state, decoder/producer/direct routes select identical semantic failures, private storage choice is unobservable, and external `Arc` sharing cannot alter identity, equality, accounting, or validation.
- Canonical-encoder fixtures run both artifact kinds through exact/current lower limits and require one complete owned box, the selected schema's exact canonical bytes, no final newline, exact-boundary wire acceptance, first-over wire evidence, deterministic decoded/structural revalidation, and decode-encode-decode equality.
  - Compile-fail/API fixtures reject `Write`, sink/callback/buffer/iterator inputs, partial counts, resumable state, `ArtifactWriteError`, and any partial returned document.
  - Counting and emission lengths must agree; filesystem/transport failure tests remain integration-owned and cannot change the completed encode result.
- `LinkLimits` construction fixtures require private immutable effective values, exact protocol defaults, zero and exact-ceiling acceptance for every closed counter, first-above-ceiling rejection without clamping or original-value mutation, deterministic replacement by successive valid programmatic calls, and a configuration error containing only the counter and submitted value.
  - Compile-fail/API fixtures reject aggregate literals, public field mutation, unchecked/lossy deserialization, post-construction setters, and every route that supplies an invalid `&LinkLimits` to a producer, decoder, cache, or linker.
  - Conflicting invalid configuration and artifact bytes produce only the earlier configuration result and perform zero artifact reads.
- Lower-limit reuse fixtures construct and decode artifacts under several valid budgets, cache them, and re-admit them under stricter, equal, and more permissive budgets.
  - They require current-limit canonical revalidation on every route, current effective-limit evidence, no retained/adopted previous limit, no limit-dependent artifact equality/bytes/fingerprint, no synthetic wire charge, and equivalence between recomputation and a complete revision-matched accounting summary.
  - They reject a prior pass/fail bit, previous limit, or lone artifact total as reuse proof; an absent or stale summary deterministically recomputes every observable decoded/structural phase.
- Artifact-decoder entry-point fixtures run slice and every reader chunking through one incremental state machine for both artifact kinds and require typed/generic dispatch equivalence.
  - Slice fixtures use exact length preflight and one-complete-document semantics.
  - Reader fixtures neither seek nor close, refuse early success at root completion, consume permitted trailing whitespace through EOF, classify later non-whitespace or another root as `TrailingData`, and stop at the first wire excess.
  - They prove success leaves the reader at EOF, an externally framed `std::io::Take` leaves its outer reader immediately after that frame, wire failure consumes exactly `effective_limit + 1` bytes, and transport failure leaves the failing-read position.
  - They reject built-in concatenated JSON, NDJSON, root-array batch, length-prefix, and container framing.
  - A transport failure before EOF wins over every provisional contract result, while an already selected wire overrun performs no further read and wins over a hypothetical later transport failure.
  - They keep `ArtifactReadError::Transport` outside artifact/linker evidence, provide no async runtime or decompressor in `intlify_contract`, and require external async/no-`std` adapters to preserve the same bounded semantics.
- Reader-call fixtures inject every positive short-read boundary and arbitrary consecutive `Interrupted` results without changing state or output; treat `Ok(0)` from the decoder's always-nonempty buffer as EOF; and map `WouldBlock`, `TimedOut`, and every other non-interruption error immediately to `Transport`.
  - They prove the decoder never calls with an empty buffer, never polls/sleeps/spins on a nonblocking reader, makes no call after wire overrun, counts each positive byte exactly once, and produces the same result as slice decoding for the same complete byte sequence.

### Aggregate limits and pattern work

- Reference-artifact byte-limit fixtures independently accept exactly 512 MiB and reject the first byte above for `reference_artifact_wire_bytes`, and accept exactly 256 MiB and reject the first decoded byte above for `reference_artifact_decoded_bytes`.
  - Cover compact wire with expanded decoded payloads and heavily escaped or framed wire with smaller decoded payloads.
  - Charge every enumerated variable-width occurrence without reuse deductions.
  - Add zero decoded bytes for fixed integers, enum or discriminant values, and framing.
  - Preflight known lengths and count streaming or invalid trailing input.
  - Enforce decoded limits on direct typed construction without synthetic wire charge.
  - Keep definition and reference budgets distinct.
  - Apply independent zero or lower caller values.
  - Return no partial artifact or cache admission.
- Reference-artifact aggregate-byte ownership fixtures require `reference_artifact_decoded_bytes_total` to charge every submitted typed artifact occurrence exactly once under decoded, direct-construction, cached, sequential, partitioned, and parallel admission paths.
  - They accept zero and exactly 1 GiB, reject the first byte above, prove four exact-256-MiB artifact charges fit while any positive fifth charge does not, apply independent caller-selected lower values including zero, give duplicate or later-rejected artifacts no deduction, and fail the complete request on checked `u64` or host-size conversion overflow.
  - Precedence fixtures require collection-count preflight, complete validation of every artifact, a complete identity-byte aggregate pass, a complete record-count aggregate pass, a complete decoded-byte aggregate pass, and only then duplicate/cross-artifact checks, index construction, and semantics.
  - They prove that a later counter crossing at an earlier identity cannot beat an earlier counter, the passes never interleave, reduction stops at the first over-limit canonical group, and equal-identity permutations and racing workers select the serial canonical counter, group, and attempted value.
  - Aggregate-error subject fixtures retain exactly the canonical `ReferenceArtifactIdentity` group that selected any of the three aggregate failures, use no arbitrary duplicate occurrence or unbounded contributing vector, omit occurrence/producer/delivery/record/payload details, and give the collection-count preflight no fabricated identity.

#### Operational-limit evidence fixtures

- **Counter vocabulary:** require all forty closed counter variants and the exact snake-case adapter spellings defined in [Link limit evidence](#link-limit-evidence).
  - The list includes `locale_bytes`, `entry_structural_path_bytes`, `catalog_key_bytes`, `message_bytes`, `total_message_bytes`, `catalog_scope_name_bytes`, and `scope_mapping_entries`.
  - Selector and pattern spellings include `selector_path_bytes`, `selector_pattern_bytes`, `selector_pattern_tokens`, `pattern_match_states_total`, and `reason_bytes`.
  - Path spellings include `path_segments`, `path_segment_bytes`, `path_bytes`, `logical_aliases`, and `source_path_bytes`.
  - Artifact-byte spellings include `reference_artifact_wire_bytes`, `reference_artifact_decoded_bytes`, `definition_artifact_wire_bytes`, and `definition_artifact_decoded_bytes`.
  - Result spellings include `findings_total`, `finding_bytes_total`, `bundle_plans_total`, `resolved_messages_total`, and `bundle_plan_bytes_total`.
  - Reject unknown, custom, or raw counter values.
- **Subject vocabulary:** enforce compatibility with `Request`, `DefinitionArtifactEnvelope`, `ReferenceArtifactGroup`, `DefinitionArtifactGroup`, `DeliveryGraph`, `DeliveryUnitGroup`, `ResolvedPolicy`, `FallbackSource`, and `ScopeMappings`.
- **Effective limit:** retain the invocation's exact effective lower or default limit.
- **Locale subjects:** permit `LocaleBytes` only with `DefinitionArtifactGroup` or `ResolvedPolicy`.
- **Definition subjects:** permit `EntryStructuralPathBytes`, `CatalogKeyBytes`, `MessageBytes`, and `TotalMessageBytes` only with `DefinitionArtifactGroup`.
- **Scope subjects:** permit `CatalogScopeNameBytes` only with `ReferenceArtifactGroup`, `DefinitionArtifactGroup`, `ResolvedPolicy`, or `ScopeMappings`, according to the owning context.
  - Permit `ScopeMappingEntries` only with `ScopeMappings`.
- **Reference subjects:** permit `SelectorPathBytes`, `SelectorPatternBytes`, `SelectorPatternTokens`, and `ReasonBytes` only with `ReferenceArtifactGroup`.
- **Path subjects:** permit the three path counters only with the subject that owns the path.
  - Use `DefinitionArtifactEnvelope` for a primary definition path.
  - Use `DefinitionArtifactGroup` for an alias.
  - Use `ReferenceArtifactGroup` for an origin.
  - Permit `LogicalAliases` and `SourcePathBytes` only with `DefinitionArtifactGroup`.
- **Pattern-work subject:** permit `PatternMatchStatesTotal` only with `Request`.
- **Finding-result subjects:** permit `FindingsTotal` and `FindingBytesTotal` only with `Request`; reject finding kind, subject, evidence, canonical position, blocking disposition, target, and reporter detail.
- **Plan-result subjects:** permit `BundlePlansTotal`, `ResolvedMessagesTotal`, and `BundlePlanBytesTotal` only with `Request`; reject delivery unit, locale, message, definition, payload, plan position, exporter, target, worker, and reporter detail.
- **Per-value observations:** require the ten per-value field, selector, and path-segment counters, plus `ScopeMappingEntries`, `PathSegments`, and `LogicalAliases`, to carry exactly `Exact(effective_limit + 1)` and never `ArithmeticOverflow`.
- **Running-sum observations:** require `PathBytes`, `SourcePathBytes`, and `TotalMessageBytes` to carry their exact checked attempted running sums and never `ArithmeticOverflow`.
- **Pattern-work observations:** require `PatternMatchStatesTotal` to carry the exact attempted total after one complete evaluation, never `effective_limit + 1` or `ArithmeticOverflow`.
- **Finding-result observations:** require `FindingsTotal` to carry exactly `Exact(effective_limit + 1)` and `FindingBytesTotal` to carry the exact first attempted semantic-payload running total; neither admits `ArithmeticOverflow`.
- **Plan-result observations:** require `BundlePlansTotal` and `ResolvedMessagesTotal` to carry exactly `Exact(effective_limit + 1)` and `BundlePlanBytesTotal` to carry the exact first attempted semantic-payload running total; none admits `ArithmeticOverflow`.
- **Other observations:** admit only `Exact(attempted > effective_limit)` or a true `ArithmeticOverflow`.
- **Excluded evidence:** reject raw field, locale, message, scope, selector, pattern, token, reason, path, segment, alias, candidate, and matcher-state values.
  - Also reject individual mappings or endpoints; `CatalogScopeId`; `EntryReference`; alias, record, mapping, definition, or occurrence indexes and ordinals; complete mapping, value, token, path, or alias counts and lengths; saturation; wrapping; `u64::MAX` substitution; decimal or big-integer alternatives; protocol-ceiling duplication; remaining budget; allocation data; and configured-override copies.
- **Transport:** independently enforce each document's wire ceiling and an integration-owned bounded decompressed batch or stream budget.
  - Prove that `LinkLimits`, linker admission, and cache keys expose no `reference_artifact_wire_bytes_total` and synthesize no wire charge from canonical re-encoding.
- Definition-request aggregate-limit fixtures accept an empty collection and the exact `65,536` artifact, `4,000,000` definition-record, and 1 GiB decoded-byte boundaries, and reject the first occurrence or byte above each effective limit fail-complete.
  - Cover zero and independent lower caller values.
  - Allow zero-definition artifacts when `definitions_total` is zero.
  - Require an empty collection when `definition_artifacts` or `definition_artifact_decoded_bytes_total` is zero.
  - Prove that four exact-256-MiB charges fit while any positive fifth charge does not.
  - Charge duplicate or later-rejected occurrences without sorting, filtering, deduplication, interning, cache, partition, or parallel deductions.
  - Reject checked `u64` and host-size conversion overflow without a partial index or result.
  - Direct-construction fixtures recompute every observable decoded charge exactly once without synthetic wire bytes.
  - Transport fixtures independently enforce every document's `definition_artifact_wire_bytes` and an integration-owned bounded decompressed batch/stream budget, while proving that `LinkLimits`, linker admission, artifact/fingerprint identity, and cache keys expose no `definition_artifact_wire_bytes_total` and synthesize no aggregate wire charge from canonical re-encoding.
  - Precedence fixtures require artifact-count preflight, complete validation of every artifact, a complete definition-record aggregate pass, a complete decoded-byte aggregate pass, and only then duplicate/cross-artifact checks, index construction, and semantics.
  - Canonical-reduction fixtures group by exact `SourceDocumentIdentity`, charge every equal-identity occurrence into counter-specific subtotals, stop at the first over-limit canonical group, and require input permutations and racing workers to select the serial counter, group, and attempted value.
  - Error-subject fixtures use `Request` only for collection count and exactly one `DefinitionArtifactGroup` for either aggregate failure, with no arbitrary occurrence or unbounded contributing vector.
- M0 resolved-policy numeric-limit fixtures accept exactly 1,024 production-locale occurrences and exactly 4,096 configured root occurrences and reject each first-over count fail-complete before proportional work or storage. They reject raw `fallback` and expose no fallback-bearing typed field, limit override, or emitted fallback-counter evidence; the two reserved common counter variants remain unconstructible until M2.
  - Root-default fixtures require omitted `roots` and an explicit empty array to produce the same immutable policy, fingerprint, cache identity, findings, and plans. They prove that neither form infers a root from catalog definitions, producer declarations, or an empty reference-artifact set.
  - M2 fixtures additionally accept exactly 1,024 fallback-source occurrences and exactly 64 target occurrences in one source locale's fallback sequence and reject each first-over count fail-complete before proportional work or storage.
  - Locale-value fixtures accept exact 1-byte and 255-byte occurrences; reject empty and 256-byte values; preserve case, whitespace, control scalars, and canonically equivalent Unicode as distinct exact identities without BCP 47 validation or rewriting; and charge each occurrence independently to enclosing decoded accounting.
  - M0/M1 fixtures prove the 1,024-occurrence and 261,120-byte production-only derived maxima. M2 fixtures prove the 67,584-occurrence and 17,233,920-byte production-plus-fallback maxima. Both require no `policy_locale_bytes_total` counter, lower override, or aggregate-evidence variant.
  - Every milestone rejects an empty production-locale set. M2 accepts an omitted fallback sequence as no fallback and rejects every fallback source or target outside the declared production-locale set without inference or silent removal.
  - Accounting fixtures preflight every admitted collection length; charge duplicate, malformed, out-of-set, and later-rejected occurrences without overwrite, sorting, filtering, deduplication, interning, cache, partition, or parallel deductions; require duplicate-member detection before map materialization and occurrence-preserving direct construction; and apply independent active lower budgets.
  - M2 fixtures require the distinct `fallback_sources` counter, derive 65,536 as the structural target maximum, and prove that M2 exposes no `fallback_entries_total` counter or lower override.
  - Every milestone's semantic fixtures reject duplicate checked production locales and roots with equal checked `(scope, domain, selector)` regardless of reason without first/last-wins, normalization, reason selection, or merging. M2 additionally rejects duplicate checked fallback sources, a source appearing in its own sequence, and a repeated target within one sequence.
  - M2 fallback-order fixtures prepend the source exactly once, preserve the complete declared target order, never recursively expand another source's sequence, and accept reciprocal source arrays as separate finite chains.
  - Counter-registry fixtures retain the exact reserved `fallback_sources` and `fallback_targets_per_source` spellings from M0 so later ordinals remain stable, while proving that M0/M1 resolved-policy construction, lower-limit selection, and operational evidence can use only `production_locales` and `configured_roots`. M2 activates the two reserved spellings; every stage rejects policy-prefixed, config-shaped, aggregate-fallback, alias, and custom alternatives.
  - M2 empty-chain fixtures charge the source and zero targets before rejecting an explicit empty member and accept omission as the only no-fallback form.
  - M0 canonical-order fixtures permute locale, root, and configuration order and require identical checked policies by exact checked-locale/root order. M2 extends them to fallback-source and map enumeration order while proving fallback target permutations remain distinct and are never sorted.
  - M0 admission-precedence fixtures require production/root preflights, the complete production `locale_bytes` pass and validation, root validation, then remaining M0 policy semantics. Beginning with M2, fixtures require this order:
    1. the `production_locales`, `fallback_sources`, and `configured_roots` outer preflights;
    2. a complete production `locale_bytes` pass, followed by remaining production validation;
    3. a complete fallback-source `locale_bytes` pass, followed by remaining source validation with equal-source rejection;
    4. one complete target-count pass in canonical checked-source order;
    5. a complete target `locale_bytes` pass in canonical source and declared target-priority order;
    6. target semantics;
    7. root validation; and
    8. remaining policy semantics.
  - They require every earlier phase to win over a later failure regardless of submitted-member or worker order.
  - M2 duplicate-plus-overrun fixtures require an equal checked fallback-source error to win over either duplicate occurrence's target overrun while proving bounded decoding.
  - Subject fixtures use `ResolvedPolicy` for each outer count and every policy `LocaleBytes` failure.
    - Beginning with M2, use the exact canonical checked `FallbackSource(locale)` only for a target-list count overrun.
    - Require every locale-byte observation to equal `Exact(effective_limit + 1)` without full-length scanning or `ArithmeticOverflow`.
    - Reject every mismatched pair.
    - Retain no occurrence index, target vector or locale, configuration order, or worker identity.
  - Sequential, partitioned, and parallel paths must select the same counter, subject, effective limit, and observation.

- Pattern candidate-set and ordering fixtures form one set per `Pattern` reference record from the distinct canonical keys in its exact resolved scope-domain pair, evaluate each key once across locale, source, entry, and ambiguity duplicates, and produce zero evaluations for an empty pair.
  - They order records by canonical `ReferenceRecordIdentity` and then candidates by exact canonical `CatalogKey` bytes, charge equal records independently, never multiply evaluations for repeated definition occurrences, and require concurrent execution to reproduce the same accounting and first attempted excess as the serial canonical order.

- Pattern aggregate-counter fixtures require the twenty-fifth `PatternMatchStatesTotal` variant, exact `pattern_match_states_total` spelling, and `Request` subject.
  - They start the counter at zero per checked request, atomically add each complete exact evaluation count in canonical order with checked `u64` arithmetic, never reset at inner boundaries, and charge no non-`Pattern` selector or empty candidate set.
  - They accept exactly 100,000,000 states and reject the first complete evaluation whose attempted total is greater: 757 maximum-cost evaluations reach 99,998,186 and a 758th maximum-cost evaluation attempts 100,130,284.
  - They prove the ceiling is not an evaluation-count limit, the observation is the exact attempted running total rather than `effective_limit + 1`, and `ArithmeticOverflow` is unconstructible because no attempted total exceeds 100,132,098.
  - They reject artifact, record, candidate, pattern, matcher-state, and worker evidence; apply zero and lower immutable caller values; select the same first rejected evaluation and total deterministically; return no partial outcome or relaxed retry; and prove cache, matcher, and parallel reuse provide no deduction.
  - Evolution fixtures reject any same-version change; permit an increase only under a newer compatible minor, with exact opt-in while major is zero; require a breaking major for a reduction or any name, unit, candidate-set, order, reset, or deduction change; and prove a caller lower value changes request/cache admission without changing artifact versions or bytes.

### Export error contract

- Export-error-precedence fixtures map `InternalInvariant`, `UnsupportedBatch`, `GenerationFailed`, `OutputLimitExceeded`, and `InvalidOutput` to explicit comparison values `0..=4`; exercise every pair and multi-kind permutation under sequential and concurrent discovery; and then apply the selected kind's own evidence precedence.
  - They prove enum declaration/discriminant, wire spelling, severity, exit code, validation scheduling, hash-map iteration, cancellation, and worker completion do not affect selection.
  - Cross-exporter fixtures keep separate invocations separate and require integration-owned fail-complete aggregation without synthetic merged `ExportError` evidence.
- Export-error-union fixtures prove `ExportError` stores exactly one `ExportErrorEvidence`, derives the same-named `ExportErrorKind`, and exposes only read-only kind/evidence access.
  - Each of the five public variant constructors accepts only its matching checked evidence; built-in, third-party Rust, and non-Rust adapter paths produce the same union.
  - Compile-fail and adapter fixtures reject separately stored or supplied kind fields, mismatched pairs, absent or multiple evidence, generic/default/tuple/unsafe constructors, mutable setters, independent-pair deserialization, `From` pair shortcuts, unknown/custom variants, preserved unknown payloads, and adapter relabeling; evolution fixtures require coordinated kind/evidence/mapping/constructor/presentation/adapter/fixture changes.
- Internal-invariant-violation fixtures admit exactly `ValidatedBatchContract`, `CapabilityPreflightContract`, `GenerationState`, `OutputBudgetAccounting`, `ArtifactAssemblyState`, and `SharedConstructorState`; preserve declaration precedence under sequential and concurrent multi-detection permutations; and exercise each stated impossible state.
  - They keep preparation failures, known unsupported requirements, ordinary generation failures, actual limit/overflow failures, invalid candidate records/sets, and ordinary constructor rejection in their respective error kinds; reject generic bug/assertion/unreachable/adapter/custom variants and reinterpretation of existing meanings; and require a public compatibility change for a new common invariant class.
  - `ValidatedBatchContract` fixtures cover duplicate plan coordinates, same-plan logical-message collisions, unequal snapshots for one equal `DefinitionLocation`, and noncanonical plan/message order after successful preparation. They reject external definition-artifact lookup, zero/multiple artifact resolution, and ordinary preparation failure as meanings of this variant.
- Internal-invariant evidence fixtures require exactly one private, checked, closed invariant and derive one static presentation message.
  - Reject candidate-data violations, resource limits, unsupported requirements, ordinary generation failures, and generic third-party errors.
  - Reject arbitrary messages or codes, chains, panic or exception objects, backtraces, thread IDs, locations, indexes, rejected values, implementation state, and secondary invariant vectors.
  - Return no candidate or partial set.
  - Runtime-safety fixtures do not require panic, OOM, abort, or foreign-exception capture, while explicit multi-invariant detections select fixed precedence under sequential and concurrent permutations.
- Invalid-output-violation fixtures admit exactly the fourteen declared variants and preserve declaration precedence under every applicable multi-violation permutation.
  - They cover root/payload envelope shape, logical and target path grammar, duplicate artifact identities, kind grammar, malformed/out-of-range format versions, metadata shape, media grammar, relationship shape/kind/target resolution, duplicate and conflicting edges, self-edges, and lazy-to-eager-closure conflicts.
  - Classify byte and count ceilings as `OutputLimitExceeded`.
  - Accept unknown valid kinds, unregistered valid media types, canonical empty metadata, constructor-canonicalized ordering, and allowed cycles.
  - Route unsupported valid kind or version, destination collision, unsupported valid cycles, and registration to integration errors.
  - Route exporter-internal impossible state to `InternalInvariant`.
  - Reject reordered or reinterpreted variants and `Other`, `Unknown`, custom, platform, or generic structural variants.
- Invalid-output evidence fixtures require exactly one closed violation with private checked construction and read-only access.
  - Derive presentation from the violation.
  - Reject raw or display paths, invalid substrings or characters, kind or media spellings, payload or metadata excerpts, relationship endpoints, indexes, and spans.
  - Reject free-form reasons, arbitrary codes, nested errors, optional locations, and exporter or platform detail.
  - Prove that structured adapters, typed constructors, built-in exporters, and third-party exporters expose the same shape.
  - Multi-violation permutations select fixed violation precedence rather than discovery, input, hash-map, or worker order and return neither rejected values nor secondary violations.
- Output-limit-counter fixtures admit exactly the thirteen declared variants and exact inclusive ceiling values; preserve declaration precedence under every multi-counter violation permutation; choose `ArithmeticOverflow` over exact observations and otherwise the greatest exact attempt for one counter; and prove sequential, streaming, and concurrent validation produce identical evidence.
  - They reuse the three per-path counters for relationship targets while keeping artifact logical-path and relationship-target cumulative bytes separate; classify kind/media overlength as a limit but invalid grammar and malformed/out-of-range format versions as `InvalidOutput`; and reject reordered, unknown, generic, configured, lower-budget, platform, and custom counters or changed existing ceilings.

### Export preparation and output artifacts

- Crate-boundary fixtures keep `intlify_linker` dependent on `intlify_contract` but independent of `ox_mf2_parser`, `intlify_export`, and `intlify_cli`.
  - `intlify_export` depends on `intlify_linker`, `intlify_contract`, and `ox_mf2_parser` and owns shared preparation, mapped diagnostics, validated batches, common exporter contracts, and the built-in ESM exporter.
  - `intlify_cli` depends on `intlify_export`, `intlify_linker`, `intlify_contract`, and `intlify_resource`; it owns the built-in target registry, typed factory wiring, registration, and orchestration.
  - A non-CLI Rust integration fixture depends on `intlify_export` and invokes the shared preparation and exporter contracts without importing `intlify_cli`.
  - Workspace ownership does not imply crates.io publication or make CLI-owned registry/factory types part of `intlify_export`.
- Message-payload JSON fixtures accept empty and non-empty Unicode-scalar strings, preserve exact decoded UTF-8 across equivalent escapes without normalization or newline conversion, canonically escape the outer JSON, and reject object, array, number, boolean, and `null` forms.
  - Projection fixtures compare the payload byte-for-byte with the admitted 013 `message_text` and exercise overlapping per-message, per-source-total, decoded-artifact, and actual-wire budgets.
  - No M0 fixture accepts an AST, snapshot, compiled payload, raw host string, fallback, or format tag in place of the string.
  - Structural artifact and linker fixtures retain syntax-invalid and syntax-valid-but-semantically-invalid payloads as definitions and prove that key findings are unchanged apart from their normal source evidence.
- Export-gate fixtures construct the stable-identity union of definitions selected across all valid plans and run the shared parse, `build_semantic_model`, and `validate_semantics` pipeline once per selected record over exact decoded payloads before any exporter call.
  - Parser-diagnostic records skip SemanticModel construction and semantic validation. Parser-clean records run both semantic phases, including records that return ordinary semantic diagnostics.
  - They cover repeated placement/locale/unit references, equal text at distinct identities, optional byte-identical parse-and-semantic-result reuse with record-specific evidence, `None` plans skipping the gate, `Some(Vec::new())` passing an empty gate, and unused invalid definitions not blocking a disjoint output.
  - They preserve stable record order, parser order or semantic order within a record, category, and exact `DefinitionLocation` mapping; prove reused and non-reused computation within one call observationally equivalent; and exercise invalid built-in and third-party artifacts, exactly one selected exporter per transaction, an exporter returning multiple artifact kinds, direct/custom integrations, and `--check`.
  - Cross-call fixtures invoke `prepare_export` repeatedly over equal and changed outcomes and require a new complete validation operation each time, with no process-global, persistent, serialized-proof, location-only, or prior-batch reuse.
  - Limit-type fixtures require private immutable `ExportValidationLimits`, make `MAX_DIAGNOSTIC_RETENTION` equal exactly `10,000`, make `protocol_defaults()` and `Default` select exactly `1,000`, accept every `u32` value from zero through the maximum, and reject the first value above it through `ExportValidationLimitConfigurationError` without clamping or changing the original value.
  - Configuration-error fixtures store only the submitted `u32` and expose exactly `submitted()`; compile-fail fixtures reject public construction, setters, deserialization, a duplicated maximum/counter, and presentation storage.
  - Direct Rust API fixtures prevent an above-ceiling `u32` from reaching `prepare_export` and return the exact `ExportValidationLimitConfigurationError`.
  - Initial M3 CLI fixtures always construct the fixed `1,000` value and cannot emit either that configuration error or `invalid_options` for diagnostic retention.
  - Future CLI controls and custom raw-input adapters must add conformance fixtures for their own admitted numeric domain, decoding failure, checked above-ceiling failure, and structured error mapping before exposing a raw retention value.
  - Retention fixtures continue parsing every selected record after the retention limit is full, retain only the bounded deterministic prefix, mapping-validate and count all later diagnostics with checked arithmetic, and expose an exact total plus truncation state.
  - Initial M3 CLI retention fixtures always supply exactly `1,000`; expose no command option, config field, environment variable, reporter override, target override, or worker-derived override; and still parse every selected record to compute the exact total.
    - JSON output exposes the retained prefix, exact total, and derived truncation state. Text output reports the exact omitted count when truncated. Separate programmatic fixtures retain the checked zero and `10,000` boundary behavior without changing the CLI contract.
  - Failure-shape fixtures require private immutable fields and the exact `diagnostics()`, `total_diagnostics()`, and `truncated()` read-only accessors; cover nonzero-total construction, an empty retained prefix at limit zero, exact-limit and truncated results, checked length conversion, the `total_diagnostics >= diagnostics.len()` invariant, equivalence of `truncated` with a positive derived omitted count, and camel-case adapter field names without redundant omitted-count or limit fields.
    - Compile-fail fixtures reject public construction, struct literals, setters, mutable diagnostic slices, deserialization, defaults, unchecked conversion, and inconsistent total/truncation state.
  - Diagnostic-coordinate fixtures cover empty, scalar-boundary, escape-adjacent, and end-of-message half-open spans; primary and label spans; exact attachment of source identity and entry evidence; no host-offset shifting in the shared result; a published artifact with no host source; and equivalent optional local 013 mapping into a presentation DTO without mutating the shared failure.
  - Record-shape fixtures cover the complete portable diagnostic contract.
    - Require the exact field set and structured order.
    - Require one reused `DefinitionLocation` rather than duplicate Rust source/entry fields, while projecting the same top-level structured `source` then `entry` fields.
    - Require the exact `definition`, `kind`, `severity`, `message`, `span`, and `labels` diagnostic accessors; exact `category` and `code` kind accessors; exact `span` and `message` label accessors; and exact `start` and `end` span accessors.
    - Compile-fail fixtures reject public constructors and literals, setters, mutable label slices, deserialization, defaults, unchecked conversions, partial builders, duplicate source/entry storage, and a nested structured `definition` object.
    - Retain severity, stable code spelling, exact static message references, and label order.
    - Emit category as exactly `parser` or `semantic` with its matching closed code variant, and omit `SourceId`, `location`, and host spans.
    - Accept byte-boundary spans without adding a second scalar-boundary rule.
    - Keep the record independent of parser workspace storage.
    - Use read-only construction that cannot create inconsistent code, severity, and message triples.
    - Accept exactly 32 labels.
    - Map a thirty-third label, inverted span, out-of-payload span, or foreign label source to the exact closed `DiagnosticMappingInvariant`, including for diagnostics beyond the retention prefix, with no partial message-validation failure or exporter invocation.
    - Check mapping before incrementing the exact diagnostic total, map checked `u64` addition failure to `DiagnosticCountOverflow`, and never wrap, saturate, or return a partial count.
    - Map SemanticModel construction and validation errors to their separate `ExportPreparationError` variants while retaining the exact `SemanticInvariantError`, 012-owned reason, and call-site stage; discard accumulated ordinary diagnostics and invoke no exporter.
  - Serialization fixtures prove static messages are not copied into the mapped record and the bounded writer emits their exact text.
  - Preparation-error-union fixtures admit exactly `MessageValidation`, `SemanticModelConstruction`, `SemanticValidation`, and `InternalInvariant`.
    - They reject a generic operational wrapper, arbitrary strings, `Box<dyn Error>`, `anyhow::Error`, semantic-error relabeling, invalid-limit variants, `Other`, `Unknown`, and custom extensions.
    - Construction and validation variants retain their exact parser-owned error; the internal variant accepts only `ExportPreparationInvariant`.
    - Shared-placement fixtures keep the three operational variants in one top-level error after a checked outcome, retain `analysis` with `generationBlocked: true` and complete findings, omit `messageValidation`, leave `results` empty, and invoke no exporter or registration path.
    - Their reduced summary retains the resolved operation/target count, zero prepared and diagnostic counters, exact finding counters, and one `errorCount`; it omits every target-result-derived partition, output-state, and mode-specific counter.
    - They reject per-target duplication, first-target assignment, an empty or discarded checked analysis, partial ordinary diagnostics, synthetic blocked/error target results, and target-count-dependent error counts.
  - Preparation-invariant fixtures admit exactly `DiagnosticCountOverflow`, `DiagnosticMapping`, and `OutcomeContract`, with the exact four mapping variants and five outcome-contract variants.
    - Complete outcome preflight precedes parsing; outcome variant declaration order and canonical affected identity select one contradiction.
    - A clean outcome is scanned in stable definition order; within one diagnostic, label count precedes primary span, parser-ordered label spans, label-source equality, and then checked total increment.
    - Input, hash-map, optional within-call reuse, worker, and retention-state permutations select the same error and discard every earlier ordinary diagnostic.
    - They reject parser semantic API failures as preparation-owned invariants and reject generic assertion, panic, custom, source-I/O, cache, and exporter variants.
    - Structured-error fixtures map the ten exact Rust values bijectively to the ten fixed `details.invariant` tokens under `message_export_preparation_invariant_failed`.
    - They require `reason` followed by `invariant` and reject category/violation splitting, unknown or alternate tokens, source or entry identity, plan/message indexes, paths, spans, payload excerpts, nested errors, and secondary invariant vectors.
  - Batch fixtures prove `None -> Ok(None)` without parsing and `Some(Vec::new()) -> Ok(Some(empty batch))`.
    - Parser diagnostics, semantic diagnostics, and operational errors return no batch.
    - Every `Ok(Some(batch))`, including an empty batch, causes the selected transaction to invoke its exporter exactly once. Empty-batch exporters may return either a checked empty set or checked target-native bootstrap/loader/metadata artifacts; orchestration neither synthesizes an empty set nor rejects the transaction.
    - API fixtures expose only `plans() -> &[MessageBundlePlan]`; they reach validated messages through each plan's `messages()` and use ordinary slice operations for length, emptiness, and iteration.
    - Proof fixtures require successful private `prepare_export` construction itself to be the validation proof and the batch to retain only its immutable outcome borrow.
    - Compile-fail fixtures reject public or unsafe construction, deserialization, mutable plans, per-message validated flags, status maps, digest/location proof collections, independent markers, an outcome or findings accessor, a flattened/unique-definition accessor, definition-artifact input, parser/semantic workspace access, target options, exporter-specific data, and a persisted proof token.
    - Static trait assertions require `ValidatedExportBatch<'_>: Send + Sync` and every value reachable from `plans()` to remain immutable and thread-safe. A successful batch retains no parser workspace, AST, mutable semantic state, or cache guard.
    - Custom-integration concurrency fixtures reuse one batch across separately constructed target exporter instances under scoped workers without repeating validation; sequential and concurrent target scheduling produce the same canonical per-target results after integration-owned aggregation.
    - Initial M3 CLI fixtures remain sequential in canonical target-name order and prove that the trait capability alone does not activate a worker pool before the shared scheduler follow-up.
  - Exporter-trait fixtures require `PlatformExporter: Send` and permit moving each independently constructed instance into one worker for exactly one invocation.
    - They do not require `Sync`, never share or invoke one instance twice, and reject exporter-created worker pools or runtime selection as part of the common trait contract.
    - Third-party and built-in factories return the same object-safe `Box<dyn PlatformExporter>` boundary; target worker count, cancellation, join behavior, result ordering, and error aggregation remain integration-owned.
    - Stable-record resolution precedes parsing.
    - A batch borrows its exact immutable outcome and exposes only its validated plans.
    - No constructor, deserializer, or persisted-token bypass exists.
    - A changed input requires new preparation; repeated fresh preparation of equal input is equivalent.
    - The same batch can feed separate independent invocations of built-in and third-party `PlatformExporter` implementations without another message-validation pass.
    - Every transaction still invokes exactly one exporter.
  - Compile-fail/API-surface fixtures reject raw `MessageBundlePlan` as a `PlatformExporter::export` argument and prevent registry admission of a raw-plan callback.
  - Any ordinary selected-record parser or semantic diagnostic yields one integration-owned fail-complete result with zero exporter invocations and no asset, generated source, blob, loader map, manifest, or partial registration.
  - The fixtures do not produce a `LinkFinding` or `LinkOperationalError` for MF2 parser or semantic validation.
- Exporter-result API fixtures prove heterogeneous built-in and third-party implementations can coexist behind `dyn PlatformExporter`; every invocation returns the concrete `Result<ExportArtifactSet, ExportError>` without associated types or an exporter-specific erased adapter; a successful call returns one complete in-memory set and performs no final-build registration; and an error exposes no partial set.
  - Responsibility fixtures keep MF2 parser/semantic validation and batch preparation in `ExportPreparationError`, linker analysis in `LinkOutcome` / `LinkOperationalError`, exporter interpretation/generation/shared-result construction in `ExportError`, and platform capability, destination mapping, platform collision, and final registration in integration operational errors.
  - They prove no failure is relabeled merely to pass it through another layer.
  - Error-boundary fixtures require one closed kind with matching checked, bounded, deterministic typed evidence and read-only access.
    - Reject arbitrary strings, `anyhow::Error`, `Box<dyn Error>`, `Any`, platform errors, opaque source chains, exporter-specific payloads, and open custom-error variants.
    - Derive human-facing text in presentation.
    - Require built-in and third-party failures to return the same common shape with no candidate or partial set.
  - Kind fixtures admit exactly `UnsupportedBatch`, `GenerationFailed`, `OutputLimitExceeded`, `InvalidOutput`, and `InternalInvariant`; classify representative failures at each boundary; reject `Other`, `Custom`, `Unknown`, I/O, platform, cancellation, allocation, and namespaced variants; and prove evidence cannot reclassify or contradict its enclosing kind.
  - Unsupported-batch evidence fixtures require exactly one feature and one of `Batch`, `Plan`, or `Definition`.
    - Retain exact delivery-unit, locale, source, and entry identities where applicable.
    - Reject mismatched, missing, additional, index-based, path-based, span, excerpt, output-path, free-form-reason, exporter-ID, vector, and unknown-location forms.
    - Prove capability preflight selects the same first evidence under plan, input, hash-map, and worker permutations before generating payload or metadata.
  - Feature fixtures admit exactly `BatchComposition`, `DeliveryUnitPartitioning`, `LocalePartitioning`, `FallbackSemantics`, and `MessageSemantics`.
    - Require respectively `Batch`, `Plan`, `Plan`, `Plan`, and `Definition` locations.
    - Preserve the declared tie-break order.
    - Distinguish known unsupported message semantics from a claimed-supported generation failure.
    - Reject generic representation, target-option, custom-string, unknown, and mismatched-location forms.
    - Require a public compatibility change rather than reinterpretation when a new common plan feature is added.
  - Generation-failure fixtures admit exactly `MessageCompilation`, `PayloadEncoding`, and `MetadataGeneration`.
    - Require `Definition` for message compilation and the narrowest `Batch`, `Plan`, or `Definition` input location for the other stages.
    - Select stage declaration order and then canonical location order under sequential and concurrent failure permutations.
    - Reject generated paths or kinds, indexes, excerpts, arbitrary codes or reasons, compiler diagnostics, source chains, implementation errors, invalid stage and location pairs, and unknown stages.
  - They distinguish final-constructor limit/contract failures and internal invariants, return no generated candidate or partial set, and keep implementation-local debug output outside common equality and conformance behavior.
  - Output-limit evidence fixtures require exactly one counter and either `Exact(value > ceiling)` or `ArithmeticOverflow`.
    - Accept an exact attempted value more than one above the ceiling.
    - Reject equal or below-ceiling exact values, saturation, wrapping, clamping, and `u64::MAX` substituted for overflow.
    - Reject copied ceiling, remaining, lower-budget, or allocation fields; every location or index; and arbitrary detail.
    - Prove that the fixed ceiling is derived only from the counter and that equivalent early and final-constructor failures retain the same evidence shape.
- Export-artifact envelope fixtures prove that every built-in and third-party target returns the same checked `Vec<ExportArtifact>` shape.
  - Generated text and binary payloads use bounded owned bytes.
  - Loader maps and manifests are ordinary records.
  - Ordering and metadata are deterministic.
  - Admit no closed target enum, `Any`, callback, trait object, exporter-specific side channel, unbounded payload, duplicate `ExportArtifactPath`, or partial set.
  - Artifact-count fixtures accept an empty set and exactly 65,536 submitted records.
    - Reject length-conversion failure and the 65,537th record before semantic or graph work.
    - Charge later-invalid and duplicate-path records without deductions for sorting, deduplication, equal payloads, relationship targets, or platform mappings.
    - Exercise known-length preflight and incremental decode admission.
    - Reject every override source.
    - Prove that the built-in bounded collector and final checked constructor return no partitioned or partial set.
  - Path-shape fixtures accept a non-empty ordered segment array such as `["assets", "messages", "en.json"]` and non-ASCII scalar segments.
    - Reject an empty array, empty segment, exact `.`, `..`, slash, backslash, `U+0000`, every `Cc` scalar, and slash-string identity form.
    - Preserve segment boundaries and exact decoded Unicode through structured adapters, without normalization, case folding, trimming, percent decoding, or separator rewriting.
    - Distinguish canonically equivalent encodings.
    - Prove that slash-joined display is presentation-only.
  - Path-limit fixtures accept exact boundaries and reject the first value over 255 decoded UTF-8 bytes per segment, 64 segments per path, 4,096 decoded segment bytes per path, and 64 MiB across all submitted artifact logical paths.
  - Charge later-invalid and duplicate paths independently without deductions for equal segments, shared prefixes, sorting, or interning.
    - Exclude array or record framing, display separators, allocation capacity, kind, payload, metadata, and separately counted relationship targets.
    - Exercise known-length preflight, incremental decode accounting, checked conversion and addition, and overflow.
    - Reject every override source.
    - Fail the complete set without truncation, artifact dropping, set partitioning, fallback renaming, or partial output.
  - Canonical-order fixtures permute input, exporter construction, parallel completion, and hash-map iteration; sort segment by segment using exact UTF-8 bytes with a shorter equal-prefix path first; reject an exact duplicate after sorting; and prove array order carries no dependency or registration semantics.
  - Path-mapping fixtures map the same segment sequence to emitted files, virtual modules, custom sections, and in-memory keys without changing set identity or canonical order.
    - Reject absolute or host paths and implicit current-directory or output-root resolution in the common layer.
    - Reject a case-insensitive, normalization, escaping, or reserved-name collision between distinct logical paths.
    - Report an unmappable or non-injective destination as one integration operational error without overwrite, fallback renaming, or partial registration.
- Export-kind fixtures distinguish `dev.intlify/esm-module` from `dev.intlify/loader-map`, distinguish same-MIME payloads with different semantic roles, accept an unknown conforming third-party kind without a common allowlist, and compare exact validated IDs.
  - Grammar fixtures share the `ProducerId` validator behavior while retaining distinct Rust types.
    - Accept valid built-in, third-party, and lowercase IDNA namespaces.
    - Accept exactly 255 bytes and reject the first byte over.
    - Cover reverse-DNS label boundaries and one kebab slug.
    - Reject empty, uppercase, underscore, whitespace, Unicode-domain, URI-like, percent-encoded, query or fragment, and extra-slash forms.
    - Perform no normalization, alias lookup, or implicit conversion to or from `ProducerId`.
  - They prove MIME type, extension, destination, and payload bytes never infer or replace a kind; common validation performs no network or ownership lookup; and a selected integration rejects an unsupported valid kind fail-complete without generic-blob fallback, content sniffing, artifact dropping, or partial registration.
- Export-format-version fixtures require a distinct `ExportArtifactFormatVersion { major: u16, minor: u16 }` on every artifact; cover `0`, `1`, and `u16::MAX` for both components; reject missing, signed, fractional, string, patch, prerelease, build, overflowing, and implicit `ArtifactVersion` forms; and prove `(kind, format_version)` affects equality, fingerprints, cache keys, structured output, and `--check`.
  - The two initial built-in kinds emit and accept exact `0.1`; draft fixtures accept that exact pair and reject `0.0`, `0.2`, and every other pair.
  - Stable fixtures accept the same supported major through the per-kind `max_minor`, interpret and normalize every accepted older minor through explicit defaults, and reject a newer minor or different major.
  - They prove each kind owns independent support entries and tests despite the parallel negotiation rule.
  - Compatibility preflight validates the complete canonical set before inspecting or interpreting any payload content or metadata and rejects unknown kinds, unsupported versions, and mixed incompatibilities without downgrade, version rewriting, content sniffing, exporter invocation retry, or partial registration.
- Export-metadata-boundary fixtures require one metadata object on every artifact with exactly `media_type` and `relationships`; accept `{ None, empty }` as the canonical empty semantic value; and reject an absent object, missing required field, duplicate or unknown field, flattened alternative, and retained round-trip extension.
  - They expose only checked, bounded, deterministic, portable registration fields through read-only accessors.
  - Compile-fail and construction fixtures reject arbitrary JSON/object/map values, stringly typed key/value entries, namespaced extension bags, `Any`, callbacks, trait objects, exporter-defined side channels, kind-specific representation data outside the payload, and platform policy embedded in metadata.
  - Third-party kinds use the same boundary.
  - Exclusion fixtures prove no integrity, source-map, permission/executable, compression/content-encoding, locale, delivery-unit, or platform-option field exists in `0.1`, and that hashing, separate source-map artifacts, plans/kind payloads, and integration inputs retain the stated ownership without an extension escape hatch.
  - MIME-presence fixtures distinguish `Some(ExportMediaType)` from `None`; prove the complete metadata affects equality, fingerprints, cache keys, structured output, and `--check`; and prove absence is never filled from extension, kind, payload, content sniffing, or platform defaults.
  - MIME-grammar fixtures accept `text/javascript`, `application/json`, `application/vnd.example+json`, syntactically conforming unregistered names, and exact 127-byte component / 255-byte total boundaries.
  - They reject an empty or 128-byte component; missing or extra slash; uppercase, Unicode, wildcard, whitespace, comment, percent-encoded, URI-like, and parameterized forms; every character outside the restricted grammar; and alias, case, IANA, suffix-registry, ownership, or network normalization/lookup.
  - An integration that requires MIME rejects a missing or unsupported claim fail-complete before registration.
  - Evolution fixtures require separate bounded typed fields, an unambiguous older-version default, and the affected per-kind minor-version update before parameters or another additive common field can appear, and a format-major update for removal, changed meaning/default, or newly required data.
- Export-payload shape fixtures require one private owned `Box<[u8]>` payload per artifact and expose only `&[u8]`; accept empty, UTF-8 text, every byte value, BOM-like prefixes, mixed line endings, interior NUL, and invalid UTF-8; and preserve the exact bytes through equality, fingerprints, cache keys, lossless structured round trips, and `--check`.
  - Compile-fail/API fixtures prevent an absent payload, borrowed exporter scratch, public mutable `Vec`, file/mmap handle, reader, stream, callback, trait object, `Any`, or deferred producer closure.
  - Transformation fixtures prove the common layer performs no UTF-8 or semantic validation, text conversion, BOM/newline/Unicode normalization, transcoding, compression, content sniffing, or media-type inference, and that integrations inspect bytes only after complete kind/version preflight.
  - Per-artifact limit fixtures accept empty and exactly 256 MiB; reject the first byte over, length-conversion failure, and every override source; count exact stored bytes without capacity/character/expansion/compression adjustment; and return no truncation, split artifact, representation change, or partial set.
  - Cumulative-limit fixtures accept an empty set, any number of empty payloads, and an exact 1 GiB total; reject the first byte over, length-conversion failure, checked-addition overflow, and every override source; charge equal payload bytes independently without deduplication, interning, compression, representation, kind, or relationship deductions; and cover multiple individually valid payloads whose sum is invalid.
  - Bounded-writer fixtures exercise exact-boundary chunked generation and first-over failure before reserve/append/format/encode/final-box operations, coordinate one set-scoped budget across sequential and concurrent generation, return one `ExportError` without an over-limit final set, and prove final checked construction revalidates the per-artifact and cumulative limits for built-in and third-party results.
- Export-relationship shape fixtures accept an empty vector and exact logical-path targets in the same set; reject an external package/module/URL/host path, slash-joined path, destination spelling, nonexistent or ambiguous target, duplicate `(kind, target)` pair, invalid kind, noncanonical input, and checked-count overflow; and return no partial set.
  - Kind fixtures accept only `EagerLoad` tag `0` and `LazyLoad` tag `1` under format `0.1`, reject unknown tags, and prove tags do not claim a Rust ABI.
  - Permutation fixtures prove artifact-path uniqueness is established before target resolution; canonical ordering is kind tag then exact target path; vector position has no scheduling semantics; and original segment-array identity is retained across platform mapping.
  - Semantic fixtures prove eager targets belong to the source's initial load closure, lazy targets remain outside it but addressable through a deferred path, and an incapable integration fails before registration without promotion/demotion or platform-import inference.
  - Conflict fixtures reject eager and lazy edges from one source to the same exact target without precedence, promotion/demotion, merge, or first/last selection; permit distinct sources to share a target and one source to reference distinct targets; and prove mapped destinations do not define conflict identity.
  - Self-edge fixtures reject exact eager and lazy self-targets after resolution and before cycle analysis without edge deletion or warning downgrade, while mapped-path collisions and filesystem aliases do not define common self-identity.
  - Cycle fixtures accept two- and multi-node eager SCCs, pure lazy cycles, and mixed cycles whose lazy edges remain outside their source eager closures; reject direct and indirect lazy-to-eager-reachable conflicts, including a lazy edge within one eager SCC; and prove input permutation and mapped destinations do not change the result.
  - Integration fixtures reject a commonly valid but unsupported cyclic graph before registration without edge rewriting, artifact duplication, or acyclic-subset publication.
  - Relationship-limit fixtures cover count, byte, accounting, and failure boundaries.
    - Accept zero and the exact per-artifact and cumulative count boundaries of 4,096 and 65,536.
    - Accept the exact 64 MiB target-byte boundary.
    - Reject the 4,097th local record, 65,537th cumulative record, and first target byte over 64 MiB before graph work.
    - Convert lengths to `u64` and use checked accumulation.
    - Charge every target occurrence's exact decoded segment bytes without framing, separator, or interning deductions.
    - Charge invalid, duplicate, conflicting, and self-edge records before later semantic validation.
    - Preflight available structured counts and incrementally decoded bytes before unbounded allocation.
    - Reject conversion or addition overflow and every override source.
    - Return no path or vector truncation, split artifact or set, dropped edge, or partial output.
  - M3 fixtures map loader-map edges exactly from `eagerLocales`, emit lazy edges for every other supported locale, and add no reverse edge.
  - Equality, fingerprint, cache, structured-output, and `--check` fixtures vary one relationship at a time.

### Artifact wire and identity conformance

- Definition-source JSON fixtures require exactly `namespace` and `path`, accept either input order, and canonically emit namespace first.
  - Namespace fixtures accept only `{"kind":"project"}` with its exact decoded tag; reject a `"package"` kind, a `package` member, `null`, mixed/flattened/string forms, duplicate or unknown members, case variants, aliases, and unknown tags; and prove only the explicit fixed `Project` namespace participates in source equality and fingerprint inputs under v0.1.
- Artifact-version JSON fixtures require the exact two-member `{"major":0,"minor":1}` object for the current writer; accept either input-member order but re-emit `major` then `minor`; exercise `0`, `1`, and `u16::MAX` for each member; and reject duplicate, unknown, missing, `null`, boolean, dotted-string, decimal-string, array, combined-number, signed, fractional, exponent, and overflowing forms.
  - Bootstrap parsing validates this shape before selecting the remaining versioned schema, and no artifact minor version changes the transport-level shape.

#### Artifact-version conformance

- Keep golden artifacts for every producer.
- Require the exact v0.1 major and minor values.
- Exercise `0`, `1`, and `u16::MAX` component boundaries without signed, floating, string, patch, or variable-width forms.
- Require exact four-byte big-endian fingerprint-payload goldens.
- For draft versions, accept only the exact supported version and reject every neighboring mismatch.
- For stable versions, accept same-major `0..=max_minor` values and normalize every accepted older minor.
- Reject a newer minor or different major with observed and supported version evidence.
- Accept mixed compatible minor versions in one request.
- Never allow producer identity to override compatibility.
- Perform no implicit downgrade or version rewriting.
- Reject a writer that lowers only its version fields.
- Define no M0 required-feature negotiation.
- Include third-party-producer fixtures.
- Round-trip an explicit `UnboundedDynamic` record without rewriting it to `AllInScope`.

#### M0 definition-wire conformance

- **Document encoding:** accept UTF-8 JSON without a BOM, with one root object followed by EOF.
  - Accept the four JSON whitespace scalars.
  - Reject invalid UTF-8, BOMs, comments, trailing commas, multiple roots, and trailing non-whitespace.
- **Object members:** reject duplicate members at every depth, unknown or missing members, and forbidden `null` values.
  - Accept non-semantic input-member permutations.
- **String decoding:** accept noncanonical whitespace and escape spellings that decode identically.
  - Reject unpaired surrogates without normalization.
- **Unsigned integers:** parse lexically at every field boundary without IEEE-754 conversion.
  - Reject signs, fractions, exponents, overflow, imprecise numbers, and ad hoc decimal strings.
- **Semantic ordering:** preserve the defined array ordering.
- **Canonical output:** emit the exact canonical root and nested-member order, without whitespace or a final newline.
  - Cover canonical integer and string escaping, including control and non-ASCII scalars.
  - Require byte-identical repeated emission across platforms.
- **Accounting and identity:** charge actual input wire bytes for accepted noncanonical spellings.
  - Require equal semantic fingerprints for equivalent JSON spellings.
- **Transport boundary:** reject alternate or content-sniffed encodings under the same contract version.
- M0 reference-wire envelope fixtures apply the same UTF-8 JSON lexical, duplicate, unknown/missing/null, member-order, whitespace, escaping, EOF, and alternate-encoding matrix to the reference decoder.
  - They require exactly the six root members and canonically emit `kind`, `version`, `producer`, `identity`, `deliveryUnit`, then `references`; share the version, producer, identity, and delivery-unit codecs; retain the exact semantic reference-array order including an empty array; reject a serialized record ordinal, sorting, or array-to-map alternative; and prove accepted noncanonical root spelling re-emits byte-identically across platforms.
  - Nested project-namespace, scope-name, catalog-key-domain, selector-envelope, fixed prefix-representation, inclusive-prefix, empty-prefix-rejection, shared selector-path-accounting, and 64 MiB selector-path-boundary fixtures are mandatory and enforce their fixed shapes, semantics, and limits.
  - Pattern fixtures enforce structural-token matching rather than serialized-byte or intra-token matching; require `*` to consume exactly one complete token and `**` to consume zero or more complete tokens across every domain token kind; prove both pattern and candidate sequences must be fully consumed without implicit leading or trailing operators; and enforce slash segmentation plus the exact `~0` / `~1` / `~2` literal escape table.
  - Pattern-token fixtures distinguish operator `*` and `**`, literal `~2` and `~2~2`, and raw token `~2` encoded as `~02`.
    - Reject raw asterisks in other segments, dangling or unknown tilde escapes, percent or backslash alternatives, and typed-domain-invalid decoded literals.
    - Prove that `~2` has no meaning in `Exact` or `Prefix`.
    - Reject an empty pattern without converting it to root-only, `**`, or `AllInScope` semantics.
    - Reject every adjacent run of `**` without collapsing it, while accepting literal `**` tokens, `*/*`, and `**/*/**`.
  - Pattern-accounting fixtures measure the complete decoded canonical string before pattern parsing, charge every occurrence to the dedicated per-value check and enclosing decoded total without reuse deductions, keep actual JSON in the wire total, apply zero/lower caller budgets, and prove no literal/operator subcounter or reuse of `selector_path_bytes` exists.
  - Boundary fixtures accept a valid canonical pattern at exactly 128 MiB, reject the first decoded byte above it, exercise checked addition and host-size conversion, apply caller-selected lower values, prove the independent enclosing decoded budget can still reject an individually valid pattern, and cover worst-case literal-asterisk expansion from a 64 MiB 013 key.
  - Matcher-model fixtures compare the iterative NFA/DP result against exhaustive small cases.
    - Cover every literal, `*`, and `**` transition, including zero consumption.
    - Require complete anchoring.
    - Process every reachable `(pattern position, candidate position)` once in canonical row order.
    - Prove that iterative, compiled, trie or batched, cached, and parallel implementations return identical matches and logical accounting.
    - Permit no recursion, exponential backtracking, partial results, or widened fallback.
  - Work-unit fixtures count `(0, 0)`, each distinct reachable pair, and a reachable accepting pair once; reject full-rectangle, edge, fan-in, queue-operation, instruction, and wall-clock accounting; count unreachable pairs zero; use checked `u64` addition and the exact first attempted excess; and charge conceptual evaluations without cache, equality, interning, trie, bitset, SIMD, or parallel deductions.
  - Token-limit fixtures accept one and exactly 513 parsed tokens, reject the 514th before retention, count literal / `*` / `**` occurrences equally, apply zero and lower caller limits, exercise the 256-consuming-plus-257-globstar maximum, reject every larger provably unmatchable pattern, and require coordinated 013/014 revision when the depth premise changes.
  - Derived-maximum fixtures prove the complete 514-by-257 rectangle is 132,098, no reachable count can exceed it, an observed excess is an internal invariant failure, no independent `pattern_match_states` limit or override exists, and lowering `selector_pattern_tokens` reduces the possible per-evaluation work.
  - Aggregate work-ceiling fixtures apply the exact 100,000,000-state boundary and first-over rejection.
  - A root-envelope fixture claims the Pattern portion complete only when it includes the fixed candidate-set, ordering, counter, and ceiling contracts.
- Message-reference record-envelope fixtures require non-null `scope`, `domain`, and `selector`; accept all four omission/presence combinations of `reason` and `origin`; and canonically emit required fields first, followed by present `reason` and then present `origin`.
  - They accept non-semantic input-member permutations but reject missing required fields, duplicate/unknown/mistyped members, `null` for any field, default placeholders for absence, `ordinal`, repeated delivery/provenance/identity fields, a `metadata` wrapper, and flattened extensions.
  - Round-trip fixtures prove omitted members decode to `None`, writers never emit nulls, exact optional presence/value changes record evidence and artifact bytes but not `ReferenceRecordIdentity`, and later additive fields require an omission default plus versioned canonical position rather than v0.1 extension retention.
- Reason-text fixtures accept exact decoded UTF-8 boundaries of 1 and 4,096 bytes, non-ASCII scalars, whitespace-only non-empty text, tab/LF/CR, and distinct CRLF/LF/CR values; reject empty, the first byte over, non-string/null values, unpaired surrogates, and every forbidden C0/C1 code point whether literal or escaped.
  - Limit fixtures require the twenty-sixth `ReasonBytes` variant, exact `reason_bytes` spelling, only `ReferenceArtifactGroup(identity)`, and exactly `Exact(effective_limit + 1)`; reject `Request`, a raw reason, record ordinal, complete length, conversion-failure alternative, and `ArithmeticOverflow`; and prove bounded first-over admission makes both alternate numeric failures unreachable.
  - Precedence fixtures run one complete ordinal-order reason-byte pass after selector token admission and before reason grammar or origin admission, with identical selection under decoder, direct-construction, cache, partition, and worker paths.
  - Round trips preserve exact bytes without trimming, Unicode/case/newline normalization, localization, or Markdown parsing and canonicalize only the outer JSON escaping.
  - Accounting fixtures apply zero/lower caller budgets before retention, charge repeated equal reasons independently to decoded totals, return no truncation/replacement/hash/sanitization/drop, and prove reason content affects evidence/cache but never matching, reachability, record identity, or disposition.
- Source-origin fixtures require exactly `source` then `span` in canonical output while accepting non-semantic member permutations; reuse the exact project-only `SourceDocumentIdentity` namespace/path codec; require exactly lexical `u32` `start` then `end`; and accept `0..0`, non-empty ranges, `u32::MAX..u32::MAX`, and repeated source identities across records.
  - They reject a package namespace, missing, duplicate, unknown, mistyped, `null`, signed, fractional, exponent, string, overflowing, and reversed coordinates; host/display/slash paths, URIs, line/column, excerpts, digests, binary offsets, and producer-specific payloads; and namespace inheritance or fallback.
  - Producer fixtures validate exact source length and scalar boundaries before construction; detached-decoder fixtures validate only width/order; reporter fixtures use a resolved range only against compatible exact UTF-8 source bytes and otherwise preserve unavailable/stale evidence without clamping, shifting, widening, or failing linking.
  - Path-limit fixtures require the twenty-seventh through twenty-ninth `PathSegments`, `PathSegmentBytes`, and `PathBytes` variants with exact `path_segments`, `path_segment_bytes`, and `path_bytes` spellings.
  - For origins they require only `ReferenceArtifactGroup(identity)`, exactly `Exact(effective_limit + 1)` for the first two counters, and the exact attempted per-path running total for `PathBytes`; they reject `Request`, `DefinitionArtifactEnvelope`, raw paths/segments, record ordinals, full submitted lengths, `effective_limit + 1` substitution for `PathBytes`, and `ArithmeticOverflow`.
  - Precedence fixtures run complete ordinal-order path-count, ordinal/segment-order segment-byte, and ordinal-order per-path-total passes after the complete reason-byte pass and before origin grammar/span validation, with identical selection under decoder, direct, cache, partitioned, and parallel routes.
  - Accounting fixtures charge repeated paths independently to `reference_artifact_decoded_bytes` and exact spelling to `reference_artifact_wire_bytes`, add no variable decoded bytes for fixed `u32` fields, and prove origin affects evidence/cache but never linking semantics or record identity.
- Shared portable-path subject fixtures use payload-free `DefinitionArtifactEnvelope` for every primary definition-source path failure even when a direct, cached, or lower-budget route already knows the eventual source; use `DefinitionArtifactGroup(source)` for aliases only after the primary identity is checked and for both definition-only set counters; and use `ReferenceArtifactGroup(identity)` only for origins.
  - They reject every cross-role pairing, raw or partial path/segment/alias data, alias or record indexes, and use of `DefinitionArtifactEnvelope` by another currently fixed counter.
  - Boundary fixtures accept exactly 1,024 segments, 4,096 bytes per segment, and 262,144 bytes per path; reject the first excess with the fixed observations; prove the `PathBytes` attempted total cannot exceed 266,240; and require empty-path grammar to remain distinct from numeric admission.
  - Definition-alias admission fixtures require this exact order:
    1. complete primary numeric and grammar validation;
    2. alias-count preflight before any alias validation or canonical-order and duplicate check;
    3. complete, non-interleaved `PathSegments`, `PathSegmentBytes`, and `PathBytes` passes over submitted alias order;
    4. one complete `SourcePathBytes` pass over the primary and submitted aliases;
    5. one complete alias-grammar pass in alias and segment order;
    6. one adjacent canonical-set pass; and
    7. remaining physical-binding and artifact semantics.
  - They accept zero and exactly 4,096 submitted aliases, reject the first excess with the thirtieth `LogicalAliases` variant, exact `logical_aliases` spelling, `DefinitionArtifactGroup(source)`, and `Exact(effective_limit + 1)`, and charge invalid and duplicate occurrences without retaining the complete count or alias index.
  - Cumulative fixtures accept exactly 64 MiB.
    - Reject the first complete-path addition above it with the thirty-first `SourcePathBytes` variant, exact `source_path_bytes` spelling, the same group subject, and the exact attempted running total.
    - Prove that the total cannot exceed 67,371,008 and never uses `ArithmeticOverflow`.
    - Charge the primary even when there are no aliases and reject every valid artifact under a zero cumulative lower limit.
    - Require sequential, cached, partitioned, and parallel routes to select the same earliest complete pass and canonical evidence.
  - Alias-semantic fixtures require complete grammar validation to win over every ordering failure, then require `[source.path] + logicalAliases` to be strictly increasing under exact portable-path ordering.
  - They classify an equal adjacent pair as duplicate logical identity and a descending pair as noncanonical order, select the first violating submitted adjacent pair, and reject every implementation that sorts, deduplicates, drops an alias, changes primary, compares unchecked paths, or makes worker completion select the result.
- Catalog-scope identity fixtures require exactly the `namespace` and `name` object members and canonicalize them in that order while accepting non-semantic member permutations.
  - They accept the exact project namespace plus every non-empty exact 013 scope-name spelling within 255 decoded UTF-8 bytes and preserve case, Unicode, whitespace, and canonically equivalent scalar sequences without trimming or normalization.
  - They reject a package namespace, an empty or 256-byte name, missing/duplicate/unknown/mistyped/flattened/`null` members, bare or combined strings, arrays, integers, hashes, opaque producer objects, and published contextual `Project` claims.
  - Resolution fixtures map project config `scope: "app"` to structural `Project/app`, permit only implementation-local interning, keep distinct project names distinct until an explicit mapping, and perform no name-only fallback or implicit surrounding-namespace inheritance.
  - Equality/order/fingerprint fixtures compare the fixed namespace then exact UTF-8 name bytes and encode tags `0x01`/`0x02`; accounting charges each name occurrence without interner deductions.
  - Limit fixtures accept exact 1-byte and 255-byte boundaries, reject the first byte over the ceiling before retention, count after JSON unescaping, distinguish decoded from serialized escape bytes, apply zero and lower `catalog_scope_name_bytes` budgets, charge every enclosing occurrence without interner deductions, and fail complete input without truncation, hashing, aliasing, replacement, or normalization.
- Scope-mapping fixtures accept an empty table and exact sorted one-hop mappings; accept distinct structural sources mapping many-to-one to one declared validated target; and leave every unmapped scope unchanged.
  - They reject an unknown or entry-level-only endpoint, duplicate source even with an equal target, self-map, any target that also occurs as a source, chain, cycle, name-only/wildcard/prefix/case/normalization fallback, surrounding-namespace inheritance, reverse lookup, and input-order precedence.
  - Permutation fixtures produce the same table and outcomes, and checked/direct-link validation returns one fail-complete operational error before semantic work for every invalid table.
  - Uniform-resolution fixtures apply the table to references, definitions, policy, and completeness inputs before domain and semantic indexes; use `ResolvedCatalogScopeId` in finding/coverage/plan semantics; expose a mapped domain conflict as invalid input and a mapped definition collision as ordinary complete ambiguity evidence; retain original artifact scopes through provenance; and never rewrite artifact bytes or fingerprints.
  - Cache fixtures keep extraction/producer caches valid while changing the mapping invalidates the link-result cache through the complete canonical table.
  - Count-limit fixtures accept zero and exactly 4,096 submitted entries and reject the first occurrence above each effective limit before endpoint or mapping-semantic validation.
  - Count-limit evidence uses `ScopeMappingEntries`, payload-free `ScopeMappings`, and exactly `Exact(effective_limit + 1)`.
    - Do not use `Request`, an entry, endpoint, or index, a complete submitted count, or `ArithmeticOverflow`.
    - Compare known lengths against the bounded limit without converting the full length.
    - Stop incremental admission before retaining the first excess entry.
    - Charge invalid and duplicate entries without sorting, deduplication, equality, interning, many-to-one, or cache deductions.
    - Apply zero and lower caller budgets.
    - Return no truncated table, dropped mapping, partitioned request, partial outcome, or relaxed-limit retry.
  - Derived-byte fixtures account for each source then target name with checked arithmetic, prove the exact `2,088,960`-byte maximum implied by 4,096 entries and 255 bytes per name, reject arithmetic or host-size conversion failure, exclude the fixed project discriminant and framing, and prove no independent aggregate-byte admission counter or lower override exists in M0.
- Artifact-kind fixtures require decoded exact `message-definition` and `message-reference` strings for their respective typed decoders; reject missing, duplicate, non-string, differently cased, aliased, unknown, and opposite-artifact values before version/schema validation; and prove generic dispatch and direct typed decoding agree.
  - Canonical bytes begin with `{"kind":"message-definition"` or `{"kind":"message-reference"` respectively, and accepted noncanonical escaping or member placement re-emits the applicable prefix.
  - Each wire-only discriminator changes only its artifact's wire-byte accounting and never the typed artifact, fingerprint where present, identity, findings, or plans.
- Entry-reference fixtures project the exact 013 `EntryKey` into `EntryReference { structural_path, occurrence }`; accept an empty root structural path and `occurrence: 0`; preserve non-normalized Unicode, control scalars, escaped separators, and format-specific canonical path spelling as exact decoded UTF-8; and require exactly the `structuralPath` and `occurrence` JSON members with canonical emission in that order.
  - They reject missing, duplicate, unknown, `null`, non-string path, signed/fractional/string/overflowing occurrence, duplicate pairs, and a first, skipped, decreasing, or overflowing per-path occurrence.
  - Identity fixtures prove `SourceDocumentIdentity + EntryReference` is stable and unique, distinguish equal structural paths in different sources and successive duplicate-key occurrences in one source, and prove `EntryHandle`, catalog/display key, host span, offset map, vector index, and generated IDs do not enter the portable identity.
  - Codec fixtures cover tag `0x01` structural-path bytes and tag `0x02` `u32be` occurrence, plus independent decoded-byte and actual-wire-byte accounting.

#### Definition-artifact conformance

- **Artifact production:** produce one complete artifact for every selected source and retain stable raw-entry ordering.
  - Accept a valid source with zero entries.
  - Produce no artifact after a source-level extraction, binding, domain, or projection failure.
  - A source edit invalidates only that source's artifact.
  - Compose several artifacts independently of input order.
  - Do not infer project or scope completeness from individually complete sources.
- **Locale codec:** require the exact locale-string codec.
  - Accept 1-byte and 255-byte values; reject empty and 256-byte values.
  - Preserve case, whitespace, control characters, and non-normalized Unicode as byte-exact identity.
  - Use exact UTF-8 bytes for equality, ordering, and the fingerprint payload.
  - Charge decoded bytes per occurrence.
  - Perform no BCP 47 validation or spelling rewrite.
- **Locale admission precedence:** validate definition-local source identity, then definition count, then raw-entry `locale_bytes`.
  - Represent both protocol and lower-budget overruns as `LocaleBytes` with only `DefinitionArtifactGroup(source)` and `Exact(effective_limit + 1)`.
  - Select the canonical source group under input, duplicate, partition, and worker permutations.
  - Do not retain a full-length observation, invalid-locale copy, occurrence index, or `ArithmeticOverflow`.
- **Source identity:** require exact project-relative identity.
  - Reject package-relative identity and published contextual `Project` claims.
  - Keep identity stable across content edits.
  - Keep equal content at distinct paths separate.
  - Use segment-array wire goldens rather than slash-string paths.
  - Reject empty arrays; empty segments; `.`; `..`; `U+0000`; `/`; absolute host paths; and root escape.
  - Preserve literal backslashes and non-normalized Unicode exactly.
  - Never accept display rendering as identity input.
  - Reject duplicate source identities.
  - Do not allow fingerprints or physical identity to change source equality.
- **Producer identity shape:** require separate `id` and `revision` fields and compare revision as opaque exact text.
  - Accept built-in, third-party, and lowercase IDNA producer IDs.
  - Accept exactly 255 bytes for a producer ID and reject the first byte over.
  - Reject single-label, empty, uppercase, non-canonical, URI-like, and extra-path-component producer IDs.
  - Accept revisions at exactly 1 and 128 bytes; reject empty and 129-byte revisions.
  - Accept every declared revision grammar class.
  - Reject whitespace, control characters, Unicode, `/`, `:`, and other characters outside the grammar without trimming or normalization.
  - Perform no DNS or alias lookup.
- **Producer revision semantics:** keep a producer ID stable across release and installation changes.
  - Keep revisions deterministic and stable for equivalent builds.
  - Advance the revision for output-affecting code, feature, table, and dependency changes.
  - Leave it unchanged for changes proven to be output-independent.
  - Include no timestamp, randomness, path, or process state.
  - Use a distinct development revision for code modified from a release.
  - Accept mixed valid revisions in one request without semantic differences.
- **Fingerprint envelope:** require the exact `blake3-256` envelope and unkeyed 32-byte hash goldens.
  - Accept and decode exactly 64 lowercase hexadecimal characters.
  - Reject unknown or differently cased tags, uppercase, non-hex, or wrong-length digests, alternate encodings, and missing or extra fingerprint fields.
  - Require recomputation to match and stale-cache mismatches to fail.
  - Preserve semantic outcomes when only the fingerprint changes.
  - Exercise mutation of every included fingerprint-tuple component and byte-exact source changes outside entries.
  - Produce equal fingerprints when raw config, glob, discovery, absolute root, metadata, reporter, or command mode changes but the resolved tuple remains equal.
  - Partition caches by producer identity.
  - Reject implicit output-affecting environment inputs.
  - Do not hash output artifacts recursively.
- **Fingerprint framing:** require exact domain-header and framing-version bytes, fixed tag order, and TLV byte goldens.
  - Cover empty and multi-item sequence and record cases.
  - Require concatenation-collision pairs to produce different streams.
  - Require assembled and chunked-source hashing to agree.
  - Exercise `u64` big-endian length and count boundaries within resource limits.
  - Make the conformance encoder reject omissions, duplicates, reordering, padding, trailing data, and types without canonical payload codecs.
- **Producer interoperability:** accept an unknown conformant producer without an allowlist.
  - Never let a claimed built-in identity bypass validation or unsupported `ArtifactVersion` rejection.
  - Preserve semantic outcomes and ordering when producer provenance alone changes.
  - Invalidate producer-specific cache reuse when producer identity changes.
- **Physical aliases:** make physical-alias grouping invariant under input permutation.
  - Order the primary path and aliases by exact UTF-8 segment ordering.
  - Perform one extraction and produce one definition set per physical group.
  - Exclude non-participating aliases.
  - Serialize no physical identity.
  - Reject the complete group when namespace, canonical host-format ID, scope, or locale bindings differ.
- Catalog-key-domain wire fixtures accept exactly `"json-pointer"`, `"yaml-typed-path"`, `"xliff-1.2"`, and `"xliff-2"`; map them to the four comparable 013 variants; re-emit the exact token; order them by the fixed table; and fingerprint the exact ASCII token bytes.
  - They reject `StandaloneMf2` / `"standalone-mf2"`, empty, `null`, case/whitespace variants, aliases, unknown strings, numbers, and objects without normalization.
  - Accounting fixtures charge actual JSON bytes to the enclosing wire total, add zero variable decoded bytes for the checked enum, and prove that no domain-byte limit exists.
  - Version fixtures reject an added domain under v0.1 and require coordinated 013 semantics, schema/version, order, fingerprint, and conformance work.
- Message-selector envelope fixtures accept all five exact internally tagged object shapes and non-semantic input-member permutations, then canonically emit `kind` before `key`, `prefix`, or `pattern`.
  - They require the exact lowercase kind token and the one variant-specific non-null string where applicable; require no payload for `AllInScope` / `UnboundedDynamic`; and reject missing, duplicate, unknown, mistyped, `null`, cross-variant payload, bare-string, externally tagged, generic-`value`, array, integer-tag, and nested-`reason` forms.
  - Equality/order fixtures use the typed variant, fixed table order, and exact validated payload.
  - Accounting fixtures add zero variable decoded bytes for kind, charge `Exact.key` and `Prefix.prefix` to the same per-value `selector_path_bytes` check and `reference_artifact_decoded_bytes`, charge `Pattern.pattern` to `selector_pattern_bytes` and `reference_artifact_decoded_bytes`, charge actual JSON to `reference_artifact_wire_bytes`, and prove no `selector_kind_bytes`, `exact_key_bytes`, or `prefix_bytes` limit exists.
  - They count repeated equal selector paths independently and prove a zero lower path budget admits only a domain-valid empty `Exact`, never a `Prefix`.
  - Boundary fixtures accept a syntactically valid selector path at exactly 64 MiB, reject the first decoded byte above it, exercise checked addition and host-size conversion, apply caller-selected lower values, and prove the independent enclosing decoded budget can still reject an individually valid path.
  - Version fixtures reject an added selector kind under v0.1.
  - Prefix-representation fixtures require one canonical domain-path string, permit a structurally valid ancestor that names no definition, reject token-array/display-key/host-path/source-spelling alternatives and noncanonical spellings, and distinguish structural ancestry from serialized-byte prefixes across JSON, YAML, XLIFF 1.2, and XLIFF 2.
  - Inclusive-prefix fixtures match an equal root and every structural descendant, reject neighboring serialized prefixes, and distinguish `Exact(root)` from `Prefix(root)`.
  - Empty-prefix fixtures reject `Prefix("")` in every domain, require `AllInScope` for an intentionally complete scope-domain selection, and retain `Exact("")` only where the selected domain admits an empty root key.
  - Pattern-semantic fixtures apply the complete fixed structural grammar, matching, ordering, work-accounting, and limit contract above.
- The selector and domain conformance matrix is shared by `intlify_contract` and `intlify_linker`.
  - Cover `Exact`, `Prefix`, and `Pattern` over every admitted built-in domain.
  - Cover structural-token boundaries and reject serialized-byte or intra-token pattern matching.
  - Cover escaped separators and literal pattern characters once their grammar is fixed.
  - Require explicit no-normalization behavior and invalid-selector rejection.
  - Exercise the same serialized key under different domains.
  - Include producer goldens whose canonical selectors are evaluated without definition-side expansion.

### Linking semantics and delivery

- Ambiguous-definition fixtures group by exact `(scope, domain, key, locale)` and emit one always-blocking `ambiguous-message-definition` per group with every colliding `SourceDocumentIdentity + EntryReference`.
  - They cover two and many colliders within one source and across sources, equal and unequal payloads, equal producer/fingerprint/source-byte evidence, duplicate host keys with distinct occurrences, same key in distinct locales or domains remaining non-colliding, and physical aliases producing one definition set rather than a false collision.
  - Permutation fixtures order groups by canonical logical identity and evidence by source, then entry.
    - Keep that order independent of artifact, configuration, discovery, hash-map, and worker order.
    - Reject first, last, path, or fallback precedence and payload-based coalescing.
    - Admit no truncated or partial evidence.
    - Report every collision group rather than stopping at the first.
    - Return `Ok(LinkOutcome)` with `bundle_plans().is_none()`, not an operational error or a plan containing an ambiguous definition.
- Ambiguity-suppression fixtures derive the exact ambiguous `(scope, domain, key)` set after collecting every collision group.
  - They suppress `unresolved-message`, `unused-message`, `missing-translation`, and `orphaned-translation` for every locale of those keys, including a collision confined to one locale, without treating any collider as a successful resolution or absence fact.
  - They retain all ambiguity evidence, `unbounded-dynamic-reference`, `degraded-analysis`, and every applicable finding for unrelated exact keys; prove one ambiguous key does not stop analysis of the rest of the request; and reject suppression by source, one locale only, selector breadth, payload equality, or finding discovery order.
- Reference-artifact identity fixtures cover only the project namespace and contextual project-cache binding.
  - Reject a published `Project` claim and every package namespace.
  - Require a non-empty segment array.
  - Preserve exact Unicode bytes and segment boundaries.
  - Reject empty, `.`, `..`, `U+0000`, and slash-containing segments without normalization.
  - JSON fixtures require exactly `namespace` then `segments` on canonical output, accept non-semantic object-member order, preserve array order, and reject slash-string, host-path, missing, duplicate, unknown, or mistyped alternatives.
  - Limit fixtures accept exact boundaries and reject the first value over 255 decoded UTF-8 bytes per segment, 64 segments, and 4,096 decoded segment bytes per identity.
    - Preflight counts before array retention.
    - Use checked cumulative arithmetic in array order.
    - Charge repeated values and shared prefixes without interning deductions.
    - Exclude framing, display separators, and the fixed namespace discriminant.
    - Apply accepted zero or lower caller budgets.
    - Fail the complete artifact without truncation, merging, hashing, or replacement.
  - Equality and permutation fixtures compare the fixed namespace then exact segment sequence, reject duplicates in one request, and prove absolute/current/output paths, content or hash, producer identity/revision, delivery unit, discovery/input order, and random values neither derive nor disambiguate identity.
- Delivery-unit identity fixtures require a project-contextual non-empty segment-array JSON value and the same exact scalar preservation as reference-artifact segments; reject an empty array, invalid segment, slash-string, namespace object, platform union, numeric index, host/output path, and `null`; and prove display rendering cannot round-trip as identity.
  - Limit fixtures accept exact 255-byte segment, 64-segment, and 4,096-byte ID boundaries; reject each first-over case with checked accounting before exposure; charge repeated segments without interning deductions; apply zero/lower caller budgets; and produce no truncation, hashing, replacement, or string fallback.
  - Assignment fixtures derive IDs only from the current application's deterministic pre-output logical graph and reject package-local references plus path/hash/position/worker/random/registration-derived values.
  - Built-in single-unit fixtures require the exact `["main"]` ID, one-node/zero-edge graph, and one derived real root for CLI, editor, and N0 whole-program requests; assign every built-in JS artifact and configured external snapshot to that node; place configured roots there once; and reject another valid artifact ID as a missing node without rewriting or implicit creation.
    - They construct the graph through ordinary effective-limit admission, keep every M3 target and `--target` selection on the same graph and plans, expose no `messages.deliveryUnit` field, and permit a custom or M4 integration to supply another fully checked graph only with matching artifact IDs.
  - Graph fixtures require unique exact node IDs and exactly one matching existing node per reference artifact; allow several artifacts per node and nodes without artifacts; reject missing or duplicate nodes without implicit creation; and preserve exact equality/order across graph, artifacts, findings, plans, cache, and structured output while proving a changed delivery unit does not change artifact/record identity.
- Reference-request aggregate fixtures accept zero and exact boundaries at 65,536 submitted artifacts and 64 MiB of identity segment bytes, and reject the 65,537th artifact or first aggregate byte over its ceiling.
  - Preflight artifact count before per-artifact work.
    - Require complete validation of every artifact before aggregate selection.
    - Accumulate every admitted identity once with checked arithmetic in the first aggregate pass.
    - Trigger the count and byte ceilings independently.
    - Charge duplicates and artifacts rejected by later cross-artifact rules.
    - Take no deductions for sorting, filtering, deduplication, cache reuse, worker partitioning, interning, or shared prefixes.
  - They exclude the fixed project namespace discriminant from the identity-specific total, apply zero and lower caller budgets, preserve count-before-byte failure precedence, and return no partial findings or plans, dropped/split artifacts, truncated identity, or internal relaxed-limit retry.
- Reference-record limit fixtures accept empty and exact-boundary arrays at 1,000,000 records per artifact and an exact 4,000,000-record request total, then reject the 1,000,001st per-artifact or 4,000,001st aggregate occurrence.
  - They preflight known lengths before record allocation/decoding, use checked conversion and cumulative `u64` arithmetic, independently trigger both counters, place the request total in the second complete aggregate pass between identity bytes and decoded bytes, charge equal/duplicate/later-invalid and later-suppressed records, and take no deductions for sorting, grouping, interning, presentation, cache reuse, sharding, or worker partitioning.
  - Zero and lower caller budgets admit only the corresponding empty collections; every failure returns no truncated artifact, partial identities, sampled findings, partial plans, automatic shards, or relaxed-limit retry.
- Reference-record identity fixtures construct the identity exactly as `ReferenceArtifactIdentity + zero-based u32 ordinal` over the preserved semantic `references` array.
  - They cover an empty array, ordinal `0`, exact maximum admitted ordinal `999,999`, checked length conversion, and complete-artifact rejection before identity exposure at 1,000,001 records.
  - They reject duplicate artifact identities before semantic analysis and prove producer revision, delivery unit, request input order, content hash, origin, selector, and other record content never disambiguate or replace identity.
  - Reordering, insertion, and deletion change every affected ordinal; changing only optional reason or origin at an unchanged artifact identity and ordinal does not; and identical records at two ordinals remain distinct.
  - Structured round trips preserve array order without serializing a redundant per-record ordinal, while finding subjects, equality, cache keys, structured output, and within-kind ordering use artifact identity followed by ordinal.
- Reference-finding granularity fixtures emit separate findings for every admitted reference record even when scope, domain, selector, reason, origin, delivery unit, or missing key is equal.
  - They cover distinct `ReferenceRecordIdentity` values with otherwise equal semantic fields, distinct call sites, and origin-less native records without aggregation or deduplication.
  - M2 fixtures produce one `unresolved-message` per record with the complete canonical non-empty set of only the requested locales whose exact chains fail, rather than one finding per locale; M0 retains the same identity shape.
  - Missing-translation fixtures produce one finding for every distinct reference-record, requested-locale, and resolved-key gap. They keep two keys selected by one bounded selector separate, keep two reference records selecting the same key separate, and never aggregate gaps into a location vector merely because their selected locale or fallback chain is equal.
  - Dynamic fixtures produce one `unbounded-dynamic-reference` per `UnboundedDynamic` record in both modes and no `unresolved-message` or duplicate `degraded-analysis` for it; attach `AllInScope` degradation to its exact reference record; and prove presentation grouping never changes core or machine-result records or counts.
- Rust finding-union fixtures admit exactly the seven `LinkFindingRecord` variants and the two closed `DegradedAnalysisFinding` variants, derive one matching `LinkFindingKind`, and expose only read-only record and per-kind subject/evidence access.
  - Compile-fail and API-surface fixtures reject public struct literals, setters, unchecked or generic constructors, independent kind/record pairs, nullable supersets, generic maps, `serde_json::Value`, `Any`, custom/unknown variants, direct deserialization, and every cross-kind subject/evidence pairing.
  - Exhaustive-match fixtures compile complete wildcard-free matches over `LinkFindingKind`, `LinkFindingRecord`, and `DegradedAnalysisFinding`; reject `#[non_exhaustive]`, `Unknown`, `Other`, and `Custom`; and require a promoted variant to make stale exhaustive fixtures fail until every coordinated consumer is updated.
  - Adapter fixtures prove the typed union projects into the exact machine union without serializing Rust discriminants, field layout, debug output, or an intermediate generic value.
- Finding-order fixtures map the seven kinds to explicit precedence values `0..=6` in the declared contract order and exercise every cross-kind pair and multi-kind permutation.
  - Within each kind they exercise every component boundary and adjacent tie-break in the exact subject/evidence table, including optional `None` before `Some`, exact reason/origin ordering after subject, vector lexicography, source/span and source/entry ordering, compat before strict, and degraded variant-before-subject ordering.
  - They prove suppression precedes sorting, sorting does not deduplicate distinct findings, and rendered text/path/line-column, severity, blocking, config/artifact/discovery order, hash-map iteration, phase scheduling, worker completion, enum declaration/discriminant/derived field order, memory layout, and canonical JSON bytes do not define precedence.
  - Sequential, cached, incremental, partitioned, and parallel implementations reproduce the same explicit typed order. A changed or added kind/tie-break requires coordinated compatibility fixtures.
- Linker finding goldens exercise M0 directly without linking `intlify_lint`; later L0/L1 adapter fixtures prove one-to-one kind, subject, evidence, ordering, and operational-error mapping into the 008/013 surfaces.
- Core API fixtures distinguish `Ok` semantic outcomes from `Err` operational failures: non-blocking findings retain `Some` plans, any blocking finding returns the complete finding set with `bundle_plans: None`, a valid no-output link returns `Some(Vec::new())`, and every operational error returns no partial outcome.
  - Reentrant concurrent calls and a cached incremental wrapper must produce the same ordered outcome as independent full calls for equal logical requests.
- Link-outcome API fixtures require private findings/plans storage and exact read-only `findings()`, `bundle_plans()`, and `generation_blocked()` access; distinguish `None`, `Some(empty)`, and `Some(non-empty)`; and prove the derived boolean is true exactly for `None`.
  - Compile-fail fixtures reject struct literals, public fields, mutable slices, setters, independent generation-blocked state, public constructors, deserialization, and post-construction replacement or revalidation. No caller can reorder findings or pair blocking findings with plans.
  - Ownership fixtures destroy every request input, temporary index, and worker arena immediately after `link` and continue to inspect the complete result, proving no public request lifetime, pointer identity, arena handle, or caller-owned storage dependency escapes.
  - Static trait assertions require `LinkOutcome`, `LinkFinding`, every reachable typed subject/evidence value, `MessageBundlePlan`, and `ResolvedMessage` to be `Send + Sync`. Concurrent readers of one externally `Arc`-wrapped outcome observe identical slices without interior mutation; fixtures do not require `Clone` or any specific private sharing/storage representation.
- Definition-location API fixtures require the one linker-owned private `{ source, entry }` composition, exact read-only accessors, equality over both complete components, and canonical source-then-entry ordering.
  - Contract-component trait fixtures require `SourceDocumentIdentity` and `EntryReference` to provide the `Clone`, equality, ordering, and hashing capabilities needed by the composition without changing their private checked construction.
  - Compile-fail fixtures reject public construction, literals, setters, mutable references, deserialization, defaults, unchecked conversions, and partial builders; the linker alone constructs locations from admitted definition records.
  - Trait fixtures require `Clone`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, and `Hash`, prove that cloning preserves exact identity and process-local hash equality, and reject `Copy` plus use of Rust hash output or hasher choice as canonical order, persistent cache identity, fingerprint, wire value, or machine codec.
  - Reuse fixtures require findings, resolved messages, mapped diagnostics, and prune selection to use the same type rather than duplicate source/entry pairs, while producer, version, fingerprint, alias, index, span, payload, scope, domain, key, and locale remain outside location identity.
- Bundle-plan API fixtures require private fields and exact read-only accessors for `delivery_unit`, requested `locale`, and `messages`; resolved-message fixtures require private fields and exact read-only accessors for resolved scope, domain, key, definition locale, exact payload, and definition location.
  - Compile-fail fixtures reject public literals, constructors, setters, mutable slices, deserialization, post-construction normalization, and independently supplied inconsistent selection fields.
  - Ownership fixtures drop every definition artifact after `link` and continue to inspect exact payload and location snapshots through the outcome. Equal repeated placements may share private immutable storage, but pointer identity, storage topology, and sharing never change equality, ordering, accounting, export validation, or observable lifetime.
- Plan-universe fixtures require the complete delivery-node × production-locale Cartesian product, retain empty plans, order plans by canonical delivery unit then exact requested-locale bytes, emit exactly one plan per pair, and produce `Some(Vec::new())` only for a valid empty graph.
  - They reject sparse non-empty-only output, pair omission, duplicate pairs, target- or exporter-dependent filtering, configuration-order enumeration, inferred plans, and exporter reconstruction of missing pairs.
- Resolved-message identity fixtures deduplicate repeated references and configured reachability by exact `(resolved scope, domain, canonical key)` inside one plan and count equal selections again in another locale or delivery unit.
  - They preserve definition locale, exact payload, and one `DefinitionLocation` as attributes; reject location-, payload-, and definition-locale-based identity; and treat unequal selected attributes for one equal logical identity as a fail-complete internal invariant rather than first/last-wins or two runtime records.
  - Ordering fixtures use resolved scope, domain contract order, then canonical-key bytes and prove definition location, payload, fallback position, reference order, artifact order, hash-map order, and worker completion cannot alter it.
- Finding-result count fixtures accept zero and exactly 1,000,000 final retained findings and reject the 1,000,001st as `FindingsTotal`, `Request`, and exactly `Exact(effective_limit + 1)`.
  - They apply suppression before accounting; count every retained blocking and non-blocking record once; keep distinct reference, locale, key, and definition findings separate; and take no deductions for grouping, equal evidence, targets, interning, cache reuse, partitioning, or worker scheduling.
  - They exercise independent zero and lower caller limits, preflight count before result-vector allocation and byte accounting, and expose no complete larger count.
- Finding-result byte fixtures accept zero and exactly 256 MiB of retained variable semantic payload and reject the first complete scalar addition above as `FindingBytesTotal`, `Request`, and its exact attempted running total.
  - Across every finding variant they charge each scope name, key, locale occurrence, selector/reason payload, artifact/delivery/path segment, entry structural path, origin path, ambiguity location, and completeness contributor scope in exact subject-then-evidence and canonical-vector order.
  - They charge equal/interned/shared values and repeated fields independently, including a selected locale repeated at the end of `probedLocales`; apply zero/lower caller values; and prove cached, fresh, sequential, partitioned, and parallel construction select the same first attempted total.
  - They add zero for JSON framing/escaping/member names, fixed kind/namespace/domain/side/reason-variant/mode tokens, blocking, fixed integers, container/storage overhead, and presentation text.
- Finding-result failure fixtures make `FindingsTotal` win over `FindingBytesTotal`, return no `LinkOutcome`, findings prefix, truncation marker, blocking subset, or plans, and invoke no exporter, registration, check, prune, or lint-adapter work.
  - CLI fixtures map either counter to top-level `message_link_failed` / `limit` with exit `2`, omit `analysis`, and reject target-local placement or an empty checked-outcome placeholder. A new complete request may raise only a caller-selected lower value up to the fixed ceiling; no internal retry, shard, drop, or automatic relaxation exists.
- Bundle-plan count fixtures accept zero and exactly 1,048,576 canonical plan pairs and reject the next pair as `BundlePlansTotal`, `Request`, and exactly `Exact(effective_limit + 1)`.
  - They compute the checked node × locale product before proportional plan storage, retain empty plans in the count, prove the protocol input maxima produce exactly 67,108,864 candidate pairs without arithmetic overflow, and apply independent lower limits.
  - A zero limit admits only a valid empty graph; no sparse output, target filtering, partitioning, or internal retry recovers an overrun.
- Resolved-message count fixtures accept zero and exactly 4,000,000 final deduplicated placements and reject the next as `ResolvedMessagesTotal`, `Request`, and exactly `Exact(effective_limit + 1)`.
  - They count one same-plan logical identity once after repeated-reference deduplication, count equal selections again across plans, and give no deduction for fallback reuse, equal definition location, interning, cache reuse, target count, partitioning, or worker scheduling.
  - A zero limit admits the complete plan matrix only when every plan is empty.
- Bundle-plan byte fixtures accept zero and exactly 1 GiB of retained plan semantic payload and reject the first complete scalar addition above as `BundlePlanBytesTotal`, `Request`, and its exact attempted running total.
  - In canonical plan/message/field order they charge every delivery-unit segment and requested locale per plan, then every resolved scope name, key, definition locale, payload, source-path segment, and entry structural path per placement.
  - They charge equal and privately shared values repeatedly; add zero for namespace/domain discriminants, occurrence integers, framing, lengths, storage overhead, exporter/reporting bytes, and target options; and prove fresh, cached, sequential, partitioned, and parallel construction selects the same first attempted total without `ArithmeticOverflow`.
  - A zero byte limit admits only `Some(Vec::new())` because every non-empty plan has non-empty delivery-unit and locale payloads.
- Plan-result precedence fixtures run only after both finding counters admit the complete finding set.
  - A blocking finding returns the complete `bundle_plans: None` outcome without evaluating any plan counter.
  - For a non-blocking result, `BundlePlansTotal` wins over `ResolvedMessagesTotal`, which wins over `BundlePlanBytesTotal`; every failure returns no `LinkOutcome`, finding prefix, plan prefix, omission marker, batch, or exporter work.
  - CLI fixtures map all three counters to top-level `message_link_failed` / `limit` with exit `2` and no `analysis`; retries may change only valid caller-selected lower limits and never the plan universe.

#### Resource-limit conformance

- **Inherited 013 limits:** cover protocol-default ceilings and exact and first-over boundaries for inherited 013 `host_bytes`, `entries`, per-message bytes, total-message bytes, and distinct identity bytes.
  - Cover zero-entry and empty-message cases.
  - Project exactly one definition for every admitted entry.
  - Perform no second host parse or identity interpretation.
- **Artifact byte limits:** independently cover exact and first-over boundaries at 512 MiB wire bytes and 256 MiB decoded bytes.
  - Include wire-heavy artifacts and compact-wire, expanded-decoded artifacts that fail only the applicable counter.
  - Assume no ratio between the counters.
  - Charge envelope syntax, invalid trailing input, unescaping, binary decoding, and artifact-local reference expansion to the counter specified by the contract.
  - Charge decoded values per occurrence without string-table, zero-copy, or interner deductions.
  - Require both overlapping field-specific and decoded limits.
  - Bound streaming decompression into the decoder.
  - Add no synthetic wire charge for checked direct typed construction.
- **Portable-path limits:** cover exact and first-over boundaries for 4 KiB segment bytes, 1,024 segments, 256 KiB path bytes, 4,096 aliases, and 64 MiB cumulative source-path bytes.
  - Exclude framing and display separators from path bytes.
  - Accumulate without shared-prefix or interner deductions.
  - Cover empty aliases, the possible 4,097 total paths, and invalid empty primary paths.
  - Accumulate the primary path and then canonical aliases with checked arithmetic.
  - Perform no host-filesystem limit lookup.
  - Reject the whole group without extraction, truncation, subset selection, or primary-path changes.
  - Require consumers to revalidate that aliases are sorted and duplicate-free.
- **Caller-selected limits:** accept zero and lower caller budgets.
  - Permit an intentional lower limit to reject an otherwise valid 013 extraction.
  - Reject rather than clamp an above-ceiling `LinkLimits` value.
  - Never allow artifacts to raise budgets.
- **Evidence and accounting:** distinguish locally recheckable source facts from remotely observable artifact counters.
  - Preflight byte and count limits before allocation.
  - Use checked cumulative arithmetic and reject conversion overflow.
  - Preserve identical observable counter semantics during production, decoding, and defensive link validation.
  - Select the first overrun deterministically.
- **Failure and retry:** return no partial artifact, decoded value, findings, plan, or cache admission.
  - Retry only through a new request with larger valid limits.
  - Revalidate cached artifacts under changed limits.
  - Keep fingerprints and semantic outcomes independent of the lower limit that admitted an artifact.
  - Increase ceilings only through a versioned change and never reduce a ceiling within the same major version.
- Delivery-graph resource-limit fixtures accept empty and edgeless graphs and exact boundaries at 65,536 submitted nodes, 1,048,576 submitted directed edges, and 64 MiB of node-ID segment bytes; reject the first occurrence or byte above each ceiling; and exercise checked conversion/addition plus independent zero/lower caller values.
  - Admission-order fixtures require node-count preflight, edge-count preflight, complete per-ID validation, canonical ID-byte aggregation, and only then graph-semantic validation; a later phase never wins over an earlier one.
  - Canonical-reduction fixtures order exact `DeliveryUnitId` groups, charge duplicate occurrences through one checked group subtotal without deduction, stop at the first over-limit group, and require sequential/parallel equivalence.
  - Evidence fixtures map node/edge counts to `DeliveryGraph` and ID-byte excess to the exact selected `DeliveryUnitGroup`, rejecting mismatched or unbounded subjects.
  - Trigger per-ID and aggregate-ID limits independently.
    - Prove that 65,536 nodes need not admit the theoretical 256 MiB maximum-ID product.
    - Charge duplicate or later-invalid nodes and edges without sorting, deduplication, pruning, interning, adjacency compression, cache, or worker deductions.
    - Count every node ID once while edge references add no node-ID payload charge.
    - Return no truncated, dropped, partitioned, retried, or partial graph or request.
- Delivery-graph semantic and placement fixtures interpret every edge as `parent -> child`, derive real roots only from indegree zero, admit multiple roots and disconnected finite DAG components, and reject duplicate nodes, duplicate edges, unknown endpoints, self-edges, and cycles after numeric admission.
  - They require every reference artifact to bind one existing node; reject configured roots with an empty graph; and place each configured root in every real root.
  - M0–M3 fixtures admit omitted or explicit `duplicate`, reject `hoist` and every unknown token without fallback, retain one equal resolved message per `(delivery unit, locale)` despite repeated same-unit references, retain one copy in each of several referencing units, and prove ancestor shape never relocates duplicate placement.
  - Future-hoist fixtures remain M4-only and must prove that a virtual super-root is never emitted as a delivery unit.
- JS recognizer fixtures require exact `kind`, `scope`, `domain`, and `keySyntax` fields; accept only `lookup` and `set` call kinds; reject every omission, default, inferred value, unknown token, scope/domain mismatch, and non-`canonical` syntax for YAML or XLIFF.
  - Call-kind fixtures map a static `lookup` value to `Exact`, a non-static `lookup` value to `UnboundedDynamic`, and a valid static `set` value to `Pattern`; reject a dynamic or invalid `set` value fail-complete; and prove callee spelling, return use, argument text, imported name, and type information never infer or change kind.
  - Callee-shape fixtures match only exact direct identifiers and non-computed, non-optional static member chains rooted at an identifier or `this`; cover `t`, `i18n.t`, `i18n.global.t`, and `this.$t`; and reject computed, optional, call/new/tagged/private/super-derived, dynamic, expression-evaluated, normalized, and suffix-fallback forms.
    - Callee-key fixtures accept exact 1- and 255-byte plus 1- and 64-segment boundaries under `[A-Za-z_$][A-Za-z0-9_$]*`; accept special root `this`; reject the first-over boundaries, empty components, leading/trailing/consecutive dots, non-ASCII, and every fixed reserved root other than special `this`; permit reserved spellings in later property position; and canonicalize valid recognizers by exact ASCII bytes without trimming, folding, normalization, or source/map-order effects.
    - Syntactic-binding fixtures require equal callee shapes to match across imports, declarations, lexical shadowing, unrelated runtime values, and `this` contexts; reject module/symbol/type/provenance lookup and differently spelled aliases; preserve each source origin; and prove only include selection or exact callee syntax changes participation in M0.
  - Dot-path fixtures distinguish separator `.`, escaped `\.`, and escaped `\\`; reject empty, leading/trailing/consecutive segments, dangling backslash, and every other escape; convert literal segments through exact JSON Pointer `~0` / `~1` escaping; and preserve literal asterisks for normal lookup calls.
  - Bounding-call fixtures make only whole `*` / `**` dot segments into pattern operators, escape every literal asterisk as `~2`, and prove call kind rather than source spelling selects `Exact` versus `Pattern`.
  - Canonical/literal fixtures cover empty-key intent and validate without syntax guessing or retry.
- Scope-completeness fixtures construct exactly one canonical record per resolved target-inventory scope, including a scope whose configured input failed before producing an artifact; reject missing, extra, duplicate, unsorted direct values, side-inapplicable reasons, and artifact-authored or raw-config-authored closure claims.
  - Execution fixtures derive `Closed` only from complete inventory participation, accept a closed zero-reference producer result, apply fixed partial-reason precedence, and fold many-to-one mappings so any partial source keeps the resolved side partial.
  - Semantic fixtures retain ambiguity and present-reference analysis where valid; suppress unresolved/coverage absence claims for definition-partial scopes and unused findings unless both sides are closed; emit one canonical `degraded-analysis` per partial side; and return complete findings with `bundle_plans: None` for any targeted partial scope.
  - Command fixtures require both sides closed for `emit` and additionally no unbounded-dynamic degradation for `prune`; cache fixtures invalidate only semantic results when the completeness table changes.

### End-to-end behavior

- Linker semantic goldens: resolution, chain fallback, reachability, placement — deterministic across runs and platforms.
- The constructive invariant, executed: every emitted target re-links clean — zero `unresolved-message`, zero `unused-message` when pruned.
- Degradation tests: reference-record `AllInScope` and each partial completeness side produce their exact `degraded-analysis` variant, while configured roots, `Prefix`, `Pattern`, and unavailable open-world-participant codecs do not.
- Dynamic-policy fixtures require omitted and explicit `compat` configuration to resolve to the same immutable policy, fingerprint, cache identity, findings, and plans. They consume the same `UnboundedDynamic` artifact in strict and compat modes: strict returns one blocking finding and no valid plan; compat returns the non-blocking finding, retains the exact scope-domain pair, suppresses `unused-message` only there, and does not duplicate `degraded-analysis`. They reject `null`, non-string values, case or whitespace variants, and every unknown token without coercion or fallback to the default.
- Native format-survival fixtures: strip/LTO/COMDAT matrices proving tagged IDs survive (or over-retain, never under-report).
- Prune safety under the separate 013 structural-mutation invariants; byte-determinism with `--check`.
- Benchmarks measure the problems: initial-bundle bytes, per-locale and per-unit asset bytes before/after, scan and link wall time — `tools/messages-bench`.

### Field-level limit evidence

- Definition-identity field-limit fixtures accept an empty root `EntryStructuralPath` and every domain-valid empty root `CatalogKey`, accept each exact 64 MiB decoded boundary, and reject the first decoded byte above each independently under protocol and caller-selected lower limits.
  - They require the closed `EntryStructuralPathBytes` and `CatalogKeyBytes` variants with exact structured spellings, only `DefinitionArtifactGroup(source)`, and exactly `Exact(effective_limit + 1)` without a raw field, `EntryReference`, definition/occurrence index, complete-length scan, or `ArithmeticOverflow`.
  - Precedence fixtures run complete non-interleaved raw-entry passes for `locale_bytes`, `entry_structural_path_bytes`, and `catalog_key_bytes` in that order after source identity and definitions-count admission, require an earlier pass to win over an earlier-occurrence failure in a later pass, and prove identical selection under JSON-member, input, duplicate-source, partition, and worker permutations.
  - Accounting fixtures charge every occurrence again to `definition_artifact_decoded_bytes`, including equal and interned values, keep those counters and reference-side `selector_path_bytes` distinct, and prove that no `definition_identity_bytes_total` counter or consumer-side reconstruction of 013's distinct interner accounting exists.
  - An enclosing per-artifact or request decoded-byte limit may still reject individually valid values.

- Definition-message limit fixtures accept empty messages and the exact 1 MiB per-message and 64 MiB per-source-total boundaries; reject the first per-message byte above and the first raw-entry addition whose exact checked sum exceeds the total; and apply independent zero and lower values.
  - They require `MessageBytes` with `Exact(effective_limit + 1)` and `TotalMessageBytes` with the exact attempted running sum, both using only `DefinitionArtifactGroup(source)` and never `ArithmeticOverflow`, a message copy, record/index evidence, or `EntryReference`.
  - Admission fixtures run this one raw-entry message pass only after the four fixed locale/catalog-scope-name/structural-path/catalog-key passes, check the per-message counter before adding that record to the total, let an earlier total overrun beat a later per-message overrun, and preserve the same result under decoder, cache, duplicate-source, partition, and worker permutations.
  - Accounting fixtures charge empty and repeated values without deductions and require both inherited counters plus the overlapping artifact decoded/wire counters to pass independently.

- Catalog-scope-name limit fixtures accept exact 1- and 255-byte names, reject empty grammar and the first byte above the effective ceiling independently, and apply zero/lower values without normalization.
  - They require the closed `CatalogScopeNameBytes` variant, exact `catalog_scope_name_bytes` spelling, and exactly `Exact(effective_limit + 1)` without `ArithmeticOverflow`, a complete-length scan, raw `CatalogScopeName`/`CatalogScopeId`, or any record/mapping index.
  - Subject fixtures use only the exact owning `ReferenceArtifactGroup`, `DefinitionArtifactGroup`, `ResolvedPolicy`, or payload-free `ScopeMappings` context and reject `Request` or every cross-context pairing.
  - Precedence fixtures place the complete name-byte pass at the owner-specific point:
    - for definitions, after `locale_bytes` and before structural-path, key, and message admission;
    - for references, after identity, delivery-unit, and record-count checks but before remaining record validation;
    - for the resolved policy, after target semantics and before remaining root validation; and
    - for mappings, after mapping-count preflight, checking source and then target in submitted entry order, before remaining endpoint and mapping semantics.
  - They require a future scope-bearing policy collection to obtain its own occurrence representation, count bound, and explicit phase rather than silently joining the configured-root pass.
  - All decoder, direct-construction, cache, duplicate, and worker paths expose the same counter, subject, limit, observation, and serial winner.

- Resolved-policy catalog-scope precedence fixtures require one complete submitted-root-order `catalog_scope_name_bytes` pass after target semantics, followed by remaining root-field validation, equal checked `(scope, domain, selector)` duplicate rejection, canonical root ordering, and only then the remaining policy semantics.
  - They require `ResolvedPolicy` with `Exact(effective_limit + 1)` for a root-name overrun and prove that a future coverage-baseline mapping cannot participate until its own occurrence-preserving representation, count ceiling, and policy phase are fixed.

- Selector-limit evidence fixtures require the closed `SelectorPathBytes`, `SelectorPatternBytes`, and `SelectorPatternTokens` variants at ordinals 22 through 24 and the exact `selector_path_bytes`, `selector_pattern_bytes`, and `selector_pattern_tokens` spellings.
  - They accept the exact 64 MiB, 128 MiB, and 513-token boundaries and reject each first value above the effective limit with the established `ReferenceArtifactGroup(identity)` and exactly `Exact(effective_limit + 1)`.
  - They reject `Request`, raw selector/pattern/token payloads, record ordinals, complete submitted lengths or token counts, and `ArithmeticOverflow`.
  - Decoder, direct-construction, producer, cache, partitioned, and parallel fixtures select identical evidence; `Exact` and `Prefix` share only `SelectorPathBytes`, while `Pattern` passes `SelectorPatternBytes` before parsing and `SelectorPatternTokens` while admitting parsed tokens.
  - Checked-addition and host-size-conversion coverage proves that alternate failure forms are unreachable under bounded first-over admission; those forms are not accepted.
  - Cross-record precedence fixtures require the complete scope-name pass, then the complete ordinal-order selector-path byte pass, then the complete ordinal-order pattern-byte pass, then the ordinal-and-segment-order pattern parse/token phase, and only afterward remaining record validation.
  - They prove that the first structural parse error or token overrun in the third phase wins without fabricating a token count or deferring the parse error to search later records, and that worker completion cannot change the selected phase or failure.

## Relationship to Other Documents

| Document | Relationship |
| --- | --- |
| [013-ox-mf2-resource-catalog-adapter-design.md](./013-ox-mf2-resource-catalog-adapter-design.md) | definition side: complete per-document extraction artifacts projected outside the resource crate into one linker artifact per selected source, entry identity/spans, canonical catalog-key/domain production without selector matching, explicit project-local scope and locale binding, formatter value write-back, and the separate M5 structural-mutation boundary used by `prune` |
| [006-ox-mf2-phase-3a-tooling-foundation-design.md](./006-ox-mf2-phase-3a-tooling-foundation-design.md) | config envelope, schema pipeline, reporters, operational errors, exit codes |
| [008-ox-mf2-phase-3c-linter-design.md](./008-ox-mf2-phase-3c-linter-design.md) | later presentation adapter: linker findings ship as rules through its contracts and the catalog-level addendum, without gating linker M0 |
| [003-ox-mf2-phase-2-binary-ast-snapshot-design.md](./003-ox-mf2-phase-2-binary-ast-snapshot-design.md) | candidate payload representation; format-version precedent |
| [009-ox-mf2-phase-3d-lsp-editor-design.md](./009-ox-mf2-phase-3d-lsp-editor-design.md) | editor surfacing; incremental open-world findings |
| `refers/formatjs` | code-first reference-producer comparison recorded in Message-Domain Prior Art |

## Deferred Follow-Up Notes

The following work is intentionally outside the current milestone contracts and does not block their implementation.

### Artifact v1 Freeze

M0 deliberately ships the mutable WIP v0.1 artifact contract. A v1 compatibility promise is not inferred from implementing M0, publishing a crate, or allowing independent producers to exercise the v0.1 conformance suite.

The freeze decision is reconsidered only after N0 has validated final-binary scanning, native reference-ID recovery, and any resulting envelope constraints.

Promotion must be explicit and coordinated across contract types, JSON schemas, writers, readers, negotiation, fixtures, and documentation; only that promotion starts the stable same-major compatibility promise.

### CLI Plan Inspection

`MessageBundlePlan` remains an internal typed result through M0–M3. Tests and in-process integrations may inspect it directly, but `intlify messages emit` does not serialize or dump it and no public plan wire schema is implied.

A CLI inspection mode is added only when a concrete consumer needs one.

That follow-up must define its versioned schema, output and resource limits, path-redaction and security behavior, whether blocked links emit any inspection payload, reporter/exit-code interaction, and compatibility with `--check`; it cannot expose an implementation debug representation as an accidental public contract.

### Strict Coverage Generation Policy

M2 and M3 emit `missing-translation` as a non-blocking finding. `coverageBaseline` selects a comparison and typed-key baseline; it is not a hidden strictness switch. No initial config field, CLI flag, environment variable, lint severity, reporter, target, or exporter promotes a coverage finding into a generation blocker.

A future strict coverage policy may be promoted only with a concrete product requirement. Its design must define:

- the raw config shape, omission/default behavior, scope and locale granularity, milestone admission, generated schema, and section-local validation order;
- the exact checked `LinkPolicy` member and whether it affects only `missing-translation` or another coverage finding;
- interaction with fallback, coverage baselines, completeness suppression, target-independent all-or-nothing plans, and the machine `blocking` field;
- policy equality, request/cache identity, deterministic finding order, and plan invalidation;
- independence from configurable lint severity and preset membership; and
- CLI summary, text/JSON reporting, exit behavior, migration, and conformance fixtures.

Until that follow-up is promoted, no dormant field name or strict alias is reserved or accepted.

### CLI Export-Diagnostic Retention Control

The initial M3 CLI uses the fixed retention limit of 1,000 and exposes no user control. Programmatic integrations may use the existing checked `0..=10,000` argument without changing linker or artifact identity.

A future CLI option or config field requires a demonstrated need. Its design must define together:

- exact CLI option and configuration-field names, omission/default behavior, and precedence when both surfaces exist;
- the raw numeric domain and width accepted before conversion to `u32`;
- separate decoding and checked-above-ceiling failures;
- the exact `invalid_options` reason, path, submitted value, and maximum-value evidence for each admitted raw surface;
- help text, machine output, and text omission reporting;
- invocation identity and whether the chosen value belongs in any command cache key; and
- conformance fixtures covering exact boundaries and conflicting surfaces.

Reporter choice, target count, environment state, and worker scheduling cannot become implicit limits. Until this follow-up is promoted, no option name, configuration path, raw-value mapping, or `invalid_options` reason is reserved.

### Cross-Call Export Validation Cache

The initial export-preparation implementation retains no process-global, command-external, persistent, or cross-call parser/semantic validation cache. Every `prepare_export` call validates its selected definitions completely.

Within one call it still validates each exact `DefinitionLocation` once. It may reuse computation for byte-identical payloads only as an invisible optimization that preserves each record's independent diagnostic mapping, complete count, ordering, and observable result.

A cross-call cache is promoted only after measurement shows that repeated preparation remains a material cost. Its design must define together:

- exact payload identity, length, digest algorithm and collision confirmation rather than location-only reuse;
- parser implementation revision plus semantic-model and validator revisions;
- whether parser configuration, function registries, data-model features, or other semantic inputs participate;
- separate success and failure caching rules, including complete diagnostic counts and bounded retained evidence;
- location-independent computation versus per-`DefinitionLocation` diagnostic mapping;
- memory and entry ceilings, eviction, concurrency, cancellation, poisoning, and process lifetime;
- invalidation across dependency, feature, target, toolchain, and contract changes;
- interaction with `ExportValidationLimits` without treating a prior retained prefix as a complete result under another limit;
- cache ownership, observability, metrics, persistence and trust boundaries; and
- fresh-versus-cached conformance fixtures under sequential and concurrent calls.

Until promotion, no cache directory, config field, environment variable, serialized validation proof, reusable batch token, or location-only cache key exists.

### Binding-Aware JS/TS Recognizers

M0 recognizers match exact configured callee syntax and deliberately do not claim module, lexical-binding, or type identity. Binding-aware recognition is deferred until measured false positives justify the added language-service cost and configuration.

A promoted follow-up must define whether provenance is declared through import source/export name, composable or factory return, destructuring, dependency injection, global augmentation, framework metadata, or a checked combination. It must cover renamed imports, re-exports, local wrappers, lexical shadowing, JS without a type checker, TS with multiple project configs, Vue SFC script blocks, and unsaved editor buffers without silently changing existing syntactic recognizers.

It must also fix capability negotiation, parser/type-checker and module-resolution versions, source/project discovery, failure-to-partial-completeness mapping, bounded work and memory, cache identity and invalidation, configuration schema and append-only validation order, deterministic source evidence, and fallback behavior. A failed semantic resolver cannot silently revert one configured binding to broad syntactic matching.

### Binding-Aware Static Key Evaluation

M0 evaluates only a selector expression's self-contained string syntax. It deliberately does not propagate file-local constants, imported constants, object properties, or other values obtained through binding or module analysis.

A future evaluator may admit those sources only with a concrete demand and a versioned producer capability. Its contract must define lexical binding and shadowing, declaration order, reassignment and mutability, destructuring, import/re-export and module resolution, cycles, supported expression operators, evaluation limits, source-origin attribution, cache identity, editor-buffer behavior, and failure-to-completeness mapping. Failure of the promoted evaluator cannot silently treat a configured `set` call as valid or convert an uncertain lookup into `Exact`.

### Framework-Declarative Reference Producers

M0's JS/Vue recognizer consumes call expressions only. Component props, directives, macros, route metadata, JSX attributes, and other framework declarations do not acquire message semantics from familiar names such as `i18n-t`, `keypath`, or `v-t`.

A future declarative producer requires a concrete framework integration and must define exact component/directive/macro identity, static and dynamic selector conversion, scope/domain/key syntax, alias and binding behavior, template/preprocessor coverage, source-span mapping, unsupported-value failure, completeness, bounded work, cache identity, and conformance fixtures. It must coexist with call recognition without duplicating equal source occurrences or weakening pruning safety.

Until promotion, projects represent those references with intentional configured roots or a complete external `MessageReferenceArtifact`; no dormant tag/prop recognizer configuration is accepted.

### Missing-Message and Missing-Translation Stub Scaffolding

The initial linker, lint adapter, and resource write-back integration do not insert placeholders for `unresolved-message` or `missing-translation`. They expose findings only.

No placeholder text, source-locale copy, destination catalog, host entry shape, insertion order, or write eligibility can be inferred safely from linker evidence, and a lint autofix must not make those translation-workflow decisions implicitly.

A future explicit scaffolding command requires a concrete workflow and a dedicated design.

That design must:

- distinguish a key absent from every locale from a translation absent in selected locales;
- choose exact target source documents and structural entry identities;
- define MF2 placeholder content and argument compatibility;
- handle duplicate keys, concurrent changes, read-only entries and formats, ordering, and host escaping;
- integrate validated write-back with dry-run, `--check`, atomic commit, and rollback semantics; and
- specify interaction with translation-management systems and generated or externally managed catalogs.

Until that design is promoted, no config placeholder or dormant CLI option is reserved.

### Selector-Breadth Warning Policy

M0 through M3 emit `degraded-analysis` only for a reference-record `AllInScope`. An exact `Prefix` or `Pattern` remains bounded regardless of how many definitions happen to match, and an explicit configured root is reachability policy rather than producer degradation.

A future warning policy may classify another bounded selector as broad only after a concrete UX requirement establishes a stable rule. It must define:

- whether breadth is measured by matched keys, locale-bearing definitions, a ratio within one scope-domain pair, serialized payload, or another exact unit;
- whether the threshold is a protocol constant or checked configuration, including raw shape, omission/default behavior, bounds, validation order, and error evidence;
- evaluation relative to scope mapping, ambiguity suppression, locale projection, fallback, configured roots, and completeness;
- whether configured roots participate and how they obtain a stable typed subject;
- whether the result remains non-blocking, affects only lint presentation, or introduces a separately named policy;
- deterministic accounting, finding granularity and ordering, cache identity, incremental invalidation, and machine codec evolution; and
- behavior when the current catalog changes across the boundary without any source selector change.

Until promotion, no match-count threshold, percentage, broad-prefix alias, CLI flag, or environment override exists.

### Conservative Open-World Participant Degradation

M0 through M3 have no open-world-participant `degraded-analysis` variant. The current contracts cannot state, in one checked portable value, that a package, native library, plugin, or external producer supplied a conservative available-reference set instead of the exact surviving set.

An authoritative configured reference artifact is therefore treated as the exact selected snapshot for its invocation. An input whose completeness cannot be proven contributes the applicable `PartialReason` and produces blocking `partial-completeness`; it is never silently relabeled as safe conservative over-retention.

A future integration may promote a non-blocking conservative-participant variant only together with:

- a concrete typed participant identity appropriate to that integration, without precommitting to a generic string or the deferred `PackageIdentity`;
- versioned exact-versus-available selection provenance and the producer/build boundary authorized to assert it;
- the complete affected resolved scope-domain set and proof that every possibly surviving reference or definition is retained;
- behavior for incomplete, conflicting, stale, or under-inclusive evidence, all of which must fall back to partial completeness or an operational failure rather than a non-blocking claim;
- artifact schema/version, fingerprint, cache identity, trust binding, resource limits, deterministic participant and scope ordering, and compatibility rules;
- the exact `degraded-analysis` subject/evidence codec, finding granularity, blocking disposition, reachability and prune effects; and
- build-integration fixtures for the first concrete JS bundler, native scanner, or plugin model that needs the feature.

Until promotion, the machine union rejects `"open-world-participant"`, generic/custom participant evidence, and arbitrary participant IDs.

### Package-provided resources and published artifacts

M0 focuses on the common application-owned resource workflow. Libraries and resource-only packages can provide catalogs, but that use case does not justify freezing a cross-ecosystem package identity, trust model, or published-artifact contract before a concrete integration needs it.

M0 therefore admits only the contextual `Project` namespace and rejects every package namespace in reference artifacts, definition artifacts, source origins, and catalog scopes.

The following shape is retained only as a candidate for the future design, not as an active or frozen contract:

```rust
pub struct PackageIdentity {
    pub ecosystem: PackageEcosystemId,
    pub source: PackageSourceIdentity,
    pub name: PackageName,
    pub version: PackageVersion,
}

pub enum ArtifactNamespace {
    Project,
    Package(PackageIdentity),
}
```

The follow-up must first validate whether all four fields are necessary. It is activated only by a concrete library or resource-package integration and must then define:

- the package identity codec, field limits, equality, ordering, fingerprint framing, schema/version evolution, and conformance fixtures;
- ecosystem adapters that bind a claim to an authoritative lockfile or resolved build graph without adding package-manager, filesystem, registry, or network behavior to `intlify_linker`;
- package-relative definition sources, published reference artifacts, package catalog scopes, and the trust evidence used to admit each artifact;
- available-reference metadata versus final surviving roots, including how the consuming build integration selects package modules or native IDs and binds them to contextual `DeliveryUnitId` values;
- final-application scope mapping, library root selection, and composition for npm, Cargo, C/C++, and other ecosystems, including the first public raw mapping field, schema, section-local validation order, error evidence, and cache-invalidation contract; and
- aggregate resource accounting for the added namespace payload. The previously considered independent 8 MiB mapping-byte ceiling is only a candidate to re-evaluate if package payloads make the Project-only derived maximum insufficient.

Until this follow-up is promoted, no producer or consumer emits or accepts a package namespace, and the linker performs no package discovery.

An application may use package content only after explicitly materializing or importing it as ordinary application-owned resources under the `Project` contract; that representation carries no package provenance or implicit package semantics.

### Hoisted and Per-Scope Placement

M0 through M3 intentionally support only target-wide `duplicate` placement. Hoisting is deferred until M4 supplies real bundler delivery DAGs whose loading behavior can validate the optimization; per-scope placement is deferred with it because mixed placement changes plan identity, configuration precedence, and runtime loading assumptions.

The candidate M4 design adds one analysis-only virtual super-root with edges to every real indegree-zero root, computes a deterministic dominator tree, and considers the deepest real node that dominates every referencing unit.

The follow-up must define whether a message remains duplicated when no common real dominator exists, how eager and lazy boundaries constrain otherwise valid dominators, how configured roots interact with hoisting, exact tie-breaking and canonical traversal, incremental invalidation, plan/cache fingerprints, and output-size versus load-latency diagnostics.

Fixtures must include multi-root, diamond, shared-async-child, disconnected-component, and changing-chunk-ID graphs from real bundlers. The virtual node can never appear in `MessageBundlePlan` or exporter output.

Until that design is promoted, `hoist` and scope-level overrides are unsupported input rather than accepted no-ops, aliases, or hidden experimental behavior.

### Multiple Exporters in One Export Transaction

A future transaction coordinator may invoke multiple exporters as one atomic export operation after a concrete product requires coordinated outputs that cannot reasonably be represented by one exporter.

Candidate use cases include emitting an ESM representation together with a native or binary representation, producing old and new formats during a migration, serving web and native targets from one build, or combining built-in and explicitly configured third-party exporters.

This follow-up is not required merely because an output contains several files or artifact kinds. Per-locale ESM modules plus a loader map, generated source plus a manifest, and a baked Rust module plus a companion blob remain outputs from one exporter in one `ExportArtifactSet` when they form one target-native representation.

Before multiple exporters can participate in one transaction, a dedicated design must define:

- stable exporter invocation identities, canonical invocation order, and a fixed maximum selected-exporter count;
- reuse of one `ValidatedExportBatch`, bounded parallel scheduling, cancellation, and completion barriers;
- fail-complete collection of every exporter result before any final-build registration;
- deterministic cross-exporter error aggregation without merging evidence into a synthetic `ExportError`;
- cross-set exact logical-path collision checks, mapped-destination collision checks, and kind/version capability preflight;
- atomic staging and commit, or equivalent rollback semantics that prevent user-visible partial output;
- transaction fingerprints, cache identity, and `--check` behavior; and
- whether relationships remain set-local or gain an explicitly versioned cross-set target model.

Until that follow-up is promoted, every export transaction selects exactly one exporter. Separate transactions may reuse the same borrowed batch, but they have independent results, failures, registration, and atomicity boundaries.
