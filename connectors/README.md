# Framework Connectors

These examples route framework tool calls through the AstraGraph proxy while preserving a consistent policy/audit path.

## Included adapters

- `common/astragraph_proxy_client.py`: shared typed client (`ProxyClient`) for MCP + A2A calls
- `quickstart.py`: framework-agnostic smoke flow (safe MCP, blocked MCP, A2A handoff)
- `langgraph/agent_example.py`: LangGraph/LangChain adapter
- `crewai/crew_example.py`: CrewAI adapter
- `autogen/assistant_example.py`: AutoGen adapter

## Prerequisites

- AstraGraph proxy running at `http://127.0.0.1:7070` (or set `ASTRAGRAPH_PROXY_URL`)
- Python 3.11+
- `pip install requests`

## Quickstart (framework-agnostic)

From repo root:

```bash
python3 connectors/quickstart.py --proxy-base-url http://127.0.0.1:7070
```

Expected behavior:
- safe MCP tool call succeeds
- risky MCP tool call is policy-blocked (`403 POLICY_VIOLATION`)
- A2A handoff call returns `200`

## Framework runs

LangGraph:

```bash
pip install langgraph langchain langchain-openai requests
python3 connectors/langgraph/agent_example.py --prompt "Use safe_tool with value='hello'"
```

CrewAI:

```bash
pip install crewai requests
python3 connectors/crewai/crew_example.py --value "hello"
```

AutoGen:

```bash
pip install autogen-agentchat autogen-ext requests
python3 connectors/autogen/assistant_example.py --prompt "Use safe_tool with value 'hello'"
```

## Environment variables

- `ASTRAGRAPH_PROXY_URL`: proxy base URL (default `http://127.0.0.1:7070`)
- `ASTRAGRAPH_CONNECTOR_MODEL`: default model value for connector examples
