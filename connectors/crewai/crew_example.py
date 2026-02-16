"""CrewAI connector skeleton using a custom AstraGraph-backed tool.

Reference pattern:
- CrewAI Agent/Task/Crew usage
- Custom tool pattern via BaseTool
"""

from __future__ import annotations

import argparse
import os
from typing import Type

from pydantic import BaseModel, Field

from connectors.common.astragraph_proxy_client import ProxyClient, ProxyResult


PROXY_BASE_URL = os.getenv("ASTRAGRAPH_PROXY_URL", "http://127.0.0.1:7070")
MODEL = os.getenv("ASTRAGRAPH_CONNECTOR_MODEL", "gpt-4o-mini")
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


class AstraGraphToolInput(BaseModel):
    value: str = Field(..., description="Input payload for safe_tool")


def run(
    *,
    model_name: str = MODEL,
    task_value: str = "hello",
    verbose: bool = True,
) -> str:
    from crewai import Agent, Crew, Task
    from crewai.tools import BaseTool

    class AstraGraphSafeTool(BaseTool):
        name: str = "astragraph_safe_tool"
        description: str = "Execute safe_tool through AstraGraph MCP proxy."
        args_schema: Type[BaseModel] = AstraGraphToolInput

        def _run(self, value: str) -> str:
            result = _proxy_client.mcp_tools_call(
                "safe_tool",
                {"thinking": f"crewai request: {value}", "value": value},
            )
            return _format_result(result)

    agent = Agent(
        role="Operations Analyst",
        goal="Safely execute tool calls and summarize outcomes",
        backstory="You use policy-governed tools and report results clearly.",
        tools=[AstraGraphSafeTool()],
        verbose=verbose,
        llm=model_name,
    )
    task = Task(
        description=(
            f"Call astragraph_safe_tool with value '{task_value}' and summarize output."
        ),
        expected_output="A short summary of the tool execution result.",
        agent=agent,
    )
    crew = Crew(agents=[agent], tasks=[task], verbose=verbose)
    return str(crew.kickoff())


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Run CrewAI connector example")
    parser.add_argument("--model", default=MODEL, help="CrewAI LLM model name")
    parser.add_argument("--value", default="hello", help="Tool input value")
    parser.add_argument(
        "--quiet",
        action="store_true",
        help="Disable CrewAI verbose mode",
    )
    args = parser.parse_args()
    print(run(model_name=args.model, task_value=args.value, verbose=not args.quiet))
