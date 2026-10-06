# intlify_authoring_js

JavaScript and TypeScript host Producer for [Intlify](../../design/000-intlify-overview-design.md)'s source authoring.

> [!IMPORTANT]
>
> This crate implements part of Phase 2 of [design 016](../../design/016-intlify-source-authoring-and-intent-identity-design.md). It admits and reads source units, recognizes the explicit authoring forms `intent`, `mf2` and `noIntent`, recognizes ordinary UI text assigned to a proven DOM receiver, reads `@intlify` metadata, and assembles the units into a sealed `authoring-inventory`. See [Current status](#current-status).

A Producer reads host source and hands [`intlify_authoring`](../intlify_authoring/README.md) what it found, already decoded. What a message means, what its use site must supply, its locale, class and revision are all decided there, so the same text never means different things depending on which host found it. This crate owns the part specific to JavaScript: which bytes a unit is, which grammar reads them, and what a literal decodes to and from where.

## Reading a unit

An invocation runs in three steps.

`admit_units` checks everything the caller supplied before anything is parsed: the context's kind and profile pin, each unit's owner and grammar, its bytes against its snapshot, and the declared scope. Every refusal there is operational, because no edit to source could fix it. The one exception is a unit whose bytes are exactly what its snapshot names but are not UTF-8: that is the author's to fix, so the unit is admitted and reported as failed.

`analyze_unit` then reads one admitted unit, parsing it exactly once under the grammar its snapshot names. A unit is accepted only when the parser reports nothing, the semantic checks carrying the language's early errors report nothing, and a script contains no module syntax. Anything else rejects the whole tree, and the unit fails with one diagnostic at the earliest range the parser named. Units are analyzed independently, so a caller can run them in any order or on several workers.

An accepted unit is read for the explicit forms the profile registers and, when the profile admits the standard `document`, for UI text, and what is found is handed to `intlify_authoring`. The result says whether the unit is checked, blocked or failed. Its facts are reachable through `checked()` only when every occurrence resolved; `inspection_facts()` returns what was independently established whatever the outcome, for inspection and not as complete input.

`assemble_inventory` finally merges the analyses into one inventory, sealed as an `authoring-inventory` artifact that `intlify_authoring` admits. See [Assembling an inventory](#assembling-an-inventory).

## Grammars

| Grammar                     | Reads                            |
| --------------------------- | -------------------------------- |
| `intlify-grammar-js-module` | JavaScript under the module goal |
| `intlify-grammar-js-script` | JavaScript under the script goal |
| `intlify-grammar-ts-module` | TypeScript under the module goal |
| `intlify-grammar-ts-script` | TypeScript under the script goal |

The caller chooses the grammar in the snapshot. Nothing here reads a file name or suffix, and a unit that fails is not retried under another goal. JSX and TSX are not registered: a JSX profile has to say how text between tags is extracted first.

`tests/grammar_pin.rs` records what revision `"0"` accepts and rejects for a fixed corpus. A parser upgrade that changes any of it fails there, so the revision is reviewed before the upgrade is accepted.

Two readings are stricter than the pinned parser on its own:

- **A TypeScript script has no module syntax.** The parser does not check this under TypeScript, because it cannot yet tell a script from a module there. A unit declared a script is a script, so an `import` or `export` in it is a rejection in both languages.
- **Regular expression literals are checked.** An invalid pattern is an early error in the language, and without the check the parser accepts units every engine refuses to load.

A unit's text is its bytes, a byte order mark included. To the language that mark is whitespace, so a hashbang after it is not at the start of the text and the unit is rejected. A host that strips the mark while decoding would run the file.

## Decoding literals

016 reads a message from the host's cooked value, so `intent('a\nb')` and a `mf2` template with the same escape both hand MF2 a line feed. The parser computes that value but not where each decoded byte came from. The decoder here reads the literal again from its source bytes, recording an `InputSegment` per run as it goes, and requires its text to equal the parser's byte for byte. A disagreement stops the invocation rather than choosing one reading. The same map locates what the shared crate reports inside a message: an MF2 syntax error carries the decoded range the parser saw and, as `source_range()`, the bytes of the unit it came from, even when its declaration is blocked.

Runs split wherever a source byte stops answering for exactly one decoded byte. Verbatim text and a lone carriage return a template reads as a line feed are positional; an escape, a CRLF a template reads as one line feed, and a line continuation are each a run of their own.

The profile does not accept, as message source:

- a surrogate that does not pair as two adjacent `\uXXXX` escapes;
- an escape a tagged template keeps without a cooked value;
- a legacy octal escape, or `\8` or `\9`, which only sloppy code admits.

JavaScript also pairs surrogates spelled other ways, such as a `\uXXXX` escape followed by a `\u{...}` one. The pinned parser does not, and the profile does not accept a spelling its parser reads differently from the language.

## Explicit forms

`intent`, `mf2` and `noIntent` are recognized by the binding a name resolves to, never by its spelling. The profile registers exact module exports as intrinsics, and a unit's direct named import of one binds it under any local name. A shadowing parameter, a local function called `intent`, or an import from another module is not an intrinsic, and nothing is reported about it.

A known intrinsic used other than as the callee of a direct call, or as the tag of an `mf2` template, is reported where it is used: assigned, passed, called optionally or through `.call`, constructed, wrapped in parentheses, or exported. A registered module imported another way is reported once, where it is imported: a default or namespace import, a literal dynamic `import()`, a TypeScript `import = require()`, or a re-export. A re-export would hand the intrinsics to other modules under a specifier nobody registered, where their calls would read as ordinary calls.

The first argument of `intent()` is classified by its syntax, seen through parentheses and TypeScript assertions:

| Argument | Result |
| --- | --- |
| a string, or a template without substitutions | an `intent-literal` declaration used at the call |
| an `mf2` tagged template | an `mf2-declaration` used at the call |
| a `const` binding initialized with an `mf2` tag | a use of that shared declaration |
| an alias of such a binding, or an imported binding | unsupported |
| a conditional or logical expression | unsupported; neither alternative is chosen |
| anything else, or a template with substitutions | dynamic |

An `mf2` tag not used anywhere is still a declaration. Every use of a shared declaration names the same declaration occurrence, and equal text at two declarations is two declarations.

The second argument is read only as a plain object literal: static keys, plain values and shorthand properties, in source order. Values are kept by position and never evaluated. Spreads, computed or numeric keys, accessors, methods, `__proto__: value`, and anything that is not an object literal are reported, because the names they supply could change at run time. Whether the names match what the message requires is decided by `intlify_authoring`. A mismatch at a declaration's own use site blocks the declaration; a mismatch at a use of a shared declaration blocks only that use.

`noIntent(value, reason)` needs a nonempty static reason. The value is not read. An exclusion whose value is itself `intent(...)` or an `mf2` tag, or an `intent()` whose source is `noIntent(...)`, is a contradiction and is reported without making either a fact.

## Automatic UI text

When the profile admits the standard `document` (`with_dom_globals([DomGlobal::Document])`), a static literal assigned with `=` to `textContent` of a proven receiver is displayed text. It becomes a `ui-literal` declaration with the `text-content` usage from the `intlify-web-dom-usage` profile, used where it is assigned, and its braces stay characters. A context reading such units has to register that usage profile.

A receiver is proven when it starts at `document.querySelector()` or `document.createElement()` with exactly one static string, on the `document` global itself, and reaches the assignment through `const` aliases in the same function, and when nothing on any path to the assignment may have changed what its display property does. The walk follows each function's own control flow: branches, loops until nothing more changes, `switch` fallthrough, labels, and `try`, `catch` and `finally`, including jumps that leave through a `finally`. A reference to the receiver is harmless only as the object of a member access or a method call, an operand of a comparison, `typeof`, `!` or `void`, a condition, or the initializer of a `const` alias. A member that reaches its prototype or redefines its properties (`__proto__`, `constructor`, `__defineGetter__`, `__defineSetter__`), or one whose key is known only at run time, is not harmless. Passing it anywhere, storing it, assigning it to another variable, returning, yielding or exporting it, deleting or writing computed properties, changing its prototype, or capturing it in a closure invalidates it for every alias, and nothing restores it. The one thing that starts over is a `const` running `document.createElement()` again, as a loop body does: it binds a new element, unless a hoisted function or a closure made earlier in the block already reaches it.

What cannot be proven is not guessed:

| Receiver | Literal assigned |
| --- | --- |
| No known origin: a parameter, `this.el`, an import, an object property, another DOM API | Outside the profile; counted by `outside_profile()` |
| Known origin, not followed: `let` or `var`, a script's top level, an alias in another function, a use from another function, an origin without one static string, a function with `with` or a sloppy direct `eval` | Reported as `authoring-form-unsupported` |
| Followed, but some path invalidated it | Reported as `receiver-evidence-invalidated` |
| Past a bound: alias chain, origins one function tracks, proof steps | Reported as `authoring-resource-limit` |

A proven sink assigned anything but a literal, a compound assignment to one, or an `mf2` tag assigned to one is reported too. An `intent()` or `noIntent()` assigned to a sink is explicit authoring and is read only by the explicit recognizer, whatever the receiver: explicit forms take no usage from where they are written, so their revision never depends on the receiver around them.

## Metadata

A block comment starting with `@intlify`, followed by one JSON object, describes the declaration in the statement right after it:

```js
/* @intlify { "sourceLocale": "en", "surfaceClass": "checkout", "description": "Primary payment action" } */
button.textContent = 'Pay now'
```

The same comment means the same thing above ordinary UI text, an `intent()` message and an `mf2` declaration. Its members are `sourceLocale`, `surfaceClass` and `description`, each optional and each a nonempty string. Whether a locale canonicalizes, whether a class is in the vocabulary, and how long a value may be are decided by `intlify_authoring`, as they are for values from anywhere else. An annotated class comes before the invocation's default, and there is no default description.

A declaration belongs to a statement when that statement is the innermost member of a statement list around it, with no function or class in between. So an annotation never describes a whole function or block, and never looks past the next statement. Anything else is reported as `authoring-metadata-invalid`, never ignored:

| Annotation | Detail |
| --- | --- |
| `// @intlify` or `/** @intlify` | `metadata-comment-form` |
| Not exactly one JSON value | `metadata-json-malformed` |
| Not an object | `metadata-not-object` |
| A member named twice | `metadata-member-duplicate` |
| Another member, such as a misspelled one | `metadata-member-unknown` |
| A value that is not a string, or is empty | `metadata-value-not-string`, `metadata-value-empty` |
| Inside an expression, after the last statement, or before a statement in no list | `metadata-misplaced` |
| Two before one statement; neither applies | `metadata-repeated` |
| The next statement declares nothing of its own | `metadata-target-absent` |
| The next statement declares more than one message | `metadata-target-ambiguous` |
| The next statement only uses a shared declaration, whose metadata a use cannot redefine | `metadata-target-reference` |

A reported annotation that belongs to one declaration keeps that declaration from being established, since what the author wrote about it is not known. An annotation longer than the `annotation_bytes` bound is reported as `authoring-resource-limit`. When the statement after an annotation holds only something already reported, such as a sink whose receiver could not be proven, no second record is made. A profile that registers no intrinsic and admits no DOM global can declare nothing, so it reads no annotation.

## Assembling an inventory

`assemble_inventory` takes the analyses of one invocation's units, in any order, together with the scope they were admitted for. Before it copies a single fact it checks that each analysis is a member of that scope at the revision it read, was read under this context and this profile, belongs to this owner, and is handed over once, and that a complete scope has every member read. The declarations and diagnostics of all units together are bounded by the shared crate's limits on one invocation. Every refusal is operational.

The facts are then put in canonical order, validated as an `AuthoringInventory`, and sealed with their integrity digest. The result depends only on which analyses were given: supplying the units in another order, or reading each on its own thread, gives the same artifact bytes. Diagnostics come in design 016's reporting order across units.

What the result may be used for is answered by type:

| Outcome | `checked_inventory()` | Use |
| --- | --- | --- |
| every unit checked, complete scope | `Some(CheckedInventory::Complete(_))` | complete checked authoring input |
| every unit checked, partial scope | `Some(CheckedInventory::Partial(_))` | checked facts about that part only; never evidence that a declaration is absent from the project |
| some unit blocked or failed | `None` | `inspection_inventory()` still returns the sealed record, for inspection only |

A failed unit is recorded with its outcome and no facts, because a unit that could not be read is not evidence that it declares nothing. A reference records where it is written, not whether it runs.

## Measurement

The non-default `benchmark` feature measures two operations as a design 026 owner, through the owner run in `intlify_measurement`. Ordinary builds never include it.

| Operation | Inside the interval | Outside it |
| --- | --- | --- |
| `source_discovery / parse_and_classify` | One admitted unit to its classified declarations, uses, exclusions and host diagnostics: parsing, the semantic build, the explicit forms, DOM receiver proofs, and annotations | Admission and its digest check, and handing declarations to `intlify_authoring`, which is Phase 1's message analysis |
| `authoring_result / inventory_assembly` | Analyzed units to the sealed inventory and its JSON bytes | Analyzing the units |

Source discovery runs the same function `analyze_unit` runs first, so nothing is reimplemented for measurement.

The 30 fixtures are design 016's required workloads for this phase: a unit with no candidates, short UI literals at three scales, equal text at distinct declarations, complex MF2, sparse and dense parameters, many references to one declaration at three scales, design 028's representative application, conditional selection, an escaped receiver, invalid host syntax, a complete and a partial inventory of the same units, and the exact and first-over sides of the bounds on syntax tree nodes, alias chains, proof steps, annotation bytes, parameters, unit bytes and units.

Each fixture declares the path it takes: complete, blocked, or refused with an operational failure. One that takes another path stops the run before its Plan is issued. A bound is taken from the fixture itself, so its exact side is the fixture's own count and its first-over side is one below. A bound on one declaration or use site blocks it; a bound on a unit or the invocation refuses. A unit over its byte bound is refused at admission, outside both intervals, so only its exact side is measured.

Every measured invocation reuses its case's workspace and is compared with an expectation established on a fresh one. The tests also compare each unit fixture after the workspace served a success, a failure, and a cancellation. The logical work records each count as exact, as a lower bound when the operation stopped past it, as unavailable when this harness does not count it, or as not applicable when the path does no such work.

`vp run bench:authoring-js:smoke` captures one run, writes its records, reads them back, and re-admits them; withholding any one record is refused. The numbers are for presentation only: nothing compares them with a threshold.

## What this crate never does

- It retrieves no file, package, or network resource, and evaluates no host code.
- It never picks a grammar from a file name, a suffix, or the source.
- It never reads facts out of a tree the parser recovered.
- It never re-derives what a message means. Every semantic decision is `intlify_authoring`'s.
- No result borrows from the workspace. The arena is reset before every unit, and ranges, counts and diagnostics are copied out first.

## Current status

This is an unpublished, workspace-internal crate. Implemented:

- the authoring profile pin and the closed grammar registry;
- unit admission, with the declared scope and completeness;
- one parse and one semantic build per unit, with host syntax errors reported as failed units;
- the host cooked decoder and its input map, checked against the parser and against hand-derived fixtures;
- intrinsic bindings, and the explicit forms `intent`, `mf2` and `noIntent`, handed to `intlify_authoring` with their input maps;
- bounded DOM recognition with all-path receiver evidence, and the `text-content` usage profile;
- `@intlify` metadata, attached to exactly one declaration;
- assembling the analyzed units into a sealed `authoring-inventory`, checked against the declared scope and the same however the units were scheduled;
- limits for units, bytes, syntax tree nodes, input map segments, references, exclusions, parameters, alias chains, tracked origins, proof steps and annotation bytes, a reusable workspace, and cancellation. A bound on one literal, one use site, one annotation or one function's proof — and the shared crate's bounds on one message — blocks what it bounds with an `authoring-resource-limit` record, and the rest of the unit is still read. A bound on the invocation or a whole unit is an operational failure;
- design 026 measurement of source discovery and inventory assembly.

As in `intlify_authoring`, the only context kind this phase admits is the test context. A context claiming a production kind is refused before anything is read, because production admission needs checked 015 inputs that later phases supply.
