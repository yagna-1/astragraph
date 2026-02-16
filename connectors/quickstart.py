#!/usr/bin/env python3
"""Framework-agnostic connector quickstart checks through AstraGraph proxy."""

from __future__ import annotations

import argparse
import json
import time

from connectors.common.astragraph_proxy_client import ProxyClient


def run_quickstart(
    *,
    proxy_base_url: str,
    workflow_id: str,
    task_id: str,
    target_agent_id: str,
    timeout: float,
) -> dict:
    client = ProxyClient(proxy_base_url=proxy_base_url, timeout=timeout)

    safe_result = client.mcp_tools_call(
        "safe_tool",
        {
            "thinking": "connector quickstart safe path",
            "value": "hello",
        },
    )

    blocked_result = client.mcp_tools_call(
        "export_data",
        {
            "thinking": "connector quickstart export path",
            "table": "customers",
        },
    )

    a2a_result = client.a2a_tasks_send(
        workflow_id=workflow_id,
        task_id=task_id,
        target_agent_id=target_agent_id,
        message_text="connector quickstart handoff",
    )

    summary = {
        "safe_status": safe_result.status_code,
        "safe_ok": safe_result.is_success,
        "blocked_status": blocked_result.status_code,
        "blocked_policy": blocked_result.is_policy_block,
        "blocked_rule": blocked_result.policy_rule_id,
        "queue_fallback": blocked_result.is_queue_fallback,
        "a2a_status": a2a_result.status_code,
        "a2a_payload_kind": "sse"
        if "event: task_status" in str(a2a_result.payload.get("raw", ""))
        else "json",
    }
    return summary


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Run connector quickstart checks against AstraGraph proxy"
    )
    parser.add_argument("--proxy-base-url", default="http://127.0.0.1:7070")
    parser.add_argument("--workflow-id", default=f"wf-connector-quickstart-{int(time.time())}")
    parser.add_argument("--task-id", default="task-connector-quickstart")
    parser.add_argument("--target-agent-id", default="contract-reviewer")
    parser.add_argument("--timeout", type=float, default=10.0)
    args = parser.parse_args()

    summary = run_quickstart(
        proxy_base_url=args.proxy_base_url,
        workflow_id=args.workflow_id,
        task_id=args.task_id,
        target_agent_id=args.target_agent_id,
        timeout=args.timeout,
    )
    print(json.dumps(summary))

    expected_ok = (
        summary["safe_ok"]
        and summary["blocked_status"] == 403
        and summary["blocked_policy"]
        and summary["a2a_status"] == 200
    )
    return 0 if expected_ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
