//! Story S2: `connect_device`, `disconnect_device` and `remove_device`
//! report what the device answered.
//!
//! An ERROR packet is a `BoseError::DeviceError`, except the one
//! SoundLink Color II reply to `connect_device` (ERROR `0b`) that stays
//! accepted until a hardware test says what it means. The reply is read
//! packet by packet from its header, so the call returns as soon as the
//! reply is complete and the next request reads its own reply.
//!
//! Replies marked "capture" come from a SoundLink Color II on firmware
//! 4.0.1; "C" marks the acknowledgement the original `based.c`
//! expected; the others are synthetic.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use bose_connect::protocol::{connect_device, disconnect_device, get_battery_level, remove_device};
use bose_connect::{BdAddr, BoseError};

/// How long a client call may take before the test calls it hung.
const HANG_LIMIT: Duration = Duration::from_secs(2);

/// Run `client` against a fake device driven by `device`. Fails the
/// test if `client` does not return within [`HANG_LIMIT`].
fn run_against_device<R, C, D>(device: D, client: C) -> R
where
    R: Send + 'static,
    C: FnOnce(&mut UnixStream) -> R + Send + 'static,
    D: FnOnce(&mut UnixStream) + Send + 'static,
{
    let (mut client_end, mut device_end) = UnixStream::pair().expect("UnixStream::pair");
    let device_thread = thread::spawn(move || device(&mut device_end));
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(client(&mut client_end));
    });
    let result = rx
        .recv_timeout(HANG_LIMIT)
        .expect("the client call hung or panicked");
    device_thread.join().expect("the fake device panicked");
    result
}

/// Read one request of `N` bytes on the device side and check it.
fn expect_request<const N: usize>(device: &mut UnixStream, expected: [u8; N]) {
    let mut got = [0u8; N];
    device.read_exact(&mut got).expect("read request");
    assert_eq!(got, expected, "unexpected request bytes");
}

fn answer(device: &mut UnixStream, reply: &[u8]) {
    device.write_all(reply).expect("write reply");
}

/// Packet = `prefix` followed by the target address.
fn with_addr(prefix: &[u8]) -> Vec<u8> {
    let mut packet = prefix.to_vec();
    packet.extend_from_slice(&ADDR);
    packet
}

const ADDR: [u8; 6] = [0x04, 0x52, 0xc7, 0xba, 0x68, 0x0d];
const OTHER_ADDR: [u8; 6] = [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff];
const CONNECT_REQUEST: [u8; 11] = [
    0x04, 0x01, 0x05, 0x07, 0x00, 0x04, 0x52, 0xc7, 0xba, 0x68, 0x0d,
];
const DISCONNECT_REQUEST: [u8; 10] = [0x04, 0x02, 0x05, 0x06, 0x04, 0x52, 0xc7, 0xba, 0x68, 0x0d];
const REMOVE_REQUEST: [u8; 10] = [0x04, 0x03, 0x05, 0x06, 0x04, 0x52, 0xc7, 0xba, 0x68, 0x0d];
const BATTERY_REQUEST: [u8; 4] = [0x02, 0x02, 0x01, 0x00];
const BATTERY_REPLY_80: [u8; 5] = [0x02, 0x02, 0x03, 0x01, 0x50];

fn target() -> BdAddr {
    BdAddr { b: ADDR }
}

fn is_device_error(result: &Result<(), BoseError>, b: u8, f: u8, c: u8) -> bool {
    matches!(
        result,
        Err(BoseError::DeviceError { block, function, code })
            if (*block, *function, *code) == (b, f, c)
    )
}

// ---- success ----

#[test]
fn connect_succeeds_on_the_c_acknowledgement() {
    // C: `04 01 07 06 <addr>`.
    let result = run_against_device(
        |d| {
            expect_request(d, CONNECT_REQUEST);
            answer(d, &with_addr(&[0x04, 0x01, 0x07, 0x06]));
        },
        |c| connect_device(c, target()),
    );
    assert!(result.is_ok(), "got {result:?}");
}

#[test]
fn disconnect_succeeds_on_the_c_acknowledgement() {
    // C: `04 02 07 06 <addr>`.
    let result = run_against_device(
        |d| {
            expect_request(d, DISCONNECT_REQUEST);
            answer(d, &with_addr(&[0x04, 0x02, 0x07, 0x06]));
        },
        |c| disconnect_device(c, target()),
    );
    assert!(result.is_ok(), "got {result:?}");
}

#[test]
fn disconnect_succeeds_on_processing_then_result() {
    // Capture: `04 02 07 00`, then `04 02 06 06 <addr>`.
    let result = run_against_device(
        |d| {
            expect_request(d, DISCONNECT_REQUEST);
            answer(d, &[0x04, 0x02, 0x07, 0x00]);
            answer(d, &with_addr(&[0x04, 0x02, 0x06, 0x06]));
        },
        |c| disconnect_device(c, target()),
    );
    assert!(result.is_ok(), "got {result:?}");
}

#[test]
fn remove_succeeds_on_the_c_acknowledgement() {
    // C: `04 03 06 06 <addr>`.
    let result = run_against_device(
        |d| {
            expect_request(d, REMOVE_REQUEST);
            answer(d, &with_addr(&[0x04, 0x03, 0x06, 0x06]));
        },
        |c| remove_device(c, target()),
    );
    assert!(result.is_ok(), "got {result:?}");
}

#[test]
fn remove_succeeds_when_the_device_closes_without_answering() {
    let result = run_against_device(
        |d| expect_request(d, REMOVE_REQUEST),
        |c| remove_device(c, target()),
    );
    assert!(result.is_ok(), "got {result:?}");
}

#[test]
fn remove_succeeds_when_nothing_arrives_before_the_receive_timeout() {
    // Capture: the SoundLink Color II sends nothing back. On a real
    // connection the read ends with the socket's receive timeout.
    let result = run_against_device(
        |d| {
            expect_request(d, REMOVE_REQUEST);
            thread::sleep(Duration::from_millis(600));
        },
        |c| {
            c.set_read_timeout(Some(Duration::from_millis(200)))
                .expect("set_read_timeout");
            remove_device(c, target())
        },
    );
    assert!(result.is_ok(), "got {result:?}");
}

// ---- the SoundLink Color II connect reply ----

#[test]
fn connect_still_accepts_the_soundlink_color_ii_error_0b() {
    // Capture: `04 01 04 07 0b <addr>`. Kept as success until a
    // hardware test shows whether the source really connects.
    let result = run_against_device(
        |d| {
            expect_request(d, CONNECT_REQUEST);
            answer(d, &with_addr(&[0x04, 0x01, 0x04, 0x07, 0x0b]));
        },
        |c| connect_device(c, target()),
    );
    assert!(result.is_ok(), "got {result:?}");
}

// ---- errors ----

#[test]
fn connect_reports_any_other_error_code() {
    let result = run_against_device(
        |d| {
            expect_request(d, CONNECT_REQUEST);
            answer(d, &with_addr(&[0x04, 0x01, 0x04, 0x07, 0x01]));
        },
        |c| connect_device(c, target()),
    );
    assert!(is_device_error(&result, 0x04, 0x01, 0x01), "got {result:?}");
}

#[test]
fn disconnect_reports_error_0b() {
    let result = run_against_device(
        |d| {
            expect_request(d, DISCONNECT_REQUEST);
            answer(d, &with_addr(&[0x04, 0x02, 0x04, 0x07, 0x0b]));
        },
        |c| disconnect_device(c, target()),
    );
    assert!(is_device_error(&result, 0x04, 0x02, 0x0b), "got {result:?}");
}

#[test]
fn remove_reports_an_error_packet() {
    let result = run_against_device(
        |d| {
            expect_request(d, REMOVE_REQUEST);
            answer(d, &[0x04, 0x03, 0x04, 0x01, 0x05]);
        },
        |c| remove_device(c, target()),
    );
    assert!(is_device_error(&result, 0x04, 0x03, 0x05), "got {result:?}");
}

#[test]
fn a_reply_for_another_address_is_rejected() {
    let result = run_against_device(
        |d| {
            expect_request(d, CONNECT_REQUEST);
            let mut reply = vec![0x04, 0x01, 0x07, 0x06];
            reply.extend_from_slice(&OTHER_ADDR);
            answer(d, &reply);
        },
        |c| connect_device(c, target()),
    );
    assert!(
        matches!(result, Err(BoseError::AckMismatch)),
        "got {result:?}"
    );
}

#[test]
fn a_reply_for_another_function_is_rejected() {
    let result = run_against_device(
        |d| {
            expect_request(d, DISCONNECT_REQUEST);
            answer(d, &with_addr(&[0x04, 0x0f, 0x07, 0x06]));
        },
        |c| disconnect_device(c, target()),
    );
    assert!(
        matches!(result, Err(BoseError::AckMismatch)),
        "got {result:?}"
    );
}

#[test]
fn processing_followed_by_silence_is_not_success() {
    let result = run_against_device(
        |d| {
            expect_request(d, DISCONNECT_REQUEST);
            answer(d, &[0x04, 0x02, 0x07, 0x00]);
        },
        |c| disconnect_device(c, target()),
    );
    assert!(
        matches!(result, Err(BoseError::ShortRead { .. })),
        "got {result:?}"
    );
}

// ---- the stream stays in sync ----

#[test]
fn after_a_connect_error_the_next_request_reads_its_own_reply() {
    let (connect, battery) = run_against_device(
        |d| {
            expect_request(d, CONNECT_REQUEST);
            answer(d, &with_addr(&[0x04, 0x01, 0x04, 0x07, 0x01]));
            expect_request(d, BATTERY_REQUEST);
            answer(d, &BATTERY_REPLY_80);
        },
        |c| (connect_device(c, target()), get_battery_level(c)),
    );
    assert!(
        is_device_error(&connect, 0x04, 0x01, 0x01),
        "got {connect:?}"
    );
    assert_eq!(battery.expect("battery after the error"), 80);
}

#[test]
fn after_a_two_packet_disconnect_the_next_request_reads_its_own_reply() {
    let (disconnect, battery) = run_against_device(
        |d| {
            expect_request(d, DISCONNECT_REQUEST);
            answer(d, &[0x04, 0x02, 0x07, 0x00]);
            answer(d, &with_addr(&[0x04, 0x02, 0x06, 0x06]));
            expect_request(d, BATTERY_REQUEST);
            answer(d, &BATTERY_REPLY_80);
        },
        |c| (disconnect_device(c, target()), get_battery_level(c)),
    );
    assert!(disconnect.is_ok(), "got {disconnect:?}");
    assert_eq!(battery.expect("battery after disconnect"), 80);
}

#[test]
fn connect_returns_as_soon_as_the_reply_is_complete() {
    // The device keeps the connection open after its reply. The call
    // must not wait for more bytes.
    let elapsed = run_against_device(
        |d| {
            expect_request(d, CONNECT_REQUEST);
            answer(d, &with_addr(&[0x04, 0x01, 0x07, 0x06]));
            thread::sleep(Duration::from_millis(1500));
        },
        |c| {
            let start = Instant::now();
            let result = connect_device(c, target());
            (result.is_ok(), start.elapsed())
        },
    );
    assert!(elapsed.0, "connect failed");
    assert!(
        elapsed.1 < Duration::from_millis(500),
        "connect took {:?}",
        elapsed.1
    );
}
