// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Unpadded base64url and the `rci0_` text of a Resolver Construction
//! Identity.
//!
//! Design 015 presents a Resolver Construction Identity as `rci0_` followed by
//! the unpadded base64url (RFC 4648, section 5) of its 32 digest bytes: exactly
//! 43 characters. Decoding is canonical. Padding, characters outside the
//! alphabet, a length no byte string encodes to, and a last character whose
//! unused bits are not zero are all refused, so each byte string has exactly
//! one accepted text.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// The prefix of a revision-`"0"` Resolver Construction Identity's text.
pub(crate) const CONSTRUCTION_IDENTITY_PREFIX: &str = "rci0_";

/// The length of the base64url text of 32 bytes.
const DIGEST_TEXT_LENGTH: usize = 43;

/// Why text is not the canonical unpadded base64url of any byte string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextFailure {
    /// A character outside the base64url alphabet, padding included.
    Character,
    /// One more than a multiple of four characters, which no bytes encode to.
    Length,
    /// The last character has an unused bit set.
    UnusedBits,
}

/// Why text is not the text of a Resolver Construction Identity. Each is
/// `malformed-construction-identity` at the invocation boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IdentityTextFailure {
    /// The text does not start with `rci0_`.
    Prefix,
    /// The text after the prefix is not 43 characters long.
    Length,
    /// The 43 characters are not canonical base64url.
    Text(TextFailure),
}

/// The unpadded base64url of bytes.
pub(crate) fn encode(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let mut group = 0_u32;
        for (index, &byte) in chunk.iter().enumerate() {
            group |= u32::from(byte) << (16 - 8 * index);
        }
        // One byte gives two characters, two give three, and three give four.
        for index in 0..=chunk.len() {
            let value = (group >> (18 - 6 * index)) & 0x3f;
            text.push(char::from(ALPHABET[value as usize]));
        }
    }
    text
}

/// The bytes whose canonical unpadded base64url is the text.
pub(crate) fn decode(text: &str) -> Result<Vec<u8>, TextFailure> {
    let values = text
        .bytes()
        .map(value)
        .collect::<Option<Vec<u8>>>()
        .ok_or(TextFailure::Character)?;
    if values.len() % 4 == 1 {
        return Err(TextFailure::Length);
    }
    let mut bytes = Vec::with_capacity(values.len() * 3 / 4);
    for chunk in values.chunks(4) {
        let mut group = 0_u32;
        for (index, &value) in chunk.iter().enumerate() {
            group |= u32::from(value) << (18 - 6 * index);
        }
        // Two characters give one byte, three give two, and four give three;
        // the bits below the last byte were never part of one.
        let count = chunk.len() - 1;
        if group & (0x00ff_ffff >> (8 * count)) != 0 {
            return Err(TextFailure::UnusedBits);
        }
        for index in 0..count {
            bytes.push((group >> (16 - 8 * index)) as u8);
        }
    }
    Ok(bytes)
}

fn value(character: u8) -> Option<u8> {
    let value = match character {
        b'A'..=b'Z' => character - b'A',
        b'a'..=b'z' => character - b'a' + 26,
        b'0'..=b'9' => character - b'0' + 52,
        b'-' => 62,
        b'_' => 63,
        _ => return None,
    };
    Some(value)
}

/// The text of a Resolver Construction Identity's digest.
pub(crate) fn construction_identity(digest: &[u8; 32]) -> String {
    format!("{CONSTRUCTION_IDENTITY_PREFIX}{}", encode(digest))
}

/// The digest a Resolver Construction Identity's text presents.
pub(crate) fn parse_construction_identity(text: &str) -> Result<[u8; 32], IdentityTextFailure> {
    let rest = text
        .strip_prefix(CONSTRUCTION_IDENTITY_PREFIX)
        .ok_or(IdentityTextFailure::Prefix)?;
    if rest.len() != DIGEST_TEXT_LENGTH {
        return Err(IdentityTextFailure::Length);
    }
    let bytes = decode(rest).map_err(IdentityTextFailure::Text)?;
    Ok(<[u8; 32]>::try_from(bytes.as_slice()).expect("43 characters decode to 32 bytes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 4648's test vectors, which base64 and base64url spell alike.
    const RFC_4648: [(&str, &str); 7] = [
        ("", ""),
        ("f", "Zg"),
        ("fo", "Zm8"), // spellchecker:disable-line
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg"),
        ("fooba", "Zm9vYmE"),
        ("foobar", "Zm9vYmFy"),
    ];

    #[test]
    fn the_rfc_4648_vectors_encode_without_padding() {
        for (bytes, text) in RFC_4648 {
            assert_eq!(encode(bytes.as_bytes()), text, "{bytes:?}");
        }
    }

    #[test]
    fn the_rfc_4648_vectors_decode_back() {
        for (bytes, text) in RFC_4648 {
            assert_eq!(decode(text).unwrap(), bytes.as_bytes(), "{text:?}");
        }
    }

    #[test]
    fn the_last_two_values_are_dash_and_underscore() {
        // 0xfb 0xff is 111110 111111 1111(00): 62, 63, and 60.
        assert_eq!(encode(&[0xfb, 0xff]), "-_8");
        assert_eq!(decode("-_8").unwrap(), [0xfb, 0xff]);
        assert_eq!(encode(&[0xff; 3]), "____");
        assert_eq!(encode(&[0x00; 3]), "AAAA");
        assert_eq!(encode(&[0xf8, 0x00, 0x00]), "-AAA");
    }

    #[test]
    fn every_byte_survives_the_round_trip() {
        let all = (0..=255).collect::<Vec<u8>>();
        for end in 0..=all.len() {
            assert_eq!(decode(&encode(&all[..end])).unwrap(), &all[..end], "{end}");
        }
    }

    #[test]
    fn every_character_of_the_alphabet_has_its_value() {
        for (index, &character) in ALPHABET.iter().enumerate() {
            assert_eq!(value(character), Some(index as u8));
        }
        for character in *b"+/= @[`{:." {
            assert_eq!(value(character), None, "{}", char::from(character));
        }
    }

    #[test]
    fn padding_and_characters_outside_the_alphabet_are_refused() {
        for text in ["Zg==", "Zm8=", "+/8", "Zm 9v", "Zm9v\n", "é"] {
            assert_eq!(decode(text), Err(TextFailure::Character), "{text:?}");
        }
    }

    #[test]
    fn a_length_one_past_a_multiple_of_four_is_refused() {
        for text in ["A", "Zm9vY", "Zm9vYmFyZ"] {
            assert_eq!(decode(text), Err(TextFailure::Length), "{text:?}");
        }
        // A character problem anywhere comes first.
        assert_eq!(decode("Zm9v="), Err(TextFailure::Character));
    }

    #[test]
    fn a_last_character_with_an_unused_bit_set_is_refused() {
        // "f" is 011001 10(0000); any of the four unused bits set is refused.
        for text in ["Zh", "Zi", "Zk", "Zo"] {
            assert_eq!(decode(text), Err(TextFailure::UnusedBits), "{text:?}");
        }
        // Setting only data bits spells another byte: "Zw" is "g".
        assert_eq!(decode("Zw").unwrap(), b"g");
        // Bytes 0x66 0x6f are 011001 100110 1111(00); either unused bit set is
        // refused.
        for text in ["Zm9", "Zm-"] {
            assert_eq!(decode(text), Err(TextFailure::UnusedBits), "{text:?}");
        }
        // A whole group has no unused bits.
        assert_eq!(decode("____").unwrap(), [0xff; 3]);
    }

    #[test]
    fn an_identity_is_its_prefix_and_43_characters() {
        let zero = construction_identity(&[0; 32]);
        assert_eq!(zero, format!("rci0_{}", "A".repeat(43)));
        let ones = construction_identity(&[0xff; 32]);
        // 32 bytes end with two characters' worth, so the last value is 0b111100.
        assert_eq!(ones, format!("rci0_{}8", "_".repeat(42)));
        assert_eq!(parse_construction_identity(&zero), Ok([0; 32]));
        assert_eq!(parse_construction_identity(&ones), Ok([0xff; 32]));
        let digest = core::array::from_fn::<u8, 32, _>(|index| index as u8);
        let text = construction_identity(&digest);
        assert_eq!(text, "rci0_AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8");
        assert_eq!(parse_construction_identity(&text), Ok(digest));
    }

    #[test]
    fn an_identity_text_is_refused_for_its_first_defect() {
        let body = "A".repeat(43);
        assert_eq!(
            parse_construction_identity(&format!("rci1_{body}")),
            Err(IdentityTextFailure::Prefix)
        );
        assert_eq!(
            parse_construction_identity(&body),
            Err(IdentityTextFailure::Prefix)
        );
        for length in [0, 42, 44] {
            assert_eq!(
                parse_construction_identity(&format!("rci0_{}", "A".repeat(length))),
                Err(IdentityTextFailure::Length),
                "{length}"
            );
        }
        assert_eq!(
            parse_construction_identity(&format!("rci0_{}=", "A".repeat(42))),
            Err(IdentityTextFailure::Text(TextFailure::Character))
        );
        // The last character of 43 carries two unused bits.
        assert_eq!(
            parse_construction_identity(&format!("rci0_{}B", "A".repeat(42))),
            Err(IdentityTextFailure::Text(TextFailure::UnusedBits))
        );
    }
}
