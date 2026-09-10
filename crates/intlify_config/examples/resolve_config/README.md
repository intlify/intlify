# Try configuration resolution

Edit a project-profile configuration and inspect its normalized locales or validation errors from the terminal. This developer example calls `intlify_config`'s existing minimum implementation; it is separate from the Intlify CLI.

> [!IMPORTANT]
>
> This is not a complete project-profile resolver. It uses a **finite test locale provider**, does not fetch policy or target-profile bodies, and does not produce a `LocalizationProjectProfile`. A successful run means only that structural validation, profile selection, and the minimum locale core succeeded.

## Run the example

From the repository root, with the workspace Rust toolchain installed:

```sh
cargo run -p intlify_config --features dev-example --example resolve_config -- \
  crates/intlify_config/examples/resolve_config/intlify.config.json
```

The supplied [configuration](./intlify.config.json) declares one profile, `app`. It intentionally uses `EN-us` so you can see canonicalization:

```text
Status: resolved (stage: locale)
Profile: app
Source locale: (not declared)
Requested locales: ["en-US","ja"]
Default requested locale: en-US

Suggested canonical spellings (file is unchanged):
  /profiles/app/requestedLocales/1 -> en-US
  /profiles/app/defaultRequestedLocale -> en-US
```

The output also includes a notice about the example's limited scope. No source-locale default is invented when `defaultSourceLocale` is absent. Requested locales are shown in canonical order, not input order.

## Edit and rerun

You can edit the supplied file, or copy it to a scratch file and pass that path:

```sh
cp crates/intlify_config/examples/resolve_config/intlify.config.json /tmp/intlify-example.config.json
cargo run -p intlify_config --features dev-example --example resolve_config -- \
  /tmp/intlify-example.config.json
```

Try these changes **one at a time** under `profiles.app`, starting from the supplied configuration each time:

| Change | Expected result |
| --- | --- |
| Add `"defaultSourceLocale": "iw-IL"` | The source locale becomes `he-IL`, with a suggested correction. |
| Set `requestedLocales` to `["en", "EN"]` and `defaultRequestedLocale` to `"en"` | `CanonicalLocaleDuplicate`, with both occurrence paths. Duplicates are not silently removed. |
| Set `defaultRequestedLocale` to `"fr"` | `DefaultNotRequested`, because `fr` is not in the requested set. |
| Set `defaultRequestedLocale` to `"en_US"` | `InvalidLocaleIdentifier`. |
| Set `defaultRequestedLocale` to `"pt-BR"` | `UnsupportedLocaleInput`: the fixture lacks this locale; this does **not** mean the locale is invalid. |
| Remove `policies.glossarySet` | `RequiredFieldMissing`. This field must be present, even when its value is `null`. |

Errors include a line and byte column when a source position is available. Locale errors also include a JSON Pointer to the affected field. On a rejected run, no partial locale core is printed; already-known spelling corrections may still be shown. The example never rewrites your file.

## Select a profile and inspect JSON

One declared profile is selected automatically. If you add another complete profile under a different key in `profiles`, use `--profile NAME` to select one explicitly:

```sh
cargo run -p intlify_config --features dev-example --example resolve_config -- \
  crates/intlify_config/examples/resolve_config/intlify.config.json --profile app --json
```

Profile names are matched exactly. The whole file must be structurally valid, including unselected profiles.

`--json` prints a pretty-printed **display object**, not a stable API, formal diagnostic record, or serialized profile. It includes the reached stage, selected profile, locale core (or `null`), spelling corrections, and diagnostics. Source offsets are zero-based UTF-8 byte offsets with an exclusive end; line and byte-column numbers are one-based.

```sh
cargo run -p intlify_config --features dev-example --example resolve_config -- --help
```

Exit codes are `0` for a resolved minimum locale core, `1` for a rejected configuration or profile selector, and `2` for usage, file I/O, or internal errors. Configuration results go to stdout; usage, I/O, and internal errors go to stderr. `--json` applies to configuration results, not those stderr errors.

## Scope and limits

The non-default `dev-example` feature exposes only an unstable, doc-hidden display helper. It does not enable measurement collectors or change the normal library's API or the existing CLI configuration workflow.

The finite provider accepts these canonical spellings:

```text
ar-EG-u-nu-latn  de  de-DE  en  en-US  en-u-ca-gregory-nu-latn
fr  fr-FR  he-IL  ja  und  und-u-ca-islamic-civil  zh-Hant-TW
```

It also recognizes these exact aliases:

| Input                     | Canonical spelling        |
| ------------------------- | ------------------------- |
| `EN`                      | `en`                      |
| `EN-us`                   | `en-US`                   |
| `en-u-nu-latn-ca-gregory` | `en-u-ca-gregory-nu-latn` |
| `iw-IL`                   | `he-IL`                   |
| `und-u-ca-islamicc`       | `und-u-ca-islamic-civil`  |

Other spellings may be unsupported, including valid locales or casing variants. There is no CLDR download, platform locale lookup, or production locale-data integration.

The sample includes mandatory policy and target-profile references so it passes the real project-profile configuration schema. Their identities and digests are **illustrative placeholders**, not verified artifacts. Only reference structure is checked. The example does not resolve those bodies, negotiate locales, compute fallback, validate target/deployment semantics, or generate runtime output. Keep these fields as supplied when experimenting with the minimum locale core.

The example uses its own fixed safety limits: a 1,000,000-byte file, 100,000 parser tokens/nodes/collection entries, depth 64, 1,000,000 total decoded string bytes, 100,000 bytes per decoded string, 8 profiles, 64 bytes per profile ID, 1,000,000 structural-analysis units, 128 bytes per locale identifier spelling, 16 active locale occurrences, and 8 requested locales. These are developer-example limits, **not** product defaults or an admitted Resource Limit Policy. Oversized files are rejected before parsing; file reads stop after the limit plus one byte.

The schema describes the configuration shape, including fields beyond the minimum implementation: [project-profile-config-v0.schema.json](../../schema/project-profile-config-v0.schema.json).

## Test the example

```sh
cargo test -p intlify_config --features dev-example --example resolve_config
cargo test -p intlify_config --features dev-example --lib structural::example
```
