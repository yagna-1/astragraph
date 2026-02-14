"""Shared AstraGraph proxy client helpers for framework connectors."""

from __future__ import annotations

import json
import uuid
from dataclasses import dataclass
from typing import Any

import requests


@dataclass
class ProxyResult:
    status_code: int
    payload: dict[str, Any]

    @property
    def is_policy_block(self) -> bool:
        error = self.payload.get("error", {})
        return isinstance(error, dict) and error.get("message") == "POLICY_VIOLATION"


def mcp_tools_call(
    tool_name: str,
    arguments: dict[str, Any],
    *,
    proxy_base_url: str = "http://127.0.0.1:7070",
    request_id: str | None = None,
    timeout: float = 10.0,
) -> ProxyResult:
    """Send a JSON-RPC MCP tools/call through AstraGraph proxy."""
    rid = request_id or f"req-{uuid.uuid4()}"
    payload = {
        "jsonrpc": "2.0",
        "id": rid,
        "method": "tools/call",
        "params": {"name": tool_name, "arguments": arguments},
    }
    response = requests.post(
        f"{proxy_base_url.rstrip('/')}/mcp/tools/call",
        json=payload,
        timeout=timeout,
    )
    return ProxyResult(status_code=response.status_code, payload=_safe_json(response))


def a2a_tasks_send(
    *,
    workflow_id: str,
    task_id: str,
    target_agent_id: str,
    message_text: str,
    proxy_base_url: str = "http://127.0.0.1:7070",
    timeout: float = 10.0,
) -> ProxyResult:
    """Send an A2A task through AstraGraph proxy."""
    payload = {
        "id": workflow_id,
        "task_id": task_id,
        "target_agent_id": target_agent_id,
        "message": {"parts": [{"text": message_text}]},
    }
    response = requests.post(
        f"{proxy_base_url.rstrip('/')}/a2a/tasks/send",
        json=payload,
        timeout=timeout,
    )
    return ProxyResult(status_code=response.status_code, payload=_safe_json(response))


def _safe_json(response: requests.Response) -> dict[str, Any]:
    try:
        return response.json()
    except json.JSONDecodeError:
        return {"raw": response.text}
