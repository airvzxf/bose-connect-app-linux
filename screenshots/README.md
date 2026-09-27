# Screenshots

Visual artefacts of the Bose Connect for Linux GUI build.

## `bose-connect-mockup.png`

A faithful PNG rendering of the wireframe the engineering team
agreed on before writing any widget code. Hand-drawn with
`magick` so the layout is reproducible without running the binary
or having a Bluetooth headset on hand. Used as the canonical
visual reference inside the team.

## `bose-connect-canvas.png` and `bose-connect-canvas.ppm`

The actual `cairo::ImageSurface` produced by `--headless
--screenshot /tmp/bose-screenshot.ppm`. The PPM is the raw output
from the binary; the PNG is the converted version (ImageMagick
`magick PPM:… PNG24:…`). The file is mostly the libadwaita
canvas colour (`#EEF0F5`, Breeze background) because in headless
mode the widget tree never sees a real layout pass — but the
file is a valid 1180×760 PPM/PNG and proves that the
`cairo::ImageSurface → premultiplied ARGB32 → un-premultiply
→ PPM P6` pipeline produces the right pixels.

For a real painted frame, run the binary in interactive mode
under a Wayland or X11 session, then capture the result with
`grim` (Wayland) or `scrot` (X11). The PPM/PNG here is a
contract test, not a marketing screenshot.

## Live tray icon and BlueZ discovery surface

The PPM/PNG artefacts above only paint the widget tree. In a
real KDE Plasma 6 session the binary additionally:

- registers a `StatusNotifierItem-<pid>-1` with
  `org.kde.StatusNotifierWatcher` (hand-rolled on `gio::DBus`,
  ~250 lines in `services/tray.rs`). Clicking the icon calls
  `Activate(int, int)` which the GUI forwards into
  `AppMsg::TrayEvent(Activate)`.
- walks the `org.bluez` ObjectManager at startup
  (`GetManagedObjects`) and subscribes to `InterfacesAdded` /
  `InterfacesRemoved` for live updates. The Bose Connect
  service UUID `0000fddd-0000-1000-8000-00805f9b34fb` flags
  Bose candidates in the discover list.

To capture the SNI Register call into a `dbus-monitor` log,
run the binary under a session bus that has the watcher
running:

```bash
dbus-monitor --session \
    "interface='org.kde.StatusNotifierWatcher'" 2>&1 | tee /tmp/sni.log
```

## Regenerating

```bash
# 1. Run the headless smoke test, which writes the PPM.
RUST_LOG=info target/debug/bose-connect-gui \
    --headless --run-secs 3 \
    --screenshot screenshots/bose-connect-canvas.ppm

# 2. Convert the PPM to a real PNG.
magick screenshots/bose-connect-canvas.ppm \
    -depth 8 PNG24:screenshots/bose-connect-canvas.png

# 3. The mockup is checked in so the wireframe stays close to
#    the implementation. Touch it only when the implementation
#    actually changes.
```
