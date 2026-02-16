# Kubernetes Multi-Tenant Reference Deployment

Last validated: 2026-02-16 (`main`)

## First 15 Minutes (Docker + Helm)

1. Local validation (Docker):

```bash
./scripts/e2e_run.sh
```

2. Create namespace and install chart:

```bash
kubectl create namespace astragraph-dev
helm upgrade --install astragraph charts/astragraph -n astragraph-dev
```

3. Verify pods:

```bash
kubectl get pods -n astragraph-dev
```

## Multi-Tenant Layout

- Namespace per project (`astragraph-project-a`, `astragraph-project-b`)
- Distinct policy signing key and auth token set per namespace
- Optional shared observability namespace for OTEL collector + Prometheus

## Isolation Controls

- NetworkPolicy: restrict proxy/policy/graph/verifier east-west traffic to namespace.
- RBAC: separate read/admin/audit roles by project.
- Secret management: bind per-namespace secrets from cloud secret manager or CSI driver.

## Suggested Helm Values per Tenant

- unique service account names
- unique ingress hosts
- unique `ASTRAGRAPH_POLICY_BUNDLE_SIGNING_KEY`
- tenant-specific resource quotas and limits
