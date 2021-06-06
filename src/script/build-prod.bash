#!/usr/bin/env bash
set -ev

CURRENT_PATH=$(dirname "${0}")
cd "${CURRENT_PATH}" || exit

cd ..

rm -fR ./build

cmake \
  -S . \
  -B ./build \
  -DCMAKE_BUILD_TYPE=Release

cmake \
  --build ./build \
  --config Release \
  --parallel "$(nproc)"

# Uncomment when the create the tests
# ctest -C Release
