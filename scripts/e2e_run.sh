#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

VERIFIER_MODE="${ASTRAGRAPH_E2E_VERIFIER_MODE:-mock}"
if [[ "${1:-}" == "--real-verifier" ]]; then
  VERIFIER_MODE="real"
fi
if [[ "${VERIFIER_MODE}" != "mock" && "${VERIFIER_MODE}" != "real" ]]; then
  echo "Invalid ASTRAGRAPH_E2E_VERIFIER_MODE='${VERIFIER_MODE}'. Use 'mock' or 'real'." >&2
  exit 1
fi
echo "E2E verifier mode: ${VERIFIER_MODE}"

make certs

ASTRAGRAPH_E2E_VERIFIER_MODE="${VERIFIER_MODE}" docker compose -f docker-compose.yml -f docker-compose.e2e.yml up -d \
  otel-collector graph policy verifier proxy lead-scorer proposal-writer contract-reviewer

python3 scripts/e2e_three_agent_gate.py --timeout-secs 900

ASTRAGRAPH_E2E_VERIFIER_MODE="${VERIFIER_MODE}" docker compose -f docker-compose.yml -f docker-compose.e2e.yml down --remove-orphans
