#!/usr/bin/env python3
"""Summarize tools/measure_mcp_pgo.sh outputs into summary.json."""

from __future__ import annotations

import json
import sys
from pathlib import Path


def main() -> int:
    outdir = Path(sys.argv[1])
    baseline_bytes = int((outdir / "baseline.bytes").read_text())
    pgo_bytes = int((outdir / "pgo.bytes").read_text())
    baseline_rt = json.loads((outdir / "baseline.runtime.json").read_text())
    pgo_rt = json.loads((outdir / "pgo.runtime.json").read_text())
    summary = {
        "metric": "rustwright-mcp PGO before/after",
        "training": "pgo_train_mcp.py initialize/tools.list/browser_status + cargo test --bin rustwright-mcp",
        "profraw_count": int((outdir / "profraw_count.txt").read_text()),
        "baseline": {
            "bytes": baseline_bytes,
            "mib": round(baseline_bytes / (1024 * 1024), 3),
            "build_seconds": float((outdir / "baseline.build_seconds").read_text()),
            "runtime_median_s": baseline_rt["elapsed_s"]["median"],
            "runtime_mean_s": baseline_rt["elapsed_s"]["mean"],
        },
        "pgo": {
            "bytes": pgo_bytes,
            "mib": round(pgo_bytes / (1024 * 1024), 3),
            "build_seconds": float((outdir / "pgo.build_seconds").read_text()),
            "runtime_median_s": pgo_rt["elapsed_s"]["median"],
            "runtime_mean_s": pgo_rt["elapsed_s"]["mean"],
        },
        "delta": {
            "bytes": pgo_bytes - baseline_bytes,
            "bytes_pct": round((pgo_bytes - baseline_bytes) / baseline_bytes * 100, 2),
            "runtime_median_s": pgo_rt["elapsed_s"]["median"]
            - baseline_rt["elapsed_s"]["median"],
            "runtime_median_pct": round(
                (pgo_rt["elapsed_s"]["median"] - baseline_rt["elapsed_s"]["median"])
                / baseline_rt["elapsed_s"]["median"]
                * 100,
                2,
            ),
        },
    }
    text = json.dumps(summary, indent=2)
    print(text)
    (outdir / "summary.json").write_text(text + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
