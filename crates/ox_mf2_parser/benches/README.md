# Phase 1 benchmark harness

The Phase 1 parser ships two benchmark integration points:

1. **In-crate CLI** — `src/bin/ox_mf2_bench.rs` builds as `ox-mf2-bench`. It exposes one phase per invocation (`parse_message_owned`, `parse_cst`, `parse_cst_no_trivia`, `lower_semantic`, `cst_view_traversal`, `diagnostics`, `source_mapping`, `parse_batch_sequential`) plus the reproducibility switches required by `design/002` §"Benchmark Policy": `--reuse-workspace` / `--no-reuse-workspace`, `--reserve` / `--no-reserve`, `--collect-trivia` / `--no-collect-trivia`, `--parse-semantic`, `--iterations N`, and `--input` / `--input-text` / `--corpus`.

   ```sh
   cargo run --release -p ox_mf2_parser --bin ox-mf2-bench -- \
     --phase parse_cst --iterations 10000 --input-text 'Hello, {$name}!'
   ```

2. **External baseline tool** — `tools/mf-parser-bench/rs` now ships two extra targets, `ox-mf2-parse` and `ox-mf2-parse-and-lower`, that sit alongside the existing `ox-content-*` / `mf2-tools-*` targets so the JS-orchestrated comparison runs all parsers against the same corpus.

`benches/hyperfine.sh` is a thin wrapper that builds the release binary and runs hyperfine across `parse_message_owned`, `parse_cst`, `parse_cst_no_trivia`, `lower_semantic`, and batch-style `owned_materialize`. Each phase is invoked separately so the resulting hyperfine summary keeps the phases on distinct rows rather than collapsing them into one number.

When publishing comparison numbers, always state:

- the allocator (system / mimalloc / jemalloc / …)
- whether CLI startup time is included
- the corpus identity and size
- whether `--reuse-workspace`, `--reserve`, `--collect-trivia`, and `--parse-semantic` were on
- the host (CPU + OS) and the hyperfine warmup / runs settings

The matching policy lives in `design/002-ox-mf2-phase-1-rust-parser-design.md` §"Benchmark Policy" and `design/002-ox-mf2-phase-1-rust-parser-design.md` §"Reproducibility policy".
