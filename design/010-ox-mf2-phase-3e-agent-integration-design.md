# ox-mf2 Phase 3E Agent Integration Design

This document tracks detailed integration design for agent coding tools that consume ox-mf2.

The Phase 3 tooling boundary is defined in [005-ox-mf2-phase-3-tooling-transport-design.md](./005-ox-mf2-phase-3-tooling-transport-design.md). That document fixes the high-level product boundary. This document is the implementation-facing place to refine Codex, Claude Code, Grok Build, and other agent coding workflows.

## Status

Phase 3E implementation is not scheduled yet, and the remaining agent-integration design work is intentionally pending. The shared CLI JSON consumption profile recorded below is the accepted baseline already established from the formatter, linter, and resource contracts. It does not schedule repo instructions, a skill, plugin, hook, MCP/ACP server, or another agent-facing deliverable. Product shape and integration-specific design resume only when Phase 3E is explicitly scheduled; the pending decisions are collected under [Deferred Follow-Up Notes](#deferred-follow-up-notes).

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

The v0.1 workflow requires agents to coordinate `intlify lint` and `intlify fmt --check`; it does not provide a combined `intlify check` command. The `check` name remains reserved, and a dedicated post-v0.1 addendum may define it after the formatter and linter reporters and exit behavior are implemented contracts.

## Agent Consumption Profile

Agent integrations consume a stable subset of the existing `--reporter json` contracts rather than a new reporter or agent-specific schema. Stability is scoped by the top-level `schemaVersion`: an integration must recognize the version before interpreting the payload, reject an unsupported version instead of guessing, and ignore unknown object fields within a supported version. While `schemaVersion` remains `"0"`, this profile is the design and fixture contract for repository integrations, not a claim of post-1.0 compatibility.

Every consumable formatter or linter envelope requires these profile fields:

- `schemaVersion`, `command`, `version`, and `projectRoot`
- `summary.status`
- array-valued `results` and top-level `errors`

`projectRoot` follows the shared nullable pre-project-error rule. Command-specific summary fields such as `operation`, mode, and counts remain available and should be used when needed, but an agent must not require a counter that the command contract omits for a pre-target failure. A profile consumer uses `summary.status` and operational errors rather than deriving process success solely from whether `results` is empty.

Every standalone target result requires `path`, `status`, `diagnostics`, and `errors`. Formatter results additionally require `changed`. A catalog target uses the resource-owned mutually exclusive variant: it requires `path`, aggregate `status`, `entries`, and `errors`, omits file-level `diagnostics`, and additionally requires aggregate `changed` for formatter output. Every successful catalog entry requires the structured `key` identity `{ path, occurrence }`, `status`, and `diagnostics`; formatter entries also require `changed` and `readOnly`. Entry results do not grow an `errors` array. An incomplete catalog operational failure remains a file-level error with an empty `entries` array and uses `details.entryKey` when the failed entry is known.

Every diagnostic in the profile requires:

- `category`, stable `code`, and `severity`
- primary UTF-8 byte `span`
- `location`, which may be `null` under the diagnostic contract
- human-readable `message`
- array-valued `labels`, whose entries retain their span and display message

Diagnostic and label message wording is display text and may evolve. Agents branch on `category`, `code`, `severity`, and structured locations, never on message substrings. Catalog diagnostic spans and locations are already mapped to complete host-document coordinates; an agent must not reinterpret them as message-local offsets.

Every operational error requires `kind`, stable `code`, and human-readable `message`, with optional top-level `path` and the code-specific stable `details` contract. Agents branch first on `code` and, when defined, `details.reason`; they do not parse message prose, dependency names, Rust debug output, or platform-specific text. Top-level errors are command-global, while `results[].errors` are target-local. The same error must not be counted or presented twice merely because both locations are inspected.

The profile adds no `suggestion`, `fix`, documentation URL, agent instruction, or hidden source-text field. An agent may derive a proposed follow-up from stable codes, source context it is separately authorized to read, rule documentation, and workflow state, but that derived advice is not CLI data. A future combined `intlify check` command is outside this profile until its dedicated addendum defines its result variants and aggregation.

Profile fixtures cover clean, diagnostic, check-difference, target-local error, global setup error, standalone, and catalog envelopes for both fmt and lint. If the deferred common CLI scheduler has landed before this pending agent work resumes, fixtures also cover its command-fatal worker-runtime error with empty results. Tests project only the required profile fields, add unknown fields to prove forward-compatible ignoring, remove each required field to prove rejection, and verify that changed message prose does not affect agent branching.

Machine-readable output should be stable enough for agents to identify affected files, source spans, diagnostic `category`, diagnostic `code`, and `severity`. For configurable lint diagnostics, `code` is the rule id. Suggested follow-up actions are agent-derived helper output built from diagnostics, rule documentation, and workflow context; the initial formatter and linter core contracts do not expose a fix or suggestion API.

Lint result contracts, diagnostic codes, reporter behavior, and operational error separation are owned by [008-ox-mf2-phase-3c-linter-design.md](./008-ox-mf2-phase-3c-linter-design.md). Parser-owned semantic diagnostic behavior is owned by [012-ox-mf2-parser-semantic-validation-design.md](./012-ox-mf2-parser-semantic-validation-design.md). Agent integrations should consume those contracts instead of inferring lint or semantic behavior from human-readable output.

Agent integrations should use stable diagnostic codes and configurable rule ids as their machine-facing keys. During repository development, [linter-rules/index.md](./linter-rules/index.md) is the design-time entry point for reader-facing descriptions and remediation context; integrations may summarize that material for a specific environment but should not invent a separate rule catalog.

Initial agent integrations must not depend on a docs slug, public documentation URL, diagnostic `help` field, or runtime rule-metadata API. The linter's generated docs slug is internal metadata and the design-time pages are not a public runtime lookup contract. Exposing any of those values later requires the explicit public metadata/help contract owned by the linter design.

Resource/catalog-aware formatter and linter ownership is already defined rather than deferred to the agent layer. [013-ox-mf2-resource-catalog-adapter-design.md](./013-ox-mf2-resource-catalog-adapter-design.md) owns opted-in host formats, message extraction, host-string re-escaping, write-back validation, and resource-result aggregation; the formatter and linter designs continue to own the per-message results consumed by that adapter. Agent integrations may expose those workflows through the stable CLI JSON surface, but must not bypass the adapter contract or invent a second resource diagnostic schema. Phase 3E still owns the product decision about when those CLI workflows become part of the initial agent experience and how agents summarize file-, entry-, and message-scoped results.

## Integration Shapes

Agent integrations may be distributed in several forms:

- repo instructions that tell agents which CLI commands to run
- skills or command packs that wrap common ox-mf2 workflows
- hooks that enforce format or lint checks around agent edits
- MCP servers that expose parse, lint, format, or diagnostic query tools
- agent plugins that bundle skills, commands, hooks, MCP config, or documentation
- headless scripts for CI, review, release, and migration workflows

These integration shapes should call shared CLI, Rust, N-API, WASM, or future MCP APIs. They should not implement separate MF2 parsing, SemanticModel construction, parser-owned semantic validation, formatting, or linting logic.

## Deferred Follow-Up Notes

The following design decisions remain pending until Phase 3E is explicitly scheduled. Their presence here does not authorize implementation or select a product shape:

- Choose the first integration shape after the CLI: repository instructions, an agent skill, an MCP server, or a plugin bundle.
- Decide whether an MCP server exposes high-level tools such as `lint_file` and `format_file`, lower-level tools such as `parse_message`, `lint_message`, and `format_message`, or a deliberately limited combination.
- Define how agent integrations summarize top-level and target-local operational `errors[]` separately from parser, semantic, formatter, and linter diagnostics without redefining their schemas.
- Define edit authorization, read-only/review-only behavior, confirmation boundaries, and hook failure behavior before any agent hook may invoke write-mode formatting.
- Decide when resource/catalog-aware CLI workflows enter the agent experience and how file-, entry-, and message-scoped results are presented without bypassing the shared adapter contract.
- After the relevant [message linker](./014-ox-mf2-message-linker-design.md) milestones and contracts have stabilized, decide which `intlify messages` workflows enter the agent experience. Future integrations must consume the linker-owned stable CLI results and, where direct API integration is justified, its language-neutral artifact contracts. They must not introduce an agent-specific reference producer, reachability analysis, locale/fallback resolver, pruning model, or bundle planner. This reminder neither schedules Phase 3E nor adds linker commands to the current Agent Consumption Profile.
- Define vendor-neutral source documentation and the generation or adaptation boundary for Codex, Claude Code, Grok Build, and future agent plugin formats without making one vendor canonical.
- Revalidate the Agent Consumption Profile against the formatter, linter, and resource JSON fixtures that actually ship at the then-current `schemaVersion` before publishing any integration.
- Revisit the profile only through the existing CLI schema owners; do not add an agent-only reporter, suggestion field, or protocol merely to begin Phase 3E.
