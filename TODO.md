# To-do list

Open work only. Finished items are in `CHANGELOG.md` and the git
history; the captures behind each protocol item are in
[`docs/protocol.md`](docs/protocol.md).

## Priority 1: party and stereo mode (#8)

- [ ] GUI: join, leave and party/stereo for two SoundLink Color II
      (the library and the CLI have them). With this computer as the
      source: pause the audio and count down a minute before joining,
      restart the audio stream after every mode change, and warn that
      the speakers can lock up. Answer #8 only when this is released.
- [ ] Make PipeWire use 44.1 kHz for a SoundLink Color II when the
      computer opens the audio link (it picks 48 kHz, which locked up
      the speakers in party mode). The `--audio-state` reconnect hint
      leads to exactly that link.
- [ ] A left/right swap: `04 0b 02 01 03` is accepted but changes
      nothing. Untested: join with the other speaker as the primary.

## Priority 2: other features

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
