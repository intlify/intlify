#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$ROOT_DIR"

WARMUP="${WARMUP:-1}"
RUNS="${RUNS:-2}"

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

mkdir -p results/raw results/reports

args=(
  --warmup "$WARMUP"
  --runs "$RUNS"
  --export-json results/raw/bindings-node.json
  --export-markdown results/raw/bindings-node.md
)

operations=(parse-message parse-batch decode-snapshot snapshot-to-bytes-copy)
runtimes=()

if node scripts/binding-runtime-available.mjs napi; then
  runtimes+=(napi)
else
  echo "Skipping N-API binding benchmarks: native artifact is unavailable." >&2
fi

if node scripts/binding-runtime-available.mjs wasm; then
  runtimes+=(wasm)
else
  echo "Skipping WASM binding benchmarks: dist artifacts are unavailable." >&2
fi

if ((${#runtimes[@]} == 0)); then
  echo "No binding runtimes are available. Run: node scripts/setup-bindings.mjs" >&2
  exit 1
fi

for runtime in "${runtimes[@]}"; do
  for operation in "${operations[@]}"; do
    iterations="$(node scripts/read-binding-calibration.mjs "${runtime}" "${operation}")"
    command_name="ox-mf2-${runtime}-${operation}"
    command="node js/run-binding.mjs --runtime ${runtime} --operation ${operation} --iterations ${iterations}"
    args+=(--command-name "$command_name" "$command")
  done
done

hyperfine "${args[@]}"