#!/usr/bin/env bash
set -ev

CURRENT_PATH=$(dirname "${0}")
cd "${CURRENT_PATH}" || exit

cd ..

rm -fR ./build

cmake \
  -S . \
  -B ./build \
  -DCMAKE_BUILD_TYPE=Debug \
  -DAUTOFORMATTING=True \
  -DVALIDATE_QA=True

cmake \
  --build ./build \
  --config Debug \
  --parallel "$(nproc)"

# Uncomment when the create the tests
# ctest -C Release
