<!-- @license MIT -->

# Phase 3 fixtures

Design 016's Phase 3 adds persistent identity on top of the inventories Phases 1 and 2 produce. The fixtures here pin the representation of registry history, and the Intent and reference artifacts compiled against it.

## Data files

| File | Contents | Read by |
| --- | --- | --- |
| `registry-vectors.json` | One registry chain: a genesis and three updates, the three inventories they were planned from, and the source texts those inventories name | `tests/registry_admission.rs`, `tests/registry_transitions.rs`, `tools/shared-json-vectors` |
| `compile-vectors.json` | Design 028's representative module carried from source to compiled artifacts: two revisions read by the JS Producer, a genesis, the update that allocates its declarations and the registry it produces, and the Intents and references compiled for each revision | `tests/compilation.rs`, `tools/shared-json-vectors` |

## The registry chain

The chain is one story about two units, `checkout` and `nav`:

| Update | Source change | Decisions |
| --- | --- | --- |
| 1 | Checkout revision 1 has pay and cancel; nav has home | Three allocations with `confirmed-new`. The base is the genesis, so nothing has history |
| 2 | A header goes in above pay, and cancel's line becomes a copy of pay | Pay continues with `verified-edit`, cancel retires with `complete-absence`, the copy gets a new ID, and a `copy` link ties it to pay. Home's unit did not change, so it needs no decision |
| 3 | Cancel's line comes back at the end | Pay and the copy continue with `verified-edit`, and cancel's old ID is restored with an `explicit` basis |

Every snapshot after the genesis is written out by hand from its base and its update, not produced by applying one. That makes it an expectation for replay to be checked against, rather than a record of what the code currently does: `tests/registry_transitions.rs` applies each update to its base and requires exactly the hand-written snapshot, and verifies the whole chain from its genesis.

The Intent ID and registry identity values are fixed fixture values. They stand in for what a host draws from operating-system randomness. The edits claim the verifier profile `intlify-continuity-edit-replay` revision `0`, whose rules are the continuity phase's to implement.

## The compiled module

`compile-vectors.json` follows [design 028's representative module](../../../intlify_authoring_js/fixtures/phase2/representative-application.js) through the first half of plan 021's acceptance examples. The JS Producer reads it as unit `app`, with three declarations (the greeting `mf2` tag, the `'Save'` text and the `'Welcome'` literal) and four use sites, two of which use the greeting.

| Step | What the vectors hold |
| --- | --- |
| Allocate | The genesis has no history, so all three declarations are new: one update with three allocations, and the registry it produces |
| Compile revision 1 | Each declaration is held exactly by its entry, so its Intent carries no continuity. The two greeting use sites have the same target |
| Compile revision 2 | `'Welcome'` becomes `'Welcome back'` inside its quotes. The registry is unchanged; every Intent continues from its revision 1 declaration across that one edit, and only the welcome's revision changes |

The update, the registry and every Intent and reference are written out by hand from the inventories' declarations and the fixed IDs, not produced by reconciling or compiling. `tests/compilation.rs` requires reconciliation to plan exactly that update and registry from the genesis, and compilation to give exactly those Intents and references.

## What the independent checker verifies

The Node checker in `tools/shared-json-vectors` shares no code with this crate. For the registry chain, and for the registry steps of the compiled module, it:

- re-derives every artifact's integrity digest, with only the top-level `integrityDigest` removed;
- resolves every reference to exactly one earlier artifact of the kind its member requires, so the chain is built in one direction and nothing names its own result;
- checks that a snapshot's update was planned against that snapshot's base, and that the chain keeps its owner, scope and registry identity;
- checks every source snapshot against the retained text it names, by digest and byte length;
- replays every source edit over the retained bytes and compares the result with the after bytes.

For the compiled module's Intents and references it also:

- recomputes each Intent's revision from its declaration's projection, and requires one Intent per declaration of each compilation;
- requires each Intent's ID to be active in its registry, held at its declaration, or, with a continuity, held at where the continuity starts;
- requires a continuity to be one edit under the edit-replay profile, from that snapshot to the declaration's, carrying the old range exactly onto the declaration with no other active entry held or carried there;
- requires each reference's targets in Intent ID order, each the ID and revision of the Intent it names, and those Intents' declarations to be exactly the ones the use site may use.

## Regenerating

```sh
cargo run -p intlify_authoring_identity --example generate_registry_vectors -- --write
cargo run -p intlify_authoring_identity --example generate_compile_vectors -- --write
vp run vectors:authoring:check
```

A vector is regenerated only when the representation or the inventories it is built from change on purpose. Regenerating to make a failing check pass would turn the independent check into a record of whatever this crate currently emits.
