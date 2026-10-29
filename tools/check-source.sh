#!/usr/bin/env bash
# Lightweight source checks. Full native and desktop suites are separate release gates.
set -euo pipefail
cd "$(dirname "$0")/.."
npm ci --ignore-scripts
npm run test:smoke
find npm bin src tools test -type f -name '*.mjs' -print0 | xargs -0 -n1 node --check
npm pack --ignore-scripts --dry-run
