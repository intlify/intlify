# ox-mf2 Parser Semantic Validation Design

This document defines the parser-owned semantic validation contract for ox-mf2. It is the canonical owner for core semantic diagnostics referenced by the Phase 3C linter design. The linter, the message linker's shared export-preparation layer, future validators, and LSP/editor integrations consume this contract instead of reimplementing MF2 data model validation.

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

Some non-goals are still tracked in [Deferred Follow-Up Notes](#deferred-follow-up-notes) when they are plausible future parser-owned work; they are excluded only from the Phase 3C initial contract.

## Ownership

`ox_mf2_parser` owns CST construction, parser diagnostics, `SemanticModel` construction, and core semantic validation. The parser crate exposes semantic diagnostics through a parser-owned API so downstream consumers do not need to infer MF2 data model errors themselves.

`intlify_lint` consumes parser semantic diagnostics and shapes them for CLI, N-API, and WASM outputs. It must not reimplement parser-owned semantic checks.

`intlify_export` consumes the same parser construction and validation contract at the shared export-preparation gate defined by [014-ox-mf2-message-linker-design.md](./014-ox-mf2-message-linker-design.md#mf2-syntax-and-semantic-validation-export-gate). Before invoking an exporter, it validates the identity-deduplicated union of plan-selected delivery definitions and the coverage-baseline definitions required to derive signatures for every admitted M1 key model. It derives language-neutral MF2 argument-signature information only from those parser- and semantically clean baseline definitions. The M1 `intlify_linker` model remains key-only and parser-independent, and target-specific MF2 validity rules remain forbidden.

`intlify_resource` and its host-format adapters remain independent of `ox_mf2_parser`. They own host parsing, extraction, mapping, and write-back under [013-ox-mf2-resource-catalog-adapter-design.md](./013-ox-mf2-resource-catalog-adapter-design.md), but do not construct `SemanticModel` or run semantic validation. A host integration may pass extracted message text to a parser-backed lint, editor, or export consumer without moving parser responsibility into the resource crate.

## SemanticModel and Validation API

Semantic validation runs after parsing and explicit `SemanticModel` construction. The zero diagnostic guarantee from [002-ox-mf2-phase-1-rust-parser-design.md](./002-ox-mf2-phase-1-rust-parser-design.md) applies: a parse result with zero parser diagnostics is syntactically valid per the MF2 ABNF. Semantic validation may therefore assume grammar-valid CST shapes.

The parser crate exposes semantic validation as an explicit API:

```rust
pub fn validate_semantics(
    model: &SemanticModel,
) -> Result<Vec<SemanticDiagnostic>, SemanticInvariantError>
```

`SemanticModel` owns semantic facts. `validate_semantics` owns diagnostic production and returns diagnostics in deterministic report order through the `Ok` branch. Semantic diagnostics are returned separately from parser diagnostics. They are not stored permanently on `SemanticModel`, are not mixed into `ParseResult.diagnostics`, and are not encoded into Binary AST snapshot diagnostic sections. If semantic validation detects an invariant failure, it returns `Err(SemanticInvariantError)` with kind `InvariantViolation`; downstream host boundaries convert that error to `internal_error` with `details.reason: "semantic_invariant_failed"` and `details.stage: "semantic_validation"`.

SemanticModel construction is also part of the parser-owned invariant boundary. `ParseResult` never owns an optional `SemanticModel`; parsing and semantic construction are separate phases. If parser diagnostics exist, downstream tooling does not construct a model or run semantic validation. If parser diagnostics are empty, SemanticModel construction and semantic validation must succeed. A construction or validation invariant failure is not a user-facing semantic diagnostic; it is an implementation failure that downstream CLI, N-API, WASM, or linter layers map to `internal_error`. Construction failures use `details.reason: "semantic_invariant_failed"` and `details.stage: "semantic_model_construction"`.

The implementation exposes the following parser-owned Rust API boundary:

```rust
pub fn build_semantic_model(
    sources: &SourceStore,
    result: &ParseResult,
) -> Result<SemanticModel, SemanticInvariantError>
```

The parser owns the error classification needed by every downstream boundary:

```rust
pub struct SemanticInvariantError {
    kind: SemanticInvariantErrorKind,
}

pub enum SemanticInvariantErrorKind {
    ApiMisuse,
    InvariantViolation,
}

impl SemanticInvariantError {
    pub fn kind(&self) -> SemanticInvariantErrorKind;
}
```

Both types form one closed parser contract. `SemanticInvariantError` has private fields and parser-owned construction, and exposes no public constructor, mutable state, deserializer, arbitrary code, free-form classification string, or consumer-defined extension. Presentation text and implementation-local context do not change `kind()`.

`ApiMisuse` means that the caller supplied a parse result containing parser diagnostics or a detectably inconsistent `SourceStore` / `ParseResult` pair. `InvariantViolation` means that a correctly attached, parser-diagnostic-free result could not produce a valid semantic model, or that `validate_semantics` detected a contradiction in a valid model.

`build_semantic_model` may return either kind. `validate_semantics` accepts an already constructed `SemanticModel` and returns only `InvariantViolation`; it does not reinterpret an ordinary semantic diagnostic as an error.

The error does not store the downstream presentation stage. The caller already knows whether the failure came from `build_semantic_model` or `validate_semantics` and combines that call-site fact with `kind()`: `ApiMisuse` maps to `semantic_api_misuse` with no additional required details field, while `InvariantViolation` maps to `semantic_invariant_failed` with required stage `semantic_model_construction` or `semantic_validation`.

`sources` must be the original owner that assigned every `SourceId` referenced by `result`; semantic construction reads source slices through that pair to derive cooked identifiers, literal values, and comparison keys. The function neither reparses nor mutates the syntax artifact and never stores the model back into `ParseResult`. Phase 1 `parse_message` satisfies this requirement by returning a `StandaloneParseResult`; standalone callers pass `parsed.sources()` and `parsed.result()` from that same wrapper.

Calling this boundary with a parse result that contains parser diagnostics returns `Err` with `SemanticInvariantErrorKind::ApiMisuse`. A detectably inconsistent owner/result attachment uses the same kind. `intlify_lint` and other downstream tools must check parser diagnostics first and skip SemanticModel construction when they are present. Downstream host boundaries convert misuse to `internal_error` with only required `details.reason: "semantic_api_misuse"` rather than adding the currently redundant construction stage or panicking, because CLI, N-API, and WASM callers should receive one identical structured operational error for implementation bugs. If the attachment is valid and parser diagnostics are empty but construction still returns `Err`, the error kind is `InvariantViolation` and the downstream host boundary converts it to `internal_error` with `details.reason: "semantic_invariant_failed"` and `details.stage: "semantic_model_construction"`.

## SemanticModel Fact Surface

`SemanticModel` is the canonical owner for semantic facts shared by parser semantic validation, the Phase 3C linter, shared export preparation, and future parser-backed validator or LSP/editor consumers. The lint and export crates must not build a parallel semantic fact model for these records.

Initial facts include:

- declarations: `.input` and `.local` declarations, declaration id, variable name, declaration kind, source declaration order, bound-name span, and declaration span
- references: variable reference id, reference kind, source span, declaration visibility point, resolved declaration id or unresolved state, and declaration-dependency context
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

Reference records also carry dependency context separately from their syntactic kind: an optional enclosing declaration id and an `isDeclarationDependency` flag. A reference inside an input declaration's function annotation subtree or a `.local` right-hand side can therefore be `FunctionOption` or `MarkupOption` while still having `isDeclarationDependency = true`. The function annotation subtree is the whole annotation attached to an expression, while function option value references are the variable references that occur inside annotation options. Bound variable occurrences are declaration facts, not dependency references. Attributes do not produce variable references in the current normative MF2 ABNF in `refers/message-format-wg/spec/message.abnf` because attribute values are literals only. Older exploration documents or stale test descriptions that mention variable-valued attributes are not part of this design contract.

The parser must expose shared semantic helper capability for facts that multiple consumers need:

- `output_references()` or equivalent returns non-selector references owned by the message body's expression and markup subtree. This includes pattern placeholder expressions, function option value references, and markup option value references.
- `selection_references()` or equivalent returns selector setup reachability roots. This includes `.match` selector variable occurrences, the resolved selector declaration chain, the function annotation subtree that annotates the selector, function option value references inside that selector annotation subtree, and declaration dependency references used by that selector setup.

`selection_references()` is the conceptual parser API name; reader-facing linter documentation may describe the same reachability roots as "selector setup references".

These helper names are conceptual. The implementation may choose different Rust names, but the capability to read output references and selection references through parser-owned read-only views is required by this design. In other words, the public Rust names are implementation details, while the two helper capabilities are consumer-facing contracts. The fact ownership and traversal boundary are also required. Selector annotation reachability is owned by parser semantic helpers: downstream consumers must not reimplement `.input` / `.local` selector chains, selector annotation discovery, annotation option references, or invalid-dependency cascade behavior from raw declaration facts.

Shared helper APIs should return read-only view iterators rather than raw ids only. At minimum, a reference view must expose a reference id, variable name, reference kind, source span, resolved declaration id or unresolved state, enclosing declaration id, and declaration dependency flag. A conceptual shape is:

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
    is_declaration_dependency: bool,
}
```

The exact Rust names and lifetimes are implementation details, but consumers should be able to read source span, resolved state, syntactic occurrence kind, and dependency context without mutating the model. Declaration, option, attribute, and matcher variant facts should follow the same read-only iterator/view pattern.

## Diagnostic Shape

Semantic diagnostics use a parser-owned representation:

```rust
enum SemanticDiagnosticCode {
    DuplicateDeclaration,
    InvalidDeclarationDependency,
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
| `invalid-declaration-dependency` | self-reference and forward-binding cases of the Duplicate Declaration family |
| `missing-selector-annotation` | MF2 Missing Selector Annotation data model error |
| `variant-key-arity-mismatch` | MF2 Variant Key Mismatch data model error |
| `missing-fallback-variant` | MF2 Missing Fallback Variant data model error |
| `duplicate-variant` | MF2 Duplicate Variant data model error |
| `duplicate-option-name` | MF2 Duplicate Option Name data model error |

All semantic diagnostics are emitted as `error`. Message wording and label wording are not stable compatibility surfaces. Code, severity, primary span, and report ordering are stable.

## Diagnostic Code Catalog API

The parser crate exposes JSON-visible diagnostic code catalogs through enum-owned iterator APIs rather than generated string-only tables:

The relationship between these string catalogs, compact numeric parser classifications, numeric API errors, and Phase 3 operational errors is indexed in [appendix-ox-mf2-error-code.md](./appendix-ox-mf2-error-code.md).

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

Semantic validation suppresses cascade diagnostics when a broken dependency chain would otherwise produce secondary errors. For example, if an `invalid-declaration-dependency` makes a selector chain unreliable, dependent `missing-selector-annotation` diagnostics for that same chain are suppressed. Independent diagnostics that do not rely on the broken chain are still emitted, such as variant key arity mismatches, missing fallback variants, or `missing-selector-annotation` for another selector.

For example, a message can have one selector declaration chain broken by `invalid-declaration-dependency` and also contain a variant whose key count does not match the selector count. The dependent `missing-selector-annotation` for the broken selector chain is suppressed, but the independent `variant-key-arity-mismatch` still reports. Semantic validation should produce root-cause diagnostics plus independent diagnostics, not every derivable downstream symptom and not a global stop-after-first-error result.

## Duplicate Family Policy

Duplicate-family diagnostics report every duplicate after the first occurrence in each duplicate group. The first occurrence is not reported. The second and later occurrences each produce one diagnostic whose primary span is the duplicate occurrence and whose label points to the first occurrence.

The duplicate declaration family is partitioned:

- declaration self-references and forward references that are later bound report `invalid-declaration-dependency` only
- plain re-binding of an already-declared variable reports `duplicate-declaration` only
- a declaration's bound variable occurrence is not a dependency reference

## Diagnostic Catalog

This section is the canonical parser-owned semantic validation catalog. Reader-facing linter documentation for the same JSON-visible codes lives in [linter-rules/index.md](./linter-rules/index.md), but those pages do not redefine spans, ordering, cascade suppression, or duplicate-family partitioning.

### duplicate-declaration

Reports a declaration that plainly re-binds a variable that was already bound by a previous declaration. `.input` and `.local` declarations share one variable namespace, per the MF2 declaration rules. Dependency-position occurrences in previous declarations belong to `invalid-declaration-dependency`, not this diagnostic.

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

The primary span is the later declaration's bound variable, with a label on the first declaration. When three or more declarations bind the same variable, every declaration after the first produces one diagnostic. Dependency-order violations belong to `invalid-declaration-dependency`.

### invalid-declaration-dependency

Reports declarations that violate MF2 declaration dependency rules. A declaration must not bind a variable that appeared in a dependency/reference position within a previous declaration. An input declaration must not bind a variable that appears in its own function annotation subtree, including option value references inside that subtree. A local declaration must not bind a variable that appears in its own expression. Self-references, forward references that are later bound, and therefore all dependency cycles are invalid, including acyclic-looking forward references. Bound variable occurrences are excluded from this diagnostic and belong to `duplicate-declaration` when plainly re-bound.

```mf2
.input {$count :number minimumFractionDigits=$digits}
.input {$digits :number}
{{{$count}}}
```

```mf2
.input {$count :number minimumFractionDigits=$count}
{{{$count}}}
```

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

- self-reference: the self-referencing declaration's bound variable span, whether the reference appears in an input declaration function annotation subtree or a local declaration expression
- forward reference later bound: the later declaration's bound variable span, because the violation is only known when the referenced variable becomes locally bound; this applies equally to input declaration function option value references and local declaration expression references
- direct cycle: the declaration that closes the cycle, usually the later declaration in source order
- longer forward chain: the declaration whose binding completes the previously referenced chain
- multiple previous appearances of the same later-bound variable: one diagnostic on the later declaration's bound variable span, with labels on the earlier appearances
- three-or-more declaration cycle: the source-order declaration that first completes the cycle is the primary span owner, with labels on the relevant earlier dependency references

The semantic validation policy reports the root dependency violation once, keeping the primary location on the source construct that makes the dependency invalid.

### missing-selector-annotation

Reports a selector variable that does not directly or indirectly through `.local` chains reference a declaration with a function annotation subtree. A selector variable with no declaration also reports this diagnostic. External input variables are valid in patterns, but MF2 requires every selector to resolve to an annotated declaration.

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
.input {$gender :string}
.input {$count :number}
.match $gender $count
male {{He has items.}}
* * {{Fallback}}
```

```mf2
.input {$count :number}
.match $count
one few {{Items}}
* {{Fallback}}
```

The primary span is the variant key list: from the first key start to the last key end. This applies to both extra-key and missing-key cases. For diagnostic-free parses, a key-list span should be available. If the implementation cannot derive one, semantic validation falls back to the variant span, and then to an empty current-offset span only as a defensive last resort. The selector list may be used as a label to show the expected key count.

### missing-fallback-variant

Reports a matcher without a fallback variant. Per the MF2 rule, at least one variant must have all keys equal to the catch-all key `*`, regardless of selector functions or selector domains.

The primary span is the `.match` keyword span, because no fallback token exists. For diagnostic-free parses, the `.match` keyword span should be available. If the implementation cannot derive it, semantic validation uses the current-offset empty span only as a defensive last resort. Labels may point at the matcher body or variant list for human-readable output, but labels are not fixture-locked.

```mf2
.input {$count :number}
.match $count
0 {{No items}}
1 {{One item}}
```

```mf2
.input {$gender :string}
.input {$count :number}
.match $gender $count
male 1 {{He has one item}}
female 1 {{She has one item}}
```

### duplicate-variant

Reports duplicate variant key tuples. Literal keys are compared by their cooked string values after the NFC normalization rule defined in the Phase 1 parser design, not by syntactical appearance, so `1` and `|1|` collide.

Arity-invalid variants do not participate in duplicate tuple comparison, because their tuple cannot be evaluated against the selector list. They also do not count as fallback candidates for `missing-fallback-variant`. An arity-invalid catch-all such as `* *` in a single-selector matcher therefore reports `variant-key-arity-mismatch` and still allows `missing-fallback-variant` to report independently.

```mf2
.input {$count :number}
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

- the exact closed `SemanticInvariantErrorKind` set, read-only `kind()` accessor, and parser-only checked construction
- `ApiMisuse` for a parse result with parser diagnostics and for every detectably inconsistent source-owner/result attachment
- `InvariantViolation` for valid-call model-construction and semantic-validation contradictions
- downstream classification from `kind()` plus call site without message-string inspection or a stored presentation stage, including omission of `stage` for `semantic_api_misuse`
- every semantic diagnostic with positive and negative cases
- deterministic report ordering
- duplicate-family partitioning
- duplicate groups with three or more occurrences
- cascade suppression for broken dependency chains
- input declaration function option value self-reference and forward-dependency cases, including multiple forward references to the same later-bound variable
- direct and three-or-more declaration cycle primary span behavior
- arity-invalid variants excluded from duplicate and fallback candidate checks
- missing fallback primary span behavior
- selector annotation chains through `.input` and `.local`
- cooked identifier comparison and NFC normalization for duplicate options and duplicate variants

Fixtures lock diagnostic code, severity, primary span, and report order. Diagnostic-free parses should provide the normal primary spans described above, so defensive last-resort fallback spans are not expected in ordinary semantic fixtures. Message text and label wording are not fixture-locked.

The minimum cascade fixture set includes these compound cases:

- `invalid-declaration-dependency` suppresses the dependent `missing-selector-annotation` for the same broken selector chain:

  ```mf2
  .local $selector = {$later}
  .local $later = {|x|}
  .match $selector
  * {{ok}}
  ```

  Expected diagnostics: `invalid-declaration-dependency` only.

- a broken selector chain does not suppress an independent `variant-key-arity-mismatch`:

  ```mf2
  .input {$count :number}
  .local $selector = {$later}
  .local $later = {|x|}
  .match $selector $count
  one {{bad}}
  * * {{ok}}
  ```

  Expected diagnostics: `invalid-declaration-dependency` and `variant-key-arity-mismatch`; the dependent `missing-selector-annotation` for `$selector` is suppressed.

- a broken selector chain does not suppress `missing-selector-annotation` for another independent selector:

  ```mf2
  .local $selector = {$later}
  .local $later = {|x|}
  .match $selector $other
  * * {{ok}}
  ```

  Expected diagnostics: `invalid-declaration-dependency` and `missing-selector-annotation` for `$other`; the dependent `missing-selector-annotation` for `$selector` is suppressed.

- dependency-family violations do not also report `duplicate-declaration` for the same root cause:

  ```mf2
  .local $a = {$b}
  .local $b = {$a}
  {{{$a}}}
  ```

  Expected diagnostics: `invalid-declaration-dependency`; no `duplicate-declaration` for the dependency root cause.

Fixture updates follow the existing parser fixture update flow. The semantic fixture test should support an update command equivalent to:

```sh
UPDATE_SNAPSHOTS=1 cargo test -p ox_mf2_parser semantic
```

The exact test filter can differ, but semantic diagnostics must be updateable through the parser crate's normal snapshot/fixture workflow. Rust unit tests may cover narrow helper behavior, but span/order regression coverage should come from semantic fixtures.

The parser crate should expose parser diagnostic code and semantic diagnostic code catalogs so downstream crates can verify JSON-visible diagnostic code namespace uniqueness.

## Implementation Phasing

The parser semantic validation implementation is a Phase 3C prerequisite for the linter. It should land before `crates/intlify_lint` depends on core semantic diagnostics. The same implemented parser contract is also a prerequisite for the 014 M3 shared export-preparation gate before `crates/intlify_export` can produce a `ValidatedExportBatch`. Product-level linter PR ordering remains owned by [008-ox-mf2-phase-3c-linter-design.md](./008-ox-mf2-phase-3c-linter-design.md), while linker/export milestone ordering remains owned by [014-ox-mf2-message-linker-design.md](./014-ox-mf2-message-linker-design.md); this section scopes only the parser-side prerequisite work.

Suggested implementation steps:

1. define the closed `SemanticInvariantErrorKind` and read-only `SemanticInvariantError::kind()` contract
2. expose `build_semantic_model(sources, result) -> Result<SemanticModel, SemanticInvariantError>` as the only source-backed construction path, including the paired accessors on `StandaloneParseResult`
3. define `SemanticDiagnosticCode` and `SemanticDiagnostic`
4. expose `validate_semantics(model: &SemanticModel) -> Result<Vec<SemanticDiagnostic>, SemanticInvariantError>`
5. implement declaration dependency diagnostics
6. implement selector annotation diagnostics
7. implement matcher variant diagnostics
8. implement duplicate option diagnostics
9. add error-kind, construction, attachment, ordering, and cascade tests
10. expose parser and semantic diagnostic code catalogs
11. add the cross-catalog collision test in `intlify_lint` once the linter crate exists

## Deferred Follow-Up Notes

These items are intentionally deferred and do not block this design document's Phase 3C contract. Later implementation or release plans may promote individual items when they become necessary.

- Snapshot-backed semantic validation, including the snapshot-to-`SemanticModel` path. This parser-owned path must land before detailed design or implementation of any future linter `lintSnapshot` API begins. Only after this parser path exists does a separate linter follow-up own the consumer API design. Its promotion gates are to construct `SemanticModel` from decoded snapshot bytes without silently reparsing source text, verify parser diagnostic capability, preserve all semantic facts needed by validation and linting, provide source/span consistency guarantees equivalent to source-backed validation, and carry fixtures proving source-backed and snapshot-backed validation return the same diagnostic codes, order, and spans; these gates are not a current linter API contract.
- Selector-function domain modeling for future `unreachable-variant`.
- Additional body-owned reference kinds beyond the Phase 3C initial `output_references()` fact surface.
- Additional semantic facts needed by future parser-backed validator, compiler, or LSP/editor consumers. Any promoted consumer must use this parser-owned surface without moving semantic-model construction or validation into `intlify_resource` or its host-format adapters.
- Public documentation pages and static help text for semantic diagnostic codes. The design-time pages under `design/linter-rules/` do not define the runtime `help` field or public docs URL contract.

## Open Questions

No parser semantic validation open questions remain at this design level.
