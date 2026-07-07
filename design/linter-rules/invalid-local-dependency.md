# invalid-local-dependency

> report invalid MF2 local declaration dependencies

## Metadata

| Category   | Severity | Configurable |
| ---------- | -------- | ------------ |
| `semantic` | `error`  | no           |

## Details

This core semantic diagnostic reports `.local` declaration dependency patterns that violate the MF2 declaration rules.

A `.local` declaration must not bind a variable that appears in its own expression, and must not bind a variable that already appeared in a previous declaration's expression. This covers self references, forward references that are later bound, and dependency cycles.

This diagnostic is always enabled after successful parsing, is emitted as `error`, and cannot be configured through `lint.rules`.

Primary spans, labels, ordering, cascade behavior, and duplicate-family partitioning are defined canonically by the [semantic validation design](../012-ox-mf2-parser-semantic-validation-design.md).

### Fail

Some examples of **incorrect** code for this diagnostic:

```mf2
.local $label = {$label}
{{{$label}}}
```

```mf2
.local $a = {$b}
.local $b = {$a}
{{{$a}}}
```

### Pass

Some examples of **correct** code for this diagnostic:

```mf2
.local $label = {|items|}
.local $message = {$label}
{{{$message}}}
```

```mf2
.input {$count :number}
.local $label = {$count}
{{{$label}}}
```

## Configuration

This diagnostic has no configuration. It is always enabled as `error` and cannot be configured through `lint.rules`.

## Related diagnostics and rules

- [duplicate-declaration](./duplicate-declaration.md)
- [missing-selector-annotation](./missing-selector-annotation.md)
- [no-unused-declaration](./no-unused-declaration.md)

## Status

Designed as a parser-owned semantic diagnostic surfaced by the Phase 3C linter.

## Design References

- [Semantic validation design](../012-ox-mf2-parser-semantic-validation-design.md)
- [Linter design](../008-ox-mf2-phase-3c-linter-design.md)
