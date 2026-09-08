# Intlify Shared Artifact and Version Admission Design

**Intlify Shared Artifact and Version Admission Specification Revision:** `"0"`

## Purpose

This design fixes the shared representations needed to implement the minimum scope of [015](./015-intlify-project-profile-and-locale-policy-design.md): complete configuration structure, an internal locale-canonicalization boundary, and the private source/requested/default-locale core. It also supplies the shared identity and integrity rules required by that implementation's initial [026](./026-intlify-conformance-and-measurement-design.md) measurement path.

There are two immediate uses:

- **Configuration:** encode an exact Policy or Target Profile reference so `intlify_config` can generate and validate the complete `intlify.config.json` schema without test-only reference placeholders.
- **Measurement:** retain an owner result, identify and validate common records, and follow exact references from a planned run through observations and evaluations to a structured report.

The configuration path establishes the shape of a reference, not the validity of its referenced artifact. The measurement path establishes the representation and integrity of a record, not whether its result is successful or its runner is trusted. Those decisions remain with the owning specifications.

This revision specifies only this minimum shared subset. It does not complete every artifact family assigned to 017 by [000](./000-intlify-overview-design.md), nor does adopting it establish complete 015 Profile Specification revision-`"0"` support.

The adoption scope is 015 Implementation Phase 1 plus only the finite, test-owned canonicalization and private locale-core slices of Phases 2 and 3. Defining the complete configuration structure does not require resolving every referenced Policy or Target Profile body in that minimum slice.

## Goals

- Fix the JSON representation of `PolicyReference` and `TargetProfileReference` used by configuration schema version `"0"`.
- Separate reference structure, supported-version admission, referenced-content validation, and trust.
- Fix common verification envelopes, top-level and nested record references, exact quantities, and canonical digest framing for initial measurement records.
- Keep record-instance identity, integrity, Measurement Case identity, semantic observations, and native owner-result provenance distinct.
- Allow the initial 015 implementation to use these definitions without implementing Intent artifacts, a public checked-profile format, deployment artifacts, or a general migration system.

## Non-Goals

- Defining Policy bodies, Target Profile bodies, artifact acquisition, bootstrap trust, signatures, or authorization.
- Encoding the full `LocalizationProjectProfile`, its binding sidecar, Resolver Construction Root Package, Programmatic Entry Snapshot, Finding/Evidence model, or canonicalization-data artifacts.
- Defining Intent identity, source or localized message artifacts, `LibraryManifest`, Store records, Release artifacts, target output, or Runtime ABI.
- Replacing 026's measurement semantics, owner benchmark schemas, logical work, checksum algorithms, or component interval definitions.
- Adding comparison, budget, runner-qualification, profiling, cross-target, or Release capabilities to the initial observational measurement scope.
- Defining public crate/API names, schema distribution paths, registry services, binary transport, or migration commands.

## Ownership and Dependencies

| Owner | Responsibility in this subset |
| --- | --- |
| 015 | Configuration members and presence, field-role expectations, structural versus semantic admission, selection, locale behavior, resource limits, and the active owner measurement boundaries |
| 017 | Shared reference JSON, verification envelope and reference representation, identity domains, canonical framing, integrity coverage, and exact-version selection |
| 018 | Artifact authentication, provenance, trust, and authorized evidence use; an integrity digest never supplies these decisions |
| 026 | Verification and measurement body semantics, common reasons, expected-input resolution, case dimensions, samples, environment, projection admission, and report meaning |
| Owner measurement implementation | The closed native result schema, native identity/checksum, fixture expectations, Measurement Profile, and exact versioned projection into 026 |
| 029 | Discovery, acquisition, publication, packaging, and user-facing workflow; none is performed by a shared decoder |

The implementation may colocate small reusable types with their first consumer. This design does not require a new crate or promote the existing resource-oriented `intlify_contract` formats into source-first authority. Reuse of code is separate from reuse of a format or version domain.

## Terminology

| Term | Meaning here |
| --- | --- |
| Exact reference | The complete kind, identity, artifact revision, owning specification revision, and semantic content digest tuple |
| Structurally admitted reference | A value that satisfies the closed reference schema; no artifact body has thereby been admitted |
| Record identity | A domain-qualified name for one immutable record instance |
| Integrity digest | A digest of the complete canonical stored record except its own integrity-digest member |
| Semantic projection | The exact logical fields and ordering selected by the owning specification for equality or identity, excluding that owner's declared non-semantic metadata |
| Native owner result | The complete result retained under its own schema, identity, validation rules, and checksum; it is not replaced by common evidence |
| Admission registry | A finite implementation-supplied mapping of exact supported schema/specification tuples to local validators and codecs; it is not a network registry |

## Design Overview

| Input | Shared operation defined here | Owning operation that follows |
| --- | --- | --- |
| Policy/Target reference in configuration | Closed reference decoding | 015 structural admission, followed later by exact artifact resolution and owner body checks |
| Native benchmark result | Preserve its owner-qualified identity and native integrity information | Owner validation and the registered 026 Measurement Projection |
| Common verification record | Decode, select the exact schema, verify integrity, and resolve typed references | 026 case, sample, run, projection, and report validation |

017 does not add an executable compiler phase. The reference definitions are used by 015's Configuration Foundation, and measurement encoding is used alongside each active measured boundary.

## Shared JSON Primitives

The following definitions are normative for this subset. They are not aliases for every identity or version elsewhere in Intlify.

| Type | JSON representation and validation |
| --- | --- |
| `IdentityToken` | String matching the complete ASCII grammar `^[a-z0-9](?:[a-z0-9._-]*[a-z0-9])?$`, with no normalization or trimming |
| `RevisionToken` | The same string grammar, interpreted as an exact opaque revision, not a numeric value or an executable selector |
| `SemanticDigest` / `IntegrityDigest` | String `sha256:` followed by exactly 64 lowercase hexadecimal digits; all 256 bits are retained |
| `UInt64` | Shortest ASCII decimal string for an integer in `0..=18446744073709551615`, with the lexical grammar below plus the range check |
| `PositiveUInt64` | `UInt64` excluding `"0"` |
| `ByteString` | An even-length lowercase hexadecimal string, two digits per byte; a field's schema specifies whether the empty byte sequence is allowed |
| `VersionedIdentity` | Closed object `{ "identity": IdentityToken, "revision": RevisionToken }`, both members required |

The `UInt64` lexical grammar is `^(0|[1-9][0-9]*)$`.

Grammar checks match the entire string, including rejecting trailing line terminators. All decoders reject malformed UTF-8, a byte-order mark, trailing non-whitespace data, duplicate object members, and unpaired surrogate escapes before a map can discard information. Strings contain Unicode scalar values only. Escape spelling and object-member order are not semantic; Unicode normalization, case folding, and locale collation are never implicit.

An artifact revision is a literal pin. There is no range, branch, mutable-tag, timestamp-selection, or `latest` lookup operation in this representation. Structural admission cannot prove that a publisher has kept a revision immutable; exact body matching and conflict rejection are later owner checks. The same label must never be reinterpreted as a floating selector by an adapter.

015's input and string bounds continue to apply to configuration references. Measurement readers must apply explicit finite byte, depth, string, node, and collection limits before allocation or recursive decoding; a reader-capacity failure cannot be represented as an admitted truncated record. This subset introduces no project Resource Limit Policy default.

Common measurement JSON encodes **every** integer quantity, count, repetition, ordinal, and byte offset as `UInt64`, including small values. Field schemas retain their narrower constraints: a repetition is positive, while an ordinal may be zero. JSON numbers, signs, leading zeroes, exponent notation, and fractional spellings are invalid in these common record fields. This does not change 015's Portable JSON Number domain or an existing native owner-result schema.

Fixed objects are closed, newly defined common tagged unions use an explicit string `kind`, and absent optional members are omitted. Registered native-owner fragments retain the representation specified under Verification Record Representation. `null` is admitted only by an explicitly nullable field. In particular, the two nullable policy slots in 015 remain required members; `null` and omission are not interchangeable.

## Configuration Reference Representation

### PolicyReference

`PolicyReference` is one closed object with exactly five required members:

| Member | Type | Meaning |
| --- | --- | --- |
| `kind` | One Policy kind from the table below | Prevents cross-kind coercion |
| `identity` | `IdentityToken` | Opaque artifact identity |
| `revision` | `RevisionToken` | Exact artifact revision |
| `specificationRevision` | `RevisionToken` | Exact revision of that kind's owning Policy specification |
| `semanticDigest` | `SemanticDigest` | Pin to the semantic content selected by that specification |

| 015 configuration member | Expected `kind` | Presence owned by 015 |
| --- | --- | --- |
| `policies.resourceLimits` | `resource-limit-policy` | Required reference |
| `policies.trust` | `trust-policy` | Required reference |
| `policies.sourceAdmission` | `source-admission-policy` | Required reference |
| `policies.approval` | `approval-policy` | Required reference |
| `policies.selection` | `selection-policy` | Required reference |
| `policies.providerRouting` | `provider-routing-policy` | Required member, reference or explicit `null` |
| `policies.glossarySet` | `glossary-set` | Required member, reference or explicit `null` |

These seven strings close the Policy kind vocabulary for the configuration-reference subset. Each kind selects its owning specification; the reference has no independently overridable specification-ID member. The common structural schema admits the declared Policy kind union. The expected kind for a particular 015 role is checked at the owning semantic reference-admission step, not inferred from the enclosing property or repaired by changing `kind`.

The following materializes the reference syntax of 015's `RESOURCE_LIMITS_REF` blueprint:

```json
{
  "kind": "resource-limit-policy",
  "identity": "storefront-resource-limits",
  "revision": "1",
  "specificationRevision": "0",
  "semanticDigest": "sha256:1111111111111111111111111111111111111111111111111111111111111111"
}
```

The digest is the illustrative pin already present in 015, not a claim that a matching Policy body has been constructed or verified. A structural fixture may use it; a body-admission fixture must supply a body and its actually computed digest.

### TargetProfileReference

`TargetProfileReference` has the same five required member names and primitive types, with `kind` fixed to `target-profile`. Its `specificationRevision` names the owning Target Profile specification revision, not a Policy revision, Runtime ABI, target platform, or package version.

```json
{
  "kind": "target-profile",
  "identity": "storefront-browser",
  "revision": "1",
  "specificationRevision": "0",
  "semanticDigest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
}
```

This is likewise a reference-shape example using 015's illustrative pin. The enclosing project Target ID remains a separate 015 identity; it is not copied into or inferred from this artifact reference.

### Equality, schema generation, and admission

Exact reference equality compares all five fields. Canonical tuple order is `(kind, identity, revision, specificationRevision, semanticDigest)` using unsigned UTF-8 byte comparison; revisions are not numerically or semantically version-ordered. A matching digest cannot erase a different kind, identity, or revision. Duplicate/conflicting submitted artifacts are classified under 015's rules, not by silently deduplicating references.

The generated Draft 7 configuration schema must contain closed reusable definitions for both reference types: `type: "object"`, all five `required` members, `additionalProperties: false`, the fixed kind enumeration/constant, and the primitive constraints above. Runtime structural validation and schema validation must agree. `$testPolicy`, `$testTarget`, an unconstrained JSON value, and `{ "...": "Reference" }` are not production alternatives.

The reference representation used by configuration `schemaVersion: "0"` is fixed by that configuration schema's admitted authority. References do not add a second author-controlled `schemaVersion` or `$schema` member. The `$schema` editor hint never loads or selects an artifact validator.

Structural success does not establish supported Policy/Target semantics, artifact availability, body integrity, trust, or a checked profile. The minimum private locale core can retain these structurally admitted references without resolving their bodies, but cannot expose that result as a complete `LocalizationProjectProfile`.

## Verification Record Representation

### Initial record family

The common wire subset covers these record kinds, each with record-schema revision `"0"` and governing specification `intlify-design-026` revision `"0"`:

| `recordKind` | Body semantics owned by 026 |
| --- | --- |
| `measurement-run-plan` | Run/profile/subject/build binding, complete ordered selected case inventory, requirements, applicability rules, and planned runner |
| `measurement-evidence-set` | Exact Plan and owner-result binding, complete build/environment, and only admitted successful cases and samples |
| `measurement-run-evaluation` | Exact Plan, owner-input resolution, complete case inventory and evaluations, outcome, and ordered reasons |
| `structured-report` | Ordered typed sections and lossless references/projections of their evidence and evaluations |

Only the `measurement-observation` Structured Report section is activated for the 015 minimum. It still retains the full required/optional case accounting, missing or invalid attempts, and truncation state required by 026; an empty Evidence Set list is allowed where 026 permits it. No aggregate score or numeric budget is introduced.

Each record uses the closed outer form:

```text
VerificationRecord<T> {
  envelope: {
    recordKind: registered record-kind string
    recordSchemaRevision: RevisionToken
    governingSpecification: VersionedIdentity
    recordIdentity: RecordIdentity
    integrityDigest: IntegrityDigest
    producingTool: VersionedIdentity
    creationContext?: { createdAtUnixNanoseconds: UInt64 }
  }
  body: T
}
```

`creationContext`, when present, is the closed one-field object shown above. Its time is optional wall-clock context since the Unix epoch, not a measured duration. Absence remains absence. Machine paths, hostnames, environment dumps, credentials, and arbitrary metadata are not extension fields.

`T` is not an arbitrary JSON extension. The finite admission registry selects one complete closed body schema and validator for the exact record tuple. The implementation materializes 026's adopted body types and constraints in those schemas. This document fixes the shared representation, while 026 remains the authority for field applicability, typed descriptor contents, reason-stage registries, and outcome validation. A decoder without the required body schema rejects the record; it cannot validate just the envelope and claim common-record admission.

Those generated shared schemas are version-controlled companions to this specification, not consumer-selected plugin schemas. All implementations admitting the same tuple must pin the same complete schema and canonical fixtures. A consumer may support fewer tuples, but cannot redefine a supported tuple's fields, Case-identity projection, or canonical variant orders locally. Materializing these schemas and fixtures is part of the adopting implementation work, not permission to infer missing definitions at decode time.

Body member names use lower camel case, literal enum/registry IDs retain their owning spelling, and newly defined common variant objects use `kind`. Owner phase/cost tokens and the 026 environment-field IDs such as `os_family` are literal IDs, not member names to rename. Schema generation must pin the complete member/variant inventory of each adopted body type and test it with round-trip fixtures; it must not infer a schema or variant order from incoming data.

An explicitly adopted, closed native-owner fragment retains its pinned owner representation, including native variant tags. This exception is limited to schema-registered fragment paths; it is not an arbitrary JSON payload or a permission to mix owner revisions. For the initial 015 Measurement Case projection, those paths are `intervalBoundary`, `variant`, `executionState`, `workload`, and `measurementMethod`. Their complete schema definitions are included in the Case-identity schema; quantities still satisfy the common exact-string domain. A change to an adopted fragment requires reviewing the owner schema/profile and common projection versions rather than silently changing an existing Case identity's meaning.

The Evidence schema additionally adopts each case's `observedDescriptors` and each sample's `semanticObservation.completeObservation` from the pinned native owner representation. Its `identityProjection` embeds the Case projection above. The observed `clock_or_sampler` value retains the closed native Clock Observation, and report rows retain the same native `executionState`. These paths have complete definitions in their enclosing schema; none accepts an arbitrary native payload. Native checksum and sample-execution values remain qualified by their algorithm, framing, domain, and source owner result where applicable, rather than acquiring a shared SHA-256 spelling.

### Initial 015 schema companions

The adopting implementation must provide the following generated, closed Draft 7 schema companions and their canonical fixtures. These are repository implementation artifacts, not public schema distribution URLs or an application-facing API.

| Schema companion | Representation fixed by the schema |
| --- | --- |
| [Project-profile configuration](../crates/intlify_config/schema/project-profile-config-v0.schema.json) | Complete 015 configuration schema version `"0"`, including the formal reference types defined above |
| [Measurement Run Plan](../crates/intlify_config/schema/measurement-run-plan-v0.schema.json) | Complete `measurement-run-plan` envelope and body for record-schema revision `"0"` / governing specification `intlify-design-026` revision `"0"` |
| [Measurement Case identity input](../crates/intlify_config/schema/measurement-case-identity-v0.schema.json) | The complete digest input, including every applicable 026 semantic dimension for the six initial 015 component-duration boundaries |
| [Measurement Evidence Set](../crates/intlify_config/schema/measurement-evidence-set-v0.schema.json) | Exact Plan/native-result binding, complete applicable Build and 27-field Environment Observation, admitted cases and raw samples |
| [Measurement Run Evaluation](../crates/intlify_config/schema/measurement-run-evaluation-v0.schema.json) | Exact input resolution, complete planned inventory, one typed evaluation per case, aggregate outcome and ordered reasons |
| [Observational Structured Report](../crates/intlify_config/schema/measurement-structured-report-v0.schema.json) | The initial single-run `measurement-observation` section, exact nested sample/evaluation references, complete missing-case inventory and truncation state |

The Run Plan binds an acquired build observation, subject, profile, runner instance, and the complete ordered inventory before fixture preparation or measurement. Its Case identities use independently fixed complete work vectors, not work observed only after a successful run. Preparation failure therefore does not erase a planned required case. The initial profile makes every selected case required and unconditional; it introduces no optional-case applicability language. `plannedRunnerClass` is a required nullable field in this wire schema, with `null` meaning no qualified class was selected.

The Plan's native build reference preserves the owner schema, checksum algorithm, framing, domain, and value. It is a binding to the acquired Build Observation, not an attestation of the running executable or a substitute for the complete Build and Environment information required when evidence is projected. The initial validator retains the independently issued Plan and captured native result, resolves the explicit finite record collection, regenerates the exact applicable projection, and rechecks Evaluation and Report relationships. Schema and digest checks alone never replace those semantic checks.

The initial producer uses one Evidence Set when at least one case is projectable, and one Evaluation and observational Report for the run. A valid negative run may have no Evidence Set. The section prohibits numeric decisions and uses explicit complete truncation state; no multi-run aggregation, optional/conditional case policy, age-based reuse, or truncation facility is activated by these schema companions. A schema-valid `not-applicable` alternative cannot satisfy the initial Plan's unconditional cases without the 026-required rule and proof.

Environment entries have the closed form `{ localRecordIdentity, observation: { field, state } }`. `field` selects the registry-owned typed value/state schema; `state` uses the common `kind` discriminator. The schema pins all 27 entries in 026 registry order, including each field's allowed states. The field registry is `intlify-design-026-environment-fields` revision `"0"`. The initial native harness and projection identities are `intlify-config-owner-run-harness` revision `"1"` and `intlify-config-minimum-to-026` revision `"0"`.

Two initial applicability rules are registered: `intlify-config-native-unmanaged-component-context` revision `"0"` proves that a native Rust component is not executed by a language runtime, browser, managed VM, or JIT/GC configuration; `intlify-config-duration-only-no-memory-observer` revision `"0"` proves that a duration-only case has no memory-observer requirement. Neither proves physical-host execution or absence of a container, emulator, simulator, or hypervisor. Unknown applicable environment/build facts retain common reasons. A reported kernel release is not a kernel-build identity, a parallelism hint is not a logical CPU count, and neither Cargo inputs nor a source snapshot attest the effective executable/build configuration or a clean working tree.

### Record and run identities

```text
RecordIdentity {
  domain: IdentityToken
  value: Unicode-scalar string
}
```

Identity domains are admitted explicitly by the local registry. Equality compares the complete `(domain, value)` pair as exact UTF-8 bytes; concatenated display labels and paths are not identity encodings.

New common record instances use domain `intlify-verification-record-v0` and a value consisting of exactly 64 lowercase hexadecimal digits from 32 fresh random bytes. The producer acquires those bytes once from an operating-system cryptographic randomness source. Acquisition failure is an explicit production failure, with no timestamp, counter, address, or unchecked fallback. Persisting or retransmitting that record retains its identity; changing any stored field requires a new instance identity and integrity digest. Record IDs do not enter component semantic checksums or Measurement Case identity.

Measurement Run identity uses the same closed object and byte rule in the separate domain `intlify-measurement-run-v0`. It is acquired before capture and binds exactly one immutable Run Plan. A new profile, build, subject, selected inventory, or planned runner requires a new run and Plan as specified by 026. Independent Plan, Evidence, Evaluation, and Report records have distinct record-instance identities even when they refer to the same run.

Random instance naming is not a reproducibility failure: revalidation of a retained record uses the retained IDs. Repeated semantic evaluation uses the same owner-defined semantic projection, not equality of newly minted instance IDs.

Native owner-result identities use separately registered owner domains and retain their exact native value and schema association. The projection must retain the native result's complete identity, checksum algorithm/framing revision, checksum value, and source schema/profile revisions. A source format without immutable result identity needs an explicit owner-schema identity rule before it can participate; an arbitrary fixture label or a bare checksum is not silently upgraded into one.

For the initial `intlify_config` owner result schema `intlify-config-owner-run-result/1`, domain `intlify-config-owner-result-v1` uses the same 32-byte OS-random value rule. The identity is reserved before capture, bound into the native plan, and retained by the single completed owner result, including an incomplete or invalid result. The native checksum remains separately recorded with its original algorithm/framing. Changing result content requires a new result instance; recomputing a checksum cannot authorize a replacement observation under the old identity.

The initial local harness also registers `intlify-config-local-runner-instance-v0` with the same random-value rule. This identifies the one local runner invocation fixed in the Run Plan, not a stable machine, a hardware fingerprint, or a qualified Runner Class. These domains are role-specific: a correctly spelled native result or runner identity cannot replace a common record or Measurement Run identity.

### Top-level and nested references

The two 026 reference alternatives have these closed wire shapes:

```text
VerificationRecordReference =
  { kind: "top-level", recordIdentity: RecordIdentity }
  | {
      kind: "nested-record",
      reference: {
        parentRecordIdentity: RecordIdentity,
        localRecordIdentity: IdentityToken
      }
    }
```

Nested records do not copy the parent envelope or acquire a separate top-level integrity digest. A parent schema fixes which children are referenceable and their local-identity construction. Local identities must be unique within the complete parent, stable for that immutable instance, and distinct across child roles. A sample ordinal may be part of a schema-defined sample-local identity; an array position must never substitute for Measurement Case identity. A local identity alone is not a globally resolvable reference.

Initial common local identities use distinct role prefixes: `inventory-case-`, `environment-field-`, `measurement-case-`, `measurement-sample-`, `case-evaluation-`, `measurement-observation-section-`, `measurement-row-`, `report-sample-`, and `missing-case-`. The schema's fixed inventory order supplies canonical decimal case/sample ordinals; environment entries use their exact registry field ID. Samples include both case and sample ordinals. These local names do not replace each case's complete `mc0_` semantic identity. The native owner-result adapter registers `attempt-<ordinal>` for the exact retained attempt with that canonical decimal ordinal under `intlify-config-owner-run-result/1`; resolution checks its complete parent identity and admitted attempt inventory before a diagnostic or source reference resolves.

The record resolver accepts an explicit finite collection of retained common records and registered native owner results. It performs no file discovery or network access. Equal identities with unequal canonical stored content are an identity conflict even if both supplied digests recompute correctly. Equal duplicate submissions are reported as duplicate input instead of selecting an occurrence by insertion order. Neither condition is repaired by hashing the records again.

Resolution checks the expected record kind/schema, integrity, owner adapter where applicable, and nested identity before returning `resolved`. Record-kind-specific checks then enforce exact run/Plan/case binding. Reference resolution of an evaluation with an `invalid` or `incomplete` outcome does not turn that evaluation into successful evidence.

### Shared input states and reasons

The closed JSON representation of 026's result alternatives is:

```text
EvaluationInputResolution {
  expected: typed expected record/case/selector identity
  result:
    { kind: "resolved", reference: VerificationRecordReference }
    | {
        kind: "unavailable",
        sourceEvaluations: VerificationRecordReference[],
        reasons: VerificationReason[]
      }
    | {
        kind: "invalid",
        submittedInput?: VerificationRecordReference,
        reasons: VerificationReason[]
      }
}

VerificationReason {
  code:
    { kind: "common", code: registered common code }
    | {
        kind: "owner-specific",
        ownerIdentity: IdentityToken,
        ownerRevision: RevisionToken,
        code: registered owner code
      }
  stage: registered stage identity
  affected: typed record/nested-record/expected-selector reference
  related: ordered typed references
  detail: closed detail type selected by the evaluator's reason registry
}
```

The expected/affected selectors, stages, codes, and detail types are the adopted evaluator's closed 026 schema, not open maps, free-text reasons, or a newly invented generic selector language. The common cause inventory, non-success requirement for at least one common cause, and reason ordering are exactly those in 026. Physical object-key order and the decimal spelling of numeric details must never replace 026's numeric, variant, field, or stage comparators.

The initial measurement evaluator activates common reasons only. Its stage order is `input-admission`, `plan`, `binding`, `inventory`, `case-result`, `evidence`, `environment`, `reporting`. The closed reason variants and members are included in the Evaluation/Evidence/Report schemas. Their semantic comparison orders are:

- References: `top-level` before `nested-record`; a nested reference compares parent identity then local identity. Record identities compare domain then value by unsigned UTF-8 bytes.
- Affected selectors: `record` (reference), `planned-case` (run Plan then Case identity), `environment-field` (record identity then field ID), `build-field` (record identity then field ID). Field IDs compare by their literal bytes, not registry position. Affected/expected selectors may name a missing or rejected input; diagnostic and related references must actually resolve.
- Typed details: `missing-observation`, `missing-input`, `unsupported-tuple`, `invalid-input`, `duplicate-input`, `binding-mismatch`, `inventory-mismatch`, `invocation-failed`, `invalid-owner-case`, `semantic-mismatch`, `sample-count`, `projection-mismatch`, `report-mismatch`. Fields compare in the listed semantic order: cause for missing observation; subtype then diagnostic reference for invocation failure; diagnostic reference for invalid-owner-case and semantic mismatch; required count then available count for sample count. Other variants have no fields.
- Missing-observation cause order: `not-collected`, `native-acquisition-unavailable`, `effective-build-not-attested`, `executable-not-attested`, `kernel-release-is-not-build-identity`, `parallelism-hint-is-not-logical-cpu-count`, `finite-provider-has-no-locale-service-profile`, `source-control-state-not-attested`.
- Invocation subtype order: `measurement-overflow`, `counter-overflow`, `repetition-overflow`, `duration-conversion-overflow`, `clock-failure`, `invocation-panicked`, `prerequisite-unavailable`, `collector-allocation`, `output-failure`, `observation-panicked`, `work-observation-failure`.

These orders refine 026's stage/code/affected/related/detail priority; counts compare numerically and strings/IDs by unsigned bytes. Distinct details remain distinct reasons, and only completely equal reasons are deduplicated. Unknown stages, variants, and code/detail combinations are invalid, not extension text. No owner-specific reason namespace is activated without its own registered revision.

## Canonical Encoding and Digests

### Scope of canonical encoding

Canonical encoding here operates on the **complete schema-admitted JSON value**, not source bytes, debug output, hash-table iteration, or a host serializer's incidental order. All quantities have already become exact decimal strings. Allowed values are `null`, booleans, Unicode-scalar strings, arrays, and objects; JSON numbers are not part of this common-record encoding.

This is not a new canonicalization of `intlify.config.json`, native owner results, or every Intlify artifact. In particular, 015's existing Resolver Construction Identity, Snapshot, disclosure, and ResourceBoundValue framing are unchanged. A Policy/Target `semanticDigest` is an opaque exact pin at this minimum boundary; computing it from a body remains deferred with that body's schema and semantic projection.

### Value framing revision `"0"`

Let `U64(n)` be an eight-byte unsigned big-endian integer. Every conversion to a length/count is checked. Define a frame as:

```text
F(tag, payload) = tag:u8 || U64(payload byte length) || payload
```

The complete canonical value function `C` is:

```text
C(null)      = F(0x01, empty)
C(false)     = F(0x02, empty)
C(true)      = F(0x03, empty)
C(string s)  = F(0x04, exact UTF-8 bytes of s)
C(array a)   = F(0x05, U64(element count) || C(a[0]) || ... || C(a[n-1]))
C(object o)  = F(0x06, U64(member count)
                     || C(key[0]) || C(value[0])
                     || ...
                     || C(key[n-1]) || C(value[n-1]))
```

Object keys are ordered by ascending unsigned UTF-8 bytes, with a shorter equal prefix first. Duplicate keys have already been rejected. Array order is preserved exactly. The owning semantic schema must establish a canonical sequence for a logical set before encoding; this function does not sort arrays, deduplicate elements, infer defaults, or erase `null`.

`0x00` and all unlisted tags are invalid. There is no padding, terminator, platform-endian integer, native pointer, or implicit field separator. All lengths count bytes, not Unicode scalar values or UTF-16 code units.

Small encoding vectors, written as hexadecimal, are:

| JSON value | `C(value)`                           |
| ---------- | ------------------------------------ |
| `null`     | `010000000000000000`                 |
| `false`    | `020000000000000000`                 |
| `true`     | `030000000000000000`                 |
| `""`       | `040000000000000000`                 |
| `"a"`      | `04000000000000000161`               |
| `[]`       | `0500000000000000080000000000000000` |
| `{}`       | `0600000000000000080000000000000000` |

### Domain-separated digest function

For a registered ASCII domain string `D` and admitted value `V`:

```text
H(D, V) = SHA-256(
  UTF8("intlify.shared-json.v0") || 0x00 || C(D) || C(V)
)
```

Its JSON presentation is `sha256:` followed by the complete lowercase hexadecimal digest. The fixed prefix, zero byte, domain string frame, and value frame are all included. A body field cannot select the algorithm or substitute another digest domain.

This minimum registers these uses:

| Domain `D` | Exact input value |
| --- | --- |
| `verification-record-integrity` | The complete `{ envelope, body }` record with only `envelope.integrityDigest` omitted |
| `measurement-case-identity` | `{ governingSpecification, identitySchemaRevision, projection }`, where the specification is `intlify-design-026` / `"0"`, identity-schema revision is `"0"`, and `projection` contains exactly the 026 Measurement Case semantic dimensions |

A Measurement Case identity is presented as `mc0_` followed by the 64 lowercase hexadecimal digits of the second digest. Its projection is a closed type fixed by the adopted measurement schema. It excludes sample values, creation time, record/run instance identities, branch/path/worker identities, and the implementation revision being compared. Expected semantic observations and native owner case bindings are retained separately rather than substituted for that projection.

This subset introduces no universal semantic-result digest. Owner semantic observations keep their registered algorithm, framing, and value, and 026 determines which logical fields establish semantic equality. A common integrity digest must never be reused as semantic-result identity merely because both happen to be hashes.

### Self-exclusion and complete integrity

To produce a common record, establish the final identity and all stored fields, remove the single integrity member from an encoding view, compute `H`, and insert the resulting digest. On admission, validate the complete stored schema, build the same one-member-excluded view, recompute, and compare all digest bits.

Only that member is excluded. `recordIdentity`, producing-tool identity, governing/schema revisions, creation context when present, all body fields, nested records, native owner bindings, references, raw samples, reasons, and outcomes remain covered. A missing or `null` integrity member in the submitted record is invalid; omission is an internal hashing view, not an alternative wire format. A nested field named `integrityDigest` is not recursively removed.

Integrity covers canonical stored content. JSON whitespace, escape spelling, and object-member ordering are transport differences and do not change it. If a transport also needs exact file-byte integrity, that is a separate transport digest and must not replace this record digest.

The encoding is injective over the admitted value domain; SHA-256 is a digest, not collision-free equality. Implementations with both values available compare canonical content when checking conflicting identity claims rather than relying on digest equality alone. Rehashing altered observations does not prove they match the original owner result, fixture, or Run Plan. Those independent inputs must still be checked by 026.

## Version Admission and Failure Behavior

The following version domains remain independent even when their initial value is `"0"`:

| Domain | Selects |
| --- | --- |
| 015 configuration `schemaVersion` | Authoring schema through the constructed configuration authority |
| Reference `specificationRevision` | The owning Policy/Target semantic specification, checked when that artifact is admitted |
| Common `recordKind` + `recordSchemaRevision` | Physical envelope/body schema and codec |
| `governingSpecification.identity` + `.revision` | 026 semantics for the common record |
| Native owner schema/profile and Measurement Projection revisions | The exact source result and its lossless common mapping |
| Package/tool version | Producer implementation identity, not schema compatibility |

The initial common-record vocabulary contains exactly the four kind/schema/specification tuples listed above. A reader admits only the tuples for which it implements the complete registered schema and validator; an unimplemented tuple remains unsupported even if its envelope is recognized. Unknown kinds or revisions are unsupported, never interpreted as the newest known revision, accepted by dropping fields, or repaired by filling defaults. A syntactically valid reference to a future Policy revision may remain structurally representable; that does not authorize later artifact resolution to accept unsupported semantics.

A common reader follows this order:

1. Apply finite input limits and strictly decode JSON without losing duplicate or malformed input information.
2. Read the fixed envelope selector fields and find the exact local schema/codec tuple; no submitted schema URI is executed or fetched.
3. Validate the complete closed envelope and body, including canonical primitives, variant fields, required presence, and local-identity uniqueness.
4. Verify the self-excluding integrity digest and reject identity conflicts in the supplied record collection.
5. Resolve exact top-level and nested inputs through registered common or native owner validators.
6. Apply the 026 operation's semantic checks: profile/Plan/case correspondence, all required attempts, sample eligibility, checksum/work matching, environment, reasons, and report consistency.

The decoder returns a typed failure, not a partial admitted record, when one of its steps cannot complete. When a 026 evaluator is available, it records unavailable or invalid inputs with the applicable common reasons (`schema-invalid`, `integrity-digest-mismatch`, `inconsistent-record`, and the other 026-defined causes). A malformed submitted record is not reused as the envelope of that evaluation. Unsupported input is not itself proof of structural corruption; 026 retains that distinction in `EvaluationInputResolution`.

This subset has no automatic migration or compatibility negotiation. A future migration needs its own explicit source/target schemas, semantic preservation rules, and fixtures. It must produce new record identities and integrity rather than editing an immutable record in place. This is a deferral of migration features, not permission for silent downgrade today.

## Performance and Storage Requirements

The [026 Performance Implementation Architecture](./026-intlify-conformance-and-measurement-design.md#performance-implementation-architecture) applies to these helpers:

- Decode under bounded input before allocating a second full representation; avoid a stringify/parse cycle just to check a reference.
- Validate shared immutable reference values once at the owning admission boundary and borrow them afterward where their lifetime permits.
- Stream canonical frames into the digest after checked length calculation, or use bounded reusable scratch outside the measured component interval. Do not require a full canonical byte copy in every record or sample.
- Establish canonical object/member order without depending on randomized map iteration. Do not add global interning, caches, arenas, custom hash tables, SIMD, or unsafe code merely to implement this encoding.
- Generate record identities, encode/projection/report records, and verify digests outside 015's existing measured intervals. This work does not change the semantic operation or its checksum.
- Keep measurement collectors, random-ID production, fixtures, and report code out of the ordinary `intlify_config` dependency path. Configuration reference types must not depend on the benchmark stack.

Decoded retained records own their required content or share an explicit immutable owner. No retained reference may borrow resettable scratch, a temporary serializer buffer, a file mapping whose lifetime has ended, or the next invocation's workspace.

## Conformance and Fixtures

Implementations must materialize machine-readable schemas and fixtures for the adopted subset. The following are required acceptance checks, not an assertion that those implementation artifacts already exist:

| Area | Required fixtures |
| --- | --- |
| Configuration references | Every Policy kind and Target Profile reference; all required fields; unknown/duplicate members; wrong primitive types; invalid identity/digest spelling; distinct revision and specification-revision changes |
| 015 presence and ownership | Five required policy references, both required nullable slots, missing versus `null`, wrong declared kind retained for the owning semantic check, and Target ID distinct from artifact identity |
| Generated schema | Draft 7 validation agrees with typed structural admission; full 015 root works without test reference placeholders; regeneration has no unexpected diff |
| Value framing | Frozen exact bytes for every tag, empty values, nested objects/arrays, multibyte keys/values, object-order permutations, array-order changes, escape equivalence, and absent versus explicit `null` |
| Exact values and limits | `"0"`, positive minima, `"9007199254740992"`, `"18446744073709551615"`, first-over, invalid numeric spellings, and each decoder capacity bound/first-over |
| Digests | Frozen preimage and SHA-256 known-answer vectors from an independent implementation; distinct digest domains; no truncation; exactly one excluded member; changed identity, creation context, nested digest, sample, reason, or owner binding changes integrity |
| Instance and Case identity | Fresh run/record identities but stable case projection; retained record round-trip keeps IDs; case-dimension changes alter Case identity; timing, tool implementation revision, and instance IDs do not |
| Reference admission | Missing parent/child, duplicate local identity, wrong record kind, unsupported tuple, conflicting content under one ID, cross-run/case rebinding, and source evaluations whose negative outcome remains negative after resolution |
| Owner provenance | Native checksum algorithm/framing and complete result retained unchanged; unsupported/lossy owner projection rejected; a rehashed altered result still fails against independent owner/fixture inputs |
| Integrated measurement | Planned run to retained owner result to common evidence/evaluation to structured report and revalidation, including measured, incomplete, invalid, missing, unsupported, and partial-observation cases required by 026 |

Byte-framing tests must not compute their expected bytes with the same encoder under test. The same rule applies to digest and schema expectations. Owner-only round trips and envelope-only validation are not substitutes for the integrated 026 path.

## Adoption in the 015 Minimum Implementation

This document introduces no new implementation Phase or separate prerequisite to complete all of 017.

| Adopting work | Required use of this design |
| --- | --- |
| 015 Phase 1 — Configuration Foundation | Replace test-only reference type instantiations with the formal closed types; generate the complete configuration schema and validate freshness/structural equivalence |
| Initial measurement for Phase 1 | Materialize the four adopted common record schemas, the registered native-result adapter and projection, exact quantities, identities, canonical framing, and end-to-end admission/report fixtures |
| Limited Phase 2/3 canonicalization and locale core | Reuse that same measurement path with the active owner dimensions and finite provider/data bindings; do not introduce a second record format |

The reference-schema definition and record framing remove those specific design dependencies. They do not by themselves finish schema generation, owner projection, harness/CI integration, or any 015 Phase. Complete checked-profile artifacts, Policy/Target body admission, and full revision-`"0"` resolver conformance remain outside the minimum implementation claim.

## Decision Log

| ID | Decision | Rationale |
| --- | --- | --- |
| 017-001 | Limit this revision's detailed scope to the 015 configuration-reference and initial measurement dependencies | Enables the minimum implementation without designing unrelated shared artifact families |
| 017-002 | Encode each configuration reference as one closed five-field exact tuple | Preserves 015's identity and presence semantics without embedding bodies or acquisition behavior |
| 017-003 | Keep record instance, run, case, integrity, and native owner semantic observations separate | Prevents measurement metadata or a locally computed checksum from becoming another kind of authority |
| 017-004 | Use typed length-framed canonical values, full domain-separated SHA-256, and exactly one self-excluded integrity member | Makes record integrity deterministic without self-reference or serializer-dependent JSON text |
| 017-005 | Admit only explicitly registered versions and retain owner schemas/algorithms | Avoids silent migration and allows existing owner results to remain independently verifiable |

## Deferred Follow-Up Notes

These subjects remain assigned to 017 but are not prerequisites for the scoped configuration and observational measurement path:

- complete source-first message, reference, candidate, dependency, library, Store, Release, and target artifact schemas;
- full Profile, construction-authority, Snapshot, canonicalization-data, binding, and Finding/Evidence representations;
- Policy/Target body schemas, semantic digest projections, and their trust/admission integration;
- measurement capabilities not adopted by the minimum, including comparison/budget, qualification, profiling, campaigns, and cross-platform reports;
- general semantic-result identity, alternate physical encodings, transport containers, registry distribution, and cross-version migrations.

Any future extension must state its owning semantics and schema-version impact. These notes do not authorize an open extension map or an unsupported success path in the current subset.

## Relationship to Other Documents

| Document | Relationship |
| --- | --- |
| [000 — Intlify overview](./000-intlify-overview-design.md) | Assigns 017's wider artifact and version-admission responsibilities; only the minimum subset is specified here |
| [015 — Project profile and locale policy](./015-intlify-project-profile-and-locale-policy-design.md) | Owns the reference tuples, configuration use sites, minimal implementation boundaries, and resolver semantics implemented using these encodings |
| [018 — Security, trust, and provenance](./018-intlify-security-trust-and-provenance-design.md) | Owns trust/authentication; digest and schema success supply neither |
| [026 — Conformance and measurement](./026-intlify-conformance-and-measurement-design.md) | Owns the adopted records' semantic fields, projections, admission outcomes, and performance/storage requirements |
| [029 — Product workflow and packaging](./029-intlify-product-workflow-and-packaging-design.md) | Owns public workflow, artifact/schema acquisition, publication, and packaging |
