# To-do list

Open work only. Finished items are in `CHANGELOG.md` and the git
history; the captures behind each protocol item are in
[`docs/protocol.md`](docs/protocol.md).

## Priority 1: the GUI as a product

- [ ] **No `--real` / `--mock`.** The app always talks to real
      devices. The simulated device stays only for tests and
      screenshots (a hidden developer flag or an environment
      variable), and the demo data must not use real addresses.
- [ ] **Empty and disconnected states.**
    - [ ] No Bose device paired with this computer: the main window
          says so and explains how to pair one.
    - [ ] Paired devices but none connected: the window shows the
          normal layout, empty, with "connect to a device".
    - [ ] The device menu (top left) lists the paired Bose devices
          from BlueZ, shows which are connected, and lets the user
          pick one. It always offers "manage devices", even when the
          list is empty.
    - [ ] No device is "locked" in: switching devices closes the old
          connection and opens the new one.
- [ ] **Languages: English by default, Spanish as the alternative.**
      Every user-visible string goes through one translation layer
      (gettext or Fluent) that follows the desktop locale, so another
      language is one new file a contributor can add. Document how to
      contribute a translation.
- [ ] The prompt language of a QC Ultra shows "English" while the
      device and the CLI say Spanish (`ES`); decode its 7-byte
      language payload in the GUI as the CLI does.

## Priority 2: party and stereo mode (#8)

- [ ] On two SoundLink Color II speakers: find the commands that
      join, leave and swap the speakers, add them to the library,
      expose them in the CLI and the GUI, and verify each one on the
      hardware. Answer #8 only when it works for end users. No Linux
      tool is known to offer this yet.

## Priority 3: other features

- [ ] QC Ultra: create and edit custom audio modes (`1f 06` SET).
- [ ] QC Ultra: change the prompt language from the laptop. The CLI
      refuses it until it is tested there; the write now keeps the
      other bits of the byte (`0x80`, voice prompts).
- [ ] GUI: follow BlueZ's `PropertiesChanged` so the "Audio en esta
      computadora" row updates live instead of every 60 s.

## Protocol questions (need hardware; batch them with the party-mode session)

- [ ] Paired-list byte `cc` (`01`, `03`): a count, or one bit per
      list position? Every capture had the connected devices first.
- [ ] QC Ultra: decode `1f 08`, `1f 02`, the audio-mode settings
      bytes and the "Source / multipoint" settings.
- [ ] QC Ultra: it sends no notifications to a second client. Find
      out whether it needs a subscription request.
- [ ] QC Ultra media keys: play/pause (`01`) never completes and
      locks every key and the phone's own controls until the phone
      reconnects. The CLI and the GUI refuse it there.
- [ ] `write_paired` treats "no reply before the receive timeout" as
      success (SoundLink Color II remove-device). Check whether a
      late reply follows; only test with a source you can pair again.

## Packaging and portability

- [ ] Debian and AUR recipes in `packaging/` have never been built:
      both install the `.desktop`, metainfo and bash-completion files
      from paths that do not exist (they live under
      `packaging/linux/`). Fix the paths and build each package once.
- [ ] Docker image for the Rust build (`libbluetooth-dev` is the
      only system dependency of the CLI).
- [ ] macOS and Windows: only `connection.rs` (the BlueZ RFCOMM
      socket) and the BlueZ audio state need a platform shim.

## Documentation

- [ ] An example under `crates/bose-connect/examples/` that drives a
      real device end to end.
- [ ] A Python example using `ctypes` against `libbose_connect.so`.

## QA

- [ ] Cargo-fuzz harnesses for `parse_hex_packet` and
      `parse_bdaddr` (the public-input parsers).
- [ ] Coverage reporting (cargo-llvm-cov) on the protocol module.
- [ ] Property tests (`proptest`) for `masked_memory_cmp`,
      `str_copy` and `unit_to_hex_string`.

## Ideas (not decisions)

- [ ] Device profiles: spike
      [`docs/ideas/device-profiles.md`](docs/ideas/device-profiles.md)
      before building anything.

## Decisions

- The GUI redraws the whole page when the device snapshot changes,
  instead of updating widgets in place or per section (2026-11-03).
  Every setter reads the device back and the page is drawn from that
  snapshot, so it never shows a stale value; per-section updates can
  miss an input (an audio mode also changes other settings) and leave
  old data on screen, as the phone app does. The cost is a visible
  redraw when the user changes a setting.
- BlueZ is not patched or worked around: when the headphones start
  the connection and BlueZ collides with them, the CLI and the GUI
  show it and suggest reconnecting from this computer.
