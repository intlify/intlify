<!-- @license MIT -->

# Bound fixtures

Design 015 gives every capability and `projectProfileResolution` bound one value domain, the [ResourceBoundValue](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#terminology): a count from 1 through 18446744073709551614 whose semantic form is eight bytes, big-endian, and whose JSON form is the count's shortest decimal string. The [Project Profile Resolver Capability Specification](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#resolver-capability-authority-and-invocation-value) closes two namespaces of bounds: `policyComparable`, the ordered mirror of the Resource Limit Policy's [`projectProfileResolution` tables](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#project-profile-resolution-resource-limits), and `bootstrapOnly`, with a mandatory minimum for every bound. This file maps each of those rules to where it is checked, so a reader can go from a requirement to the code that pins it.

The values here are test values, and the minima are revision `"0"`'s mandatory minima, not product defaults. No capability, Resource Limit Policy, or Capability Specification artifact is written here.

## Running them

```sh
# every unit test
cargo test -p intlify_config --all-targets --all-features

# the committed vectors, and the independent Node check of them
vp run vectors:bounds:check
```

## Data files

| File | Contents | Read by |
| --- | --- | --- |
| `vectors.json` | 14 decimal strings with the eight bytes of their semantic form, from `"1"` through `"18446744073709551614"`; 20 strings and two byte forms that are no bound, each with the reason the capability boundary gives; and the 106 capability bound paths, 52 `policyComparable` and then 54 `bootstrapOnly`, each with its minimum | `src/artifacts/vectors/bounds.rs` tests; `tools/shared-json-vectors` |

Bytes are written in hexadecimal and every count as a decimal string. The Node checker shares no code with this crate: it reads every string with `BigInt`, encodes every value itself, and lists the capability bounds from its own copy of design 015's tables. It does not read the design document, so a design change can land before the code follows.

## The matrix

The unit tests are at the end of each source file, named here by their function.

| 015 rule | Checked by |
| --- | --- |
| A bound is an unsigned integer from 1 through 18446744073709551614 | `input_limits.rs`: `a_bound_is_a_count_from_one_through_one_below_u64_max`. Node: every value |
| Its semantic form is the count's eight bytes, big-endian | `input_limits.rs`: `the_semantic_form_is_the_counts_eight_bytes_big_endian`. `artifacts/vectors/bounds.rs`: `every_value_reads_back_with_its_semantic_form`. Node: every value and refused byte form |
| JSON spells a bound as the shortest decimal string from `"1"` through `"18446744073709551614"` | `input_limits.rs`: `a_decimal_string_is_judged_by_its_spelling_then_zero_then_its_size`, `parse_keeps_the_bound_and_drops_the_failure`. Node: every value and refused string |
| The capability boundary refuses a non-digit character or a leading zero before another digit as `non-canonical-bound`, then `"0"` as `non-positive-bound`, then a canonical spelling of 18446744073709551615 or more as `unrepresentable-bound`. The empty string, which 015 does not list, spells no count and is `non-canonical-bound` too | `input_limits.rs`: `a_decimal_string_is_judged_by_its_spelling_then_zero_then_its_size`. `artifacts/vectors/bounds.rs`: `every_refused_vector_fails_where_the_independent_implementation_says`, `every_failure_appears_among_the_refused_vectors`. Node: every refused string |
| `policyComparable` is the exact ordered mirror of the `projectProfileResolution` bounds, each with the minimum one | `resolver/capability.rs`: `policy_comparable_mirrors_the_policy_section_with_the_minimum_one`. Node: the first 52 capability bounds |
| `bootstrapOnly` has exactly the table's groups and bounds in its order, with the mandatory minimum vector | `resolver/capability.rs`: `the_groups_are_015s_in_canonical_order`, `bootstrap_only_has_015s_bounds_in_canonical_order`, `the_minimum_is_one_except_for_three_byte_bounds`, `bootstrap_only_follows_with_its_minimum_vector`. Node: the last 54 capability bounds |
| Each `bootstrapOnly` bound fills and reads its own field | `resolver/capability.rs`: `every_bound_reads_its_own_field`, `from_bounds_fills_each_field_from_its_own_name` |
| A capability bound path, such as `bootstrapOnly.snapshot.maxNodes`, is the namespace, the group's path, and the bound, joined by dots | `resolver/capability.rs`: `a_capability_bound_path_joins_the_namespace_the_group_and_the_name`. Node: every path |
| The committed vectors are the ones this crate writes | `artifacts/vectors/bounds.rs`: `the_committed_vectors_are_the_ones_this_crate_writes`, `the_capability_bounds_are_the_inventory_in_its_order` |

The two Finding codes that report exceeded bounds outside the Resource Limit Policy, `project-profile-bootstrap-limit-exceeded` and `project-profile-reporting-limit-exceeded`, and the latter's reasons `entry` and `shared`, are spelled in `resolver/evaluation.rs` and checked by `codes_and_reasons_are_spelled_as_015_registers_them`. The capability boundary that admits a submitted capability against this inventory, the Capability Specification artifact that lists it, and the checks that report those codes come with their own stages. So does the machine form of each bound's counting unit, which design 015 describes in prose only.
