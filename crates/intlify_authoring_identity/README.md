# intlify_authoring_identity

Persistent Intent identity for [Intlify](../../design/000-intlify-overview-design.md)'s source authoring.

> [!IMPORTANT]
>
> This crate is the start of Phase 3 of [design 016](../../design/016-intlify-source-authoring-and-intent-identity-design.md). So far it holds the registry identity, the representation of registry history, and its transitions and replay. See [Current status](#current-status).

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

## Schemas and vectors

| File | Contents | Checked by |
| --- | --- | --- |
| `schema/intent-registry-v0.schema.json` | The closed Draft 7 schema of a sealed `intent-registry` | `vp run schema:authoring:check`, `src/schema.rs` |
| `schema/intent-registry-update-v0.schema.json` | The closed Draft 7 schema of a sealed `intent-registry-update` | `vp run schema:authoring:check`, `src/schema.rs` |
| `fixtures/phase3/registry-vectors.json` | One real chain, a genesis and three updates, with the inventories and source texts behind it | `vp run vectors:authoring:check`, `tests/registry_admission.rs` |

See [`fixtures/phase3/README.md`](./fixtures/phase3/README.md) for what the vectors hold and what the independent checker verifies.

## Current status

This is an unpublished, workspace-internal crate. Implemented so far: the registry identity, the representation and structural admission of registry snapshots and updates, applying an update to its base, and replaying a chain from an anchor.

Not yet implemented: continuity verification, newness and absence, reconciliation, and read-only compilation. Local persistence and production publication are a later step again, after checked 015 inputs exist.
