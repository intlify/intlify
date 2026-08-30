# Intlify Project Profile and Locale Policy Design

## Purpose

This design defines how one named input selected from a canonical repository configuration becomes the complete, checked `LocalizationProjectProfile` consumed by shared Intlify compiler stages. The primary repository input is `intlify.config.json`, described by a versioned JSON Schema. An optional programmatic frontend, such as a future `defineIntlifyConfig()`, may construct the same JSON-compatible `IntlifyConfig` data model, but it does not create a second configuration language or bypass validation.

One repository configuration may declare one or more named profile inputs. Each resolver invocation selects exactly one of them and produces the profile for one final-application localization project.

In practical terms, the profile gives every downstream stage the same answers to four questions:

- which localization project and Selection Scope are being processed;
- which source and requested locales apply, including project defaults, target subsets, and effective defaults;
- which Target Profiles, Deployment Compatibility Groups, and delivery inputs belong to the selected build; and
- which versioned negotiation, fallback, coverage, Provider, governance, trust, and resource policies apply.

![High-level role of the Intlify Localization Project Profile](./assets/015-intlify-project-profile-and-locale-policy-overview.svg)

The following example shows the file-first path and the optional programmatic path converging before shared compilation. The programmatic API name is illustrative; its input semantics are not separate from `intlify.config.json`.

![Canonical configuration resolution into one Localization Project Profile before cross-platform compilation](./assets/015-intlify-cross-platform-project-profile-resolution.svg)

The shared Rust crate `intlify_config` owns the `IntlifyConfig` authoring model, configuration-schema version admission, JSON Schema definition and generation, named-profile selection, semantic resolution, configuration Findings, and the `LocalizationProjectProfile` settings IR. It defines provider-driven locale-canonicalization and external-policy-artifact-set boundaries but does not embed raw or generated CLDR data or acquire policy artifacts. `intlify_cli` is a product adapter: it discovers and reads repository configuration, obtains the user-facing selector, supplies an admitted canonicalization data artifact and a finite already acquired Policy Artifact Set through those boundaries, calls `intlify_config`, and renders the outcome. Optional programmatic frontends call the same core rather than depending on CLI internals.

Planning, synchronization, linking, export, Release Assembly, tooling, and execution integrations consume the resulting settings IR instead of rereading configuration or inventing their own defaults. Cross-platform Producers, Lowering Backends, Target Exporters, and Runtime integrations begin downstream of this common configuration boundary. Credentials and other secrets remain outside the profile.

## Goals

- Define what one resolved `LocalizationProjectProfile` represents and how it is identified.
- Define explicit named-profile selection when one repository configuration contains several final-application localization projects.
- Define the semantic split between author-facing `IntlifyConfig` and the checked `LocalizationProjectProfile` settings IR.
- Define `intlify.config.json` and its versioned JSON Schema as the primary repository configuration surface.
- Make `intlify_config` the reusable implementation owner of configuration models, schema generation, profile resolution, and the checked settings IR, while keeping product workflow in adapters such as `intlify_cli`.
- Require file-based and optional programmatic inputs to enter the same resolver with the same semantics.
- Define project requested locales, source-locale defaults, requested-locale defaults, Target Profile subsets, target overrides, and effective defaults.
- Keep requested-locale negotiation separate from message locale fallback and single-message evaluation.
- Define the profile inputs for coverage, Provider routing, approval, Glossary Sets, delivery, trust, and resource policies without taking ownership from their detailed designs.
- Define exact typed references and explicit presence states for externally owned Provider, governance, Glossary, trust, and resource policies without embedding their bodies or credentials in project configuration.
- Separate stable delivery specification and placement policy in the profile from the realized host-build Delivery Unit Graph and physical target output.
- Define how Target Profiles form one or more Deployment Compatibility Groups.
- Define deterministic resolution, validation, Finding production, and consumer-visible dependency inputs.
- Keep the reusable `intlify_config` core free of embedded CLDR-derived data by admitting canonicalization through a versioned provider and separate data artifact.
- Make invalid, ambiguous, incomplete, or incompatible configuration fail before synchronization, linking, export, or production execution.
- Provide paired `IntlifyConfig`, resolved-profile, and Finding fixtures that the shared resolver and downstream consumers can use.

## Non-Goals

- Defining TOML, YAML, framework-specific, or platform-specific configuration formats equivalent to `intlify.config.json`.
- Freezing the name, package, or language binding of an optional programmatic configuration helper such as `defineIntlifyConfig()`; those product-facing details belong to [029](./029-intlify-product-workflow-and-packaging-design.md).
- Defining repository-root discovery, workspace profile selection, command-line option precedence, or configuration UX owned by [029](./029-intlify-product-workflow-and-packaging-design.md).
- Defining formatter, linter, or other unrelated tool-specific sections that may coexist in the root Intlify configuration schema.
- Defining source authoring, `intent()`, `mf2`, Intent identity, or source-evidence rules owned by [016](./016-intlify-source-authoring-and-intent-identity-design.md).
- Defining the complete shared-artifact wire encoding, canonical digest framing, specification-version admission, or migration mechanism owned by [017](./017-intlify-shared-artifact-and-version-admission-design.md).
- Defining canonicalization data acquisition, download, installation, cache layout, or offline product UX owned by [029](./029-intlify-product-workflow-and-packaging-design.md).
- Defining external-policy artifact discovery, registry protocols, acquisition, installation, cache layout, or offline product UX owned by [029](./029-intlify-product-workflow-and-packaging-design.md), or each policy body's schema and admission semantics owned by 018, 021, and 022.
- Defining trust roots, credentials, signatures, actor authorization, or provenance evidence owned by [018](./018-intlify-security-trust-and-provenance-design.md).
- Defining the project graph, common Finding envelope, cache implementation, or query protocol owned by [019](./019-intlify-project-graph-query-and-incremental-design.md).
- Defining requirement-planning, message-locale-fallback selection, reachability, Bundle Plan, or pruning algorithms owned by [020](./020-intlify-requirement-planning-and-linking-design.md).
- Defining Provider/TMS transport, candidate lifecycle, governance decisions, or Translation Store protocols owned by [021](./021-intlify-translation-store-and-governance-design.md) and [022](./022-intlify-provider-and-localization-sync-design.md).
- Defining Target Profile capabilities, target artifact formats, generated bindings, or export behavior owned by [024](./024-intlify-target-profile-and-export-design.md).
- Defining Release publication, deployment activation, execution admission, withdrawal, or rollback owned by [025](./025-intlify-release-assembly-and-deployment-design.md).
- Defining one physical Runtime implementation owned by [027](./027-intlify-reference-runtime-design.md).

## Ownership and Dependencies

This document owns the semantic meaning and deterministic resolution rules of the resolved `LocalizationProjectProfile`, including locale sets, locale defaults, locale-policy inputs, Target Profile membership, and Deployment Compatibility Group declarations.

It defines the information that downstream specifications may rely on. It does not absorb their internal policy evaluation, artifact, execution, or deployment responsibilities.

| Area | Responsibility relative to this design |
| --- | --- |
| Canonical configuration input | Uses `intlify.config.json` as the primary repository surface and one JSON-compatible `IntlifyConfig` data model for every resolver entry path |
| `intlify_config` crate | Owns the authoring model, configuration-schema version, JSON Schema generation, structural admission, profile selection, semantic resolver, canonicalization-provider and external-policy-artifact-set input boundaries, configuration Findings, checked profile IR, and the 015 conformance-suite harness and traceability check without embedding CLDR-derived data or acquiring policy artifacts |
| JSON Schema validation | Runs through `intlify_config` and validates the structural shape of the 015-owned project-profile input before semantic resolution |
| `intlify_cli` adapter | Owns repository discovery, file I/O, CLI selector acquisition, acquired canonicalization/policy-artifact input assembly, command integration, and outcome rendering without owning configuration semantics |
| Optional programmatic frontend | Constructs the same `IntlifyConfig` value for embedded use; exact API naming, packaging, and language bindings remain 029-owned |
| 015 project-profile resolver | Is implemented by `intlify_config`; selects one named profile, then applies dependency-aware fail-complete validation and returns either one complete `LocalizationProjectProfile` or a blocked outcome with no partial profile |
| Locale canonicalization provider | Supplies one already acquired, immutable data artifact through a read-only boundary; it performs no implicit network access and cannot select alternate semantics |
| Policy artifact-set input | Supplies one finite set of already acquired immutable external-policy artifacts; `intlify_config` checks reference closure and 015-owned normalized facts while 018, 021, and 022 own body admission |
| ICU4X reference adapter | Candidate initial physical implementation using `icu_locale` without default compiled data and an explicit ICU4X data provider; it remains subordinate to Intlify conformance |
| 016 source authoring | Supplies Intent source-locale declarations and uses the resolved default only when source authoring omits one |
| 017 shared artifacts | Defines shared encodings, version admission, canonical identities, migration, canonicalization-data-artifact admission, and external-policy reference and artifact representation for the resolved model |
| 018 trust and provenance | Defines trust inputs, delegation, credentials, signatures, and authorization referenced by the profile |
| 019 project graph and queries | Tracks profile dependencies and represents, queries, and projects Findings, evaluation status, suppression causes, and explanations to clients |
| 020 planning and linking | Selects exactly one Deployment Compatibility Group per transaction; consumes its locale, coverage, fallback, target-applicability, and delivery-policy inputs; admits graph applicability as an exact partition of selected targets; and owns reachability and placement |
| 021 Store and governance | Consumes Selection Scope and governance-policy references without redefining locale policy |
| 022 synchronization | Consumes Provider-routing, Glossary Set, refresh, and applicable locale-demand inputs |
| 023 localization execution | Consumes locale-negotiation, locale-service, and scoped-locale semantics |
| 024 target export | Owns Target Profile capability and the physical output paths, chunk/resource identities, loader relationships, and target-artifact details derived from selected placement |
| 025 Release and deployment | Owns one independent Release boundary per selected Deployment Compatibility Group, hydration-coupled execution consistency, publication, activation, rollback, and other Release behavior |
| 029 product workflow | Owns file discovery, workspace selection, user-facing selector input, commands, schema packaging, optional helper API UX, canonicalization-data and external-policy-artifact acquisition and caching, and product packaging without defining alternate configuration or profile-selection semantics |

## Inherited Decisions from 000

The following are fixed inputs from the overview and are not open questions in this document:

- shared compiler stages consume a resolved language-neutral profile rather than unchecked authoring configuration;
- each Message Intent has exactly one source locale;
- a project default source locale applies only when source authoring omits an Intent source locale;
- libraries retain the source locale of each published Intent;
- requested locale is a semantic dimension and does not imply one emitted artifact per locale;
- the project default requested locale is independent of the default source locale;
- each Target Profile declares a supported requested-locale subset of the project set;
- a Target Profile may override the project default requested locale;
- each Target Profile resolves exactly one effective default requested locale inside its supported subset;
- locale negotiation, message locale fallback, and single-message evaluation are separate operations;
- one Locale Compiler transaction covers exactly one selected Deployment Compatibility Group;
- independently released groups have independent Requirement Plans and Release Snapshots; and
- Provider, TMS, governance, or production credentials never become ordinary resolved-profile data available to build or execution stages.

## Terminology to Refine

The product-wide definitions in 000 remain authoritative while this design is incomplete, except for the narrower configuration decision explicitly recorded here and awaiting follow-up alignment in 000. This section will refine only the profile-specific semantics and relationships needed by consumers.

| Term | Profile-specific question to resolve |
| --- | --- |
| `IntlifyConfig` | JSON-compatible, author-facing input model shared by `intlify.config.json` and optional programmatic frontends |
| `intlify.config.json` | Primary repository configuration document that declares one or more named project-profile inputs |
| Programmatic configuration frontend | Optional typed or embedded API that constructs `IntlifyConfig` without introducing different semantics |
| Configuration schema version | Configuration-specific string version admitted by `intlify_config`; its initial value is `"0"` and it is independent of CLI reporter and shared-artifact versions |
| Profile ID | Configuration-scoped opaque name used to select one profile declaration; it is not inferred from a package path, target, or Selection Scope |
| Configuration source evidence | Non-semantic origin and location data for file input, programmatic input, and Profile ID selectors, retained for Findings and source maps |
| Resolution outcome | Conceptual result of one resolver invocation: either one complete checked profile with only non-blocking Findings, or a blocked result with no profile, accumulated Findings, and evaluation status; these labels do not reserve a public API shape |
| Evaluation status | Deterministic record of which specified phase and subject checks were evaluated or not evaluated, including the causal blocking Finding identities for each dependency-suppressed check; an unevaluated check is not itself a synthetic Finding |
| Localization Project Profile Specification | Intlify-owned semantic specification for the normalized checked-profile model; its initial revision is `"0"` and its version domain is independent of configuration schema, artifact encoding, package, and Runtime ABI versions |
| Profile semantic equality | Equality of the Profile Specification identity/revision and every field in the canonical semantic projection; digest equality is an implementation aid under the same framing specification rather than a substitute for this definition |
| Resolution staleness | State in which an admission or source dependency changed and the resolver must run again; a stale resolution input does not imply that the newly resolved profile has different semantics |
| Reproducibility input closure | Finite immutable set of materialized configuration, selector, admitted semantic specifications, data, referenced artifacts, and capability inputs needed to reproduce one resolution outcome |
| Project Profile Resolver Conformance Suite | Versioned machine-readable fixture suite for the 015-owned resolver semantics; revision `"0"` binds each case's inputs, entry-path applicability, expected outcome, evidence, dependency status, and traceability without defining a public profile wire format |
| Localization Project Profile | Checked project-configuration IR for one final-application localization project, including its exact scope, identity, required sections, and completeness rules |
| Locale identifier | Valid Unicode BCP 47 Locale Identifier as defined by UTS #35, used as the shared semantic locale namespace across project configuration and downstream specifications |
| Locale Canonicalization Specification | Versioned Intlify-owned semantic specification that identifies the exact UTS #35 algorithm and CLDR-derived data requirements and fixes the conformance fixtures used to derive canonical locale identity |
| Locale Canonicalization Data Artifact | Separately versioned, immutable, provider-readable realization of one Locale Canonicalization Specification, carrying a representation-independent canonical dataset identity and digest plus representation-specific integrity metadata |
| Locale canonicalization provider | Read-only boundary through which the resolver receives an already acquired data artifact without embedding it in `intlify_config` or performing network access |
| Project requested-locale set | Required finite non-empty set of explicit canonical requested locales, bounded by an admitted versioned resource-limit policy |
| Default source locale | Optional canonical project default inherited only by application-owned Intents that omit an explicit source locale; absence is an explicit checked profile state |
| Default requested locale | Required canonical project-wide negotiation default that must belong to the project requested-locale set and remains independent of the source default |
| Effective default requested locale | Exactly one canonical default per Target Profile, resolved from its explicit override when present and otherwise from the project default |
| Target Profile reference | Identity, checked capability/profile revision, requested-locale subset, optional requested-locale-default override, and group-membership admission inputs |
| Deployment Compatibility Group | Non-empty exact set of Target Profile identities generated and assembled under one independent Requirement Plan and Release compatibility boundary |
| Deployment Compatibility Group ID | Configuration-scoped opaque identity used for exact compiler-transaction selection; no platform, package, path, target, or release semantics are inferred from it |
| Selected Deployment Compatibility Group | Exactly one checked group chosen as compiler-transaction input; it is not a project-profile default or a merge of several groups |
| Hydration coupling | Explicit finite directed relation from an SSR-capable Target Profile to a Browser hydration-client Target Profile in the same group, requiring compatible locale selection, selected definitions, logical rendering, and Release identity |
| Locale Negotiation Profile | Immutable versioned policy that fixes how an ordered application preference sequence is matched against one Target Profile's supported requested locales, without containing that dynamic sequence, the target set, or its effective default |
| Message locale fallback policy | Immutable versioned map from each applicable project requested locale to its complete ordered definition-locale candidate sequence after the requested locale itself |
| Intent source-locale fallback candidate | Explicit semantic fallback candidate resolved by 020 to the checked source locale of each individual Intent; it is not the project source default or a literal locale stored in the fallback table |
| Coverage policy | Immutable versioned default and scoped-rule specification that resolves one effective coverage mode for each canonical requested locale × checked Intent surface class |
| Effective coverage mode | Exactly one of `direct-required` or `fallback-allowed`, resolved independently of source-equal fulfillment, target packaging, Store state, and fallback-candidate eligibility |
| Source-equal fulfillment | Derived state in which a requirement's requested locale equals its checked Intent source locale and the admitted source artifact fulfills direct demand without Provider work |
| Policy reference | Typed immutable pin for an externally owned Provider, governance, Glossary, trust, or resource policy, composed of policy kind, opaque policy identity, exact policy revision, policy-specification revision, and semantic content digest |
| Policy artifact set | Finite set of already acquired immutable external-policy artifacts supplied explicitly to one resolver invocation without network lookup or policy discovery |
| Explicit policy absence | Checked semantic state permitted only for a policy kind whose presence is optional; it is not an inferred default or a fabricated empty policy artifact |
| Delivery policy | Versioned profile fact that fixes portable Delivery Graph semantics and placement behavior without containing a realized host-build graph; revision `"0"` admits only `duplicate` placement |
| Delivery Unit Graph artifact | Immutable compiler-transaction input supplied by a host build integration, containing logical units, directed loading/dependency edges, canonical roots, reference bindings, Target Profile applicability, and an exact identity, revision, and semantic digest |
| Delivery Unit identity | Project-contextual logical identity of one graph node; it is not a path, filename, URL, content hash, numeric chunk ID, or globally published artifact identity |
| Physical delivery output | 024-owned realization of selected placement as paths, chunks, resources, hashes, URLs, loader IDs, eager/lazy relationships, generated code, or native package metadata |
| Selection Scope | Governance namespace selected by the project profile without inferring target semantics |

## Design Overview

The author-facing configuration and the compiler-facing settings IR are distinct models:

```text
intlify_cli repository adapter -> intlify.config.json --+
optional programmatic frontend -------------------------+-> JSON-compatible IntlifyConfig

JSON-compatible IntlifyConfig --------------------------+
admitted Locale Canonicalization -----------------------+-> intlify_config
Specification + data provider                                  -> admit schemaVersion "0"
                                                               -> JSON Schema validation
                                                               -> select exactly one named profile
                                                               -> admit specification + provider data
                                                               -> locale canonicalization
                                                               -> semantic resolution
                                                               -> LocalizationProjectProfile
```

The resolved profile has four conceptual groups. Their exact representation remains to be designed.

```text
LocalizationProjectProfile
  + project and Selection Scope identity
  + locale model, canonicalization dependency identity, and locale-policy inputs
  + Target Profile and Deployment Compatibility Group declarations
  + versioned references to delivery, Provider, governance, trust, and resource policies
```

The resolver must discard authoring conveniences that have no semantic meaning while preserving enough file, JSON Pointer, or programmatic-call evidence for actionable Findings.

## Profile Scope and Identity

One `LocalizationProjectProfile` represents one **final-application localization project**: the logical configuration and governance unit that owns exactly one Selection Scope and one coherent set of project-wide locale and policy inputs.

The profile unit is explicit configuration semantics. It is not automatically a repository, workspace, package, Target Profile, Deployment Compatibility Group, deployable binary, or directory. In a monorepo it commonly corresponds to one application package, but it may instead cover a coordinated set of application packages when they intentionally share one Selection Scope and the same project-wide locale and policy authority.

One profile may contain several Target Profiles and several independently released Deployment Compatibility Groups. Differences in target capabilities or release cadence therefore do not by themselves require separate project profiles.

A final application must use a separate profile when any of the following differ independently:

- Selection Scope or governance authority;
- project requested-locale set or project-level locale defaults; or
- project-wide policy authority that must be resolved without composing another profile.

A source-first library does not define the consuming application's profile. It publishes a `LibraryManifest`; the final application's selected profile supplies requested locales, Selection Scope, policies, targets, and release grouping. A demonstration or executable application inside a library repository may declare its own application profile.

Each named profile declaration has a configuration-scoped Profile ID used only for explicit selection, source dependency tracking, and evidence. A Profile ID is distinct from project identity and Selection Scope, and no target, governance, package, or directory semantics may be inferred from it. It is excluded from profile semantic equality and the profile digest. The selected declaration's checked project identity and every specification-required exact identity or revision are semantic profile inputs; exact public field spelling remains a later API decision.

## Canonical Configuration Input and Resolution

### Primary repository input

`intlify.config.json` is the primary and only normative repository configuration format for the project-profile input defined here. One root document may declare one or more named profile inputs; the exact container-field and selector spelling are not yet public API decisions. The exact repository-root discovery, workspace selection, command UX, and user-facing way to provide the selector remain owned by 029. Intlify does not require platform-specific configuration DSLs for Web, Apple, Android, JVM, native, or system targets; cross-platform behavior is expressed by Target Profiles and downstream integrations after profile resolution.

An external tool may generate `intlify.config.json`, but TOML, YAML, executable framework configuration, and platform-native objects are not additional configuration semantics recognized by the shared resolver.

Each resolver invocation admits one `IntlifyConfig` and an optional Profile ID selector, then resolves exactly one profile declaration according to these rules:

- when the configuration contains exactly one profile, the selector may be omitted;
- when it contains more than one profile, a selector is required;
- an explicit selector must name exactly one declared profile;
- a missing, unknown, or ambiguous selection produces a blocking Finding and no resolved profile;
- repository layout, package location, current working directory, or target selection never silently chooses a profile; and
- profile declarations are not implicitly merged.

The initial configuration semantics do not include profile inheritance, shared profile defaults, or profile composition. Each selected declaration must be complete enough for independent semantic resolution. A later proposal may add authoring convenience only if it materializes one unambiguous profile input before the resolver-owned semantics defined here.

### Implementation ownership and existing configuration reuse

The reusable implementation belongs in a dedicated Rust crate named `intlify_config`, not in `intlify_cli`. The core crate must be usable by the CLI, embedded compiler integrations, tests, and future language bindings without importing command parsing, terminal rendering, repository discovery, or other CLI-only concerns.

The existing `intlify_cli` configuration implementation is the migration baseline rather than a second configuration system. The `intlify_config` implementation should reuse or extract its established behavior where applicable, including:

- duplicate-object-member rejection before ordinary deserialization;
- strict JSON-compatible typed authoring models;
- deterministic JSON Pointer-based validation evidence;
- Rust-model-driven JSON Schema generation;
- a committed generated-schema artifact with freshness checks;
- optional root `$schema` editor metadata that does not select runtime semantics; and
- deterministic structural validation and error ordering.

This reuse does not make the existing CLI-owned `ProjectConfig` the checked profile IR and does not automatically preserve resource-first configuration semantics. `IntlifyConfig`, the selected profile input, and `LocalizationProjectProfile` remain distinct models. The exact code-migration sequence and compatibility lifetime of current CLI configuration entry points belong to implementation planning and 029.

`intlify_config` owns the canonical schema content and generator. The initial extraction retains the current JSON Schema Draft 7 generation baseline; changing the schema dialect later requires an explicit compatibility decision independent of configuration `schemaVersion`. Product packages such as the CLI may publish or re-export the generated schema at a user-facing package path, but they must not maintain a divergent schema or resolver. `intlify_cli` remains responsible for locating the file and selector and for adapting `intlify_config` Findings to command output.

### `IntlifyConfig` and JSON Schema

`IntlifyConfig` is the unchecked, JSON-compatible authoring model. The 015-owned project-profile fields are described by a versioned JSON Schema so files, editors, CLI tooling, and optional APIs share one structural definition. Root-schema composition and package publication remain coordinated with 029 and existing tooling specifications.

The initial configuration schema version is the string `"0"`. It denotes a pre-stable configuration specification owned by `intlify_config`. It is a separate version domain from the CLI JSON reporter's `schemaVersion`, even though both initially serialize the value `"0"`, and it is also separate from shared-artifact, manifest, Runtime ABI, and package versions. Implementations must use a configuration-specific constant and admission path rather than importing the CLI reporter constant.

Configuration `schemaVersion` selects authoring admission and belongs to the resolution and reproducibility dependency closure. It is not a field of the canonical profile semantic projection. A future configuration version that resolves to the same Profile Specification revision and the same canonical semantic fields therefore produces an equal profile, although changing the configuration version always requires re-resolution.

Canonical profile-bearing configuration identifies this version through the root `schemaVersion` member. The root `$schema` member remains optional editor-facing metadata and never selects runtime schema or resolver behavior. An explicitly unsupported `schemaVersion` is blocking. The compatibility treatment of existing unversioned configuration, including whether a product adapter temporarily materializes it as version `"0"`, remains owned by 029 and must occur without creating alternate `intlify_config` semantics.

JSON Schema validation admits structural shape, primitive types, required fields, and closed or versioned field sets. The semantic resolver remains responsible for locale canonicalization, cross-field membership, reference admission, default resolution, Target Profile subsets, Deployment Compatibility Groups, and deterministic Findings. Schema success alone never creates a `LocalizationProjectProfile`.

Every fixed object owned by the version-`"0"` configuration specification is closed. An object member not declared by the applicable generated schema is a blocking structural Finding at the member's exact source evidence. This rule applies to the root and every nested `intlify_config`-owned object, and equivalent file and programmatic inputs must produce the same Finding. Root `$schema` and `schemaVersion` are declared fields, not exceptions to unknown-member handling.

Version `"0"` defines no generic pass-through extension namespace. A future extensible section must be introduced explicitly with an owning specification, a declared field, bounded value rules, and deterministic validation; arbitrary unknown members never become extensions by convention. Adding a recognized formatter, linter, profile, or other composed root section requires an explicit schema and implementation update. `intlify_config` must not retain an unknown member in `IntlifyConfig` or copy it into `LocalizationProjectProfile`.

### Configuration source evidence

`intlify_config` must preserve enough non-semantic origin and location information to explain every configuration Finding and allow CLI, editor, LSP, and agent integrations to identify the exact input to change. This evidence belongs to Finding records and the source map returned with the resolution outcome; it is not a semantic member of `LocalizationProjectProfile`.

For file input, evidence consists of:

- a project-relative, slash-normalized path;
- an RFC 6901 JSON Pointer into the materialized `IntlifyConfig`; and
- a half-open UTF-8 byte range when a corresponding source token exists.

Line and column positions, along with other client-specific position forms, are derived by the presenting adapter from the source text and byte range. A missing-member Finding points to the nearest owning object. An invalid member or value points to the applicable key or value token. A cross-field Finding may have one primary evidence item and related evidence for the other relevant locations.

For programmatic input, evidence consists of:

- a stable source label or URI supplied by the frontend;
- the JSON Pointer into the materialized JSON-compatible value; and
- an optional call-site span when the frontend can provide one.

Programmatic evidence must not depend on a stack trace, function identity, class instance, object address, or hidden process state. The absence of a call-site span does not change a Finding's code, severity, semantic reason, or subject.

Profile-selector evidence records the selector value and its origin, such as a CLI option or programmatic argument. A selector Finding relates that origin to applicable profile declarations when those declarations are available.

Equivalent file and programmatic inputs produce the same Finding code, severity, semantic reason, and subject. Their origin and location evidence may differ. Evidence is excluded from profile semantic equality, profile digest, and checked-profile serialization. It may be retained separately by 019 for diagnostics and dependency explanation, but it must not embed an entire source file, credentials, or arbitrary host objects. The common Finding envelope and exact evidence encoding remain owned by 019.

### Optional programmatic frontend

An embedding API may accept or construct the same `IntlifyConfig` value without first writing a file. A helper provisionally illustrated as `defineIntlifyConfig()` may provide static typing and editor completion, but its result remains unchecked input to the shared resolver.

The programmatic path must satisfy these invariants:

- it produces only JSON-compatible data covered by the same schema;
- it cannot carry functions, class instances, platform handles, credentials, or hidden process state into the profile;
- it cannot directly construct or assert a checked `LocalizationProjectProfile`;
- it uses the same named-profile declarations and selector rules as file input;
- it runs the same semantic resolver and produces the same Findings as equivalent file input; and
- reproducibility depends on the materialized `IntlifyConfig` value and admitted references, not host-language object identity.

The exact helper name, language bindings, and embedding ergonomics belong to 029.

### Resolved output

`LocalizationProjectProfile` is a complete, checked settings IR and the only configuration model consumed by shared compiler stages. One resolver invocation always returns exactly one of these conceptual outcomes, whose labels do not reserve a Rust enum, wire tag, or public API:

- a checked outcome contains exactly one complete profile, zero or more non-blocking Findings, and complete evaluated status for every required profile-resolution check; or
- a blocked outcome contains no profile, every independently reportable Finding admitted under the reporting bounds, and evaluation status that identifies checks not evaluated because their prerequisites failed.

A blocking Finding always selects the blocked outcome. The resolver never exposes a partially normalized profile, a valid prefix, a profile containing unresolved placeholders, or a checked outcome with dependency-suppressed work. Non-blocking Findings such as canonical replacement suggestions may accompany the complete checked profile.

## LocalizationProjectProfile Semantic Model

Profile Specification revision `"0"` defines the required and optional semantic groups below without prematurely freezing a Rust struct or wire encoding. The canonical semantic projection begins with the Profile Specification identity and revision and excludes Profile ID, selector, configuration schema version, authoring evidence, and resolution Findings.

Its semantic groups are:

- Profile Specification identity and revision;
- project and Selection Scope identity;
- project locale sets and defaults;
- locale-negotiation and message-locale-fallback policy references;
- coverage-policy references;
- required approval/selection and trust/source-admission references, plus explicit present-or-absent Provider-routing and Glossary Set states;
- Target Profile references and Deployment Compatibility Groups;
- versioned delivery-graph semantics and placement policy, without a realized Delivery Unit Graph;
- trust, integrity, and resource-limit references; and
- semantic specification-version and capability-reference inputs shared with 017; physical implementation capacity remains resolution admission rather than profile semantics.

Every externally defined semantic reference included by these groups uses the exact identity, revision, specification revision, and semantic content digest required by its owning specification. The canonical projection contains normalized values rather than source spelling, object-member order, redundant authoring rules, physical provider representation, acquisition metadata, or implementation object identity. 017 owns the shared encoding and digest framing for this projection.

## Locale Identity and Canonicalization

The normative locale-identifier domain is the valid **Unicode BCP 47 Locale Identifier** defined by [Unicode Technical Standard #35](https://unicode.org/reports/tr35/). This is the hyphen-separated BCP 47-compatible subset of the more general Unicode Locale Identifier syntax. It applies uniformly to every source, requested, default, fallback, definition, target-supported, and policy locale represented by `LocalizationProjectProfile` or derived from it. Representative members include `en`, `en-US`, `zh-Hant-TW`, and `ar-EG-u-nu-latn`; Unicode locale extensions are therefore part of the shared semantic namespace.

The shared resolver does not admit an arbitrary opaque string, the underscore-separated CLDR form `en_US`, the CLDR special form `root`, a script-leading form such as `Latn`, a POSIX form such as `en_US.UTF-8`, a legacy ICU form such as `en_US@calendar=gregorian`, or another platform-specific locale identifier as profile locale identity. A compatibility adapter owned by 029 may explicitly convert a legacy source before semantic resolution, for example `en_US` to `en-US`, `root` to `und`, or `Latn` to `und-Latn`. The adapter must submit only the converted standard-form value while preserving the original spelling and conversion as non-semantic source evidence; the shared resolver never performs this repair implicitly. Target exporters or execution integrations may adapt the checked common identifier to a platform representation under 023 and 024, but that representation cannot redefine profile locale identity.

During semantic resolution, every admitted locale identifier is converted to its UTS #35 canonical form. Canonicalization includes canonical casing and ordering and replacement of deprecated aliases according to the admitted canonicalization data. For example:

```text
EN-us                    -> en-US
iw-IL                    -> he-IL
en-u-nu-latn-ca-gregory  -> en-u-ca-gregory-nu-latn
```

The canonical form is the locale's semantic identity stored in `LocalizationProjectProfile` and used by semantic equality and profile digests. The original authoring spelling is retained only as configuration source evidence. An admitted spelling that differs from its canonical form produces a non-blocking configuration Finding at that evidence with the exact canonical replacement as its suggested action. The resolver still produces a usable profile when no blocking Finding exists.

Two spellings that canonicalize to the same form name the same locale identity. After canonicalization, the resolver requires uniqueness within each locale collection or locale-keyed namespace that is semantically defined as one set. An exact duplicate or an alias collision such as `iw-IL` and `he-IL` produces one blocking duplicate-locale Finding that identifies the canonical locale and relates every conflicting occurrence. Resolution does not select the first occurrence or silently deduplicate the inputs, and it produces no partial profile. Reusing the same canonical locale in different semantic roles or independently defined collections is not a duplicate unless another profile invariant explicitly relates those scopes.

Every locale collection semantically defined as a set is ordered after canonicalization and duplicate detection by ascending unsigned UTF-8 bytes of the complete canonical identifier. Because the admitted identifiers are ASCII, this is a simple host-independent lexical order: a shorter prefix sorts before its continuation, so `en` precedes `en-US`. Checked-profile serialization, equality inputs, digest inputs, and consumer iteration over such a set use that order. Authoring order is non-semantic and changing it alone cannot change the resolved profile or its digest; locale-aware collation, host `localeCompare` behavior, and host locale data are never used.

An explicitly ordered locale sequence is not a set. A message-locale-fallback chain, locale-negotiation preference sequence, or another specification-defined ordered policy preserves the order resolved by its owning specification and is not sorted by this rule. JSON array order has semantic meaning only when the applicable schema explicitly declares the field to be an ordered sequence; using an array to author a semantic set does not make its source order normative.

Canonicalization does not add likely subtags or otherwise maximize an identifier. `en` remains `en` rather than becoming `en-Latn-US`, and is distinct from `en-US` as profile locale identity. An input such as `en_US` is outside the normative domain and is rejected rather than repaired by this resolver.

Canonicalization is governed by one versioned, Intlify-owned `Locale Canonicalization Specification` supplied by the admitted toolchain specification set. It defines the exact UTS #35 semantics, required CLDR-derived data revision, and conformance fixtures needed to reproduce canonical identities. The specification is normative; no particular physical engine, third-party library, or provider encoding defines Intlify semantics.

The physical engine receives one matching `Locale Canonicalization Data Artifact` through a read-only provider boundary. The artifact is separately distributed, immutable, and admitted by specification identity, canonical dataset identity and digest, compatible provider schema, and representation-specific integrity metadata before use. `intlify_config` contains neither raw CLDR sources nor a generated default CLDR table, and its resolver performs no download, cache lookup, environment discovery, or implicit network access. A product adapter or embedding host acquires the artifact and passes the provider explicitly.

An implementation may use generated baked data, a serialized blob loaded from local storage, or another conforming provider. Those forms are physical delivery choices and must produce identical results for the same admitted specification. `intlify.config.json` and optional programmatic frontends do not select or override the provider or its semantics. Missing data, provider-schema mismatch, digest mismatch, unsupported specification identity, or incomplete required data is blocking before canonicalization, with no fallback to host ICU, ECMA-402, operating-system, or platform locale behavior.

The initial reference adapter pins ICU4X `2.2.0` and the ICU4X 2.2 serialized-provider schema. It disables the default `compiled_data` feature, enables the provider-backed `serde` path, and constructs [`LocaleCanonicalizer`](https://docs.rs/icu/2.2.0/icu/locale/struct.LocaleCanonicalizer.html) with `try_new_extended_with_buffer_provider`. The extended mode is selected because it supplies the `LocaleExpander` data needed for all admitted locales rather than only common locales. Its required marker families are `LocaleAliasesV1`, `LocaleLikelySubtagsLanguageV1`, `LocaleLikelySubtagsScriptRegionV1`, and `LocaleLikelySubtagsExtendedV1`; an admitted provider must contain all four.

Using the extended expander does not change Decision 015-012: canonicalization may use likely-subtag data internally to disambiguate aliases, but it does not maximize the resulting identifier. ICU4X release, mode, provider schema, and marker set are pinned physical adapter details and do not participate in profile semantic identity. ICU4X remains a physical implementation rather than the normative specification: documented parser or canonicalization gaps require an explicit wrapper, an admitted-domain revision, or rejection of the adapter version, never silent divergence.

The initial Locale Canonicalization Specification has revision `"0"` in its own version domain, independent of configuration `schemaVersion`. Its initial canonical dataset contains the logical payload of the four marker families above from `icu_locale_data` `2.2.0`, whose published data was generated from CLDR `48.2.0`, plus the minimal Intlify-owned validity and canonicalization-override data needed by the conformance layer described below. The generated conformance data is derived from the matching CLDR validity and Unicode BCP 47 key/type records; it is not a copy of the full CLDR distribution. The data crate and source records are artifact-generation inputs only and are not linked into or distributed with `intlify_config`.

Before exporting any baked, blob, or other provider representation, the artifact generator deterministically orders and canonically frames the selected logical marker and conformance data and computes a full SHA-256 semantic digest. The exact canonical framing belongs to 017. The resulting manifest records specification revision `"0"`, the `icu_locale_data` release and immutable package checksum, CLDR revision, marker set, conformance-data subset, and semantic digest. A generated release manifest or lockfile pins the literal digest value; this design does not manually copy a generated hexadecimal value into prose.

The semantic digest identifies the canonical logical dataset independently of provider representation. Each physical artifact also carries its own transport digest for byte-level integrity, but changing only between conforming baked and blob exports does not change profile semantics. A digest proves content identity, not publisher authenticity; trust, signatures, and provenance remain owned by 018.

The valid Unicode BCP 47 locale domain from Decision 015-011 remains normative rather than being weakened to syntax-only well-formed input. The initial adapter therefore wraps ICU4X with a thin, Intlify-owned conformance layer. Before ICU4X canonicalization, that layer validates language, script, region, variant, Unicode extension key/type, and transformed-extension key/type membership against the pinned validity data. It delegates conforming canonicalization behavior to ICU4X and applies explicit override mappings where ICU4X `2.2.0` cannot realize revision `"0"` semantics. One known example is the missing `islamicc` to `islamic-civil` Unicode calendar-type alias documented by ICU4X.

A merely well-formed value is not admitted solely because ICU4X can parse it. Inputs such as `zz`, `en-u-ca-madeup`, and `en-u-zz-abc` receive a blocking configuration Finding when their components are not valid under revision `"0"`. A deprecated value is admitted only when the pinned specification provides a deterministic canonical replacement.

Revision `"0"` admits language, script, and region components whose pinned CLDR validity status is `regular`, `special`, `macroregion`, or `unknown`, in addition to a deterministically replaceable `deprecated` spelling. It rejects `reserved` and `private_use` components before canonicalization with a blocking configuration Finding. The status in the pinned dataset, rather than a code's historical origin in an ISO private-use range, controls admission. Publicly defined CLDR forms such as `en-XA`, `ar-XB`, `en-XK`, `und`, and `en-ZZ` therefore remain admissible, while forms containing `qfz`, `Qaaq`, or `XC` do not. The same component-status rule applies inside a transformed-language field in the `t` extension.

Only the registered Unicode `u` and transformed-content `t` extensions are admitted in revision `"0"`, and their keys and types must be valid in the pinned Unicode BCP 47 data. A syntactically well-formed private-use extension such as `en-x-brand` and an `other_extensions` singleton such as `en-a-foo` are outside the admitted domain. A registered `u` or `t` key or type remains governed by its pinned public definition even when that definition describes a private-use function; it is not treated as an opaque `-x-` sequence.

The resolver never strips a rejected component or extension and never silently reinterprets the remaining prefix as another locale. No profile identity, fallback, negotiation, formatting, Provider-routing, or other shared Intlify semantics may depend on an opaque private agreement in revision `"0"`. Admitting such data later requires a new Locale Canonicalization Specification revision and an explicit specification for scope, canonicalization, matching, composition-collision prevention, target capabilities, and permitted semantic consumers. Until then, application concepts such as brand, tenant, or product variant require a separately specified profile or policy dimension rather than a private locale tag.

The correction mappings are generated rather than maintained as handwritten exceptions. Artifact generation reads the pinned CLDR `48.2.0` `common/bcp47` key/type records, selects deprecated, preferred, and alias relationships whose source is itself a valid direct Unicode BCP 47 input, resolves every alias chain to its final preferred value, and compares the result with the pinned ICU4X `2.2.0` behavior. Only mappings that ICU4X does not already realize require the correction path. Legacy-only aliases that cannot occur in the direct domain remain the responsibility of the explicit compatibility adapter from Decision 015-018.

The generated revision-`"0"` correction set remains part of the admitted canonical logical dataset even when another conforming engine can perform those mappings natively. The canonicalization pipeline reapplies canonical syntax and ordering after correction and must reach an idempotent result: canonicalizing the output again produces the same bytes. A mapping that cannot be flattened or represented deterministically blocks artifact or adapter admission.

The initial adapter's gap inventory is generated from the pinned CLDR `48.2.0` `common/testData/localeIdentifiers/localeCanonicalization.txt` corpus. The generator covers all four upstream sets: `explicit`, `fromAliases`, `decanonicalized`, and `withIrrelevants`. Because the upstream corpus uses underscore-separated CLDR locale syntax, the generator parses each source and expected identifier and serializes their semantic subtag sequences into the hyphen-separated direct-domain syntax; it does not pass the raw upstream spelling to the shared resolver.

Every upstream case receives a stable case identity and an explicit disposition. A case that can be projected into revision `"0"`'s admitted direct domain becomes a normative canonicalization fixture. A case that cannot be projected is retained as outside-domain evidence with a machine-readable reason rather than silently skipped. For each admitted case, the harness records the normative expected result, raw ICU4X `2.2.0` behavior, and ICU4X behavior through the Intlify conformance layer. Every raw-engine mismatch produces a machine-readable gap-registry entry classified as corrected or blocking; directly matching behavior is recorded as delegated, and non-projectable input is recorded as outside-domain.

The admitted ICU4X adapter and Intlify layer must produce zero unexplained mismatches against the projected corpus. An unknown or unclassified mismatch blocks adapter admission and CI. Hand-authored regression fixtures may supplement the generated corpus but cannot override its normative expected results. The corpus revision, repository path, and full source-content SHA-256 digest, together with the generated registry and run outcome, are recorded in adapter conformance evidence. The test-corpus byte digest does not by itself participate in profile or canonical-data-artifact semantic identity; a change affects those identities only when it changes admitted normative data, generated corrections, or the Locale Canonicalization Specification.

Every reference-engine difference is therefore represented by a conformance fixture and classified as directly delegated, corrected by the Intlify layer, intentionally outside the admitted domain, or blocking adapter admission. The layer never accepts a merely well-formed but invalid identifier as though it were valid, and it never falls back to host locale behavior. A gap that cannot be corrected deterministically from the admitted data blocks that adapter version. The generated machine-readable registry is the complete per-case release evidence, while the following table is its stable human-readable group summary.

### Initial conformance-gap registry

| Difference group | Classification | Revision `"0"` disposition |
| --- | --- | --- |
| General UTS #35 compatibility forms and legacy host syntaxes, including underscore-separated CLDR identifiers, `root`, script-leading identifiers, POSIX suffixes, and legacy ICU keyword syntax | Outside admitted domain | The shared resolver rejects the direct input. An explicit 029-owned compatibility adapter may convert it to one valid Unicode BCP 47 Locale Identifier before resolution and must retain the original spelling and conversion as source evidence. ICU4X rejection of the original form is not a reference-engine conformance failure. |
| Primary language subtags longer than three characters | Outside admitted domain for revision `"0"` | The pinned CLDR `48.2.0` language-validity data contains no valid primary language code longer than three characters, so the validity layer rejects such input before ICU4X parsing. If a future Locale Canonicalization Specification admits one, ICU4X `2.2.0` becomes blocking unless an Intlify wrapper can represent and canonicalize it or the reference engine is upgraded. Revision `"0"` never follows later validity data implicitly. |
| Syntactically well-formed identifiers containing a language, script, region, variant, Unicode extension key/type, or transformed-extension key/type that is invalid under the pinned data | Outside admitted domain, enforced by Intlify validation | The conformance layer rejects the input before ICU4X canonicalization with a blocking configuration Finding; ICU4X parser acceptance is irrelevant. Generated positive and negative validity fixtures define the boundary. Rejecting a fixture that the specification marks valid is an adapter conformance failure and blocks adapter admission rather than being reported as a user error. |
| CLDR `reserved` or `private_use` language, script, or region components; opaque `-x-` private-use sequences; and non-`u`/`t` extension singletons | Outside admitted domain for revision `"0"` | The validity layer rejects the complete identifier without stripping any component. Public CLDR `regular`, `special`, `macroregion`, and `unknown` codes remain admitted regardless of historical origin. A future revision can expand the domain only with explicit portable semantics, collision rules, and target-capability requirements. |
| Valid Unicode BCP 47 key/type aliases present in pinned CLDR `common/bcp47` data but not canonicalized by ICU4X `2.2.0`, including `und-u-ca-islamicc` to `und-u-ca-islamic-civil` | Corrected by Intlify layer | The artifact generator derives and flattens the valid-direct-input alias mappings, removes those already realized by ICU4X, and emits the remaining correction set without handwritten exceptions. The pipeline applies the correction, restores canonical syntax and ordering, and proves idempotence. An unrepresentable or non-deterministic mapping blocks admission. |

`LocalizationProjectProfile` retains the admitted canonicalization-specification identity and representation-independent canonical dataset identity and digest; they participate in profile equality and digests. Provider schema, baked-versus-blob representation, and transport digest are admission and integrity inputs but do not change semantic profile identity when they realize the same canonical dataset. The exact artifact encoding, provider-schema admission, and cross-artifact identity mechanism belong to 017. Toolchain/lockfile pinning and data acquisition, installation, caching, and offline workflow belong to 029. The ordinary application Runtime does not receive this general canonicalization artifact merely because the compiler used it; any runtime-facing dynamic locale-input requirement belongs to 023 and 027.

Adopting new UTS #35 or CLDR data creates a new Intlify canonicalization-specification revision and requires profile re-resolution and dependent-artifact invalidation. It does not by itself change configuration `schemaVersion`; that version changes only when the authoring specification requires it.

The opaque, exact-byte `Locale` currently defined by 014 is an existing resource-first artifact type, not the semantic locale model for this source-first profile. It cannot be reused unchanged across this boundary. Any migration or adapter between existing 014 artifacts and the normative profile locale domain must be made explicit by 017, 020, and 029 rather than silently accepting opaque values here.

The remaining design will define:

- any additional evidence required when an invalid authoring spelling is rejected.

## Source Locale Defaults

The author-facing `defaultSourceLocale` field is optional. When present, the resolver validates and canonicalizes it under the same locale rules as every other profile locale and stores its canonical identity as the project default. An application-owned Intent inherits that value only when its authoring surface omits an explicit source locale; an explicit Intent source locale remains authoritative and may differ from the project default. A declared default is valid even when every current Intent is explicit, and its being unused does not produce a Finding.

A library Intent never inherits the consuming application's default. Every published library Intent retains the exact source locale established by its own source authoring and Library Manifest, so application composition cannot reinterpret library source text.

Omitting `defaultSourceLocale` is valid during profile resolution because the configuration resolver runs before source discovery. `LocalizationProjectProfile` represents the result as an explicit semantic absence of a project source-locale default, not as unresolved state. This design does not reserve a Rust enum or wire encoding for that state, but every conforming representation must distinguish absence from every locale value. In particular, it must not substitute `und`, an empty string, the host locale, the project default requested locale, or another inferred value.

After source discovery, every Intent must still resolve to exactly one source locale. If an application-owned Intent omits its source locale while the checked profile records no project default, the 016 source-authoring stage produces a blocking Finding at that Intent occurrence and no downstream checked Intent is emitted for it. The Finding may suggest either adding an explicit source locale to the Intent or adding `defaultSourceLocale` to the selected project declaration. Configuration omission alone is not a Finding, and a project with no localizable Intents or with only explicitly sourced application Intents remains valid.

The presence or absence of the project default and its canonical value when present participate in profile equality and digests. Changing that state invalidates the profile and its source-resolution dependency; 019 owns source-graph scheduling and any proof that downstream recomputation can be narrower when every affected Intent has an explicit source locale.

## Requested Locale Set

The author-facing `requestedLocales` field is required and must explicitly contain at least one locale identifier. Revision `"0"` admits only a finite enumeration of individual identifiers; it does not admit `*`, `all`, a language range, a query, or another dynamically expanded form. The resolver never derives membership from `defaultSourceLocale`, a default requested locale, Target Profiles, source Intents, host locale state, CLDR coverage, or Provider availability. A single-locale application declares its one requested locale explicitly.

After locale validation and canonicalization, exact duplicates and alias collisions block under Decision 015-023. The resulting unique canonical identities form the semantic project requested-locale set and are ordered under Decision 015-024. A duplicate does not increase semantic set cardinality, but it remains an independent blocking error and is never silently removed to make an otherwise invalid declaration pass.

Revision `"0"` defines no product-wide fixed maximum cardinality. Instead, resolution requires an admitted, versioned resource-limit policy to provide a positive finite maximum requested-locale cardinality, semantically referred to here as `maxRequestedLocales`; its exact policy encoding belongs to the resource-policy design. The canonical set cardinality must not exceed that value. Exceeding it produces a blocking Resource admission Finding and never truncates the set, selects the first members, or produces a partial profile.

The raw configuration document, array length, decoding allocations, and canonicalization work remain subject to separate bounded-input limits, so repeated duplicates cannot bypass resource protection merely because they collapse to fewer semantic identities. A tool whose declared implementation capability cannot satisfy the admitted resource-limit policy rejects capability or policy admission; it does not silently replace the pinned maximum with a host-memory-derived or implementation-default value.

Target Profile subset validation, independently released group subsets, downstream invalidation, and the classification of intentional locale exclusions versus localization debt remain to be completed in their applicable sections below.

## Requested-Locale Default Resolution

The author-facing `defaultRequestedLocale` field is required even when `requestedLocales` contains exactly one member. The resolver validates and canonicalizes it under the common locale rules, then requires the canonical identity to be a member of the canonical project requested-locale set. It never infers the value from declaration order, the sole set member, `defaultSourceLocale`, source Intents, a Target Profile, host locale state, or locale negotiation.

Each admitted Target Profile declares a non-empty supported requested-locale set that must be a subset of the project set. It may also declare one optional default-requested-locale override under the 024-owned Target Profile specification. The project resolver computes exactly one effective default for each referenced Target Profile:

```text
if the Target Profile has an override:
  effective default = canonical override
otherwise:
  effective default = canonical project defaultRequestedLocale
```

The selected value must belong to that Target Profile's supported subset and therefore to the project set. An override outside the subset is blocking. When no override exists and the project default is outside the subset, resolution is also blocking; the resolver does not choose the first, sole, lexically smallest, or negotiated locale from the subset. An override never adds membership to either set.

The checked profile stores the canonical project default and the canonical effective default associated with every referenced Target Profile. Independently released Target Profiles may resolve different effective defaults through explicit overrides. Compatibility constraints for Target Profiles in one hydration-coupled or otherwise coupled Deployment Compatibility Group remain owned by the group decision below.

Locale negotiation consumes the already resolved effective default as its terminal no-match result. Negotiation does not choose, mutate, or validate default authority, and message locale fallback does not participate in this algorithm. `defaultRequestedLocale` and `defaultSourceLocale` remain independent even when their canonical values happen to be equal.

## Locale Negotiation Policy Inputs

`LocalizationProjectProfile` retains an immutable reference to an admitted Locale Negotiation Profile identity and revision. The profile fixes the matching algorithm, preference-normalization rules, and any finite project-authored negotiation aliases. It does not copy a Target Profile's supported requested-locale subset or effective default, and it never contains application, user, request, browser, operating-system, or HTTP preference values.

One negotiation invocation has exactly these semantic inputs:

1. one admitted Locale Negotiation Profile;
2. one Target Profile's canonical non-empty supported requested-locale subset;
3. that Target Profile's already resolved effective default requested locale;
4. one finite ordered sequence of application-supplied locale preferences; and
5. the admitted Locale Canonicalization Specification and resource-limit policy required by the selected negotiation profile.

It returns exactly one canonical member of the supported subset. When no preference produces a match, it returns the already resolved effective default. It never returns no locale, chooses another default, or enters message locale fallback.

The ordered preference sequence contains locale identifiers, not a raw preference source. An application, framework, HTTP, or platform adapter owns acquisition and protocol-specific parsing of inputs such as `Accept-Language` quality values, wildcard and exclusion semantics, `navigator.languages`, or operating-system settings. It supplies an ordered sequence after that processing. Raw headers, quality weights, wildcards, malformed protocol tokens, user state, and request state are not `LocalizationProjectProfile` data and are not interpreted by the core negotiator. An empty normalized sequence is valid and resolves to the effective default. The exact typed execution failure for an unchecked adapter that submits an invalid locale identifier remains owned by 023; the negotiator never repairs it as an opaque or platform locale.

Revision `"0"` admits one portable deterministic algorithm, semantically named **portable lookup** here without reserving a public wire spelling. It processes the normalized preferences in their supplied order. Each preference is canonicalized with the same admitted Locale Canonicalization Specification used by the project profile, then evaluated through an [RFC 4647 Lookup](https://www.rfc-editor.org/rfc/rfc4647.html#section-3.4)-style sequence:

1. test the complete canonical candidate for exact membership in the target-supported set;
2. test an applicable explicit negotiation alias for that candidate;
3. progressively remove the rightmost, most-specific suffix under the specified lookup and singleton-boundary rules, testing exact membership and an applicable alias at each resulting candidate;
4. continue with the next application preference; and
5. return the effective default when every preference is exhausted.

Exact target-supported membership therefore takes precedence over an alias. Preference order takes precedence over later preferences, and no canonical set ordering is used as a tie-breaker. Unicode `u` and `t` extensions participate in the complete exact candidate and are removed only through the versioned lookup-candidate rules; negotiation never mutates the selected supported locale by reattaching an unmatched extension.

The optional negotiation alias map is a finite semantic map from a canonical preference candidate to one canonical member of the project requested-locale set. Its keys and values are validated and canonicalized under the common locale rules. Canonical duplicate keys or conflicting definitions are blocking. Alias mappings are direct and non-recursive: an alias destination is returned only when it belongs to the current target-supported subset; otherwise that alias is inapplicable and lookup continues. An alias changes negotiation selection only. It does not redefine locale canonicalization, add membership to a project or target locale set, create a message locale fallback edge, or change Provider and Store identity.

For example, pure prefix lookup cannot select supported `fr-FR` from preference `fr`. A project may explicitly declare the negotiation alias `fr -> fr-FR`; a target that supports `fr-FR` then selects it, while a target that excludes `fr-FR` continues lookup and eventually uses another preference or its effective default. The exact configuration field names and external reusable-policy packaging remain to be fixed with the general policy-reference decision.

The admitted resource-limit policy supplies positive finite bounds for the number of normalized preferences processed by one invocation and the number of alias entries admitted by one profile, semantically referred to here as `maxLocalePreferences` and `maxNegotiationAliases`. Revision `"0"` defines no product-wide numeric defaults. Limit failure never truncates a sequence or alias map and never turns an over-limit prefix into an authoritative negotiation result.

An application may bypass negotiation by directly selecting one canonical member of the target-supported subset. Membership checking still applies, and an unsupported direct selection is not silently negotiated or replaced by the default.

CLDR/UTS #35 best-fit matching and platform-managed best-fit behavior are not portable lookup revision `"0"`. A future portable best-fit profile requires a new versioned algorithm, pinned matching-data requirements, conformance fixtures, resource limits, and dependency identity. A future platform-managed profile additionally requires Target Profile capability and allowed-variation rules and cannot present its result as portable deterministic lookup. Until then, an application may use host-specific selection outside the Intlify negotiator and submit the resulting supported locale through the direct-selection path.

Target-specific and Deployment Compatibility Group validation consumes the applicable negotiation-profile identity, target-supported set, effective default, and declared coupling. Independently released targets may produce different results because their supported subsets differ. Hydration-coupled targets must later prove compatible negotiation results under the group rules; no platform result or target set is silently treated as equivalent merely because both use the same profile revision.

## Message Locale Fallback Policy Inputs

`LocalizationProjectProfile` retains an immutable reference to one admitted message locale fallback policy identity and revision together with its checked canonical mapping. The mapping is project-wide in revision `"0"`: Target Profile, Deployment Compatibility Group, delivery-unit, Provider, Store, and runtime conditions do not alter the candidate order for the same project requested locale.

The policy maps an applicable canonical project requested locale to its complete ordered fallback sequence. The requested locale itself is always the implicit first definition-locale candidate and is never authored inside that sequence. Omitting the fallback declaration and declaring an empty mapping both resolve to the same canonical empty policy, meaning that every requested locale has only its direct candidate. A present mapping entry must have a non-empty sequence; omitting one requested-locale member means no fallback for that member.

Revision `"0"` admits exactly two fallback-candidate kinds without reserving their eventual configuration or wire spellings:

1. a literal canonical definition locale; or
2. the semantic **Intent source-locale candidate**, resolved later for each checked Intent.

A literal candidate is validated and canonicalized under the common locale rules. It does not need to belong to the project requested-locale set because it names a possible definition locale rather than a user-selectable or emitted requested locale. Its presence does not add project or Target Profile locale membership, create a locale-negotiation result, or by itself create Provider work. The finite union of literal candidates is retained as checked definition-locale demand evidence for planning and Store queries.

The Intent source-locale candidate is not a synonym for `defaultSourceLocale`. After 016 has resolved every application and library Intent to exactly one source locale, 020 evaluates this candidate against that per-Intent value. An application Intent may have inherited the project default, an explicitly sourced application Intent retains its explicit value, and a library Intent retains its published source locale. The policy therefore expresses an explicit “fall back to this Intent's source” rule without assuming that every Intent shares one source locale.

For example:

```text
requested locale: ja-JP
configured fallback candidates: [ja, Intent source locale]

one Intent whose source locale is en:
  complete candidate order = ja-JP -> ja -> en

one library Intent whose source locale is de:
  complete candidate order = ja-JP -> ja -> de
```

Each configured sequence is complete and non-recursive, preserving the proven 014 model. If `ja` maps to `[en]` and `en` maps to `[fr]`, resolution for requested `ja` considers only `ja -> en`; it does not splice in `en`'s separate sequence. Reciprocal mappings are therefore finite independent sequences rather than graph cycles. The exact 020 probe representation for an Intent source-locale candidate that resolves to the requested locale or an already probed literal remains a Linker-algorithm detail; it cannot change configured priority or authorize an additional locale.

Configuration resolution rejects duplicate mapping keys after canonicalization, an explicit empty sequence, a literal candidate equal to its requested-locale key, a repeated literal candidate, or more than one Intent source-locale candidate in the same sequence. It never deduplicates, recursively expands, or reorders a submitted sequence. Mapping keys are canonically ordered by ascending unsigned UTF-8 bytes after collision detection, while each candidate sequence preserves declared priority.

No implicit candidate is added from a locale parent, Locale Negotiation Profile, project or target default requested locale, `defaultSourceLocale`, Target Profile subset, host locale, available Store content, or Provider capability. A parent such as `ja` and the Intent source-locale candidate appear only when the policy explicitly declares them.

The distinction among selection, permission, and execution is:

```text
locale negotiation
  -> selects one requested locale for the user or operation

015 message locale fallback policy
  -> supplies the ordered definition-locale candidates

015 coverage policy
  -> states whether fallback may satisfy this requirement

020 Message Linker
  -> selects the first eligible admitted definition against exact source evidence,
     one pinned Translation Store snapshot, and applicable governance

target output
  -> materializes that exact selection and retains definitionLocale
```

Message locale fallback never erases direct localization demand. A `direct-required` requirement remains blocking when its direct requested-locale definition is missing even if a fallback candidate is eligible. A `fallback-allowed` requirement may use the first eligible fallback selected by 020, while retaining explicit coverage debt for the missing direct definition. A requested locale equal to the checked Intent source locale is source-equal direct fulfillment rather than fallback; source admission and approval remain independently required where policy says so.

015 owns policy identity/revision admission, canonical locale validation, mapping-key membership, candidate-kind admission, ordering, duplicate and self-reference checks, and resource-bound checks. 020 owns Store/source eligibility, exact probing, the selected artifact and definition locale, unresolved and coverage Findings, trace representation, and Bundle Plan materialization. The fallback policy cannot approve a source or localized artifact, override a Selection Decision, or choose among competing artifacts at one definition locale.

The admitted resource-limit policy supplies positive finite bounds for the number of fallback-source entries, candidates per sequence, and total locale-resolution probes for one Linker transaction, semantically referred to here as `maxFallbackSources`, `maxFallbackCandidatesPerSource`, and `maxFallbackResolutionProbes`. Revision `"0"` defines no product-wide numeric defaults. A limit failure never truncates a mapping, candidate sequence, Intent set, or probe trace and never emits a partial checked profile or Bundle Plan.

The Linker records exactly one admitted definition for each required Intent revision × requested locale or reports the applicable blocking failure. Target export materializes that selected definition into the requested-locale output and retains its `definitionLocale` as provenance and MF2 evaluation context. Runtime and target-native execution load or reference that exact materialized selection; they never re-run this fallback sequence or search another locale definition.

## Coverage Policy Inputs

`LocalizationProjectProfile` retains an immutable reference to one admitted coverage-policy identity and revision together with its resolved finite decision table. Revision `"0"` admits exactly two configured coverage modes:

- `direct-required`; and
- `fallback-allowed`.

`source-equal` is not a third configured mode. It is a fulfillment state derived for one Intent revision × requested locale when that requested locale equals the Intent's checked source locale after 016 source resolution.

The author-facing coverage declaration has an optional project default and a finite semantic set of scoped override rules. Omitting the complete declaration, omitting its default, or explicitly selecting `direct-required` as the default all resolve to the same safe project default. An author may explicitly select `fallback-allowed` as the project default; doing so changes policy semantics but still does not erase direct localization demand. Omitting or explicitly declaring an empty rule set has the same semantics.

Revision `"0"` rule matching has exactly two language-neutral dimensions:

1. a non-empty canonical subset of the project requested-locale set; and
2. a non-empty subset of the versioned checked Intent surface-class vocabulary exposed by 016.

A rule must constrain at least one dimension. Omitting one selector means that the rule matches every admitted value in that dimension; it is not a dynamic wildcard token. Locale selectors are finite canonical sets and surface selectors are finite registered-value sets. Revision `"0"` admits no regex, prefix, host-language type, source path, package path, arbitrary metadata query, or runtime predicate.

Target Profile, Deployment Compatibility Group, Delivery Unit, Provider route, Store state, approval state, source locale, definition locale, and application runtime state are not coverage-rule selector dimensions. Target and delivery applicability determine whether a requirement edge exists, but every occurrence of the same Intent revision × requested locale within one project profile resolves the same coverage mode. Target-specific communication semantics require a distinct Message Intent rather than a target-conditioned coverage rule, and delivery placement cannot weaken localization quality.

For one canonical requested locale and one checked Intent surface class, resolution proceeds as follows:

1. collect every matching override rule;
2. compare their finite matched domains, treating rule A as more specific than rule B when A's domain is a strict subset of B's domain;
3. discard every matching rule that is strictly less specific than another matching rule;
4. when no rule remains, use the project default;
5. when all maximally specific rules select the same mode, use that mode and retain their canonical rule identities as explanation evidence; and
6. when maximally specific rules select different modes, produce a blocking coverage-policy conflict rather than using declaration order.

A rule constraining both locale and surface is normally more specific than matching locale-only or surface-only rules. A combined rule can therefore resolve their overlap explicitly. Two incomparable maximal rules with different modes remain conflicting wherever their matched domains overlap and no more-specific rule covers that overlap. Authoring order, JSON object order, filesystem order, and canonical locale-set order never break a tie.

For example:

```text
project default: direct-required

locale fr-CA:
  fallback-allowed

surface internal-tool:
  fallback-allowed

locale fr-CA + surface checkout:
  direct-required
```

The resolver validates the rules against the finite project locale set and versioned surface-class vocabulary, then derives a complete checked decision table for their bounded cross-product before requirement planning. Exact duplicate canonical selectors are blocking rather than silently merged. Semantically equivalent authoring forms produce the same table; rule source positions remain non-semantic evidence. Each table cell retains its effective mode and the canonical identities of the default or maximally specific rules that justify it.

016 assigns or derives the checked coverage-facing Intent surface class under its own authoring specification. An Intent with a missing, invalid, or unknown required class fails that stage; consumers do not infer a class from source text, DOM placement, file path, framework component name, package, target, or delivery unit.

Requirement planning records both the effective configured coverage mode and whether the requirement has a source-equal fulfillment path:

```text
effective coverage mode: direct-required | fallback-allowed
source-equal fulfillment: present | absent
```

For `direct-required`, an eligible direct definition at the requested locale is required. An eligible fallback candidate cannot make a missing, stale, invalid, unapproved, or otherwise ineligible direct definition release-admissible. 020 owns the exact blocking Finding and may retain checked fallback evidence for explanation, but it cannot use fallback as successful fulfillment.

For `fallback-allowed`, direct localization remains in the Requirement Plan and in non-source-equal Provider demand. If no eligible direct definition exists, 020 may select the first eligible candidate from the message locale fallback policy fixed by Decision 015-030. Such selection always emits a visible non-blocking coverage-debt Finding with the typed direct-candidate failure cause and selected definition locale. The coverage policy has no ignore or silent-fallback mode. If no eligible fallback exists, the requirement remains blocking.

For source-equal fulfillment, the checked source artifact satisfies the direct locale dimension and creates no Provider work. Any required source admission, approval, provenance, or trust evidence still applies. A coverage rule cannot reinterpret a different requested locale as source-equal or make an inadmissible source artifact selectable.

Coverage mode controls only whether an eligible fallback may satisfy Release Assembly after direct-candidate failure. It does not change fallback order, canonical locale identity, Provider routing, candidate acquisition, approval, Selection Decisions, source admission, target capability, delivery placement, or runtime behavior. Missing, stale, invalid, unapproved, and otherwise ineligible direct states remain distinct typed causes rather than one unstructured “missing” condition.

The admitted resource-limit policy supplies positive finite bounds for coverage-rule count, locale and surface selector occurrences, the resolved locale × surface decision-table cardinality, and rule-domain comparison work, semantically referred to here as `maxCoverageRules`, `maxCoverageLocaleSelectors`, `maxCoverageSurfaceSelectors`, `maxCoverageDecisionCells`, and `maxCoverageResolutionWork`. Revision `"0"` defines no product-wide numeric defaults. Limit failure never truncates rules or a decision table and never resolves only the first project locales or surface classes.

## Provider, Governance, and Glossary References

Every externally owned Provider, governance, Glossary, trust, or resource policy reference covered by this section uses this common logical form:

```text
PolicyReference
  = policy kind
  + opaque policy identity
  + exact policy revision
  + policy-specification revision
  + semantic content digest
```

This notation defines semantic fields, not JSON member names or a frozen wire encoding; 017 owns their shared representation. Policy kind is part of identity and prevents an artifact admitted as one kind from being coerced into another kind. Policy identity and revision remain meaningful even when two artifacts have the same content digest, while the digest proves which semantic content the revision names.

For these externally owned policies, `intlify.config.json`, or an equivalent programmatic value, contains typed references rather than policy bodies. This does not prohibit the 015-owned authoring declarations for locale negotiation, message fallback, and coverage defined above. A product adapter supplies one finite, already acquired, immutable Policy Artifact Set as explicit resolver input. Resolution performs no network access, registry discovery, workspace search, mutable-tag lookup, or environment-dependent default selection. A reference must resolve to exactly one artifact of the declared kind, identity, exact revision, policy-specification revision, and semantic digest.

Floating selectors such as `latest`, semantic-version ranges, branch names, mutable tags, or timestamps without content identity are outside revision `"0"`. A missing artifact, multiple matches, unsupported policy-specification revision, kind mismatch, identity or revision mismatch, digest mismatch, or one identity/revision pair presented with conflicting digests is blocking and produces no partial checked profile.

The responsible 018, 021, or 022 specification owns each policy body's schema and semantic admission. The 015 resolver owns common reference closure and presence checks. When profile resolution itself needs a policy value, such as a resource bound, it consumes the admitted typed artifact and records both its exact reference and the normalized 015-owned facts. It does not copy unrelated Provider, governance, trust, or Glossary policy bodies into the profile.

Revision `"0"` has these presence rules:

- a resource-limit policy reference is required;
- trust/source-admission policy references are required;
- approval/selection policy references are required;
- Provider-routing is an explicit present-or-absent state; and
- Glossary Set input is an explicit present-or-absent state, with exact set cardinality and composition left to 022.

A project that requires no additional human approval still references an explicit immutable policy with that meaning. Omission never becomes an implicit permissive approval, trust, source-admission, selection, or resource policy. A project or product may select a built-in policy artifact, but its exact identity, revision, specification revision, and digest are pinned like any other artifact; no product default is inferred during shared resolution.

Provider-routing absence does not block profile resolution or a build that can use an admitted pinned Store snapshot. An explicit synchronization operation that has non-source-equal Provider demand but no applicable route produces a typed 022-owned failure; it does not mutate the profile or invoke an ambient Provider. Refresh behavior, when applicable, is pinned by the 022-owned Provider/synchronization policy and has no implicit 015 default.

Glossary Set absence means synchronization supplies no glossary context and performs no glossary-derived machine check. It does not invent an empty Glossary Set revision. Present Glossary inputs must be exact immutable references and participate in Provider-work equivalence.

Provider/TMS secrets, reviewer credentials, secret locators, and runtime authentication handles are never policy-reference fields or profile facts. The applicable product integration supplies separately scoped credential and private trust-bootstrap inputs only to the authorized 018-, 021-, or 022-owned operation; 018 decides which non-secret trust identities or public evidence may appear in the pinned trust policy artifact.

The detailed Provider, Store, and governance workflows remain owned by 021 and 022.

## Delivery Policy and Topology Inputs

`LocalizationProjectProfile` contains stable delivery semantics, not the realized build graph. A checked revision-`"0"` profile records:

- the Intlify Delivery Graph Specification identity and revision `"0"`, which fix the portable meaning of logical unit identity and directed loading/dependency edges; and
- the Intlify Delivery Placement Policy identity and revision `"0"`, whose only admitted effective mode is `duplicate`.

For placement authoring, omission and an explicit `duplicate` value resolve to the same checked mode. An explicitly authored `hoist`, target-specific placement override, scope-specific placement override, or unknown mode is unsupported in revision `"0"` and is never normalized to `duplicate`. The checked profile records the specification revisions and effective mode even when authoring uses the omission form. These 015-owned delivery fields are not external-policy artifacts governed by Decision 015-032.

The realized topology is a separate immutable compiler-transaction input supplied after profile resolution by the applicable bundler, build plugin, compiler integration, object scanner, or whole-program adapter:

```text
LocalizationProjectProfile
  + selected Target Profiles
  + host-supplied Delivery Unit Graph artifact input
  + checked source and reference artifacts
  -> requirement planning and message linking
```

Each submitted graph artifact carries an exact graph identity, artifact revision, semantic digest, and finite language-neutral graph content. Its checked view contains:

- project-contextual logical `DeliveryUnitId` nodes;
- finite directed loading/dependency edges;
- a canonically ordered root set derived from nodes with no incoming edge;
- the exact binding of each applicable message-reference occurrence to one existing unit;
- a finite exact set of applicable Target Profile identities; and
- source evidence sufficient to explain which host integration supplied each semantic fact.

Graph admission occurs only after one Deployment Compatibility Group is selected. The submitted finite non-empty graph-artifact set must form an exact partition of that group's Target Profile members: every artifact has a non-empty applicability subset of the selected group, applicability subsets are pairwise disjoint, and their union equals the complete selected member set. One graph may therefore apply to several targets, but every selected Target Profile is covered by exactly one graph artifact. A graph applicable to an unknown, unselected, or differently grouped target and a target covered by zero or several graphs are blocking 020 admission failures.

Targets in one hydration relation may use different graph artifacts and different logical Delivery Unit structures. Hydration compatibility is defined over locale choice, selected messages, logical rendering, and Release identity rather than graph isomorphism, chunk equivalence, or equal Delivery Unit IDs.

A `DeliveryUnitId` is logical and project-contextual. It is never inferred from or reinterpreted as an absolute, current, output, or temporary path; filename; URL; MIME value; content hash; platform enum; bundler numeric chunk ID; output array index; random value; or worker-completion order. The host integration assigns IDs deterministically before physical output generation and supplies the exact same checked identities to source/reference producers and graph construction.

In graph revision `"0"`, an edge `parent -> child` means the child may become loadable only after the parent is loadable. It is not a message-copy edge or source-reference-flow edge. Nodes and directed edge pairs are exact and duplicate-free; every endpoint names an existing node; self-edges and cycles are invalid. Multiple roots and disconnected acyclic components are valid. Input order is non-semantic; checked nodes, edges, roots, Target Profile identities, and reference bindings use specification-defined canonical ordering.

Every applicable message-reference occurrence binds to exactly one existing graph node. The core never creates an implicit unit, chooses a nearest unit, infers one from a file path or target name, or silently moves an unbound reference to a root. Reachability and placement use only the admitted graph, source/reference facts, profile policy, and selected-target inputs.

Revision `"0"` gives `route`, `feature`, `module`, framework component, package, and similar labels no core placement semantics. They may remain non-semantic display or source evidence. `eager` and `lazy` are 024-owned physical loader relationships, while `shared` is an outcome of placement rather than an authored unit kind. A later policy may promote a category only with a new versioned registered vocabulary, exact matching and composition rules, target capability semantics, and conformance fixtures.

For a CLI, editor, final-binary scan, or other integration that can honestly observe only one whole-program unit, the host supplies the standard one-node graph whose logical ID is `["main"]` and whose edge set is empty. The profile does not infer, synthesize, or store that graph. An integration with finer evidence may supply a larger checked graph; a post-link scan does not claim sub-unit granularity it cannot prove.

024 owns physical realization after 020 selects placement. Output directories, portable output paths, filenames, hashes, URLs, target-native resource names, runtime loader IDs, eager/lazy loader records, generated code, package metadata, and actual load timing are not project-profile or Delivery Unit Graph semantics. An exporter maps exact selected `(requested locale, Target Profile, Delivery Unit)` placement to physical artifacts without changing the selected definition or logical unit relation.

The admitted resource-limit policy supplies positive finite bounds for graph-artifact count, node and edge occurrences, Delivery Unit identity size, Target Profile applicability, reference-binding count, decoded allocation, and validation/placement work. The host and 020 preflight their submitted collections before graph-proportional processing. They never truncate nodes, edges, target applicability, or bindings, partition one authoritative graph implicitly, or process only a valid prefix.

Delivery Graph Specification identity/revision and Delivery Placement Policy identity/revision/effective mode are semantic profile inputs. Graph artifact identity, revision, semantic digest, nodes, edges, applicability, and bindings are compiler-transaction inputs rather than profile identity. A semantic graph change invalidates affected Requirement Plans, Message Bundle Plans, target outputs, and Releases without changing an otherwise identical profile. A physical filename, path, hash, URL, or loader-registration change is an exporter or Release dependency and does not change profile or graph semantics when the logical graph and placement remain identical.

019 owns dependency slicing and graph-evidence projection. 020 owns graph admission, reachability, placement, and pruning. 024 owns physical target output. 025 owns Release consistency. None of those consumers may reread host build state to invent topology after the checked graph input has been admitted.

## Target Profiles and Deployment Compatibility Groups

A profile contains a finite non-empty set of checked Target Profile references and one or more finite non-empty Deployment Compatibility Groups. Each group has one configuration-scoped opaque Group ID and a non-empty semantic set of exact Target Profile identities. Group IDs and Target Profile identities are duplicate-free and canonically ordered by their checked identity encoding; declaration order is non-semantic.

Revision `"0"` requires the group member sets to form an exact partition of the complete profile Target Profile set. Every target belongs to exactly one group. An empty group, unknown member, duplicate member, target omitted from all groups, or target assigned to several groups is blocking. Membership is never inferred from platform family, Target Profile capability, package, directory, output path, graph applicability, hydration relation, or declaration order. A single-target group is valid.

One compiler transaction selects exactly one checked group by optional exact Group ID input. When the profile has one group, omitted selection and explicit selection of that sole ID are equivalent. When several groups exist, omission is blocking. An explicit unknown ID is always blocking. The compiler never chooses a first group, combines groups named by several selectors, selects a group from a Target Profile name, or treats a subset of one group as a transaction group.

The Group ID selector and its source evidence are compiler-transaction input rather than profile identity. The selected group identity is semantic transaction input. One transaction derives exactly one group-scoped Localization Requirement Plan, one group-scoped Message Bundle Plan, the complete output set for every member Target Profile, and one Release Snapshot. Every required member output must be available before Release Assembly; failure of one member cannot silently publish the remaining targets as that group's Release.

Different groups are independent Requirement Plan and Release boundaries. A host build may orchestrate several transactions, but no merged plan, Release Snapshot, publication, activation, or rollback authority is inferred across groups. A higher-level synchronization workflow may aggregate compatible Store-independent plans to deduplicate equivalent Provider demand while retaining every group, target, and delivery-applicability edge; that aggregation never couples their Releases.

### Hydration coupling

A group may contain a finite semantic set of explicit directed hydration relations:

```text
SSR Target Profile -> Browser hydration-client Target Profile
```

Each endpoint is a distinct member of the same group. The source endpoint must expose the 024-owned checked SSR-renderer capability and the destination endpoint the checked Browser hydration-client capability. Exact capability names and schemas remain owned by 024. Relation pairs are duplicate-free and authoring order is non-semantic. Fan-out and fan-in are valid, but one Target Profile cannot appear in both server and client roles inside the same revision-`"0"` group. The relation set is therefore explicitly bipartite and finite; no relation is inferred from names, platform labels, graph edges, import relations, output formats, or co-membership alone.

For every hydration relation, revision `"0"` requires the two Target Profiles to have:

- exactly the same canonical supported requested-locale set;
- exactly the same effective default requested locale;
- the same Locale Negotiation Profile identity and revision and the same canonical alias map; and
- therefore the same requested-locale result for every admitted normalized preference input.

A different locale subset, effective default, negotiation revision, or alias map is blocking even if a small observed fixture happens to negotiate the same locale. A future revision may admit a narrower shared hydration locale domain only with explicit selection, transition, and failure semantics; revision `"0"` never silently intersects the two sets.

020 and 030 derive the initial-render message closure for each relation from checked source/reference and delivery inputs. For every Intent applicable to both sides of that closure and every selected requested locale, the group-scoped Linker selection must retain the same Intent revision, selected source or localized artifact identity, definition locale, and selection/admission evidence. Target lowering may change representation but cannot choose another definition or rerun message locale fallback.

The two Target Profiles may use different physical engines, output formats, Delivery Unit Graphs, and locale-service implementations. Their admitted MF2 semantic capabilities and Locale Service Profiles must nevertheless carry 024/026-owned compatibility evidence sufficient to guarantee that the same checked Intent, requested and definition locales, parameter values, functions, and application data produce the same logical text or structured parts required by the relation. Hydration render equivalence is not graph isomorphism, equal chunking, equal resource bytes, or an unconditional byte-for-byte serialization rule; 030 owns framework projection and the exact initial-render comparison surface.

025 includes both target output sets, their exact Target and Locale Service Profile identities, selected-definition evidence, and the hydration relation in one Release Snapshot. Publication or physical activation need not be simultaneous, but coupled execution may combine only outputs admitted from that same Release identity. A server response from one Release and a hydrating client from another is a typed deployment or execution-admission failure, never permission to rerender with another locale or definition. Ahead-of-time targets must discharge the equivalent consistency check during export, packaging, application admission, or deployment.

Hydration-free group members still share the group's Release compatibility boundary but have no implied render-equivalence relation. Independently grouped Web, mobile, native, worker, service, or other targets may use different requested-locale subsets, effective defaults, negotiation outcomes, graphs, output formats, publication cadence, and rollback history as allowed by their own checked profiles.

The admitted resource-limit policy supplies positive finite bounds for Target Profile count, group count, members per group, total membership occurrences, hydration-relation count, selector bytes, and static compatibility work. Profile resolution checks the complete partition and relation set without truncation, dropping an unassigned target, deleting an overlapping membership, or retaining only a compatible prefix.

024 owns exact Target Profile capability and Locale Service Profile schemas. 020 owns selected-group planning, graph-partition admission, shared Linker selection, and initial message applicability. 025 owns Release Assembly and coupled execution admission. 026 owns cross-target equivalence conformance, and 030 owns Browser/SSR framework hydration projection.

## Deterministic Resolution Algorithm

This section defines an ordered, dependency-aware fail-complete resolution pipeline, expected to cover:

1. let a product adapter materialize one `IntlifyConfig` value, an optional Profile ID selector, their file or programmatic source evidence, and one finite already acquired Policy Artifact Set;
2. let `intlify_config` admit configuration schema version `"0"`, independently admit the toolchain-supplied Localization Project Profile Specification identity and revision `"0"`, and reject an explicitly unsupported version or specification;
3. admit exactly one toolchain-supplied Locale Canonicalization Specification and one matching provider-backed data artifact, or produce a blocking specification/data-admission Finding;
4. validate the applicable JSON Schema and reject non-JSON or structurally invalid input;
5. select exactly one named profile, or produce a blocking profile-selection Finding;
6. resolve project identity and Selection Scope;
7. resolve the required typed resource-limit reference to exactly one matching artifact, admit its identity, exact revision, policy-specification revision, and semantic digest, and admit the positive finite bounds needed to constrain resolution, including a maximum requested-locale cardinality that the implementation can satisfy;
8. validate locale-component statuses and extension admission, then canonicalize locale identifiers with the admitted specification, producing non-blocking replacement Findings for admitted non-canonical spellings;
9. reject exact duplicates and alias collisions within each locale collection or locale-keyed namespace after canonicalization;
10. sort each semantic locale set by ascending unsigned UTF-8 bytes of its canonical identifiers while preserving specification-defined ordered locale sequences;
11. require a non-empty explicit project requested-locale set, enforce its canonical cardinality against the admitted maximum without truncation, require its canonical `defaultRequestedLocale` member, and represent `defaultSourceLocale` as either one canonical locale or explicit absence without inspecting source Intents;
12. admit the applicable Locale Negotiation Profile identity and revision and its canonical alias map; admit the message locale fallback policy identity and revision and canonicalize its complete finite mapping; admit the coverage-policy identity and revision, validate its default and finite scoped rules, and resolve its complete locale × surface decision table; admit their resource bounds; then resolve required trust/source-admission and approval/selection references and explicit present-or-absent Provider-routing and Glossary Set states from the supplied artifact set; and finally admit any remaining typed exact policy references;
13. admit the Intlify Delivery Graph Specification identity and revision `"0"`, resolve omitted or explicit `duplicate` placement to the same Intlify Delivery Placement Policy identity and revision-`"0"` checked mode, and reject unsupported or scoped placement without accepting a realized host graph as profile configuration;
14. resolve Target Profile references, non-empty requested-locale subsets, and optional canonical default overrides, requiring every target locale to belong to the project set;
15. form finite non-empty Deployment Compatibility Groups with unique opaque IDs, canonical member sets, and a complete exact-one group partition of every Target Profile;
16. resolve each effective default requested locale by override-first precedence and require it to belong to the applicable Target Profile subset;
17. canonicalize and validate finite hydration relations, endpoint roles, same-group membership, locale-set equality, effective-default equality, negotiation-profile and alias equality, and statically knowable Target/Locale Service compatibility requirements; and
18. construct the Profile Specification revision-`"0"` canonical semantic projection, provide its equality and digest inputs to 017, and produce exactly one deterministic checked or blocked resolution outcome under the rules below.

Every specified check declares the typed values and earlier checks it requires. The resolver evaluates every check whose prerequisites are admitted, even after an independent blocking Finding occurs. A blocking Finding marks only the checks that depend on its unavailable or invalid result as not evaluated; it does not stop an unrelated locale, policy reference, Target Profile, group, or hydration relation from being checked.

An unsupported configuration version, a non-materializable root value, or failure to admit the specification, data, or safe resource bounds needed by a later phase may suppress a broad dependent phase because that phase has no safe typed interpretation or execution envelope. This is dependency suppression, not discretionary fail-fast behavior. The evaluation status records the exact causal blocking Finding identities, and the resolver does not emit synthetic cascade Findings for facts it could not prove.

Evaluation traverses phases in the numbered order and subjects in their specification-defined canonical order. Emitted Findings are ordered by phase rank, canonical subject identity when available, stable Finding code, and canonical source-evidence order. Invalid subjects without a canonical identity use canonical source-evidence order. Filesystem enumeration, JSON member order, worker completion, and concurrency do not affect the outcome, Finding order, or suppression causes.

Profile resolution validates every declared group but does not select a group or admit host graph artifacts. Before 020 plans requirements, its transaction admission applies the exact Group ID selection rule, restricts the transaction to the complete selected member set, and admits a graph-artifact applicability partition for that set. Selection and graph admission never rewrite the checked profile.

## Findings and Failure Model

This section will define component-owned Finding codes and evidence while using the common Finding envelope owned by 019.

The catalog will cover at least these families:

| Family | Candidate situations |
| --- | --- |
| Configuration version | Invalid type or explicitly unsupported configuration `schemaVersion` |
| Unknown field | Undeclared member in the root or any nested fixed configuration object |
| Profile selection | Missing selector for multiple declarations, unknown Profile ID, or ambiguous selection |
| Profile identity and specification | Missing, duplicate, ambiguous, or incompatible project or Selection Scope input; or missing, unsupported, or incompatible Localization Project Profile Specification identity/revision |
| Locale specification and data | Missing, unsupported, or incompatible Locale Canonicalization Specification; missing data; provider-schema mismatch; digest mismatch; or incomplete required data |
| Locale identity | Input outside the Unicode BCP 47 Locale Identifier domain, or an invalid, duplicate, non-canonical, unsupported, or incompatible locale input |
| Requested-locale set | Missing or empty `requestedLocales`; wildcard, range, query, or dynamic expansion; or membership inferred from another profile field or host state |
| Default resolution | Invalid project source default; an application Intent lacking both an explicit source locale and a project default; invalid target requested-locale override; or effective default outside the target subset |
| Locale negotiation | Missing or unsupported profile identity or revision; invalid or colliding alias key; invalid or non-project alias destination; missing or incompatible negotiation bound; or alias/profile incompatibility |
| Message fallback | Unsupported policy identity or revision; non-project mapping key; empty sequence; literal self-reference; duplicate literal or source candidate; invalid literal definition locale; or missing or incompatible fallback bound |
| Coverage policy | Unsupported identity or revision; invalid mode; empty or out-of-domain selector; duplicate canonical selector; unknown surface class; unresolved maximal-rule conflict; or missing or incompatible coverage bound |
| Policy reference | Missing required reference; unsupported kind or policy-specification revision; floating selector; unresolved or ambiguous reference; kind, identity, revision, or digest mismatch; conflicting content for one identity/revision; invalid optional-absence state; or unavailable required normalized policy fact |
| Target membership | Missing or duplicate Group ID; empty group; unknown or duplicate member; unassigned Target Profile; Target Profile assigned to several groups; non-canonical or over-limit membership input; or locale-subset mismatch |
| Group selection | Missing selector when several groups exist; unknown Group ID; several submitted selectors; or an attempted target-subset selection |
| Cross-target compatibility | Invalid or duplicate hydration relation; endpoint outside the group; equal endpoint; invalid server/client capability or dual role; unequal supported locale set, effective default, negotiation profile, or alias map; or statically incompatible MF2, Target, or Locale Service capability |
| Delivery policy | Missing or unsupported Delivery Graph Specification or Delivery Placement Policy identity/revision; unsupported `hoist`, scoped override, or placement mode; or an attempt to embed a realized graph or physical target-output fact in project configuration |
| Resource admission | Missing or incompatible resource-limit policy; non-positive or non-finite cardinality limit; requested-locale maximum exceeded; or profile size, input count, depth, allocation, or work limit exceeded |
| Finding reporting | Finding-count, related-evidence, evidence-byte, or evaluation-status bound exceeded, making the remaining diagnostic evaluation explicitly incomplete |

For each Finding, the completed design must define stable code, severity, blocking behavior, primary evidence, related evidence, deterministic order, dependency cause, and suggested action where safe.

A failure is independently reportable exactly when all typed values needed to prove it remain admitted. The resolver collects every such Finding under the applicable bounds. A failure is dependent when proving it requires an invalid or unavailable result; that check is marked not evaluated with the causal blocking Finding identities and does not emit an ordinary Finding. Severity does not determine independence, and one blocking Finding does not downgrade or hide another independently provable Finding.

Structural invalidity suppresses semantic checks only for the affected value or subtree when other typed siblings remain independently admissible. An invalid locale occurrence suppresses canonical-membership, default, duplicate, or compatibility facts that require its missing canonical identity, while other locale occurrences continue. An invalid Target Profile suppresses compatibility comparisons that require that profile but not checks over independent targets or groups. Failure to admit the configuration version, root shape, canonicalization semantics, or safe resource bounds suppresses every later check that cannot be interpreted or bounded without that prerequisite.

Suppression is represented only in evaluation status. Client projections may explain that a check was not evaluated and name its causes, but they must not fabricate warnings or errors for a condition the resolver did not establish. If several blocking Findings suppress the same check, its cause set is canonical and duplicate-free. A checked outcome has no suppressed required checks; a blocked outcome has no `LocalizationProjectProfile`, even when some independent branches were fully evaluated.

The admitted resource-limit policy supplies positive finite bounds for emitted Finding count, related-evidence count and bytes, and evaluation-status entries. Before that policy is admitted, equivalent bootstrap bounds come from implementation-capability admission. Traversal and admission use the deterministic order above and reserve capacity for one typed blocking reporting-limit Finding and terminal incomplete-evaluation status. On the first over-limit item, the resolver retains the already admitted canonical prefix only as explicitly incomplete diagnostic output, emits that limit Finding, marks all remaining specified checks not evaluated because of it, and returns a blocked outcome. It never presents the prefix as a complete report or produces a profile.

An accepted non-canonical locale spelling produces a non-blocking Locale identity Finding whose primary evidence is the authoring occurrence and whose suggested action is the exact canonical spelling. A canonical-identity collision within one uniqueness scope produces a blocking Locale identity Finding: one occurrence is selected deterministically as primary evidence, every other conflicting occurrence is related evidence, and the canonical identity is reported. Exact duplicates and alias collisions have the same blocking semantics. The resolver never applies first-wins behavior or silent deduplication.

An omitted `defaultSourceLocale` does not produce a configuration Finding. If later source discovery finds an application-owned Intent with no explicit source locale, 016 produces the blocking source-default Finding at the Intent occurrence and may relate the selected profile declaration as the location for adding a default. A library Intent with missing source-locale evidence is a library-source admission failure and never falls back to the application profile.

Missing or empty `requestedLocales` is blocking and never triggers inference from another locale field. A dynamic selector is rejected rather than expanded. After canonicalization, a semantic set larger than the admitted maximum produces a blocking Resource admission Finding that reports the actual canonical cardinality, admitted maximum, and policy revision. Duplicate-locale and maximum-cardinality Findings are both reported when each can be proved from the admitted canonical values; an occurrence with no canonical identity suppresses only cardinality or membership facts that require that missing identity. Raw input limits remain independently evaluable before potentially expensive per-member work and cannot be weakened by semantic duplicate collapse.

Missing `defaultRequestedLocale`, a canonical project default outside `requestedLocales`, an empty or non-subset Target Profile locale set, a target override outside that subset, and an inherited project default outside a non-overriding target subset are blocking. Primary evidence identifies the invalid default or target declaration and related evidence identifies the applicable set. No Finding recovery may select another locale implicitly or emit an effective default from an invalid target.

A missing, unsupported, stale, or incompatible Locale Negotiation Profile reference is blocking. Invalid alias identifiers, canonical alias-key collisions, conflicting alias definitions, alias destinations outside the project requested-locale set, and alias maps above the admitted bound are also blocking and produce no partial checked profile. An alias destination that is valid project membership but absent from one target subset is not a configuration error by itself; the rule is inapplicable for that target and conformance traces expose that step before lookup continues.

Application preference acquisition and protocol parsing do not produce configuration Findings. An execution integration that cannot materialize the required finite ordered sequence, canonicalize it under the admitted specification, enforce its preference bound, or return only a supported locale uses the 023-owned typed execution failure model rather than changing the checked project profile or silently choosing a host default.

Fallback-policy omission is valid and resolves to the canonical empty policy without a Finding. An explicitly unsupported or incompatible policy reference, mapping key outside the project requested-locale set, empty declared sequence, literal self-reference, duplicate literal, repeated Intent source-locale candidate, invalid literal locale, or first-over resource bound is blocking and produces no partial checked profile. A valid literal definition locale outside the project requested-locale set is not a Finding and never becomes requested-locale membership.

The configuration resolver does not inspect source Intents to validate the semantic source-locale candidate. A later application Intent with neither explicit nor inherited source locale and a library Intent without published source-locale evidence already fail under Decision 015-026 before 020 may use that candidate. Missing, ineligible, or unapproved definitions encountered while probing an otherwise valid policy are 020-owned Linker or coverage outcomes, not configuration Findings and not permission to mutate the chain.

Coverage-policy omission and an omitted or explicit `direct-required` default are valid and equivalent. An unsupported policy revision, unknown mode, rule with no constrained dimension, empty explicit selector, locale outside the project requested-locale set, unknown surface class, duplicate canonical selector, unresolved overlap whose maximally specific rules select different modes, or first-over resource bound is blocking. The resolver emits no partial decision table or checked profile and never selects the first authored rule.

A valid `fallback-allowed` result is not a configuration Finding. During planning and linking, a missing or ineligible direct definition remains visible: direct-required produces the applicable blocking 020 outcome, while fallback-allowed produces a non-blocking coverage-debt Finding only when an eligible fallback is actually selected. No eligible fallback remains blocking. Source-equal fulfillment and source-admission failures retain their separate typed causes and are not projected as configuration conflicts.

A missing resource-limit, trust/source-admission, or approval/selection reference is blocking. So is a reference using `latest`, a version range, a branch, a mutable tag, an environment-selected default, or another floating selector. A reference that resolves to zero or multiple artifacts, resolves across policy kinds, names an unsupported policy-specification revision, or fails identity, exact-revision, or semantic-digest admission produces no partial profile. Reusing one identity/revision pair with different semantic digests is a conflict rather than replacement or last-wins behavior.

Explicit Provider-routing absence and explicit Glossary Set absence are valid and do not produce configuration Findings. Provider absence becomes a typed 022 failure only when an explicit synchronization operation must route non-source-equal Provider work. Glossary absence remains a checked no-glossary state; consumers do not synthesize an empty artifact or search for an ambient glossary. An explicit immutable no-additional-human-approval policy is valid, while omission of approval policy is not equivalent to it.

Omitted delivery placement and explicit `duplicate` placement are valid and equivalent under Delivery Placement Policy revision `"0"`. An unsupported Delivery Graph Specification or Delivery Placement Policy identity/revision, `hoist`, unknown mode, target- or scope-conditioned placement override, graph node or edge embedded in profile configuration, or physical path/chunk/loader declaration in the 015-owned delivery section is blocking and produces no partial profile. Resolution never treats a filename, route label, Target Profile name, or source path as an implicit Delivery Unit.

A usable checked profile can exist before any host Delivery Unit Graph is available. Missing transaction graph input, unsupported graph-artifact revision, identity or digest mismatch, unknown Target Profile applicability, duplicate node or edge, unknown endpoint, self-edge, cycle, non-canonical ordering, unbound or multiply bound reference, or graph resource overrun is a 020-owned planning/linking admission failure rather than a configuration Finding. It produces no partial Requirement Plan or Message Bundle Plan and never mutates the already resolved profile.

A missing or duplicate Group ID, empty group, unknown or duplicate member, unassigned Target Profile, or Target Profile assigned to several groups is a blocking configuration Finding. The resolver reports the conflicting group/member evidence and produces no partial profile by dropping a target, choosing one owner, merging groups, or inventing a single-target group. A valid single-target group produces no Finding.

Group selection happens after a complete profile exists. Omission with exactly one checked group and explicit selection of that group are equivalent. Omission with several groups, an unknown selector, several submitted selectors, or any attempt to select a target subset is a blocking 020 transaction-admission failure. It produces no partial Requirement Plan and does not make the underlying profile invalid.

The selected group's graph applicability must be a pairwise-disjoint exact cover of its Target Profile members. Empty applicability, unknown or out-of-group targets, overlapping graph applicability, an uncovered member, or graph input for an unselected group is a blocking 020 graph-admission failure. One artifact covering several selected targets is valid, and hydration-related targets are not required to share a graph.

An equal hydration endpoint, endpoint outside the group, duplicate relation, endpoint lacking its declared SSR/client capability, or one target appearing in both roles is blocking configuration input. So is unequal canonical requested-locale membership, effective default, negotiation-profile identity/revision, or canonical alias map across a relation. The resolver never intersects locale sets, chooses one default, removes a relation, or infers a replacement endpoint.

Selection divergence in the derived initial-render closure, missing render-equivalence capability evidence, incompatible locale-service behavior, incomplete member output, or mixed Release identity is a downstream 020, 024, 025, 026, or 030 Finding according to the stage that first has complete evidence. These failures never authorize a different definition, client rerender fallback, partial group Release, or cross-group output substitution.

Configuration Findings must follow the source-evidence rules defined above. 015 owns the independence, suppression, outcome, and deterministic-order semantics; the common Finding envelope, evaluation-status representation, query model, and client-specific projection remain owned by 019.

## Dependency, Invalidation, and Reproducibility

This design separates three related decisions:

| Concept | Meaning in 015 |
| --- | --- |
| Profile semantic equality | Whether two complete checked profiles have the same Profile Specification identity/revision and field-for-field equal canonical semantic projections |
| Resolution staleness | Whether an admission or source dependency changed, requiring the resolver to run again even when the next profile may remain semantically equal |
| Reproducibility | Whether the finite immutable input closure needed to regenerate the same resolution outcome is available |

Profile equality is defined by canonical projection equality, not by source-byte equality or by a digest alone. Under one 017-owned framing specification, a digest is the portable fast identity for that projection, but changing Profile Specification identity/revision always changes profile semantics even when all later field bytes happen to match.

Revision `"0"` profile equality includes the checked project and Selection Scope identities; canonical locale sets and defaults; locale, fallback, coverage, delivery, trust, governance, Provider, Glossary, and resource-policy semantics; Target Profile references and their exact semantic capability revisions; Deployment Compatibility Groups and hydration relations; canonicalization-specification and dataset identity; and every other exact semantic reference enumerated below.

It excludes Profile ID and selector; configuration `schemaVersion`; Conformance Suite revision, case IDs, and fixture metadata; raw JSON bytes, member order, accepted authoring spelling, redundant rule evidence, and source positions; Findings and evaluation status; tool binary, package, host-library, adapter-object, and physical provider representation versions; implementation capacity beyond semantic capability references; acquisition, cache, transport, and credential metadata; and compiler-transaction or execution inputs such as selected Group ID, Delivery Unit Graphs, normalized user preferences, Store inventory, Provider results, physical outputs, and Release activation state.

Configuration source evidence is retained separately as diagnostic and dependency-location metadata. Its path or URI, JSON Pointer, and range do not participate in profile semantic equality, the profile digest, or checked-profile serialization. A product adapter or 019 project graph may use the underlying source identity and content revision to schedule re-resolution, but changing only the presentation evidence must not change the resulting checked-profile semantics.

Locale-bearing semantic inputs participate in profile equality and digests through their canonical identifiers, the admitted Locale Canonicalization Specification identity, and the representation-independent canonical dataset identity and digest. Changing only an accepted authoring spelling without changing its canonical form does not change resolved-profile semantics. Changing the admitted specification revision or canonical dataset identity or digest always invalidates the resolved profile and its dependent artifacts, even if the current project's canonical spellings happen to remain byte-identical. Changing only provider schema or physical representation may require re-admission or re-execution, but it does not change profile semantics when the admitted canonical dataset is identical.

The project source-default state is semantic profile input: explicit absence differs from every present canonical locale, and two present states compare through their canonical locale identities. Source occurrences that inherit the default depend on both that state and their omission of an explicit locale. An explicitly sourced application Intent and every admitted library Intent retain their own source locale rather than acquiring a semantic dependency on the project's default, although a profile-identity change may still require admission or graph checks before 019 proves narrower recomputation safe.

The canonical project requested-locale set and the admitted resource-limit-policy reference and revision are semantic profile inputs; authoring order and the physical implementation's capacity are not. Changing set membership invalidates requirement planning and every consumer of the affected requested-locale dimension. Changing only the admitted maximum still changes the checked policy reference, while 019 determines whether downstream work whose locale membership is unchanged can be reused.

The canonical project default, each Target Profile's canonical supported subset and optional override, and each resolved effective default are semantic profile inputs. A project-default change affects every target that inherits it but does not change the effective value of an explicitly overriding target; the profile itself still changes. Target-specific invalidation follows these resolved dependency edges rather than assuming that source locale, project requested default, target override, effective default, negotiation, and message fallback are one coupled value.

The Locale Negotiation Profile identity and revision, portable-lookup revision, canonical alias map, applicable Locale Canonicalization Specification and canonical dataset identity, and admitted negotiation resource-policy reference and revision are semantic profile inputs. Alias-map authoring order is non-semantic; canonical keys are ordered by ascending unsigned UTF-8 bytes for equality and digest inputs after collision detection. Changing any negotiation rule or alias invalidates affected target manifests and execution integrations even when one observed preference fixture still selects the same locale.

The normalized application preference sequence is per-invocation input rather than project-profile identity. One negotiation result depends on that ordered sequence together with the applicable profile revision, target-supported subset, and effective default. Preference acquisition evidence, HTTP header spelling, host API object identity, and user or request identity are not profile digest inputs.

The message locale fallback policy identity and revision, canonical mapping keys, ordered candidate kinds and literal locale identities, applicable Locale Canonicalization Specification and canonical dataset identity, and admitted fallback resource-policy reference and revision are semantic profile inputs. Mapping-member authoring order is non-semantic after canonical key ordering; candidate order within each sequence is semantic. Omission and an explicit empty mapping have identical profile semantics.

A literal fallback candidate contributes a direct dependency on that definition locale without changing requested-locale demand. The Intent source-locale candidate contributes a typed dependency from each applicable requirement to that Intent's checked source locale after 016; changing an unrelated Intent's source locale cannot invalidate another Intent's fallback resolution. Changing fallback policy never erases or rewrites the Store-independent direct requirement, but it invalidates affected Store queries, Linker selection, Bundle Plans, target outputs, and Releases. Current Store inventory, candidate eligibility, and selected artifact identity remain downstream inputs rather than project-profile semantics.

The coverage-policy identity and revision, explicit or defaulted project mode, resolved canonical locale × surface decision table, versioned Intent surface-class vocabulary identity, and admitted coverage resource-policy reference and revision are semantic profile inputs. Rule authoring order and source positions are non-semantic. Two admitted declarations that resolve to the same table under the same policy and vocabulary revisions have the same coverage semantics even when their redundant same-mode rule evidence differs; diagnostic source dependencies may still schedule re-resolution.

Each requirement depends on exactly one table cell selected by its canonical requested locale and checked Intent surface class, plus its separately derived source-equal state. Changing one cell invalidates only requirements in that locale × surface domain after 019 proves the dependency slice; changing the surface vocabulary revision invalidates the complete table. Target membership and delivery applicability may add or remove requirement edges but do not alter the mode of an edge that remains. Store contents, approval state, fallback eligibility, Provider results, and coverage-debt Findings are downstream facts rather than profile equality inputs.

Every present policy reference participates in profile equality and digests through its policy kind, opaque identity, exact policy revision, policy-specification revision, and semantic content digest. Each permitted explicit-absence state is also semantic and differs from every present reference. A change to any of those fields changes the profile even when the newly referenced artifact currently produces the same observed fixture result. The path, URI, cache location, retrieval timestamp, transport encoding, adapter object identity, credential binding, and acquisition evidence do not change profile semantics when the admitted reference and content are identical.

Resource-policy changes invalidate profile resolution and every fact derived under its bounds, although 019 may prove reusable downstream output when admitted values and semantic inputs remain sufficient. Trust/source-admission and approval/selection changes invalidate affected source or Store admission, Linker eligibility, and Release decisions. Provider-routing, refresh, or Glossary changes invalidate affected synchronization work and candidate provenance without authorizing hidden work during a build. An optional-policy transition between present and absent is a semantic change. Exact invalidation slicing remains owned by 019.

Delivery Graph Specification identity/revision and Delivery Placement Policy identity/revision/effective mode are semantic profile inputs. Omitted and explicit `duplicate` authoring have identical profile semantics. A graph artifact's identity, revision, semantic digest, logical nodes, edges, roots, target applicability, and reference bindings are separate compiler-transaction dependencies and never enter profile equality. A graph-only change invalidates affected planning, linking, target output, and Release dependencies while preserving an otherwise equal profile.

Graph source-evidence positions, display labels, artifact file location, bundler object identity, submitted order, worker-completion order, and physical-output facts are non-semantic. When logical graph content and applicability are identical, changing those values does not change graph semantics. Changing an output path, filename, content hash, URL, loader identifier, package registration, or actual load timing may invalidate exporter or deployment work but does not retroactively change the checked graph or profile.

The canonical Target Profile identity set, each Group ID, each canonical non-empty member set, and each canonical directed hydration-relation set are semantic profile inputs. Group and member authoring order, relation authoring order, selector source position, and display labels are non-semantic. Reordering declarations without changing those checked sets produces the same profile semantics and digest inputs.

The Group ID selector and selected group are compiler-transaction dependencies rather than profile equality inputs. Selecting another group reuses the same profile but creates an independent Requirement Plan, Message Bundle Plan, target-output set, and Release dependency closure. Changing group membership invalidates every affected group's planning and Release closure; it cannot migrate an existing plan or Release authority to the new group. Unaffected groups may be reused only after 019 proves their dependency slices.

Each selected target depends on exactly one admitted graph-artifact applicability entry. Changing the graph partition, even with byte-identical graph nodes and edges, invalidates the affected transaction because target applicability is semantic transaction input. A graph shared by several targets creates one graph-content dependency plus an applicability edge for each target; it does not merge their Target Profile or output identities.

Each hydration relation depends on both endpoint Target Profile revisions, canonical requested-locale sets, effective defaults, negotiation profile and aliases, MF2 capability admission, Locale Service compatibility evidence, group-scoped Linker selections for the initial-render closure, both complete target output sets, and the final Release identity. Changing one edge does not add render-equivalence requirements to unrelated co-members, but it changes the profile and invalidates that group's complete Release compatibility evaluation.

Resolution staleness is tracked separately from equality. The initial implementation treats the source identity and content revision of the complete materialized `IntlifyConfig`, configuration `schemaVersion`, Profile ID selector including omission, Profile Specification identity/revision, canonicalization specification and data artifact, every referenced semantic artifact, Policy Artifact Set admission envelope, and applicable implementation-capability admission as re-resolution dependencies. A change to any of them schedules resolution again.

Because the initial file integration tracks the complete `intlify.config.json`, changing an unselected declaration also schedules re-resolution. It never composes that declaration into the selected profile. If the complete configuration remains admitted and the selected declaration produces the same canonical semantic projection, profile equality and its digest remain unchanged and downstream work may be reused. If the edit makes root structural admission or profile selection invalid, the new outcome is blocked. 019 may later prove a narrower source dependency slice, but it cannot change these resolution semantics or infer cross-profile composition.

The reproducibility input closure contains the materialized JSON-compatible `IntlifyConfig` value rather than its physical source encoding; the selector state; configuration schema version; Profile Specification identity/revision; Locale Canonicalization Specification and representation-independent dataset identity/digest; exact identities, revisions, specification revisions, and semantic digests for every referenced Target Profile, policy, and other semantic artifact; the Policy Artifact Set facts examined for bounded admission; and the resource-policy and implementation-capability admission that governed evaluation. Unreferenced artifact payloads, tool binary identity, host object identity, and provider physical representation are not semantic profile inputs, although an operation may record them separately as provenance.

The same closure must produce the same checked-or-blocked outcome, byte-identical canonical profile projection when checked, ordered Finding semantics, and evaluated/not-evaluated dependency structure. Exact reproduction of diagnostic locations additionally requires the same canonical source-evidence input; client-specific line/column presentation may still differ. Provider representation, JSON member order, filesystem enumeration, concurrency, optional frontend implementation, host-language object identity, conforming tool binary, and host locale-library version cannot change the semantic result under the same admitted specifications.

Finding source locations and presentation details remain non-semantic profile evidence. A non-blocking Finding never changes the semantic completeness of a checked profile, and a blocked outcome never contributes a partial profile identity. 017 owns canonical encoding and digest framing; 019 owns dependency slicing, cache keys, staleness scheduling, and downstream reuse decisions.

## Security and Credential Handling

The resolved profile may identify Provider, Store, trust, publication, or delivery policy by immutable reference, but it must not carry Provider/TMS secrets, reviewer credentials, publication signing keys, deployment credentials, or production request data into ordinary compiler or execution consumers.

This section will define the profile-specific credential exclusion and redaction requirements. The complete trust, provenance, authorization, and signature specification remains owned by 018.

A canonicalization provider is a pure input to one resolver invocation. It cannot perform implicit network access, read credentials, discover an unpinned host data source, or mutate its artifact while resolution is running. Data artifact integrity, size, decoded-allocation, and work limits must be checked before any locale result is trusted; 017 and 018 own the shared artifact and trust mechanisms.

Configuration byte size, nesting, collection-member count, decoded allocation, and locale-canonicalization work are bounded independently from the semantic maximum requested-locale cardinality. Limit checks occur before the work they protect, use explicit admitted values or implementation capabilities, and fail closed without truncating input or consulting ambient host-memory heuristics.

Finding count, related-evidence count and bytes, evaluation-status entries, and diagnostic-ordering work are independently bounded. The resolver reserves enough bootstrap capacity to report one terminal reporting-limit Finding and incomplete evaluation status even when the semantic resource-limit artifact cannot be admitted. A reporting overrun is blocking, exposes no profile, and never disguises a bounded diagnostic prefix as complete evaluation.

Negotiation alias count and per-invocation normalized-preference count are independently bounded by the admitted resource-limit policy. An execution adapter parses and bounds raw protocol input before materializing the normalized sequence; the core negotiator then enforces its own semantic sequence and work bounds before matching. Neither layer may truncate input and treat the truncated prefix as authoritative.

Fallback mapping-source count, per-source candidate count, and total Linker probe work are independently bounded by the admitted resource-limit policy. Configuration resolution checks mapping and sequence bounds before retaining them; 020 checks transaction-wide expanded probe work after finite Intent and requirement admission. Neither layer may truncate a chain, omit an Intent, or return the first portion of a Bundle Plan as complete.

Coverage rule count, selector occurrences, resolved decision-table cells, and rule-domain comparison work are independently bounded by the admitted resource-limit policy. The resolver preflights submitted collections before comparison and checks the finite locale × surface cross-product before materializing any authoritative table. It never drops overlapping rules, surface classes, or project locales to fit a limit.

Policy-reference count, encoded length, artifact count, artifact byte size, decoded allocation, validation depth, and admission work are bounded before policy-body interpretation. The Policy Artifact Set is explicit and finite; neither the resolver nor a policy validator may fetch a missing artifact, consult a mutable registry, or substitute a locally cached revision. A limit or reference-closure failure is blocking and never drops a policy suffix to create a smaller authoritative profile.

Because the resource-limit artifact is itself inside that input, pre-admission Policy Artifact Set byte, count, depth, allocation, and work bounds come from explicit implementation-capability admission rather than from the untrusted policy being opened. After that artifact is admitted, its versioned semantic limits govern the remaining resolution and downstream stages. An implementation never lets the artifact being checked enlarge the bootstrap envelope needed to check it.

Policy references and normalized profile facts are safe to expose to ordinary compiler consumers only after the owning specifications classify their fields as non-secret. Credential material, secret-resolving locators, reviewer sessions, trust-bootstrap secrets, and runtime authentication handles remain in separately authorized operation inputs and must be redacted from configuration Findings and dependency evidence.

Host graph inputs are untrusted bounded artifacts. Graph-artifact count, encoded bytes, node and edge occurrences, ID lengths, target-applicability entries, reference bindings, decoded allocation, cycle-detection work, and placement work are checked before or during the protected phase under explicit implementation and admitted resource limits. Validation never follows filesystem paths, URLs, loader IDs, or build-host object references and never expands labels into graph structure.

A graph failure is fail-complete for its planning/linking transaction. The host and 020 never remove a cycle edge, deduplicate a conflicting occurrence, drop an unknown target or unbound reference, collapse units into `["main"]`, split a graph to fit a bound, or substitute a previous cached graph. Evidence is bounded and refers to logical identities and non-secret host source locations without serializing arbitrary build-system objects.

Target Profile count, Group ID bytes, group count, submitted membership occurrences, members per group, hydration-relation occurrences, relation endpoint bytes, graph-applicability entries, and compatibility-comparison work are bounded independently. Checks account for every submitted occurrence before duplicate collapse and use checked arithmetic and deterministic canonical order. A limit failure never truncates a group, drops a target or relation, selects only the first group, or treats an incomplete membership partition as authoritative.

## Consumer Semantics

This section will define the exact subset of profile facts each downstream stage may consume.

| Consumer | Profile facts to specify |
| --- | --- |
| Source producer | Present-or-absent project source-default state, application inheritance rule, library non-inheritance rule, and applicable source-policy references |
| Project graph and query service | Semantic profile identity, complete re-resolution dependency closure, source evidence, checked-or-blocked outcome, Findings, evaluated/not-evaluated status, suppression causes, staleness scheduling, downstream reuse slices, and explanations |
| Requirement planner | Exactly one selected Deployment Compatibility Group and its complete target set; canonical project requested-locale set and default; Target Profile subsets and effective defaults; target applicability; direct demand preserved independently of fallback; effective locale × surface coverage mode; source-equal path; rule evidence; and profile delivery-specification/placement inputs; it receives the exact graph partition separately through 020 |
| Synchronization | Non-source-equal direct locale demand regardless of fallback permission, exact Provider-routing/refresh and Glossary references or their checked absence, and separately authorized credential bindings |
| Governance and Store | Selection Scope and exact approval, selection, trust, and source-admission policy references without credential material |
| Message Linker | Selected group identity, requested locales, checked fallback mapping and source-locale candidate, effective coverage mode and source-equal path, source and Store admission inputs, target applicability, hydration initial-render applicability, profile placement policy, and the separately admitted logical Delivery Unit Graph partition |
| Target Exporter | Selected group and Target Profiles, locale subsets, applicable negotiation-profile identity and revision, hydration role, and selected logical placement; it owns physical paths, resources, hashes, URLs, and loader relationships |
| Release Assembly | One selected Deployment Compatibility Group, every complete member output, hydration compatibility evidence, and the exact applicable approval, selection, trust, and resource-policy references |
| Execution integration | One deployment-admitted Release identity; supported locales; effective default; locale-negotiation profile reference; canonicalization dependency; normalized preference input requirements; direct-selection membership rule; exact materialized definition with no runtime message fallback; and same-Release enforcement for hydration-coupled outputs |

The completed design must prevent consumers from silently applying their own defaults or reinterpreting unchecked `IntlifyConfig` input.

## Conformance and Fixtures

The Project Profile Resolver Conformance Suite is a versioned machine-readable suite whose initial revision is `"0"`. It tests the 015-owned semantic resolver independently of a specific CLI, binding API, physical canonicalization provider, or public checked-profile encoding.

### Case manifest and expected outcome

Every case has one stable case ID and a machine-readable manifest containing:

- Conformance Suite revision and applicable Profile Specification revision;
- applicability to raw-file admission, materialized file value, programmatic value, shared semantic resolution, or a generated corpus;
- raw `intlify.config.json` bytes when syntax, duplicate-member, token-range, or file evidence matters, and otherwise one exact JSON-compatible `IntlifyConfig` value;
- Profile ID selector state, including omission, plus entry-path-specific selector evidence when applicable;
- the exact Locale Canonicalization Specification and data identity, Policy Artifact Set admission facts, referenced policy and Target Profile artifacts, resource policy, and implementation-capability input needed by the case;
- the component that is expected to establish the result: raw-file adapter, JSON Schema validator, 015 semantic resolver, or an explicitly named downstream owner;
- one expected checked or blocked outcome;
- equivalence, semantic-mutation, boundary, suppression, or generated-corpus relation IDs connecting the case to its comparison cases; and
- Decision Log IDs and normative-rule IDs that the case verifies.

A checked expected outcome contains the fixture-only canonical JSON view of the complete profile semantic projection, its Profile Specification identity/revision and digest inputs, any ordered non-blocking Findings, complete evaluation status, and the expected re-resolution and semantic dependency sets. A blocked expected outcome contains no profile and records ordered Findings, evaluated/not-evaluated status with canonical suppression causes, and the dependency facts admitted before blocking.

The profile JSON view is a test representation ordered by the 015 semantics. It does not reserve a Rust type, public field spelling, shared-artifact encoding, or wire compatibility rule; 017 remains the owner of canonical artifact encoding and digest framing. Finding expectations assert code, severity, blocking state, semantic subject, primary and related evidence, safe suggestion when present, canonical order, and causal Finding identities.

### File and programmatic pairing

Every case whose input can be represented as a JSON-compatible value runs through both a file-value adapter and a programmatic-value adapter. Both paths must produce the same semantic outcome, canonical profile projection, Finding semantics and order, evaluation status, and dependency sets. Their expected origin, path or URI, byte range, call-site span, and other source-evidence fields are stored as entry-path-specific expectations and may differ.

Raw JSON syntax failures, duplicate object members, malformed encoding, and exact token-range behavior are file-only because no materialized JSON-compatible programmatic value can preserve them. Programmatic source-label, call-site, and host-value-boundary cases may be programmatic-only. Every unpaired case declares the reason and its adapter owner in the manifest; an unpaired semantic-resolver case is invalid. Platform-specific configuration semantics are not an allowed exception.

### Sufficiency and traceability

Suite revision `"0"` is sufficient only when its generated traceability report satisfies all of these conditions:

- every testable Accepted Decision and normative 015 invariant maps to at least one positive case and, where rejection or suppression is meaningful, at least one negative case;
- every non-fixture ownership, packaging, static-generation, or integration decision maps to an explicit verification owner and check instead of being silently marked covered;
- every resource bound has an exact-bound case and a first-over case;
- every class of non-semantic input has an equivalence relation proving an unchanged profile, while every semantic dependency class has a mutation relation proving the expected equality or invalidation change;
- Finding independence and each dependency-suppression class have cases that assert both emitted Findings and checks deliberately left not evaluated;
- each raw-file-only or programmatic-only exception has a machine-readable reason and a paired semantic case when the underlying semantic value is representable;
- every generated corpus records its normative source identity, revision, complete source digest, generator revision, projection rule, and all explicit outside-domain cases, with zero unexplained omissions or mismatches; and
- no Accepted Decision, normative rule, or declared case relation remains unmapped.

The suite and traceability report run in CI for `intlify_config`; each future binding or product adapter runs the applicable entry-path subset, and each alternate physical resolver implementation runs the complete shared semantic subset. A semantic change to checked profile expectations, Finding semantics, suppression behavior, or dependency identity requires a corresponding Accepted design decision or Profile Specification revision. A Conformance Suite revision alone may add coverage, provenance, or non-semantic fixture metadata, but it cannot silently redefine resolver semantics.

Suite revision `"0"` includes at least the following coverage:

- JSON Schema success and failure, including missing, unknown, incorrectly typed, and incompatible-version fields;
- exact configuration `schemaVersion: "0"`, unsupported explicit versions, and `$schema` values that do not alter resolver semantics;
- unknown root and nested fields rejected at exact evidence, including the same Finding through file and programmatic paths;
- file evidence containing a project-relative path, JSON Pointer, and UTF-8 byte range, with line and column positions derived by an adapter;
- programmatic evidence containing a stable source label or URI and JSON Pointer, both with and without an optional call-site span;
- profile-selector evidence originating from both a CLI option and a programmatic argument;
- equivalent file and programmatic failures with the same Finding semantics but different origin and location evidence;
- source evidence excluded from profile semantic equality, profile digest, and checked-profile serialization;
- Profile Specification revision `"0"` admitted independently of configuration `schemaVersion`, with either revision changing only its own version domain;
- missing, unsupported, or incompatible Profile Specification identity/revision producing a blocking admission Finding;
- two declarations with different Profile IDs and selectors but the same checked project identity and canonical semantic projection producing equal profiles and digest inputs;
- a future configuration schema revision and revision `"0"` authoring that resolve to the same Profile Specification revision and canonical projection producing equal profiles, while both remain distinct re-resolution inputs;
- changes to any canonical semantic field or exact semantic reference identity, revision, specification revision, or content digest changing profile equality;
- raw JSON encoding, object order, accepted spelling, source evidence, Finding presentation, conforming tool binary, host library, and physical provider representation changes leaving profile equality unchanged;
- selected Group ID, Delivery Unit Graph, normalized user preferences, Store inventory, Provider results, physical outputs, and Release activation excluded from profile equality while remaining dependencies of their owning transactions;
- a valid edit confined to an unselected declaration scheduling initial file-based re-resolution but preserving the selected profile digest when its canonical projection is unchanged;
- an unselected-declaration edit that breaks complete-root structural admission producing a blocked new outcome without composing that declaration into the selected profile;
- equal reproducibility input closures producing the same outcome, byte-identical canonical profile projection, Finding order, and evaluation-status dependency structure across conforming implementations;
- changed source-location evidence preserving profile equality while exact diagnostic-location reproduction requires that evidence in addition to the semantic closure;
- a checked outcome containing one complete profile plus deterministic non-blocking canonical-replacement Findings;
- a blocked outcome containing all independently provable Findings under the admitted bounds and no partial profile;
- invalid independent sibling locales, policy references, Target Profiles, or groups all producing Findings rather than stopping at the first blocking failure;
- one invalid locale or Target Profile suppressing only checks that require its unavailable canonical identity or checked profile, while independent subjects continue;
- unsupported configuration version, non-materializable root shape, missing canonicalization semantics, or missing safe resource bounds suppressing every downstream check that cannot be interpreted or bounded safely;
- dependency-suppressed checks appearing only as not evaluated with canonical causal Finding identities, never as fabricated cascade Findings;
- identical outcome, Finding order, and suppression-cause sets under permuted JSON members, filesystem enumeration, worker scheduling, and concurrency;
- exact reporting-bound and first-over cases for Finding count, related evidence, evidence bytes, evaluation-status entries, and diagnostic work, with reserved terminal limit evidence and no profile on overrun;
- future recognized schema sections admitted only after an explicit schema and implementation update, with no generic pass-through extension behavior;
- equivalent CLI-adapter and direct-`intlify_config` inputs that produce the same profile or Findings;
- one declared profile with an omitted or explicit valid selector;
- several declared profiles with an explicit valid selector;
- missing, unknown, and ambiguous profile selection with no partial profile output;
- independently resolved profiles that prove there is no implicit cross-profile merge;
- byte-distinct JSON documents and programmatic values that materialize the same `IntlifyConfig` semantics;
- valid Unicode BCP 47 Locale Identifiers covering language, script, region, and Unicode locale extensions;
- arbitrary opaque and platform-specific identifiers plus `en_US`, `root`, script-leading, POSIX, and legacy ICU forms rejected by the shared resolver;
- explicit compatibility conversions such as `en_US` to `en-US`, `root` to `und`, and `Latn` to `und-Latn` producing the same checked locale as direct standard-form input while retaining conversion evidence and leaving shared resolver semantics unchanged;
- a syntactically well-formed primary language subtag longer than three characters rejected as invalid under the pinned CLDR `48.2.0` validity data before ICU4X parsing;
- a fixture for a hypothetical later specification that admits such a language subtag, proving that the ICU4X `2.2.0` adapter is blocked unless a conforming wrapper or upgraded engine handles it;
- syntactically well-formed but invalid cases such as `zz`, `en-u-ca-madeup`, and `en-u-zz-abc` rejected with a blocking configuration Finding before ICU4X canonicalization;
- generated positive and negative validity boundaries for language, script, region, variant, Unicode extension key/type, and transformed-extension key/type data;
- an adapter that rejects a normative valid fixture blocked from admission instead of projecting the failure as a user-input Finding;
- deprecated values admitted only when a deterministic canonical replacement is present;
- `regular`, `special`, `macroregion`, and `unknown` CLDR-status components admitted, including `en-XA`, `ar-XB`, `en-XK`, `und`, and `en-ZZ`;
- CLDR `reserved` and `private_use` components rejected before canonicalization, including representative `qfz`, `Qaaq`, and `XC` cases in both the primary language identifier and a `t` extension's transformed-language field;
- registered and valid `u` and `t` extension fields admitted under the pinned Unicode BCP 47 data, including any publicly registered private-use function distinguished from an opaque `-x-` sequence;
- `en-x-brand`, `en-a-foo`, and equivalent private-use or non-`u`/`t` extension inputs rejected as complete identifiers without stripping the rejected suffix or resolving the remaining prefix as another locale;
- a hypothetical future specification that admits private-use or another registered extension requiring a new canonicalization-specification revision plus explicit scope, canonicalization, matching, composition-collision, target-capability, and semantic-consumer rules;
- `und-u-ca-islamicc` canonicalized to `und-u-ca-islamic-civil` by generated correction data;
- generated coverage for every valid-direct-input CLDR BCP 47 key/type alias, separating ICU4X-delegated mappings from Intlify-corrected mappings without a handwritten exception list;
- legacy-only key/type aliases excluded from the direct correction set and left to explicit compatibility conversion;
- every generated alias chain flattened to its final preferred value and every corrected result remaining byte-identical after a second canonicalization pass;
- artifact or adapter admission rejected for a cyclic, unrepresentable, or otherwise non-deterministic correction mapping;
- non-canonical casing, deprecated aliases, and extension ordering converted to one canonical identifier;
- each admitted non-canonical authoring spelling producing a non-blocking Finding with its exact canonical replacement and source evidence while still permitting one complete profile;
- exact duplicate spellings and distinct aliases that canonicalize to one identity producing the same blocking duplicate-locale Finding semantics, with every conflicting occurrence identified and no first-wins or silent-deduplication behavior;
- the same canonical locale used in independent semantic roles or collections remaining valid when no profile invariant relates their uniqueness scopes;
- semantic locale sets sorted after canonicalization and duplicate detection by ascending unsigned UTF-8 bytes, including prefix cases such as `en` before `en-US`;
- permutations of one authored locale set producing byte-identical checked-profile ordering and digest inputs without using locale-aware or host-dependent collation;
- explicitly ordered fallback, negotiation-preference, and equivalent locale sequences retaining their specification-resolved order rather than receiving set ordering;
- `en` preserved without likely-subtag maximization and kept distinct from `en-US`;
- authoring spellings excluded from checked-profile equality and digests when their canonical identifiers are equal;
- identical canonical output across hosts with different ICU, ECMA-402, and operating-system locale data;
- missing, unsupported, and incompatible Locale Canonicalization Specification identities rejected without host fallback;
- canonicalization-specification identity included in profile equality and digests, with a revision change invalidating dependent artifacts;
- configuration `schemaVersion` remaining unchanged when only the canonicalization specification is revised;
- `intlify_config` built and exercised without ICU4X default compiled data or any embedded CLDR-derived table;
- the ICU4X `2.2.0` reference adapter using `try_new_extended_with_buffer_provider` through the ICU4X 2.2 serialized-provider schema, with all four required marker families and no identifier maximization;
- specification revision `"0"` remaining independent of configuration `schemaVersion` and admitting only the selected logical marker data from `icu_locale_data` `2.2.0` plus the minimal conformance data derived from CLDR `48.2.0`;
- the same selected logical dataset producing one full SHA-256 semantic digest across conforming baked and blob exports while each physical export has its own transport digest;
- the generated artifact manifest recording the immutable upstream package checksum, source-data revision, marker set, and literal semantic digest without making generated digest text normative prose;
- equivalent baked and serialized-blob providers producing byte-identical canonical profiles for the same admitted data identity;
- missing data, provider-schema mismatch, digest mismatch, truncated data, omitted required marker families, and omitted conformance data rejected before canonicalization;
- baked-versus-blob representation, provider schema, and transport digest excluded from profile semantics while the specification identity and representation-independent canonical dataset identity and digest remain semantic dependency inputs;
- the selected ICU4X adapter passing all Intlify locale conformance fixtures, with every known gap explicitly wrapped, excluded by an admitted-domain revision, or rejected;
- valid-but-noncanonical identifiers corrected by the ICU4X path or explicit Intlify override data, while merely well-formed but invalid identifiers are rejected;
- an uncorrectable reference-engine gap blocking adapter admission rather than silently producing host- or engine-specific semantics;
- all four sets in the pinned CLDR `48.2.0` locale-canonicalization corpus projected into hyphen-separated direct-domain fixtures where representable;
- every non-projectable upstream corpus case retained as outside-domain evidence with a machine-readable reason and no silently skipped case;
- each admitted corpus case compared across its normative expected result, raw ICU4X `2.2.0`, and ICU4X through the Intlify conformance layer;
- a generated machine-readable gap registry that records delegated, corrected, outside-domain, and blocking dispositions, with zero unexplained wrapper mismatches required for adapter admission and CI;
- the corpus revision, repository path, full source-content SHA-256 digest, generated registry, and run outcome recorded as adapter conformance evidence without making the test-corpus byte digest a profile or canonical-data-artifact semantic input by itself;
- hand-authored regression fixtures supplementing but never overriding generated normative corpus expectations;
- a provider that attempts implicit I/O or changes data during resolution being outside the conforming provider boundary;
- one Target Profile in one valid single-target Deployment Compatibility Group;
- several Target Profiles partitioned exactly once across independently released Web, mobile, native, or service groups;
- missing or duplicate Group IDs, empty groups, unknown or duplicate members, unassigned targets, and overlapping group membership producing blocking configuration Findings with no inferred or partial group;
- group and member declaration permutations producing the same canonical profile ordering and digest inputs;
- omitted selection and explicit exact selection being equivalent for a one-group profile;
- omitted selection with several groups, an unknown Group ID, multiple selectors, and a target-subset selector failing 020 transaction admission without invalidating the profile;
- one selected group producing exactly one group-scoped Requirement Plan, Message Bundle Plan, complete member-output set, and Release Snapshot without publishing a valid member prefix after another member fails;
- graph-artifact applicability subsets being non-empty, pairwise disjoint, and an exact cover of selected group members, including one graph validly shared by several targets;
- hydration-related Browser and SSR targets being allowed to use different graph artifacts and logical Delivery Unit structures;
- hydration coupling declared only through explicit directed SSR-to-Browser relations, with no inference from names, platform labels, graph edges, output formats, or co-membership;
- valid finite fan-out and fan-in hydration relations, while duplicate pairs, equal endpoints, endpoints outside the group, missing role capability, and one target used in both roles are blocking;
- every hydration pair having equal canonical supported requested-locale sets, effective defaults, negotiation-profile identity/revision, and canonical alias maps;
- any hydration locale, default, negotiation, or alias mismatch being rejected without intersecting sets or accepting one observed matching negotiation result;
- the same normalized preference sequence selecting the same requested locale on both hydration endpoints;
- every shared initial-render Intent retaining the same Intent revision, selected artifact identity, definition locale, and selection/admission evidence across both outputs;
- different physical engines, output formats, Locale Service implementations, and graphs remaining valid only with capability and conformance evidence that proves equal logical text or structured parts for the same checked input;
- hydration equivalence not requiring graph isomorphism, equal chunking, equal resource bytes, or unconditional byte-for-byte serialization;
- missing render-equivalence evidence, divergent selected definition, incompatible locale-service behavior, or incomplete member output blocking the applicable downstream stage without a client rerender or alternate-definition fallback;
- both hydration outputs bound to one Release Snapshot, with staggered physical activation permitted but mixed-Release coupled execution rejected explicitly;
- relation-free co-members sharing Release compatibility without acquiring an implicit render-equivalence relation;
- independently grouped targets retaining separate plans, Releases, publication, activation, and rollback authority while compatible synchronization may deduplicate Provider demand without merging those authorities;
- group membership and hydration relations participating in profile semantics while the selected Group ID remains compiler-transaction input; and
- exact and first-over Target Profile, group, Group ID byte, membership, hydration relation, graph-applicability, and compatibility-work limits without truncation or prefix selection;
- a single-locale application with exactly one explicit `requestedLocales` member;
- missing and empty `requestedLocales` rejected without inference from source defaults, requested defaults, Target Profiles, source Intents, host locale state, CLDR coverage, or Provider availability;
- wildcard, `all`, language-range, query, and other dynamically expanded requested-locale declarations rejected in revision `"0"`;
- the canonical unique requested-locale cardinality accepted exactly at the admitted `maxRequestedLocales` value and rejected at the first value above it without truncation or partial profile output;
- duplicate and alias-collision inputs remaining blocking even though repeated canonical identities do not increase semantic set cardinality;
- generic raw-input member and work limits rejecting duplicate-heavy input before it can evade protection through semantic collapse;
- a missing, non-positive, non-finite, incompatible, or implementation-unsupported maximum-cardinality policy rejected rather than replaced by an ambient host or implementation default;
- a required canonical project `defaultRequestedLocale`, including a single-locale project that does not infer the sole `requestedLocales` member;
- a project `defaultRequestedLocale` outside the canonical project requested-locale set rejected without selecting another member;
- each Target Profile declaring a non-empty requested-locale subset of the project set;
- a target override taking precedence over the project default and being accepted only when it belongs to that target's subset;
- a Target Profile without an override inheriting the project default only when that default belongs to its subset, and otherwise producing a blocking Finding without first-member, sole-member, sorted-member, or negotiated-locale inference;
- independently released Target Profiles resolving different effective defaults through explicit overrides;
- the project requested default remaining independent from `defaultSourceLocale` and message locale fallback;
- locale negotiation consuming the already resolved effective default as its terminal no-match result without choosing or mutating default authority;
- a portable-lookup revision `"0"` profile selecting an exact target-supported canonical preference before consulting an alias or a less-specific candidate;
- an ordered sequence such as `de`, `fr` preserving application priority and selecting the first matching preference rather than canonical target-set order;
- `fr -> fr-FR` selecting `fr-FR` when that project alias destination is supported by the target, and the same alias remaining inapplicable without adding target membership when another target excludes `fr-FR`;
- canonical duplicate or conflicting alias keys, non-project alias destinations, unsupported profile revisions, and first-over `maxNegotiationAliases` declarations producing blocking configuration Findings with no partial profile;
- lookup candidate generation covering exact identifiers, progressively less-specific candidates, and Unicode singleton boundaries, with `u` and `t` extensions never being reattached to a different supported locale;
- an empty normalized preference sequence, an exhausted sequence, and a sequence with no applicable exact, alias, or less-specific match all returning the already resolved effective default;
- direct selection accepting exactly a canonical target-supported member and never silently negotiating or defaulting an unsupported value;
- raw `Accept-Language` syntax, quality weighting, wildcards, exclusions, `navigator.languages`, and operating-system preference acquisition being normalized by adapters rather than stored in or parsed by `LocalizationProjectProfile`;
- exact and first-over `maxLocalePreferences` cases proving that over-limit input is rejected without truncating the sequence into an authoritative result;
- identical portable-lookup inputs producing identical results across conforming hosts, while a host best-fit result cannot claim the portable profile revision;
- independently released targets being allowed to negotiate differently from different supported subsets, with hydration-coupled compatibility left to the applicable group validation;
- omitted and explicitly empty fallback declarations producing the same canonical empty policy and direct-only candidate order for every requested locale;
- one requested-locale mapping producing an implicit direct candidate followed by its exact declared literal and Intent-source candidates, without permitting the requested locale to be authored again;
- mapping keys outside the canonical project requested-locale set rejected, while a valid literal definition candidate outside that set remains admitted without adding requested or target membership;
- an Intent source-locale candidate resolving independently to an inherited application source, an explicit application source, and a published library source;
- a missing checked Intent source locale failing at the 016-owned source-admission stage rather than being replaced by `defaultSourceLocale` during linking;
- `ja-JP -> [ja, Intent source locale]` producing `ja-JP -> ja -> en` for an English-source Intent and `ja-JP -> ja -> de` for a German-source library Intent;
- no implicit parent, project or target requested default, project source default, negotiation result, host locale, Store locale, or Provider locale being appended to a fallback sequence;
- `ja -> [en]` and `en -> [fr]` remaining two independent complete sequences, with `ja` never recursively reaching `fr`, while reciprocal entries remain finite;
- mapping-member permutations producing identical profile ordering and digest inputs while a candidate-order permutation changes policy semantics;
- an explicit empty sequence, literal self-reference, duplicate literal, repeated Intent source-locale candidate, invalid literal locale, unsupported policy revision, and first-over `maxFallbackSources` or `maxFallbackCandidatesPerSource` declaration producing a blocking configuration Finding;
- exact and first-over `maxFallbackResolutionProbes` transactions proving that 020 never truncates a chain, omits an applicable Intent, or emits a partial Bundle Plan;
- a direct-required missing definition remaining blocking despite an eligible fallback, and a fallback-allowed requirement retaining direct localization demand and coverage debt after selection of an eligible fallback;
- a source-equal requested locale using checked source fulfillment directly rather than classifying it as fallback;
- 020 selecting exactly one eligible definition and retaining its definition locale and probe evidence without allowing fallback policy to approve or choose among same-locale artifacts;
- target output materializing the selected fallback definition under the requested locale while runtime and native execution never search the policy chain;
- omitted coverage configuration, an omitted project default, and an explicit `direct-required` default producing the same strict resolved table when no override rule exists;
- an explicit `fallback-allowed` project default changing every otherwise unmatched table cell without erasing direct demand;
- exactly `direct-required` and `fallback-allowed` admitted as configured modes, with `source-equal` rejected as an authored mode and derived separately per requirement;
- finite locale-only, surface-only, and locale-plus-surface rules matched against the project requested-locale set and versioned checked surface-class vocabulary;
- Target Profile, Deployment Compatibility Group, Delivery Unit, Provider, Store, source locale, definition locale, source path, package path, and runtime state rejected as revision-`"0"` coverage selector dimensions;
- a locale-plus-surface rule taking precedence over matching locale-only and surface-only rules regardless of authoring order;
- overlapping incomparable maximal rules with equal modes resolving that mode and retaining canonical explanation identities, while different modes produce a blocking conflict unless a more-specific rule covers the overlap;
- rule permutations and JSON member permutations producing the same complete locale × surface decision table, with no first-authored or canonical-locale-order tie-break;
- an unknown mode, unconstrained rule, empty selector, out-of-project locale, unknown surface class, duplicate canonical selector, unresolved maximal-rule conflict, and unsupported policy revision producing a blocking configuration Finding with no partial table;
- exact and first-over `maxCoverageRules`, selector, decision-cell, and resolution-work limits proving that no rule, locale, surface, or table suffix is truncated;
- each Requirement Plan record retaining its effective coverage mode, source-equal state, and canonical rule or default evidence independently from target and delivery applicability;
- direct-required blocking Release Assembly when the direct candidate is missing, stale, invalid, unapproved, or otherwise ineligible despite an eligible configured fallback;
- fallback-allowed preserving non-source-equal Provider demand and emitting visible typed non-blocking coverage debt whenever 020 selects an eligible fallback;
- fallback-allowed with no eligible direct or fallback definition remaining blocking rather than becoming ignored debt or runtime fallback;
- source-equal fulfillment creating no Provider work while still enforcing applicable source admission, approval, provenance, and trust requirements;
- coverage rules unable to reorder fallback, approve an artifact, override a Selection Decision, change Provider routing, or condition production execution;
- a one-cell coverage change invalidating only its dependent locale × surface requirements when 019 can prove that slice, while a surface-vocabulary revision invalidates the complete table;
- a present `defaultSourceLocale` canonicalized into the profile and inherited only by application-owned Intents that omit an explicit source locale;
- an omitted `defaultSourceLocale` producing a usable profile with an explicit absent state and no configuration Finding, including projects with no localizable Intents and projects whose application Intents are all explicitly sourced;
- no substitution of `und`, an empty string, host locale, default requested locale, or another inferred value for an absent project source default;
- an application Intent that omits its source locale resolving through a present project default, and the same Intent producing a blocking 016-owned Finding when the profile default is absent;
- an explicit application Intent source locale remaining authoritative over a different project default;
- every library Intent retaining its published source locale without inheriting the consuming application's default, and missing library source-locale evidence failing library admission;
- the source-default present/absent state and canonical value participating in profile equality and digests while authoring spelling remains non-semantic;
- direct-required and fallback-allowed coverage;
- locale negotiation distinct from message locale fallback;
- required exact resource-limit, trust/source-admission, and approval/selection references producing one complete checked profile;
- an explicit immutable no-additional-human-approval policy being valid while omission of approval policy is blocking;
- missing required policy references producing blocking Findings without an inferred permissive or implementation default;
- explicit Provider-routing absence allowing profile resolution and an existing-Store build, while explicit synchronization with non-source-equal Provider demand produces a typed 022 failure;
- explicit Glossary Set absence remaining a checked no-glossary state without creating an empty artifact or consulting an ambient glossary;
- each typed reference resolving to exactly one artifact matching its policy kind, opaque identity, exact revision, policy-specification revision, and semantic digest;
- `latest`, semantic-version ranges, branch names, mutable tags, timestamp-only identities, and environment-selected defaults being rejected as floating policy references;
- zero-match, multiple-match, cross-kind, unsupported-specification, identity-mismatch, revision-mismatch, and digest-mismatch references producing blocking Findings;
- one policy identity/revision pair presented with different semantic digests producing a conflict rather than replacement or last-wins behavior;
- equivalent file and programmatic declarations resolving through the same finite already acquired Policy Artifact Set with no network, registry, workspace, or environment discovery;
- profile output retaining exact references and normalized 015-owned facts such as admitted resource bounds without copying unrelated Provider, governance, trust, or Glossary bodies;
- credentials, secret-resolving locators, reviewer sessions, trust-bootstrap secrets, and runtime authentication handles remaining outside references, profile serialization, Findings, and ordinary compiler inputs;
- changing only artifact path, cache location, retrieval timestamp, transport representation, or adapter object identity preserving profile semantics when the admitted reference and content are identical;
- changing policy kind, identity, exact revision, policy-specification revision, semantic digest, or a permitted present/absent state changing profile semantics and invalidating the applicable consumer dependencies;
- Provider-routing, refresh, and Glossary changes invalidating affected synchronization work without triggering Provider access during a normal build;
- trust/source-admission or approval/selection changes invalidating affected admission, Linker eligibility, and Release decisions;
- exact and first-over policy-reference, artifact-count, artifact-size, decoded-allocation, validation-depth, and admission-work limits without dropping a policy or emitting a partial profile, including pre-admission artifact-set limits that cannot be enlarged by the resource policy being checked;
- omitted and explicit `duplicate` placement producing the same Delivery Placement Policy revision-`"0"` checked profile fact;
- `hoist`, an unknown mode, and target- or scope-specific placement overrides producing blocking configuration Findings rather than normalizing to `duplicate`;
- profile configuration containing only the Delivery Graph Specification and placement policy, with submitted graph nodes, edges, reference bindings, and physical output declarations rejected from that configuration surface;
- the same checked profile being reusable with distinct dev, production, Browser, SSR, mobile, native, and whole-program graph artifacts;
- each graph artifact carrying exact identity, revision, semantic digest, finite logical content, and Target Profile applicability;
- logical Delivery Unit identities remaining independent of absolute, current, output, and temporary paths, filenames, URLs, hashes, platform enums, numeric chunk IDs, array positions, random values, and worker-completion order;
- `parent -> child` meaning loading/dependency order, with a canonical derived root set, multiple roots, and disconnected acyclic components;
- duplicate nodes or edges, unknown endpoints, self-edges, cycles, unknown Target Profile applicability, and graph revision or digest mismatch failing 020 admission without a partial Requirement Plan or Message Bundle Plan;
- every applicable message-reference occurrence binding to exactly one existing unit, with missing and multiple bindings rejected rather than moved to a root or nearest unit;
- route, feature, module, framework, and package labels remaining non-semantic evidence and producing identical graph semantics when only those labels change;
- eager/lazy relationships remaining physical 024 output and shared placement remaining a computed result rather than an authored unit kind;
- CLI, editor, final-binary, and honest whole-program integrations explicitly supplying the one-node `["main"]` graph instead of obtaining it from the profile;
- a post-link scan remaining single-unit while a build integration with pre-link evidence may supply finer logical units;
- a logical graph change invalidating affected plans, target outputs, and Releases without changing an otherwise equal profile;
- physical path, filename, content-hash, URL, loader-ID, or registration changes invalidating only applicable export or Release dependencies when logical graph semantics are unchanged;
- exact and first-over graph-artifact, node, edge, ID-size, target-applicability, reference-binding, allocation, validation-work, and placement-work limits without truncation, implicit partitioning, or a valid-prefix result;
- no consumer rereading host state or a previous cache entry to invent topology after graph admission;
- duplicate, invalid, missing, stale, unsupported, and cross-target-incompatible inputs;
- deterministic ordering under permuted JSON object member order; and
- exact and first-over resource-limit cases.

Checked-in expectations are reviewable normative test evidence rather than output regenerated and accepted automatically from the implementation under test. Generated fixtures retain their pinned upstream provenance and generator inputs. Updating an implementation may regenerate candidate files for review, but CI accepts them only when the case manifest, traceability report, and applicable design or specification decision remain consistent.

## Implementation Phasing

The accepted semantic decisions establish the following implementation dependency order:

1. establish `intlify_config`, extract the reusable existing configuration behavior, and define the version-`"0"` `IntlifyConfig` project-profile schema;
2. define the Locale Canonicalization Specification, provider boundary, data-artifact admission requirements, and a data-free `intlify_config` integration, then evaluate the ICU4X reference adapter against conformance fixtures;
3. define profile scope, named-profile selection, identity, locale-policy inputs, typed external-policy references and artifact-set admission, delivery-specification and placement-policy resolution, and deterministic semantic resolution;
4. exact Target Profile partitioning into Deployment Compatibility Groups, compiler-transaction group-selector admission, hydration-relation validation, and the graph-applicability handoff to 020;
5. dependency-aware Finding collection, checked/blocked outcomes, evaluation status, reporting limits, profile equality, re-resolution staleness, reproducibility closure, and Conformance Suite revision-`"0"` case manifest, paired-entry harness, golden expectations, and traceability report; and
6. file loader, optional programmatic frontend, canonicalization-data product integration, host Delivery Unit Graph handoff, and downstream-consumer evidence.

Apart from the accepted internal Rust crate name `intlify_config`, these checkpoints do not reserve package names, commands, or public APIs.

## Decision Log

Resolved decisions will be recorded here as the design proceeds.

| ID | Decision | Status | Rationale | Affected sections |
| --- | --- | --- | --- | --- |
| 015-001 | Use `intlify.config.json` as the primary and only normative repository format for project-profile configuration | Accepted | A repository-scoped declarative input is sufficient across target platforms and avoids platform-specific configuration DSLs and resolvers | Purpose; Goals; Canonical Configuration Input and Resolution; Conformance and Fixtures |
| 015-002 | Keep `IntlifyConfig` and `LocalizationProjectProfile` as separate models | Accepted | The authoring model may omit defaults or contain unnormalized values, while compiler consumers require a complete, checked settings IR | Design Overview; LocalizationProjectProfile Semantic Model; Deterministic Resolution Algorithm |
| 015-003 | Allow optional programmatic frontends only as equivalent constructors of JSON-compatible `IntlifyConfig` | Accepted | Embedded and typed use cases remain possible without creating alternate semantics or bypassing the shared resolver | Purpose; Canonical Configuration Input and Resolution; Dependency, Invalidation, and Reproducibility |
| 015-004 | Use JSON Schema for structural admission and the shared resolver for semantic validation | Accepted | Cross-field locale, target, policy, and default invariants cannot be delegated to structural validation alone | Ownership and Dependencies; Canonical Configuration Input and Resolution; Findings and Failure Model |
| 015-005 | Define one profile as one final-application localization project with exactly one Selection Scope and coherent project-wide locale and policy authority | Accepted | Repository, package, target, and release boundaries do not reliably identify the unit that owns final-application localization decisions | Goals; Profile Scope and Identity; Consumer Semantics |
| 015-006 | Allow one root configuration to declare one or more named profiles while requiring each resolver invocation to select exactly one without inheritance or implicit merging | Accepted | Monorepositories need several independent application profiles without turning repository layout into semantics or producing composite checked profiles | Purpose; Design Overview; Canonical Configuration Input and Resolution; Deterministic Resolution Algorithm; Conformance and Fixtures |
| 015-007 | Implement the reusable configuration and profile-resolution core in a dedicated `intlify_config` crate, using the existing `intlify_cli` configuration behavior as the extraction and migration baseline | Accepted | The checked profile and its resolver must be reusable by CLI, compiler, embedded, and future binding frontends without depending on CLI workflow concerns | Purpose; Goals; Ownership and Dependencies; Canonical Configuration Input and Resolution; Implementation Phasing |
| 015-008 | Start the independent configuration schema-version domain at the string value `"0"`; keep `$schema` editor-only and do not share the CLI reporter version constant | Accepted | The configuration specification is pre-stable and can follow the existing reporter convention while retaining independent evolution and admission semantics | Terminology to Refine; Canonical Configuration Input and Resolution; Deterministic Resolution Algorithm; Conformance and Fixtures |
| 015-009 | Reject unknown members in every fixed version-`"0"` configuration object and add extensibility only through explicit versioned schema sections | Accepted | Strict admission catches author and agent mistakes, prevents silent semantic loss, and keeps CLI and programmatic resolution deterministic | Canonical Configuration Input and Resolution; Findings and Failure Model; Conformance and Fixtures |
| 015-010 | Preserve non-semantic configuration origin and location evidence for file, programmatic, and selector input in resolution Findings and source maps, while excluding it from `LocalizationProjectProfile` semantic identity and digest | Accepted | Humans, editors, and agents need actionable locations without making host-specific source positions part of portable checked configuration | Terminology to Refine; Canonical Configuration Input and Resolution; Findings and Failure Model; Dependency, Invalidation, and Reproducibility; Conformance and Fixtures |
| 015-011 | Use valid Unicode BCP 47 Locale Identifiers as defined by UTS #35 as the normative semantic locale domain and require explicit adapters for legacy, opaque, or platform-specific identifiers | Accepted | One standard locale namespace can express language, script, region, and Unicode locale preferences consistently across compiler, target, and execution integrations | Terminology to Refine; Locale Identity and Canonicalization; Findings and Failure Model; Conformance and Fixtures; Deferred Follow-Up Notes |
| 015-012 | Canonicalize admitted locale identifiers during resolution, use the canonical form for semantic identity, equality, and digests, retain the authoring spelling only as evidence, and do not add likely subtags | Accepted | Portable checked profiles need one deterministic identity while preserving meaningful distinctions such as `en` versus `en-US` | Locale Identity and Canonicalization; Dependency, Invalidation, and Reproducibility; Conformance and Fixtures |
| 015-013 | Let the Intlify toolchain supply one versioned Locale Canonicalization Specification, prohibit configuration or host APIs from selecting canonicalization semantics, and include its identity in profile equality and digests | Accepted | Compiler-owned immutable rules prevent locale identity from drifting with host libraries while keeping author configuration simple and reproducible | Terminology to Refine; Locale Identity and Canonicalization; Deterministic Resolution Algorithm; Findings and Failure Model; Dependency, Invalidation, and Reproducibility; Conformance and Fixtures |
| 015-014 | Keep `intlify_config` free of embedded CLDR-derived data, admit a separately versioned data artifact through a read-only provider boundary, and use ICU4X as the initial reference-implementation candidate subject to Intlify conformance | Accepted | Provider-driven data keeps reusable modules small, permits baked or dynamic physical delivery, and reuses ICU4X data-management work without making its current behavior normative | Purpose; Goals; Ownership and Dependencies; Terminology to Refine; Locale Identity and Canonicalization; Deterministic Resolution Algorithm; Findings and Failure Model; Dependency, Invalidation, and Reproducibility; Security and Credential Handling; Conformance and Fixtures; Implementation Phasing |
| 015-015 | Pin the initial reference adapter to ICU4X `2.2.0`, extended locale canonicalization through the ICU4X 2.2 serialized `BufferProvider` schema, default `compiled_data` disabled, and the four marker families required by that constructor | Accepted | Extended mode covers every admitted locale while an explicit provider keeps CLDR-derived data outside `intlify_config`; the physical pin makes the reference implementation reproducible without turning ICU4X details into profile semantics | Locale Identity and Canonicalization; Dependency, Invalidation, and Reproducibility; Conformance and Fixtures; Deferred Follow-Up Notes |
| 015-016 | Define Locale Canonicalization Specification revision `"0"` from the four selected logical marker payloads in `icu_locale_data` `2.2.0` plus minimal conformance data derived from CLDR `48.2.0`, identify that dataset with a full representation-independent SHA-256 semantic digest, and record the generated value in an artifact manifest or lockfile | Accepted | Pinning the minimal logical dataset and its content digest makes canonicalization reproducible without distributing all CLDR data or coupling semantic identity to baked/blob encoding; generated evidence avoids manually maintained digest text | Locale Identity and Canonicalization; Dependency, Invalidation, and Reproducibility; Security and Credential Handling; Conformance and Fixtures; Deferred Follow-Up Notes |
| 015-017 | Preserve the valid Unicode BCP 47 locale domain and wrap ICU4X with an Intlify-owned conformance layer that validates against pinned data, delegates conforming behavior, applies explicit deterministic overrides, and blocks admission for uncorrectable gaps | Accepted | ICU4X parsing alone establishes syntactic well-formedness and its canonicalizer documents missing mappings; a small versioned layer preserves strict portable semantics without embedding full CLDR data or treating physical-engine behavior as normative | Locale Identity and Canonicalization; Findings and Failure Model; Dependency, Invalidation, and Reproducibility; Conformance and Fixtures |
| 015-018 | Classify general UTS #35 compatibility forms, CLDR forms, POSIX forms, legacy ICU forms, and platform-specific identifiers as outside the direct shared-resolver domain, while permitting explicit pre-resolution adapters that produce one valid Unicode BCP 47 identifier and retain conversion evidence | Accepted | A precise BCP 47-compatible input boundary removes the ambiguity in “Unicode Locale Identifier,” matches the existing no-repair rule, and keeps compatibility conversion explicit without making ICU4X parser limitations normative | Terminology to Refine; Locale Identity and Canonicalization; Findings and Failure Model; Conformance and Fixtures; Deferred Follow-Up Notes |
| 015-019 | Classify primary language subtags longer than three characters as outside revision `"0"` because none is valid in the pinned CLDR `48.2.0` language data, and block ICU4X `2.2.0` for any future specification that admits one unless a conforming wrapper or engine upgrade handles it | Accepted | The initial adapter limitation does not affect the initial valid domain, but tying the classification to the pinned specification prevents a future data revision from silently turning that limitation into divergent behavior | Locale Identity and Canonicalization; Dependency, Invalidation, and Reproducibility; Conformance and Fixtures |
| 015-020 | Generate the valid-direct-input CLDR BCP 47 key/type alias mappings missing from ICU4X `2.2.0` as a deterministic Intlify correction set, flatten alias chains, restore canonical syntax and ordering, and require idempotent output | Accepted | Data-derived corrections cover `islamicc` and any equivalent pinned gap without handwritten exceptions, while keeping legacy-only aliases outside the direct domain and rejecting correction data that cannot reproduce one stable canonical identity | Locale Identity and Canonicalization; Dependency, Invalidation, and Reproducibility; Conformance and Fixtures |
| 015-021 | Treat ICU4X parsing as syntax admission only and use pinned Intlify validity data to reject well-formed but invalid language, script, region, variant, Unicode extension, and transformed-extension components before canonicalization | Accepted | Separating syntax from data-backed validity prevents engine over-acceptance from weakening the normative domain, while generated boundary fixtures distinguish user-input Findings from adapter conformance failures | Locale Identity and Canonicalization; Findings and Failure Model; Conformance and Fixtures |
| 015-022 | Generate the complete initial adapter conformance inventory from all four sets in the pinned CLDR `48.2.0` locale-canonicalization corpus, project representable cases into the hyphen-separated direct domain, retain explicit reasons for outside-domain cases, and require zero unexplained mismatches after the Intlify conformance layer | Accepted | A generated, digest-pinned corpus and machine-readable per-case registry discover unknown ICU4X differences without a handwritten list, while preserving the direct-domain boundary and making every exclusion, correction, and admission failure reviewable | Locale Identity and Canonicalization; Dependency, Invalidation, and Reproducibility; Conformance and Fixtures |
| 015-023 | Emit a non-blocking replacement Finding for each admitted non-canonical locale spelling, and reject exact duplicates or alias collisions within one locale uniqueness scope without first-wins or silent deduplication | Accepted | Canonical suggestions keep configuration readable without blocking valid input, while rejecting canonical-identity collisions prevents ambiguous per-locale authority and preserves all conflicting source evidence | Locale Identity and Canonicalization; Deterministic Resolution Algorithm; Findings and Failure Model; Conformance and Fixtures |
| 015-024 | Order every semantic locale set by ascending unsigned UTF-8 bytes of canonical identifiers after duplicate detection, treat authoring order as non-semantic, and preserve order only for fields explicitly specified as ordered locale sequences | Accepted | Canonical ASCII byte ordering is simple and host-independent, while separating sets from fallback or negotiation sequences prevents deterministic serialization from erasing policy semantics | Locale Identity and Canonicalization; Requested Locale Set; Deterministic Resolution Algorithm; Dependency, Invalidation, and Reproducibility; Conformance and Fixtures |
| 015-025 | In revision `"0"`, admit CLDR `regular`, `special`, `macroregion`, `unknown`, and deterministically replaceable `deprecated` components plus valid registered `u` and `t` extensions; reject `reserved`, `private_use`, opaque `-x-`, and non-`u`/`t` extensions without stripping them | Accepted | The closed admitted set preserves publicly specified cross-platform locale semantics, keeps CLDR pseudo and unknown identifiers usable, and prevents private agreements or pass-through syntax from silently affecting shared compiler behavior | Locale Identity and Canonicalization; Deterministic Resolution Algorithm; Findings and Failure Model; Conformance and Fixtures |
| 015-026 | Make `defaultSourceLocale` optional, represent omission as an explicit checked profile state, inherit a present default only for application-owned Intents that omit their source locale, and block such an Intent during source authoring when no default exists | Accepted | Profile resolution precedes source discovery, so configuration omission is valid by itself; explicit absence preserves a complete settings IR without guessing a locale, while the later Intent-level check still guarantees exactly one source locale and never reinterprets library Intents | Source Locale Defaults; Deterministic Resolution Algorithm; Findings and Failure Model; Dependency, Invalidation, and Reproducibility; Consumer Semantics; Conformance and Fixtures |
| 015-027 | Require `requestedLocales` to enumerate a finite non-empty set with no dynamic expansion, define no product-wide fixed maximum, and enforce canonical set cardinality against a positive finite maximum from an admitted versioned resource-limit policy without truncation | Accepted | One explicit locale supports source-only applications and keeps requirement planning finite, while policy-bound maxima protect resolution without imposing an arbitrary global ceiling on large projects or relying on host-dependent defaults | Requested Locale Set; Deterministic Resolution Algorithm; Findings and Failure Model; Dependency, Invalidation, and Reproducibility; Security and Credential Handling; Conformance and Fixtures |
| 015-028 | Require a canonical project `defaultRequestedLocale`, resolve each Target Profile's effective default by explicit-override-first precedence, and block any project default, target subset, override, or inherited result that violates project or target membership | Accepted | Explicit project authority and one simple precedence rule avoid order- and negotiation-based inference while allowing independently released targets to choose different defaults without coupling source locale or message fallback | Requested-Locale Default Resolution; Deterministic Resolution Algorithm; Findings and Failure Model; Dependency, Invalidation, and Reproducibility; Consumer Semantics; Conformance and Fixtures |
| 015-029 | Define Locale Negotiation Profile revision `"0"` as bounded portable deterministic lookup over a normalized ordered preference sequence, target-supported locale subset, effective default, and optional direct project aliases, while keeping raw preference acquisition, best-fit matching, and message locale fallback outside it | Accepted | A small versioned input specification produces one portable supported locale without host-dependent best-fit data, preserves explicit application priority and project aliases, and keeps dynamic request state out of the checked project profile | Terminology to Refine; Locale Negotiation Policy Inputs; Deterministic Resolution Algorithm; Findings and Failure Model; Dependency, Invalidation, and Reproducibility; Security and Credential Handling; Consumer Semantics; Conformance and Fixtures |
| 015-030 | Define message locale fallback revision `"0"` as a bounded project-wide map from requested locales to complete ordered, non-recursive literal-definition or Intent-source candidate sequences; keep direct demand and coverage permission separate and leave eligible-artifact selection and materialization to 020 | Accepted | Explicit complete chains preserve deterministic 014 behavior while source-aware candidates support application and library Intents with different source locales, definition locales need not become requested outputs, and no runtime or host fallback can invent another selection | Terminology to Refine; Message Locale Fallback Policy Inputs; Deterministic Resolution Algorithm; Findings and Failure Model; Dependency, Invalidation, and Reproducibility; Security and Credential Handling; Consumer Semantics; Conformance and Fixtures |
| 015-031 | Resolve coverage revision `"0"` into a bounded project-wide requested-locale × checked-Intent-surface decision table with `direct-required` and `fallback-allowed` modes, a safe default of `direct-required`, specificity-based order-independent overrides, and separately derived source-equal fulfillment | Accepted | A finite checked table gives planners and the Linker one explainable permission per requirement, prevents target or delivery packaging from weakening localization quality, preserves direct Provider demand and visible debt under fallback, and rejects ambiguous overlapping authority instead of using source order | Terminology to Refine; Coverage Policy Inputs; Deterministic Resolution Algorithm; Findings and Failure Model; Dependency, Invalidation, and Reproducibility; Security and Credential Handling; Consumer Semantics; Conformance and Fixtures |
| 015-032 | Represent externally owned Provider, governance, Glossary, trust, and resource policy inputs as typed immutable references containing policy kind, opaque identity, exact revision, policy-specification revision, and semantic content digest; resolve them only from one explicit finite already acquired artifact set; require resource-limit, trust/source-admission, and approval/selection policies; and model Provider-routing and Glossary Set input as explicit present-or-absent states | Accepted | Exact typed pins make profile resolution reproducible and detect replaced content, required safe policies prevent omission from becoming permissive behavior, optional Provider/Glossary absence keeps existing-Store builds valid, and separating acquisition and credentials prevents hidden network or secret-bearing compiler behavior | Terminology to Refine; LocalizationProjectProfile Semantic Model; Provider, Governance, and Glossary References; Deterministic Resolution Algorithm; Findings and Failure Model; Dependency, Invalidation, and Reproducibility; Security and Credential Handling; Consumer Semantics; Conformance and Fixtures |
| 015-033 | Keep Delivery Graph Specification and Delivery Placement Policy in `LocalizationProjectProfile`, admit only `duplicate` placement in revision `"0"`, and supply the realized immutable logical Delivery Unit Graph as a separate host-build compiler-transaction input while leaving physical paths, chunks, resources, and loader relationships to target export | Accepted | Stable profile policy can be reused across targets and dev/production builds, while an exact host graph reflects real code splitting and native granularity without duplicating stale topology in configuration; logical graph changes invalidate plans without changing profile identity, and physical output changes remain exporter or Release concerns | Terminology to Refine; Goals; Ownership and Dependencies; LocalizationProjectProfile Semantic Model; Delivery Policy and Topology Inputs; Deterministic Resolution Algorithm; Findings and Failure Model; Dependency, Invalidation, and Reproducibility; Security and Credential Handling; Consumer Semantics; Conformance and Fixtures |
| 015-034 | Partition every Target Profile into exactly one non-empty Deployment Compatibility Group, select exactly one complete group per compiler transaction, require graph applicability to partition its selected targets, and model Browser/SSR hydration as explicit finite directed relations that preserve locale selection, selected definitions, logical render equivalence, and same-Release coupled execution | Accepted | Exact membership gives every target one unambiguous Requirement Plan and Release authority; independent groups retain independent publication and rollback; explicit hydration edges permit fan-in/out without platform-name inference; strict revision-`"0"` locale and negotiation equality prevents divergent initial locale selection; and capability-based render evidence preserves cross-platform physical implementation freedom without allowing mixed definitions or Releases | Terminology to Refine; Ownership and Dependencies; Target Profiles and Deployment Compatibility Groups; Delivery Policy and Topology Inputs; Deterministic Resolution Algorithm; Findings and Failure Model; Dependency, Invalidation, and Reproducibility; Security and Credential Handling; Consumer Semantics; Conformance and Fixtures |
| 015-035 | Use dependency-aware fail-complete Finding collection: evaluate every check whose prerequisites are admitted, suppress only checks that depend on invalid or unavailable results, represent suppression as deterministic not-evaluated status with causal Finding identities, return no partial profile on any blocking Finding, and fail explicitly when bounded diagnostic reporting becomes incomplete | Accepted | This reports independent problems in one run without inventing cascade errors, preserves safe phase gates when typed semantics or execution bounds are unavailable, makes parallel implementations reproducible, and prevents a truncated diagnostic prefix from masquerading as complete validation | Terminology to Refine; Ownership and Dependencies; Canonical Configuration Input and Resolution; Deterministic Resolution Algorithm; Findings and Failure Model; Dependency, Invalidation, and Reproducibility; Security and Credential Handling; Consumer Semantics; Conformance and Fixtures; Implementation Phasing |
| 015-036 | Define profile semantic equality by Profile Specification revision `"0"` plus field-for-field canonical semantic projection; track re-resolution staleness separately from semantic change; and define a finite immutable reproducibility input closure containing materialized configuration, selector, admitted specifications and data, referenced semantic artifacts, and applicable capability admission | Accepted | Separating these identities lets source or admission changes trigger safe re-resolution without forcing downstream invalidation when semantics remain equal, excludes authoring and physical implementation details from the portable profile, and gives conforming implementations an exact closure from which to reproduce the same outcome | Terminology to Refine; Profile Scope and Identity; Canonical Configuration Input and Resolution; LocalizationProjectProfile Semantic Model; Deterministic Resolution Algorithm; Findings and Failure Model; Dependency, Invalidation, and Reproducibility; Consumer Semantics; Conformance and Fixtures; Implementation Phasing |
| 015-037 | Define Project Profile Resolver Conformance Suite revision `"0"` as a machine-readable case-manifest suite with exact checked/blocked expectations, paired file/programmatic execution for every representable semantic value, explicit adapter-only exceptions, decision-to-verification traceability, boundary/equivalence/mutation/suppression coverage, pinned generated corpora, and review-gated golden changes | Accepted | A single versioned suite proves shared resolver semantics across entry paths and implementations without freezing a public wire format, distinguishes legitimate evidence differences from semantic divergence, makes coverage gaps visible, and prevents implementation output from silently redefining the specification | Terminology to Refine; Ownership and Dependencies; Conformance and Fixtures; Implementation Phasing |

## Deferred Follow-Up Notes

The following remain in their owning designs unless a concrete 015 semantic dependency requires a narrower interface here:

- repository-root discovery, workspace profile selection, commands, generated-schema publication paths, optional helper API UX, compatibility handling for existing unversioned configuration, and packaging: 029;
- alignment of 000's broader illustrative host-configuration wording with the canonical JSON decision recorded here: 000;
- compatibility disposition for the existing `intlify.config.jsonc` discovery described by 006; if retained, it must materialize the same `IntlifyConfig` and remains non-primary: 029 and the compatibility specification;
- explicit migration from 014's opaque, exact-byte locale values into the normative Unicode BCP 47 Locale Identifier domain: 017, 020, and 029;
- source authoring and Intent source-locale evidence: 016;
- artifact encoding, digest framing, version migration, capability admission, Locale Canonicalization Specification identity encoding, provider-readable canonicalization-data-artifact admission, external-policy reference and artifact representation, and Delivery Unit Graph artifact representation: 017;
- trust roots, actor powers, credentials, signatures, and provenance: 018;
- common Finding envelope, graph queries, cache implementation, and incremental scheduling: 019;
- selected-group admission, exact Delivery Unit Graph applicability, initial-render closure, requirement planning, fallback selection, reachability, placement, and pruning: 020;
- Store, governance, Provider, TMS, and synchronization workflows: 021 and 022;
- locale-service execution semantics, portable values, and any runtime-facing dynamic canonicalization-data requirement: 023;
- exact Target Profile capability and Browser/SSR hydration-role admission, physical delivery mapping, loader relationships, and output schemas: 024;
- independent group Release assembly, same-Release hydration coupling, publication, deployment, and execution admission: 025;
- cross-target logical-render equivalence and conformance evidence: 026;
- ICU4X adapter packaging and dependency-lock realization of the version pin: implementation planning;
- any reference Runtime data-provider realization: 027;
- toolchain/lockfile pinning plus acquisition, installation, caching, and offline workflow for the Locale Canonicalization Specification, its Data Artifact, and external-policy artifacts: 029; and
- Vue/SSR tooling integration and hydration projection: 030.

Only decisions marked Accepted in the Decision Log are fixed here. The internal Rust crate name `intlify_config` is accepted, but it does not by itself reserve a public package or binding API. No unaccepted field, helper name, command, wire tag, or additional format is reserved merely by appearing as a candidate in this scaffold.

## Relationship to Other Documents

| Document | Relationship |
| --- | --- |
| [000 — Intlify overview](./000-intlify-overview-design.md) | Defines the product-wide architecture, terminology, inherited locale invariants, Roadmap, and Expected Outcomes refined here. Its broader illustrative configuration-format wording requires alignment with the canonical JSON decision recorded by 015. |
| [006 — Tooling foundation](./006-ox-mf2-phase-3a-tooling-foundation-design.md) | Provides the existing CLI-owned parser, duplicate-member rejection, strict validation, JSON Schema generation, freshness checks, and configuration Findings used as the `intlify_config` extraction baseline. 015 makes strict JSON primary and leaves JSONC compatibility to follow-up product design. |
| [014 — Message linker](./014-ox-mf2-message-linker-design.md) | Provides current locale, fallback, delivery, and resolved-policy implementation experience; 020 owns the source-first linker evolution that consumes this profile. |
| [016 — Source authoring and Intent identity](./016-intlify-source-authoring-and-intent-identity-design.md) | Owns Intent source-locale declarations and evidence that use the project default defined here only when omitted. |
| [017 — Shared artifacts and version admission](./017-intlify-shared-artifact-and-version-admission-design.md) | Owns canonical encoding and digest framing for the profile semantic projection plus shared identities, version admission, migration, provider-readable Locale Canonicalization Data Artifact admission, external-policy reference and artifact representation, and Delivery Unit Graph artifact representation for the semantic model defined here. |
| [018 — Security, trust, and provenance](./018-intlify-security-trust-and-provenance-design.md) | Owns trust and credential specifications referenced, but not embedded as secrets, by the profile. |
| [019 — Project graph, query, and incremental processing](./019-intlify-project-graph-query-and-incremental-design.md) | Owns re-resolution dependency tracking, staleness scheduling, semantic dependency slicing, cache and downstream reuse decisions, common Finding and evaluation-status projection, query, and incremental processing over profile inputs. |
| [020 — Requirement planning and linking](./020-intlify-requirement-planning-and-linking-design.md) | Selects exactly one complete Deployment Compatibility Group per compiler transaction; consumes resolved locale, coverage, fallback, target-applicability, and profile delivery-policy inputs; admits an exact graph-applicability partition for the selected targets; and owns initial-render closure, reachability, placement, and pruning. |
| [021 — Translation Store and governance](./021-intlify-translation-store-and-governance-design.md) | Consumes Selection Scope and governance-policy references. |
| [022 — Provider and localization sync](./022-intlify-provider-and-localization-sync-design.md) | Consumes Provider-routing, Glossary Set, refresh, and locale-demand inputs. |
| [023 — Localization execution specification](./023-intlify-localization-execution-specification-design.md) | Consumes locale-negotiation, locale-service, and scoped-locale semantics and owns any runtime-facing dynamic canonicalization-data requirement. |
| [024 — Target Profile and export](./024-intlify-target-profile-and-export-design.md) | Owns exact Target Profile capabilities, Browser/SSR hydration-role admission, and the physical paths, resources, loader relationships, and output semantics produced from selected logical placement. |
| [025 — Release Assembly and deployment](./025-intlify-release-assembly-and-deployment-design.md) | Owns independent Release authority for each Deployment Compatibility Group, same-Release hydration coupling, publication, deployment, activation, rollback, and execution admission. |
| [026 — Conformance and measurement](./026-intlify-conformance-and-measurement-design.md) | Owns cross-target logical-render equivalence and the capability and conformance evidence required by explicit hydration relations. |
| [027 — Reference Runtime](./027-intlify-reference-runtime-design.md) | Implements one physical execution path that consumes the effective requested-locale and negotiation inputs defined here through 023–025 and owns any reference Runtime realization of a canonicalization data provider. |
| [029 — Product workflow and packaging](./029-intlify-product-workflow-and-packaging-design.md) | Owns `intlify.config.json` discovery, workspace selection, the user-facing mechanism that supplies one Profile ID selector, generated-schema publication and re-export, legacy unversioned-config compatibility, optional programmatic helper UX, canonicalization-data and external-policy-artifact acquisition and caching, commands, packaging, offline behavior, and workflow without introducing alternate configuration or profile-selection semantics. |
| [030 — Vue and SSR tooling integration](./030-intlify-vue-ssr-tooling-integration-design.md) | Owns Vue/SSR tooling integration and the projection of explicit hydration relations into framework-specific build and execution behavior. |
