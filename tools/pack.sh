#!/bin/sh
# Builds the add-on as the one file the mod manager installs:
# dist/bodyworks-<version>.apocaddon, a zip of the interface and the module.
set -eu
cd "$(dirname "$0")/.."
cargo build --release -p apoc-bodyslide-app --target wasm32-unknown-unknown
version=$(sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' ui/addon.json)
rm -rf dist/pack && mkdir -p dist/pack
cp ui/* dist/pack/
cp target/wasm32-unknown-unknown/release/apoc_bodyslide_app.wasm dist/pack/bodyworks.wasm
cp LICENSE dist/pack/LICENSE
out="dist/bodyworks-$version.apocaddon"
rm -f "$out"
(cd dist/pack && zip -q -9 -X "../../$out" *)
rm -rf dist/pack
ls -l "$out"
sha256sum "$out"
