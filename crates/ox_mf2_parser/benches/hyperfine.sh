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
# Phases are grouped so cross-parser comparisons pick the right baseline.

# Parser-core baselines — compare with other parsers.
hyperfine \
  --warmup "${WARMUP}" \
  --runs "${RUNS}" \
  --command-name 'parser-core: parse_cst_no_trivia' \
  "${BIN} --phase parse_cst_no_trivia --iterations ${ITERATIONS} --reuse-workspace --reserve ${INPUT_FLAG[*]}"

hyperfine \
  --warmup "${WARMUP}" \
  --runs "${RUNS}" \
  --command-name 'parser-core: parse_cst (with trivia)' \
  "${BIN} --phase parse_cst --iterations ${ITERATIONS} --reuse-workspace --reserve ${INPUT_FLAG[*]}"

# Optional / downstream cost — pair with one of the parser-core baselines.
hyperfine \
  --warmup "${WARMUP}" \
  --runs "${RUNS}" \
  --command-name 'downstream: lower_semantic' \
  "${BIN} --phase lower_semantic --iterations ${ITERATIONS} --reuse-workspace --reserve ${INPUT_FLAG[*]}"

hyperfine \
  --warmup "${WARMUP}" \
  --runs "${RUNS}" \
  --command-name 'downstream: owned_materialize (clone-only)' \
  "${BIN} --phase owned_materialize --iterations ${ITERATIONS} --reuse-workspace --reserve ${INPUT_FLAG[*]}"

# Convenience API — NOT a parser-core baseline (includes fresh sources +
# workspace + line-index build + owned materialise on every iteration).
hyperfine \
  --warmup "${WARMUP}" \
  --runs "${RUNS}" \
  --command-name 'convenience: parse_message_owned' \
  "${BIN} --phase parse_message_owned --iterations ${ITERATIONS} ${INPUT_FLAG[*]}"
