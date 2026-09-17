# Intlify Translation Store and Governance Design

## Purpose

This design defines the smallest Translation Store and governance operations needed by [020](./020-intlify-requirement-planning-and-linking-design.md)'s linker and [028](./028-intlify-javascript-web-vertical-slice-design.md)'s generated Web application. It answers three different questions:

- Is this supplied translation a technically valid candidate for the exact Intent revision and definition locale?
- Does the applicable policy and review evidence permit using it, and which candidate has an authorized actor selected?
- Which complete, immutable Store view did synchronization, linking, or a later publication check actually read?

The Store holds compiler-managed localization artifacts and separate evidence, not application-authored translation keys or a mutable catalog. A Provider may supply Japanese text for an English Intent, but supplying or storing that text does not approve or select it. The normal build reads one pinned Store snapshot and never makes those decisions itself.

For the three Intents in 028, an explicit synchronization can store three validated Japanese candidates. A review transaction can approve them, and a separate selection transaction can make them usable by the linker. The three English definitions remain compiler-derived source artifacts; they are not stored translation candidates. Source wording review, when required, stores approval evidence rather than a second copy of the source catalog.

| State | Meaning | Sufficient for localized linking? |
| --- | --- | --- |
| Technically valid and stored | The exact candidate and its validation/provenance evidence belong to the pinned snapshot | No |
| Selectable | The candidate satisfies the applicable policy, review, provenance, and revocation checks | No; eligibility does not choose a candidate |
| Selected | One applicable active Selection Decision chooses that exact selectable artifact | Yes for 021's checks; 020 still verifies the requirement, and 024 still admits the target |

The initial scope is one local application, one versioned Selection Scope, one Store lineage, and the direct-localization path used by 028. It does not complete remote TMS storage, organization-wide review policy, or the entire I1 roadmap.

## Goals

- Preserve technically valid candidates independently from approval, selection, and Release publication.
- Bind every operation to exact Intent, artifact, evidence, policy, scope, and snapshot identities.
- Define deterministic candidate validation and the minimum Approval/Selection Policy behavior without proving linguistic correctness.
- Support explicit approval, rejection, selection, replacement, and revocation with immutable history.
- Keep source-locale definitions out of the localized-candidate collection while supporting separately required source review.
- Publish complete Store transitions against an exact current base without silent overwrite or partial visibility.
- Supply bounded read results to synchronization, linking, and later Release publication without invoking those workflows.
- Apply 026's storage-lifetime, bounded-work, equivalence, and measurement requirements from the first implementation.

## Non-Goals

- Source discovery, Intent allocation/reconciliation, reachability, Requirement Plan construction, or message-locale fallback.
- Provider invocation, routing, retries, refresh policy, Glossary composition, or TMS transport; those belong to 022 and later integrations.
- Public commands, package names, review UI, a remote database protocol, distributed writers, or a general policy language.
- Content-only approval reuse, reviewer quorum, delegation chains, expiring review rules, or automatic candidate ranking.
- Cross-owner/library imports, multiple Selection Scopes in one transaction, Store federation, migration, recovery installation, compaction, or garbage collection.
- Defining shared wire schemas/digest framing, authenticating principals, or extending 018's existing registry grants implicitly.
- Target code generation, Runtime formatting, Release Assembly/publication, deployment activation, or online revocation during rendering.
- Completing every Policy body or the production project-profile resolver before a declared local test slice can be exercised.

## Ownership and Dependencies

| Owner | Responsibility in this subset |
| --- | --- |
| 000 | Source-first artifact lifecycle, distinct supply/governance/build powers, immutable Store history, and publication-time revocation semantics |
| 015 | Checked application and Selection Scope, canonical locale facts, exact Policy references, and applicable admitted binding projections |
| 016 | Current Intent identity/revision, source meaning and parameters, and checked source-definition derivation inputs |
| 017 | Complete source/localized artifact and evidence representations, Store/transition identities, canonical content/integrity projections, and version admission |
| 018 | Trusted input origins, source-admission policy, principal establishment, scoped operation authorization, provenance verification, and common trust/resource rules |
| 019 | Dependency/diagnostic projection and later queries; not candidate selection authority |
| 020 | Store-independent requirements, source/direct-localized verification during linking, and final definition/placement decisions |
| 021 | Candidate-validation orchestration, Approval/Selection Policy meaning, review and selection state, Store validity, and logical atomic transactions |
| 022 | Candidate acquisition and request provenance, snapshot-bound synchronization satisfaction, Provider work, and explicit refresh |
| 023/024 | Parameter/function/capability meaning and authoritative target admission; stored validity is not executable-target compatibility |
| 025 | Release/publication rules and the authorized revocation view required by a new publication |
| 026 | Conformance, performance architecture, owner measurements, and evidence admission |
| 028/029 | Finite Web scenario, trusted local orchestration/acquisition, and concrete storage/publication adapters |

The missing 017/018 extensions are adopted in their owning documents before shared serialization or authorized Store publication is claimed. 021 fixes the logical operations those extensions must represent; it does not reuse an authoring artifact kind or a registry permission for an unrelated purpose.

## Terminology

| Term | Meaning here |
| --- | --- |
| Store lineage | One host-established immutable-snapshot history for the selected application and governance scope; distinct from the Intent registry chain |
| Candidate key | Complete owner-qualified Intent ID, Intent revision, and canonical definition locale; several candidate artifacts may share it |
| Selection key | Exact versioned Selection Scope plus the candidate key; no target, group, or delivery unit is part of this key |
| Review key | Scope, source/localized subject kind, exact ArtifactDigest, and the exact applicable review-policy basis |
| Head record | The explicitly current review or selection record in one snapshot's governance history; not evidence that it satisfies every later policy |
| Selectable | An operation-relative result of actual validation, provenance, policy, review, and revocation checks, not a mutable flag on a message |
| Selected | The one selection-head artifact that remains selectable under the consumer's exact inputs and whose supporting evidence still applies |
| Store transition | A checked finite change from one exact base snapshot to one complete immutable result, awaiting separately authorized publication |

These are logical values and rules, not frozen Rust types, JSON members, filenames, or new digest algorithms. Complete identity equality uses the corresponding 015/017/018 representations rather than display labels or source text.

## Minimum Inputs and Supported Profile

Every operation receives already acquired immutable inputs and names its supported operation and owner-specification revisions. Its adapter has closed input/result shapes and independent validation fixtures; an arbitrary callback or caller-supplied success flag is not an admitted validator.

| Input | Required content |
| --- | --- |
| Scope | One application OwnerIdentity, exact versioned Selection Scope, explicitly established Store lineage, and operation scope |
| Project/policies | Applicable checked 015 facts and exact Approval, Selection, Trust, Source Admission, and Resource Limit bindings; other localization dependencies only when consumed |
| Message basis | Actual checked Intent/source definitions and parameter semantics for the supplied operation, not only ID/revision labels |
| Store | For an existing lineage, one exact base snapshot, required immutable bodies/evidence, and the host's accepted lineage/publication anchor; initialization instead supplies an explicitly authorized empty-genesis request with expected absence |
| Candidate supply | For validation, finite candidate MF2 plus the exact 022 request/acquisition provenance and applicable localization-context inputs |
| Authority | Current host-established principal, scoped operation powers, applicable confirmations, and publication destination when writing |
| Capacities | Explicit finite submitted-byte/count, message, evidence/history, index, transition, diagnostic, scratch, and storage limits |

The 028 profile uses direct `ja` definitions for source `en`, literal text and external string interpolation, and no Glossary or target-specific wording variation. These are supplied scenario values, not new configuration defaults. An unsupported policy, semantic-compatibility profile, or scope is rejected explicitly rather than weakened to make the fixture pass.

Production adoption follows [015's consumer input boundaries](./015-intlify-project-profile-and-locale-policy-design.md#consumer-input-boundaries). A local test host may supply a closed, explicitly test-owned subset and exercise real validators, governance, and publication. It must retain the actual policy bodies, source facts, and authority checks; it cannot relabel PR #205's partial configuration core as a complete production Profile.

## Design Overview

| Step | Operation/result | Required separation |
| --- | --- | --- |
| 1. Admit a view | Verify exact Store, scope, inputs, history/index consistency, and required read authority | No following a mutable current pointer during a pinned read |
| 2. Validate supply | Produce a technically valid candidate and separate technical evidence from exact source/request inputs | No approval, selection, or automatic publication |
| 3. Publish candidates | Atomically add a complete admitted candidate batch and its evidence against one base | Existing selections do not change merely because a new candidate exists |
| 4. Review | Publish an authorized approval or rejection for an exact source/localized artifact | Review does not edit message content or select a replacement |
| 5. Select | Publish an explicit exact candidate choice with its policy and supporting evidence | Selectability is necessary; timestamps, Provider scores, and map order never choose |
| 6. Consume | Return snapshot-bound source-admission or active-selection evidence to 020/022 | No Provider call, governance mutation, fallback search, or Release publication |
| 7. Correct or revoke | Publish new review/selection/revocation records and invalidate affected current choices | Old snapshots and already assembled/deployed Releases remain unchanged |

Candidate publication, review, selection, and revocation are separate transaction classes in the initial profile. Each may batch finite independent subjects, with at most one operation for a given subject/key in that batch. A multi-power compound transaction is a later extension, not required for 028.

## Candidate Validation and Identity

### Validation procedure

The validator performs the following real checks before creating a stored-candidate proposal:

1. Admit the supplied owner, Intent ID/revision, definition locale, request association, provenance, profile revisions, and resource bounds. The definition locale must equal the requested direct locale and differ from the Intent's source locale.
2. Verify the exact source/Intent semantic basis and compatible request context. A DOM selector, equal wording, generated handle, or a Provider-supplied ID cannot establish that association.
3. Decode and parse the supplied MF2 with the admitted shared parser, build its semantic model, and run parser-owned semantic validation. Do not execute JavaScript or substitute a placeholder regex.
4. Derive actual external parameter requirements and capabilities. In the initial literal/string profile, the candidate has exactly the source's external parameter-name set with compatible required string values; order and repeated occurrences may differ. Missing/extra names, incompatible uses, or unresolved references fail. Unsupported declarations, selectors, markup, or function behavior are explicit profile failures, not fabricated successful checks.
5. Verify the applicable machine-checkable constraints and freshness/provenance inputs supplied by the owning specifications. A supported absence remains explicit; an unavailable required validator or body is not permission to skip its check.
6. Construct canonical content, parameter/capability facts, and provenance associations, then compute and verify the owner-defined content and complete-artifact identities.
7. Produce independently identified technical-validation evidence bound to that exact artifact, source basis, input/specification revisions, and checks. Only a complete result may be staged for candidate publication.

A technical failure creates diagnostics, not a LocalizedMessageArtifact or an automatic RejectionRecord. Rejection is an authorized review decision about an admitted subject. Invalid raw Provider responses may be retained by 022's bounded acquisition/reporting facilities, not inserted as successful Store candidates.

Candidate validation establishes only its adopted semantic-compatibility subset. Optional synchronization target preflight remains advisory to the later authoritative 024 admission. A message can be valid MF2 while unsupported by an execution target; diagnostics must not conflate syntax, validation-profile support, and target support. Locale labels and parameter checks do not prove translation quality or the natural language of the text.

### Distinct identities and non-circular evidence

`ContentDigest` identifies canonical message content under the adopted 017 projection. `ArtifactDigest` identifies the complete immutable localized envelope, including Intent revision, definition locale, parameter/capability facts, and provenance reference. The same content with different admitted provenance can yield distinct artifacts and does not share approval automatically.

The dependency direction is source/request provenance → localized artifact → validation evidence → review/selection evidence → Store snapshot. Candidate envelopes do not contain validation or approval records that refer back to their own ArtifactDigest. Acquisition provenance is established from request/source/content inputs independently of the future artifact or result snapshot. Store transition/publication records bind the base and completed result without making that result hash include a reference to its own transition or publication receipt.

017 specifies the exact canonical projections and record schemas under its [Minimum Web Localization Representation](./017-intlify-shared-artifact-and-version-admission-design.md#minimum-web-localization-representation): `message-content-digest` for `ContentDigest`, the `localized-message` envelope for `ArtifactDigest`, and the evidence, transition, and snapshot kinds. The semantic projection used for IntentRevision is not the canonical localized-content projection. Arbitrary JSON hashing, a parser's in-memory layout, or an authoring inventory digest cannot substitute for those definitions.

## Minimum Governance Policy

021 owns the following closed logical behavior for the initial Approval and Selection Policy subset. Their exact bodies/references and semantic digest projections are materialized through 017; 015 continues to resolve references and pass actual admitted bodies.

| Policy dimension | Initial behavior |
| --- | --- |
| Localized approval requirement | Explicitly `required` or `not-required`; omission is invalid. Required means one applicable positive record from an actor authorized for that exact review operation. |
| Review subject | Exact source or localized ArtifactDigest, explicit Selection Scope, and applicable policy basis; content-only review reuse is unsupported. |
| Reviewer authorization | The current 018-admitted power and scope, not an actor name embedded in evidence. Quorum, role expressions, and implicit Provider approval are unsupported. |
| Selection mode | Explicit Selection Decision choosing one exact artifact. No implicit choice, scoring, newest-first rule, or target-dependent choice. |
| Selection support | Exact applicable validation/provenance and positive review evidence, or an explicit policy result that no separate localized approval is required. |
| Negative decisions | An applicable current rejection or revocation prevents eligibility; `not-required` approval does not bypass it. |
| Replacement | A new decision names the expected previous head, including expected absence. Silent last-write-wins replacement is forbidden. |

The profile's 028 success fixture explicitly requires localized approval; a separate negative/alternative-policy case exercises `not-required`. An automatic local policy actor may review and select only through these same separate operations with independently established powers. The policy is not a promise that a human reviewed the wording.

Source Admission Policy, owned by 018, decides whether authenticated source is sufficient or separate source wording approval is required. When required, 021 verifies an exact source ApprovalRecord under the supplied review-policy basis. The localized `not-required` mode cannot waive a separate source requirement. The source-admission evaluator, policy body, and allowed authority mode must actually be adopted; an `approved: true` annotation or successful source analysis cannot replace them.

## Logical Records and Snapshot Invariants

The minimum retained record families have the following meanings. Each exact evidence identity is independent of the message ContentDigest and ArtifactDigest; 017 owns their typed encoding.

| Record | Required logical contents |
| --- | --- |
| LocalizedMessageArtifact | Candidate key, canonical MF2/message facts, ContentDigest, complete ArtifactDigest, parameters/capabilities, and immutable acquisition provenance reference |
| Validation Evidence | Exact candidate/source subject, actual validator/specification and input basis, complete check outcomes, and retained technical evidence |
| ApprovalRecord / RejectionRecord | Exact Review key, positive/negative verdict, expected prior review head, authorized actor/confirmation provenance, and review inputs/reason |
| SelectionDecision | Selection key, selected ArtifactDigest, expected prior selection head, exact Policy references, supporting validation/review/provenance references, and authorized decision provenance |
| RevocationRecord | Exact source/localized artifact or supported validation/provenance/approval/selection evidence target, explicit scope, authorized actor and reason; no wildcard or content-only target |
| TranslationStoreSnapshot | Lineage, application/scope binding, exact parent or explicit genesis, complete candidate/evidence membership, current review/selection heads, and retained revocations |
| Checked transition | Exact base, immutable additions, expected heads, resulting complete state, consumed policies/inputs, and necessary invalidation explanations |
| Publication provenance | Exact authorized transition/result, destination/lineage, publisher and authority state, and actual host publication outcome; separate from future write permission |

Snapshots are append-only with respect to retained artifacts and evidence. Review and selection heads may change, but their previous records remain available. The first profile keeps bounded finite history without compaction; exceeding its retained-history capacity blocks a write rather than dropping evidence or inventing a new lineage.

Admission rejects conflicting content under one identity, missing required bodies, wrong-owner/scope references, dangling heads, cyclic/invalid supersession, inconsistent parent/result bindings, and unsupported record kinds or revisions. Duplicate entries inside a submitted batch are rejected before deduplication. An exact artifact/evidence already present in the base may be reused idempotently after equality/integrity checks; equal content alone does not deduplicate different candidate artifacts.

There is at most one review head per Review key and one selection head per Selection key. Head maps are checked projections of explicit accepted transitions, not independently writable flags. A later snapshot can retain an earlier selection without reselecting it only while its exact supporting records remain current and applicable. A changed external policy can make a retained head unusable for a new invocation without corrupting or rewriting the historical snapshot.

Membership and independent batch additions have set semantics. Reordering them cannot change a result; duplicate subject operations remain invalid. History follows exact parent and supersession references, never timestamps or physical record order. Canonical output ordering uses the adopted 017 complete-identity/key rules, not hash iteration or worker completion order. Derived invalidation sets retain every distinct affected head and cause without duplicating the head mutation.

## Review, Selection, and Revocation

### Review and source approval

A review operation receives an actual admitted subject and its required source/technical evidence. It checks the review-specific authority and policy, and names the exact previous review head or its absence. A positive or negative record replaces that head only through a checked published transition. A new positive review may explicitly supersede a rejection; merely adding another positive record or choosing the most recent timestamp cannot do so.

Replacing a review record invalidates selections that relied on the superseded evidence. They require a new explicit Selection Decision even if the new verdict is also positive. The same transition removes affected selection heads and retains the old decisions plus the exact cause; it never silently selects another candidate or substitutes new support into an old decision.

A first rejection also invalidates any current selection of that subject under the applicable review-policy basis, including a choice made while localized approval was explicitly not required. Absence of an applicable rejection was part of that choice's eligibility. There need not be an older approval record to supersede for the negative decision to take effect.

Source review retains the exact source artifact reference and review evidence, not a localized candidate payload. The compiler/host supplies the source body and derivation inputs for operations that inspect or use it; any replay retention remains identified compiler/operation input, outside the localized-candidate collection. An approved source artifact receives no SelectionDecision. 020 deterministically chooses it only after source-admission checks pass.

### Explicit localized selection

Selecting a candidate requires that its exact artifact and validation evidence already belong to the base snapshot. Evaluate permitted provenance, exact policy inputs, technical freshness, current review head where required, and the snapshot's applicable revocations. Then check the actor's independent selection power and the expected previous selection head before constructing the new decision.

The choice is an explicit ArtifactDigest, not a candidate-search algorithm. Several selectable artifacts may coexist; no selection is inferred even when only one exists. A decision records its exact base/evaluation inputs and support, but an unrelated later candidate addition does not by itself invalidate it. New content, evidence, or policy cannot be substituted solely because it would also be selectable.

Candidate publication has no effect on a valid existing choice. An authorized replacement may explicitly supersede it with another selectable artifact under the same key. A new Intent revision has a different key; prior approval or selection is not transferred by ID continuity, matching wording, or registry lineage.

### Revocation and historical views

The initial revocation operation targets one exact artifact or an exact supported evidence record within the admitted application/scope. A source subject may be externally supplied rather than a stored candidate. Record targets must be present in the retained evidence domain. Unsupported target kinds, a missing subject, or attempts to revoke a revocation are rejected. The minimum has no undo-revocation operation.

Compute the complete finite dependency set of current selections affected by that target. Publishing the revocation and invalidating those heads is one atomic transition; inability to complete the dependency checks or retain mandatory evidence blocks publication. The artifact, earlier records, and all historical snapshots remain unchanged. Revoking support does not automatically choose other existing support or a replacement candidate.

Readers pinned to an earlier snapshot continue to obtain that snapshot's historical answer. Readers of the new snapshot cannot use the revoked subject/support. A new positive review does not override an artifact revocation. A later replacement artifact or support set must pass its own complete validation, review, and explicit selection operations.

025 owns the different check for a new Release publication: it obtains and records an exact authorized revocation view, which may be newer than the build's pinned Store. 021 supplies the snapshot-bound revocation facts and dependency evaluation, not deployment authority or a guarantee about future revocations. A revocation after that view was checked remains an impact on an existing/in-flight publication; it does not rewrite the old Store, Release, publication record, or deployed application.

## Exact-Base Atomic Publication

### Preparation and authorization

The logical core is read-only. Preparing a transition validates one base and a finite transaction class, checks subjects/expected heads, constructs the complete result and required head invalidations, and returns a checked proposal or explicit failure. Record identities and explicit choices are fixed inputs for replay; preparation does not call a Provider or generate fresh durable identities as a hidden side effect.

The initial operation set separates Store reads, explicit new-lineage initialization, candidate publication, source/localized review, selection/replacement, revocation, and Store publication. Review/selection/revocation authority permits the exact decision; making it current additionally requires Store-publication authority. A publisher may commit the exact checked decision but cannot author or alter it implicitly. 018 defines the corresponding scoped powers (`read-store`, `initialize-store`, `stage-candidates`, `review-message`, `select-message`, `revoke-evidence`, and `publish-store`) under its [Minimum Web Localization Extension](./018-intlify-security-trust-and-provenance-design.md#minimum-web-localization-extension); the registry grants alone do not authorize these operations, and the adopting implementation must test them.

Initialization requires a separately established new Store binding and explicit empty-genesis authorization. Missing/corrupt files are not a new-store signal. Store identity and current-state authority do not derive from the Intent registry, working directory, or a self-consistent untrusted snapshot chain.

### Publication adapter obligations

The local host must:

1. Retain the exact base, checked additions/result, expected heads, and decision/publication authority associations.
2. Stage and verify all newly required immutable bodies without exposing a partially complete current snapshot.
3. Serialize the final current-base and still-applicable authority checks with the publication point. A changed base or authority requires fresh admission/replanning, not a silent merge or automatic decision replay.
4. Atomically make one complete result and its matching publication provenance current. Readers see the old view or the complete new view, never a mixture.
5. Report whether the exact transition committed, provably did not commit, or has an indeterminate outcome after interrupted I/O. Resolve an indeterminate result against protected transaction/current-state evidence before retrying; never report a guaranteed rollback or successful publication without proof.

For local persistence, 029's [Store lineage and Release destination adapters](./029-intlify-product-workflow-and-packaging-design.md#store-lineage-and-release-destination-adapters) apply its conditional publication and interruption handling to this Store; they supply persistence, not a Store codec or authorization domain. The Store adapter needs its own exact subject/result bindings and real reopen, concurrency, and fault tests. A mutex-only in-memory model proves no disk durability.

Two writers using the same base cannot both replace the current view. Identical admitted additions with no new decisions or head changes produce an explicit unchanged result, not a gratuitous new snapshot. Changed evidence/provenance or an explicit new review is not unchanged merely because the displayed text is equal. Repeating a previously attempted publication uses its exact retained transition identity and outcome evidence; it cannot silently run a new write against a later base.

A proven pre-publication failure leaves the current view unchanged. Unreachable staged bytes are not published Store membership and may be dealt with by a later host cleanup policy; no cleanup or recovery algorithm is implicitly part of normal reads. Release or deployment failure after Store publication does not roll that Store transaction back.

## Read Handoffs to Synchronization, Linking, and Publication

All reads use one exact admitted snapshot and explicit policy/source inputs. They never follow a mutable current pointer midway, scan another Store, infer missing bodies, or change governance state.

| Consumer | Minimum read result | Consumer-owned work |
| --- | --- | --- |
| 022 synchronization | Bounded candidates and their technical/context freshness, review/revocation status, and exact active-selection result for requested keys; source-review facts remain separate | Derive snapshot-bound satisfaction and missing/stale/refresh work; pending review alone does not grant supply or approval authority |
| 020 linking | One active exact localized selection and complete support, or explicit absent/ineligible/invalid-input outcome; source artifacts receive separate source-admission/review evaluation | Require a complete current plan, verify direct coverage, choose compiler-derived source, and form the bundle |
| 025 publication | Revocation facts for exact included source/localized artifacts and supporting evidence in one authorized view | Obtain the required view, decide publication admission, record it, and coordinate with deployment |
| Inspection | Scoped candidate/history/head explanations with exact evidence and explicit boundedness | Presentation, Store-wide audit, review UX, and safe disclosure |

An active-selection query does not return the first eligible candidate when the head is absent or ineligible. Well-formed absence, policy-stale evidence, and explicit rejection/revocation are domain results; malformed snapshots, unavailable required support, denied reads, and exceeded limits are admission/operational failures rather than empty candidate sets. 022 cannot turn an invalid Store read into new Provider work.

Candidate facts and selection satisfaction are views of the supplied snapshot, not fields added to 020's Store-independent Requirement Plan. Source-equal requirements never become Provider jobs, including when source approval is missing. 022 determines how unselected, rejected, stale, or explicitly refreshed direct demand affects its finite work set; 021 does not invent that scheduling policy.

The initial Store reader may validate/index its complete bounded snapshot once, then query the current finite requirement keys. Unselected or historical valid records can remain unapproved, revoked, or inapplicable without generating ordinary build coverage diagnostics. Structural/integrity failures needed to admit the snapshot still fail; scoped reporting is not permission to ignore corrupted Store membership.

## Minimum Integration Example

The following sequence uses 028's three exact Intent revisions and an explicit policy requiring localized approval. Source admission is independently satisfied by the supplied source policy/evidence. Snapshot labels below are explanatory, not serialized identities.

| Step | Store state and expected observation |
| --- | --- |
| Empty genesis S0 | No localized candidates or selections; 020 retains six requirements and cannot link the three direct `ja` rows |
| Candidate publication S1 | Real 022 fixture supply and 021 validation add three `ja` artifacts/evidence; no `en` candidate is stored; no localized selection exists |
| Approval S2 | Authorized positive reviews make the three candidates selectable; linking still fails without Selection Decisions |
| Selection S3 | Explicit choices make all three `ja` rows resolvable; together with three admitted compiler-derived `en` definitions, 020 links six definitions for both Web targets |
| Unchanged replay | Reads/builds retain S3 and the same choices; no Store writes or Provider calls. 022's repeated unchanged sync has no new work. |
| Rejection or revocation S4 | An affected selection is invalidated in the same transaction; a build pinned to S4 fails that strict row, while a historical S3 build remains reproducible |
| Explicit replacement | A separately validated, eligible replacement plus a new authorized selection can satisfy the row in a later snapshot; no text/ID-based approval transfer |

An additional case requires separate English source review: its approval records are stored without English candidate payloads or source Selection Decisions. A missing or changed source ArtifactDigest blocks source use until its applicable admission is established, not until a Provider translates English into English.

These are actual operation expectations, not fixture dispatch rules. The Japanese payloads may be deterministic test supply, but validation, decision checks, snapshot construction, read selection, and failure behavior execute their implementations.

## Dependencies, Invalidation, and Performance

Retain exact consumed dependencies: source Intent/revision and semantic basis; candidate content/artifact/provenance; parser and compatibility profiles; applicable localization-context inputs; Policy bodies/references; snapshot lineage/membership; review/selection/revocation records; limits/options; and current read/publication authority at their respective operations. Policy, provenance, or supporting evidence can change eligibility without changing message bytes or IntentRevision.

Source locations and host-only expression changes do not independently create a new localization demand when the checked Intent identity/revision and applicable semantic inputs remain the same. Their current source evidence still needs its owning checks. Snapshot invalidation never deletes history, retires an Intent, calls a Provider, or mutates an already returned view.

Apply [026's performance implementation architecture](./026-intlify-conformance-and-measurement-design.md#performance-implementation-architecture):

- Retain candidate, evidence, and snapshot storage immutably, with bounded structural sharing where useful. Do not copy full messages into every review, selection, requirement, or target placement.
- Build exact candidate/review/selection and supporting-evidence dependency indexes once per admitted view. Batch lookups and revocation impact checks must not perform a full-history scan for every message or target.
- Keep worker/invocation Scratch Workspaces separate from retained results. Reuse capacity after success, failure, and cancellation; old readers remain valid after reset and publication.
- Parse each exact candidate once per compatible validation basis, then reuse its real MF2 facts for parameters and capabilities. Cached integrity/semantic facts never stand in for current read or publication authority.
- Use dense local IDs for admitted finite indexes where useful. Fast process-local hashes do not replace 017 identities, determine canonical order, or admit unbounded attacker-controlled keys.
- Start with a bounded synchronous core and one local writer serialization point. No distributed cache, nested worker pool, custom arena, SIMD, or unsafe optimization is required.
- Keep optional expanded history/snippets/profiling out of the common unrequested path. Mandatory technical/governance evidence is not optional inspection data.

| Owner operation | Minimum observations and separation |
| --- | --- |
| Candidate validation | Input/parsed bytes, parse and compatibility-check counts, exact outcome/evidence, duration, and applicable allocations; exclude Provider I/O |
| Snapshot admission/indexing | Records/bytes/edges admitted, lookup-index construction, failures, retained and scratch storage; distinguish decoding from semantic admission |
| Eligibility and selection reads | Requested keys, candidate/support checks, result identity, cold/reused work, and duration; no hidden acquisition |
| Transition preparation | Additions, affected heads/dependency edges, required checks, result/evidence identity, and workspace growth |
| Publication workflow | Staged/synchronized bytes, serialization/current-base checks, commit/conflict/indeterminate outcomes, and I/O duration separate from the logical core |

The implementation plan pins the applicable cases, Method Descriptors, intervals, repetition/cache state, Memory Observation Domains, and independent semantic observations. Retain validated descriptive baselines through 026; no numerical timing/allocation pass threshold is invented here. Required correctness and evidence completeness are completion gates. Optional profiling uses a non-default build feature and separately validated instrumentation, not primary timing samples.

## Diagnostics, Limits, and Verification

Use [019's minimum Diagnostic projection](./019-intlify-project-graph-query-and-incremental-design.md#minimum-common-diagnostic-projection), preserving parser, policy, provenance, and authorization ownership. The 021 reason families cover invalid/unsupported Store input, invalid candidate compatibility, missing/inapplicable review or selection, rejected/revoked subjects, expected-head/base conflicts, unauthorized operations, capacity exhaustion, and publication-outcome uncertainty. A diagnostic identifies exact scope, candidate/selection key, subject/evidence, snapshot/base, and stage when those facts are safely established.

Bound submitted bytes/counts before deduplication, MF2 source/semantic work, retained artifacts/evidence/history, index and revocation-dependency edges, transaction additions, diagnostics, and host I/O/retry work. Check arithmetic before expansion/allocation. An exceeded limit or unavailable required record cannot become a truncated successful snapshot, selection, or transaction. No implicit numeric capacity defaults are specified here.

| Fixture family | Required cases |
| --- | --- |
| Candidate semantics | Literal and string interpolation; reordered/repeated parameters; malformed MF2; missing/extra/incompatible parameters; wrong Intent/revision/locale/request; unsupported validation capability; no source-equal candidate publication |
| Identity and evidence | Equal content with different provenance/artifacts; modified body under an existing identity; missing/wrong validation subject; conflicting/duplicate batch records; non-circular independent digest and record fixtures |
| Policy and review | Required versus explicit not-required approval; rejection still blocks; exact scope/policy/ArtifactDigest matching; unauthorized approval; positive/negative replacement with expected heads; no content-only reuse |
| Source lifecycle | Authenticated-source path under its actual policy, separately required review, missing/stale/revoked approval, no stored source candidate or source Selection Decision |
| Selection | Stored and selectable but unselected states; several selectable candidates; no first/newest choice; explicit replacement; unrelated candidate addition preserving selection; changed support/policy making a choice unusable |
| Revocation/history | Artifact and support revocation, complete affected-head invalidation, no implicit alternate support, historical reproducibility, new-view rejection, and distinct build versus publication-view checks |
| Transactions | Exact-base/head conflict; two writers with one base; duplicate replay and unchanged result; candidate batches with one invalid member publishing nothing; decision power without publication power and the converse |
| Persistence | Real reopen/restart, complete body/provenance visibility, interrupted staging/commit/acknowledgement, uncertain-outcome resolution, and no automatic reset of missing/corrupt history |
| Boundaries and reuse | Every adopted exact/first-over bound, cancellation/failure then workspace reuse, immutable old reader after new publication, cold/reused/full-rebuild equivalence, and no secret-bearing diagnostic output |
| Integration | Actual 020/022 results for S0–S4, six requirements but three localized candidates, zero build-time Provider/governance calls, and actual selection evidence consumed before 024/025/027/028 execution |

Expected candidate facts, policy outcomes, transition/head sets, and historical/current read results are independently specified. Round-trip codecs, a prefilled selected map, fabricated policy flags, or a Provider returning the expected string do not establish conformance. The adopted case inventory and complete applicable 026 records must pass even when timings are only descriptive.

## Adoption with 016 and 028

| Adopting work | Required scope and claim |
| --- | --- |
| 021 logical-core development | Closed finite test-owned message/policy/Store inputs; actual candidate, eligibility, history, and transition rules; independent fixtures and scoped 026 evidence |
| 016 Phase 3 | The existing Intent registry remains a different lifecycle. Store design is not a prerequisite for bounded registry-core work and does not grant its host new powers. |
| 016 Phase 4 / complete local bundle handoff | Actual 020 requirements, source/Intent associations, 017 source/localized/evidence/Store representations, and applicable 015/018 inputs; real read-only source/selection validation |
| 028 localization supply | Adopt 022's exact request/provenance and candidate handoff, 018's Store/governance authority extension, and a real local publication adapter; current 029 registry work alone is insufficient |
| 016 Phase 5 / generated Web execution | The selected Store view must flow through actual 020 linking, 023/024 capability/export, 025 Release/publication, and 027/028 execution/equivalence with 026 evidence |

This design is a minimum owner specification, not a claim that those consumers or formats are implemented. The implementation plan must list each missing owner addition before its dependent operation. It need not complete all Store topologies, policy features, or downstream execution features to start the logical core, but cannot mark the Web path complete with Store mocks or analysis-only tests.

## Decision Log

| ID | Decision | Rationale |
| --- | --- | --- |
| 021-001 | Start with one local application, Selection Scope, and immutable Store lineage | Bounds the first Web path without deriving governance identity from a target or registry |
| 021-002 | Separate technical validation, stored membership, selectability, and explicit selection | Neither Provider output nor an approved candidate is automatically build input |
| 021-003 | Initially bind reviews to exact artifacts and permit no content-only reuse | Preserves provenance, scope, and policy applicability while avoiding a review-reuse inference engine |
| 021-004 | Keep compiler-derived source payloads outside the candidate collection and never select them through SelectionDecision | Preserves source-first authorship and separate source-admission obligations |
| 021-005 | Use explicit expected review/selection heads and invalidate selections whose support is replaced or revoked | Prevents silent overwrite, implicit support substitution, and automatic fallback selection |
| 021-006 | Keep candidate, review, selection, and revocation publication as separate transaction classes | Makes the minimum authority/evidence path testable without a compound workflow language |
| 021-007 | Publish exact-base complete immutable states and report uncertain I/O outcomes honestly | Prevents partial visibility, lost updates, and unsafe automatic retry |
| 021-008 | Keep historical Store evaluation distinct from new Release publication checks | Preserves reproducibility without claiming a historical artifact remains currently publishable |
| 021-009 | Leave request scheduling, shared encoding, authentication, and physical target behavior with their owners | Adds the missing Store semantics without creating a competing compiler or security architecture |
| 021-010 | Adopt bounded indexes/workspaces and real conformance/measurement with each operation | Avoids per-key full-history work and unverifiable performance or publication claims |

## Deferred Follow-Up Notes

### Required follow-up for minimum Web execution

- 017: the source/localized message, validation/governance/provenance, Approval/Selection Policy, and Store snapshot/transition representations are now defined under its Minimum Web Localization Representation, with publication provenance kept host-private; the adopting implementation must materialize their schemas and independent fixtures.
- 018: the governance/Store read and write powers, confirmations, permitted provenance, and minimum Source Admission Policy are now defined under its Minimum Web Localization Extension; Trust and Resource Limit bodies remain deferred, and no registry grant is extended by implication.
- 022: define request/acquisition provenance, candidate handoff, current-candidate satisfaction versus missing/stale/refresh demand, and the deterministic fixture Provider workflow. This is the next supply-side design, not an extra field in the Requirement Plan.
- 028/029: implement the local Store publication adapter, orchestration, and persistence/failure tests alongside the actual owner validators. A private test adapter is sufficient for the declared local scope, not for a production Profile or remote Store claim.
- 023/024/025/027: adopt the supported message/value capabilities, generated artifact and Release admission, publication revocation-view integration, and reference execution path before claiming localized Web execution.

### Broader extensions

- Content-based approval reuse, multi-reviewer/quorum policy, broader policy expressions, automated ranking/selection rules, richer source review, and reversible or wider-scope revocation profiles.
- Rich MF2 candidate compatibility, Glossary/constraint sets, library imports, multiple scopes/owners, remote/TMS storage, partial materialization, or distributed publication.
- Review interfaces, full Store audit/revocation-impact queries, public workflow/packaging, authenticated recovery, migration, history compaction, and garbage collection.
- Persistent/shared incremental caches, more selective dependency projections, parallel validation/publication scheduling, and broader performance budgets under 026.

## Relationship to Other Documents

- [000](./000-intlify-overview-design.md#localization-provider-and-tms-integration) defines Store/governance responsibilities and immutable publication semantics; this document narrows them to a local implementation profile.
- [020](./020-intlify-requirement-planning-and-linking-design.md#source-definitions-and-localized-selection) consumes the exact source and localized selection evidence defined here; it neither supplies nor chooses candidates.
- [028](./028-intlify-javascript-web-vertical-slice-design.md#design-overview) supplies the finite three-candidate/two-locale integration and subsequent real execution tests.
- [026](./026-intlify-conformance-and-measurement-design.md#adoption-and-verification-requirements) supplies applicable correctness, evidence, measurement, and optional-profiling requirements; there is no separate 026 implementation phase to finish first.
