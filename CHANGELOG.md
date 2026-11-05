# Changelog

All notable changes to this project are documented here. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased][]

### Changed

* GUI: joining with this computer as the source no longer always waits
  a minute. The GUI watches the audio link to the speaker every two
  seconds; when it has already been quiet for a minute the join starts
  at once, otherwise the countdown shows the time left and a "Join
  now" button skips it.

### Added

* GUI: party and stereo mode for two SoundLink Color II, in a "Party
  mode" group on the speaker's page: "Join with…" (the other speaker,
  from this computer's and the speaker's paired devices, and the
  music source: this computer or a phone connected to the speaker), a
  Party/Stereo switch and "Separate". With this computer as the
  source it pauses the media players (MPRIS), waits for a minute
  without audio before joining, and restarts the audio stream after a
  mode change while music plays; with a phone as the source it first
  drops this computer's audio profiles so the speaker keeps the
  control connection. The controls stay disabled until the speaker
  answers, and a failed command is shown in the group. A row shows the
  sample rate of the audio link, with a "Use 44.1 kHz" button that
  writes a WirePlumber drop-in and restarts WirePlumber, and a warning
  that the speakers lock up now and then. Tested on two speakers,
  firmware 4.0.1, with both sources.
* `BoseDevice::paired_device` (`get_paired_device`, `PairedDevice`):
  the same `04 05` request as `device_info`, also returning the
  product id of a paired Bose device.

### Fixed

* GUI: a reply that comes late no longer closes the connection; three
  in a row do. A SoundLink Color II sometimes answers after more than
  a second, and in party mode with a phone as the source it accepts no
  new connection, so the GUI lost the speaker until party mode ended.
* GUI: errors are also written to the terminal log.

## [0.5.0][] - 2026-11-05

### Added

* NC 700 Headphones (`0x4024`, RFCOMM channel 8), contributed by
  @ciarancoffey (#109) and verified on firmware 1.8.2: stepped noise
  cancelling `01 05` (`-c off|high|0..10`, the Bose Music app's scale;
  the device counts the other way round), favourites `01 0f`
  (`--nc-favorites`), remember last setting `01 0e` (`--remember-mode`),
  motion auto-off `01 14` (`--motion-auto-off`), the shortcut with
  event byte `05` and a `wake-word` action
  (`ShortcutAction::ToggleWakeWord`), and the existing equalizer, self
  voice, auto-off, voice prompts, volume, prompt language, pairing and
  paired-device commands.
* `--watch <SECONDS>` (`enable_notifications`, `read_notification`):
  subscribe to the device's own notifications (`09 02`) and print them.

* Party and stereo mode for two SoundLink Color II speakers:
  `BoseDevice::join_party`, `leave_party`, `party_mode` and
  `set_party_mode` (`PartyMode`, `has_party_mode`), and the CLI flags
  `--party-join`, `--party-source`, `--party-leave` and `--party-mode`.
  Verified on two speakers, firmware 4.0.1; `docs/protocol.md` records
  the captures and what made it work with the computer as the music
  source (44.1 kHz, a minute of silence before joining, an audio-stream
  restart after every mode change).

* GUI: the device menu (top left) and the "My Bose" page list the Bose
  devices paired with this computer, read from BlueZ (`Modalias`
  vendor `009E`), say which ones BlueZ has connected, and mark the one
  in use; "Manage devices…" is always there. With no Bose device
  paired the window explains how to pair one; with none connected it
  shows the normal layout, empty, with "connect to a device". A new
  "Disconnect" frees the device for other clients. Tested on a QC
  Ultra and a QC35 II.
* GUI in English by default, with Spanish as the alternative, following
  the desktop language. One translation layer (Fluent): a new language
  is one `crates/bose-connect-gui/locales/<language>.ftl` file;
  `CONTRIBUTING.md` explains how to add one.
* `PromptLanguage::from_status_byte` and `LANGUAGE_CODE_MASK` decode a
  prompt-language status byte whatever its flag bits.

### Changed

* Connect and disconnect (`write_paired`) succeed when the device
  acknowledges the address with PROCESSING and sends no RESULT within
  15 s (the NC 700 never sends one); the CLI then checks the connection
  state or the paired list instead of retrying.
* `set_pairing` and `set_auto_off` read the value back when the device
  does not answer the write.
* The prompt-language check compares the language and prompts bits
  only, not bit 6 (the NC 700 drops it when it stores the byte).
* `ShortcutAction` has a new variant; exhaustive matches need an arm.
* GUI: `--real`, `--real-bluetooth` and `--mock` are gone; the app
  always talks to real devices. At startup it connects to the last
  device only while BlueZ has it connected (else to a connected Bose
  device). The simulated device is for tests and screenshots only
  (`BOSE_CONNECT_GUI_SIMULATOR=1`, `disconnected` or `no-devices`),
  uses locally administered addresses and never overwrites the last
  device.
* GUI: switching devices closes the old RFCOMM connection before
  opening the new one and clears the old device's data at once.
* GUI: the prompt language is read-only on the QC Ultra, as in the
  CLI; elsewhere the write keeps the device's other bits of the byte.
* GUI: placeholder controls that did nothing (packet injection, a
  fixed "last response", the scan and "forget" buttons of "My Bose")
  are removed.

### Fixed

* `--paired-devices` failed with an ACK mismatch when only the second
  listed device was connected (`cc = 02`) or none had an audio link
  (`cc = 00`, a QC35 II after its phone was disconnected); the byte has
  one bit per list position. `DevicesConnected::Zero` is new.
* `--pairing on|off` changed the QC35 II's pairing mode but reported an
  ACK mismatch: firmware 4.8.1 answers with the state
  `04 08 06 02 <flag> 01`, not a one-byte echo.
* GUI: a QC Ultra set to Spanish showed its prompt language as
  "English" (verified on firmware 1.6.7: `< 01 03 03 07 a6 …` now
  shows Español, as the CLI's `ES`).
* GUI: a failed connection kept showing the previous device's data.
* GUI: the simulator's battery drain stopped right after start.

## [0.4.0][] - 2026-11-03

### Added

* `bose-connect-gui --real --address AA:BB:CC:DD:EE:FF` talks to the
  device through the library over one persistent RFCOMM connection
  (reopened after a socket error), instead of the simulated device.
  Device calls run off the GTK main thread, one at a time; a real
  device is re-read every 60 s. The last address is remembered.
  Pause is refused on the QC Ultra, as in the CLI. Tested on a QC
  Ultra, a QC35 II and a SoundLink Color II.
* `--audio-state` and a GUI row show how BlueZ sees the device's audio
  link to this computer (`org.bluez.Device1` and
  `org.bluez.MediaTransport1` over D-Bus), with a hint to reconnect
  from this computer when it is connected without a working audio
  stream. On the QC Ultra, a transport outside the device's stream
  endpoints (`dev_XX/fd0` instead of `dev_XX/sepN/fd0`) counts as not
  working: that is what BlueZ leaves after a headphone-started
  connection that plays nothing there. On a QC35 II the same kind of
  transport plays, so other products are not flagged. `--info`
  prints it too. Library: `AudioState`.
* `--pairing` (and the GUI pairing switch) on the SoundLink Color II.
  Entering pairing mode disconnects every audio source.
* Audio modes and "remember my mode" on the QC45 (device id
  `0x4039`): `has_audio_modes` is true for it, so `--audio-mode`,
  `--remember-mode` and the audio mode in `--device-status` work there.
* Per-feature predicates `has_equalizer`, `has_battery_prompt`,
  `has_immersive_audio`, `has_auto_play_pause`, `has_auto_answer` and
  `has_shortcut`, true for the QC Ultra only so far. `--device-status`
  prints each setting only on the devices that have it.
* Equalizer and shortcut on the QC45: `has_equalizer` and
  `has_shortcut` are true for it, so `--equalizer`, `--shortcut` and
  both lines in `--device-status` work there.
* Voice prompts and prompt language on the QC45: `--voice-prompts`
  and `--prompt-language` work there (`has_legacy_settings` is true
  for it).
* Self-voice on the QC45: `has_self_voice` is true for it, so
  `--self-voice` works there.

### Changed

* License metadata now matches the `LICENSE` file: AGPL-3.0-or-later
  in `Cargo.toml` (crates.io), the Debian and AUR recipes, the
  AppStream metainfo and the GUI's About dialog. They said GPL-3.0.
* README: the project is presented as a Rust app and library inspired
  by Denton-L/based-connect rather than as a fork of it; the CLI usage
  block is the current `--help`.
* TODO.md lists open work only.

### Fixed

* `set_audio_mode` accepts a PROCESSING reply without a RESULT (QC45)
  and reads the new mode back with a GET.
* `get_audio_modes` stops at the first slot that answers ERROR `06` or
  `08` instead of failing; the QC45 has four slots, not ten.
* GUI: the page flickered after every device answer, including the
  60 s refresh, because it was rebuilt each time. It is now rebuilt
  only when the device snapshot changed or a setter failed, and keeps
  its scroll position when it is.
* GUI Bluetooth discovery found no devices (object paths were read as
  strings) and would have panicked on `InterfacesAdded` (an
  `a{sa{sv}}` dict passed to `VariantDict`). Bose devices are now
  recognised by BlueZ's `Modalias` vendor id `009E`; none of the
  tested devices lists the Bose Connect UUID.
* `device_info` read the name of a Bose device (another SoundLink
  Color II in the list) with a leading `01` byte: such devices have 3
  bytes between the status and the name, not 2. The status byte's
  high bits no longer turn a connected device into "disconnected".
* GUI mock: every setting change froze the window (each setter took
  a read lock while holding the write lock).
* GUI: the voice-prompts switch and the profiles no longer reset the
  prompt language to English.
* `set_language_keep_voice_prompts` (`--prompt-language`) keeps the
  voice-prompts setting and the other bits of the prompt-language
  byte. It used to turn voice prompts on every time and to clear the
  unknown bit `0x80` of the QC Ultra and the QC45.
* `set_prompt_language` (also behind `--voice-prompts`) no longer
  fails when the write is not answered (QC45): it reads the byte back
  with a GET until it matches.

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

[Unreleased]: https://github.com/airvzxf/bose-connect-app-linux/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/airvzxf/bose-connect-app-linux/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/airvzxf/bose-connect-app-linux/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/airvzxf/bose-connect-app-linux/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/airvzxf/bose-connect-app-linux/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/airvzxf/bose-connect-app-linux/releases/tag/v0.1.0
