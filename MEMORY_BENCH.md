# Speed and memory benchmark

`bench-full` records timing and peak resident memory in the same JSON result for
every implementation run. Memory collection is always on; no additional flag is
required.

Each passed item in `results` contains a `memory` block with:

- `rss_self_kb`: peak RSS of the benchmark's Python process. For Rustwright,
  this includes the in-process Rust client. For `playwright-python`, it includes
  the Python client but not its Node driver.
- `rss_tree_kb`: peak summed RSS of that Python process and all descendants,
  including the driver and Chromium process tree.
- sampling provenance, interval, availability, and sample count.

On Linux, the sampler reads parent PIDs and `VmRSS` directly from
`/proc/<pid>/status`. Other platforms fall back to `ps`. It samples every 50 ms
and retains independent peaks for the root process and the full process tree. It
takes one baseline before the implementation run, then samples on a daemon
background thread. Case timers still wrap only the operation under test.

If neither backend yields complete positive `rss_self_kb` and `rss_tree_kb`
values in KiB, both fields are unavailable for canonical evidence. `bench-full`
treats that run as failed, so the matrix cannot publish a latency-only or
partial-tree comparison.

Whole-tree RSS is normally Chromium-dominated. Report `rss_tree_kb` for the
actual end-to-end process cost and `rss_self_kb` alongside it as the available
library-host portion. The latter is not perfectly symmetric:
`playwright-python` puts additional client logic in its Node child, while
Rustwright keeps its client in the measured Python process. The harness does not
attempt to classify driver children separately from Chromium, because command
names and process layouts vary by browser build and platform.

For release **binary** cost (size / strip / LTO), use the host-safe helper
`tools/measure_release_binary_cost.sh` — no browser launch. Keep raw outputs
under ignored `.benchmark-data/`.

For MCP **process RSS vs Playwright MCP**, use `tools/compare_mcp_rss.py`
(host-safe `initialize` + `tools/list`; no Chromium). On linux x86_64:

| MCP server | Median process VmRSS |
|---|---:|
| `rustwright-mcp` | 5.0 MiB (5128 KiB) |
| `@playwright/mcp` 0.0.80 | 135.2 MiB (138456 KiB) |

≈ **27×** smaller / **−96%** for the MCP server process alone.

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

For repeated runs, the raw peak remains in every `results` item and `aggregate`
contains distribution summaries for both RSS fields. Benchmark output belongs
under the ignored `.benchmark-data/` directory; do not commit raw result JSON,
terminal logs, or generated reports.

## Exact Testbox dispatch

From the repository root, this command warms a Blacksmith Testbox, builds the
benchmark image, runs only Rustwright and `playwright-python`, downloads the
ignored JSON artifact, and records strict-suite speed plus memory together:

```bash
RUSTWRIGHT_TESTBOX_DOWNLOAD_RESULTS=1 tools/run_benchmark_testbox.sh -- 'set -euo pipefail; mkdir -p .benchmark-data/results; timestamp="$(date -u +%Y%m%dT%H%M%SZ)"; BENCHMARK_FULL_ITERATIONS=10 TEST_DOCKER_MEMORY_LIMIT=8g RUSTWRIGHT_DOCKER_IMAGE=rustwright-verify-testbox tools/docker_test.sh bench-full --impl rustwright --impl playwright --suite strict --lifecycle warm-browser --repetitions 3 --output ".benchmark-data/results/bench-full-strict-speed-memory-${timestamp}.json" --json'
```

For a local Docker preflight using an already-built image, run:

```bash
BENCHMARK_FULL_ITERATIONS=10 TEST_DOCKER_MEMORY_LIMIT=8g tools/docker_test.sh bench-full --impl rustwright --impl playwright --suite strict --lifecycle warm-browser --repetitions 3 --output .benchmark-data/results/bench-full-strict-speed-memory.json --json
```
