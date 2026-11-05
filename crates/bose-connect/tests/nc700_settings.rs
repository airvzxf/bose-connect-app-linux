//! NC 700 Headphones (`0x4024`, firmware 1.8.2-11524+e0f7590) settings
//! read and written on hardware on 2026-11-04. Every request below was
//! sent from a laptop and every reply is the captured one.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use bose_connect::protocol::{
    enable_notifications, get_motion_auto_off, get_noise_cancelling_favorites,
    get_noise_cancelling_steps, get_paired_devices, get_remember_noise_cancelling, get_shortcut,
    has_noise_cancelling, has_noise_cancelling_steps, has_pairing_toggle, read_notification,
    send_packet, set_auto_off, set_motion_auto_off, set_noise_cancelling_favorites,
    set_noise_cancelling_level, set_pairing, set_prompt_language, set_remember_noise_cancelling,
    set_shortcut_event,
};
use bose_connect::{
    AutoOff, BoseError, DevicesConnected, NoiseCancellingSteps, Pairing, ShortcutAction,
};

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
        let result = client(&mut client_end);
        drop(client_end);
        let _ = tx.send(result);
    });
    let result = rx
        .recv_timeout(HANG_LIMIT)
        .expect("the client call hung or panicked");
    device_thread.join().expect("the fake device panicked");
    result
}

/// Hex string ("05 05 03") to bytes.
fn bytes(hex: &str) -> Vec<u8> {
    hex.split_whitespace()
        .map(|b| u8::from_str_radix(b, 16).expect("hex byte"))
        .collect()
}

/// Read one request on the device side and check it.
fn expect_request(device: &mut UnixStream, hex: &str) {
    let expected = bytes(hex);
    let mut got = vec![0u8; expected.len()];
    device.read_exact(&mut got).expect("read request");
    assert_eq!(got, expected, "unexpected request bytes");
}

fn answer(device: &mut UnixStream, hex: &str) {
    device.write_all(&bytes(hex)).expect("write reply");
}

/// The client sent nothing more and closed the connection.
fn expect_nothing_more(device: &mut UnixStream) {
    let mut buf = [0u8; 16];
    let n = device.read(&mut buf).expect("read");
    assert_eq!(n, 0, "unexpected extra request {:02x?}", &buf[..n]);
}

// ---- noise cancelling (01 05) ----

#[test]
fn noise_cancelling_raw_step_10_is_off_in_the_app() {
    let steps = run_against_device(
        |d| {
            expect_request(d, "01 05 01 00");
            answer(d, "01 05 03 03 0b 0a 01");
        },
        get_noise_cancelling_steps,
    )
    .expect("steps");
    assert_eq!(steps.steps, 11);
    assert_eq!(steps.raw, 10);
    assert!(steps.enabled);
    assert_eq!(steps.level(), 0, "raw 10 is the app's 0 (off)");
}

#[test]
fn noise_cancelling_raw_step_0_is_the_strongest() {
    let steps = run_against_device(
        |d| {
            expect_request(d, "01 05 01 00");
            answer(d, "01 05 03 03 0b 00 01");
        },
        get_noise_cancelling_steps,
    )
    .expect("steps");
    assert_eq!(steps.level(), 10, "raw 0 is the app's 10");
}

#[test]
fn noise_cancelling_level_10_writes_raw_step_0_and_reads_it_back() {
    let result = run_against_device(
        |d| {
            expect_request(d, "01 05 01 00");
            answer(d, "01 05 03 03 0b 0a 01");
            // The write is not answered: the next packet is the read-back.
            expect_request(d, "01 05 02 02 00 01");
            expect_request(d, "01 05 01 00");
            answer(d, "01 05 03 03 0b 00 01");
            expect_nothing_more(d);
        },
        |c| set_noise_cancelling_level(c, 10),
    );
    assert!(result.is_ok(), "got {result:?}");
}

#[test]
fn noise_cancelling_level_0_writes_raw_step_10() {
    let result = run_against_device(
        |d| {
            expect_request(d, "01 05 01 00");
            answer(d, "01 05 03 03 0b 00 01");
            expect_request(d, "01 05 02 02 0a 01");
            expect_request(d, "01 05 01 00");
            answer(d, "01 05 03 03 0b 0a 01");
            expect_nothing_more(d);
        },
        |c| set_noise_cancelling_level(c, 0),
    );
    assert!(result.is_ok(), "got {result:?}");
}

#[test]
fn noise_cancelling_level_out_of_range_is_not_sent() {
    let result = run_against_device(
        |d| {
            expect_request(d, "01 05 01 00");
            answer(d, "01 05 03 03 0b 0a 01");
            expect_nothing_more(d);
        },
        |c| set_noise_cancelling_level(c, 11),
    );
    assert!(
        matches!(result, Err(BoseError::InvalidArgument(_))),
        "got {result:?}"
    );
}

#[test]
fn noise_cancelling_unconfirmed_write_is_an_error() {
    let result = run_against_device(
        |d| {
            expect_request(d, "01 05 01 00");
            answer(d, "01 05 03 03 0b 0a 01");
            expect_request(d, "01 05 02 02 05 01");
            expect_request(d, "01 05 01 00");
            answer(d, "01 05 03 03 0b 0a 01");
        },
        |c| set_noise_cancelling_level(c, 5),
    );
    assert!(
        matches!(result, Err(BoseError::UnconfirmedValue { .. })),
        "got {result:?}"
    );
}

#[test]
fn legacy_noise_cancelling_function_is_not_supported() {
    // `01 06` is the QC35 function; the NC 700 answers ERROR 04
    // (function not supported), so `has_noise_cancelling` stays false.
    let reply = run_against_device(
        |d| {
            expect_request(d, "01 06 01 00");
            answer(d, "01 06 04 01 04");
        },
        |c| send_packet(c, &[0x01, 0x06, 0x01, 0x00]),
    )
    .expect("reply");
    assert_eq!(reply, bytes("01 06 04 01 04"));
    assert!(!has_noise_cancelling(0x4024));
    assert!(has_noise_cancelling_steps(0x4024));
}

#[test]
fn raw_step_for_level_follows_the_device_step_count() {
    assert_eq!(NoiseCancellingSteps::raw_for_level(11, 0), Some(10));
    assert_eq!(NoiseCancellingSteps::raw_for_level(11, 10), Some(0));
    assert_eq!(NoiseCancellingSteps::raw_for_level(11, 11), None);
    assert_eq!(NoiseCancellingSteps::raw_for_level(0, 0), None);
}

// ---- favourites (01 0f) ----

#[test]
fn favorites_are_the_selected_index_and_the_raw_steps() {
    let favorites = run_against_device(
        |d| {
            expect_request(d, "01 0f 01 00");
            answer(d, "01 0f 03 04 02 00 05 0a");
        },
        get_noise_cancelling_favorites,
    )
    .expect("favorites");
    assert_eq!(favorites.selected, 2);
    assert_eq!(favorites.raw, vec![0, 5, 10]);
    // The Bose Music app shows them as 10, 5 and 0.
    assert_eq!(favorites.levels(11), vec![10, 5, 0]);
}

#[test]
fn setting_favorites_keeps_the_selected_index() {
    let result = run_against_device(
        |d| {
            expect_request(d, "01 05 01 00");
            answer(d, "01 05 03 03 0b 0a 01");
            expect_request(d, "01 0f 01 00");
            answer(d, "01 0f 03 04 02 00 05 0a");
            // 9, 4, 1 in the app are the raw steps 1, 6, 9.
            expect_request(d, "01 0f 02 04 02 01 06 09");
            expect_request(d, "01 0f 01 00");
            answer(d, "01 0f 03 04 02 01 06 09");
            expect_nothing_more(d);
        },
        |c| set_noise_cancelling_favorites(c, &[9, 4, 1]),
    );
    assert!(result.is_ok(), "got {result:?}");
}

#[test]
fn favorites_out_of_range_or_of_the_wrong_count_are_not_sent() {
    for levels in [&[11u8, 4, 1][..], &[9, 4][..]] {
        let levels = levels.to_vec();
        let result = run_against_device(
            |d| {
                expect_request(d, "01 05 01 00");
                answer(d, "01 05 03 03 0b 0a 01");
                expect_request(d, "01 0f 01 00");
                answer(d, "01 0f 03 04 02 00 05 0a");
                expect_nothing_more(d);
            },
            move |c| set_noise_cancelling_favorites(c, &levels),
        );
        assert!(
            matches!(result, Err(BoseError::InvalidArgument(_))),
            "got {result:?}"
        );
    }
}

// ---- remember my last setting (01 0e) and motion auto-off (01 14) ----

#[test]
fn remember_last_noise_cancelling_is_read_back_because_the_write_is_not_answered() {
    let result = run_against_device(
        |d| {
            expect_request(d, "01 0e 02 01 00");
            expect_request(d, "01 0e 01 00");
            answer(d, "01 0e 03 01 00");
            expect_request(d, "01 0e 02 01 01");
            expect_request(d, "01 0e 01 00");
            answer(d, "01 0e 03 01 01");
            expect_request(d, "01 0e 01 00");
            answer(d, "01 0e 03 01 01");
            expect_nothing_more(d);
        },
        |c| {
            set_remember_noise_cancelling(c, false)?;
            set_remember_noise_cancelling(c, true)?;
            assert!(get_remember_noise_cancelling(c)?);
            Ok::<_, BoseError>(())
        },
    );
    assert!(result.is_ok(), "got {result:?}");
}

#[test]
fn motion_auto_off_is_echoed_by_the_device() {
    let result = run_against_device(
        |d| {
            expect_request(d, "01 14 01 00");
            answer(d, "01 14 03 01 01");
            expect_request(d, "01 14 02 01 00");
            answer(d, "01 14 03 01 00");
        },
        |c| {
            assert!(get_motion_auto_off(c)?);
            set_motion_auto_off(c, false)
        },
    );
    assert!(result.is_ok(), "got {result:?}");
}

// ---- shortcut (01 09) ----

#[test]
fn shortcut_uses_event_05_and_the_wake_word_action_is_07() {
    let result = run_against_device(
        |d| {
            expect_request(d, "01 09 01 00");
            answer(d, "01 09 03 0b 80 05 07 00 01 20 88 00 00 00 80");
            expect_request(d, "01 09 02 03 80 05 03");
            answer(d, "01 09 03 0b 80 05 03 00 01 20 88 00 00 00 80");
        },
        |c| {
            assert_eq!(get_shortcut(c)?, ShortcutAction::ToggleWakeWord);
            set_shortcut_event(c, 0x05, ShortcutAction::BatteryLevel)
        },
    );
    assert!(result.is_ok(), "got {result:?}");
}

// ---- auto-off (01 04) ----

#[test]
fn auto_off_is_read_back_when_the_write_is_not_answered() {
    let result = run_against_device(
        |d| {
            expect_request(d, "01 04 01 00");
            answer(d, "01 04 03 01 00");
            expect_request(d, "01 04 02 01 14");
            // No echo of the write: the call asks for the value after its
            // receive timeout.
            expect_request(d, "01 04 01 00");
            answer(d, "01 04 03 01 14");
        },
        |c| {
            // The fake device is silent after the write, as the NC 700 is.
            c.set_read_timeout(Some(Duration::from_millis(200)))
                .expect("read timeout");
            set_auto_off(c, AutoOff::Min20)
        },
    );
    assert!(result.is_ok(), "got {result:?}");
}

// ---- voice prompts (01 03) ----

#[test]
fn voice_prompts_off_is_confirmed_although_the_device_drops_flag_bit_6() {
    // `e1` (English, prompts on, flags 7 and 6) is stored as `81`.
    let result = run_against_device(
        |d| {
            expect_request(d, "01 03 02 01 c1");
            answer(d, "01 03 03 05 81 00 01 81 5e");
        },
        |c| set_prompt_language(c, 0xc1),
    );
    assert!(result.is_ok(), "got {result:?}");
}

// ---- notifications (09 02) ----

#[test]
fn notifications_subscribe_with_a_big_endian_block_bit_set() {
    // The blocks the Bose Music app subscribes to: 0, 1, 2, 4, 5, 9, 16, 18.
    let result = run_against_device(
        |d| {
            expect_request(d, "09 02 02 04 01 05 02 37");
            answer(d, "09 02 03 04 01 05 02 37");
            expect_request(d, "09 02 02 02 01 04");
            // No reply: the call must not fail on the receive timeout.
            expect_nothing_more(d);
        },
        |c| {
            c.set_read_timeout(Some(Duration::from_millis(200)))
                .expect("read timeout");
            enable_notifications(c, &[0, 1, 2, 4, 5, 9, 16, 18])?;
            enable_notifications(c, &[2])
        },
    );
    assert!(result.is_ok(), "got {result:?}");
}

#[test]
fn notifications_are_read_one_packet_at_a_time_and_none_on_timeout() {
    let got = run_against_device(
        |d| {
            // Volume 6 of 16, then the device goes quiet.
            answer(d, "05 05 03 02 10 06");
            // Wait until the client is done so the stream stays open.
            expect_nothing_more(d);
        },
        |c| {
            c.set_read_timeout(Some(Duration::from_millis(200)))
                .expect("read timeout");
            let first = read_notification(c)?;
            let second = read_notification(c)?;
            Ok::<_, BoseError>((first, second))
        },
    )
    .expect("notifications");
    let first = got.0.expect("a packet");
    assert_eq!((first.block, first.function, first.operator), (5, 5, 3));
    assert_eq!(first.payload, vec![0x10, 0x06]);
    assert!(got.1.is_none(), "nothing more was sent");
}

#[test]
fn status_button_function_is_not_supported() {
    // `02 0c` reports a button's configuration on other models; the NC 700
    // answers ERROR 04, and has no key-press packets.
    let reply = run_against_device(
        |d| {
            expect_request(d, "02 0c 01 00");
            answer(d, "02 0c 04 01 04");
        },
        |c| send_packet(c, &[0x02, 0x0c, 0x01, 0x00]),
    )
    .expect("reply");
    assert_eq!(reply, bytes("02 0c 04 01 04"));
}

// ---- paired devices (04 04) ----

#[test]
fn paired_list_reads_when_only_the_second_device_is_connected() {
    // The phone was disconnected from the NC 700: the connected byte is `02`,
    // one bit per listed device (`03` when both are connected).
    let both = run_against_device(
        |d| {
            expect_request(d, "04 04 01 00");
            answer(d, "04 04 03 0d 03 aa bb cc dd ee 01 aa bb cc dd ee 02");
        },
        get_paired_devices,
    )
    .expect("paired list");
    assert_eq!(both.connected, DevicesConnected::Two);

    let second_only = run_against_device(
        |d| {
            expect_request(d, "04 04 01 00");
            answer(d, "04 04 03 0d 02 aa bb cc dd ee 01 aa bb cc dd ee 02");
        },
        get_paired_devices,
    )
    .expect("paired list");
    assert_eq!(second_only.connected, DevicesConnected::One);
    assert_eq!(second_only.num_devices, 2);
}

#[test]
fn paired_list_reads_when_no_device_has_an_audio_link() {
    // QC35 II, firmware 4.8.1 (2026-11-04): after its only audio source, the
    // phone, was disconnected, the laptop asking over RFCOMM got `cc = 00`.
    let none = run_against_device(
        |d| {
            expect_request(d, "04 04 01 00");
            answer(d, "04 04 03 0d 00 aa bb cc dd ee 01 aa bb cc dd ee 02");
        },
        get_paired_devices,
    )
    .expect("paired list");
    assert_eq!(none.connected, DevicesConnected::Zero);
    assert_eq!(none.connected.count(), Some(0));
    assert_eq!(none.num_devices, 2);
}

// ---- pairing mode (04 08) ----

#[test]
fn pairing_on_is_confirmed_by_the_flag_when_the_write_is_not_answered() {
    let result = run_against_device(
        |d| {
            expect_request(d, "04 08 05 01 01");
            // PROCESSING, then nothing: the call reads the state itself.
            answer(d, "04 08 07 00");
            expect_request(d, "04 08 01 00");
            answer(d, "04 08 06 02 01 01");
        },
        |c| {
            c.set_read_timeout(Some(Duration::from_millis(200)))
                .expect("read timeout");
            set_pairing(c, Pairing::On)
        },
    );
    assert!(result.is_ok(), "got {result:?}");
}

#[test]
fn pairing_off_waits_for_the_flag_to_clear() {
    let result = run_against_device(
        |d| {
            expect_request(d, "04 08 05 01 00");
            expect_request(d, "04 08 01 00");
            answer(d, "04 08 06 02 01 01");
            expect_request(d, "04 08 01 00");
            answer(d, "04 08 06 02 00 01");
        },
        |c| {
            c.set_read_timeout(Some(Duration::from_millis(200)))
                .expect("read timeout");
            set_pairing(c, Pairing::Off)
        },
    );
    assert!(result.is_ok(), "got {result:?}");
    assert!(has_pairing_toggle(0x4024));
}
