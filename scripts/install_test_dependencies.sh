#!/bin/bash

set -e

CURRENT_DIR=$(cd "$(dirname "$0")" && pwd)
ROOT_DIR=$(dirname "$CURRENT_DIR")
TEST_DEPS_DIR="$ROOT_DIR/deps/test"

mkdir -p "$TEST_DEPS_DIR"

GAMEBOY_TEST_ROMS_DIR="$TEST_DEPS_DIR/game-boy-test-roms"
GAMEBOY_TEST_ROMS_ZIP="$TEST_DEPS_DIR/game-boy-test-roms.zip"

if [ ! -d "$GAMEBOY_TEST_ROMS_DIR" ]; then
    echo "Downloading game-boy-test-roms..."
    curl -fsSL -o "$GAMEBOY_TEST_ROMS_ZIP" "https://github.com/c-sp/game-boy-test-roms/releases/download/v7.0/game-boy-test-roms-v7.0.zip"

    echo "Unzipping game-boy-test-roms..."
    unzip -q "$GAMEBOY_TEST_ROMS_ZIP" -d "$GAMEBOY_TEST_ROMS_DIR"
else
    echo "game-boy-test-roms already present, skipping."
fi

rm -f "$GAMEBOY_TEST_ROMS_ZIP"

echo "Done."
