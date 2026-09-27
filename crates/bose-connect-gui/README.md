# `bose-connect-gui`

A GTK4 + libadwaita + Relm4 desktop front-end for the
[`bose-connect`](../bose-connect) RFCOMM library. Lets you drive a
Bose QuietComfort / SoundLink headphone from a modern Linux desktop
without vendor-supplied tooling — same byte-level protocol, modern UX.

```
crates/bose-connect-gui
├── src
│   ├── lib.rs          public crate surface
│   ├── main.rs         binary entry point + CLI flags + screenshot dump
│   ├── app
│   │   ├── model.rs    AppModel (single source of truth)
│   │   ├── view.rs     widget tree assembly
│   │   ├── widgets.rs  pure functions that paint AdwToolbarView
│   │   └── component.rs Relm4 Component scaffold (no Widget relm4 lifecycle yet)
│   ├── services
│   │   ├── device.rs   DeviceService trait + MockService + RealService stub
│   │   ├── state.rs    PersistedState + 4 Profiles (Focus/Travel/Home/Quiet)
│   │   ├── tray.rs     KStatusNotifierItem scaffold
│   │   ├── notifications.rs  notify-rust → org.freedesktop.Notifications
│   │   ├── media_player.rs   MPRIS state holder
│   │   └── bluetooth.rs     BlueZ D-Bus scaffold
│   ├── i18n            label helpers
│   └── transport.rs    in-process BoseIo simulator with battery-drain tick
└── resources
    ├── style.css        hero-card, tiles, sparkline, quiet-mode
    ├── icons/…          symbolic + full-color app icon (SVG)
    └── menus/app-menu.ui GMenu XML for the header bar
```

## Building

```bash
cargo build --workspace --all-targets          # entire workspace
cargo run -p bose-connect-gui                   # interactive launch
cargo run -p bose-connect-gui -- --headless      # smoke: writes
                                               # /tmp/bose-connect-gui.smoke
cargo run -p bose-connect-gui -- --dump-tree    # 297-line widget tree dump
```

## Smoke testing on this host

```bash
# 1) Run in headless mode and check the marker.
RUST_LOG=info $(cargo metadata --no-deps --format-value -q | head -1)
cargo build -p bose-connect-gui --bin bose-connect-gui
RUST_LOG=info target/debug/bose-connect-gui --headless --run-secs 7 \
    --mock-tick-ms 250 --low-battery-test
# → /tmp/bose-connect-gui.smoke = "ok"

# 2) Watch the libnotify traffic while the app runs.
dbus-monitor --session \
    "interface='org.freedesktop.Notifications',member='Notify'"

# 3) Verify registration on the D-Bus session bus.
gdbus call --session --dest org.freedesktop.DBus --object-path / \
    --method org.freedesktop.DBus.ListNames | grep airvzxf
```

## CLI flags

| Flag | Effect |
| --- | --- |
| `--real` / `--real-bluetooth` | Use the real RFCOMM transport against the wired Bose device |
| `--mock` | (default) In-process mock device |
| `--headless` | Render once and exit; writes `/tmp/bose-connect-gui.smoke` |
| `--run-secs SECONDS` | Keep the binary alive long enough to observe a timeout |
| `--mock-tick-ms MS` | Battery-drain interval for the mock (default `4000`) |
| `--low-battery-test` | Seed the mock at 10 % so the libnotify threshold fires within seconds |
| `--screenshot PATH` | Write the first frame as PNG to `PATH` (off-screen `gdk::Texture` snapshot) |
| `--dump-tree` | Print the GTK widget tree, one widget per line, to stdout |

## KDE Plasma integration

* `org.freedesktop.Notifications` via `notify-rust` — every battery
  threshold crossing emits a real `Notify` call captured by
  `dbus-monitor` (verified end-to-end on this host).
* `com.airvzxf.bose-connect-gui` registered on the D-Bus session
  bus as a `gtk::Application` (verified with `gdbus call … ListNames`).
* Custom icons compiled into the binary via `glib-build-tools`
  (hicolor + symbolic SVG).
* CSS injected at startup from the GResource bundle, so the desktop
  theme (`Breeze` here) renders the hero card / tiles / sparkline
  without the user having to restart the session.
* `~/.config/bose-connect/state.json` is persisted (last address +
  active profile) so the window comes back to the same device.

## Deferred to a follow-up PR

* `ksni`-backed KDE Plasma tray icon: the `services::tray`
  scaffolding is in place; the actual `ksni::TrayService::spawn`
  wiring is gated on being able to keep `gtk4-sys` pinned across
  the workspace.
* `MediaPlayer2` D-Bus object via `zbus_macros`.
* `BlueZ` device-discovery scan via `org.freedesktop.DBus.ObjectManager`.
* `RealService::connect` — the public surface is laid out; the
  RFCOMM open + `init_connection` round-trip is deliberately
  stubbed so the GUI builds without a real RFCOMM socket.

The protocol module `bose_connect::protocol::*` is untouched, so the
on-wire fidelity of the wired transport stays byte-for-byte identical
to the original C / Rust port.
