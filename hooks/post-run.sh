#!/usr/bin/env bash
set -euo pipefail

SKILL_NAME="${1:-unknown}"
TIMESTAMP="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
SUMMARY="${2:-governance run completed}"

mkdir -p memory
cat >> memory/key-decisions.md <<EOT

## ${TIMESTAMP}  skill:${SKILL_NAME}
- ${SUMMARY}
EOT

if [[ -n "$(git status --porcelain -- memory)" ]]; then
  git add memory
  git commit -m "agent: auto-commit ${SKILL_NAME} trace at ${TIMESTAMP}" || true
fi
