# Phase 1 performance review checklist

Before merging anything that touches the parser hot path, the scanner, recovery, or the CST/SemanticModel records, walk this list. It mirrors the questions from `design/002-ox-mf2-phase-1-rust-parser-design.md` §"Regression Rules" and pins them to actionable checks against this repo.

## Layout

- [ ] `cargo test -p ox_mf2_parser --test performance_guards` is green. `record_sizes_stay_within_budget` (in `src/tables.rs`) and `syntax_kind_stays_a_u16` (in `tests/performance_guards.rs`) lock `CstNodeRecord <= 24 B`, `CstEdgeRecord <= 8 B`, `TokenRecord <= 24 B`, `TriviaRecord <= 16 B`, `SyntaxKind = 2 B`, `Span = 8 B`.
- [ ] No new field was added to `SyntaxKind` between numeric values — additions go at the end of their category (see `src/syntax_kind.rs` `category_boundaries_are_stable`).

## Success path

- [ ] `valid_input_does_not_emit_diagnostics` covers the input you changed. If not, add a fixture under `fixtures/spec/` and rerun `UPDATE_SNAPSHOTS=1 cargo test -p ox_mf2_parser --test fixtures`.
- [ ] Diagnostic emit sites do not allocate message strings on success paths (use `DiagnosticCode::static_message`, not `format!`).
- [ ] `workspace_reuse_does_not_regrow_capacity` still passes — if you added a buffer to `ParserWorkspace` / `SemanticWorkspace`, make sure `clear()` keeps capacity and add a parallel assertion.

## Recovery

- [ ] Recovery fixtures under `fixtures/recovery/` still match. A drift means the snapshot or `diagnostics_expected` count needs an intentional bump — never blanket-regenerate.
- [ ] `recovery_does_not_cascade` still emits exactly one diagnostic per malformed-input case.
- [ ] First useful diagnostic span is anchored at the byte offset where the malformed production starts (not at EOF, not at zero).

## Trivia / source fidelity

- [ ] `collect_trivia = true` still preserves trivia spans needed for preserve-mode formatting (`fixtures/spec/*.snap` rows beginning with `TOKEN`).
- [ ] `collect_trivia = false` keeps token spans and diagnostic spans identical to the default mode.

## Phase boundary

- [ ] No semantic validation moved into the parser path. If you needed a check beyond syntax, route it through `SemanticModel` lowering (Milestone 8) instead.
- [ ] Did the parser hot path lose any work that the SemanticModel now does eagerly? If yes, document it in the PR description so `parse_message_owned` vs `lower_semantic` benchmark numbers stay comparable.

## Benchmarks

- [ ] `crates/ox_mf2_parser/benches/hyperfine.sh` runs cleanly. Numbers do not need to improve for every change — but a regression must be acknowledged in the PR description.
- [ ] When publishing comparison numbers, include the seven facts from `benches/README.md` ("When publishing comparison numbers, always state…").

### Phase categories — pick the right baseline before quoting numbers

The `ox-mf2-bench` CLI groups phases by what they measure. Quoting a number from the wrong category leads to misleading cross-parser comparisons.

- **Parser-core baselines** (compare with other parsers):
  - `parse_cst_no_trivia` — workspace-reused, borrowed result, no trivia
  - `parse_cst` — same, with trivia
- **Optional / downstream cost** (always include alongside a baseline):
  - `lower_semantic` — parser-core + `SemanticModel` lowering
  - `owned_materialize` — only the `CstTables.clone()` + diagnostic materialise cost paid by `parse_source` / `parse_message` on top of the borrowed-session path
- **Convenience APIs** (NOT parser-core, include extra setup work):
  - `parse_message_owned` — fresh `SourceStore` + line-index + fresh `ParseWorkspace` + owned materialise per iteration

When publishing numbers, name the phase explicitly (e.g. "ox-mf2 parse_cst_no_trivia") so readers know what is being timed.

## Snapshots

- [ ] `cargo test -p ox_mf2_parser --test fixtures` is green. If you intentionally changed parser shape, regenerate with `UPDATE_SNAPSHOTS=1 cargo test -p ox_mf2_parser --test fixtures` and review every diff manually before committing — accidental reshapes are exactly what the snapshots are here to catch.
