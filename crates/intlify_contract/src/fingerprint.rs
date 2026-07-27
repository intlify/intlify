// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Canonical fingerprint-payload framing primitives.
//!
//! This module owns the byte-exact tagged-field and length-prefixed sequence
//! encoding shared by contract identities. The framing makes field boundaries
//! and collection order unambiguous before a caller computes a fingerprint.
//!
//! It does not choose a hash algorithm, compute a digest, or define an artifact
//! wire format. Those responsibilities belong to the artifact or linker layer
//! that consumes the canonical payload.

#[allow(dead_code)]
pub(crate) trait FingerprintPayload {
    fn write_fingerprint_payload(&self, output: &mut Vec<u8>);

    fn fingerprint_payload(&self) -> Box<[u8]> {
        let mut output = Vec::new();
        self.write_fingerprint_payload(&mut output);
        output.into_boxed_slice()
    }
}

#[allow(dead_code)]
pub(crate) fn write_tagged_field(tag: u8, payload: &[u8], output: &mut Vec<u8>) {
    output.push(tag);
    output.extend_from_slice(
        &u64::try_from(payload.len())
            .expect("a Rust slice length always fits into u64 on supported targets")
            .to_be_bytes(),
    );
    output.extend_from_slice(payload);
}

#[allow(dead_code)]
pub(crate) fn write_sequence<'a>(items: impl IntoIterator<Item = &'a [u8]>, output: &mut Vec<u8>) {
    let items = items.into_iter().collect::<Vec<_>>();
    output.extend_from_slice(
        &u64::try_from(items.len())
            .expect("a Rust slice length always fits into u64 on supported targets")
            .to_be_bytes(),
    );
    for item in items {
        output.extend_from_slice(
            &u64::try_from(item.len())
                .expect("a Rust slice length always fits into u64 on supported targets")
                .to_be_bytes(),
        );
        output.extend_from_slice(item);
    }
}

#[cfg(test)]
mod tests {
    use super::{write_sequence, write_tagged_field, FingerprintPayload};

    struct FixedPayload;

    impl FingerprintPayload for FixedPayload {
        fn write_fingerprint_payload(&self, output: &mut Vec<u8>) {
            output.extend_from_slice(b"payload");
        }
    }

    #[test]
    fn payload_helper_collects_the_exact_written_bytes() {
        assert_eq!(FixedPayload.fingerprint_payload().as_ref(), b"payload");
    }

    #[test]
    fn tagged_field_encodes_tag_u64be_length_and_payload() {
        let mut output = vec![0xff];
        write_tagged_field(0x03, b"abc", &mut output);
        assert_eq!(
            output,
            [0xff, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, b'a', b'b', b'c',]
        );
    }

    #[test]
    fn sequence_encodes_count_order_and_empty_item_boundaries() {
        let items: [&[u8]; 3] = [b"ab", b"", b"c"];
        let mut output = Vec::new();
        write_sequence(items, &mut output);
        assert_eq!(
            output,
            [
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, // item count
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, b'a', b'b', 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, b'c',
            ]
        );
    }

    #[test]
    fn empty_sequence_encodes_only_a_zero_u64be_count() {
        let items: [&[u8]; 0] = [];
        let mut output = Vec::new();
        write_sequence(items, &mut output);
        assert_eq!(output, [0; 8]);
    }
}
