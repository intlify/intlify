# missing-selector-annotation

> report MF2 selectors that do not resolve to an annotated declaration

## Metadata

| Diagnostic Category | Severity | Configurable |
| ------------------- | -------- | ------------ |
| `semantic`          | `error`  | no           |

## Details

This core semantic diagnostic reports a selector variable that does not directly or indirectly resolve to a declaration with a function annotation.

External variables are valid in normal message output, but MF2 selectors require an annotated declaration so selection behavior can be determined. This diagnostic is always enabled after successful parsing, is emitted as `error`, and cannot be configured through `lint.rules`.

Primary spans, labels, ordering, and cascade behavior are defined canonically by the [semantic validation design](../012-ox-mf2-parser-semantic-validation-design.md).

### Fail

Some examples of **incorrect** code for this diagnostic:

```mf2
.input {$count}
.match $count
1 {{One item}}
* {{Items}}
```

```mf2
.match $count
1 {{One item}}
* {{Items}}
```

### Pass

Some examples of **correct** code for this diagnostic:

```mf2
.input {$count :number}
.match $count
1 {{One item}}
* {{Items}}
```

```mf2
.input {$count}
.local $selector = {$count :number}
.match $selector
1 {{One item}}
* {{Items}}
```

## Configuration

This diagnostic has no configuration. It is always enabled as `error` and cannot be configured through `lint.rules`.

## Related diagnostics and rules

- [no-undeclared-variable](./no-undeclared-variable.md)
- [invalid-local-dependency](./invalid-local-dependency.md)

## Status

Designed as a parser-owned semantic diagnostic surfaced by the Phase 3C linter.

## Design References

- [Semantic validation design](../012-ox-mf2-parser-semantic-validation-design.md)
- [Linter design](../008-ox-mf2-phase-3c-linter-design.md)
