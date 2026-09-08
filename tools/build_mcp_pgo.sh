#!/usr/bin/env bash
# Build rustwright-mcp with fat LTO + PGO for release packaging.
# Training is host-safe (MCP protocol + bin unit tests); no browser.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
profdir="${PGO_PROFILE_DIR:-/tmp/rustwright-mcp-pgo-data}"
toolchain="${RUSTWRIGHT_PGO_TOOLCHAIN:-stable}"
profdata_bin="$(rustc "+${toolchain}" --print sysroot)/lib/rustlib/x86_64-unknown-linux-gnu/bin/llvm-profdata"
binary="${root}/mcp/target/release/rustwright-mcp"
manifest="${root}/mcp/Cargo.toml"

if [[ ! -x "$profdata_bin" ]]; then
  echo "llvm-profdata missing; install with: rustup component add llvm-tools-preview --toolchain ${toolchain}" >&2
  exit 127
fi

mkdir -p "$profdir"
find "$profdir" -mindepth 1 -delete
export CARGO_INCREMENTAL=0

echo "==> instrumented build" >&2
cargo "+${toolchain}" clean --manifest-path "$manifest" --release >/dev/null
RUSTFLAGS="-Cprofile-generate=${profdir}" \
  cargo "+${toolchain}" build --manifest-path "$manifest" --release --locked

echo "==> train" >&2
export LLVM_PROFILE_FILE="${profdir}/mcp-%p-%m.profraw"
for _ in $(seq 1 8); do
  python3 "${root}/tools/pgo_train_mcp.py" "$binary" --rounds 40 --repetitions 1 >/dev/null
done
RUSTFLAGS="-Cprofile-generate=${profdir}" \
  cargo "+${toolchain}" test --manifest-path "$manifest" --release --locked --bin rustwright-mcp >/dev/null

echo "==> merge profiles" >&2
raw_count="$(find "$profdir" -name '*.profraw' | wc -l | tr -d ' ')"
if [[ "$raw_count" -eq 0 ]]; then
  echo "no .profraw files under ${profdir}" >&2
  exit 1
fi
find "$profdir" -name '*.profraw' -print0 | sort -z | xargs -0 "$profdata_bin" merge -o "${profdir}/merged.profdata"

echo "==> profile-use release build" >&2
cargo "+${toolchain}" clean --manifest-path "$manifest" --release >/dev/null
RUSTFLAGS="-Cprofile-use=${profdir}/merged.profdata" \
  cargo "+${toolchain}" build --manifest-path "$manifest" --release --locked

stat -c '%s %n' "$binary"
file "$binary"
