#!/usr/bin/env bash
set -eo pipefail

mkdir -p res
RUSTFLAGS='-C link-arg=-s' cargo build --target=wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/debug/veax_dex.wasm res/
