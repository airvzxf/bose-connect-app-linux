//! C ABI surface for the `bose-connect` library.
//!
//! Every function in this module is `#[no_mangle] pub extern "C"`,
//! panic-safe (caught with `catch_unwind`), and uses only
//! C-compatible types in its signature. A Rust caller should use
//! the high-level API in [`crate`] instead; this module exists for
//! downstream C / C++ / Python / Ruby / Go consumers.
//!
//! See the auto-generated `bose_connect.h` header for the canonical
//! declarations (produced by `cbindgen` from this file at build
//! time).
//!
//! ## Threading
//!
//! Every function here is synchronous and **not** thread-safe on
//! the same handle. Use one handle per thread, or guard externally.
//!
//! ## Error model
//!
//! Fallible functions return a [`BoseFfiResult`] struct:
//!
//! ```c
//! typedef struct {
//!     int32_t rc;            // 0 on success, BoseError::rc_code() otherwise
//!     char    message[512];  // NUL-terminated; empty on success
//! } BoseFfiResult;
//! ```
//!
//! Callers check `rc` first; if non-zero, `message` holds a
//! human-readable description (already truncated to fit the buffer).

// The FFI surface is `extern "C"` for ABI compatibility. Functions
// take raw pointers and use them in `unsafe` blocks internally, but
// the FFI signature itself is plain `extern "C"` (not
// `unsafe extern "C"`) because the C caller does not have an
// `unsafe` keyword. Suppress the relevant clippy lints at the
// module level so each function does not need a per-line
// annotation.
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::ffi::CStr;
use std::os::raw::{c_char, c_int};
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::connection::Connection;
use crate::error::BoseError;
use crate::protocol as p;
use crate::types::{
    AutoOff, BdAddr, NoiseCancelling, Pairing, PromptLanguage, SelfVoice, MAX_NAME_LEN,
    MAX_NUM_DEVICES,
};

/// Result struct returned by every fallible FFI function.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BoseFfiResult {
    /// 0 on success, otherwise the [`BoseError::rc_code`] value.
    pub rc: i32,
    /// Error message, NUL-terminated. Empty on success. Always fits
    /// in the declared buffer (we truncate to fit).
    pub message: [c_char; 512],
}

impl BoseFfiResult {
    const fn ok() -> Self {
        Self {
            rc: 0,
            message: [0; 512],
        }
    }

    fn from_err(e: &BoseError) -> Self {
        let mut r = Self {
            rc: e.rc_code(),
            message: [0; 512],
        };
        let formatted = format!("{}", e);
        let bytes = formatted.as_bytes();
        let take = bytes.len().min(r.message.len() - 1);
        for (i, &b) in bytes[..take].iter().enumerate() {
            r.message[i] = b as c_char;
        }
        r
    }
}

/// Opaque handle to an open Bose connection.
#[repr(C)]
pub struct BoseConnectHandle {
    conn: Connection,
}

// ---------------------------------------------------------------------------
// Connection lifecycle
// ---------------------------------------------------------------------------

/// Open a new RFCOMM connection and run the init handshake.
///
/// `address` must be a NUL-terminated "AA:BB:CC:DD:EE:FF" string.
/// On success, returns a non-null heap-allocated handle (the caller
/// must eventually call [`bose_connect_close`] on it). On failure,
/// returns NULL and, if `out_error` is non-null, populates it with
/// the failure details.
#[no_mangle]
pub extern "C" fn bose_connect_open(
    address: *const c_char,
    out_error: *mut BoseFfiResult,
) -> *mut BoseConnectHandle {
    let do_open = || -> Result<*mut BoseConnectHandle, BoseError> {
        let address_str = cstr_required(address, "address")?;
        let conn = Connection::open(address_str.as_str())?;
        Ok(Box::into_raw(Box::new(BoseConnectHandle { conn })))
    };
    match catch_unwind(AssertUnwindSafe(do_open)) {
        Ok(Ok(ptr)) => {
            if !out_error.is_null() {
                unsafe {
                    std::ptr::write(out_error, BoseFfiResult::ok());
                }
            }
            ptr
        }
        Ok(Err(e)) => {
            if !out_error.is_null() {
                unsafe {
                    std::ptr::write(out_error, BoseFfiResult::from_err(&e));
                }
            }
            std::ptr::null_mut()
        }
        Err(_) => {
            if !out_error.is_null() {
                let e = BoseError::Io(std::io::Error::other("internal panic"));
                unsafe {
                    std::ptr::write(out_error, BoseFfiResult::from_err(&e));
                }
            }
            std::ptr::null_mut()
        }
    }
}

/// Close and free an open handle. Passing NULL is a no-op.
#[no_mangle]
pub extern "C" fn bose_connect_close(handle: *mut BoseConnectHandle) {
    if handle.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(handle));
    }
}

// ---------------------------------------------------------------------------
// Getter functions
// ---------------------------------------------------------------------------

/// Fetch the device id (u16, little-endian) and index revision (u8).
/// Writes 4 bytes into `out_device_id` (low byte first, then high,
/// then `index`, then padding).
#[no_mangle]
pub extern "C" fn bose_connect_get_device_id(
    handle: *mut BoseConnectHandle,
    out_device_id: *mut u8,
    out_index: *mut u8,
) -> BoseFfiResult {
    run_block(handle, |h| {
        let (did, idx) = p::get_device_id(&mut h.conn)?;
        if !out_device_id.is_null() {
            let bytes = did.to_le_bytes();
            unsafe {
                std::ptr::write(out_device_id, bytes[0]);
                std::ptr::write(out_device_id.add(1), bytes[1]);
            }
        }
        if !out_index.is_null() {
            unsafe {
                std::ptr::write(out_index, idx);
            }
        }
        Ok(())
    })
}

/// Fetch the firmware version. Writes up to 5 bytes plus NUL into
/// `out_version` (which must be at least `out_version_len` bytes).
#[no_mangle]
pub extern "C" fn bose_connect_get_firmware_version(
    handle: *mut BoseConnectHandle,
    out_version: *mut c_char,
    out_version_len: usize,
) -> BoseFfiResult {
    run_block(handle, |h| {
        let v = p::get_firmware_version(&mut h.conn)?;
        write_cstr(out_version, out_version_len, &v)
    })
}

/// Fetch the serial number.
#[no_mangle]
pub extern "C" fn bose_connect_get_serial_number(
    handle: *mut BoseConnectHandle,
    out_serial: *mut c_char,
    out_serial_len: usize,
) -> BoseFfiResult {
    run_block(handle, |h| {
        let s = p::get_serial_number(&mut h.conn)?;
        write_cstr(out_serial, out_serial_len, &s)
    })
}

/// Fetch the battery level (0..=100).
#[no_mangle]
pub extern "C" fn bose_connect_get_battery_level(
    handle: *mut BoseConnectHandle,
    out_level: *mut u8,
) -> BoseFfiResult {
    run_block(handle, |h| {
        let l = p::get_battery_level(&mut h.conn)?;
        if !out_level.is_null() {
            unsafe {
                std::ptr::write(out_level, l);
            }
        }
        Ok(())
    })
}

/// Fetch the full device status (name, prompt language, auto-off,
/// noise-cancelling level). The name and language are written into
/// caller-provided buffers; the auto-off / NC level are written to
/// out-pointers.
#[no_mangle]
pub extern "C" fn bose_connect_get_device_status(
    handle: *mut BoseConnectHandle,
    out_name: *mut c_char,
    out_name_len: usize,
    out_prompt_language: *mut u8,
    out_auto_off_minutes: *mut u16,
    out_noise_cancelling: *mut u8,
) -> BoseFfiResult {
    run_block(handle, |h| {
        let status = p::get_device_status(&mut h.conn)?;
        write_cstr(out_name, out_name_len, &status.name)?;
        if !out_prompt_language.is_null() {
            unsafe {
                std::ptr::write(out_prompt_language, status.language);
            }
        }
        if !out_auto_off_minutes.is_null() {
            unsafe {
                std::ptr::write(out_auto_off_minutes, status.minutes);
            }
        }
        if !out_noise_cancelling.is_null() {
            unsafe {
                std::ptr::write(out_noise_cancelling, status.level as u8);
            }
        }
        Ok(())
    })
}

/// Fetch the list of paired devices. `out_addresses` must point to
/// at least `MAX_NUM_DEVICES` BdAddr entries (each 6 bytes). The
/// number of populated entries is written to `out_num_devices`.
///
/// `out_connected` receives one of: 1 (one device connected),
/// 3 (two devices connected), 0xff (unknown).
#[no_mangle]
pub extern "C" fn bose_connect_get_paired_devices(
    handle: *mut BoseConnectHandle,
    out_addresses: *mut u8,
    out_num_devices: *mut usize,
    out_connected: *mut u8,
) -> BoseFfiResult {
    run_block(handle, |h| {
        let pd = p::get_paired_devices(&mut h.conn)?;
        if !out_addresses.is_null() && pd.num_devices > 0 {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    pd.addresses.as_ptr() as *const u8,
                    out_addresses,
                    pd.num_devices * 6,
                );
            }
        }
        if !out_num_devices.is_null() {
            unsafe {
                std::ptr::write(out_num_devices, pd.num_devices);
            }
        }
        if !out_connected.is_null() {
            unsafe {
                std::ptr::write(out_connected, pd.connected as u8);
            }
        }
        Ok(())
    })
}

/// Fetch the cached status (name, address, status byte) for a
/// paired device. `address_bytes` must point to 6 bytes.
#[no_mangle]
pub extern "C" fn bose_connect_get_device_info(
    handle: *mut BoseConnectHandle,
    address_bytes: *const u8,
    out_status: *mut u8,
    out_name: *mut c_char,
    out_name_len: usize,
) -> BoseFfiResult {
    run_block(handle, |h| {
        let mut addr = BdAddr { b: [0; 6] };
        if address_bytes.is_null() {
            return Err(BoseError::InvalidArgument(
                "address_bytes is null".to_string(),
            ));
        }
        unsafe {
            std::ptr::copy_nonoverlapping(address_bytes, addr.b.as_mut_ptr(), 6);
        }
        let info = p::get_device_info(&mut h.conn, addr)?;
        if !out_status.is_null() {
            unsafe {
                std::ptr::write(out_status, info.status as u8);
            }
        }
        // Convert the fixed-size byte buffer into a UTF-8 string.
        let name_slice = &info.name[..info.name_len];
        let name_str = String::from_utf8_lossy(name_slice).into_owned();
        write_cstr(out_name, out_name_len, &name_str)
    })
}

// ---------------------------------------------------------------------------
// Setter functions
// ---------------------------------------------------------------------------

/// Change the device name. `name` must be NUL-terminated and shorter
/// than [`MAX_NAME_LEN`].
#[no_mangle]
pub extern "C" fn bose_connect_set_name(
    handle: *mut BoseConnectHandle,
    name: *const c_char,
) -> BoseFfiResult {
    run_block(handle, |h| {
        let name = cstr_required(name, "name")?;
        p::set_name(&mut h.conn, name.as_str())?;
        Ok(())
    })
}

/// Set the raw prompt-language byte. The high bit carries the
/// voice-prompts on/off flag (`VP_ENABLE_BIT`); the lower 5 bits
/// hold the language code (see [`PromptLanguage`] in the header).
#[no_mangle]
pub extern "C" fn bose_connect_set_prompt_language(
    handle: *mut BoseConnectHandle,
    language: u8,
) -> BoseFfiResult {
    run_block(handle, |h| {
        p::set_prompt_language(&mut h.conn, language)?;
        Ok(())
    })
}

/// Toggle voice prompts.
#[no_mangle]
pub extern "C" fn bose_connect_set_voice_prompts(
    handle: *mut BoseConnectHandle,
    on: c_int,
) -> BoseFfiResult {
    run_block(handle, |h| {
        p::set_voice_prompts(&mut h.conn, on != 0)?;
        Ok(())
    })
}

/// Change the auto-off timer. `minutes` must be one of
/// {0, 5, 20, 40, 60, 180}.
#[no_mangle]
pub extern "C" fn bose_connect_set_auto_off(
    handle: *mut BoseConnectHandle,
    minutes: u16,
) -> BoseFfiResult {
    run_block(handle, |h| {
        let ao = match minutes {
            0 => AutoOff::Never,
            5 => AutoOff::Min5,
            20 => AutoOff::Min20,
            40 => AutoOff::Min40,
            60 => AutoOff::Min60,
            180 => AutoOff::Min180,
            _ => {
                return Err(BoseError::InvalidArgument(format!(
                    "auto-off minutes {minutes} not in [0, 5, 20, 40, 60, 180]"
                )))
            }
        };
        p::set_auto_off(&mut h.conn, ao)?;
        Ok(())
    })
}

/// Change the noise-cancelling level. `level` must be 0x00, 0x01,
/// or 0x03. Caller should verify the device supports NC first via
/// [`bose_connect_has_noise_cancelling`].
#[no_mangle]
pub extern "C" fn bose_connect_set_noise_cancelling(
    handle: *mut BoseConnectHandle,
    level: u8,
) -> BoseFfiResult {
    run_block(handle, |h| {
        let nc = match level {
            0x00 => NoiseCancelling::Off,
            0x01 => NoiseCancelling::High,
            0x03 => NoiseCancelling::Low,
            _ => {
                return Err(BoseError::InvalidArgument(format!(
                    "noise cancelling level 0x{level:02x} out of range"
                )))
            }
        };
        p::set_noise_cancelling(&mut h.conn, nc)?;
        Ok(())
    })
}

/// Toggle pairing discoverability.
#[no_mangle]
pub extern "C" fn bose_connect_set_pairing(
    handle: *mut BoseConnectHandle,
    on: c_int,
) -> BoseFfiResult {
    run_block(handle, |h| {
        p::set_pairing(
            &mut h.conn,
            if on != 0 { Pairing::On } else { Pairing::Off },
        )?;
        Ok(())
    })
}

/// Set the self-voice (sidetone) level. `level` must be 0..=3.
#[no_mangle]
pub extern "C" fn bose_connect_set_self_voice(
    handle: *mut BoseConnectHandle,
    level: u8,
) -> BoseFfiResult {
    run_block(handle, |h| {
        let sv = match level {
            0x0 => SelfVoice::Off,
            0x1 => SelfVoice::High,
            0x2 => SelfVoice::Medium,
            0x3 => SelfVoice::Low,
            _ => {
                return Err(BoseError::InvalidArgument(format!(
                    "self voice level 0x{level:02x} out of range"
                )))
            }
        };
        p::set_self_voice(&mut h.conn, sv)?;
        Ok(())
    })
}

/// Pure-Rust predicate: returns 1 if the device id corresponds to a
/// noise-cancelling-capable device, 0 otherwise. Does not require a
/// handle — the device id is the only input.
#[no_mangle]
pub extern "C" fn bose_connect_has_noise_cancelling(device_id: u16) -> c_int {
    if p::has_noise_cancelling(device_id) {
        1
    } else {
        0
    }
}

// ---------------------------------------------------------------------------
// Paired-device commands
// ---------------------------------------------------------------------------

/// Connect / disconnect / remove a paired device by Bluetooth
/// address. `address_bytes` must point to 6 bytes in
/// canonical MSB-first order. `op` selects the operation:
///   * 0 → connect
///   * 1 → disconnect
///   * 2 → remove
#[no_mangle]
pub extern "C" fn bose_connect_paired_op(
    handle: *mut BoseConnectHandle,
    op: c_int,
    address_bytes: *const u8,
) -> BoseFfiResult {
    run_block(handle, |h| {
        if address_bytes.is_null() {
            return Err(BoseError::InvalidArgument(
                "address_bytes is null".to_string(),
            ));
        }
        let mut addr = BdAddr { b: [0; 6] };
        unsafe {
            std::ptr::copy_nonoverlapping(address_bytes, addr.b.as_mut_ptr(), 6);
        }
        match op {
            0 => p::connect_device(&mut h.conn, addr)?,
            1 => p::disconnect_device(&mut h.conn, addr)?,
            2 => p::remove_device(&mut h.conn, addr)?,
            _ => {
                return Err(BoseError::InvalidArgument(format!(
                    "op {op} not in {{0, 1, 2}}"
                )))
            }
        }
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// Raw packet pass-through (mirrors `do_send_packet` in main.c)
// ---------------------------------------------------------------------------

/// Send a raw packet and receive up to `out_received_cap` bytes of
/// the response. The packet is provided as a hex string
/// ("0a1b2c3d"). On success, `out_received` (if non-null) is filled
/// with the device's reply and `out_received_len` is updated to the
/// actual number of bytes received.
#[no_mangle]
pub extern "C" fn bose_connect_send_packet(
    handle: *mut BoseConnectHandle,
    hex_packet: *const c_char,
    out_received: *mut u8,
    out_received_cap: usize,
    out_received_len: *mut usize,
) -> BoseFfiResult {
    run_block(handle, |h| {
        let hex = cstr_required(hex_packet, "hex_packet")?;
        let bytes = parse_hex_packet(hex.as_str())?;
        let received = p::send_packet(&mut h.conn, &bytes)?;

        let n = received.len().min(out_received_cap);
        if !out_received.is_null() && n > 0 {
            unsafe {
                std::ptr::copy_nonoverlapping(received.as_ptr(), out_received, n);
            }
        }
        if !out_received_len.is_null() {
            unsafe {
                std::ptr::write(out_received_len, n);
            }
        }
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Run a fallible closure against a [`BoseConnectHandle`], catching
/// panics and converting the result into a [`BoseFfiResult`].
fn run_block<R, F>(handle: *mut BoseConnectHandle, f: F) -> BoseFfiResult
where
    F: FnOnce(&mut BoseConnectHandle) -> Result<R, BoseError> + std::panic::UnwindSafe,
{
    if handle.is_null() {
        return BoseFfiResult::from_err(&BoseError::InvalidArgument("null handle".to_string()));
    }
    let h_ref = unsafe { &mut *handle };
    match catch_unwind(AssertUnwindSafe(|| f(h_ref))) {
        Ok(Ok(_)) => BoseFfiResult::ok(),
        Ok(Err(e)) => BoseFfiResult::from_err(&e),
        Err(_) => {
            let e = BoseError::Io(std::io::Error::other("internal panic in bose_connect FFI"));
            BoseFfiResult::from_err(&e)
        }
    }
}

/// Read a NUL-terminated C string from `ptr` and convert it to a
/// Rust `String`. Mirrors what the C `do_*` argument parsers do:
/// any pointer-shaped input is accepted, but non-UTF-8 byte
/// sequences are rejected with [`BoseError::InvalidArgument`].
///
/// Caller guarantees that `ptr` is either NULL or points at a valid
/// NUL-terminated C string for the lifetime of the call. We do not
/// accept `unsafe` here because the pointer arithmetic stays inside
/// this function.
fn cstr_required(ptr: *const c_char, name: &'static str) -> Result<String, BoseError> {
    if ptr.is_null() {
        return Err(BoseError::InvalidArgument(format!(
            "{name} pointer is null"
        )));
    }
    // SAFETY: caller guarantees ptr is NULL or points to a valid
    // NUL-terminated C string. We do not retain the pointer past
    // this function's return.
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map(|s| s.to_owned())
        .map_err(|_| BoseError::InvalidArgument(format!("{name} is not valid UTF-8")))
}

fn write_cstr(out: *mut c_char, out_len: usize, s: &str) -> Result<(), BoseError> {
    if out.is_null() || out_len == 0 {
        return Err(BoseError::InvalidArgument(
            "output buffer pointer is null or length is 0".to_string(),
        ));
    }
    let bytes = s.as_bytes();
    let take = bytes.len().min(out_len - 1);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out as *mut u8, take);
        std::ptr::write(out.add(take), 0);
    }
    Ok(())
}

fn parse_hex_packet(s: &str) -> Result<Vec<u8>, BoseError> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        return Err(BoseError::InvalidArgument(format!(
            "hex packet length {} is odd",
            s.len()
        )));
    }
    let bytes_str = s.as_bytes();
    let mut out = Vec::with_capacity(bytes_str.len() / 2);
    let mut i = 0;
    while i < bytes_str.len() {
        let pair = [bytes_str[i], bytes_str[i + 1]];
        let b = crate::util::str_to_byte(&pair).ok_or_else(|| {
            BoseError::InvalidArgument(format!(
                "hex packet contains non-hex characters near position {i}"
            ))
        })?;
        out.push(b);
        i += 2;
    }
    Ok(out)
}

// Suppress dead-code warnings for items used only through cbindgen.
#[allow(dead_code)]
const _: usize = MAX_NAME_LEN;
#[allow(dead_code)]
const _: usize = MAX_NUM_DEVICES;
#[allow(dead_code)]
const _: u8 = PromptLanguage::En as u8;
