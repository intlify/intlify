# Intlify Requirement Planning and Linking Design

## Purpose

This design defines the smallest deterministic planner and linker needed to turn [019](./019-intlify-project-graph-query-and-incremental-design.md)'s checked local source handoff into localization demand and selected message definitions for [028](./028-intlify-javascript-web-vertical-slice-design.md)'s Web integration.

It answers two different questions:

- Before synchronization: which Intent revisions are needed in which requested locales, targets, and Delivery Units?
- During a build: which exact source or selected localized definition satisfies each requirement against the pinned inputs?

The first answer is a Store-independent `LocalizationRequirementPlan`. The second is a Store-bound `MessageBundlePlan`. A complete requirement plan may exist while translations or approvals are missing; that does not make a complete bundle plan possible. Neither operation calls a Provider, publishes a selection, rewrites source, or renders a message.

For example, 028 has three Intents, four references, and two requested locales. Planning produces six requirements, including the three source-equal `en` requirements. The two greeting references share demand without losing their separate source occurrences. Linking later selects three compiler-derived `en` definitions and three selected direct `ja` definitions, then records their placement for both Web targets.

The initial scope is one finite application, one selected Deployment Compatibility Group, direct-required coverage, and one logical Delivery Unit per target. It is not the full library-composition, fallback, delivery-optimization, or product workflow assigned to 020 by 000.

## Goals

- Consume actual checked source, identity, scope, applicability, and profile inputs without rediscovering application code.
- Keep Store-independent requirements separate from snapshot-dependent satisfaction and Provider work.
- Preserve complete Intent identity, semantic revision, reference occurrences, requested locale, definition locale, and target placement as distinct dimensions.
- Derive source-equal demand without treating authored source as already approved or as a translated Store candidate.
- Verify an exact source definition or active scoped localized selection for every required row before producing a complete bundle plan.
- Make missing definitions, incomplete scope, unsupported profiles, and resource failures explicit and explainable.
- Apply 026's bounded storage, reusable workspace, deterministic ordering, and fresh-execution equivalence from the first implementation.

## Non-Goals

- Source parsing, DOM recognition, host call-graph discovery, registry reconciliation, or persistent Intent ID allocation.
- Library/package composition, cross-owner references, dynamic module admission, or conditional authoring forms that 016 defers beyond its first minimum.
- Multi-group planning in one transaction, synchronization aggregation, or target-specific wording for one Intent revision and definition locale.
- Message-locale fallback selection, fallback-allowed coverage, hoisting, multi-unit delivery optimization, or runtime locale negotiation in this initial profile.
- Provider routing or refresh execution, candidate validation/publication, approval, selection, revocation, or Store transactions.
- Defining missing shared artifact codecs, Policy/Target bodies, a new public API, CLI command, query language, or stable cache format.
- Target capability admission, source rewriting, generated handles, physical output layout, Release Assembly/publication, or Runtime execution.
- A persistent cache, background scheduler, general incremental engine, or Store-wide historical audit.

## Ownership and Dependencies

| Owner | Responsibility in this subset |
| --- | --- |
| 015 | Checked project and Selection Scope, canonical locale facts, groups/targets, coverage table, fallback and delivery specifications, policy references, and applicable binding projections |
| 016 | Source meaning, parameter requirements, identity/revision associations, source-definition derivation basis, and source/reference evidence |
| 017 | Existing authoring identities and representations; later complete source/localized message, graph, plan, Store, and target encodings |
| 018 | Applicable input-origin admission, source and Store read authority, safe disclosure, and provenance verification; analysis permission is not source approval |
| 019 | Immutable checked local handoff, retained dependencies, query scope, and the common Diagnostic projection |
| 020 | Group/input admission, final local reachability, requirement construction, source/selection verification at linking, logical placement, and the two complete plans |
| 021 | Store snapshot validity, source-admission and localized-governance rules, active Selection Decisions, and their exact evidence |
| 022 | Snapshot-bound satisfaction for synchronization, finite Provider-work derivation, candidate acquisition, and explicit refresh |
| 023/024 | Portable execution semantics and technical capability meaning; authoritative target admission, generated bindings, host-lowering plans, and physical output |
| 025/027 | Release consistency/publication and deployment-selected execution respectively |
| 026 | Applicable conformance, lifetime/performance architecture, observations, and profiling isolation |
| 028/029 | Actual local integration, source-verified host applicability/topology inputs, acquisition, and separately authorized workflows |

020 verifies owner results and their exact associations; it does not replace their validators with a `trusted` flag. [017's current authoring subset](./017-intlify-shared-artifact-and-version-admission-design.md#minimal-intent-and-reference-artifacts) supplies source facts, not a complete source-message, Store, or Bundle Plan format. The required additions remain with their owners and are listed under adoption below.

## Terminology

| Term | Meaning here |
| --- | --- |
| Planning basis | Exact source handoff, profile/bindings, selected group, supported operation profile, root/applicability evidence, topology, and limits consumed by planning |
| Application execution root | An explicitly admitted host entry from which message uses may execute; not a Delivery Unit root or a name inferred by the linker |
| Requirement key | Complete owner-qualified Intent ID, Intent revision, and canonical requested locale within one group-scoped plan |
| Applicability edge | Evidence that an exact reference use contributes demand for a selected Target ID and its bound logical Delivery Unit |
| Source-equal requirement | Requested locale equals that Intent's checked source locale; a source-fulfillment path, not approval or a third coverage mode |
| Definition selection | One exact admitted source or localized artifact plus its definition locale and required evidence for a requirement |
| Placement | Association of that selection with a Target ID and logical Delivery Unit; not a second semantic selection or an output filename |
| Complete plan | A checked result covering its full declared input scope, not a partial row list or a caller-supplied completeness label |

These names describe logical values and operations. They do not freeze Rust types, public exports, wire fields, artifact kinds, or another identity/digest scheme.

## Minimum Inputs and Supported Profile

The minimum planning/linking profile pins its operation revisions and adopted owner adapters before accepting input. Adapters have closed logical input/result shapes and independently tested validation rules; source-supplied plugins, executable selectors, arbitrary dependency lists, and unknown revisions cannot establish those rules.

| Input group | Minimum supplied content |
| --- | --- |
| Source handoff | One complete checked 019 local handoff with expected source membership, actual 016 outcomes, exact current 017 Intent/reference artifacts, declarations, exclusions, and retained source/evaluation evidence |
| Project context | Applicable checked 015 Profile facts and binding projection, Selection Scope, canonical locale/vocabulary inputs, coverage decisions, and exact governing specifications/policies |
| Transaction selection | The Group ID selector and its evidence; every Target ID and required Target Profile binding in the selected group |
| Host applicability | Exact declared execution roots and complete source-verified reference applicability for the selected targets under the adopted host profile |
| Delivery topology | The exact finite Delivery Unit Graph artifact partition and reference-to-unit bindings required by 015; each adopted graph has one `["main"]` logical unit and no edges |
| Admission and limits | Applicable current read/disclosure admission and explicit finite input, graph, requirement, placement, lookup, evidence, diagnostic, and scratch capacities |
| Additional link inputs | One exact admitted Store snapshot and required bodies, compiler-derived source definitions, and applicable validation/source-admission/selection evidence and Policy bindings |

The 028 fixture supplies source locale `en`, requested locales `en` and `ja`, one `ui` surface class, and two Web targets. These values are inputs, not hard-coded locale or vocabulary defaults. The core compares already canonical values and never adds a parent locale, default locale, or locale-data source of its own.

Every produced requirement must resolve to `direct-required` under the admitted 015 coverage table. The initial profile admits no applicable message-locale fallback sequence or hydration relation. Unsupported policy, topology, or authoring features fail explicitly; they are not flattened, weakened, or treated as an empty success. The execution fixture uses literal text and string interpolation, but authoritative acceptance of the selected messages by a target remains 024 work.

Production adoption follows [015's consumer input boundaries](./015-intlify-project-profile-and-locale-policy-design.md#consumer-input-boundaries), including its required bindings and retained construction inputs/evidence. Missing required bodies cannot be repaired by a digest or by rereading configuration. Finite test-owned inputs may exercise an explicitly named test profile, but neither those values nor PR #205's configuration/locale core become a complete production Profile or production-admitted plan.

## Design Overview

| Step | Operation and result | Required separation |
| --- | --- | --- |
| 1. Admit scope | Check the selected group, complete source handoff, owner/profile associations, root evidence, and topology partition | No acquisition, inferred defaults, or registry writes |
| 2. Establish applicability | Resolve the finite source-verified uses to current Intent revisions and target/unit edges | 019 computation dependencies and delivery loading edges are not application reachability |
| 3. Plan requirements | Construct the complete Store-independent requirement set and source-fulfillment bases | No Store lookup, approval evaluation, or Provider-work count |
| 4. Synchronize separately when requested | 021/022 workflows supply and govern localized candidates against their pinned Store base | This is outside both 020 operations; build never invokes it implicitly |
| 5. Replan and link | Recompute or fully validate the current planning basis, then verify source/direct localized definitions against one pinned Store snapshot | A previous plan or newly returned candidate is not build authority |
| 6. Freeze bundle handoff | Record one selection per requirement, all logical placements, exact reference associations, evidence, and diagnostics | 024 generates outputs; 025 assembles and publishes the Release |

Conceptual `plan_requirements` performs steps 1–3. Conceptual `link_outputs` consumes the current planning basis, its checked plan, and the additional link inputs for steps 5–6. These are the two operations introduced by [000](./000-intlify-overview-design.md#localization-synchronization-and-governance-workflows), not reserved public API names.

## Group, Root, and Delivery Admission

### One selected group

Follow 015's transaction-selector rule: omission is allowed only when the checked Profile contains one group; several groups require one exact Group ID selector. Unknown or multiple selectors, a Target ID in place of a Group ID, and a proper subset of a group's targets are invalid. Planning includes every member of the selected group, while other groups remain separate transactions.

Each selected target uses its checked supported requested-locale subset and effective default. Requirement expansion uses the supported subset, not only the default. Selection Scope is the governance namespace from the Profile, never derived from a target, group, platform, or source path.

### Source-verified roots and uses

019's complete inventory is necessary but does not prove final reachability. The host supplies a finite projection binding exact source entry occurrences, selected targets, reference occurrences, and the rule/evidence that establishes their applicability. The adopted Producer/host checker validates it against the retained source and expected inventory; an arbitrary list of supposedly reachable IDs is insufficient.

The first host profile supports the direct root-to-use relation of 028's single-module `render` fixture. It is not a general JavaScript call-graph algorithm. Every reference in the declared scope must have accounted-for applicability or checked non-applicability to the selected roots/targets. Missing evidence, unbounded reference escape, or an unsupported intervening host relation blocks the complete result; omission is not proof of unreachability. Merely spelling a function name `render` grants no root semantics.

The core follows the admitted use-to-Intent relations and retains the exact current ID/revision associations. The initial inline and reusable-local authoring forms resolve each use to one checked declaration; a larger target array does not admit 016's deferred conditional forms. Potentially executable uses remain included without evaluating their arguments or relying on one observed runtime branch.

Checked declarations with no reachable use remain in the 019 inventory but create no localization requirement or placement. `noIntent` exclusions remain exclusion evidence, not requirements. Neither case retires an ID, removes source code, deletes Store history, or suppresses an upstream source error. A failed complete authoring result cannot be made acceptable by pruning its failed members.

### Exact topology and bindings

Adopt [015's Delivery Unit Graph semantics](./015-intlify-project-profile-and-locale-policy-design.md#delivery-policy-and-topology-inputs). Graph applicability sets must be nonempty, pairwise disjoint, and cover exactly the selected group's Target IDs. One graph may cover both Web targets. A target covered by zero or multiple graphs, an out-of-group target, a missing endpoint, or a duplicate/conflicting reference binding fails admission.

The host explicitly supplies the standard one-node graph with logical ID `["main"]`, no edges, and its derived single root. The core does not synthesize it from a filename or target. Each applicable reference binds to exactly that graph's existing unit; the complete binding relation must agree with the source-verified applicability. A broader valid topology is unsupported by this initial profile rather than silently collapsed to one unit.

Delivery roots mean loading-order roots, not application execution roots. `duplicate` is the admitted logical placement policy. Eager loading in 028 is a later 024 physical realization, not a special unit kind or a new graph edge.

## Store-Independent Requirement Planning

### Construction and completeness

After admission, construct requirements as follows:

1. Enumerate the admitted reachable reference/target/unit edges in canonical order and resolve their complete Intent identities and current revisions.
2. For each edge, enumerate that target's supported requested locales. Preflight bounded expansion before allocating the resulting rows or associations.
3. Resolve coverage from the canonical requested locale and the declaration's checked surface class using 015's decision table. Source locale, target, and delivery placement do not change that cell's meaning.
4. Group demand by the complete requirement key. Retain every distinct contributing reference and target/unit edge; repeated delivery of the same asserted input edge is an input error, not a second logical use.
5. Record each Intent's canonical source locale and exact source-definition basis. Mark a row source-equal precisely when its requested locale equals that source locale; do not compare message wording or use the project source default again.
6. Freeze the complete plan and its retained input associations only after every required row, applicability edge, policy result, and evidence record is established.

Equal message text or equal semantic revision digests do not merge different Intent IDs. Multiple uses or targets may share a requirement, but their applicability remains inspectable. A locale unsupported by a target contributes no requirement edge or missing-localization debt for that target.

An empty requirement set is valid when checked roots and complete source/applicability evidence establish that no localizable use is reachable. Absent roots, failed scope checks, truncated enumeration, or missing declarations never produce that empty-success result.

### Logical plan contents

| Part | Required meaning |
| --- | --- |
| Basis | Exact application/project context, Selection Scope, one selected group and its complete target set, source handoff, topology partition, operation/adapter revisions, and consumed policy/locale/binding inputs |
| Requirements | Canonically ordered complete Intent ID/revision/requested-locale keys, checked surface-class coverage decision, source locale, and source-equal versus non-source-equal state |
| Applicability | Exact contributing reference occurrences, Target IDs, graph/unit bindings, and source-verified reachability reasons |
| Source fulfillment | Deterministic derivation basis from the checked Intent/declaration and adopted compiler specification to its source definition; not a source-admission result |
| Outcome and dependencies | Full-scope completion, required retained inputs/evidence, typed dependencies, and component diagnostics independent of optional presentation |

The plan has no selected definition locale, current approval/selection state, satisfied/missing flag, Store snapshot dependency, or Provider job list. Changing only Store contents or governance decisions leaves its logical contents unchanged when its declared planning inputs are fixed. Source, profile, policy, applicability, or topology changes still require their own revalidation.

A source-equal row remains in the plan even when required source approval is absent, and never creates Provider work. Non-source-equal demand is not itself a Provider job: 022 compares the plan with its pinned Store base and explicit synchronization policy/refresh inputs. It retains equivalent originating applicability when deduplicating work. Planning neither searches another Store nor predicts how many jobs an arbitrary Store state will require.

## Source Definitions and Localized Selection

### Compiler-derived source definitions

The shared compiler derives a source definition from the exact checked Intent/declaration semantics supplied by 016, without Provider synchronization. Linking receives or prepares that result under the adopted source-artifact specification and verifies the complete Intent ID/revision, canonical source locale, MF2/parameter semantics, derivation inputs, and source provenance associations.

Keep `ContentDigest` distinct from the complete source `ArtifactDigest`. An unchanged Intent revision is not proof that all source evidence or artifact associations are unchanged. Original source/inventory digests, raw text equality, or 017's minimal `message-intent` reference cannot substitute for a complete `SourceLocaleMessageArtifact` when that artifact is required by downstream admission.

For a source-equal requirement, verify the applicable source-admission policy and evidence. Authenticated source may be sufficient only when the actual policy permits it; a separate required approval names the exact source artifact and policy scope. Analysis permission, a successful parser result, source control, and the fact that the author wrote the message do not supply that approval by themselves.

A source artifact is never a localized Store candidate and never requires a `SelectionDecision`. Its payload is derived from source; the Store may hold separately required approval evidence without holding that payload. Missing source approval blocks linking under this strict profile, but does not erase the requirement or cause translation of the source locale.

### Direct localized definitions

For each non-source-equal requirement, resolve the active `SelectionDecision` for the exact Selection Scope, complete Intent ID/revision, and direct definition locale equal to the requested locale in the one pinned Store snapshot. Verify the decision and its selected complete `ArtifactDigest` under the applicable 017/018/021 rules.

The selected artifact and retained evidence must match the current source/Intent association, definition locale, MF2/parameter requirements, technical-validation specifications, provenance, and applicable governance/policy inputs. Preserve the distinction between stored, technically valid, selectable, and selected. A reviewable candidate or an eligible but unselected candidate is not a substitute for an active selection.

Missing, conflicting, stale, ineligible, or snapshot-inconsistent selection/approval evidence blocks the affected requirement. The linker never picks the first, newest, closest-worded, or highest-scored candidate; changes governance state; or substitutes another Intent revision. Equal candidate content does not make different artifact/provenance identities interchangeable.

The initial profile probes only the direct requested locale. A missing `ja` selection cannot silently use `en`, another locale, a library resource, the current filesystem, a previous bundle, or freshly returned Provider output. One requirement receives the same definition across its target/unit placements; target-specific wording requires a distinct Intent, not a hidden selection branch.

Store and governance validation remain scoped to the pinned snapshot and policy inputs. Later revocations do not mutate that historical input or trigger network reads inside linking. 025's new-publication admission separately checks its required revocation view. Conversely, reproducing a historical bundle is not proof that it is currently publishable.

## Final Linking, Placement, and Export Handoff

`link_outputs` requires a current complete planning basis. A normal build recomputes planning; verified reuse is allowed only under the complete dependency rules below. A serialized plan label, equal project name, unchanged registry, or matching Intent revision alone cannot validate an old plan.

Resolve every requirement under the source/direct-selection rules above and retain exactly one admitted definition and its evidence. Success for some locales or targets cannot produce a checked group-scoped bundle when another required row fails. Independently valid rows may be retained for authorized inspection, but are not a partial `MessageBundlePlan` usable by export.

The complete bundle has these logical parts:

| Part | Required meaning |
| --- | --- |
| Basis | Exact current Requirement Plan and source handoff, Profile/bindings, selected group, pinned Store snapshot, governing owner specifications, and source/selection admission inputs |
| Selections | Exactly one source or localized definition for each requirement key, with definition locale, complete artifact identity/body, and exact validation/governance evidence |
| Placements | Separate canonical `(requirement, Target ID, graph/unit)` associations derived from the admitted applicability and `duplicate` policy |
| Reference handoff | Every reachable source occurrence and parameter/evaluation mapping linked to its selected Intent requirement and target placements; exclusions and checked non-applicability remain available |
| Outcome and dependencies | Complete required enumeration, retained source/artifact/evidence storage, typed dependency associations, and owner-preserving diagnostics |

Multiple reference uses in the same target/unit share one definition placement while retaining their distinct reference associations. Logical deduplication does not authorize deleting host evaluations or reordering parameter expressions. Unreachable checked declarations and unselected historical candidates are omitted from output demand, not deleted from their owning inventory or Store.

In the direct-only profile, selected `definitionLocale` equals `requestedLocale`, but both meanings remain explicit for downstream execution. A future fallback extension must preserve this distinction rather than silently changing the initial algorithm or treating fallback as source equality.

024 receives the actual selected messages, parameter/capability facts, exact references, logical placements, and source/evaluation evidence. It authoritatively admits target capabilities, creates target handles or direct code, and produces the host-lowering plan and physical output sets. 020 does not derive a handle from an Intent ID, reinterpret a Delivery Unit as a path, or claim that linking success proves target execution support. Parser validity and target capability failure remain different outcomes.

Every target output must preserve the group-wide selections. Runtime-backed and ahead-of-time targets may encode them differently, but cannot choose different wording or silently drop required locale/placement entries. Complete compatible target outputs and Release/publication checks remain 024/025 responsibilities; a checked bundle is not a Release.

## Minimum Integration Example

For the exact 028 fixture, with valid source-admission evidence and direct coverage:

| Observation | Expected result |
| --- | --- |
| Source inventory | Three distinct Intent IDs, four references, and one exclusion |
| Complete requirements | Six rows: each of the three Intent revisions paired with `en` and `ja` |
| Target applicability | Both Web targets retain their contributing reference/unit edges; target count does not multiply the requirement count |
| Empty admitted Store | Requirements remain complete; linking is blocked by three missing direct `ja` selections |
| Explicit initial synchronization | Under 022's fixture rules, three `ja` requests and no `en` requests; this is a separate workflow |
| Candidates stored but not selected | Direct `ja` linking remains blocked, even if technical validation and approval succeeded |
| Source admission and direct selections valid | Six definition selections and twelve logical target/unit placements, with one unit for each of the two targets |
| Unchanged normal build | Same logical plans and selection/placement results; zero Provider calls and zero registry/Store writes |

The twelve placements are not a physical file-count promise. 024 determines whether outputs are combined, split, or encoded inline under its profile. Source text, DOM selectors, and fixture case labels never become shared lookup keys.

## Dependencies, Invalidation, and Local Reuse

020 registers complete dependency inputs for its adopted operations with 019. A local hash or graph index may accelerate lookup, but neither replaces exact owner-defined identity/content comparison nor becomes a new shared artifact digest.

| Product | Dependencies that must be retained |
| --- | --- |
| Reachability/applicability | Exact complete source handoff and current associations, root/use evidence, selected group/targets, topology/bindings, and adopted host/020 rules |
| Requirement Plan | Applicability, complete Intent IDs/revisions and source bases, consumed Profile/locale/coverage/fallback/delivery facts, governing input bindings, operation revision, and limits/options |
| Definition resolution | Current requirement/source basis, actual source artifacts, one exact Store snapshot and required bodies, source/selection/validation evidence, and applicable governance/trust policies |
| Bundle/placement handoff | Current complete requirements and admitted selections, exact reference/evaluation evidence, group/target/unit associations, required source/artifact dependencies, and result limits |

A conservative whole-Profile, whole-handoff, or whole-Store key is valid until an owner defines and tests a narrower facet. Do not guess a dependency slice from equal visible fields. Explicit policy/input absence is retained; an unavailable required body is a failure, not an empty value or cache miss that can be repaired from untrusted content.

Store changes can invalidate linking without changing Store-independent requirement meaning. Root, target-locale, coverage, topology, source, or context changes can change demand even when registry identity is unchanged. Host-expression or source-position changes may preserve Intent semantics while requiring new reference maps, source artifacts, plan associations, or lowering evidence. No retained approval or selection is carried forward under a different artifact identity without the owner's applicable rule.

The first implementation may rebuild both small plans on every invocation. Optional bounded in-process reuse stores only immutable checked products, performs current input/read/disclosure checks on hits, and satisfies the no-cache reference result. Failed or cancelled work never becomes a successful cached plan; eviction changes cost, not meaning, caller-owned result validity, or authority. No invalidation operation invokes sync, retires an ID, or deletes history.

## Diagnostics, Limits, and Trust

Use [019's common Diagnostic projection](./019-intlify-project-graph-query-and-incremental-design.md#minimum-common-diagnostic-projection) while preserving owning component codes, stages, evidence, and outcomes. The minimum 020 reason families cover unsupported/invalid planning input, incomplete scope, unresolved reference, invalid topology/applicability, unavailable or ineligible required definition, resource exhaustion, cancellation, and invariant failure. They do not rename parser, Profile, authorization, governance, or target-capability errors.

An applicable diagnostic identifies the exact invocation/plan scope and, where established, the reference/Intent revision, requested and definition locales, coverage decision, selected group/target/unit, pinned Store, source or selected artifact, and the owning failure evidence. A symbolic suggestion may point to explicit synchronization or review but cannot run it or carry permission.

Normal link diagnostics concern the current plan's applicable inputs and selected definitions. Unreachable declarations or unselected historical candidates do not produce default coverage noise. This does not waive 017/021's required Store integrity/admission checks or hide a failure in an input needed to establish the selected snapshot. Unsafe pre-admission content uses the owning disclosure-safe input-slot diagnostics, not untrusted paths or message snippets copied into public output.

Bound submitted bytes/counts, source/reference/root records, target and locale expansions, graph artifacts/bindings, requirement and placement rows, lookup/key-validation work, retained artifact/evidence bytes, diagnostic output, and scratch/cache capacity. Count submissions before deduplication and check arithmetic before products or allocations. There are no implicit numeric defaults or valid-prefix truncation rules in this design.

Missing owner checks, incomplete required evidence, denied access, capacity failure, or cancellation prevents a complete checked result. Diagnostic truncation stays explicit and never removes mandatory outcome/completeness status. A failed new invocation cannot return a previous plan as its successful result. Scratch is reset on every outcome; returned immutable results retain their own storage.

Both operations receive only their required read/admission projections. They have no Provider client, source-execution callback, registry writer, Store/governance publisher, Release-publication handle, or mutable current-locale state. The existing 018/029 source/registry authority is not automatically authority to read a Store or approve a source definition; the applicable extensions must be adopted explicitly.

## Performance and Measurement

Apply [026's performance implementation architecture](./026-intlify-conformance-and-measurement-design.md#performance-implementation-architecture) to the first owner operations:

- Retain source, Profile, Store, message, and evidence bodies immutably; use dense invocation-local indexes and bounded lookup instead of copying full bodies into every requirement or placement.
- Build source/reference/Intent and exact selection lookup indexes once for the consumed snapshot. Expand only reachable uses and supported target locales, then deduplicate requirement keys without losing applicability. Do not scan the entire Store for each row or reparse a message per target/unit/reference.
- Keep source preparation, Store/owner admission, planning, final linking, host projection, and physical generation distinguishable. Reuse real parser/validation results only with complete owner input/specification associations; shared parsing never merges Intent or candidate identity.
- Prefer ordinary collections and a reusable Scratch Workspace initially. Capacity hints come from admitted counts; pathological retained capacity has an explicit bound/release policy. Returned plans and caller-pinned snapshots cannot borrow resettable workspace storage.
- Canonicalize output explicitly. Process-local fast hashes are optional under 026's input/collision/work rules, not persisted identities. Graph/policy work needs no SIMD, custom arena, or unsafe implementation in this minimum.
- Keep the core synchronous and scheduler-neutral; do not create a nested pool or background recomputation. Optional snippets, expanded traces, and profiling stay off the ordinary path while required evidence remains available.

| Operation | Core interval and observations |
| --- | --- |
| Requirement planning | Retained admitted planning inputs through group/applicability checks and complete frozen plan; input visits, reachable IDs/uses, locale expansion, requirement/applicability counts, scratch, retained bytes, and logical outcome |
| Final linking | Current checked plan plus admitted source/Store inputs through required resolution and complete bundle; exact lookup/validation work, selected artifacts, reference/placement counts, retained bytes, and outcome |
| Repeated/rebuild path | Full new request through lookup/revalidation and necessary owner work to the new result; no-cache/cold/warm/evicted and changed-input states with separately attributable owner intervals |

Pure planner/linker intervals exclude file/network I/O and independently measured parsing, owner artifact admission, source preparation, export, and rendering. An integrated case includes those costs under explicit separate intervals rather than reporting cached work as a full build. Logical observations include complete ordered demand, source-equal/coverage facts, selection and evidence identity, reference associations, placements, and diagnostic/outcome meaning; pointer values, cache counters, timings, and optional human text are not semantic equality.

The adopting implementation pins finite workloads, actual owner/result schemas, independent expected outcomes, and the applicable 026 evidence projection before admitting measurements. Begin with descriptive baselines, work counts, and named memory domains, not invented speed budgets. [Optional profiling](./026-intlify-conformance-and-measurement-design.md#profiling-specification) uses a non-default build feature, has no required ordinary-build instrumentation cost, and does not replace primary measurements.

## Conformance and Completion

| Area | Required independent cases |
| --- | --- |
| Group and context | Single-group omission, exact multi-group selection, unknown/multiple/subset selectors, mismatched context/bindings, target-locale subsets, and test versus production input |
| Scope and reachability | Complete versus partial/blocked/truncated handoff, accounted-for roots and uses, unsupported host evidence, missing targets, exclusions, valid empty demand, and unreferenced live declarations |
| Identity and demand | Equal text at separate IDs stays separate; several uses/targets share one requirement; changed source locale, semantic revision, surface coverage, and requested-locale applicability |
| Delivery | Explicit one-node graphs; exact target partition; shared graph for both targets; missing/duplicate/conflicting bindings; unsupported broader topology; no filename-inferred unit |
| Store independence | Different empty/populated/governance snapshots do not change a fixed-input Requirement Plan; source-equal rows remain even without source approval |
| Source fulfillment | Exact compiler-derived source and policy evidence; required missing/stale approval blocks link, not planning; no source payload publication or source Selection Decision |
| Localized selection | Missing direct artifact, stored/approved but unselected candidate, wrong scope/ID/revision/locale/digest, conflicting active decisions, invalid evidence, and pinned historical versus later revoked state |
| Final bundle | One definition per requirement, complete target placements, preserved multiple-use evidence, one failed row preventing a checked bundle, and no implicit locale or previous-result fallback |
| Invalidation and limits | Changed policy/topology/source evidence with stable Intent ID, exact/first-over capacities, arithmetic overflow, input permutations, reusable workspace after failure, and fresh/cached equivalence |
| Integration and separation | Real adopted 016/017/018/019/021 results, 028's six requirements/twelve placements, explicit 022 supply, 024 capability failure remaining separate, and zero source/registry/Store/Provider side effects |

Canonical order follows complete 017 Intent and occurrence identity, then the owner's specified revision/locale order; canonical locale and Target ID strings use unsigned UTF-8 byte order, while source coordinates use numeric order. Requirement keys order by Intent ID, revision, then requested locale. Within a requirement, placement orders by Target ID and logical unit identity; retained contributing occurrences use 017 order. Graph partition and evidence references use their admitted owner order. Hash iteration, worker completion, filesystem order, and rendered message text never choose a definition or break a tie.

Completion requires the real adopted owner checks, actual planning/linking operations, independently specified expected plans, and a full-rebuild reference for incremental cases. A prefilled six-row table or a mock selection callback proves neither complete planning nor valid linking. Verification claims name the supported profile and missing extensions; a passing logical core fixture does not establish all of 016 Phase 4–5, complete 028 execution, or I1 conformance.

## Adoption with 016 and 028

The following are dependency gates for adopting this subset, not a requirement to finish every downstream design first.

| Adopting work | Required specification/implementation before its claim |
| --- | --- |
| 020 logical-core development | Closed finite test input/result shapes, real 020 scope/set/placement checks, independent oracles, and applicable 026 owner observations; test results remain explicitly test-scoped |
| Complete local requirement handoff | Actual 016/019 complete source facts and source-verified host applicability; necessary checked 015 context/bindings; applicable 018 read admission; adopted topology and plan representations when serialized |
| Complete source/direct-localized bundle | 017 source/localized/Store/plan representations and integrity; 018/021 source/Store provenance, policy-body validation, source admission, active selection, and exact snapshot evidence; actual source preparation and owner validators |
| 028 synchronization integration | 021/022 satisfaction, candidate and governance operations using this complete plan; current 029 registry-host work alone does not implement Store workflows |
| 028 generated Web execution | 023/024 message capability, Target Profile and generated/lowering formats; 025 Release admission; 027 Runtime-backed path and 028's host/equivalence checks |

017 defines the exact artifact schemas and identity domains for these plans as `delivery-unit-graph`, `requirement-plan`, `bundle-plan`, and `source-locale-message` under its [Minimum Web Localization Representation](./017-intlify-shared-artifact-and-version-admission-design.md#minimum-web-localization-representation); shared serialization or persisted production plans use those kinds rather than an extended authoring kind, an arbitrary JSON hash, or an in-memory struct layout promoted into a wire format. 020 defines their logical planning/linking meaning here. The necessary 021 Policy/Store and 024 Target-body rules are also separate owner work, not unconditional adapters hidden in this design.

The existing 016 Phase 1–2 semantics/Producer work does not wait for Store-backed linking. Conversely, the minimum local path does not discharge broader cross-owner/library, fallback, or target requirements merely because its graph is finite. Remaining owner dependencies are recorded without changing 016's phase definitions or updating other design files.

## Decision Log

| ID | Decision | Rationale |
| --- | --- | --- |
| 020-001 | Start with one local application and one complete selected group | Supplies 028's finite path without cross-owner or multi-group orchestration |
| 020-002 | Separate Store-independent requirement planning from pinned-Store final linking | Missing supply or approval does not redefine demand or authorize implicit synchronization |
| 020-003 | Require source-verified root/applicability evidence separate from 019 dependencies and delivery topology | Prevents inventory membership or loading edges from becoming invented final reachability |
| 020-004 | Group demand by complete Intent ID/revision/requested locale and retain all applicability | Deduplicates localization need without merging identities, targets, or host evaluations |
| 020-005 | Retain source-equal rows and verify source admission only at definition use | Avoids source-locale Provider jobs and approval inferred from authored text |
| 020-006 | Initially admit direct-required coverage and one explicit logical unit per target | Keeps fallback/hoisting outside the first algorithm without weakening complete coverage |
| 020-007 | Verify active scoped localized selections; never choose among candidates | Preserves governance authority and one exact definition across target placements |
| 020-008 | Return immutable complete plans or explicit blocked/failure results | Prevents partial or historical success from becoming generated output authority |
| 020-009 | Register full dependencies and use a no-cache reference under 026 | Permits bounded reuse without stale evidence, hidden work, or premature low-level optimization |
| 020-010 | Leave missing codecs, policy bodies, target admission, and publication with their owners | The local algorithm is not an alternate artifact, Store, or Runtime architecture |

## Deferred Follow-Up Notes

### Required follow-up for minimum Web execution

The following owner-specification adoption and implementation tasks remain necessary for 016 Phase 5's bounded Web path in 028. Defining 020's minimum planning/linking design does not complete them. They are required integration dependencies, not optional extensions beyond that Web scope, and do not block 016's bounded Phase 1–2 implementation. The [adoption gates above](#adoption-with-016-and-028) identify when each dependency must be satisfied; this list does not transfer those responsibilities to 020 or require every feature of the owning designs.

- 017 now defines the source/localized message, planning/topology, Store, supply, target, and Release records under its [Minimum Web Localization Representation](./017-intlify-shared-artifact-and-version-admission-design.md#minimum-web-localization-representation); portable diagnostics remain deferred with 019. Implementing those codecs and adopting 018's [Minimum Web Localization Extension](./018-intlify-security-trust-and-provenance-design.md#minimum-web-localization-extension) powers, 021's governance, and 024's Target Profile body remain required.
- Localization supply and selection: adopt and implement the minimum 021/022 explicit synchronization, candidate validation/publication, required approval, active selection, and immutable Store snapshot operations. Retain their exact evidence for 020 linking; defining the planner or supplying a candidate alone does not establish a selected localized definition.
- Code generation and source rewriting: adopt and implement the required 023/024 literal/string-interpolation semantics, target capability checks, generated references, source-lowering plans/maps, and Runtime-backed/ahead-of-time outputs while preserving host evaluation. Complete the applicable 025 Release Assembly, output compatibility, publication, and execution-admission checks before activating those outputs.
- Runtime execution and integration: implement the applicable 027 reference evaluator, artifact readiness, and locale-bound Localizer, then exercise the actual generated Runtime-backed and ahead-of-time paths through 028's host, locale-isolation, failure, and equivalence cases with the required 026 conformance and performance evidence. Checked source facts or bundle plans alone do not establish multilingual execution.

### Broader extensions

- Ordered message-locale fallback, source-locale fallback candidates, fallback-allowed coverage and debt, and exact expanded-probe evidence under 015.
- Multiple Delivery Units, broader static root/use relations, library/package composition, bounded conditional references after 016 adoption, and target-specific static program analysis.
- Compatible multi-plan synchronization, broader Store satisfaction/refresh scenarios, historical audit and revocation-impact queries, and localization-only Release workflows.
- Persistent/distributed caches, narrower incremental dependency facets, parallel orchestration, richer execution capabilities, and production tooling/package surfaces.

## Relationship to Other Documents and Existing Foundations

- [000](./000-intlify-overview-design.md#deterministic-build-link-and-export-area) owns the source-first two-operation architecture, one-group transactions, selection semantics, and separation from Release/execution.
- [015](./015-intlify-project-profile-and-locale-policy-design.md#consumer-input-boundaries), [016](./016-intlify-source-authoring-and-intent-identity-design.md#source-evidence-and-consumer-handoff), and [019](./019-intlify-project-graph-query-and-incremental-design.md#completeness-and-consumer-handoff) supply the checked facts and complete local input; this document adds final planning and selection/placement rules.
- [021](./021-intlify-translation-store-and-governance-design.md) and [022](./022-intlify-provider-and-localization-sync-design.md) own the Store/governance and explicit supply work consumed around the two operations. [028](./028-intlify-javascript-web-vertical-slice-design.md#design-overview) supplies their minimum integration case.
- [014](./014-ox-mf2-message-linker-design.md) and the existing [delivery graph](../crates/intlify_linker/src/graph.rs) provide bounded graph-admission and ordering techniques. Existing [bundle plans](../crates/intlify_linker/src/plan.rs) demonstrate retained selected definitions, but their catalog-key/resource domains and per-unit/per-locale plan model are not this group-scoped source-first specification. Reuse helpers only with independent tests for the adopted 020 rules.
