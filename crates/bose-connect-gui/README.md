# `bose-connect-gui`

A GTK4 + libadwaita desktop front-end for the
[`bose-connect`](../bose-connect) RFCOMM library. Lets you drive a
Bose QuietComfort / SoundLink headphone from a modern Linux desktop
without vendor-supplied tooling — same byte-level protocol, modern UX.

```text
crates/bose-connect-gui
├── src
│   ├── lib.rs          public crate surface
│   ├── main.rs         binary entry point + CLI flags + screenshot dump
│   ├── app
│   │   ├── model.rs    AppModel (single source of truth)
│   │   ├── view.rs     widget tree assembly
│   │   └── widgets.rs  pure functions that paint AdwToolbarView
│   ├── services
│   │   ├── device.rs        DeviceService trait + MockService + RealService stub
│   │   ├── state.rs         PersistedState + 4 Profiles (Focus/Travel/Home/Quiet)
│   │   ├── tray.rs          Hand-rolled StatusNotifierItem via gio::DBus
│   │   ├── bluetooth.rs     Hand-rolled BlueZ discovery via gio::DBus
│   │   ├── notifications.rs notify-rust → org.freedesktop.Notifications
│   │   └── media_player.rs  MPRIS state holder
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
cargo run -p bose-connect-gui -- --dump-tree    # widget tree dump

# Release build (stripped, ready for distribution).
cargo build --workspace --release --locked
# → target/release/bose-connect-gui  (3.6 MB, stripped)
# → target/release/bose-connect-app-linux  (868 KB, the CLI)
# → target/release/libbose_connect.so  (380 KB, the C ABI)
```

## Smoke testing on this host

```bash
# 1) Run in headless mode and check the marker.
cargo build -p bose-connect-gui --bin bose-connect-gui
RUST_LOG=info target/debug/bose-connect-gui --headless --run-secs 7 \
    --mock-tick-ms 250 --low-battery-test
# → /tmp/bose-connect-gui.smoke = "ok"

# 2) Watch the libnotify traffic while the app runs.
dbus-monitor --session \
    "interface='org.freedesktop.Notifications',member='Notify'"

# 3) Verify the StatusNotifierItem registered with the KDE
#    Plasma watcher on the session bus.
dbus-monitor --session \
    "interface='org.kde.StatusNotifierWatcher'" 2>&1 | tee /tmp/sni.log
# → expect a `Register` call on the watcher

# 4) Verify registration of our application on the D-Bus session bus.
gdbus call --session --dest org.freedesktop.DBus --object-path / \
    --method org.freedesktop.DBus.ListNames | grep airvzxf
```

## CLI flags

| Flag | Effect |
| --- | --- |
| `--real` / `--real-bluetooth` | Use the real RFCOMM transport |
| `--mock` | (default) In-process mock device |
| `--headless` | Render once and exit; writes `/tmp/bose-connect-gui.smoke` |
| `--run-secs SECONDS` | Exit cleanly after `SECONDS` |
| `--mock-tick-ms MS` | Battery-drain interval for the mock (default `4000`) |
| `--low-battery-test` | Seed the mock at 10 % to fire a low-battery alert |
| `--screenshot PATH` | Save the first frame to `PATH` (headless) |
| `--dump-tree` | Print the GTK widget tree, one widget per line, to stdout |

## KDE Plasma integration

* `org.freedesktop.Notifications` via `notify-rust` — every battery
  threshold crossing emits a real `Notify` call captured by
  `dbus-monitor` (verified end-to-end on this host).
* `com.airvzxf.bose-connect-gui` registered on the D-Bus session
  bus as a `gtk::Application` (verified with `gdbus call … ListNames`).
* `org.kde.StatusNotifierItem-<pid>-1` registered as the KDE
  Plasma tray icon, hand-rolled on `gio::DBus`. The watcher
  picks the icon up after a real `Register` call captured by
  `dbus-monitor --session interface='org.kde.StatusNotifierWatcher'`.
* BlueZ (`org.bluez` over the **system** bus) is queried once at
  startup via `GetManagedObjects`, plus live updates from
  `InterfacesAdded` / `InterfacesRemoved`. The Bose Connect
  service UUID `0000fddd-0000-1000-8000-00805f9b34fb` is used
  to flag Bose candidates in the discover list.
* Custom icons compiled into the binary via `glib-build-tools`
  (hicolor + symbolic SVG).
* CSS injected at startup from the GResource bundle, so the desktop
  theme (`Breeze` here) renders the hero card / tiles / sparkline
  without the user having to restart the session.
* `~/.config/bose-connect/state.json` is persisted (last address +
  active profile) so the window comes back to the same device.

## Deferred to a follow-up PR

* `RealService::connect` — the public surface is laid out; the
  RFCOMM open + `init_connection` round-trip is deliberately
  stubbed so the GUI builds without a real RFCOMM socket.
* An active Bluetooth scan must be triggered once after
  `StartDiscovery` is called; today the discovery list
  populates from `GetManagedObjects` at startup, but a
  user-driven "Scan" button requires the GTK button to be
  wired into `BluetoothDiscovery::start_scan()`.

The protocol module `bose_connect::protocol::*` is untouched, so the
on-wire fidelity of the wired transport stays byte-for-byte identical
to the original C / Rust port.
