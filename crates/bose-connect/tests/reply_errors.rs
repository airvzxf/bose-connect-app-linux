//! Story S1: every reply is read from its `block, function, operator,
//! length` header by one reader.
//!
//! `set_volume`, `send_media_key`, `get_active_device` and
//! `get_device_bd_addr` turn an ERROR packet into
//! `BoseError::DeviceError`, reject payloads of the wrong length, and
//! consume exactly their reply, so the next request on the same
//! connection reads its own reply.
//!
//! Replies marked "capture" come from real devices (see
//! `DEVELOPMENT.md`); the others are synthetic.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use bose_connect::protocol::{
    get_active_device, get_battery_level, get_device_bd_addr, send_media_key, set_volume,
};
use bose_connect::{BdAddr, BoseError, MediaKey};

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

const BATTERY_REQUEST: [u8; 4] = [0x02, 0x02, 0x01, 0x00];
const BATTERY_REPLY_80: [u8; 5] = [0x02, 0x02, 0x03, 0x01, 0x50];
const ACTIVE_DEVICE_REQUEST: [u8; 4] = [0x05, 0x01, 0x01, 0x00];
const BD_ADDR_REQUEST: [u8; 4] = [0x00, 0x06, 0x01, 0x00];

fn is_device_error(result: &Result<impl std::fmt::Debug, BoseError>, b: u8, f: u8, c: u8) -> bool {
    matches!(
        result,
        Err(BoseError::DeviceError { block, function, code })
            if (*block, *function, *code) == (b, f, c)
    )
}

// ---- send_media_key ----

#[test]
fn a_media_key_rejected_by_the_qc_ultra_is_a_device_error() {
    // Capture, QC Ultra firmware 1.6.7: every media key gets ERROR 0c.
    let result = run_against_device(
        |d| {
            expect_request(d, [0x05, 0x03, 0x05, 0x01, 0x01]);
            answer(d, &[0x05, 0x03, 0x04, 0x01, 0x0c]);
        },
        |c| send_media_key(c, MediaKey::Pause),
    );
    assert!(is_device_error(&result, 0x05, 0x03, 0x0c), "got {result:?}");
}

#[test]
fn after_a_rejected_media_key_the_next_request_reads_its_own_reply() {
    let (key, battery) = run_against_device(
        |d| {
            expect_request(d, [0x05, 0x03, 0x05, 0x01, 0x03]);
            answer(d, &[0x05, 0x03, 0x04, 0x01, 0x0c]);
            expect_request(d, BATTERY_REQUEST);
            answer(d, &BATTERY_REPLY_80);
        },
        |c| (send_media_key(c, MediaKey::Next), get_battery_level(c)),
    );
    assert!(is_device_error(&key, 0x05, 0x03, 0x0c), "got {key:?}");
    assert_eq!(battery.expect("battery after the error"), 80);
}

#[test]
fn a_media_key_answered_with_processing_then_result_succeeds() {
    // Capture, SoundLink Color II firmware 4.0.1.
    let result = run_against_device(
        |d| {
            expect_request(d, [0x05, 0x03, 0x05, 0x01, 0x04]);
            answer(d, &[0x05, 0x03, 0x07, 0x00, 0x05, 0x03, 0x06, 0x00]);
        },
        |c| send_media_key(c, MediaKey::Previous),
    );
    assert!(result.is_ok(), "got {result:?}");
}

// ---- set_volume ----

#[test]
fn the_qc_ultra_volume_reply_reports_a_scale_of_31() {
    // Capture, QC Ultra firmware 1.6.7.
    let result = run_against_device(
        |d| {
            expect_request(d, [0x05, 0x05, 0x02, 0x01, 0x0a]);
            answer(d, &[0x05, 0x05, 0x03, 0x02, 0x1f, 0x0a]);
        },
        |c| set_volume(c, 0x0a),
    );
    assert_eq!(result.expect("set_volume"), 31);
}

#[test]
fn a_volume_reply_after_a_processing_packet_is_read() {
    let result = run_against_device(
        |d| {
            expect_request(d, [0x05, 0x05, 0x02, 0x01, 0x0a]);
            answer(d, &[0x05, 0x05, 0x07, 0x00]);
            answer(d, &[0x05, 0x05, 0x03, 0x02, 0x64, 0x0a]);
        },
        |c| set_volume(c, 0x0a),
    );
    assert_eq!(result.expect("set_volume"), 100);
}

#[test]
fn a_volume_reply_with_a_one_byte_payload_is_rejected_and_consumed() {
    let (volume, battery) = run_against_device(
        |d| {
            expect_request(d, [0x05, 0x05, 0x02, 0x01, 0x0a]);
            answer(d, &[0x05, 0x05, 0x03, 0x01, 0x64]);
            expect_request(d, BATTERY_REQUEST);
            answer(d, &BATTERY_REPLY_80);
        },
        |c| (set_volume(c, 0x0a), get_battery_level(c)),
    );
    assert!(
        matches!(volume, Err(BoseError::AckMismatch)),
        "got {volume:?}"
    );
    assert_eq!(battery.expect("battery after the bad reply"), 80);
}

// ---- get_active_device ----

#[test]
fn an_error_reply_to_get_active_device_is_a_device_error_and_consumed() {
    let (active, battery) = run_against_device(
        |d| {
            expect_request(d, ACTIVE_DEVICE_REQUEST);
            answer(d, &[0x05, 0x01, 0x04, 0x01, 0x05]);
            expect_request(d, BATTERY_REQUEST);
            answer(d, &BATTERY_REPLY_80);
        },
        |c| (get_active_device(c), get_battery_level(c)),
    );
    assert!(is_device_error(&active, 0x05, 0x01, 0x05), "got {active:?}");
    assert_eq!(battery.expect("battery after the error"), 80);
}

#[test]
fn an_active_device_payload_that_is_not_nine_bytes_is_rejected() {
    let result = run_against_device(
        |d| {
            expect_request(d, ACTIVE_DEVICE_REQUEST);
            answer(
                d,
                &[0x05, 0x01, 0x03, 0x06, 0xd4, 0x6d, 0x6d, 0x17, 0x4d, 0xa4],
            );
        },
        get_active_device::<UnixStream>,
    );
    assert!(
        matches!(result, Err(BoseError::AckMismatch)),
        "got {result:?}"
    );
}

#[test]
fn the_active_device_is_the_last_six_payload_bytes() {
    // Capture, SoundLink Color II firmware 4.0.1.
    let result = run_against_device(
        |d| {
            expect_request(d, ACTIVE_DEVICE_REQUEST);
            answer(
                d,
                &[
                    0x05, 0x01, 0x03, 0x09, 0x00, 0x02, 0x01, 0xd4, 0x6d, 0x6d, 0x17, 0x4d, 0xa4,
                ],
            );
        },
        get_active_device::<UnixStream>,
    );
    assert_eq!(
        result.expect("get_active_device"),
        BdAddr {
            b: [0xd4, 0x6d, 0x6d, 0x17, 0x4d, 0xa4]
        }
    );
}

// ---- get_device_bd_addr ----

#[test]
fn an_error_reply_to_get_device_bd_addr_is_a_device_error_and_consumed() {
    let (own, battery) = run_against_device(
        |d| {
            expect_request(d, BD_ADDR_REQUEST);
            answer(d, &[0x00, 0x06, 0x04, 0x01, 0x05]);
            expect_request(d, BATTERY_REQUEST);
            answer(d, &BATTERY_REPLY_80);
        },
        |c| (get_device_bd_addr(c), get_battery_level(c)),
    );
    assert!(is_device_error(&own, 0x00, 0x06, 0x05), "got {own:?}");
    assert_eq!(battery.expect("battery after the error"), 80);
}

#[test]
fn a_bd_addr_payload_that_is_not_six_bytes_is_rejected() {
    let result = run_against_device(
        |d| {
            expect_request(d, BD_ADDR_REQUEST);
            answer(d, &[0x00, 0x06, 0x03, 0x05, 0x04, 0x52, 0xc7, 0xba, 0x68]);
        },
        get_device_bd_addr::<UnixStream>,
    );
    assert!(
        matches!(result, Err(BoseError::AckMismatch)),
        "got {result:?}"
    );
}
