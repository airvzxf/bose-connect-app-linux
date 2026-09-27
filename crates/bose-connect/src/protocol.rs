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
    AutoOff, BdAddr, Device, DeviceStatus, DevicesConnected, NoiseCancelling, Pairing,
    PromptLanguage, SelfVoice, MAX_NAME_PACK_LEN_HELPER, MAX_NUM_DEVICES, NOISE_CANCELLING_0C,
    NOISE_CANCELLING_14, NOISE_CANCELLING_20, VP_ENABLE_BIT, VP_MASK,
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

const GET_NAME_ACK: [u8; 5] = [0x01, 0x02, 0x03, 0x00, 0x00];
const GET_NAME_MASK: [u8; 5] = [0xff, 0xff, 0xff, 0x00, 0xff];

const SET_NAME_SEND_PREFIX: [u8; 4] = [0x01, 0x02, 0x02, 0x00];

const GET_PROMPT_LANGUAGE_ACK: [u8; 9] = [0x01, 0x03, 0x03, 0x05, 0x00, 0x00, 0x00, 0x00, 0xde];
const GET_PROMPT_LANGUAGE_MASK: [u8; 9] = [0xff, 0xff, 0xff, 0xff, 0x00, 0xff, 0x00, 0x00, 0xff];

const SET_PROMPT_LANGUAGE_SEND: [u8; 5] = [0x01, 0x03, 0x02, 0x01, 0x00];

const GET_AUTO_OFF_ACK: [u8; 5] = [0x01, 0x04, 0x03, 0x01, 0x00];
const GET_AUTO_OFF_MASK: [u8; 5] = [0xff, 0xff, 0xff, 0xff, 0x00];

const SET_AUTO_OFF_SEND: [u8; 5] = [0x01, 0x04, 0x02, 0x01, 0x00];

const GET_NOISE_CANCELLING_ACK: [u8; 6] = [0x01, 0x06, 0x03, 0x02, 0x00, 0x0b];
const GET_NOISE_CANCELLING_MASK: [u8; 6] = [0xff, 0xff, 0xff, 0xff, 0x00, 0xff];

const SET_NOISE_CANCELLING_SEND: [u8; 5] = [0x01, 0x06, 0x02, 0x01, 0x00];

const GET_DEVICE_STATUS_SEND: [u8; 4] = [0x01, 0x01, 0x05, 0x00];
const GET_DEVICE_STATUS_ACK: [u8; 4] = [0x01, 0x01, 0x07, 0x00];

const GET_DEVICE_STATUS_FINAL_ACK: [u8; 4] = [0x01, 0x01, 0x06, 0x00];

const GET_FIRMWARE_VERSION_SEND: [u8; 4] = [0x00, 0x05, 0x01, 0x00];
const GET_FIRMWARE_VERSION_ACK: [u8; 4] = [0x00, 0x05, 0x03, 0x05];

const GET_SERIAL_NUMBER_SEND: [u8; 4] = [0x00, 0x07, 0x01, 0x00];
const GET_SERIAL_NUMBER_ACK: [u8; 3] = [0x00, 0x07, 0x03];

const GET_BATTERY_LEVEL_SEND: [u8; 4] = [0x02, 0x02, 0x01, 0x00];
const GET_BATTERY_LEVEL_ACK: [u8; 4] = [0x02, 0x02, 0x03, 0x01];

const GET_PAIRED_DEVICES_SEND: [u8; 4] = [0x04, 0x04, 0x01, 0x00];
const GET_PAIRED_DEVICES_ACK: [u8; 3] = [0x04, 0x04, 0x03];

const SET_PAIRING_SEND: [u8; 5] = [0x04, 0x08, 0x05, 0x01, 0x00];
const SET_PAIRING_ACK: [u8; 5] = [0x04, 0x08, 0x06, 0x01, 0x00];

const SET_SELF_VOICE_SEND: [u8; 7] = [0x01, 0x0b, 0x02, 0x02, 0x01, 0x00, 0x38];
const SET_SELF_VOICE_ACK: [u8; 7] = [0x01, 0x0b, 0x03, 0x03, 0x01, 0x00, 0x0f];

const GET_DEVICE_INFO_SEND_PREFIX: [u8; 4] = [0x04, 0x05, 0x01, 6];
const GET_DEVICE_INFO_ACK: [u8; 3] = [0x04, 0x05, 0x03];

const CONNECT_DEVICE_SEND_PREFIX: [u8; 5] = [0x04, 0x01, 0x05, 6 + 1, 0x00];
const CONNECT_DEVICE_ACK_PREFIX: [u8; 4] = [0x04, 0x01, 0x07, 6];

const DISCONNECT_DEVICE_SEND_PREFIX: [u8; 4] = [0x04, 0x02, 0x05, 6];
const DISCONNECT_DEVICE_ACK_PREFIX: [u8; 4] = [0x04, 0x02, 0x07, 6];

const REMOVE_DEVICE_SEND_PREFIX: [u8; 4] = [0x04, 0x03, 0x05, 6];
const REMOVE_DEVICE_ACK_PREFIX: [u8; 4] = [0x04, 0x03, 0x06, 6];

const BT_ADDR_LEN: usize = 6;
const CN_BASE_PACK_LEN: usize = 4;
const MAX_BT_PACK_LEN_T: usize = crate::types::MAX_BT_PACK_LEN;
const VER_STR_LEN_T: usize = crate::types::VER_STR_LEN;
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
/// Conservative allow-list: only the QC35 series (device IDs
/// `0x4014` and `0x4020`) is known to expose self-voice. The
/// SoundLink II (`0x400d`) does not. If a new device is found
/// to support self-voice, add its device ID here and the CLI's
/// pre-flight check will start allowing the command.
pub fn has_self_voice(device_id: u16) -> bool {
    matches!(device_id, NOISE_CANCELLING_14 | NOISE_CANCELLING_20)
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
pub fn send_packet<I: BoseIo>(io: &mut I, send: &[u8]) -> BoseResult<Vec<u8>> {
    io.bose_write_all(send)?;
    let mut buf = vec![0u8; MAX_BT_PACK_LEN_T];
    let mut total = 0usize;
    loop {
        match io.read(&mut buf[total..]) {
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
/// `based.c`. Returns a 5-character firmware version string (the
/// 6-byte buffer holds 5 chars plus a NUL).
pub fn get_firmware_version<I: BoseIo>(io: &mut I) -> BoseResult<String> {
    io.bose_write_check(&GET_FIRMWARE_VERSION_SEND, &GET_FIRMWARE_VERSION_ACK)?;
    let mut version = [0u8; VER_STR_LEN_T - 1];
    io.bose_read_exact(&mut version)?;
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
pub fn get_battery_level<I: BoseIo>(io: &mut I) -> BoseResult<u8> {
    io.bose_write_check(&GET_BATTERY_LEVEL_SEND, &GET_BATTERY_LEVEL_ACK)?;
    let mut level = [0u8; 1];
    io.bose_read_exact(&mut level)?;
    Ok(level[0])
}

/// `int get_device_status(int sock, char name[MAX_NAME_LEN], enum
/// PromptLanguage *language, enum AutoOff *minutes, enum
/// NoiseCancelling *level)` in `based.c`.
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

    let name = get_name(io)?;
    let language = get_prompt_language(io)?;
    let minutes = get_auto_off(io)?;

    let level = if has_noise_cancelling(device_id) {
        get_noise_cancelling(io)?
    } else {
        NoiseCancelling::Dne
    };

    let mut final_ack = [0u8; 4];
    io.bose_read_exact(&mut final_ack)?;
    if final_ack != GET_DEVICE_STATUS_FINAL_ACK {
        return Err(BoseError::AckMismatch);
    }

    Ok(DeviceStatusReport {
        name,
        language,
        minutes,
        level,
    })
}

/// Aggregate return type for [`get_device_status`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceStatusReport {
    /// Device name as reported by the device.
    pub name: String,
    /// Voice-prompt language. The high bit (`VP_ENABLE_BIT`) carries
    /// the "voice-prompts on" flag; the lower 5 bits hold the
    /// language code. Use [`PromptLanguage::from_u8`] and
    /// `byte & VP_ENABLE_BIT` to split.
    pub language: u8,
    /// Auto-off value as raw minutes. `0` means "never".
    pub minutes: u16,
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
    if got as u16 != minutes as u16 {
        return Err(unconfirmed(minutes as u16, got as u16));
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

/// `int connect_device(int sock, bdaddr_t address)` in `based.c`.
pub fn connect_device<I: BoseIo>(io: &mut I, address: BdAddr) -> BoseResult<()> {
    let mut send = [0u8; BYTES_POSITION_11];
    send[..5].copy_from_slice(&CONNECT_DEVICE_SEND_PREFIX);
    send[BYTES_POSITION_5..BYTES_POSITION_5 + BT_ADDR_LEN].copy_from_slice(&address.b);

    let mut ack = [0u8; BYTES_POSITION_10];
    ack[..4].copy_from_slice(&CONNECT_DEVICE_ACK_PREFIX);
    ack[BYTES_POSITION_4..BYTES_POSITION_4 + BT_ADDR_LEN].copy_from_slice(&address.b);

    io.bose_write_check(&send, &ack)
}

/// `int disconnect_device(int sock, bdaddr_t address)` in `based.c`.
pub fn disconnect_device<I: BoseIo>(io: &mut I, address: BdAddr) -> BoseResult<()> {
    let mut send = [0u8; BYTES_POSITION_10];
    send[..4].copy_from_slice(&DISCONNECT_DEVICE_SEND_PREFIX);
    send[BYTES_POSITION_4..BYTES_POSITION_4 + BT_ADDR_LEN].copy_from_slice(&address.b);

    let mut ack = [0u8; BYTES_POSITION_10];
    ack[..4].copy_from_slice(&DISCONNECT_DEVICE_ACK_PREFIX);
    ack[BYTES_POSITION_4..BYTES_POSITION_4 + BT_ADDR_LEN].copy_from_slice(&address.b);

    io.bose_write_check(&send, &ack)
}

/// `int remove_device(int sock, bdaddr_t address)` in `based.c`.
pub fn remove_device<I: BoseIo>(io: &mut I, address: BdAddr) -> BoseResult<()> {
    let mut send = [0u8; BYTES_POSITION_10];
    send[..4].copy_from_slice(&REMOVE_DEVICE_SEND_PREFIX);
    send[BYTES_POSITION_4..BYTES_POSITION_4 + BT_ADDR_LEN].copy_from_slice(&address.b);

    let mut ack = [0u8; BYTES_POSITION_10];
    ack[..4].copy_from_slice(&REMOVE_DEVICE_ACK_PREFIX);
    ack[BYTES_POSITION_4..BYTES_POSITION_4 + BT_ADDR_LEN].copy_from_slice(&address.b);

    io.bose_write_check(&send, &ack)
}

// ---------------------------------------------------------------------------
// Private helpers. Mirror the static C functions in `based.c`.
// ---------------------------------------------------------------------------

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

fn get_name<I: BoseIo>(io: &mut I) -> BoseResult<String> {
    let mut buffer = [0u8; 5]; // GET_NAME_ACK length
    io.bose_read_exact(&mut buffer)?;
    if masked_memory_cmp(&GET_NAME_ACK, &buffer, &GET_NAME_MASK) != 0 {
        return Err(BoseError::AckMismatch);
    }

    let length = (buffer[BYTES_POSITION_3] - 1) as usize;
    let mut name = vec![0u8; length];
    io.bose_read_exact(&mut name)?;
    Ok(String::from_utf8_lossy(&name).into_owned())
}

fn get_prompt_language<I: BoseIo>(io: &mut I) -> BoseResult<u8> {
    let mut buffer = [0u8; 9];
    io.bose_read_exact(&mut buffer)?;
    if masked_memory_cmp(&GET_PROMPT_LANGUAGE_ACK, &buffer, &GET_PROMPT_LANGUAGE_MASK) != 0 {
        return Err(BoseError::AckMismatch);
    }
    Ok(buffer[BYTES_POSITION_4])
}

fn get_auto_off<I: BoseIo>(io: &mut I) -> BoseResult<u16> {
    let mut buffer = [0u8; 5];
    io.bose_read_exact(&mut buffer)?;
    if masked_memory_cmp(&GET_AUTO_OFF_ACK, &buffer, &GET_AUTO_OFF_MASK) != 0 {
        return Err(BoseError::AckMismatch);
    }
    Ok(buffer[BYTES_POSITION_4] as u16)
}

fn get_noise_cancelling<I: BoseIo>(io: &mut I) -> BoseResult<NoiseCancelling> {
    let mut buffer = [0u8; 6];
    io.bose_read_exact(&mut buffer)?;
    if masked_memory_cmp(
        &GET_NOISE_CANCELLING_ACK,
        &buffer,
        &GET_NOISE_CANCELLING_MASK,
    ) != 0
    {
        return Err(BoseError::AckMismatch);
    }
    NoiseCancelling::from_u8(buffer[BYTES_POSITION_4]).ok_or(BoseError::InvalidArgument(format!(
        "noise cancelling byte 0x{:02x} out of range",
        buffer[BYTES_POSITION_4]
    )))
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
}
