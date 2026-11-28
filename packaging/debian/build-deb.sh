#!/usr/bin/env bash
# Builds the Debian / Ubuntu package into target/debian/.
#
# Needs Rust and the development files of GTK 4.22, libadwaita 1.9
# and BlueZ. On Ubuntu 26.04:
#   sudo apt install build-essential pkg-config dpkg-dev \
#       libgtk-4-dev libadwaita-1-dev libbluetooth-dev
# CI and the release workflow run this script in an ubuntu:26.04
# container.
set -euo pipefail

CARGO_DEB_VERSION=3.8.0

cd "$(dirname "$0")/../.."

if ! cargo deb --version 2>/dev/null | grep -qF "${CARGO_DEB_VERSION}"; then
    cargo install --locked "cargo-deb@${CARGO_DEB_VERSION}"
fi

cargo build --release --locked -p bose-connect-gui -p bose-connect-cli
cargo deb -p bose-connect-gui --no-build
ls -l target/debian/*.deb
