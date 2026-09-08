#!/usr/bin/env bash
# Build rustwright-mcp with fat LTO + PGO for release packaging.
# Training is host-safe (MCP protocol + bin unit tests); no browser.
#
# Environment:
#   RUSTWRIGHT_PGO_TOOLCHAIN  rustup toolchain (default: stable)
#   RUSTWRIGHT_PGO_TARGET     rustc target triple (default: host). Same-arch
#                             Linux musl on a GNU host is allowed for static
#                             release assets; other cross targets are rejected.
#   PGO_PROFILE_DIR           profile scratch dir
#
# Linux musl PGO uses the host musl-gcc (x86_64-linux-musl-gcc /
# aarch64-linux-musl-gcc). Do not use cargo-zigbuild for instrumented musl
# links — Zig rejects __llvm_profile_runtime. Plain (non-PGO) musl release
# builds still use cargo-zigbuild in CI.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
profdir="${PGO_PROFILE_DIR:-${RUNNER_TEMP:-/tmp}/rustwright-mcp-pgo-data}"
toolchain="${RUSTWRIGHT_PGO_TOOLCHAIN:-stable}"
target="${RUSTWRIGHT_PGO_TARGET:-}"
manifest="${root}/mcp/Cargo.toml"

host_triple="$(rustc "+${toolchain}" -vV | sed -n 's/^host: //p')"
if [[ -z "$target" ]]; then
  target="$host_triple"
fi

# PGO needs a runnable binary on this host. Exact host triples are fine; so is
# same-arch Linux musl on a GNU host (static musl binaries run there).
pgo_target_ok=0
if [[ "$target" == "$host_triple" ]]; then
  pgo_target_ok=1
else
  case "${host_triple}/${target}" in
    x86_64-unknown-linux-gnu/x86_64-unknown-linux-musl) pgo_target_ok=1 ;;
    aarch64-unknown-linux-gnu/aarch64-unknown-linux-musl) pgo_target_ok=1 ;;
  esac
fi
if [[ "$pgo_target_ok" -ne 1 ]]; then
  echo "PGO build requires a native-runnable target (host=${host_triple}, target=${target})" >&2
  exit 2
fi

profdata_bin="$(rustc "+${toolchain}" --print sysroot)/lib/rustlib/${host_triple}/bin/llvm-profdata"
case "$host_triple" in
  *-windows-*) binary_name="rustwright-mcp.exe" ;;
  *) binary_name="rustwright-mcp" ;;
esac
binary="${root}/mcp/target/${target}/release/${binary_name}"
# cargo also writes target/release/ for the host when --target matches host on
# some setups; prefer the explicit target path used by release packaging.
legacy_binary="${root}/mcp/target/release/${binary_name}"

if [[ ! -x "$profdata_bin" && ! -f "$profdata_bin" ]]; then
  echo "llvm-profdata missing at ${profdata_bin}" >&2
  echo "install with: rustup component add llvm-tools-preview --toolchain ${toolchain}" >&2
  exit 127
fi

case "$target" in
  x86_64-unknown-linux-musl)
    musl_cc="${CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER:-x86_64-linux-musl-gcc}"
    if ! command -v "$musl_cc" >/dev/null 2>&1; then
      echo "musl PGO requires ${musl_cc} (install musl-tools)" >&2
      exit 127
    fi
    export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER="$musl_cc"
    export CC_x86_64_unknown_linux_musl="${CC_x86_64_unknown_linux_musl:-$musl_cc}"
    ;;
  aarch64-unknown-linux-musl)
    musl_cc="${CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER:-aarch64-linux-musl-gcc}"
    if ! command -v "$musl_cc" >/dev/null 2>&1; then
      echo "musl PGO requires ${musl_cc} (install musl-tools)" >&2
      exit 127
    fi
    export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER="$musl_cc"
    export CC_aarch64_unknown_linux_musl="${CC_aarch64_unknown_linux_musl:-$musl_cc}"
    ;;
esac

mkdir -p "$profdir"
find "$profdir" -mindepth 1 -delete
export CARGO_INCREMENTAL=0

resolve_binary() {
  if [[ -f "$binary" ]]; then
    printf '%s\n' "$binary"
  elif [[ -f "$legacy_binary" ]]; then
    printf '%s\n' "$legacy_binary"
  else
    echo "built binary not found at ${binary} or ${legacy_binary}" >&2
    exit 1
  fi
}

echo "==> instrumented build (${toolchain}, ${target})" >&2
cargo "+${toolchain}" clean --manifest-path "$manifest" --release --target "$target" >/dev/null || true
RUSTFLAGS="-Cprofile-generate=${profdir}" \
  cargo "+${toolchain}" build --manifest-path "$manifest" --release --locked --target "$target"
binary="$(resolve_binary)"

echo "==> train" >&2
export LLVM_PROFILE_FILE="${profdir}/mcp-%p-%m.profraw"
for _ in $(seq 1 8); do
  python3 "${root}/tools/pgo_train_mcp.py" "$binary" --rounds 40 --repetitions 1 >/dev/null
done
RUSTFLAGS="-Cprofile-generate=${profdir}" \
  cargo "+${toolchain}" test --manifest-path "$manifest" --release --locked --target "$target" --bin rustwright-mcp >/dev/null

echo "==> merge profiles" >&2
raw_count="$(find "$profdir" -name '*.profraw' | wc -l | tr -d ' ')"
if [[ "$raw_count" -eq 0 ]]; then
  echo "no .profraw files under ${profdir}" >&2
  exit 1
fi
find "$profdir" -name '*.profraw' -print0 | sort -z | xargs -0 "$profdata_bin" merge -o "${profdir}/merged.profdata"

echo "==> profile-use release build" >&2
cargo "+${toolchain}" clean --manifest-path "$manifest" --release --target "$target" >/dev/null || true
RUSTFLAGS="-Cprofile-use=${profdir}/merged.profdata" \
  cargo "+${toolchain}" build --manifest-path "$manifest" --release --locked --target "$target"
binary="$(resolve_binary)"

if command -v stat >/dev/null 2>&1; then
  if stat -c '%s %n' "$binary" >/dev/null 2>&1; then
    stat -c '%s %n' "$binary"
  else
    stat -f '%z %N' "$binary"
  fi
fi
file "$binary" || true
