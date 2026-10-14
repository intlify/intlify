<!-- @license MIT -->

# Vendored CLDR 48.2.0 records

These are the CLDR files the generator reads to make the conformance data of Locale Canonicalization Specification revision `"0"`, and the corpus the adapter is gated against, unchanged from the [Unicode CLDR](https://github.com/unicode-org/cldr) repository at tag `release-48-2`. They keep the layout of the CLDR distribution root, which is what the artifact manifest's `generation.cldr.sources` addresses are relative to.

| Path | Read for |
| --- | --- |
| `common/bcp47/*.xml` (15 files) | Every `u` and `t` key with its types, and the corrections derived from deprecated types and aliases |
| `common/validity/*.xml` (5 files) | The validity groups of languages, regions, scripts, subdivisions, and variants |
| `common/testData/localeIdentifiers/localeCanonicalization.txt` | The corpus gate: every case of its four sets, with the result CLDR expects |

The files are under the Unicode License v3, in [`LICENSE`](LICENSE), copied from the same tag. Nothing here is linked into the adapter: the generator and the corpus gate read them only under the `generate` feature. The manifest records the SHA-256 of each file the generator reads, and the gap registry that of the corpus, which is evidence about the adapter rather than an input to the data.

Revision `"0"` pins CLDR 48.2.0. Adopting other CLDR data is a new specification revision, not an update of these files.
