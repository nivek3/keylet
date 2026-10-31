#!/bin/bash
set -euo pipefail
umask 077
root=$(cd "$(dirname "$0")/.." && pwd)
if [[ $(uname -s) != Darwin ]]; then
  echo 'macOS is required' >&2
  exit 1
fi
export CARGO_TARGET_DIR="$root/target"
export MACOSX_DEPLOYMENT_TARGET=14.0
mkdir -p "$root/node/dist"
if [[ ${1:-} == --universal && $# == 1 ]]; then
  for target in aarch64-apple-darwin x86_64-apple-darwin; do
    cargo build --release --locked --manifest-path "$root/Cargo.toml" -p wallet-bridge --target "$target"
  done
  output=$(mktemp "$root/node/dist/.bridge.XXXXXX")
  trap 'rm -f "$output"' EXIT
  lipo -create "$root/target/aarch64-apple-darwin/release/libwallet_bridge.dylib" \
    "$root/target/x86_64-apple-darwin/release/libwallet_bridge.dylib" -output "$output"
  lipo "$output" -verify_arch arm64
  lipo "$output" -verify_arch x86_64
  codesign --force --sign - "$output"
  chmod 644 "$output"
  mv "$output" "$root/node/dist/keylet.node"
elif [[ ${1:-} == --test && $# == 1 ]]; then
  cargo build --locked --manifest-path "$root/Cargo.toml" -p wallet-bridge
  cp "$root/target/debug/libwallet_bridge.dylib" "$root/node/test/keylet-test.node"
elif [[ $# == 0 ]]; then
  cargo build --release --locked --manifest-path "$root/Cargo.toml" -p wallet-bridge
  cp "$root/target/release/libwallet_bridge.dylib" "$root/node/dist/keylet.node"
else
  echo 'Usage: build-bridge.sh [--universal|--test]' >&2
  exit 1
fi
