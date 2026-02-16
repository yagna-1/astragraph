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
    request_url: str | None = None

    @property
    def is_policy_block(self) -> bool:
        error = self.payload.get("error", {})
        return isinstance(error, dict) and error.get("message") == "POLICY_VIOLATION"

    @property
    def is_success(self) -> bool:
        return 200 <= self.status_code < 300 and "error" not in self.payload

    @property
    def is_queue_fallback(self) -> bool:
        if not self.is_policy_block:
            return False
        data = self.payload.get("error", {}).get("data", {})
        return isinstance(data, dict) and data.get("message") == "QUEUE"

    @property
    def queue_detail(self) -> str | None:
        if not self.is_queue_fallback:
            return None
        data = self.payload.get("error", {}).get("data", {})
        detail = data.get("data", {}).get("detail")
        if isinstance(detail, str):
            return detail
        fallback_detail = data.get("detail")
        return fallback_detail if isinstance(fallback_detail, str) else None

    @property
    def policy_rule_id(self) -> str | None:
        if not self.is_policy_block:
            return None
        rule_id = self.payload.get("error", {}).get("data", {}).get("rule_id")
        return rule_id if isinstance(rule_id, str) and rule_id else None

    @property
    def error_message(self) -> str | None:
        error = self.payload.get("error", {})
        if not isinstance(error, dict):
            return None
        message = error.get("message")
        if isinstance(message, str):
            return message
        return None


def normalize_proxy_base_url(proxy_base_url: str) -> str:
    base = (proxy_base_url or "").strip()
    if not base:
        return "http://127.0.0.1:7070"
    return base.rstrip("/")


class ProxyClient:
    def __init__(self, *, proxy_base_url: str = "http://127.0.0.1:7070", timeout: float = 10.0):
        self.proxy_base_url = normalize_proxy_base_url(proxy_base_url)
        self.timeout = timeout

    def mcp_tools_call(
        self,
        tool_name: str,
        arguments: dict[str, Any],
        *,
        request_id: str | None = None,
        timeout: float | None = None,
    ) -> ProxyResult:
        rid = request_id or f"req-{uuid.uuid4()}"
        payload = {
            "jsonrpc": "2.0",
            "id": rid,
            "method": "tools/call",
            "params": {"name": tool_name, "arguments": arguments},
        }
        return self._post_json(
            f"{self.proxy_base_url}/mcp/tools/call",
            payload,
            timeout=timeout,
        )

    def a2a_tasks_send(
        self,
        *,
        workflow_id: str,
        task_id: str,
        target_agent_id: str,
        message_text: str,
        timeout: float | None = None,
    ) -> ProxyResult:
        payload = {
            "id": workflow_id,
            "task_id": task_id,
            "target_agent_id": target_agent_id,
            "message": {"parts": [{"text": message_text}]},
        }
        return self._post_json(
            f"{self.proxy_base_url}/a2a/tasks/send",
            payload,
            timeout=timeout,
        )

    def _post_json(
        self,
        url: str,
        payload: dict[str, Any],
        *,
        timeout: float | None = None,
    ) -> ProxyResult:
        try:
            response = requests.post(url, json=payload, timeout=timeout or self.timeout)
        except requests.RequestException as exc:
            return ProxyResult(
                status_code=599,
                payload={
                    "error": {
                        "message": "TRANSPORT_ERROR",
                        "data": {"detail": str(exc)},
                    }
                },
                request_url=url,
            )
        return ProxyResult(
            status_code=response.status_code,
            payload=_safe_json(response),
            request_url=url,
        )


def mcp_tools_call(
    tool_name: str,
    arguments: dict[str, Any],
    *,
    proxy_base_url: str = "http://127.0.0.1:7070",
    request_id: str | None = None,
    timeout: float = 10.0,
) -> ProxyResult:
    """Send a JSON-RPC MCP tools/call through AstraGraph proxy."""
    client = ProxyClient(proxy_base_url=proxy_base_url, timeout=timeout)
    return client.mcp_tools_call(
        tool_name,
        arguments,
        request_id=request_id,
        timeout=timeout,
    )


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
    client = ProxyClient(proxy_base_url=proxy_base_url, timeout=timeout)
    return client.a2a_tasks_send(
        workflow_id=workflow_id,
        task_id=task_id,
        target_agent_id=target_agent_id,
        message_text=message_text,
        timeout=timeout,
    )


def _safe_json(response: requests.Response) -> dict[str, Any]:
    try:
        return response.json()
    except json.JSONDecodeError:
        return {"raw": response.text}
