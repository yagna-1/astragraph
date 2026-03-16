#!/usr/bin/env bash
set -euo pipefail

if [[ "${ASTRAGRAPH_FAIL_CLOSED:-true}" != "true" ]]; then
  echo "ASTRAGRAPH_FAIL_CLOSED must stay true" >&2
  exit 1
fi

echo "pre-run checks passed"
