#!/usr/bin/env bash
set -euo pipefail

corepack enable >/dev/null 2>&1 || true

if [ "{{yarn.version}}" = "latest" ]; then
  exit 0
fi

corepack prepare "yarn@{{yarn.version}}" --activate

