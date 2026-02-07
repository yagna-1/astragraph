# AstraGraph

Distributed causal-graph observability and policy enforcement for tool-using, multi-agent systems (MCP + A2A).

AstraGraph runs as a **sidecar proxy** that intercepts agent tool calls, applies **Policy-as-Code** decisions before execution, and writes a **causal coordination graph** plus **audit violations** you can query.

## Quickstart (10 minutes)

Prereqs: Docker Desktop, Python 3, Rust toolchain (optional for local dev).

```bash
cd ASTRAGRAPH
./scripts/e2e_run.sh
```

Expected result: the 3-agent E2E gate blocks `export_data`, records an audit violation, and exits with `status: pass`.

## Repository Layout

- `ASTRAGRAPH/proxy/`: Rust sidecar proxy (MCP + A2A interceptors)
- `ASTRAGRAPH/graph/`: Rust graph service (REST + gRPC)
- `ASTRAGRAPH/policy/`: Rust policy engine (REST + gRPC, hot-reload)
- `ASTRAGRAPH/verifier/`: Python verifier service (mock + vLLM path)
- `ASTRAGRAPH/scripts/`: local demo + fixtures + E2E gate

## License

Apache-2.0 (see `LICENSE`).

