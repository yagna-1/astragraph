#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

make certs

docker compose -f docker-compose.yml -f docker-compose.e2e.yml up -d \
  otel-collector graph policy verifier proxy lead-scorer proposal-writer contract-reviewer

python3 scripts/e2e_three_agent_gate.py --timeout-secs 900

docker compose -f docker-compose.yml -f docker-compose.e2e.yml down --remove-orphans

