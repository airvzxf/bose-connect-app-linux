//! Devices send packets nobody asked for: a second copy of a reply,
//! active-source changes, play state, track metadata. None of them may
//! be taken as the reply to the next request on the same connection.
//!
//! Every byte sequence below was captured on 2026-10-07 (SoundLink
//! Color II firmware 4.0.1, QC35 II 4.8.1, QC Ultra 1.6.7).

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use bose_connect::protocol::{
    get_battery_level, get_device_status, get_paired_devices, send_media_key, set_volume,
};
use bose_connect::{BdAddr, DevicesConnected, MediaKey};

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

/// Hex string ("05 05 03") to bytes.
fn bytes(hex: &str) -> Vec<u8> {
    hex.split_whitespace()
        .map(|b| u8::from_str_radix(b, 16).expect("hex byte"))
        .collect()
}

const BATTERY_REQUEST: [u8; 4] = [0x02, 0x02, 0x01, 0x00];

#[test]
fn the_soundlink_color_ii_second_volume_reply_does_not_answer_the_next_request() {
    // Capture: `05 05 02 01 14` is answered twice; the next battery
    // request read the second copy before (AckMismatch on hardware).
    let (volume, battery) = run_against_device(
        |d| {
            expect_request(d, [0x05, 0x05, 0x02, 0x01, 0x14]);
            answer(d, &bytes("05 05 03 02 64 14 05 05 03 02 64 14"));
            expect_request(d, BATTERY_REQUEST);
            answer(d, &bytes("02 02 03 01 64"));
        },
        |c| (set_volume(c, 0x14), get_battery_level(c)),
    );
    assert_eq!(volume.expect("set_volume"), 100);
    assert_eq!(battery.expect("battery"), 100);
}

#[test]
fn a_copy_that_arrives_after_the_next_request_is_skipped() {
    // The second copy may still be in flight when the next request is
    // sent; it is a packet of another function for that request.
    let (volume, battery) = run_against_device(
        |d| {
            expect_request(d, [0x05, 0x05, 0x02, 0x01, 0x14]);
            answer(d, &bytes("05 05 03 02 64 14"));
            expect_request(d, BATTERY_REQUEST);
            answer(d, &bytes("05 05 03 02 64 14 02 02 03 01 64"));
        },
        |c| (set_volume(c, 0x14), get_battery_level(c)),
    );
    assert_eq!(volume.expect("set_volume"), 100);
    assert_eq!(battery.expect("battery"), 100);
}

#[test]
fn track_metadata_after_a_media_key_does_not_answer_the_next_request() {
    // Capture, SoundLink Color II: "next" is answered with PROCESSING,
    // RESULT, the play state and the new track (title, artist, album),
    // twice.
    let metadata = "05 04 03 03 01 00 c8 05 06 07 00 \
        05 06 03 13 00 53 75 6d 6d 65 72 74 69 6d 65 20 53 61 64 6e 65 73 73 \
        05 06 03 0d 01 4c 61 6e 61 20 44 65 6c 20 52 65 79 05 06 06 00";
    let (key, battery) = run_against_device(
        move |d| {
            expect_request(d, [0x05, 0x03, 0x05, 0x01, 0x03]);
            let mut burst = bytes("05 03 07 00 05 03 06 00");
            burst.extend(bytes(metadata));
            burst.extend(bytes(metadata));
            answer(d, &burst);
            expect_request(d, BATTERY_REQUEST);
            answer(d, &bytes("02 02 03 01 64"));
        },
        |c| (send_media_key(c, MediaKey::Next), get_battery_level(c)),
    );
    assert!(key.is_ok(), "got {key:?}");
    assert_eq!(battery.expect("battery"), 100);
}

#[test]
fn a_notification_before_the_reply_is_skipped() {
    let volume = run_against_device(
        |d| {
            expect_request(d, [0x05, 0x05, 0x02, 0x01, 0x0a]);
            let mut burst = bytes("05 01 03 09 00 02 01 00 62 01 8c 0c b7");
            burst.extend(bytes("05 05 03 02 19 0a"));
            answer(d, &burst);
        },
        |c| set_volume(c, 0x0a),
    );
    assert_eq!(volume.expect("set_volume"), 25);
}

#[test]
fn the_qc_ultra_paired_list_is_read_from_its_header() {
    // Capture, QC Ultra: two devices, both connected.
    let paired = run_against_device(
        |d| {
            expect_request(d, [0x04, 0x04, 0x01, 0x00]);
            answer(
                d,
                &bytes("04 04 03 0d 03 00 62 01 8c 0c b7 d4 6d 6d 17 4d a4"),
            );
        },
        get_paired_devices::<UnixStream>,
    )
    .expect("get_paired_devices");
    assert_eq!(paired.num_devices, 2);
    assert_eq!(paired.connected, DevicesConnected::Two);
    assert_eq!(
        paired.addresses[..2],
        [
            BdAddr {
                b: [0x00, 0x62, 0x01, 0x8c, 0x0c, 0xb7]
            },
            BdAddr {
                b: [0xd4, 0x6d, 0x6d, 0x17, 0x4d, 0xa4]
            },
        ]
    );
}

#[test]
fn a_notification_inside_the_status_dump_is_skipped() {
    let status = run_against_device(
        |d| {
            expect_request(d, [0x00, 0x03, 0x01, 0x00]);
            answer(d, &bytes("00 03 03 03 40 20 02"));
            expect_request(d, [0x01, 0x01, 0x05, 0x00]);
            let mut burst = bytes("01 01 07 00");
            burst.extend(bytes("01 02 03 04 00 51 43 33"));
            burst.extend(bytes("05 01 03 09 00 02 01 00 62 01 8c 0c b7"));
            burst.extend(bytes("01 03 03 01 a6"));
            burst.extend(bytes("01 04 03 01 14"));
            burst.extend(bytes("01 06 03 02 01 0b"));
            burst.extend(bytes("01 01 06 00"));
            answer(d, &burst);
        },
        get_device_status::<UnixStream>,
    )
    .expect("get_device_status");
    assert_eq!(status.device_id, 0x4020);
    assert_eq!(status.name, "QC3");
    assert_eq!(status.language, 0xa6);
    assert_eq!(status.minutes, Some(20));
}
