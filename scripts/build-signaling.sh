#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if ! command -v worker-build >/dev/null 2>&1; then cargo install worker-build --version 0.8.7 --locked; fi
cd crates/signaling
worker-build --release --no-opt --no-panic-recovery
