"""Aggregate human review outcomes into verifier tuning feedback."""

from __future__ import annotations

import argparse
import json
import os
from collections import defaultdict


def load_records(path: str) -> list[dict]:
    records: list[dict] = []
    with open(path, "r", encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if not line:
                continue
            records.append(json.loads(line))
    return records


def normalize_label(value: str) -> str:
    upper = value.strip().upper()
    if upper in {"BLOCK", "BLOCKED"}:
        return "BLOCK"
    return "ALLOW"


def summarize(records: list[dict]) -> dict:
    fp = 0
    fn = 0
    by_policy: dict[str, dict[str, int]] = defaultdict(
        lambda: {"total": 0, "false_positive": 0, "false_negative": 0}
    )

    for record in records:
        predicted = normalize_label(str(record.get("predicted", "ALLOW")))
        human = normalize_label(str(record.get("human_label", "ALLOW")))
        policy_id = str(record.get("policy_id", "unknown"))

        by_policy[policy_id]["total"] += 1
        if predicted == "BLOCK" and human == "ALLOW":
            fp += 1
            by_policy[policy_id]["false_positive"] += 1
        elif predicted == "ALLOW" and human == "BLOCK":
            fn += 1
            by_policy[policy_id]["false_negative"] += 1

    total = len(records)
    output = {
        "total_reviews": total,
        "false_positive": fp,
        "false_negative": fn,
        "false_positive_rate": round(fp / total, 6) if total else 0.0,
        "false_negative_rate": round(fn / total, 6) if total else 0.0,
        "by_policy": by_policy,
        "prompt_tuning_actions": suggest_actions(fp, fn, by_policy),
    }
    return output


def suggest_actions(fp: int, fn: int, by_policy: dict[str, dict[str, int]]) -> list[str]:
    suggestions: list[str] = []
    if fp > 0:
        suggestions.append(
            "Increase verifier caution on borderline blocks; prefer queue fallback near threshold."
        )
    if fn > 0:
        suggestions.append(
            "Add missed attack examples to distillation set and tighten refusal instructions."
        )
    for policy_id, counters in by_policy.items():
        if counters["false_positive"] >= 3:
            suggestions.append(
                f"Policy {policy_id}: add policy-specific allow exemplars to reduce false positives."
            )
        if counters["false_negative"] >= 3:
            suggestions.append(
                f"Policy {policy_id}: add adversarial tool-call traces to verifier fine-tuning."
            )
    if not suggestions:
        suggestions.append("No tuning changes required from current review batch.")
    return suggestions


def main() -> int:
    parser = argparse.ArgumentParser(description="Verifier review feedback loop")
    parser.add_argument(
        "--input",
        default=os.getenv("ANON_REVIEW_FEEDBACK", "tests/anonymized/review_feedback.jsonl"),
        help="JSONL file with predicted vs human_label review records",
    )
    parser.add_argument(
        "--output",
        default=os.getenv(
            "ANON_REVIEW_FEEDBACK_OUT", "verifier/distillation/feedback_summary.json"
        ),
        help="Path to output summary JSON",
    )
    parser.add_argument(
        "--require-inputs",
        action="store_true",
        help="Exit non-zero if input is missing or empty",
    )
    args = parser.parse_args()

    if not os.path.exists(args.input):
        print("feedback_input_missing")
        return 1 if args.require_inputs else 0

    records = load_records(args.input)
    if not records and args.require_inputs:
        print("feedback_input_empty")
        return 1

    summary = summarize(records)
    os.makedirs(os.path.dirname(args.output), exist_ok=True)
    with open(args.output, "w", encoding="utf-8") as handle:
        json.dump(summary, handle, indent=2, sort_keys=True)
        handle.write("\n")

    print(json.dumps(summary, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
