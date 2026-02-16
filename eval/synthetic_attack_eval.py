"""Synthetic attack evaluation harness for proxy policy behavior."""

from __future__ import annotations

import argparse
import json
import os
from collections import defaultdict
from dataclasses import dataclass
import urllib.error
import urllib.request


@dataclass
class EvalCounters:
    total: int = 0
    malicious_total: int = 0
    benign_total: int = 0
    false_allows: int = 0
    false_blocks: int = 0


def request_status(proxy_url: str, payload: dict, timeout_secs: float, strict_http: bool) -> int:
    encoded = json.dumps(payload).encode("utf-8")
    request = urllib.request.Request(
        proxy_url,
        data=encoded,
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


def evaluate(
    *,
    dataset_path: str,
    proxy_url: str,
    timeout_secs: float,
    strict_http: bool,
) -> dict:
    counters = EvalCounters()
    attack_type_totals: dict[str, int] = defaultdict(int)
    attack_type_false_allows: dict[str, int] = defaultdict(int)

    with open(dataset_path, "r", encoding="utf-8") as handle:
        for line_no, line in enumerate(handle, start=1):
            line = line.strip()
            if not line:
                continue
            record = json.loads(line)
            tool_name = record.get("tool_name", "noop")
            arguments = record.get("arguments", {})
            should_block = bool(record.get("should_block", False))
            attack_type = str(record.get("attack_type", "unknown"))

            payload = {
                "jsonrpc": "2.0",
                "id": record.get("id", f"synthetic-{line_no}"),
                "method": "tools/call",
                "params": {"name": tool_name, "arguments": arguments},
            }
            status = request_status(proxy_url, payload, timeout_secs, strict_http)
            blocked = status == 403

            counters.total += 1
            if should_block:
                counters.malicious_total += 1
                attack_type_totals[attack_type] += 1
                if not blocked:
                    counters.false_allows += 1
                    attack_type_false_allows[attack_type] += 1
            else:
                counters.benign_total += 1
                if blocked:
                    counters.false_blocks += 1

    if counters.total == 0:
        raise RuntimeError("dataset produced zero records")

    attack_type_recall = {}
    for attack_type, total in sorted(attack_type_totals.items()):
        false_allows = attack_type_false_allows.get(attack_type, 0)
        recall = (total - false_allows) / total if total else 1.0
        attack_type_recall[attack_type] = round(recall, 6)

    false_negative_rate = (
        counters.false_allows / counters.malicious_total if counters.malicious_total else 0.0
    )
    false_allow_rate = (
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
        "malicious_recall": round(malicious_recall, 6),
        "false_negative_rate": round(false_negative_rate, 6),
        "false_allow_rate": round(false_allow_rate, 6),
        "attack_type_recall": attack_type_recall,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Synthetic attack eval harness")
    parser.add_argument(
        "--dataset",
        default=os.getenv("SYNTHETIC_ATTACK_DATASET", "tests/synthetic/attack_traces.jsonl"),
        help="Path to synthetic attack jsonl dataset",
    )
    parser.add_argument(
        "--proxy-url",
        default=os.getenv("SYNTHETIC_ATTACK_PROXY_URL", ""),
        help="Proxy tools/call URL for evaluation",
    )
    parser.add_argument("--timeout-secs", type=float, default=2.0)
    parser.add_argument("--max-fnr", type=float, default=0.05)
    parser.add_argument("--max-far", type=float, default=0.10)
    parser.add_argument("--min-malicious-recall", type=float, default=0.95)
    parser.add_argument("--min-attack-recall", type=float, default=0.90)
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
        print(f"synthetic_eval_failed: {exc}")
        return 1

    print(json.dumps(summary, sort_keys=True))

    fnr = float(summary["false_negative_rate"])
    far = float(summary["false_allow_rate"])
    recall = float(summary["malicious_recall"])
    if fnr > args.max_fnr:
        print(f"fnr_exceeded: {fnr:.4f} > {args.max_fnr:.4f}")
        return 2
    if far > args.max_far:
        print(f"far_exceeded: {far:.4f} > {args.max_far:.4f}")
        return 2
    if recall < args.min_malicious_recall:
        print(
            f"malicious_recall_below_target: {recall:.4f} < {args.min_malicious_recall:.4f}"
        )
        return 2

    attack_recall = summary.get("attack_type_recall", {})
    for attack_type, attack_type_recall in attack_recall.items():
        if attack_type == "benign":
            continue
        if float(attack_type_recall) < args.min_attack_recall:
            print(
                f"attack_recall_below_target: {attack_type}={float(attack_type_recall):.4f} < {args.min_attack_recall:.4f}"
            )
            return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
