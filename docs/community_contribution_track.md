# Community Contribution Track

Last updated: 2026-02-16

## Scope

This track defines how community contributors add:

- new framework connectors
- reusable policy bundles
- evaluation datasets and gates

## Connector Contributions

Required for new connectors:

1. Add implementation under `connectors/<framework>/`.
2. Provide runnable example and CLI usage.
3. Add integration test coverage under `tests/integration/`.
4. Document expected blocked/allow behavior through `connectors/common/astragraph_proxy_client.py`.

## Policy Bundle Contributions

Required for policy bundles:

1. Add YAML under `policy-bundles/`.
2. Add regression pack cases under `tests/policy_regressions/`.
3. Include expected allow/block/queue behavior and rationale.

## Evaluation Contributions

Required for new eval datasets/gates:

1. Place datasets under `tests/` or `eval/` with provenance notes.
2. Add CI gate command in `.github/workflows/ci.yaml`.
3. Define thresholds and failure criteria in docs.

## Review and Acceptance

- Security-critical paths require maintainer review.
- PRs must pass Rust tests, integration tests, and configured eval gates.
- Contributions with breaking API/policy behavior need migration guidance.
