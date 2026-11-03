<!-- @license MIT -->

# Construction authorities

Design 015's [resolver construction](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#active-project-profile-specification-and-profile-resolution-specification-input) is given five authorities. This directory holds the revision `"0"` artifacts of four of them, at the paths 015's Suite Index example uses below the conformance suite's root, `conformance/`: the Profile Specification Schema, the Active Project Profile Specification, the Configuration Schema Authority Set with the one configuration schema it maps, and the Resolver Capability Specification. The Registry Package comes with its own stage.

This crate writes every artifact from its own values, and compiles in only their digests: the Profile Specification's semantic digest, and the file digests of the other four. Construction compares them with the authorities it is given, as the last row of the construction-failure precedence. The Profile Specification's file digest is compared only with the tuple declared beside its bytes, so a body that differs from this one in formatting or member order alone is the same Profile Specification.

A file digest covers every byte, so these files are kept out of the repository formatter and out of line-ending conversion. Change them only by changing the values and rewriting them.

## Running them

```sh
# every unit test, including the committed bytes and the pinned digests
cargo test -p intlify_config --all-features --lib construction

# rewrite the artifacts after changing a value; it prints the digests to pin
# in src/construction/profile.rs, src/construction/authority_set.rs, and
# src/construction/capability.rs, and the Root Package's in
# src/construction/root.rs
cargo run -p intlify_config --features vectors --example generate_construction_artifacts -- --write

# the committed bytes, and the independent Node checks
vp run construction:check
```

## Files

| File | Authority | Contents |
| --- | --- | --- |
| `project-profile-specification-v0.schema.json` | Profile Specification Schema, `intlify-project-profile-specification-schema` | The Draft 7 schema of the Profile Specification's body, generated from the body's types |
| `project-profile-specification-v0.json` | Profile Specification, `intlify-project-profile` | The value each of the eight specification slots supports, the locale canonicalization dataset, the 16 phases in rank order, the profile view's 12 semantic groups, the 52 `projectProfileResolution` bound paths, the four other authorities, and the domain separators of the subject and evidence tokens, of the two source revisions, and of the Programmatic Entry Snapshot, with the Snapshot's 10 marker reasons |
| `configuration-schema-authority-set-v0.json` | Configuration Schema Authority Set, `intlify-configuration-schema-authority-set` | The Profile Specification the set belongs to, by identity, revision, and semantic digest, and `schemaVersion` `"0"` mapped to the configuration schema's identity, revision, dialect, and file digest |
| `configuration-v0.schema.json` | The configuration schema for `schemaVersion` `"0"`, `intlify-configuration-schema` | The bytes of the published [configuration schema](../../schema/project-profile-config-v0.schema.json) |
| `resolver-capability-v0.json` | Resolver Capability Specification, `intlify-project-profile-resolver-capability` | The 52 `policyComparable` bounds, each with the unit it counts in, and the 54 `bootstrapOnly` bounds, each with its unit and its minimum, in the canonical order |

Every authority is at revision `"0"`.

## The Profile Specification

- **Values.** The body is written from the values this crate implements: the slots' values from 015's table of supported values, with `"asserted"` for `intent-surface-class-vocabulary`, which has no fixed value; the dataset the canonicalization specification admits; the resolver's phases; the members of 015's profile view; the bounds of the Resource Limit Policy's `projectProfileResolution` section, each as its group's path and its name; the identities construction admits; and the domain separators and marker reasons as 015 writes them.
- **Schema.** The schema constrains only the form of each value: every object is closed and requires its members, an identity, revision, phase, domain separator, or marker reason is a token, a digest is a `sha256:` presentation, a semantic group or bound path is a name of its form, an array is not empty and has no repeats, and a slot is `{ identity, revision }` or `"asserted"`. No position admits a JSON number, so a schema-valid body is in 017's value domain.
- **Semantic digest.** The semantic digest is 017's `H` over the complete body, under the domain `project-profile-specification-semantic-digest`. It covers every value, but neither the body's formatting nor its member order. What each value must be is fixed by this digest, which construction compares with the one compiled in, not by the schema.

## The Configuration Schema Authority Set

The set names the Profile Specification it belongs to, which construction compares with the admitted Profile Specification, and maps each `schemaVersion` to one configuration schema, keyed in ascending unsigned UTF-8 order. Revision `"0"` maps `"0"` alone, to `intlify-configuration-schema` at revision `"0"` in Draft 7. The mapped schema's bytes are the configuration schema as this crate publishes it, so its file digest is that of the published file.

## The Resolver Capability Specification

- **Values.** The body is written from this crate's capability inventory: every `policyComparable` bound, the mirror of a `projectProfileResolution` bound, with its unit, and every `bootstrapOnly` bound with its unit and its revision-`"0"` minimum as a shortest decimal string, both in the canonical order. A bound is listed by its path inside its namespace, such as `configurationInput.maxNodes`, the form of the Profile Specification's bound paths. Every `policyComparable` minimum is one, so the body does not list it; three `bootstrapOnly` minima are larger.
- **Units.** Design 015 describes each counting unit in prose only, so a unit is its cell of 015's tables, word for word and with its code spans, and no other vocabulary is coined. The 52 `policyComparable` bounds count in 45 distinct units, and the 54 `bootstrapOnly` bounds each in its own.
- **Envelope schema.** The body's envelope schema, `resolver-capability-specification` in the Root Package's [authority-envelope schema package](../construction/README.md), closes its values, as its precedence row says: it is a closed object of the two lists, and each list is exactly its bounds in order, each bound one `const` of its path, unit, and minimum. It is written from the same inventory as the body.

The Node checker shares no code with this crate. It builds the Profile Specification and the Capability Specification from its own copy of 015's tables and the locale data's manifest, checks that the Capability Specification's envelope schema closes exactly those bounds, computes the semantic digest with its own framing of 017's values, hashes every artifact, compares the mapped schema with the published configuration schema, checks that every position of the Profile Specification Schema names a type other than a number, and reads the five pinned digests out of the crate's source. It does not read the design document.

## Construction's checks

Construction checks the authorities it is given in the order of 015's construction-failure precedence, and refuses with the first row that fails. Rows 2 to 30 are these authorities':

| Rows | Component | Checks, in order |
| --- | --- | --- |
| 2–7 | Profile Specification Schema | Presence; the declared tuple against its envelope schema, the bytes as strict UTF-8 JSON, and a root `$schema` naming Draft 7; identity; revision; file digest; the Draft 7 meta-schema |
| 8–14 | Profile Specification | Presence; the declared tuple, with its semantic digest, and strict UTF-8 JSON; identity; revision; file digest; the body against the admitted Profile Specification Schema; the semantic digest the tuple declares |
| 15–19 | Configuration Schema Authority Set | Presence; the declared tuple, strict UTF-8 JSON, the container's envelope schema, its `schemaVersion` keys in ascending unsigned UTF-8 order, and bytes for every mapped schema; identity; revision; file digest |
| 20–24 | Each mapped schema, in `schemaVersion` order | Identity; revision; dialect; file digest; strict UTF-8 JSON, a root `$schema` naming Draft 7, and the Draft 7 meta-schema |
| 25–30 | Resolver Capability Specification | Presence; the declared tuple and strict UTF-8 JSON; identity; revision; file digest; the body against its envelope schema |

The identities, revisions, and dialect each row admits, the envelope schemas, and the meta-schema are read from the built-in Root Package.

- **Strict UTF-8 JSON.** Bytes are read as 017 has every decoder read them: malformed UTF-8, a byte-order mark, trailing data, a duplicate member, and an unpaired surrogate escape are refused. Numbers are read; whether a body may hold one is its schema's to say. Construction applies the construction-root bounds to the whole input first, so reading applies no capacity of its own.
- **The meta-schema.** A schema the meta-schema admits but the validator cannot read is refused with the meta-schema's row: one that names another document, nests a `$schema`, applies itself in place in a cycle, or holds a pattern outside the validator's subset.
- **What the rows compare.** The rows compare each component with what its tuple declares. Comparing with the digests compiled in is construction's last row, so a body this crate does not implement passes these rows when it is declared as itself. A body with a number, which another schema may admit, has no semantic digest to declare.
- **Mapped schemas.** Each mapped schema passes all of its rows before the next is checked. Bytes for a `schemaVersion` the set does not map are read by no check.
- **The Capability Specification's body.** Design 015 checks each authority's envelope, identity, revision, and digest before its body, so the body is checked against its envelope schema in a row of its own, after the file digest. A container's envelope schema, in contrast, is part of its envelope row.

## The matrix

The unit tests are at the end of each source file below `src/construction/`, named here by their function.

| 015 rule | Checked by |
| --- | --- |
| The eight slots in their order, each with the value revision `"0"` supports, and the dataset the canonicalization specification admits | `profile.rs`: `the_slots_are_015s_eight_with_the_value_each_supports`. `src/resolver/specification.rs`: `the_supported_values_are_the_profile_specifications_slots`. Node: the body and the manifest |
| The phases, the semantic groups, the bound paths, the authorities, the domain separators, and the marker reasons, each in 015's order | `profile.rs`: `the_body_lists_015s_inventories_in_their_order`, `the_body_names_the_identities_construction_admits`. Node: the body |
| The bound paths are the Capability Specification's mirrored paths without their namespace | `profile.rs`: `the_body_lists_015s_inventories_in_their_order` |
| The body reads back as its type | `profile.rs`: `the_body_reads_back_as_its_type` |
| The semantic digest is 017's `H` over the complete body, without its formatting or member order | `profile.rs`: `the_semantic_digest_covers_the_values_and_not_their_form`. Node: the semantic digest |
| The schema is a Draft 7 schema the validator reads, and the body is valid against it | `profile.rs`: `the_schema_is_a_draft_7_schema_the_validator_reads`. `generate.rs`: `the_set_belongs_to_the_written_profile_specification_and_maps_the_written_schema` |
| The schema closes every object, requires every member, and checks the form of every value | `profile.rs`: `the_schema_closes_every_object_and_requires_every_member`, `the_schema_checks_the_form_of_every_value` |
| No position of the body admits a number | `profile.rs`: `no_position_of_the_body_admits_a_number`. Node: every schema position |
| The set belongs to the Profile Specification and maps `schemaVersion` `"0"` to the configuration schema, as construction admits them | `authority_set.rs`: `the_set_maps_schema_version_0_to_the_configuration_schema`, `the_set_names_what_construction_admits`. `generate.rs`: `the_set_belongs_to_the_written_profile_specification_and_maps_the_written_schema`. Node: the set |
| Its `schemaVersion` keys are in ascending unsigned UTF-8 order | `authority_set.rs`: `schema_versions_are_keyed_in_ascending_unsigned_utf_8_order` |
| The set is an instance of its envelope schema, and reads back as its type | `authority_set.rs`: `the_set_is_an_instance_of_its_envelope_schema`, `the_set_reads_back_as_its_type` |
| The mapped schema is the published configuration schema, a Draft 7 schema the validator reads | `authority_set.rs`: `the_member_is_the_configuration_schema_this_crate_publishes`. Node: the mapped bytes |
| The Capability Specification lists every bound in the canonical order by its path inside its namespace, each with its unit as 015's table writes it, and each `bootstrapOnly` bound with its minimum | `capability.rs`: `the_body_lists_every_bound_in_the_canonical_order`, `the_body_is_written_as_015_lists_it`. `src/resolver/limits.rs`: `each_bound_counts_in_the_unit_of_its_own_row_of_015s_table`. `src/resolver/capability.rs`: `each_bootstrap_bound_counts_in_the_unit_of_its_own_row_of_015s_table`, `a_capability_bound_counts_in_the_unit_of_its_table`. Node: the body |
| The Capability Specification reads back as its type and names what construction admits | `capability.rs`: `the_body_reads_back_as_its_type`, `the_specification_names_what_construction_admits` |
| Its envelope schema closes its groups, bounds, units, order, and minimum vector | `capability.rs`: `the_body_schema_closes_the_body_s_values`. `validator/differential_tests.rs`: `the_configuration_and_envelope_schemas_judge_as_the_oracle_does`. Node: the schema |
| Construction admits the committed authorities | `profile.rs`: `construction_admits_the_committed_schema_and_profile_specification`. `authority_set.rs`: `construction_admits_the_committed_set_and_its_schema`. `capability.rs`: `construction_admits_the_committed_capability_specification`, `the_body_s_form_is_not_its_bytes` |
| The Profile Specification Schema's rows 2 to 7, each refusing with its row, in their order | `profile.rs`: `each_schema_check_refuses_with_its_row`, `the_schema_s_rows_keep_their_order` |
| The Profile Specification's rows 8 to 14, each refusing with its row, in their order, against what the tuple declares | `profile.rs`: `each_body_check_refuses_with_its_row`, `the_body_s_rows_keep_their_order`, `a_body_with_a_number_has_no_semantic_digest_to_declare`, `these_rows_compare_the_declared_tuple_and_not_the_compiled_digests` |
| The set's rows 15 to 19, each refusing with its row, in their order | `authority_set.rs`: `each_set_check_refuses_with_its_row`, `schema_versions_are_checked_in_ascending_unsigned_utf_8_order`, `the_set_s_rows_keep_their_order` |
| Each mapped schema's rows 20 to 24, all of one schema's before the next's | `authority_set.rs`: `each_mapped_schema_check_refuses_with_its_row`, `each_mapped_schema_passes_all_its_rows_before_the_next`, `bytes_for_a_schema_version_the_set_does_not_map_are_read_by_no_check` |
| The Capability Specification's rows 25 to 30, each refusing with its row, the body's after the file digest | `capability.rs`: `each_check_refuses_with_its_row`, `the_body_is_checked_after_the_file_digest`. `component.rs`: `a_body_with_a_row_of_its_own_is_checked_after_the_file_digest` |
| The checks every component starts with, strict UTF-8 JSON, the Draft 7 checks, and each check's row | `component.rs`: `strict_json_reads_017s_json_with_its_numbers`, `strict_json_refuses_what_017_has_every_decoder_refuse`, `strict_json_applies_no_reader_capacity_of_its_own`, `every_row_is_the_only_one_of_its_component_and_check`, `every_component_starts_with_its_own_five_rows`, `each_check_refuses_with_its_row`, `the_first_failing_row_is_the_one_reported`, `a_container_is_checked_against_its_second_envelope_schema`, `a_schema_declares_draft_7_by_its_root_s_own_uri`, `an_admitted_schema_is_one_the_meta_schema_admits_and_the_validator_reads` |
| The members of the built-in root construction checks against | `root.rs`: `the_built_in_root_reads_the_admission_body_and_the_schemas`, `the_admission_body_reads_only_as_its_closed_type`, `every_envelope_schema_must_be_held_and_read`, `the_meta_schema_is_the_one_listed_for_draft_7` |
| The committed bytes are the ones this crate writes, and the pinned digests are theirs | `generate.rs`: `the_committed_authorities_are_the_ones_this_crate_writes`, `the_pinned_authority_digests_are_the_ones_the_authorities_make`, `every_artifact_ends_with_one_newline_and_has_no_carriage_return`. Node: every digest |

