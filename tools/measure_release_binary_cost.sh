#!/usr/bin/env bash
# Measure release binary size for rustwright-mcp (and optionally rustwright-cli).
# Host-safe: no browser launch. Pair with MEMORY_BENCH.md / Testbox for RSS.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
outdir="${1:-.benchmark-data/release-cost}"
mkdir -p "$outdir"

measure_one() {
  local manifest="$1"
  local bin_name="$2"
  local label="$3"
  local target_dir
  target_dir="$(dirname "$manifest")/target/release"
  echo "==> building ${label} (release)" >&2
  cargo +stable build --manifest-path "$manifest" --release --locked
  local path="${target_dir}/${bin_name}"
  local bytes
  bytes="$(stat -c '%s' "$path")"
  local sha
  sha="$(sha256sum "$path" | awk '{print $1}')"
  printf '%s\n' "$bytes" >"${outdir}/${label}.bytes"
  printf '%s\n' "$sha" >"${outdir}/${label}.sha256"
  python3 -c '
import json, sys
label, path, bytes_s, sha = sys.argv[1:5]
bytes_i = int(bytes_s)
print(json.dumps({
    "label": label,
    "path": path,
    "bytes": bytes_i,
    "mib": round(bytes_i / (1024 * 1024), 3),
    "sha256": sha,
}, indent=2))
' "$label" "$path" "$bytes" "$sha"
}

measure_one "${root}/mcp/Cargo.toml" rustwright-mcp mcp
if [[ "${MEASURE_CLI:-0}" == "1" ]]; then
  measure_one "${root}/cli/Cargo.toml" rustwright-cli cli
fi
