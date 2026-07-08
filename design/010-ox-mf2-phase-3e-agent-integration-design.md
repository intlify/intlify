# ox-mf2 Phase 3E Agent Integration Design

This document tracks detailed integration design for agent coding tools that consume ox-mf2.

The Phase 3 tooling boundary is defined in [005-ox-mf2-phase-3-tooling-transport-design.md](./005-ox-mf2-phase-3-tooling-transport-design.md). That document fixes the high-level product boundary. This document is the implementation-facing place to refine Codex, Claude Code, Grok Build, and other agent coding workflows.

## Goals

- Let agent coding tools use ox-mf2 parser, formatter, linter, and snapshot workflows without depending on editor-specific APIs.
- Treat the `intlify` CLI and stable machine-readable output as the first agent-facing integration surface.
- Keep agent plugins, skills, commands, hooks, MCP servers, and ACP clients as wrappers around the shared core contracts.
- Make diagnostics, formatting results, config errors, and operational errors easy for agents to parse and summarize.
- Preserve one source of truth for formatting rules, lint diagnostics, configuration semantics, AST structure, parser-owned semantic validation, and linter result contracts.

## Non-Goals

- Making a Codex, Claude Code, Grok Build, or other agent plugin a direct Phase 3 product.
- Adding agent-specific types to parser, formatter, or linter result objects.
- Replacing the CLI/API contracts with agent-specific command protocols.
- Defining an MCP server or ACP server as the first implementation requirement.
- Depending on one vendor's plugin or skill format as the canonical ox-mf2 integration model.

## Initial Workflow

The initial agent workflow should use the same CLI and JSON contracts designed for humans, CI, and editor adapters.

Agents can run:

- `intlify lint` to collect parser, semantic, and lint diagnostics
- `intlify fmt --check` to detect formatting drift
- `intlify fmt` to apply formatting when the task explicitly allows edits
- future `intlify` check commands when formatter and linter workflows are composed

Machine-readable output should be stable enough for agents to identify affected files, source spans, diagnostic `category`, diagnostic `code`, and `severity`. For configurable lint diagnostics, `code` is the rule id. Suggested follow-up actions are agent-derived helper output built from diagnostics, rule documentation, and workflow context; the initial formatter and linter core contracts do not expose a fix or suggestion API.

Lint result contracts, diagnostic codes, reporter behavior, and operational error separation are owned by [008-ox-mf2-phase-3c-linter-design.md](./008-ox-mf2-phase-3c-linter-design.md). Parser-owned semantic diagnostic behavior is owned by [012-ox-mf2-parser-semantic-validation-design.md](./012-ox-mf2-parser-semantic-validation-design.md). Agent integrations should consume those contracts instead of inferring lint or semantic behavior from human-readable output.

Agent integrations should use [linter-rules/index.md](./linter-rules/index.md) as the rule documentation entry point when they need rule descriptions, docs slugs, or user-facing remediation context. They may summarize or link those docs for a specific environment, but they should not invent a separate rule catalog.

## Integration Shapes

Agent integrations may be distributed in several forms:

- repo instructions that tell agents which CLI commands to run
- skills or command packs that wrap common ox-mf2 workflows
- hooks that enforce format or lint checks around agent edits
- MCP servers that expose parse, lint, format, or diagnostic query tools
- agent plugins that bundle skills, commands, hooks, MCP config, or documentation
- headless scripts for CI, review, release, and migration workflows

These integration shapes should call shared CLI, Rust, N-API, WASM, or future MCP APIs. They should not implement separate MF2 parsing, SemanticModel construction, parser-owned semantic validation, formatting, or linting logic.

## Open Questions

- Which CLI JSON output fields are required for reliable agent consumption across lint, format-check, and combined check workflows?
- Should `intlify` provide a single `check` command that agents can call instead of coordinating formatter and linter commands themselves?
- Which integration shape should be implemented first after the CLI: repo instructions, agent skill, MCP server, or plugin bundle?
- Should an MCP server expose high-level tools such as `lint_file` and `format_file`, or lower-level tools such as `parse_message`, `lint_message`, and `format_message`?
- How should agent integrations summarize and present existing JSON envelope `errors[]` separately from parser, semantic, formatter, and linter diagnostics without redefining their schemas?
- How should agent hooks avoid modifying files unexpectedly when the user's agent workflow is read-only or review-only?
- Should agent integrations support resource/catalog-aware workflows before the formatter and linter core own those layers?
- How should Codex, Claude Code, Grok Build, and future agent plugin formats be documented without making one vendor's format canonical?
