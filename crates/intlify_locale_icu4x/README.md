# intlify_locale_icu4x

The ICU4X 2.2.0 reference adapter for [Intlify](../../design/000-intlify-overview-design.md)'s locale canonicalization.

> [!IMPORTANT]
>
> This crate is part of Phase 2 of [design 015](../../design/015-intlify-project-profile-and-locale-policy-design.md), the Locale Canonicalization Boundary. It reads the four ICU4X marker families that the Locale Canonicalization Specification revision `"0"` admits and builds ICU4X's canonicalizer from them. It does not yet apply the Intlify conformance layer, read the [017 artifact manifest](../../design/017-intlify-shared-artifact-and-version-admission-design.md#locale-canonicalization-data-representation), or serve as `intlify_config`'s provider. See [Current status](#current-status).

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

## The committed blob

`data/markers.postcard` is the four families from `icu_locale_data` 2.2.0, ICU4X's data generated from CLDR 48.2.0, in ICU4X 2.2's blob schema. ICU4X's exporter orders everything it writes, so the same data always gives the same bytes. To regenerate it, or to check that it is current:

```sh
cargo run -p intlify_locale_icu4x --features generate --example generate_locale_data -- --write
cargo run -p intlify_locale_icu4x --features generate --example generate_locale_data -- --check
```

The test suite checks the same freshness under `--all-features`.

## Current status

This crate reads the marker blob and gives ICU4X's raw behavior. Still to come in Phase 2:

- the reader of 017's Locale Canonicalization Data Artifact: the manifest, payload file digests, the dataset's semantic digest, and limits;
- the conformance data generated from CLDR 48.2.0 (validity and corrections), the conformance layer, and the complete artifact;
- the provider for `intlify_config`, behind a non-default feature there;
- the gate against CLDR's canonicalization corpus.
