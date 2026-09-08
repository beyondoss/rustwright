#!/usr/bin/env python3
"""Diagnostic microbench for sync optionless click actionability.

Measures host-visible wall time for a delayed-visibility click. Not launch
evidence — local verification only.
"""

from __future__ import annotations

import argparse
import json
import statistics
import time


def _run(iterations: int, delay_ms: int) -> dict:
    from rustwright.sync_api import sync_playwright

    samples_ms: list[float] = []
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True)
        page = browser.new_page()
        for i in range(iterations):
            page.set_content(
                f"""
                <button id="ready" style="display:none">Go</button>
                <script>
                setTimeout(() => {{
                  const el = document.getElementById('ready');
                  el.style.display = 'block';
                  el.addEventListener('click', () => {{ el.dataset.clicked = '1'; }});
                }}, {delay_ms});
                </script>
                """
            )
            start = time.perf_counter()
            page.locator("#ready").click(timeout=10_000)
            elapsed_ms = (time.perf_counter() - start) * 1000.0
            clicked = page.locator("#ready").evaluate("el => el.dataset.clicked")
            if clicked != "1":
                raise RuntimeError(f"click did not land on iteration {i}")
            samples_ms.append(elapsed_ms)
        browser.close()

    overhead = [s - delay_ms for s in samples_ms]
    path = "locator_click_actionable"
    try:
        from rustwright.sync_api import Locator
        import inspect
        if "locator_click_actionable" not in inspect.getsource(Locator._click_impl):
            path = "python_wait_for_single"
    except Exception:
        path = "unknown"

    return {
        "iterations": iterations,
        "delay_ms": delay_ms,
        "mean_ms": statistics.mean(samples_ms),
        "median_ms": statistics.median(samples_ms),
        "p95_ms": sorted(samples_ms)[max(0, int(len(samples_ms) * 0.95) - 1)],
        "min_ms": min(samples_ms),
        "max_ms": max(samples_ms),
        "mean_overhead_ms": statistics.mean(overhead),
        "median_overhead_ms": statistics.median(overhead),
        "samples_ms": samples_ms,
        "path": path,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--iterations", type=int, default=12)
    parser.add_argument("--delay-ms", type=int, default=400)
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    result = _run(args.iterations, args.delay_ms)
    if args.json:
        print(json.dumps(result, indent=2))
    else:
        print(
            f"click_actionability iterations={result['iterations']} delay={result['delay_ms']}ms "
            f"mean={result['mean_ms']:.1f}ms overhead={result['mean_overhead_ms']:.1f}ms "
            f"path={result['path']}"
        )


if __name__ == "__main__":
    main()
