import json
from pathlib import Path

ATTACK_TYPES = [
    "tool_poisoning",
    "rug_pull",
    "privilege_escalation",
    "cross_agent_injection",
    "preference_manipulation",
]


def main() -> None:
    output_path = Path(__file__).with_name("attack_traces.jsonl")
    with output_path.open("w", encoding="utf-8") as handle:
        for idx in range(1, 201):
            attack_type = ATTACK_TYPES[(idx - 1) % len(ATTACK_TYPES)]
            record = {
                "id": f"attack-{idx:03d}",
                "attack_type": attack_type,
                "description": f"Placeholder scenario for {attack_type}.",
                "expected": "BLOCK",
            }
            handle.write(json.dumps(record) + "\n")


if __name__ == "__main__":
    main()
