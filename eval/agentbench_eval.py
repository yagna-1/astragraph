"""AgentBench FAR evaluation harness (minimal)."""

from __future__ import annotations

import argparse
import json
import os
import urllib.request


def main() -> int:
    parser = argparse.ArgumentParser(description="AgentBench FAR harness")
    parser.add_argument(
        "--dataset",
        default=os.getenv("AGENTBENCH_DATASET", "eval/agentbench.jsonl"),
        help="Path to AgentBench jsonl dataset",
    )
    parser.add_argument(
        "--proxy-url",
        default=os.getenv("AGENTBENCH_PROXY_URL", ""),
        help="Proxy tools/call URL for evaluation",
    )
    parser.add_argument("--max-far", type=float, default=None)
    parser.add_argument(
        "--require-inputs",
        action="store_true",
        help="Exit non-zero if dataset or proxy URL is missing",
    )
    parser.add_argument(
        "--strict-http",
        action="store_true",
        help="Exit non-zero when requests cannot be delivered to the proxy",
    )
    args = parser.parse_args()

    if not os.path.exists(args.dataset):
        print("Dataset not found.")
        return 1 if args.require_inputs else 0

    if not args.proxy_url:
        print("Proxy URL not provided.")
        return 1 if args.require_inputs else 0

    total = 0
    malicious_total = 0
    legitimate_total = 0
    false_allows = 0
    false_blocks = 0

    with open(args.dataset, "r", encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if not line:
                continue
            record = json.loads(line)
            should_block = bool(record.get("should_block", False))
            tool_name = record.get("tool_name", "noop")
            arguments = record.get("arguments", {})

            payload = json.dumps(
                {
                    "jsonrpc": "2.0",
                    "id": f"eval-{total}",
                    "method": "tools/call",
                    "params": {"name": tool_name, "arguments": arguments},
                }
            ).encode("utf-8")
            request = urllib.request.Request(
                args.proxy_url,
                data=payload,
                headers={"content-type": "application/json"},
                method="POST",
            )
            try:
                with urllib.request.urlopen(request, timeout=2) as response:
                    status = response.status
            except Exception:
                if args.strict_http:
                    print("Proxy request failed; strict mode enabled.")
                    return 1
                status = 403

            blocked = status == 403
            total += 1
            if should_block:
                malicious_total += 1
            else:
                legitimate_total += 1
            if should_block and not blocked:
                false_allows += 1
            if not should_block and blocked:
                false_blocks += 1

    if total == 0:
        print("No records processed.")
        return 1

    far = (false_blocks / legitimate_total) if legitimate_total else 0.0
    vdr = (
        (malicious_total - false_allows) / malicious_total if malicious_total else 1.0
    )
    print(
        " ".join(
            [
                f"total={total}",
                f"malicious={malicious_total}",
                f"legitimate={legitimate_total}",
                f"false_allows={false_allows}",
                f"false_blocks={false_blocks}",
                f"VDR={vdr:.4f}",
                f"FAR={far:.4f}",
            ]
        )
    )
    if args.max_far is not None and far > args.max_far:
        print(f"FAR exceeded target: {far:.4f} > {args.max_far:.4f}")
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
