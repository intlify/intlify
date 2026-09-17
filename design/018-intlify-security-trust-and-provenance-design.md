# Intlify Security, Trust, and Provenance Design

## Purpose

This design defines the minimum security rules needed by [016](./016-intlify-source-authoring-and-intent-identity-design.md)'s local source-authoring and persistent-identity implementation. It answers three questions:

- May this invocation use these exact source and registry inputs for this application?
- Who may initialize or update the application's Intent registry, including making an explicit identity choice?
- What must the host verify before a checked update becomes the current registry?

For example, a compiler can validate a proposed change from registry `R1` to `R2` without having permission to publish it. A separate authorized update operation may publish `R2` only for the checked application, destination, source inventory, and decisions, and only while `R1` remains the current base. A valid digest, a source-controlled file, or an `explicit` reason in the update does not supply that permission.

| Question | Responsible result |
| --- | --- |
| Are the bytes, versions, source associations, and identity transitions valid? | 017 representation/integrity admission and 016 semantic checks |
| Are these inputs and this actor's requested operation permitted? | The scoped 018 admission and authorization defined here |
| Did the exact authorized update become current without overwriting another update? | 029's atomic publication result |

These results remain separate even when one local process produces all three. Ordinary compilation stays read-only. Source admission for analysis is not source-locale approval for localization or Release use.

The initial scope is one explicitly selected local application owner and its single registry chain. It supports 016 Phases 1–3; it does not complete the broader security responsibilities assigned to 018 by [000](./000-intlify-overview-design.md#security-trust-and-reproducibility), or all the security dependencies of [028](./028-intlify-javascript-web-vertical-slice-design.md#ownership-and-dependencies). The [Minimum Web Localization Extension](#minimum-web-localization-extension) below adds the Store, supply, Release, and execution powers that 028 needs for 016 Phases 4–5 under the same local authority mode; remote and production trust remain unspecified.

## Goals

- Keep content integrity, semantic validity, permitted provenance, actor authorization, and current registry state distinct.
- Permit bounded local development without requiring remote identity services, signing infrastructure, or network access.
- Prevent source content, metadata, artifact labels, and self-consistent registry history from authorizing themselves.
- Bind every write authorization to the exact owner, destination, operation, inputs, decisions, and result.
- Preserve explicit initialization, read-only compilation, verified continuity, and fail-complete updates from 016/017.
- Define the minimum handoff to 029 without choosing commands, storage formats, locking primitives, or public APIs.
- Apply 026's resource, ownership, reuse, measurement, and optional-profiling requirements from the first adopting implementation.
- Extend the same local mode to Store reads and writes, governance decisions, Provider invocation and disclosure, Release publication and activation, and generated-code origin admission for the 028 minimum.

## Non-Goals

- Remote authentication, multi-tenant services, transitive trust delegation, certificate/key formats, signatures, or distributed revocation.
- Library/package publisher trust, cross-owner updates, or one invocation managing multiple application owners.
- Remote or multi-tenant Store and Release services, reviewer quorum and role expressions, production deployment credentials, or Runtime-held credentials of any kind.
- Completing 015's Trust Policy or Resource Limit Policy artifact bodies, or its bootstrap Resource Limit Policy Verification Input representation; the minimum Source Admission Policy body is defined below.
- Replacing 015 checked-profile construction, 016 identity semantics, or 017 schemas and digest rules.
- Implementing registry recovery/rollback installation, history compaction, or migration. Validating retained history is supported; making recovered history current requires a separately adopted workflow.
- Defining a general security-record wire protocol, public authorization API, CLI syntax, configuration members, or a new crate.

## Ownership and Dependencies

| Owner | Responsibility in this subset |
| --- | --- |
| 015 | Checked project identity and applicable source/trust/resource inputs, exact binding projections, configuration bootstrap, and disclosure rules for its results |
| 016 | Supported authoring, source completeness, continuity/newness/absence verification, explicit identity choices, and update validity |
| 017 | Existing source/inventory/Intent/registry representations, exact identity and reference domains, bounded decoding, integrity, and replay encoding |
| 018 | Local authority establishment requirements, permitted input origins, action separation, exact authorization scope, and provenance requirements for those decisions |
| 019 | Shared diagnostic projection, queries, and incremental orchestration; not required to run the local admission checks |
| 020/021/022 | Planning, Store/governance, and supply semantics; 018 supplies the powers, disclosure rules, and provenance verification those operations consume |
| 024/025/027 | Target output, Release, and execution semantics; 018 supplies publication/activation powers and the code-origin admission profile |
| 026 | Applicable conformance, performance architecture, measurement evidence, and profiling isolation |
| 029 | Trusted local invocation/acquisition adapters, principal establishment, authority lifecycle, registry binding/currentness, atomic persistence, and operation UX |

The 018 rules are defined here rather than delegated to an arbitrary host callback returning `true`. The 029 adapter must provide the concrete trusted observations and conditional-publication guarantees those rules require. Specifying the rules does not implement that adapter.

Production use still requires the applicable actual checked 015 inputs. A test context may exercise a finite subset under 016/017, but neither a test grant nor PR #205's configuration/locale core becomes a complete production `LocalizationProjectProfile`. This document does not introduce a shortcut around the unfinished policy-body or bootstrap formats.

## Terminology

| Term | Meaning here |
| --- | --- |
| Trusted local host | The explicitly selected tool/integration implementation that establishes the invocation's authority and controls its publication destination; not the application code being analyzed |
| Principal | A caller identity established by that host through its supported local invocation mechanism, not a name asserted inside source or an artifact |
| Local Authority Context | One immutable, host-established set of owner, policy, origin, grant, and lifecycle bindings for a bounded invocation |
| Input provenance | The checked association between exact input content and the acquisition route permitted by the authority context; it does not assert translation quality or historical human authorship |
| Explicit decision confirmation | Host-established evidence that an authorized caller selected a particular identity action against exact inputs; the serialized decision reason alone is insufficient |
| Publication permit | An operation-local result authorizing one exact checked transition; not a transferable bearer credential or proof of completed publication |
| Publication provenance | Host-retained facts associating a successfully published transition with its authority, actors, decisions, and exact inputs/result |

These are logical values and rules, not public type names or new JSON objects. In-process checked values must be distinguishable from unvalidated caller data; deserializing a label such as `trusted`, `allowed`, or `confirmed` cannot construct them.

## Minimum Local Authority

### Explicit establishment

The first supported authority mode is an explicitly selected trusted-local-host mode. The operator or embedding application chooses the host and grants its powers through a trusted invocation/setup path outside the source and artifacts being analyzed. No signature is required by this mode itself. It is not an implicit unsigned default and cannot override a signature or provenance requirement in an applicable admitted policy.

The host binds the principal using its supported local session or embedding-call mechanism and records the responsible adapter and its version. Accepting a `principal` string from a source annotation, JSON artifact, or untrusted API argument is not authentication. A configuration reference can identify policy to check, but cannot establish the authority that trusts that policy merely by pointing to itself.

The local assumption is explicit: the host implementation, its authority-establishment path, and its protected registry/authority state are trusted. Arbitrary analyzed source and submitted artifacts are not. This subset does not protect against a compromised host or an actor already able to rewrite that protected state; deployments needing stronger assurance require a separately adopted authentication/storage profile. A repository directory or writable registry file is not, by itself, protected authority state.

### Required context

Before admission, the host supplies one complete immutable context with the following logical bindings:

| Binding | Required content |
| --- | --- |
| Host and principal | Established caller identity and the exact local authority/acquisition adapter profiles; authentication handles stay host-local |
| Authority state | A host-protected revision of the complete authority state, its invocation/session scope, and a way for publication to verify that the same state is still applicable |
| Application | Exact 017 `OwnerIdentity` with kind `application`; for production, its identity equals the checked 015 `projectId` |
| Analysis context | Actual admitted 015 projection/bindings and supported 016/017 profiles, or explicitly test-only inputs under the corresponding test mode |
| Input scope | Declared finite source membership, permitted acquisition routes, and the exact registry/history anchors accepted for this operation |
| Registry destination | An opaque host binding for the owner's one registry chain and declared owning scope; existing bindings retain the exact `registryIdentity` |
| Powers and mode | A finite map from established principals to explicit action sets from the closed table below and, separately, whether an explicitly enabled development auto-update session is active |
| Protection and disclosure | Applicable admitted trust/source/resource constraints plus finite implementation capacities and safe reporting rules |

The context contains the actual checked inputs or retained access to them, not just their names/digests. All applicable constraints must be satisfied; a host grant can restrict project policy but cannot weaken it. Missing policy bodies, unsupported required verification, absent ownership, or inconsistent bindings block the production operation. None receives a permissive default.

The grant map has one entry per established principal and no repeated or unknown actions, wildcard principals, inherited roles, or implicit administrative grant. Duplicate/conflicting entries are invalid rather than merged. Every principal uses the same admitted local authority domain; matching a display name is not principal equality. The invocation selects one authenticated caller, while an explicit-choice confirmation retains its separately established confirmer.

The host must not reuse one authority-state revision for changed grants, policy/context bindings, permitted origins, or session scope. Ending a session makes its state unavailable for a new publication. Publication validates the actual host-established state, not a caller-supplied revision string. A changed state may be admitted for a fresh evaluation but cannot keep an old permit valid by retaining its label.

One owner has one retained ID collision domain under 016/017. Choosing another analysis scope, path, or destination must not create a second authoritative chain for that owner. A test context and destination remain test-only and cannot be promoted by relabeling their owner/context fields.

There is no delegation chain in this subset. A worker receives only its explicitly scoped inputs and powers; it cannot mint authority contexts, extend their lifetime, select another principal, or grant itself additional actions. The same local person may hold several grants, but the operations and checks remain separate.

## Action Separation

The initial grant vocabulary is closed. These labels identify logical powers, not CLI commands or 015 configuration fields.

| Action | What it permits | What it does not permit |
| --- | --- | --- |
| `analyze-source` | Admit the scoped source inputs and derive 016 authoring facts | Running application code, approving source messages, or publishing registry state |
| `read-registry` | Admit the scoped current or explicitly pinned historical registry and replay its supplied history | Treating historical input as current, initializing, or updating |
| `initialize-registry` | Publish one checked empty genesis for a separately confirmed new owner binding | Replacing missing/corrupt history, importing a different root, or allocating all declaration IDs as a hidden step |
| `update-registry` | Publish one complete checked plan/result against the exact current base | Selecting ambiguous history without confirmation, bypassing 016 checks, initializing, or installing a recovery/rollback |
| `resolve-identity` | Confirm an exact explicit continuation/allocation/restoration decision for this owner and input scope | Publishing it without `update-registry`, reusing another owner's ID, or bypassing structural transition rules |

Analysis with a registry requires the relevant read and source grants. Preparing an update requires the checks on its source/base; publication additionally requires the appropriate write grant. A returned analysis or replay result never carries write power merely because it contains a valid plan.

The evaluator denies actions without an explicit applicable grant and rejects unknown actions. A grant is scoped to the context's owner, destination, and operation mode. Holding `update-registry` does not imply `initialize-registry` or `resolve-identity`. No action grants Provider, governance, Release, or deployment authority.

## Input Admission and Provenance

Admission consumes already acquired immutable inputs. The local host provides acquisition associations independently of the submitted payload: the permitted route, its established adapter context, and the exact source snapshot or artifact reference/body obtained through it. A path, package origin, source-control commit, or checksum asserted by the payload cannot create that association.

The required checks are:

1. Establish the authority context, supported operation, owner, and required grants under finite bootstrap capacity before processing untrusted collections.
2. Validate the finite input membership and acquisition associations against that context. A registry base must be explicitly anchored by the host's accepted binding/publication provenance or an independently authorized exact historical anchor; a self-consistent chain cannot anchor itself.
3. Apply 017's bounded decoding, exact kind/schema/specification selection, complete-body integrity, and reference-resolution rules. Apply its source byte-length/digest, owner/unit/revision/grammar, range, duplicate, and conflict checks to actual retained content.
4. Apply 016's source, context, completeness, declaration, and history checks to those exact inputs, including the supported continuity verifier. Do not execute source, choose plugins from it, or replace unavailable snapshots with filenames or digest pins.
5. Produce only the admitted facts for the requested scope, or a blocked result with safe bounded diagnostics. Separate provenance/authorization failures from syntax, identity, and resource failures.

Read-only use may consume an explicitly pinned older snapshot without requiring it to be the destination's current base. The result preserves that historical scope. Publication separately requires currentness; permission to replay an old snapshot is not permission to roll the destination back.

For retained authorized history, admission checks the trusted anchor and required publication provenance, then performs 017 replay. It does not demand a new human confirmation of every old decision merely to read history, and historical authorization is not reused as permission for a new write. If the requested replay needs unavailable or untrusted history, it remains blocked; it does not bootstrap from the oldest file that happens to be available.

Complete/partial membership follows 016/017 exactly. A partial inventory may support proven covered updates while preserving unseen entries; it cannot authorize retirement. Every submitted unit in an update inventory must be checked. Removing failed units from a failed complete attempt does not establish completeness or confirmed newness.

## Exact Operation Authorization

### Request binding

Authorization evaluates an immutable operation request against one authority context and the actual checked owner results. It must retain and compare the complete applicable values, not invent a second hash/identity scheme.

| Operation | Required exact request binding |
| --- | --- |
| Source/registry read | Owner, declared scope, actual input collection and acquisition associations, context/profile inputs, and any pinned registry/history anchors |
| Initialization | Owner and destination, explicit new-owner confirmation, observed uninitialized binding, and the complete 017 empty genesis and its exact reference |
| Update | Owner/destination/registry identity, exact base, current inventory and context inputs, complete update artifact, exact computed result, invocation mode, and required explicit decision confirmations |
| Explicit identity choice | Owner/destination/registry identity, exact base and current inventory, complete identity action including IDs/occurrences/basis/reason, and the context under which the choice was presented |

An explicit choice may precede final plan construction, so its confirmation does not reference a future plan/result. Final update authorization checks every such confirmation against the actual actions and includes the complete plan/result. This preserves 017's non-circular history. Changing the base, inventory, action, owner, destination, relevant context, or required reason invalidates the corresponding confirmation; changing the final plan/result invalidates its publication permit.

Confirmations are issued only through the trusted host interaction/call path for a principal with `resolve-identity`. A serialized `ExplicitBasis`, nonempty explanation, `verified-edit` label, or development-mode flag is data to check, not a confirmation. Confirmation can resolve an otherwise ambiguous historical choice, but cannot waive 016/017 owner, occurrence, collision, one-to-one, source-validity, or transition checks.

The confirmer and publisher may be the same principal with both grants, or separately established principals. Every required confirmation must still be permitted by the active authority state when authorizing the new publication. Removing/changing the applicable grant or state requires reauthorization, not a fresh timestamp attached to old approval.

### Initialization

Initialization requires `initialize-registry`, an explicit new-owner request, and host-established evidence that the authoritative binding is uninitialized. A filesystem `not found`, corrupt payload, inaccessible destination, or absent checkout history is not that state. If the host cannot distinguish new from unavailable existing state, initialization is blocked.

The authorized result is exactly 017's empty genesis with a freshly host-allocated registry identity and no base/update references. Declaration ID allocation follows as a separate checked update. Publication must conditionally establish this binding only while it is still uninitialized; another initialization must not be overwritten. A recovery candidate with existing history is never rewritten as a new genesis.

### Manual and development updates

An explicit manual update request may accept the plan's verified continuations, independently confirmed new declarations, and complete-absence retirements without creating an explicit historical choice for every action. Any action using an explicit basis, including restoration, requires the exact authorized confirmation above.

Development auto-updates additionally require a host-established, explicitly enabled session for this owner/destination, as well as `update-registry`. They accept only 016's eligible verified continuations, confirmed-new allocations, and proven retirements. An ambiguous choice, explicit-basis action, restoration, initialization, or recovery cannot be auto-accepted; the host must use the appropriate separate explicit operation. A source annotation or plan member cannot enable the session, and normal compilation does not gain its powers.

All changes must satisfy 016/017 before a permit is produced. An unresolved decision blocks the whole proposed update. If decisions and lineage links are both empty, reuse the base without publishing a new snapshot or creating publication provenance for a nonexistent transition.

These permissions do not adopt an unsupported authoring form or enable a 016 form deferred beyond the first implementation. An implementation admits only its explicitly adopted 016 semantic subset, even when the caller holds every grant in this table.

## Publication Handoff to 029

| Step | Required behavior |
| --- | --- |
| Prepare | Retain the exact admitted source/base/context, resolve decisions, fix allocation candidates, and compute the complete 017 plan and resulting snapshot |
| Authorize | Evaluate the action grants and explicit confirmations for that complete request; return a scoped permit or a typed failure |
| Commit conditionally | The 029 host checks the permit/request association, still-applicable authority/session state, and exact current base or uninitialized binding as part of the same protected commit operation |
| Retain success | Make the resulting registry and its required publication provenance current consistently; preserve the old immutable history |
| Report/re-read | Return the actual publication outcome; subsequent read-only compilation admits the resulting exact snapshot without inheriting write power |

A permit is local to its request and authority context. It is not serialized into source or any existing 017 authoring artifact, and an imported object with matching fields cannot act as a permit. A cross-process or durable authorization representation requires a separately specified authenticated transport/record profile; it is not part of this minimum.

The commit check must cover authority changes as well as concurrent registry changes. If authorization/grants/session state changed before the publication point, the host must reauthorize or refuse publication. Checking once and then performing an unconditional write is insufficient. 029 chooses the storage and synchronization mechanism that enforces this condition; 018 does not prescribe a lock, database, filesystem path, or distributed protocol.

Missing current state, stale base, authority mismatch, or incomplete required checks must not change the current registry. The host never silently rebases, merges, selects another destination, initializes, or retries with regenerated IDs. After a conflict, replanning uses the actual new base and applicable authority, with refreshed confirmations where their bindings changed.

### Retained provenance and failure after commit

For every successful initialization/update, the host retains the exact owner/destination binding, host/authority state, publisher, operation/mode, base or uninitialized condition, applicable inventory/context, update/result references, and required confirmer/action associations. Initialization omits update-only fields; historical source and bodies required for claimed replay remain available through the owner's retained-input rules. A timestamp may be contextual but never substitutes for any exact binding or currentness check.

This is the minimum provenance content, not a new portable artifact envelope. Existing references use 017's representations. Its local persistence and protection belong to 029; arbitrary text in a repository file is not authenticated provenance. Receipts retain only safe non-secret actor/state identifiers, never session credentials or reusable publication capabilities. They record past facts and confer no future authority.

Successful publication must make the state/provenance relationship consistent at the host's declared publication point. If storage fails or cancellation occurs before it, no new current state is exposed. If acknowledgement is lost after a possible commit, the outcome is indeterminate until the host checks that exact transition; it must not report a guaranteed rollback or blindly publish the request again. The 029 design must define its crash-consistency and outcome-recovery mechanism before an implementation claims durable publication.

## Minimum Web Localization Extension

028 needs the local host to hold, separately, the powers that [021](./021-intlify-translation-store-and-governance-design.md#exact-base-atomic-publication), [022](./022-intlify-provider-and-localization-sync-design.md#requests-responses-and-acquisition-provenance), and [025](./025-intlify-release-assembly-and-deployment-design.md#release-publication) require: reading and publishing one Store lineage, making governance decisions, disclosing source context to one registered Provider adapter, publishing and activating one Release, and admitting generated code before execution. This extension adds those powers to the same trusted-local-host mode, authority context, request binding, permit, and provenance rules defined above. It adds no remote authentication, signature, delegation, or multi-tenant capability, and it does not change 016/017/021/022/025 semantics.

### Additional powers

The grant vocabulary grows by the following closed actions. Each is scoped to the context's owner and, where stated, to one exact Selection Scope, Store lineage, adapter, or destination binding. Holding one implies none of the others, and none implies a registry power.

| Action | What it permits | What it does not permit |
| --- | --- | --- |
| `read-store` | Admit one exact pinned or current snapshot of the owner's Store lineage and its evidence for analysis, planning, linking, synchronization, or publication views | Treating a historical snapshot as current, initializing, or writing |
| `initialize-store` | Publish one empty genesis for a separately confirmed new Store binding | Replacing missing or corrupt history or importing another lineage |
| `stage-candidates` | Submit a complete validated candidate batch as a `candidate-publication` transition | Approving, selecting, or committing the transition |
| `review-message` | Confirm one exact approval or rejection decision for one subject and policy basis | Selecting, revoking, or committing |
| `select-message` | Confirm one exact selection or replacement decision for one Selection Key | Reviewing, revoking, choosing among candidates, or committing |
| `revoke-evidence` | Confirm one exact revocation of one artifact or evidence record | Undoing a revocation or selecting a replacement |
| `publish-store` | Commit one checked transition against the exact current base of the lineage | Authoring or altering any decision in it |
| `disclose-source` | Project the permitted request fields of admitted source artifacts to one registered adapter | Disclosing occurrences, parameter values, Store history, policies, or unrelated messages |
| `invoke-provider` | Dispatch the admitted finite request list to one registered adapter identity/revision | Publishing candidates, choosing routes, or loading adapters from source |
| `publish-release` | Run one publication transaction for one Release to one destination binding | Activating, choosing the Release for deployment, or reusing another destination's permit |
| `activate-release` | Switch one destination's activation reference to one verified published Release under an expected-current condition | Publishing, rechecking views, or removing a previous Release |

Execution admission is host verification under the trusted local profile and requires no grant; it produces evidence, not authority. A synchronization actor that also reviews and selects must hold each power independently, as 021 and 022 require; holding `publish-store` never implies a decision power.

### Store, governance, and supply requests

Requests bind to exact values as in the registry operations above:

| Operation | Required exact request binding |
| --- | --- |
| Store read | Owner, Selection Scope, lineage identity, the exact snapshot reference or the current-anchor observation, and the purpose |
| Store initialization | Owner, Selection Scope, destination binding, explicit new-lineage confirmation, observed absence, and the complete 017 genesis |
| Governance decision | Owner, Selection Scope, exact base snapshot, complete decision record body including subject, key, expected prior head, support, and reason, and the presenting context |
| Store transition commit | Owner, lineage, exact base, transition class, complete additions, expected heads, invalidations, and the computed result |
| Disclosure and dispatch | Owner, Selection Scope, plan reference, Store base, routing policy reference, adapter identity/revision and configuration digest, and the complete request list |

A governance decision is confirmed only through the trusted host interaction path by a principal holding the matching power. A serialized `approval-record`, `selection-decision`, or `revocation-record` is data to check; its `actor` member records the confirmed principal after the fact and cannot substitute for confirmation. Changing the base, subject, key, expected head, support, or reason invalidates the confirmation. Committing requires `publish-store` and the still-current base; a stale base is a conflict, never a rebase.

The Provider-visible projection is a closed allowlist: Intent ID and revision, source and requested locale, exact source MF2, canonical parameter names, the declaration description, the response profile, and the correlation. Everything else the host retains is denied. A response is inert data; correlation proves association only, and an adapter's `trusted` or `approved` assertion is ignored. The adapter is registered through trusted setup with its identity, revision, and configuration digest; a routing policy that names an unregistered adapter blocks dispatch.

### Minimum Source Admission Policy

The Source Admission Policy body defined by 017 has one member, `sourceApproval`, with the following meaning:

| Value | Source artifact admitted for linking and publication when |
| --- | --- |
| `authenticated-sufficient` | The `source-locale-message` was derived by the trusted local compiler from a checked authoring inventory whose source units were acquired through routes permitted by the authority context, and its derivation references resolve to those exact retained artifacts |
| `required` | The conditions above hold and an `approval-record` whose subject is that exact source artifact under the applicable approval policy is the review head in the pinned snapshot, with no applicable rejection or revocation |

Authentication here means the 016/029 acquisition provenance retained by the host, not a signature. An unavailable inventory, a derivation reference to another artifact, or a route outside the context blocks admission under either value. The localized approval mode never waives a required source approval, and a source artifact never receives a selection decision.

Trust Policy and Resource Limit Policy bodies remain undefined by this extension. The authority context supplies the permitted routes, adapter registrations, destination bindings, and finite capacities that those policies would otherwise carry; a configuration reference to either kind remains a structurally admitted pin.

### Release publication, activation, and origin admission

`publish-release` binds to the exact `release-snapshot`, all member bytes, the destination binding, the publication policy, and the revocation view obtained under `read-store`. The permit covers one transaction and is invalidated by any change to those inputs or to the authority state before the destination adapter's commit point. `activate-release` binds to the exact `release-publication-record`, the destination, and the expected current activation reference; the permit is invalidated by an intervening activation or authority change. Neither permit is serialized, and a record's `publisher` or a manifest's presence at a destination is not a permit.

The local code-origin admission profile applies before any generated module is imported or any payload is decoded:

1. Resolve the activation reference to its record and snapshot and reverify that chain.
2. Read each required file from the host-owned immutable staged namespace for that Release identity; application code, the serving adapter, and the Provider have no write access to it.
3. Verify each file's bytes against the descriptor's file digest and byte length, and the descriptor against the snapshot.
4. Import from the verified bytes, or serve them through a boundary that rechecks the digest on every read; a mutable URL, a matching filename, a cached copy outside the namespace, or a successful import is not verification.
5. Produce execution-admission evidence naming the destination, record, Release, member, verified files, and verifier revision, and hand it to 027 or the AOT constructor as input.

Substitution between verification and use, a file outside the descriptor, a digest mismatch, or a namespace not owned by the host fails admission for that member; nothing is rendered from unverified bytes. The profile protects against unverified or swapped local files under the trusted-host assumption; it does not protect against a compromised host or claim remote signing.

### Provenance and disclosure

Store transitions, Release publications, and activations retain provenance with the same minimum content as registry publications: owner and scope, host and authority state, the confirmed principals and their actions, exact base or expected reference, the complete request and result references, and the actual outcome. Store and destination provenance are host-private under 029's rules; only the safe identifiers that 017 records carry. Source MF2, descriptions, decision reasons, Provider responses, and paths remain untrusted potentially sensitive data outside default diagnostics and telemetry.

## Failures and Disclosure

| Failure category | Required distinction |
| --- | --- |
| Authority unavailable/invalid | Missing establishment, unsupported local mode, malformed authority bindings, or unverified caller identity; not an anonymous allowed principal |
| Action denied | No matching grant, wrong owner/destination/mode, or an explicitly forbidden origin; not malformed MF2 |
| Input unavailable/invalid | Missing required body/history, bad integrity/schema/version, conflicting content, or invalid acquisition association; preserve the owning cause |
| Identity confirmation missing/invalid | A serialized reason without authorized confirmation, changed confirmation inputs, or an unpermitted historical choice |
| Update semantically blocked | Invalid continuity/newness/absence, collision, failed units, or unresolved associations under 016/017; authorization cannot repair it |
| Publication conflict | Stale registry base, competing initialization, or changed authority/session at commit; the prepared permit is not success |
| Disclosure denied | A requested source or context field exceeds the permitted Provider-visible projection; not an acquisition failure |
| Origin unverified | Generated bytes were not obtained from the verified immutable namespace or changed between verification and use; not a formatting failure |
| Operational failure | Capacity exhaustion, cancellation, or host/storage failure; distinguish a proven uncommitted operation from an indeterminate publication |

The initial evaluator has fixed stages: authority/request admission, input/provenance admission, owner semantic checks, exact-operation authorization, and host conditional commit. A blocked earlier stage prevents dependent stages from claiming success. Capacity/cancellation stops evaluation with its operational cause; bounded diagnostics may retain independently established causes but never an accepted partial permit. Within a stage, order safe diagnostic references canonically using the owning exact identity/reference rules rather than collection traversal order. 016/019 own their common diagnostic projection; this document introduces no general diagnostic envelope.

Source text, descriptions, explicit-decision reasons, paths, and submitted actor labels remain potentially sensitive untrusted data. They are not executable instructions or default telemetry/profiler labels. Detailed source/history bodies needed for local verification stay in authorized retained inputs, not copied into security reports. An input rejected before safe admission must not be exposed verbatim or directly hashed into a public diagnostic identity. Retain only the applicable owning disclosure-safe projections.

Intlify must not attach private keys, bearer credentials, authentication handles, or authority-minting capabilities to source, registry, Intent, report, or ordinary compile inputs. User-supplied source may itself contain sensitive text; admitting it for analysis does not classify that text as safe for disclosure. Read-only workers receive the narrow checked verification projection rather than the publication context or its storage handle. The design does not claim to sandbox arbitrary code with access to the trusted host process.

## Resource Protection and Performance

Apply [026's performance architecture](./026-intlify-conformance-and-measurement-design.md#performance-implementation-architecture) alongside 016's [Security and Resource Limits](./016-intlify-source-authoring-and-intent-identity-design.md#security-and-resource-limits) and [Performance and Measurement](./016-intlify-source-authoring-and-intent-identity-design.md#performance-and-measurement).

- Bound authority/grant/origin entries, input bytes/depth/counts, history resolution/replay, confirmation/action comparisons, and retained diagnostics/provenance before or during protected work. The active implementation profile must supply finite typed limits; no unknown or missing limit means unlimited work. Existing 015/016/017 limits keep their owner-defined accounting.
- Inputs cannot enlarge the authority or capacity used to admit themselves. This subset does not redefine 015's bootstrap Resource Limit Policy Verification Input or authorize substituting its local invocation context for that input.
- Index exact owner, grant, origin, and artifact bindings; avoid all-pairs history search or repeated decoding per declaration. Account for duplicate submissions before any safe internal reuse.
- Reuse immutable integrity/semantic facts only for identical content and validator/specification inputs. Admission caches also bind the applicable context/origin/policy; content-only cache hits do not establish authority. Never reuse a publication permit solely because the plan digest matches: grants, session, destination, and currentness require their live checks.
- Keep immutable source/authority/history storage, invocation Scratch Workspace, returned checked results, and host commit state in separate lifetimes. Reset scratch after success, denial, cancellation, and failure without retaining prior grants or confirmations in the next invocation.
- Use safe bounded data structures first. SIMD, unsafe code, custom arenas, remote caches, and bundled locale data are not requirements of this subset. Internal fast hashes do not replace 017 identities or integrity checks.

The first measurement cases distinguish authority/input admission, authorization of an already owner-checked request, and 029's publication I/O. Retain logical work counts, exact admitted/denied outcomes, and fresh/reused equivalence. Do not count 016 parsing/reconciliation twice or describe publication latency as core authorization time. The adopting implementation pins its method, workload, checksum projection, resource domains, and required 026 records before accepting observations. Numeric performance budgets are not introduced here; optional profiling remains isolated under 026.

## Conformance and Fixtures

Fixtures must invoke actual evaluators, semantic validators, and host condition checks; a mock returning an expected allow/deny value is insufficient.

| Area | Required cases |
| --- | --- |
| Establishment | Explicit local authority succeeds; missing mode/context, artifact-asserted principal/grants, duplicate/conflicting grant entries, self-authorizing policy references, and production use of test contexts fail |
| Least authority | Read-only analysis succeeds without write grants; each missing initialization/update/explicit-choice grant is denied independently; one grant does not imply another |
| Input binding | Exact acquired source/base succeeds; wrong owner, scope, registry identity, destination, source bytes, provenance route, or context fails; forged flags cannot bypass admission |
| Historical admission | A pinned authorized older base can be read; a self-consistent unanchored chain, unavailable replay inputs, and treating the older base as current fail |
| Genesis | Explicit new-owner empty genesis succeeds once; missing/corrupt/inaccessible existing state is not new; concurrent initialization rejects the stale attempt |
| Confirmations | Exact authorized explicit choices succeed; a reason alone, wrong actor/grant, altered base/inventory/action/context, and automatic restoration are rejected |
| Automatic updates | Explicitly enabled eligible continuation/allocation/retirement succeeds under 016; disabled/ended sessions, ambiguous choices, implicit initialization/recovery, and partial-inventory retirement fail |
| Owner semantics | Valid authorization cannot override collision, failed source units, invalid one-to-one continuation, or unresolved decisions; partial proven updates preserve unseen entries |
| Publication isolation | Stale-base, changed-authority, wrong-destination, mutated-result, and reused-permit-for-another-request attempts cannot publish; an empty plan has no new publication |
| Persistence outcome | State/provenance agree after success; failure before commit preserves the previous current state; lost acknowledgement remains indeterminate until exact reconciliation rather than blind retry |
| Storage and reporting | Exact/first-over limits, repeated hostile inputs, cancellation/failure followed by workspace reuse, and secret-bearing reasons/labels preserve fail-closed behavior and safe reporting |
| Store, supply, and Release powers | Each Store, governance, disclosure, Provider, publication, and activation power denied independently; a decision without publication power and the converse; an automatic policy actor only with independently held powers; source admission under both policy values |
| Origin admission | Verified bytes imported; substituted, mutable-URL, cached-outside-namespace, and unverified bytes rejected before import; admission evidence carries no write power |

Required observations include exact owner references, accepted versus denied operation, unchanged current state after proven pre-commit failure, required provenance after success, and zero Provider/network/source-execution calls. Tests must vary the actual grants, inputs, confirmations, and host state rather than dispatching solely on test names. Conformance covers only the adopted local profile; it establishes neither signature verification nor distributed trust.

## Adoption with 016

| Adopting work | Use of this design | Remaining prerequisites |
| --- | --- | --- |
| Phases 1–2 | Finite input/provenance checks and strict test-context separation; ordinary analysis receives no write capability | Existing 016/017 analysis representations and active 026 checks; no need to complete production authentication or all Policy bodies for a declared test-only slice |
| Phase 3, pure core | Evaluate scoped permissions and exact explicit choices with actual 016/017 history checks; keep proposed results separate from publication | Concrete supported continuity verifiers and independently established test authority; correct codecs alone do not prove authorization |
| Phase 3, local publication | Explicit local authority, empty-genesis/update authorization, protected current-base/authority checks, and retained provenance | Applicable checked 015 inputs/policies, a real 029 establishment/acquisition/publication adapter, and its persistence/failure tests |
| Phase 4–5 and 028 | Apply the Web extension's powers, Source Admission evaluator, disclosure projection, publication/activation permits, and origin admission through the same local host | Actual 017 Web representations, 021/022/025 operations, and the 029-style Store, destination, and deployment adapters; library/import and remote trust remain separate |

This is a shared specification adopted within those phases, not another product milestone. A plan must list the necessary 015/017 policy/representation additions and 029 guarantees before the production operations that use them. Its claims must distinguish a tested in-memory evaluator, actual local publication, and full production Profile or Web integration. No broader pending security feature is required merely to start 016's bounded source-analysis implementation.

## Decision Log

| ID | Decision | Rationale |
| --- | --- | --- |
| 018-001 | Limit the initial detailed scope to one local application's source/Intent registry | Unblocks bounded 016 work without designing all supply, Release, or distributed trust workflows |
| 018-002 | Require an explicitly established local authority mode independent of analyzed inputs | Allows local operation without treating unsigned data or a repository location as self-authenticating |
| 018-003 | Separate source/read, initialization, update, and explicit-choice powers | Keeps normal compilation read-only and prevents an update grant from silently authorizing history reset or ambiguous reuse |
| 018-004 | Bind confirmations to exact choices and permits to the complete checked request/result | Prevents labels, stale decisions, or changed output from being substituted after authorization |
| 018-005 | Require current base and still-applicable authority at the protected publication point | A valid prepared plan or earlier permission check cannot justify a stale or revoked write |
| 018-006 | Retain publication provenance separately from 017 authoring content and future authority | Preserves non-circular artifacts and distinguishes historical facts from reusable permission |
| 018-007 | Leave unavailable recovery and cross-process authentication unsupported | Avoids replacing missing trusted history with new IDs or pretending local checked objects are portable credentials |
| 018-008 | Apply bounded admission, cache scoping, and 026 verification from the first adoption | Performance reuse must not leak authority, skip checks, or create successful partial results |
| 018-009 | Add closed Store, governance, supply, publication, and activation powers to the same local mode, each held independently | Lets one local host run the 028 workflow without a decision power implying a commit power or the converse |
| 018-010 | Define the minimum Source Admission Policy as authenticated-sufficient or required, with authentication meaning retained acquisition provenance | Makes source use checkable locally without signatures or a policy language |
| 018-011 | Restrict Provider disclosure to a closed request projection and treat responses as inert | Keeps occurrences, values, history, and credentials out of supply requests |
| 018-012 | Require verify-then-import from a host-owned immutable namespace before generated code runs | A checksum, filename, or successful import cannot establish code origin |

## Deferred Follow-Up Notes

- Complete common Trust/Source Admission/Resource Limit Policy bodies, resource-bootstrap verification input representation/admission, exact shared policy/provenance encodings, and their 015/017 adoption. The local authority context above is not one of those artifacts.
- Add cryptographic authentication/signatures, external identities, scoped delegation, key rotation, durable/cross-process authorization, remote publication, and stronger hostile-host/storage models only with explicit profiles.
- Specify authenticated recovery/current-anchor installation, rollback, source-control conflict UX, history retention/compaction, and migrations with 016/017/029 rather than treating initialization as recovery.
- Add library/publisher trust, reviewer roles and quorum, remote Store/Release services, production deployment credentials, and their exact provenance under the owning integration designs; the local Web extension covers only the 028 minimum.
- Extend safe disclosure and security evidence transport with 019/026; broader audit/query services and trusted-runner attestation are not requirements for the local evaluator.

## Relationship to Other Documents

- [000 — Security, trust, and reproducibility](./000-intlify-overview-design.md#security-trust-and-reproducibility) supplies the least-authority and untrusted-input principles; this design covers only their first local authoring use.
- [015 — Consumer input boundaries](./015-intlify-project-profile-and-locale-policy-design.md#consumer-input-boundaries) supplies the checked project facts and applicable binding projections without mutation credentials or implicit trust bootstrap.
- [016 — Identity registry and reconciliation](./016-intlify-source-authoring-and-intent-identity-design.md#identity-registry-and-reconciliation) supplies valid associations, explicit updates, newness/absence, and the initialization/recovery distinction.
- [017 — Registry snapshots and update plans](./017-intlify-shared-artifact-and-version-admission-design.md#registry-snapshots-and-update-plans) supplies exact stored facts and replay rules; those representations do not carry caller authority.
- [026 — Performance implementation architecture](./026-intlify-conformance-and-measurement-design.md#performance-implementation-architecture) supplies the applicable storage, lookup, measurement, and profiling requirements.
- [021 — Exact-base atomic publication](./021-intlify-translation-store-and-governance-design.md#exact-base-atomic-publication), [022 — Requests, responses, and acquisition provenance](./022-intlify-provider-and-localization-sync-design.md#requests-responses-and-acquisition-provenance), and [025 — Release publication](./025-intlify-release-assembly-and-deployment-design.md#release-publication) consume the Web extension's powers and permits; their operation semantics remain theirs.
- [028 — Ownership and dependencies](./028-intlify-javascript-web-vertical-slice-design.md#ownership-and-dependencies) requires production supply/Release security beyond this minimum. [029](./029-intlify-product-workflow-and-packaging-design.md) implements the local host obligations rather than redefining authorization semantics.
