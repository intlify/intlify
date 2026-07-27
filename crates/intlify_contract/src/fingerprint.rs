// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

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
