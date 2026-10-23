#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo test --locked -p toolbox-core -p toolbox-signaling
cargo check --locked --workspace --target wasm32-unknown-unknown
