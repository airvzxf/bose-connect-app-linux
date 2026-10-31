# Changelog

All notable changes to this project are documented here. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased][]

Based on a hardware session with a QC45 (firmware 4.0.4) on
2026-10-31; the captures are in `docs/protocol.md` and replayed by the
tests.

### Added

* Audio modes and "remember my mode" on the QC45 (device id
  `0x4039`): `has_audio_modes` is true for it, so `--audio-mode`,
  `--remember-mode` and the audio mode in `--device-status` work there.
* Per-feature predicates `has_equalizer`, `has_battery_prompt`,
  `has_immersive_audio`, `has_auto_play_pause`, `has_auto_answer` and
  `has_shortcut`, true for the QC Ultra only so far. `--device-status`
  prints each setting only on the devices that have it.

### Fixed

* `set_audio_mode` accepts a PROCESSING reply without a RESULT (QC45)
  and reads the new mode back with a GET.
* `get_audio_modes` stops at the first slot that answers ERROR `06` or
  `08` instead of failing; the QC45 has four slots, not ten.

## [0.3.0][] - 2026-10-27

Based on hardware sessions with a SoundLink Color II (firmware 4.0.1),
a QC35 II (4.8.1) and a QC Ultra (1.6.7) on 2026-10-07/08; the
captures are in `docs/protocol.md` and replayed by the tests.

### Added

* QC Ultra settings, each read and written from a laptop on hardware:
  equalizer (`get_equalizer`, `set_equalizer_band`), immersive audio,
  battery-level prompt at power-on, automatic play/pause, automatic
  call answering, "remember my mode" and the earcup shortcut. CLI:
  `--equalizer`, `--immersive-audio`, `--battery-prompt`,
  `--auto-play-pause`, `--auto-answer`, `--remember-mode`,
  `--shortcut`; `--device-status` prints them.
* Auto-off on the QC Ultra (3-byte layout), including
  `AutoOff::Min1440` (24 hours); `--auto-off` and `--voice-prompts`
  work there.
* RFCOMM channel 1 as a second fallback (8, then 2, then 1): the QC
  Ultra has two control channels and the phone's Bose app holds one.
* `BoseIo::bose_send` and `BoseIo::bose_discard_pending`.

### Fixed

* Unsolicited packets (a second volume reply, active-source changes,
  track metadata, late results) no longer answer the next request on
  the same connection: stale input is dropped before each request and
  packets of other functions are skipped.
* Every reply, the C acknowledgements included, is read from its
  header; ERROR packets are `BoseError::DeviceError` everywhere.
* `connect_device`, `disconnect_device` and `remove_device` wait for
  the final RESULT (up to 15 s) instead of taking the first PROCESSING
  packet, or an ERROR, as success. The SoundLink Color II reply
  `04 01 04 07 0b` is an error ("not a paired source").
* The CLI no longer retries a paired-device operation after a
  `DeviceError`.
* `set_self_voice` no longer sends the stray trailing `38` of the C
  packet, which the device read as the start of the next request
  (verified on a QC Ultra and a QC35 II).
* `send_media_key` waits up to 5 s for the RESULT, which the QC Ultra
  sends 1-3 s after PROCESSING; `get_device_status` no longer fails
  when a setting outside the report comes as an ERROR packet.

### Changed

* `--send-media-key pause` is refused on the QC Ultra: it never
  completes there and locks media control until the phone reconnects.
* `DeviceStatusReport.minutes` is decoded on the QC Ultra.
* The protocol notes moved from `DEVELOPMENT.md` to `docs/protocol.md`,
  organised by command, with the device and firmware of each capture
  and the Rust call that sends it.

### Breaking changes (library API)

* `AutoOff` gains `Min1440`; exhaustive matches need a new arm.
* `BoseIo` gains the required method `bose_discard_pending` (the trait
  is sealed, so only this crate implements it).

## [0.2.0][] - 2026-10-05

### Added

* Bose QuietComfort (QC) Ultra Headphones support (#60): RFCOMM
  channel fallback (8, then 2) with connection retries, audio modes
  (`--audio-mode`, `get_audio_mode`, `get_audio_modes`,
  `set_audio_mode`) and `--self-voice` on that device.
* Volume, media keys, active source and own Bluetooth address (#63):
  `--set-volume`, `--send-media-key`, `--active-device`,
  `--device-bd-addr`, and `set_volume`, `send_media_key`,
  `get_active_device`, `get_device_bd_addr` in the library.
* `BoseError::DeviceError` (FFI code `-13`): the device answered with
  an ERROR packet (#60, #63).
* `libbose_connect.so` is built alongside the rlib (#55).
* `bose-connect-gui`: a GTK4 + libadwaita desktop app with KDE Plasma
  integration (#64). **Still a mockup:** it runs on a simulated device
  and is not yet connected to the library. Not part of the release
  artefacts or crates.io.

### Changed

* Every reply is parsed from its `block, function, operator, length`
  header instead of fixed-size acknowledgements (#60).
* `send_packet` does one write and one read, as the C version did; it
  no longer waits for a receive timeout after the device answered
  (#63).
* Minimum supported Rust version is now 1.85 (the previously declared
  1.75 could not build the dependency tree).

### Fixed

* `--info` on a QC35 II with firmware 4.8.1: the device-status reply
  has extra packets the fixed-size parser rejected (#60).
* An ERROR reply to `set_volume` no longer leaves a byte in the socket
  that desynchronised the next request (#63).

### Breaking changes (library API)

* `DeviceStatusReport` gains `device_id`, and `minutes` is now
  `Option<u16>` (`None` where the auto-off layout is not decoded, for
  example on the QC Ultra; the FFI writes `0xffff`).
* `BoseError` gains the `DeviceError` variant.

### Known issues

* A SoundLink Color II (firmware 4.0.1) switches itself off after its
  9th RFCOMM connection since power-on. Each CLI invocation opens one
  connection. See `DEVELOPMENT.md`.
* The QC Ultra rejects media keys (ERROR `0c`).
* The first byte of the `set_volume` reply is the top of the device's
  volume scale, which each product treats differently; see the table
  in `DEVELOPMENT.md`.

## [0.1.0][] - 2026-09-27

First Rust release: a port of the original C implementation, with the
`bose-connect` library, the `bose-connect-app-linux` CLI and a C FFI
(`bose_connect.h`).

[Unreleased]: https://github.com/airvzxf/bose-connect-app-linux/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/airvzxf/bose-connect-app-linux/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/airvzxf/bose-connect-app-linux/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/airvzxf/bose-connect-app-linux/releases/tag/v0.1.0
