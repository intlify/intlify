<!-- @license MIT -->

# Phase 3 fixtures

Design 016's Phase 3 adds persistent identity on top of the inventories Phases 1 and 2 produce. The fixtures here pin the representation of registry history.

## Data files

| File | Contents | Read by |
| --- | --- | --- |
| `registry-vectors.json` | One registry chain: a genesis and three updates, the three inventories they were planned from, and the source texts those inventories name | `tests/registry_admission.rs`, `tools/shared-json-vectors` |

The chain is one story about two units, `checkout` and `nav`:

| Update | Source change | Decisions |
| --- | --- | --- |
| 1 | Checkout revision 1 has pay and cancel; nav has home | Three allocations with `confirmed-new`. The base is the genesis, so nothing has history |
| 2 | A header goes in above pay, and cancel's line becomes a copy of pay | Pay continues with `verified-edit`, cancel retires with `complete-absence`, the copy gets a new ID, and a `copy` link ties it to pay. Home's unit did not change, so it needs no decision |
| 3 | Cancel's line comes back at the end | Pay and the copy continue with `verified-edit`, and cancel's old ID is restored with an `explicit` basis |

Every snapshot after the genesis is written out by hand from its base and its update, not produced by applying one. That makes it an expectation for replay to be checked against, rather than a record of what the code currently does.

The Intent ID and registry identity values are fixed fixture values. They stand in for what a host draws from operating-system randomness. The edits claim the verifier profile `intlify-continuity-edit-replay` revision `0`, whose rules are the continuity phase's to implement.

## What the independent checker verifies

The Node checker in `tools/shared-json-vectors` shares no code with this crate. For this file it:

- re-derives every artifact's integrity digest, with only the top-level `integrityDigest` removed;
- resolves every reference to exactly one earlier artifact of the kind its member requires, so the chain is built in one direction and nothing names its own result;
- checks that a snapshot's update was planned against that snapshot's base, and that the chain keeps its owner, scope and registry identity;
- checks every source snapshot against the retained text it names, by digest and byte length;
- replays every source edit over the retained bytes and compares the result with the after bytes.

## Regenerating

```sh
cargo run -p intlify_authoring_identity --example generate_registry_vectors -- --write
vp run vectors:authoring:check
```

A vector is regenerated only when the representation or the inventories it is built from change on purpose. Regenerating to make a failing check pass would turn the independent check into a record of whatever this crate currently emits.
