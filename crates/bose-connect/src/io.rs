//! Transport-agnostic IO trait used by the protocol module.
//!
//! The protocol commands in [`crate::protocol`] are generic over
//! any type that implements [`BoseIo`]. Production code uses
//! `Connection` (an RFCOMM socket over Bluetooth); tests use a
//! `UnixStream` pair to simulate the device. Keeping the protocol
//! layer transport-agnostic makes it possible to write
//! deterministic, headless unit tests that exercise every packet
//! path without a physical headset.
//!
//! See [`crate::protocol`] for the trait-bound protocol functions
//! and `tests/protocol_roundtrip.rs` for the integration tests.

use std::io::{Read, Write};

use crate::error::{BoseError, BoseResult};

/// Minimum IO surface required by the protocol layer.
///
/// Implemented by [`crate::connection::Connection`] in production
/// and by `UnixStream` wrappers in tests. The trait is sealed
/// (`Sealed` super-trait) so external crates cannot add new
/// implementations — that keeps the wire-byte guarantees
/// (`bose_read_exact` blocking until the buffer is full, etc.)
/// in this crate's hands.
pub trait BoseIo: Read + Write + private::Sealed {
    /// Read exactly `buf.len()` bytes, blocking until the buffer
    /// is full or EOF is hit (in which case a [`BoseError::ShortRead`]
    /// is returned).
    ///
    /// Named `bose_read_exact` instead of `read_exact` to avoid the
    /// method-name collision with [`std::io::Read::read_exact`]
    /// (which has a different return type: `std::io::Result`
    /// instead of `BoseResult`).
    fn bose_read_exact(&mut self, buf: &mut [u8]) -> BoseResult<()>;

    /// Write all `buf` bytes, retrying on partial writes.
    fn bose_write_all(&mut self, buf: &[u8]) -> BoseResult<()>;

    /// Read and drop every byte that is already waiting, without
    /// blocking. Devices send packets nobody asked for (a second
    /// volume reply, active-source changes, track metadata, late
    /// results of a connect); none of them can be the reply to a
    /// request that has not been sent yet.
    fn bose_discard_pending(&mut self) -> BoseResult<()>;

    /// Send one request: drop whatever is still waiting from earlier
    /// traffic ([`BoseIo::bose_discard_pending`]), then write `packet`.
    /// Every protocol request goes through here, so its reply is the
    /// first thing read after it.
    fn bose_send(&mut self, packet: &[u8]) -> BoseResult<()> {
        self.bose_discard_pending()?;
        self.bose_write_all(packet)
    }

    /// Write `send`, then read `ack.len()` bytes and compare to
    /// `ack` byte-for-byte.
    fn bose_write_check(&mut self, send: &[u8], ack: &[u8]) -> BoseResult<()> {
        self.bose_write_all(send)?;
        let mut got = vec![0u8; ack.len()];
        self.bose_read_exact(&mut got)?;
        if got != ack {
            return Err(BoseError::AckMismatch);
        }
        Ok(())
    }
}

mod private {
    pub trait Sealed {}
    impl Sealed for crate::connection::Connection {}
    impl Sealed for std::os::unix::net::UnixStream {}
}

impl BoseIo for crate::connection::Connection {
    fn bose_read_exact(&mut self, buf: &mut [u8]) -> BoseResult<()> {
        let expected = buf.len();
        let mut got = 0usize;
        while got < expected {
            match Read::read(self, &mut buf[got..]) {
                Ok(0) => {
                    return Err(BoseError::ShortRead {
                        expected,
                        actual: got as isize,
                    });
                }
                Ok(n) => got += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    fn bose_write_all(&mut self, buf: &[u8]) -> BoseResult<()> {
        let expected = buf.len();
        let mut written = 0usize;
        while written < expected {
            match Write::write(self, &buf[written..]) {
                Ok(0) => {
                    return Err(BoseError::ShortWrite {
                        expected,
                        actual: written as isize,
                    });
                }
                Ok(n) => written += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    fn bose_discard_pending(&mut self) -> BoseResult<()> {
        let mut buf = [0u8; 256];
        loop {
            match self.recv_nonblocking(&mut buf) {
                Ok(0) => return Ok(()),
                Ok(_) => continue,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
                Err(e) => return Err(e.into()),
            }
        }
    }
}

/// `UnixStream` adapter that implements [`BoseIo`] for tests and
/// integration harnesses.
///
/// Driving a Bose device over a Unix socket is *meaningless* — the
/// device speaks RFCOMM, not Unix-domain — so this impl is
/// documented as "for testing only" and lives in the public API
/// specifically so the `tests/` integration suite can use it. It
/// is **not** gated behind `#[cfg(test)]` because Rust's
/// `tests/*.rs` files are a separate compilation unit and do not
/// see `#[cfg(test)]` items from the library crate.
impl BoseIo for std::os::unix::net::UnixStream {
    fn bose_read_exact(&mut self, buf: &mut [u8]) -> BoseResult<()> {
        let expected = buf.len();
        let mut got = 0usize;
        while got < expected {
            match Read::read(self, &mut buf[got..]) {
                Ok(0) => {
                    return Err(BoseError::ShortRead {
                        expected,
                        actual: got as isize,
                    });
                }
                Ok(n) => got += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    fn bose_write_all(&mut self, buf: &[u8]) -> BoseResult<()> {
        let expected = buf.len();
        let mut written = 0usize;
        while written < expected {
            match Write::write(self, &buf[written..]) {
                Ok(0) => {
                    return Err(BoseError::ShortWrite {
                        expected,
                        actual: written as isize,
                    });
                }
                Ok(n) => written += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    fn bose_discard_pending(&mut self) -> BoseResult<()> {
        self.set_nonblocking(true)?;
        let mut buf = [0u8; 256];
        let result = loop {
            match Read::read(self, &mut buf) {
                Ok(0) => break Ok(()),
                Ok(_) => continue,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break Ok(()),
                Err(e) => break Err(e.into()),
            }
        };
        self.set_nonblocking(false)?;
        result
    }
}
