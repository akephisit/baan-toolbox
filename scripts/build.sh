#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
rustup target add wasm32-unknown-unknown
mkdir -p dist/app dist/processor
npx wasm-pack build crates/processor --target web --out-dir pkg --release --no-opt -- --locked
npx wasm-pack build crates/app --target web --out-dir pkg --release --no-opt -- --locked
cp -R crates/app/pkg/. dist/app/
cp -R crates/processor/pkg/. dist/processor/
cp assets/{index.html,style.css,icon.svg,processor-worker.js} dist/
node scripts/config.mjs
cp assets/_headers dist/_headers
printf '\nBuild ready: dist/\n'
