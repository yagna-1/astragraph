#!/usr/bin/env python3
"""Three-agent dockerized E2E gate assertions."""

from __future__ import annotations

import argparse
import json
import sys
import time
import urllib.error
import urllib.request


def request_json(url: str, payload: dict, timeout: float = 5.0) -> tuple[int, str]:
    body = json.dumps(payload).encode("utf-8")
    request = urllib.request.Request(
        url,
        data=body,
        headers={"content-type": "application/json"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return response.status, response.read().decode("utf-8")
    except urllib.error.HTTPError as error:
        return error.code, error.read().decode("utf-8")


def wait_for_proxy(url: str, timeout_secs: int) -> None:
    deadline = time.time() + timeout_secs
    probe = {
        "jsonrpc": "2.0",
        "id": "probe-e2e",
        "method": "tools/call",
        "params": {"name": "safe_tool", "arguments": {"thinking": "probe"}},
    }
    while time.time() < deadline:
        try:
            status, _ = request_json(url, probe, timeout=2.0)
            if status in (200, 403):
                return
        except Exception:
            pass
        time.sleep(1.0)
    raise RuntimeError("proxy did not become ready in time")


def assert_true(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def get_json(url: str) -> list[dict]:
    request = urllib.request.Request(
        url,
        headers={"authorization": "Bearer dev-token"},
        method="GET",
    )
    with urllib.request.urlopen(request, timeout=5) as response:
        return json.loads(response.read().decode("utf-8"))


def run_standard_gate(
    workflow_id: str,
    mcp_url: str,
    a2a_url: str,
    graph_base_url: str,
) -> dict:
    a2a_payload = {
        "id": workflow_id,
        "task_id": "task-three-agent-1",
        "target_agent_id": "contract-reviewer",
        "message": {"parts": [{"text": "handoff to contract reviewer"}]},
    }
    status, body = request_json(a2a_url, a2a_payload)
    assert_true(status == 200, f"a2a task send expected 200, got {status}")
    assert_true(
        "event: task_status" in body and '"state": "COMPLETED"' in body,
        "a2a stream missing expected task_status completion event",
    )

    safe_payload = {
        "jsonrpc": "2.0",
        "id": workflow_id,
        "method": "tools/call",
        "params": {
            "name": "safe_tool",
            "arguments": {"thinking": "safe path", "approval": True},
        },
    }
    status, body = request_json(mcp_url, safe_payload)
    assert_true(status == 200, f"safe tool call expected 200, got {status}")
    safe_json = json.loads(body)
    assert_true("result" in safe_json, "safe tool response missing result payload")

    blocked_payload = {
        "jsonrpc": "2.0",
        "id": workflow_id,
        "method": "tools/call",
        "params": {
            "name": "export_data",
            "arguments": {"thinking": "attempt export", "table": "customers"},
        },
    }
    status, body = request_json(mcp_url, blocked_payload)
    assert_true(status == 403, f"blocked tool call expected 403, got {status}")
    blocked_json = json.loads(body)
    assert_true(
        blocked_json.get("error", {}).get("message") == "POLICY_VIOLATION",
        "blocked tool did not return policy violation",
    )
    rule_id = blocked_json.get("error", {}).get("data", {}).get("rule_id", "")
    assert_true(rule_id == "rule-export-block", f"unexpected rule_id: {rule_id!r}")

    graph_url = f"{graph_base_url.rstrip('/')}/graphs/{workflow_id}/nodes"
    nodes = get_json(graph_url)
    safe_found = any(
        node.get("tool_name") == "safe_tool" and node.get("status") == "allowed"
        for node in nodes
    )
    blocked_found = any(
        node.get("tool_name") == "export_data" and node.get("status") == "blocked"
        for node in nodes
    )
    assert_true(safe_found, "graph missing allowed safe_tool action node")
    assert_true(blocked_found, "graph missing blocked export_data action node")

    violations_url = (
        f"{graph_base_url.rstrip('/')}/audit/violations?workflow_id={workflow_id}"
    )
    violations = get_json(violations_url)
    assert_true(len(violations) >= 1, "expected at least one audit violation record")
    return {"nodes_checked": len(nodes), "violations": len(violations)}


def run_queue_fallback_gate(
    workflow_id: str,
    mcp_url: str,
    graph_base_url: str,
) -> dict:
    queue_payload = {
        "jsonrpc": "2.0",
        "id": workflow_id,
        "method": "tools/call",
        "params": {
            "name": "safe_tool",
            "arguments": {"thinking": "queue fallback when verifier unavailable"},
        },
    }
    status, body = request_json(mcp_url, queue_payload)
    assert_true(status == 403, f"queue fallback expected 403, got {status}")
    queue_json = json.loads(body)
    error = queue_json.get("error", {})
    assert_true(
        error.get("message") == "POLICY_VIOLATION",
        "queue fallback missing policy violation envelope",
    )
    error_data = error.get("data", {})
    assert_true(
        isinstance(error_data, dict),
        "queue fallback missing structured error data",
    )
    assert_true(error_data.get("code") == 503, "queue fallback missing code 503")
    assert_true(error_data.get("message") == "QUEUE", "queue fallback missing QUEUE message")

    graph_url = f"{graph_base_url.rstrip('/')}/graphs/{workflow_id}/nodes"
    nodes = get_json(graph_url)
    queued_action_found = any(
        node.get("tool_name") == "safe_tool" and node.get("status") == "blocked"
        for node in nodes
    )
    assert_true(queued_action_found, "graph missing queued safe_tool action node")
    return {"nodes_checked": len(nodes), "queue_status": "verified"}


def main() -> int:
    parser = argparse.ArgumentParser(description="Run three-agent E2E gate")
    parser.add_argument("--proxy-base-url", default="http://127.0.0.1:7070")
    parser.add_argument("--graph-base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--timeout-secs", type=int, default=120)
    parser.add_argument(
        "--scenario",
        choices=["standard", "queue-fallback"],
        default="standard",
    )
    parser.add_argument("--workflow-id", default=None)
    args = parser.parse_args()

    workflow_id = args.workflow_id or (
        "wf-three-agent-e2e"
        if args.scenario == "standard"
        else "wf-three-agent-e2e-queue"
    )
    mcp_url = f"{args.proxy_base_url.rstrip('/')}/mcp/tools/call"
    a2a_url = f"{args.proxy_base_url.rstrip('/')}/a2a/tasks/send"

    wait_for_proxy(mcp_url, args.timeout_secs)

    if args.scenario == "standard":
        result = run_standard_gate(workflow_id, mcp_url, a2a_url, args.graph_base_url)
    else:
        result = run_queue_fallback_gate(workflow_id, mcp_url, args.graph_base_url)

    print(
        json.dumps(
            {
                "workflow_id": workflow_id,
                "scenario": args.scenario,
                "status": "pass",
                **result,
            }
        )
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"E2E gate failed: {exc}", file=sys.stderr)
        raise
