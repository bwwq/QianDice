#!/usr/bin/env bash
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
data="$root/data"
mkdir -p "$data"
nohup "$root/qianbian" --root "$root" --data-dir "$data" >> "$data/launcher.log" 2>&1 &
for attempt in $(seq 1 100); do test -f "$data/config/admin-token.txt" && break; sleep .1; done
endpoint="$("$root/qianbian" --root "$root" --data-dir "$data" endpoint)"
exec "$root/runtime/current/qianbian_ui" --data-dir "$data" --endpoint "$endpoint"
