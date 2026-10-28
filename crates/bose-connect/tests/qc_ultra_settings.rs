//! QC Ultra Headphones settings decoded on hardware on 2026-10-07/08
//! (firmware 1.6.7): every request below was sent from a laptop and the
//! Bose app showed the new value; every reply is the captured one.
//! QC35 requests marked "C" are the bytes of the original `based.c`.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use bose_connect::protocol::{
    get_battery_prompt, get_equalizer, get_immersive_audio, get_shortcut, set_auto_answer_calls,
    set_auto_off, set_auto_play_pause, set_battery_prompt, set_equalizer_band, set_immersive_audio,
    set_remember_audio_mode, set_shortcut,
};
use bose_connect::{AutoOff, BoseError, EqBand, EqualizerBand, ImmersiveAudio, ShortcutAction};

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

// ---- auto-off ----

#[test]
fn auto_off_40_minutes_uses_the_qc_ultra_3_byte_layout() {
    let result = run_against_device(
        |d| {
            expect_request(d, "01 04 01 00");
            answer(d, "01 04 03 03 05 00 00");
            expect_request(d, "01 04 02 03 28 00 00");
            answer(d, "01 04 03 03 28 00 00");
        },
        |c| set_auto_off(c, AutoOff::Min40),
    );
    assert!(result.is_ok(), "got {result:?}");
}

#[test]
fn auto_off_24_hours_splits_the_minutes_around_a_zero_byte() {
    let result = run_against_device(
        |d| {
            expect_request(d, "01 04 01 00");
            answer(d, "01 04 03 03 b4 00 00");
            expect_request(d, "01 04 02 03 a0 00 05");
            answer(d, "01 04 03 03 a0 00 05");
        },
        |c| set_auto_off(c, AutoOff::Min1440),
    );
    assert!(result.is_ok(), "got {result:?}");
}

#[test]
fn auto_off_never_on_the_qc_ultra() {
    let result = run_against_device(
        |d| {
            expect_request(d, "01 04 01 00");
            answer(d, "01 04 03 03 b4 00 00");
            expect_request(d, "01 04 02 03 00 00 00");
            answer(d, "01 04 03 03 00 00 00");
        },
        |c| set_auto_off(c, AutoOff::Never),
    );
    assert!(result.is_ok(), "got {result:?}");
}

#[test]
fn auto_off_keeps_the_c_packet_on_the_1_byte_layout() {
    let result = run_against_device(
        |d| {
            expect_request(d, "01 04 01 00");
            answer(d, "01 04 03 01 14");
            expect_request(d, "01 04 02 01 b4");
            answer(d, "01 04 03 01 b4");
        },
        |c| set_auto_off(c, AutoOff::Min180),
    );
    assert!(result.is_ok(), "got {result:?}");
}

#[test]
fn auto_off_24_hours_is_refused_on_the_1_byte_layout_before_writing() {
    let result = run_against_device(
        |d| {
            expect_request(d, "01 04 01 00");
            answer(d, "01 04 03 01 14");
            expect_nothing_more(d);
        },
        |c| set_auto_off(c, AutoOff::Min1440),
    );
    assert!(
        matches!(result, Err(BoseError::InvalidArgument(_))),
        "got {result:?}"
    );
}

// ---- battery prompt ----

#[test]
fn the_battery_prompt_is_the_last_byte_of_the_language_payload() {
    let on = run_against_device(
        |d| {
            expect_request(d, "01 03 01 00");
            answer(d, "01 03 03 07 a6 00 01 81 5e 01 01");
        },
        get_battery_prompt::<UnixStream>,
    );
    assert!(on.expect("get_battery_prompt"));
}

#[test]
fn turning_the_battery_prompt_off_keeps_the_language_byte() {
    let result = run_against_device(
        |d| {
            expect_request(d, "01 03 01 00");
            answer(d, "01 03 03 07 a6 00 01 81 5e 01 01");
            expect_request(d, "01 03 02 02 a6 00");
            answer(d, "01 03 03 07 a6 00 01 81 5e 01 00");
        },
        |c| set_battery_prompt(c, false),
    );
    assert!(result.is_ok(), "got {result:?}");
}

#[test]
fn a_device_with_the_1_byte_language_layout_has_no_battery_prompt() {
    let result = run_against_device(
        |d| {
            expect_request(d, "01 03 01 00");
            answer(d, "01 03 03 01 a6");
            expect_nothing_more(d);
        },
        |c| set_battery_prompt(c, true),
    );
    assert!(
        matches!(result, Err(BoseError::InvalidArgument(_))),
        "got {result:?}"
    );
}

// ---- equalizer ----

#[test]
fn the_equalizer_reports_three_bands_with_their_range() {
    let bands = run_against_device(
        |d| {
            expect_request(d, "01 07 01 00");
            answer(d, "01 07 03 0c f6 0a 05 00 f6 0a 00 01 f6 0a fd 02");
        },
        get_equalizer::<UnixStream>,
    )
    .expect("get_equalizer");
    assert_eq!(
        bands,
        vec![
            EqualizerBand {
                band: EqBand::Bass,
                min: -10,
                max: 10,
                value: 5
            },
            EqualizerBand {
                band: EqBand::Mid,
                min: -10,
                max: 10,
                value: 0
            },
            EqualizerBand {
                band: EqBand::Treble,
                min: -10,
                max: 10,
                value: -3
            },
        ]
    );
}

#[test]
fn setting_treble_to_minus_3() {
    let result = run_against_device(
        |d| {
            expect_request(d, "01 07 01 00");
            answer(d, "01 07 03 0c f6 0a 05 00 f6 0a 00 01 f6 0a 00 02");
            expect_request(d, "01 07 02 02 fd 02");
            answer(d, "01 07 03 0c f6 0a 05 00 f6 0a 00 01 f6 0a fd 02");
        },
        |c| set_equalizer_band(c, EqBand::Treble, -3),
    );
    assert!(result.is_ok(), "got {result:?}");
}

#[test]
fn an_out_of_range_equalizer_value_is_never_sent() {
    // Sending `01 07 02 02 0b 01` locked the equalizer until a power
    // cycle on hardware.
    let result = run_against_device(
        |d| {
            expect_request(d, "01 07 01 00");
            answer(d, "01 07 03 0c f6 0a 00 00 f6 0a 00 01 f6 0a 00 02");
            expect_nothing_more(d);
        },
        |c| set_equalizer_band(c, EqBand::Mid, 11),
    );
    assert!(
        matches!(result, Err(BoseError::InvalidArgument(_))),
        "got {result:?}"
    );
}

// ---- immersive audio, switches, shortcut ----

#[test]
fn immersive_audio_reads_motion() {
    let setting = run_against_device(
        |d| {
            expect_request(d, "05 0f 01 00");
            answer(d, "05 0f 03 01 02");
        },
        get_immersive_audio::<UnixStream>,
    );
    assert_eq!(
        setting.expect("get_immersive_audio"),
        ImmersiveAudio::Motion
    );
}

#[test]
fn immersive_audio_set_to_still() {
    let result = run_against_device(
        |d| {
            expect_request(d, "05 0f 02 01 01");
            answer(d, "05 0f 03 01 01");
        },
        |c| set_immersive_audio(c, ImmersiveAudio::Still),
    );
    assert!(result.is_ok(), "got {result:?}");
}

#[test]
fn the_three_switches_turn_off() {
    let result = run_against_device(
        |d| {
            expect_request(d, "1f 05 02 01 00");
            answer(d, "1f 05 03 01 00");
            expect_request(d, "01 18 02 01 00");
            answer(d, "01 18 03 01 00");
            expect_request(d, "01 1b 02 01 00");
            answer(d, "01 1b 03 01 00");
        },
        |c| {
            set_remember_audio_mode(c, false)?;
            set_auto_play_pause(c, false)?;
            set_auto_answer_calls(c, false)
        },
    );
    assert!(result.is_ok(), "got {result:?}");
}

#[test]
fn a_switch_that_does_not_change_is_unconfirmed() {
    let result = run_against_device(
        |d| {
            expect_request(d, "01 18 02 01 00");
            answer(d, "01 18 03 01 01");
        },
        |c| set_auto_play_pause(c, false),
    );
    assert!(
        matches!(result, Err(BoseError::UnconfirmedValue { .. })),
        "got {result:?}"
    );
}

#[test]
fn the_shortcut_reads_battery_level() {
    let action = run_against_device(
        |d| {
            expect_request(d, "01 09 01 00");
            answer(d, "01 09 03 07 80 09 03 00 09 40 0a");
        },
        get_shortcut::<UnixStream>,
    );
    assert_eq!(action.expect("get_shortcut"), ShortcutAction::BatteryLevel);
}

#[test]
fn the_shortcut_set_to_immersive_audio() {
    let result = run_against_device(
        |d| {
            expect_request(d, "01 09 02 03 80 09 13");
            answer(d, "01 09 03 07 80 09 13 00 09 40 0a");
        },
        |c| set_shortcut(c, ShortcutAction::ImmersiveAudio),
    );
    assert!(result.is_ok(), "got {result:?}");
}
