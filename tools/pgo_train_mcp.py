#!/usr/bin/env python3
"""Train / microbench rustwright-mcp over newline-framed MCP JSON-RPC.

Host-safe: no browser. Speaks initialize + tools/list (+ browser_status) and
samples the server process VmRSS / VmHWM from /proc during the session.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import threading
import time
from pathlib import Path


SAMPLE_INTERVAL_SECONDS = 0.01


def read_vm_rss_kb(pid: int) -> int | None:
    try:
        status = Path(f"/proc/{pid}/status").read_text(encoding="utf-8")
    except (FileNotFoundError, ProcessLookupError, PermissionError):
        return None
    for line in status.splitlines():
        if line.startswith("VmRSS:"):
            return int(line.split()[1])
    return None


def read_vm_hwm_kb(pid: int) -> int | None:
    try:
        status = Path(f"/proc/{pid}/status").read_text(encoding="utf-8")
    except (FileNotFoundError, ProcessLookupError, PermissionError):
        return None
    for line in status.splitlines():
        if line.startswith("VmHWM:"):
            return int(line.split()[1])
    return None


class RssSampler:
    def __init__(self, pid: int) -> None:
        self.pid = pid
        self.samples: list[int] = []
        self._stop = threading.Event()
        self._thread = threading.Thread(target=self._run, daemon=True, name="mcp-rss")

    def start(self) -> None:
        first = read_vm_rss_kb(self.pid)
        if first is not None:
            self.samples.append(first)
        self._thread.start()

    def stop(self) -> None:
        self._stop.set()
        self._thread.join(timeout=2)

    def _run(self) -> None:
        while not self._stop.is_set():
            value = read_vm_rss_kb(self.pid)
            if value is not None:
                self.samples.append(value)
            self._stop.wait(SAMPLE_INTERVAL_SECONDS)

    def summary(self) -> dict[str, int | None]:
        if not self.samples:
            return {"rss_peak_kb": None, "rss_final_kb": None, "rss_sample_count": 0}
        return {
            "rss_peak_kb": max(self.samples),
            "rss_final_kb": self.samples[-1],
            "rss_sample_count": len(self.samples),
        }


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


def one_session(
    binary: Path,
    rounds: int,
    *,
    idle_settle_s: float,
) -> dict[str, float | int | None]:
    env = os.environ.copy()
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
    sampler = RssSampler(proc.pid)
    sampler.start()
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

        # Quiesce briefly so "idle" is post-work RSS, not mid-alloc.
        if idle_settle_s > 0:
            time.sleep(idle_settle_s)
        idle_rss_kb = read_vm_rss_kb(proc.pid)
        hwm_kb = read_vm_hwm_kb(proc.pid)
        elapsed_s = time.perf_counter() - started
        rss = sampler.summary()
        return {
            "elapsed_s": elapsed_s,
            "tools_list_rounds": rounds,
            "tools_list_response_bytes": list_bytes,
            "rss_peak_kb": rss["rss_peak_kb"],
            "rss_final_kb": rss["rss_final_kb"],
            "rss_idle_kb": idle_rss_kb,
            "rss_hwm_kb": hwm_kb,
            "rss_sample_count": rss["rss_sample_count"],
        }
    finally:
        sampler.stop()
        if proc.stdin is not None:
            proc.stdin.close()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=5)


def median(values: list[float]) -> float:
    ordered = sorted(values)
    return ordered[len(ordered) // 2]


def summarize_optional_ints(values: list[int | None]) -> dict[str, float | int | None]:
    present = [value for value in values if value is not None]
    if not present:
        return {"min": None, "median": None, "max": None, "mean": None, "samples": []}
    return {
        "min": min(present),
        "median": int(median([float(value) for value in present])),
        "max": max(present),
        "mean": round(sum(present) / len(present), 1),
        "samples": present,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("--rounds", type=int, default=20)
    parser.add_argument("--repetitions", type=int, default=5)
    parser.add_argument("--idle-settle-s", type=float, default=0.25)
    parser.add_argument("--json-out", type=Path)
    args = parser.parse_args()

    if not args.binary.is_file():
        print(f"missing binary: {args.binary}", file=sys.stderr)
        return 2

    samples = [
        one_session(args.binary, args.rounds, idle_settle_s=args.idle_settle_s)
        for _ in range(args.repetitions)
    ]
    elapsed = [float(sample["elapsed_s"]) for sample in samples]
    payload = {
        "binary": str(args.binary),
        "rounds": args.rounds,
        "repetitions": args.repetitions,
        "idle_settle_s": args.idle_settle_s,
        "elapsed_s": {
            "min": min(elapsed),
            "median": median(elapsed),
            "max": max(elapsed),
            "mean": sum(elapsed) / len(elapsed),
            "samples": elapsed,
        },
        "rss_peak_kb": summarize_optional_ints(
            [sample["rss_peak_kb"] if isinstance(sample["rss_peak_kb"], int) else None for sample in samples]
        ),
        "rss_idle_kb": summarize_optional_ints(
            [sample["rss_idle_kb"] if isinstance(sample["rss_idle_kb"], int) else None for sample in samples]
        ),
        "rss_hwm_kb": summarize_optional_ints(
            [sample["rss_hwm_kb"] if isinstance(sample["rss_hwm_kb"], int) else None for sample in samples]
        ),
    }
    text = json.dumps(payload, indent=2)
    print(text)
    if args.json_out is not None:
        args.json_out.parent.mkdir(parents=True, exist_ok=True)
        args.json_out.write_text(text + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
