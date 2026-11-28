# To-do list

Open work only. Finished items are in `CHANGELOG.md` and the git
history; the captures behind each protocol item are in
[`docs/protocol.md`](docs/protocol.md).

## Priority 1: party and stereo mode (#8)

- [ ] Twice the GUI opened its control connection less than a minute
      after the SoundLink Color II connected its audio to this computer
      (once while the stream started, once with it idle): the speaker
      did not answer the first request and locked up 20 s later. Two
      minutes after the audio connected, the GUI connected fine. Find
      out whether a delay avoids it before telling users.
- [ ] A left/right swap: `04 0b 02 01 03` is accepted but changes
      nothing. Untested: join with the other speaker as the primary.

## Priority 2: installers for more distributions

- [ ] The Debian and AUR recipes in `packaging/` have never been
      built: both install the `.desktop`, metainfo and bash-completion
      files from paths that do not exist (they live under
      `packaging/linux/`). Fix the paths and build each package once.
      Since #126 the GUI installs nothing at start, so the packages
      are the only source of the launcher entry and the themed icon.
- [ ] Make the app installable on more distributions. Nothing is
      published yet: `packaging/aur/PKGBUILD` (Arch, and derivatives
      such as CachyOS and Manjaro) is not on the AUR, and
      `packaging/debian/cargo-deb.toml` (Debian, Ubuntu) is not built
      by `release.yml`. Publish the AUR package, attach the `.deb` to
      each release, and look at Fedora (RPM), Flatpak and AppImage;
      the GUI needs GTK 4.22 and libadwaita 1.9, which older releases
      of some distributions do not ship.

## Priority 3: other features

- [ ] GUI: show a failed command (a toast, say) instead of only
      putting the widget back, and keep the value the user picked on
      screen while "Waiting for the device…" is shown (it shows the
      old value until the answer arrives).
- [ ] QC Ultra: create and edit custom audio modes (`1f 06` SET).
- [ ] QC Ultra: change the prompt language from the laptop. The CLI
      refuses it until it is tested there; the write now keeps the
      other bits of the byte (`0x80`, voice prompts).
- [ ] GUI: follow BlueZ's `PropertiesChanged` so the "Audio on this
      computer" row and the device menu update live instead of every
      60 s (or when the menu opens).
- [ ] GUI: the self-voice row always shows "Medium" and the "Visible
      for pairing" switch always starts off; read both from the
      device like the other settings.
- [ ] QC35 II (firmware 4.8.1): `--disconnect-device <phone>`
      disconnects the phone, then the headphones close the control
      connection and the CLI fails with "Connection reset by peer"
      before it can verify. Reconnect and verify instead of failing.

## Protocol questions (need hardware; batch them with the party-mode session)

- [ ] QC Ultra prompt-language byte: bit 6 was set in English (`e1`)
      but not in Spanish (`a6`), firmware 1.6.7. Find what it means
      before writing the language from the laptop.
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

## Portability

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
