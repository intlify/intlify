# intlify_local_host

The local host for [Intlify](../../design/000-intlify-overview-design.md)'s persistent Intent identity.

> [!IMPORTANT]
>
> This crate is part of the pure core of Phase 3 of [design 016](../../design/016-intlify-source-authoring-and-intent-identity-design.md). It holds [design 018](../../design/018-intlify-security-trust-and-provenance-design.md)'s authority evaluator and [design 029](../../design/029-intlify-product-workflow-and-packaging-design.md)'s registry operations against an in-memory host. The only authority it establishes is explicitly test-owned, and nothing it publishes is written anywhere. See [Current status](#current-status).

[`intlify_authoring_identity`](../intlify_authoring_identity/README.md) decides whether an identity change holds: whether an update applies to its base, whether its continuations, newness and absence are shown, whether an inventory compiles against a registry. This crate decides who may make the change: which established caller may analyze, read, confirm an explicit choice, initialize or update one application's registry, for which exact inputs, under which authority.

## What this crate never does

- It never decides whether an identity change is valid, and never weakens a check that design 016 or 017 makes. Authorization takes a plan that reconciliation made, so an input with a collision, a failed unit, an unproven continuity or an unresolved choice has nothing to authorize.
- It establishes no authority from data. A principal, a grant, a confirmation and a permit have no serialization and no public constructor; a label, a serialized reason or a copied set of fields is never one of them.
- It writes nothing durable. The in-memory host makes a permitted change current in its own memory only, and a publication there is not evidence that anything was written to disk or would survive a restart.
- It draws no identity from anything but the host's randomness: never a timestamp, a counter or a digest of the source.

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

## In-memory host

`host::MemoryHost` runs design 029's registry operations against one owner binding held in memory. It is test-owned like the authority it establishes: it is compiled only under the `test-authority` feature, so an ordinary build can neither open it nor draw a fresh identity.

| Binding state | What it allows |
| --- | --- |
| Not enrolled | Only explicit enrollment |
| Uninitialized | Opening sessions, and one explicit initialization |
| Active | Reads, analysis and updates of the one registry chain |
| Unavailable | Nothing: the control state cannot be validated, and it is never read as a new owner or an uninitialized binding |

`MemoryHost::open` opens a session for one caller. It establishes one authority from the host's grants, its development session and its current registry, together with the caller's acquisition, and pins the registry current at that moment, along with any earlier snapshot the acquisition asks to pin for reading.

| Operation | What it does | What it gives |
| --- | --- | --- |
| `Session::read` | Reads the current registry or a pinned earlier one | The registry, marked current or historical |
| `Session::analyze` | Compiles an inventory against the current registry | Intents and references, or what is unresolved |
| `Session::prepare` | Reconciles, drawing exactly the fresh Intent IDs the new declarations need | A prepared update, an unchanged result, or what is unresolved |
| `Session::confirm` | Confirms one explicit decision under the session's authority | A `Confirmation` |
| `Session::publish` | Makes a prepared update's result current, conditionally | A `Publication`, or a conflict, denial or failure |
| `Session::initialize` | Draws a registry identity and makes an empty genesis current, conditionally | A `Publication`, or a conflict, denial or failure |

A write runs under one guard, in design 029's order. The binding has to be usable, the prepared base still current, and the authority state unchanged since the session opened. Then the request is authorized, and its result sealed, read back under the host's limits and made current. A conflict changes nothing. Changing the grants, or starting or ending a development session, refuses every write of a session opened before: a new session has to be opened, and its confirmations given again. An unchanged result holds only while its base is still current.

Fresh values come only from the `Randomness` the host was set up with, `OsRandomness` unless a test supplies its own, and they are drawn only once reconciliation shows how many new declarations need one. A failure to draw is an allocation failure; nothing stands in for the value. Every publication keeps its provenance, a `PublicationRecord`, and says where it lives: `Durability::InMemory`.

## Current status

This is an unpublished, workspace-internal crate. Implemented so far: design 018's authority evaluator against explicitly test-owned authority, and design 029's registry operations against an in-memory, test-owned host: enrollment, initialization, reads, analysis, preparation with fresh IDs from operating-system randomness, confirmation, conditional publication and development sessions.

Not yet implemented: local persistence (generations on disk, pending transactions, outcome resolution, locks and fault injection) and production publication, which come with a later plan once checked 015 inputs and a 029 host adapter exist. A lineage link names Intent IDs as given, so a link to an ID the same preparation draws cannot be supplied before the draw.
