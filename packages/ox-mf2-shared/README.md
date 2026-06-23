# @intlify/ox-mf2-shared

Shared public TypeScript constants for ox-mf2 language bindings.

## Sync policy

`src/error-codes.ts` numeric values MUST match the Rust guard tests in `crates/ox_mf2_parser/tests/snapshot_compat.rs` and `crates/ox_mf2_parser/tests/error_codes.rs`.

When adding or remapping an API error code in Rust, update this file in the same commit.
