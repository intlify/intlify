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
fn validate_semantics(
    model: &SemanticModel,
) -> Result<Vec<SemanticDiagnostic>, SemanticInvariantError>
```

`SemanticModel` owns semantic facts. `validate_semantics` owns diagnostic production and returns diagnostics in deterministic report order through the `Ok` branch. Semantic diagnostics are returned separately from parser diagnostics. They are not stored permanently on `SemanticModel`, are not mixed into `ParseResult.diagnostics`, and are not encoded into Binary AST snapshot diagnostic sections. If semantic validation detects an invariant failure, it returns `Err(SemanticInvariantError)`; downstream host boundaries convert that error to `internal_error` with `details.reason: "semantic_invariant_failed"` and `details.stage: "semantic_validation"`.

SemanticModel construction is also part of the parser-owned invariant boundary. If parser diagnostics exist, downstream tooling does not run semantic validation. If parser diagnostics are empty, SemanticModel construction and semantic validation must succeed. A construction or validation invariant failure is not a user-facing semantic diagnostic; it is an implementation failure that downstream CLI, N-API, WASM, or linter layers map to `internal_error`. Construction failures use `details.reason: "semantic_invariant_failed"` and `details.stage: "semantic_model_construction"`.

The implementation should expose an internal parser boundary equivalent to:

```rust
fn semantic_model_from_parse_result(
    parse: &ParseResult,
) -> Result<SemanticModel, SemanticInvariantError>
```

Calling this boundary with a parse result that contains parser diagnostics is caller misuse and returns `Err`. `intlify_lint` and other downstream tools must check parser diagnostics first and skip SemanticModel construction when they are present. Downstream host boundaries convert this misuse to `internal_error` with `details.reason: "semantic_api_misuse"` rather than panicking, because CLI, N-API, and WASM callers should receive structured operational errors for implementation bugs. If parser diagnostics are empty and construction still returns `Err`, the downstream host boundary converts that error to `internal_error` with `details.reason: "semantic_invariant_failed"` and `details.stage: "semantic_model_construction"`.

## SemanticModel Fact Surface

`SemanticModel` is the canonical owner for semantic facts shared by parser semantic validation, the Phase 3C linter, and future LSP/editor or resource/catalog consumers. The lint crate must not build a parallel semantic fact model for these records.

Initial facts include:

- declarations: `.input` and `.local` declarations, declaration id, variable name, declaration kind, source declaration order, bound-name span, and declaration span
- references: variable reference id, reference kind, source span, declaration visibility point, resolved declaration id or unresolved state, and local-dependency context
- selector references: variable references that appear as `.match` selectors
- message body references: variable references that appear in pattern placeholders and body expressions
- option occurrences: owner id, owner kind, option identifier, cooked identifier, identifier span, and owner-local occurrence order
- attribute occurrences: expression and markup placeholder owner id, owner kind, attribute identifier, cooked identifier, identifier span, and owner-local occurrence order
- matcher variants: matcher owner id, selector count, variant id, key tuple, key spans, body span, and variant order

SemanticModel fact iterators have stable semantic order, not implementation collection order. This document is the canonical source for parser-owned fact ordering:

- declarations: source declaration order
- references: source order
- selector references: `.match` selector order
- message body references: source order
- option occurrences: owner primary span source order, then owner-local occurrence order
- attribute occurrences: owner primary span source order, then owner-local occurrence order
- matcher variants: matcher owner primary span source order, then variant order

These orders are part of the consumer contract because semantic diagnostics and lint rules use them as final tie-breakers for otherwise identical spans and codes. Downstream consumers such as `intlify_lint` may derive their own report occurrence keys from these orders, but they must not redefine parser-owned fact ordering. The exact id types are implementation details. Implementations may assign owner ids in source order, but the public ordering contract is owner primary span source order, not raw id allocation order.

The initial reference kind taxonomy is:

```rust
enum ReferenceKind {
    LocalRhs,
    Selector,
    MessageBody,
    FunctionOption,
    MarkupOption,
}
```

Option occurrences distinguish the owner kind:

```rust
enum OptionOwnerKind {
    Function,
    Markup,
}
```

Function options and markup options are both collected into the semantic fact surface. Owner-local option checks compare only options with the same owner id and owner kind. Function and markup owners are never compared with each other, and different markup placeholders are never compared with each other. Open, close, and standalone markup placeholders are separate owners.

Attribute occurrences also distinguish the owner kind:

```rust
enum AttributeOwnerKind {
    Expression,
    MarkupOpen,
    MarkupClose,
    MarkupStandalone,
}
```

Owner-local attribute checks compare only attributes with the same owner id and owner kind. The `no-duplicate-attribute` lint rule consumes this parser-owned fact surface and must not reconstruct attribute owner taxonomy by walking the CST.

Reference records also carry dependency context separately from their syntactic kind: an optional enclosing declaration id and an `isLocalDependency` flag. A reference inside a `.local` right-hand side can therefore be `FunctionOption` or `MarkupOption` while still having `isLocalDependency = true`. Attributes do not produce variable references in the current MF2 grammar because attribute values are literals only.

The parser must expose shared semantic helper capability for facts that multiple consumers need:

- `output_references()` or equivalent returns non-selector references owned by the message body's expression and markup subtree. This includes pattern placeholder expressions, function option values, markup option values, and future body-owned reference kinds.
- `selection_references()` or equivalent returns selector setup reachability roots. This includes `.match` selector variables, selector declaration chains, selector declaration or `.local` selector expression function annotations, selector annotation option value references, and local dependency references used by that selector setup.

These helper names are conceptual. The implementation may choose different Rust names, but the ability to read output references and selection references through parser-owned read-only views is required by this design. The fact ownership and traversal boundary are also required. Selector annotation reachability is owned by parser semantic helpers: downstream consumers must not reimplement `.input` / `.local` selector chains, selector annotation discovery, annotation option references, or invalid-dependency cascade behavior from raw declaration facts.

Shared helper APIs should return read-only view iterators rather than raw ids only. At minimum, a reference view must expose a reference id, variable name, reference kind, source span, resolved declaration id or unresolved state, enclosing declaration id, and local dependency flag. A conceptual shape is:

```rust
impl SemanticModel {
    fn output_references(&self) -> impl Iterator<Item = ReferenceRef<'_>>;
    fn selection_references(&self) -> impl Iterator<Item = ReferenceRef<'_>>;
}

struct ReferenceRef<'a> {
    id: ReferenceId,
    name: &'a str,
    kind: ReferenceKind,
    span: Span,
    resolved_declaration: Option<DeclarationId>,
    enclosing_declaration: Option<DeclarationId>,
    is_local_dependency: bool,
}
```

The exact Rust names and lifetimes are implementation details, but consumers should be able to read source span, resolved state, syntactic occurrence kind, and dependency context without mutating the model. Declaration, option, attribute, and matcher variant facts should follow the same read-only iterator/view pattern.

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

## Diagnostic Code Catalog API

The parser crate exposes JSON-visible diagnostic code catalogs through enum-owned iterator APIs rather than generated string-only tables:

```rust
impl DiagnosticCode {
    pub fn all() -> &'static [DiagnosticCode];
    pub fn json_code(self) -> &'static str;
}

impl SemanticDiagnosticCode {
    pub fn all() -> &'static [SemanticDiagnosticCode];
    pub fn json_code(self) -> &'static str;
}
```

The exact names are implementation details, but parser diagnostics and semantic diagnostics must both expose an equivalent `all()` plus JSON-visible code mapping. Downstream crates, especially `intlify_lint`, collect these catalogs and combine them with lint rule ids to test that the shared JSON-visible diagnostic `code` namespace has no collisions. Parser tests should also detect when an enum variant is added without being included in `all()`.

## Diagnostic Ordering and Cascade Policy

Semantic validation reports every independent violation in one pass; it does not stop at the first semantic diagnostic.

Semantic diagnostics are ordered by:

1. primary span start
2. primary span end
3. JSON-visible semantic diagnostic code in ASCII ascending order
4. the relevant `SemanticModel` stable occurrence order for exact ties

Each violation site produces exactly one diagnostic with exactly one code. Overlapping semantic candidates are partitioned so that no source location is reported under two semantic codes for the same root cause.

Semantic validation suppresses cascade diagnostics when a broken dependency chain would otherwise produce secondary errors. For example, if an `invalid-local-dependency` makes a selector chain unreliable, dependent `missing-selector-annotation` diagnostics for that same chain are suppressed. Independent diagnostics that do not rely on the broken chain are still emitted, such as variant key arity mismatches, missing fallback variants, or `missing-selector-annotation` for another selector.

For example, a message can have one selector declaration chain broken by `invalid-local-dependency` and also contain a variant whose key count does not match the selector count. The dependent `missing-selector-annotation` for the broken selector chain is suppressed, but the independent `variant-key-arity-mismatch` still reports. Semantic validation should produce root-cause diagnostics plus independent diagnostics, not every derivable downstream symptom and not a global stop-after-first-error result.

## Duplicate Family Policy

Duplicate-family diagnostics report every duplicate after the first occurrence in each duplicate group. The first occurrence is not reported. The second and later occurrences each produce one diagnostic whose primary span is the duplicate occurrence and whose label points to the first occurrence.

The duplicate declaration family is partitioned:

- self-references and forward references that are later bound report `invalid-local-dependency` only
- plain re-binding of an already-declared variable reports `duplicate-declaration` only

## Diagnostic Catalog

This section is the canonical parser-owned semantic validation catalog. Reader-facing linter documentation for the same JSON-visible codes lives in [linter-rules/index.md](./linter-rules/index.md), but those pages do not redefine spans, ordering, cascade suppression, or duplicate-family partitioning.

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

Primary span selection is deterministic:

- self-reference: the self-referencing declaration's bound variable span
- forward reference later bound: the later declaration's bound variable span, because the violation is only known when the referenced variable becomes locally bound
- direct cycle: the declaration that closes the cycle, usually the later declaration in source order
- longer forward chain: the declaration whose binding completes the previously referenced chain
- multiple previous appearances of the same later-bound variable: one diagnostic on the later declaration's bound variable span, with labels on the earlier appearances

The semantic validation policy reports the root dependency violation once, keeping the primary location on the source construct that makes the dependency invalid.

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

The primary span is the `.match` selector variable occurrence. If the selector is declared but the resolved declaration chain has no function annotation, labels may point to the relevant declaration or chain entries. If the selector variable is undeclared, the selector occurrence remains the primary span and labels are optional. If an invalid dependency chain makes annotation resolution unreliable, the dependent `missing-selector-annotation` is suppressed as described in the cascade policy.

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

The primary span is the variant key list: from the first key start to the last key end. This applies to both extra-key and missing-key cases. If a recovered CST cannot provide a key-list span, semantic validation falls back to the variant span, and then to an empty current-offset span only as a last resort. The selector list may be used as a label to show the expected key count.

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

Reports duplicate option identifiers within one option owner. Function and markup options are both represented as data-model `Options`, and duplicates are a Duplicate Option Name data model error.

Duplicate detection is owner-local: options are compared only within the same function call or the same markup placeholder. Function options and markup options are not compared with each other. Different markup placeholders are not compared with each other; open, close, and standalone markup placeholders are separate owners. Option identifiers are compared by cooked identifier string after the NFC normalization rule defined in the Phase 1 parser design, and comparison is case-sensitive. The primary span is the later duplicate option identifier, with a label on the first occurrence. When three or more options share the same cooked identifier, every option after the first produces one diagnostic.

```mf2
{$count :number minimumFractionDigits=2 minimumFractionDigits=3}
```

```mf2
{{{#link href=|/a| href=|/b|/}}}
```

## Fixtures and Validation

Parser semantic validation fixtures live under `crates/ox_mf2_parser/fixtures/semantic/` and are exercised by parser crate tests. They must cover:

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

The minimum cascade fixture set includes these compound cases:

- `invalid-local-dependency` suppresses the dependent `missing-selector-annotation` for the same broken selector chain:

  ```mf2
  .local $selector = {$later}
  .local $later = {|x|}
  .match $selector
  * {{ok}}
  ```

  Expected diagnostics: `invalid-local-dependency` only.

- a broken selector chain does not suppress an independent `variant-key-arity-mismatch`:

  ```mf2
  .input {$count :number}
  .local $selector = {$later}
  .local $later = {|x|}
  .match $selector $count
  one {{bad}}
  * * {{ok}}
  ```

  Expected diagnostics: `invalid-local-dependency` and `variant-key-arity-mismatch`; the dependent `missing-selector-annotation` for `$selector` is suppressed.

- a broken selector chain does not suppress `missing-selector-annotation` for another independent selector:

  ```mf2
  .local $selector = {$later}
  .local $later = {|x|}
  .match $selector $other
  * * {{ok}}
  ```

  Expected diagnostics: `invalid-local-dependency` and `missing-selector-annotation` for `$other`; the dependent `missing-selector-annotation` for `$selector` is suppressed.

- dependency-family violations do not also report `duplicate-declaration` for the same root cause:

  ```mf2
  .local $a = {$b}
  .local $b = {$a}
  {{{$a}}}
  ```

  Expected diagnostics: `invalid-local-dependency`; no `duplicate-declaration` for the dependency root cause.

Fixture updates follow the existing parser fixture update flow. The semantic fixture test should support an update command equivalent to:

```sh
UPDATE_SNAPSHOTS=1 cargo test -p ox_mf2_parser semantic
```

The exact test filter can differ, but semantic diagnostics must be updateable through the parser crate's normal snapshot/fixture workflow. Rust unit tests may cover narrow helper behavior, but span/order regression coverage should come from semantic fixtures.

The parser crate should expose parser diagnostic code and semantic diagnostic code catalogs so downstream crates can verify JSON-visible diagnostic code namespace uniqueness.

## Implementation Phasing

The parser semantic validation implementation is a Phase 3C prerequisite for the linter. It should land before `crates/intlify_lint` depends on core semantic diagnostics.

Suggested implementation steps:

1. define `SemanticDiagnosticCode` and `SemanticDiagnostic`
2. expose `validate_semantics(model: &SemanticModel) -> Result<Vec<SemanticDiagnostic>, SemanticInvariantError>`
3. implement declaration dependency diagnostics
4. implement selector annotation diagnostics
5. implement matcher variant diagnostics
6. implement duplicate option diagnostics
7. add fixtures and ordering/cascade tests
8. expose parser and semantic diagnostic code catalogs
9. add the cross-catalog collision test in `intlify_lint` once the linter crate exists

## Deferred Follow-Up Notes

These items are intentionally deferred and do not block this design document's Phase 3C contract. Later implementation or release plans may promote individual items when they become necessary.

- Snapshot-backed semantic validation, including the snapshot-to-`SemanticModel` path. This parser-owned path is the canonical prerequisite for any future linter `lintSnapshot` API. A future snapshot-backed path must construct `SemanticModel` from decoded snapshot bytes without silently reparsing source text, verify parser diagnostic capability, preserve all semantic facts needed by validation and linting, provide source/span consistency guarantees equivalent to source-backed validation, and carry fixtures proving source-backed and snapshot-backed validation return the same diagnostic codes, order, and spans.
- Selector-function domain modeling for future `unreachable-variant`.
- Additional semantic facts needed by resource/catalog adapters.
- Public documentation pages and static help text for semantic diagnostic codes. The design-time pages under `design/linter-rules/` do not define the runtime `help` field or public docs URL contract.

## Open Questions

No parser semantic validation open questions remain at this design level.
