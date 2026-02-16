"""AutoGen connector skeleton with AstraGraph-backed tools.

Reference pattern:
- AssistantAgent from autogen-agentchat
- OpenAIChatCompletionClient from autogen-ext
"""

from __future__ import annotations

import asyncio
import argparse
import os

from connectors.common.astragraph_proxy_client import ProxyClient, ProxyResult


PROXY_BASE_URL = os.getenv("ASTRAGRAPH_PROXY_URL", "http://127.0.0.1:7070")
MODEL = os.getenv("ASTRAGRAPH_CONNECTOR_MODEL", "gpt-4.1-mini")
_proxy_client = ProxyClient(proxy_base_url=PROXY_BASE_URL, timeout=10.0)


def _format_result(result: ProxyResult) -> str:
    if result.is_policy_block:
        if result.is_queue_fallback:
            detail = result.queue_detail or "queued for verification"
            return f"queued_by_policy(status={result.status_code}, detail={detail})"
        rule = result.policy_rule_id
        if rule:
            return f"blocked_by_policy(status={result.status_code}, rule={rule})"
        return f"blocked_by_policy(status={result.status_code})"
    if not result.is_success:
        return f"proxy_error(status={result.status_code}, error={result.error_message})"
    return str(result.payload)


def safe_tool(value: str) -> str:
    result = _proxy_client.mcp_tools_call(
        "safe_tool",
        {"thinking": f"autogen request: {value}", "value": value},
    )
    return _format_result(result)


async def run(*, model_name: str = MODEL, task_prompt: str | None = None) -> None:
    from autogen_agentchat.agents import AssistantAgent
    from autogen_ext.models.openai import OpenAIChatCompletionClient

    model_client = OpenAIChatCompletionClient(model=model_name)
    agent = AssistantAgent(
        name="astragraph_guarded_agent",
        model_client=model_client,
        tools=[safe_tool],
        system_message="Use provided tools and return concise results.",
    )
    prompt = task_prompt or "Use safe_tool with value 'hello' and report the output."
    result = await agent.run(task=prompt)
    print(result)
    await model_client.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Run AutoGen connector example")
    parser.add_argument("--model", default=MODEL, help="AutoGen model name")
    parser.add_argument(
        "--prompt",
        default="Use safe_tool with value 'hello' and report the output.",
        help="Task prompt to run",
    )
    args = parser.parse_args()
    asyncio.run(run(model_name=args.model, task_prompt=args.prompt))
