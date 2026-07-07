# duplicate-option-name

> report duplicate MF2 option names on one owner

## Metadata

| Diagnostic Category | Severity | Configurable |
| ------------------- | -------- | ------------ |
| `semantic`          | `error`  | no           |

## Details

This core semantic diagnostic reports duplicate option identifiers within one function call or markup placeholder.

Duplicate detection is owner-local. Function options are compared only with options on the same function call, and markup options are compared only with options on the same markup placeholder. Different markup placeholders are separate owners, and open, close, and standalone markup placeholders are separate owners. Option identifiers are compared by cooked identifier string after the parser's NFC normalization rule, and comparison is case-sensitive.

This diagnostic is always enabled after successful parsing, is emitted as `error`, and cannot be configured through `lint.rules`.

Primary spans, labels, ordering, cascade behavior, and duplicate handling are defined canonically by the [semantic validation design](../012-ox-mf2-parser-semantic-validation-design.md).

### Fail

Some examples of **incorrect** code for this diagnostic:

```mf2
{$count :number minimumFractionDigits=2 minimumFractionDigits=3}
```

```mf2
{{{#link href=|/a| href=|/b|/}}}
```

### Pass

Some examples of **correct** code for this diagnostic:

```mf2
{$count :number minimumFractionDigits=2 maximumFractionDigits=3}
```

```mf2
{{{#link href=|/a| title=|docs|/}}}
```

## Configuration

This diagnostic has no configuration. It is always enabled as `error` and cannot be configured through `lint.rules`.

## Related diagnostics and rules

- [no-duplicate-attribute](./no-duplicate-attribute.md)

## Status

Designed as a parser-owned semantic diagnostic surfaced by the Phase 3C linter.

## Design References

- [Semantic validation design](../012-ox-mf2-parser-semantic-validation-design.md)
- [Linter design](../008-ox-mf2-phase-3c-linter-design.md)
