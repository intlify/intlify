# Intlify Release Assembly and Deployment Design

## Purpose

This design defines the smallest Release Assembly, publication, activation, and execution-admission specification needed to turn [024](./024-intlify-target-profile-and-export-design.md)'s complete target output sets into the deployment-selected Release that [027](./027-intlify-reference-runtime-design.md) and [028](./028-intlify-javascript-web-vertical-slice-design.md) execute.

[020](./020-intlify-requirement-planning-and-linking-design.md) chooses definitions, and 024 generates one complete output set per target. 025 defines when those output sets form one compatible Release, what an authorized publication must check and record before the Release becomes visible at a destination, how a deployment host selects one published Release, and what execution must verify before generated code or data is used. It does not select messages, generate outputs, own a repository or CDN, or run application code.

For example, 028's build produces one Runtime-backed and one ahead-of-time output set from the same six selected definitions. Release Assembly binds both to one immutable `ReleaseSnapshot`. A separately authorized publication rechecks every included artifact against the exact current revocation view, stages the bytes, and exposes the manifest together with a `ReleasePublicationRecord`. The test deployment host then activates that Release, and each execution path admits only outputs belonging to it. A translation revoked after that check is an impact on the existing publication, not a reason to rewrite it.

| Owner | Question answered |
| --- | --- |
| 020 | Which exact definition satisfies each requirement, and where is it logically placed? |
| 024 | Which generated code, bindings, data, and mappings implement those selections for one target? |
| 025 | Do all target outputs of the selected group belong to one compatible Release, may that Release be published and activated, and what must execution verify first? |
| 027/028 | Can the reference Runtime and generated Web execution pair load, run, and demonstrate those guarantees? |

The initial scope is one local application, one selected Deployment Compatibility Group of hydration-free Web targets, one local publication destination, and one local deployment host. It supplies the fixture `ReleasePublicationRecord` that [000](./000-intlify-overview-design.md#i1-javascriptweb-vertical-slice) requires for I1 without conflating publication and activation. It is not the production publication, rollout, withdrawal, rollback, or retention design assigned to 025 by [000](./000-intlify-overview-design.md#release-assembly-and-deployment).

## Goals

- Assemble exactly one immutable `ReleaseSnapshot` per selected group from complete, independently validated 024 output sets and the exact pinned build basis.
- Keep Release-assembled, publication-admitted, deployment-activated, and execution-admitted as distinct states with distinct evidence.
- Bind every publication to one destination and one exact authorized revocation view while keeping historical assembly reproducible.
- Expose a Release manifest and its `ReleasePublicationRecord` atomically after all bytes are staged and verified; never expose a valid-looking partial Release.
- Make activation an explicit host operation that verifies publication evidence and never removes the previously active Release.
- Verify generated code and data bytes, their Release association, and their trusted origin before executable modules are imported, and reject mixed-Release combinations.
- Preserve non-circular identity from target descriptors through the snapshot, publication record, activation reference, and execution-admission evidence.
- Apply 026's footprint grouping, initialization separation, evidence rules, and storage lifetimes from the first implementation.

## Non-Goals

- Message selection, target capability admission, code generation, source lowering, or changing any 020/024 output.
- Production repositories, CDNs, application stores, deployment orchestrators, remote publication integrations, signing, or optional publication fencing.
- Hydration-coupled groups, SSR/Browser consistency evidence, multiple compatibility groups in one transaction, or cross-group Release authority.
- Localization-only Releases, digest-identical output reuse across Releases, rollout strategies, withdrawal, rollback, garbage collection, and retention policy.
- Revocation-impact queries across existing publications, audit services, notification, or online revocation checks during activation or execution.
- Release wire schemas, digest domains, kind/schema tuples, or publication-record encodings owned by 017; actor authentication and publication powers owned by 018.
- Public commands, configuration members, package names, or destination discovery owned by 029.
- Numeric performance budgets, mandatory Budget Evaluations, or profiling as Release prerequisites.

## Ownership and Dependencies

| Owner | Responsibility in this subset |
| --- | --- |
| 000 | Release lifecycle states, atomic publication semantics, publication-time revocation view, the group compatibility boundary, and host-owned activation |
| 015 | Checked project identity, Selection Scope, the selected Deployment Compatibility Group, member Target IDs, Target Profile references, supported requested locales, and hydration relations |
| 017 | Existing digest framing and record identity rules; the Release snapshot, publication record, and admission-evidence representations to be adopted |
| 018 | Publisher and activator authority, publication permits and provenance, trusted code-origin admission, and disclosure rules |
| 020/021 | Exact selections, placements, source-admission and selection evidence, and the snapshot-bound revocation facts consumed at publication |
| 023 | Execution profile pins that every member must share; no execution behavior is redefined here |
| 024 | Complete validated output sets, target descriptors, binding tables, execution dependency relations, and per-target physical mode |
| 025 | Group compatibility admission, snapshot contents and identity, publication admission and record contents, activation obligations, and execution-admission coordination |
| 026 | Footprint and initialization measurement categories, Release evidence admission, and conformance requirements |
| 027 | Runtime-backed execution admission consuming the deployment-selected Release and admission evidence as inputs |
| 028/029 | Trusted local destination, deployment host, serving adapter, browser execution, storage protocol, and later product workflow |

The tables below fix logical contents and owner obligations. Shared wire schemas, digest domains, and exact version tuples are adopted in 017's [Minimum Web Localization Representation](./017-intlify-shared-artifact-and-version-admission-design.md#minimum-web-localization-representation) as the `release-snapshot`, `release-publication-record`, and `execution-admission-evidence` kinds. 018's [Minimum Web Localization Extension](./018-intlify-security-trust-and-provenance-design.md#minimum-web-localization-extension) defines the `publish-release` and `activate-release` powers; 025 names the powers, not their authentication.

## Terminology

| Term | Meaning here |
| --- | --- |
| Release basis | Exact project/profile, Selection Scope, selected group, Requirement Plan, Bundle Plan, pinned Store snapshot, source artifacts, specification pins, and assembler revision consumed by one assembly |
| Member output set | One Target ID's complete 024 output set: target descriptor, execution-required files, verification-only evidence, binding table, and locale payloads |
| Release identity | The deterministic integrity identity of one complete canonical `ReleaseSnapshot`; equal inputs and assembler yield equal identities |
| Destination | One host-established publication namespace with an opaque binding, a manifest role, and an immutable Release-scoped artifact namespace; not a path string supplied by an artifact |
| Revocation view | One exact authorized Store snapshot and its applicable revocation and rejection facts as evaluated by 021 for the included artifacts and evidence |
| Publication transaction | One authorized attempt to make one Release visible at one destination; it commits completely, provably does not commit, or is indeterminate |
| Activation reference | Host-owned mutable selection of one published Release at one destination; changing it is deployment activation |
| Deployment-selected Release | The Release named by the current activation reference, resolved through its publication record and snapshot, never through a `latest` path |
| Execution-admission evidence | Immutable host-established value recording that one member output set was verified against the deployment-selected Release before use |

These are logical values and operations. They do not reserve type names, JSON members, filenames, URL layouts, or public API spellings.

## Minimum Inputs and Results

Each operation consumes actual immutable checked inputs, not labels claiming that an earlier stage succeeded.

| Input | Required content |
| --- | --- |
| Group | The checked 015 group selected by the build transaction, its complete Target ID member set, each member's Target Profile reference and supported requested locales, and its hydration relation set |
| Build basis | Exact project/profile identity and semantic digest, Selection Scope, Requirement Plan and Bundle Plan identities, pinned Store snapshot identity and lineage, and the source-artifact set consumed by linking |
| Member output sets | For every member, the complete 024 output set with its frozen target descriptor, all inventoried bytes, binding table, locale payloads, execution dependency relation, and the 024 set validator's actual result |
| Selection evidence | For every requirement, the exact selected ArtifactDigest, definition locale, and the source-admission or selection/approval evidence references verified by 020 |
| Specifications | Exact MF2/parser-semantic, 023 execution profile, 024 target/binding/payload codec, and 025 assembly profile revisions |
| Publication inputs | Destination binding, publisher principal and applicable 018 powers, publication policy revision and required evidence set, and authorized read access to obtain the revocation view |
| Activation inputs | Deployment host binding, the record proposed for activation, and the expected current activation reference |
| Limits | Finite member, file, byte, dependency-edge, selection, evidence, staged-byte, record, lock-attempt, and diagnostic capacities |

Assembly returns one complete `ReleaseSnapshot` or owner-preserving diagnostics and no snapshot. Publication returns `published`, `unchanged`, `denied`, `blocked`, `conflict`, `not-published`, or `indeterminate`. Activation returns the activated Release association or a typed failure. Execution admission returns immutable evidence for one member or a typed failure with no partially admitted output.

The first core tests may construct closed test-owned output sets and run the actual validators. They cannot replace a 024 target descriptor with a file listing, use a fixture boolean as a publication permit or revocation view, or promote PR #205's partial configuration core to a production Profile.

## Design Overview

| Step | Operation and checked result | Required separation |
| --- | --- | --- |
| 1. Admit members | Verify that every member Target ID has one complete validated output set from the same build basis | No regeneration, reselection, or assembly of a partial group |
| 2. Check compatibility | Compare selection basis, specification pins, locale coverage, physical modes, and hydration requirements across members | Equal bytes or filenames are not compatibility evidence |
| 3. Freeze the snapshot | Produce the canonical `ReleaseSnapshot` and its deterministic Release identity | No destination, view, publisher, timestamp, or record identity inside the snapshot |
| 4. Publish | Under authority, obtain the exact revocation view, recheck included artifacts and evidence, stage and verify bytes, then expose manifest and record atomically | Assembly reproducibility is not publication admissibility |
| 5. Activate | The deployment host verifies the record for its destination and switches the activation reference under an expected-current condition | Publication never activates; activation never republishes |
| 6. Admit execution | Verify the deployment-selected Release, member bytes, and trusted origin before importing generated modules; hand admission evidence to 027 or the AOT constructor | Runtime never chooses the Release or queries publication authorities |

Steps 1–3 are one deterministic assembly operation invoked by the build after the complete group is available. Steps 4–6 are separately authorized host operations. One process may run all of them in the 028 harness; their inputs, powers, and results remain separate.

## Release Assembly and Group Compatibility

### Member admission

Assembly consumes the complete member set of the selected group. A member whose export failed, whose descriptor is missing, or whose 024 set validation did not succeed prevents assembly of that group. The successful members may be retained for inspection but cannot be assembled or presented as the group's Release. Assembly does not invoke an exporter, wait for a member, or substitute a previous Release's output for a missing member.

Each member is verified against its own descriptor: every inventoried file's bytes match the descriptor's integrity reference and byte length, every dependency endpoint is present, every required role is filled, and the descriptor's target, profile, mode, and basis references match the group and build basis. Verification-only maps and evidence must be present for assembly even though eager execution readiness follows only the execution dependency relation.

### Compatibility checks

All members must share one exact selection basis:

| Dimension | Required agreement across members |
| --- | --- |
| Project and scope | Same checked project identity, Profile semantic digest, and Selection Scope |
| Transaction | Same selected Group ID, Requirement Plan identity, and Bundle Plan identity |
| Store | Same pinned Store snapshot identity and lineage |
| Selections | Identical selected ArtifactDigest, definition locale, and evidence references for every shared requirement; no member may carry a selection absent from the Bundle Plan |
| Specifications | Same MF2/parser-semantic, 023 execution profile, and 024 codec revisions; each member additionally pins its own physical mode and adapter or helper profile |
| Locales | Each member's locale payload set equals its checked supported requested-locale subset; no extra or missing locale |
| Placements | Each member's placements equal the Bundle Plan's placements for its Target ID and sole `["main"]` unit |

Different physical modes are compatible by design: a Runtime-backed member and an AOT member of the same group encode the same selections differently. A member that selects different wording, omits a required locale, or pins a different execution profile is incompatible even if it validates on its own.

Locale-service absence, explicit message direction, and the sole logical Delivery Unit per member are recorded as member facts. The minimum profile requires the group's hydration relation set to be empty. A group with any hydration relation is unsupported and fails assembly explicitly rather than being assembled without the consistency evidence that 015 and 030 require.

### Snapshot contents

| Part | Required contents |
| --- | --- |
| Basis | The complete Release basis above and the exact assembler implementation and assembly profile revision |
| Members | Canonical Target ID order; for each: Target Profile reference, physical mode, adapter or helper profile, descriptor integrity reference, binding-table reference, locale payload references per requested locale, execution dependency relation, and verification-only evidence references |
| Selections | Canonically ordered requirement keys with selected ArtifactDigest, definition locale, and evidence references, retained so publication can recheck them without reading the Bundle Plan body |
| Relations | The empty hydration relation set in this profile, recorded explicitly |
| Identity | The Release identity computed over the complete canonical snapshot with its own identity member excluded, under 017's self-exclusion and framing rules |

The snapshot references member descriptors by integrity reference. It contains no destination, revocation view, publisher, publication record, activation state, absolute path, or timestamp. Assembling the same complete inputs with the same assembler yields the same identity; reordering member submission, worker scheduling, or map iteration cannot change it. A snapshot assembled from a historical Store snapshot reproduces its earlier identity, which proves reproducibility and nothing about current publishability.

The snapshot is a localization release manifest. It is not the application's deployment manifest, a package lockfile, or a Runtime Manifest; those remain 024 and host concerns.

## Release Publication

### Authority and policy

Publication requires a host-established publisher principal holding a publication power scoped to the exact destination, plus authorized read access to obtain the revocation view. This power is 018's `publish-release` grant; the existing source/registry grants, Store publication authority, and the build actor's role do not imply it. Publication holds repository authority only and receives no Provider, TMS, governance, or deployment credential.

The publication policy is an explicit input naming the destination, the required evidence set, and the view requirement. The minimum local policy requires the 024 set validation results for every member and the 021 recheck below; it requires no 026 Budget Evaluation or equivalence record. A production policy may require the Release evidence described by [026](./026-intlify-conformance-and-measurement-design.md#release-evidence). An absent or unsupported policy blocks publication; there is no permissive default.

### Publication admission

1. Admit the complete snapshot, all member bytes, and the publication inputs under finite limits. Recompute the Release identity from the supplied snapshot and compare; a caller-supplied identity is not trusted.
2. Obtain the latest obtainable authorized revocation view. In the local profile this is the current Store snapshot of the build's lineage, read under 021's rules and 018's `read-store` power; it must be the pinned snapshot or a descendant of it. An unavailable, unreadable, or foreign-lineage view blocks publication, and the pinned build snapshot is not a substitute view.
3. Re-evaluate every retained selection against that exact view. A localized selection blocks publication when its artifact is revoked or rejected, or when its supporting validation, approval, or selection evidence is revoked. A source selection blocks publication when its source-admission evidence is revoked. A failure identifies the exact requirement, artifact, revocation or rejection record, and view identity. Supersession by a newer positive review or selection is recorded as an observation and does not block this minimum.
4. Verify the policy's required evidence set against the actual retained results; a missing or stale item blocks publication.
5. Stage every member's inventoried bytes into the destination's immutable namespace for this Release identity, verify the written bytes against their integrity references, and flush them. Existing unequal content under the same address is a conflict, not an overwrite.
6. Atomically expose the manifest and the `ReleasePublicationRecord` together as the final step. A consumer of the destination observes either the previous complete state or the complete new Release with its record.
7. Return the actual outcome with the record reference. An error after proven visibility is `published` with maintenance required, not a rollback.

Steps 1–4 are the publication-admission core and perform no destination writes. Steps 5–7 are the destination adapter's protected commit. The core is deterministic for its inputs; the record it produces is an event with a fresh instance identity.

Republishing the same Release identity to the same destination when a valid record and matching bytes already exist reports `unchanged` and references the existing record; it creates no second record and reruns no destination writes. Different bytes under the same Release identity, or two concurrent transactions for the same identity and destination, are conflicts. Distinct Releases may be published to the same destination independently because each occupies its own immutable namespace.

### Record contents

| Part | Required contents |
| --- | --- |
| Subject | Release identity and the destination binding's identity |
| View | Exact revocation view identity: Store snapshot identity and lineage plus the 021 evaluation basis |
| Policy and actor | Publication policy identity/revision, safe publisher identifier, and the authority state reference established by 018 |
| Evidence | References to the required evidence set actually verified, including the 024 set validation results |
| Repository identity | The destination's publication identity for this transaction, such as the exposed manifest address and its byte integrity |
| Identity | A fresh instance identity under 017's record identity rule and an integrity digest over the complete record with that member excluded |

The record contains no activation state, credentials, source text, or absolute workspace paths. One snapshot may have records at several destinations; each is a separate transaction. A record proves a past publication fact relative to its recorded view; it does not prove admissibility against later views or that the Release is active anywhere.

### Time semantics and atomicity

Assembly and publication have different time semantics. A revocation committed after the recorded view, including during the check-to-visibility interval, is an impact on this publication and requires a replacement Release or a host-owned withdrawal; it does not mutate the record, the snapshot, or the destination bytes. Optional fencing against a revocation head is a deployment capability, not a minimum requirement. Publication requires no distributed transaction with the Store, the revocation authority, or the deployment host, and a failed publication never rolls back the pinned Store snapshot.

The local destination adapter is 029's [Release destination adapter](./029-intlify-product-workflow-and-packaging-design.md#store-lineage-and-release-destination-adapters) and provides the guarantees that its protocol requires of protected publication: no-follow directory access, cooperative locking, exclusive temporaries, staged and flushed objects, an atomic final switch, and explicit outcome resolution after interruption. A missing acknowledgement resolves to `published` or `not-published` only through that adapter's retained transaction evidence; otherwise it remains `indeterminate` and blocks further writes to that destination until resolved. 025 does not prescribe filenames, lock primitives, or the pending-record layout. The destination is a host-managed location outside analyzed source; Intlify does not own the project's repository, CDN, or deployment system.

## Deployment Activation

Activation is an explicit host deployment operation, not a consequence of publication. The 028 test deployment host performs it; production deployment systems remain outside Intlify.

Before activating, the host verifies the proposed `ReleasePublicationRecord` for its own destination: the record's integrity, that its destination identity equals the host's destination, that the exposed manifest bytes match the record's repository identity, that the manifest decodes to the snapshot whose identity the record names, and that the record's policy and actor provenance satisfy the applicable 018 rules. It then switches the activation reference under an expected-current condition: the caller names the activation reference it observed, and the switch fails as a conflict if another activation intervened. Activation requires 018's `activate-release` power, distinct from the publication power.

A missing, malformed, foreign-destination, or unverifiable record blocks activation. The host does not activate from a manifest without its record, from a staged namespace without visibility, or from a mutable `latest` pointer. Activation neither rechecks the revocation view nor requires a fresh publication. A revocation after publication is handled by a replacement Release, which is a new assembly and publication, not by editing the active one.

Activating Release B does not remove, overwrite, or invalidate Release A's bytes, manifest, or record. A's retention, withdrawal, and eventual garbage collection are host policy outside this minimum. Already running applications bound to A continue with A; nothing in this design moves a live application to B.

## Execution Admission

### Establishing the deployment-selected Release

Before any generated module is imported or any locale payload is decoded, the execution host resolves the current activation reference to its record and snapshot and verifies that chain again. The result is the deployment-selected Release for this application instance; it is bound once and does not follow later activation changes.

For the member being executed, the host verifies the output set bytes it will use against the member descriptor's integrity references and the descriptor against the snapshot, reads those bytes from the trusted immutable namespace, and establishes that the bytes imported or served are the bytes verified. The minimum trusted local profile satisfies this by importing from the host-owned staged objects or by serving content whose integrity is rechecked at the serving boundary; a mutable URL, an equal filename, or a successful import is not that guarantee. 018 and 029 adopt this adapter's code-origin rules before a real Web claim.

The host then produces execution-admission evidence recording the destination, record identity, Release identity, member Target ID and physical mode, verified descriptor identity and byte inventory, the verification adapter and method revision, and the outcome. This evidence is an in-process checked value in the minimum, not a serialized credential; it confers no publication or activation power.

### Handoff to 027 and the AOT path

027's runtime-backed admission and the 024 AOT constructor receive the deployment-selected Release, the member descriptor, the verified bytes, and the admission evidence as inputs. They verify the Release identity, Target ID, Target Profile, physical mode, specification pins, binding table, and locale payload associations before eager construction, and they create bound handles only for that Release, target, requested locale, unit, and binding table as 024 requires. Neither queries the destination, the record, the Store, or a revocation authority, and neither may proceed from a missing or failed admission.

Mixed-Release combinations fail before use: a handle, binding table, locale payload, or generated module whose Release identity or descriptor differs from the deployment-selected Release is rejected, including when its bytes equal those of an admitted object. Interleaving instances bound to different Releases in one process is permitted only because each instance retains its own bound Release; sharing prepared state across them follows 027's identity rules.

No admission failure renders source text, searches another locale, loads another Release, or retries publication. Failures identify the exact stage and subject through 019's Diagnostic projection.

## Minimum Integration Example

For the exact 028 fixture, with both Web targets in one group and the Store history S0–S4 defined by 021:

| Scenario | Required observation |
| --- | --- |
| Both member output sets complete at S3 | One snapshot with two members, four locale payloads, six retained selections, and one deterministic Release identity; repeated assembly yields the same identity |
| One member export fails | No Release; the successful member is retained for inspection only |
| Members built from different Bundle Plans or Store snapshots | Compatibility failure naming the differing basis dimension |
| Publish to the local destination with current view S3 | `published` with one record naming S3; bytes staged and verified before visibility |
| Republish the same Release to the same destination | `unchanged` referencing the existing record; no destination writes |
| Revocation publishes S4, then publish a Release assembled from S3 | Assembly reproduces the S3 identity; publication is `blocked` identifying the RevocationRecord and view S4 |
| Publish with the Store unavailable | `blocked` as view unavailable; the pinned S3 is not used as the view |
| Activate the published record | Deployment-selected Release established; a record for another destination or a tampered manifest fails activation |
| Concurrent activations with the same expected reference | One succeeds and the other is a conflict |
| Execute `en` and `ja` on both members | Admission succeeds from verified bytes; both physical paths render the six selected definitions |
| Handle or payload from a previous Release | Rejected before formatting; no partial readiness |
| One staged locale payload byte altered | Execution admission failure for that member; the record and activation are unchanged |
| Build and execution | Zero Provider, governance, publication, or activation calls during build, and zero publication or Store calls during execution |

These are actual operation expectations. Fixture destinations and deployment hosts may be temporary directories and in-process adapters, but assembly, admission, staging, atomic exposure, activation checks, and execution verification execute their real implementations.

## Dependencies, Invalidation, and Reuse

| Product | Dependencies that must be retained |
| --- | --- |
| Release snapshot | All member descriptors and bytes, the complete build basis, specification pins, group facts, and the assembler revision |
| Publication record | Release identity, destination binding, exact revocation view and 021 evaluation basis, policy revision, publisher authority state, and required evidence results |
| Activation | The verified record, destination, expected-current reference, and activator authority |
| Execution admission | Activation observation, verified descriptor and bytes, verification adapter revision, and the 027/AOT profile pins |

A changed member output, selection, Store snapshot, or specification pin produces a different Release identity; nothing patches an existing snapshot. A new revocation view does not change a snapshot or record; it changes whether a new publication is admissible. A changed activation changes only which Release new application instances select. Retained records and snapshots are immutable history and are never rewritten by later failures.

The minimum requires no cache. Execution admission may reuse verified bytes only under content integrity plus Release identity and descriptor association; cache loss changes cost, not admission outcomes. A publication permit, revocation view, or activation observation is never reused across transactions.

## Diagnostics, Limits, and Disclosure

Use [019's minimum Diagnostic projection](./019-intlify-project-graph-query-and-incremental-design.md#minimum-common-diagnostic-projection) while preserving 020, 021, and 024 ownership of their failures. The 025 reason families cover invalid or unsupported assembly input, missing member, incompatible member basis, unsupported hydration relation, publication denied, view unavailable or foreign, artifact or evidence ineligible in the checked view, required evidence missing, staging conflict, publication conflict, indeterminate outcome, invalid or mismatched publication record, activation conflict, mixed-Release rejection, integrity or origin failure at admission, resource exhaustion, cancellation, and invariant failure.

A diagnostic identifies the exact Release identity, destination, member Target ID, requirement key, artifact or evidence reference, view identity, record identity, and stage where safely established. Records and diagnostics carry safe identifiers only: no source text, parameter values, absolute paths, credentials, session handles, or unbounded adapter output. An input rejected before safe admission uses its invocation-local slot.

Bound member count, files and bytes per member and per Release, dependency edges, selections and evidence references, staged bytes, record bytes, view evaluation work, lock attempts, and diagnostics. Count submissions before deduplication and check arithmetic before allocation or staging. Exceeding a limit or lacking a required body cannot become a smaller apparently valid Release, a truncated record, or a successful activation.

## Performance and Measurement

Apply [026's performance implementation architecture](./026-intlify-conformance-and-measurement-design.md#performance-implementation-architecture) and its footprint and initialization categories:

- Retain member bytes and descriptors immutably and reference them from the snapshot; do not copy output bytes into the manifest or into every selection row.
- Build member, selection, and dependency indexes once per assembly; verify each inventoried file once against its descriptor rather than per selection or locale.
- Keep assembly, publication admission, destination I/O, activation, and execution verification as separate intervals; report staged bytes and flush work separately from the logical core.
- Use a per-invocation Scratch Workspace for ordering and verification state; returned snapshots, records, and evidence own their storage.
- Start sequential and synchronous; parallel staging, custom arenas, SIMD, unsafe code, and persistent caches are not requirements.

| Operation/surface | Minimum observations |
| --- | --- |
| Release assembly, core | Members, files, bytes, and selections admitted; verification work; snapshot bytes; Release identity; outcome |
| Publication admission, core | Selections rechecked, view identity, evidence items verified, and outcome; no destination I/O |
| Destination commit, product workflow | Staged bytes and files, verification rereads, flush and switch operations, lock attempts, and the actual outcome |
| Activation and execution admission | Record verification work, verified bytes, and `release_admission` initialization separated from engine, Localizer, and binding construction |
| Complete generated output, generated output | Complete Release output set and initial eager closure per member under the checked Release/Target relation: payload, packaged, and transfer bytes, generated file and Delivery Unit counts, and load requests |

Footprint grouping derives from the snapshot's checked member and dependency relations, not filename parsing. The adopting plan pins Method Descriptors, cases, environments, compression profiles, and independent expectations before accepting observations. Start with descriptive baselines; no numeric budget is introduced. Production Release artifacts contain no profiling instrumentation.

## Conformance and Minimum Completion

| Case family | Required independent observations |
| --- | --- |
| Members and basis | Complete group; missing or failed member; descriptor and byte mismatch; missing verification-only evidence; members from different plans, Store snapshots, or profiles |
| Compatibility | Equal selections under different physical modes admitted; differing wording, locale set, placements, or execution pins rejected; nonempty hydration relation unsupported |
| Snapshot identity | Determinism under member reordering and repeated assembly; changed member or basis changes identity; no destination, view, timestamp, or record fields; reproduction from a historical Store snapshot |
| Authority | Publication and activation each denied without their own power; build actor and Store publisher lack publication power; changed authority before commit is refused |
| Revocation view | Current view admits; revoked artifact, selection, approval, or source admission blocks with the exact record and view; unavailable or foreign-lineage view blocks; the pinned snapshot never substitutes |
| Atomic visibility | Fault injection before, during, and after staging, verification, and the final switch; previous complete state or complete new state only; indeterminate reported honestly; unchanged republish |
| Activation | Valid record activates; foreign destination, tampered manifest, missing record, and stale expected reference fail; previous Release bytes and record untouched |
| Execution admission | Verified bytes admitted; altered bytes, wrong descriptor, mixed-Release handle or payload, unsupported mode or pins, and untrusted origin rejected before import or formatting |
| Limits and reuse | Exact and first-over capacities, cancellation followed by workspace reuse, immutable retained history, and no cached permits or views |
| Integration | Actual 020/021/024 results through the 028 harness: two members, four payloads, six selections, S3 publication, S4 blocking, activation, and both execution paths with scoped 026 records |

Completion of this minimum requires actual 024 output sets, an actual 021 view, real destination and deployment adapters with reopen and fault-injection tests for the adopted local disk profile, and execution through 027 and the AOT path under 028. A mock record, an in-memory destination alone, or a test that compares filenames cannot establish it. Pure assembly and admission tests may complete their own scope earlier without marking 016 Phase 5 complete.

## Adoption with 016 and 028

| Adopting work | Minimum prerequisite and completed scope |
| --- | --- |
| Assembly and compatibility core | Actual 024 descriptors and validators, 020 selections, and 015 group facts; independent snapshot fixtures |
| Publication admission core | 021 view evaluation over actual Store snapshots, the 018 publication power and permit, and policy inputs; no destination required |
| Local destination and deployment host | Adopt 029's storage capabilities for the destination adapter and implement activation with expected-current switching; fault and reopen tests |
| Execution integration | 027 runtime-backed admission and the 024 AOT constructor consuming the deployment-selected Release and admission evidence |
| 016 Phase 5 / 028 gate | Real upstream supply and selection, both complete outputs, a published and activated Release, both execution paths, and applicable 026 evidence |

These are adoption dependencies, not claims that their implementations are finished. 016 Phases 1–3 do not depend on this design.

## Decision Log

| ID | Decision | Rationale |
| --- | --- | --- |
| 025-001 | Start with one hydration-free group, one local destination, and one local deployment host | Completes the finite 028 Release surface without production repositories or rollout design |
| 025-002 | Assemble only from complete validated output sets for every member | One failed member cannot publish the remaining targets as the group's Release |
| 025-003 | Keep the snapshot, publication record, activation reference, and admission evidence as distinct objects | The four lifecycle states cannot be inferred from one another |
| 025-004 | Compute a deterministic Release identity over descriptors and the pinned basis with no backward links | Makes reproduction checkable and Release composition non-circular |
| 025-005 | Recheck every included artifact and evidence against one exact obtained view at publication and record it | Historical assembly stays reproducible while publication reflects current governance |
| 025-006 | Stage and verify all bytes, then expose manifest and record together as the final atomic step | Consumers never observe a valid-looking partial Release |
| 025-007 | Make activation an explicit expected-current switch that preserves the previous Release | Publication cannot activate, and a concurrent switch cannot be lost |
| 025-008 | Verify bytes, association, and origin before import and reject mixed Releases by identity | Successful import, equal filenames, or equal bytes are not admission |
| 025-009 | Fail explicitly on hydration relations, localization-only Releases, fencing, and rollback | Unsupported features are not silently degraded to pass the fixture |
| 025-010 | Apply 026 footprint, initialization, and evidence rules from the first slice without budgets | Whole-Release cost is observable before numeric thresholds exist |

## Deferred Follow-Up Notes

### Required follow-up for minimum Web execution

- 017: the Release snapshot, publication record, and execution-admission evidence are now encoded under its Minimum Web Localization Representation with exact kind/schema tuples, the `intlify-release-publication-v0` identity domain, and the Release identity as the snapshot's integrity digest; the adopting implementation registers the validators and fixtures.
- 018: the `publish-release` and `activate-release` powers, publisher and activator provenance, permits bound to the exact snapshot and destination, and the local code-origin admission profile are now defined under its Minimum Web Localization Extension; the adopting implementation must evaluate them against real host state.
- 021: the revocation-view read is supplied by its publication read handoff under 018's `read-store` power; the exact evaluation-basis identifier recorded here remains to be registered by the adopting implementation.
- 027: consuming the deployment-selected Release and admission evidence and rejecting mixed-Release inputs by identity are now defined by its Minimum Web Runtime Adapter; implementing them remains 027 work.
- 028/029: implement the local destination adapter, serving adapter, deployment host, and their fault, reopen, and concurrency tests; retain scoped 026 records.

### Broader extensions

- Hydration-coupled groups with 030's initial-render consistency evidence, multi-group orchestration, and localization-only Releases with digest-identical reuse.
- Remote repositories, CDNs, application packages, signing, publication fencing, multi-destination workflows, and deployment adapters.
- Withdrawal, rollback, retention, garbage collection, revocation-impact queries across publications, and audit services.
- Serialized or cross-process admission evidence, durable activation protocols, and Release-gating 026 budgets and equivalence evidence under production policy.

## Relationship to Other Documents and Existing Foundations

- [000](./000-intlify-overview-design.md#release-assembly-and-deployment) owns the five lifecycle operations, atomic publication, and the publication-time view; this design fixes their first local subset.
- [015](./015-intlify-project-profile-and-locale-policy-design.md#target-profiles-and-deployment-compatibility-groups) supplies the selected group, members, locales, and hydration relations; [020](./020-intlify-requirement-planning-and-linking-design.md#final-linking-placement-and-export-handoff) and [024](./024-intlify-target-profile-and-export-design.md#output-integrity-and-non-circular-release-handoff) supply the selections and output sets consumed here.
- [021](./021-intlify-translation-store-and-governance-design.md#read-handoffs-to-synchronization-linking-and-publication) supplies snapshot-bound revocation facts; [018](./018-intlify-security-trust-and-provenance-design.md#publication-handoff-to-029) and [029](./029-intlify-product-workflow-and-packaging-design.md#conditional-publication-protocol) supply authority and protected publication mechanics.
- [026](./026-intlify-conformance-and-measurement-design.md#release-evidence) owns evidence admission and footprint categories; [027](./027-intlify-reference-runtime-design.md#artifact-model) and [028](./028-intlify-javascript-web-vertical-slice-design.md#execution-pair-and-locale-lifecycle) consume the deployment-selected Release.
- Existing [output manifests](../crates/intlify_cli/src/messages/registration/manifest.rs), [journaled output transactions](../crates/intlify_cli/src/messages/registration/transaction.rs), and [prior-state inspection](../crates/intlify_cli/src/messages/registration/inspection.rs) are reusable staging and commit foundations. Their resource-output manifest, single-root replacement, and rollback semantics are not the Release snapshot, publication record, or activation protocol.
