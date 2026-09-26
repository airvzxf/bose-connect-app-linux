#!/usr/bin/env bash
# Grant the Bluetooth capability (CAP_NET_RAW) needed to open RFCOMM/L2CAP
# sockets on modern Linux kernels. Must be run with sudo after every build.
#
# Usage:
#   sudo ./script/grant-bluetooth-caps.bash
#
# This is required because `setcap` binds the capability to the file inode,
# so every rebuild of the binary drops it.

set -e

PROJECT_ROOT="$(cd "$(dirname "${0}")/.." && pwd)"
FORK_BIN="${PROJECT_ROOT}/build/bose-connect-app-linux"
ORIG_BIN="/home/wolf/workspace/projects/based-connect/based-connect"

if [[ "${EUID}" -ne 0 ]]; then
  echo "Please run with sudo: sudo $0"
  exit 1
fi

if [[ -f "${FORK_BIN}" ]]; then
  setcap cap_net_raw+ep "${FORK_BIN}"
  echo "OK: cap_net_raw+ep applied to ${FORK_BIN}"
  getcap "${FORK_BIN}"
else
  echo "WARN: ${FORK_BIN} not found (skipping fork binary)"
fi

if [[ -f "${ORIG_BIN}" ]]; then
  setcap cap_net_raw+ep "${ORIG_BIN}"
  echo "OK: cap_net_raw+ep applied to ${ORIG_BIN}"
  getcap "${ORIG_BIN}"
else
  echo "WARN: ${ORIG_BIN} not found (skipping original binary)"
fi
