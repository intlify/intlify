# Linter Diagnostics and Rules

This directory documents the semantic diagnostics and configurable lint rules surfaced by the ox-mf2 Phase 3C linter.

These pages are design-time reader-facing documentation for Phase 3C review. They are not the public runtime contract for diagnostic JSON `help`, CLI output URLs, npm package docs, or website docs. Public docs, generated docs, and static help text remain future product decisions.

## Canonical Sources

- [008-ox-mf2-phase-3c-linter-design.md](../008-ox-mf2-phase-3c-linter-design.md) owns the linter product contract, pipeline, configuration, reporters, bindings, and release boundaries.
- [012-ox-mf2-parser-semantic-validation-design.md](../012-ox-mf2-parser-semantic-validation-design.md) owns parser semantic validation details for core semantic diagnostics, including spans, labels, ordering, and cascade suppression.
- This directory owns design-time reader-facing documentation, examples, configuration notes, and related diagnostic/rule links.

Configurable lint rule pages may describe rule-specific detection boundaries, primary span intent, and fixture expectations at design time. Core semantic diagnostic pages do not redefine those details; they point back to the parser-owned semantic validation design.

The examples in these pages are illustrative reader-facing examples. They are not parser fixtures, linter fixtures, or conformance tests.

Metadata tables in these pages are design-time summaries for readers. They are not the runtime rule metadata source, generated docs source, or public help-text contract. In semantic diagnostic pages, `Diagnostic Category` describes the JSON-visible diagnostic category such as `semantic`. In configurable rule pages, `Category` describes rule metadata such as `best-practice` or `correctness`; those rule metadata categories are not diagnostic JSON `category` values.

## Core Semantic Diagnostics

Core semantic diagnostics are always enabled after successful parsing, always reported as `error`, and cannot be configured through `lint.rules`.

| Diagnostic Code | Severity | Configurable | Description | Details |
| --- | --- | --- | --- | --- |
| `duplicate-declaration` | `error` | no | Report duplicate MF2 variable declarations. | [duplicate-declaration](./duplicate-declaration.md) |
| `invalid-declaration-dependency` | `error` | no | Report invalid MF2 declaration self-references and forward dependencies. | [invalid-declaration-dependency](./invalid-declaration-dependency.md) |
| `missing-selector-annotation` | `error` | no | Report MF2 selectors that do not resolve to an annotated declaration. | [missing-selector-annotation](./missing-selector-annotation.md) |
| `variant-key-arity-mismatch` | `error` | no | Report MF2 variant key counts that do not match selector counts. | [variant-key-arity-mismatch](./variant-key-arity-mismatch.md) |
| `missing-fallback-variant` | `error` | no | Report MF2 matchers without a catch-all fallback variant. | [missing-fallback-variant](./missing-fallback-variant.md) |
| `duplicate-variant` | `error` | no | Report duplicate MF2 matcher variant key tuples. | [duplicate-variant](./duplicate-variant.md) |
| `duplicate-option-name` | `error` | no | Report duplicate MF2 option names on one owner. | [duplicate-option-name](./duplicate-option-name.md) |

## Configurable Lint Rules

Configurable lint rules run only after parser and semantic diagnostics are clean. They can be configured through `lint.rules` with `"off"`, `"warn"`, or `"error"`.

The configurable-rule table mirrors the product-level metadata in the Phase 3C linter design for navigation convenience.

| Rule ID | Category | Default Severity | Recommended | Configurable | Fixable | Description | Details |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `no-unused-declaration` | `best-practice` | `warn` | yes | yes | no | Report MF2 declarations that do not affect output or selector setup. | [no-unused-declaration](./no-unused-declaration.md) |
| `no-duplicate-attribute` | `best-practice` | `warn` | yes | yes | no | Report duplicate MF2 attributes on one placeholder. | [no-duplicate-attribute](./no-duplicate-attribute.md) |
| `no-undeclared-variable` | `correctness` | `off` | no | yes | no | Report undeclared non-selector MF2 variable references. | [no-undeclared-variable](./no-undeclared-variable.md) |
