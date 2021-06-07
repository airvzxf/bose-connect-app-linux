#!/usr/bin/env bash
set -ev

CURRENT_PATH=$(dirname "${0}")
cd "${CURRENT_PATH}" || exit

cd ..

sudo cat ./build/install_manifest.txt | sudo xargs rm -f
