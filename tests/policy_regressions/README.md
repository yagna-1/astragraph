# Policy Regression Packs

Policy regression packs are YAML files loaded by the `astragraph-policy` test
suite (`policy_regression_packs_pass` in `policy/src/evaluator.rs`).

Each pack must define:

- `name`: pack identifier
- `policy`: full policy YAML string
- `cases`: list of evaluation inputs and expected outputs

Case fields:

- `name`, `agent`, `tool`
- `args` (optional map)
- `now_utc` (optional, e.g. `10:30 UTC`)
- `expect`:
  - `decision`: `ALLOW` or `BLOCK`
  - `rule_id` (optional)
  - `threshold` (optional)
  - `fallback` (optional: `ALLOW`/`BLOCK`/`QUEUE`)
  - `require_confirmation` (optional)
