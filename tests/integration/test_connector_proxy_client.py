import json
import pathlib
import sys

import requests

ROOT = pathlib.Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from connectors.common.astragraph_proxy_client import ProxyClient, mcp_tools_call


class _FakeResponse:
    def __init__(self, status_code: int, payload: dict | None = None, raw_text: str = ""):
        self.status_code = status_code
        self._payload = payload
        self.text = raw_text

    def json(self):
        if self._payload is None:
            raise json.JSONDecodeError("invalid", self.text, 0)
        return self._payload


def test_mcp_tools_call_builds_jsonrpc_payload(monkeypatch):
    captured: dict = {}

    def _fake_post(url, *, json, timeout):  # noqa: A002
        captured["url"] = url
        captured["json"] = json
        captured["timeout"] = timeout
        return _FakeResponse(200, {"jsonrpc": "2.0", "id": json["id"], "result": {"ok": True}})

    monkeypatch.setattr(requests, "post", _fake_post)
    result = mcp_tools_call(
        "safe_tool",
        {"thinking": "test", "value": "x"},
        proxy_base_url="http://127.0.0.1:7070/",
        request_id="req-test-1",
        timeout=3.0,
    )

    assert result.status_code == 200
    assert result.is_success
    assert captured["url"] == "http://127.0.0.1:7070/mcp/tools/call"
    assert captured["json"]["method"] == "tools/call"
    assert captured["json"]["params"]["name"] == "safe_tool"
    assert captured["timeout"] == 3.0


def test_client_returns_transport_error_payload(monkeypatch):
    def _fake_post(url, *, json, timeout):  # noqa: A002
        raise requests.RequestException("connection refused")

    monkeypatch.setattr(requests, "post", _fake_post)
    client = ProxyClient(proxy_base_url="http://127.0.0.1:7070", timeout=1.0)
    result = client.mcp_tools_call("safe_tool", {"thinking": "t"})

    assert result.status_code == 599
    assert result.error_message == "TRANSPORT_ERROR"
    assert not result.is_success


def test_queue_fallback_detection(monkeypatch):
    def _fake_post(url, *, json, timeout):  # noqa: A002
        return _FakeResponse(
            403,
            {
                "jsonrpc": "2.0",
                "id": json.get("id"),
                "error": {
                    "code": 403,
                    "message": "POLICY_VIOLATION",
                    "data": {"code": 503, "message": "QUEUE", "data": {"detail": "queued"}},
                },
            },
        )

    monkeypatch.setattr(requests, "post", _fake_post)
    client = ProxyClient(proxy_base_url="http://127.0.0.1:7070", timeout=1.0)
    result = client.mcp_tools_call("safe_tool", {"thinking": "t"})

    assert result.is_policy_block
    assert result.is_queue_fallback
    assert result.queue_detail == "queued"


def test_a2a_task_send_payload_shape(monkeypatch):
    captured: dict = {}

    def _fake_post(url, *, json, timeout):  # noqa: A002
        captured["url"] = url
        captured["json"] = json
        return _FakeResponse(200, None, raw_text="event: task_status\ndata: {}\n\n")

    monkeypatch.setattr(requests, "post", _fake_post)
    client = ProxyClient(proxy_base_url="http://127.0.0.1:7070", timeout=2.0)
    result = client.a2a_tasks_send(
        workflow_id="wf-1",
        task_id="task-1",
        target_agent_id="contract-reviewer",
        message_text="handoff",
    )

    assert result.status_code == 200
    assert captured["url"] == "http://127.0.0.1:7070/a2a/tasks/send"
    assert captured["json"]["id"] == "wf-1"
    assert captured["json"]["task_id"] == "task-1"
    assert captured["json"]["target_agent_id"] == "contract-reviewer"
