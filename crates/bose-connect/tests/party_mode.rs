//! Party and stereo mode of two SoundLink Color II speakers.
//!
//! The replies are the sequences captured on 2026-11-04 from two
//! SoundLink Color II, firmware 4.0.1: the "White" speaker is the one
//! the library talks to, the "Black" one (2C:41:A1:0B:7C:82) is joined
//! to it, and the laptop D4:6D:6D:17:4D:A4 is the music source.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use bose_connect::protocol::{
    disconnect_device, get_battery_level, get_party_mode, join_party, set_party_mode,
};
use bose_connect::{BdAddr, BoseError, PartyMode};

/// How long a client call may take before the test calls it hung.
const HANG_LIMIT: Duration = Duration::from_secs(5);

/// The client's receive timeout, standing in for the 1 s of
/// `Connection`.
const RECEIVE_TIMEOUT: Duration = Duration::from_millis(200);

const BLACK: [u8; 6] = [0x2c, 0x41, 0xa1, 0x0b, 0x7c, 0x82];
const LAPTOP: [u8; 6] = [0xd4, 0x6d, 0x6d, 0x17, 0x4d, 0xa4];

const BATTERY_REQUEST: [u8; 4] = [0x02, 0x02, 0x01, 0x00];
const BATTERY_REPLY_80: [u8; 5] = [0x02, 0x02, 0x03, 0x01, 0x50];

const GET_MODE: [u8; 4] = [0x04, 0x0b, 0x01, 0x00];
/// Voice-prompt notification sent while joining: bit 7 of the
/// prompt-language byte goes off (`a6` → `26`).
const PROMPTS_OFF: [u8; 9] = [0x01, 0x03, 0x03, 0x05, 0x26, 0x00, 0x04, 0xcf, 0xde];
const PROMPTS_ON: [u8; 9] = [0x01, 0x03, 0x03, 0x05, 0xa6, 0x00, 0x04, 0xcf, 0xde];

/// Run `client` against a fake device driven by `device`. Fails the
/// test if `client` does not return within [`HANG_LIMIT`].
fn run_against_device<R, C, D>(device: D, client: C) -> R
where
    R: Send + 'static,
    C: FnOnce(&mut UnixStream) -> R + Send + 'static,
    D: FnOnce(&mut UnixStream) + Send + 'static,
{
    let (mut client_end, mut device_end) = UnixStream::pair().expect("UnixStream::pair");
    client_end
        .set_read_timeout(Some(RECEIVE_TIMEOUT))
        .expect("set_read_timeout");
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

/// Read one request on the device side and check it.
fn expect_request(device: &mut UnixStream, expected: &[u8]) {
    let mut got = vec![0u8; expected.len()];
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

fn join_request() -> Vec<u8> {
    let mut r = packet(&[0x04, 0x01, 0x05, 0x0d, 0x90], BLACK, &[]);
    r.extend_from_slice(&LAPTOP);
    r
}

fn mode_status(mode: u8) -> [u8; 5] {
    [0x04, 0x0b, 0x03, 0x01, mode]
}

/// Join that succeeded (2026-11-04 20:05:42): PROCESSING, the prompt
/// notification, RESULT `01`, the mode notification and a second
/// RESULT `05`, all within about 2 s. The next request on the same
/// connection still gets its own reply.
#[test]
fn join_returns_on_the_result_for_the_peer() {
    let level = run_against_device(
        |device| {
            expect_request(device, &join_request());
            let mut reply = packet(&[0x04, 0x01, 0x07, 0x06], BLACK, &[]);
            reply.extend_from_slice(&PROMPTS_OFF);
            reply.extend(packet(&[0x04, 0x01, 0x06, 0x07], BLACK, &[0x01]));
            reply.extend_from_slice(&mode_status(0x01));
            reply.extend(packet(&[0x04, 0x01, 0x06, 0x07], BLACK, &[0x05]));
            answer(device, &reply);
            expect_request(device, &BATTERY_REQUEST);
            answer(device, &BATTERY_REPLY_80);
        },
        |client| {
            join_party(client, BdAddr { b: BLACK }, BdAddr { b: LAPTOP })?;
            get_battery_level(client)
        },
    );
    assert_eq!(level.expect("join, then battery"), 0x50);
}

/// Join that failed (2026-11-04 20:33:34, the other speaker locked up):
/// PROCESSING, then ERROR `0d` about 20 s later.
#[test]
fn join_reports_error_0d_when_the_peer_does_not_answer() {
    let result = run_against_device(
        |device| {
            expect_request(device, &join_request());
            let mut reply = packet(&[0x04, 0x01, 0x07, 0x06], BLACK, &[]);
            reply.extend_from_slice(&PROMPTS_OFF);
            answer(device, &reply);
            thread::sleep(RECEIVE_TIMEOUT * 2);
            let mut error = packet(&[0x04, 0x01, 0x04, 0x07, 0x0d], BLACK, &[0x01]);
            error.extend_from_slice(&PROMPTS_ON);
            answer(device, &error);
        },
        |client| join_party(client, BdAddr { b: BLACK }, BdAddr { b: LAPTOP }),
    );
    assert!(
        matches!(
            result,
            Err(BoseError::DeviceError {
                block: 0x04,
                function: 0x01,
                code: 0x0d
            })
        ),
        "got {result:?}"
    );
}

#[test]
fn party_mode_decodes_off_party_and_stereo() {
    for (byte, mode) in [
        (0x00, PartyMode::Off),
        (0x01, PartyMode::Party),
        (0x02, PartyMode::Stereo),
    ] {
        let got = run_against_device(
            move |device| {
                expect_request(device, &GET_MODE);
                answer(device, &mode_status(byte));
            },
            get_party_mode,
        );
        assert_eq!(got.expect("party mode"), mode);
    }
}

/// Stereo while joined (2026-11-04 20:06:16): the STATUS came twice;
/// the read-back sees its own reply, not the duplicate.
#[test]
fn set_stereo_reads_the_duplicate_status_and_confirms() {
    let result = run_against_device(
        |device| {
            expect_request(device, &[0x04, 0x0b, 0x02, 0x01, 0x02]);
            let mut reply = mode_status(0x02).to_vec();
            reply.extend_from_slice(&mode_status(0x02));
            answer(device, &reply);
            expect_request(device, &GET_MODE);
            answer(device, &mode_status(0x02));
            expect_request(device, &BATTERY_REQUEST);
            answer(device, &BATTERY_REPLY_80);
        },
        |client| {
            set_party_mode(client, PartyMode::Stereo)?;
            get_battery_level(client)
        },
    );
    assert_eq!(result.expect("stereo, then battery"), 0x50);
}

/// Stereo after a failed join (2026-11-04 19:48): the speaker echoes
/// `02` once but stays in mode `00`.
#[test]
fn set_stereo_when_not_joined_is_unconfirmed() {
    let result = run_against_device(
        |device| {
            expect_request(device, &[0x04, 0x0b, 0x02, 0x01, 0x02]);
            answer(device, &mode_status(0x02));
            expect_request(device, &GET_MODE);
            answer(device, &mode_status(0x00));
        },
        |client| set_party_mode(client, PartyMode::Stereo),
    );
    assert!(
        matches!(result, Err(BoseError::UnconfirmedValue { .. })),
        "got {result:?}"
    );
}

#[test]
fn set_party_mode_off_is_refused_without_sending() {
    let result = run_against_device(
        |device| {
            let mut buf = [0u8; 1];
            // The client closes its end without writing anything.
            assert_eq!(device.read(&mut buf).expect("read"), 0);
        },
        |client| {
            let r = set_party_mode(client, PartyMode::Off);
            client.shutdown(std::net::Shutdown::Both).expect("shutdown");
            r
        },
    );
    assert!(
        matches!(result, Err(BoseError::InvalidArgument(_))),
        "got {result:?}"
    );
}

/// Leaving party mode is a disconnect of the other speaker
/// (2026-11-04 20:07:32).
#[test]
fn leave_is_a_disconnect_of_the_peer() {
    let result = run_against_device(
        |device| {
            expect_request(device, &packet(&[0x04, 0x02, 0x05, 0x06], BLACK, &[]));
            let mut reply = vec![0x04, 0x02, 0x07, 0x00];
            reply.extend(packet(&[0x04, 0x02, 0x07, 0x06], BLACK, &[]));
            reply.extend(packet(&[0x04, 0x02, 0x06, 0x06], BLACK, &[]));
            reply.extend_from_slice(&PROMPTS_ON);
            reply.extend(packet(&[0x04, 0x02, 0x06, 0x06], BLACK, &[]));
            answer(device, &reply);
        },
        |client| disconnect_device(client, BdAddr { b: BLACK }),
    );
    result.expect("leave");
}
