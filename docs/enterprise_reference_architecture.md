# Enterprise Reference Architecture

Last validated: 2026-02-16 (`main`)

## Scope

This reference architecture maps AstraGraph runtime controls to enterprise requirements for:

- authentication and authorization
- key management and transport security
- SIEM evidence exports
- AstraCloud-ready multi-tenant boundaries

## Control Plane Boundaries (AstraCloud-ready)

- **Organization boundary**: top-level trust boundary and billing/audit owner.
- **Project boundary**: isolated runtime deployments (separate proxy/policy/graph/verifier stack).
- **Policy domain boundary**: scoped policy packs (for example `finance`, `hr`, `support`) with independent rollout history and evidence exports.

Recommended tenancy mapping:

1. One Kubernetes namespace per project boundary.
2. Distinct policy signing keys per policy domain.
3. Distinct bearer tokens/roles per organization project.

## Runtime Security Controls

- **AuthN/AuthZ**
  - REST services require bearer token and role checks (`read`, `admin`, `audit`).
  - Prefer external OIDC issuer + short-lived JWTs in production.
- **Key management**
  - mTLS for service-to-service gRPC (graph/policy/verifier/proxy).
  - `ASTRAGRAPH_POLICY_BUNDLE_SIGNING_KEY` for signed policy bundle verification.
  - Rotate keys with overlap windows; store in cloud KMS/secret manager.
- **Audit integrity**
  - Fail-closed proxy behavior for blocked/queue outcomes.
  - Signed policy rollout lifecycle events + history.

## SIEM and Evidence Export

AstraGraph graph service exposes `GET /audit/export` with export modes:

- raw CSV (`format=csv` default)
- raw JSON (`format=json`)
- SOC2 evidence envelope (`schema=soc2_v1`)
- ISO/IEC 42001 evidence envelope (`schema=iso42001_v1`)

Schema files:

- `docs/schemas/audit_export_soc2_v1.schema.json`
- `docs/schemas/audit_export_iso42001_v1.schema.json`

Example SOC2 export:

```bash
curl -H "Authorization: Bearer dev-token" \
  "http://localhost:8080/audit/export?schema=soc2_v1&org=acme&project=payments&policy_domain=finance"
```

Example ISO/IEC 42001 export:

```bash
curl -H "Authorization: Bearer dev-token" \
  "http://localhost:8080/audit/export?schema=iso42001_v1&org=acme&project=payments&policy_domain=finance"
```

## Deployment Pattern

1. Edge ingress routes MCP/A2A traffic through AstraGraph proxy sidecars.
2. Policy service enforces signed policy bundles and staged rollout controls.
3. Graph service stores causal/audit records and exports evidence bundles.
4. Verifier service scores high-risk actions with model-upgrade gating in CI.
5. SIEM ingests schema exports on a schedule (CSV/JSON/SOC2/ISO42001).
