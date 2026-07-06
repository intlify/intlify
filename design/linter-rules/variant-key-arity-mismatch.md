# variant-key-arity-mismatch

> require MF2 variant key counts to match selector counts

## Rule Details

This core semantic diagnostic reports a matcher variant whose key count does not match the number of selectors in the `.match` statement.

The parser can recover a matcher shape syntactically, but the MF2 data model is not valid unless every variant has the same arity as the selector list. This diagnostic is always enabled after successful parsing, is emitted as `error`, and cannot be configured through `lint.rules`.

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

Nothing. This is a core semantic diagnostic and is not configurable.

## Related diagnostics and rules

- [missing-fallback-variant](./missing-fallback-variant.md)
- [duplicate-variant](./duplicate-variant.md)

## Version

This diagnostic is part of the Phase 3C linter design.

## Implementation

- [Linter design](../008-ox-mf2-phase-3c-linter-design.md)
- [Semantic validation design](../012-ox-mf2-parser-semantic-validation-design.md)
