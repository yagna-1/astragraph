"""CrewAI connector skeleton using a custom AstraGraph-backed tool.

Reference pattern:
- CrewAI Agent/Task/Crew usage
- Custom tool pattern via BaseTool
"""

from __future__ import annotations

import os
from typing import Type

from crewai import Agent, Crew, Task
from crewai.tools import BaseTool
from pydantic import BaseModel, Field

from connectors.common.astragraph_proxy_client import mcp_tools_call


PROXY_BASE_URL = os.getenv("ASTRAGRAPH_PROXY_URL", "http://127.0.0.1:7070")


class AstraGraphToolInput(BaseModel):
    value: str = Field(..., description="Input payload for safe_tool")


class AstraGraphSafeTool(BaseTool):
    name: str = "astragraph_safe_tool"
    description: str = "Execute safe_tool through AstraGraph MCP proxy."
    args_schema: Type[BaseModel] = AstraGraphToolInput

    def _run(self, value: str) -> str:
        result = mcp_tools_call(
            "safe_tool",
            {"thinking": f"crewai request: {value}", "value": value},
            proxy_base_url=PROXY_BASE_URL,
        )
        if result.status_code == 403 and result.is_policy_block:
            return f"blocked_by_policy: {result.payload}"
        return str(result.payload)


def run() -> str:
    agent = Agent(
        role="Operations Analyst",
        goal="Safely execute tool calls and summarize outcomes",
        backstory="You use policy-governed tools and report results clearly.",
        tools=[AstraGraphSafeTool()],
        verbose=True,
    )
    task = Task(
        description="Call astragraph_safe_tool with value 'hello' and summarize output.",
        expected_output="A short summary of the tool execution result.",
        agent=agent,
    )
    crew = Crew(agents=[agent], tasks=[task], verbose=True)
    return str(crew.kickoff())


if __name__ == "__main__":
    print(run())
