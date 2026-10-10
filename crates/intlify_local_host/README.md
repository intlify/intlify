# intlify_local_host

The local host for [Intlify](../../design/000-intlify-overview-design.md)'s persistent Intent identity.

> [!IMPORTANT]
>
> This crate is part of the pure core of Phase 3 of [design 016](../../design/016-intlify-source-authoring-and-intent-identity-design.md). So far it holds [design 018](../../design/018-intlify-security-trust-and-provenance-design.md)'s authority evaluator, and the only authority it establishes is explicitly test-owned. See [Current status](#current-status).

[`intlify_authoring_identity`](../intlify_authoring_identity/README.md) decides whether an identity change holds: whether an update applies to its base, whether its continuations, newness and absence are shown, whether an inventory compiles against a registry. This crate decides who may make the change: which established caller may analyze, read, confirm an explicit choice, initialize or update one application's registry, for which exact inputs, under which authority.

## What this crate never does

- It never decides whether an identity change is valid, and never weakens a check that design 016 or 017 makes. Authorization takes a plan that reconciliation made, so an input with a collision, a failed unit, an unproven continuity or an unresolved choice has nothing to authorize.
- It establishes no authority from data. A principal, a grant, a confirmation and a permit have no serialization and no public constructor; a label, a serialized reason or a copied set of fields is never one of them.
- It publishes nothing and generates no ID yet. Making a permitted change current is the host's conditional commit, which comes next, against in-memory state.

## Local authority

A `LocalAuthority` is design 018's Local Authority Context: one immutable value a trusted host establishes for one bounded invocation.

| Binding | What it holds |
| --- | --- |
| Destination | The host's binding of one application owner's registry chain: the owner, the owning scope, and the chain's registry identity once initialized |
| Analysis context | The exact `AuthoringBasis` every admitted inventory has to be resolved against |
| Acquired sources | The source snapshots the host acquired for this invocation, one per unit |
| Anchors | The registry snapshots the host accepted for this invocation |
| Grants | For each established principal, its set of actions; no repeated entry, no repeated action, no wildcard |
| Development session | Whether an explicitly enabled development session is active for the destination |

A clone is the same authority. Establishing another, even with the same content, gives a different one, and a confirmation or permit issued under one is not valid under the other: a change of grants, session or settings means asking again. In this phase the only way to establish an authority is `test_authority::TestAuthority`, compiled only under the non-default `test-authority` feature, and it admits only a test-owned analysis context.

## Actions

The actions are closed, and none implies another.

| Action | What it permits | What it does not permit |
| --- | --- | --- |
| `analyze-source` | Admitting the scoped source inputs and deriving authoring facts | Publishing anything |
| `read-registry` | Admitting the scoped registry, current or pinned | Treating a pinned snapshot as current, initializing or updating |
| `initialize-registry` | Publishing one empty genesis for an admitted new owner binding | Replacing missing history, or allocating any ID |
| `update-registry` | Publishing one complete checked plan against the exact base | Accepting an explicit choice nobody confirmed, initializing |
| `resolve-identity` | Confirming one exact explicit identity choice | Publishing it |

## Evaluation

| Operation | Needs | Bound to | Gives |
| --- | --- | --- | --- |
| `Invocation::authorize_analysis` | `analyze-source`, and `read-registry` with a registry | The inventory, and the registry when given | Nothing to publish with |
| `Invocation::authorize_read` | `read-registry` | The registry, current or pinned | Nothing to publish with |
| `Invocation::confirm` | `resolve-identity` | The base, the inventory, and an `ExplicitDecision` bound to both | A `Confirmation` |
| `Invocation::authorize_update` | `update-registry` | The base, the inventory, the plan made from them, a confirmation for every explicit decision, and the mode | An `UpdatePermit` |
| `Invocation::authorize_initialization` | `initialize-registry` | An uninitialized destination and an empty genesis of its owner and scope | An `InitializationPermit` |

The checks run in design 018's stages: the caller and its grant, the inputs against the authority, then the exact operation. Inputs are bound by their owner, owning scope and context, by every unit being a snapshot the host acquired (all of them, for a complete inventory), and by the registry being the destination's chain and an accepted anchor. Each refusal is its own `AuthorizationFailure`, so a denied action never reads as a broken input.

A manual update accepts proven decisions as they are and needs a confirmation for each explicit one, and refuses a confirmation that pairs with no decision. A development update needs the authority's session and accepts only proven decisions; a plan with an explicit decision or a restore is manual work, confirmed or not. A permit binds the authority, the publisher, the mode, the base, the inventory, the update, the result to publish and the confirmations used, and `UpdatePermit::check` refuses it for another request or under a changed authority.

## Current status

This is an unpublished, workspace-internal crate. Implemented so far: design 018's authority evaluator against explicitly test-owned authority, covering establishment, least authority, input binding, confirmations, automatic updates, owner semantics, and the binding of a permit to its request.

Not yet implemented: the in-memory host that initializes, prepares updates with operating-system randomness, confirms, publishes under the exact-base and authority checks at the point of commit, and runs development sessions. Local persistence and production publication come after that, once checked 015 inputs and a 029 host adapter exist.
