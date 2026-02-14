# Framework Connectors (Skeletons)

These examples show how to route agent tool calls through the AstraGraph proxy.

- `langgraph/agent_example.py`: LangGraph + LangChain tool wiring
- `crewai/crew_example.py`: CrewAI custom tool wiring
- `autogen/assistant_example.py`: Microsoft AutoGen AssistantAgent tool wiring
- `common/astragraph_proxy_client.py`: shared proxy client helpers

They are intentionally lightweight starter adapters, not production SDKs.

## Prerequisites

- AstraGraph proxy running (`http://127.0.0.1:7070`)
- Python dependencies for the chosen framework

## Why this structure

Each framework adapter delegates tool execution to the same proxy client helper,
so policy enforcement, graph writes, and audit behavior are consistent.
