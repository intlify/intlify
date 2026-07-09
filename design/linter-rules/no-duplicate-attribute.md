# no-duplicate-attribute

> disallow duplicate MF2 attributes on one placeholder

## Metadata

| Category        | Default Severity | Recommended | Configurable | Fixable |
| --------------- | ---------------- | ----------- | ------------ | ------- |
| `best-practice` | `warn`           | yes         | yes          | no      |

## Details

This configurable lint rule reports repeated attribute identifiers on one expression or markup placeholder.

The MF2 syntax specification (`refers/message-format-wg/spec/syntax.md`) says attribute identifiers SHOULD be unique and defines last-one-wins behavior for duplicates. This makes duplicate attributes a best-practice lint warning rather than a core semantic error.

Duplicate detection is owner-local. Attributes are compared only within the same expression placeholder, open markup placeholder, close markup placeholder, or standalone markup placeholder. Attribute identifiers are compared by cooked identifier string after the parser's NFC normalization rule, and comparison is case-sensitive. The primary span is the later duplicate attribute identifier, with a label on the first occurrence.

The rule consumes the parser-owned attribute owner facts, including `AttributeOwnerKind`, and does not reconstruct attribute owner taxonomy by walking the CST.

Attribute values are literals only in the current MF2 grammar, so this rule checks duplicate attribute identifiers only. It does not resolve value expressions or variable references.

This boundary is intentionally different from `duplicate-option-name`: duplicate attributes are configurable lint diagnostics, while duplicate option names are parser-owned semantic diagnostics because MF2 models option duplicates as data-model errors.

### Fail

Some examples of **incorrect** code for this rule:

```mf2
{$name :string @note=|a| @note=|b|}
```

```mf2
{{{#link @kind=|primary| @kind=|secondary|}docs{/link}}}
```

### Pass

Some examples of **correct** code for this rule:

```mf2
{$name :string @note=|a| @description=|b|}
```

```mf2
{{{#link @kind=|primary| @target=|docs|}docs{/link}}}
```

## Configuration

No rule-specific options exist in Phase 3C.

This rule is configurable through `lint.rules` with `"off"`, `"warn"`, or `"error"`. It is enabled as `"warn"` in `recommended`.

## Related diagnostics and rules

- [duplicate-option-name](./duplicate-option-name.md)

## Status

Designed for the Phase 3C linter as a configurable lint rule.

## Design References

- [Linter design](../008-ox-mf2-phase-3c-linter-design.md)
- [Semantic validation design](../012-ox-mf2-parser-semantic-validation-design.md)
