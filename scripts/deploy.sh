#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
: "${PAGES_PROJECT:?Set PAGES_PROJECT to your Cloudflare Pages project name}"
: "${SIGNALING_URL:?Set SIGNALING_URL to the deployed Worker HTTPS origin}"
: "${ALLOWED_ORIGINS:?Set ALLOWED_ORIGINS to the exact Pages/custom-domain origins, comma-separated}"
# Set DISCOVERY_SECRET using `npx wrangler secret put DISCOVERY_SECRET` before this command.
npx wrangler deploy --var "ALLOWED_ORIGINS:$ALLOWED_ORIGINS"
bash scripts/build.sh
npx wrangler pages deploy --cwd deploy/pages --project-name "$PAGES_PROJECT" --branch main
