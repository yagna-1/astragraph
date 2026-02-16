# Repository Structure and Naming

This guide clarifies top-level naming to reduce contributor confusion.

## Similar Names, Different Roles

- `policy/`: Rust policy engine source code (workspace crate), includes REST `/policies/*` handlers and evaluators.
- `policy-bundles/`: sample policy YAML bundles used by quickstart, simulation, and examples.
- `verifier/`: verifier reference implementation and scoring modules.
- `data/`: local runtime artifacts produced by services (for local/dev/e2e), including:
  - `data/policies/history.jsonl`
  - `data/graphs/violations.jsonl`

## Runtime vs Example Assets

- Runtime services read/write state under `data/`.
- Example/static policy files live under `policy-bundles/`.
- API route names remain `/policies/*` because they refer to the policy service domain, not filesystem folder names.

## Contributor Rule of Thumb

- Changing enforcement logic: start in `proxy/` and `policy/`.
- Changing policy examples/docs: use `policy-bundles/`.
- Changing verifier model behavior: use `verifier/` and verify with eval gates in `eval/`.
