# Intlify Shared Artifact and Version Admission Design

**Intlify Shared Artifact and Version Admission Specification Revision:** `"0"`

## Purpose

This design fixes the minimum shared representations needed by [015](./015-intlify-project-profile-and-locale-policy-design.md)'s configuration/locale foundation, its initial [026](./026-intlify-conformance-and-measurement-design.md) measurement path, and [016](./016-intlify-source-authoring-and-intent-identity-design.md)'s source-authoring and persistent-identity implementation.

There are three independently adopted uses:

- **Configuration:** encode an exact Policy or Target Profile reference so `intlify_config` can generate and validate the complete `intlify.config.json` schema without test-only reference placeholders.
- **Measurement:** retain an owner result, identify and validate common records, and follow exact references from a planned run through observations and evaluations to a structured report.
- **Source authoring:** encode an owner-qualified Intent ID, a reproducible semantic revision, source/declaration/reference facts, and exact-base registry updates. This supplies 016's Phase 1–3 foundation and a minimal Intent/reference handoff, not the complete downstream artifact system.

The configuration path establishes the shape of a reference, not the validity of its referenced artifact. The measurement path establishes the representation and integrity of a record, not whether its result is successful or its runner is trusted. The authoring path encodes 016's facts and identity decisions; it does not prove source approval, authorize registry publication, or establish executable target compatibility. Those decisions remain with the owning specifications.

This revision specifies only these explicitly scoped shared subsets. It does not complete every artifact family assigned to 017 by [000](./000-intlify-overview-design.md), nor does adopting it establish complete 015 Profile Specification revision-`"0"` support.

The existing 015 adoption scope remains Phase 1 plus only the finite, test-owned canonicalization and private locale-core slices of Phases 2 and 3. Defining the complete configuration structure does not require resolving every referenced Policy or Target Profile body in that minimum slice. The additive 016 scope has its own artifact-kind/schema tuples; it changes neither the existing configuration references nor the four measurement record schemas or their digests.

## Goals

- Fix the JSON representation of `PolicyReference` and `TargetProfileReference` used by configuration schema version `"0"`.
- Separate reference structure, supported-version admission, referenced-content validation, and trust.
- Fix common verification envelopes, top-level and nested record references, exact quantities, and canonical digest framing for initial measurement records.
- Keep record-instance identity, integrity, Measurement Case identity, semantic observations, and native owner-result provenance distinct.
- Allow the initial 015 implementation to use these definitions without implementing Intent artifacts, a public checked-profile format, deployment artifacts, or a general migration system.
- Fix only the authoring representations needed to implement and test 016's adopted rules without path/text-derived persistent IDs or implementation-defined revision hashes.
- Retain exact source, input, base, decision, and result bindings while keeping decoding, semantic validation, and publication authorization separate.

## Non-Goals

- Defining Policy bodies, Target Profile bodies, artifact acquisition, bootstrap trust, signatures, or authorization.
- Encoding the full `LocalizationProjectProfile`, its binding sidecar, Resolver Construction Root Package, Programmatic Entry Snapshot, Finding/Evidence model, or canonicalization-data artifacts.
- Redefining 016's recognition, semantic-revision, identity-continuity, or automatic-update rules.
- Completing `SourceLocaleMessageArtifact`, localized message artifacts, `LibraryManifest`, Store records, Release artifacts, target output, or Runtime ABI.
- Defining a general authoring-result/diagnostic protocol, project graph, module acquisition, host lowering, or registry update commands.
- Replacing 026's measurement semantics, owner benchmark schemas, logical work, checksum algorithms, or component interval definitions.
- Adding comparison, budget, runner-qualification, profiling, cross-target, or Release capabilities to the initial observational measurement scope.
- Defining public crate/API names, schema distribution paths, registry services, binary transport, or migration commands.

## Ownership and Dependencies

| Owner | Responsibility in this subset |
| --- | --- |
| 015 | Configuration members and presence, field-role expectations, structural versus semantic admission, selection, locale behavior, resource limits, and the active owner measurement boundaries |
| 016 | Source recognition and extraction, MF2/context projection meaning, parameter requirements, continuity/newness/absence checks, and valid identity decisions |
| 017 | Shared reference JSON, verification and authoring representations, identity domains, canonical framing, integrity coverage, and exact-version selection |
| 018 | Artifact authentication, provenance, trust, and authorized evidence use; an integrity digest never supplies these decisions |
| 019 | Complete project queries, shared diagnostic reporting, dependency storage, and incremental orchestration; the minimum authoring inventory below is not a project graph |
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
| Owner-qualified Intent ID | The complete application/library owner and opaque owner-local value identifying one message lineage; distinct from artifact integrity and a target-local Message Handle |
| Authoring artifact reference | Exact kind, schema, authoring specification, and complete-content digest for one immutable authoring artifact; not a configuration Policy reference or verification-record identity |

## Design Overview

| Input | Shared operation defined here | Owning operation that follows |
| --- | --- | --- |
| Policy/Target reference in configuration | Closed reference decoding | 015 structural admission, followed later by exact artifact resolution and owner body checks |
| Native benchmark result | Preserve its owner-qualified identity and native integrity information | Owner validation and the registered 026 Measurement Projection |
| Common verification record | Decode, select the exact schema, verify integrity, and resolve typed references | 026 case, sample, run, projection, and report validation |
| Declaration and reference facts | Encode the Intent projection, stable ID, and source/evaluation evidence | 016 semantic validation; later planning and lowering retain their own checks |
| Registry snapshot and update plan | Bind exact base, inventory, decisions, and immutable result | 016 association checks, 018 authorization, and 029 atomic publication |

017 does not add an executable compiler phase. Its representations are adopted by the relevant 015/016 operations and by measurement alongside each active measured boundary.

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

## Minimum Intent Authoring Representation

This additive subset encodes 016's logical rules. It is intentionally limited to declaration semantics, local finite references, source evidence, and persistent registry history. It does not complete 016 Phases 4–5, admit every host/import form, or mark 016 decisions still recorded as `Proposed` as accepted.

The notation below defines closed JSON objects, not Rust structs or public authoring APIs. Every shown member is required unless marked `?`; arrays are present even when empty. `Text` is a Unicode-scalar string, `NonemptyText` excludes the empty string, and counts, indexes, and byte positions use the existing `UInt64` string representation. All sizes, recursion, collection counts, and reference-resolution work are subject to explicit caller-supplied finite limits. Unknown members/variants are not extension points.

### Identity and artifact envelope

```text
OwnerIdentity = { kind: "application" | "library", identity: IdentityToken }
MessageIntentId = { owner: OwnerIdentity, value: Opaque128 }
Opaque128 = exactly 32 lowercase hexadecimal digits

AuthoringArtifact<K, B> {
  kind: K
  schemaRevision: "0"
  authoringSpecification: { identity: "intlify-design-016", revision: "0" }
  body: B
  integrityDigest: IntegrityDigest
}

AuthoringArtifactReference {
  kind: registered authoring artifact kind
  schemaRevision: RevisionToken
  authoringSpecification: VersionedIdentity
  integrityDigest: IntegrityDigest
}
```

An application's owner identity is its checked 015 `projectId`, not its configuration-scoped Profile ID, package name, Selection Scope, or path. A library owner identity is supplied by its separately admitted library context. The same identity token under the two owner kinds denotes different owners; spelling is not proof of publisher identity or authorization.

An update host obtains an owner-local ID once from 16 fresh bytes of operating-system cryptographic randomness, retains the lowercase hexadecimal value in the update plan, and rejects collisions against that owner's active and retired IDs. Failure to obtain randomness or a collision is an explicit allocation failure requiring a new candidate/plan; no timestamp, source hash, traversal-order, or unchecked fallback is permitted. Replaying a plan never generates the value again. These internal IDs need not be written by developers or used as generated target handles; display abbreviations do not change stored identity.

Intent equality compares `(owner.kind, owner.identity, value)` completely. Canonical ordering uses those fields in that order, with unsigned UTF-8 byte comparison. `IntentRevision` uses the complete `SemanticDigest` spelling defined below, not this random-ID domain.

The new kind/schema/specification tuples are exactly:

| Kind | Closed body | Use |
| --- | --- | --- |
| `authoring-inventory` | `AuthoringInventory` | One finite source-analysis scope before persistent identity assignment |
| `message-intent` | `MessageIntentBody` | One declaration's persistent identity, revision, and source facts |
| `message-reference` | `MessageReferenceBody` | One use site's exact finite references and parameter-expression evidence |
| `intent-registry` | `IntentRegistrySnapshot` | One immutable owner/scope registry state |
| `intent-registry-update` | `IntentRegistryUpdate` | One fully specified candidate update against one exact base |

All five initially use schema revision `"0"` and authoring specification `intlify-design-016` revision `"0"`, denoting the adopted 016 rule set. A kind selects the complete body schema; there is no arbitrary `B` at runtime. Existing readers without these tuples report unsupported input. Adding them does not extend a measurement-kind enum or reinterpret a verification record as an authoring artifact.

Artifact integrity is `H("authoring-artifact-integrity", artifact-with-only-top-level-integrityDigest-omitted)`, using the existing framing below. The complete kind, schema, specification, and body are covered. Only that top-level member is excluded; a missing or `null` stored digest is invalid. An artifact reference compares all its fields and must resolve to exactly that admitted artifact. Reference ordering is kind, schema revision, authoring-specification identity/revision, then digest, using unsigned UTF-8 bytes. Equal digests do not merge distinct Intent IDs, and different source evidence may produce different artifact digests for the same Intent ID/revision. This digest is not the source/localized-message `ArtifactDigest` consumed by Store selection or Release binding in 000; that later artifact family remains deferred.

### Semantic projection and IntentRevision

```text
IntentProjection {
  mf2Specification: VersionedIdentity
  message: MessageProjection
  sourceLocale: NonemptyText
  parameters: Text[]
  usage?: { profile: VersionedIdentity, value: NonemptyText }
  description?: NonemptyText
}

MessageProjection { declarations: Declaration[], body: MessageBody }
Declaration =
  { kind: "input", name: Text, function?: Function, attributes: Attribute[] }
  | { kind: "local", name: Text, expression: Expression }
MessageBody =
  { kind: "pattern", parts: PatternPart[] }
  | { kind: "match", selectors: Text[], variants: Variant[] }
Variant { keys: VariantKey[], parts: PatternPart[] }
VariantKey = { kind: "literal", value: Text } | { kind: "catch-all" }
PatternPart =
  { kind: "text", value: Text }
  | Expression
  | {
      kind: "markup", form: "open" | "close" | "standalone",
      name: Text, options: Option[], attributes: Attribute[]
    }
Expression { kind: "expression", operand?: Value, function?: Function, attributes: Attribute[] }
Value = { kind: "literal", value: Text } | { kind: "variable", name: Text }
Function { name: Text, options: Option[] }
Option { name: Text, value: Value }
Attribute { name: Text, value?: Text }
```

The local semantic-profile registry binds `mf2Specification` to the exact supported syntax and 001/002/012 parser-owned semantics. Its implementation and independent fixtures must be pinned before that tuple is admitted; a package version or successful JSON decode is not a substitute. The projection schema is `intlify-intent-projection` revision `"0"`. It encodes the adopted MF2 structures above without persisting parser node IDs, CST tables, host ASTs, or a second selector-validation implementation.

Projection construction follows these rules:

- Parse and validate through 012 first. Reconstruct the structured message from parser-owned facts and read-only CST views; invalid or unsupported syntax has no admitted projection. An expression must have an operand, a function, or both. Declaration bindings, selector annotation, variant arity/fallback, and option validity remain parser-owned checks.
- Decode host escapes before MF2 parsing, then preserve actual MF2 text and literal values after MF2 escape decoding. Quoted/unquoted literals with equal decoded values have the same representation. Numeric-looking literals remain strings; there is no host number conversion, trimming, dedenting, whitespace compression, or Unicode normalization of literal content, including literal variant keys.
- Names and identifiers use parser-owned semantic names, without syntactic sigils; name matching and resolution retain 002/012's rules. Do not substitute a normalized matching key for the original decoded text/literal value. In particular, a parser's normalized variant-comparison key is not the revision's literal payload.
- Merge adjacent text parts and omit empty text parts. Simple versus quoted-pattern wrappers do not add a node. Preserve declaration, selector, variant, option, attribute, and pattern-part order; preserve local names, initializer structure, and references. Do not alpha-rename, expand expressions, reorder branches, or infer equal output for all inputs.
- `parameters` is the duplicate-free, unsigned-UTF-8-byte-sorted set of external names required by 016's checked message analysis, not the host parameter object's values or order. The structured message retains the symbolic selector/function/options requirements; this subset does not encode a second runtime type system.
- `sourceLocale` is the exact canonical locale already established by 016 using its admitted locale inputs. `usage`, when present, carries the exact bounded semantic-usage profile and its value; a coverage class or DOM tag cannot substitute for that profile. The initial optional semantic metadata is `description`; richer structured constraints require a new explicitly adopted projection revision, not an open metadata object.

The exact revision calculation is:

```text
IntentRevision = H("intent-semantic-revision", {
  projectionSpecification: { identity: "intlify-intent-projection", revision: "0" },
  projection: IntentProjection
})
```

The persistent Intent ID, host grammar/profile, source positions, original host spelling, reference count, `surfaceClass`, default-versus-explicit locale evidence, policies, glossary, target, Provider, and locale-data revisions are not additional revision fields. Their applicable evidence/dependencies remain separate. A change to the admitted MF2/projection specification is explicit, not an implementation silently changing an existing digest's meaning.

Admission reconstructs this projection from retained MF2 source and the admitted 016 context, then compares both the complete projection and revision. It does not trust a caller's precomputed digest, external-name list, or `usage` claim. Equal revision digests with unequal available projections are a conflict, not deduplication. Literal whitespace, local renames, branch order, context changes, and host-only changes follow 016's independent equality/change vectors.

### Source facts and bounded inventory

```text
ExactInputBinding { identity: IdentityToken, revision: RevisionToken, semanticDigest: SemanticDigest }
ByteRange { start: UInt64, end: UInt64 }
SourceSnapshot {
  owner: OwnerIdentity, unit: IdentityToken, revision: RevisionToken,
  grammar: VersionedIdentity, byteLength: UInt64, utf8Digest: IntegrityDigest
}
Occurrence {
  source: SourceSnapshot, range: ByteRange,
  role: "ui-literal" | "intent-literal" | "mf2-declaration"
        | "reference" | "parameter-expression" | "exclusion"
}
AuthoringBasis {
  authoringProfile: VersionedIdentity
  contextKind: "application-profile" | "library-context" | "test-context"
  context: ExactInputBinding
  surfaceVocabulary: ExactInputBinding
  localeCanonicalization: VersionedIdentity
  localeData: { identity: IdentityToken, semanticDigest: SemanticDigest }
  defaultSurfaceClass?: NonemptyText
}
DeclarationFacts {
  occurrence: Occurrence
  mf2Source: Text
  projection: IntentProjection
  sourceLocaleBasis: "explicit" | "context-default"
  surfaceClass: NonemptyText
  extractionMap: { extracted: ByteRange, source: ByteRange }[]
}
ParameterBinding { name: Text, expression: Occurrence }
ReferenceFacts {
  occurrence: Occurrence
  declarations: Occurrence[]
  parameters: ParameterBinding[]
}
AuthoringInventory {
  owner: OwnerIdentity, scope: IdentityToken, basis: AuthoringBasis
  completeness: "complete" | "partial"
  units: { source: SourceSnapshot, outcome: "checked" | "blocked" | "failed" }[]
  declarations: DeclarationFacts[]
  references: ReferenceFacts[]
  exclusions: { occurrence: Occurrence, reason: NonemptyText }[]
}
```

`utf8Digest` is the complete lowercase `sha256:` digest of the exact source bytes, not `H` of reserialized text. Admission receives those already acquired immutable bytes and checks length, digest, owner, grammar, and revision association. A source unit token is a caller-supplied identity within the owner; it is not a file path or persistent Intent ID. Equal owner/unit/revision tuples with inconsistent bytes or grammar are conflicts. A move may change the unit token without changing an established Intent identity.

Ranges are half-open UTF-8 byte ranges, bounded by the named source or extracted MF2 byte length and aligned to scalar boundaries wherever text is addressed. Empty insertion ranges and EOF are representable; actual declaration/reference ranges must identify the asserted syntax occurrence. Extraction segments are ordered by extracted start, non-overlapping, and cover the emitted MF2 bytes. Source ranges may repeat or cover several emitted bytes for escapes, and generated literal delimiters may use an explicit zero-width source position. A mapping never claims byte-for-byte correspondence merely from equal lengths.

`AuthoringBasis` retains exact dependency pins, not full Profile, vocabulary, or locale-data bodies. For `application-profile`, `context.identity` is the checked `projectId`, `context.revision` is its governing 015 Profile Specification revision, and `context.semanticDigest` is the complete checked profile's semantic digest. The vocabulary uses its exact artifact identity/revision/digest; canonicalization uses its specification identity/revision and the separate dataset identity/digest as defined by 015. No extra dataset revision or canonicalization-specification digest is invented here. Library/test contexts use their explicitly pinned owning input identity/revision/digest and corresponding validators, not an application default.

The caller supplies and validates the actual applicable inputs under 015/016 and checks each pin against those inputs. There is no new default Profile, configuration member, independent vocabulary resolver, or schema for the complete `LocalizationProjectProfile` here. `context-default` requires the corresponding present canonical default; a library never uses the consuming application's default. A `test-context` is admitted only by an explicitly test-only invocation and cannot satisfy production Profile or library admission. Changing that string does not turn a test input into checked production evidence.

An inventory is the declared finite scope of one analysis, not a complete serialized authoring outcome or a 019 graph. A `complete` label is checked against the caller's expected scope and source membership; it does not prove completeness by itself. Every declaration, reference, exclusion, and parameter occurrence must resolve against its actual supplied snapshot and checked unit, with the corresponding role; declaration facts admit only the three declaration roles. Blocked/failed units prevent complete authoring success; detailed diagnostics remain 016/019-owned and cannot be reconstructed as empty success from this record. A registry-update inventory requires every submitted unit to be checked. A separate partial invocation may analyze a smaller declared scope, but a failed complete attempt cannot simply erase its failed units and retain the complete claim.

Units are sorted by source-unit identity and contain at most one revision of each unit. Declaration/reference/exclusion arrays are sorted by occurrence: owner kind/identity, unit, revision, grammar identity/revision, source digest, numeric start/end, then role spelling. Duplicate or conflicting occurrences are rejected, not silently removed. All inventory sources belong to the declared owner. The first reference form names a nonempty finite set of declarations in this inventory, sorted by occurrence; cross-owner/library handoff requires a later explicit schema/profile extension. Parameter bindings preserve host property/evaluation order, have unique checked names, and retain source expressions rather than serialized runtime values.

### Minimal Intent and reference artifacts

```text
MessageIntentBody {
  intentId: MessageIntentId
  intentRevision: SemanticDigest
  inventory: AuthoringArtifactReference
  declaration: Occurrence
  registry: AuthoringArtifactReference
  continuity?: { from: Occurrence, basis: ContinuationBasis }
}
MessageReferenceBody {
  inventory: AuthoringArtifactReference
  occurrence: Occurrence
  targets: {
    intentId: MessageIntentId, intentRevision: SemanticDigest,
    intentArtifact: AuthoringArtifactReference
  }[]
}
```

The inventory reference must resolve to `authoring-inventory`. A message Intent's declaration selects exactly one `DeclarationFacts`; its ID must be active in the exact `intent-registry` reference, and its revision is computed from the current declaration's projection, not read from the registry. Without `continuity`, the declaration must exactly match that active association. With `continuity`, `from` must match it and the supplied basis must establish 016's one-to-one continuation to the current declaration. The same checks as update decisions apply, including competing claims across the current inventory. This permits read-only compilation to use checked continuity without secretly publishing a locator update; it cannot allocate or restore an ID absent from the active base.

The projection, original extracted MF2, locale, class, input pins, and minimum source evidence are supplied through the retained inventory. These are exact logical source definitions, not a registry of translations. A consumer must retain or be explicitly supplied the referenced inventory and source/context inputs; an unavailable dependency is not a self-contained checked message. A context-only semantic change can change the Intent revision while leaving the registry association unchanged.

A reference artifact selects one `ReferenceFacts` in that inventory. `targets` is the exact same finite declaration set after identity resolution, ordered by the complete Intent ID; each target resolves to a matching `message-intent` artifact, with no duplicate IDs or competing revisions for one ID. The referenced inventory retains parameter names, expressions, and evaluation order. Multiple uses of one declaration share its ID; equal text in separate declarations does not. This representation does not accept an unimplemented conditional form merely because its target array is finite; 016's applicable authoring rules and still-proposed choices remain explicit.

These initial artifacts support a source-analysis/identity handoff, not final reachability, delivery placement, source approval, or generated execution. Source-locale MF2 can be read deterministically from the Intent's declaration facts without a Provider. Encoding a complete `SourceLocaleMessageArtifact` with its distinct content/artifact identities, execution requirements, and source-admission provenance is deferred to the adopting Phase 4–5 work; neither the inventory nor its digest substitutes for that artifact.

### Registry snapshots and update plans

```text
RegistryEntry {
  intentId: MessageIntentId
  state: "active" | "retired"
  declaration: Occurrence
}
IntentRegistrySnapshot {
  owner: OwnerIdentity, scope: IdentityToken, registryIdentity: Opaque128
  base?: AuthoringArtifactReference
  update?: AuthoringArtifactReference
  entries: RegistryEntry[]
}
IntentRegistryUpdate {
  owner: OwnerIdentity
  base: AuthoringArtifactReference
  inventory: AuthoringArtifactReference
  decisions: IdentityDecision[]
  lineageLinks: {
    kind: "copy" | "split" | "merge",
    predecessors: MessageIntentId[], successors: MessageIntentId[]
  }[]
}
IdentityDecision =
  { kind: "continue", intentId: MessageIntentId, from: Occurrence, to: Occurrence, basis: ContinuationBasis }
  | { kind: "allocate", intentId: MessageIntentId, to: Occurrence, basis: AllocationBasis }
  | { kind: "retire", intentId: MessageIntentId, from: Occurrence, basis: { kind: "complete-absence" } }
  | { kind: "restore", intentId: MessageIntentId, from: Occurrence, to: Occurrence, basis: ExplicitBasis }
ExplicitBasis { kind: "explicit", reason: NonemptyText }
ContinuationBasis =
  { kind: "unchanged-snapshot" }
  | { kind: "verified-edit", profile: VersionedIdentity, changes: SourceEdit[] }
  | ExplicitBasis
AllocationBasis = { kind: "confirmed-new" } | ExplicitBasis
SourceEdit {
  before?: SourceSnapshot, after?: SourceSnapshot,
  replacements: { range: ByteRange, text: Text }[]
}
```

The base reference always names `intent-registry`; the update's inventory names `authoring-inventory`. A non-genesis snapshot has both `base` and `update`, referring to the exact previous snapshot and `intent-registry-update`. All owners and the declared owning scope must agree. One authoritative registry chain covers the owner's retained ID domain; an analysis scope or a second file cannot create a separate collision domain for the same owner.

A genesis snapshot omits both references and has empty `entries`. Its host-generated `registryIdentity` uses the same 16-byte random encoding in a separate registry identity role and remains unchanged throughout that chain. A new root requires a separately authorized initialization operation; a missing/corrupt registry never authorizes it. Recovery reuses the admitted retained identity and history, not a newly initialized chain presented as continuation. Neither a syntactically valid root nor a digest proves that a registry is current.

Entries are sorted by complete Intent ID and retain retired entries with their last declaration association. The registry records identity history, not the authority for a current semantic revision; retained inventories and Intent artifacts preserve the corresponding historical message facts. There is exactly one entry for each ID, all IDs belong to the registry owner, and active entries cannot assign two IDs to the same exact declaration. Decisions are sorted by Intent ID and contain at most one action per ID. Every `to` selects a checked declaration in the current inventory, with at most one assigned identity; every `from` must exactly match the named base entry. New IDs cannot collide with either state in the base.

The entry transition is closed:

| Action | Required base state | Result |
| --- | --- | --- |
| `continue` | Active entry with the same ID and exact `from` | Active entry at `to`; the ID is unchanged |
| `allocate` | No active or retired entry for this ID | New active entry at `to` |
| `retire` | Active entry with exact `from`, proven absent under 016's complete-owning-inventory rules | Retired entry retaining its last declaration association; no history deletion |
| `restore` | Retired entry with exact `from` and an explicit restoration decision | The same historical ID becomes active at `to` |

Unchanged base entries are copied exactly. Every current declaration in the admitted inventory must either retain an exact active association or be covered by one checked `continue`, `allocate`, or `restore` action. An uncovered new/changed association or unresolved identity choice prevents an applicable update, not just the affected entry. Partial inventories may update proven covered associations but cannot authorize retirement or discard unseen entries. Failed units cannot be used as evidence of newness, continuity, or absence.

For `unchanged-snapshot`, `from` and `to` must be exactly equal and name the actual unchanged snapshot. For `verified-edit`, the declared profile selects a supported 016 continuity verifier, not arbitrary executable code. Every `SourceEdit` has at least one of `before`/`after`; those references resolve to the exact old/current bytes. Replacement ranges use the original `before` coordinates, are ordered and non-overlapping, and replay in that order without offset reinterpretation. Two zero-width insertions at the same position are combined before recording. Replaying them must produce the exact `after` bytes; an absent `before` denotes an empty starting buffer, and an absent `after` requires an empty result and absence from the current complete membership where removal is claimed. Units cannot occur twice on either side of the change list.

The change list is ordered by the before/after source tuples, absence before presence. A source tuple compares owner kind/identity, unit, revision, grammar identity/revision, digest, and numeric byte length. It must cover the affected source snapshots and agree with the decision's declaration pair. Replaying an edit proves byte correspondence only: the 016 verifier must additionally establish its accepted one-to-one declaration continuation and reject competing claims/copies. A supplied mapping, successful diff replay, or `verified-edit` label alone is not proof. An unsupported verifier/profile or insufficient history blocks automatic continuation; the host may obtain an explicit decision without weakening any base/source checks.

Similarly, `confirmed-new` requires the independent 016 newness check; inability to find a continuation is not that check. `complete-absence` requires a successfully analyzed complete owning inventory and resolved identity choices. `explicit` records the requested historical choice and explanation, not actor authorization. The host must have the applicable explicit decision bound to this exact base, inventory, and action, and later publication must be authorized for the complete plan. An automatic-development setting is never inferred from these wire labels.

Lineage links retain copy/split/merge intent without assigning identity or approval. Their nonempty predecessor/successor sets are sorted by complete ID, and links are sorted by kind and those sets. IDs must resolve in the base and resulting snapshot respectively; duplicate links are invalid. A copy has one predecessor and one distinct newly allocated successor, a split one predecessor and at least two distinct successors, and a merge at least two distinct predecessors and one successor. Any continuing predecessor is still subject to the same one-action/one-current-declaration rules. Links neither authorize retirement/restoration nor transfer translation approval.

### Non-circular history and publication

Construct and validate artifacts in this order:

1. Retain the current inventory and exact base registry. The inventory contains no assigned IDs or reference to a future registry.
2. Fix all decisions, including host-supplied new ID values, and encode the update plan. The plan references the base and inventory, not its future result.
3. Apply that complete plan deterministically, create a new registry snapshot referencing the base and plan, and compute its integrity digest. It retains the same owner, scope, and registry identity. If both decisions and lineage links are empty, do not publish a new registry; reuse the existing snapshot. Refreshing semantic revisions or source-analysis evidence alone does not require an identity update.
4. The host may publish only if the exact base is still current. The current-base check and replacement form one atomic operation under 029 and the applicable 018 authorization. A stale base requires revalidation/replanning; no partial publication or last-writer-wins merge is permitted.
5. Subsequent read-only compilation consumes the accepted immutable snapshot and emits Intent/reference artifacts. Retained result references do not mutate the plan or create a plan/result digest cycle.

A reader replays a retained update from its admitted base and inventory and compares the complete resulting snapshot, not just its digest. Base snapshots used as trusted starting points still require the applicable admission; an untrusted self-consistent chain does not establish authorization or currentness. History traversal is explicit and bounded. Required source snapshots, context inputs, or bases that are unavailable prevent that replay; an old locator or matching message text is not recovery evidence.

### Authoring admission and fixtures

The authoring reader first performs bounded strict decoding, selects the exact kind/schema/specification, validates the entire closed body, and verifies integrity. It then resolves the required finite artifact/source/input collection and performs 016's semantic, source-association, completeness, and registry-transition checks for the requested operation. Repeated artifact submissions are duplicate input; conflicting content under an equal reference is an identity conflict. Repeated references to one retained artifact are valid and do not require duplicate stored artifacts. It fetches no file, schema, registry, plugin, or network resource. A digest pin or `contextKind` cannot replace a missing checked input. Source/edit text, descriptions, and explicit-decision reasons remain untrusted potentially sensitive content, not executable instructions or default telemetry/profiler labels.

Structural/integrity admission, checked authoring facts for a declared scope, and authorization to publish are separate results. Unsupported versions/verifiers, invalid fields, missing inputs, inconsistent references/projections, identity conflicts, and exceeded limits remain distinct typed failures; 016/019 own their diagnostic projection. There is no new general diagnostic envelope here. In particular, a valid partial inventory is not a complete build input, and a plan with unresolved choices is not a publishable `intent-registry-update`.

The adopting implementation must materialize closed Draft 7 schemas for the five registered kinds, the standalone projection, and their referenced value types, together with independent fixtures. File paths and public package names remain implementation/029 choices. Required fixture groups are:

| Area | Required independent expectations |
| --- | --- |
| IDs and domains | Exact 128-bit spelling, owner-kind separation, same local value under different owners, active/retired collision, allocation failure, stable replay, and registry ID distinct from Intent ID |
| Projection | Literal/host escape equivalence, exact spaces/newlines and canonically equivalent but byte-distinct literal Unicode, names versus literal matching keys, local declarations, branch/order changes, function/options/attributes/markup, external-name requirements, and 016's context-versus-policy/class distinctions |
| Revision and integrity | Frozen canonical preimages and full SHA-256 answers from an independent implementation; domain separation; source/evidence changes affecting artifact integrity without changing semantic revision; equal digest claims with unequal available content |
| Source and scope | UTF-8/UTF-16 conversion, CRLF, multibyte/EOF/zero-width mapping, forged ranges/digests/roles, duplicate units/occurrences, missing snapshots, partial membership, failed units, and test inputs rejected by production admission |
| Intent/reference handoff | Shared declaration versus equal text, exact target ID/revision/artifact matching, finite-set equality, parameter evaluation order, unavailable inventory/context, and no automatic promotion to source approval, reachability, or execution conformance |
| Registry | Genesis versus recovery, explicit edit/move and verified edit replay, rejected ambiguous/unverifiable continuity, newness versus uncertainty, copy/split/merge/restore, complete-only retirement, tombstone retention, stale bases, and non-circular exact update replay |
| Storage and failure | Exact/first-over bounds, indexed lookup, failure/cancellation followed by workspace reuse, no borrowed resettable storage in retained artifacts, deterministic ordering, and no mutation from ordinary compilation |

Schema round trips alone do not prove these semantics. JSON examples with invented digest pins may test shape only; integrity/replay fixtures must use actual retained inputs and independently computed expectations. Source/revision comparison fixtures must not use a formatter's output or parser debug dump as the expected canonical representation.

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

Canonical encoding here operates on the **complete schema-admitted JSON value**, not source bytes, debug output, hash-table iteration, or a host serializer's incidental order. All quantities have already become exact decimal strings. Allowed values are `null`, booleans, Unicode-scalar strings, arrays, and objects; JSON numbers are not part of this encoding. The existing verification-record domains and the explicitly added authoring domains use the same unchanged value framing.

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
| `authoring-artifact-integrity` | The complete `AuthoringArtifact` with only its top-level `integrityDigest` omitted; no nested member is excluded |
| `intent-semantic-revision` | `{ projectionSpecification, projection }` with exactly the projection specification and `IntentProjection` defined in Minimum Intent Authoring Representation |

A Measurement Case identity is presented as `mc0_` followed by the 64 lowercase hexadecimal digits of the second digest. Its projection is a closed type fixed by the adopted measurement schema. It excludes sample values, creation time, record/run instance identities, branch/path/worker identities, and the implementation revision being compared. Expected semantic observations and native owner case bindings are retained separately rather than substituted for that projection.

These uses introduce no universal semantic-result digest. Owner semantic observations keep their registered algorithm, framing, and value, and 026 determines which logical fields establish semantic equality. The Intent revision domain is limited to the specified 016 projection. A common integrity digest must never be reused as semantic-result identity merely because both happen to be hashes.

### Self-exclusion and complete integrity

The following self-exclusion rule is unchanged for verification records. Authoring artifacts use the separately specified top-level exclusion and digest domain above.

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
| Authoring `kind` + `schemaRevision` + `authoringSpecification` | One complete registered authoring body/codec and the adopted 016 rules; independent of measurement-kind support |
| Intent projection and MF2 specification revisions | The exact semantic projection and parser-owned meaning used to compute an Intent revision |
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
- Authoring/registry codecs reuse the small framing primitives without depending on measurement collectors or `intlify_config`'s benchmark implementation. New Intent/registry random values are supplied by the authorized host outside read-only authoring operations.
- Build bounded indexes for artifact references, source snapshots, occurrences, and registry IDs once per admitted collection. Decode/parse a shared source or projection once where its exact inputs permit reuse; do not resolve every reference by scanning all artifacts or repeatedly walk the entire ancestry for each declaration.
- Preserve identity distinctions while sharing immutable source/projection storage. Measure projection/revision encoding, artifact integrity, and registry replay separately from file acquisition/publication, under the applicable 016/026 operation definitions. No new numeric performance threshold is introduced here.

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

## Adoption in the 016 Implementation

The authoring additions are adopted within 016's existing phases, not a new 017 implementation sequence or a requirement to finish the wider artifact system first.

| Adopting work | Minimum use of this design | Remaining prerequisites |
| --- | --- | --- |
| Phase 1 — Shared authoring semantics | Implement the closed projection and revision encoder with independent equality/change vectors and pinned parser semantics | Explicit finite test-owned context may be used under 016; richer constraints and portable execution semantics are not inferred |
| Phase 2 — Initial JS/TS Producer | Implement source snapshots, ranges/maps, declaration/reference facts, inventory completeness, and input binding checks | 016 owns recognition/profile decisions and the actual host/parser checks; serializable facts are not proof of generated host behavior |
| Phase 3 — Persistent identity and reconciliation | Adopt owner/local IDs, the registry/update schemas, exact replay and collision rules, and minimal Intent/reference artifacts against accepted associations | Production use requires the necessary checked 015 inputs, actual supported continuity verifiers, applicable 018 authorization, and 029 host exact-base/atomic publication; test doubles do not satisfy these |
| Phase 4–5 — Broader handoff and integration | Reuse the established identity/revision/source basis, extending only the missing artifact families with their own exact schema revisions | Cross-owner/library references, 019 graph/diagnostic handoff, complete source-locale artifacts, 020 planning, and 023/024/028 execution/lowering remain separately adopted work |

Thus the minimum lets an implementation parse source, construct reproducible message facts, exercise persistent identity history, and exchange the initial local artifacts without inventing its own ID or revision format. It does not declare all of Phase 3 complete merely because codecs exist. Existing configuration and measurement implementations need not adopt these new kinds until they consume authoring artifacts.

## Decision Log

| ID | Decision | Rationale |
| --- | --- | --- |
| 017-001 | Initially limit detailed scope to the 015 configuration-reference and initial measurement dependencies; add the independently adopted 016 subset below | Preserves the existing minimum's formats and implementation scope while allowing the next consumer's necessary representations |
| 017-002 | Encode each configuration reference as one closed five-field exact tuple | Preserves 015's identity and presence semantics without embedding bodies or acquisition behavior |
| 017-003 | Keep record instance, run, case, integrity, and native owner semantic observations separate | Prevents measurement metadata or a locally computed checksum from becoming another kind of authority |
| 017-004 | Use typed length-framed canonical values, full domain-separated SHA-256, and exactly one self-excluded integrity member | Makes record integrity deterministic without self-reference or serializer-dependent JSON text |
| 017-005 | Admit only explicitly registered versions and retain owner schemas/algorithms | Avoids silent migration and allows existing owner results to remain independently verifiable |
| 017-006 | Encode owner-qualified Intent IDs with 16 host-generated random bytes and retain retired IDs; keep artifact integrity and semantic revision in separate domains | Avoids source-derived identity, owner collisions, implicit ID reuse, and random work inside deterministic compilation |
| 017-007 | Use one closed, parser-backed message projection and full domain-separated SHA-256 for IntentRevision | Makes 016's equality/change rules reproducible without hashing raw host text, formatter output, runtime values, or policy inputs |
| 017-008 | Add only finite authoring inventories, minimal Intent/reference artifacts, registry snapshots, and update plans under new explicit tuples | Enables the initial authoring/identity implementation without completing Profile, source-locale Release, library, or runtime schemas |
| 017-009 | Bind registry history through inventory → update → result references, preserving exact-base atomic publication and explicit genesis/recovery separation | Avoids self-referential digests, partial updates, stale-base overwrite, and history reset disguised as initialization |
| 017-010 | Keep semantic revisions derived from current declaration facts, not registry state; permit read-only use of checked continuity evidence | Separates message changes from identity bookkeeping and avoids requiring registry writes for context-only semantic changes |
| 017-011 | Require complete kind-specific schema and semantic validation with explicit actual inputs; treat basis labels, digest pins, and test contexts as insufficient for production admission | Preserves 015/016/018/019/029 ownership instead of turning successful decoding into a complete or authorized result |

## Deferred Follow-Up Notes

These subjects remain assigned to 017 but are not prerequisites for the scoped configuration, observational measurement, or initial local authoring/identity paths:

- complete source-locale/localized message, cross-owner/library reference, candidate, dependency, library, Store, Release, and target artifact schemas beyond the initial authoring subset;
- full Profile, construction-authority, Snapshot, canonicalization-data, binding, and Finding/Evidence representations;
- Policy/Target body schemas, semantic digest projections, and their trust/admission integration;
- measurement capabilities not adopted by the minimum, including comparison/budget, qualification, profiling, campaigns, and cross-platform reports;
- extended semantic-context/constraint projections, general semantic-result identity, alternate physical encodings, transport containers, registry distribution/compaction, and cross-version migrations.

Any future extension must state its owning semantics and schema-version impact. These notes do not authorize an open extension map or an unsupported success path in the current subset.

## Relationship to Other Documents

| Document | Relationship |
| --- | --- |
| [000 — Intlify overview](./000-intlify-overview-design.md) | Assigns 017's wider artifact and version-admission responsibilities; only the minimum subset is specified here |
| [015 — Project profile and locale policy](./015-intlify-project-profile-and-locale-policy-design.md) | Owns the reference tuples, configuration use sites, minimal implementation boundaries, and resolver semantics implemented using these encodings |
| [016 — Source authoring and Intent identity](./016-intlify-source-authoring-and-intent-identity-design.md) | Owns recognition, message/revision meaning, source facts, continuity, and registry-transition validity; this document fixes the minimum shared representations without completing its later integrations |
| [018 — Security, trust, and provenance](./018-intlify-security-trust-and-provenance-design.md) | Owns trust/authentication; digest and schema success supply neither |
| [026 — Conformance and measurement](./026-intlify-conformance-and-measurement-design.md) | Owns the adopted records' semantic fields, projections, admission outcomes, and performance/storage requirements |
| [029 — Product workflow and packaging](./029-intlify-product-workflow-and-packaging-design.md) | Owns public workflow, artifact/schema acquisition, publication, and packaging |
