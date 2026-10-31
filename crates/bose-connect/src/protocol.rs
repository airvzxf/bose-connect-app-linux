//! Bose Connect protocol commands.
//!
//! Every public function here corresponds to one C function in
//! `library/based.c`. The function signatures, packet byte
//! sequences, masked-ACK masks, and short-read / short-write
//! semantics are preserved verbatim; only the type system and error
//! handling are upgraded. If the C code says "send
//! `[0x01, 0x02, 0x02, ANY]`", the Rust code sends exactly the same
//! bytes in the same order.
//!
//! Replies are read as `block, function, operator, length` followed
//! by `length` payload bytes rather than compared against fixed-size
//! ACKs: the QC Ultra Headphones send the same packets with longer
//! payloads. The audio-mode functions (block `0x1f`) are new and were
//! captured from a QC Ultra Headphones on firmware 1.6.7.
//!
//! The protocol functions are **generic** over [`crate::io::BoseIo`]:
//! production code passes a `Connection` (RFCOMM socket) and tests
//! pass a `UnixStream` pair. This keeps the byte-level guarantees
//! in one place while making the protocol layer headlessly
//! testable.
//!
//! ## Byte-order guarantee
//!
//! The `BdAddr` bytes are stored in **canonical MSB-first** order
//! (see [`crate::types::BdAddr`]) and the protocol packet layouts
//! below assume that. No byte swapping happens here.

use std::time::{Duration, Instant};

use crate::error::{unconfirmed, BoseError, BoseResult};
use crate::io::BoseIo;
use crate::types::{
    AutoOff, BdAddr, Device, DeviceStatus, DevicesConnected, EqBand, EqualizerBand, ImmersiveAudio,
    MediaKey, NoiseCancelling, Pairing, PromptLanguage, SelfVoice, ShortcutAction, MAX_AUDIO_MODES,
    MAX_NAME_PACK_LEN_HELPER, MAX_NUM_DEVICES, NOISE_CANCELLING_0C, NOISE_CANCELLING_14,
    NOISE_CANCELLING_20, QC45_HEADPHONES, QC_ULTRA_HEADPHONES, VP_ENABLE_BIT, VP_MASK,
};

// Re-export the helper that wraps CN_BASE_PACK_LEN + MAX_NAME_LEN - 1
// so the size below stays in lockstep with the type definition.
use crate::types as t;

// ---------------------------------------------------------------------------
// Packet constants. The naming matches the C `#define`s in `based.c` so a
// side-by-side diff stays readable. Each `*_SEND` is the literal byte
// sequence the host writes; each `*_ACK` (and `*_MASK`) is the expected
// response after masking. Any byte listed as `ANY` is "don't care" — the
// mask byte at that position is 0x00 so it does not affect the comparison.
// ---------------------------------------------------------------------------

const BYTES_POSITION_2: usize = 2;
const BYTES_POSITION_3: usize = 3;
const BYTES_POSITION_4: usize = 4;
const BYTES_POSITION_5: usize = 5;
const BYTES_POSITION_10: usize = 10;
const BYTES_POSITION_11: usize = 11;

const GET_DEVICE_ID_SEND: [u8; 4] = [0x00, 0x03, 0x01, 0x00];

const SET_NAME_SEND_PREFIX: [u8; 4] = [0x01, 0x02, 0x02, 0x00];

const SET_PROMPT_LANGUAGE_SEND: [u8; 5] = [0x01, 0x03, 0x02, 0x01, 0x00];

const SET_AUTO_OFF_SEND: [u8; 5] = [0x01, 0x04, 0x02, 0x01, 0x00];
const GET_AUTO_OFF_SEND: [u8; 4] = [0x01, 0x04, 0x01, 0x00];
/// `01 04 02 03 lo 00 hi`: auto-off in the QC Ultra's 3-byte layout.
const SET_AUTO_OFF_WIDE_SEND_PREFIX: [u8; 4] = [0x01, 0x04, 0x02, 0x03];

const GET_PROMPT_LANGUAGE_SEND: [u8; 4] = [0x01, 0x03, 0x01, 0x00];
/// `01 03 02 02 <language byte> <battery prompt>`, QC Ultra.
const SET_BATTERY_PROMPT_SEND_PREFIX: [u8; 4] = [0x01, 0x03, 0x02, 0x02];
/// Length of the QC Ultra prompt-language payload; its last byte is
/// the "battery level at power-on" prompt.
const PROMPT_LANGUAGE_WIDE_LEN: usize = 7;

const GET_EQUALIZER_SEND: [u8; 4] = [0x01, 0x07, 0x01, 0x00];
/// `01 07 02 02 <value> <band>`, QC Ultra.
const SET_EQUALIZER_SEND_PREFIX: [u8; 4] = [0x01, 0x07, 0x02, 0x02];

const GET_SHORTCUT_SEND: [u8; 4] = [0x01, 0x09, 0x01, 0x00];
/// `01 09 02 03 80 09 <action>`, QC Ultra.
const SET_SHORTCUT_SEND_PREFIX: [u8; 4] = [0x01, 0x09, 0x02, 0x03];
/// Button id and event of the shortcut: the right earcup's volume
/// strip, touch and hold.
const SHORTCUT_BUTTON_EVENT: [u8; 2] = [0x80, 0x09];

const IMMERSIVE_AUDIO: (u8, u8) = (0x05, 0x0f);
const AUTO_PLAY_PAUSE: (u8, u8) = (0x01, 0x18);
const AUTO_ANSWER_CALLS: (u8, u8) = (0x01, 0x1b);
const REMEMBER_AUDIO_MODE: (u8, u8) = (0x1f, 0x05);

const SET_NOISE_CANCELLING_SEND: [u8; 5] = [0x01, 0x06, 0x02, 0x01, 0x00];

const GET_DEVICE_STATUS_SEND: [u8; 4] = [0x01, 0x01, 0x05, 0x00];

const GET_DEVICE_STATUS_FINAL_ACK: [u8; 4] = [0x01, 0x01, 0x06, 0x00];

const GET_FIRMWARE_VERSION_SEND: [u8; 4] = [0x00, 0x05, 0x01, 0x00];

const GET_SERIAL_NUMBER_SEND: [u8; 4] = [0x00, 0x07, 0x01, 0x00];

const GET_BATTERY_LEVEL_SEND: [u8; 4] = [0x02, 0x02, 0x01, 0x00];

const GET_PAIRED_DEVICES_SEND: [u8; 4] = [0x04, 0x04, 0x01, 0x00];

const SET_PAIRING_SEND: [u8; 5] = [0x04, 0x08, 0x05, 0x01, 0x00];

/// The C packet ended with a third payload byte `38` that its length
/// byte (`02`) does not cover; it is dropped (see [`set_self_voice`]).
const SET_SELF_VOICE_SEND: [u8; 6] = [0x01, 0x0b, 0x02, 0x02, 0x01, 0x00];

// ---------------------------------------------------------------------------
// Media / volume / audio-routing commands. These are NOT in the
// original C source (`based.c` has no counterpart). The request
// packets for media keys, volume and active device are listed in
// DEVELOPMENT.md (sniffed by the original author); the responses and
// `get_device_bd_addr` were captured live against a Bose SoundLink
// Color II (Foreman, firmware 4.0.1).
//
// These are *direct* Bose codes — they are not generic volume /
// media-key commands that ride on a separate standard (AVCTP, HSP,
// etc.). The Bose firmware implements them at the RFCOMM layer;
// the device echoes back a status byte so callers can verify the
// change took effect.
//
// A rejected request is answered with an ERROR packet
// `[block, function, 0x04, len, code…]` instead of the usual STATUS
// (0x03) / RESULT (0x06) reply; see `check_reply`.
// ---------------------------------------------------------------------------

/// `set_volume` request — subsystem 0x05, opcode 0x05, command
/// 0x02 (SET), length 0x01, payload = volume level. The reply is
/// `[0x05, 0x05, 0x03, 0x02, max, volume_echo]`; the valid range is
/// device-specific, see [`set_volume`] for what `max` means on each
/// device.
const SET_VOLUME_SEND_PREFIX: [u8; 4] = [0x05, 0x05, 0x02, 0x01];

/// Media-key request — subsystem 0x05, opcode 0x03, operator 0x05,
/// length 0x01, payload = media key code. Keys:
/// 0x01 play/pause toggle, 0x03 next, 0x04 previous. The play/pause
/// toggle is exposed as `MediaKey::Pause` — see
/// [`crate::types::MediaKey`] for why. The reply is two packets
/// (verified on a SoundLink Color II): `[0x05, 0x03, 0x07, 0x00]`
/// (PROCESSING) followed by `[0x05, 0x03, 0x06, 0x00]` (RESULT).
const SEND_MEDIA_KEY_SEND_PREFIX: [u8; 4] = [0x05, 0x03, 0x05, 0x01];

/// `get_active_device` request — returns the BT address of the
/// source currently feeding the A2DP sink. Subsystem 0x05, opcode
/// 0x01, command 0x01 (GET), length 0x00. The reply is
/// `[0x05, 0x01, 0x03, 0x09]` and a 9-byte payload,
/// `[0x00, 0x02, 0x01, addr_0..addr_5]` on the live capture: an
/// unknown status triple, then the MSB-first BT address of the source.
const GET_ACTIVE_DEVICE_SEND: [u8; 4] = [0x05, 0x01, 0x01, 0x00];

/// Payload length of the `get_active_device` reply.
const ACTIVE_DEVICE_PAYLOAD_LEN: usize = 9;

/// `get_device_bd_addr` — returns the speaker's *own* Bluetooth
/// address. Subsystem 0x00, opcode 0x06, command 0x01 (GET),
/// length 0x00. The reply is `[0x00, 0x06, 0x03, 0x06]` and the
/// 6-byte MSB-first BT address of the speaker itself (not the source).
/// Useful for distinguishing which physical speaker a given socket
/// is bound to when the same laptop has two paired speakers (e.g.
/// the test rig with the White + Black Foremans).
const GET_DEVICE_BD_ADDR_SEND: [u8; 4] = [0x00, 0x06, 0x01, 0x00];

const GET_DEVICE_INFO_SEND_PREFIX: [u8; 4] = [0x04, 0x05, 0x01, 6];

const CONNECT_DEVICE_SEND_PREFIX: [u8; 5] = [0x04, 0x01, 0x05, 6 + 1, 0x00];
#[allow(dead_code)]
const CONNECT_DEVICE_ACK_PREFIX: [u8; 4] = [0x04, 0x01, 0x07, 6];

const DISCONNECT_DEVICE_SEND_PREFIX: [u8; 4] = [0x04, 0x02, 0x05, 6];
#[allow(dead_code)]
const DISCONNECT_DEVICE_ACK_PREFIX: [u8; 4] = [0x04, 0x02, 0x07, 6];

const REMOVE_DEVICE_SEND_PREFIX: [u8; 4] = [0x04, 0x03, 0x05, 6];
#[allow(dead_code)]
const REMOVE_DEVICE_ACK_PREFIX: [u8; 4] = [0x04, 0x03, 0x06, 6];

// Audio modes (function block 0x1f). Only present on the newer
// "Bose app" generation; captured from a real QC Ultra Headphones on
// firmware 1.6.7 and a real QC45 on firmware 4.0.4.
const GET_AUDIO_MODE_SEND: [u8; 4] = [0x1f, 0x03, 0x01, 0x00];
const SET_AUDIO_MODE_SEND_PREFIX: [u8; 4] = [0x1f, 0x03, 0x05, 0x02];
const GET_AUDIO_MODE_CONFIG_SEND_PREFIX: [u8; 4] = [0x1f, 0x06, 0x01, 0x01];
/// Offset of the NUL-padded mode name inside a `1f 06` payload.
const AUDIO_MODE_NAME_OFFSET: usize = 6;
/// Width of the NUL-padded mode name field.
const AUDIO_MODE_NAME_LEN: usize = 32;
/// Name the device reports for an unused audio-mode slot.
const AUDIO_MODE_EMPTY_NAME: &str = "None";

// Operator byte (packet byte 2) of the device's replies.
const OP_STATUS: u8 = 0x03;
const OP_ERROR: u8 = 0x04;
const OP_RESULT: u8 = 0x06;
const OP_PROCESSING: u8 = 0x07;

const BT_ADDR_LEN: usize = 6;
const CN_BASE_PACK_LEN: usize = 4;
const MAX_BT_PACK_LEN_T: usize = crate::types::MAX_BT_PACK_LEN;
const MAX_SERIAL_SIZE_T: usize = crate::types::MAX_SERIAL_SIZE;
const MAX_NAME_LEN_T: usize = t::MAX_NAME_LEN;
const MAX_NAME_PACK_LEN: usize = MAX_NAME_PACK_LEN_HELPER;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// `int has_noise_cancelling(unsigned int device_id)` in `based.c`.
pub fn has_noise_cancelling(device_id: u16) -> bool {
    matches!(
        device_id,
        NOISE_CANCELLING_14 | NOISE_CANCELLING_20 | NOISE_CANCELLING_0C
    )
}

/// `has_self_voice` is a new predicate that does not exist in
/// the original C code. It is used by the CLI to short-circuit
/// `--self-voice` on devices known not to support the command
/// (so the user gets an instant error instead of waiting for
/// the 1 s `SO_RCVTIMEO` × 3 retry budget on a command the
/// device will never answer).
///
/// Conservative allow-list: the QC35 series (device IDs `0x4014`
/// and `0x4020`) and the QC Ultra Headphones (`0x4066`, same
/// packet format, verified on hardware) expose self-voice. The
/// SoundLink II (`0x400d`) does not. If a new device is found
/// to support self-voice, add its device ID here and the CLI's
/// pre-flight check will start allowing the command.
pub fn has_self_voice(device_id: u16) -> bool {
    matches!(
        device_id,
        NOISE_CANCELLING_14 | NOISE_CANCELLING_20 | QC_ULTRA_HEADPHONES
    )
}

/// `has_audio_modes` is true for devices that replace the
/// noise-cancelling levels with named audio modes (Quiet, Aware,
/// Immersion, …) in function block `0x1f`: the QC Ultra Headphones
/// (`0x4066`) and the QC45 (`0x4039`), both verified on hardware.
pub fn has_audio_modes(device_id: u16) -> bool {
    matches!(device_id, QC_ULTRA_HEADPHONES | QC45_HEADPHONES)
}

/// `has_qc_ultra_settings` is true for devices with the settings
/// decoded on the QC Ultra Headphones (`0x4066`): equalizer,
/// immersive audio, battery prompt, auto play/pause, auto answer and
/// the shortcut. Not tested on other devices, the QC45 included.
pub fn has_qc_ultra_settings(device_id: u16) -> bool {
    matches!(device_id, QC_ULTRA_HEADPHONES)
}

/// `has_legacy_settings` is false for devices whose prompt-language
/// payload is not the 1-byte QC35 layout. Changing the *language* there
/// has not been tested (the QC Ultra's byte carries an extra bit `0x80`
/// that the language setters would clear), so the CLI refuses
/// `--prompt-language` on them. Auto-off and voice prompts follow the
/// layout the device reports and work on both.
pub fn has_legacy_settings(device_id: u16) -> bool {
    !matches!(device_id, QC_ULTRA_HEADPHONES)
}

/// `has_pairing_toggle` is a new predicate that does not exist
/// in the original C code. It mirrors [`has_self_voice`] but
/// for the `--pairing` flag (which controls RFCOMM
/// discoverability). Only the QC35 series supports it; the
/// SoundLink II returns an `AckMismatch` immediately, but the
/// 1 s timeout per retry means the C version wastes ~3 s on
/// every call before failing. Pre-flighting here saves that
/// wall-clock cost.
pub fn has_pairing_toggle(device_id: u16) -> bool {
    matches!(device_id, NOISE_CANCELLING_14 | NOISE_CANCELLING_20)
}

/// `int send_packet(int sock, const void *send, size_t send_n,
/// uint8_t received[MAX_BT_PACK_LEN])` in `based.c`. Returns the
/// bytes received, or a [`BoseError::ShortRead`] /
/// [`BoseError::ShortWrite`] on partial IO.
///
/// Faithful to the C version: a single `write()` followed by a
/// single `read()`. The C code never looped — it relied on `read()`
/// returning however many bytes the kernel had buffered, which on a
/// stream socket is exactly one short packet's worth for typical
/// Bose responses (5–15 bytes).
///
/// The original Rust port added a drain-the-socket loop with
/// `TimedOut`/`WouldBlock` break conditions. That was a behavioural
/// deviation from the C source: it made `--send-packet` block for
/// at least one full `SO_RCVTIMEO` (1 s on `Connection`) after the
/// device finished sending, and it caused infinite hangs in
/// `UnixStream`-driven tests that don't set a receive timeout
/// (see `tests/send_packet_timeout.rs`). Reverted to a single
/// `read()` to match `based.c` byte-for-byte.
pub fn send_packet<I: BoseIo>(io: &mut I, send: &[u8]) -> BoseResult<Vec<u8>> {
    io.bose_send(send)?;
    let mut buf = vec![0u8; MAX_BT_PACK_LEN_T];
    let n = io.read(&mut buf)?;
    buf.truncate(n);
    Ok(buf)
}

/// `int get_device_id(int sock, unsigned int *device_id, unsigned int *index)`
/// in `based.c`.
pub fn get_device_id<I: BoseIo>(io: &mut I) -> BoseResult<(u16, u8)> {
    io.bose_send(&GET_DEVICE_ID_SEND)?;
    // Reply `00 03 03 03 pp pp ii`: the product id big-endian (bswap_16
    // in the C code), then the index.
    let payload = read_response(io, 0x00, 0x03)?;
    let [hi, lo, index, ..] = payload[..] else {
        return Err(BoseError::AckMismatch);
    };
    Ok((u16::from_be_bytes([hi, lo]), index))
}

/// `int set_name(int sock, const char *name)` in `based.c`. Returns
/// `Ok(())` if the device echoed back the same name, `Err`
/// otherwise.
pub fn set_name<I: BoseIo>(io: &mut I, name: &str) -> BoseResult<()> {
    if name.len() >= MAX_NAME_LEN_T {
        return Err(BoseError::InvalidArgument(format!(
            "name exceeds {} character maximum (got {})",
            MAX_NAME_LEN_T - 1,
            name.len()
        )));
    }

    // Build the wire packet. We assemble a `Vec` rather than a
    // fixed-size array because the packet length depends on the
    // name length, but the prefix is fixed.
    let mut packet = [0u8; MAX_NAME_PACK_LEN];
    packet[..4].copy_from_slice(&SET_NAME_SEND_PREFIX);
    packet[BYTES_POSITION_3] = name.len() as u8;
    // Fill the name bytes, mirroring `str_copy` semantics: copy
    // printable ASCII as-is, terminate with NUL.
    let name_bytes = name.as_bytes();
    packet[CN_BASE_PACK_LEN..CN_BASE_PACK_LEN + name_bytes.len()].copy_from_slice(name_bytes);

    let send_size = CN_BASE_PACK_LEN + name_bytes.len();
    io.bose_send(&packet[..send_size])?;

    let got = get_name(io)?;
    if got != name {
        return Err(unconfirmed(name, &got));
    }
    Ok(())
}

/// `int get_firmware_version(int sock, char version[VER_STR_LEN])` in
/// `based.c`. The QC35 returns a 5-character version (`"1.3.2"`);
/// newer devices return a longer one (`"1.6.7+g6ebabd2"` on the QC
/// Ultra), so the length is taken from the reply header.
pub fn get_firmware_version<I: BoseIo>(io: &mut I) -> BoseResult<String> {
    io.bose_send(&GET_FIRMWARE_VERSION_SEND)?;
    let version = read_response(io, 0x00, 0x05)?;
    Ok(String::from_utf8_lossy(&version).into_owned())
}

/// `int get_serial_number(int sock, char serial[MAX_SERIAL_SIZE])`
/// in `based.c`. Returns the serial as a UTF-8 / ASCII string.
pub fn get_serial_number<I: BoseIo>(io: &mut I) -> BoseResult<String> {
    io.bose_send(&GET_SERIAL_NUMBER_SEND)?;
    let serial = read_response(io, 0x00, 0x07)?;
    Ok(String::from_utf8_lossy(&serial).into_owned())
}

/// `int get_battery_level(int sock, unsigned int *level)` in `based.c`.
/// Returns the battery level as a percent (0..=100).
///
/// The QC35 replies with a 1-byte payload; the QC Ultra appends
/// three more bytes (`64 ff ff 00`) that are ignored here.
pub fn get_battery_level<I: BoseIo>(io: &mut I) -> BoseResult<u8> {
    io.bose_send(&GET_BATTERY_LEVEL_SEND)?;
    let payload = read_response(io, 0x02, 0x02)?;
    payload.first().copied().ok_or(BoseError::AckMismatch)
}

/// No C counterpart (see the packet-constant comment above). Sets the
/// device's local volume level.
///
/// The device responds with `[0x05, 0x05, 0x03, 0x02, max,
/// volume_echo]`. We verify the volume echo matches `level` and
/// return `max`, the top of the device's volume scale. It is *not*
/// the battery level: it never changes with the level or the battery.
/// How devices treat it differs (verified on hardware):
///
/// | Device                     | `max`        | Accepted | Above the range         |
/// | -------------------------- | ------------ | -------- | ----------------------- |
/// | QC35 II (fw 4.8.1)         | `0x19` (25)  | `0..=25` | ERROR `05 05 04 01 06`  |
/// | QC Ultra (fw 1.6.7)        | `0x1f` (31)  | `0..=31` | clamped to 31, no error |
/// | SoundLink Color II (4.0.1) | `0x64` (100) | `0..=99` | ERROR `05 05 04 01 06`  |
///
/// So `max` is the highest accepted level except on the SoundLink
/// Color II, which rejects it. The range is not checked here: an
/// out-of-range level comes back as [`BoseError::DeviceError`], or, on
/// a device that clamps instead (QC Ultra), as
/// [`BoseError::UnconfirmedValue`].
pub fn set_volume<I: BoseIo>(io: &mut I, level: u8) -> BoseResult<u8> {
    let send = [
        SET_VOLUME_SEND_PREFIX[0],
        SET_VOLUME_SEND_PREFIX[1],
        SET_VOLUME_SEND_PREFIX[2],
        SET_VOLUME_SEND_PREFIX[3],
        level,
    ];
    io.bose_send(&send)?;

    // payload[0] = top of the volume scale. payload[1] = the volume
    // level the device now has, which should equal `level` on success.
    let payload = read_response(io, SET_VOLUME_SEND_PREFIX[0], SET_VOLUME_SEND_PREFIX[1])?;
    let [max, echo] = payload[..] else {
        return Err(BoseError::AckMismatch);
    };
    if echo != level {
        return Err(unconfirmed(level, echo));
    }
    Ok(max)
}

/// No C counterpart (see the packet-constant comment above). Sends a media transport key
/// (pause/play toggle, next, previous) to the A2DP source currently
/// feeding the speaker.
///
/// The device responds with two concatenated 4-byte packets:
/// `[0x05, 0x03, 0x07, 0x00]` (PROCESSING) and
/// `[0x05, 0x03, 0x06, 0x00]` (RESULT). We read both and verify them.
pub fn send_media_key<I: BoseIo>(io: &mut I, key: MediaKey) -> BoseResult<()> {
    let send = [
        SEND_MEDIA_KEY_SEND_PREFIX[0],
        SEND_MEDIA_KEY_SEND_PREFIX[1],
        SEND_MEDIA_KEY_SEND_PREFIX[2],
        SEND_MEDIA_KEY_SEND_PREFIX[3],
        key as u8,
    ];
    io.bose_send(&send)?;

    // The QC Ultra sends the RESULT 1-3 s after PROCESSING (captured
    // 2026-10-07 and 2026-10-27), past the 1 s receive timeout.
    let deadline = Instant::now() + MEDIA_KEY_RESULT_TIMEOUT;
    loop {
        let (header, payload) = read_packet_before(io, deadline)?;
        match check_reply(
            SEND_MEDIA_KEY_SEND_PREFIX[0],
            SEND_MEDIA_KEY_SEND_PREFIX[1],
            header,
            payload,
        )? {
            Some((OP_STATUS | OP_RESULT, _)) => break,
            _ => continue,
        }
    }
    Ok(())
}

/// No C counterpart (see the packet-constant comment above). Returns the BT address of
/// the device currently feeding audio to the speaker's A2DP sink.
///
/// The response is 13 bytes total: `[0x05, 0x01, 0x03, 0x09, 0x00,
/// 0x02, 0x01, addr_0, addr_1, addr_2, addr_3, addr_4, addr_5]`.
/// The first three payload bytes are an unknown status triple;
/// the last six are the MSB-first BT address.
pub fn get_active_device<I: BoseIo>(io: &mut I) -> BoseResult<BdAddr> {
    io.bose_send(&GET_ACTIVE_DEVICE_SEND)?;

    // Skip the 3-byte status triple. The Bose firmware emits these
    // unconditionally; their meaning is unknown but they are stable
    // across the observed SLC II captures (`0x00, 0x02, 0x01`).
    let payload = read_response(io, GET_ACTIVE_DEVICE_SEND[0], GET_ACTIVE_DEVICE_SEND[1])?;
    if payload.len() != ACTIVE_DEVICE_PAYLOAD_LEN {
        return Err(BoseError::AckMismatch);
    }
    bd_addr(&payload[ACTIVE_DEVICE_PAYLOAD_LEN - BT_ADDR_LEN..])
}

/// No C counterpart (see the packet-constant comment above). Returns the speaker's
/// *own* Bluetooth address, regardless of whether an audio source
/// is connected.
///
/// The response is 10 bytes: `[0x00, 0x06, 0x03, 0x06, addr_0,
/// addr_1, addr_2, addr_3, addr_4, addr_5]`. The payload is just
/// the 6-byte MSB-first BT address.
pub fn get_device_bd_addr<I: BoseIo>(io: &mut I) -> BoseResult<BdAddr> {
    io.bose_send(&GET_DEVICE_BD_ADDR_SEND)?;
    bd_addr(&read_response(
        io,
        GET_DEVICE_BD_ADDR_SEND[0],
        GET_DEVICE_BD_ADDR_SEND[1],
    )?)
}

/// `int get_device_status(int sock, char name[MAX_NAME_LEN], enum
/// PromptLanguage *language, enum AutoOff *minutes, enum
/// NoiseCancelling *level)` in `based.c`.
///
/// The device answers with a burst of settings packets framed by
/// `01 01 07 00` and `01 01 06 00`. The C code expected a fixed
/// sequence (name, language, auto-off, NC); newer devices send more
/// packets, in a different order and with longer payloads, so each
/// packet is parsed from its own header and unknown ones are skipped.
pub fn get_device_status<I: BoseIo>(io: &mut I) -> BoseResult<DeviceStatusReport> {
    // get_device_status internally calls get_device_id first. We do
    // not want the caller to do it twice, so we factor that out.
    let (device_id, _index) = get_device_id(io)?;

    io.bose_send(&GET_DEVICE_STATUS_SEND)?;

    let mut name = None;
    let mut language = None;
    let mut minutes = None;
    let mut level = NoiseCancelling::Dne;
    for _ in 0..MAX_STATUS_DUMP_PACKETS {
        let (header, payload) = read_packet(io)?;
        if header == GET_DEVICE_STATUS_FINAL_ACK {
            return Ok(DeviceStatusReport {
                device_id,
                name: name.ok_or(BoseError::AckMismatch)?,
                language: language.ok_or(BoseError::AckMismatch)?,
                minutes,
                level,
            });
        }
        // Packets of other blocks are unsolicited notifications. Inside
        // the dump a setting can come as an ERROR packet: the QC Ultra
        // sends `01 05 04 01 08` while its audio mode is modified
        // (captured 2026-10-27). Only the settings of the report count.
        if header[0] != 0x01
            || (header[2] == OP_ERROR && !matches!(header[1], 0x02 | 0x03 | 0x04 | 0x06))
        {
            continue;
        }
        let Some((operator, payload)) = check_reply(0x01, header[1], header, payload)? else {
            continue;
        };
        if operator != OP_STATUS {
            // `01 01 07 00` opens the dump; nothing else is expected.
            if header[1] == 0x01 && operator == OP_PROCESSING {
                continue;
            }
            return Err(BoseError::AckMismatch);
        }
        match header[1] {
            0x02 => name = Some(parse_name(&payload)?),
            0x03 => language = Some(first_byte(&payload)?),
            0x04 => minutes = parse_auto_off(&payload),
            0x06 => level = parse_noise_cancelling(&payload)?,
            // EQ, buttons, sidetone, … are not part of the report.
            _ => {}
        }
    }
    Err(BoseError::AckMismatch)
}

/// Aggregate return type for [`get_device_status`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceStatusReport {
    /// Device id, as returned by [`get_device_id`].
    pub device_id: u16,
    /// Device name as reported by the device.
    pub name: String,
    /// Voice-prompt language. The high bit (`VP_ENABLE_BIT`) carries
    /// the "voice-prompts on" flag; the lower 5 bits hold the
    /// language code. Use [`PromptLanguage::from_u8`] and
    /// `byte & VP_ENABLE_BIT` to split.
    pub language: u8,
    /// Auto-off value as raw minutes. `Some(0)` means "never";
    /// `None` means the device uses a payload layout this crate
    /// does not decode (QC Ultra).
    pub minutes: Option<u16>,
    /// Current noise-cancelling level, or [`NoiseCancelling::Dne`]
    /// if the device has no noise-cancelling hardware.
    pub level: NoiseCancelling,
}

/// `int set_pairing(int sock, enum Pairing pairing)` in `based.c`.
pub fn set_pairing<I: BoseIo>(io: &mut I, pairing: Pairing) -> BoseResult<()> {
    let mut send = SET_PAIRING_SEND;
    send[BYTES_POSITION_4] = pairing as u8;
    io.bose_send(&send)?;
    // Reply `04 08 06 01 xx` (the C acknowledgement).
    match read_response(io, 0x04, 0x08)?[..] {
        [echo] if echo == pairing as u8 => Ok(()),
        _ => Err(BoseError::AckMismatch),
    }
}

/// `int set_self_voice(int sock, enum SelfVoice selfVoice)` in `based.c`.
///
/// **Deviates from the C bytes on purpose.** The C packet
/// `01 0b 02 02 01 xx 38` declares a 2-byte payload but carries 3, so
/// the device read the trailing `38` as the first byte of the *next*
/// request on the same connection: on a QC Ultra (firmware 1.6.7) the
/// request after it was answered with `38 01 04 01 03` (ERROR for
/// block 38). `01 0b 02 02 01 xx` was verified on the QC Ultra
/// (2026-10-07) and on a QC35 II, firmware 4.8.1 (2026-10-27).
pub fn set_self_voice<I: BoseIo>(io: &mut I, level: SelfVoice) -> BoseResult<()> {
    let mut send = SET_SELF_VOICE_SEND;
    send[BYTES_POSITION_5] = level as u8;
    io.bose_send(&send)?;
    // Reply `01 0b 03 03 01 xx 0f` (the C acknowledgement).
    match read_response(io, 0x01, 0x0b)?[..] {
        [0x01, echo, ..] if echo == level as u8 => Ok(()),
        _ => Err(BoseError::AckMismatch),
    }
}

/// `int set_noise_cancelling(int sock, enum NoiseCancelling level)` in
/// `based.c`. The C code does not verify that the device actually
/// supports noise-cancelling before sending; callers should run
/// [`has_noise_cancelling`] on the result of [`get_device_id`]
/// first to surface a friendly error.
pub fn set_noise_cancelling<I: BoseIo>(io: &mut I, level: NoiseCancelling) -> BoseResult<()> {
    let mut send = SET_NOISE_CANCELLING_SEND;
    send[BYTES_POSITION_4] = level as u8;
    io.bose_send(&send)?;

    let got = get_noise_cancelling(io)?;
    if got as u8 != level as u8 {
        return Err(unconfirmed(level as u8, got as u8));
    }
    Ok(())
}

/// `int set_auto_off(int sock, enum AutoOff minutes)` in `based.c`.
///
/// The payload layout follows what the device reports for `01 04`
/// first: the QC35 sends 1 byte of minutes (the C packet
/// `01 04 02 01 mm`), the QC Ultra 3 bytes `lo 00 hi` (minutes,
/// little-endian, split around a zero byte: `05 00 00` is 5 minutes,
/// `a0 00 05` is 1440). Both were set from a laptop on hardware.
/// [`AutoOff::Min1440`] is refused on the 1-byte layout.
pub fn set_auto_off<I: BoseIo>(io: &mut I, minutes: AutoOff) -> BoseResult<()> {
    let value = minutes as u16;
    let [lo, hi] = value.to_le_bytes();

    io.bose_send(&GET_AUTO_OFF_SEND)?;
    match read_response(io, 0x01, 0x04)?.len() {
        1 if hi == 0 => {
            let mut send = SET_AUTO_OFF_SEND;
            send[BYTES_POSITION_4] = lo;
            io.bose_send(&send)?;
        }
        1 => {
            return Err(BoseError::InvalidArgument(format!(
                "this device cannot set auto-off to {value} minutes"
            )))
        }
        3 => {
            let mut send = [0u8; 7];
            send[..4].copy_from_slice(&SET_AUTO_OFF_WIDE_SEND_PREFIX);
            send[4..].copy_from_slice(&[lo, 0x00, hi]);
            io.bose_send(&send)?;
        }
        _ => return Err(BoseError::AckMismatch),
    }

    let got = get_auto_off(io)?;
    if got != Some(value) {
        return Err(unconfirmed(
            value,
            got.map_or_else(|| "undecodable".to_string(), |m| m.to_string()),
        ));
    }
    Ok(())
}

/// `int set_prompt_language(int sock, enum PromptLanguage language)`
/// in `based.c`. `language` should already have the
/// [`VP_ENABLE_BIT`] set or cleared as desired; use
/// [`set_voice_prompts`] for the convenience wrapper.
pub fn set_prompt_language<I: BoseIo>(io: &mut I, language_byte: u8) -> BoseResult<()> {
    let mut send = SET_PROMPT_LANGUAGE_SEND;
    send[BYTES_POSITION_4] = language_byte;
    io.bose_send(&send)?;

    let got = get_prompt_language(io)?;
    let sent_clean = language_byte & VP_MASK;
    let got_clean = got & VP_MASK;
    if sent_clean != got_clean {
        return Err(unconfirmed(sent_clean, got_clean));
    }
    Ok(())
}

/// `int set_voice_prompts(int sock, int on)` in `based.c`.
pub fn set_voice_prompts<I: BoseIo>(io: &mut I, on: bool) -> BoseResult<()> {
    // 1. Read the current device status so we know the current
    //    prompt-language byte (the set is "flip the voice-prompts
    //    bit and re-send the same language code").
    let status = get_device_status(io)?;

    // 2. Toggle the voice-prompts bit. The C code uses
    //    `pl |= VP_ENABLE_BIT` / `pl &= ~VP_ENABLE_BIT`.
    let new_byte = if on {
        status.language | VP_ENABLE_BIT
    } else {
        status.language & !VP_ENABLE_BIT
    };

    // 3. Re-send.
    set_prompt_language(io, new_byte)
}

/// `int get_paired_devices(int sock, bdaddr_t addresses[MAX_NUM_DEVICES],
/// size_t *num_devices, enum DevicesConnected *connected)` in `based.c`.
pub fn get_paired_devices<I: BoseIo>(io: &mut I) -> BoseResult<PairedDevices> {
    io.bose_send(&GET_PAIRED_DEVICES_SEND)?;

    // Reply `04 04 03 ll cc <one 6-byte address per device>`.
    let payload = read_response(io, 0x04, 0x04)?;
    let Some((&connected, list)) = payload.split_first() else {
        return Err(BoseError::AckMismatch);
    };
    let connected = DevicesConnected::from_u8(connected).ok_or(BoseError::AckMismatch)?;
    if list.len() % BT_ADDR_LEN != 0 || list.len() / BT_ADDR_LEN > MAX_NUM_DEVICES {
        return Err(BoseError::AckMismatch);
    }
    let num_devices = list.len() / BT_ADDR_LEN;

    let mut addresses = [BdAddr { b: [0; 6] }; MAX_NUM_DEVICES];
    for (slot, chunk) in addresses.iter_mut().zip(list.chunks_exact(BT_ADDR_LEN)) {
        *slot = bd_addr(chunk)?;
    }

    Ok(PairedDevices {
        addresses,
        num_devices,
        connected,
    })
}

/// Aggregate return type for [`get_paired_devices`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairedDevices {
    /// Up to [`MAX_NUM_DEVICES`] addresses. Only the first
    /// `num_devices` entries are populated.
    pub addresses: [BdAddr; MAX_NUM_DEVICES],
    /// Number of populated entries in `addresses`.
    pub num_devices: usize,
    /// How many of those devices are currently connected.
    pub connected: DevicesConnected,
}

/// `int get_device_info(int sock, bdaddr_t address, struct Device *device)`
/// in `based.c`. Returns the [`Device`] struct populated by the
/// protocol.
pub fn get_device_info<I: BoseIo>(io: &mut I, address: BdAddr) -> BoseResult<Device> {
    let mut send = [0u8; BYTES_POSITION_10];
    send[..4].copy_from_slice(&GET_DEVICE_INFO_SEND_PREFIX);
    send[BYTES_POSITION_4..BYTES_POSITION_4 + BT_ADDR_LEN].copy_from_slice(&address.b);

    io.bose_send(&send)?;

    // Reply `04 05 03 ll <addr> ss gg gg <name>`.
    let payload = read_response(io, 0x04, 0x05)?;
    if payload.len() < BT_ADDR_LEN + 1 + BYTES_POSITION_2 {
        return Err(BoseError::AckMismatch);
    }
    let (addr, rest) = payload.split_at(BT_ADDR_LEN);
    if addr != address.b {
        return Err(unconfirmed(
            format!("{:02X?}", address.b),
            format!("{:02X?}", addr),
        ));
    }
    // After the status byte come 2 bytes of garbage. The C code's
    // comment "TODO(wolf): figure out what the first byte of garbage
    // is for" is preserved here as a literal 2-byte skip — we don't
    // pretend to know what they mean.
    let status = DeviceStatus::from_u8(rest[0]).unwrap_or(DeviceStatus::Disconnected);
    let name = &rest[1 + BYTES_POSITION_2..];
    if name.len() > MAX_NAME_LEN_T {
        return Err(BoseError::InvalidArgument(format!(
            "device name length {} exceeds MAX_NAME_LEN={}",
            name.len(),
            MAX_NAME_LEN_T
        )));
    }

    let mut device = Device {
        address,
        status,
        name: [0u8; MAX_NAME_LEN_T],
        name_len: name.len(),
    };
    device.name[..name.len()].copy_from_slice(name);
    Ok(device)
}

/// Current audio mode index (QC Ultra, QC45). Resolve it to a name with
/// [`get_audio_mode_name`].
pub fn get_audio_mode<I: BoseIo>(io: &mut I) -> BoseResult<u8> {
    io.bose_send(&GET_AUDIO_MODE_SEND)?;
    let payload = read_response(io, 0x1f, 0x03)?;
    first_byte(&payload)
}

/// Name of the audio mode stored in slot `index`, or `None` if the
/// slot is unused. Slots run from 0 to [`MAX_AUDIO_MODES`] - 1.
pub fn get_audio_mode_name<I: BoseIo>(io: &mut I, index: u8) -> BoseResult<Option<String>> {
    let mut send = [0u8; 5];
    send[..4].copy_from_slice(&GET_AUDIO_MODE_CONFIG_SEND_PREFIX);
    send[BYTES_POSITION_4] = index;
    io.bose_send(&send)?;

    let payload = read_response(io, 0x1f, 0x06)?;
    if payload.len() < AUDIO_MODE_NAME_OFFSET + AUDIO_MODE_NAME_LEN || payload[0] != index {
        return Err(BoseError::AckMismatch);
    }
    let field = &payload[AUDIO_MODE_NAME_OFFSET..AUDIO_MODE_NAME_OFFSET + AUDIO_MODE_NAME_LEN];
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    let name = String::from_utf8_lossy(&field[..end]).into_owned();
    if name.is_empty() || name == AUDIO_MODE_EMPTY_NAME {
        Ok(None)
    } else {
        Ok(Some(name))
    }
}

/// Every configured audio mode as `(index, name)`, in slot order.
///
/// Reads up to [`MAX_AUDIO_MODES`] slots and stops at the first slot
/// the device answers with ERROR `06` (QC45, from slot 4 on) or `08`
/// (QC Ultra, from slot 10 on). That error on slot 0 is returned.
pub fn get_audio_modes<I: BoseIo>(io: &mut I) -> BoseResult<Vec<(u8, String)>> {
    let mut modes = Vec::new();
    for index in 0..MAX_AUDIO_MODES {
        match get_audio_mode_name(io, index) {
            Ok(Some(name)) => modes.push((index, name)),
            Ok(None) => {}
            Err(BoseError::DeviceError {
                code: 0x06 | 0x08, ..
            }) if index > 0 => break,
            Err(e) => return Err(e),
        }
    }
    Ok(modes)
}

/// Switch to the audio mode in slot `index` (QC Ultra, QC45).
///
/// The QC Ultra answers with a RESULT packet carrying the new mode
/// index. The QC45 answers with a PROCESSING packet only (`1f 03 07
/// 00`, no RESULT within 10 s on firmware 4.0.4), so the new index is
/// then read back with [`get_audio_mode`].
pub fn set_audio_mode<I: BoseIo>(io: &mut I, index: u8) -> BoseResult<()> {
    let mut send = [0u8; 6];
    send[..4].copy_from_slice(&SET_AUDIO_MODE_SEND_PREFIX);
    send[BYTES_POSITION_4] = index;
    // Byte 5 = 0: switch silently, without the voice-prompt announce.
    io.bose_send(&send)?;

    let got = read_audio_mode_switch(io)?;
    if got != index {
        return Err(unconfirmed(index, got));
    }
    Ok(())
}

/// The mode index after a `1f 03` switch: the RESULT's first byte, or
/// after a PROCESSING packet, the index read back with
/// [`get_audio_mode`].
fn read_audio_mode_switch<I: BoseIo>(io: &mut I) -> BoseResult<u8> {
    for _ in 0..MAX_UNSOLICITED_PACKETS {
        let (header, payload) = read_packet(io)?;
        match check_reply(0x1f, 0x03, header, payload)? {
            Some((OP_STATUS | OP_RESULT, payload)) => return first_byte(&payload),
            Some((OP_PROCESSING, _)) => return get_audio_mode(io),
            None => continue,
            Some(_) => return Err(BoseError::AckMismatch),
        }
    }
    Err(BoseError::AckMismatch)
}

/// `int connect_device(int sock, bdaddr_t address)` in `based.c`.
pub fn connect_device<I: BoseIo>(io: &mut I, address: BdAddr) -> BoseResult<()> {
    let mut send = [0u8; BYTES_POSITION_11];
    send[..5].copy_from_slice(&CONNECT_DEVICE_SEND_PREFIX);
    send[BYTES_POSITION_5..BYTES_POSITION_5 + BT_ADDR_LEN].copy_from_slice(&address.b);
    write_paired(io, &send, &address.b)
}

/// `int disconnect_device(int sock, bdaddr_t address)` in `based.c`.
pub fn disconnect_device<I: BoseIo>(io: &mut I, address: BdAddr) -> BoseResult<()> {
    let mut send = [0u8; BYTES_POSITION_10];
    send[..4].copy_from_slice(&DISCONNECT_DEVICE_SEND_PREFIX);
    send[BYTES_POSITION_4..BYTES_POSITION_4 + BT_ADDR_LEN].copy_from_slice(&address.b);
    write_paired(io, &send, &address.b)
}

/// `int remove_device(int sock, bdaddr_t address)` in `based.c`.
pub fn remove_device<I: BoseIo>(io: &mut I, address: BdAddr) -> BoseResult<()> {
    let mut send = [0u8; BYTES_POSITION_10];
    send[..4].copy_from_slice(&REMOVE_DEVICE_SEND_PREFIX);
    send[BYTES_POSITION_4..BYTES_POSITION_4 + BT_ADDR_LEN].copy_from_slice(&address.b);
    write_paired(io, &send, &address.b)
}

/// Whether the QC Ultra announces the battery level at power-on (the
/// last byte of its 7-byte `01 03` prompt-language payload). Devices
/// with the 1-byte layout have no such setting:
/// [`BoseError::InvalidArgument`].
pub fn get_battery_prompt<I: BoseIo>(io: &mut I) -> BoseResult<bool> {
    io.bose_send(&GET_PROMPT_LANGUAGE_SEND)?;
    wide_prompt_payload(read_response(io, 0x01, 0x03)?)
        .map(|p| p[PROMPT_LANGUAGE_WIDE_LEN - 1] != 0)
}

/// Turn the QC Ultra's battery-level prompt at power-on on or off,
/// keeping the language byte: `01 03 02 02 <language byte> <on>`
/// (verified on hardware, firmware 1.6.7; the 7-byte write is ignored
/// for this byte).
pub fn set_battery_prompt<I: BoseIo>(io: &mut I, on: bool) -> BoseResult<()> {
    io.bose_send(&GET_PROMPT_LANGUAGE_SEND)?;
    let language = wide_prompt_payload(read_response(io, 0x01, 0x03)?)?[0];

    let mut send = [0u8; 6];
    send[..4].copy_from_slice(&SET_BATTERY_PROMPT_SEND_PREFIX);
    send[4] = language;
    send[5] = u8::from(on);
    io.bose_send(&send)?;

    let got =
        wide_prompt_payload(read_response(io, 0x01, 0x03)?)?[PROMPT_LANGUAGE_WIDE_LEN - 1] != 0;
    if got != on {
        return Err(unconfirmed(on, got));
    }
    Ok(())
}

/// The 7-byte QC Ultra prompt-language payload, or
/// [`BoseError::InvalidArgument`] for the 1-byte layout.
fn wide_prompt_payload(payload: Vec<u8>) -> BoseResult<Vec<u8>> {
    match payload.len() {
        PROMPT_LANGUAGE_WIDE_LEN => Ok(payload),
        1 => Err(BoseError::InvalidArgument(
            "this device has no battery-prompt setting".to_string(),
        )),
        _ => Err(BoseError::AckMismatch),
    }
}

/// Equalizer bands (QC Ultra, `01 07`), as `min max value band` per
/// band: `01 07 03 0c f6 0a 00 00 f6 0a 00 01 f6 0a 00 02` is bass, mid
/// and treble at 0, each in `-10..=10`.
pub fn get_equalizer<I: BoseIo>(io: &mut I) -> BoseResult<Vec<EqualizerBand>> {
    io.bose_send(&GET_EQUALIZER_SEND)?;
    parse_equalizer(&read_response(io, 0x01, 0x07)?)
}

/// Set one equalizer band (QC Ultra): `01 07 02 02 <value> <band>`.
///
/// The value is checked against the range the device reports first.
/// On firmware 1.6.7 an out-of-range value is answered with ERROR `06`
/// and then every equalizer request fails with ERROR `0c` until the
/// headphones are switched off and on (captured 2026-10-07), so it is
/// never sent.
pub fn set_equalizer_band<I: BoseIo>(io: &mut I, band: EqBand, value: i8) -> BoseResult<()> {
    let current = get_equalizer(io)?;
    let range = current
        .iter()
        .find(|b| b.band == band)
        .ok_or_else(|| BoseError::InvalidArgument(format!("this device has no {band:?} band")))?;
    if !(range.min..=range.max).contains(&value) {
        return Err(BoseError::InvalidArgument(format!(
            "{band:?} must be in {}..={}, got {value}",
            range.min, range.max
        )));
    }

    let mut send = [0u8; 6];
    send[..4].copy_from_slice(&SET_EQUALIZER_SEND_PREFIX);
    send[4] = value as u8;
    send[5] = band as u8;
    io.bose_send(&send)?;

    let after = parse_equalizer(&read_response(io, 0x01, 0x07)?)?;
    match after.iter().find(|b| b.band == band) {
        Some(b) if b.value == value => Ok(()),
        Some(b) => Err(unconfirmed(value, b.value)),
        None => Err(BoseError::AckMismatch),
    }
}

fn parse_equalizer(payload: &[u8]) -> BoseResult<Vec<EqualizerBand>> {
    if payload.is_empty() || payload.len() % 4 != 0 {
        return Err(BoseError::AckMismatch);
    }
    payload
        .chunks_exact(4)
        .map(|c| {
            Ok(EqualizerBand {
                band: EqBand::from_u8(c[3]).ok_or(BoseError::AckMismatch)?,
                min: c[0] as i8,
                max: c[1] as i8,
                value: c[2] as i8,
            })
        })
        .collect()
}

/// Immersive audio (QC Ultra, `05 0f`): `00` off, `01` still, `02`
/// motion.
pub fn get_immersive_audio<I: BoseIo>(io: &mut I) -> BoseResult<ImmersiveAudio> {
    ImmersiveAudio::from_u8(get_byte(io, IMMERSIVE_AUDIO)?).ok_or(BoseError::AckMismatch)
}

/// Set immersive audio (QC Ultra): `05 0f 02 01 xx`. Verified on
/// hardware: the app follows and the sound changes. Any value other
/// than the audio mode's own makes the current mode read `ff`.
pub fn set_immersive_audio<I: BoseIo>(io: &mut I, setting: ImmersiveAudio) -> BoseResult<()> {
    set_byte(io, IMMERSIVE_AUDIO, setting as u8)
}

/// Whether playback pauses when the headphones are taken off and resumes
/// when they are put on (QC Ultra, `01 18`).
pub fn get_auto_play_pause<I: BoseIo>(io: &mut I) -> BoseResult<bool> {
    Ok(get_byte(io, AUTO_PLAY_PAUSE)? != 0)
}

/// Turn automatic play/pause on or off (QC Ultra): `01 18 02 01 xx`.
pub fn set_auto_play_pause<I: BoseIo>(io: &mut I, on: bool) -> BoseResult<()> {
    set_byte(io, AUTO_PLAY_PAUSE, u8::from(on))
}

/// Whether a call is answered when the headphones are put on (QC
/// Ultra, `01 1b`).
pub fn get_auto_answer_calls<I: BoseIo>(io: &mut I) -> BoseResult<bool> {
    Ok(get_byte(io, AUTO_ANSWER_CALLS)? != 0)
}

/// Turn automatic call answering on or off (QC Ultra): `01 1b 02 01 xx`.
pub fn set_auto_answer_calls<I: BoseIo>(io: &mut I, on: bool) -> BoseResult<()> {
    set_byte(io, AUTO_ANSWER_CALLS, u8::from(on))
}

/// Whether the headphones start in the last audio mode used (QC Ultra,
/// QC45, `1f 05`, "Remember my mode").
pub fn get_remember_audio_mode<I: BoseIo>(io: &mut I) -> BoseResult<bool> {
    Ok(get_byte(io, REMEMBER_AUDIO_MODE)? != 0)
}

/// Turn "remember my mode" on or off (QC Ultra, QC45): `1f 05 02 01 xx`.
pub fn set_remember_audio_mode<I: BoseIo>(io: &mut I, on: bool) -> BoseResult<()> {
    set_byte(io, REMEMBER_AUDIO_MODE, u8::from(on))
}

/// Shortcut action (QC Ultra, `01 09`): reply
/// `01 09 03 07 80 09 <action> 00 09 40 0a`; the last 4 bytes are
/// unknown and constant.
pub fn get_shortcut<I: BoseIo>(io: &mut I) -> BoseResult<ShortcutAction> {
    io.bose_send(&GET_SHORTCUT_SEND)?;
    parse_shortcut(&read_response(io, 0x01, 0x09)?)
}

/// Set the shortcut action (QC Ultra): `01 09 02 03 80 09 <action>`.
/// Verified on hardware for [`ShortcutAction::ImmersiveAudio`]: the
/// app follows and the button does it. Writing
/// [`ShortcutAction::Off`] has not been tried from a laptop yet.
pub fn set_shortcut<I: BoseIo>(io: &mut I, action: ShortcutAction) -> BoseResult<()> {
    let mut send = [0u8; 7];
    send[..4].copy_from_slice(&SET_SHORTCUT_SEND_PREFIX);
    send[4..6].copy_from_slice(&SHORTCUT_BUTTON_EVENT);
    send[6] = action as u8;
    io.bose_send(&send)?;
    let got = parse_shortcut(&read_response(io, 0x01, 0x09)?)?;
    if got != action {
        return Err(unconfirmed(format!("{action:?}"), format!("{got:?}")));
    }
    Ok(())
}

fn parse_shortcut(payload: &[u8]) -> BoseResult<ShortcutAction> {
    match *payload {
        [0x80, 0x09, action, ..] => ShortcutAction::from_u8(action).ok_or(BoseError::AckMismatch),
        _ => Err(BoseError::AckMismatch),
    }
}

/// GET a 1-byte setting: `block function 01 00`.
fn get_byte<I: BoseIo>(io: &mut I, (block, function): (u8, u8)) -> BoseResult<u8> {
    io.bose_send(&[block, function, 0x01, 0x00])?;
    first_byte(&read_response(io, block, function)?)
}

/// SET a 1-byte setting, `block function 02 01 value`, and check the
/// echo.
fn set_byte<I: BoseIo>(io: &mut I, (block, function): (u8, u8), value: u8) -> BoseResult<()> {
    io.bose_send(&[block, function, 0x02, 0x01, value])?;
    let got = first_byte(&read_response(io, block, function)?)?;
    if got != value {
        return Err(unconfirmed(value, got));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Private helpers. Mirror the static C functions in `based.c`, except
// `write_paired` and the reply readers (`read_first_byte` and below),
// which have no C counterpart.
// ---------------------------------------------------------------------------

/// How long [`send_media_key`] waits for its RESULT.
const MEDIA_KEY_RESULT_TIMEOUT: Duration = Duration::from_secs(5);

/// How long [`write_paired`] waits for the final RESULT of a
/// paired-device command. Reconnecting a phone to a SoundLink Color II
/// or a QC35 II took up to ~8 s between the PROCESSING packet and the
/// RESULT (captured 2026-10-07).
const PAIRED_RESULT_TIMEOUT: Duration = Duration::from_secs(15);

/// Send a paired-device packet (`connect_device`, `disconnect_device`,
/// `remove_device`) and wait for its final RESULT.
///
/// Observed replies (`<addr>` is the requested address):
///
/// | Command    | Device                | Reply                                                                  |
/// | ---------- | --------------------- | ---------------------------------------------------------------------- |
/// | connect    | SLC II 4.0.1, QC35 II | `04 01 07 06 <addr>`, then after up to ~8 s one or more `04 01 06 07 <addr> cc` (`cc` = `01`, `05`, `07`) |
/// | connect    | already connected     | `04 01 07 06 <addr>`, `04 01 06 07 <addr> 07` at once                  |
/// | connect    | not a paired source   | `04 01 04 07 0b <addr>` (ERROR `0b`)                                   |
/// | connect    | QC Ultra, own address | `04 01 07 06 <addr>`, later `04 01 04 07 0d <addr>` (ERROR `0d`)       |
/// | disconnect | SLC II 4.0.1          | `04 02 07 00`, `04 02 07 06 <addr>`, then `04 02 06 06 <addr>`         |
/// | disconnect | QC35 II 4.8.1         | `04 02 07 07 21 <addr>`, then `04 02 06 06 <addr>`                     |
/// | remove     | C code                | `04 03 06 06 <addr>`                                                   |
/// | remove     | SLC II 4.0.1          | nothing at all                                                         |
///
/// Notifications of other functions (`05 01` active source, …) arrive
/// in between and are skipped.
///
/// Rules:
///
/// 1. No byte at all before the receive timeout or end of stream:
///    success (SLC II `remove_device`).
/// 2. A RESULT for this function whose payload starts with `address`:
///    success.
/// 3. An ERROR for this function: [`BoseError::DeviceError`].
/// 4. Anything else is read past until [`PAIRED_RESULT_TIMEOUT`]; then
///    a timed-out [`BoseError::Io`]. End of stream after the first
///    byte is [`BoseError::ShortRead`].
pub(crate) fn write_paired<I: BoseIo>(
    io: &mut I,
    send: &[u8],
    address: &[u8; 6],
) -> BoseResult<()> {
    io.bose_send(send)?;
    let (block, function) = (send[0], send[1]);

    let first = match read_first_byte(io)? {
        FirstByte::Byte(byte) => byte,
        FirstByte::Timeout | FirstByte::Closed => return Ok(()),
    };
    let deadline = Instant::now() + PAIRED_RESULT_TIMEOUT;
    let (mut header, mut payload) = read_packet_from(io, first)?;
    loop {
        if let Some((OP_RESULT, result)) = check_reply(block, function, header, payload)? {
            if result.starts_with(address) {
                return Ok(());
            }
        }
        (header, payload) = read_packet_before(io, deadline)?;
    }
}

/// `static int masked_memory_cmp(const uint8_t *ptr1, uint8_t *ptr2,
/// size_t num, const uint8_t *mask)` in `based.c`. Returns 0 on
/// equality, otherwise the difference of the first mismatching
/// masked bytes.
#[allow(dead_code)]
pub(crate) fn masked_memory_cmp(a: &[u8], b: &[u8], mask: &[u8]) -> i32 {
    for ((a_byte, b_byte), mask_byte) in a.iter().zip(b.iter()).zip(mask.iter()) {
        let a_masked = a_byte & mask_byte;
        let b_masked = b_byte & mask_byte;
        if a_masked != b_masked {
            return (a_masked as i32) - (b_masked as i32);
        }
    }
    0
}

#[allow(dead_code)]
fn read_check<I: BoseIo>(io: &mut I, ack: &[u8], mask: Option<&[u8]>) -> BoseResult<()> {
    let mut got = vec![0u8; ack.len()];
    io.bose_read_exact(&mut got)?;
    let cmp = match mask {
        Some(m) => masked_memory_cmp(ack, &got, m),
        None => {
            // Replicate `memcmp` semantics: 0 if equal, otherwise
            // the difference of the first mismatching bytes.
            for (a, b) in ack.iter().zip(got.iter()) {
                if a != b {
                    return Err(BoseError::AckMismatch);
                }
            }
            0
        }
    };
    if cmp != 0 {
        Err(BoseError::AckMismatch)
    } else {
        Ok(())
    }
}

/// Most packets [`read_response`] reads past (notifications of other
/// functions, PROCESSING packets) before it gives up on a reply.
const MAX_UNSOLICITED_PACKETS: usize = 64;

/// Most packets [`get_device_status`] accepts in one status dump. The
/// QC Ultra sends 14.
const MAX_STATUS_DUMP_PACKETS: usize = 64;

/// The first byte of a reply, or why there is none.
enum FirstByte {
    /// A byte arrived.
    Byte(u8),
    /// The receive timeout expired (`WouldBlock` / `TimedOut`).
    Timeout,
    /// End of stream.
    Closed,
}

/// Read the first byte of a reply without turning a receive timeout
/// into an error.
fn read_first_byte<I: BoseIo>(io: &mut I) -> BoseResult<FirstByte> {
    let mut byte = [0u8; 1];
    loop {
        return match std::io::Read::read(io, &mut byte) {
            Ok(0) => Ok(FirstByte::Closed),
            Ok(_) => Ok(FirstByte::Byte(byte[0])),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                Ok(FirstByte::Timeout)
            }
            Err(e) => Err(e.into()),
        };
    }
}

/// Read one packet: the 4-byte header (`block, function, operator,
/// length`) followed by `length` payload bytes.
fn read_packet<I: BoseIo>(io: &mut I) -> BoseResult<([u8; 4], Vec<u8>)> {
    let mut first = [0u8; 1];
    io.bose_read_exact(&mut first)?;
    read_packet_from(io, first[0])
}

/// Same as [`read_packet`], when the first header byte (`first`) was
/// already read.
fn read_packet_from<I: BoseIo>(io: &mut I, first: u8) -> BoseResult<([u8; 4], Vec<u8>)> {
    let mut header = [first, 0, 0, 0];
    io.bose_read_exact(&mut header[1..])?;
    let mut payload = vec![0u8; usize::from(header[BYTES_POSITION_3])];
    io.bose_read_exact(&mut payload)?;
    Ok((header, payload))
}

/// Same as [`read_packet`], but waits through receive timeouts until
/// `deadline` for the packet to start.
fn read_packet_before<I: BoseIo>(io: &mut I, deadline: Instant) -> BoseResult<([u8; 4], Vec<u8>)> {
    loop {
        match read_first_byte(io)? {
            FirstByte::Byte(first) => return read_packet_from(io, first),
            FirstByte::Closed => {
                return Err(BoseError::ShortRead {
                    expected: CN_BASE_PACK_LEN,
                    actual: 0,
                })
            }
            FirstByte::Timeout if Instant::now() >= deadline => {
                return Err(BoseError::Io(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "the device sent no final result",
                )))
            }
            FirstByte::Timeout => {}
        }
    }
}

/// Check one packet (`header`, `payload`) against the request's
/// `block` and `function`.
///
/// * A packet for another `block.function`: `Ok(None)`. Devices send
///   notifications nobody asked for (a second volume reply, the
///   active source, track metadata); they are not the reply.
/// * An ERROR packet: [`BoseError::DeviceError`], with the first payload
///   byte as the code (`0` if the payload is empty).
/// * Otherwise the operator and payload, for the caller to judge.
fn check_reply(
    block: u8,
    function: u8,
    header: [u8; 4],
    payload: Vec<u8>,
) -> BoseResult<Option<(u8, Vec<u8>)>> {
    if header[0] != block || header[1] != function {
        return Ok(None);
    }
    if header[BYTES_POSITION_2] == OP_ERROR {
        return Err(BoseError::DeviceError {
            block,
            function,
            code: payload.first().copied().unwrap_or(0),
        });
    }
    Ok(Some((header[BYTES_POSITION_2], payload)))
}

/// Read the device's reply to a `block.function` request and return
/// its payload, whatever its length. Notifications of other functions
/// and PROCESSING packets are skipped, up to
/// [`MAX_UNSOLICITED_PACKETS`]; an ERROR packet becomes
/// [`BoseError::DeviceError`]. Every packet is consumed whole, so the
/// stream stays in sync for the next request on the same connection.
fn read_response<I: BoseIo>(io: &mut I, block: u8, function: u8) -> BoseResult<Vec<u8>> {
    for _ in 0..MAX_UNSOLICITED_PACKETS {
        let (header, payload) = read_packet(io)?;
        match check_reply(block, function, header, payload)? {
            Some((OP_STATUS | OP_RESULT, payload)) => return Ok(payload),
            None | Some((OP_PROCESSING, _)) => continue,
            Some(_) => return Err(BoseError::AckMismatch),
        }
    }
    Err(BoseError::AckMismatch)
}

/// Parse a 6-byte MSB-first Bluetooth address.
fn bd_addr(bytes: &[u8]) -> BoseResult<BdAddr> {
    let b = bytes.try_into().map_err(|_| BoseError::AckMismatch)?;
    Ok(BdAddr { b })
}

fn first_byte(payload: &[u8]) -> BoseResult<u8> {
    payload.first().copied().ok_or(BoseError::AckMismatch)
}

/// Name payload: a `0x00` byte followed by the name.
fn parse_name(payload: &[u8]) -> BoseResult<String> {
    match payload.split_first() {
        Some((0x00, name)) => Ok(String::from_utf8_lossy(name).into_owned()),
        _ => Err(BoseError::AckMismatch),
    }
}

/// Auto-off payload in minutes: 1 byte on the QC35, `lo 00 hi` on the
/// QC Ultra. `None` for any other layout.
fn parse_auto_off(payload: &[u8]) -> Option<u16> {
    match *payload {
        [minutes] => Some(u16::from(minutes)),
        [lo, 0x00, hi] => Some(u16::from_le_bytes([lo, hi])),
        _ => None,
    }
}

fn parse_noise_cancelling(payload: &[u8]) -> BoseResult<NoiseCancelling> {
    let byte = first_byte(payload)?;
    NoiseCancelling::from_u8(byte).ok_or(BoseError::InvalidArgument(format!(
        "noise cancelling byte 0x{:02x} out of range",
        byte
    )))
}

fn get_name<I: BoseIo>(io: &mut I) -> BoseResult<String> {
    parse_name(&read_response(io, 0x01, 0x02)?)
}

fn get_prompt_language<I: BoseIo>(io: &mut I) -> BoseResult<u8> {
    first_byte(&read_response(io, 0x01, 0x03)?)
}

fn get_auto_off<I: BoseIo>(io: &mut I) -> BoseResult<Option<u16>> {
    Ok(parse_auto_off(&read_response(io, 0x01, 0x04)?))
}

fn get_noise_cancelling<I: BoseIo>(io: &mut I) -> BoseResult<NoiseCancelling> {
    parse_noise_cancelling(&read_response(io, 0x01, 0x06)?)
}

// ---------------------------------------------------------------------------
// Parse helpers. Mirrors `get_language()` in `main.c` — this lives in
// the CLI crate because the mapping is CLI-only; the protocol itself
// takes raw bytes (see [`PromptLanguage`]).
// ---------------------------------------------------------------------------

/// Map a 2-letter CLI argument (`"en"`, `"fr"`, …) to the on-wire
/// byte. Returns `None` for unknown codes.
pub fn language_from_arg(s: &str) -> Option<PromptLanguage> {
    PromptLanguage::from_arg(s)
}

// Suppress "unused" warnings on items we keep for documentation parity
// with the C source.
#[allow(dead_code)]
const _: usize = MAX_SERIAL_SIZE_T;

#[cfg(test)]
mod tests {
    use super::*;

    /// `masked_memory_cmp` should treat masked-off bytes as equal.
    #[test]
    fn masked_cmp_ignores_dont_care_bits() {
        let a = [0x01, 0x02, 0x03];
        let b = [0x01, 0xff, 0x03];
        let mask = [0xff, 0x00, 0xff]; // middle byte is "don't care"
        assert_eq!(masked_memory_cmp(&a, &b, &mask), 0);
    }

    /// Non-zero on the first differing masked byte.
    #[test]
    fn masked_cmp_reports_difference() {
        let a = [0x01, 0x02, 0x03];
        let b = [0x01, 0x03, 0x03];
        let mask = [0xff, 0xff, 0xff];
        // 0x02 - 0x03 = -1
        assert_eq!(masked_memory_cmp(&a, &b, &mask), -1);
    }

    /// Empty slices compare equal.
    #[test]
    fn masked_cmp_empty() {
        assert_eq!(masked_memory_cmp(&[], &[], &[]), 0);
    }

    /// `has_noise_cancelling` knows the three Bose device IDs.
    #[test]
    fn nc_device_ids() {
        assert!(has_noise_cancelling(0x4014));
        assert!(has_noise_cancelling(0x4020));
        assert!(has_noise_cancelling(0x400c));
        assert!(!has_noise_cancelling(0x1234));
        assert!(!has_noise_cancelling(0));
    }

    /// Audio modes on the QC Ultra and the QC45; the other QC Ultra
    /// settings on the QC Ultra only.
    #[test]
    fn audio_mode_device_ids() {
        assert!(has_audio_modes(0x4066));
        assert!(has_audio_modes(0x4039));
        assert!(!has_audio_modes(0x4014));
        assert!(has_qc_ultra_settings(0x4066));
        assert!(!has_qc_ultra_settings(0x4039));
        assert!(!has_qc_ultra_settings(0x4014));
    }

    // ========================================================================
    // Media / volume / audio-routing command tests. Bytes are taken from
    // live captures against two SoundLink Color II speakers and from
    // DEVELOPMENT.md.
    //
    // These tests are hermetic: they use `UnixStream` to drive the
    // protocol side and verify the byte sequence is correct *regardless*
    // of whether the real Bose firmware still emits the same bytes in
    // the future. If a future firmware revision changes a header byte
    // the test will fail loudly, which is the desired signal.
    // ========================================================================

    mod media_volume_tests {
        // The closures below look like `|c| get_active_device(c)`,
        // which clippy flags as a redundant closure. They aren't —
        // the closure is required to fix the generic `BoseIo` type
        // parameter to `UnixStream`. Suppress the lint for this
        // module.
        #![allow(clippy::redundant_closure)]

        use super::*;
        use crate::types::MediaKey;
        use std::io::{Read, Write};
        use std::os::unix::net::UnixStream;
        use std::sync::mpsc;
        use std::thread;
        use std::time::{Duration, Instant};

        /// Drive `client_op` on a fresh `UnixStream` pair, with
        /// `device_side` simulating the Bose device in a background
        /// thread. `device_side` runs with a 5-second hard timeout so
        /// a buggy client can't hang the test forever.
        fn drive<F, G, R>(device_side: G, client_op: F) -> R
        where
            F: FnOnce(&mut UnixStream) -> R + Send + 'static,
            G: FnOnce(&mut UnixStream) + Send + 'static,
            R: Send + 'static,
        {
            let (mut client, mut server) = UnixStream::pair().expect("UnixStream::pair");
            let (tx, rx) = mpsc::channel();

            let server_handle = thread::spawn(move || {
                let _result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    device_side(&mut server);
                }));
                let _ = tx.send(());
            });

            let client_result = client_op(&mut client);
            drop(client);

            // Give the server thread up to 5 s to finish naturally.
            // If it doesn't, just leave it; the OS will reap it when
            // the test process exits. Don't `join()` — that would
            // mask a buggy protocol function that hangs.
            let _ = rx.recv_timeout(Duration::from_secs(5));
            let _ = server_handle.join();
            client_result
        }

        /// Drain whatever the client sent, then write `reply` back.
        /// The drain reads up to 64 bytes, which is enough for every
        /// command in this module (longest send is 5 bytes).
        fn reply(server: &mut UnixStream, reply: &[u8]) {
            let mut buf = [0u8; 64];
            let _ = server.read(&mut buf);
            if !reply.is_empty() {
                server.write_all(reply).unwrap();
                server.flush().ok();
            }
        }

        // ---- set_volume ----

        #[test]
        fn set_volume_succeeds_when_device_echoes_level() {
            // Live capture: sent `05 05 02 01 0a`, received
            // `05 05 03 02 64 0a` (scale max 100, volume 10).
            let result = drive(
                move |s| reply(s, &[0x05, 0x05, 0x03, 0x02, 0x64, 0x0a]),
                move |c| set_volume(c, 0x0a),
            );
            let max = result.expect("set_volume failed");
            assert_eq!(max, 100, "expected scale max 100, got {max}");
        }

        #[test]
        fn set_volume_succeeds_at_volume_zero() {
            // Live capture: sent `05 05 02 01 00`, received
            // `05 05 03 02 64 00` (scale max 100, volume 0 / muted).
            let result = drive(
                move |s| reply(s, &[0x05, 0x05, 0x03, 0x02, 0x64, 0x00]),
                move |c| set_volume(c, 0x00),
            );
            let max = result.expect("set_volume failed");
            assert_eq!(max, 100, "expected scale max 100, got {max}");
        }

        #[test]
        fn set_volume_succeeds_at_max_level() {
            // Live capture on a SoundLink Color II: 99 is the highest
            // level it accepts (`05 05 03 02 64 63`).
            let result = drive(
                move |s| reply(s, &[0x05, 0x05, 0x03, 0x02, 0x64, 0x63]),
                move |c| set_volume(c, 0x63),
            );
            let max = result.expect("set_volume failed");
            assert_eq!(max, 100, "expected scale max 100, got {max}");
        }

        #[test]
        fn set_volume_skips_a_packet_of_another_function() {
            // A notification of another function is not the reply.
            let result = drive(
                move |s| {
                    reply(
                        s,
                        &[
                            0x06, 0x05, 0x03, 0x02, 0x64, 0x0a, 0x05, 0x05, 0x03, 0x02, 0x64, 0x0a,
                        ],
                    )
                },
                move |c| set_volume(c, 0x0a),
            );
            assert_eq!(result.expect("set_volume"), 100);
        }

        #[test]
        fn set_volume_rejects_echo_mismatch() {
            // Reply echoes volume 5 even though we asked for 10.
            let result = drive(
                move |s| reply(s, &[0x05, 0x05, 0x03, 0x02, 0x64, 0x05]),
                move |c| set_volume(c, 0x0a),
            );
            // The echo mismatch surfaces as `unconfirmed(...)`, not
            // `AckMismatch` — verify it's *some* error.
            assert!(result.is_err(), "expected error, got {:?}", result);
        }

        #[test]
        fn set_volume_out_of_range_is_device_error_and_stream_stays_in_sync() {
            // Live capture on a SoundLink Color II: `05 05 02 01 80`
            // is answered with the ERROR packet `05 05 04 01 06`. The
            // error payload must be consumed so that the next request
            // on the same connection reads its own reply.
            let result = drive(
                move |s| {
                    reply(s, &[0x05, 0x05, 0x04, 0x01, 0x06]);
                    reply(s, &[0x05, 0x05, 0x03, 0x02, 0x64, 0x14]);
                },
                move |c| (set_volume(c, 0x80), set_volume(c, 0x14)),
            );
            assert!(
                matches!(
                    result.0,
                    Err(BoseError::DeviceError {
                        block: 0x05,
                        function: 0x05,
                        code: 0x06
                    })
                ),
                "got {:?}",
                result.0
            );
            assert_eq!(result.1.expect("second set_volume failed"), 100);
        }

        // ---- send_media_key ----

        #[test]
        fn send_media_key_pause() {
            // Live capture for `05 03 05 01 01` (toggle play/pause):
            // two-packet ACK. Verified live on both White and Black
            // speakers — sending the toggle while paused resumes,
            // sending it while playing pauses.
            let result = drive(
                move |s| reply(s, &[0x05, 0x03, 0x07, 0x00, 0x05, 0x03, 0x06, 0x00]),
                move |c| send_media_key(c, MediaKey::Pause),
            );
            assert!(result.is_ok(), "expected Ok, got {:?}", result);
        }

        #[test]
        fn send_media_key_next() {
            let result = drive(
                move |s| reply(s, &[0x05, 0x03, 0x07, 0x00, 0x05, 0x03, 0x06, 0x00]),
                move |c| send_media_key(c, MediaKey::Next),
            );
            assert!(result.is_ok(), "expected Ok, got {:?}", result);
        }

        #[test]
        fn send_media_key_previous() {
            let result = drive(
                move |s| reply(s, &[0x05, 0x03, 0x07, 0x00, 0x05, 0x03, 0x06, 0x00]),
                move |c| send_media_key(c, MediaKey::Previous),
            );
            assert!(result.is_ok(), "expected Ok, got {:?}", result);
        }

        #[test]
        fn send_media_key_accepts_result_without_processing() {
            // The PROCESSING packet is optional, as for every reply
            // read from its header.
            let result = drive(
                move |s| reply(s, &[0x05, 0x03, 0x06, 0x00]),
                move |c| send_media_key(c, MediaKey::Pause),
            );
            assert!(result.is_ok(), "expected Ok, got {:?}", result);
        }

        #[test]
        fn send_media_key_without_a_result_is_an_error() {
            // PROCESSING, a notification of another function, then the
            // stream ends: no RESULT, so no success.
            let result = drive(
                move |s| reply(s, &[0x05, 0x03, 0x07, 0x00, 0x05, 0x04, 0x06, 0x00]),
                move |c| send_media_key(c, MediaKey::Pause),
            );
            assert!(
                matches!(result, Err(BoseError::ShortRead { .. })),
                "got {result:?}"
            );
        }

        // ---- get_active_device ----

        #[test]
        fn get_active_device_returns_bt_address() {
            // Live capture: sent `05 01 01 00`, received
            // `05 01 03 09 00 02 01 d4 6d 6d 17 4d a4`
            // (active source = D4:6D:6D:17:4D:A4).
            let result = drive(
                move |s| {
                    reply(
                        s,
                        &[
                            0x05, 0x01, 0x03, 0x09, 0x00, 0x02, 0x01, 0xd4, 0x6d, 0x6d, 0x17, 0x4d,
                            0xa4,
                        ],
                    )
                },
                move |c| get_active_device(c),
            );
            let addr = result.expect("get_active_device failed");
            assert_eq!(
                addr,
                BdAddr {
                    b: [0xd4, 0x6d, 0x6d, 0x17, 0x4d, 0xa4]
                },
                "got {addr:?}"
            );
        }

        #[test]
        fn get_active_device_without_its_reply_is_an_error() {
            let result = drive(
                move |s| {
                    reply(
                        s,
                        &[
                            0x05, 0x02, 0x03, 0x09, 0x00, 0x02, 0x01, 0xd4, 0x6d, 0x6d, 0x17, 0x4d,
                            0xa4,
                        ],
                    )
                },
                move |c| get_active_device(c),
            );
            assert!(
                matches!(result, Err(BoseError::ShortRead { .. })),
                "got {result:?}"
            );
        }

        // ---- get_device_bd_addr ----

        #[test]
        fn get_device_bd_addr_returns_speakers_own_address() {
            // Live capture from the White speaker: sent `00 06 01 00`,
            // received `00 06 03 06 04 52 c7 ba 68 0d`
            // (own address = 04:52:C7:BA:68:0D).
            let result = drive(
                move |s| {
                    reply(
                        s,
                        &[0x00, 0x06, 0x03, 0x06, 0x04, 0x52, 0xc7, 0xba, 0x68, 0x0d],
                    )
                },
                move |c| get_device_bd_addr(c),
            );
            let addr = result.expect("get_device_bd_addr failed");
            assert_eq!(
                addr,
                BdAddr {
                    b: [0x04, 0x52, 0xc7, 0xba, 0x68, 0x0d]
                },
                "got {addr:?}"
            );
        }

        #[test]
        fn get_device_bd_addr_without_its_reply_is_an_error() {
            let result = drive(
                move |s| {
                    reply(
                        s,
                        &[0x00, 0x07, 0x03, 0x06, 0x04, 0x52, 0xc7, 0xba, 0x68, 0x0d],
                    )
                },
                move |c| get_device_bd_addr(c),
            );
            assert!(
                matches!(result, Err(BoseError::ShortRead { .. })),
                "got {result:?}"
            );
        }

        /// Sanity-check: all four new command functions must return
        /// promptly. If this test takes more than 1 second, the read
        /// loop in any of the new functions has regressed to the bug
        /// fixed in `tests/send_packet_timeout.rs`.
        #[test]
        fn all_commands_return_promptly() {
            const BUDGET: Duration = Duration::from_millis(500);

            let start = Instant::now();
            let _ = drive(
                move |s| reply(s, &[0x05, 0x05, 0x03, 0x02, 0x64, 0x0a]),
                move |c| set_volume(c, 0x0a),
            );
            assert!(
                start.elapsed() < BUDGET,
                "set_volume took {:?}",
                start.elapsed()
            );

            let start = Instant::now();
            let _ = drive(
                move |s| reply(s, &[0x05, 0x03, 0x07, 0x00, 0x05, 0x03, 0x06, 0x00]),
                move |c| send_media_key(c, MediaKey::Pause),
            );
            assert!(
                start.elapsed() < BUDGET,
                "send_media_key took {:?}",
                start.elapsed()
            );

            let start = Instant::now();
            let _ = drive(
                move |s| {
                    reply(
                        s,
                        &[
                            0x05, 0x01, 0x03, 0x09, 0x00, 0x02, 0x01, 0xd4, 0x6d, 0x6d, 0x17, 0x4d,
                            0xa4,
                        ],
                    )
                },
                move |c| get_active_device(c),
            );
            assert!(
                start.elapsed() < BUDGET,
                "get_active_device took {:?}",
                start.elapsed()
            );

            let start = Instant::now();
            let _ = drive(
                move |s| {
                    reply(
                        s,
                        &[0x00, 0x06, 0x03, 0x06, 0x04, 0x52, 0xc7, 0xba, 0x68, 0x0d],
                    )
                },
                move |c| get_device_bd_addr(c),
            );
            assert!(
                start.elapsed() < BUDGET,
                "get_device_bd_addr took {:?}",
                start.elapsed()
            );
        }
    }
}
