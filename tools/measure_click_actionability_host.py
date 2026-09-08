#!/usr/bin/env python3
"""Diagnostic microbench for sync optionless click actionability.

Measures host-visible wall time for a delayed-visibility click. Not launch
evidence — local verification only. Uses mock keychain Chromium args.
"""

from __future__ import annotations

import argparse
import json
import statistics
import time


def _run(iterations: int) -> dict:
    from rustwright.sync_api import sync_playwright

    samples_ms: list[float] = []
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True)
        page = browser.new_page()
        for i in range(iterations):
            delay_ms = 120
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
            page.locator("#ready").click(timeout=5_000)
            elapsed_ms = (time.perf_counter() - start) * 1000.0
            clicked = page.locator("#ready").evaluate("el => el.dataset.clicked")
            if clicked != "1":
                raise RuntimeError(f"click did not land on iteration {i}")
            samples_ms.append(elapsed_ms)
        browser.close()

    return {
        "iterations": iterations,
        "delay_ms": 120,
        "mean_ms": statistics.mean(samples_ms),
        "median_ms": statistics.median(samples_ms),
        "p95_ms": sorted(samples_ms)[max(0, int(len(samples_ms) * 0.95) - 1)],
        "min_ms": min(samples_ms),
        "max_ms": max(samples_ms),
        "samples_ms": samples_ms,
        "path": "locator_click_actionable",
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--iterations", type=int, default=12)
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    result = _run(args.iterations)
    if args.json:
        print(json.dumps(result, indent=2))
    else:
        print(
            f"click_actionability iterations={result['iterations']} "
            f"mean={result['mean_ms']:.1f}ms median={result['median_ms']:.1f}ms "
            f"p95={result['p95_ms']:.1f}ms path={result['path']}"
        )


if __name__ == "__main__":
    main()
