#!/bin/sh
# Builds the game to web/nightglow.wasm (the folder Strux serves, see strux.json) after running the tests.
# Needs Rust with the wasm32-unknown-unknown target: rustup target add wasm32-unknown-unknown
set -eu
cd "$(dirname "$0")"
cargo test --quiet
cargo build --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/nightglow.wasm web/nightglow.wasm
ls -l web/nightglow.wasm
