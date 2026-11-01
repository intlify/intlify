<!-- @license MIT -->

# Validator value fixtures

Design 015's [revision `"0"`](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#active-project-profile-specification-and-profile-resolution-specification-input) admits Draft 7 alone, under a validator this crate implements. Draft 7's keywords compare numbers by their mathematical values, compare JSON values for `const`, `enum`, and `uniqueItems`, and name schemas by URI references whose fragments may be JSON Pointers. This file states how revision `"0"` reads each of those, and maps each rule to where it is checked.

## Running them

```sh
# every unit test
cargo test -p intlify_config --all-features --lib construction::validator

# the committed vectors, and the independent Node check of them
vp run vectors:validator-values:check
```

## The readings

- **Numbers.** A JSON number is read from its text as a sign, its significant digits without a leading or trailing zero, and the exponent of its last digit, an integer of any size. So `1`, `1.0`, and `10e-1` are one number, and `1e99999999999999999999` keeps its exponent. Numbers compare by value. A number is an integer when it has no fractional part, as Draft 7's `integer` asks. The count of a length keyword is a non-negative integer, read as 2^64 - 1 past that, which no length reaches.
- **`multipleOf`.** A number is a multiple of a divisor when dividing leaves an integer, exactly: `0.3` is a multiple of `0.1`. A divisor is positive and has at most 19 significant digits. That one capacity keeps every remainder in 64 bits, so the work grows with the number's length alone.
- **Equality.** Two values are equal when they are of one type and equal as that type: numbers by value, strings by code points, arrays item by item, and objects by their members in any order. `uniqueItems` holds when no two items are equal.
- **URI references.** A reference is read by RFC 3986's grammar, its IP literals included, and resolved against its base by section 5.2's strict algorithm. Two references name one resource when their components are equal after section 6.2.2's syntax-based normalization. Normalizing decodes the escapes of unreserved characters, uppercases the other escapes' hexadecimal digits, lowercases the scheme and the host, and removes the dot segments the decoding spells. No scheme is special, so a default port stays, and nothing is retrieved.
- **JSON Pointers.** A fragment that is empty or starts with `/` is a JSON Pointer, and any other fragment is a plain name. A pointer's escapes are decoded as UTF-8 first. The text is then split at each `/`, with `~1` read as `/` and `~0` as `~`. A token names an object's member by its name, or an array's element by an index without a leading zero, so `-`, `01`, and `+1` name nothing in an array.

## Data files

| File | Contents | Read by |
| --- | --- | --- |
| `vectors.json` | 39 texts read as JSON numbers, 21 of them not numbers; 46 numbers with their 2,116 pairwise orders, whether each is an integer, and its count; 18 divisors, six of them refused, with whether each of the 46 numbers is a multiple of each of the 12 accepted; 59 JSON values in 42 equality classes, and 15 arrays tested for `uniqueItems`; 87 texts read as URI references, 49 of them outside the grammar; RFC 3986 section 5.4's 42 examples; 33 references resolved against each of 12 bases; 16 normalized references; and 36 fragments read as pointers into one document, with the values they name | `src/artifacts/vectors/validator_values.rs` tests; `tools/shared-json-vectors` |

An order is `<`, `=`, or `>` for each pair of numbers, the left one varying slowest, and a multiple is `1` or `0` for each pair of a number and an accepted divisor, the number varying slowest. JSON values are written as JSON text, so a number keeps its spelling.

The Node checker shares no code with this crate. It compares numbers as BigInt fractions and decides `multipleOf` by modular exponentiation. It compares JSON values on its own, reading each number's text through `JSON.parse`'s source access. It reads RFC 3986's grammar with its own expressions and Node's `net.isIPv6`, and resolves and normalizes by the RFC's algorithms as the RFC writes them. It decodes fragments with `decodeURIComponent`. It also carries its own transcription of RFC 3986 section 5.4's examples and RFC 6901's, so a reading both get wrong in the same way is still caught where the RFCs speak.

## The matrix

The unit tests are at the end of each module below `src/construction/validator/`, named here by their function.

| Rule | Checked by |
| --- | --- |
| A number reads to its significant digits and the exponent of its last digit, of any size, and a text outside JSON's grammar reads to nothing | `number.rs`: `a_number_reads_to_its_significant_digits_and_the_exponent_of_the_last`, `a_text_that_is_not_a_json_number_reads_to_nothing`. Node: `numbers.parsed` |
| Numbers compare by value | `number.rs`: `numbers_compare_by_their_values`, `an_integer_of_any_size_adds_and_compares`. Node: `numbers.order` |
| An integer has no fractional part, and a count saturates at 2^64 - 1 | `number.rs`: `an_integer_has_no_fractional_part`, `a_count_is_a_non_negative_integer_saturating_at_the_largest_u64`. Node: `numbers.integers`, `numbers.counts` |
| A divisor is positive and has at most 19 significant digits | `number.rs`: `a_divisor_is_positive_with_at_most_nineteen_significant_digits`. Node: `numbers.divisors` |
| `multipleOf` divides exactly, reading at most the number's digits and four zeros per divisor digit | `number.rs`: `a_multiple_leaves_an_integer_when_divided`, `the_cover_holds_every_factor_of_two_and_of_five_a_divisor_has`, `a_remainder_reads_every_digit_of_a_long_number`. Node: `numbers.multiples` |
| Equal values share a key, and unequal values do not | `equal.rs`: `equal_values_share_a_key_and_unequal_values_do_not`, `a_key_spells_its_tag_and_its_lengths`, `a_number_that_is_not_a_json_number_has_no_key`. Node: `equality.classes` |
| `uniqueItems` holds when no two items are equal | `equal.rs`: `unique_items_are_pairwise_unequal`. Node: `equality.uniqueItems` |
| A reference is read by RFC 3986's grammar, IP literals included | `uri.rs`: `a_reference_splits_into_five_components`, `a_text_outside_rfc_3986s_grammar_is_not_a_reference`, `an_ip_literal_is_an_ipv6_address_or_a_future_form`. Node: `references.parsed` |
| A reference resolves by section 5.2's strict algorithm | `uri.rs`: `rfc_3986s_examples_resolve_as_it_lists`, `merging_follows_the_base_path_or_its_authority`, `dot_segments_are_removed_with_the_segment_before_each_double_dot`, `a_path_of_two_slashes_with_no_authority_stays_a_path`. Node: `references.examples`, `references.targets` |
| References compare by their components after section 6.2.2's normalization | `uri.rs`: `normalizing_decodes_unreserved_escapes_and_lowercases_the_scheme_and_the_host`, `components_tell_an_absent_part_from_an_empty_one`. Node: `references.normalized` |
| A fragment is a JSON Pointer when it is empty or starts with `/`, decoded before it is read | `pointer.rs`: `rfc_6901s_pointers_name_its_values`, `a_token_reads_tilde_one_as_a_slash_and_tilde_zero_as_a_tilde`, `a_fragment_decodes_its_escapes_before_it_is_read`. Node: `pointers.fragments` |
| An array index is decimal without a leading zero, and an object's member is named by its name | `pointer.rs`: `an_array_index_is_decimal_without_a_leading_zero`. Node: `pointers.fragments` |
| The committed vectors are the ones this crate writes | `src/artifacts/vectors/validator_values.rs`: `the_committed_vectors_are_the_ones_this_crate_writes`, `the_numbers_reach_every_reading`, `the_equality_classes_hold_equal_spellings_together`, `the_references_reach_every_reading`, `the_fragments_reach_every_reading`, `an_input_that_does_not_read_names_itself` |
