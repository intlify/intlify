<!-- @license MIT -->

# Phase 2 fixtures

Host source fixtures for the JavaScript and TypeScript Producer. Each one is a file of exact bytes and a case that says what those bytes are and what reading them must give. The complete matrix from each design 016 fixture family to the test that checks it is written when the phase closes.

## Data files

| File | Contents | Read by |
| --- | --- | --- |
| `extraction.json` | The host-side cases of 016's Extraction family: what each literal in a file decodes to, and where each run of decoded text came from | `src/cooked/tests.rs` |
| `extraction/*` | The source files those cases name | the same test, through the case's digest |

## Exact bytes

Several files carry bytes a checkout or a formatter would otherwise rewrite: CRLF inside a template, a line continuation before CRLF, a byte order mark. The directory is marked `-text` in `.gitattributes` and left out of formatting and linting in `vite.config.ts`.

Neither setting is relied on alone. Every case names its file's byte length and SHA-256 digest, and the test checks the file against them before decoding anything, the same way a Producer checks supplied bytes against a snapshot. A file whose line endings were rewritten fails there, instead of passing against different source than the case describes.

## Expected values

Every expected value was derived by hand from the ECMAScript grammar and the file's bytes, not copied from the decoder. A string literal's span includes its quotes; a template element's span is its content alone. A decoded literal gives its text and its input map as `[decodedStart, decodedEnd, sourceStart, sourceEnd]` runs. A literal the profile does not accept gives the form and the source range of the escape responsible.

The decoder's output is also compared with the parser's own cooked value on every literal. Hand-derived values and the second decoder check different things: the first catches a decoder that agrees with the parser on the text but maps it to the wrong bytes, and the second catches a decoder whose text is wrong.
