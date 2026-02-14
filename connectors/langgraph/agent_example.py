"""LangGraph connector skeleton using AstraGraph as tool execution gateway.

References:
- LangGraph prebuilt agent pattern (create_react_agent)
- LangChain tool decorators
"""

from __future__ import annotations

import os
from typing import Any

from langchain.chat_models import init_chat_model
from langchain_core.tools import tool
from langgraph.prebuilt import create_react_agent

from connectors.common.astragraph_proxy_client import mcp_tools_call


PROXY_BASE_URL = os.getenv("ASTRAGRAPH_PROXY_URL", "http://127.0.0.1:7070")


@tool
def safe_tool(value: str) -> str:
    """Call the safe MCP tool through AstraGraph."""
    result = mcp_tools_call(
        "safe_tool",
        {"thinking": f"langgraph request: {value}", "value": value},
        proxy_base_url=PROXY_BASE_URL,
    )
    if result.status_code == 403 and result.is_policy_block:
        return f"blocked_by_policy: {result.payload}"
    return str(result.payload)


@tool
def export_data(table: str) -> str:
    """Call a risky MCP tool through AstraGraph (expected to be policy-governed)."""
    result = mcp_tools_call(
        "export_data",
        {"thinking": f"langgraph export request for {table}", "table": table},
        proxy_base_url=PROXY_BASE_URL,
    )
    if result.status_code == 403 and result.is_policy_block:
        return f"blocked_by_policy: {result.payload}"
    return str(result.payload)


def run(prompt: str) -> dict[str, Any]:
    model = init_chat_model("openai:gpt-4o-mini")
    agent = create_react_agent(model=model, tools=[safe_tool, export_data])
    return agent.invoke({"messages": [{"role": "user", "content": prompt}]})


if __name__ == "__main__":
    output = run("Use safe_tool with value='hello' and summarize the result.")
    print(output)
