# Intlify Project Profile and Locale Policy Design

## Purpose

This design defines how one canonical project configuration becomes the complete, checked `LocalizationProjectProfile` consumed by shared Intlify compiler stages. The primary repository input is `intlify.config.json`, described by a versioned JSON Schema. An optional programmatic frontend, such as a future `defineIntlifyConfig()`, may construct the same JSON-compatible `IntlifyConfig` data model, but it does not create a second configuration language or bypass validation.

In practical terms, the profile gives every downstream stage the same answers to four questions:

- which localization project and Selection Scope are being processed;
- which source and requested locales apply, including project defaults, target subsets, and effective defaults;
- which Target Profiles, Deployment Compatibility Groups, and delivery inputs belong to the selected build; and
- which versioned negotiation, fallback, coverage, Provider, governance, trust, and resource policies apply.

![High-level role of the Intlify Localization Project Profile](./assets/015-intlify-project-profile-and-locale-policy-overview.svg)

The following example shows the file-first path and the optional programmatic path converging before shared compilation. The programmatic API name is illustrative; its input semantics are not separate from `intlify.config.json`.

![Canonical configuration resolution into one Localization Project Profile before cross-platform compilation](./assets/015-intlify-cross-platform-project-profile-resolution.svg)

The shared resolver owns JSON-compatible input admission, semantic validation, normalization, and default resolution. Planning, synchronization, linking, export, Release Assembly, tooling, and execution integrations consume the resulting settings IR instead of rereading configuration or inventing their own defaults. Cross-platform Producers, Lowering Backends, Target Exporters, and Runtime integrations begin downstream of this common configuration boundary. Credentials and other secrets remain outside the profile.

## Goals

- Define what one resolved `LocalizationProjectProfile` represents and how it is identified.
- Define the semantic split between author-facing `IntlifyConfig` and the checked `LocalizationProjectProfile` settings IR.
- Define `intlify.config.json` and its versioned JSON Schema as the primary repository configuration surface.
- Require file-based and optional programmatic inputs to enter the same resolver with the same semantics.
- Define project requested locales, source-locale defaults, requested-locale defaults, Target Profile subsets, target overrides, and effective defaults.
- Keep requested-locale negotiation separate from message locale fallback and single-message evaluation.
- Define the profile inputs for coverage, Provider routing, approval, Glossary Sets, delivery, trust, and resource policies without taking ownership from their detailed designs.
- Define how Target Profiles form one or more Deployment Compatibility Groups.
- Define deterministic resolution, validation, Finding production, and consumer-visible dependency inputs.
- Make invalid, ambiguous, incomplete, or incompatible configuration fail before synchronization, linking, export, or production execution.
- Provide paired `IntlifyConfig`, resolved-profile, and Finding fixtures that the shared resolver and downstream consumers can use.

## Non-Goals

- Defining TOML, YAML, framework-specific, or platform-specific configuration formats equivalent to `intlify.config.json`.
- Freezing the name, package, or language binding of an optional programmatic configuration helper such as `defineIntlifyConfig()`; those product-facing details belong to [029](./029-intlify-product-workflow-and-packaging-design.md).
- Defining repository-root discovery, workspace profile selection, command-line option precedence, or configuration UX owned by [029](./029-intlify-product-workflow-and-packaging-design.md).
- Defining formatter, linter, or other unrelated tool-specific sections that may coexist in the root Intlify configuration schema.
- Defining source authoring, `intent()`, `mf2`, Intent identity, or source-evidence rules owned by [016](./016-intlify-source-authoring-and-intent-identity-design.md).
- Defining the complete shared-artifact wire encoding, canonical digest framing, specification-version admission, or migration mechanism owned by [017](./017-intlify-shared-artifact-and-version-admission-design.md).
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
| JSON Schema validation | Validates the structural shape of the 015-owned project-profile input before semantic resolution |
| Optional programmatic frontend | Constructs the same `IntlifyConfig` value for embedded use; exact API naming, packaging, and language bindings remain 029-owned |
| 015 project-profile resolver | Semantically validates, normalizes, and resolves one complete `LocalizationProjectProfile` according to this specification |
| 016 source authoring | Supplies Intent source-locale declarations and uses the resolved default only when source authoring omits one |
| 017 shared artifacts | Defines shared encodings, version admission, canonical identities, and migration for the resolved model |
| 018 trust and provenance | Defines trust inputs, delegation, credentials, signatures, and authorization referenced by the profile |
| 019 project graph and queries | Tracks profile dependencies and projects profile Findings and explanations to clients |
| 020 planning and linking | Consumes locale, coverage, fallback, target-applicability, and delivery inputs from the profile |
| 021 Store and governance | Consumes Selection Scope and governance-policy references without redefining locale policy |
| 022 synchronization | Consumes Provider-routing, Glossary Set, refresh, and applicable locale-demand inputs |
| 023 localization execution | Consumes locale-negotiation, locale-service, and scoped-locale semantics |
| 024 target export | Owns Target Profile capability and output details referenced by this design |
| 025 Release and deployment | Owns Release behavior for the Deployment Compatibility Groups declared here |
| 029 product workflow | Owns file discovery, workspace selection, commands, schema packaging, optional helper API UX, and product packaging without defining alternate configuration semantics |

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
| `intlify.config.json` | Primary repository configuration document whose project-profile fields enter the shared resolver |
| Programmatic configuration frontend | Optional typed or embedded API that constructs `IntlifyConfig` without introducing different semantics |
| Localization Project Profile | Checked project-configuration IR, including exact project scope, identity, required sections, and completeness rules |
| Project requested-locale set | Membership, ordering, canonicalization, duplicate handling, and empty-set rules |
| Default source locale | Defaulting conditions and interaction with application and library Intent sources |
| Default requested locale | Project-level negotiation default and validation rules |
| Effective default requested locale | Exact target override-resolution and error behavior |
| Target Profile reference | Identity, membership, locale subset, and cross-document admission inputs |
| Deployment Compatibility Group | Membership, uniqueness, coupling declarations, and profile-level validation |
| Locale Negotiation Profile | Versioned policy inputs exposed by the project profile |
| Message locale fallback policy | Versioned Linker inputs, applicability, and relation to coverage |
| Coverage policy | Direct-required, fallback-allowed, source-equal, and debt-reporting inputs |
| Delivery policy | Delivery topology and Delivery Unit inputs visible to planning and export |
| Selection Scope | Governance namespace selected by the project profile without inferring target semantics |

## Design Overview

The author-facing configuration and the compiler-facing settings IR are distinct models:

```text
intlify.config.json -----------------------+
                                            +-> JSON-compatible IntlifyConfig
optional programmatic frontend ------------+      -> JSON Schema validation
                                                   -> semantic resolution
                                                   -> LocalizationProjectProfile
```

The resolved profile has four conceptual groups. Their exact representation remains to be designed.

```text
LocalizationProjectProfile
  + project and Selection Scope identity
  + locale model and locale-policy inputs
  + Target Profile and Deployment Compatibility Group declarations
  + versioned references to delivery, Provider, governance, trust, and resource policies
```

The resolver must discard authoring conveniences that have no semantic meaning while preserving enough file, JSON Pointer, or programmatic-call evidence for actionable Findings.

## Profile Scope and Identity

This section will define:

- what one profile represents: final application, deployable product, workspace member, or another exact unit;
- whether a workspace resolves several independent profiles or one composite profile;
- project identity, Selection Scope association, and profile revision inputs;
- which identities are opaque and which carry semantic meaning;
- whether profile composition is allowed and, if so, which layer owns it; and
- completeness requirements before a profile can be consumed.

## Canonical Configuration Input and Resolution

### Primary repository input

`intlify.config.json` is the primary and only normative repository configuration format for the project-profile input defined here. The exact repository-root discovery, workspace selection, and command UX remain owned by 029. Intlify does not require platform-specific configuration DSLs for Web, Apple, Android, JVM, native, or system targets; cross-platform behavior is expressed by Target Profiles and downstream integrations after profile resolution.

An external tool may generate `intlify.config.json`, but TOML, YAML, executable framework configuration, and platform-native objects are not additional configuration semantics recognized by the shared resolver.

### `IntlifyConfig` and JSON Schema

`IntlifyConfig` is the unchecked, JSON-compatible authoring model. The 015-owned project-profile fields are described by a versioned JSON Schema so files, editors, CLI tooling, and optional APIs share one structural definition. Root-schema composition and package publication remain coordinated with 029 and existing tooling specifications.

JSON Schema validation admits structural shape, primitive types, required fields, and closed or versioned field sets. The semantic resolver remains responsible for locale canonicalization, cross-field membership, reference admission, default resolution, Target Profile subsets, Deployment Compatibility Groups, and deterministic Findings. Schema success alone never creates a `LocalizationProjectProfile`.

### Optional programmatic frontend

An embedding API may accept or construct the same `IntlifyConfig` value without first writing a file. A helper provisionally illustrated as `defineIntlifyConfig()` may provide static typing and editor completion, but its result remains unchecked input to the shared resolver.

The programmatic path must satisfy these invariants:

- it produces only JSON-compatible data covered by the same schema;
- it cannot carry functions, class instances, platform handles, credentials, or hidden process state into the profile;
- it cannot directly construct or assert a checked `LocalizationProjectProfile`;
- it runs the same semantic resolver and produces the same Findings as equivalent file input; and
- reproducibility depends on the materialized `IntlifyConfig` value and admitted references, not host-language object identity.

The exact helper name, language bindings, and embedding ergonomics belong to 029.

### Resolved output

`LocalizationProjectProfile` is a complete, checked settings IR and the only configuration model consumed by shared compiler stages. This subsection will define the success and failure boundary of profile resolution, including whether warnings can accompany a usable profile and which failures prevent any partial output.

## LocalizationProjectProfile Semantic Model

This section will define the required and optional semantic groups of the resolved profile without prematurely freezing a Rust struct or wire encoding.

Candidate semantic groups to evaluate are:

- project and Selection Scope identity;
- project locale sets and defaults;
- locale-negotiation and message-locale-fallback policy references;
- coverage and source-admission policy references;
- Provider-routing, approval, and Glossary Set references;
- Target Profile references and Deployment Compatibility Groups;
- delivery topology and placement-policy inputs;
- trust, integrity, and resource-limit references; and
- specification-version and capability-admission inputs shared with 017.

## Locale Identity and Canonicalization

This section will define:

- the accepted locale-identifier domain;
- validation and canonicalization rules;
- equality and deterministic ordering;
- duplicate and alias handling;
- treatment of extensions, private-use subtags, and implementation-specific locale identifiers;
- whether canonicalization changes identity or only presentation; and
- the file and JSON Pointer or programmatic-call evidence retained when an authoring spelling is normalized or rejected.

## Source Locale Defaults

This section will define:

- the project default source locale;
- when an Intent inherits that default;
- how an explicit application Intent source locale overrides it;
- how library Intent source locales remain authoritative during composition;
- whether omission is valid when every Intent declares a source locale; and
- Findings for absent, invalid, or contradictory source-locale input.

## Requested Locale Set

This section will define:

- the project requested-locale set and its minimum cardinality;
- deterministic locale ordering;
- Target Profile subset validation;
- whether independently released groups may select different target subsets;
- how locale-set changes affect downstream planning and synchronization; and
- which exclusions are intentional rather than missing-localization debt.

## Requested-Locale Default Resolution

This section will specify the algorithm that resolves:

```text
project default requested locale
  + optional Target Profile override
  + target supported requested-locale subset
  -> exactly one effective default requested locale
```

It will define precedence, membership checks, incompatible defaults, omission behavior, and deterministic Findings.

## Locale Negotiation Policy Inputs

This section will define the versioned inputs needed to choose one supported requested locale from application-supplied preferences while leaving application preference acquisition and single-message formatting outside this design.

Topics include:

- direct requested-locale selection versus negotiation;
- fallback to the effective default requested locale;
- matching rule identity and revision;
- input preference shape expected by execution integrations;
- behavior when no preference matches;
- cross-target negotiation compatibility; and
- hydration-coupled server and client requirements.

## Message Locale Fallback Policy Inputs

This section will define the profile inputs consumed by the Linker when the requested locale lacks an eligible definition.

It will keep the following distinction explicit:

```text
locale negotiation
  -> selects one requested locale for the user or operation

message locale fallback
  -> selects one admitted definition locale for one required message
```

Exact selection and materialization algorithms remain owned by 020.

## Coverage Policy Inputs

This section will define the profile-level inputs for:

- direct-required localization;
- fallback-allowed localization;
- source-equal fulfillment;
- visible coverage debt;
- source-admission requirements;
- policy applicability by locale, Intent surface, target, or delivery unit; and
- deterministic precedence when several policy rules could apply.

## Provider, Governance, and Glossary References

This section will define how the resolved profile names versioned Provider-routing, refresh, approval, selection, source-admission, and Glossary Set inputs without embedding credentials or taking ownership of their schemas.

The detailed Provider, Store, and governance workflows remain owned by 021 and 022.

## Delivery Policy and Topology Inputs

This section will define the language-neutral delivery facts needed before requirement planning and linking, including:

- declared Delivery Units and their identities;
- target applicability;
- group membership and delivery coupling;
- eager, lazy, shared, route, feature, or equivalent semantic categories if required;
- deterministic ordering and duplicate handling; and
- which topology details remain host-build inputs rather than project-profile data.

Placement and pruning algorithms remain owned by 020 and target output remains owned by 024.

## Target Profiles and Deployment Compatibility Groups

This section will define:

- how a profile references Target Profiles owned by 024;
- uniqueness and membership rules;
- whether every Target Profile must belong to exactly one selected group per compiler invocation;
- project-locale subset and effective-default validation per target;
- group-level cross-target requirements;
- hydration-coupled Browser and SSR declarations;
- independent Release boundaries for Web, mobile, native, or other groups; and
- Findings for missing, overlapping, incompatible, or empty groups.

The exact Target Profile capability schema belongs to 024. Release Assembly and deployment behavior belong to 025.

## Deterministic Resolution Algorithm

This section will define an ordered fail-complete resolution pipeline, expected to cover:

1. admit one materialized `IntlifyConfig` value and its file or programmatic source evidence;
2. validate the applicable JSON Schema version and reject non-JSON or structurally invalid input;
3. resolve project identity and Selection Scope;
4. validate and canonicalize locale identifiers;
5. resolve project locale sets and defaults;
6. admit policy references and revisions;
7. resolve Target Profile references and requested-locale subsets;
8. form and validate Deployment Compatibility Groups;
9. resolve each effective default requested locale;
10. validate cross-target locale and hydration requirements;
11. produce deterministic Findings or one complete resolved profile.

The final order, independent-error collection, and cascade-suppression rules remain open.

## Findings and Failure Model

This section will define component-owned Finding codes and evidence while using the common Finding envelope owned by 019.

The catalog will cover at least these families:

| Family | Candidate situations |
| --- | --- |
| Profile identity | Missing, duplicate, ambiguous, or incompatible project and Selection Scope input |
| Locale identity | Invalid, duplicate, non-canonical, unsupported, or incompatible locale input |
| Default resolution | Missing project default, invalid target override, or effective default outside the target subset |
| Policy reference | Missing, incompatible, stale, or unresolved versioned policy input |
| Target membership | Unknown Target Profile, duplicate membership, empty group, or locale-subset mismatch |
| Cross-target compatibility | Incompatible negotiation, effective default, locale-service, or hydration requirement |
| Delivery input | Unknown, duplicate, cyclic, incomplete, or target-inapplicable delivery declaration |
| Resource admission | Profile size, count, depth, or work limit exceeded |

For each Finding, the completed design must define stable code, severity, blocking behavior, primary evidence, related evidence, deterministic order, dependency cause, and suggested action where safe.

## Dependency, Invalidation, and Reproducibility

This section will define which exact identities and revisions make the resolved profile stale, including the materialized `IntlifyConfig`, schema version, locale sets, target membership, policy references, and configuration source evidence.

It will specify semantic equality and deterministic resolution inputs while leaving shared digest framing and cache implementation to 017 and 019. Two resolution executions over the same materialized `IntlifyConfig`, tool/specification versions, and referenced revisions must not disagree because of JSON member order, filesystem enumeration, concurrency, optional frontend implementation, or host-language object identity.

## Security and Credential Handling

The resolved profile may identify Provider, Store, trust, publication, or delivery policy by immutable reference, but it must not carry Provider/TMS secrets, reviewer credentials, publication signing keys, deployment credentials, or production request data into ordinary compiler or execution consumers.

This section will define the profile-specific credential exclusion and redaction requirements. The complete trust, provenance, authorization, and signature specification remains owned by 018.

## Consumer Semantics

This section will define the exact subset of profile facts each downstream stage may consume.

| Consumer | Profile facts to specify |
| --- | --- |
| Source producer | Default source locale and applicable source-policy references |
| Project graph and query service | Profile dependencies, source evidence, Findings, and explanations |
| Requirement planner | Requested locales, target applicability, coverage, policy, and delivery inputs |
| Synchronization | Provider-routing, refresh, Glossary Set, locale-demand, and policy references |
| Governance and Store | Selection Scope and applicable policy references |
| Message Linker | Requested locales, fallback, coverage, target, and delivery inputs |
| Target Exporter | Selected Target Profiles, locale subsets, and group membership |
| Release Assembly | One selected Deployment Compatibility Group and its compatibility declarations |
| Execution integration | Supported locales, effective default, and locale-negotiation profile reference |

The completed design must prevent consumers from silently applying their own defaults or reinterpreting unchecked `IntlifyConfig` input.

## Conformance and Fixtures

The fixture plan will include `intlify.config.json`, equivalent programmatic-value, resolved-profile, and Finding fixtures for at least:

- JSON Schema success and failure, including missing, unknown, incorrectly typed, and incompatible-version fields;
- byte-distinct JSON documents and programmatic values that materialize the same `IntlifyConfig` semantics;
- one Browser target;
- hydration-coupled Browser and SSR targets;
- independently released Web and mobile groups;
- per-target requested-locale subsets and default overrides;
- explicit and inherited source locales, including library Intents;
- direct-required and fallback-allowed coverage;
- locale negotiation distinct from message locale fallback;
- versioned Provider, governance, Glossary Set, delivery, trust, and resource references;
- duplicate, invalid, missing, stale, unsupported, and cross-target-incompatible inputs;
- deterministic ordering under permuted JSON object member order; and
- exact and first-over resource-limit cases.

The completed design will assign each fixture to the JSON Schema validator, semantic resolver, or downstream consumer. Optional programmatic frontends must demonstrate that an equivalent materialized `IntlifyConfig` produces the same profile or Findings as file input; platform-specific configuration resolvers are not a conformance extension point.

## Implementation Phasing

The implementation plan will be defined after the semantic decisions above are complete. The intended design checkpoints are:

1. `IntlifyConfig` project-profile schema, profile scope, identity, and inherited invariants;
2. JSON Schema admission, locale-policy inputs, and deterministic semantic resolution;
3. Target Profile and Deployment Compatibility Group validation;
4. Findings, limits, dependency identity, and paired configuration/profile fixtures; and
5. file loader, optional programmatic frontend, and downstream-consumer integration evidence.

These checkpoints do not reserve package names, commands, or public APIs.

## Decision Log

Resolved decisions will be recorded here as the design proceeds.

| ID | Decision | Status | Rationale | Affected sections |
| --- | --- | --- | --- | --- |
| 015-001 | Use `intlify.config.json` as the primary and only normative repository format for project-profile configuration | Accepted | A repository-scoped declarative input is sufficient across target platforms and avoids platform-specific configuration DSLs and resolvers | Purpose; Goals; Canonical Configuration Input and Resolution; Conformance and Fixtures |
| 015-002 | Keep `IntlifyConfig` and `LocalizationProjectProfile` as separate models | Accepted | The authoring model may omit defaults or contain unnormalized values, while compiler consumers require a complete, checked settings IR | Design Overview; LocalizationProjectProfile Semantic Model; Deterministic Resolution Algorithm |
| 015-003 | Allow optional programmatic frontends only as equivalent constructors of JSON-compatible `IntlifyConfig` | Accepted | Embedded and typed use cases remain possible without creating alternate semantics or bypassing the shared resolver | Purpose; Canonical Configuration Input and Resolution; Dependency, Invalidation, and Reproducibility |
| 015-004 | Use JSON Schema for structural admission and the shared resolver for semantic validation | Accepted | Cross-field locale, target, policy, and default invariants cannot be delegated to structural validation alone | Ownership and Dependencies; Canonical Configuration Input and Resolution; Findings and Failure Model |

## Deferred Follow-Up Notes

The following remain in their owning designs unless a concrete 015 semantic dependency requires a narrower interface here:

- repository-root discovery, workspace profile selection, commands, schema publication, optional helper API UX, and packaging: 029;
- alignment of 000's broader illustrative host-configuration wording with the canonical JSON decision recorded here: 000;
- compatibility disposition for the existing `intlify.config.jsonc` discovery described by 006; if retained, it must materialize the same `IntlifyConfig` and remains non-primary: 029 and the compatibility specification;
- source authoring and Intent source-locale evidence: 016;
- artifact encoding, digest framing, version migration, and capability admission: 017;
- trust roots, actor powers, credentials, signatures, and provenance: 018;
- common Finding envelope, graph queries, cache implementation, and incremental scheduling: 019;
- requirement planning, fallback selection, reachability, placement, and pruning: 020;
- Store, governance, Provider, TMS, and synchronization workflows: 021 and 022;
- locale-service execution semantics and portable values: 023;
- Target Profile capability and output schemas: 024;
- Release publication, deployment, and execution admission: 025.

Only the configuration boundary recorded in the Decision Log is fixed here. No unaccepted field, package, helper name, command, wire tag, or additional format is reserved merely by appearing as a candidate in this scaffold.

## Open Questions

The design will resolve these questions one at a time and move each accepted answer into its owning section and the Decision Log.

1. What exact unit does one `LocalizationProjectProfile` represent?
2. Does one `intlify.config.json` describe exactly one project profile, or can 029 select among named profiles in one repository document?
3. Which JSON Schema version-admission, unknown-field, and configuration source-evidence rules are normative?
4. What locale-identifier domain, canonicalization, equality, and ordering rules are normative?
5. When is the project default source locale required, and how is omission represented?
6. What are the minimum and maximum project requested-locale sets?
7. How are project defaults, Target Profile overrides, and effective defaults resolved?
8. What exact inputs constitute a Locale Negotiation Profile?
9. What profile inputs define message locale fallback without moving the Linker algorithm into 015?
10. How are coverage rules scoped and ordered?
11. How are Provider, approval, Glossary Set, trust, and resource policies referenced by revision?
12. Which delivery-topology facts belong in the profile rather than the host build graph?
13. How are Target Profiles assigned to Deployment Compatibility Groups, including hydration coupling?
14. Which failures are independently reportable, and which must suppress dependent Findings?
15. Which identities and revisions determine profile equality, staleness, and reproducibility?
16. Which paired file, programmatic-value, profile, and Finding fixtures are sufficient to prove resolver conformance?

## Relationship to Other Documents

| Document | Relationship |
| --- | --- |
| [000 — Intlify overview](./000-intlify-overview-design.md) | Defines the product-wide architecture, terminology, inherited locale invariants, Roadmap, and Expected Outcomes refined here. Its broader illustrative configuration-format wording requires alignment with the canonical JSON decision recorded by 015. |
| [006 — Tooling foundation](./006-ox-mf2-phase-3a-tooling-foundation-design.md) | Provides existing JSON/JSONC discovery, JSON Schema, CLI, and project-configuration implementation experience. 015 makes strict JSON primary and leaves the compatibility disposition of JSONC to follow-up product design. |
| [014 — Message linker](./014-ox-mf2-message-linker-design.md) | Provides current locale, fallback, delivery, and resolved-policy implementation experience; 020 owns the source-first linker evolution that consumes this profile. |
| [016 — Source authoring and Intent identity](./016-intlify-source-authoring-and-intent-identity-design.md) | Owns Intent source-locale declarations and evidence that use the project default defined here only when omitted. |
| [017 — Shared artifacts and version admission](./017-intlify-shared-artifact-and-version-admission-design.md) | Owns shared encodings, identities, version admission, and migration for the semantic model defined here. |
| [018 — Security, trust, and provenance](./018-intlify-security-trust-and-provenance-design.md) | Owns trust and credential specifications referenced, but not embedded as secrets, by the profile. |
| [019 — Project graph, query, and incremental processing](./019-intlify-project-graph-query-and-incremental-design.md) | Owns dependency tracking, common Finding projection, query, and incremental processing over profile inputs. |
| [020 — Requirement planning and linking](./020-intlify-requirement-planning-and-linking-design.md) | Consumes resolved locale, coverage, fallback, target-applicability, and delivery inputs. |
| [021 — Translation Store and governance](./021-intlify-translation-store-and-governance-design.md) | Consumes Selection Scope and governance-policy references. |
| [022 — Provider and localization sync](./022-intlify-provider-and-localization-sync-design.md) | Consumes Provider-routing, Glossary Set, refresh, and locale-demand inputs. |
| [023 — Localization execution specification](./023-intlify-localization-execution-specification-design.md) | Consumes locale-negotiation, locale-service, and scoped-locale semantics. |
| [024 — Target Profile and export](./024-intlify-target-profile-and-export-design.md) | Owns Target Profile capabilities and output semantics referenced by project and group declarations. |
| [025 — Release Assembly and deployment](./025-intlify-release-assembly-and-deployment-design.md) | Owns Release behavior for the Deployment Compatibility Groups declared and validated here. |
| [027 — Reference Runtime](./027-intlify-reference-runtime-design.md) | Implements one physical execution path that consumes the effective requested-locale and negotiation inputs defined here through 023–025. |
| [029 — Product workflow and packaging](./029-intlify-product-workflow-and-packaging-design.md) | Owns `intlify.config.json` discovery, workspace selection, schema packaging, optional programmatic helper UX, commands, packaging, and user-facing workflow without introducing alternate configuration semantics. |
