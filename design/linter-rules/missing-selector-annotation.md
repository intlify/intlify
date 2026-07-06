# missing-selector-annotation

> require MF2 selectors to resolve to an annotated declaration

## Rule Details

This core semantic diagnostic reports a selector variable that does not directly or indirectly resolve to a declaration with a function annotation.

External variables are valid in normal message output, but MF2 selectors require an annotated declaration so selection behavior can be determined. This diagnostic is always enabled, is emitted as `error`, and cannot be configured through `lint.rules`.

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

## Options

Nothing. This is a core semantic diagnostic and is not configurable.

## Related rules

- [no-undeclared-variable](./no-undeclared-variable.md)
- [invalid-local-dependency](./invalid-local-dependency.md)

## Version

This diagnostic is part of the Phase 3C linter design.

## Implementation

- [Linter design](../008-ox-mf2-phase-3c-linter-design.md)
- [Semantic validation design](../012-ox-mf2-parser-semantic-validation-design.md)
