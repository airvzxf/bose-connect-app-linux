# Changelog

All notable changes to this project are documented here. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased][]

### Fixed

* `connect_device`, `disconnect_device` and `remove_device` report an
  ERROR reply as `BoseError::DeviceError` instead of success. The one
  exception is the SoundLink Color II reply to `connect_device`
  (ERROR `0b`), still accepted until a hardware test says what it
  means.
* The same three commands read their reply packet by packet and return
  as soon as it is complete, instead of reading up to 64 bytes until the
  receive timeout. They no longer wait a second after the device
  answered, and no longer swallow the start of the next reply.
* `get_active_device` and `get_device_bd_addr` turn an ERROR reply into
  `BoseError::DeviceError` and consume it, so the next request on the
  same connection reads its own reply.

### Changed

* `set_volume`, `send_media_key`, `get_active_device` and
  `get_device_bd_addr` read their replies through the same header-based
  reader as the other commands: a PROCESSING packet before the reply is
  skipped, and a payload of the wrong length is `AckMismatch`.
* The protocol notes moved from `DEVELOPMENT.md` to `docs/protocol.md`,
  organised by command, with the device and firmware of each capture and
  the Rust call that sends it. `DEVELOPMENT.md` keeps the firmware-update
  notes.

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

[Unreleased]: https://github.com/airvzxf/bose-connect-app-linux/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/airvzxf/bose-connect-app-linux/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/airvzxf/bose-connect-app-linux/releases/tag/v0.1.0
