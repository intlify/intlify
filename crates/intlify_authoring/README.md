# intlify_authoring

Language-neutral authoring semantics for [Intlify](../../design/000-intlify-overview-design.md), a toolchain for compiling localization from application source.

> [!IMPORTANT]
>
> This crate implements Phase 1 of [design 016](../../design/016-intlify-source-authoring-and-intent-identity-design.md). It assigns no persistent identity and reads no host language. A checked result is authoring evidence, not an identity-resolved authoring result. See [Current status](#current-status).

`intlify_authoring` holds the meaning that every host Producer shares: what a message _is_, what it requires of its use site, and when two messages are the same. A Producer for JavaScript, for Vue, or for any other host recognises its own syntax and then hands the decoded result here, so that the same source text does not acquire different semantics depending on which host found it.

## Role in Intlify

Three things have to agree across hosts, and all three live here.

**Displayed text and authored MF2 mean the same thing.** Ordinary UI text is encoded as an MF2 quoted pattern and then analysed by exactly the same parser and semantic pipeline as explicitly authored MF2. Braces in displayed text therefore never become interpolation merely because an application enabled localization.

**A message's requirements come from the message.** Which external parameters a use site must supply is read from the parsed message, not from a convention about how the call was written.

**Sameness is decided once.** Two messages have the same Intent revision when their projection is the same value. That projection, and the digest taken over it, decide when a translation stays valid and when it must be revisited, so they cannot be re-derived differently by each host.

## What a Producer supplies

A host Producer is responsible for everything language-specific, and supplies this crate with the result:

- the already decoded message text, as displayed text or as authored MF2;
- an `Occurrence` locating the declaration inside a `SourceSnapshot`;
- where that decoded text came from, as an `InputSegment` map, when the host decoded anything;
- the metadata values it recognised — source locale, surface class, description;
- the parameter names supplied at the use site, in the host's evaluation order, or nothing at all when a declaration has no use site yet;
- an `AuthoringContext` giving the checked inputs: the owner, the default source locale, the exact surface-class vocabulary, and a locale canonicalisation.

`resolve_declarations` then returns one `AuthoringResult` for the batch.

Two of those are worth stating as facts rather than as fields. Supplying an input map is what moves the returned extraction map into host coordinates; omitting it leaves the map in the coordinates of the supplied text, which is a different fact and not a missing one. Supplying no parameters at all says the declaration has no use site in this batch, which is what a reusable declaration looks like before anything references it; supplying an empty list says a use site supplied nothing, and a message that requires a name then reports it missing.

## What this crate never does

These are guarantees, not current limitations:

- It retrieves no file, network, registry, or plugin, evaluates no host code, and holds no mutable global state.
- It never substitutes a requested locale, a host locale, `und`, or a language inferred from the text. A wrong source locale mistranslates silently instead of failing, so an absent basis blocks the declaration.
- It never invents a surface class from a file path, a component name, or a DOM tag. Membership in the vocabulary is exact.
- It never normalises literal content. The parser normalises a cooked literal so that matching works, but a revision has to carry the actual content, or two variant keys the parser matches identically would merge into one revision.
- It never evaluates or serialises a use-site expression. Only its position is retained.
- It never claims a byte-for-byte correspondence it was not given. Composing two maps slices only where a run asserts a positional correspondence, so an escape resolves to the character it escaped and never to the whole literal around it.

## Outcomes

One invocation produces one of three things, and they are deliberately not interchangeable:

| Outcome | Meaning | Reachable facts |
| --- | --- | --- |
| Checked | Every declaration in the scope resolved | `checked()` returns them all |
| Blocked | At least one declaration could not be resolved | `checked()` returns `None`; `inspection_facts()` returns what was independently established |
| Operational failure | The invocation could not run — a bound was exhausted, a provider could not answer, or a context input was invalid | `Err`, with no diagnostics to show an author |

A blocked result keeps its facts because they support inspection, and puts them behind a differently named accessor because they are not the complete authoring input a build requires. An operational failure is separated from a diagnostic because the author wrote nothing wrong and cannot fix it by editing source.

## Current status

This is an unpublished, workspace-internal crate.

Implemented:

- literal encoding with its complete extraction map, and the MF2 parser handoff, sharing one semantic pipeline;
- external parameter requirements, and their comparison with the use site;
- source-locale, surface-class, usage and description resolution against a checked context;
- design 017's semantic projection, its committed JSON Schema, and the Intent revision taken over it, with independent vectors re-hashed by a separate implementation;
- observational measurement of four operations, producing design 026's records through the shared implementation in `intlify_measurement`.

The fixture matrix in [`fixtures/phase1/README.md`](fixtures/phase1/README.md) maps each requirement to the test that pins it, and names the rows a later phase owns.

## What Phase 2 adds

A host Producer is the next phase's work. The entry points it will use already exist and are not expected to change shape:

- `resolve_declarations(context, inputs, limits, workspace)` for a batch, or `resolve_declarations_with_cancellation` when the caller owns a probe;
- `AuthoringResult`'s accessors for facts and diagnostics;
- `compare_parameters` for a reference, whose declaration was analysed separately;
- `compose_extraction_map` to carry the extraction map back through the host's own decoding;
- `SourceSnapshot::verify` to turn supplied bytes into evidence before addressing ranges in them;
- `AuthoringLimits` for the bounds, and `AnalysisWorkspace` for scratch reuse across declarations.

What Phase 2 has to add on top:

- **Host analysis** — source discovery, intrinsic bindings, UI surface recognition, annotation syntax, and exclusion markers.
- **Reading host escapes** — producing the input map this crate composes with, from the host's own decoding rules.
- **Usage profile registration** — semantic usage is admitted only under a registered profile, and Phase 1 admits none from production.
- **References and inventory** — enumerating reference sites and assembling the authoring inventory.

Phase 3 adds identity: allocating a `MessageIntentId`, the registry artifacts and their codecs, and reconciling declaration history. Production locale canonicalisation is design 015's Phase 2; the provider trait duplicated here is unified at that integration.
