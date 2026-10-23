#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [ ! -f .dev.vars ]; then
  node -e "require('fs').writeFileSync('.dev.vars', 'DISCOVERY_SECRET='+require('crypto').randomBytes(32).toString('hex')+'\\n',{mode:0o600})"
fi
SIGNALING_URL=http://localhost:8787 bash scripts/build.sh
npx wrangler dev --ip 127.0.0.1 --port 8787 &
signaling_pid=$!
python3 -m http.server 8080 --bind 127.0.0.1 --directory dist &
web_pid=$!
trap 'kill "$signaling_pid" "$web_pid" 2>/dev/null || true' EXIT INT TERM
printf '\nOpen http://localhost:8080 — press Ctrl+C to stop.\n'
wait
