import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from eval.synthetic_attack_eval import evaluate


def _write_dataset(path: pathlib.Path, records: list[dict]) -> None:
    with path.open("w", encoding="utf-8") as handle:
        for record in records:
            handle.write(json.dumps(record) + "\n")


def test_evaluate_synthetic_summary_counts(tmp_path, monkeypatch):
    dataset_path = tmp_path / "synthetic.jsonl"
    _write_dataset(
        dataset_path,
        [
            {
                "id": "r1",
                "attack_type": "tool_poisoning",
                "tool_name": "export_data",
                "arguments": {"a": 1},
                "should_block": True,
            },
            {
                "id": "r2",
                "attack_type": "rug_pull",
                "tool_name": "delete_record",
                "arguments": {"a": 2},
                "should_block": True,
            },
            {
                "id": "r3",
                "attack_type": "benign",
                "tool_name": "safe_tool",
                "arguments": {"a": 3},
                "should_block": False,
            },
            {
                "id": "r4",
                "attack_type": "benign",
                "tool_name": "noop",
                "arguments": {"a": 4},
                "should_block": False,
            },
        ],
    )

    statuses = iter([403, 200, 200, 403])

    def _fake_request_status(proxy_url, payload, timeout_secs, strict_http):  # noqa: ARG001
        return next(statuses)

    monkeypatch.setattr(
        "eval.synthetic_attack_eval.request_status",
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
    assert summary["malicious_recall"] == 0.5
    assert summary["false_negative_rate"] == 0.5
    assert summary["false_allow_rate"] == 0.5


def test_evaluate_attack_type_recall_breakdown(tmp_path, monkeypatch):
    dataset_path = tmp_path / "synthetic.jsonl"
    _write_dataset(
        dataset_path,
        [
            {
                "id": "r1",
                "attack_type": "tool_poisoning",
                "tool_name": "export_data",
                "arguments": {},
                "should_block": True,
            },
            {
                "id": "r2",
                "attack_type": "tool_poisoning",
                "tool_name": "export_data",
                "arguments": {},
                "should_block": True,
            },
            {
                "id": "r3",
                "attack_type": "rug_pull",
                "tool_name": "delete_record",
                "arguments": {},
                "should_block": True,
            },
        ],
    )

    statuses = iter([403, 200, 403])

    def _fake_request_status(proxy_url, payload, timeout_secs, strict_http):  # noqa: ARG001
        return next(statuses)

    monkeypatch.setattr(
        "eval.synthetic_attack_eval.request_status",
        _fake_request_status,
    )

    summary = evaluate(
        dataset_path=str(dataset_path),
        proxy_url="http://127.0.0.1:7070/mcp/tools/call",
        timeout_secs=1.0,
        strict_http=True,
    )

    assert summary["attack_type_recall"]["tool_poisoning"] == 0.5
    assert summary["attack_type_recall"]["rug_pull"] == 1.0
