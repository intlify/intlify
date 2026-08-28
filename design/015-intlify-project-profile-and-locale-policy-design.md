# Intlify Project Profile and Locale Policy Design

## Status

This document is an initial detailed-design scaffold. It inherits the product direction and terminology from [000](./000-intlify-overview-design.md), but it does not yet make decisions beyond that overview.

The sections below identify the specification areas, responsibility splits, evidence, and open questions that must be resolved before this design becomes normative. Placeholder headings and candidate categories do not reserve a public field, type, configuration key, command, wire tag, or package name.

## Purpose

This document defines the language-neutral resolved project input consumed by shared Intlify compiler stages and the locale-policy semantics needed to produce it.

Host tooling may accept JavaScript, TypeScript, TOML, YAML, framework configuration, workspace metadata, or another host-specific format. Before shared compilation begins, that tooling resolves the host input into one checked `LocalizationProjectProfile`. Shared planning, synchronization, linking, export, Release Assembly, tooling, and execution integrations consume that resolved profile rather than interpreting host configuration independently.

Conceptually:

```text
host-specific configuration and workspace inputs
  -> host configuration discovery and parsing
  -> project-profile resolution and validation
  -> one language-neutral LocalizationProjectProfile
     -> project graph and queries
     -> requirement planning and linking
     -> synchronization and governance inputs
     -> target export and Release Assembly
     -> locale negotiation and localization execution inputs
```

## Goals

- Define what one resolved `LocalizationProjectProfile` represents and how it is identified.
- Define the semantic split between host-facing configuration and the language-neutral resolved profile.
- Define project requested locales, source-locale defaults, requested-locale defaults, Target Profile subsets, target overrides, and effective defaults.
- Keep requested-locale negotiation separate from message locale fallback and single-message evaluation.
- Define the profile inputs for coverage, Provider routing, approval, Glossary Sets, delivery, trust, and resource policies without taking ownership from their detailed designs.
- Define how Target Profiles form one or more Deployment Compatibility Groups.
- Define deterministic resolution, validation, Finding production, and consumer-visible dependency inputs.
- Make invalid, ambiguous, incomplete, or incompatible configuration fail before synchronization, linking, export, or production execution.
- Provide language-neutral fixtures that host configuration resolvers and downstream consumers can share.

## Non-Goals

- Freezing the user-facing JavaScript, TOML, YAML, framework, workspace, or CLI configuration syntax.
- Defining configuration-file discovery, command-line option precedence, or workspace UX owned by [029](./029-intlify-product-workflow-and-packaging-design.md).
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
| Host configuration integration | Discovers and parses host-facing configuration, resolves host-specific references, and invokes the profile resolver |
| 015 project-profile resolver | Normalizes and validates one language-neutral profile according to this specification |
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
| 029 product workflow | Owns public configuration syntax, discovery, workspace behavior, commands, and packaging |

## Inherited Decisions from 000

The following are fixed inputs from the overview and are not open questions in this document:

- shared compiler stages consume a resolved language-neutral profile rather than host configuration objects;
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

The definitions in 000 remain authoritative while this design is incomplete. This section will refine only the profile-specific semantics and relationships needed by consumers.

| Term | Profile-specific question to resolve |
| --- | --- |
| Localization Project Profile | Exact project scope, identity, required sections, and completeness rules |
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

The resolved profile has four conceptual groups. Their exact representation remains to be designed.

```text
LocalizationProjectProfile
  + project and Selection Scope identity
  + locale model and locale-policy inputs
  + Target Profile and Deployment Compatibility Group declarations
  + versioned references to delivery, Provider, governance, trust, and resource policies
```

This design must keep the semantic model independent of host configuration syntax while preserving enough source evidence for actionable Findings.

## Profile Scope and Identity

This section will define:

- what one profile represents: final application, deployable product, workspace member, or another exact unit;
- whether a workspace resolves several independent profiles or one composite profile;
- project identity, Selection Scope association, and profile revision inputs;
- which identities are opaque and which carry semantic meaning;
- whether profile composition is allowed and, if so, which layer owns it; and
- completeness requirements before a profile can be consumed.

## Configuration Resolution Responsibility Split

### Host-facing inputs

This subsection will define which facts a host integration supplies to the language-neutral resolver, including source provenance and host-location evidence, without freezing a public config format.

### Discovery, layering, and precedence

This subsection will decide whether discovery, inheritance, workspace defaults, environment overlays, command-line overrides, and framework defaults are entirely 029-owned or require language-neutral precedence semantics here.

### Resolved output

This subsection will define the success and failure boundary of profile resolution, including whether warnings can accompany a usable profile and which failures prevent any partial output.

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
- the evidence retained when a host spelling is normalized or rejected.

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

1. admit host-supplied configuration facts and source evidence;
2. resolve project identity and Selection Scope;
3. validate and canonicalize locale identifiers;
4. resolve project locale sets and defaults;
5. admit policy references and revisions;
6. resolve Target Profile references and requested-locale subsets;
7. form and validate Deployment Compatibility Groups;
8. resolve each effective default requested locale;
9. validate cross-target locale and hydration requirements;
10. produce deterministic Findings or one complete resolved profile.

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

This section will define which exact identities and revisions make the resolved profile stale, including locale sets, target membership, policy references, and host configuration evidence.

It will specify semantic equality and deterministic resolution inputs while leaving shared digest framing and cache implementation to 017 and 019. Two resolution executions over the same admitted facts, tool/specification versions, and referenced revisions must not disagree because of map order, filesystem enumeration, concurrency, or host-language object identity.

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

The completed design must prevent consumers from silently applying their own defaults or reinterpreting unresolved host configuration.

## Conformance and Fixtures

The fixture plan will include language-neutral resolved-profile fixtures and host-resolver projection fixtures for at least:

- one Browser target;
- hydration-coupled Browser and SSR targets;
- independently released Web and mobile groups;
- per-target requested-locale subsets and default overrides;
- explicit and inherited source locales, including library Intents;
- direct-required and fallback-allowed coverage;
- locale negotiation distinct from message locale fallback;
- versioned Provider, governance, Glossary Set, delivery, trust, and resource references;
- duplicate, invalid, missing, stale, unsupported, and cross-target-incompatible inputs;
- deterministic ordering under permuted host input; and
- exact and first-over resource-limit cases.

The completed design will assign each fixture to the owning component and identify the conformance evidence required from third-party host configuration resolvers.

## Implementation Phasing

The implementation plan will be defined after the semantic decisions above are complete. The intended design checkpoints are:

1. profile scope, identity, locale model, and inherited invariants;
2. locale-policy inputs and deterministic resolution;
3. Target Profile and Deployment Compatibility Group validation;
4. Findings, limits, dependency identity, and conformance fixtures; and
5. host resolver and downstream-consumer integration evidence.

These checkpoints do not reserve package names, commands, or public APIs.

## Decision Log

Resolved decisions will be recorded here as the design proceeds.

| ID | Decision | Status | Rationale | Affected sections |
| --- | --- | --- | --- | --- |
| — | No 015-specific decisions have been made yet | Open | Initial scaffold only | All |

## Deferred Follow-Up Notes

The following remain in their owning designs unless a concrete 015 semantic dependency requires a narrower interface here:

- exact host configuration syntax, discovery, workspace inheritance, commands, and packaging: 029;
- source authoring and Intent source-locale evidence: 016;
- artifact encoding, digest framing, version migration, and capability admission: 017;
- trust roots, actor powers, credentials, signatures, and provenance: 018;
- common Finding envelope, graph queries, cache implementation, and incremental scheduling: 019;
- requirement planning, fallback selection, reachability, placement, and pruning: 020;
- Store, governance, Provider, TMS, and synchronization workflows: 021 and 022;
- locale-service execution semantics and portable values: 023;
- Target Profile capability and output schemas: 024;
- Release publication, deployment, and execution admission: 025; and
- public product workflow and configuration UX: 029.

No dormant field, type, package, configuration key, command, wire tag, or format name is reserved merely by appearing as a candidate in this scaffold.

## Open Questions

The design will resolve these questions one at a time and move each accepted answer into its owning section and the Decision Log.

1. What exact unit does one `LocalizationProjectProfile` represent?
2. Which configuration layering and precedence semantics belong here rather than 029?
3. What locale-identifier domain, canonicalization, equality, and ordering rules are normative?
4. When is the project default source locale required, and how is omission represented?
5. What are the minimum and maximum project requested-locale sets?
6. How are project defaults, Target Profile overrides, and effective defaults resolved?
7. What exact inputs constitute a Locale Negotiation Profile?
8. What profile inputs define message locale fallback without moving the Linker algorithm into 015?
9. How are coverage rules scoped and ordered?
10. How are Provider, approval, Glossary Set, trust, and resource policies referenced by revision?
11. Which delivery-topology facts belong in the profile rather than the host build graph?
12. How are Target Profiles assigned to Deployment Compatibility Groups, including hydration coupling?
13. Which failures are independently reportable, and which must suppress dependent Findings?
14. Which identities and revisions determine profile equality, staleness, and reproducibility?
15. Which conformance fixtures are sufficient to admit a third-party host configuration resolver?

## Relationship to Other Documents

| Document | Relationship |
| --- | --- |
| [000 — Intlify overview](./000-intlify-overview-design.md) | Defines the product-wide architecture, terminology, inherited locale invariants, Roadmap, and Expected Outcomes refined here. |
| [006 — Tooling foundation](./006-ox-mf2-phase-3a-tooling-foundation-design.md) | Provides existing CLI and project-configuration implementation experience; it does not freeze the new source-first resolved profile. |
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
| [029 — Product workflow and packaging](./029-intlify-product-workflow-and-packaging-design.md) | Owns public configuration syntax, discovery, workspaces, commands, packaging, and user-facing workflow. |
