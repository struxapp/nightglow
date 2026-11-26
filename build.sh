#!/bin/sh
# Checks and builds the game: lint (Rust and the page), tests, then web/nightglow.wasm, the module the page loads.
# web/ is the folder that is served (see strux.json).
# Needs Rust with the wasm32-unknown-unknown target, clippy and rustfmt:
#   rustup target add wasm32-unknown-unknown && rustup component add clippy rustfmt
# and Node.js (npx) for oxlint, the page's linter. No package.json or node_modules needed.
# ./build.sh lint   only runs the linters.
set -eu
cd "$(dirname "$0")"

cargo fmt --check
cargo clippy --all-targets -- -D warnings
npx --yes oxlint@1.95.0 --deny-warnings web
if [ "${1:-}" = "lint" ]; then
  exit 0
fi

cargo test --quiet
cargo build --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/nightglow.wasm web/nightglow.wasm
ls -l web/nightglow.wasm
