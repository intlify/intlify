# intlify_authoring_js

JavaScript and TypeScript host Producer for [Intlify](../../design/000-intlify-overview-design.md)'s source authoring.

> [!IMPORTANT]
>
> This crate implements part of Phase 2 of [design 016](../../design/016-intlify-source-authoring-and-intent-identity-design.md). It admits and reads source units and recognizes the explicit authoring forms `intent`, `mf2` and `noIntent`. Automatic DOM recognition and `@intlify` metadata are not implemented yet. See [Current status](#current-status).

A Producer reads host source and hands [`intlify_authoring`](../intlify_authoring/README.md) what it found, already decoded. What a message means, what its use site must supply, its locale, class and revision are all decided there, so the same text never means different things depending on which host found it. This crate owns the part specific to JavaScript: which bytes a unit is, which grammar reads them, and what a literal decodes to and from where.

## Reading a unit

An invocation runs in two steps.

`admit_units` checks everything the caller supplied before anything is parsed: the context's kind and profile pin, each unit's owner and grammar, its bytes against its snapshot, and the declared scope. Every refusal there is operational, because no edit to source could fix it. The one exception is a unit whose bytes are exactly what its snapshot names but are not UTF-8: that is the author's to fix, so the unit is admitted and reported as failed.

`analyze_unit` then reads one admitted unit, parsing it exactly once under the grammar its snapshot names. A unit is accepted only when the parser reports nothing, the semantic checks carrying the language's early errors report nothing, and a script contains no module syntax. Anything else rejects the whole tree, and the unit fails with one diagnostic at the earliest range the parser named. Units are analyzed independently, so a caller can run them in any order or on several workers.

An accepted unit is read for the explicit forms the profile registers, and what is found is handed to `intlify_authoring`. The result says whether the unit is checked, blocked or failed. Its facts are reachable through `checked()` only when every occurrence resolved; `inspection_facts()` returns what was independently established whatever the outcome, for inspection and not as complete input.

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

016 reads a message from the host's cooked value, so `intent('a\nb')` and a `mf2` template with the same escape both hand MF2 a line feed. The parser computes that value but not where each decoded byte came from. The decoder here reads the literal again from its source bytes, recording an `InputSegment` per run as it goes, and requires its text to equal the parser's byte for byte. A disagreement stops the invocation rather than choosing one reading.

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
- limits for units, bytes, syntax tree nodes, input map segments, references, exclusions and parameters, a reusable workspace, and cancellation.

The profile admits no DOM global yet, so ordinary UI text is not recognized automatically. The later changes add:

- bounded DOM recognition with all-path receiver evidence;
- `@intlify` metadata;
- assembling a checked `authoring-inventory` from the analyzed units;
- design 026 measurement of source discovery and inventory assembly.

As in `intlify_authoring`, the only context kind this phase admits is the test context. A context claiming a production kind is refused before anything is read, because production admission needs checked 015 inputs that later phases supply.
