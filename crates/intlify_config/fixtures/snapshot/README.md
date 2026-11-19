<!-- @license MIT -->

# Snapshot fixtures

Design 015 lets an embedding pass its configuration as a [Programmatic Entry Snapshot](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#optional-programmatic-frontend): a closed, canonical tree that a language binding builds from a host value without running host behavior. The tree has seven variants (null, boolean, number, string, array, object, and rejected-node marker), the framing `S` gives two equal trees the same bytes, and SHA-256 over `intlify.programmatic-entry-snapshot.v0` followed by `S(root)` is the snapshot-content digest, which is also the Programmatic Entry revision. This file maps each of those rules to where it is checked.

The Snapshots here are test values. None of them is a configuration a resolver admits for a project.

## Running them

```sh
# every unit test
cargo test -p intlify_config --all-targets --all-features

# the committed vectors, and the independent Node check of them
vp run vectors:snapshot:check
```

## Data files

| File | Contents | Read by |
| --- | --- | --- |
| `vectors.json` | 39 canonical Snapshots in a JSON notation, each with its `S` bytes, its digest, and its counts under the nine `bootstrapOnly.snapshot` bounds: every scalar and container, numbers up to the largest portable magnitude, keys in unsigned UTF-8 order, a configuration, ordinary markers beside values with a terminal bootstrap marker last, each of the nine ordinary reasons, a bootstrap marker naming each bound, and the largest witness, whose `U64` has every bit set | `src/artifacts/vectors/snapshot.rs` tests; `tools/shared-json-vectors` |

Bytes are written in hexadecimal, and counts and witnesses as decimal strings. A node is an object with one member naming its variant; an object lists its members as `[key, node]` pairs, and a number is the shortest decimal that reads back as its binary64 value. The Node checker shares no code with this crate. It reads the notation as canonical: it refuses a member named twice in one object of the JSON text, a node that is not one variant, an object member that is not one `[key, node]` pair, keys that are not unique and ascending, a null, boolean, or witness of another JSON type, a number that is not the shortest decimal of a portable binary64 value or is negative zero, strings that are not Unicode scalars, markers outside the closed vocabulary, and a node after the `bootstrap-limit-exceeded` marker. It frames, digests, and counts every tree from design 015, and finds the marker reasons and bounds the vectors cover in the trees, not in the vectors' names.

## The matrix

The unit tests are at the end of each source file, named here by their function.

| 015 rule | Checked by |
| --- | --- |
| `S` frames null, false, true, a string, an array, an object, a number, and a marker with the tags `0x01` to `0x08`, a container's count as `U64` first (D15:875-886) | `snapshot.rs`: `the_tree_frames_and_digests_as_015_defines` (bytes computed outside the crate), `each_scalar_frames_with_its_tag`. Node: every vector's bytes |
| Object keys are unique and ordered by ascending unsigned UTF-8 bytes, negative zero is framed as positive zero, and array order is kept (D15:889) | `snapshot.rs`: `object_members_frame_in_unsigned_utf8_key_order`, `each_scalar_frames_with_its_tag`. Node: the `object-key-order` vector and every object's order |
| An ordinary marker's record holds its reason; a `bootstrap-limit-exceeded` marker's also holds the bound's capability path and its witness as `U64` (D15:861, 889) | `snapshot.rs`: `a_marker_record_holds_its_reason_and_any_bound_and_witness`, `each_bound_names_its_capability_path`. Node: the `marker-*` and `bootstrap-*` vectors |
| A `bootstrap-limit-exceeded` marker is the Snapshot's only one and the last node its traversal visits (D15:867, 869) | Node: every tree. Checking the marker against the capability comes with the `programmatic-entry-snapshot` boundary |
| The digest is SHA-256 over the domain separator and `S(root)`, as `sha256:` and 64 lowercase hexadecimal digits (D15:889) | `snapshot.rs`: `the_tree_frames_and_digests_as_015_defines` (digest computed outside the crate). Node: every vector's digest |
| Each `bootstrapOnly.snapshot` bound counts in its own unit, a marker being a node and the traversal visiting every node and edge (D15:1796-1804) | `snapshot.rs`: `the_counts_follow_each_bound_unit`, `a_marker_free_tree_counts_as_the_same_json_value`. Node: every vector's counts |
| The committed vectors are the ones this crate writes | `artifacts/vectors/snapshot.rs`: `the_committed_vectors_are_the_ones_this_crate_writes`, `every_vector_reads_back_as_its_snapshot`, `the_vectors_hold_every_variant_reason_and_bound` |

The `programmatic-entry-snapshot` boundary (`src/boundary/snapshot.rs`) admits a submitted Snapshot into this canonical form or refuses it with one of the five reasons of its row. Its unit tests check each reason, the order of the checks, the terminal marker left out of the bounds, and that the boundary counts a submitted tree in the units above.
