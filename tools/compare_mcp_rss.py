#!/usr/bin/env python3
"""Compare MCP server process VmRSS: rustwright-mcp vs @playwright/mcp.

Host-safe protocol-only workload (initialize + tools/list). Does not launch
Chromium. Samples VmRSS of the MCP server process from /proc.

Example:

  python3 tools/compare_mcp_rss.py \\
    --rustwright mcp/target/release/rustwright-mcp \\
    --playwright node:/path/to/node_modules/@playwright/mcp/cli.js
"""

from __future__ import annotations

import argparse
import json
import os
import select
import subprocess
import threading
import time
from pathlib import Path

SAMPLE_INTERVAL = 0.01


def read_rss_kb(pid: int) -> int | None:
    try:
        for line in Path(f"/proc/{pid}/status").read_text(encoding="utf-8").splitlines():
            if line.startswith("VmRSS:"):
                return int(line.split()[1])
    except (FileNotFoundError, ProcessLookupError, PermissionError, OSError):
        return None
    return None


def send_newline(proc: subprocess.Popen[bytes], message: dict) -> None:
    assert proc.stdin is not None
    proc.stdin.write((json.dumps(message, separators=(",", ":")) + "\n").encode())
    proc.stdin.flush()


def recv_newline(proc: subprocess.Popen[bytes], timeout: float = 15.0) -> dict:
    assert proc.stdout is not None
    deadline = time.time() + timeout
    buf = b""
    while time.time() < deadline:
        ready, _, _ = select.select(
            [proc.stdout], [], [], min(0.5, max(0.01, deadline - time.time()))
        )
        if not ready:
            continue
        chunk = os.read(proc.stdout.fileno(), 65536)
        if not chunk:
            raise RuntimeError(f"mcp stdout closed; buf={buf[:200]!r}")
        buf += chunk
        if b"\n" in buf:
            line, _rest = buf.split(b"\n", 1)
            return json.loads(line.decode())
    raise TimeoutError(f"timeout waiting for MCP response; buf={buf[:300]!r}")


def parse_command(spec: str) -> list[str]:
    if spec.startswith("node:"):
        return ["node", spec[len("node:") :]]
    return [spec]


def one_session(cmd: list[str], rounds: int, settle_s: float, cwd: Path | None) -> dict:
    env = os.environ.copy()
    env.setdefault("PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD", "1")
    for key in list(env):
        if key.startswith("RUSTWRIGHT_MCP_"):
            env.pop(key, None)
    proc = subprocess.Popen(
        cmd,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        cwd=str(cwd) if cwd else None,
        env=env,
    )
    samples: list[int] = []
    stop = threading.Event()

    def sampler() -> None:
        while not stop.is_set():
            value = read_rss_kb(proc.pid)
            if value is not None:
                samples.append(value)
            stop.wait(SAMPLE_INTERVAL)

    thread = threading.Thread(target=sampler, daemon=True)
    thread.start()
    try:
        time.sleep(0.15)
        send_newline(
            proc,
            {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {"name": "mcp-rss-compare", "version": "0.0.0"},
                },
            },
        )
        init = recv_newline(proc)
        if "result" not in init:
            raise RuntimeError(f"initialize failed: {init}")
        send_newline(proc, {"jsonrpc": "2.0", "method": "notifications/initialized"})
        list_bytes = 0
        tool_count = None
        for index in range(rounds):
            req_id = 2 + index
            send_newline(
                proc,
                {
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "method": "tools/list",
                    "params": {},
                },
            )
            response = recv_newline(proc)
            if response.get("id") != req_id or "result" not in response:
                raise RuntimeError(f"tools/list failed: {response}")
            tools = response["result"].get("tools") or []
            tool_count = len(tools)
            list_bytes += len(json.dumps(response, separators=(",", ":")))
        if settle_s > 0:
            time.sleep(settle_s)
        idle = read_rss_kb(proc.pid)
        return {
            "rss_peak_kb": max(samples) if samples else None,
            "rss_idle_kb": idle,
            "rss_sample_count": len(samples),
            "tools_list_response_bytes": list_bytes,
            "tool_count": tool_count,
        }
    finally:
        stop.set()
        thread.join(timeout=1)
        if proc.stdin is not None:
            proc.stdin.close()
        try:
            proc.kill()
            proc.wait(timeout=3)
        except Exception:
            pass


def median(values: list[int]) -> int:
    ordered = sorted(values)
    return ordered[len(ordered) // 2]


def run_many(
    label: str, cmd: list[str], reps: int, rounds: int, settle_s: float, cwd: Path | None
) -> dict:
    rows = []
    for index in range(reps):
        row = one_session(cmd, rounds=rounds, settle_s=settle_s, cwd=cwd)
        rows.append(row)
        print(
            f"{label} rep{index + 1}: peak={row['rss_peak_kb']} "
            f"idle={row['rss_idle_kb']} tools={row['tool_count']}",
            flush=True,
        )
    peaks = [row["rss_peak_kb"] for row in rows if row["rss_peak_kb"] is not None]
    idles = [row["rss_idle_kb"] for row in rows if row["rss_idle_kb"] is not None]
    return {
        "label": label,
        "cmd": cmd,
        "reps": reps,
        "tool_count": rows[0]["tool_count"] if rows else None,
        "rss_peak_kb": {
            "min": min(peaks),
            "median": median(peaks),
            "max": max(peaks),
            "samples": peaks,
        },
        "rss_idle_kb": {
            "min": min(idles),
            "median": median(idles),
            "max": max(idles),
            "samples": idles,
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--rustwright",
        required=True,
        help="Path to rustwright-mcp binary",
    )
    parser.add_argument(
        "--playwright",
        required=True,
        help="Playwright MCP command: path or node:/abs/path/to/cli.js",
    )
    parser.add_argument("--rounds", type=int, default=20)
    parser.add_argument("--repetitions", type=int, default=5)
    parser.add_argument("--settle-seconds", type=float, default=0.5)
    parser.add_argument(
        "--playwright-cwd",
        type=Path,
        default=None,
        help="Working directory for the Playwright MCP process",
    )
    parser.add_argument(
        "--out",
        type=Path,
        default=Path(".benchmark-data/mcp-rss-compare/mcp_rss_compare.json"),
    )
    args = parser.parse_args()

    rustwright_cmd = parse_command(args.rustwright)
    playwright_cmd = parse_command(args.playwright)
    rustwright = run_many(
        "rustwright-mcp",
        rustwright_cmd,
        reps=args.repetitions,
        rounds=args.rounds,
        settle_s=args.settle_seconds,
        cwd=None,
    )
    playwright = run_many(
        "@playwright/mcp",
        playwright_cmd,
        reps=args.repetitions,
        rounds=args.rounds,
        settle_s=args.settle_seconds,
        cwd=args.playwright_cwd,
    )
    rw_peak = rustwright["rss_peak_kb"]["median"]
    pw_peak = playwright["rss_peak_kb"]["median"]
    rw_idle = rustwright["rss_idle_kb"]["median"]
    pw_idle = playwright["rss_idle_kb"]["median"]
    payload = {
        "metric": "MCP server process VmRSS during initialize + tools/list",
        "note": "Host-safe protocol-only; no Chromium launched. MCP process RSS only.",
        "rustwright_mcp": rustwright,
        "playwright_mcp": playwright,
        "comparison": {
            "peak_median_kb": {"rustwright_mcp": rw_peak, "playwright_mcp": pw_peak},
            "idle_median_kb": {"rustwright_mcp": rw_idle, "playwright_mcp": pw_idle},
            "peak_median_mib": {
                "rustwright_mcp": round(rw_peak / 1024, 2),
                "playwright_mcp": round(pw_peak / 1024, 2),
            },
            "idle_median_mib": {
                "rustwright_mcp": round(rw_idle / 1024, 2),
                "playwright_mcp": round(pw_idle / 1024, 2),
            },
            "peak_reduction_pct": round((1 - rw_peak / pw_peak) * 100, 1),
            "playwright_over_rustwright_peak": round(pw_peak / rw_peak, 1),
        },
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(payload["comparison"], indent=2))
    print(f"wrote {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
