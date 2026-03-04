#!/bin/bash
set -euo pipefail

BUNDLE_NAME="VolumeHUD"
APP_DIR="target/${BUNDLE_NAME}.app/Contents"

cargo build --release -p cli-music-hud

mkdir -p "${APP_DIR}/MacOS"
cp target/release/cli-music-hud "${APP_DIR}/MacOS/"
cp cli-music-hud/resources/Info.plist "${APP_DIR}/"

echo "Built ${BUNDLE_NAME}.app at target/${BUNDLE_NAME}.app"
echo "To install: cp -r target/${BUNDLE_NAME}.app /Applications/"
