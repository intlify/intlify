#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
RUST_BIN="$ROOT_DIR/rs/target/release/mf-parser-bench-rs"

cd "$ROOT_DIR"

WARMUP="${WARMUP:-3}"
RUNS="${RUNS:-10}"
SHOW_OUTPUT="${SHOW_OUTPUT:-0}"

while (($# > 0)); do
  case "$1" in
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
    *)
      echo "Unknown argument: $1" >&2
      exit 1
      ;;
  esac
done

if ! command -v hyperfine >/dev/null 2>&1; then
  echo "hyperfine is required. Install it and re-run this script." >&2
  exit 1
fi

if [[ ! -x "$RUST_BIN" ]]; then
  echo "Rust runner is missing. Run: node scripts/setup.mjs" >&2
  exit 1
fi

mkdir -p results/raw results/normalized results/reports
mkdir -p .tmp/hyperfine-stderr

run_suite() {
  local suite="$1"
  local corpus="$2"
  shift 2

  local json="results/raw/${suite}.json"
  local markdown="results/raw/${suite}.md"
  local args=(
    --warmup "$WARMUP"
    --runs "$RUNS"
    --export-json "$json"
    --export-markdown "$markdown"
  )

  if [[ "$SHOW_OUTPUT" == "1" ]]; then
    args+=(--show-output)
  fi

  while (($# > 0)); do
    local runtime="$1"
    local target="$2"
    shift 2

    local iterations
    iterations="$(node scripts/read-calibration.mjs "$target" "$corpus")"

    local summary="results/normalized/${target}__${corpus}.json"
    local stderr=".tmp/hyperfine-stderr/${target}__${corpus}.stderr"
    local command
    if [[ "$runtime" == "js" ]]; then
      command="node js/run-parser.mjs --target $target --corpus $corpus --iterations $iterations --summary-json $summary 2> $stderr"
    else
      command="$RUST_BIN --target $target --corpus $corpus --iterations $iterations --summary-json $summary 2> $stderr"
    fi

    args+=(--command-name "$target" "$command")
  done

  hyperfine "${args[@]}"
}

run_suite \
  mf2-common-js \
  mf2-common \
  js messageformat-parse-message \
  js messageformat-parse-cst \
  js messageformat-cst-to-message \
  js messageformat-constructor

run_suite \
  mf2-common-rust \
  mf2-common \
  rust ox-content-parse \
  rust ox-content-parse-and-validate \
  rust mf2-tools-parse \
  rust mf2-tools-parse-and-analyze \
  rust ox-mf2-parse \
  rust ox-mf2-parse-and-lower

run_suite \
  mf2-app-js \
  mf2-app \
  js messageformat-parse-message \
  js messageformat-parse-cst \
  js messageformat-cst-to-message \
  js messageformat-constructor

run_suite \
  mf2-app-rust \
  mf2-app \
  rust ox-content-parse \
  rust ox-content-parse-and-validate \
  rust mf2-tools-parse \
  rust mf2-tools-parse-and-analyze \
  rust ox-mf2-parse \
  rust ox-mf2-parse-and-lower

run_suite \
  mf1-icu-js \
  mf1-icu \
  js formatjs-icu-parse
