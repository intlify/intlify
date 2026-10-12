<!-- @license MIT -->

# Locale canonicalization data fixtures

Design 017's [Locale canonicalization data admission and fixtures](../../../design/017-intlify-shared-artifact-and-version-admission-design.md#locale-canonicalization-data-admission-and-fixtures) names six areas of independent expectations. This file maps each one to where it is checked, so a reader can go from a requirement to the code that pins it.

Every value here is a test value. The marker payloads are ICU4X's data cut down to two entries per map and list, and the conformance data is modelled on CLDR's records. None of it is the dataset revision `"0"` admits, which PR 4 of plan 022 generates from CLDR 48.2.0.

## Running them

```sh
# every unit test, including the ones that build blobs
cargo test -p intlify_locale_icu4x --all-targets --all-features

# the committed schemas, vectors, and fixture artifacts, and the independent Node checks
vp run schema:locale:check
vp run vectors:locale:check
```

## Data files

| File | Contents | Read by |
| --- | --- | --- |
| `dataset/vectors.json` | A small dataset, its conformance payload as canonical JSON text, its semantic digest, 5 edits that must change it, and 3 payloads that must be refused | `src/conformance.rs`, `src/dataset.rs`, `src/generate.rs`, `src/reader.rs` tests; `tools/shared-json-vectors` |
| `artifact/conformance.json` | The dataset's conformance payload, exact bytes | `src/reader.rs` tests; `tools/shared-json-vectors` |
| `artifact/markers-narrow.postcard`, `artifact/markers-wide.postcard` | The dataset's marker payloads as a blob in each index layout | `src/generate.rs`, `src/payload.rs`, `src/reader.rs` tests; `tools/shared-json-vectors` |
| `artifact/manifest-narrow.json`, `artifact/manifest-wide.json`, `artifact/manifest-vendored.json` | Sealed manifests naming the narrow blob, the wide blob, and the narrow blob with CLDR sources read from another place | `src/manifest.rs`, `src/reader.rs`, `src/schema.rs` tests; `tools/shared-json-vectors` |

The artifact files are exact bytes, since their digests are in the manifests, so the repository formatter leaves them alone. The Node checker shares no code with this crate: it writes canonical text itself, checks the order rules itself, frames and hashes with its own implementation, recomputes each manifest's integrity, and hashes each payload file.

## The matrix

The unit tests are at the end of each source file, named here by their function.

| 017 area | Checked by |
| --- | --- |
| Dataset framing | `dataset.rs`: `the_preimage_is_exactly_the_dataset_017_frames`, `the_digests_are_the_vectors_an_independent_implementation_checked` (with the edit that swaps ICU4X's pairs), `every_payload_the_vectors_refuse_is_refused`, `the_four_families_are_required_once_each_in_name_order`. `conformance.rs`: `a_collection_out_of_order_is_unsorted_and_a_repeat_is_repeated`. Node: the canonical text, the order rules, the digest, and the refused payloads |
| Representation independence | `reader.rs`: `every_fixture_artifact_is_admitted_with_the_vectors_semantic_digest`, `two_layouts_or_moved_sources_keep_one_dataset_and_change_the_artifact`. `generate.rs`: `both_layouts_carry_the_payloads_they_were_written_from`. `dataset.rs`: the edits to one validity code, type, and correction. Node: the narrow and wide artifacts share a dataset and differ in the markers payload's file digest |
| Manifest | `manifest.rs`: `integrity_covers_everything_but_its_own_member`, `a_repeated_member_is_found_at_any_depth`, `an_unknown_member_is_found_at_any_depth`, `another_kind_or_schema_revision_is_unsupported`, `bytes_that_are_not_one_closed_manifest_are_malformed`, `several_problems_report_the_first_of_015s_order`, `each_failure_has_its_015_reason`. `reader.rs`: `a_manifest_whose_digest_is_not_its_own_is_refused`, `a_dataset_schema_or_payload_format_this_reader_does_not_implement_is_refused`. Node: each manifest's integrity with only its top-level digest left out, and the vendored provenance |
| Payload integrity | `reader.rs`: `a_payload_must_be_supplied_with_its_declared_length_and_digest`, `a_declared_length_or_digest_that_disagrees_with_the_bytes_is_refused`, `the_steps_fail_in_017s_order`. Node: each payload file's byte length and SHA-256 |
| Marker and conformance shape | `reader.rs`: `a_blob_with_a_family_missing_or_added_is_refused_by_name` (with `generate`), `a_payload_that_does_not_decode_under_its_format_is_refused_with_the_reason`. `markers.rs`: the index checks for each family missing, an additional family, and the order. `conformance.rs`: `a_value_outside_the_closed_structure_is_refused_as_a_shape` (unknown classes and statuses), `a_collection_out_of_order_is_unsorted_and_a_repeat_is_repeated` (a correction listed twice), `a_code_may_be_in_one_group_of_its_field_only`, `a_correction_maps_straight_to_a_value_nothing_corrects_further` |
| Limits | `manifest.rs`: `the_byte_and_depth_limits_admit_their_exact_values`, `limits_come_before_anything_is_decoded`. `reader.rs`: `the_payload_count_and_declared_lengths_are_limited_before_anything_is_fetched`, `the_logical_marker_payloads_are_limited_before_they_are_built`. `conformance.rs`: `each_limit_admits_its_exact_count_and_refuses_the_next`, `limits_are_checked_before_the_value_is_projected` |
