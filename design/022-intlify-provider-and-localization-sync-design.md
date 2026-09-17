# Intlify Provider and Localization Sync Design

## Purpose

This design defines the smallest explicit localization-supply workflow needed by [028](./028-intlify-javascript-web-vertical-slice-design.md)'s generated Web application. It takes [020](./020-intlify-requirement-planning-and-linking-design.md)'s complete localization requirements, compares them with one pinned [021](./021-intlify-translation-store-and-governance-design.md) Store snapshot, and acquires only the required translation candidates.

A Localization Provider supplies candidate MF2. It may eventually be an AI agent, TMS, translation service, or human-input adapter; the first implementation uses a deterministic local fixture Provider. The synchronization host associates each response with its exact source request, runs real candidate validation, and publishes a complete candidate batch through 021. The Provider neither approves its output nor chooses what an application will display.

For the three English Intents in 028, the initial synchronization requests three Japanese candidates and no English candidates. Running it again while those candidates await review does not request the same translations again. Separate review and selection operations make them usable by the linker. An explicit refresh can acquire a replacement without changing the currently selected artifact.

| Operation | Responsibility | Does not do |
| --- | --- | --- |
| Derive work | Compare complete requirements with one Store view and explicit refresh inputs | Change the Requirement Plan, perform fallback, or choose a translation |
| Acquire candidates | Send finite requests to the configured Provider and retain their actual responses/provenance | Approve, select, or directly write Store artifacts |
| Validate and publish | Run 021's actual candidate checks and atomically publish the admitted batch | Treat successful supply as successful governance or build completion |

The initial scope is one local application, one Selection Scope, one group-scoped plan, and one fixture Provider. It completes the minimum supply-side specification, not external integrations or the full localized Web execution path. Ordinary analysis, tests, and builds do not acquire translations implicitly.

## Goals

- Derive a finite snapshot-bound work set without adding Store state to the Requirement Plan.
- Distinguish missing or semantically stale supply from approval, rejection, revocation, and selection issues.
- Make refresh explicit and keep source-equal requirements out of Provider work.
- Associate each candidate with actual source, request, adapter, and acquisition evidence.
- Keep Provider output inert and separate acquisition, validation, governance, and publication powers.
- Define repeatable local behavior, bounded failures, cancellation, and exact-base publication without hidden retries.
- Exercise real 021 validation and publication with 028's deterministic translations.
- Apply 026's bounded-work, lifetime, reuse, and measurement requirements from the first implementation.

## Non-Goals

- Requirement construction, source identity allocation, final locale fallback, target-dependent wording, or candidate selection.
- External AI/TMS services, network protocols, remote authentication, streaming, multiple Providers, or a general routing language.
- Glossary composition, rich translation constraints, full MF2 compatibility, linguistic-quality scoring, or automatic review.
- Multi-plan aggregation, cross-owner/library supply, partial Store materialization, distributed scheduling, watch mode, or persistent job queues.
- Automatic retry, resuming an interrupted job as a new authorized invocation, exactly-once external calls, or rollback of Provider-side effects.
- Public CLI spellings, package names, configuration APIs, review UI, or production packaging.
- Defining shared wire schemas/digest algorithms, extending registry permissions implicitly, or implementing downstream code generation and Runtime execution.

## Ownership and Dependencies

| Owner | Responsibility in this subset |
| --- | --- |
| 000 | Explicit synchronization, Provider neutrality, source-first lifecycle, and separate supply/governance/build powers |
| 015 | Checked application/locale/scope facts, Policy references, explicit Provider Routing and Glossary presence or absence, and applicable binding projections |
| 016 | Current Intent identity/revision, source meaning, parameter facts, and source-definition inputs |
| 017 | Shared requirement, source/localized artifact, request/provenance, Policy, and Store representations; canonical identities and version admission |
| 018 | Input/disclosure trust, actual principal and scoped acquisition/publication authority, provenance verification, and resource rules |
| 019 | Common Diagnostic and dependency projections; later scheduling rather than an additional source of translation demand |
| 020 | Complete Store-independent requirements, direct coverage, and later source/localized linking |
| 021 | Store admission/read results, candidate validation, governance, and exact-base atomic candidate publication |
| 022 | Work derivation, minimum routing, Provider request/response semantics, acquisition evidence, and synchronization outcomes |
| 023/024 | Message/value compatibility and authoritative target admission; acquisition does not establish target support |
| 026 | Conformance, performance architecture, owner measurements, and evidence admission |
| 028/029 | Finite Web scenario, trusted local invocation, fixture adapter, and concrete Store/publication integration |

022 defines logical operations and the minimum data they require. Missing shared encodings and security profiles are adopted in 017/018 before their dependent operations claim interoperable serialization or authorized publication. The existing registry host is not automatically a Provider or Store host.

## Terminology

| Term | Meaning here |
| --- | --- |
| Sync view | Read-only evaluation of one complete Requirement Plan against one exact Store base and explicit synchronization inputs |
| Work key | Complete owner-qualified Intent ID, Intent revision, and canonical requested locale within the invocation's exact Selection Scope |
| Localization basis | The checked source/Intent semantic inputs that a candidate must match; distinct from where the source was found |
| Work item | One admitted non-source-equal work key, its actual source/context, acquisition reason, and every originating applicability edge |
| Refresh set | An explicitly supplied finite set of current work keys for which new acquisition is requested even when matching supply exists |
| Provider binding | One host-established adapter identity/revision and its admitted non-secret behavior/configuration inputs |
| Acquisition attempt | One explicit invocation's finite dispatch/response record; not a Store transaction, review, or publication permit |

These terms do not reserve Rust types, public API names, JSON members, or a new digest family. Request correlation tokens are host operation identifiers, not persistent Intent identities or proof of authority.

## Minimum Inputs and Supported Profile

Every operation names its supported owner-specification and validation-profile revisions. Required values are actual checked immutable bodies, not labels, arbitrary digests, or callbacks that report success.

| Input | Required content |
| --- | --- |
| Application and scope | One checked application owner, exact versioned Selection Scope, selected Deployment Compatibility Group, and established Store lineage |
| Requirements | One complete current 020 Requirement Plan with all direct-required rows, source-equal paths, and originating target/delivery/reference applicability |
| Source basis | Actual source artifacts and checked Intent definitions, revisions, source locale, MF2, semantic context, and external parameter requirements |
| Store | One admitted immutable base snapshot, complete required candidate/governance support, and the host's accepted lineage/publication anchor |
| Policies | Actual applicable Trust, Source Admission, Approval, Selection, and Resource Limit bindings; Provider Routing binding or explicit absence; explicit supported absence of Glossary |
| Request options | An explicit finite refresh set, including an empty set for normal synchronization; no implicit refresh from build failure |
| Provider host | The trusted registered adapter and finite fixture data when acquisition is required; no adapter discovery through application source |
| Authority and bounds | Current scoped read, source-use/disclosure, acquisition, and candidate-publication capabilities where consumed; explicit finite input, work, response, retained-evidence, scratch, and I/O capacities |

The first profile adopts 028's source `en`, requested `en` and `ja`, one group, and literal/external-string MF2 compatibility. It has no fallback, Glossary, target-specific wording, remote service, or runtime parameter evaluation. The locale values and fixture map are scenario inputs, not configuration defaults.

Production adoption follows [015's consumer input boundaries](./015-intlify-project-profile-and-locale-policy-design.md#consumer-input-boundaries). An internal test host may provide an explicitly test-owned closed subset with real validators and authority checks. PR #205's partial configuration implementation is not by itself a complete production Profile.

## Design Overview

| Step | Operation and retained result |
| --- | --- |
| 1. Admit inputs | Check completeness/current source association, scope, exact Store base, supported policies, refresh keys, read authority, and finite limits |
| 2. Derive work | Produce per-requirement supply/governance observations and a canonical finite Provider work set without mutation |
| 3. Prepare acquisition | Resolve the admitted binding and confirm authority for the exact requests and candidate destination before the first call |
| 4. Acquire and validate | Dispatch each request once in canonical work-key order; retain its response association and run actual 021 validation |
| 5. Prepare publication | Build one complete 021 candidate batch only when every work item succeeded and its evidence is complete |
| 6. Publish or report | Use 021's separately authorized exact-base transaction; report published, unchanged/no-work, failed, conflict, or indeterminate outcome |
| 7. Govern separately | A caller may invoke 021 review/selection with independently established powers; this is not part of successful synchronization |

The work-derivation core is synchronous, read-only, and scheduler-neutral. The initial host executes acquisition sequentially, with one attempt and at most one candidate response per work item. This is a bounded local integration, not a job-service requirement.

## Work Derivation and Explicit Refresh

### Source and Store evaluation

Validate the complete plan against its supplied current source/profile basis before examining demand. 022 neither expands reachability nor drops source-equal, unselected, or difficult rows. It reads one pinned Store view throughout derivation; a malformed or unavailable required Store input is a failure, not an empty Store.

Matching supply means a stored candidate associated with the exact work key and the same checked 016/017 authoring semantic projection: source message meaning, canonical source locale, parameter requirements, and semantic usage/description. Verify the actual source/request association, not only equal labels. Its acquisition may refer to an older source artifact whose evidence changed without changing that semantic projection. Matching supply does not mean current technical evidence, approval, or selection is sufficient.

Each row retains separate observations for matching supply, technical/context applicability, source admission, review/revocation, and active selection. A usable active selection implies no missing supply, but the reverse is not true. The following rules apply in order after input admission:

| Requirement observation | Normal acquisition | Reported meaning |
| --- | --- | --- |
| Requested locale equals the Intent's source locale | Never | Use the compiler-derived source path; report any source-admission/review issue separately |
| Non-source-equal key is in the admitted refresh set | One work item | Explicit replacement supply, irrespective of existing candidate or selection state |
| Applicable active selection exists | None | Current localized selection is available under this exact view |
| Matching supply exists without an applicable active selection | None | Report required review, revalidation, selection, or rejection/revocation/provenance remediation; supply already exists |
| No candidate matches the current localization basis and requested direct locale | One work item | Missing supply, or semantically stale supply when prior related candidates explain the absence |
| Required input/check cannot be admitted or completed | No successful work set | Invalid/unsupported/unavailable/denied/over-limit operation, not missing translation |

Candidate observations aggregate the finite matching set. A historical rejected candidate does not suppress a separate current usable candidate; a selectable but unselected candidate does not become an implicit choice. If every matching candidate is rejected, revoked, or otherwise unusable, report explicit remediation or refresh need rather than silently generating more candidates. Governance issues are exposed to the caller without mutating them.

A different Intent revision or localization-relevant semantic context cannot be satisfied by old wording, a text-equal artifact, a DOM occurrence, or another owner's ID. Conversely, source locations and host-only expression edits do not independently create demand when the checked identity/revision and semantic basis are unchanged. Current source provenance still receives its owning checks.

Parser/validation-policy changes require the applicable real compatibility/evidence checks, not automatic translation merely because a version label changed. Matching content that needs new validation or review is reported as such. If a required body or validator is unavailable or unsupported, derivation fails explicitly instead of asking a Provider to repair an unverifiable input.

### Refresh and deduplication

The refresh set contains exact non-source-equal keys already present in the current plan. Reject duplicate, unknown, wrong-scope, obsolete-revision, and source-equal refresh keys before dispatch. Refresh is a trusted explicit invocation input and receives the same acquisition/disclosure checks as missing work; Provider output, a failed build, and a stale approval cannot create it.

Normal missing/stale demand and refresh for the same key yield one work item, with the explicit refresh reason retained. Multiple references, two Web targets, and repeated placements also yield one item. Preserve every originating group/target/delivery/reference association; do not treat target count as translation count.

Order the finite work set by the adopted 017 canonical ordering of complete Intent identity, revision, and requested locale, not source traversal, Store iteration, hash-map order, or translated text. Request order is deterministic even though a later external Provider need not be.

The initial profile does not aggregate multiple Requirement Plans. A later aggregator must retain each authoritative plan and prove the localization-input equivalence required by 000; sharing a work key alone is insufficient.

## Minimum Provider Routing

022 owns the logical meaning of the initial Provider Routing Policy subset; 017 owns its exact representation and digest projection. A present routing binding selects exactly one registered adapter identity/revision and its complete supported non-secret behavior/configuration basis for every work item. Selection is explicit, not an executable selector, source import, filesystem path, or downloaded plugin.

The host registers that exact adapter through trusted setup. Unsupported binding revisions, ambiguous registrations, or a present but incompatible policy fail admission. The fixture adapter receives an immutable finite response map; changing it changes the recorded adapter configuration basis.

Provider Routing may be explicitly absent when the derived work set is empty. If work exists, absence or an unavailable binding is a blocked acquisition before any calls; there is no default Provider or fallback to an external service. Glossary must be explicitly absent in this initial profile. A present Glossary or unsupported routing dimension fails rather than being silently ignored.

A routing/configuration change invalidates the synchronization/request view and affects future acquisition. It does not, by itself, make an existing semantically matching candidate require retranslation. Its historical acquisition basis remains recorded, and current Trust/provenance/governance rules may require remediation or an explicit refresh. No routing rule selects a stored candidate or creates target-specific wording.

## Requests, Responses, and Acquisition Provenance

### Host record and Provider-visible request

The host fixes the finite request list and correlation tokens before dispatch. Tokens are unique within the attempt and are checked together with its host-established attempt identity, so a result from another attempt cannot satisfy a current request. It retains the complete request basis separately from the allowlisted Provider payload:

| Location | Minimum retained or transmitted facts |
| --- | --- |
| Host request basis | Exact owner/scope, plan and applicability, Store base, source artifacts and Intent semantic basis, consumed Policy bodies/references and explicit absences, supported profiles/limits, Provider binding/configuration, refresh reason, and authorized operation association |
| Provider-visible request | Exact request correlation, owner-qualified Intent ID/revision, source and requested locale, source MF2, external parameter requirements, permitted meaning/description context, and adopted response/compatibility profile |
| Host acquisition record | Exact issued request, actual registered adapter/configuration, attempt/outcome, observed response association/body identity, and host-established provenance required by 018 |

The initial translation context contains only checked meaning/description inputs represented by the adopted source semantic basis. Occurrence locations, target placements, surface classification used only for coverage/routing, and the Store base remain host evidence, not additional wording dimensions. A later context/Glossary extension must define its candidate-freshness rules explicitly.

Do not disclose runtime parameter values, arbitrary application code, repository files, registry/Store history, unrelated messages, review credentials, or full Policy bodies merely because the host retains them. A Provider receives `Hello {$name}!` and the external string requirement, not the current user's name. The host applies actual 018 source-use/disclosure rules even when the first adapter is local.

### Response handling

One response contains one candidate MF2 value associated with exactly one issued correlation and its immutable request key. The fixture returns no executable program, alternative ranking, claimed approval, or Store artifact. Unsupported response members, wrong/missing/duplicate correlation, mismatched Intent/revision/locale, unsolicited results, and exceeded byte/count limits are explicit failures under the adopted closed adapter profile.

Strings in a candidate are inert message data. The host does not follow embedded instructions, load response-selected code, evaluate application expressions, or accept a Provider's `trusted` or `approved` assertion. A correlation match establishes association only; it does not establish trust or semantic validity.

The registered adapter may report an explicit acquisition failure instead of a candidate. Missing fixture entries, malformed responses, timeout, and cancellation do not become source-language fallback or successful empty candidates. Late results after a cancelled/failed attempt are discarded from staging and cannot publish that attempt.

### Evidence and identity

022 records what was actually requested and observed; 018 verifies the admitted provenance mode, and 021 checks the actual source/request association before constructing a localized artifact. Adapter identity, supplied actor labels, fixture expected text, and a response's own digest are not independent proof.

The non-circular dependency direction is source/request/acquisition evidence → localized artifact → technical validation evidence → Store snapshot. A request does not contain its future candidate ArtifactDigest or result snapshot identity. 017 defines the exact shared bodies, canonical projections, and integrity associations before persistence/interchange; host correlation tokens are not substitutes for those identities.

A refresh can return the same MF2 text with new acquisition provenance. Its ContentDigest may remain equal while its complete ArtifactDigest differs. Such a result is a new candidate for review purposes; it does not inherit approval or replace a selection by content equality. Exact replay of an already admitted artifact instead follows 021's identity-based reuse rules.

## Validation, Publication, and Operation Outcomes

Acquire immutable inputs, admit the complete work set, and preflight the actual acquisition/disclosure and destination permissions before dispatch. Recheck still-applicable authority at the operation points required by 018; an early check is not a reusable publication permit.

For each work item, retain the bounded response/provenance and call 021's candidate validator with actual source, context, policy, and profile inputs. The first profile requires literal text or the same external string-parameter set as the source. Validation parses real MF2 and derives actual parameter/capability facts; matching a fixture label or string is not validation.

Stop dispatching new items at the first acquisition or validation failure. Report completed items, the failing item, and items not attempted. Stage a single complete candidate batch only after all work items have succeeded. No prefix of a failed attempt becomes visible in the Store.

Publish through 021's candidate transaction against the exact pinned base. The adapter stages all required artifact/provenance/evidence bodies, verifies the current base and publication authority at the serialization point, and exposes either the previous or complete new snapshot. A stale base is a conflict, not permission to merge, silently rebase, or retry the Provider.

| Outcome | Required interpretation |
| --- | --- |
| Completed, no work | No Provider calls, candidate publication, or Store writes; supply/governance/source observations are still reported, and build readiness is not implied |
| Completed, published | Every work item produced an admitted candidate and one exact candidate batch became visible; approval/selection remain separate |
| Completed, unchanged | 021 established that the exact admitted batch adds no new records; never infer this from text equality alone |
| Failed or cancelled before commit | This attempt publishes no candidate batch; retained observations do not imply Store membership |
| Publication conflict or denial | No automatic rebase, new acquisition, review, or selection; a new explicit invocation needs current inputs and authority |
| Publication outcome indeterminate | Report the exact attempted transition and require the storage host to resolve its outcome; claim neither rollback nor completed publication |

Once a candidate transaction has committed, cancellation or reporting failure cannot relabel it uncommitted. External Provider effects are not rolled back even when no Store publication occurs. Resolving an uncertain publication uses the actual 021/029 storage protocol, not a repeat of the translation request.

An existing active selection remains unchanged by refresh publication. Separate authorized review/selection decides whether to use a replacement. Missing source wording approval may block source use or later linking under its policy, but never creates source-equal translation work; any permitted non-source acquisition still needs its own source-use/disclosure authorization.

## Cancellation, Replay, and Retention

The initial attempt has finite item, byte, call-duration, staging, reporting, and retained-evidence bounds and no automatic retries. Cancellation is observed between bounded core steps and through the adapter's cancellation mechanism. A timed-out adapter is not assumed to have undone work; no new result from that attempt can be accepted after closure.

Retain the bounded issued requests, completed response bodies/provenance, failures/not-attempted membership, validation evidence, and publication outcome required by the adopted replay profile. Unsupported or invalid responses remain acquisition evidence, not successful Store candidates. Retention/disclosure follows 018 and must not leak credentials or uncontrolled response text through ordinary diagnostics.

Replay of captured acquisition runs actual association and candidate validators without contacting a Provider. It proves deterministic behavior for those retained inputs, not fresh translation or current authority. Reusing captured supply in a new publication is a new explicit operation: rederive against current inputs, verify semantic/provenance applicability, and obtain current authority. Do not relabel old acquisition as a new response or reuse a prior correlation as a permit.

Durable resumption, response-history compaction, retry/backoff policy, and external idempotency protocols are broader extensions. Required evidence cannot be dropped to report success under a capacity limit.

## Minimum Integration Example

Use the actual three Intent IDs/revisions produced for [028's representative application](./028-intlify-javascript-web-vertical-slice-design.md#representative-application). The fixture is initialized with exact request associations and these deterministic candidate bodies:

| Readable case label | Source MF2       | Direct `ja` candidate MF2 |
| ------------------- | ---------------- | ------------------------- |
| Save                | `Save`           | `保存`                    |
| Heading             | `Welcome`        | `ようこそ`                |
| Greeting            | `Hello {$name}!` | `こんにちは、{$name}!`    |

Case labels and text are explanations, not lookup identities. The adapter matches the admitted exact Intent/revision/locale and source basis; an unknown request fails. It never substitutes a DOM selector, ignores a changed source revision, or treats `noIntent('Intlify', ...)` as a request.

| Scenario | Required observation |
| --- | --- |
| Initial empty admitted Store S0 | Six plan requirements; exactly three `ja` work items and zero `en` items; real validation/publication produces S1 with three candidates |
| Repeat against S1 before review | Zero new Provider calls or Store writes; the report distinguishes existing supply from missing approval/selection |
| Repeat after separate approval S2 | Still zero new calls; selectable candidates are not selected automatically |
| Repeat after separate selection S3 | Zero new calls/writes and three applicable localized selections; 020 can use them with separately admitted source definitions |
| One checked Intent revision changes | Only its direct `ja` demand is new/stale; old candidate/review/selection evidence cannot satisfy the new revision |
| One current key is explicitly refreshed | Exactly one request; a new-provenance artifact needs its own governance and does not replace an existing selection |
| Matching supply is rejected or revoked | No automatic regeneration; report remediation/refresh need and the blocked selection separately |
| One request/response/validation fails | No candidate prefix is published; remaining items are explicitly not attempted |
| Concurrent publication changes the base | Reject the stale candidate transition without Provider retry or lost updates |
| Normal build with any Store state | Zero Provider calls and no supply/governance writes; missing coverage is an explicit build result |

The two greeting references and two output targets do not increase the work count. Changing only the render argument from `Ada` to `Kai` requires no analysis, sync, or rebuild and discloses neither value to the Provider.

The deterministic response map is the only supplied translation oracle. Work derivation, request checks, MF2 validation, provenance, Store transitions, governance, and later linking must execute their real implementations; a fixture cannot return a preapproved or preselected map.

## Dependencies, Reuse, and Performance

Retain the exact consumed plan/source semantic and evidence inputs, Store lineage/base, current candidate/support observations, Policy bodies and explicit absences, Provider adapter/configuration, refresh set, supported validation profiles, bounds, and invocation authority associations. A changed Store, policy, route, or request option invalidates its affected sync view without mutating the authoritative Requirement Plan or old artifact history.

Apply [026's performance implementation architecture](./026-intlify-conformance-and-measurement-design.md#performance-implementation-architecture):

- Admit/index the bounded Store view once through 021, then batch-query unique requirement keys. Do not rescan all history for every occurrence or target.
- Retain source, context, candidate, and evidence storage immutably; work items refer to shared admitted values rather than copying source text per placement.
- Use invocation/worker Scratch Workspaces for deduplication, ordering, and staging. Reset after success, failure, and cancellation without invalidating retained results; apply finite capacity/high-water rules.
- Reuse actual parser/semantic facts only under their complete compatible validation basis. Never replace current authority or Store-base checks with a cache hit.
- Check submitted counts/bytes before deduplication and arithmetic before expansion. Dense local indexes may accelerate finite work; process-local hashes do not replace 017 identities or canonical ordering.
- Start with the sequential adapter and scheduler-neutral core. Parallel acquisition, custom arenas, SIMD, unsafe code, and persistent caches are not prerequisites.
- Keep optional snippets, expanded traces, and feature-gated profiling out of primary measurements; required provenance and diagnostics remain part of the operation.

| Measurement owner | Minimum observations and separation |
| --- | --- |
| 022 work derivation | Submitted/unique requirement and refresh counts, source-equal exclusions, Store queries, outcome classes, retained applicability, cold/reused work, duration, and applicable allocations |
| Provider host/acquisition | Actual issued/completed/failed/not-attempted calls, request/response bytes, fixture I/O or future service time, cancellation, and retained evidence |
| 021 candidate validation | Actual parses/semantic checks, accepted/rejected bytes and results; distinguish from Provider wait and 022 scheduling |
| 021/029 publication | Batch/staging sizes, base/authority checks, commit/conflict/uncertain outcome, and physical I/O distinct from logical preparation |

The implementation plan pins cases, Method Descriptors, intervals, repetition/cache state, Memory Observation Domains, and independent result observations under 026. Retain descriptive baselines without inventing numerical timing/allocation pass thresholds here. Correctness and complete applicable evidence are completion gates; optional profiling is a separately validated non-default build mode.

## Diagnostics, Limits, and Verification

Use [019's minimum Diagnostic projection](./019-intlify-project-graph-query-and-incremental-design.md#minimum-common-diagnostic-projection). Preserve owner/stage and safely established scope, exact work key, request association, plan/base, and outcome. Distinguish missing/stale supply, governance-needed observations, invalid refresh, unavailable/unsupported routing, denied disclosure/acquisition, response-association failure, candidate validation, exceeded limits, cancellation, publication conflict, and uncertain outcome.

Expected lack of selection is not a malformed Store; a malformed Store is not expected lack of supply. Diagnostic/report limits cannot truncate an incomplete attempt into success. Do not include secrets, arbitrary source snippets, or raw unbounded Provider responses in normal error messages.

| Fixture family | Required cases |
| --- | --- |
| Complete demand | All six 028 requirements retained, three unique direct work items, source-equal exclusion, duplicate references/targets not duplicating requests, and stale/incomplete plan rejection |
| Supply versus governance | Empty Store; current selected, selectable/unselected, pending-review, policy-stale, rejected/revoked, mixed historical/current candidates; no automatic selection or governance-driven regeneration |
| Freshness and refresh | Changed revision/context; non-semantic source edits; routing/checker changes; exact explicit refresh; duplicate/unknown/obsolete/source-equal refresh rejection; equal content with distinct provenance |
| Routing and input admission | Explicit absent route with no work, absent route with work, unsupported/present policy or Glossary, missing required body, wrong scope/base, invalid Store read, and no hidden fallback |
| Request/provenance | Actual source association, wrong/duplicate/unsolicited response, unknown fixture request, changed adapter configuration, inert instruction-bearing text, and no runtime-value/credential disclosure |
| Real candidate checks | Malformed MF2, missing/extra/incompatible parameters, unsupported semantic profile, provenance mismatch, and no fabricated validation/approval flags |
| Authority and publication | Acquisition power without publication/governance power and the converse; changed authority; exact-base conflict; failed member publishes nothing; complete reopen-visible evidence; no implicit selection change |
| Failure and replay | Adapter failure, timeout/cancellation, late response, interrupted/uncertain commit, captured-response validation without calls, fresh authorization for later publication, and no automatic retry |
| Boundaries and reuse | Every adopted exact/first-over byte/count/work/retention bound; arithmetic overflow; failure/cancellation then workspace reuse; old-view immutability; fresh-versus-reused equivalence |
| Integration | Actual 020/021 handoffs, zero build-time Provider calls, separate review/selection, and complete applicable 026 evidence before later generated Web execution claims |

Expected work keys, request membership, disclosure projection, outcome sets, Store effects, and non-effects are independently specified. Recording only the final Japanese text, a zero exit status, or fixture call counts does not prove request/provenance, governance separation, or atomic publication.

## Adoption with 016 and 028

| Adopting work | Required scope and claim |
| --- | --- |
| 022 logical-core development | Closed finite test-owned inputs; actual requirement/Store classification, refresh rules, request association, bounded failures, and independent 026 fixtures |
| 016 Phases 1–3 | Analysis and identity work can proceed independently; neither source extraction nor registry publication acquires translations |
| 016 Phase 4 / complete local bundle handoff | Actual 020 plan and 021 source/selection reads; the supplied Store view must identify its real candidate and governance evidence |
| 028 localization supply | Adopt actual 017 request/artifact/Store representations, 018 source/acquisition/publication authority, real 021 validators, and the local fixture/publication adapters |
| 016 Phase 5 / generated Web execution | Flow the selected snapshot through 020, actual 023/024 capabilities/code generation, 025 Release/publication, and 027/028 execution/equivalence with applicable 026 evidence |

This document does not claim those formats, hosts, or consumers are already implemented. The implementation plan lists each owner addition before its dependent operation. The fixture path need not finish external AI/TMS or public CLI design, but must not mark localization complete using canned Store selections or analysis-only tests.

## Decision Log

| ID | Decision | Rationale |
| --- | --- | --- |
| 022-001 | Start with one complete group-scoped plan, one pinned Store base, and one deterministic local Provider | Bounds acquisition without changing the compiler's authoritative requirement scope |
| 022-002 | Separate supply presence from review, revocation, and active selection | Prevents repeated translation merely because governance is unfinished or negative |
| 022-003 | Exclude source-equal work and require exact explicit refresh keys | Preserves compiler-derived source and keeps replacement acquisition intentional |
| 022-004 | Deduplicate by exact Intent revision and direct locale while retaining applicability | Multiple references/targets do not create wording variants or duplicate translation requests |
| 022-005 | Admit one trusted registered binding, with no implicit Provider or Glossary | Keeps the minimum adapter finite and prevents hidden external acquisition |
| 022-006 | Separate Provider-visible data from retained host evidence | Supplies necessary meaning without exposing runtime values, arbitrary code, or governance credentials |
| 022-007 | Use actual response association and 021 validation with non-circular provenance | Fixture text and Provider assertions cannot establish artifact validity or approval |
| 022-008 | Dispatch sequentially, stop on failure, and publish only a complete exact-base candidate batch | Avoids partial visible supply, hidden retry, and accidental lost updates |
| 022-009 | Keep refresh, replay, current authority, and separate governance distinct | Equal wording and old acquisition do not transfer approval or publication permission |
| 022-010 | Apply bounded shared storage/workspaces and stage-separated measurement from the start | Avoids per-placement duplication and conflating Provider wait with compiler performance |

## Deferred Follow-Up Notes

### Required follow-up for minimum Web execution

- 017: the `acquisition-record` and `provider-routing-policy` representations, together with the source/localized, validation, requirement, Store, and transition representations required by 020/021, are now defined under its Minimum Web Localization Representation with closed decoding, canonical ordering, identity projections, and non-circular references; the adopting implementation must materialize them before shared persistence/interchange.
- 018: the `disclose-source`, `invoke-provider`, `read-store`, and `stage-candidates` powers, the Provider-visible projection, and the provenance/retention rules are now defined under its Minimum Web Localization Extension in the same trusted-local-host mode; existing source/registry grants do not authorize these operations by implication.
- 021/028/029: implement real validation, candidate transactions, protected local publication/outcome resolution, the deterministic registered Provider, and their integration/failure tests. Separate governance still needs its own actual 021/018 checks.
- 023/024/025/027: the minimum message/value behavior, target code generation/admission, Release/publication, and the Runtime-backed execution path required by 028 are now defined; their implementations remain separate work.

### Broader extensions

- External AI agents, TMS/human adapters, network credentials, provider-specific errors, backoff, idempotency, durable resumption, streaming, and bounded concurrent acquisition.
- Locale/context/risk routing, multiple Provider results, Glossary and constraint sets, rich MF2 compatibility, and explicit refresh/remediation policies beyond this fixed profile.
- Compatible multi-plan aggregation, library/multi-owner supply, sparse or remote Stores, shared incremental caches, and broader performance budgets under 026.
- Public sync/dev workflows, automatic governance orchestration with independent powers, review UX, retention cleanup, migration, and production packaging.

## Relationship to Other Documents

- [000](./000-intlify-overview-design.md#localization-synchronization-and-governance-workflows) defines explicit supply and governance separation; this document narrows synchronization to the finite local profile.
- [020](./020-intlify-requirement-planning-and-linking-design.md) supplies complete requirements and later consumes selections; synchronization observations never become fields in that plan.
- [021](./021-intlify-translation-store-and-governance-design.md#read-handoffs-to-synchronization-linking-and-publication) supplies the Store facts and owns actual candidate validation/governance/publication.
- [028](./028-intlify-javascript-web-vertical-slice-design.md#design-overview) supplies the three-request integration and the later real localized browser execution goal.
- [026](./026-intlify-conformance-and-measurement-design.md#adoption-and-verification-requirements) supplies applicable correctness, evidence, measurement, and profiling requirements; it has no separate implementation phase to finish first.
