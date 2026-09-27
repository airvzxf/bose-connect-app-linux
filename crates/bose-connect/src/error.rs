//! Error types for the `bose-connect` library.
//!
//! Every fallible function returns [`Result<T, BoseError>`]. The C FFI
//! layer converts these to integer error codes (see
//! `BoseError::rc_code`) so C consumers can inspect failure modes
//! without a full Rust unwinder.

use std::fmt;

/// All errors that can be returned by this crate.
#[derive(Debug, thiserror::Error)]
pub enum BoseError {
    /// The Bluetooth address string was malformed (failed `bachk`).
    #[error("invalid bluetooth address: {0}")]
    InvalidAddress(String),

    /// Failed to open the RFCOMM socket (`socket(AF_BLUETOOTH, ...)`).
    #[error("failed to open bluetooth socket: {0}")]
    SocketOpen(#[source] std::io::Error),

    /// `setsockopt(SO_SNDTIMEO / SO_RCVTIMEO)` failed.
    #[error("failed to set socket timeout ({which}): {source}")]
    SocketTimeout {
        /// Which socket option failed (`SO_SNDTIMEO` or `SO_RCVTIMEO`).
        which: &'static str,
        /// Underlying OS error.
        #[source]
        source: std::io::Error,
    },

    /// `connect()` to the device failed.
    #[error("failed to connect to {address}: {source}")]
    Connect {
        /// The address we tried to connect to.
        address: String,
        /// Underlying OS error.
        #[source]
        source: std::io::Error,
    },

    /// `init_connection` handshake did not match the expected ACK.
    #[error("init_connection failed: device did not acknowledge the handshake")]
    InitFailed,

    /// Short read: the device returned fewer bytes than the
    /// requested fixed-size payload. Mirrors the C code's
    /// `status != receive_n` short-read path.
    #[error("short read: expected {expected} bytes, got {actual}")]
    ShortRead {
        /// How many bytes we asked for.
        expected: usize,
        /// How many the device actually returned.
        actual: isize,
    },

    /// Short write: the kernel accepted fewer bytes than the
    /// request. Mirrors the C code's `status != send_n` short-write
    /// path.
    #[error("short write: expected {expected} bytes, sent {actual}")]
    ShortWrite {
        /// How many bytes we asked for.
        expected: usize,
        /// How many the kernel actually accepted.
        actual: isize,
    },

    /// Masked ACK comparison failed: the device's response did not
    /// match the expected pattern under the protocol mask.
    #[error("device response did not match expected ACK")]
    AckMismatch,

    /// The value echoed by the device after a `set_*` command did
    /// not match what was sent. Mirrors the C `abs(sent - got)`
    /// status paths.
    #[error("device did not confirm setting: sent {sent:?}, got {got:?}")]
    UnconfirmedValue {
        /// What we sent.
        sent: String,
        /// What the device echoed.
        got: String,
    },

    /// The caller passed an out-of-range argument (e.g. an unknown
    /// language string, an auto-off value not in the device's enum).
    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    /// The operation was attempted on a closed connection.
    #[error("connection is closed")]
    Closed,

    /// Catch-all for unexpected `std::io::Error`s from `read`/`write`.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}

impl BoseError {
    /// Integer error code for the C FFI. Stable across versions;
    /// new variants append.
    #[doc(hidden)]
    pub fn rc_code(&self) -> i32 {
        match self {
            BoseError::InvalidAddress(_) => -1,
            BoseError::SocketOpen(_) => -2,
            BoseError::SocketTimeout { .. } => -3,
            BoseError::Connect { .. } => -4,
            BoseError::InitFailed => -5,
            BoseError::ShortRead { .. } => -6,
            BoseError::ShortWrite { .. } => -7,
            BoseError::AckMismatch => -8,
            BoseError::UnconfirmedValue { .. } => -9,
            BoseError::InvalidArgument(_) => -10,
            BoseError::Closed => -11,
            BoseError::Io(_) => -12,
        }
    }
}

/// Display helper for the FFI error-message buffer. The C side passes
/// a `char *` of fixed size and reads it back after the call.
///
/// Currently unused because the FFI module renders the message
/// inline. Kept for future expansion (e.g. when we add logging
/// hooks that want a stable string view of the error).
#[allow(dead_code)]
pub(crate) fn format_for_ffi(err: &BoseError) -> String {
    // Use the same `Display` impl but cap at 511 bytes to match the
    // conventional FFI buffer sizes.
    let mut s = format!("{}", err);
    s.truncate(511);
    s
}

/// Public convenience: a `Result` alias used throughout the crate.
pub type BoseResult<T> = Result<T, BoseError>;

/// Helper used by the FFI to wrap a formatted `Display` chain into a
/// stable `BoseError::UnconfirmedValue` without leaking the inner
/// types. Used by the `set_*` echo-check paths.
pub(crate) fn unconfirmed(sent: impl fmt::Display, got: impl fmt::Display) -> BoseError {
    BoseError::UnconfirmedValue {
        sent: sent.to_string(),
        got: got.to_string(),
    }
}
