"""AutoGen connector skeleton with AstraGraph-backed tools.

Reference pattern:
- AssistantAgent from autogen-agentchat
- OpenAIChatCompletionClient from autogen-ext
"""

from __future__ import annotations

import asyncio
import os

from autogen_agentchat.agents import AssistantAgent
from autogen_ext.models.openai import OpenAIChatCompletionClient

from connectors.common.astragraph_proxy_client import mcp_tools_call


PROXY_BASE_URL = os.getenv("ASTRAGRAPH_PROXY_URL", "http://127.0.0.1:7070")


def safe_tool(value: str) -> str:
    result = mcp_tools_call(
        "safe_tool",
        {"thinking": f"autogen request: {value}", "value": value},
        proxy_base_url=PROXY_BASE_URL,
    )
    if result.status_code == 403 and result.is_policy_block:
        return f"blocked_by_policy: {result.payload}"
    return str(result.payload)


async def run() -> None:
    model_client = OpenAIChatCompletionClient(model="gpt-4.1-mini")
    agent = AssistantAgent(
        name="astragraph_guarded_agent",
        model_client=model_client,
        tools=[safe_tool],
        system_message="Use provided tools and return concise results.",
    )
    result = await agent.run(task="Use safe_tool with value 'hello' and report the output.")
    print(result)
    await model_client.close()


if __name__ == "__main__":
    asyncio.run(run())
