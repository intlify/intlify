# duplicate-option-name

> disallow duplicate MF2 option names on one owner

## Details

This core semantic diagnostic reports duplicate option identifiers within one function call or markup placeholder.

Duplicate detection is owner-local. Function options are compared only with options on the same function call, and markup options are compared only with options on the same markup placeholder. Different markup placeholders are separate owners. Option identifiers are compared by cooked identifier string after the parser's NFC normalization rule, and comparison is case-sensitive.

This diagnostic is always enabled after successful parsing, is emitted as `error`, and cannot be configured through `lint.rules`.

### Fail

Some examples of **incorrect** code for this diagnostic:

```mf2
{$count :number minimumFractionDigits=2 minimumFractionDigits=3}
```

```mf2
{{{#link href=|/a| href=|/b|/}}}
```

### Pass

Some examples of **correct** code for this diagnostic:

```mf2
{$count :number minimumFractionDigits=2 maximumFractionDigits=3}
```

```mf2
{{{#link href=|/a| title=|docs|/}}}
```

## Configuration

This diagnostic has no configuration. It is always enabled as `error` and cannot be configured through `lint.rules`.

## Related diagnostics and rules

- [no-duplicate-attribute](./no-duplicate-attribute.md)

## Version

This diagnostic is part of the Phase 3C linter design.

## Implementation

- [Linter design](../008-ox-mf2-phase-3c-linter-design.md)
- [Semantic validation design](../012-ox-mf2-parser-semantic-validation-design.md)
