//! Integration tests for the protocol layer.
//!
//! These tests build a `UnixStream` pair and use one end as a fake
//! Bose device: the test thread drives the protocol module against
//! the other end, and a mock-side routine reads what the protocol
//! sent, then writes the device's expected reply. We assert
//! byte-for-byte that the protocol sent the same packets the
//! original C code would have, and that we correctly parse the
//! device's reply.
//!
//! The point is to catch *any* accidental drift in the on-wire
//! byte sequences — a single off-by-one in a packet length
//! constant would make these tests fail loudly, well before the
//! code reaches a real Bose headphone.
//!
//! No Bluetooth hardware required; runs in CI on the standard
//! GitHub-hosted Ubuntu runner.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::thread;

use bose_connect::protocol::{
    get_audio_mode, get_audio_mode_name, get_battery_level, get_device_id, get_device_status,
    get_firmware_version, get_serial_number, set_audio_mode, set_auto_off, set_name, set_pairing,
    set_prompt_language, set_self_voice, set_voice_prompts,
};
use bose_connect::{
    AutoOff, BdAddr, BoseError, NoiseCancelling, Pairing, PromptLanguage, SelfVoice, VP_ENABLE_BIT,
};

/// Helper: create a `UnixStream` pair and hand back (client, server)
/// streams. The `client` end is what the protocol module sees; the
/// `server` end is driven by the mock-device thread.
fn mock_pair() -> (UnixStream, UnixStream) {
    UnixStream::pair().expect("UnixStream::pair")
}

/// Run `client_op` in a thread and let the server end simulate the
/// Bose device. `server_op` receives the server stream and is
/// expected to consume whatever the client sends and reply with the
/// device's expected response.
fn with_mock_device<F, G, R>(client_op: F, server_op: G) -> R
where
    F: FnOnce(&mut UnixStream) -> R + Send + 'static,
    G: FnOnce(&mut UnixStream) + Send + 'static,
    R: Send + 'static,
{
    let (mut client, mut server) = mock_pair();
    let server_handle = thread::spawn(move || {
        server_op(&mut server);
    });
    let result = client_op(&mut client);
    // Wait for the server thread to drain; if it panicked we
    // surface the panic to the caller via `join`.
    server_handle.join().expect("server thread panicked");
    result
}

// ---------------------------------------------------------------------------
// Each test below does the same dance:
//   1. Open a UnixStream pair.
//   2. Spawn a thread that pretends to be the Bose device — it
//      reads what the protocol sent, then writes the canned reply.
//   3. Call the protocol function under test against the client
//      stream.
//   4. Verify the result matches what the device was supposed to
//      echo.
// ---------------------------------------------------------------------------

#[test]
fn get_device_id_round_trip() {
    let result = with_mock_device(
        |client| {
            // Reply on the wire:
            //   4 bytes ACK: 00 03 03 03
            //   2 bytes device id (big-endian): 40 20
            //   1 byte index: 02
            // (from the server thread)
            get_device_id(client)
        },
        |server| {
            let mut buf = [0u8; 4];
            server.read_exact(&mut buf).unwrap();
            assert_eq!(buf, [0x00, 0x03, 0x01, 0x00], "device-id send packet");
            server.write_all(&[0x00, 0x03, 0x03, 0x03]).unwrap();
            server.write_all(&[0x40, 0x20]).unwrap(); // device id BE
            server.write_all(&[0x02]).unwrap(); // index
        },
    );
    assert_eq!(result.unwrap(), (0x4020, 0x02));
}

#[test]
fn get_battery_level_round_trip() {
    let result = with_mock_device(get_battery_level, |server| {
        let mut buf = [0u8; 4];
        server.read_exact(&mut buf).unwrap();
        assert_eq!(buf, [0x02, 0x02, 0x01, 0x00], "battery send packet");
        server.write_all(&[0x02, 0x02, 0x03, 0x01]).unwrap();
        server.write_all(&[42]).unwrap();
    });
    assert_eq!(result.unwrap(), 42);
}

#[test]
fn get_firmware_version_round_trip() {
    let result = with_mock_device(get_firmware_version, |server| {
        let mut buf = [0u8; 4];
        server.read_exact(&mut buf).unwrap();
        assert_eq!(buf, [0x00, 0x05, 0x01, 0x00], "firmware send packet");
        server.write_all(&[0x00, 0x05, 0x03, 0x05]).unwrap();
        server.write_all(b"1.3.2").unwrap();
    });
    assert_eq!(result.unwrap(), "1.3.2");
}

#[test]
fn get_serial_number_round_trip() {
    let result = with_mock_device(get_serial_number, |server| {
        let mut buf = [0u8; 4];
        server.read_exact(&mut buf).unwrap();
        assert_eq!(buf, [0x00, 0x07, 0x01, 0x00], "serial send packet");
        server.write_all(&[0x00, 0x07, 0x03]).unwrap();
        server.write_all(&[12]).unwrap();
        server.write_all(b"073456789ABC").unwrap();
    });
    assert_eq!(result.unwrap(), "073456789ABC");
}

#[test]
fn set_name_round_trip() {
    let result = with_mock_device(
        |client| set_name(client, "Bose QC35"),
        |server| {
            // set_name sends a variable-length packet: 4-byte prefix
            // + name length byte (at byte 3) + name bytes (at byte
            // 4 onwards). The name is 9 chars.
            let mut buf = [0u8; 4];
            server.read_exact(&mut buf).unwrap();
            assert_eq!(buf, [0x01, 0x02, 0x02, 0x09], "set-name send prefix");
            let mut name_buf = [0u8; 9];
            server.read_exact(&mut name_buf).unwrap();
            assert_eq!(&name_buf, b"Bose QC35");

            // Device echoes back the same name via get_name.
            // get_name first sends a 5-byte masked-ACK response:
            //   01 02 03 LL 00   (where LL is name length + 1)
            // then the name bytes.
            server.write_all(&[0x01, 0x02, 0x03, 0x0a, 0x00]).unwrap();
            server.write_all(b"Bose QC35").unwrap();
        },
    );
    result.unwrap();
}

#[test]
fn set_pairing_round_trip() {
    let result = with_mock_device(
        |client| set_pairing(client, Pairing::On),
        |server| {
            let mut buf = [0u8; 5];
            server.read_exact(&mut buf).unwrap();
            assert_eq!(buf, [0x04, 0x08, 0x05, 0x01, 0x01], "pairing on packet");
            // ACK: same with opcode 0x06.
            server.write_all(&[0x04, 0x08, 0x06, 0x01, 0x01]).unwrap();
        },
    );
    result.unwrap();
}

#[test]
fn set_self_voice_round_trip() {
    let result = with_mock_device(
        |client| set_self_voice(client, SelfVoice::High),
        |server| {
            let mut buf = [0u8; 7];
            server.read_exact(&mut buf).unwrap();
            assert_eq!(
                buf,
                [0x01, 0x0b, 0x02, 0x02, 0x01, 0x01, 0x38],
                "self-voice high packet"
            );
            server
                .write_all(&[0x01, 0x0b, 0x03, 0x03, 0x01, 0x01, 0x0f])
                .unwrap();
        },
    );
    result.unwrap();
}

#[test]
fn set_auto_off_round_trip() {
    let result = with_mock_device(
        |client| set_auto_off(client, AutoOff::Min20),
        |server| {
            // send: 01 04 02 01 14
            // device returns the masked-ACK then a 5-byte response
            // (01 04 03 01 14).
            let mut buf = [0u8; 5];
            server.read_exact(&mut buf).unwrap();
            assert_eq!(buf, [0x01, 0x04, 0x02, 0x01, 0x14], "auto-off 20min packet");
            // 5-byte auto-off GET response: 01 04 03 01 LL
            // mask: 01 04 03 01 00 — so LL is don't care.
            // We send back 20 to confirm.
            server.write_all(&[0x01, 0x04, 0x03, 0x01, 0x14]).unwrap();
        },
    );
    result.unwrap();
}

#[test]
fn set_prompt_language_round_trip() {
    // Send language byte 0x21 (English) with VP_ENABLE_BIT set.
    let byte = (PromptLanguage::En as u8) | VP_ENABLE_BIT;
    let result = with_mock_device(
        move |client| set_prompt_language(client, byte),
        move |server| {
            // send: 01 03 02 01 21
            let mut buf = [0u8; 5];
            server.read_exact(&mut buf).unwrap();
            assert_eq!(
                buf,
                [0x01, 0x03, 0x02, 0x01, byte],
                "set-prompt-language packet"
            );
            // 9-byte prompt-language GET response, masked ACK:
            //   01 03 03 05 LL 00 LL2 LL3 DE
            // mask:
            //   FF FF FF FF 00 FF 00 00 FF
            // so LL (byte 4) is the language byte, LL2/LL3 are
            // don't-care, byte 6 is don't-care.
            server
                .write_all(&[0x01, 0x03, 0x03, 0x05, 0x21, 0x00, 0x00, 0x00, 0xde])
                .unwrap();
        },
    );
    result.unwrap();
}

#[test]
fn set_voice_prompts_round_trip() {
    // set_voice_prompts first runs get_device_status which calls
    // get_device_id, then reads the rest of the status fields, then
    // re-sends the prompt-language with the bit flipped. This is
    // the most complex single round-trip in the protocol layer, so
    // it gets its own test.
    let result = with_mock_device(
        |client| set_voice_prompts(client, true),
        |server| {
            // Step 1: get_device_id.
            let mut buf = [0u8; 4];
            server.read_exact(&mut buf).unwrap();
            assert_eq!(buf, [0x00, 0x03, 0x01, 0x00]);
            server.write_all(&[0x00, 0x03, 0x03, 0x03]).unwrap();
            server.write_all(&[0x40, 0x20]).unwrap();
            server.write_all(&[0x01]).unwrap();

            // Step 2: GET_DEVICE_STATUS_SEND — server reads first,
            // then writes the ACK.
            let mut buf = [0u8; 4];
            server.read_exact(&mut buf).unwrap();
            assert_eq!(buf, [0x01, 0x01, 0x05, 0x00]);
            server.write_all(&[0x01, 0x01, 0x07, 0x00]).unwrap();

            // Step 3: get_name — masked ACK + 0-byte name (length
            // byte 1 means name is 0 chars).
            server.write_all(&[0x01, 0x02, 0x03, 0x01, 0x00]).unwrap();
            // No name bytes follow because length-1 = 0.

            // Step 4: get_prompt_language — 9-byte masked ACK.
            // Byte 4 is the language; we echo en=0x21 with VP bit
            // currently OFF so the test exercises the bit-set path.
            server
                .write_all(&[0x01, 0x03, 0x03, 0x05, 0x21, 0x00, 0x00, 0x00, 0xde])
                .unwrap();

            // Step 5: get_auto_off — 5-byte masked ACK.
            server.write_all(&[0x01, 0x04, 0x03, 0x01, 0x00]).unwrap();

            // Step 6: has_noise_cancelling(0x4020) is true, so
            // get_noise_cancelling is called. 6-byte masked ACK
            // (byte 4 is don't-care, but the constant is 0x0b).
            server
                .write_all(&[0x01, 0x06, 0x03, 0x02, 0x01, 0x0b])
                .unwrap();

            // Step 7: GET_DEVICE_STATUS_FINAL_ACK. The client
            // *reads* the 4-byte final ACK — the server writes it.
            server.write_all(&[0x01, 0x01, 0x06, 0x00]).unwrap();

            // Step 8: SET_PROMPT_LANGUAGE_SEND with VP bit SET.
            // The protocol reads back what the device stored; we
            // echo the same byte so the test passes.
            let mut buf = [0u8; 5];
            server.read_exact(&mut buf).unwrap();
            assert_eq!(
                buf,
                [
                    0x01,
                    0x03,
                    0x02,
                    0x01,
                    PromptLanguage::En as u8 | VP_ENABLE_BIT
                ],
                "set_voice_prompts(true) should re-send with VP bit set"
            );
            server
                .write_all(&[
                    0x01,
                    0x03,
                    0x03,
                    0x05,
                    PromptLanguage::En as u8 | VP_ENABLE_BIT,
                    0x00,
                    0x00,
                    0x00,
                    0xde,
                ])
                .unwrap();
        },
    );
    result.unwrap();
}

#[test]
fn ack_mismatch_surfaces_as_error() {
    let result: Result<(), BoseError> = with_mock_device(
        |client| set_pairing(client, Pairing::Off),
        |server| {
            let mut buf = [0u8; 5];
            server.read_exact(&mut buf).unwrap();
            assert_eq!(buf, [0x04, 0x08, 0x05, 0x01, 0x00]);
            // Reply with the wrong ACK (different pairing byte).
            server.write_all(&[0x04, 0x08, 0x06, 0x01, 0x01]).unwrap();
        },
    );
    assert!(matches!(result, Err(BoseError::AckMismatch)));
}

#[test]
fn short_read_surfaces_as_error() {
    // Server sends fewer bytes than the protocol expects. The
    // short-read path must surface as BoseError::ShortRead.
    let result: Result<u8, BoseError> = with_mock_device(get_battery_level, |server| {
        let mut buf = [0u8; 4];
        server.read_exact(&mut buf).unwrap();
        // ACK OK, then no level byte.
        server.write_all(&[0x02, 0x02, 0x03, 0x01]).unwrap();
        // Close the stream to signal EOF.
        let _ = server;
    });
    assert!(
        matches!(result, Err(BoseError::ShortRead { .. })),
        "got {result:?}"
    );
}

#[test]
fn set_name_too_long_rejected_early() {
    // set_name must reject names >= MAX_NAME_LEN before sending
    // anything on the wire. We use a UnixStream pair to detect any
    // accidental write.
    let (mut client, server) = mock_pair();
    let long = "x".repeat(64);
    let result = set_name(&mut client, &long);
    assert!(matches!(result, Err(BoseError::InvalidArgument(_))));
    // Drop both ends so the server's `read_to_end` returns EOF.
    drop(client);
    let mut server = server;
    let mut buf = Vec::new();
    let _ = server.read_to_end(&mut buf);
    assert!(buf.is_empty(), "no bytes should reach the wire");

    // Suppress dead-code warning for BdAddr import (kept for the
    // connect/disconnect-device tests below if we add them).
    let _ = BdAddr::ANY;
}

// Suppress dead-code warning for the VP_MASK constant — we use it
// implicitly through the masked-ACK machinery but a strict
// `-D warnings` build still flags the unused import.
#[allow(dead_code)]
const _: u8 = bose_connect::VP_MASK;

// ---------------------------------------------------------------------------
// QC Ultra Headphones (device id 0x4066, firmware 1.6.7). The replies
// below are byte-for-byte captures from a real device.
// ---------------------------------------------------------------------------

/// Read a request of `N` bytes from the mock device side and check it.
fn expect_request<const N: usize>(server: &mut UnixStream, expected: [u8; N]) {
    let mut buf = [0u8; N];
    server.read_exact(&mut buf).unwrap();
    assert_eq!(buf, expected);
}

#[test]
fn qc_ultra_firmware_version_is_variable_length() {
    let v = with_mock_device(
        |client| get_firmware_version(client).unwrap(),
        |server| {
            expect_request(server, [0x00, 0x05, 0x01, 0x00]);
            server.write_all(&[0x00, 0x05, 0x03, 0x0e]).unwrap();
            server.write_all(b"1.6.7+g6ebabd2").unwrap();
        },
    );
    assert_eq!(v, "1.6.7+g6ebabd2");
}

#[test]
fn qc_ultra_battery_level_ignores_extra_bytes() {
    let level = with_mock_device(
        |client| get_battery_level(client).unwrap(),
        |server| {
            expect_request(server, [0x02, 0x02, 0x01, 0x00]);
            server
                .write_all(&[0x02, 0x02, 0x03, 0x04, 0x64, 0xff, 0xff, 0x00])
                .unwrap();
        },
    );
    assert_eq!(level, 100);
}

#[test]
fn qc_ultra_device_status() {
    let status = with_mock_device(
        |client| get_device_status(client).unwrap(),
        |server| {
            expect_request(server, [0x00, 0x03, 0x01, 0x00]);
            server
                .write_all(&[0x00, 0x03, 0x03, 0x03, 0x40, 0x66, 0x01])
                .unwrap();
            expect_request(server, [0x01, 0x01, 0x05, 0x00]);
            let mut reply = vec![0x01, 0x01, 0x07, 0x00];
            reply.extend_from_slice(&[0x01, 0x00, 0x03, 0x05]);
            reply.extend_from_slice(b"1.1.0");
            reply.extend_from_slice(&[0x01, 0x02, 0x03, 0x19, 0x00]);
            reply.extend_from_slice(b"Bose QC Ultra Headphones");
            reply.extend_from_slice(&[
                0x01, 0x03, 0x03, 0x07, 0xe1, 0x00, 0x01, 0x81, 0x5e, 0x01, 0x01,
            ]);
            reply.extend_from_slice(&[0x01, 0x04, 0x03, 0x03, 0xa0, 0x00, 0x05]);
            reply.extend_from_slice(&[0x01, 0x05, 0x03, 0x03, 0x0b, 0x00, 0x03]);
            reply.extend_from_slice(&[
                0x01, 0x07, 0x03, 0x0c, 0xf6, 0x0a, 0x00, 0x00, 0xf6, 0x0a, 0x00, 0x01, 0xf6, 0x0a,
                0x00, 0x02,
            ]);
            reply.extend_from_slice(&[0x01, 0x0b, 0x03, 0x03, 0x01, 0x02, 0x0f]);
            reply.extend_from_slice(&[0x01, 0x1b, 0x03, 0x01, 0x01]);
            reply.extend_from_slice(&[0x01, 0x01, 0x06, 0x00]);
            server.write_all(&reply).unwrap();
        },
    );
    assert_eq!(status.device_id, 0x4066);
    assert_eq!(status.name, "Bose QC Ultra Headphones");
    assert_eq!(status.language, 0xe1);
    assert_eq!(status.minutes, None);
    assert_eq!(status.level, NoiseCancelling::Dne);
}

#[test]
fn qc_ultra_get_audio_mode() {
    let mode = with_mock_device(
        |client| get_audio_mode(client).unwrap(),
        |server| {
            expect_request(server, [0x1f, 0x03, 0x01, 0x00]);
            server.write_all(&[0x1f, 0x03, 0x03, 0x01, 0x02]).unwrap();
        },
    );
    assert_eq!(mode, 2);
}

/// `1f 06` reply for slot `index` holding `name`.
fn audio_mode_config(index: u8, name: &str) -> Vec<u8> {
    let mut payload = vec![index, 0x00, 0x02, 0x00, 0x00, 0x01];
    let mut field = [0u8; 32];
    field[..name.len()].copy_from_slice(name.as_bytes());
    payload.extend_from_slice(&field);
    payload.extend_from_slice(&[0x00; 9]);
    let mut reply = vec![0x1f, 0x06, 0x03, payload.len() as u8];
    reply.extend_from_slice(&payload);
    reply
}

#[test]
fn qc_ultra_audio_mode_names() {
    let names = with_mock_device(
        |client| {
            (
                get_audio_mode_name(client, 1).unwrap(),
                get_audio_mode_name(client, 3).unwrap(),
            )
        },
        |server| {
            expect_request(server, [0x1f, 0x06, 0x01, 0x01, 0x01]);
            server.write_all(&audio_mode_config(1, "Aware")).unwrap();
            expect_request(server, [0x1f, 0x06, 0x01, 0x01, 0x03]);
            server.write_all(&audio_mode_config(3, "None")).unwrap();
        },
    );
    assert_eq!(names, (Some("Aware".to_string()), None));
}

#[test]
fn qc_ultra_set_audio_mode() {
    let result = with_mock_device(
        |client| set_audio_mode(client, 1),
        |server| {
            expect_request(server, [0x1f, 0x03, 0x05, 0x02, 0x01, 0x00]);
            server.write_all(&[0x1f, 0x03, 0x06, 0x01, 0x01]).unwrap();
        },
    );
    assert!(result.is_ok(), "expected Ok, got {:?}", result);
}

#[test]
fn device_error_packet_surfaces_as_error() {
    let result = with_mock_device(
        |client| get_audio_mode_name(client, 10),
        |server| {
            expect_request(server, [0x1f, 0x06, 0x01, 0x01, 0x0a]);
            server.write_all(&[0x1f, 0x06, 0x04, 0x01, 0x08]).unwrap();
        },
    );
    assert!(matches!(
        result,
        Err(BoseError::DeviceError {
            block: 0x1f,
            function: 0x06,
            code: 0x08
        })
    ));
}
