#!/usr/bin/env bash
# Compare rustwright-mcp process RSS: fat-LTO baseline vs PGO.
# Rebuilds both binaries; reuses PGO_PROFILE_DIR/merged.profdata when present.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
outdir="${1:-.benchmark-data/pgo-rss}"
profdir="${PGO_PROFILE_DIR:-/tmp/rustwright-mcp-pgo-data}"
toolchain="${RUSTWRIGHT_PGO_TOOLCHAIN:-stable}"
binary="${root}/mcp/target/release/rustwright-mcp"
manifest="${root}/mcp/Cargo.toml"
merged="${profdir}/merged.profdata"

mkdir -p "$outdir"
export CARGO_INCREMENTAL=0
export OUTDIR="$outdir"

build_one() {
  local label="$1"
  shift
  echo "==> build ${label}" >&2
  cargo "+${toolchain}" clean --manifest-path "$manifest" --release >/dev/null
  env "$@" cargo "+${toolchain}" build --manifest-path "$manifest" --release --locked
  stat -c '%s' "$binary" | tee "${outdir}/${label}.bytes"
  cp -f "$binary" "${outdir}/rustwright-mcp.${label}"
}

measure_one() {
  local label="$1"
  local path="${outdir}/rustwright-mcp.${label}"
  python3 "${root}/tools/pgo_train_mcp.py" "$path" \
    --rounds 80 \
    --repetitions 9 \
    --idle-settle-s 0.5 \
    --json-out "${outdir}/${label}.rss.json"
}

build_one baseline RUSTFLAGS="${RUSTFLAGS_BASE:-}"
measure_one baseline

if [[ ! -f "$merged" ]]; then
  echo "missing ${merged}; run tools/measure_mcp_pgo.sh first to collect profiles" >&2
  exit 2
fi
build_one pgo RUSTFLAGS="-Cprofile-use=${merged}"
measure_one pgo

python3 - <<'PY'
import json
import os
from pathlib import Path

outdir = Path(os.environ["OUTDIR"])
baseline = json.loads((outdir / "baseline.rss.json").read_text())
pgo = json.loads((outdir / "pgo.rss.json").read_text())
baseline_bytes = int((outdir / "baseline.bytes").read_text())
pgo_bytes = int((outdir / "pgo.bytes").read_text())

def delta(a, b):
    if a is None or b is None:
        return None
    return b - a

def pct(a, b):
    if a in (None, 0) or b is None:
        return None
    return round((b - a) / a * 100, 2)

summary = {
    "metric": "rustwright-mcp process RSS baseline vs PGO",
    "workload": "initialize + tools/list x80 + browser_status; idle settle 0.5s; 9 reps",
    "note": "VmRSS of rustwright-mcp only (no Chromium). Code-size driven RSS, not page-store RSS.",
    "baseline": {
        "bytes": baseline_bytes,
        "rss_peak_kb_median": baseline["rss_peak_kb"]["median"],
        "rss_idle_kb_median": baseline["rss_idle_kb"]["median"],
        "rss_hwm_kb_median": baseline["rss_hwm_kb"]["median"],
    },
    "pgo": {
        "bytes": pgo_bytes,
        "rss_peak_kb_median": pgo["rss_peak_kb"]["median"],
        "rss_idle_kb_median": pgo["rss_idle_kb"]["median"],
        "rss_hwm_kb_median": pgo["rss_hwm_kb"]["median"],
    },
    "delta": {
        "bytes": pgo_bytes - baseline_bytes,
        "bytes_pct": pct(baseline_bytes, pgo_bytes),
        "rss_peak_kb_median": delta(baseline["rss_peak_kb"]["median"], pgo["rss_peak_kb"]["median"]),
        "rss_peak_pct": pct(baseline["rss_peak_kb"]["median"], pgo["rss_peak_kb"]["median"]),
        "rss_idle_kb_median": delta(baseline["rss_idle_kb"]["median"], pgo["rss_idle_kb"]["median"]),
        "rss_idle_pct": pct(baseline["rss_idle_kb"]["median"], pgo["rss_idle_kb"]["median"]),
    },
}
text = json.dumps(summary, indent=2)
print(text)
(outdir / "summary.json").write_text(text + "\n", encoding="utf-8")
PY
