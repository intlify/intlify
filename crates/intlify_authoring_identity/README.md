# intlify_authoring_identity

Persistent Intent identity for [Intlify](../../design/000-intlify-overview-design.md)'s source authoring.

> [!IMPORTANT]
>
> This crate is the start of Phase 3 of [design 016](../../design/016-intlify-source-authoring-and-intent-identity-design.md). So far it holds only the registry identity. See [Current status](#current-status).

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

## Current status

This is an unpublished, workspace-internal crate. Implemented so far: the registry identity.

Not yet implemented: the registry and update representations and their admission, transitions and replay, continuity verification, reconciliation, and read-only compilation. Local persistence and production publication are a later step again, after checked 015 inputs exist.
