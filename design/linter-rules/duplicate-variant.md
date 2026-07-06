# duplicate-variant

> disallow duplicate MF2 matcher variant key tuples

## Rule Details

This core semantic diagnostic reports duplicate matcher variant key tuples.

Variant keys are compared by their cooked string values after the parser's NFC normalization rule. Syntactically different keys can therefore collide if they represent the same cooked value. This diagnostic is always enabled after successful parsing, is emitted as `error`, and cannot be configured through `lint.rules`.

### Fail

Some examples of **incorrect** code for this diagnostic:

```mf2
.input {$count :number}
.match $count
1 {{One item}}
|1| {{Single item}}
* {{Items}}
```

```mf2
.input {$gender :string}
.input {$count :number}
.match $gender $count
male 1 {{He has one item}}
male 1 {{Duplicate}}
* * {{Fallback}}
```

### Pass

Some examples of **correct** code for this diagnostic:

```mf2
.input {$count :number}
.match $count
1 {{One item}}
2 {{Two items}}
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

## Configuration

Nothing. This is a core semantic diagnostic and is not configurable.

## Related diagnostics and rules

- [variant-key-arity-mismatch](./variant-key-arity-mismatch.md)
- [missing-fallback-variant](./missing-fallback-variant.md)

## Version

This diagnostic is part of the Phase 3C linter design.

## Implementation

- [Linter design](../008-ox-mf2-phase-3c-linter-design.md)
- [Semantic validation design](../012-ox-mf2-parser-semantic-validation-design.md)
