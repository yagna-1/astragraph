"""Evaluation harness for anonymized production-like traces."""

from __future__ import annotations

import argparse
import json
import os
from collections import defaultdict
from dataclasses import dataclass
import urllib.error
import urllib.request


@dataclass
class Counters:
    total: int = 0
    malicious_total: int = 0
    benign_total: int = 0
    false_allows: int = 0
    false_blocks: int = 0


def request_status(proxy_url: str, payload: dict, timeout_secs: float, strict_http: bool) -> int:
    body = json.dumps(payload).encode("utf-8")
    request = urllib.request.Request(
        proxy_url,
        data=body,
        headers={"content-type": "application/json"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=timeout_secs) as response:
            return response.status
    except urllib.error.HTTPError as error:
        return error.code
    except Exception:
        if strict_http:
            raise
        return 403


def summarize(counters: Counters) -> dict:
    false_negative_rate = (
        counters.false_allows / counters.malicious_total if counters.malicious_total else 0.0
    )
    false_positive_rate = (
        counters.false_blocks / counters.benign_total if counters.benign_total else 0.0
    )
    malicious_recall = (
        (counters.malicious_total - counters.false_allows) / counters.malicious_total
        if counters.malicious_total
        else 1.0
    )
    return {
        "total": counters.total,
        "malicious_total": counters.malicious_total,
        "benign_total": counters.benign_total,
        "false_allows": counters.false_allows,
        "false_blocks": counters.false_blocks,
        "false_negative_rate": round(false_negative_rate, 6),
        "false_positive_rate": round(false_positive_rate, 6),
        "malicious_recall": round(malicious_recall, 6),
    }


def evaluate(
    *,
    dataset_path: str,
    proxy_url: str,
    timeout_secs: float,
    strict_http: bool,
) -> dict:
    global_counters = Counters()
    domain_counters: dict[str, Counters] = defaultdict(Counters)

    with open(dataset_path, "r", encoding="utf-8") as handle:
        for line_no, line in enumerate(handle, start=1):
            line = line.strip()
            if not line:
                continue
            record = json.loads(line)
            should_block = bool(record.get("should_block", False))
            tool_name = record.get("tool_name", "noop")
            arguments = record.get("arguments", {})
            domain = str(record.get("policy_domain", "unknown"))

            payload = {
                "jsonrpc": "2.0",
                "id": record.get("id", f"trace-{line_no}"),
                "method": "tools/call",
                "params": {"name": tool_name, "arguments": arguments},
            }
            status = request_status(proxy_url, payload, timeout_secs, strict_http)
            blocked = status == 403

            for counters in (global_counters, domain_counters[domain]):
                counters.total += 1
                if should_block:
                    counters.malicious_total += 1
                    if not blocked:
                        counters.false_allows += 1
                else:
                    counters.benign_total += 1
                    if blocked:
                        counters.false_blocks += 1

    if global_counters.total == 0:
        raise RuntimeError("dataset produced zero records")

    by_domain = {
        domain: summarize(counters)
        for domain, counters in sorted(domain_counters.items())
    }
    return {
        "dataset_type": "anonymized_traces",
        **summarize(global_counters),
        "by_domain": by_domain,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Anonymized trace evaluation harness")
    parser.add_argument(
        "--dataset",
        default=os.getenv("ANON_TRACE_DATASET", "tests/anonymized/anonymized_traces.jsonl"),
        help="Path to anonymized trace jsonl dataset",
    )
    parser.add_argument(
        "--proxy-url",
        default=os.getenv("ANON_TRACE_PROXY_URL", ""),
        help="Proxy tools/call URL for evaluation",
    )
    parser.add_argument("--timeout-secs", type=float, default=2.0)
    parser.add_argument("--max-fnr", type=float, default=0.10)
    parser.add_argument("--max-fpr", type=float, default=0.10)
    parser.add_argument("--min-recall", type=float, default=0.90)
    parser.add_argument(
        "--strict-http",
        action="store_true",
        help="Exit non-zero when requests cannot be delivered to the proxy",
    )
    parser.add_argument(
        "--require-inputs",
        action="store_true",
        help="Exit non-zero if dataset/proxy inputs are missing",
    )
    parser.add_argument(
        "--output",
        default="",
        help="Optional file path to write summary JSON",
    )
    args = parser.parse_args()

    if not os.path.exists(args.dataset):
        print("dataset_missing")
        return 1 if args.require_inputs else 0
    if not args.proxy_url:
        print("proxy_url_missing")
        return 1 if args.require_inputs else 0

    try:
        summary = evaluate(
            dataset_path=args.dataset,
            proxy_url=args.proxy_url,
            timeout_secs=args.timeout_secs,
            strict_http=args.strict_http,
        )
    except Exception as exc:
        print(f"anonymized_eval_failed: {exc}")
        return 1

    encoded = json.dumps(summary, sort_keys=True)
    print(encoded)
    if args.output:
        with open(args.output, "w", encoding="utf-8") as handle:
            handle.write(encoded)
            handle.write("\n")

    if float(summary["false_negative_rate"]) > args.max_fnr:
        print(
            f"fnr_exceeded: {float(summary['false_negative_rate']):.4f} > {args.max_fnr:.4f}"
        )
        return 2
    if float(summary["false_positive_rate"]) > args.max_fpr:
        print(
            f"fpr_exceeded: {float(summary['false_positive_rate']):.4f} > {args.max_fpr:.4f}"
        )
        return 2
    if float(summary["malicious_recall"]) < args.min_recall:
        print(
            f"malicious_recall_below_target: {float(summary['malicious_recall']):.4f} < {args.min_recall:.4f}"
        )
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
