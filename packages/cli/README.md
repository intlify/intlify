# @intlify/cli

The `@intlify/cli` package provides the public `intlify` command for Intlify MessageFormat 2 tooling.

`intlify fmt` formats direct `.mf2` messages and opted-in JSON resource catalogs with the native formatter. It supports write mode, `--check`, `--list-different`, `--stdin-filepath`, `--mode standard|preserve`, `--ignore-path`, and `--reporter json`.

`intlify messages emit` links configured JSON catalogs with JavaScript or TypeScript message references, exports the selected messages as ESM artifacts, and durably registers those artifacts under configured project-relative output roots.

`lint`, `check`, and `init` remain reserved command names. Invoking those commands in this release returns a `command_not_ready` operational error.

## Install

```sh
npm install --save-dev @intlify/cli
```

The package resolves the `@intlify/cli-native` native package for the current platform and forwards all command line behavior to the native Rust CLI binary.

## Config Schema

The project config schema is published at:

```text
@intlify/cli/schema/config.schema.json
```

Use it from `intlify.config.json`:

```json
{
  "$schema": "./node_modules/@intlify/cli/schema/config.schema.json",
  "fmt": {
    "mode": "standard",
    "ignorePatterns": ["dist/**", "node_modules/**"]
  },
  "resources": {
    "catalogs": [
      {
        "include": ["locales/**"]
      }
    ]
  },
  "lint": {}
}
```

`intlify.config.jsonc` is also supported when comments or trailing commas are useful:

```jsonc
{
  "$schema": "./node_modules/@intlify/cli/schema/config.schema.json",
  "fmt": {
    "mode": "standard",
    "ignorePatterns": []
  },
  // Linter options are added in Phase 3C.
  "lint": {}
}
```

## JSON Resource Catalogs

The initial resource/catalog formatter scope is the Tier 1 JSON adapter. Every JSON string leaf is treated as an MF2 message entry. Changed messages are re-escaped into the original value spans, the complete candidate JSON document is re-parsed and re-extracted, and bytes outside changed values are preserved.

An individually named `.json` file is an explicit opt-in and does not require `resources.catalogs`:

```sh
intlify fmt locales/en.json
```

Directory, glob, and implicit current-directory discovery do not classify arbitrary JSON files by extension or content. Add matching `resources.catalogs` definitions for those bulk inputs:

```sh
intlify fmt "locales/**"
intlify fmt .
```

`resources.catalogs` is optional. An explicit empty array disables catalog processing, while omitted configuration leaves direct-file JSON opt-in available. Catalog membership uses project-relative `include` and optional `exclude` patterns; an optional `format` field can explicitly select the canonical `json` adapter.

## Message Delivery

Message delivery is a project-wide operation. Its inputs come from `resources.catalogs` and `messages`; `messages emit` does not accept positional files, directories, globs, or stdin.

The following project links one `t()` call to the matching English and Japanese catalog entries:

```text
.
├── intlify.config.json
├── locales
│   ├── en.json
│   └── ja.json
└── src
    └── app.ts
```

`locales/en.json`:

```json
{ "title": "Title" }
```

`locales/ja.json`:

```json
{ "title": "タイトル" }
```

`src/app.ts`:

```ts
t('title')
```

Configure the catalog scope, production locales, reference recognizer, and delivery target in `intlify.config.json`:

```json
{
  "$schema": "./node_modules/@intlify/cli/schema/config.schema.json",
  "resources": {
    "catalogs": [
      {
        "scope": "app",
        "include": ["locales/*.json"],
        "locale": {
          "from": "path",
          "pattern": "locales/{locale}.json"
        }
      }
    ]
  },
  "messages": {
    "locales": ["en", "ja"],
    "coverageBaseline": { "app": "en" },
    "producers": {
      "js": {
        "include": ["src/**/*.ts"],
        "recognizers": {
          "t": {
            "kind": "lookup",
            "scope": "app",
            "domain": "json-pointer",
            "keySyntax": "dot-path"
          }
        }
      }
    },
    "delivery": {
      "targets": [
        {
          "name": "web",
          "exporter": "esm",
          "out": "generated/messages",
          "eagerLocales": ["en"]
        }
      ]
    }
  }
}
```

Write the complete managed output for every configured target:

```sh
intlify messages emit
```

The ESM exporter writes locale modules, `loader.mjs`, a typed accessor for the baseline scope, and `.intlify-output-manifest.json` under `generated/messages`. Artifact filenames are deterministic but intentionally opaque.

On Windows, write mode currently exits with `2` and publishes no output because the required durable directory-flush capability is unavailable. Read-only `--check` mode remains supported.

Compare the expected artifacts without changing the output:

```sh
intlify messages emit --check
```

A matching check exits with `0`. Missing, changed, stale, or non-canonical managed output is reported as a difference and exits with `1`; operational failures exit with `2`. Use `--target web` to execute one configured target while retaining the same complete project analysis, and use `--reporter=json` for the stable structured result.

## Formatter Limitations

The resource + catalog formatter acceptance gate covers the JSON adapter and `intlify fmt`; catalog linting remains deferred until the Phase 3C linter exists.

- YAML, Vue SFC, JSONC, JSON5, and XLIFF adapters are not included in the initial JSON tier
- line wrapping is not supported
- formatter ignore directives inside MF2 files are not supported
- range formatting is not supported
- `.editorconfig` is not loaded

## Formatter Benchmarks

Local formatter benchmark tooling lives in `tools/format-bench`.

```sh
vp run format-bench#bench
vp run format-bench#bench:smoke
```

The benchmark result schema is validated, but timing thresholds are not used as CI gates. Parser N-API, formatter N-API, formatter WASM, and CLI artifacts are built in release mode. Cold-start stages are reported separately from warm measurements. Warm API timings run in one process after unmeasured warm-up calls, while CLI timings retain fresh-process startup on every iteration and are labeled accordingly. wasm-pack downloads its build tools into the workspace cache before measurements begin.

Resource extraction, write-back, catalog CLI, peak-memory scaling, and deterministic physical-group aggregation have a separate local-first gate:

```sh
vp run bench:resource
vp run bench:resource:smoke
vp run bench:resource:validate
```

## Message Linker Benchmarks

The core project-link path has a separate release-profile benchmark under `tools/messages-bench`:

```sh
vp run bench:messages
vp run bench:messages:smoke
vp run bench:messages:validate
```

It measures project input I/O, JavaScript/TypeScript reference production and cache hit/miss paths, artifact codecs, resource-to-definition projection, semantic-link stages, allocator-observed link peak memory, and the complete in-process workflow. Tier 1 extraction in the E2E case retains its separate resource-owned measurement.

The non-default Rust benchmark features are enabled only by the non-published runner. They do not add user-facing options or alter ordinary linker, exporter, or artifact-size behavior. CI checks buildability, required-case coverage, result structure, boundary/companion integrity, and within-run determinism; timing and memory magnitudes remain observational.
