import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from eval.anonymized_trace_eval import evaluate


def _write_dataset(path: pathlib.Path, records: list[dict]) -> None:
    with path.open("w", encoding="utf-8") as handle:
        for record in records:
            handle.write(json.dumps(record) + "\n")


def test_evaluate_anonymized_summary_and_domain_breakdown(tmp_path, monkeypatch):
    dataset_path = tmp_path / "anonymized.jsonl"
    _write_dataset(
        dataset_path,
        [
            {
                "id": "a1",
                "policy_domain": "finance",
                "tool_name": "export_data",
                "arguments": {},
                "should_block": True,
            },
            {
                "id": "a2",
                "policy_domain": "finance",
                "tool_name": "safe_tool",
                "arguments": {},
                "should_block": False,
            },
            {
                "id": "a3",
                "policy_domain": "hr",
                "tool_name": "delete_record",
                "arguments": {},
                "should_block": True,
            },
            {
                "id": "a4",
                "policy_domain": "hr",
                "tool_name": "fetch_profile",
                "arguments": {},
                "should_block": False,
            },
        ],
    )

    statuses = iter([403, 403, 200, 200])

    def _fake_request_status(proxy_url, payload, timeout_secs, strict_http):  # noqa: ARG001
        return next(statuses)

    monkeypatch.setattr(
        "eval.anonymized_trace_eval.request_status",
        _fake_request_status,
    )

    summary = evaluate(
        dataset_path=str(dataset_path),
        proxy_url="http://127.0.0.1:7070/mcp/tools/call",
        timeout_secs=1.0,
        strict_http=True,
    )

    assert summary["total"] == 4
    assert summary["malicious_total"] == 2
    assert summary["benign_total"] == 2
    assert summary["false_allows"] == 1
    assert summary["false_blocks"] == 1
    assert summary["false_negative_rate"] == 0.5
    assert summary["false_positive_rate"] == 0.5
    assert summary["by_domain"]["finance"]["false_positive_rate"] == 1.0
    assert summary["by_domain"]["hr"]["false_negative_rate"] == 1.0
