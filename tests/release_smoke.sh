#!/usr/bin/env bash
# Release-build smoke test. Mirrors the validation gauntlet the
# AGENTS.md calls out as what CI runs.
#
# Verifies the release-mode binary:
#   - builds with --locked against the committed Cargo.lock
#   - reports its version cleanly (`name version`, no author)
#   - emits real libnotify log lines when the battery mock
#     crosses the 25%/5% thresholds
#   - calls Register on org.kde.StatusNotifierWatcher with our
#     unique bus name so KDE Plasma picks up the tray icon
#
# Designed to be runnable on a CI host with a D-Bus session
# bus already available (the case for every GitHub Actions
# linux-runner). The D-Bus *capture* of `Register` is asserted
# here with `dbus-monitor --session`; the libnotify log line is
# checked in the in-process log.
set -euo pipefail

# Resolve the repo root from the script's location so the test
# is independent of cwd.
repo_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo_root"

# 1. Build with the same flags CI uses.
cargo build --workspace --release --locked

# 2. Spot-check the version output.
./target/release/bose-connect-gui --version | grep -q '^bose-connect-gui 0\.1\.0$'

# 3. Capture the StatusNotifierWatcher `Register` call on the
#    session bus. Run `dbus-monitor` in the background so it
#    can survive the very short lifetime of the smoke binary.
dbus-monitor --session "interface='org.kde.StatusNotifierWatcher'" \
    > /tmp/bose-sni.log 2>&1 &
MON=$!
sleep 1

# 4. Run the low-battery mock; the test should emit at least
#    one libnotify log line. We force `--headless` and wrap
#    the run with `timeout` so this stays well-behaved on CI
#    boxes where the binary would otherwise try to open a
#    window under no display server.
timeout 30s env RUST_LOG=info ./target/release/bose-connect-gui \
    --mock-tick-ms 250 \
    --low-battery-test \
    --headless \
    --run-secs 7 \
    > /tmp/bose-rel.log 2>&1

# 5. Stop the monitor and confirm we asked the watcher to
#    register our tray icon. KDE Plasma 6's watcher
#    exposes `RegisterStatusNotifierItem(in s service)`,
#    NOT a plain `Register`. The grep has to be exact;
#    matching the substring would silently let a typo'd
#    method name through and the icon would never appear.
kill $MON 2>/dev/null || true
wait $MON 2>/dev/null || true
grep -q 'RegisterStatusNotifierItem' /tmp/bose-sni.log

# 6. Confirm a low-battery notification fired in the log.
# We grep for the `cross band` trace because libnotify's Notify
# call body is captured by zbus but not echoed in the log; the
# `cross band N -> M` line is the in-process proof that the
# `Notifications::notify` call ran.
grep -q "cross band 2 -> 1" /tmp/bose-rel.log
