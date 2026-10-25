#!/bin/bash
set -euo pipefail
umask 077

if [[ $# != 3 && ( $# != 4 || "$4" != '--universal' ) ]]; then
  echo 'Usage: bash scripts/build-native.sh <signing identity> <profile path> <bundle id> [--universal]' >&2
  exit 1
fi
identity=$1
profile=$2
bundle_id=$3
if [[ "$identity" == '-' || ! -f "$profile" ]]; then
  echo 'A real signing identity and provisioning profile are required.' >&2
  exit 1
fi
root=$(cd "$(dirname "$0")/.." && pwd)
version=$(cargo metadata --no-deps --format-version 1 --locked --manifest-path "$root/Cargo.toml" |
  /usr/bin/python3 -c 'import json, sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "wallet-app"))')
mkdir -p "$root/build"
staging=$(mktemp -d "$root/build/.Keylet.XXXXXXXX")
app="$staging/Keylet.app"
mkdir -p "$app/Contents/MacOS"
# The decoded profile contains signing metadata, never wallet secrets.
/usr/bin/security cms -D -i "$profile" -o "${app%/*}/profile.plist"
/usr/bin/python3 "$root/scripts/prepare-bundle.py" "${app%/*}/profile.plist" "$bundle_id" "$app" "$version"
cp "$profile" "$app/Contents/embedded.provisionprofile"

export CARGO_TARGET_DIR="$root/target"
export MACOSX_DEPLOYMENT_TARGET=14.0
if [[ "${4:-}" == '--universal' ]]; then
  for target in aarch64-apple-darwin x86_64-apple-darwin; do
    cargo build --release --locked --manifest-path "$root/Cargo.toml" -p wallet-app --target "$target"
  done
  lipo -create "$root/target/aarch64-apple-darwin/release/keylet" \
    "$root/target/x86_64-apple-darwin/release/keylet" \
    -output "$app/Contents/MacOS/keylet"
  lipo -verify_arch arm64 x86_64 "$app/Contents/MacOS/keylet"
else
  cargo build --release --locked --manifest-path "$root/Cargo.toml" -p wallet-app
  cp "$root/target/release/keylet" "$app/Contents/MacOS/keylet"
fi

/usr/bin/codesign --force --sign "$identity" --options runtime --timestamp \
  --entitlements "${app%/*}/entitlements.plist" "$app"
/usr/bin/codesign --verify --strict --verbose=2 "$app"
output="$root/build/Keylet.app"
if [[ -e "$output" ]]; then
  mv "$output" "$staging/previous.app"
fi
if ! mv "$app" "$output"; then
  if [[ -e "$staging/previous.app" ]]; then
    mv "$staging/previous.app" "$output"
  fi
  exit 1
fi
rm -rf "$staging"
echo "Built app: $output"
