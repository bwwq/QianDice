#!/usr/bin/env bash
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
data="$root/data"
mkdir -p "$data"
if ! python3 - "$data/instance.json" <<'PY'
import json,sys,urllib.request
try:
    info=json.load(open(sys.argv[1])); urllib.request.urlopen('http://'+info['address']+'/api/v1/status',timeout=1)
except urllib.error.HTTPError as e:
    sys.exit(0 if e.code==401 else 1)
except Exception:
    sys.exit(1)
PY
then
  nohup "$root/qianbian" --root "$root" --data-dir "$data" > "$data/launcher.log" 2>&1 &
fi
for attempt in $(seq 1 100); do test -f "$data/config/admin-token.txt" && break; sleep .1; done
exec "$root/runtime/current/qianbian_ui" --data-dir "$data"
