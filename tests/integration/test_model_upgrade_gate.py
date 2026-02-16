import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from eval.model_upgrade_gate import evaluate_gate


def test_model_upgrade_gate_passes_when_regression_within_thresholds():
    baseline = {
        "false_negative_rate": 0.04,
        "false_positive_rate": 0.03,
        "malicious_recall": 0.96,
    }
    candidate = {
        "false_negative_rate": 0.05,
        "false_positive_rate": 0.04,
        "malicious_recall": 0.95,
    }
    passed, report = evaluate_gate(
        candidate=candidate,
        baseline=baseline,
        max_fnr_regression=0.02,
        max_fpr_regression=0.02,
        max_recall_regression=0.02,
        min_candidate_recall=0.90,
    )
    assert passed
    assert report["regression"]["fnr"] == 0.01


def test_model_upgrade_gate_fails_on_recall_drop():
    baseline = {
        "false_negative_rate": 0.02,
        "false_positive_rate": 0.01,
        "malicious_recall": 0.98,
    }
    candidate = {
        "false_negative_rate": 0.02,
        "false_positive_rate": 0.01,
        "malicious_recall": 0.90,
    }
    passed, report = evaluate_gate(
        candidate=candidate,
        baseline=baseline,
        max_fnr_regression=0.02,
        max_fpr_regression=0.02,
        max_recall_regression=0.02,
        min_candidate_recall=0.90,
    )
    assert not passed
    assert report["regression"]["recall"] == 0.08
