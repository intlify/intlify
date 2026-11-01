<!-- @license MIT -->

# Validator schema fixtures

Design 015's [revision `"0"`](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#active-project-profile-specification-and-profile-resolution-specification-input) validates schema artifacts under Draft 7 and counts its work in schema nodes and `$ref` occurrences. This file states how revision `"0"` reads a schema document before it validates anything with it, and maps each rule to where it is checked.

## Running them

```sh
# every unit test
cargo test -p intlify_config --all-features --lib construction::validator::schema

# the committed vectors, and the independent Node check of them
vp run vectors:validator-schemas:check
```

## The reading

- **Nodes.** A schema node is the root, or an object or a boolean in one of Draft 7's subschema positions: `additionalItems`, `additionalProperties`, `contains`, `else`, `if`, `not`, `propertyNames`, `then`; each member of `allOf`, `anyOf`, and `oneOf`; each value of `definitions`, `patternProperties`, and `properties`; `items` or each of its members; and each object or boolean value of `dependencies`. Nodes are numbered in document order.
- **Census.** Construction counts the nodes and the `$ref` occurrences of any JSON before any other check, so the census walks the same positions without reading anything else. A `$ref` occurrence is a schema object with a `$ref` member; a property named `$ref`, as the meta-schema has, is not one. The positions beside a `$ref` are counted too.
- **References.** An object with `$ref` is that reference alone: its other members, `$id` and `$schema` among them, are ignored (core 8.3). Every other node's `$id` is read against its base. An `$id` that starts with `#` is a plain-name anchor, `[A-Za-z][A-Za-z0-9-_:.]*`, within its resource. Any other `$id` is resolved, normalized, and stripped of an empty fragment to name a resource; a fragment that is not empty is refused. A root that names no resource is named by `json-schema:///`. Every `$ref` is resolved to a node of the document through a resource and its fragment: empty, a JSON Pointer, or an anchor. Nothing is retrieved, not even the meta-schema.
- **Dialect.** The root's `$schema`, when present, is exactly `http://json-schema.org/draft-07/schema#`, and a subschema has none (core 7).
- **Cycles.** A `$ref`, the members of `allOf`, `anyOf`, and `oneOf`, `not`, `if` with its `then` and `else`, and the schemas of `dependencies` apply nodes to the same instance. A cycle of them is refused whatever instance it would meet. `then` and `else` without `if` apply nothing.
- **Keywords.** Each keyword is read into its value, and one of the wrong shape is refused: a `pattern` or a name of `patternProperties` must compile, and a `multipleOf` must be a divisor of at most 19 significant digits. Annotations and unknown keywords are not read.
- **Order.** A document is checked for its dialect and identifiers, then its nodes (a root that is not a schema, and the keywords), its references, and its cycles, each over the nodes in document order, and the first failure is reported.

## Data files

| File | Contents | Read by |
| --- | --- | --- |
| `vectors.json` | 38 documents, each as JSON text with its census: 11 read, among them Draft 7's meta-schema and the identification example of Draft 7 core 8.2.4, with their 178 nodes, 16 resources, four anchors, and 77 resolved references; and 27 refused, each with one problem: two that are not schemas, two dialects, three identifiers, two duplicate identifiers, four keywords, one pattern, two divisors, seven references, and four cycles | `src/artifacts/vectors/validator_schemas.rs` tests; `tools/shared-json-vectors` |

Nodes, resources, anchors, and the two ends of each reference are named by their JSON Pointers within the document. No member name looks like an array index, since JavaScript objects do not keep the order of such names.

The Node checker shares no code with this crate. It walks the subschema positions itself, resolves references with the RFC 3986 and RFC 6901 module the checkers share, reads each document's numbers through `JSON.parse`'s source access, decides a pattern with JavaScript's `RegExp`, and finds cycles on its own. It gathers every problem of a document rather than the first, and requires each refused document to have exactly the one problem listed.

## The matrix

The unit tests are at the end of `src/construction/validator/schema.rs`, named here by their function.

| Rule | Checked by |
| --- | --- |
| The census counts every schema position of any JSON, and a property named `$ref` is not a reference | `the_census_counts_every_schema_position_of_any_json`. Node: every document's census |
| Nodes are numbered in document order | `nodes_take_their_places_in_document_order`. Node: every read document's nodes |
| The meta-schema reads into 70 nodes and 29 references | `the_meta_schema_reads_into_its_seventy_nodes`. Node: `meta-schema` |
| Identifiers name resources and anchors as Draft 7 core 8.2.4 lists them | `the_draft_7_identification_example_names_each_subschema`. Node: `identification-example` |
| A reference resolves within its document, through pointers, anchors, normalization, and nested resources | `a_reference_resolves_within_its_document`. Node: `references`, `default-base` |
| An object with `$ref` ignores its other members | `an_object_with_a_reference_ignores_its_other_keywords`. Node: `reference-siblings` |
| An identifier names one resource or one anchor, by its grammar | `identifiers_name_resources_and_anchors_once`, `a_plain_name_is_a_letter_then_letters_digits_and_four_marks`. Node: every document refused as `identifier` or `duplicateIdentifier` |
| Only the root names its dialect, and only Draft 7 | `only_the_root_names_its_dialect_and_only_draft_7`. Node: every document refused as `dialect` |
| A root that is neither an object nor a boolean is not a schema | `a_root_that_is_neither_an_object_nor_a_boolean_is_not_a_schema`. Node: `not-an-object`, `an-array` |
| A keyword of the wrong shape, a pattern that does not compile, and a divisor out of bounds are refused | `a_keyword_of_the_wrong_shape_is_refused`, `a_pattern_or_a_divisor_that_does_not_compile_is_refused`, `keywords_read_into_their_values`. Node: every document refused as `keyword`, `pattern`, or `divisor` |
| A reference that names no node is refused | `a_reference_that_names_no_node_is_refused`. Node: every document refused as `reference` |
| A cycle of in-place applicators is refused, and every other applicator ends | `nodes_that_apply_one_another_in_place_in_a_cycle_are_refused`, `every_node_comes_after_the_nodes_it_applies_in_place`. Node: `in-place`, `moving-applicators`, every document refused as `cycle` |
| The checks run in order, and the first failure is reported | `a_document_fails_at_the_first_check_in_order` |
| The committed vectors are the ones this crate writes | `src/artifacts/vectors/validator_schemas.rs`: `the_committed_vectors_are_the_ones_this_crate_writes`, `the_documents_reach_every_reading_and_every_failure`, `every_failure_has_its_name` |
