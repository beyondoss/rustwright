#!/usr/bin/env bash
# Measure rustwright-mcp release cost with and without PGO.
# Host-safe training: tools/pgo_train_mcp.py + mcp lib tests (no browser).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
outdir="${1:-.benchmark-data/pgo}"
profdir="${PGO_PROFILE_DIR:-/tmp/rustwright-mcp-pgo-data}"
toolchain="${RUSTWRIGHT_PGO_TOOLCHAIN:-stable}"
profdata_bin="$(rustc "+${toolchain}" --print sysroot)/lib/rustlib/x86_64-unknown-linux-gnu/bin/llvm-profdata"
binary="${root}/mcp/target/release/rustwright-mcp"
manifest="${root}/mcp/Cargo.toml"

mkdir -p "$outdir" "$profdir"
find "$profdir" -mindepth 1 -delete
export CARGO_INCREMENTAL=0

if [[ ! -x "$profdata_bin" ]]; then
  echo "llvm-profdata missing; install with: rustup component add llvm-tools-preview --toolchain ${toolchain}" >&2
  exit 127
fi

build_release() {
  local label="$1"
  shift
  echo "==> building ${label}" >&2
  local start end bytes
  start="$(date +%s.%N)"
  env "$@" cargo "+${toolchain}" build --manifest-path "$manifest" --release --locked
  end="$(date +%s.%N)"
  python3 -c "import sys; print(round(float(sys.argv[2]) - float(sys.argv[1]), 3))" "$start" "$end" \
    >"${outdir}/${label}.build_seconds"
  bytes="$(stat -c '%s' "$binary")"
  printf '%s\n' "$bytes" | tee "${outdir}/${label}.bytes"
  sha256sum "$binary" | awk '{print $1}' >"${outdir}/${label}.sha256"
  file "$binary" >"${outdir}/${label}.file.txt"
}

measure_runtime() {
  local label="$1"
  python3 "${root}/tools/pgo_train_mcp.py" "$binary" \
    --rounds 50 \
    --repetitions 7 \
    --json-out "${outdir}/${label}.runtime.json"
}

echo "==> baseline fat LTO, no PGO" >&2
cargo "+${toolchain}" clean --manifest-path "$manifest" --release >/dev/null
build_release baseline RUSTFLAGS="${RUSTFLAGS_BASE:-}"
measure_runtime baseline

echo "==> instrumented generate build" >&2
cargo "+${toolchain}" clean --manifest-path "$manifest" --release >/dev/null
build_release generate RUSTFLAGS="-Cprofile-generate=${profdir}"

echo "==> training instrumented binary + lib tests" >&2
export LLVM_PROFILE_FILE="${profdir}/mcp-%p-%m.profraw"
for _ in $(seq 1 8); do
  python3 "${root}/tools/pgo_train_mcp.py" "$binary" --rounds 40 --repetitions 1 >/dev/null
done
RUSTFLAGS="-Cprofile-generate=${profdir}" \
  cargo "+${toolchain}" test --manifest-path "$manifest" --release --locked --bin rustwright-mcp >/dev/null

echo "==> merging profiles" >&2
raw_count="$(find "$profdir" -name '*.profraw' | wc -l | tr -d ' ')"
if [[ "$raw_count" -eq 0 ]]; then
  echo "no .profraw files produced under ${profdir}" >&2
  exit 1
fi
find "$profdir" -name '*.profraw' -print0 | sort -z | xargs -0 "$profdata_bin" merge -o "${profdir}/merged.profdata"
ls -la "${profdir}/merged.profdata" >"${outdir}/merged.profdata.stat.txt"
printf '%s\n' "$raw_count" >"${outdir}/profraw_count.txt"

echo "==> optimized profile-use build" >&2
cargo "+${toolchain}" clean --manifest-path "$manifest" --release >/dev/null
build_release pgo RUSTFLAGS="-Cprofile-use=${profdir}/merged.profdata"
measure_runtime pgo

python3 "${root}/tools/summarize_mcp_pgo.py" "$outdir"
