//! Base64, for the fonts and pictures themes carry in `data:` URIs. No
//! dependency, and forgiving about the whitespace editors add.

/// Decodes standard or URL-safe base64, with or without padding.
///
/// The padding belongs at the end of the text and nowhere else, the length
/// must be one a browser would read, and the bits left over at the end
/// must be zero: a sheet that breaks those rules is not one, and the
/// caller is told so rather than given half a picture.
pub fn decode(text: &str) -> Option<Vec<u8>> {
    let symbols: Vec<u8> = text
        .bytes()
        .filter(|byte| !matches!(byte, b'\n' | b'\r' | b'\t' | b' '))
        .collect();
    // The padding is at the end, and only there.
    let padding = symbols
        .iter()
        .rev()
        .take_while(|byte| **byte == b'=')
        .count();
    if padding > 2 {
        return None;
    }
    let body = &symbols[..symbols.len() - padding];
    if body.contains(&b'=') {
        return None;
    }
    // A padded sheet is a whole number of groups, and an unpadded one
    // leaves at least two symbols: a single symbol is not a whole byte.
    let length = symbols.len();
    if padding > 0 {
        if !length.is_multiple_of(4) {
            return None;
        }
    } else if length % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(length / 4 * 3 + 3);
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for &byte in body {
        let value = match byte {
            b'A'..=b'Z' => u32::from(byte - b'A'),
            b'a'..=b'z' => u32::from(byte - b'a') + 26,
            b'0'..=b'9' => u32::from(byte - b'0') + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        };
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    // The bits left over must be zero: a sheet that carries other bits in
    // its last symbol is not one a browser would read.
    if buffer != 0 {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_padded_and_unpadded() {
        assert_eq!(decode("AAEC"), Some(vec![0, 1, 2]));
        assert_eq!(decode("AAE="), Some(vec![0, 1]));
        assert_eq!(decode("AAE"), Some(vec![0, 1]));
        assert_eq!(decode("aGVsbG8="), Some(b"hello".to_vec()));
    }

    #[test]
    fn rejects_what_is_not_base64() {
        assert_eq!(decode("hello!"), None);
    }

    #[test]
    fn the_padding_belongs_at_the_end() {
        // A '=' in the middle is not a padding, and the sheet is not one.
        assert_eq!(decode("A=AA"), None);
        assert_eq!(decode("AA=A"), None);
        assert_eq!(decode("A==="), None);
    }

    #[test]
    fn a_sheet_that_is_too_short_is_not_one() {
        // A single symbol carries six bits, not a whole byte.
        assert_eq!(decode("A"), None);
        assert_eq!(decode("A="), None);
        assert_eq!(decode("AA"), Some(vec![0]));
    }

    #[test]
    fn the_bits_left_over_must_be_zero() {
        // "AB" carries four bits that a byte does not use.
        assert_eq!(decode("AB"), None);
        assert_eq!(decode("AA"), Some(vec![0]));
    }
}
