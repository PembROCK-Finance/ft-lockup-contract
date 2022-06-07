#!/bin/bash
TARGET="${CARGO_TARGET_DIR:-target}"
set -e
pushd "$(dirname $0)"

# Removing rlib for contract building
perl -i -pe 's/\["cdylib", "rlib"\]/\["cdylib"\]/' Cargo.toml

RUSTFLAGS="--remap-path-prefix=$HOME=\$HOME" cargo build --target wasm32-unknown-unknown --release
mkdir -p ./res
cp $TARGET/wasm32-unknown-unknown/release/*.wasm ./res/

# Restoring rlib for tests
perl -i -pe 's/\["cdylib"\]/\["cdylib", "rlib"\]/' Cargo.toml
popd
