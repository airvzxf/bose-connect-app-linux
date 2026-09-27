//! Bluetooth address string conversion helpers.
//!
//! The Bose device emits addresses in canonical MSB-first byte
//! order, the same byte order BlueZ's `str2ba`/`ba2str` and the
//! original C `reverse_str2ba`/`reverse_ba2str` use. The historical
//! `reverse_` prefix on the C functions is misleading — it refers
//! to them being inverses of each other, not to any byte reversal.
//! We keep the same names in this module so the protocol code below
//! reads as a line-for-line translation of the original.

use crate::error::{BoseError, BoseResult};
use crate::types::BdAddr;

/// Convert an internal [`BdAddr`] (Bose byte order, MSB first) to a
/// canonical "AA:BB:CC:DD:EE:FF" string. Always writes 17 bytes
/// plus a NUL terminator.
pub fn reverse_ba2str(ba: &BdAddr) -> [u8; 18] {
    let mut out = [0u8; 18];
    for position in 0..6 {
        let string_position = position * 3;
        let hi = HEX[(ba.b[position] >> 4) as usize];
        let lo = HEX[(ba.b[position] & 0x0f) as usize];
        out[string_position] = hi;
        out[string_position + 1] = lo;
        out[string_position + 2] = b':';
    }
    out[17] = 0;
    out
}

/// Parse a canonical "AA:BB:CC:DD:EE:FF" string into an internal
/// [`BdAddr`] (Bose byte order). Returns an error if the input is
/// malformed.
pub fn parse_bdaddr(s: &str) -> Option<BdAddr> {
    // Equivalent of `bachk`: validate shape first.
    let bytes = s.as_bytes();
    if bytes.len() != 17 {
        return None;
    }
    // Validate separators at indices 2, 5, 8, 11, 14. Index 17 is
    // the trailing position which has no separator (the address is
    // 17 chars: "AA:BB:CC:DD:EE:FF"), so the loop stops at i=5 →
    // sep_idx=17, which we must skip.
    for &sep_idx in &[2usize, 5, 8, 11, 14] {
        if bytes[sep_idx] != b':' {
            return None;
        }
    }
    let mut out = BdAddr { b: [0; 6] };
    for i in 0..6 {
        let hi = hex_nibble(bytes[i * 3])?;
        let lo = hex_nibble(bytes[i * 3 + 1])?;
        out.b[i] = (hi << 4) | lo;
    }
    Some(out)
}

/// Parse, returning a richer error if invalid. Convenience for the
/// FFI layer.
pub(crate) fn parse_bdaddr_strict(s: &str) -> BoseResult<BdAddr> {
    parse_bdaddr(s).ok_or_else(|| BoseError::InvalidAddress(s.to_string()))
}

fn hex_nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'A'..=b'F' => Some(c - b'A' + 10),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    }
}

const HEX: &[u8; 16] = b"0123456789ABCDEF";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_known_address() {
        // AA:BB:CC:DD:EE:FF -> 0xAA 0xBB 0xCC 0xDD 0xEE 0xFF
        let input = "AA:BB:CC:DD:EE:FF";
        let parsed = parse_bdaddr(input).unwrap();
        assert_eq!(parsed.b, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
        let rendered = reverse_ba2str(&parsed);
        let s = std::str::from_utf8(&rendered[..17]).unwrap();
        assert_eq!(s, input);
    }

    #[test]
    fn parse_rejects_bad_shapes() {
        assert!(parse_bdaddr("").is_none());
        assert!(parse_bdaddr("AA:BB:CC:DD:EE").is_none()); // too short
        assert!(parse_bdaddr("AA:BB:CC:DD:EE:FF:11").is_none()); // too long
        assert!(parse_bdaddr("AA-BB-CC-DD-EE-FF").is_none()); // wrong sep
        assert!(parse_bdaddr("ZZ:BB:CC:DD:EE:FF").is_none()); // bad hex
        assert!(parse_bdaddr("aabbccddeeff").is_none()); // no separators
    }

    #[test]
    fn lower_case_input_accepted() {
        // The C code uses `strtol` which accepts both cases.
        let parsed = parse_bdaddr("aa:bb:cc:dd:ee:ff").unwrap();
        assert_eq!(parsed.b, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
    }

    #[test]
    fn reverse_ba2str_zero() {
        let rendered = reverse_ba2str(&BdAddr::ANY);
        assert_eq!(&rendered[..17], b"00:00:00:00:00:00");
        assert_eq!(rendered[17], 0);
    }
}
