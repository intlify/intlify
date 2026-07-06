# no-unused-declaration

> disallow MF2 declarations that do not affect output or selector setup

## Metadata

| Category        | Default Severity | Recommended | Configurable | Fixable |
| --------------- | ---------------- | ----------- | ------------ | ------- |
| `best-practice` | `warn`           | yes         | yes          | no      |

## Details

This configurable lint rule reports a declared variable that is not reachable from the message output or selector setup.

The rule applies to both `.input` and `.local` declarations. An unreachable declaration has no runtime effect in MF2, so the recommended preset reports it as `warn` by default.

Reachability starts from message output references and selector setup references, then follows `.local` right-hand-side dependencies backwards through declarations. Selector setup references include selector variables, selector declaration chains, selector annotations, and option value references used by selector annotations. References inside reachable function option values and markup option values also mark declarations as used. A declaration referenced only by another unreachable declaration is still unused.

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
- [invalid-local-dependency](./invalid-local-dependency.md)

## Status

Designed for the Phase 3C linter as a configurable lint rule.

## Design References

- [Linter design](../008-ox-mf2-phase-3c-linter-design.md)
- [Semantic validation design](../012-ox-mf2-parser-semantic-validation-design.md)
