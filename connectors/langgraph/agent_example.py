"""LangGraph connector skeleton using AstraGraph as tool execution gateway.

References:
- LangGraph prebuilt agent pattern (create_react_agent)
- LangChain tool decorators
"""

from __future__ import annotations

import argparse
import os
from typing import Any

from connectors.common.astragraph_proxy_client import ProxyClient, ProxyResult


PROXY_BASE_URL = os.getenv("ASTRAGRAPH_PROXY_URL", "http://127.0.0.1:7070")
MODEL = os.getenv("ASTRAGRAPH_CONNECTOR_MODEL", "openai:gpt-4o-mini")
_proxy_client = ProxyClient(proxy_base_url=PROXY_BASE_URL, timeout=10.0)

def _format_result(result: ProxyResult) -> str:
    if result.is_policy_block:
        rule = result.policy_rule_id
        if result.is_queue_fallback:
            detail = result.queue_detail or "queued for verification"
            return f"queued_by_policy(status={result.status_code}, detail={detail})"
        if rule:
            return f"blocked_by_policy(status={result.status_code}, rule={rule})"
        return f"blocked_by_policy(status={result.status_code})"
    if not result.is_success:
        return f"proxy_error(status={result.status_code}, error={result.error_message})"
    return str(result.payload)

def run(prompt: str, *, model_name: str = MODEL) -> dict[str, Any]:
    from langchain.chat_models import init_chat_model
    from langchain_core.tools import tool
    from langgraph.prebuilt import create_react_agent

    @tool
    def safe_tool(value: str) -> str:
        """Call the safe MCP tool through AstraGraph."""
        result = _proxy_client.mcp_tools_call(
            "safe_tool",
            {"thinking": f"langgraph request: {value}", "value": value},
        )
        return _format_result(result)

    @tool
    def export_data(table: str) -> str:
        """Call a risky MCP tool through AstraGraph (expected to be policy-governed)."""
        result = _proxy_client.mcp_tools_call(
            "export_data",
            {"thinking": f"langgraph export request for {table}", "table": table},
        )
        return _format_result(result)

    model = init_chat_model(model_name)
    agent = create_react_agent(model=model, tools=[safe_tool, export_data])
    return agent.invoke({"messages": [{"role": "user", "content": prompt}]})


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Run LangGraph connector example")
    parser.add_argument(
        "--prompt",
        default="Use safe_tool with value='hello' and summarize the result.",
    )
    parser.add_argument(
        "--model",
        default=MODEL,
        help="LangChain model identifier, e.g. openai:gpt-4o-mini",
    )
    args = parser.parse_args()
    output = run(args.prompt, model_name=args.model)
    print(output)
