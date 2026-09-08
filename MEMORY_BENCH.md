# Memory benchmark

## MCP process RSS vs Playwright MCP

Primary claim for this repository. Use `tools/compare_mcp_rss.py`
(host-safe `initialize` + `tools/list`; no Chromium). On linux x86_64:

| MCP server | Median process VmRSS |
|---|---:|
| `rustwright-mcp` | 5.0 MiB (5128 KiB) |
| `@playwright/mcp` 0.0.80 | 135.2 MiB (138456 KiB) |

≈ **27×** smaller / **−96%** for the MCP server process alone.

## MCP PGO

For MCP **PGO** before/after (size + protocol microbench + process VmRSS), use
`tools/measure_mcp_pgo.sh`, `tools/measure_mcp_pgo_rss.sh`, and
`tools/build_mcp_pgo.sh` — trains with `tools/pgo_train_mcp.py` plus
`cargo test --bin rustwright-mcp` (no browser; MCP process RSS only).

Measured on linux x86_64 against a fat-LTO baseline (host-safe; no Chromium):

- Binary: 7.753 MiB → 6.462 MiB (−16.7%)
- MCP process VmRSS median: 6340 KiB → 5416 KiB (−14.6%)
- Protocol microbench median stayed within noise

When attaching `rustwright-mcp` to a GitHub Release on a native-runnable
host/target, prefer `tools/build_mcp_pgo.sh` over a plain `cargo build
--release`. Release CI does this for Darwin arm64 and for Linux musl on
same-arch runners (`musl-tools` / musl-gcc — not Zig, which cannot link the
LLVM profile runtime). Cross builds without a runnable train (today:
`x86_64-apple-darwin` on arm64 macOS) stay on the non-PGO release profile.
Same-arch `*-linux-musl` on a GNU host is treated as native-runnable for PGO.
Plain musl CLI assets use `cargo zigbuild`.

## Release binary cost

For release **binary** cost (size / strip / LTO), use the host-safe helper
`tools/measure_release_binary_cost.sh` — no browser launch. Keep raw outputs
under ignored `.benchmark-data/`.
