# Intlify Product Workflow and Packaging Design

## Purpose

This design defines the smallest local host needed to run [016](./016-intlify-source-authoring-and-intent-identity-design.md)'s source-authoring and persistent-identity operations under [018](./018-intlify-security-trust-and-provenance-design.md)'s authorization rules. It turns already defined compiler and security decisions into concrete acquisition, initialization, update, and persistence workflows.

For example, an application starts with one explicitly initialized empty registry. An update captures source, obtains a checked identity plan, and publishes registry `R1`. Ordinary compilation can then read `R1` without allocating IDs or writing registry state. If another update publishes `R2` while a change against `R1` is being prepared, the stale change is rejected rather than overwriting `R2`.

| Layer | Question answered |
| --- | --- |
| 016/017 | Is the source analysis and exact identity transition valid, and how is it represented? |
| 018 | Are the inputs, actor, explicit choices, and requested publication permitted? |
| 029, this subset | How does the local host acquire those inputs and safely make the authorized result current? |

The initial deliverable is an internal local-host integration with a bounded filesystem persistence profile and real failure tests. It is not a public CLI or the complete product packaging design. The same protocol is reused, under [Store lineage and Release destination adapters](#store-lineage-and-release-destination-adapters), for the local Translation Store and Release destination that 028 needs. [000](./000-intlify-overview-design.md#detailed-design-traceability)'s wider 029 productization work remains downstream of 028; these earlier host obligations are adopted by 016 Phase 3 and later reused by the Web integration.

## Goals

- Supply immutable source, project-context, and registry inputs without moving discovery or filesystem access into the semantic core.
- Instantiate 018's trusted-local-host mode without accepting actor/grant labels from analyzed data.
- Separate read-only analysis, explicit new-owner initialization, update preparation, identity confirmation, and publication.
- Enforce one registry chain per application owner, exact-base updates, and still-applicable authority at the publication point.
- Persist the registry, required replay inputs, and publication provenance consistently without rewriting source files or managing translation catalogs.
- Distinguish a proved uncommitted failure, a completed publication, and an indeterminate outcome after interrupted I/O.
- Reuse existing acquisition/storage foundations selectively and apply 026 without requiring a database, service daemon, or general workflow engine.
- Reuse the same generation, pointer, and outcome protocol for the local Store lineage and Release destination adapters that 021 and 025 require, without redefining their semantics.

## Non-Goals

- Public command spellings, package/import names, install flows, repository-wide discovery, monorepo selection UX, bundler plugins, watch scheduling, or CI/release automation.
- Completing 015 policy bodies, 017's later artifact families, or the authentication/signature profiles deferred by 018.
- Translation synchronization, governance decisions, Release assembly, generated-code execution, or claiming the complete 028 scenario; the Store and destination adapters below reuse this host's persistence protocol without redefining 021 or 025 semantics.
- Cross-owner/library operations, remote storage, network filesystems, distributed writers, or cross-process transfer of live authorization capabilities.
- Restoring missing registry history from Git/backups, selecting a rollback, migrating stores, compacting history, or garbage-collecting retained inputs. Resolving the outcome of this host's interrupted transaction is a different, supported operation.
- Promoting source-controlled copies, PR #183 IDs, or existing resource-output manifests into the source-first registry specification.

## Ownership and Dependencies

| Owner | Responsibility in this subset |
| --- | --- |
| 015 | Configuration resolution, checked project identity and applicable binding projections; fixture contexts remain explicitly test-only |
| 016 | Source facts, completeness, continuity/newness/absence, identity choices, and valid registry transitions |
| 017 | Shared source/inventory/Intent/registry bodies, identities, integrity, and exact replay |
| 018 | Authority establishment and grant semantics, input-origin admission, confirmations, permits, and required publication provenance |
| 019 | Later discovery/query/scheduling and common diagnostic projections; no general graph service is required here |
| 021/025 | Store transition and Release publication/activation semantics; 029 supplies only their local persistence and conditional commit through the adapters below |
| 026 | Applicable conformance, storage/performance architecture, measurement, and profiling isolation |
| 028 | The later bounded Web integration, not a prerequisite for the local identity host |
| 029 | Explicit input selection, local caller/destination binding, operation orchestration, private persistence records, conditional publication, and outcome reporting |

Production authoring still requires the actual checked 015 inputs and applicable policy bodies identified by 016/018. This host cannot manufacture them from `intlify.config.json` references, PR #205's partial configuration core, or a fixture flag. The first harness can exercise actual local authorization and storage with test-owned inputs while reporting that limited scope honestly.

## Terminology

| Term | Meaning here |
| --- | --- |
| Host domain | An explicitly selected trusted local authority and protected storage namespace; not the current directory or repository checkout |
| Owner binding | The host-established association of one application owner, owning inventory scope, and one authoritative registry destination |
| Acquisition snapshot | A finite immutable collection of actual source/context bytes and their supplied identities for one invocation |
| Prepared update | The retained checked base, inventory, decisions, complete 017 plan/result, and context awaiting final authorization/publication |
| Host generation | One immutable private control record describing the host binding, authority configuration, current registry, and committed transition provenance |
| Current pointer | The small protected record selecting exactly one Host Generation; not a mutable tag accepted by a compiler artifact reader |
| Pending transaction | A durable host-owned description of one attempted pointer transition used only to determine its outcome |
| Transaction outcome resolution | Checking whether that exact attempted transition committed; not importing, reconstructing, or rolling back registry history |

These terms and operation labels do not reserve public APIs or filenames. Host-local generations/operation tokens are not `MessageIntentId`, `IntentRevision`, or a new shared 017 artifact kind.

## Minimum Host Inputs and Establishment

The trusted invocation/setup path supplies:

- an explicitly selected host domain and local storage capability;
- one application `OwnerIdentity`, owning inventory scope, and a new-or-existing owner-binding request;
- the authenticated local caller binding, admitted authority configuration, finite grant map, and session state required by 018;
- explicit configuration selection or an already checked 015 projection and its actual applicable bindings;
- a finite source-unit acquisition map, grammar/Producer/verifier profiles, exact contextual inputs, and requested complete or partial scope; and
- finite acquisition, storage, validation, reporting, lock-attempt, and retained-input capacities.

The first entry is a trusted native/in-process integration call. The embedding host establishes the local principal from its selected session/call mechanism and gives the operation a non-serializable scoped handle. Source imports, environment-derived actor names, JSON request labels, and stored receipts cannot construct that handle. A command-line frontend may later create the same invocation, but no such command or remote authentication protocol is defined here.

The host domain retains its owner-to-destination binding independently of analyzed source. Worktrees or application-package paths with the same owner must use that binding; changing a path or analysis scope cannot allocate another collision domain. The minimum is one application per invocation, not automatic global discovery of every copy of a project on disk.

Opening an existing binding is read-only. Missing or inconsistent protected control state produces `unavailable`, never a new-owner selection. Creating an owner binding requires an explicit trusted setup request confirming a genuinely new owner and the applicable initialization grant. The host rejects an existing registration and conditionally records the new binding under its domain control lock. A missing directory alone is not the confirmation. Interrupted enrollment must remain recognizable as incomplete; it cannot be retried as an unrelated new owner or silently rebound elsewhere.

The first storage location is an explicitly supplied host-managed directory outside analyzed source and its tracked registry copies. Its access controls, safe directory handles, and trusted setup path implement 018's local-host assumption. Files remain untrusted until their format, integrity, binding, and provenance are checked. This is not protection against a compromised host or an actor able to replace protected control state.

### Authority and session lifecycle

The protected host state identifies the current complete, non-secret authority configuration. A local session derives 018's exact invocation context from that configuration and the admitted operation inputs; it does not persist credentials, live permits, or confirmation capabilities. The caller supplies actual policy bodies and supported validators where required, not a callback that grants everything.

Changes to persistent grants, permitted origins, authority configuration, or owner bindings are trusted host-management operations, not effects of analysis or registry-update permissions. They use the same protected publication serialization as registry writes. Such a change advances the host generation/authority revision while preserving the exact current registry and its history. A registry publication never changes its own grants.

Invocation/session changes are serialized by a host-local guard. Final publication checks both the current protected authority configuration and the active invocation/session context. Session expiry or closure invalidates its handles; a process restart establishes fresh local authority and does not resurrect confirmations or development mode from disk. Historical provenance remains historical, not permission for a new write.

## Acquisition and Read-Only Analysis

### Configuration and source capture

Use explicit selection, not upward-directory search or an inferred package root. A file-based configuration entry reads the selected `intlify.config.json` bytes and passes them through 015's entry path; an already checked entry supplies its matching retained inputs. Schema selection, Profile selection, locale defaults, and canonicalization stay with 015. There is no fallback from failed resolution to a synthetic production context.

The source acquisition map explicitly pairs each owner-local source-unit identity with a permitted file or immutable buffer and exact grammar selection. Repeated captures preserve the supplied unit/revision rules under 017. File paths locate bytes; they neither assign persistent Intent IDs nor prove continuity. Move/edit evidence is a separate input consumed by the supported 016 verifier.

For file inputs, the adapter opens bounded regular files through the admitted directory capability, reads complete bytes under explicit limits, and freezes those bytes before hashing or parsing. It rejects unsupported special files, escaping/symlinked routes under the selected no-follow profile, duplicate/conflicting unit entries, and an acquisition that cannot establish its claimed immutable revision. Size/timestamp checks alone do not prove byte equality or stable history. Already supplied snapshots are verified against their actual bytes rather than reopened opportunistically.

The acquisition result records the exact finite membership and failed/unavailable members. A complete attempt with a failed member is not relabeled complete after removing it. Partial invocations identify their own scope and cannot prove retirement. The captured collection is not a claim of a globally atomic live filesystem snapshot: the operation uses those retained revisions, not whatever is currently at a path. A host-observed invalidation cancels or supersedes the prepared request; it does not silently substitute new bytes under an old confirmation.

Acquisition never evaluates application code, getters, template substitutions, or arbitrary executable configuration. Shared 016/017 functions receive inert retained values and no filesystem, network, acquisition callback, or publication handle.

### Read path

1. Open the existing owner binding without creating directories, lock files, current pointers, or registry roots.
2. Under a bounded shared storage lock, inspect the current pointer and any pending transaction. A pending or ambiguous publication blocks a normal current-state result and reports that explicit outcome resolution is required.
3. Pin the complete selected generation, exact registry, and required retained provenance/input references. Release the lock only after the immutable read set is retained; no automatic history deletion is available in this subset.
4. Run 018 provenance/admission plus 017/016 decoding, source association, and replay checks for the requested scope.
5. Return checked authoring facts, a reconciliation-required/blocked result, or a typed operational failure. Never allocate IDs, accept identity choices, stage a transaction, initialize, or repair storage as a compilation side effect.

An explicit historical read can use a separately admitted pinned anchor without claiming it is current. It never updates the current pointer. A test-only analysis without a registry may exercise 016 Phases 1–2; it cannot claim persistent-identity support.

## Operations and Results

| Operation | Required behavior | Result, not an implied next action |
| --- | --- | --- |
| Analyze/read | Capture or accept exact inputs and read an admitted registry when required | Checked/blocked authoring result with exact scope; no publication |
| Initialize | Use an explicit admitted new-owner binding and `initialize-registry` | Empty 017 genesis published once, or a failure; declaration allocation remains a separate update |
| Prepare update | Analyze captured inputs, run supported continuity checks, obtain fixed host allocation candidates, and construct the complete 017 plan/result | An immutable prepared update or unresolved decisions; not a publication permit |
| Confirm identity choice | Present the exact base, inventory/context, action, occurrences/IDs, and reason through the trusted host interface | 018 confirmation for that exact choice from a principal with `resolve-identity` |
| Publish update | Obtain `update-registry` authorization and conditionally install the complete prepared result | Published, unchanged, conflict/denied/blocked, or operational outcome |
| Publish Store transition | Obtain `publish-store`, or `initialize-store` for genesis, and conditionally install one checked 021 transition and its resulting snapshot | Published, unchanged, conflict/denied/blocked, or operational outcome for that lineage |
| Publish Release | Obtain `publish-release` and conditionally install one 025 Release with its record at the destination | Published, unchanged, conflict/denied/blocked, or operational outcome for that destination |
| Activate Release | Obtain `activate-release` and switch the activation reference under the expected-current condition | Activated or conflict/denied/blocked; the published Release set is unchanged |
| Enable/end development updates | Explicitly establish or end a scoped local session under 018 | Eligibility to submit the narrowly allowed update class, not permission for ordinary compilation to write |
| Inspect/resolve transaction outcome | Examine the named local operation and its retained state under fresh authorized host access | Proof of committed/uncommitted state or indeterminate/blocked; never a fresh publication of the request |

The host exposes a result before any optional next workflow begins. Analysis does not imply initialization; preparation does not imply acceptance; a confirmation does not imply publication; publication does not translate or build the application.

## Initialization and Update Workflow

### Initialization

The owner-binding state is explicit:

| State | Permitted interpretation |
| --- | --- |
| Not enrolled | No owner assertion has been established in this host domain; only explicit trusted new-owner setup may create a binding |
| Enrollment incomplete | The protected new-owner association exists, but its initial control generation has not been durably established; resolve or explicitly resume that same enrollment, without rebinding the owner or initializing a registry |
| Admitted uninitialized binding | A recorded new-owner decision exists and no registry chain has been published for it; explicit initialization may proceed |
| Active | One exact registry identity/current snapshot and publication provenance exist; reads and updates use that chain |
| Unavailable/inconsistent | Expected control/history cannot be validated or acquired; stop writes and do not reinterpret this as either preceding state |

Initialization creates exactly 017's empty genesis with a newly generated registry identity. It requires the uninitialized binding, 018 authorization, and the conditional publication protocol below. It does not scan the source and assign all IDs as a hidden step. Two initializations against the same uninitialized state cannot both become current.

### Preparation, choices, and update modes

Preparation pins the admitted current registry, acquisition inventory, context, and supported specifications. New ID bytes come from the 017-prescribed host randomness only after an explicit update workflow reaches an eligible allocation; they are retained in the plan and collision-checked against both active and retired IDs. Randomness failure or collision fails that candidate/plan. Revalidation/replay never regenerates its values.

An explicit identity confirmation precedes final plan acceptance where needed. The host binds it to exactly the values required by 018, with no future plan/result reference. Changes to those inputs require a new confirmation. Serialized `ExplicitBasis.reason`, source-control messages, or choosing the highest similarity score cannot substitute for it. Multiple unresolved choices block the whole proposed publication; validated inspection facts may still be shown.

Manual updates may accept verified continuations, independently confirmed new declarations, and complete-absence retirement. Explicit-basis actions and restoration require their own authorized confirmations. Development mode accepts only 018's eligible automatic cases in an explicitly enabled live session; ambiguous history, restoration, initialization, and recovery remain separate explicit work. It introduces no watcher, scheduling policy, or persistent auto-enable setting.

A partial but fully checked inventory may update proven covered associations while retaining unseen entries; it cannot retire them. Failed units, unknown continuity verifiers, unavailable exact history, unsupported semantic choices, or any required unresolved decision prevent a publishable result. 016 forms deferred beyond the first implementation are not enabled by this host design.

An empty decision/link plan reuses the current snapshot without a new registry publication. Context-only revision changes and new analysis evidence do not require registry mutation. If the observed base or authority changes before an unchanged result is returned, report that staleness rather than claiming the originally observed state is still current.

## Local Persistence Profile

### Immutable generations and one current pointer

Use a small protected current pointer and immutable generation/payload storage. This reuses 017 artifact bodies instead of storing another editable ID catalog. Authority-only host-management transitions and registry transitions share the control-generation mechanism, but only registry transitions create registry publication provenance.

| Storage role | Required content and constraints |
| --- | --- |
| Owner binding and stable lock | Host domain/destination identity and exact owner/scope; a persistent lock object whose identity is checked and never replaced while the binding is active |
| Immutable payloads | Actual 017 artifacts, source snapshots, required contextual inputs, and historical verification material; shared exact bytes may be stored once without dropping logical input occurrences |
| Host Generation | Private layout/profile revision, destination/owner binding, previous generation reference, operation token/kind, exact authority-configuration reference, uninitialized or exact active registry state, required payload references, and applicable 018 publication provenance |
| Current pointer | The complete reference to one stored generation, including its exact stored-byte length and integrity; never a directory scan, timestamp, or highest filename |
| Pending transaction | Operation token, exact expected and candidate generation references, operation kind, and owner/destination binding; only first control-generation enrollment may use an explicit expected-absent condition backed by the independently retained trusted new-owner association |

The current generation transitively retains the exact registry base/update/inventory/result chain and all inputs necessary for the claimed replay. A retained reference without an available required body is not valid persistence. No permitted read treats a staging directory or unreferenced generation as current merely because all its digests match.

Host control records have a closed implementation-private versioned layout with bounded decoders and independent fixtures. The implementation fixes its complete field/variant schemas before writing that layout; unknown versions/fields and mismatched owner/role associations fail closed. Local file references bind exact stored bytes using full SHA-256 and byte length; they are not 017 semantic revisions or proof of authorization. Inner artifacts retain their existing 017 identity/integrity rules. Logical references resolve through fixed safe storage roles, never through paths supplied by an artifact. Basenames and public distribution formats are not reserved by this design.

A local operation token is allocated once by the host, retained with the exact request association, and used only to identify that attempted publication and its outcome. It is neither authority nor an Intent ID. Reusing a token with different inputs/results is a conflict. A permit/confirmation capability never enters these files: persisted confirmer/authority information records past facts under 018 and cannot recreate a live capability.

### Required storage capabilities

The first durable adapter supports only admitted local filesystems with checked directory-relative/no-follow access, bounded regular-file I/O, cooperative shared/exclusive OS locking, exclusive temporary creation, same-filesystem atomic replacement of a regular pointer file, and the file/directory durability operations required by this protocol. It rejects unsupported capabilities rather than degrading to remove-then-rename, copy-over, timestamp locks, or an in-memory success result.

All participating readers, publishers, and persistent authority-management operations use the same stable lock for the host binding. Verify the opened root and lock identities; do not replace a locked file, follow a changed path, or treat a corrupt lock/control record as absent. A missing lock for an established binding is an unavailable/control-conflict state. Bounded nonblocking acquisition attempts return busy/cancelled rather than waiting indefinitely. Arbitrary non-cooperating writes remain outside 018's trusted-host assumption.

Filesystem calls are not, alone, a portable durability guarantee: Rust documents platform-dependent [file-lock behavior](https://doc.rust-lang.org/std/fs/struct.File.html#method.lock) and [rename constraints](https://doc.rust-lang.org/std/fs/fn.rename.html), and explicit [file synchronization](https://doc.rust-lang.org/std/fs/struct.File.html#method.sync_all) is distinct from merely dropping a file. The adopting adapter must pin and test its platform/filesystem profile, including directory durability, before claiming durable publication. Unsupported targets may still run in-memory conformance tests but cannot claim this disk profile.

## Conditional Publication Protocol

Preparation and user interaction happen outside the storage critical section. The host retains immutable checked values so final publication needs no arbitrary callback, source execution, or reparsing to determine what was authorized.

1. Acquire the destination's exclusive storage lock and resolve any earlier pending transaction before accepting another write. Acquire the host-local authority/session guard after the storage lock; all paths use this order and avoid reentrant calls.
2. Read and validate the current generation, binding, and authority configuration. Compare the complete expected generation, exact registry base/identity or uninitialized condition, and active session/context. Even an authority-only intervening generation invalidates the prepared publication; do not silently rebase it.
3. Apply 018 authorization to the exact prepared request/result and required confirmations. Keep the guards held so persistent authority changes and session closure cannot interleave between this final check and the publication point.
4. Save any newly required immutable payloads and the candidate generation in exclusively created host-owned staging objects. Flush their file data and directory entries, install them without overwriting unequal existing content, and flush the installation. Before pointer publication every required object must be complete and durable. Bound this work; do not expand a reference by fetching a network or source path.
5. Write the complete pending record to an exclusively created temporary, validate and flush it, then atomically install the pending marker without overwriting an existing one and flush its directory entry. Prepare and flush a temporary current-pointer file naming the candidate generation. Partial temporaries are never accepted as pending records or current state. Failure here has not switched the pointer; retain the exact owned objects needed for outcome resolution.
6. Atomically replace the current-pointer file under the held guards. This is the logical publication point: the complete registry/provenance/authority association switches together, never separate registry and receipt writes. Do not unlink the old pointer first.
7. Complete the pointer/directory durability operation. Only after it succeeds may the host acknowledge durable `published`. Remove the exact pending marker and owned pointer temporary when safe, syncing the affected directory; never remove retained committed history as cleanup.
8. Release the guards and return the actual outcome with its exact resulting references. An error clearing a marker after proved durable publication is `published` with maintenance required, not a rollback. Any later analysis uses a fresh read-only invocation.

The generation protocol also protects explicit trusted enrollment/authority-management updates. Such control transitions cannot change registry entries or manufacture an 018 grant from a source request. First enrollment conditionally retains the trusted new-owner association under the domain control lock before publishing its first uninitialized control generation. Only this transition may expect an absent pointer; its setup authority is checked against that retained association and the live trusted session. Later registry initialization expects the exact existing uninitialized generation, never absence. Incomplete enrollment keeps its owner/destination reservation; an existing or unavailable binding is never overwritten by treating the expected pointer as absent.

The host may precompute/encode outside the lock, but publication must keep the final guards through the pointer switch and its durability decision. A cancellation before the switch stops that write. Once replacement has been attempted, cancellation cannot justify discarding transaction state or promising that nothing was published; the host completes bounded outcome handling or reports indeterminate state.

### Store lineage and Release destination adapters

028 needs two more protected destinations in the same host domain: the owner's Translation Store lineage that [021](./021-intlify-translation-store-and-governance-design.md#exact-base-atomic-publication) publishes, and the Release destination that [025](./025-intlify-release-assembly-and-deployment-design.md#release-publication) publishes to and its deployment host activates. Both reuse this host's owner binding, stable lock, immutable payload storage, Host Generations, one current pointer per destination, pending transactions, and the eight-step protocol above. 021 and 025 own what a transition means; this host owns only how it becomes current.

| Destination | Generation carries | Exact-base condition | Powers at the commit point |
| --- | --- | --- | --- |
| Store lineage | The current `store-snapshot` reference, the applied `store-transition`, the retained member and evidence payloads, and Store publication provenance | The prepared transition's `base` equals the current snapshot; genesis expects the explicit uninitialized binding created for `initialize-store` | `publish-store` for a transition and `initialize-store` for genesis; decision powers never substitute |
| Release destination | The set of published Release identities with their immutable member files, the canonical manifest bytes, and each `release-publication-record` | The Release identity is absent, or present with equal bytes and a valid record, which yields `unchanged` | `publish-release` scoped to this destination |
| Activation reference | The activation generation naming one published Release and its record at this destination | The caller's expected activation reference equals the current one | `activate-release`; publication never advances this pointer |

A Store transaction stages every added artifact and evidence payload, verifies each against its 017 integrity digest, and switches the pointer to a generation that names the resulting snapshot. Readers pin one generation and its snapshot exactly as the registry read path does, and a pending marker blocks a current-state read until it is resolved. Candidate publication, review, selection, and revocation stay separate transaction classes, and an empty transition reuses the current generation without a new publication.

A Release publication stages the member files at their descriptor addresses under an immutable namespace keyed by the Release identity, verifies every file digest and byte length after writing, flushes them, and only then installs the pending marker and switches the pointer to a generation that lists the new Release and its record. The manifest bytes are the canonical JSON text of the `release-snapshot`, and the manifest and record become visible in that one switch. Distinct Releases occupy distinct namespaces, so publishing one never touches another. The serving adapter reads only from this namespace under the host root, which is how it satisfies 018's origin admission profile.

Activation is a separate generation kind for the deployment host. It verifies the record for this destination as 025 requires, then switches the activation pointer under the expected-current condition. This host never removes a published Release from the destination, and the previously active generation remains readable. Outcome resolution for all three destinations follows the table below without modification: a pending marker with the pointer at the expected generation is `not-published`, a pointer at the candidate generation is `published` once durability is proved, and anything else is indeterminate or blocked.

These adapters add no authority. Store, publication, and activation provenance are host-private records like registry provenance and retain only the safe identifiers that 017 records carry. Remote Stores, shared destinations, CDN upload, withdrawal, rollback, and garbage collection remain deferred with their owners.

## Interrupted Transactions and Outcomes

Outcome resolution is explicit host work under the existing binding and storage lock. Read-only compilation reports the need for it but never repairs control files. Resolving a transaction checks the stored marker, exact generation references, required payloads, and trusted provenance; it never imports a backup, regenerates IDs, or replays a permit as a new publication.

| Observed state | Result and permitted action |
| --- | --- |
| Failure before pointer replacement is attempted, with old state verified | `not-published`; retain the old current state and report any exact pending cleanup needed |
| Pending marker, current pointer equals its exact expected generation | The attempt did not become current; discard only its known pending control marker/owned temporary after validation. Do not publish its candidate or adopt its IDs |
| Valid first-enrollment marker with an explicit expected-absent condition, matching independently retained trusted new-owner association, and pointer still absent | The control generation was not published; retain the incomplete owner reservation. Any retry is an explicit resumption of that enrollment with fresh setup authorization, not registry initialization or discovery of a new owner |
| Pending marker, current pointer equals its exact candidate generation and all required content/provenance validates | The switch occurred; complete/check the required durability barrier and resolve to `published` only when it succeeds. Marker cleanup does not grant new publication authority |
| No pending marker, exact operation is present in the validated committed generation history | Report its recorded publication, distinguishing whether its result is still current or has been superseded. Do not overwrite the newer generation |
| Pointer unexpectedly absent, corrupt, foreign, or different from both marker references; required data/history unavailable; durability still unprovable | `indeterminate` or typed blocked/control failure with the observed cause. Preserve evidence and stop further writes; never choose a staged generation heuristically |

A lost acknowledgement may therefore resolve to committed or uncommitted state. A missing marker/receipt alone is not proof of non-publication when the necessary history cannot be established. Already durable publication is not undone by a later permission change; historical facts remain readable under fresh applicable read authority, while every new write needs new authorization.

Inspection may report what is proved without mutating state. Clearing a proved marker or completing its durability barrier is an explicit trusted host-maintenance action, serialized with publication; it cannot alter the selected generation. No still-valid old publication permit is needed merely to establish past facts, and none can authorize promoting an uncommitted candidate. All subsequent writers refuse unresolved pending state.

For a crash/restart, the adapter uses the same procedure under a newly established local session. If its admitted storage profile cannot guarantee a complete old-or-new pointer or the required installed objects, it cannot claim this durable profile. Fault injection tests establish protocol behavior; power-loss claims additionally depend on the declared platform/storage guarantees.

## Failures, Limits, and Disclosure

Preserve separate acquisition, 015 input, 016 semantic, 017 representation/integrity, 018 authorization, storage-capability, busy/stale-state, cancellation, and publication-outcome causes. A conflict is not a successful no-op. Missing credentials/authority are not invalid MF2, and a successful in-memory plan is not a completed durable write.

Every result states its declared source scope, exact base/result when available, publication state, and whether explicit maintenance is required. Interrupted or exceeded reporting never upgrades an unproved publication to success. The initial host has no general JSON reporter, diagnostic-code registry, or exit-code mapping; retain the owner results and project them only through an adopted 019/026 interface.

Finite limits cover source/configuration file counts and bytes, decoded control/authority records, payload occurrences and total retained/staged bytes, history traversal, allocation attempts, confirmations, generation depth, lock attempts, cleanup actions, and diagnostics. Check arithmetic and allocation before growth. Exhaustion never drops source units, tombstones, receipts, or history to make a smaller apparently valid state. If retained storage reaches its bound, reject new writes rather than adding implicit garbage collection.

Only known host-owned temporary/control paths may be cleaned up after exact identity checks. Never recursively remove a caller source root, another operation's files, a foreign directory, or committed payloads. Invalid pending state remains available for diagnosis; it is not repair authority.

Credentials and live handles stay in the trusted host. Source bytes, descriptions, confirmation reasons, paths, and actor metadata may be sensitive and remain in authorized retained inputs or owning safe projections, not default logs/profiler labels. Storage digests and opaque identifiers are not automatically safe public telemetry. No Provider, TMS, network, or application-code execution is available through this host subset.

## Performance and Measurement

Apply [026's performance architecture](./026-intlify-conformance-and-measurement-design.md#performance-implementation-architecture) and reuse 016/018's existing core intervals. Keep immutable acquisition/generation storage, per-invocation Scratch Workspace, prepared updates, returned results, and short-lived publication guards in distinct lifetimes. No result borrows resettable scratch or transfers a live write handle to a read-only consumer.

- Read/freeze each exact source once per invocation and reuse its validated parse/context facts under the existing owner rules. Do not reacquire the same file per declaration, reference, or requested locale.
- Use indexed exact payload/generation references and bounded history traversal. Store identical admitted payload bytes once where safe, while preserving occurrence accounting and source evidence; do not rewrite all historical source bytes in each new generation.
- Keep acquisition and semantic preparation outside storage locks where possible. The first implementation has one bounded publication at a time, no nested worker pools or parallel per-message writes. Measure lock hold time separately from acquisition/preparation.
- Cache immutable content checks only for matching bytes and owner/verifier inputs. Never cache away current-generation, authority, session, or pending-transaction checks.
- Keep optional expanded inspection/profiling out of the unrequested path while retaining mandatory provenance and diagnostics. Reset scratch after success, denial, conflict, failure, and cancellation; reuse must match fresh execution.

| Operation | Separate observations |
| --- | --- |
| Acquisition/read | File/input bytes and counts, retained snapshot construction, registry/control read work, core versus filesystem time |
| Update preparation | Existing 016 analysis/reconciliation and 018 admission intervals, fixed allocation work, immutable plan/result size |
| Publication | Lock attempts/wait/hold, newly staged versus reused bytes, file/directory sync and pointer work, exact resulting outcome |
| Outcome resolution | Retained generations/payloads examined, validation work, marker/durability operations, no hidden republishing |

The implementing slice pins finite workloads, method boundaries, environment/storage profile, fresh/reused state, logical checksums, and required 026 evidence before accepting measurements. Start with descriptive baselines, not invented numeric speed budgets. An unsupported observer is explicit; it is not zero I/O or allocation. SIMD, unsafe storage code, a database, or general incremental scheduling are not prerequisites. Optional profiling follows 026 isolation and cannot replace primary timing samples.

## Conformance and Completion

| Area | Required independent cases |
| --- | --- |
| Owner/authority setup | Explicit new/existing binding, interrupted enrollment retaining its owner reservation, wrong owner/destination, missing protected state, forged caller labels, changed grants, closed sessions, and no production promotion of test contexts |
| Acquisition | Exact bytes and stable supplied associations, duplicates/conflicts, denied paths/symlinks/special files, changing/unavailable inputs, complete/partial scopes, no source execution |
| Read-only path | No directory/control creation, ID allocation, registry publication, implicit outcome repair, or history reset; pinned historical read does not claim currentness |
| Initialization and updates | Exactly one empty genesis, later fixed allocation, verified continuation, exact explicit choice, eligible auto-updates, preserved unseen entries, rejected partial retirement, and unchanged plans |
| Concurrency | Two prepared updates from the same base yield at most one publication; competing initialization, authority-only advancement, session closure, changed lock/root identity, and bounded lock contention cannot bypass the final checks |
| Persistence | Independent decoding of complete generation/provenance/payload associations; same reference with changed bytes, missing required bodies, unsupported layout/capabilities, and incorrect private/shared identity reuse fail |
| Interrupted writes | Inject faults/cancellation before and after every creation, payload/generation sync, marker install/sync, pointer replacement/sync, and cleanup; verify old, new, or explicitly indeterminate state without a valid-looking partial result |
| Outcome resolution | Old/candidate pointer cases, enrollment-only expected absence versus lost current state, malformed marker, lost acknowledgement, later supersession, restart with fresh authority, and repeated resolution after another failure; no replayed publication or invented rollback |
| Store and Release destinations | Store genesis and exact-base transitions with pending-state reads blocked; Release staging with digest verification, unchanged republish, distinct namespaces, and manifest/record visibility in one switch; activation conflicts with the previous generation preserved; the same fault-injection and outcome cases for each destination |
| Limits and reuse | Exact/first-over capacities, retained-history exhaustion, repeated hostile inputs, workspace reuse after every outcome, and bounded safe diagnostics |

The minimum local host is complete only when actual 016/017 validators and 018 evaluators run through the real selected storage adapter, all required cases for the adopted subset pass, and scoped 026 records retain independently expected outcomes. An in-memory adapter proves only its own model. At least one admitted local disk profile needs reopen/restart and concurrent-process lock tests plus deterministic fault injection; tests must not infer durable success from a mock rename or a returned `true`.

Completion remains narrower than full 016 support: any unadopted semantic choices, unsupported policies, library features, and 028's supply/execution requirements retain their own gates. No commit/PR boundaries or public command names are prescribed by these design gates.

## Adoption with 016

| Adopting work | Minimum 029 use | Claim limit |
| --- | --- | --- |
| Phases 1–2 | Explicit finite acquisition and read-only test host | No checked production Profile, durable identity, or Web execution claim |
| Phase 3, pure core | Actual authority/confirmation orchestration and replayable allocation inputs against fixed test state | Requires 016/017 semantics and 018 evaluation; no disk durability claim from an in-memory store |
| Phase 3, local persistence | Owner binding, empty genesis, conditional updates, safe storage, provenance, and interrupted-outcome resolution defined here | Requires actual applicable 015 inputs/policies and the implemented/tested 017/018/029 subset; pending policy/bootstrap formats remain explicit prerequisites |
| Phase 4–5 / 028 | Reuse the registry host's separation of read, prepare, authorize, publish, and inspect, and the Store lineage and Release destination adapters above for 021 transitions and 025 publication/activation | Sync/governance decisions, build/Release assembly, and execution remain their owners' work; registry powers authorize none of them, and the adapters require 018's Web extension powers |

An implementation plan should deliver private codecs/fixtures, acquisition and read-only integration, authorization/confirmation wiring, conditional persistence, and fault/performance coverage in dependency order. It must name any required upstream input/representation additions before the operations that consume them, without requiring the whole later product design to be finished.

## Decision Log

| ID | Decision | Rationale |
| --- | --- | --- |
| 029-001 | Start with an internal local host for one application registry, not public workflow/packaging | Supplies 016 Phase 3 obligations while preserving 000's later productization scope |
| 029-002 | Require explicit domain/owner/input selection and trusted caller establishment | Avoids path-derived authority, inferred newness, and hidden repository discovery |
| 029-003 | Keep analysis, initialization, preparation, confirmation, and publication distinct | A valid source result or identity choice cannot silently acquire write powers |
| 029-004 | Store immutable inputs/generations and switch one protected current pointer | Keeps registry state and required provenance consistent while allowing payload reuse |
| 029-005 | Serialize current-base and authority checks with the final pointer switch | Prevents stale updates, competing initialization, and grant/session changes from invalidating an earlier-only check |
| 029-006 | Retain a pending transition and explicitly resolve uncertain outcomes | Lost acknowledgement is neither guaranteed rollback nor permission to publish again |
| 029-007 | Reject unsupported storage capabilities and keep read-only paths non-repairing | Avoids weaker cross-platform fallbacks and hidden writes during compilation |
| 029-008 | Keep control codecs private and adopt actual 017/018 values | Reuses shared identity rules without inventing portable authorization or promoting resource-output formats |
| 029-009 | Require real persistence, failure, and scoped 026 evidence | File output or mock success alone cannot establish safe persistent identity |
| 029-010 | Reuse the generation, pointer, and outcome protocol for the local Store lineage and Release destination instead of designing separate storage models | One tested persistence protocol serves registry, Store, and Release without new authority or semantics |

## Deferred Follow-Up Notes

- Public commands, authoring package names, configuration discovery, monorepo/worktree enrollment UX, installation, watch/dev scheduling, CI, packaging, and release sequencing.
- Complete 015 policy/bootstrap input adoption, shared portable provenance formats, remote/cross-process authorization, alternative storage backends, and wider platform durability profiles.
- Authenticated import of registry history, recovery/rollback selection, source-control merges, destination migration, history compaction, retention policy, and garbage collection. Transaction outcome resolution above does not implement these.
- 028's translation supply/governance decisions, build/export, Release assembly, browser execution, and later TMS integrations under their owning specifications; the local Store and Release destination adapters above cover only their persistence.
- General graph/query/inspect/audit services and stable public reporting; broader product workflow must preserve the established owner inputs, failure states, and least-authority separation.

## Relationship to Other Documents and Existing Foundations

- [000](./000-intlify-overview-design.md#detailed-design-traceability) assigns complete productization; this document first supplies the smaller host obligations already required by 016/018.
- [015](./015-intlify-project-profile-and-locale-policy-design.md#consumer-input-boundaries) supplies checked context/bindings, while [016](./016-intlify-source-authoring-and-intent-identity-design.md#identity-registry-and-reconciliation) owns source and identity validity.
- [017](./017-intlify-shared-artifact-and-version-admission-design.md#non-circular-history-and-publication) supplies exact non-circular registry history; [018](./018-intlify-security-trust-and-provenance-design.md#publication-handoff-to-029) supplies final authorization and provenance obligations.
- [026](./026-intlify-conformance-and-measurement-design.md#performance-implementation-architecture) supplies reusable performance/verification requirements; [028](./028-intlify-javascript-web-vertical-slice-design.md#adoption-with-016) later consumes this host without implying that all public workflows are finished.
- [021 — Exact-base atomic publication](./021-intlify-translation-store-and-governance-design.md#exact-base-atomic-publication) and [025 — Release publication](./025-intlify-release-assembly-and-deployment-design.md#release-publication) supply the transition and publication semantics that the Store and Release destination adapters persist; [018 — Minimum Web Localization Extension](./018-intlify-security-trust-and-provenance-design.md#minimum-web-localization-extension) supplies their powers.
- Existing [CLI registration storage](../crates/intlify_cli/src/messages/registration/transaction.rs) provides reusable no-follow directory access, stable locking, flush/capability checks, and fault-injection foundations. Its resource-output tree replacement, journal, manifest, and rollback semantics are not Intent-registry formats or authorization. Reuse helpers only after the generation/currentness protocol above is independently tested; no automatic migration is defined.
