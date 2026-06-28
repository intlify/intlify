# ox-mf2 Phase 3A Tooling Foundation Design

## Purpose

This document tracks the detailed design for Phase 3A: Tooling Foundation.

The broader Phase 3 tooling and consumer boundary is defined in [005-ox-mf2-phase-3-tooling-transport-design.md](./005-ox-mf2-phase-3-tooling-transport-design.md). That document splits implementation into consumer-facing product phases. This document covers the first phase: the shared CLI, configuration, machine-readable output, and distribution foundation needed before formatter, linter, LSP/editor, agent, or long-lived transport products are implemented.

## Goals

- Establish the `ox-mf2` CLI crate and command structure.
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
- `ox-mf2` CLI command structure
- native CLI npm package boundary
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
ox-mf2 format
ox-mf2 lint
ox-mf2 check
```

Phase 3A may implement placeholders or hidden/internal command scaffolding as needed, but user-visible incomplete commands should not appear as stable behavior until their product phase is ready.

The CLI should provide consistent global behavior for help output, version output, config path handling, machine-readable output selection, and operational errors.

## Configuration Contract

Project configuration is one JSON config with separate `format` and `lint` sections.

The initial config discovery model is root-only. Nested config discovery, nearest-config-wins behavior, and file-specific overrides are deferred until a concrete multi-workspace or resource/catalog requirement appears.

The unified config JSON Schema is the schema that users and editors should reference. Formatter and linter config models can be defined independently under the unified root schema, but users should not need separate top-level schemas for one project config file.

Open product-specific config details remain in the formatter and linter design documents.

## Machine-Readable Output

Machine-readable CLI output should use JSON and should be stable enough for CI, editor adapters, and agent coding workflows to consume.

The config schema and output schemas are separate surfaces. `lint`, `format --check`, and future combined `check` output may use command-specific JSON result schemas while sharing common conventions where practical:

- top-level summary
- file or message grouping
- operational error separation
- deterministic ordering
- stable command and version metadata

Human-readable text output can optimize for users, but integrations should use JSON output when they need to inspect diagnostics or formatting status.

## Package Boundaries

Phase 3A should define package boundaries without forcing all packages to exist immediately.

Expected package groups:

- CLI package: distributes the compiled native `ox-mf2` binary.
- Formatter packages: future formatter-specific N-API and WASM APIs.
- Linter packages: future linter-specific N-API and WASM APIs.

Parser binding packages remain focused on parsing, snapshots, and parser-level APIs. Formatter and linter APIs should not be folded into parser packages.

## Validation

Phase 3A validation should focus on foundation behavior:

- config discovery and parsing
- config schema generation
- config validation fixtures
- CLI help/version behavior
- output envelope fixtures
- deterministic JSON ordering
- operational error shape

Formatter and linter semantic correctness tests belong to later product phases.

## Open Questions

- What is the exact config file name?
- Should the CLI support an explicit `--config <path>` option in Phase 3A?
- What global option selects JSON output: `--format json`, `--output json`, or another spelling?
- Should `ox-mf2 check` appear in user-facing help as a reserved command in Phase 3A, or remain hidden/internal scaffolding until formatter and linter products both exist?
- What top-level fields are required in every JSON output envelope?
- How should operational errors be represented in JSON output without mixing them with parser, formatter, or linter diagnostics?
- What exit-code contract should apply to operational errors versus check failures?
- How should the unified config JSON Schema be generated and published with npm packages?
- Should schema generation happen from Rust types, hand-authored schema files, or a dedicated schema generation crate?
- What package name should distribute the native CLI binary?
- Should Phase 3A include smoke tests for package install and binary execution before formatter/linter commands exist?
