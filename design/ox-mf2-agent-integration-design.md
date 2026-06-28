# ox-mf2 Agent Integration Design

This document tracks detailed integration design for agent coding tools that consume ox-mf2.

The Phase 3 tooling boundary is defined in [005-ox-mf2-phase-3-tooling-transport-design.md](./005-ox-mf2-phase-3-tooling-transport-design.md). That document fixes the high-level product boundary. This document is the implementation-facing place to refine Codex, Claude Code, Grok Build, and other agent coding workflows.

## Goals

- Let agent coding tools use ox-mf2 parser, formatter, linter, and snapshot workflows without depending on editor-specific APIs.
- Treat the `ox-mf2` CLI and stable machine-readable output as the first agent-facing integration surface.
- Keep agent plugins, skills, commands, hooks, MCP servers, and ACP clients as wrappers around the shared core contracts.
- Make diagnostics, formatting results, config errors, and operational errors easy for agents to parse and summarize.
- Preserve one source of truth for formatting rules, lint diagnostics, configuration semantics, AST structure, and semantic analysis.

## Non-Goals

- Making a Codex, Claude Code, Grok Build, or other agent plugin a direct Phase 3 product.
- Adding agent-specific types to parser, formatter, or linter result objects.
- Replacing the CLI/API contracts with agent-specific command protocols.
- Defining an MCP server or ACP server as the first implementation requirement.
- Depending on one vendor's plugin or skill format as the canonical ox-mf2 integration model.

## Initial Workflow

The initial agent workflow should use the same CLI and JSON contracts designed for humans, CI, and editor adapters.

Agents can run:

- `ox-mf2 lint` to collect parser, semantic, and lint diagnostics
- `ox-mf2 format --check` to detect formatting drift
- `ox-mf2 format` to apply formatting when the task explicitly allows edits
- future `ox-mf2` check commands when formatter and linter workflows are composed

Machine-readable output should be stable enough for agents to identify affected files, source spans, diagnostic categories, rule ids, severities, and suggested follow-up actions.

## Integration Shapes

Agent integrations may be distributed in several forms:

- repo instructions that tell agents which CLI commands to run
- skills or command packs that wrap common ox-mf2 workflows
- hooks that enforce format or lint checks around agent edits
- MCP servers that expose parse, lint, format, or diagnostic query tools
- agent plugins that bundle skills, commands, hooks, MCP config, or documentation
- headless scripts for CI, review, release, and migration workflows

These integration shapes should call shared CLI, Rust, N-API, WASM, or future MCP APIs. They should not implement separate MF2 parsing, semantic lowering, formatting, or linting logic.

## Open Questions

- Which CLI JSON output fields are required for reliable agent consumption across lint, format-check, and combined check workflows?
- Should `ox-mf2` provide a single `check` command that agents can call instead of coordinating formatter and linter commands themselves?
- Which integration shape should be implemented first after the CLI: repo instructions, agent skill, MCP server, or plugin bundle?
- Should an MCP server expose high-level tools such as `lint_file` and `format_file`, or lower-level tools such as `parse_message`, `lint_message`, and `format_message`?
- How should agent integrations report operational errors separately from parser, semantic, formatter, and linter diagnostics?
- How should agent hooks avoid modifying files unexpectedly when the user's agent workflow is read-only or review-only?
- Should agent integrations support resource/catalog-aware workflows before the formatter and linter core own those layers?
- How should Codex, Claude Code, Grok Build, and future agent plugin formats be documented without making one vendor's format canonical?
