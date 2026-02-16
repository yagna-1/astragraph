import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from eval.review_feedback_loop import summarize


def test_review_feedback_loop_summary_and_actions():
    summary = summarize(
        [
            {"predicted": "BLOCK", "human_label": "ALLOW", "policy_id": "p1"},
            {"predicted": "ALLOW", "human_label": "BLOCK", "policy_id": "p1"},
            {"predicted": "BLOCK", "human_label": "BLOCK", "policy_id": "p2"},
        ]
    )
    assert summary["total_reviews"] == 3
    assert summary["false_positive"] == 1
    assert summary["false_negative"] == 1
    assert summary["false_positive_rate"] == 0.333333
    assert summary["false_negative_rate"] == 0.333333
    assert len(summary["prompt_tuning_actions"]) >= 2
