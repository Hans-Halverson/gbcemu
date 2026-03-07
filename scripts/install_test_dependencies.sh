#!/bin/bash

set -e

CURRENT_DIR=$(cd "$(dirname "$0")" && pwd)
ROOT_DIR=$(dirname "$CURRENT_DIR")
TEST_DEPS_DIR="$ROOT_DIR/deps/test"

mkdir -p "$TEST_DEPS_DIR"

echo "Downloading dmg-acid2 ROM..."
curl -fsSL -o "$TEST_DEPS_DIR/dmg-acid2.gb" \
    "https://github.com/mattcurrie/dmg-acid2/releases/download/v1.0/dmg-acid2.gb"

echo "Downloading dmg-acid2 DMG reference image..."
curl -fsSL -o "$TEST_DEPS_DIR/dmg-acid2-reference-dmg.png" \
    "https://raw.githubusercontent.com/mattcurrie/dmg-acid2/master/img/reference-dmg.png"

echo "Downloading cgb-acid2 ROM..."
curl -fsSL -o "$TEST_DEPS_DIR/cgb-acid2.gb" \
    "https://github.com/mattcurrie/cgb-acid2/releases/download/v1.1/cgb-acid2.gbc"

echo "Downloading cgb-acid2 reference image..."
curl -fsSL -o "$TEST_DEPS_DIR/cgb-acid2-reference.png" \
    "https://raw.githubusercontent.com/mattcurrie/cgb-acid2/master/img/reference.png"

echo "Done."
