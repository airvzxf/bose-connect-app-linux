#!/usr/bin/env bash
# Copyright (c) 2025, Israel Alberto Roldan Vega | GPL-3.0 license
set -euo pipefail

cargo fmt --all && cargo check && cargo clippy && cargo build #&& cargo run -- --address 2C:41:A1:02:6D:6F get-battery

# date +'%Y-%m-%dT%H:%M:%S:%3N'

# 2C:41:A1:02:6D:6F | Bose QC35 II 🐺
# cargo run -- --address 2C:41:A1:02:6D:6F discover
