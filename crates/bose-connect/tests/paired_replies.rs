//! `connect_device`, `disconnect_device` and `remove_device` report what
//! the device finally answered.
//!
//! The replies below are the sequences captured on 2026-10-07 from a
//! SoundLink Color II (firmware 4.0.1), a QC35 II (4.8.1) and a QC Ultra
//! (1.6.7), with the phone 00:62:01:8C:0C:B7 as the source; "C" marks the
//! acknowledgement the original `based.c` expected; the others are
//! synthetic. A command succeeds on the RESULT that carries the
//! requested address, which can arrive seconds after the PROCESSING
//! packet; notifications of other functions arrive in between.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use bose_connect::protocol::{connect_device, disconnect_device, get_battery_level, remove_device};
use bose_connect::{BdAddr, BoseError};

/// How long a client call may take before the test calls it hung.
const HANG_LIMIT: Duration = Duration::from_secs(5);

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

/// `prefix`, then `addr`, then `suffix`.
fn packet(prefix: &[u8], addr: [u8; 6], suffix: &[u8]) -> Vec<u8> {
    let mut p = prefix.to_vec();
    p.extend_from_slice(&addr);
    p.extend_from_slice(suffix);
    p
}

const PHONE: [u8; 6] = [0x00, 0x62, 0x01, 0x8c, 0x0c, 0xb7];
const LAPTOP: [u8; 6] = [0xd4, 0x6d, 0x6d, 0x17, 0x4d, 0xa4];
const RANDOM: [u8; 6] = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
const QC_ULTRA: [u8; 6] = [0xbc, 0x87, 0xfa, 0x45, 0x4c, 0xc2];

const BATTERY_REQUEST: [u8; 4] = [0x02, 0x02, 0x01, 0x00];
const BATTERY_REPLY_80: [u8; 5] = [0x02, 0x02, 0x03, 0x01, 0x50];

/// `05 01 03 09 00 02 01 <addr>`: active-source notification.
fn active_source(addr: [u8; 6]) -> Vec<u8> {
    packet(&[0x05, 0x01, 0x03, 0x09, 0x00, 0x02, 0x01], addr, &[])
}

fn connect_request(addr: [u8; 6]) -> [u8; 11] {
    let mut r = [0x04, 0x01, 0x05, 0x07, 0x00, 0, 0, 0, 0, 0, 0];
    r[5..].copy_from_slice(&addr);
    r
}

fn disconnect_request(addr: [u8; 6]) -> [u8; 10] {
    let mut r = [0x04, 0x02, 0x05, 0x06, 0, 0, 0, 0, 0, 0];
    r[4..].copy_from_slice(&addr);
    r
}

fn remove_request(addr: [u8; 6]) -> [u8; 10] {
    let mut r = [0x04, 0x03, 0x05, 0x06, 0, 0, 0, 0, 0, 0];
    r[4..].copy_from_slice(&addr);
    r
}

fn is_device_error(result: &Result<(), BoseError>, b: u8, f: u8, c: u8) -> bool {
    matches!(
        result,
        Err(BoseError::DeviceError { block, function, code })
            if (*block, *function, *code) == (b, f, c)
    )
}

// ---- connect ----

#[test]
fn connect_succeeds_when_the_result_arrives_after_processing_and_notifications() {
    // Capture, QC35 II: reconnecting the phone.
    let result = run_against_device(
        |d| {
            expect_request(d, connect_request(PHONE));
            answer(d, &packet(&[0x04, 0x01, 0x07, 0x06], PHONE, &[]));
            thread::sleep(Duration::from_millis(300));
            answer(d, &active_source(LAPTOP));
            answer(d, &packet(&[0x04, 0x01, 0x06, 0x07], PHONE, &[0x01]));
        },
        |c| connect_device(c, BdAddr { b: PHONE }),
    );
    assert!(result.is_ok(), "got {result:?}");
}

#[test]
fn connect_waits_through_receive_timeouts_for_the_result() {
    // The RESULT came ~8 s after PROCESSING on the real devices; here
    // 0.7 s with a 0.2 s receive timeout.
    let result = run_against_device(
        |d| {
            expect_request(d, connect_request(PHONE));
            answer(d, &packet(&[0x04, 0x01, 0x07, 0x06], PHONE, &[]));
            thread::sleep(Duration::from_millis(700));
            answer(d, &packet(&[0x04, 0x01, 0x06, 0x07], PHONE, &[0x01]));
        },
        |c| {
            c.set_read_timeout(Some(Duration::from_millis(200)))
                .expect("set_read_timeout");
            connect_device(c, BdAddr { b: PHONE })
        },
    );
    assert!(result.is_ok(), "got {result:?}");
}

#[test]
fn connecting_an_already_connected_source_succeeds() {
    // Capture, SoundLink Color II: RESULT code 07 at once.
    let result = run_against_device(
        |d| {
            expect_request(d, connect_request(PHONE));
            answer(d, &packet(&[0x04, 0x01, 0x07, 0x06], PHONE, &[]));
            answer(d, &packet(&[0x04, 0x01, 0x06, 0x07], PHONE, &[0x07]));
        },
        |c| connect_device(c, BdAddr { b: PHONE }),
    );
    assert!(result.is_ok(), "got {result:?}");
}

#[test]
fn connecting_an_address_that_is_not_a_paired_source_is_error_0b() {
    // Capture, SoundLink Color II and QC35 II and QC Ultra.
    let result = run_against_device(
        |d| {
            expect_request(d, connect_request(RANDOM));
            answer(d, &packet(&[0x04, 0x01, 0x04, 0x07, 0x0b], RANDOM, &[]));
        },
        |c| connect_device(c, BdAddr { b: RANDOM }),
    );
    assert!(is_device_error(&result, 0x04, 0x01, 0x0b), "got {result:?}");
}

#[test]
fn a_late_error_after_processing_is_a_device_error() {
    // Capture, QC Ultra: connecting its own address.
    let result = run_against_device(
        |d| {
            expect_request(d, connect_request(QC_ULTRA));
            answer(d, &packet(&[0x04, 0x01, 0x07, 0x06], QC_ULTRA, &[]));
            answer(d, &packet(&[0x04, 0x01, 0x04, 0x07, 0x0d], QC_ULTRA, &[]));
        },
        |c| connect_device(c, BdAddr { b: QC_ULTRA }),
    );
    assert!(is_device_error(&result, 0x04, 0x01, 0x0d), "got {result:?}");
}

#[test]
fn processing_alone_then_end_of_stream_is_not_success() {
    // C acknowledgement only: the final RESULT never comes.
    let result = run_against_device(
        |d| {
            expect_request(d, connect_request(PHONE));
            answer(d, &packet(&[0x04, 0x01, 0x07, 0x06], PHONE, &[]));
        },
        |c| connect_device(c, BdAddr { b: PHONE }),
    );
    assert!(
        matches!(result, Err(BoseError::ShortRead { .. })),
        "got {result:?}"
    );
}

#[test]
fn a_result_for_another_address_is_not_success() {
    let result = run_against_device(
        |d| {
            expect_request(d, connect_request(PHONE));
            answer(d, &packet(&[0x04, 0x01, 0x06, 0x07], RANDOM, &[0x01]));
        },
        |c| connect_device(c, BdAddr { b: PHONE }),
    );
    assert!(result.is_err(), "got {result:?}");
}

#[test]
fn connect_returns_as_soon_as_the_result_arrives() {
    let elapsed = run_against_device(
        |d| {
            expect_request(d, connect_request(PHONE));
            answer(d, &packet(&[0x04, 0x01, 0x07, 0x06], PHONE, &[]));
            answer(d, &packet(&[0x04, 0x01, 0x06, 0x07], PHONE, &[0x07]));
            thread::sleep(Duration::from_millis(1500));
        },
        |c| {
            let start = Instant::now();
            let result = connect_device(c, BdAddr { b: PHONE });
            (result.is_ok(), start.elapsed())
        },
    );
    assert!(elapsed.0, "connect failed");
    assert!(
        elapsed.1 < Duration::from_millis(500),
        "took {:?}",
        elapsed.1
    );
}

#[test]
fn later_results_of_a_connect_do_not_reach_the_next_request() {
    // Capture, SoundLink Color II: three RESULTs (codes 01, 05, 07) and
    // two active-source notifications follow one connect.
    let (connect, battery) = run_against_device(
        |d| {
            expect_request(d, connect_request(PHONE));
            let mut burst = packet(&[0x04, 0x01, 0x07, 0x06], PHONE, &[]);
            burst.extend(packet(&[0x04, 0x01, 0x06, 0x07], PHONE, &[0x01]));
            burst.extend(active_source(LAPTOP));
            burst.extend(packet(&[0x04, 0x01, 0x06, 0x07], PHONE, &[0x05]));
            burst.extend(active_source(PHONE));
            burst.extend(packet(&[0x04, 0x01, 0x06, 0x07], PHONE, &[0x07]));
            answer(d, &burst);
            expect_request(d, BATTERY_REQUEST);
            answer(d, &BATTERY_REPLY_80);
        },
        |c| (connect_device(c, BdAddr { b: PHONE }), get_battery_level(c)),
    );
    assert!(connect.is_ok(), "got {connect:?}");
    assert_eq!(battery.expect("battery after connect"), 80);
}

// ---- disconnect ----

#[test]
fn disconnect_succeeds_on_the_soundlink_color_ii_sequence() {
    // Capture: two PROCESSING, a notification, then the RESULT.
    let (disconnect, battery) = run_against_device(
        |d| {
            expect_request(d, disconnect_request(PHONE));
            let mut burst = vec![0x04, 0x02, 0x07, 0x00];
            burst.extend(packet(&[0x04, 0x02, 0x07, 0x06], PHONE, &[]));
            burst.extend(active_source([0; 6]));
            burst.extend(packet(&[0x04, 0x02, 0x06, 0x06], PHONE, &[]));
            answer(d, &burst);
            expect_request(d, BATTERY_REQUEST);
            answer(d, &BATTERY_REPLY_80);
        },
        |c| {
            (
                disconnect_device(c, BdAddr { b: PHONE }),
                get_battery_level(c),
            )
        },
    );
    assert!(disconnect.is_ok(), "got {disconnect:?}");
    assert_eq!(battery.expect("battery after disconnect"), 80);
}

#[test]
fn disconnect_succeeds_on_the_qc35_sequence() {
    // Capture, QC35 II: PROCESSING carries an extra byte 21.
    let result = run_against_device(
        |d| {
            expect_request(d, disconnect_request(PHONE));
            let mut burst = packet(&[0x04, 0x02, 0x07, 0x07, 0x21], PHONE, &[]);
            burst.extend(active_source([0; 6]));
            burst.extend(packet(&[0x04, 0x02, 0x06, 0x06], PHONE, &[]));
            answer(d, &burst);
        },
        |c| disconnect_device(c, BdAddr { b: PHONE }),
    );
    assert!(result.is_ok(), "got {result:?}");
}

#[test]
fn disconnect_reports_an_error_packet() {
    let result = run_against_device(
        |d| {
            expect_request(d, disconnect_request(PHONE));
            answer(d, &packet(&[0x04, 0x02, 0x04, 0x07, 0x0b], PHONE, &[]));
        },
        |c| disconnect_device(c, BdAddr { b: PHONE }),
    );
    assert!(is_device_error(&result, 0x04, 0x02, 0x0b), "got {result:?}");
}

// ---- remove ----

#[test]
fn remove_succeeds_on_the_c_acknowledgement() {
    let result = run_against_device(
        |d| {
            expect_request(d, remove_request(PHONE));
            answer(d, &packet(&[0x04, 0x03, 0x06, 0x06], PHONE, &[]));
        },
        |c| remove_device(c, BdAddr { b: PHONE }),
    );
    assert!(result.is_ok(), "got {result:?}");
}

#[test]
fn remove_succeeds_when_nothing_arrives_before_the_receive_timeout() {
    // Capture (2026-10-05), SoundLink Color II: no reply at all.
    let result = run_against_device(
        |d| {
            expect_request(d, remove_request(PHONE));
            thread::sleep(Duration::from_millis(600));
        },
        |c| {
            c.set_read_timeout(Some(Duration::from_millis(200)))
                .expect("set_read_timeout");
            remove_device(c, BdAddr { b: PHONE })
        },
    );
    assert!(result.is_ok(), "got {result:?}");
}

#[test]
fn remove_succeeds_when_the_device_closes_without_answering() {
    let result = run_against_device(
        |d| expect_request(d, remove_request(PHONE)),
        |c| remove_device(c, BdAddr { b: PHONE }),
    );
    assert!(result.is_ok(), "got {result:?}");
}

#[test]
fn remove_reports_an_error_packet() {
    let result = run_against_device(
        |d| {
            expect_request(d, remove_request(PHONE));
            answer(d, &[0x04, 0x03, 0x04, 0x01, 0x05]);
        },
        |c| remove_device(c, BdAddr { b: PHONE }),
    );
    assert!(is_device_error(&result, 0x04, 0x03, 0x05), "got {result:?}");
}
