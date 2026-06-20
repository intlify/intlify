#!/usr/bin/env bash
# Run hyperfine over the Phase 1 ox-mf2-bench CLI phases for a single
# representative input. Adjust `--phase` / `--input` / `--iterations` to
# match the comparison you're making.
#
# Usage:
#   ./benches/hyperfine.sh                       # default sweep
#   ./benches/hyperfine.sh path/to/input.mf2     # benchmark one file

set -euo pipefail

INPUT="${1:-}"
ITERATIONS="${ITERATIONS:-10000}"
WARMUP="${WARMUP:-3}"
RUNS="${RUNS:-20}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${ROOT}/../../target/release/ox-mf2-bench"

if [ ! -x "${BIN}" ]; then
  echo "Building release ox-mf2-bench..." 1>&2
  cargo build --release -p ox_mf2_parser --bin ox-mf2-bench --quiet
fi

if [ -z "${INPUT}" ]; then
  INPUT_FLAG=(--input-text 'Hello, {$name}!')
else
  INPUT_FLAG=(--input "${INPUT}")
fi

# Phases reported here mirror design/002 §"Benchmark Policy". Each
# hyperfine command runs ONE phase so wall-clock numbers stay separated.
hyperfine \
  --warmup "${WARMUP}" \
  --runs "${RUNS}" \
  --command-name 'parse_message_owned' \
  "${BIN} --phase parse_message_owned --iterations ${ITERATIONS} ${INPUT_FLAG[*]}"

hyperfine \
  --warmup "${WARMUP}" \
  --runs "${RUNS}" \
  --command-name 'parse_cst (workspace reuse)' \
  "${BIN} --phase parse_cst --iterations ${ITERATIONS} --reuse-workspace --reserve ${INPUT_FLAG[*]}"

hyperfine \
  --warmup "${WARMUP}" \
  --runs "${RUNS}" \
  --command-name 'parse_cst_no_trivia' \
  "${BIN} --phase parse_cst_no_trivia --iterations ${ITERATIONS} --reuse-workspace --reserve ${INPUT_FLAG[*]}"

hyperfine \
  --warmup "${WARMUP}" \
  --runs "${RUNS}" \
  --command-name 'lower_semantic' \
  "${BIN} --phase lower_semantic --iterations ${ITERATIONS} --reuse-workspace --reserve ${INPUT_FLAG[*]}"
