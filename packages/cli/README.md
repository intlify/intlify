# @intlify/cli

The `@intlify/cli` package provides the public `intlify` command for Intlify MessageFormat 2 tooling.

`intlify fmt` formats direct `.mf2` messages and opted-in JSON resource catalogs with the native formatter. It supports write mode, `--check`, `--list-different`, `--stdin-filepath`, `--mode standard|preserve`, `--ignore-path`, and `--reporter json`.

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

The benchmark result schema is validated, but timing thresholds are not used as CI gates.

Resource extraction, write-back, catalog CLI, peak-memory scaling, and deterministic physical-group aggregation have a separate local-first gate:

```sh
vp run bench:resource
vp run bench:resource:smoke
vp run bench:resource:validate
```
