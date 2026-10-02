#!/usr/bin/env bash
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
data="$root/data"
if (( $# )); then
  if [[ $# != 2 || $1 != --data-dir ]]; then echo '用法：qianbian-ui [--data-dir 目录]' >&2; exit 2; fi
  data="$2"
fi
mkdir -p "$data"
data="$(cd -- "$data" && pwd)"
if ! endpoint="$("$root/qianbian" --root "$root" --data-dir "$data" ready 2>/dev/null)"; then
  nohup "$root/qianbian" --root "$root" --data-dir "$data" >> "$data/launcher.log" 2>&1 &
  backend=$!
  ready=false
  for attempt in $(seq 1 100); do
    if endpoint="$("$root/qianbian" --root "$root" --data-dir "$data" ready 2>/dev/null)"; then ready=true; break; fi
    if ! kill -0 "$backend" 2>/dev/null; then break; fi
    sleep .1
  done
  if [[ $ready != true ]]; then echo "千变后台未能启动，请查看 $data/launcher.log" >&2; exit 1; fi
fi
exec "$root/runtime/current/qianbian_ui" --data-dir "$data" --endpoint "$endpoint"
