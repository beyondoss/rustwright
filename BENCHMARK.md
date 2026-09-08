# Benchmark Policy

Rustwright's supported product surfaces are the native **MCP** server and
**CLI**. Authoritative memory evidence for the MCP process lives in
[`MEMORY_BENCH.md`](MEMORY_BENCH.md) (`tools/compare_mcp_rss.py`,
`tools/build_mcp_pgo.sh`).

Language-binding / Playwright-API automation speed lanes were removed with the
Python and C-ABI binding packages. Do not revive those claims without a new
measurement path on MCP or CLI.

Browser-engine scaffolds under [`benchmarks/`](benchmarks/) remain available as
supporting page-load / synthetic signals. Keep raw outputs under ignored
`.benchmark-data/`; do not commit result JSON or generated reports.
