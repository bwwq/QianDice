#!/usr/bin/env bash
set -euo pipefail
cd web
if [[ -f package-lock.json ]]; then npm ci --ignore-scripts; else npm install --package-lock-only --ignore-scripts && npm ci --ignore-scripts; fi
npm run build
cd ..
cp -a web/dist/. assets/web/
