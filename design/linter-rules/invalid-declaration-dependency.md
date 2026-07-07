# invalid-declaration-dependency

> report invalid MF2 declaration self-references and forward dependencies

## Metadata

| Diagnostic Category | Severity | Configurable |
| ------------------- | -------- | ------------ |
| `semantic`          | `error`  | no           |

## Details

This core semantic diagnostic reports declaration dependency patterns that violate the MF2 declaration rules.

A declaration must not bind a variable that appeared in a dependency/reference position within a previous declaration. An input declaration must not bind a variable that appears in its own function annotation. A local declaration must not bind a variable that appears in its own expression. This covers input function option self references, input function option forward references that are later bound, `.local` self references, forward references that are later bound, and dependency cycles. Plain re-binding of an already-declared bound variable belongs to [duplicate-declaration](./duplicate-declaration.md).

This diagnostic is always enabled after successful parsing, is emitted as `error`, and cannot be configured through `lint.rules`.

Primary spans, labels, ordering, cascade behavior, and duplicate-family partitioning are defined canonically by the [semantic validation design](../012-ox-mf2-parser-semantic-validation-design.md).

At a high level, the primary span is the bound variable of the declaration that completes the invalid dependency. Reference occurrences that caused the dependency are labels.

### Fail

Some examples of **incorrect** code for this diagnostic:

```mf2
.input {$count :number minimumFractionDigits=$digits}
.input {$digits :number}
{{{$count}}}
```

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
.input {$digits :number}
.input {$count :number minimumFractionDigits=$digits}
{{{$count}}}
```

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
