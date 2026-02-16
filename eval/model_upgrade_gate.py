"""Model-upgrade quality gate for verifier evaluation summaries."""

from __future__ import annotations

import argparse
import json
import os


def load_summary(path: str) -> dict:
    with open(path, "r", encoding="utf-8") as handle:
        return json.load(handle)


def evaluate_gate(
    *,
    candidate: dict,
    baseline: dict,
    max_fnr_regression: float,
    max_fpr_regression: float,
    max_recall_regression: float,
    min_candidate_recall: float,
) -> tuple[bool, dict]:
    baseline_fnr = float(baseline.get("false_negative_rate", 1.0))
    baseline_fpr = float(baseline.get("false_positive_rate", 1.0))
    baseline_recall = float(baseline.get("malicious_recall", 0.0))

    candidate_fnr = float(candidate.get("false_negative_rate", 1.0))
    candidate_fpr = float(candidate.get("false_positive_rate", 1.0))
    candidate_recall = float(candidate.get("malicious_recall", 0.0))

    fnr_regression = candidate_fnr - baseline_fnr
    fpr_regression = candidate_fpr - baseline_fpr
    recall_regression = baseline_recall - candidate_recall

    report = {
        "baseline": {
            "false_negative_rate": baseline_fnr,
            "false_positive_rate": baseline_fpr,
            "malicious_recall": baseline_recall,
        },
        "candidate": {
            "false_negative_rate": candidate_fnr,
            "false_positive_rate": candidate_fpr,
            "malicious_recall": candidate_recall,
        },
        "regression": {
            "fnr": round(fnr_regression, 6),
            "fpr": round(fpr_regression, 6),
            "recall": round(recall_regression, 6),
        },
    }

    passed = (
        fnr_regression <= max_fnr_regression
        and fpr_regression <= max_fpr_regression
        and recall_regression <= max_recall_regression
        and candidate_recall >= min_candidate_recall
    )
    return passed, report


def main() -> int:
    parser = argparse.ArgumentParser(description="Verifier model upgrade gate")
    parser.add_argument("--candidate", required=True, help="Candidate eval summary JSON")
    parser.add_argument("--baseline", required=True, help="Baseline eval summary JSON")
    parser.add_argument("--max-fnr-regression", type=float, default=0.02)
    parser.add_argument("--max-fpr-regression", type=float, default=0.02)
    parser.add_argument("--max-recall-regression", type=float, default=0.02)
    parser.add_argument("--min-candidate-recall", type=float, default=0.90)
    args = parser.parse_args()

    if not os.path.exists(args.candidate):
        print("candidate_summary_missing")
        return 1
    if not os.path.exists(args.baseline):
        print("baseline_summary_missing")
        return 1

    baseline = load_summary(args.baseline)
    candidate = load_summary(args.candidate)
    passed, report = evaluate_gate(
        candidate=candidate,
        baseline=baseline,
        max_fnr_regression=args.max_fnr_regression,
        max_fpr_regression=args.max_fpr_regression,
        max_recall_regression=args.max_recall_regression,
        min_candidate_recall=args.min_candidate_recall,
    )

    print(json.dumps(report, sort_keys=True))

    fnr_regression = float(report["regression"]["fnr"])
    fpr_regression = float(report["regression"]["fpr"])
    recall_regression = float(report["regression"]["recall"])
    candidate_recall = float(report["candidate"]["malicious_recall"])

    if fnr_regression > args.max_fnr_regression:
        print(
            f"fnr_regression_exceeded: {fnr_regression:.4f} > {args.max_fnr_regression:.4f}"
        )
        return 2
    if fpr_regression > args.max_fpr_regression:
        print(
            f"fpr_regression_exceeded: {fpr_regression:.4f} > {args.max_fpr_regression:.4f}"
        )
        return 2
    if recall_regression > args.max_recall_regression:
        print(
            f"recall_regression_exceeded: {recall_regression:.4f} > {args.max_recall_regression:.4f}"
        )
        return 2
    if candidate_recall < args.min_candidate_recall:
        print(
            f"candidate_recall_below_min: {candidate_recall:.4f} < {args.min_candidate_recall:.4f}"
        )
        return 2
    return 0 if passed else 2


if __name__ == "__main__":
    raise SystemExit(main())
