# Linter Rules

This directory documents the semantic diagnostics and configurable lint rules surfaced by the ox-mf2 Phase 3C linter.

## Canonical Sources

- [008-ox-mf2-phase-3c-linter-design.md](../008-ox-mf2-phase-3c-linter-design.md) owns the linter product contract, pipeline, configuration, reporters, bindings, and release boundaries.
- [012-ox-mf2-parser-semantic-validation-design.md](../012-ox-mf2-parser-semantic-validation-design.md) owns parser semantic validation details for core semantic diagnostics, including spans, labels, ordering, and cascade suppression.
- This directory owns reader-facing rule documentation, examples, options, and related-rule links.

## Core Semantic Diagnostics

Core semantic diagnostics are always enabled, always reported as `error`, and cannot be configured through `lint.rules`.

| Rule ID | Description | Details |
| --- | --- | --- |
| `duplicate-declaration` | Disallow duplicate MF2 variable declarations. | [duplicate-declaration](./duplicate-declaration.md) |
| `invalid-local-dependency` | Disallow invalid MF2 local declaration dependencies. | [invalid-local-dependency](./invalid-local-dependency.md) |
| `missing-selector-annotation` | Require MF2 selectors to resolve to an annotated declaration. | [missing-selector-annotation](./missing-selector-annotation.md) |
| `variant-key-arity-mismatch` | Require MF2 variant key counts to match selector counts. | [variant-key-arity-mismatch](./variant-key-arity-mismatch.md) |
| `missing-fallback-variant` | Require MF2 matchers to include a catch-all fallback variant. | [missing-fallback-variant](./missing-fallback-variant.md) |
| `duplicate-variant` | Disallow duplicate MF2 matcher variant key tuples. | [duplicate-variant](./duplicate-variant.md) |
| `duplicate-option-name` | Disallow duplicate MF2 option names on one owner. | [duplicate-option-name](./duplicate-option-name.md) |

## Configurable Lint Rules

Configurable lint rules run only after parser and semantic diagnostics are clean. They can be configured through `lint.rules` with `"off"`, `"warn"`, or `"error"`.

| Rule ID | Description | Details |
| --- | --- | --- |
| `no-unused-declaration` | Disallow MF2 declarations that do not affect output or selection. | [no-unused-declaration](./no-unused-declaration.md) |
| `no-duplicate-attribute` | Disallow duplicate MF2 attributes on one placeholder. | [no-duplicate-attribute](./no-duplicate-attribute.md) |
| `no-undeclared-variable` | Disallow undeclared non-selector MF2 variable references. | [no-undeclared-variable](./no-undeclared-variable.md) |
