//! Public domain types: enumerations and the `Device` struct.
//!
//! The values are preserved exactly as the reverse-engineered
//! protocol expects them; only the Rust naming follows Rust
//! conventions. The wire representation is unchanged from the
//! original C code.

/// Bose noise-cancelling modes.
///
/// `High` and `Low` are the user-selectable levels; `Dne` ("does
/// not exist") is what the device returns when it has no
/// noise-cancelling hardware.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum NoiseCancelling {
    /// Noise-cancelling off.
    Off = 0x00,
    /// High noise-cancelling level.
    High = 0x01,
    /// Low (lighter) noise-cancelling level.
    Low = 0x03,
    /// Returned by the device when noise-cancelling hardware is not
    /// present (e.g. SoundLink II). Never sent *to* the device.
    Dne = 0xff,
}

impl NoiseCancelling {
    /// Decode the on-wire byte.
    pub fn from_u8(b: u8) -> Option<Self> {
        Some(match b {
            0x00 => Self::Off,
            0x01 => Self::High,
            0x03 => Self::Low,
            0xff => Self::Dne,
            _ => return None,
        })
    }
}

/// Auto-off timer in minutes. `Never` is a sentinel for "disabled";
/// the device returns `0` to mean "no auto-off".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum AutoOff {
    /// No auto-off.
    Never = 0,
    /// 5 minutes.
    Min5 = 5,
    /// 20 minutes.
    Min20 = 20,
    /// 40 minutes.
    Min40 = 40,
    /// 60 minutes.
    Min60 = 60,
    /// 180 minutes (3 hours).
    Min180 = 180,
}

impl AutoOff {
    /// Parse a CLI argument (`never`, `5`, `20`, …). Mirrors the C
    /// `do_set_auto_off` parser.
    pub fn from_arg(s: &str) -> Option<Self> {
        Some(match s {
            "never" => Self::Never,
            _ => match s.parse::<u16>().ok()? {
                5 => Self::Min5,
                20 => Self::Min20,
                40 => Self::Min40,
                60 => Self::Min60,
                180 => Self::Min180,
                _ => return None,
            },
        })
    }
}

/// Voice-prompt language. The Bose protocol packs the language code
/// in the lower 5 bits of the byte (with bit 5 carrying the
/// voice-prompts on/off flag, see [`crate::VP_ENABLE_BIT`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PromptLanguage {
    /// English.
    En = 0x21,
    /// French.
    Fr = 0x22,
    /// Italian.
    It = 0x23,
    /// German.
    De = 0x24,
    /// Spanish.
    Es = 0x26,
    /// Portuguese.
    Pt = 0x27,
    /// Chinese.
    Zh = 0x28,
    /// Korean.
    Ko = 0x29,
    /// Russian.
    Ru = 0x2a,
    /// Polish.
    Pl = 0x2b,
    /// Dutch.
    Nl = 0x2e,
    /// Japanese.
    Ja = 0x2f,
    /// Swedish.
    Sv = 0x32,
}

impl PromptLanguage {
    /// Parse a CLI argument (`en`, `fr`, …).
    pub fn from_arg(s: &str) -> Option<Self> {
        Some(match s {
            "en" => Self::En,
            "fr" => Self::Fr,
            "it" => Self::It,
            "de" => Self::De,
            "es" => Self::Es,
            "pt" => Self::Pt,
            "zh" => Self::Zh,
            "ko" => Self::Ko,
            "ru" => Self::Ru,
            "pl" => Self::Pl,
            "nl" => Self::Nl,
            "ja" => Self::Ja,
            "sv" => Self::Sv,
            _ => return None,
        })
    }

    /// Decode the on-wire byte. The Bose protocol masks the language
    /// in the lower 5 bits and uses bit 5 (`VP_ENABLE_BIT`) as the
    /// voice-prompts flag, so callers should AND the byte with
    /// [`VP_MASK`] before calling this.
    pub fn from_u8(b: u8) -> Option<Self> {
        Some(match b {
            0x21 => Self::En,
            0x22 => Self::Fr,
            0x23 => Self::It,
            0x24 => Self::De,
            0x26 => Self::Es,
            0x27 => Self::Pt,
            0x28 => Self::Zh,
            0x29 => Self::Ko,
            0x2a => Self::Ru,
            0x2b => Self::Pl,
            0x2e => Self::Nl,
            0x2f => Self::Ja,
            0x32 => Self::Sv,
            _ => return None,
        })
    }

    /// 2-letter canonical string for printing (`EN`, `FR`, …).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::En => "EN",
            Self::Fr => "FR",
            Self::It => "IT",
            Self::De => "DE",
            Self::Es => "ES",
            Self::Pt => "PT",
            Self::Zh => "ZH",
            Self::Ko => "KO",
            Self::Ru => "RU",
            Self::Pl => "PL",
            Self::Nl => "NL",
            Self::Ja => "JA",
            Self::Sv => "SV",
        }
    }
}

/// Pairing discoverability mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Pairing {
    /// Pairing discoverability off.
    Off = 0x00,
    /// Pairing discoverability on.
    On = 0x01,
}

impl Pairing {
    /// Parse a CLI argument (`off`, `on`).
    pub fn from_arg(s: &str) -> Option<Self> {
        Some(match s {
            "off" => Self::Off,
            "on" => Self::On,
            _ => return None,
        })
    }
}

/// Self-voice (sidetone) level. Mirrors `enum SelfVoice` in the
/// original C header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SelfVoice {
    /// Self-voice off (no sidetone).
    Off = 0x0,
    /// High self-voice level.
    High = 0x1,
    /// Medium self-voice level.
    Medium = 0x2,
    /// Low self-voice level.
    Low = 0x3,
}

impl SelfVoice {
    /// Parse a CLI argument (`off`, `high`, `medium`, `low`).
    pub fn from_arg(s: &str) -> Option<Self> {
        Some(match s {
            "off" => Self::Off,
            "high" => Self::High,
            "medium" => Self::Medium,
            "low" => Self::Low,
            _ => return None,
        })
    }
}

/// Media transport key. Mirrors the `xx` byte sent in the Bose
/// `send_media_key` packet (`05 03 05 01 0n`).
///
/// Named `Pause` because that's what the operator-facing CLI flag
/// calls it, but on the wire it's a play/pause toggle: `0x01` flips
/// between playing and paused depending on the current state.
///
/// Byte `0x02` is deliberately not exposed. On a SoundLink Color II
/// (firmware 4.0.1) the speaker acknowledges it like the other keys,
/// but the source's player sees nothing — neither play nor pause,
/// whether it was playing or paused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum MediaKey {
    /// Toggle play / pause (Bose wire byte 0x01). The CLI exposes
    /// this as `pause`; the actual effect is play↔pause.
    Pause = 0x01,
    /// Skip to next track.
    Next = 0x03,
    /// Skip to previous track.
    Previous = 0x04,
}

impl MediaKey {
    /// Parse a CLI argument (`pause`, `next`, `prev`).
    pub fn from_arg(s: &str) -> Option<Self> {
        Some(match s {
            "pause" => Self::Pause,
            "next" => Self::Next,
            "prev" | "previous" => Self::Previous,
            _ => return None,
        })
    }
}

/// Per-device status byte returned by the paired-devices query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum DeviceStatus {
    /// Device is paired but not currently connected.
    Disconnected = 0x00,
    /// Device is connected to the headphone.
    Connected = 0x01,
    /// "This" device — the one the user is currently talking to.
    This = 0x03,
}

impl DeviceStatus {
    /// Decode the on-wire byte.
    pub fn from_u8(b: u8) -> Option<Self> {
        Some(match b {
            0x00 => Self::Disconnected,
            0x01 => Self::Connected,
            0x03 => Self::This,
            _ => return None,
        })
    }

    /// One-char CLI glyph (`!` for this, `*` for connected, ` `
    /// for disconnected).
    pub fn glyph(self) -> char {
        match self {
            Self::This => '!',
            Self::Connected => '*',
            Self::Disconnected => ' ',
        }
    }
}

/// Number of devices currently connected to the headphone's A2DP
/// sink. The Bose protocol encodes "two devices" as the surprising
/// value `0x03`; one device is `0x01`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum DevicesConnected {
    /// One device connected.
    One = 0x01,
    /// Two devices connected.
    Two = 0x03,
    /// The device returned an out-of-range byte.
    Unknown,
}

impl DevicesConnected {
    /// Decode the on-wire byte. Returns [`Self::Unknown`] for any
    /// out-of-range value (callers should treat as an error).
    pub fn from_u8(b: u8) -> Option<Self> {
        Some(match b {
            0x01 => Self::One,
            0x03 => Self::Two,
            _ => return None,
        })
    }

    /// Numeric count for printing (`1` or `2`). Returns `None` for
    /// [`Self::Unknown`].
    pub fn count(self) -> Option<u8> {
        Some(match self {
            Self::One => 1,
            Self::Two => 2,
            Self::Unknown => return None,
        })
    }
}

/// A device in the paired-devices list.
///
/// Layout is fixed for FFI compatibility; the order matches the
/// original `struct Device`. The address, status, and name are
/// populated by [`crate::protocol::get_device_info`].
#[derive(Debug, Clone)]
pub struct Device {
    /// 6-byte Bluetooth device address, MSB first as the device
    /// emits it (i.e. the order is the opposite of how BlueZ's
    /// `bdaddr_t` stores bytes for some platforms).
    pub address: BdAddr,
    /// Connection status byte.
    pub status: DeviceStatus,
    /// UTF-8 / ASCII device name. Fixed-size to mirror the C struct.
    pub name: [u8; MAX_NAME_LEN],
    /// Number of valid bytes in `name` (always written by the
    /// library on a successful read; convenience for printing).
    pub name_len: usize,
}

/// Six-byte Bluetooth device address stored in **canonical** byte
/// order: `b[0]` is the most-significant byte of the address as
/// printed in "AA:BB:CC:DD:EE:FF" form. The Bose device emits
/// addresses in this same canonical order, so no byte swapping is
/// needed when round-tripping through the protocol. The historical
/// `reverse_ba2str` / `reverse_str2ba` names in the C codebase are
/// misleading: both functions operate on the canonical MSB-first
/// representation; the `reverse_` prefix refers to them being the
/// inverse of each other, not to any byte reversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct BdAddr {
    /// Six address bytes, MSB first.
    pub b: [u8; 6],
}

impl BdAddr {
    /// All-zero address (BDADDR_ANY equivalent).
    pub const ANY: Self = Self { b: [0; 6] };

    /// Build from a canonical "AA:BB:CC:DD:EE:FF" string. Returns
    /// `None` if the input is not a valid address.
    pub fn from_canonical(s: &str) -> Option<Self> {
        crate::address::parse_bdaddr(s)
    }
}

/// Maximum bytes for a Bose device name. Mirrors `MAX_NAME_LEN`.
pub const MAX_NAME_LEN: usize = 0x20;

/// Maximum number of devices in the paired-devices list. Mirrors
/// `MAX_NUM_DEVICES`.
pub const MAX_NUM_DEVICES: usize = 8;

/// Maximum payload size for `send_packet` / `init_connection` garbage
/// reads. Mirrors `MAX_BT_PACK_LEN`.
pub const MAX_BT_PACK_LEN: usize = 0x1000;

/// Length of the firmware-version string buffer including the
/// trailing NUL. Mirrors `VER_STR_LEN`.
pub const VER_STR_LEN: usize = 6;

/// Maximum bytes for the serial-number buffer including NUL.
/// Mirrors `MAX_SERIAL_SIZE`.
pub const MAX_SERIAL_SIZE: usize = 0x100;

/// RFCOMM channel the Bose Connect service listens on. Mirrors
/// `BOSE_CHANNEL`.
pub const BOSE_CHANNEL: u8 = 8;

/// Channels tried, in order, when the connection on
/// [`BOSE_CHANNEL`] is refused. The QC Ultra Headphones expose the
/// same protocol on the serial-port channel 2.
pub const BOSE_FALLBACK_CHANNELS: [u8; 1] = [2];

/// Number of audio-mode slots on devices with audio modes. The QC
/// Ultra answers slots 0..=9 and returns an error for slot 10.
pub const MAX_AUDIO_MODES: u8 = 10;

/// Bitmask for the voice-prompt language bits of the prompt-language
/// response byte (low 5 bits hold the language, bit 7 is an echo
/// status flag). Mirrors `VP_MASK`.
pub const VP_MASK: u8 = 0x7f;

/// Bit that, when set, indicates "voice-prompts on" in the
/// prompt-language response byte (bit 5). Mirrors `VP_ENABLE_BIT`.
pub const VP_ENABLE_BIT: u8 = 0x20;

/// Device IDs known to support hardware noise-cancelling. Mirrors
/// the `NOISE_CANCELLING_*` macros in `based.c`.
pub(crate) const NOISE_CANCELLING_14: u16 = 0x4014;
/// Device ID 0x4020 — QC35 II series. Mirrors `NOISE_CANCELLING_20`.
pub(crate) const NOISE_CANCELLING_20: u16 = 0x4020;
/// Device ID 0x400c — SoundLink II? Mirrors `NOISE_CANCELLING_0C`.
pub(crate) const NOISE_CANCELLING_0C: u16 = 0x400c;
/// Device ID 0x4066 — Bose QC Ultra Headphones.
pub(crate) const QC_ULTRA_HEADPHONES: u16 = 0x4066;

/// `CN_BASE_PACK_LEN + MAX_NAME_LEN - 1` — the worst-case length of
/// a `SET_NAME` packet (4-byte prefix plus 31 name bytes). Re-exported
/// here so any drift in the constants above shows up as a compile
/// error at the definition site.
pub const MAX_NAME_PACK_LEN_HELPER: usize = 4 + MAX_NAME_LEN - 1;
