# missing-fallback-variant

> require MF2 matchers to include a catch-all fallback variant

## Rule Details

This core semantic diagnostic reports a matcher that does not include a fallback variant whose keys are all catch-all keys, `*`.

The fallback requirement is independent from selector functions or known selector domains. This diagnostic is always enabled after successful parsing, is emitted as `error`, and cannot be configured through `lint.rules`.

### Fail

Some examples of **incorrect** code for this diagnostic:

```mf2
.input {$count :number}
.match $count
0 {{No items}}
1 {{One item}}
```

```mf2
.input {$gender :string}
.input {$count :number}
.match $gender $count
male 1 {{He has one item}}
female 1 {{She has one item}}
```

### Pass

Some examples of **correct** code for this diagnostic:

```mf2
.input {$count :number}
.match $count
0 {{No items}}
1 {{One item}}
* {{Items}}
```

```mf2
.input {$gender :string}
.input {$count :number}
.match $gender $count
male 1 {{He has one item}}
female 1 {{She has one item}}
* * {{Fallback}}
```

## Options

Nothing. This is a core semantic diagnostic and is not configurable.

## Related rules

- [variant-key-arity-mismatch](./variant-key-arity-mismatch.md)
- [duplicate-variant](./duplicate-variant.md)

## Version

This diagnostic is part of the Phase 3C linter design.

## Implementation

- [Linter design](../008-ox-mf2-phase-3c-linter-design.md)
- [Semantic validation design](../012-ox-mf2-parser-semantic-validation-design.md)
