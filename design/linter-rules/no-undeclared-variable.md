# no-undeclared-variable

> disallow undeclared non-selector MF2 variable references

## Rule Details

This configurable lint rule reports a non-selector variable reference that cannot be resolved to a visible `.input` or `.local` declaration.

Undeclared variables are valid external inputs in MF2, so this rule is an opt-in rule for teams that adopt a declare-all-inputs workflow. Selector variables are excluded because missing selector declarations are reported by the core semantic [missing-selector-annotation](./missing-selector-annotation.md) diagnostic.

References are resolved against declarations visible at the reference point, meaning earlier declarations only. The rule covers unresolved non-selector references in `.local` right-hand-side expressions, pattern and placeholder expressions, function option values, markup option values, and future non-selector reference kinds promoted into `SemanticModel`. References to variables declared later are already [invalid-local-dependency](./invalid-local-dependency.md) semantic errors and are not double-reported by this rule.

### Fail

Some examples of **incorrect** code for this rule:

```mf2
.input {$count :number}
{{You have {$total} items.}}
```

```mf2
.input {$count :number minimumFractionDigits=$digits}
.match $count
1 {{One item}}
* {{Items}}
```

In the second example `$count` is a selector and belongs to core semantic validation, while `$digits` is a function option value reference and belongs to this rule when it is enabled.

### Pass

Some examples of **correct** code for this rule:

```mf2
.input {$total :number}
{{You have {$total} items.}}
```

```mf2
.input {$digits :number}
.input {$count :number minimumFractionDigits=$digits}
.match $count
1 {{One item}}
* {{Items}}
```

## Options

Nothing in Phase 3C.

This rule is configurable through `lint.rules` with `"off"`, `"warn"`, or `"error"`. It defaults to `"off"` and is not enabled in `recommended`.

## Related rules

- [missing-selector-annotation](./missing-selector-annotation.md)
- [no-unused-declaration](./no-unused-declaration.md)
- [invalid-local-dependency](./invalid-local-dependency.md)

## Version

This rule is part of the Phase 3C linter design.

## Implementation

- [Linter design](../008-ox-mf2-phase-3c-linter-design.md)
- [Semantic validation design](../012-ox-mf2-parser-semantic-validation-design.md)
