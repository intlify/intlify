<!-- @license MIT -->

# Framing fixtures

Design 015 frames its typed records in one way: its [Resolver Construction Identity](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#active-project-profile-specification-and-profile-resolution-specification-input), the Root Package's canonical digest, both [Redacted tokens](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#finding-evidence-and-accounting-encoding), the content-derived source revisions, and the Programmatic Entry Snapshot's marker records. It presents a Resolver Construction Identity as `rci0_` and the unpadded base64url of its digest. This file maps each of those rules to where it is checked, so a reader can go from a requirement to the code that pins it.

The records, collections, and texts here are test values. None of them is a construction identity, token, or revision that a resolver produces.

## Running them

```sh
# every unit test
cargo test -p intlify_config --all-targets --all-features

# the committed vectors, and the independent Node check of them
vp run vectors:framing:check
```

## Data files

| File | Contents | Read by |
| --- | --- | --- |
| `vectors.json` | Five records and three collections with their bytes; 14 records and six collections that the strict readers refuse, each with the failure it reports; 11 byte strings with their unpadded base64url, among them RFC 4648's vectors and all 256 bytes; 10 texts the canonical decoder refuses; three digests with their `rci0_` text; and seven texts that are no identity's | `src/artifacts/vectors/framing.rs` tests; `tools/shared-json-vectors` |

Bytes are written in hexadecimal and a record's field count as a decimal string. The Node checker shares no code with this crate: it frames every record and collection from design 015, decides each refusal with its own reader, and encodes base64url with Node's codec, re-encoding what that codec decodes to find text whose unused bits are set.

## The matrix

The unit tests are at the end of each source file, named here by their function.

| 015 rule | Checked by |
| --- | --- |
| A field frame is `tag:u8 \|\| payload_len:u64be \|\| payload` | `framing.rs`: `a_frame_is_its_tag_then_a_big_endian_length_then_the_payload`. Node: every record's bytes |
| A record's fields carry the tags `0x01`, `0x02`, and so on in order, and a record is not itself framed; a nested record is a field's payload | `framing.rs`: `a_record_numbers_its_fields_from_one_and_is_not_framed_itself`, `a_nested_record_is_the_payload_of_one_field`, `a_256th_field_is_a_defect`, `a_record_reads_back_as_its_payloads_in_tag_order`. Node: the `nested-record` record |
| A collection is `count:u64be` followed by `item_len:u64be \|\| item` for each item | `framing.rs`: `a_collection_is_its_count_then_each_item_with_its_length`, `a_collection_reads_back_as_its_items`. Node: every collection's bytes |
| Tag `0x00` is invalid, and no unknown, repeated, omitted, or reordered tag is admitted | `framing.rs`: `each_tag_defect_has_its_own_failure`, `the_first_frame_that_breaks_the_order_decides`, `frames_keep_every_tag_and_payload_without_judging_them`. Node: every refused record |
| Bytes that end inside a frame, and a collection with another number of items than its count, are refused | `framing.rs`: `bytes_that_end_inside_a_frame_are_truncated`, `a_collection_with_another_number_of_items_than_its_count_fails`, `a_collection_that_ends_inside_its_count_or_an_item_is_truncated`. Node: the refused records and collections |
| The identity's text is `rci0_` followed by unpadded base64url, exactly 43 characters | `base64url.rs`: `the_rfc_4648_vectors_encode_without_padding`, `the_rfc_4648_vectors_decode_back`, `the_last_two_values_are_dash_and_underscore`, `every_byte_survives_the_round_trip`, `every_character_of_the_alphabet_has_its_value`, `an_identity_is_its_prefix_and_43_characters`. Node: every base64url text and identity |
| A text with a non-zero unused bit is not canonical and is refused as `malformed-construction-identity` | `base64url.rs`: `padding_and_characters_outside_the_alphabet_are_refused`, `a_length_one_past_a_multiple_of_four_is_refused`, `a_last_character_with_an_unused_bit_set_is_refused`, `an_identity_text_is_refused_for_its_first_defect`. Node: every refused text and identity |
| The committed vectors are the ones this crate writes, and every refusal has its expected failure | `artifacts/vectors/framing.rs`: `the_committed_vectors_are_the_ones_this_crate_writes`, `every_record_and_collection_reads_back_as_its_parts`, `every_refused_vector_fails_where_the_independent_implementation_says`, `every_failure_appears_among_the_refused_vectors`, `the_identity_vectors_parse_back_to_their_digests` |

The records that use this framing, their domain separators, and their digests come with the code that builds them: the Root Package and Resolver Construction Identity with construction, the tokens with the Finding Keys, and the revisions with the identity/revision boundaries. Mapping each reader failure to a boundary reason, such as `malformed-construction-identity`, belongs to that boundary.
