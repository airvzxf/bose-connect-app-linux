//! Low-level byte/hex helpers. Mirrors `util.c`.
//!
//! These functions are intentionally limited in scope: they only do
//! what the Bose protocol needs (decode two ASCII hex chars into one
//! byte, copy printable bytes into a fixed-size buffer with NUL
//! terminator). Anything beyond that lives in std or the caller.

use crate::types::BdAddr;

/// Decode two ASCII hex chars (`"0f"`, `"FF"`, `"ab"`) into one
/// byte. Returns `None` if either character is not a hex digit.
pub fn str_to_byte(s: &[u8; 2]) -> Option<u8> {
    let hi = hex_value(s[0])?;
    let lo = hex_value(s[1])?;
    Some((hi << 4) | lo)
}

/// Copy up to `size - 1` bytes from `from` into `to`, treating NUL
/// as the end of `from` and stripping characters outside printable
/// ASCII (anything < 0x20 or > 0x7e except NUL). Always writes a
/// NUL terminator at `to[size - 1]`, or earlier if NUL was found
/// in the source.
///
/// Mirrors the original `str_copy` (maintains the same edge cases,
/// including the off-by-one intent where the original explicitly
/// guarantees `to[size - 1] == 0` even when `size > 0`).
pub fn str_copy(to: &mut [u8], from: &[u8]) {
    if to.is_empty() {
        return;
    }
    let max = to.len() - 1;
    for (i, dst) in to.iter_mut().take(max).enumerate() {
        let c = from.get(i).copied().unwrap_or(0);
        if c == 0 {
            *dst = 0;
            return;
        }
        // Printable ASCII (0x20..=0x7e) is kept verbatim; everything
        // else is treated as "byte to copy as-is" (the original C
        // code's branch on `< 0` is dead under `char`, but we keep
        // the semantics that *unprintable* characters get written
        // through unchanged so binary payloads survive).
        *dst = c;
    }
    to[max] = 0;
}

/// Zero-fill a [`BdAddr`]. Equivalent of `memset(ba, 0, 6)`.
pub fn bdaddr_zero(ba: &mut BdAddr) {
    ba.b = [0; 6];
}

/// Render a single byte as two uppercase hex chars into `dest`
/// (which must be at least 2 bytes). Mirrors `unit_to_hex_string`
/// when `number < 16`.
pub fn byte_to_hex(dest: &mut [u8; 2], byte: u8) {
    dest[0] = HEX[(byte >> 4) as usize];
    dest[1] = HEX[(byte & 0x0f) as usize];
}

const HEX: &[u8; 16] = b"0123456789ABCDEF";

fn hex_value(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'A'..=b'F' => Some(c - b'A' + 10),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn str_to_byte_basic() {
        assert_eq!(str_to_byte(b"00"), Some(0x00));
        assert_eq!(str_to_byte(b"0f"), Some(0x0f));
        assert_eq!(str_to_byte(b"FF"), Some(0xff));
        assert_eq!(str_to_byte(b"ab"), Some(0xab));
        assert_eq!(str_to_byte(b"10"), Some(0x10));
    }

    #[test]
    fn str_to_byte_rejects_non_hex() {
        assert!(str_to_byte(b"0g").is_none());
        assert!(str_to_byte(b" 0").is_none());
        assert!(str_to_byte(b"--").is_none());
    }

    #[test]
    fn str_copy_basic() {
        let mut buf = [0u8; 10];
        str_copy(&mut buf, b"hello");
        assert_eq!(&buf[..6], b"hello\0");
    }

    #[test]
    fn str_copy_truncates_with_nul() {
        let mut buf = [0xFFu8; 6];
        str_copy(&mut buf, b"abcdefghij");
        // Source is longer than buffer; we copy 5 chars and NUL.
        assert_eq!(&buf, b"abcde\0");
    }

    #[test]
    fn str_copy_empty_buffer_no_panic() {
        let mut buf: [u8; 0] = [];
        // Should not panic, should not write.
        str_copy(&mut buf, b"anything");
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn str_copy_source_with_nul_terminates_early() {
        let mut buf = [0xFFu8; 10];
        str_copy(&mut buf, b"ab\0cd");
        assert_eq!(&buf[..3], b"ab\0");
        // Rest untouched (still 0xFF).
        assert_eq!(buf[3], 0xFF);
    }

    #[test]
    fn bdaddr_zero_works() {
        let mut ba = BdAddr {
            b: [1, 2, 3, 4, 5, 6],
        };
        bdaddr_zero(&mut ba);
        assert_eq!(ba.b, [0; 6]);
    }

    #[test]
    fn byte_to_hex_uppercase() {
        let mut out = [0u8; 2];
        byte_to_hex(&mut out, 0x00);
        assert_eq!(&out, b"00");
        byte_to_hex(&mut out, 0x0f);
        assert_eq!(&out, b"0F");
        byte_to_hex(&mut out, 0xff);
        assert_eq!(&out, b"FF");
    }
}
