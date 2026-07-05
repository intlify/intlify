# @intlify/cli

The `@intlify/cli` package provides the public `intlify` command for Intlify MessageFormat 2 tooling.

`intlify fmt` formats `.mf2` files with the native formatter. It supports write mode, `--check`, `--list-different`, `--stdin-filepath`, `--mode standard|preserve`, `--ignore-path`, and `--reporter json`.

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

## Formatter Limitations

`intlify fmt` in Phase 3B is scoped to direct `.mf2` files.

- resource/catalog formatting is not supported
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
