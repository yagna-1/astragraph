# RULES.md - AstraGraph

## Enforced rules (AstraGraph policy: astragraph-meta)

- Policy enforcement must remain fail-closed.
- Verifier thresholds cannot be lowered without explicit approval.
- Policy bundles must be signed before activation.
- Audit records must be emitted for allow and block outcomes.

## Human review required (PR, not direct commit)

- Any change to verifier scoring thresholds.
- Changes to policy signature validation paths.
- Any update that alters fail-closed behavior.

## Auto-blocked (AstraGraph fail-closed)

- Unsigned policy bundle activation.
- Attempts to run enforcement in fail-open mode.
- Requests that bypass policy checks.
