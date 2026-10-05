//! Regression test for `send_packet`: must return promptly after the
//! device sends its response, not block until the socket is closed.
//!
//! The C version in `based.c` did a single `read()` call:
//!
//! ```c
//! int send_packet(int sock, const void *send, size_t send_n,
//!                 uint8_t received[MAX_BT_PACK_LEN]) {
//!   int status = (int)write(sock, send, send_n);
//!   if (status != send_n) {
//!     return status ? status : 1;
//!   }
//!   return (int)read(sock, received, MAX_BT_PACK_LEN);
//! }
//! ```
//!
//! The original Rust port added a loop that kept reading until either
//! the buffer was full, EOF, or the SO_RCVTIMEO fired. That loop
//! produces two observable regressions:
//!
//! 1. In tests with `UnixStream` (no SO_RCVTIMEO), `send_packet`
//!    blocks forever waiting for more data after the device has
//!    already sent the complete response.
//! 2. In production with a real Bose RFCOMM socket, `send_packet`
//!    still spends an extra second waiting for the SO_RCVTIMEO after
//!    the device finishes sending, delaying the binary's exit by
//!    that amount on every `--send-packet` invocation.
//!
//! Both are a deviation from the original C semantics and from the
//! `AGENTS.md` requirement that the protocol layer be a line-for-line
//! translation of `based.c`. The fix matches the C version exactly:
//! a single `read()` call.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::thread;
use std::time::{Duration, Instant};

use bose_connect::protocol::send_packet;

/// Build a UnixStream pair.
fn pair() -> (UnixStream, UnixStream) {
    UnixStream::pair().expect("UnixStream::pair")
}

/// `send_packet` must return within a tight bound (a few hundred ms
/// of slack, definitely < 1 second) after the device finishes
/// sending. Anything close to the 10-second socket keep-alive the
/// server uses is a clear sign the read loop is still spinning.
#[test]
fn send_packet_returns_after_first_response() {
    const SERVER_KEEP_ALIVE: Duration = Duration::from_secs(10);
    const ACCEPTABLE_DELAY: Duration = Duration::from_millis(500);

    let (mut client, mut server) = pair();

    let _server_handle = thread::spawn(move || {
        // Drain the sent packet
        let mut buf = [0u8; 64];
        let _ = server.read(&mut buf);
        // Send a single 5-byte Bose-style response, flush, then keep
        // the socket open for 10 seconds. If `send_packet` is well-
        // behaved it returns ~immediately; if the bug is back it
        // blocks until this 10-second timer expires.
        server.write_all(&[0x05, 0x05, 0x03, 0x02, 0x64]).unwrap();
        server.flush().ok();
        thread::sleep(SERVER_KEEP_ALIVE);
        drop(server);
    });

    let start = Instant::now();
    let result = send_packet(&mut client, &[0x05, 0x05, 0x02, 0x01, 0x0a]);
    let elapsed = start.elapsed();

    // Note: we deliberately do NOT `join()` the server handle. The
    // server thread is just there to keep the socket alive long
    // enough that a buggy `send_packet` would hang. Joining it
    // would mask the bug (the test would take 10 s either way).
    // The OS reaps the thread when the test process exits.
    drop(client);

    assert!(
        elapsed < ACCEPTABLE_DELAY,
        "send_packet took {elapsed:?} (expected < {ACCEPTABLE_DELAY:?}); \
         the read loop is still busy-waiting after the device finished sending"
    );

    let got = result.expect("send_packet returned an error");
    assert_eq!(got, vec![0x05, 0x05, 0x03, 0x02, 0x64]);
}

/// Sanity check: an empty/EOF response must surface as zero bytes,
/// not hang or error.
#[test]
fn send_packet_handles_empty_response() {
    let (mut client, mut server) = pair();

    let _server_handle = thread::spawn(move || {
        // Drain the sent packet but send nothing back. Closing the
        // server end causes read() to return 0 (EOF).
        let mut buf = [0u8; 64];
        let _ = server.read(&mut buf);
        drop(server);
    });

    let start = Instant::now();
    let result = send_packet(&mut client, &[0x05, 0x05, 0x02, 0x01, 0x0a]);
    let elapsed = start.elapsed();

    drop(client);

    assert!(
        elapsed < Duration::from_millis(500),
        "send_packet took {elapsed:?} on empty response"
    );

    // Either Ok(empty) or a clean error is acceptable; what matters
    // is that the call returned promptly and didn't hang.
    if let Ok(v) = &result {
        assert!(v.is_empty(), "unexpected bytes: {v:?}");
    }
}
