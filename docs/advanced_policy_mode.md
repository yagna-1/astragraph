# Advanced Policy Mode (Feature-Flagged)

Last validated: 2026-02-16 (`main`)

## Status

Advanced policy mode is available behind the stable feature flag:

- `ASTRAGRAPH_POLICY_ADVANCED_MODE=true`

When disabled, AstraGraph evaluates only legacy YAML `spec.rules`.
When enabled, AstraGraph evaluates `spec.advanced_rules` first if `spec.runtime.advanced_mode` is configured.

## Supported Engines

- `DSL`
- `OPA_COMPAT` (input-style expressions mapped to runtime evaluator)

## Policy Shape

```yaml
spec:
  runtime:
    version: v2
    advanced_mode:
      engine: OPA_COMPAT
  advanced_rules:
    - id: adv-export-block
      description: Block export in advanced mode
      expression: input.action.tool == "export_data" && input.agent.tier >= 3
      action: BLOCK
      require_confirmation: true
```

## Migration Tooling

Generate v2 advanced-rule suggestions from existing YAML:

```bash
cargo run -p astragraph-policy --bin policy_migrate -- \
  --input policies/e2e-policy.yaml \
  --engine OPA_COMPAT \
  --output /tmp/e2e-policy-v2.yaml
```

The tool preserves legacy `spec.rules` for backward compatibility and appends migrated `spec.advanced_rules`.
