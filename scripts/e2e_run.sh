#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

VERIFIER_MODE="${ASTRAGRAPH_E2E_VERIFIER_MODE:-mock}"
RUN_QUEUE_FALLBACK="${ASTRAGRAPH_E2E_QUEUE_FALLBACK:-false}"
for arg in "$@"; do
  case "${arg}" in
    --real-verifier) VERIFIER_MODE="real" ;;
    --queue-fallback) RUN_QUEUE_FALLBACK="true" ;;
    *)
      echo "Unknown argument: ${arg}" >&2
      echo "Usage: ./scripts/e2e_run.sh [--real-verifier] [--queue-fallback]" >&2
      exit 1
      ;;
  esac
done
if [[ "${VERIFIER_MODE}" != "mock" && "${VERIFIER_MODE}" != "real" ]]; then
  echo "Invalid ASTRAGRAPH_E2E_VERIFIER_MODE='${VERIFIER_MODE}'. Use 'mock' or 'real'." >&2
  exit 1
fi
echo "E2E verifier mode: ${VERIFIER_MODE}"
echo "Queue fallback scenario: ${RUN_QUEUE_FALLBACK}"

make certs

cleanup() {
  ASTRAGRAPH_E2E_VERIFIER_MODE="${VERIFIER_MODE}" docker compose -f docker-compose.yml -f docker-compose.e2e.yml down --remove-orphans
}
trap cleanup EXIT

ASTRAGRAPH_E2E_VERIFIER_MODE="${VERIFIER_MODE}" docker compose -f docker-compose.yml -f docker-compose.e2e.yml up -d \
  otel-collector graph policy verifier proxy lead-scorer proposal-writer contract-reviewer
python3 scripts/e2e_three_agent_gate.py --timeout-secs 900 --scenario standard

if [[ "${RUN_QUEUE_FALLBACK}" == "true" ]]; then
  ASTRAGRAPH_E2E_VERIFIER_MODE="${VERIFIER_MODE}" docker compose -f docker-compose.yml -f docker-compose.e2e.yml down --remove-orphans
  ASTRAGRAPH_E2E_VERIFIER_MODE="${VERIFIER_MODE}" ASTRAGRAPH_POLICY_ID=e2e-queue-policy docker compose -f docker-compose.yml -f docker-compose.e2e.yml up -d \
    otel-collector graph policy verifier proxy lead-scorer proposal-writer contract-reviewer
  docker compose -f docker-compose.yml -f docker-compose.e2e.yml stop verifier
  python3 scripts/e2e_three_agent_gate.py --timeout-secs 900 --scenario queue-fallback
fi
