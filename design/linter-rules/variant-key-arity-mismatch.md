# variant-key-arity-mismatch

> report MF2 variant key counts that do not match selector counts

## Metadata

| Category   | Severity | Configurable |
| ---------- | -------- | ------------ |
| `semantic` | `error`  | no           |

## Details

This core semantic diagnostic reports a matcher variant whose key count does not match the number of selectors in the `.match` statement.

The parser can recover a matcher shape syntactically, but the MF2 data model is not valid unless every variant has the same arity as the selector list. This diagnostic is always enabled after successful parsing, is emitted as `error`, and cannot be configured through `lint.rules`.

Primary spans, labels, ordering, and cascade behavior are defined canonically by the [semantic validation design](../012-ox-mf2-parser-semantic-validation-design.md).

### Fail

Some examples of **incorrect** code for this diagnostic:

```mf2
.input {$gender :string}
.input {$count :number}
.match $gender $count
male {{He has items.}}
* * {{Fallback}}
```

```mf2
.input {$count :number}
.match $count
1 few {{Items}}
* {{Fallback}}
```

### Pass

Some examples of **correct** code for this diagnostic:

```mf2
.input {$gender :string}
.input {$count :number}
.match $gender $count
male 1 {{He has one item.}}
* * {{Fallback}}
```

```mf2
.input {$count :number}
.match $count
1 {{One item}}
* {{Items}}
```

## Configuration

This diagnostic has no configuration. It is always enabled as `error` and cannot be configured through `lint.rules`.

## Related diagnostics and rules

- [missing-fallback-variant](./missing-fallback-variant.md)
- [duplicate-variant](./duplicate-variant.md)

## Status

Designed as a parser-owned semantic diagnostic surfaced by the Phase 3C linter.

## Design References

- [Semantic validation design](../012-ox-mf2-parser-semantic-validation-design.md)
- [Linter design](../008-ox-mf2-phase-3c-linter-design.md)
