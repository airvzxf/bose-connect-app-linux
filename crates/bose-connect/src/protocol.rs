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

use crate::error::{unconfirmed, BoseError, BoseResult};
use crate::io::BoseIo;
use crate::types::{
    AutoOff, BdAddr, Device, DeviceStatus, DevicesConnected, MediaKey, NoiseCancelling, Pairing,
    PromptLanguage, SelfVoice, MAX_AUDIO_MODES, MAX_NAME_PACK_LEN_HELPER, MAX_NUM_DEVICES,
    NOISE_CANCELLING_0C, NOISE_CANCELLING_14, NOISE_CANCELLING_20, QC_ULTRA_HEADPHONES,
    VP_ENABLE_BIT, VP_MASK,
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
const GET_DEVICE_ID_ACK: [u8; 4] = [0x00, 0x03, 0x03, 0x03];

const SET_NAME_SEND_PREFIX: [u8; 4] = [0x01, 0x02, 0x02, 0x00];

const SET_PROMPT_LANGUAGE_SEND: [u8; 5] = [0x01, 0x03, 0x02, 0x01, 0x00];

const SET_AUTO_OFF_SEND: [u8; 5] = [0x01, 0x04, 0x02, 0x01, 0x00];

const SET_NOISE_CANCELLING_SEND: [u8; 5] = [0x01, 0x06, 0x02, 0x01, 0x00];

const GET_DEVICE_STATUS_SEND: [u8; 4] = [0x01, 0x01, 0x05, 0x00];
const GET_DEVICE_STATUS_ACK: [u8; 4] = [0x01, 0x01, 0x07, 0x00];

const GET_DEVICE_STATUS_FINAL_ACK: [u8; 4] = [0x01, 0x01, 0x06, 0x00];

const GET_FIRMWARE_VERSION_SEND: [u8; 4] = [0x00, 0x05, 0x01, 0x00];

const GET_SERIAL_NUMBER_SEND: [u8; 4] = [0x00, 0x07, 0x01, 0x00];
const GET_SERIAL_NUMBER_ACK: [u8; 3] = [0x00, 0x07, 0x03];

const GET_BATTERY_LEVEL_SEND: [u8; 4] = [0x02, 0x02, 0x01, 0x00];

const GET_PAIRED_DEVICES_SEND: [u8; 4] = [0x04, 0x04, 0x01, 0x00];
const GET_PAIRED_DEVICES_ACK: [u8; 3] = [0x04, 0x04, 0x03];

const SET_PAIRING_SEND: [u8; 5] = [0x04, 0x08, 0x05, 0x01, 0x00];
const SET_PAIRING_ACK: [u8; 5] = [0x04, 0x08, 0x06, 0x01, 0x00];

const SET_SELF_VOICE_SEND: [u8; 7] = [0x01, 0x0b, 0x02, 0x02, 0x01, 0x00, 0x38];
const SET_SELF_VOICE_ACK: [u8; 7] = [0x01, 0x0b, 0x03, 0x03, 0x01, 0x00, 0x0f];

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
// (0x03) / RESULT (0x06) reply; see `read_reply_header`.
// ---------------------------------------------------------------------------

/// `set_volume` request — subsystem 0x05, opcode 0x05, command
/// 0x02 (SET), length 0x01, payload = volume level. The valid range
/// is device-specific and reported by the device itself (see
/// [`SET_VOLUME_ACK_PREFIX`]): `0..=99` on the SoundLink Color II,
/// `0..=24` on the QC35 per DEVELOPMENT.md. Out-of-range levels are
/// rejected with the ERROR packet `05 05 04 01 06`.
const SET_VOLUME_SEND_PREFIX: [u8; 4] = [0x05, 0x05, 0x02, 0x01];

/// Response header for `set_volume`. The full response is 6 bytes:
/// `[0x05, 0x05, 0x03, 0x02, levels, volume_echo]`. `levels` is the
/// number of volume steps the device supports (`0x64` = 100 on the
/// SoundLink Color II, `0x19` = 25 on the QC35), so the highest valid
/// level is `levels - 1`. It is *not* the battery level: verified on
/// hardware by sweeping the level, the byte stayed `0x64` and 100 was
/// the first value rejected.
const SET_VOLUME_ACK_PREFIX: [u8; 4] = [0x05, 0x05, 0x03, 0x02];

/// Media-key request — subsystem 0x05, opcode 0x03, operator 0x05,
/// length 0x01, payload = media key code. Keys:
/// 0x01 play/pause toggle, 0x03 next, 0x04 previous. The play/pause
/// toggle is exposed as `MediaKey::Pause` — see
/// [`crate::types::MediaKey`] for why.
const SEND_MEDIA_KEY_SEND_PREFIX: [u8; 4] = [0x05, 0x03, 0x05, 0x01];

/// First reply packet of `send_media_key`. The full response is 8
/// bytes (two concatenated packets, verified on a SoundLink Color II):
/// `[0x05, 0x03, 0x07, 0x00]` (PROCESSING) followed by
/// `[0x05, 0x03, 0x06, 0x00]` (RESULT).
const SEND_MEDIA_KEY_ACK_PREFIX: [u8; 4] = [0x05, 0x03, 0x07, 0x00];

/// Second reply packet of `send_media_key` (RESULT). Concatenated
/// immediately after the PROCESSING packet on the wire.
const SEND_MEDIA_KEY_FINAL_ACK: [u8; 4] = [0x05, 0x03, 0x06, 0x00];

/// `get_active_device` request — returns the BT address of the
/// source currently feeding the A2DP sink. Subsystem 0x05, opcode
/// 0x01, command 0x01 (GET), length 0x00. Response is 13 bytes;
/// the last 6 bytes are the MSB-first BT address of the source.
const GET_ACTIVE_DEVICE_SEND: [u8; 4] = [0x05, 0x01, 0x01, 0x00];

/// Header of the `get_active_device` response. 9-byte payload
/// follows (length byte = 0x09). The payload structure observed on
/// the live capture is `[0x00, 0x02, 0x01, addr_0..addr_5]`; the
/// first three bytes are an unknown status triple, the last six
/// are the BT address.
const GET_ACTIVE_DEVICE_ACK_PREFIX: [u8; 4] = [0x05, 0x01, 0x03, 0x09];

/// `get_device_bd_addr` — returns the speaker's *own* Bluetooth
/// address. Subsystem 0x00, opcode 0x06, command 0x01 (GET),
/// length 0x00. Response is 10 bytes; the last 6 bytes are the
/// MSB-first BT address of the speaker itself (not the source).
/// Useful for distinguishing which physical speaker a given socket
/// is bound to when the same laptop has two paired speakers (e.g.
/// the test rig with the White + Black Foremans).
const GET_DEVICE_BD_ADDR_SEND: [u8; 4] = [0x00, 0x06, 0x01, 0x00];

/// Header of the `get_device_bd_addr` response. 6-byte payload
/// follows (length byte = 0x06). Payload is the speaker's own BT
/// address in MSB-first order.
const GET_DEVICE_BD_ADDR_ACK_PREFIX: [u8; 4] = [0x00, 0x06, 0x03, 0x06];

const GET_DEVICE_INFO_SEND_PREFIX: [u8; 4] = [0x04, 0x05, 0x01, 6];
const GET_DEVICE_INFO_ACK: [u8; 3] = [0x04, 0x05, 0x03];

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
// "Bose app" generation (QC Ultra Headphones); captured from a real
// QC Ultra Headphones on firmware 1.6.7.
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
/// Immersion, …) in function block `0x1f`. Only the QC Ultra
/// Headphones (`0x4066`) are known so far.
pub fn has_audio_modes(device_id: u16) -> bool {
    matches!(device_id, QC_ULTRA_HEADPHONES)
}

/// `has_legacy_settings` is false for devices whose prompt-language
/// and auto-off payloads do not use the 1-byte QC35 layout. On those
/// devices the `set_prompt_language` / `set_voice_prompts` /
/// `set_auto_off` packets are not known to be correct, so the CLI
/// refuses them instead of writing a guessed value.
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
    io.bose_write_all(send)?;
    let mut buf = vec![0u8; MAX_BT_PACK_LEN_T];
    let n = io.read(&mut buf)?;
    buf.truncate(n);
    Ok(buf)
}

/// `int get_device_id(int sock, unsigned int *device_id, unsigned int *index)`
/// in `based.c`.
pub fn get_device_id<I: BoseIo>(io: &mut I) -> BoseResult<(u16, u8)> {
    io.bose_write_check(&GET_DEVICE_ID_SEND, &GET_DEVICE_ID_ACK)?;
    let mut did_buf = [0u8; 2];
    io.bose_read_exact(&mut did_buf)?;
    // The device emits the device-id as big-endian over the wire;
    // bswap_16 in the original code converts it to host order.
    let device_id = u16::from_be_bytes(did_buf);
    let mut index = [0u8; 1];
    io.bose_read_exact(&mut index)?;
    Ok((device_id, index[0]))
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
    io.bose_write_all(&packet[..send_size])?;

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
    io.bose_write_all(&GET_FIRMWARE_VERSION_SEND)?;
    let version = read_response(io, 0x00, 0x05)?;
    Ok(String::from_utf8_lossy(&version).into_owned())
}

/// `int get_serial_number(int sock, char serial[MAX_SERIAL_SIZE])`
/// in `based.c`. Returns the serial as a UTF-8 / ASCII string.
pub fn get_serial_number<I: BoseIo>(io: &mut I) -> BoseResult<String> {
    io.bose_write_check(&GET_SERIAL_NUMBER_SEND, &GET_SERIAL_NUMBER_ACK)?;
    let mut len_byte = [0u8; 1];
    io.bose_read_exact(&mut len_byte)?;
    let len = len_byte[0] as usize;
    let mut serial = vec![0u8; len];
    io.bose_read_exact(&mut serial)?;
    Ok(String::from_utf8_lossy(&serial).into_owned())
}

/// `int get_battery_level(int sock, unsigned int *level)` in `based.c`.
/// Returns the battery level as a percent (0..=100).
///
/// The QC35 replies with a 1-byte payload; the QC Ultra appends
/// three more bytes (`64 ff ff 00`) that are ignored here.
pub fn get_battery_level<I: BoseIo>(io: &mut I) -> BoseResult<u8> {
    io.bose_write_all(&GET_BATTERY_LEVEL_SEND)?;
    let payload = read_response(io, 0x02, 0x02)?;
    payload.first().copied().ok_or(BoseError::AckMismatch)
}

/// No C counterpart (see the packet-constant comment above). Sets the
/// speaker's local volume level.
///
/// The device responds with `[0x05, 0x05, 0x03, 0x02, levels,
/// volume_echo]`. We verify the volume echo matches `level` and
/// return `levels`, the number of volume steps the device supports
/// (valid levels are `0..levels`). The range is not checked here:
/// an out-of-range level comes back as [`BoseError::DeviceError`].
pub fn set_volume<I: BoseIo>(io: &mut I, level: u8) -> BoseResult<u8> {
    let send = [
        SET_VOLUME_SEND_PREFIX[0],
        SET_VOLUME_SEND_PREFIX[1],
        SET_VOLUME_SEND_PREFIX[2],
        SET_VOLUME_SEND_PREFIX[3],
        level,
    ];
    io.bose_write_all(&send)?;

    let header = read_reply_header(io, &SET_VOLUME_SEND_PREFIX)?;
    if header != SET_VOLUME_ACK_PREFIX {
        return Err(BoseError::AckMismatch);
    }

    let mut payload = [0u8; 2];
    io.bose_read_exact(&mut payload)?;
    // payload[0] = number of volume steps. payload[1] = the volume
    // level the device now has, which should equal `level` on success.
    if payload[1] != level {
        return Err(unconfirmed(level, payload[1]));
    }
    Ok(payload[0])
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
    io.bose_write_all(&send)?;

    let data = read_reply_header(io, &SEND_MEDIA_KEY_SEND_PREFIX)?;
    if data != SEND_MEDIA_KEY_ACK_PREFIX {
        return Err(BoseError::AckMismatch);
    }

    let mut final_ack = [0u8; 4];
    io.bose_read_exact(&mut final_ack)?;
    if final_ack != SEND_MEDIA_KEY_FINAL_ACK {
        return Err(BoseError::AckMismatch);
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
    io.bose_write_all(&GET_ACTIVE_DEVICE_SEND)?;

    let mut header = [0u8; 4];
    io.bose_read_exact(&mut header)?;
    if header != GET_ACTIVE_DEVICE_ACK_PREFIX {
        return Err(BoseError::AckMismatch);
    }

    // Skip the 3-byte status triple. The Bose firmware emits these
    // unconditionally; their meaning is unknown but they are stable
    // across the observed SLC II captures (`0x00, 0x02, 0x01`).
    let mut status = [0u8; 3];
    io.bose_read_exact(&mut status)?;

    let mut addr = BdAddr { b: [0; 6] };
    io.bose_read_exact(&mut addr.b)?;
    Ok(addr)
}

/// No C counterpart (see the packet-constant comment above). Returns the speaker's
/// *own* Bluetooth address, regardless of whether an audio source
/// is connected.
///
/// The response is 10 bytes: `[0x00, 0x06, 0x03, 0x06, addr_0,
/// addr_1, addr_2, addr_3, addr_4, addr_5]`. The payload is just
/// the 6-byte MSB-first BT address.
pub fn get_device_bd_addr<I: BoseIo>(io: &mut I) -> BoseResult<BdAddr> {
    io.bose_write_all(&GET_DEVICE_BD_ADDR_SEND)?;

    let mut header = [0u8; 4];
    io.bose_read_exact(&mut header)?;
    if header != GET_DEVICE_BD_ADDR_ACK_PREFIX {
        return Err(BoseError::AckMismatch);
    }

    let mut addr = BdAddr { b: [0; 6] };
    io.bose_read_exact(&mut addr.b)?;
    Ok(addr)
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

    io.bose_write_all(&GET_DEVICE_STATUS_SEND)?;
    let mut ack = [0u8; 4];
    io.bose_read_exact(&mut ack)?;
    if ack != GET_DEVICE_STATUS_ACK {
        return Err(BoseError::AckMismatch);
    }

    let mut name = None;
    let mut language = None;
    let mut minutes = None;
    let mut level = NoiseCancelling::Dne;
    loop {
        let (header, payload) = read_packet(io)?;
        if header == GET_DEVICE_STATUS_FINAL_ACK {
            break;
        }
        if header[0] != 0x01 || header[2] != OP_STATUS {
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

    Ok(DeviceStatusReport {
        device_id,
        name: name.ok_or(BoseError::AckMismatch)?,
        language: language.ok_or(BoseError::AckMismatch)?,
        minutes,
        level,
    })
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
    let mut ack = SET_PAIRING_ACK;
    ack[BYTES_POSITION_4] = pairing as u8;
    io.bose_write_check(&send, &ack)
}

/// `int set_self_voice(int sock, enum SelfVoice selfVoice)` in `based.c`.
pub fn set_self_voice<I: BoseIo>(io: &mut I, level: SelfVoice) -> BoseResult<()> {
    let mut send = SET_SELF_VOICE_SEND;
    send[BYTES_POSITION_5] = level as u8;
    let mut ack = SET_SELF_VOICE_ACK;
    ack[BYTES_POSITION_5] = level as u8;
    io.bose_write_check(&send, &ack)
}

/// `int set_noise_cancelling(int sock, enum NoiseCancelling level)` in
/// `based.c`. The C code does not verify that the device actually
/// supports noise-cancelling before sending; callers should run
/// [`has_noise_cancelling`] on the result of [`get_device_id`]
/// first to surface a friendly error.
pub fn set_noise_cancelling<I: BoseIo>(io: &mut I, level: NoiseCancelling) -> BoseResult<()> {
    let mut send = SET_NOISE_CANCELLING_SEND;
    send[BYTES_POSITION_4] = level as u8;
    io.bose_write_all(&send)?;

    let got = get_noise_cancelling(io)?;
    if got as u8 != level as u8 {
        return Err(unconfirmed(level as u8, got as u8));
    }
    Ok(())
}

/// `int set_auto_off(int sock, enum AutoOff minutes)` in `based.c`.
pub fn set_auto_off<I: BoseIo>(io: &mut I, minutes: AutoOff) -> BoseResult<()> {
    let mut send = SET_AUTO_OFF_SEND;
    send[BYTES_POSITION_4] = minutes as u8;
    io.bose_write_all(&send)?;

    let got = get_auto_off(io)?;
    if got != Some(minutes as u16) {
        return Err(unconfirmed(
            minutes as u16,
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
    io.bose_write_all(&send)?;

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
    io.bose_write_check(&GET_PAIRED_DEVICES_SEND, &GET_PAIRED_DEVICES_ACK)?;

    let mut num_devices_byte = [0u8; 1];
    io.bose_read_exact(&mut num_devices_byte)?;
    let num_devices = num_devices_byte[0] as usize / BT_ADDR_LEN;

    let mut num_connected_byte = [0u8; 1];
    io.bose_read_exact(&mut num_connected_byte)?;
    let connected =
        DevicesConnected::from_u8(num_connected_byte[0]).ok_or(BoseError::AckMismatch)?;

    let mut addresses = [BdAddr { b: [0; 6] }; MAX_NUM_DEVICES];
    for slot in addresses.iter_mut().take(num_devices) {
        let mut buf = [0u8; BT_ADDR_LEN];
        io.bose_read_exact(&mut buf)?;
        slot.b = buf;
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

    io.bose_write_check(&send, &GET_DEVICE_INFO_ACK)?;

    let mut len_byte = [0u8; 1];
    io.bose_read_exact(&mut len_byte)?;
    let mut length = len_byte[0] as usize;

    let mut device = Device {
        address: BdAddr { b: [0; 6] },
        status: DeviceStatus::Disconnected,
        name: [0u8; MAX_NAME_LEN_T],
        name_len: 0,
    };

    io.bose_read_exact(&mut device.address.b)?;
    length -= BT_ADDR_LEN;

    if device.address.b != address.b {
        return Err(unconfirmed(
            format!("{:02X?}", address.b),
            format!("{:02X?}", device.address.b),
        ));
    }

    let mut status_byte = [0u8; 1];
    io.bose_read_exact(&mut status_byte)?;
    length -= 1;
    device.status = DeviceStatus::from_u8(status_byte[0]).unwrap_or(DeviceStatus::Disconnected);

    // First 2 bytes of garbage. The C code's comment
    // "TODO(wolf): figure out what the first byte of garbage is for"
    // is preserved here as a literal 2-byte skip — we don't pretend
    // to know what they mean.
    let mut garbage = [0u8; BYTES_POSITION_2];
    io.bose_read_exact(&mut garbage)?;
    length -= BYTES_POSITION_2;

    if length > MAX_NAME_LEN_T {
        return Err(BoseError::InvalidArgument(format!(
            "device name length {} exceeds MAX_NAME_LEN={}",
            length, MAX_NAME_LEN_T
        )));
    }
    io.bose_read_exact(&mut device.name[..length])?;
    device.name_len = length;
    // NUL-terminate defensively in case the device sent a name
    // shorter than the declared length.
    device.name[length] = 0;

    Ok(device)
}

/// Current audio mode index (QC Ultra). Resolve it to a name with
/// [`get_audio_mode_name`].
pub fn get_audio_mode<I: BoseIo>(io: &mut I) -> BoseResult<u8> {
    io.bose_write_all(&GET_AUDIO_MODE_SEND)?;
    let payload = read_response(io, 0x1f, 0x03)?;
    first_byte(&payload)
}

/// Name of the audio mode stored in slot `index`, or `None` if the
/// slot is unused. Slots run from 0 to [`MAX_AUDIO_MODES`] - 1.
pub fn get_audio_mode_name<I: BoseIo>(io: &mut I, index: u8) -> BoseResult<Option<String>> {
    let mut send = [0u8; 5];
    send[..4].copy_from_slice(&GET_AUDIO_MODE_CONFIG_SEND_PREFIX);
    send[BYTES_POSITION_4] = index;
    io.bose_write_all(&send)?;

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
pub fn get_audio_modes<I: BoseIo>(io: &mut I) -> BoseResult<Vec<(u8, String)>> {
    let mut modes = Vec::new();
    for index in 0..MAX_AUDIO_MODES {
        if let Some(name) = get_audio_mode_name(io, index)? {
            modes.push((index, name));
        }
    }
    Ok(modes)
}

/// Switch to the audio mode in slot `index` (QC Ultra). The device
/// answers with a RESULT packet carrying the new mode index.
pub fn set_audio_mode<I: BoseIo>(io: &mut I, index: u8) -> BoseResult<()> {
    let mut send = [0u8; 6];
    send[..4].copy_from_slice(&SET_AUDIO_MODE_SEND_PREFIX);
    send[BYTES_POSITION_4] = index;
    // Byte 5 = 0: switch silently, without the voice-prompt announce.
    io.bose_write_all(&send)?;

    let got = first_byte(&read_response(io, 0x1f, 0x03)?)?;
    if got != index {
        return Err(unconfirmed(index, got));
    }
    Ok(())
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

// ---------------------------------------------------------------------------
// Private helpers. Mirror the static C functions in `based.c`, except
// `read_reply_header`, which has no C counterpart.
// ---------------------------------------------------------------------------

/// Read a 4-byte reply header for the request `request` (only its
/// block and function bytes are used). If the device answered with an
/// ERROR packet for that function, its payload is consumed — so the
/// stream stays in sync for the next request on the same connection —
/// and [`BoseError::DeviceError`] is returned with the first payload
/// byte as the code. Any other header is returned for the caller to
/// check.
fn read_reply_header<I: BoseIo>(io: &mut I, request: &[u8; 4]) -> BoseResult<[u8; 4]> {
    let mut header = [0u8; 4];
    io.bose_read_exact(&mut header)?;
    if header[0] == request[0] && header[1] == request[1] && header[2] == OP_ERROR {
        let mut payload = vec![0u8; usize::from(header[3])];
        io.bose_read_exact(&mut payload)?;
        return Err(BoseError::DeviceError {
            block: header[0],
            function: header[1],
            code: payload.first().copied().unwrap_or(0),
        });
    }
    Ok(header)
}

/// Send a paired-device packet and validate the response with a
/// permissive matcher. This replaces the C `write_check` for the
/// `connect_device` / `disconnect_device` / `remove_device` flows
/// because the SLC II 4.0.1 firmware does not return the canonical
/// 10-byte ACK that the original C code expects.
///
/// Observed SLC II responses (from `--send-packet` capture):
///
/// | Command          | Bytes sent                          | Bytes received                                          |
/// |------------------|-------------------------------------|---------------------------------------------------------|
/// | `CONNECT_DEVICE` | `04 01 05 07 00 + addr` (11)         | `04 01 04 07 0b + addr` (11) — opcode differs, length 7 |
/// | `DISCONNECT_DEVICE` | `04 02 05 06 + addr` (10)         | `04 02 07 00 04 02 06 06 + addr` (14) — 2 packets     |
/// | `REMOVE_DEVICE`  | `04 03 05 06 + addr` (10)            | empty — no response at all                               |
///
/// Acceptance rule (any of these = success):
///
/// 1. The response is **empty** — the device silently accepted the
///    command (REMOVE_DEVICE on SLC II).
/// 2. The first two bytes match the first two bytes of the sent
///    packet (the Bose subsystem prefix, e.g. `04 02` for
///    DISCONNECT_DEVICE), **and** the address bytes appear
///    anywhere in the response. This catches both the
///    canonical `prefix + addr` ACK and the SLC II's variant
///    (`prefix + status + addr`) responses.
///
/// Anything else returns [`BoseError::AckMismatch`].
pub(crate) fn write_paired<I: BoseIo>(
    io: &mut I,
    send: &[u8],
    address: &[u8; 6],
) -> BoseResult<()> {
    io.bose_write_all(send)?;

    // Read up to 64 bytes or until timeout. The SLC II has been
    // observed to send between 0 and 14 bytes depending on the
    // command; the Bose QC35 sends the canonical 10-byte ACK. 64
    // bytes is plenty of headroom for both.
    //
    // We read incrementally (not with `bose_read_exact`) so an
    // empty / partial response surfaces as the actual byte count,
    // not as an error. `bose_read_exact` would treat anything
    // shorter than the requested size as `ShortRead`, which would
    // mask the SLC II's silent-accept behaviour for REMOVE_DEVICE.
    let mut buf = vec![0u8; 64];
    let mut total = 0usize;
    while total < buf.len() {
        match std::io::Read::read(io, &mut buf[total..]) {
            Ok(0) => break,
            Ok(n) => {
                total += n;
                if total == buf.len() {
                    break;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => break,
            Err(e) => return Err(e.into()),
        }
    }
    buf.truncate(total);

    // Rule 1: silent acceptance.
    if buf.is_empty() {
        return Ok(());
    }

    // Rule 2: prefix + address-anywhere.
    if buf.len() >= 2
        && send.len() >= 2
        && buf[0] == send[0]
        && buf[1] == send[1]
        && buf.windows(6).any(|w| w == address)
    {
        return Ok(());
    }

    Err(BoseError::AckMismatch)
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

/// Read one packet: the 4-byte header (`block, function, operator,
/// length`) followed by `length` payload bytes.
fn read_packet<I: BoseIo>(io: &mut I) -> BoseResult<([u8; 4], Vec<u8>)> {
    let mut header = [0u8; 4];
    io.bose_read_exact(&mut header)?;
    let mut payload = vec![0u8; header[BYTES_POSITION_3] as usize];
    io.bose_read_exact(&mut payload)?;
    Ok((header, payload))
}

/// Read the device's reply to a `block.function` request and return
/// its payload, whatever its length. PROCESSING packets are skipped;
/// an ERROR packet becomes [`BoseError::DeviceError`].
fn read_response<I: BoseIo>(io: &mut I, block: u8, function: u8) -> BoseResult<Vec<u8>> {
    loop {
        let (header, payload) = read_packet(io)?;
        if header[0] != block || header[1] != function {
            return Err(BoseError::AckMismatch);
        }
        match header[BYTES_POSITION_2] {
            OP_STATUS | OP_RESULT => return Ok(payload),
            OP_PROCESSING => continue,
            OP_ERROR => {
                return Err(BoseError::DeviceError {
                    block,
                    function,
                    code: payload.first().copied().unwrap_or(0),
                })
            }
            _ => return Err(BoseError::AckMismatch),
        }
    }
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

/// Auto-off payload. Only the 1-byte QC35 layout (minutes) is
/// understood; the QC Ultra sends 3 bytes (`a0 00 05`) whose meaning
/// is unknown.
fn parse_auto_off(payload: &[u8]) -> Option<u16> {
    match payload {
        [minutes] => Some(*minutes as u16),
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

    /// `write_paired` is the permissive ACK matcher used by
    /// `connect_device`, `disconnect_device`, and `remove_device`.
    /// It accepts three response shapes observed across Bose
    /// firmwares: canonical QC35 ACK, SLC II variant with a
    /// status byte, and the silent-accept (empty) response
    /// observed on SLC II `REMOVE_DEVICE`. Each test below
    /// drives `write_paired` over a `UnixStream` pair and asserts
    /// the right `BoseResult`.
    mod write_paired_tests {
        use super::*;
        use std::io::{Read, Write};
        use std::os::unix::net::UnixStream;
        use std::thread;

        /// Build a pair, return (client, server) so the test can
        /// drive the protocol side and the helper thread can
        /// simulate the Bose device side.
        fn pair() -> (UnixStream, UnixStream) {
            UnixStream::pair().expect("UnixStream::pair")
        }

        /// Drain the `send` packet from the device side and
        /// reply with `reply` (may be empty for silent-accept
        /// tests).
        fn reply_with(server: &mut UnixStream, reply: &[u8]) {
            let mut len = [0u8; 1];
            // The protocol layer reads up to 64 bytes; the test
            // helper doesn't need to consume all of them, just
            // enough to know what was sent.
            let _ = server.read(&mut len);
            let _ = server.read(&mut [0u8; 64]);
            if !reply.is_empty() {
                server.write_all(reply).unwrap();
                server.flush().ok();
            }
        }

        /// Run a closure on the client side while the server
        /// side runs `device_side`.
        fn with_pair<R, F, G>(device_side: G, client_op: F) -> R
        where
            F: FnOnce(&mut UnixStream) -> R + Send + 'static,
            G: FnOnce(&mut UnixStream) + Send + 'static,
            R: Send + 'static,
        {
            let (mut client, mut server) = pair();
            let server_handle = thread::spawn(move || device_side(&mut server));
            let result = client_op(&mut client);
            server_handle.join().expect("server thread panicked");
            result
        }

        #[test]
        fn accepts_canonical_qc35_ack() {
            // CONNECT_DEVICE canonical QC35 ACK: 10 bytes,
            // opcode 0x07, length 6, then 6-byte address.
            let addr = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
            let send = [
                0x04, 0x01, 0x05, 0x07, 0x00, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF,
            ];
            let ack = [0x04, 0x01, 0x07, 0x06, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
            let result = with_pair(
                move |s| reply_with(s, &ack),
                move |c| write_paired(c, &send, &addr),
            );
            assert!(result.is_ok(), "expected Ok, got {:?}", result);
        }

        #[test]
        fn accepts_slc2_variant_with_status_byte() {
            // Observed SLC II response: opcode differs (0x04 not
            // 0x07), length differs (7 not 6), 1-byte status
            // prefix (0x0b) before the address.
            let addr = [0x04, 0x52, 0xC7, 0xBA, 0x68, 0x0D];
            let send = [
                0x04, 0x01, 0x05, 0x07, 0x00, 0x04, 0x52, 0xC7, 0xBA, 0x68, 0x0D,
            ];
            let ack = [
                0x04, 0x01, 0x04, 0x07, 0x0B, 0x04, 0x52, 0xC7, 0xBA, 0x68, 0x0D,
            ];
            let result = with_pair(
                move |s| reply_with(s, &ack),
                move |c| write_paired(c, &send, &addr),
            );
            assert!(result.is_ok(), "expected Ok, got {:?}", result);
        }

        #[test]
        fn accepts_silent_empty_response() {
            // Observed SLC II REMOVE_DEVICE: device sends no
            // bytes back at all. The 1s SO_RCVTIMEO fires, the
            // read loop breaks on timeout, and we treat silence
            // as acceptance.
            let addr = [0x04, 0x52, 0xC7, 0xBA, 0x68, 0x0D];
            let send = [0x04, 0x03, 0x05, 0x06, 0x04, 0x52, 0xC7, 0xBA, 0x68, 0x0D];
            let result = with_pair(
                move |s| {
                    // Drain the sent packet but send nothing
                    // back.
                    let mut buf = [0u8; 64];
                    let _ = std::io::Read::read(s, &mut buf);
                },
                move |c| write_paired(c, &send, &addr),
            );
            assert!(result.is_ok(), "expected Ok, got {:?}", result);
        }

        #[test]
        fn accepts_disconnect_two_packet_response() {
            // Observed SLC II DISCONNECT_DEVICE: device sends
            // two concatenated responses. The prefix of the
            // first matches the sent command's prefix, the
            // address appears somewhere in the response, so the
            // permissive matcher accepts.
            let addr = [0x04, 0x52, 0xC7, 0xBA, 0x68, 0x0D];
            let send = [0x04, 0x02, 0x05, 0x06, 0x04, 0x52, 0xC7, 0xBA, 0x68, 0x0D];
            let ack = [
                0x04, 0x02, 0x07, 0x00, 0x04, 0x02, 0x06, 0x06, 0x04, 0x52, 0xC7, 0xBA, 0x68, 0x0D,
            ];
            let result = with_pair(
                move |s| reply_with(s, &ack),
                move |c| write_paired(c, &send, &addr),
            );
            assert!(result.is_ok(), "expected Ok, got {:?}", result);
        }

        #[test]
        fn rejects_response_with_wrong_prefix() {
            // Response starts with `04 0f` (different subsystem)
            // — must be rejected even though the address
            // appears somewhere in the response.
            let addr = [0x04, 0x52, 0xC7, 0xBA, 0x68, 0x0D];
            let send = [0x04, 0x02, 0x05, 0x06, 0x04, 0x52, 0xC7, 0xBA, 0x68, 0x0D];
            let ack = [0x04, 0x0F, 0x07, 0x06, 0x04, 0x52, 0xC7, 0xBA, 0x68, 0x0D];
            let result = with_pair(
                move |s| reply_with(s, &ack),
                move |c| write_paired(c, &send, &addr),
            );
            assert!(result.is_err(), "expected Err, got {:?}", result);
            assert!(matches!(result, Err(BoseError::AckMismatch)));
        }

        #[test]
        fn rejects_response_missing_address() {
            // Response prefix matches but the address bytes
            // don't appear anywhere in the response.
            let addr = [0x04, 0x52, 0xC7, 0xBA, 0x68, 0x0D];
            let send = [
                0x04, 0x01, 0x05, 0x07, 0x00, 0x04, 0x52, 0xC7, 0xBA, 0x68, 0x0D,
            ];
            let ack = [0x04, 0x01, 0x07, 0x06, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
            let result = with_pair(
                move |s| reply_with(s, &ack),
                move |c| write_paired(c, &send, &addr),
            );
            assert!(result.is_err(), "expected Err, got {:?}", result);
            assert!(matches!(result, Err(BoseError::AckMismatch)));
        }
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
            // `05 05 03 02 64 0a` (100 volume steps, volume 10).
            let result = drive(
                move |s| reply(s, &[0x05, 0x05, 0x03, 0x02, 0x64, 0x0a]),
                move |c| set_volume(c, 0x0a),
            );
            let steps = result.expect("set_volume failed");
            assert_eq!(steps, 100, "expected 100 steps, got {steps}");
        }

        #[test]
        fn set_volume_succeeds_at_volume_zero() {
            // Live capture: sent `05 05 02 01 00`, received
            // `05 05 03 02 64 00` (100 volume steps, volume 0 / muted).
            let result = drive(
                move |s| reply(s, &[0x05, 0x05, 0x03, 0x02, 0x64, 0x00]),
                move |c| set_volume(c, 0x00),
            );
            let steps = result.expect("set_volume failed");
            assert_eq!(steps, 100, "expected 100 steps, got {steps}");
        }

        #[test]
        fn set_volume_succeeds_at_max_level() {
            // Live capture on a SoundLink Color II: 99 is the highest
            // level it accepts (`05 05 03 02 64 63`).
            let result = drive(
                move |s| reply(s, &[0x05, 0x05, 0x03, 0x02, 0x64, 0x63]),
                move |c| set_volume(c, 0x63),
            );
            let steps = result.expect("set_volume failed");
            assert_eq!(steps, 100, "expected 100 steps, got {steps}");
        }

        #[test]
        fn set_volume_rejects_wrong_header() {
            // Reply starts with the wrong subsystem — must be rejected.
            let result = drive(
                move |s| reply(s, &[0x06, 0x05, 0x03, 0x02, 0x64, 0x0a]),
                move |c| set_volume(c, 0x0a),
            );
            assert!(matches!(result, Err(BoseError::AckMismatch)));
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
        fn send_media_key_rejects_wrong_first_packet() {
            let result = drive(
                move |s| reply(s, &[0x05, 0x03, 0x06, 0x00, 0x05, 0x03, 0x06, 0x00]),
                move |c| send_media_key(c, MediaKey::Pause),
            );
            assert!(matches!(result, Err(BoseError::AckMismatch)));
        }

        #[test]
        fn send_media_key_rejects_wrong_final_ack() {
            // First packet is correct, second (final ACK) is wrong.
            let result = drive(
                move |s| reply(s, &[0x05, 0x03, 0x07, 0x00, 0x05, 0x03, 0x04, 0x00]),
                move |c| send_media_key(c, MediaKey::Pause),
            );
            assert!(matches!(result, Err(BoseError::AckMismatch)));
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
        fn get_active_device_rejects_wrong_header() {
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
            assert!(matches!(result, Err(BoseError::AckMismatch)));
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
        fn get_device_bd_addr_rejects_wrong_header() {
            let result = drive(
                move |s| {
                    reply(
                        s,
                        &[0x00, 0x07, 0x03, 0x06, 0x04, 0x52, 0xc7, 0xba, 0x68, 0x0d],
                    )
                },
                move |c| get_device_bd_addr(c),
            );
            assert!(matches!(result, Err(BoseError::AckMismatch)));
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
