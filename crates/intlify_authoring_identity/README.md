# intlify_authoring_identity

Persistent Intent identity for [Intlify](../../design/000-intlify-overview-design.md)'s source authoring.

> [!IMPORTANT]
>
> This crate is the pure core of Phase 3 of [design 016](../../design/016-intlify-source-authoring-and-intent-identity-design.md): the registry identity, the representation of registry history, its transitions and replay, the checks of continuity, newness and absence, reconciliation, and read-only compilation into Intent and reference artifacts. Nothing here persists or publishes a registry. See [Current status](#current-status).

[`intlify_authoring`](../intlify_authoring/README.md) decides what a message is and records what one analysis found as an `authoring-inventory`. It assigns no identity, and a Producer such as [`intlify_authoring_js`](../intlify_authoring_js/README.md) never learns one. This crate adds persistent identity on top of that record:

- the registry an owner's Intent IDs live in, and the transitions that change it;
- the evidence that lets an ID continue across an edit or a move, and the proof a declaration is new or gone;
- the reconciliation that plans an update against one exact registry;
- the read-only compilation that resolves an inventory against a registry into Intent and reference artifacts.

## What this crate never does

- It never generates an identity's value. Design 017 requires a new value to come from operating-system randomness on an authorized update host, retained in the update plan and never generated again on replay.
- It reads no file and publishes no registry. Making a result current is the host's exact-base check, under design 018's authorization and design 029's persistence.
- It never decides identity from wording, a path, a position, the order declarations were found in, or similarity. What cannot be proven stays unresolved.

## Identities

| Identity | Type | Role |
| --- | --- | --- |
| Intent ID | `intlify_authoring::MessageIntentId` | One message lineage: an owner and a 128-bit value. Equal values under two owners are two Intents |
| Registry identity | `RegistryIdentity` | One owner's registry chain. Spelled like an Intent ID's value, but a separate type, so the two are never compared or exchanged |

Both are 32 lowercase hexadecimal digits on the wire. `Opaque128::from_bytes` spells 16 bytes that way; where the bytes come from is what makes a value fresh, and that is the host's concern.

## Registry history

Design 017 records identity history as a chain of two artifact kinds, sealed in the same envelope as an inventory.

| Kind | Type | What it holds |
| --- | --- | --- |
| `intent-registry` | `IntentRegistrySnapshot` | One immutable state: each Intent ID, whether it is active or retired, and the declaration it was last associated with |
| `intent-registry-update` | `IntentRegistryUpdate` | One candidate update against one exact base: the decisions (`continue`, `allocate`, `retire`, `restore`), the basis each claims, and copy, split and merge links |

A chain starts with a genesis, which has no history and no entries. Every later snapshot names the snapshot it came from and the update that produced it, and an update names its base and the inventory it was planned from. Nothing names its own result, so each digest exists before the next artifact needs it.

`admit_registry` and `admit_update` read one artifact in 017's order: bounded strict decoding, the exact kind, schema and specification, the closed body, the integrity digest, the caller's `IdentityLimits`, and then the body's structural rules. Each rule has its own failure in `SnapshotFailure` or `UpdateFailure`. Every reference member has to name its one kind, and the committed schemas narrow those members the same way, so neither is looser than the other.

Admission is deliberately narrow. An admitted snapshot is well formed and unaltered since sealing; that is not proof it is the result of its update, that its chain starts from an accepted anchor, or that it is current. An admitted update is well formed; a `verified-edit` or `confirmed-new` label proves nothing by being well formed.

## Applying and replaying

`apply` applies one admitted update to its exact base under the inventory it was planned from, following 017's closed transition table:

| Decision | Base entry it needs | Result |
| --- | --- | --- |
| `continue` | Active, holding exactly `from` | Active at `to`; the ID is unchanged |
| `allocate` | None, active or retired | A new active entry at `to` |
| `retire` | Active, holding exactly `from`, with a complete inventory | Retired, keeping its last declaration |
| `restore` | Retired, holding exactly `from` | The same ID active again at `to` |

The update applies as a whole or not at all, and each refusal has its own `TransitionFailure`. Every unit of the inventory has to be checked. Every declaration of the inventory has to end up with exactly one identity: either an active entry the update leaves alone already holds it, or one decision gives it one. A partial inventory updates what it covers, cannot retire anything, and leaves every entry it cannot see exactly as it was. An update with no decisions and no links is `Transition::Unchanged`: 017 records no new state for it, and reuses the base.

`replay` rebuilds a stored snapshot from its base, update and inventory, and compares the complete result with it, never only its digest. `verify_history` walks a head snapshot back to an `Anchor` the host names, the genesis it initialized or a snapshot it accepted, resolving every base, update and inventory by its exact reference in a `RetainedHistory`, and then replays the chain forward within the caller's bound. A chain that only reaches some other genesis is not anchored, and a missing artifact leaves it blocked with the reference that named nothing; no older snapshot or matching text stands in for it.

What `apply` checks is the transition, not the bases. A `verified-edit` is not replayed against source and a `confirmed-new` is not proven: an update whose bases do not hold can still apply, and its result can still replay exactly.

## Continuity, newness and absence

`verify_bases` checks why each decision of an applicable update was made, against the evidence a host supplies in `ContinuityInputs`: the bytes of every snapshot in `RetainedSources`, the edits between snapshots, the units of the owning scope, and the update that produced the base. Each decision gets a `BasisVerdict`: `Proven` from the evidence, `Explicit` when it is a choice that needs the host's confirmation, or `Unproven` with the `BasisGap` that is missing. An unproven claim is never resolved by a guess.

| Basis | Shown when |
| --- | --- |
| `unchanged-snapshot` | The declaration is exactly where it was |
| `verified-edit` | The profile is `intlify-continuity-edit-replay` revision `0`; the change list is exactly one edit, from the base declaration's snapshot to the current one's; replaying it over the retained bytes gives exactly the after bytes; it carries the old range onto the current declaration's range and role; and nothing else accounts for either side |
| `confirmed-new` | The base has no history, or the declaration lies entirely inside text an edit inserted, and no old declaration is carried onto it or still sits there |
| `complete-absence` | The rest of the plan is resolved, and either the declaration's unit left the scope with no account of where it went, or one edit from its snapshot to a snapshot the inventory holds replaces its whole range |

An edit carries a range only through replacements clear of both its ends: one before it shifts it, one strictly inside moves its end, one after leaves it alone. A replacement that touches or crosses an end leaves the range's fate unreadable, because replacing a literal's quotes and inserting a new message beside one look the same from the edit alone. Two accounts of one base snapshot, two edits from it, or an edit from a snapshot that is still current, mean a copy or a conflict, and neither side is a continuation.

The edits a host supplies have to read like one change list from the base to the current inventory, or the bases are not checked at all: each unit at most once on each side, each edit starting from a snapshot the base has (or from nothing, for a unit the base does not have) and ending at a snapshot the inventory holds (or nowhere, for a unit the inventory no longer has). An edit that writes from nothing a unit the base already has, or one that ends at a revision nobody holds, is not an account of the change.

Newness and absence are claimed automatically only when the host's membership is exactly the inventory's units (a superset for a partial inventory) and the inventory was resolved against the same pins as the one that produced the base; a change of binding configuration can make declarations appear or vanish without any edit. They are proven when an update is planned: `confirmed-new` records no evidence, so a later replay does not prove it again.

## Reconciliation

`reconcile` plans one update from an inventory against an exact base, following design 016's procedure. The host supplies the continuity evidence in `ContinuityInputs`, explicit decisions bound to the base and inventory they were made for (`ExplicitDecision`), lineage links, and fresh candidate IDs it drew itself. Nothing here generates an ID.

| Step | What reconciliation does |
| --- | --- |
| Admit | The base and the inventory have to pair (owner, scope, every unit checked), and the evidence and the candidates have to be well formed |
| Explicit decisions | Applied when bound to this exact base and inventory and fitting the base; otherwise reported as conflicts. They always need the host's confirmation |
| Unchanged | An active entry that still holds its exact declaration keeps it |
| Continued | An entry whose declaration is gone continues onto the one declaration a single replaying edit carries it to, if nothing else claims that declaration |
| New | A declaration left over is new where the base has no history or an edit into its own unit inserted all of its text; it takes the next candidate, in canonical order |
| Absent | In a complete inventory, an entry is retired where its unit left the scope with no account of it or one edit replaced its declaration; a partial inventory keeps what it cannot settle |

The result is `Unchanged`, `Planned` (the sealed and admitted update, the unsealed result, and each decision's `Eligibility`), or `Unresolved` (diagnostics in 016's reporting order, with the classification behind them). A planned update is applied to its base and its bases are checked with `verify_bases` before it is returned, so planning and checking share one set of rules. A candidate that collides with an ID the base holds, active or retired, is refused rather than skipped. `IdentityWorkspace` keeps capacity between runs, and `reconcile_with_cancellation` stops between steps without a partial result.

## Intent and reference artifacts

Design 017 hands an inventory's declarations and use sites on with two more kinds, sealed in the same envelope.

| Kind | Type | What it holds |
| --- | --- | --- |
| `message-intent` | `MessageIntentBody` | One declaration's Intent ID, the revision its current projection gives, the inventory and registry it was compiled from, and, when the declaration is not the one the registry holds, the continuity that carried the ID onto it |
| `message-reference` | `MessageReferenceBody` | One use site and, for every declaration it may use, that declaration's Intent ID, revision and `message-intent` artifact, in Intent ID order |

`admit_intent` and `admit_reference` read them in the same order as the registry kinds, and report a broken structural rule as an `IntentFailure` or a `ReferenceFailure`. A continuity is held to the rules of a `continue` decision between the same two declarations. A reference names at least one target, all of the use site's owner, and no ID twice; `IdentityLimits::targets` bounds how many. The parameters a use site supplies stay in the inventory the reference names.

Admission is narrow here too. A well-formed Intent can still name a retired ID, a stale revision or a continuity nothing proves; whether an artifact is what compiling its inventory against its registry gives is checked against a compilation, below.

## Read-only compilation

`compile` resolves an inventory against one exact registry, and gives one `message-intent` per declaration and one `message-reference` per use site. It reads identities and never makes one: it takes no candidate IDs and no way to write a registry, and it leaves both inputs as they were.

A declaration takes the ID of the active entry that holds it exactly, or of the one entry a single verified edit carries onto it while nothing else claims it. That is step 4 of reconciliation, decided by the same code. Only in the second case does the Intent carry a continuity, the entry's declaration and that one edit, so a build can use a checked edit or move before the update that records it is published. The revision is always computed from the current declaration.

Any other declaration, new or with a history the evidence does not show, makes the result `Unresolved`: an `authoring-identity-update-required` diagnostic (`identity-association-missing`) for each such declaration, and an `authoring-identity-conflict` where entries compete for one. No artifact is returned then, so a subset of a scope never reads as the whole of it. An entry whose declaration is gone does not stop compilation; retiring it is an update's question. A partial inventory compiles to the artifacts of its smaller scope, and `CompiledScope::completeness` says so.

What compilation returns stays within the `IdentityLimits` its readers admit it under: a use site that would name more targets than `IdentityLimits::targets` is a `Limit` failure, not an artifact nobody can read back. `CompileEvidence` is reconciliation's evidence less the membership, held to the same rules. A reader checks a supplied artifact with `CompiledScope::check_intent` or `check_reference`: it compiles the same scope with the same evidence, and the artifact has to be exactly the one compiled for its declaration or use site, field by field. An `unchanged-snapshot` continuity is refused because a declaration the registry holds needs none, and an `explicit` one because the confirmation it needs cannot travel with an artifact. `IdentityWorkspace` is shared with reconciliation, and `compile_with_cancellation` stops between steps and before each artifact without a partial result.

## Schemas and vectors

| File | Contents | Checked by |
| --- | --- | --- |
| `schema/intent-registry-v0.schema.json` | The closed Draft 7 schema of a sealed `intent-registry` | `vp run schema:authoring:check`, `src/schema.rs` |
| `schema/intent-registry-update-v0.schema.json` | The closed Draft 7 schema of a sealed `intent-registry-update` | `vp run schema:authoring:check`, `src/schema.rs` |
| `schema/message-intent-v0.schema.json` | The closed Draft 7 schema of a sealed `message-intent` | `vp run schema:authoring:check`, `src/schema.rs` |
| `schema/message-reference-v0.schema.json` | The closed Draft 7 schema of a sealed `message-reference` | `vp run schema:authoring:check`, `src/schema.rs` |
| `fixtures/phase3/registry-vectors.json` | One real chain, a genesis and three updates, with the inventories and source texts behind it | `vp run vectors:authoring:check`, `tests/registry_admission.rs` |
| `fixtures/phase3/compile-vectors.json` | Design 028's module through the JS Producer, from a genesis to the Intents and references compiled for two revisions | `vp run vectors:authoring:check`, `tests/compilation.rs` |

See [`fixtures/phase3/README.md`](./fixtures/phase3/README.md) for what the vectors hold and what the independent checker verifies.

## Measurement

The non-default `benchmark` feature measures two operations as a design 026 owner, through the owner run in `intlify_measurement`. Ordinary builds never include it, nor the JS Producer its fixtures are read with.

| Operation | Inside the interval | Outside it |
| --- | --- | --- |
| `identity_reconciliation / association_planning` | `reconcile`: an admitted base and inventory to association decisions and a proposed plan, with fixed allocation candidates and the case's lent workspace | Producing and admitting the inventory, building the base, checking the retained source bytes, and drawing any ID |
| `identity_reconciliation / registry_replay` | `verify_history`: a retained chain walked from its head back to its genesis and replayed forward | Building the chain, indexing the retained history, and accepting the anchor |

Allocation is not measured: drawing an ID belongs to the host, apart from deterministic reconciliation, so a fixture's candidates are fixed values.

The 16 fixtures are design 016's required workloads for this phase: the first allocation of two units, a catalog of 128 short UI literals allocated from its genesis and then read again unchanged, a second `'Pay now'` that keeps the first's ID apart from its own with the edit that shows it, the same copy without the edit, a deletion seen by a complete and by a partial view, and the exact and first-over sides of the bounds on candidates and diagnostics; and replays of a two-step chain, of the catalog's one step of many entries, of a chain missing an update, and of the bound on history steps.

Every inventory is read from source text by the real JS Producer under the test-owned context, and every base is built by planning and applying the updates that lead to it, all before any interval opens. Each fixture declares the path it takes: complete, blocked, or refused with an operational failure. A bound is taken from the fixture itself, so its exact side is the fixture's own count and its first-over side is one below.

Every measured planning reuses its case's workspace and is compared with an expectation established on a fresh one; the tests also compare it after the workspace served a success, a failure, and a cancellation. The logical work records each count as exact, as unavailable when this harness does not count it, or as not applicable when the path does no such work.

`vp run bench:authoring-identity:smoke` captures one run, writes its records, reads them back, and re-admits them; withholding any one record is refused. The numbers are for presentation only: nothing compares them with a threshold.

## Current status

This is an unpublished, workspace-internal crate. It completes the pure core of Phase 3: the registry identity, the representation and structural admission of registry snapshots and updates, applying an update to its base, replaying a chain from an anchor, checking the continuity, newness and absence an update claims, reconciling an inventory against a base, the representation and structural admission of Intent and reference artifacts, read-only compilation, and the observational measurement of reconciliation and replay. [`fixtures/phase3/README.md`](./fixtures/phase3/README.md) maps each of the phase's fixture families to the tests that check it, and names what is left to later work.

Who may confirm an explicit decision or publish a plan is evaluated by [`intlify_local_host`](../intlify_local_host/README.md), and publishing itself belongs to that host.

## What comes next

Local persistence and production publication come with a later plan. Its starting point is what this crate already gives: a plan or an unresolved report from an admitted base and inventory, which the local host authorizes and publishes in memory. It needs, before it starts:

- checked design 015 inputs and policy bodies, in place of the test-owned context this phase admits;
- a bootstrap format, which design 029 still lists as a prerequisite;
- a 029 host adapter that writes generations and one current pointer, resolves an interrupted transaction, and holds a lock.

Two questions to the design 017 and 019 owners are open, and their answers may change the representation:

- An update does not record the host's membership it was planned against. A reader replaying it sees that the inventory it names claimed to be complete, but cannot check that it covered every unit, which is what a retirement relied on. Reconciliation takes the membership from the host and requires a complete inventory to match it exactly.
- An inventory's basis does not include the binding configuration it was read under. A configuration that stops recognizing some declarations is seen here only where it also changes the basis; until the basis carries it, any change of basis stops automatic newness and absence, and the host has to treat a configuration change as one.
