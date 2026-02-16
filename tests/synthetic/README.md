Synthetic attack trace suite (250 scenarios).

- `attack_traces.jsonl` includes mixed malicious + benign scenarios.
- Records include:
  - `attack_type`
  - `tool_name`
  - `arguments`
  - `should_block`
  - `expected` (`BLOCK`/`ALLOW`)
- `generate_traces.py` regenerates the dataset deterministically.
- `eval/synthetic_attack_eval.py` evaluates proxy block/allow behavior and reports recall/FNR/FAR.
