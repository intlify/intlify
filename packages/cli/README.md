# @intlify/cli

The `@intlify/cli` package provides the public `intlify` command for Intlify MessageFormat 2 tooling.

Phase 3A reserves `fmt`, `lint`, `check`, and `init` so integrations can target a stable command surface before formatter and linter engines are implemented. Invoking those commands in this release returns a `command_not_ready` operational error.

## Install

```sh
npm install --save-dev @intlify/cli
```

The package resolves a target-specific optional native package for the current platform and forwards all command line behavior to the native Rust CLI binary.

## Config Schema

The project config schema is published at:

```text
@intlify/cli/schema/config.schema.json
```

Use it from `intlify.config.json`:

```json
{
  "$schema": "./node_modules/@intlify/cli/schema/config.schema.json",
  "fmt": {},
  "lint": {}
}
```

`intlify.config.jsonc` is also supported when comments or trailing commas are useful:

```jsonc
{
  "$schema": "./node_modules/@intlify/cli/schema/config.schema.json",
  // Formatter options are added in Phase 3B.
  "fmt": {},
  // Linter options are added in Phase 3C.
  "lint": {}
}
```
