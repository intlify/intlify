# Phase 1 parser fixtures

Each fixture is two files alongside one another:

```
<name>.mf2     # raw input text
<name>.snap    # newline-delimited snapshot driven by the fixture runner
```

Optional metadata lives in `<name>.meta` as `key = value` lines:

```
spec_section = "syntax.md#simple-message"
production = "simple-message"
description = "empty simple message"
diagnostics_expected = "0"
```

Buckets:

- `spec/` — conformance fixtures derived from `refers/message-format-wg/spec`. The `spec_section` field points back at the source-of-truth section.
- `recovery/` — malformed inputs. `diagnostics_expected` is checked.
- `implementation/` — non-normative, derived from `refers/messageformat`, `refers/mf2-tools`, `refers/formatjs`, or `refers/ox-content`. Mark `non_normative = true` in the meta.
- `generated/` — large or synthetic inputs (added with Milestone 11).

The runner at `crates/ox_mf2_parser/tests/fixtures.rs` walks the buckets and:

1. Reads the input.
2. Parses with `parse_source`.
3. Renders a deterministic snapshot of `CstTables` + diagnostics.
4. Diffs the snapshot against the on-disk `.snap` file.

To regenerate snapshots after an intentional grammar change, run:

```sh
UPDATE_SNAPSHOTS=1 cargo test -p ox_mf2_parser --test fixtures
```

`refers/message-format-wg` submodule SHA used when fixtures were last regenerated is recorded in `fixtures/CONFORMANCE.md`.
