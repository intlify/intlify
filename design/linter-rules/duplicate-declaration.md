# duplicate-declaration

> disallow duplicate MF2 variable declarations

## Rule Details

This core semantic diagnostic reports a declaration that binds a variable that was already declared earlier in the same MF2 message.

`.input` and `.local` declarations share one variable namespace. This diagnostic is always enabled after successful parsing, is emitted as `error`, and cannot be configured through `lint.rules`.

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

Nothing. This is a core semantic diagnostic and is not configurable.

## Related diagnostics and rules

- [invalid-local-dependency](./invalid-local-dependency.md)
- [no-unused-declaration](./no-unused-declaration.md)
- [no-undeclared-variable](./no-undeclared-variable.md)

## Version

This diagnostic is part of the Phase 3C linter design.

## Implementation

- [Linter design](../008-ox-mf2-phase-3c-linter-design.md)
- [Semantic validation design](../012-ox-mf2-parser-semantic-validation-design.md)
