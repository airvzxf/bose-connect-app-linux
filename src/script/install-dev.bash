#!/usr/bin/env bash
set -ev

CURRENT_PATH=$(dirname "${0}")
cd "${CURRENT_PATH}" || exit

./build-dev.bash

cd ..

sudo cmake --install ./build --config Debug
