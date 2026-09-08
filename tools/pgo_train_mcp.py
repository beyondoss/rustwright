#!/usr/bin/env python3
"""Train / microbench rustwright-mcp over newline-framed MCP JSON-RPC.

Host-safe: no browser. Speaks initialize + tools/list (+ optional junk calls)
to exercise server/protocol/shaping paths used on every MCP session.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path


def recv_line(proc: subprocess.Popen[str]) -> dict:
    assert proc.stdout is not None
    line = proc.stdout.readline()
    if not line:
        raise RuntimeError("mcp stdout closed before response")
    return json.loads(line)


def send_line(proc: subprocess.Popen[str], message: dict) -> None:
    assert proc.stdin is not None
    proc.stdin.write(json.dumps(message, separators=(",", ":")) + "\n")
    proc.stdin.flush()


def one_session(binary: Path, rounds: int) -> dict[str, float | int]:
    env = os.environ.copy()
    # Keep training deterministic and offline.
    for key in list(env):
        if key.startswith("RUSTWRIGHT_MCP_"):
            env.pop(key, None)

    started = time.perf_counter()
    proc = subprocess.Popen(
        [str(binary)],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        env=env,
    )
    try:
        send_line(
            proc,
            {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {"name": "pgo-train", "version": "0.0.0"},
                },
            },
        )
        init = recv_line(proc)
        if "result" not in init:
            raise RuntimeError(f"initialize failed: {init}")
        send_line(proc, {"jsonrpc": "2.0", "method": "notifications/initialized"})

        list_bytes = 0
        for index in range(rounds):
            req_id = 2 + index
            send_line(
                proc,
                {
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "method": "tools/list",
                    "params": {},
                },
            )
            response = recv_line(proc)
            if response.get("id") != req_id or "result" not in response:
                raise RuntimeError(f"tools/list failed: {response}")
            list_bytes += len(json.dumps(response, separators=(",", ":")))

        # Hit unknown-tool / validation paths without launching Chromium.
        send_line(
            proc,
            {
                "jsonrpc": "2.0",
                "id": 9000,
                "method": "tools/call",
                "params": {"name": "browser_status", "arguments": {}},
            },
        )
        status = recv_line(proc)
        if "result" not in status and "error" not in status:
            raise RuntimeError(f"browser_status unexpected: {status}")

        elapsed_s = time.perf_counter() - started
        return {
            "elapsed_s": elapsed_s,
            "tools_list_rounds": rounds,
            "tools_list_response_bytes": list_bytes,
        }
    finally:
        if proc.stdin is not None:
            proc.stdin.close()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=5)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("--rounds", type=int, default=20)
    parser.add_argument("--repetitions", type=int, default=5)
    parser.add_argument("--json-out", type=Path)
    args = parser.parse_args()

    if not args.binary.is_file():
        print(f"missing binary: {args.binary}", file=sys.stderr)
        return 2

    samples = [one_session(args.binary, args.rounds) for _ in range(args.repetitions)]
    elapsed = [float(sample["elapsed_s"]) for sample in samples]
    elapsed.sort()
    payload = {
        "binary": str(args.binary),
        "rounds": args.rounds,
        "repetitions": args.repetitions,
        "elapsed_s": {
            "min": elapsed[0],
            "median": elapsed[len(elapsed) // 2],
            "max": elapsed[-1],
            "mean": sum(elapsed) / len(elapsed),
            "samples": elapsed,
        },
    }
    text = json.dumps(payload, indent=2)
    print(text)
    if args.json_out is not None:
        args.json_out.parent.mkdir(parents=True, exist_ok=True)
        args.json_out.write_text(text + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
