# no-unused-declaration

> disallow MF2 declarations that do not affect output or selector setup

## Metadata

| Category        | Default Severity | Recommended | Configurable | Fixable |
| --------------- | ---------------- | ----------- | ------------ | ------- |
| `best-practice` | `warn`           | yes         | yes          | no      |

## Details

This configurable lint rule reports a declared variable that is not reachable from the message output or selector setup.

The rule applies to both `.input` and `.local` declarations. An unreachable declaration has no runtime effect in MF2, so the recommended preset reports it as `warn` by default.

Reachability starts from message output references and selector setup references, then follows declaration dependency references backwards through declarations. Declaration dependency references include input declaration function option value references and local declaration expression references. Selector setup references include `.match` selector variable occurrences, resolved selector declaration chains, function annotation subtrees that annotate selectors, and function option value references inside those selector annotation subtrees. References inside reachable function option values and markup option values also mark declarations as used. A declaration referenced only by another unreachable declaration is still unused. This rule runs only after parser and semantic diagnostics are clean, so invalid declaration dependency graphs are reported by [invalid-declaration-dependency](./invalid-declaration-dependency.md) before this rule runs.

### Fail

Some examples of **incorrect** code for this rule:

```mf2
.input {$count :number}
.local $unused = {$count}
{{You have {$count} items.}}
```

```mf2
.input {$count :number}
.local $label = {$count}
{{No count here}}
```

In the second example both `$label` and `$count` are unused: `$count` is only referenced by the unreachable `$label`.

### Pass

Some examples of **correct** code for this rule:

```mf2
.input {$count :number}
.local $label = {$count}
{{You have {$label} items.}}
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

This rule is configurable through `lint.rules` with `"off"`, `"warn"`, or `"error"`. It is enabled as `"warn"` in `recommended`.

## Related diagnostics and rules

- [no-undeclared-variable](./no-undeclared-variable.md)
- [duplicate-declaration](./duplicate-declaration.md)
- [invalid-declaration-dependency](./invalid-declaration-dependency.md)

## Status

Designed for the Phase 3C linter as a configurable lint rule.

## Design References

- [Linter design](../008-ox-mf2-phase-3c-linter-design.md)
- [Semantic validation design](../012-ox-mf2-parser-semantic-validation-design.md)
