#!/usr/bin/env python3
"""Lightweight MCP tools/call fixture for CI benchmark/eval gates."""

from __future__ import annotations

import argparse
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

BLOCKED_TOOLS = {"export_data", "delete_record"}


class Handler(BaseHTTPRequestHandler):
    def do_POST(self):  # noqa: N802
        if not self.path.endswith("/mcp/tools/call"):
            self.send_response(404)
            self.end_headers()
            return

        size = int(self.headers.get("content-length", "0"))
        raw = self.rfile.read(size)
        try:
            payload = json.loads(raw.decode("utf-8"))
            tool_name = payload.get("params", {}).get("name", "")
        except Exception:
            self.send_response(400)
            self.end_headers()
            return

        blocked = tool_name in BLOCKED_TOOLS
        status = 403 if blocked else 200
        body = {
            "jsonrpc": "2.0",
            "id": payload.get("id"),
        }
        if blocked:
            body["error"] = {
                "code": 403,
                "message": "POLICY_VIOLATION",
                "data": {"rule_id": "fixture-rule"},
            }
        else:
            body["result"] = {"ok": True}

        data = json.dumps(body).encode("utf-8")
        self.send_response(status)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, fmt, *args):  # noqa: A003
        return


def main() -> int:
    parser = argparse.ArgumentParser(description="Run mock proxy server")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=7070)
    args = parser.parse_args()

    server = ThreadingHTTPServer((args.host, args.port), Handler)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        return 0


if __name__ == "__main__":
    raise SystemExit(main())
