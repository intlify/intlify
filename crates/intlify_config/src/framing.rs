// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The field framing of design 015's typed records.
//!
//! Design 015 frames its Resolver Construction Identity, the Root Package's
//! canonical digest, both Redacted tokens, the content-derived source
//! revisions, and the Programmatic Entry Snapshot's marker records the same
//! way. A field frame is `tag:u8 || payload_len:u64be || payload`. A record's
//! fields carry the record-local tags `0x01`, `0x02`, and so on, each exactly
//! once and in ascending order, and a record is not itself framed: its bytes are
//! its field frames. A collection payload is `count:u64be` followed by
//! `item_len:u64be || item` for every item.
//!
//! Reading is strict. Nothing is repaired, skipped, or reordered, and a reader
//! never allocates from a count it has not yet seen the items for.

/// Why bytes are not one record or collection of the expected shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameFailure {
    /// The bytes end inside a length or a payload.
    Truncated,
    /// A frame has tag `0x00`, which no field has.
    ZeroTag,
    /// A frame's tag is beyond the record's last field.
    UnknownTag,
    /// A tag occurs a second time.
    RepeatedTag,
    /// A tag follows a greater one.
    OutOfOrder,
    /// One of the record's tags never occurs.
    MissingTag,
    /// A collection holds more or fewer items than its count says.
    CountMismatch,
}

/// One record being written, each field taking the next tag from `0x01`.
#[derive(Debug, Default)]
pub(crate) struct Record {
    bytes: Vec<u8>,
    fields: u8,
}

impl Record {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Append the next field.
    ///
    /// # Panics
    ///
    /// Panics at a 256th field. 015's records have a handful of fixed fields,
    /// so a record that long is a defect in the caller.
    #[must_use]
    pub(crate) fn field(mut self, payload: &[u8]) -> Self {
        self.fields = self
            .fields
            .checked_add(1)
            .expect("a 015 record has at most 255 fields");
        frame(self.fields, payload, &mut self.bytes);
        self
    }

    /// The record's bytes: its field frames, not wrapped in another frame.
    pub(crate) fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

/// Write one field frame.
pub(crate) fn frame(tag: u8, payload: &[u8], output: &mut Vec<u8>) {
    output.push(tag);
    output.extend_from_slice(&wide(payload.len()).to_be_bytes());
    output.extend_from_slice(payload);
}

/// One collection payload: the item count, then each item's length and bytes.
pub(crate) fn collection<'a>(items: impl IntoIterator<Item = &'a [u8]>) -> Vec<u8> {
    let items = items.into_iter().collect::<Vec<_>>();
    let mut output = Vec::new();
    output.extend_from_slice(&wide(items.len()).to_be_bytes());
    for item in items {
        output.extend_from_slice(&wide(item.len()).to_be_bytes());
        output.extend_from_slice(item);
    }
    output
}

fn wide(value: usize) -> u64 {
    u64::try_from(value).expect("a slice length fits in u64 on every supported target")
}

/// Split bytes into their field frames, in order, without judging the tags.
///
/// A caller whose record shape depends on a field, like a Snapshot marker
/// whose reason decides whether it has one field or three, reads the frames
/// first and then checks the tags with [`tags`].
pub(crate) fn frames(bytes: &[u8]) -> Result<Vec<(u8, &[u8])>, FrameFailure> {
    let mut frames = Vec::new();
    let mut rest = bytes;
    while let Some((&tag, after)) = rest.split_first() {
        let (payload, next) = span(after)?;
        frames.push((tag, payload));
        rest = next;
    }
    Ok(frames)
}

/// Check that the frames' tags are exactly `0x01` to `fields`, each once, in
/// ascending order.
///
/// The first frame that breaks the order decides: tag `0x00`, then a tag beyond
/// `fields`, then a tag seen before, then a tag below its predecessor. A record
/// whose frames all pass but lack a tag fails last, as missing it.
pub(crate) fn tags(frames: &[(u8, &[u8])], fields: u8) -> Result<(), FrameFailure> {
    let mut seen = [false; 256];
    let mut last = 0;
    for &(tag, _) in frames {
        if tag == 0 {
            return Err(FrameFailure::ZeroTag);
        }
        if tag > fields {
            return Err(FrameFailure::UnknownTag);
        }
        if seen[usize::from(tag)] {
            return Err(FrameFailure::RepeatedTag);
        }
        if tag < last {
            return Err(FrameFailure::OutOfOrder);
        }
        seen[usize::from(tag)] = true;
        last = tag;
    }
    if (1..=fields).any(|tag| !seen[usize::from(tag)]) {
        return Err(FrameFailure::MissingTag);
    }
    Ok(())
}

/// Read one record of exactly `fields` fields, returning their payloads in tag
/// order.
pub(crate) fn record(bytes: &[u8], fields: u8) -> Result<Vec<&[u8]>, FrameFailure> {
    let frames = frames(bytes)?;
    tags(&frames, fields)?;
    Ok(frames.into_iter().map(|(_, payload)| payload).collect())
}

/// Read one collection payload, returning its items in order.
pub(crate) fn items(bytes: &[u8]) -> Result<Vec<&[u8]>, FrameFailure> {
    let (count, mut rest) = bytes
        .split_first_chunk::<8>()
        .ok_or(FrameFailure::Truncated)?;
    let count = u64::from_be_bytes(*count);
    let mut items = Vec::new();
    while !rest.is_empty() {
        if wide(items.len()) == count {
            return Err(FrameFailure::CountMismatch);
        }
        let (item, next) = span(rest)?;
        items.push(item);
        rest = next;
    }
    if wide(items.len()) != count {
        return Err(FrameFailure::CountMismatch);
    }
    Ok(items)
}

/// One length-prefixed span: an eight-byte big-endian length, then that many
/// bytes, and what follows them.
fn span(bytes: &[u8]) -> Result<(&[u8], &[u8]), FrameFailure> {
    let (prefix, rest) = bytes
        .split_first_chunk::<8>()
        .ok_or(FrameFailure::Truncated)?;
    let length = u64::from_be_bytes(*prefix);
    if length > wide(rest.len()) {
        return Err(FrameFailure::Truncated);
    }
    // No more than the bytes that remain, so it fits in usize.
    Ok(rest.split_at(length as usize))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One frame written out by hand: tag, eight length bytes, payload.
    fn by_hand(tag: u8, payload: &[u8]) -> Vec<u8> {
        let mut bytes = vec![tag, 0, 0, 0, 0, 0, 0, 0];
        bytes.push(u8::try_from(payload.len()).unwrap());
        bytes.extend_from_slice(payload);
        bytes
    }

    #[test]
    fn a_frame_is_its_tag_then_a_big_endian_length_then_the_payload() {
        let mut output = vec![0xff];
        frame(0x03, b"abc", &mut output);
        assert_eq!(
            output,
            [0xff, 0x03, 0, 0, 0, 0, 0, 0, 0, 0x03, b'a', b'b', b'c']
        );
        let mut empty = Vec::new();
        frame(0x01, b"", &mut empty);
        assert_eq!(empty, [0x01, 0, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn a_record_numbers_its_fields_from_one_and_is_not_framed_itself() {
        let bytes = Record::new().field(b"a").field(b"").field(b"bc").finish();
        let mut expected = by_hand(0x01, b"a");
        expected.extend(by_hand(0x02, b""));
        expected.extend(by_hand(0x03, b"bc"));
        assert_eq!(bytes, expected);
        assert_eq!(Record::new().finish(), Vec::<u8>::new());
    }

    #[test]
    fn a_nested_record_is_the_payload_of_one_field() {
        let inner = Record::new().field(b"x").finish();
        let outer = Record::new().field(&inner).field(b"y").finish();
        let mut expected = by_hand(0x01, &by_hand(0x01, b"x"));
        expected.extend(by_hand(0x02, b"y"));
        assert_eq!(outer, expected);
        let fields = record(&outer, 2).unwrap();
        assert_eq!(record(fields[0], 1).unwrap(), [b"x".as_slice()]);
    }

    #[test]
    #[should_panic(expected = "at most 255 fields")]
    fn a_256th_field_is_a_defect() {
        let mut record = Record::new();
        for _ in 0..256 {
            record = record.field(b"");
        }
    }

    #[test]
    fn a_collection_is_its_count_then_each_item_with_its_length() {
        let items: [&[u8]; 3] = [b"ab", b"", b"c"];
        assert_eq!(
            collection(items),
            [
                0, 0, 0, 0, 0, 0, 0, 3, // count
                0, 0, 0, 0, 0, 0, 0, 2, b'a', b'b', //
                0, 0, 0, 0, 0, 0, 0, 0, //
                0, 0, 0, 0, 0, 0, 0, 1, b'c',
            ]
        );
        assert_eq!(collection([]), [0; 8]);
    }

    #[test]
    fn a_record_reads_back_as_its_payloads_in_tag_order() {
        let bytes = Record::new().field(b"a").field(b"").field(b"bc").finish();
        assert_eq!(record(&bytes, 3).unwrap(), [b"a".as_slice(), b"", b"bc"]);
        assert_eq!(record(&[], 0).unwrap(), Vec::<&[u8]>::new());
    }

    #[test]
    fn frames_keep_every_tag_and_payload_without_judging_them() {
        let mut bytes = by_hand(0x02, b"b");
        bytes.extend(by_hand(0x00, b""));
        assert_eq!(
            frames(&bytes).unwrap(),
            [(0x02, b"b".as_slice()), (0x00, b"".as_slice())]
        );
    }

    #[test]
    fn bytes_that_end_inside_a_frame_are_truncated() {
        let whole = by_hand(0x01, b"abc");
        // Inside the length, and inside the payload; a lone tag has no length.
        for end in [1, 5, 9, 11] {
            assert_eq!(
                record(&whole[..end], 1),
                Err(FrameFailure::Truncated),
                "{end}"
            );
        }
        // A length past the end, even one no target can address.
        let mut huge = vec![0x01];
        huge.extend_from_slice(&u64::MAX.to_be_bytes());
        assert_eq!(record(&huge, 1), Err(FrameFailure::Truncated));
    }

    #[test]
    fn each_tag_defect_has_its_own_failure() {
        let record_of = |tags: &[u8]| {
            tags.iter()
                .flat_map(|&tag| by_hand(tag, b"p"))
                .collect::<Vec<_>>()
        };
        let cases: [(&[u8], u8, FrameFailure); 7] = [
            (&[0x01, 0x00], 2, FrameFailure::ZeroTag),
            (&[0x01, 0x03], 2, FrameFailure::UnknownTag),
            (&[0x01, 0x01], 2, FrameFailure::RepeatedTag),
            (&[0x02, 0x01], 2, FrameFailure::OutOfOrder),
            (&[0x01, 0x03], 3, FrameFailure::MissingTag),
            (&[0x01], 2, FrameFailure::MissingTag),
            (&[], 1, FrameFailure::MissingTag),
        ];
        for (tags, fields, failure) in cases {
            assert_eq!(
                record(&record_of(tags), fields),
                Err(failure),
                "{tags:?} of {fields}"
            );
        }
    }

    #[test]
    fn the_first_frame_that_breaks_the_order_decides() {
        let record_of = |tags: &[u8]| {
            tags.iter()
                .flat_map(|&tag| by_hand(tag, b""))
                .collect::<Vec<_>>()
        };
        // A repeat that is also below its predecessor is a repeat.
        assert_eq!(
            record(&record_of(&[0x01, 0x02, 0x01]), 2),
            Err(FrameFailure::RepeatedTag)
        );
        // A tag beyond the record before a zero tag.
        assert_eq!(
            record(&record_of(&[0x09, 0x00]), 2),
            Err(FrameFailure::UnknownTag)
        );
        // Out of order before a later missing tag is noticed.
        assert_eq!(
            record(&record_of(&[0x03, 0x01]), 4),
            Err(FrameFailure::OutOfOrder)
        );
        // The last field is a tag like any other: 255 fields read back.
        let all = (1..=255).collect::<Vec<u8>>();
        assert_eq!(record(&record_of(&all), 255).unwrap().len(), 255);
    }

    #[test]
    fn a_collection_reads_back_as_its_items() {
        let three: [&[u8]; 3] = [b"ab", b"", b"c"];
        assert_eq!(items(&collection(three)).unwrap(), three);
        assert_eq!(items(&collection([])).unwrap(), Vec::<&[u8]>::new());
    }

    #[test]
    fn a_collection_with_another_number_of_items_than_its_count_fails() {
        let two: [&[u8]; 2] = [b"a", b"b"];
        let mut fewer = collection(two);
        fewer[7] = 3;
        assert_eq!(items(&fewer), Err(FrameFailure::CountMismatch));
        let mut more = collection(two);
        more[7] = 1;
        assert_eq!(items(&more), Err(FrameFailure::CountMismatch));
        let mut none = collection([]);
        none.extend_from_slice(&[0; 8]);
        assert_eq!(items(&none), Err(FrameFailure::CountMismatch));
        // Bytes after the counted items are a mismatch even when they are no
        // whole item.
        let mut trailing = collection([b"a".as_slice()]);
        trailing.extend_from_slice(&[1, 2, 3]);
        assert_eq!(items(&trailing), Err(FrameFailure::CountMismatch));
    }

    #[test]
    fn a_collection_that_ends_inside_its_count_or_an_item_is_truncated() {
        let bytes = collection([b"abc".as_slice()]);
        for end in [0, 7, 12, 17] {
            assert_eq!(items(&bytes[..end]), Err(FrameFailure::Truncated), "{end}");
        }
    }
}
