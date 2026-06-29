# ox-mf2 Phase 3A Tooling Foundation Design

## Purpose

This document tracks the detailed design for Phase 3A: Tooling Foundation.

The broader Phase 3 tooling and consumer boundary is defined in [005-ox-mf2-phase-3-tooling-transport-design.md](./005-ox-mf2-phase-3-tooling-transport-design.md). That document splits implementation into consumer-facing product phases. This document covers the first phase: the shared CLI, configuration, machine-readable output, and distribution foundation needed before formatter, linter, LSP/editor, agent, or long-lived transport products are implemented.

## Goals

- Establish the Phase 3A CLI crate and the `intlify` command structure.
- Define the shared CLI package and native binary distribution boundary.
- Define the unified project configuration model with `fmt` and `lint` sections.
- Publish a unified JSON Schema for editor completion and config validation.
- Define shared machine-readable output conventions for future `fmt --check`, `lint`, and combined `check` workflows.
- Keep formatter and linter resolved config models separate internally while exposing one project-level config file to users.
- Keep CLI output schemas separate from the project config schema.
- Provide enough package and API boundaries for Phase 3B formatter and Phase 3C linter work to proceed independently.
- Keep later LSP/editor, agent, and transport workflows as consumers of the foundation instead of direct Phase 3A deliverables.

## Non-Goals

- Implementing the formatter engine.
- Implementing the linter engine.
- Implementing formatter or linter N-API/WASM packages beyond defining their package boundaries.
- Implementing an LSP server, editor extension, or editor adapter.
- Implementing agent-specific plugins, skills, hooks, MCP servers, or ACP integrations.
- Implementing MessagePack transport or a long-lived daemon.
- Defining formatter layout rules, linter rule semantics, suppression directives, or resource/catalog mapping details.
- Supporting nested config discovery or nearest-config-wins behavior in the initial foundation.
- Supporting file-specific config overrides in the initial foundation.

## Deliverables

Phase 3A deliverables:

- `crates/ox_mf2_cli` crate skeleton
- `intlify` CLI command structure
- native `intlify` CLI npm package boundary
- unified project config model
- generated unified project config JSON Schema
- shared machine-readable output envelope conventions
- package boundary notes for future formatter/linter CLI, N-API, and WASM packages
- validation fixtures for config parsing and CLI JSON output shape

The Rust CLI crate is defined as:

- crate directory: `crates/ox_mf2_cli`
- Cargo package name: `ox_mf2_cli`
- binary target name: `intlify`
- binary entry point: `src/main.rs`
- library target: `src/lib.rs`

The native binary copied into npm packages is `intlify` on Unix platforms and `intlify.exe` on Windows.

The binary entry point should stay thin. CLI core behavior, including command routing, config discovery, config loading, config validation, reporter selection, and output shaping, should live in library modules under `src/lib.rs`. This is required so Phase 3A can test config loader behavior directly without adding hidden public CLI commands.

## Ownership

The CLI crate owns command routing, config discovery, config loading, output formatting, exit-code behavior, and package-level CLI composition.

Formatter and linter crates own their resolved config models once Phase 3B and Phase 3C begin. Phase 3A owns only the project-level config envelope, schema generation boundary, and normalization path that lets the CLI pass `fmt` and `lint` sections to product-specific crates later.

Parser crates remain responsible for parsing, diagnostics, Binary AST snapshots, and semantic lowering. Phase 3A should not move parser behavior into the CLI crate.

## Architecture

Phase 3A introduces the CLI shell and distribution layer without moving parser behavior or implementing formatter/linter engines. The architecture separates the user-facing `intlify` binary from product-specific engines so later phases can add formatter and linter behavior behind the same command and reporting contracts.

![Phase 3A CLI foundation architecture](./assets/006-ox-mf2-phase-3a-cli-architecture.svg)

The public `@intlify/cli` wrapper package owns the user-facing command, bundled config schema, native package resolution, and release-time installed-package smoke-test coverage. Platform-specific native npm packages own only the compiled native `intlify` binary for their target. The Rust CLI crate owns runtime command routing, config loading, reporter selection, JSON envelope shaping, and exit-code mapping.

Formatter and linter crates remain product-specific extension points. Both future engines consume parser-owned parse artifacts instead of owning parsing themselves. Phase 3A only defines how their future config sections, results, and operational errors flow through the CLI foundation.

## CLI Surface

The initial CLI should reserve the user-facing command shape without requiring all product implementations to exist immediately:

```text
intlify fmt
intlify lint
intlify check
```

Phase 3A reserves the `fmt`, `lint`, and `check` command names but keeps them out of normal `intlify --help` output until the required product engines are ready. If any reserved command is invoked directly in Phase 3A, the CLI returns an operational error, exits with code `2`, and uses `kind: "unsupported"` with `code: "command_not_ready"` in JSON reporter output. `intlify check` requires both formatter and linter products, so its placeholder error uses `details.requiredPhase: "3B+3C"`.

The CLI should provide consistent global behavior for help output, version output, config path handling, machine-readable output selection, and operational errors. Phase 3A global options are:

- `--help`
- `-h`
- `--version`
- `-V`
- `--config <path>`
- `--reporter <text|json>`

The default reporter is `text`. Machine-readable JSON output is selected with `--reporter json`. Phase 3A does not support `--cwd` or `--root`; project root discovery is fixed by the config discovery contract below.

Value-taking long options accept both separated and equals forms. `--reporter json` and `--reporter=json` are equivalent. `--config path` and `--config=path` are equivalent. Phase 3A does not define `-r` or `-c`; the only short options are `-h` and `-V`.

`intlify --version` reports the public `@intlify/cli` package version. The JSON envelope `version` field uses the same value. The wrapper package, native packages, Rust binary, and CLI crate should be released with matching versions; version mismatches should be caught by build, validation, or publish workflows instead of being surfaced as a normal runtime mode.

The first monorepo-managed `@intlify/cli` release is `0.14.0`. The monorepo version policy remains unified: ox-mf2 npm packages, ox-mf2 crates, `@intlify/cli`, native CLI packages, and the Rust CLI crate should all release as `0.14.0` for that release. This keeps the existing standalone `@intlify/cli` npm version history, which has already reached `0.13.1`, compatible with the unified monorepo version policy.

Top-level help and version behavior:

- `intlify`, `intlify --help`, and `intlify -h` write top-level help to stdout and exit with `0`.
- `intlify --version` and `intlify -V` write the public `@intlify/cli` version to stdout and exit with `0`.
- Top-level help does not list reserved `fmt`, `lint`, or `check` commands as normal available commands in Phase 3A.
- `intlify fmt --help`, `intlify lint --help`, and `intlify check --help` write reserved-command placeholder help to stdout and exit with `0`.
- Reserved-command placeholder help states that the command is reserved but not available in the current release.

`intlify --reporter json` without a subcommand follows the same behavior as `intlify`: it writes human-readable top-level help to stdout and exits with `0`. The JSON reporter affects command result output and operational errors, but it does not JSON-encode help output for no-subcommand execution.

Global options can appear before or after the subcommand. For example, `intlify --reporter json fmt` and `intlify fmt --reporter json` are equivalent. Duplicate global options are operational input errors with `kind: "input"`, `code: "duplicate_cli_option"`, and exit code `2`.

Operational error precedence:

1. Help and version flags return help/version output and exit with `0`.
2. CLI argument shape errors are reported next, including unknown options, missing option values, duplicate options, and unsupported reporters.
3. Command routing errors are reported next, including unknown commands and reserved commands that return `command_not_ready`.
4. Config discovery, loading, and validation errors are reported only after the command is known to require config.

For example, `intlify fmt --config missing.json --reporter json` returns `command_not_ready` in Phase 3A rather than `config_not_found`, because the reserved formatter command does not execute far enough to require config loading.

Phase 3A input and routing error codes:

- `invalid_cli_argument`: malformed CLI input not covered by a more specific code, with `kind: "input"`
- `unknown_cli_option`: unknown option, with `kind: "input"`
- `missing_cli_option_value`: missing value for a value-taking option, with `kind: "input"`
- `duplicate_cli_option`: duplicate global option, with `kind: "input"`
- `reporter_not_supported`: unsupported reporter value, with `kind: "reporter"`
- `unknown_command`: unknown subcommand, with `kind: "unsupported"`
- `command_not_ready`: reserved command without an implementation in the current phase, with `kind: "unsupported"`

For `unknown_command`, the top-level envelope `command` remains `"intlify"` and the unknown subcommand is reported in `errors[].details.command`. For example, `intlify foo --reporter json` reports `details: { "command": "foo" }`.

## Configuration Contract

Project configuration is one JSON config with separate `fmt` and `lint` sections. The config file name is `intlify.config.json`.

The initial config discovery model is root-only. Root means the git repository root found by walking up from `cwd`; when no git repository root exists, root falls back to `cwd`. The discovered config path is `<root>/intlify.config.json`. Nested config discovery, nearest-config-wins behavior, and file-specific overrides are deferred until a concrete multi-workspace or resource/catalog requirement appears.

The CLI supports an explicit `--config <path>` option in Phase 3A. This is an escape hatch for CI, fixtures, and integrations, not nested discovery. When `--config` is provided, the CLI loads that exact file instead of the root `intlify.config.json`. Relative `--config` paths are resolved from the process `cwd`; absolute paths are used as-is. `--config` replaces config discovery, but it does not change `projectRoot`.

When root discovery does not find `intlify.config.json`, the CLI continues with the default project config without emitting a warning or error. The default normalized project config is:

```json
{
  "fmt": {},
  "lint": {}
}
```

When `--config <path>` is provided and that file does not exist, the CLI returns an operational config error with `code: "config_not_found"` and exits with code `2`.

The unified config JSON Schema is the schema that users and editors should reference. Formatter and linter config models can be defined independently under the unified root schema, but users should not need separate top-level schemas for one project config file.

The unified config JSON Schema is published with the public `@intlify/cli` wrapper package. The schema is exported at `./schema/config.schema.json` from that package and can internally separate formatter and linter configuration under definitions such as `$defs.fmt` and `$defs.lint`. Formatter and linter detail schemas remain separately owned, but users reference one schema for `intlify.config.json`. Native packages may contain internal implementation artifacts, but they do not define a public config schema path.

Schema generation uses Rust config types as the source of truth. Phase 3A should generate the unified schema from the project-level Rust config model, using schema annotations for editor-facing descriptions and examples where needed. A dedicated schema generation crate is not required in Phase 3A.

The generated `packages/cli/schema/config.schema.json` file is committed to the repository because it is a public artifact. Schema generation tests or CI checks should verify that regenerating the schema from Rust config types produces the committed file. Any Rust config-model change that affects the public schema must update the committed schema in the same change.

The root-level `$schema` field is allowed as metadata for editor completion and validation. It is accepted by validation but is not passed into the resolved config model.

The recommended `$schema` value for a root `intlify.config.json` is:

```json
{
  "$schema": "./node_modules/@intlify/cli/schema/config.schema.json",
  "fmt": {},
  "lint": {}
}
```

The `$schema` field is optional. The CLI does not use the `$schema` value to locate its validation schema at runtime; it is editor-facing metadata only.

Unknown fields are validation errors at the root level, except for `$schema`, and inside `fmt` and `lint` sections. This keeps typo detection strict; future configuration fields should be added through explicit schema and config-model updates.

In Phase 3A, `fmt` and `lint` must be objects and only empty objects are valid product configs. Product-specific formatter and linter options are not accepted until Phase 3B and Phase 3C add explicit schema and config-model fields.

Phase 3A config error codes:

- `config_not_found`: explicit `--config <path>` does not exist
- `config_read_failed`: config exists but cannot be read because of permissions or IO failures
- `config_parse_failed`: config cannot be parsed as JSON
- `config_validation_failed`: config parses as JSON but fails schema or config-model validation
- `config_schema_generation_failed`: config schema generation fails in a build or validation workflow

Open product-specific config details remain in the formatter and linter design documents.

## Machine-Readable Output

Machine-readable CLI output should use JSON and should be stable enough for CI, editor adapters, and agent coding workflows to consume.

The config schema and output schemas are separate surfaces. `lint`, `fmt --check`, and future combined `check` output may use command-specific JSON result schemas while sharing common conventions where practical.

Phase 3A publishes only the config JSON Schema. The output envelope remains documented and fixture-tested, but no public output JSON Schema is published while `schemaVersion` is `"0"`. Publishing output schemas should be reconsidered in Phase 3B or Phase 3C after command-specific result shapes become clearer.

The initial shared top-level JSON envelope uses `schemaVersion: "0"` while the output contract remains pre-stable. It contains:

- `schemaVersion`: output contract version
- `command`: command that produced the result, such as `fmt`, `lint`, or future `check`
- `version`: CLI/package version
- `projectRoot`: discovered project root
- `summary`: command-level aggregate status and optional command-specific counts
- `results`: command-specific file, message, diagnostic, or formatting results
- `errors`: operational errors separated from parser, formatter, and linter diagnostics

Command-specific result schemas should preserve deterministic ordering and stable command/version metadata through this envelope.

The shared `summary.status` values are:

- `success`: successful execution, corresponding to exit code `0`
- `failure`: command executed successfully but reported a check, lint, or formatting failure, corresponding to exit code `1`
- `error`: operational error, corresponding to exit code `2`

In Phase 3A, only `summary.status` is required in the shared envelope. Command-specific count fields are not defined until formatter, linter, or combined check result schemas are defined in Phase 3B or Phase 3C.

The `projectRoot` field is the discovered project root: the git repository root when available, otherwise the process `cwd`. It is an absolute path and is slash-normalized in machine-readable output. File paths inside command results or operational errors are relative to `projectRoot` and also use `/` separators on every platform. The `results` and `errors` fields are always arrays, even when empty.

If a path cannot be represented relative to `projectRoot`, such as an explicit `--config` path outside the project root, machine-readable output may use an absolute slash-normalized path for that field. No extra boolean is added to distinguish relative and absolute paths; consumers can determine that from the path string.

The envelope `command` field is the resolved command name when a subcommand is known: `fmt`, `lint`, or `check`. If no subcommand is resolved, if an invalid top-level argument prevents command resolution, or if a wrapper-level native resolution error occurs before the Rust CLI starts, `command` is `"intlify"`. Unknown command tokens are reported in `errors[].details` while keeping `command: "intlify"`. The envelope does not use `null` or `"unknown"` for the command field.

Reserved command placeholder JSON output uses the same envelope. For example, `intlify fmt --reporter json` returns:

```json
{
  "schemaVersion": "0",
  "command": "fmt",
  "version": "0.14.0",
  "projectRoot": "/repo",
  "summary": {
    "status": "error"
  },
  "results": [],
  "errors": [
    {
      "kind": "unsupported",
      "code": "command_not_ready",
      "message": "The fmt command is reserved but not available in this release.",
      "details": {
        "phase": "3A",
        "requiredPhase": "3B"
      }
    }
  ]
}
```

For `lint`, `details.requiredPhase` is `"3C"`. For `check`, `details.requiredPhase` is `"3B+3C"`.

Operational errors are represented only in the top-level `errors` array. They are CLI execution failures rather than parser, formatter, or linter diagnostics.

CLI operational error codes are stable string identifiers scoped to the CLI JSON output contract. They are intentionally separate from the numeric `OxMf2ErrorCode` API namespace defined in [appendix-ox-mf2-error-code.md](./appendix-ox-mf2-error-code.md). CLI operational failures may wrap lower-level API errors later, but the top-level CLI `errors[].code` field remains a string.

Each operational error contains:

- `kind`: broad error group, such as `config`, `input`, `io`, `reporter`, `unsupported`, or `internal`
- `code`: stable machine-readable error code
- `message`: human-readable message
- `path`: optional related file path
- `details`: optional structured data for integrations

If an unsupported reporter is requested, the CLI returns an operational reporter error with `kind: "reporter"`, `code: "reporter_not_supported"`, and exit code `2`. Its `details` object contains the requested `reporter` value and `supportedReporters: ["text", "json"]`.

If the Rust CLI can parse `--reporter json` before rejecting invalid command-line arguments, invalid arguments are reported as a JSON envelope with `kind: "input"`, `code: "invalid_cli_argument"`, and exit code `2`. If argument parsing fails before reporter selection can be determined, the CLI falls back to a human-readable stderr error and exits with code `2`.

Human-readable text output can optimize for users, but integrations should use `--reporter json` when they need to inspect diagnostics or formatting status. Phase 3A supports only `text` and `json` reporter names. The reporter name leaves room for future human-readable or integration-specific reporters without overloading formatter terminology.

Output streams:

- With `--reporter json`, the JSON envelope is written to stdout and no human-readable log is emitted.
- With the human-readable reporter, normal results and summaries are written to stdout.
- Human-readable operational errors are written to stderr.
- `--version` and `--help` write to stdout.
- Invalid CLI arguments write to stderr and exit with code `2` unless `--reporter json` can be parsed before the argument error is reported.
- Wrapper-level native resolution failures may fall back to a minimal human-readable stderr message when the Rust CLI cannot be started and a JSON envelope cannot be produced.

Exit codes:

- `0`: success, including passing check-style commands
- `1`: check failure, such as lint diagnostics, format mismatch, or future combined `check` failure
- `2`: operational error, such as config errors, IO errors, invalid CLI arguments, or unsupported reporters

If check failures and operational errors both occur, the CLI exits with `2`.

## Package Boundaries

Phase 3A should define package boundaries without forcing all packages to exist immediately.

Expected package groups:

- CLI package: `@intlify/cli`, distributing the `intlify` command as a wrapper package.
- CLI native packages: platform-specific optional packages that contain the compiled native `intlify` binary.
- Formatter packages: future formatter-specific N-API and WASM APIs.
- Linter packages: future linter-specific N-API and WASM APIs.

Initial package directories:

- `packages/cli`: `@intlify/cli`
- `packages/cli-darwin-x64`: `@intlify/cli-darwin-x64`
- `packages/cli-darwin-arm64`: `@intlify/cli-darwin-arm64`
- `packages/cli-linux-x64-gnu`: `@intlify/cli-linux-x64-gnu`
- `packages/cli-linux-arm64-gnu`: `@intlify/cli-linux-arm64-gnu`
- `packages/cli-linux-x64-musl`: `@intlify/cli-linux-x64-musl`
- `packages/cli-win32-x64-msvc`: `@intlify/cli-win32-x64-msvc`

`@intlify/cli` should resolve the current platform's optional native package and execute that package's binary. This keeps the public npm entry point stable while avoiding a single package that ships every platform binary. The platform package model should follow the same general direction as the existing native ox-mf2 package publishing flow.

Wrapper execution contract:

- Native binary file names are `intlify` on Unix platforms and `intlify.exe` on Windows.
- The native binary is stored at the root of each native package.
- The wrapper passes `process.argv.slice(2)` through unchanged.
- The wrapper forwards stdin, stdout, stderr, and `process.env` to the native process.
- The wrapper exits with the native process exit code.
- The wrapper forwards process termination signals to the native process where the host platform allows it.
- The wrapper owns output only for wrapper-level native resolution failures.

Platform resolution table:

| Runtime platform | Runtime arch | Runtime libc | Native package                 |
| ---------------- | ------------ | ------------ | ------------------------------ |
| `darwin`         | `x64`        | n/a          | `@intlify/cli-darwin-x64`      |
| `darwin`         | `arm64`      | n/a          | `@intlify/cli-darwin-arm64`    |
| `linux`          | `x64`        | `glibc`      | `@intlify/cli-linux-x64-gnu`   |
| `linux`          | `arm64`      | `glibc`      | `@intlify/cli-linux-arm64-gnu` |
| `linux`          | `x64`        | `musl`       | `@intlify/cli-linux-x64-musl`  |
| `win32`          | `x64`        | n/a          | `@intlify/cli-win32-x64-msvc`  |

Unsupported platform, architecture, or libc combinations return `native_platform_unsupported`. Linux libc detection is performed by the wrapper. If libc detection fails, the wrapper reports `native_platform_unsupported` rather than guessing.

Initial CLI native package names:

- `@intlify/cli-darwin-x64`
- `@intlify/cli-darwin-arm64`
- `@intlify/cli-linux-x64-gnu`
- `@intlify/cli-linux-arm64-gnu`
- `@intlify/cli-linux-x64-musl`
- `@intlify/cli-win32-x64-msvc`

Future native package candidates:

- `@intlify/cli-win32-arm64-msvc`
- `@intlify/cli-linux-arm64-musl`

Wrapper-level native resolution error codes:

- `native_platform_unsupported`: the current platform is not in the supported platform table
- `native_package_not_found`: the platform is supported but the optional native package cannot be resolved
- `native_binary_not_found`: the native package resolves but the expected binary path does not exist
- `native_binary_failed`: the native binary exists but cannot be spawned or executed

These are operational errors and use exit code `2`. The wrapper should parse only the minimum command-line surface needed to detect `--reporter json` for native resolution failures. When `--reporter json` is detected and the wrapper can construct the standard JSON envelope safely, it should write the native resolution error to stdout in `errors`. Otherwise, it should print a minimal human-readable stderr fallback.

The wrapper's minimal reporter parser detects only `--reporter json` and `--reporter=json`. If either JSON reporter form appears anywhere in argv, wrapper-level native resolution failures use the JSON envelope. The wrapper does not validate duplicate reporter options, missing reporter values, unknown options, or unsupported reporter values. If the Rust CLI cannot be started and no JSON reporter form is present, wrapper-level native resolution failures use the human-readable stderr fallback.

Once the native binary is spawned successfully, wrapper-level native resolution is complete. The wrapper forwards the Rust CLI process exit code unchanged, including operational error exits and unexpected non-zero exits. Those exits are not reported as `native_binary_failed`.

For wrapper-level native resolution failures, the wrapper does not perform git-root discovery. If it emits a JSON envelope, `projectRoot` is the absolute slash-normalized `process.cwd()` value and the native resolution error includes `details.projectRootSource: "cwd-fallback"`. Normal Rust CLI execution uses the standard git-root discovery contract.

`@intlify/cli` already exists as a standalone package and repository. Phase 3A treats this monorepo as the future source of truth for `@intlify/cli`; the standalone `intlify/cli` repository should be deprecated as part of the migration. Because the existing package has already reached `v0.13.1`, the first monorepo-managed `@intlify/cli` release is `0.14.0`.

`packages/cli/package.json` contract:

- `name`: `@intlify/cli`
- `version`: monorepo release version; `0.14.0` for the first monorepo-managed release
- `type`: `module`
- `bin`: `{ "intlify": "./bin/intlify.mjs" }`
- `files`: `["bin", "schema", "README.md", "package.json"]`
- `optionalDependencies`: all initial native packages, pinned to the same exact version as `@intlify/cli`
- `engines.node`: `>=22.12.0`

Native package `package.json` contract:

- `name`: `@intlify/cli-<target>`
- `version`: same exact version as `@intlify/cli`
- `files` on Unix targets: `["intlify", "README.md", "package.json"]`
- `files` on Windows targets: `["intlify.exe", "README.md", "package.json"]`
- no `engines` field is required
- no `bin` entry; the public command is exposed only by `@intlify/cli`

Linux libc selection is represented by package name and wrapper resolution logic rather than npm package metadata.

Native package `os` and `cpu` matrix:

| Native package                 | `os`         | `cpu`       |
| ------------------------------ | ------------ | ----------- |
| `@intlify/cli-darwin-x64`      | `["darwin"]` | `["x64"]`   |
| `@intlify/cli-darwin-arm64`    | `["darwin"]` | `["arm64"]` |
| `@intlify/cli-linux-x64-gnu`   | `["linux"]`  | `["x64"]`   |
| `@intlify/cli-linux-arm64-gnu` | `["linux"]`  | `["arm64"]` |
| `@intlify/cli-linux-x64-musl`  | `["linux"]`  | `["x64"]`   |
| `@intlify/cli-win32-x64-msvc`  | `["win32"]`  | `["x64"]`   |

Build and package assembly pipeline:

- Build the Rust CLI with `cargo build --release -p ox_mf2_cli --bin intlify`.
- Run cross-target binary builds in the GitHub Actions release matrix.
- Copy the built binary into the matching native package root as `intlify` or `intlify.exe`.
- Generate `config.schema.json` from Rust config types and write it to the committed `packages/cli/schema/config.schema.json` artifact.
- Expose schema generation as `vp run cli#schema`.
- Expose schema verification as `vp run cli#schema:check`, comparing regenerated schema output with the committed schema artifact.
- Expose root scripts `schema:cli` and `schema:cli:check` as wrappers around those Vite Task commands.
- Validate version consistency across `packages/cli/package.json`, every `packages/cli-<target>/package.json`, `crates/ox_mf2_cli/Cargo.toml`, and the monorepo release version.
- Validate package contents with `npm pack --dry-run` or the equivalent release-pack step.
- Run release-time installed-package smoke tests for `intlify --version`, reserved command placeholder behavior, native package resolution, and schema file presence. Do not use npm lifecycle `postinstall` for smoke testing in user environments.

Parser binding packages remain focused on parsing, snapshots, and parser-level APIs. Formatter and linter APIs should not be folded into parser packages.

## Validation

Phase 3A validation should focus on foundation behavior:

- config discovery and parsing through crate-level unit/integration tests
- default config behavior when no root config exists through crate-level unit/integration tests
- explicit `--config` missing-file errors through crate-level unit/integration tests
- config error code fixtures through crate-level unit/integration tests
- unknown-field validation through crate-level unit/integration tests
- config schema generation through crate-level unit/integration tests
- config validation fixtures through crate-level unit/integration tests
- CLI help/version behavior
- reserved command placeholder behavior
- release-time installed-package smoke tests for CLI wrapper/native resolution
- native package resolution error handling
- output envelope fixtures
- slash-normalized `projectRoot` and result paths
- stdout/stderr behavior
- deterministic JSON ordering
- operational error shape

Formatter and linter semantic correctness tests belong to later product phases.

Phase 3A does not add hidden internal CLI commands just to exercise config loading. Reserved `fmt`, `lint`, and `check` commands stop at `command_not_ready`, so config loader behavior is verified directly at the crate level rather than through public CLI execution.

## Deferred Follow-Up Notes

The following items are intentionally not delivered in Phase 3A, but should remain visible for later implementation phases:

- Formatter and linter engines remain follow-up products. Phase 3A only reserves `intlify fmt`, `intlify lint`, and `intlify check` and returns placeholder operational errors for those commands.
- User-visible `intlify check` behavior is deferred until both Phase 3B formatter and Phase 3C linter products exist.
- Formatter-specific option names, defaults, layout rules, ignore directive behavior, and formatter result schemas belong to [007-ox-mf2-phase-3b-formatter-design.md](./007-ox-mf2-phase-3b-formatter-design.md).
- Linter-specific rule semantics, presets, include/exclude behavior, ignore behavior, severity policy details, and diagnostic result schemas belong to [008-ox-mf2-phase-3c-linter-design.md](./008-ox-mf2-phase-3c-linter-design.md).
- Command-specific JSON result schemas for `fmt --check`, `lint`, and combined `check` are deferred to the product phases. Phase 3A owns only the shared envelope and operational error shape.
- Formatter and linter N-API/WASM packages are deferred to their product phases. Phase 3A only records package boundaries and keeps parser binding packages focused on parser-level APIs.
- Resource/catalog parsing, host-file escaping, outer document edits, and resource-level linting or formatting remain layered workflows outside the Phase 3A CLI foundation.
- LSP/editor adapters, agent integrations, and MessagePack or daemon transport remain later consumers of this foundation.
- Nested config discovery, nearest-config-wins behavior, file-specific config overrides, `--cwd`, and `--root` remain out of scope until a concrete multi-workspace or adapter requirement appears.
- Additional native package targets such as `@intlify/cli-win32-arm64-msvc` and `@intlify/cli-linux-arm64-musl` are future candidates, not initial Phase 3A requirements.

## Open Questions

No unresolved Phase 3A foundation questions remain in this document. Product-specific formatter, linter, LSP/editor, and agent questions are tracked in their dedicated design documents.
