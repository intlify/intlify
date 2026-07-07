# no-undeclared-variable

> disallow undeclared non-selector MF2 variable references

## Metadata

| Category      | Default Severity | Recommended | Configurable | Fixable |
| ------------- | ---------------- | ----------- | ------------ | ------- |
| `correctness` | `off`            | no          | yes          | no      |

## Details

This configurable lint rule reports a non-selector variable reference that cannot be resolved to a visible `.input` or `.local` declaration.

Undeclared variables are valid external inputs in MF2, so this rule is an opt-in rule for teams that adopt a declare-all-inputs workflow. Selector variables are excluded because missing selector declarations are reported by the core semantic [missing-selector-annotation](./missing-selector-annotation.md) diagnostic. Selector exclusion applies only to the `.match` selector variable occurrence itself. Other unresolved variables that appear while setting up selector annotations, such as annotation option values, remain non-selector references and may be reported by this rule.

In other words, "selector setup reference" and "selector variable occurrence" are not the same boundary. The selector occurrence belongs to semantic validation; unresolved non-selector references used by selector setup still belong to this rule when no parser or semantic diagnostic short-circuits configurable rules.

References are resolved against declarations visible at the reference point, meaning earlier declarations only. The rule covers unresolved non-selector references in input declaration function options, local declaration expressions, pattern and placeholder expressions, markup option values, and future non-selector reference kinds promoted into `SemanticModel`. Forward references in declaration dependency contexts that become [invalid-declaration-dependency](./invalid-declaration-dependency.md) semantic errors are not double-reported by this rule.

### Boundary Examples

The selector occurrence itself is not reported by this rule. In practice, this example produces [missing-selector-annotation](./missing-selector-annotation.md), so configurable rules short-circuit before this rule runs:

```mf2
.match $count
* {{Items}}
```

When this rule is enabled, a non-selector reference used while setting up selector annotations is reported:

```mf2
.input {$count :number minimumFractionDigits=$digits}
.match $count
* {{Items}}
```

A forward declaration dependency is owned by [invalid-declaration-dependency](./invalid-declaration-dependency.md), not by this rule:

```mf2
.input {$count :number minimumFractionDigits=$digits}
.input {$digits :number}
{{{$count}}}
```

If no later declaration binds the variable, the same unresolved option value reference remains a lint-rule candidate when this rule is enabled:

```mf2
.input {$count :number minimumFractionDigits=$digits}
{{{$count}}}
```

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

## Configuration

No rule-specific options exist in Phase 3C.

This rule is configurable through `lint.rules` with `"off"`, `"warn"`, or `"error"`. It defaults to `"off"` and is not enabled in `recommended`.

## Related diagnostics and rules

- [missing-selector-annotation](./missing-selector-annotation.md)
- [no-unused-declaration](./no-unused-declaration.md)
- [invalid-declaration-dependency](./invalid-declaration-dependency.md)

## Status

Designed for the Phase 3C linter as a configurable lint rule.

## Design References

- [Linter design](../008-ox-mf2-phase-3c-linter-design.md)
- [Semantic validation design](../012-ox-mf2-parser-semantic-validation-design.md)
