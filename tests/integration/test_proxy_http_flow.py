import json
import os
import urllib.request

import pytest


@pytest.mark.skipif(
    os.getenv("ASTRAGRAPH_E2E") != "1",
    reason="Set ASTRAGRAPH_E2E=1 to run end-to-end tests",
)
def test_proxy_tools_call_roundtrip():
    url = os.getenv("ASTRAGRAPH_PROXY_URL", "http://127.0.0.1:7070/mcp/tools/call")
    payload = json.dumps(
        {
            "jsonrpc": "2.0",
            "id": "req-1",
            "method": "tools/call",
            "params": {"name": "noop", "arguments": {"ping": "pong"}},
        }
    ).encode("utf-8")
    request = urllib.request.Request(
        url,
        data=payload,
        headers={"content-type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=3) as response:
        assert response.status in (200, 403)
