# Verifier Interface Contract

This document defines the production contract for replacing AstraGraph's reference verifier with your own backend.

## Scope

- **Source of truth:** `proto/astragraph.proto`
- **Service:** `astragraph.v1.VerifierService`
- **RPCs:**
  - `ScoreAction(VerifierRequest) returns (VerifierResponse)` (optional in current proxy path)
  - `StreamScore(stream VerifierRequest) returns (stream VerifierResponse)` (**required** in current proxy path)

The proxy currently calls `StreamScore` with a single request item and expects at least one response item.

## Request/Response Schema

### `VerifierRequest`

- `policy_text` (`string`): policy context string assembled by proxy (policy id, rule id, threshold, fallback).
- `agent_reasoning` (`string`): extracted chain-of-thought style trace fragment (may be empty when unavailable).
- `agent_action` (`string`): normalized action summary (tool + arguments).

### `VerifierResponse`

- `deviation_score` (`float`): higher means higher policy deviation risk.
- `verifier_model` (`string`): model/version identifier for auditability.
- `latency_ms` (`uint32`): model inference latency in milliseconds.
- `verifier_thinking` (`string`): verifier rationale text stored in verification nodes.

## Behavioral Requirements

- Return a deterministic and bounded `deviation_score` in `[0.0, 1.0]`.
- Keep response shape stable; missing fields break downstream audit assumptions.
- Preserve `verifier_model` with immutable version tags (for model-upgrade gates).
- Return at least one `VerifierResponse` message per incoming `VerifierRequest` on `StreamScore`.
- Handle malformed/empty reasoning safely (no panics, no unbounded retries).

## Failure Semantics

- If the verifier is unavailable, the proxy applies policy fallback:
  - `QUEUE` => queue-fallback policy violation envelope (`403` + queue detail)
  - `BLOCK`/`ALLOW` => current enforcement fallback behavior
- Startup dependency can be controlled with `ASTRAGRAPH_VERIFIER_REQUIRED_AT_STARTUP`:
  - `true` (default): proxy waits for verifier reachability at startup
  - `false`: proxy starts in degraded mode and handles verifier outages via fallback path
- Timeouts/unavailable must fail fast; avoid long hangs that stall proxy request path.

## Security + Transport

- mTLS is expected in standard AstraGraph deployment:
  - server cert/key and CA are configured via `ASTRAGRAPH_VERIFIER_TLS_CERT`, `ASTRAGRAPH_VERIFIER_TLS_KEY`, `ASTRAGRAPH_VERIFIER_TLS_CA`.
- Do not log raw sensitive action arguments unless explicitly redacted.
- Ensure model prompts cannot execute tool calls or side effects.

## Compatibility Requirements

- Keep protobuf compatibility with `proto/astragraph.proto` v1.
- For breaking changes, version the proto package and update proxy + CI together.
- Validate compatibility against:
  - `docs/api_policy_compatibility_matrix.md`
  - `eval/model_upgrade_gate.py`
  - `eval/anonymized_trace_eval.py`

## Minimal Production Checklist

- [ ] Implements `StreamScore` contract exactly as above.
- [ ] Has availability SLO and timeout budget aligned with proxy gate latency.
- [ ] Produces stable `verifier_model` version strings.
- [ ] Supports safe rollout (shadow/canary) and rollback.
- [ ] Passes AstraGraph CI eval gates before promotion.
