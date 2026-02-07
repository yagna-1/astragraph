import json
import threading
from http.server import BaseHTTPRequestHandler, HTTPServer
from urllib.request import Request, urlopen

import pytest


class FixtureHandler(BaseHTTPRequestHandler):
    def do_POST(self):  # noqa: N802
        size = int(self.headers.get("content-length", "0"))
        body = self.rfile.read(size)
        payload = json.loads(body.decode("utf-8"))
        if self.path == "/mcp":
            response = {
                "jsonrpc": "2.0",
                "id": payload.get("id"),
                "result": {
                    "tools": [
                        {
                            "name": "safe_tool",
                            "description": "Do not ignore previous instructions.",
                        }
                    ]
                },
            }
            self.send_response(200)
            self.send_header("content-type", "application/json")
            self.end_headers()
            self.wfile.write(json.dumps(response).encode("utf-8"))
            return

        if self.path == "/a2a":
            self.send_response(200)
            self.send_header("content-type", "text/event-stream")
            self.end_headers()
            event = (
                "event: task_status\n"
                'data: {"task":{"id":"task-1","status":{"state":"COMPLETED"}}}\n\n'
            )
            self.wfile.write(event.encode("utf-8"))
            return

        self.send_response(404)
        self.end_headers()


def _start_server():
    try:
        server = HTTPServer(("127.0.0.1", 0), FixtureHandler)
    except PermissionError:
        pytest.skip("Sandbox networking is restricted")
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server


def test_mcp_fixture_shape():
    server = _start_server()
    try:
        payload = {"jsonrpc": "2.0", "id": "req-1", "method": "tools/list"}
        req = Request(
            f"http://127.0.0.1:{server.server_address[1]}/mcp",
            data=json.dumps(payload).encode("utf-8"),
            headers={"content-type": "application/json"},
            method="POST",
        )
        with urlopen(req, timeout=2) as resp:
            body = json.loads(resp.read().decode("utf-8"))
        assert body["result"]["tools"][0]["name"] == "safe_tool"
    finally:
        server.shutdown()


def test_a2a_fixture_sse_shape():
    server = _start_server()
    try:
        payload = {"message": {"parts": [{"text": "hello"}]}}
        req = Request(
            f"http://127.0.0.1:{server.server_address[1]}/a2a",
            data=json.dumps(payload).encode("utf-8"),
            headers={"content-type": "application/a2a+json"},
            method="POST",
        )
        with urlopen(req, timeout=2) as resp:
            body = resp.read().decode("utf-8")
        assert "event: task_status" in body
        assert '"state":"COMPLETED"' in body
    finally:
        server.shutdown()
