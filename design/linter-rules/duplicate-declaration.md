# duplicate-declaration

> report duplicate MF2 variable declarations

## Metadata

| Diagnostic Category | Severity | Configurable |
| ------------------- | -------- | ------------ |
| `semantic`          | `error`  | no           |

## Details

This core semantic diagnostic reports a declaration that binds a variable that was already declared earlier in the same MF2 message.

`.input` and `.local` declarations share one variable namespace. This diagnostic is always enabled after successful parsing, is emitted as `error`, and cannot be configured through `lint.rules`.

Primary spans, labels, ordering, cascade behavior, and duplicate-family partitioning are defined canonically by the [semantic validation design](../012-ox-mf2-parser-semantic-validation-design.md).

### Fail

Some examples of **incorrect** code for this diagnostic:

```mf2
.input {$count :number}
.input {$count :number}
{{{$count}}}
```

```mf2
.local $label = {$count}
.local $label = {|items|}
{{{$label}}}
```

### Pass

Some examples of **correct** code for this diagnostic:

```mf2
.input {$count :number}
.local $label = {$count}
{{{$label}}}
```

```mf2
.input {$count :number}
.input {$price :number}
{{Count: {$count}, price: {$price}}}
```

## Configuration

This diagnostic has no configuration. It is always enabled as `error` and cannot be configured through `lint.rules`.

## Related diagnostics and rules

- [invalid-local-dependency](./invalid-local-dependency.md)
- [no-unused-declaration](./no-unused-declaration.md)
- [no-undeclared-variable](./no-undeclared-variable.md)

## Status

Designed as a parser-owned semantic diagnostic surfaced by the Phase 3C linter.

## Design References

- [Semantic validation design](../012-ox-mf2-parser-semantic-validation-design.md)
- [Linter design](../008-ox-mf2-phase-3c-linter-design.md)
