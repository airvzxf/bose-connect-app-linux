#!/usr/bin/env bash
# Copyright (c) 2025, Israel Alberto Roldan Vega | GPL-3.0 license
set -euo pipefail

echo "=== Format ==="
cargo fmt --all

echo "=== Clippy: Fix ==="
cargo clippy --fix --allow-dirty --all-features --all-targets

echo "=== Clippy ==="
cargo clippy --all-targets --all-features -- -D warnings

#echo "=== Check ==="
#cargo check --all-targets --all-features

echo "=== Test ==="
cargo test --all-features --quiet

echo "=== Build ==="
cargo build

if [[ $# -gt 0 ]]; then
  echo "=== Run | ${*} ==="
  cargo run -- "${@}"
fi

# 2C:41:A1:02:6D:6F | Bose QC35 II 🐺
# date +'%Y-%m-%dT%H:%M:%S:%3N'
