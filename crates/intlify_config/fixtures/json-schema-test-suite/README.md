<!-- @license MIT -->

# Draft 7 validation fixtures

Design 015's [revision `"0"`](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#active-project-profile-specification-and-profile-resolution-specification-input) validates instances against schema artifacts under Draft 7, and counts each validation's work in `(schema node, instance node)` pairs, which it bounds beforehand by the product of the two node counts. This directory holds the required Draft 7 tests of the [JSON Schema Test Suite](https://github.com/json-schema-org/JSON-Schema-Test-Suite), which the validator passes. This file states how revision `"0"` validates an instance against a [read schema](../validator-schemas/README.md), and maps each rule to where it is checked.

## Running them

```sh
# the unit tests, the suite, and the comparison with the oracle
cargo test -p intlify_config --lib construction::validator
```

## The validation

- **Visits.** A validation visits each instance node once. The schema nodes applied to a node are its seeds, with every node they apply to the same instance: the node a `$ref` names, the members of `allOf`, `anyOf`, and `oneOf`, `not`, `if` with its `then` and `else`, and the schema of each dependency whose name the object has. The root's seed is the schema's root. Any other node's seeds are the nodes that the nodes applied to its parent name for it through `properties`, `patternProperties`, `additionalProperties`, `items`, `additionalItems`, and `contains`. A node no schema node applies to is not visited.
- **Order.** The children are visited first, and then the applied nodes are decided in the schema's order, each after the nodes it applies in place. Every branch is decided whatever the others decide, `then` and `else` alike, so that what the nodes ask of the children is known before the children are visited.
- **Keywords.** Numbers are compared as exact decimals, and an integer is a number without a fraction, however it is written. A string's length is its code points. A `pattern` is matched anywhere in the string. Two values are equal when they are of one type and equal as that type, objects whatever the order of their members. `format`, `contentMediaType`, and `contentEncoding` are annotations and are not checked.
- **Result.** A validation answers only whether the instance is valid: a failed construction carries no details but the bound it exceeded (D15:3086).

## The pairs

- **Pairs.** A pair is a schema node and an instance node that it is applied to. `propertyNames` applies its node to the name of each member, which is a string but not a node (design 015's `json-node`), so a node applied to a name is counted with the member's value. The pairs of a validation are, for each instance node, the nodes applied to its value or to its name, each once.
- **Bound.** Both sets are of the schema's nodes, so the pairs are at most the schema's nodes times the instance's nodes, the product design 015 counts before the validation, and each pair decides its node at most twice. Counted apart, the names could pass the product: in `{"a": {"a": {"a": {}}}}` against a schema whose `propertyNames` and `additionalProperties` both refer to the root, the values meet seven pairs and the names six, more than the product of three nodes and four.
- **Names.** A name meets the nodes applied to it in place, but no dependency's schema, since a name has no members.

## Data files

| Path | Contents | Read by |
| --- | --- | --- |
| `tests/draft7/*.json` | The suite's 37 required Draft 7 files, with 258 groups of a schema and its tests, 925 tests in all | `src/construction/validator/suite_tests.rs`, `src/construction/validator/differential_tests.rs` |
| `LICENSE` | The suite's MIT license |  |

The files are unchanged from commit [`8cab295687d150de3900a160c6ad002b53ddb30e`](https://github.com/json-schema-org/JSON-Schema-Test-Suite/tree/8cab295687d150de3900a160c6ad002b53ddb30e/tests/draft7) of the suite, under the MIT license in [`LICENSE`](LICENSE), copied from the same commit. The `optional` directory is not included. The files are kept out of formatting and of spell checking, which would change or flag the suite's data.

Thirteen groups need a document other than their own schema, which revision `"0"` never retrieves, so the validator refuses to read them, as `Reference`: the 11 groups of `refRemote.json`, and the two that refer to the meta-schema by its URI, `definitions.json`'s "validate definition against metaschema" and `ref.json`'s "remote ref, containing refs itself". The other 245 groups are read, and every one of their 898 tests has the result the suite expects.

## The oracle

The validator is also compared with the `jsonschema` crate, version 0.56, as Draft 7 with formats unchecked. The inputs keep to what the two read alike: no number a float cannot hold exactly, no character on which `\s`, `.`, or `\b` part them, no `format` or content keyword, no `$id` beside a `$ref`, no other document, and no cycle of in-place applicators, on which the oracle would not stop. Nor are two objects compared whose members were written in different orders: under this crate's `preserve_order` feature of `serde_json`, the oracle compares objects member by member in order, and fails the suite's three tests that ask otherwise.

The inputs are:

- the Draft 7 meta-schema against 316 documents, the suite's 258 schemas, this crate's schema artifacts, and the validator schema vectors, and against the suite's schemas with each of 36 keywords and values set into each of their schema objects;
- the configuration schema against the minimal and complete configurations, the construction admission schema against its body, and the four authority envelope schemas against an envelope each, all with the values near them: each node replaced by each of eight values, each member and item removed, an unknown member added to each object, and the first item of each array repeated;
- 2,000 schemas made by a seeded generator, each against eight generated instances.

## The matrix

The unit tests are at the end of `src/construction/validator/validation.rs`, named here by their function.

| Rule | Checked by |
| --- | --- |
| An instance's nodes are its values, and not its names | `instance_nodes_are_values_and_not_names` |
| Each type holds of its values, and an integer is a number without a fraction | `each_type_holds_of_its_values`. Suite: `type.json` |
| `const` and `enum` compare values exactly | `const_and_enum_compare_values_exactly`. Suite: `const.json`, `enum.json` |
| The numeric keywords read numbers exactly | `the_numeric_keywords_read_numbers_exactly`. Suite: `multipleOf.json` and the four bounds |
| The string keywords count code points, and a pattern is not anchored | `the_string_keywords_count_code_points`. Suite: `maxLength.json`, `minLength.json`, `pattern.json` |
| The array keywords apply to items, and `additionalItems` only past an array of `items` | `the_array_keywords_apply_to_items`. Suite: the array keywords' files |
| The object keywords apply to members, a dependency only beside its name, and `propertyNames` to each name | `the_object_keywords_apply_to_members`. Suite: the object keywords' files |
| The applicators combine the results of their nodes | `the_applicators_combine_their_nodes`, `a_reference_applies_its_node_and_nothing_beside_it`. Suite: `allOf.json`, `anyOf.json`, `oneOf.json`, `not.json`, `if-then-else.json`, `ref.json` |
| A dependency's schema applies in place only beside its name | `src/construction/validator/schema.rs`: `every_node_comes_after_the_nodes_it_applies_in_place` |
| Each applied node meets an instance node once | `each_applied_node_meets_an_instance_node_once` |
| Every branch is decided whatever the others decide | `every_branch_is_decided_whatever_the_others_decide` |
| A name is counted with its member's value, within the product | `a_name_is_counted_with_its_members_value` |
| A name meets no dependency's schema, since it has no members | `no_dependency_applies_to_a_name` |
| A node no schema node applies to is not visited | `a_child_no_node_applies_to_is_not_visited` |
| The suite's groups are read but those that need another document, and every test has its expected result, with the pairs of their definition within the product | `src/construction/validator/suite_tests.rs`: `every_group_is_read_but_those_that_need_another_document`, `every_test_has_the_result_the_suite_expects` |
| The validator judges as the oracle does, with the pairs of their definition within the product | `src/construction/validator/differential_tests.rs`: `the_oracle_passes_the_suite_but_where_it_compares_objects_in_order`, `the_meta_schema_judges_schemas_as_the_oracle_does`, `the_configuration_and_envelope_schemas_judge_as_the_oracle_does`, `random_schemas_judge_random_instances_as_the_oracle_does` |
