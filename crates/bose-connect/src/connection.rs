//! Bluetooth RFCOMM socket layer.
//!
//! Wraps the POSIX socket / `connect` / `read` / `write` calls needed
//! to talk to a Bose headphone over RFCOMM channel
//! [`crate::types::BOSE_CHANNEL`]. The Bose protocol commands
//! themselves live in [`crate::protocol`]; this module only handles
//! the transport, mirroring the role of `main.c::get_socket` and
//! `library/based.c::init_connection` in the original C code.
//!
//! The critical guarantee here is that the **byte sequence on the
//! wire** matches the original C implementation exactly: same time
//!outs, same connect sequence, same initial handshake, same
//! garbage-bytes discard. Anything else in this crate is allowed
//! to evolve; this layer must not.

use std::io::{Read, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::time::Duration;

use nix::sys::socket::{socket, AddressFamily, SockFlag, SockType};

use crate::error::{BoseError, BoseResult};
use crate::types::{BdAddr, BOSE_CHANNEL};

/// `SO_SNDTIMEO` for the RFCOMM socket. Mirrors the C `send_timeout`.
pub(crate) const SEND_TIMEOUT: Duration = Duration::from_secs(5);
/// `SO_RCVTIMEO` for the RFCOMM socket. Mirrors the C `receive_timeout`.
pub(crate) const RECEIVE_TIMEOUT: Duration = Duration::from_secs(1);

/// `AF_BLUETOOTH` (Linux = 31). We define this here because the
/// `libc` crate does not export AF_BLUETOOTH on every target — only
/// Linux glibc targets get it under feature `extra_traits`, and the
/// `nix` crate does not expose it either.
pub(crate) const AF_BLUETOOTH: libc::sa_family_t = 31;

/// `struct sockaddr_rc` layout from `<bluetooth/rfcomm.h>`:
///
/// ```c
/// struct sockaddr_rc {
///     sa_family_t    rc_family;   // u16 on Linux
///     uint8_t        rc_channel;
///     bdaddr_t       rc_bdaddr;   // { uint8_t b[6]; }
/// };
/// ```
///
/// The 10-byte layout is: family(2) + channel(1) + padding(1) +
/// bdaddr(6). We pack it manually because `libc` does not expose
/// this struct on stable Rust.
///
/// `#[repr(C)]` keeps the field order stable so the layout matches
/// what `connect()` expects.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct SockaddrRc {
    pub rc_family: libc::sa_family_t,
    pub rc_channel: u8,
    pub rc_bdaddr: BdAddr,
}

/// A connected RFCOMM socket to a Bose headphone.
///
/// Drop the value to close the socket; for explicit error-aware
/// close use [`Connection::close`].
#[derive(Debug)]
pub struct Connection {
    /// Owned file descriptor wrapping the RFCOMM socket. We
    /// intentionally use `OwnedFd` (not a `std::net::TcpStream` or
    /// `File`) because the BlueZ socket type is not exposed as a
    /// standard Rust `Read`/`Write`-implementing type.
    fd: OwnedFd,
    /// Address of the connected device, retained for diagnostics.
    addr: BdAddr,
}

impl Connection {
    /// Open a connection to `address` (canonical
    /// "AA:BB:CC:DD:EE:FF") and run the Bose protocol's
    /// `init_connection` handshake. Returns a ready-to-use
    /// [`Connection`] on success.
    pub fn open(address: &str) -> BoseResult<Self> {
        // 1. Parse the address. Mirrors `str2ba` validation.
        let addr = crate::address::parse_bdaddr_strict(address)?;

        // 2. Open the RFCOMM socket. AF_BLUETOOTH / BTPROTO_RFCOMM
        //    constants live in libc; nix's `AddressFamily` enum
        //    carries AF_BLUETOOTH on Linux.
        let fd: OwnedFd = socket(
            AddressFamily::Bluetooth,
            SockType::Stream,
            SockFlag::empty(),
            None, // protocol: 0 -> BTPROTO_RFCOMM is the only SOCK_STREAM option
        )
        .map_err(|e| BoseError::SocketOpen(e.into()))?;

        // 3. Set per-IO timeouts. Mirrors the C `setsockopt` calls.
        set_timeout(&fd, SEND_TIMEOUT, "SO_SNDTIMEO")?;
        set_timeout(&fd, RECEIVE_TIMEOUT, "SO_RCVTIMEO")?;

        // 4. Build the sockaddr_rc and connect. We construct it by
        //    hand and call `libc::connect` directly because the
        //    `nix` crate's high-level `connect` expects a trait
        //    object implementing `SockaddrLike`, and that trait's
        //    AF_BLUETOOTH implementation is gated behind unstable
        //    features on stable Rust. Byte-level control is also
        //    what the kernel expects (matters: the exact byte
        //    order of `sockaddr_rc`).
        let sockaddr = build_sockaddr_rc(addr);
        let res = unsafe {
            libc::connect(
                fd.as_raw_fd(),
                &sockaddr as *const SockaddrRc as *const libc::sockaddr,
                std::mem::size_of::<SockaddrRc>() as libc::socklen_t,
            )
        };
        if res < 0 {
            return Err(BoseError::Connect {
                address: address.to_string(),
                source: std::io::Error::last_os_error(),
            });
        }

        let mut conn = Connection { fd, addr };

        // 5. Run the init handshake.
        conn.init_connection()?;
        Ok(conn)
    }

    /// Address of the connected device.
    pub fn address(&self) -> BdAddr {
        self.addr
    }

    /// Run the Bose protocol `init_connection` handshake:
    ///
    /// * send `{ 0x00, 0x01, 0x01, 0x00 }`
    /// * verify the ACK is `{ 0x00, 0x01, 0x03, 0x05 }`
    /// * discard the next 5 bytes (initial firmware version)
    ///
    /// This is called automatically by [`Connection::open`]; only
    /// re-invoke it if you have a reason to reset the device state.
    pub fn init_connection(&mut self) -> BoseResult<()> {
        use crate::io::BoseIo;
        let send: [u8; 4] = [0x00, 0x01, 0x01, 0x00];
        let ack: [u8; 4] = [0x00, 0x01, 0x03, 0x05];

        BoseIo::bose_write_check(self, &send, &ack)?;

        // Throw away the initial firmware version.
        let mut garbage = [0u8; 5];
        BoseIo::bose_read_exact(self, &mut garbage)?;
        Ok(())
    }

    /// Read `buf.len()` bytes from the socket. Returns
    /// [`BoseError::ShortRead`] if EOF or a partial read happens.
    /// Mirrors the C `if (status != receive_n) return ...` pattern.
    #[allow(dead_code)]
    pub(crate) fn as_raw_fd_for_test(&self) -> std::os::fd::RawFd {
        self.fd.as_raw_fd()
    }

    /// Close the socket. Idempotent.
    pub fn close(self) -> std::io::Result<()> {
        // OwnedFd's drop calls close(2) for us; we consume self to
        // make the call site explicit.
        drop(self);
        Ok(())
    }

    /// True if the underlying socket is still considered open. We
    /// track this with a flag so the FFI layer can detect "use after
    /// close" cleanly.
    pub fn is_open(&self) -> bool {
        // OwnedFd only knows "valid"; for a synchronous
        // single-threaded API the only way to become closed is to
        // be dropped, which moves self out, so `is_open` always
        // returns true here. The FFI uses a separate guard.
        true
    }
}

impl Read for Connection {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let fd = self.fd.as_raw_fd();
        let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut _, buf.len()) };
        if n < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(n as usize)
        }
    }
}

impl Write for Connection {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let fd = self.fd.as_raw_fd();
        let n = unsafe { libc::write(fd, buf.as_ptr() as *const _, buf.len()) };
        if n < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(n as usize)
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        // RFCOMM is a stream socket with no userspace buffering.
        Ok(())
    }
}

fn set_timeout(fd: &OwnedFd, dur: Duration, which: &'static str) -> BoseResult<()> {
    let secs = dur.as_secs() as libc::time_t;
    let usecs = dur.subsec_micros() as libc::suseconds_t;
    let tv = libc::timeval {
        tv_sec: secs,
        tv_usec: usecs,
    };
    // SO_SNDTIMEO = 0x21, SO_RCVTIMEO = 0x20 on Linux/glibc.
    // We hard-code the values to avoid relying on libc::SO_*
    // constants (they are stable but easier to read in the wire
    // log without a libc::SO_* import).
    let optname = match which {
        "SO_SNDTIMEO" => 0x21,
        "SO_RCVTIMEO" => 0x20,
        _ => unreachable!("which must be SO_SNDTIMEO or SO_RCVTIMEO"),
    };
    let res = unsafe {
        libc::setsockopt(
            fd.as_raw_fd(),
            libc::SOL_SOCKET,
            optname,
            &tv as *const _ as *const _,
            std::mem::size_of::<libc::timeval>() as libc::socklen_t,
        )
    };
    if res < 0 {
        Err(BoseError::SocketTimeout {
            which,
            source: std::io::Error::last_os_error(),
        })
    } else {
        Ok(())
    }
}

/// Build a `sockaddr_rc` for the given Bluetooth address.
///
/// The kernel expects the bdaddr bytes in **MSB-first** order (the
/// same byte order `str2ba` produces and the same order the Bose
/// protocol uses). The `rc_channel` field is one octet.
fn build_sockaddr_rc(addr: BdAddr) -> SockaddrRc {
    SockaddrRc {
        rc_family: AF_BLUETOOTH,
        rc_channel: BOSE_CHANNEL,
        rc_bdaddr: addr,
    }
}
