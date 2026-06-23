# @intlify/ox-mf2-shared

Shared public TypeScript types, constants, option validators, error classes, and test normalization helpers for ox-mf2 language bindings.

## Sync policy

Numeric values in `src/constants.ts` and `src/error-codes.ts` MUST match the Rust guard tests in `crates/ox_mf2_parser/tests/snapshot_compat.rs` and `crates/ox_mf2_parser/tests/error_codes.rs`.

When adding or remapping a parser kind, diagnostic kind, section kind, or API error code in Rust, update this package in the same commit.

Published N-API and WASM packages must not depend on this private package at runtime. Their build steps copy or bundle the shared declarations and helpers.
