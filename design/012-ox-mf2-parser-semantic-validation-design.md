# ox-mf2 Parser Semantic Validation Design

This document defines the parser-owned semantic validation contract for ox-mf2. It is the canonical owner for core semantic diagnostics referenced by the Phase 3C linter design. The linter, future validators, LSP/editor integrations, and resource/catalog adapters consume this contract instead of reimplementing MF2 data model validation.

## Goals

- Define the parser-owned `SemanticModel` validation API.
- Define core semantic diagnostics for MF2 Data Model Errors.
- Keep parser diagnostics, semantic diagnostics, and configurable lint rules as separate diagnostic layers.
- Provide deterministic diagnostic ordering, duplicate handling, and cascade suppression rules.
- Provide enough fixtures and validation coverage for downstream tools to rely on parser semantic validation.

## Non-Goals

- Configurable lint rules.
- Linter presets, rule options, or linter reporter behavior.
- Formatter style diagnostics or fixes.
- Resource/catalog-level diagnostics.
- Snapshot-backed semantic validation.
- JavaScript plugin APIs or custom semantic validators.

## Ownership

`ox_mf2_parser` owns CST construction, parser diagnostics, semantic lowering, `SemanticModel`, and core semantic validation. The parser crate exposes semantic diagnostics through a parser-owned API so downstream consumers do not need to infer MF2 data model errors themselves.

`intlify_lint` consumes parser semantic diagnostics and shapes them for CLI, N-API, and WASM outputs. It must not reimplement parser-owned semantic checks.

## SemanticModel and Validation API

Semantic validation runs after parsing and semantic lowering. The zero diagnostic guarantee from [002-ox-mf2-phase-1-rust-parser-design.md](./002-ox-mf2-phase-1-rust-parser-design.md) applies: a parse result with zero parser diagnostics is syntactically valid per the MF2 ABNF. Semantic validation may therefore assume grammar-valid CST shapes.

The parser crate exposes semantic validation as an explicit API:

```rust
fn validate_semantics(model: &SemanticModel) -> Vec<SemanticDiagnostic>
```

`SemanticModel` owns semantic facts. `validate_semantics` owns diagnostic production and returns diagnostics in deterministic report order. Semantic diagnostics are returned separately from parser diagnostics. They are not stored permanently on `SemanticModel`, are not mixed into `ParseResult.diagnostics`, and are not encoded into Binary AST snapshot diagnostic sections.

## Diagnostic Shape

Semantic diagnostics use a parser-owned representation:

```rust
enum SemanticDiagnosticCode {
    DuplicateDeclaration,
    InvalidLocalDependency,
    MissingSelectorAnnotation,
    VariantKeyArityMismatch,
    MissingFallbackVariant,
    DuplicateVariant,
    DuplicateOptionName,
}

struct SemanticDiagnostic {
    code: SemanticDiagnosticCode,
    severity: Severity,
    span: Span,
    labels: Vec<DiagnosticLabel>,
}
```

The exact Rust names are implementation details, but the JSON-visible stable codes are kebab-case strings:

| Code | Meaning |
| --- | --- |
| `duplicate-declaration` | MF2 Duplicate Declaration data model error |
| `invalid-local-dependency` | self-reference and forward-binding cases of the Duplicate Declaration family |
| `missing-selector-annotation` | MF2 Missing Selector Annotation data model error |
| `variant-key-arity-mismatch` | MF2 Variant Key Mismatch data model error |
| `missing-fallback-variant` | MF2 Missing Fallback Variant data model error |
| `duplicate-variant` | MF2 Duplicate Variant data model error |
| `duplicate-option-name` | MF2 Duplicate Option Name data model error |

All semantic diagnostics are emitted as `error`. Message wording and label wording are not stable compatibility surfaces. Code, severity, primary span, and report ordering are stable.

## Diagnostic Ordering and Cascade Policy

Semantic validation reports every independent violation in one pass; it does not stop at the first semantic diagnostic.

Semantic diagnostics are ordered by:

1. primary span start
2. primary span end
3. semantic diagnostic code

Each violation site produces exactly one diagnostic with exactly one code. Overlapping semantic candidates are partitioned so that no source location is reported under two semantic codes for the same root cause.

Semantic validation suppresses cascade diagnostics when a broken dependency chain would otherwise produce secondary errors. For example, if an `invalid-local-dependency` makes a selector chain unreliable, dependent `missing-selector-annotation` diagnostics for that same chain are suppressed. Independent diagnostics that do not rely on the broken chain are still emitted, such as variant key arity mismatches, missing fallback variants, or `missing-selector-annotation` for another selector.

## Duplicate Family Policy

Duplicate-family diagnostics report every duplicate after the first occurrence in each duplicate group. The first occurrence is not reported. The second and later occurrences each produce one diagnostic whose primary span is the duplicate occurrence and whose label points to the first occurrence.

The duplicate declaration family is partitioned:

- self-references and forward references that are later bound report `invalid-local-dependency` only
- plain re-binding of an already-declared variable reports `duplicate-declaration` only

## Diagnostic Catalog

### duplicate-declaration

Reports a declaration that binds a variable that already appeared in a previous declaration. `.input` and `.local` share one variable namespace, per the MF2 declaration rules.

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

The primary span is the later declaration's bound variable, with a label on the first declaration. When three or more declarations bind the same variable, every declaration after the first produces one diagnostic. Dependency-order violations belong to `invalid-local-dependency`.

### invalid-local-dependency

Reports `.local` declarations that violate MF2 declaration dependency rules: a declaration must not bind a variable that appears in its own expression, and must not bind a variable that already appeared in a previous declaration's expression. Self-references, forward references that are later bound, and therefore all dependency cycles are invalid, including acyclic-looking forward references.

```mf2
.local $label = {$label}
{{{$label}}}
```

```mf2
.local $a = {$b}
.local $b = {$a}
{{{$a}}}
```

The primary span is the bound variable of the declaration that completes the violation, with labels on the earlier appearances. Cases in this dependency family are never additionally reported as `duplicate-declaration`.

### missing-selector-annotation

Reports a selector variable that does not directly or indirectly through `.local` chains reference a declaration with a function. A selector variable with no declaration also reports this diagnostic. External input variables are valid in patterns, but MF2 requires every selector to resolve to an annotated declaration.

```mf2
.input {$count}
.match $count
one {{One item}}
* {{Items}}
```

Selector annotations can be reached indirectly:

```mf2
.input {$count :number}
.local $selector = {$count}
.match $selector
one {{One item}}
* {{Items}}
```

The function annotation may also come from the `.local` expression itself:

```mf2
.input {$count}
.local $selector = {$count :number}
.match $selector
one {{One item}}
* {{Items}}
```

If the selector variable is undeclared, this semantic diagnostic is emitted independently from any lint rule state:

```mf2
.match $count
one {{One item}}
* {{Items}}
```

### variant-key-arity-mismatch

Reports a matcher variant whose key count does not match the selector count. The parser accepts arbitrary key counts syntactically, so this is a semantic diagnostic. The primary span is the offending variant's key list, with a label on the selector list.

```mf2
.match $gender $count
male {{He has items.}}
* * {{Fallback}}
```

```mf2
.match $count
one few {{Items}}
* {{Fallback}}
```

### missing-fallback-variant

Reports a matcher without a fallback variant. Per the MF2 rule, at least one variant must have all keys equal to the catch-all key `*`, regardless of selector functions or selector domains.

The primary span is the `.match` keyword span, because no fallback token exists. Recovery cases that cannot recover the `.match` keyword span use the current offset empty span. Labels may point at the matcher body or variant list for human-readable output, but labels are not fixture-locked.

```mf2
.match $count
0 {{No items}}
1 {{One item}}
```

```mf2
.match $gender $count
male 1 {{He has one item}}
female 1 {{She has one item}}
```

### duplicate-variant

Reports duplicate variant key tuples. Literal keys are compared by their cooked string values after the NFC normalization rule defined in the Phase 1 parser design, not by syntactical appearance, so `1` and `|1|` collide.

Arity-invalid variants do not participate in duplicate tuple comparison, because their tuple cannot be evaluated against the selector list. They also do not count as fallback candidates for `missing-fallback-variant`. An arity-invalid catch-all such as `* *` in a single-selector matcher therefore reports `variant-key-arity-mismatch` and still allows `missing-fallback-variant` to report independently.

```mf2
.match $count
1 {{One item}}
|1| {{Single item}}
* {{Items}}
```

### duplicate-option-name

Reports duplicate option identifiers within one function. Per the MF2 rule, option identifiers must be unique within a function; duplicates are a Duplicate Option Name data model error.

Duplicate detection is owner-local: options are compared only within the same function call. Option identifiers are compared by cooked identifier string after the NFC normalization rule defined in the Phase 1 parser design, and comparison is case-sensitive. The primary span is the later duplicate option identifier, with a label on the first occurrence. When three or more options share the same cooked identifier, every option after the first produces one diagnostic.

```mf2
{$count :number minimumFractionDigits=2 minimumFractionDigits=3}
```

## Fixtures and Validation

Parser semantic validation fixtures live with parser fixtures and tests. They must cover:

- every semantic diagnostic with positive and negative cases
- deterministic report ordering
- duplicate-family partitioning
- duplicate groups with three or more occurrences
- cascade suppression for broken dependency chains
- arity-invalid variants excluded from duplicate and fallback candidate checks
- missing fallback primary span behavior
- selector annotation chains through `.input` and `.local`
- cooked identifier comparison and NFC normalization for duplicate options and duplicate variants

Fixtures lock diagnostic code, severity, primary span, and report order. Message text and label wording are not fixture-locked.

The parser crate should expose parser diagnostic code and semantic diagnostic code catalogs so downstream crates can verify JSON-visible diagnostic code namespace uniqueness.

## Implementation Phasing

The parser semantic validation implementation is a Phase 3C prerequisite for the linter. It should land before `crates/intlify_lint` depends on core semantic diagnostics.

Suggested implementation steps:

1. define `SemanticDiagnosticCode` and `SemanticDiagnostic`
2. expose `validate_semantics(model: &SemanticModel)`
3. implement declaration dependency diagnostics
4. implement selector annotation diagnostics
5. implement matcher variant diagnostics
6. implement duplicate option diagnostics
7. add fixtures and ordering/cascade tests
8. expose parser and semantic diagnostic code catalogs

## Deferred Follow-Up Notes

- Snapshot-backed semantic validation, including the snapshot-to-semantic path.
- Selector-function domain modeling for future `unreachable-variant`.
- Additional semantic facts needed by resource/catalog adapters.
- Documentation pages and static help text for semantic diagnostic codes.

## Open Questions

No parser semantic validation open questions remain at this design level.
