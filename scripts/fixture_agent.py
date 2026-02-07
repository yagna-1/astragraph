#!/usr/bin/env python3
"""Deterministic fixture agent server for dockerized E2E tests."""

from __future__ import annotations

import argparse
import json
from http.server import BaseHTTPRequestHandler, HTTPServer


def build_agent_card(role: str) -> dict:
    return {
        "name": role,
        "version": "1.0",
        "capabilities": ["mcp", "a2a"],
    }


class FixtureAgentHandler(BaseHTTPRequestHandler):
    role = "agent"

    def do_GET(self):  # noqa: N802
        if self.path == "/.well-known/agent-card.json":
            self._json_response(200, build_agent_card(self.role))
            return
        if self.path == "/healthz":
            self._json_response(200, {"ok": True, "role": self.role})
            return
        self.send_response(404)
        self.end_headers()

    def do_POST(self):  # noqa: N802
        if self.path.endswith("/tasks/send"):
            if self.role != "proposal-writer":
                self._json_response(404, {"error": "unsupported"})
                return
            self._a2a_task_status_stream()
            return

        if self.path.endswith("/tools/call"):
            if self.role != "contract-reviewer":
                self._json_response(404, {"error": "unsupported"})
                return
            self._mcp_tool_call()
            return

        self.send_response(404)
        self.end_headers()

    def _mcp_tool_call(self) -> None:
        payload = self._read_json_body()
        if payload is None:
            self._json_response(400, {"error": "invalid json"})
            return
        tool_name = payload.get("params", {}).get("name", "")
        response = {
            "jsonrpc": "2.0",
            "id": payload.get("id"),
            "result": {
                "ok": True,
                "handled_by": self.role,
                "tool_name": tool_name,
            },
        }
        self._json_response(200, response)

    def _a2a_task_status_stream(self) -> None:
        size = int(self.headers.get("content-length", "0"))
        raw = self.rfile.read(size)
        payload = json.loads(raw.decode("utf-8")) if raw else {}
        task_id = payload.get("task_id", "task-e2e-1")
        working = (
            "event: task_status\n"
            f'data: {json.dumps({"task": {"id": task_id, "status": {"state": "WORKING"}}})}\n\n'
        )
        completed = (
            "event: task_status\n"
            "data: "
            + json.dumps(
                {
                    "task": {
                        "id": task_id,
                        "status": {"state": "COMPLETED"},
                        "artifacts": [
                            {
                                "parts": [
                                    {
                                        "text": "handoff approved for contract-reviewer"
                                    }
                                ]
                            }
                        ],
                    }
                }
            )
            + "\n\n"
        )
        encoded = (working + completed).encode("utf-8")
        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        self.send_header("cache-control", "no-cache")
        self.send_header("content-length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

    def _read_json_body(self) -> dict | None:
        size = int(self.headers.get("content-length", "0"))
        raw = self.rfile.read(size)
        if not raw:
            return {}
        try:
            return json.loads(raw.decode("utf-8"))
        except json.JSONDecodeError:
            return None

    def _json_response(self, status: int, payload: dict) -> None:
        data = json.dumps(payload).encode("utf-8")
        self.send_response(status)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, fmt, *args):  # noqa: A003
        return


def main() -> int:
    parser = argparse.ArgumentParser(description="Run fixture agent server")
    parser.add_argument("--role", required=True)
    parser.add_argument("--host", default="0.0.0.0")
    parser.add_argument("--port", type=int, required=True)
    args = parser.parse_args()

    handler = type("RoleHandler", (FixtureAgentHandler,), {"role": args.role})
    server = HTTPServer((args.host, args.port), handler)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        return 0


if __name__ == "__main__":
    raise SystemExit(main())
