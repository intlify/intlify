# intlify_locale_icu4x

The ICU4X 2.2.0 reference adapter for [Intlify](../../design/000-intlify-overview-design.md)'s locale canonicalization.

> [!IMPORTANT]
>
> This crate is part of Phase 2 of [design 015](../../design/015-intlify-project-profile-and-locale-policy-design.md), the Locale Canonicalization Boundary. It reads [design 017](../../design/017-intlify-shared-artifact-and-version-admission-design.md#locale-canonicalization-data-representation)'s Locale Canonicalization Data Artifact in 017's six steps, rebuilds its logical dataset to check the semantic digest, and builds ICU4X's canonicalizer from the four marker families that the Locale Canonicalization Specification revision `"0"` admits. It does not yet apply the Intlify conformance layer or serve as `intlify_config`'s provider. See [Current status](#current-status).

Design 015 keeps canonicalization semantics in an Intlify-owned specification and treats ICU4X as one physical engine behind a provider boundary. `intlify_config` holds no locale data and links no ICU4X. This crate is where the engine lives: it receives the data as a blob, checks that the blob carries exactly what revision `"0"` admits, and answers what ICU4X 2.2.0 does with it.

## What this crate never does

- It links no compiled locale data in an ordinary build. The four marker families reach it only through the blob a caller supplies. The `generate` feature turns on ICU4X's compiled data for the generator alone.
- It does not decide whether a locale identifier is valid. ICU4X's raw canonicalization accepts identifiers revision `"0"` rejects, such as `zz`, `qfz`, `en-x-brand`, and `en-u-ca-madeup`, and leaves `und-u-ca-islamicc` uncorrected. The conformance layer of design 015 makes those decisions, not ICU4X.
- It never adds likely subtags: `en` stays `en` and is a different locale from `en-US`.
- It does not prove a blob's integrity. ICU4X decodes damaged but well-formed data as other data, so the 017 reader checks a payload's file digest before this crate reads it and the dataset's semantic digest after.

## The four marker families

| Marker | ICU4X path | Used for |
| --- | --- | --- |
| `LocaleAliasesV1` | `locale/aliases/v1` | Language, script, region, variant, and subdivision aliases |
| `LocaleLikelySubtagsExtendedV1` | `locale/likely/subtags/extended/v1` | Likely subtags for every locale, not only common ones |
| `LocaleLikelySubtagsLanguageV1` | `locale/likely/subtags/language/v1` | Likely subtags keyed by language |
| `LocaleLikelySubtagsScriptRegionV1` | `locale/likely/subtags/script/region/v1` | Likely subtags keyed by script and region |

Design 015 requires all four, so that ICU4X's extended canonicalizer can disambiguate aliases for every admitted locale. A blob with one missing, with another family, with an index out of order, or in a blob schema from before ICU4X 2.0 is refused by name before ICU4X builds anything.

## Logical payloads

Each family's logical payload is the value of ICU4X 2.2's human-readable serde serialization of its data struct, which ICU4X documents as the stable representation of its data. Design 017 frames these values into the dataset's semantic digest, so the same data has the same payloads whichever export carried it; a test checks that the blob gives exactly what ICU4X's compiled data gives. A map whose keys are not strings stays the ordered array of key and value pairs ICU4X writes. A payload holding a number, boolean, or null cannot be framed and is refused.

## The dataset and its semantic digest

Design 017's Locale Canonicalization Dataset is the dataset schema `intlify-locale-canonicalization-dataset` / `"0"`, the four logical payloads in name order, and the conformance data: CLDR validity codes by field and status, every `u` and `t` key with its types, and the flattened corrections. Its semantic digest is `H("locale-canonicalization-dataset", dataset)` under 017's value framing, so the same data has the same digest whichever export carried it.

The conformance data is its own payload, written as canonical JSON text revision `"0"`, which `intlify_shared_json::text` writes and reads. Reading it checks the closed structure and the order 017 fixes, so the same records have one spelling and one digest:

- validity groups by field and then status, each group's codes sorted without repeats, and no code in two groups of one field;
- keys by extension and then key, and each key's types by kind and then class or type, without repeats;
- corrections by extension, key, and then `from`, each `from` once, and none whose target another correction of the same key changes again.

The caller's limits on groups, codes, keys, types, corrections, and their total are checked on the decoded value before any of it is projected. Whether these are the records a specification revision admits is for the semantic digest to decide, not the reader.

## Committed schemas, vectors, and fixture artifacts

`schema/locale-canonicalization-data-v0.schema.json` and `schema/locale-conformance-data-v0.schema.json` are the closed Draft 7 schemas of the manifest and the conformance value. Draft 7 cannot say "sorted", so the orders above are the reader's alone. `fixtures/dataset/vectors.json` holds a small dataset, its conformance payload as canonical JSON text, its semantic digest, edits that must each change the digest, and payloads that must be refused. Its marker payloads are ICU4X's data cut down to two entries per map and list, and its conformance data is modelled on CLDR's records; neither is the dataset revision `"0"` admits. `tools/shared-json-vectors` checks the vectors with an implementation that shares no code with this crate.

`fixtures/artifact` realizes that dataset as artifacts: its conformance payload, its marker payloads as a blob in each index layout, and three sealed manifests, two that differ only in the blob they name and one that differs only in where its CLDR sources were read from. The same tools check each manifest's integrity and each payload file. [`fixtures/README.md`](fixtures/README.md) maps each area of 017's fixtures to where it is checked.

```sh
cargo run -p intlify_locale_icu4x --example generate_locale_schema -- --write
cargo run -p intlify_locale_icu4x --example generate_dataset_vectors -- --write
cargo run -p intlify_locale_icu4x --features generate --example generate_artifact_fixtures -- --write
vp run schema:locale:check
vp run vectors:locale:check
```

## Reading an artifact

An artifact is a manifest beside two payloads: the conformance data as canonical JSON text, and the four marker families as an ICU4X 2.2 blob. The reader fetches nothing. The caller reads the manifest, learns from it where each payload is, and hands over the exact bytes:

```rust
let manifest = VerifiedManifest::read(&manifest_bytes, &limits)?; // 017 steps 1 to 3
let conformance = read(manifest.address(Role::Conformance))?;
let markers = read(manifest.address(Role::Markers))?;
let data = manifest.admit(Some(&conformance), Some(markers.into()), &limits)?; // steps 4 to 6
```

`VerifiedManifest::read` is design 015's `locale-canonicalization-data-artifact` pre-invocation boundary: the manifest's byte and depth limits, then its strict, closed decoding. A failure there is a `ReadFailure::Envelope`, and `EnvelopeFailure::reason` names its closed 015 reason. The byte and depth limits come first, because 017 checks limits before anything is decoded; after them the reasons follow 015's order (malformed, duplicate member, unknown member). Then the limits the manifest decides (the payload count and each payload's declared length, before the caller fetches anything) and its integrity digest.

`admit` checks each payload's length and file digest, decodes each under the one format its role admits, and rebuilds the dataset to compare its semantic digest. Every failure after the boundary is a `ReadFailure::Data`, which 015 reports as `project-profile-locale-data-not-admitted`. Admitting data is not admitting a specification: whether the specification is supported and the dataset is the one it admits are the owner's checks, which come after.

Every limit is the caller's, with no defaults: the manifest's bytes and depth, the payload count, each payload's declared length, the four logical marker payloads as compact JSON text (counted before any is built), and the conformance counts.

## The committed blob

`data/markers.postcard` is the four families from `icu_locale_data` 2.2.0, ICU4X's data generated from CLDR 48.2.0, in ICU4X 2.2's blob schema. ICU4X's exporter orders everything it writes, so the same data always gives the same bytes. To regenerate it, or to check that it is current:

```sh
cargo run -p intlify_locale_icu4x --features generate --example generate_locale_data -- --write
cargo run -p intlify_locale_icu4x --features generate --example generate_locale_data -- --check
```

The test suite checks the same freshness under `--all-features`.

## Current status

This crate reads 017's Locale Canonicalization Data Artifact, rebuilds its dataset, and gives ICU4X's raw behavior. Still to come in Phase 2:

- the conformance data generated from CLDR 48.2.0 (validity and corrections), the conformance layer, and the complete artifact;
- the provider for `intlify_config`, behind a non-default feature there;
- the gate against CLDR's canonicalization corpus.
