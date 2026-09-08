# Benchmarks

See [BENCHMARK.md](../BENCHMARK.md) and [MEMORY_BENCH.md](../MEMORY_BENCH.md).

Rustwright's primary product surface is the native **MCP** / **CLI** stack.
Automation-library speed lanes that required the retired Python binding have
been removed. What remains here are browser-engine / page-load scaffolds and
MCP memory measurement helpers under `tools/`.

## Browser speed candidates

List the available browser-speed scaffolds:

```bash
python benchmarks/browser_speed/list.py
```

Setup commands place downloaded tools, cloned repos, and outputs under ignored
`.benchmark-data/`.

Useful starting points:

```bash
python benchmarks/crossbench/run.py --setup
python benchmarks/speedometer/run.py --repeat 20 --browser chrome-stable --dry-run
python benchmarks/tachometer/run.py --setup
python benchmarks/browsertime/run.py --setup
```

## MCP memory

Reproduce the MCP process RSS comparison with `tools/compare_mcp_rss.py`.
