"""Proxy latency benchmark against the HTTP MCP proxy."""

from __future__ import annotations

import argparse
import json
import statistics
import time
import urllib.request
from concurrent.futures import ThreadPoolExecutor


def main() -> int:
    parser = argparse.ArgumentParser(description="Benchmark proxy overhead")
    parser.add_argument(
        "--url",
        default="http://127.0.0.1:7070/mcp/tools/call",
        help="MCP tools/call proxy endpoint",
    )
    parser.add_argument("--iterations", type=int, default=50)
    parser.add_argument("--concurrency", type=int, default=1)
    parser.add_argument("--p99-target-ms", type=float, default=None)
    parser.add_argument(
        "--require-success",
        action="store_true",
        help="Exit non-zero if no requests succeed",
    )
    args = parser.parse_args()

    payload = json.dumps(
        {
            "jsonrpc": "2.0",
            "id": "bench-1",
            "method": "tools/call",
            "params": {"name": "noop", "arguments": {"ping": "pong"}},
        }
    ).encode("utf-8")
    latencies = []

    def one_call() -> float | None:
        request = urllib.request.Request(
            args.url,
            data=payload,
            headers={"content-type": "application/json"},
            method="POST",
        )
        start = time.perf_counter()
        try:
            with urllib.request.urlopen(request, timeout=2) as response:
                response.read()
        except Exception:
            return None
        return (time.perf_counter() - start) * 1000.0

    if args.concurrency <= 1:
        for _ in range(args.iterations):
            latency = one_call()
            if latency is not None:
                latencies.append(latency)
    else:
        with ThreadPoolExecutor(max_workers=args.concurrency) as executor:
            for latency in executor.map(lambda _: one_call(), range(args.iterations)):
                if latency is not None:
                    latencies.append(latency)

    if not latencies:
        print("No successful responses.")
        return 1 if args.require_success else 0

    latencies.sort()
    p95 = latencies[int(len(latencies) * 0.95) - 1]
    p99 = latencies[int(len(latencies) * 0.99) - 1]
    print(f"count={len(latencies)} avg_ms={statistics.mean(latencies):.2f}")
    print(
        f"p95_ms={p95:.2f} p99_ms={p99:.2f} min_ms={latencies[0]:.2f} max_ms={latencies[-1]:.2f}"
    )
    if args.p99_target_ms is not None and p99 > args.p99_target_ms:
        print(f"p99 exceeded target: {p99:.2f} > {args.p99_target_ms:.2f}")
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
