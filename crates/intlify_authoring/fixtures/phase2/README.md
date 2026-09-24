<!-- @license MIT -->

# Phase 2 fixtures

Design 016's Phase 2 adds a host Producer on top of the language-neutral semantics in this crate. The fixtures here pin the representation that Producer fills. The complete matrix from each 016 fixture family to the test that checks it is written when the phase closes.

## Data files

| File | Contents | Read by |
| --- | --- | --- |
| `inventory-vectors.json` | Three sealed `authoring-inventory` artifacts | `tests/inventory_admission.rs`, `tools/shared-json-vectors` |

Each vector is a complete artifact rather than a preimage. The independent Node checker removes the top-level `integrityDigest` itself and re-hashes what remains, so it checks the exclusion rule as well as the framing and the hash.

Each vector also carries the revision this crate computes for each of its declarations. The checker recomputes every revision itself and compares it with those, which is what ties the two implementations together on revisions as well as on integrity.

Two vectors declare that their declarations share revisions with the first. With the revisions already tied to this crate's, the checker then confirms both halves of that claim: the artifacts differ, because source positions are part of what an inventory records, and the revisions do not, because positions are not part of what a message means. Comparing the checker's own answers across vectors without the first step would prove nothing, since equal projections agree under any deterministic function, including a wrong one.

## Regenerating

```sh
cargo run -p intlify_authoring --features test-context --example generate_inventory_vectors -- --write
vp run vectors:authoring:check
```

A vector is regenerated only when the representation changes on purpose. Regenerating to make a failing check pass would turn the independent check into a record of whatever this crate currently emits.
