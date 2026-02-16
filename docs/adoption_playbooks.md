# Adoption Playbooks

Last updated: 2026-02-16

## Playbook 1: Local Security Validation (single team)

- Use `ops/profiles/dev.env`.
- Run `./scripts/e2e_run.sh` and `./scripts/e2e_run.sh --queue-fallback`.
- Validate blocked actions and audit records in graph APIs.

Exit check:

- Policy deny path and queue fallback path are both reproducible.

## Playbook 2: Staging Rollout (platform team)

- Use `ops/profiles/non-dev.env`.
- Enable signed policy bundles (`ASTRAGRAPH_POLICY_BUNDLE_SIGNING_KEY`).
- Execute staged rollout endpoints (`/policies/:name/rollout`, promote/rollback).
- Run compatibility + eval gates before rollout promotion.

Exit check:

- Rollout history and rollback behavior verified with audit evidence.

## Playbook 3: Enterprise Compliance Pipeline

- Configure SIEM evidence exports (`/audit/export`) with SOC2/ISO schema options.
- Map org/project/policy-domain boundaries from `docs/enterprise_reference_architecture.md`.
- Schedule recurring export + review workflow in operations runbooks.

Exit check:

- Evidence exports validate against schema and satisfy audit review cadence.
