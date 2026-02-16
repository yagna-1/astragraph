# API + Policy Compatibility Matrix

Last validated: 2026-02-16 (`main`)

This matrix defines compatibility expectations between AstraGraph runtime services and policy bundles.

## Runtime API Surface

| Surface | Version | Status | Notes |
| --- | --- | --- | --- |
| Proxy MCP ingress (`POST /mcp/tools/call`) | v1 | Stable | JSON-RPC `tools/call` interception with policy enforcement |
| Proxy A2A ingress (`POST /a2a/tasks/send`) | v1 | Stable | HTTP + SSE compatible forwarding with enforcement |
| Graph REST (`/graphs`, `/audit/*`) | v1 | Stable | Read/audit APIs with bearer auth |
| Graph SLO REST (`GET /audit/slo`) | v1 | Stable | Latency percentiles + block rate + false-positive review queue slices |
| Graph export schema REST (`GET /audit/export?schema=*`) | v1 | Stable | SOC2/ISO42001 evidence envelope exports |
| Policy REST (`/policies/*`) | v1 | Stable | Validate/rollout/history/rollback APIs |
| Policy gRPC (`evaluate_action`) | v1 | Stable | Proxy-to-policy decision contract |
| Graph gRPC (`stream_nodes`, `stream_edges`, `get_drift_path`) | v1 | Stable | Proxy-to-graph write/read contract |
| Verifier gRPC (`score_action`, `stream_score`) | v1 | Stable | Proxy-to-verifier scoring contract |

## Policy Bundle Compatibility

| Policy Field | Required | Supported Values | Runtime Behavior |
| --- | --- | --- | --- |
| `apiVersion` | Yes | `astragraph.io/v1` | Parser rejects unsupported values |
| `kind` | Yes | `AgentPolicy` | Parser rejects unsupported values |
| `metadata.version` | Yes | semantic-ish string (`"1.0"`, `"1.1"`) | Stored in policy history/rollouts |
| `spec.runtime.version` | Optional | `v1`, `v2` | Enables versioned policy runtime behavior |
| `spec.runtime.advanced_mode.engine` | Optional | `DSL`, `OPA_COMPAT` | Advanced rules only evaluated when `ASTRAGRAPH_POLICY_ADVANCED_MODE=true` |
| `spec.advanced_rules[].expression` | Optional | DSL or OPA-compatible expression | Evaluated before legacy rules when advanced mode is enabled |
| `spec.verification.fallback` | Yes | `ALLOW`, `BLOCK`, `QUEUE` | Mapped to proxy fallback decision path |
| `spec.rules[].action` | Yes | `ALLOW`, `BLOCK` | Evaluated in policy engine |
| `spec.rules[].require_confirmation` | Optional | `true`/`false` | Included in policy decision payload |
| `spec.rules[].time_window` | Optional | `start`, `end`, optional `outside_window_action` | Time-window aware allow/block decisions |

## Signed Bundle Enforcement Compatibility

| Feature | Config | Behavior |
| --- | --- | --- |
| Bundle signing disabled | `ASTRAGRAPH_POLICY_BUNDLE_SIGNING_KEY` unset | `signature` field optional; requests behave as before |
| Bundle signing enabled | `ASTRAGRAPH_POLICY_BUNDLE_SIGNING_KEY=<secret>` | `POST /policies/validate` and `POST /policies/:name/rollout` require valid `HS256` JWT signature with exact raw YAML in `yaml` claim |
| Signature mismatch | any | Validate returns `valid=false`; rollout returns `401` |

## Backward Compatibility Rules

1. Runtime v1 APIs remain backward compatible for additive changes only.
2. Policy parser remains strict on `apiVersion`/`kind` to prevent ambiguous runtime behavior.
3. New policy features must default to safe behavior when omitted.
4. Fallback mode semantics (`ALLOW`/`BLOCK`/`QUEUE`) are treated as compatibility-critical and cannot be repurposed.
