# Phase 1 conformance report

This file tracks which spec snapshot the fixtures were generated against.

| Reference repository | Submodule path | Pinned commit | Role |
| --- | --- | --- | --- |
| Unicode MessageFormat WG | `refers/message-format-wg` | `d115a614079678850aac8b52742360e888b8f027` | normative grammar + spec fixtures |
| TC39 proposal-intl-messageformat | `refers/proposal-intl-messageformat` | tracked via `.gitmodules` | non-normative API & runtime fixtures |
| `refers/messageformat`, `refers/mf2-tools`, `refers/formatjs`, `refers/ox-content` | external implementations | tracked via `.gitmodules` | compatibility / regression / benchmark inputs |

Fixture roles:

- `fixtures/spec/` is normative; every `*.meta` records the `spec_section` it derives from.
- `fixtures/recovery/` is malformed-input regression material. The parser's first useful diagnostic for each is anchored in the snapshot.
- `fixtures/implementation/` is non-normative; flag with `non_normative = true` in the meta and record the upstream source.
- `fixtures/generated/` (added with Milestone 11) holds large synthetic batches used by the benchmark harness.

When the WG spec changes, refresh `refers/message-format-wg`, re-run `UPDATE_SNAPSHOTS=1 cargo test -p ox_mf2_parser --test fixtures`, and update both the commit row above and the per-fixture `spec_section` / `production` fields where the production names changed.
