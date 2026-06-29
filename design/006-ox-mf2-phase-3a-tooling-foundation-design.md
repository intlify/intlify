# ox-mf2 Phase 3A Tooling Foundation Design

## Purpose

This document tracks the detailed design for Phase 3A: Tooling Foundation.

The broader Phase 3 tooling and consumer boundary is defined in [005-ox-mf2-phase-3-tooling-transport-design.md](./005-ox-mf2-phase-3-tooling-transport-design.md). That document splits implementation into consumer-facing product phases. This document covers the first phase: the shared CLI, configuration, machine-readable output, and distribution foundation needed before formatter, linter, LSP/editor, agent, or long-lived transport products are implemented.

## Goals

- Establish the Phase 3A CLI crate and the `intlify` command structure.
- Define the shared CLI package and native binary distribution boundary.
- Define the unified project configuration model with `format` and `lint` sections.
- Publish a unified JSON Schema for editor completion and config validation.
- Define shared machine-readable output conventions for future `format --check`, `lint`, and combined `check` workflows.
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

## Ownership

The CLI crate owns command routing, config discovery, config loading, output formatting, exit-code behavior, and package-level CLI composition.

Formatter and linter crates own their resolved config models once Phase 3B and Phase 3C begin. Phase 3A owns only the project-level config envelope, schema generation boundary, and normalization path that lets the CLI pass `format` and `lint` sections to product-specific crates later.

Parser crates remain responsible for parsing, diagnostics, Binary AST snapshots, and semantic lowering. Phase 3A should not move parser behavior into the CLI crate.

## CLI Surface

The initial CLI should reserve the user-facing command shape without requiring all product implementations to exist immediately:

```text
intlify format
intlify lint
intlify check
```

Phase 3A may implement placeholders or hidden/internal command scaffolding as needed, but user-visible incomplete commands should not appear as stable behavior until their product phase is ready. `intlify check` remains hidden/internal scaffolding until formatter and linter products both exist.

The CLI should provide consistent global behavior for help output, version output, config path handling, machine-readable output selection, and operational errors. Machine-readable JSON output is selected with `--reporter json`.

## Configuration Contract

Project configuration is one JSON config with separate `format` and `lint` sections. The config file name is `intlify.config.json`.

The initial config discovery model is root-only. Nested config discovery, nearest-config-wins behavior, and file-specific overrides are deferred until a concrete multi-workspace or resource/catalog requirement appears.

The CLI supports an explicit `--config <path>` option in Phase 3A. This is an escape hatch for CI, fixtures, and integrations, not nested discovery. When `--config` is provided, the CLI loads that exact file instead of the root `intlify.config.json`.

The unified config JSON Schema is the schema that users and editors should reference. Formatter and linter config models can be defined independently under the unified root schema, but users should not need separate top-level schemas for one project config file.

The unified config JSON Schema is published with the native CLI npm package. The schema is exported at `./schema/config.schema.json` and can internally separate formatter and linter configuration under definitions such as `$defs.format` and `$defs.lint`. Formatter and linter detail schemas remain separately owned, but users reference one schema for `intlify.config.json`.

Schema generation uses Rust config types as the source of truth. Phase 3A should generate the unified schema from the project-level Rust config model, using schema annotations for editor-facing descriptions and examples where needed. A dedicated schema generation crate is not required in Phase 3A.

Open product-specific config details remain in the formatter and linter design documents.

## Machine-Readable Output

Machine-readable CLI output should use JSON and should be stable enough for CI, editor adapters, and agent coding workflows to consume.

The config schema and output schemas are separate surfaces. `lint`, `format --check`, and future combined `check` output may use command-specific JSON result schemas while sharing common conventions where practical.

The shared top-level JSON envelope contains:

- `schemaVersion`: output contract version
- `command`: command that produced the result, such as `format`, `lint`, or future `check`
- `version`: CLI/package version
- `cwd`: path resolution base
- `summary`: command-level aggregate status and counts
- `results`: command-specific file, message, diagnostic, or formatting results
- `errors`: operational errors separated from parser, formatter, and linter diagnostics

Command-specific result schemas should preserve deterministic ordering and stable command/version metadata through this envelope.

Operational errors are represented only in the top-level `errors` array. They are CLI execution failures rather than parser, formatter, or linter diagnostics.

Each operational error contains:

- `kind`: broad error group, such as `config`, `input`, `io`, `reporter`, or `internal`
- `code`: stable machine-readable error code
- `message`: human-readable message
- `path`: optional related file path
- `details`: optional structured data for integrations

Human-readable text output can optimize for users, but integrations should use `--reporter json` when they need to inspect diagnostics or formatting status. The reporter name leaves room for future human-readable or integration-specific reporters without overloading formatter terminology.

Exit codes:

- `0`: success, including passing check-style commands
- `1`: check failure, such as lint diagnostics, format mismatch, or future combined `check` failure
- `2`: operational error, such as config errors, IO errors, invalid CLI arguments, or unsupported reporters

If check failures and operational errors both occur, the CLI exits with `2`.

## Package Boundaries

Phase 3A should define package boundaries without forcing all packages to exist immediately.

Expected package groups:

- CLI package: `@intlify/cli`, distributing the compiled native `intlify` binary.
- Formatter packages: future formatter-specific N-API and WASM APIs.
- Linter packages: future linter-specific N-API and WASM APIs.

`@intlify/cli` already exists as a standalone package and repository. Phase 3A treats this monorepo as the future source of truth for `@intlify/cli`; the standalone `intlify/cli` repository should be deprecated as part of the migration. Because the existing package has already reached `v0.13.1`, the first monorepo-managed `@intlify/cli` release must not publish a version lower than `0.13.1`.

Parser binding packages remain focused on parsing, snapshots, and parser-level APIs. Formatter and linter APIs should not be folded into parser packages.

## Validation

Phase 3A validation should focus on foundation behavior:

- config discovery and parsing
- config schema generation
- config validation fixtures
- CLI help/version behavior
- native CLI package install and binary execution smoke tests
- output envelope fixtures
- deterministic JSON ordering
- operational error shape

Formatter and linter semantic correctness tests belong to later product phases.

## Open Questions

No unresolved Phase 3A foundation questions remain in this document. Product-specific formatter, linter, LSP/editor, and agent questions are tracked in their dedicated design documents.
