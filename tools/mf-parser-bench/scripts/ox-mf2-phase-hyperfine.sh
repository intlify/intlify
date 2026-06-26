#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
MANIFEST="$ROOT_DIR/rs/Cargo.toml"
BIN="$ROOT_DIR/rs/target/release/ox-mf2-bench"

INPUT=""
INPUT_TEXT="Hello, {\$name}!"
ITERATIONS="${ITERATIONS:-10000}"
WARMUP="${WARMUP:-3}"
RUNS="${RUNS:-20}"
SHOW_OUTPUT="${SHOW_OUTPUT:-0}"

usage() {
  cat <<'USAGE'
Usage: bash tools/mf-parser-bench/scripts/ox-mf2-phase-hyperfine.sh [options]

Options:
  --input <path>        Read one MF2 input file.
  --input-text <text>   Use an inline MF2 input. Defaults to "Hello, {$name}!".
  --iterations <n>      Inner iterations per ox-mf2-bench invocation.
  --warmup <n>          Hyperfine warmup runs.
  --runs <n>            Hyperfine measured runs.
  --show-output         Show command output from hyperfine.
  --help                Print this help.
USAGE
}

while (($# > 0)); do
  case "$1" in
    --input)
      INPUT="$2"
      INPUT_TEXT=""
      shift 2
      ;;
    --input-text)
      INPUT_TEXT="$2"
      INPUT=""
      shift 2
      ;;
    --iterations)
      ITERATIONS="$2"
      shift 2
      ;;
    --warmup)
      WARMUP="$2"
      shift 2
      ;;
    --runs)
      RUNS="$2"
      shift 2
      ;;
    --show-output)
      SHOW_OUTPUT=1
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if ! command -v hyperfine >/dev/null 2>&1; then
  echo "hyperfine is required. Install it and re-run this script." >&2
  exit 1
fi

if [[ ! -x "$BIN" ]]; then
  echo "Building release ox-mf2-bench..." >&2
  cargo build --release --manifest-path "$MANIFEST" --bin ox-mf2-bench --quiet
fi

input_args=()
if [[ -n "$INPUT" ]]; then
  input_args=(--input "$INPUT")
else
  input_args=(--input-text "$INPUT_TEXT")
fi

hyperfine_args=(--warmup "$WARMUP" --runs "$RUNS")
if [[ "$SHOW_OUTPUT" == "1" ]]; then
  hyperfine_args+=(--show-output)
fi

command_string() {
  local args=("$@")
  local command
  printf -v command "%q " "${args[@]}"
  printf "%s" "$command"
}

add_phase() {
  local name="$1"
  local phase="$2"
  shift 2

  local command
  command="$(command_string \
    "$BIN" \
    --phase "$phase" \
    --iterations "$ITERATIONS" \
    "$@" \
    "${input_args[@]}")"

  hyperfine_args+=(--command-name "$name" "$command")
}

# Parser-core baselines: compare with other parsers.
add_phase \
  "parser-core: parse_cst_no_trivia" \
  parse_cst_no_trivia \
  --reuse-workspace \
  --reserve

add_phase \
  "parser-core: parse_cst (with trivia)" \
  parse_cst \
  --reuse-workspace \
  --reserve

# Optional / downstream cost: pair with a parser-core baseline.
add_phase \
  "downstream: lower_semantic" \
  lower_semantic \
  --reuse-workspace \
  --reserve

add_phase \
  "downstream: owned_materialize (clone-only)" \
  owned_materialize \
  --reuse-workspace \
  --reserve

# Convenience API: not a parser-core baseline.
add_phase \
  "convenience: parse_message_owned" \
  parse_message_owned

hyperfine "${hyperfine_args[@]}"
