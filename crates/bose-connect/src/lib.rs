//! # bose-connect
//!
//! Rust library for controlling Bose Bluetooth headphones over
//! RFCOMM (the reverse-engineered Bose Connect protocol).
//!
//! Two surfaces are exposed:
//!
//! 1. **Pure-Rust API** for application developers — see the
//!    [`protocol`] and [`types`] modules. The recommended entry
//!    point is [`BoseDevice`]: it owns a [`Connection`] and runs
//!    the init handshake, so callers don't have to manage socket
//!    lifetimes themselves.
//!
//! 2. **C ABI** for FFI consumers — see the [`ffi`] module. The
//!    generated `bose_connect.h` header (built by `cbindgen` from
//!    `cbindgen.toml`) is the canonical declaration. C/C++ projects
//!    link against `libbose_connect.so`; Python/Ruby/Go consumers
//!    use their language's C-FFI shim.
//!
//! ## Wire protocol fidelity
//!
//! Every public function in this crate corresponds to one C function
//! in the original `library/based.c`. The packet byte sequences,
//! masked-ACK masks, and short-read / short-write semantics are
//! preserved verbatim. If the original code sends
//! `[0x01, 0x02, 0x02, ANY]`, the Rust code sends exactly the same
//! bytes in the same order.
//!
//! ## Quick start
//!
//! ```no_run
//! use bose_connect::{BoseDevice, PromptLanguage};
//!
//! let mut device = BoseDevice::open("AA:BB:CC:DD:EE:FF").unwrap();
//! println!("Battery: {}%", device.battery_level().unwrap());
//! device.set_language_keep_voice_prompts(PromptLanguage::En).unwrap();
//! ```

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(unreachable_pub)]

pub mod address;
pub mod connection;
pub mod error;
pub mod io;
pub mod protocol;
pub mod types;
pub mod util;

#[cfg(feature = "ffi")]
pub mod ffi;

pub use crate::connection::Connection;
pub use crate::error::{BoseError, BoseResult};
pub use crate::io::BoseIo;
pub use crate::protocol::{
    get_battery_level, get_device_id, get_device_info, get_device_status, get_firmware_version,
    get_paired_devices, get_serial_number, has_noise_cancelling, set_auto_off, set_name,
    set_noise_cancelling, set_pairing, set_prompt_language, set_self_voice, set_voice_prompts,
    DeviceStatusReport, PairedDevices,
};
pub use crate::types::{
    AutoOff, BdAddr, Device as DeviceInfo, DeviceStatus, DevicesConnected, NoiseCancelling,
    Pairing, PromptLanguage, SelfVoice, BOSE_CHANNEL, MAX_BT_PACK_LEN, MAX_NAME_LEN,
    MAX_NUM_DEVICES, MAX_SERIAL_SIZE, VER_STR_LEN, VP_ENABLE_BIT, VP_MASK,
};

/// High-level driver: owns a `Connection` and exposes every
/// protocol command as a method. This is the recommended entry
/// point for Rust callers — it runs `init_connection` exactly once
/// at construction time and closes the socket on drop.
///
/// Named `BoseDevice` (rather than `Device`) to leave room for
/// [`crate::types::Device`] (the wire-format struct returned by
/// `get_device_info`) to also be re-exported under its natural
/// name in the future.
#[derive(Debug)]
pub struct BoseDevice {
    conn: Connection,
}

impl BoseDevice {
    /// Open a new connection to `address` and run the protocol
    /// handshake.
    ///
    /// # Errors
    ///
    /// Returns [`BoseError::InvalidAddress`] if `address` is not a
    /// canonical "AA:BB:CC:DD:EE:FF" string, or any of the socket
    /// / handshake errors on transport failure.
    pub fn open(address: &str) -> BoseResult<Self> {
        Ok(Self {
            conn: Connection::open(address)?,
        })
    }

    /// Borrow the underlying [`Connection`]. Most callers will not
    /// need this; the methods on [`BoseDevice`] cover the entire
    /// protocol surface. It exists for FFI shims and tests that
    /// need direct socket access.
    pub fn connection(&mut self) -> &mut Connection {
        &mut self.conn
    }

    /// Fetch the device id and index revision.
    pub fn device_id(&mut self) -> BoseResult<(u16, u8)> {
        get_device_id(&mut self.conn)
    }

    /// Fetch the firmware version string (5 ASCII chars).
    pub fn firmware_version(&mut self) -> BoseResult<String> {
        get_firmware_version(&mut self.conn)
    }

    /// Fetch the serial number string.
    pub fn serial_number(&mut self) -> BoseResult<String> {
        get_serial_number(&mut self.conn)
    }

    /// Fetch the battery level (0..=100).
    pub fn battery_level(&mut self) -> BoseResult<u8> {
        get_battery_level(&mut self.conn)
    }

    /// Fetch the full device status (name, language, auto-off,
    /// noise-cancelling level).
    pub fn device_status(&mut self) -> BoseResult<DeviceStatusReport> {
        get_device_status(&mut self.conn)
    }

    /// Fetch the paired-devices list.
    pub fn paired_devices(&mut self) -> BoseResult<PairedDevices> {
        get_paired_devices(&mut self.conn)
    }

    /// Fetch info for a single paired device.
    pub fn device_info(&mut self, address: BdAddr) -> BoseResult<DeviceInfo> {
        get_device_info(&mut self.conn, address)
    }

    /// Change the device name. `name` must be at most
    /// [`MAX_NAME_LEN`] - 1 (31) ASCII chars.
    pub fn set_name(&mut self, name: &str) -> BoseResult<()> {
        set_name(&mut self.conn, name)
    }

    /// Set the prompt language. The byte carries both the
    /// language code (low 5 bits) and the voice-prompts on/off
    /// flag (`VP_ENABLE_BIT`, bit 5). For the convenience helpers
    /// see [`BoseDevice::set_voice_prompts`] and
    /// [`BoseDevice::set_language_keep_voice_prompts`].
    pub fn set_prompt_language(&mut self, language_byte: u8) -> BoseResult<()> {
        set_prompt_language(&mut self.conn, language_byte)
    }

    /// Set the prompt language while preserving the current
    /// voice-prompts on/off state.
    pub fn set_language_keep_voice_prompts(&mut self, language: PromptLanguage) -> BoseResult<()> {
        let status = self.device_status()?;
        let new = (status.language & VP_ENABLE_BIT) | (language as u8);
        self.set_prompt_language(new)
    }

    /// Toggle voice prompts without changing the language.
    pub fn set_voice_prompts(&mut self, on: bool) -> BoseResult<()> {
        set_voice_prompts(&mut self.conn, on)
    }

    /// Change the auto-off timer.
    pub fn set_auto_off(&mut self, minutes: AutoOff) -> BoseResult<()> {
        set_auto_off(&mut self.conn, minutes)
    }

    /// Change the noise-cancelling level.
    pub fn set_noise_cancelling(&mut self, level: NoiseCancelling) -> BoseResult<()> {
        set_noise_cancelling(&mut self.conn, level)
    }

    /// Toggle pairing discoverability.
    pub fn set_pairing(&mut self, on: bool) -> BoseResult<()> {
        set_pairing(&mut self.conn, if on { Pairing::On } else { Pairing::Off })
    }

    /// Set the self-voice (sidetone) level.
    pub fn set_self_voice(&mut self, level: SelfVoice) -> BoseResult<()> {
        set_self_voice(&mut self.conn, level)
    }

    /// Connect to a paired device.
    pub fn connect_device(&mut self, address: BdAddr) -> BoseResult<()> {
        protocol::connect_device(&mut self.conn, address)
    }

    /// Disconnect a paired device.
    pub fn disconnect_device(&mut self, address: BdAddr) -> BoseResult<()> {
        protocol::disconnect_device(&mut self.conn, address)
    }

    /// Remove a paired device from the pairing list.
    pub fn remove_device(&mut self, address: BdAddr) -> BoseResult<()> {
        protocol::remove_device(&mut self.conn, address)
    }
}
