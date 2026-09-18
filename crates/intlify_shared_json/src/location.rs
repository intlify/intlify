// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Byte-based source positions shared by configuration decoders.

/// One-based line and zero-based UTF-8 byte column, independent of a file path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourcePosition {
    pub line: usize,
    pub column: usize,
}

pub(crate) fn duplicate_member_position(
    source: &str,
    detected_line: usize,
    detected_column: usize,
) -> SourcePosition {
    let detected =
        byte_offset_for_line_column(source, detected_line, detected_column).unwrap_or(source.len());
    let opening = previous_unescaped_quote(source, detected)
        .and_then(|closing| closing.checked_sub(1))
        .and_then(|before_closing| previous_unescaped_quote(source, before_closing))
        .unwrap_or(detected.min(source.len()));
    let (line, column) = line_and_byte_column(source, opening);
    SourcePosition { line, column }
}

fn byte_offset_for_line_column(source: &str, line: usize, column: usize) -> Option<usize> {
    if line == 0 {
        return None;
    }

    let mut line_start = 0;
    for _ in 1..line {
        let newline = source.as_bytes()[line_start..]
            .iter()
            .position(|byte| *byte == b'\n')?;
        line_start += newline + 1;
    }
    Some(
        line_start
            .saturating_add(column.saturating_sub(1))
            .min(source.len()),
    )
}

fn previous_unescaped_quote(source: &str, before_or_at: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut index = before_or_at.min(bytes.len().checked_sub(1)?);

    loop {
        if bytes[index] == b'"' {
            let preceding_backslashes = bytes[..index]
                .iter()
                .rev()
                .take_while(|byte| **byte == b'\\')
                .count();
            if preceding_backslashes % 2 == 0 {
                return Some(index);
            }
        }
        index = index.checked_sub(1)?;
    }
}

fn line_and_byte_column(source: &str, byte_offset: usize) -> (usize, usize) {
    let prefix = &source.as_bytes()[..byte_offset.min(source.len())];
    let mut line = 1;
    let mut line_start = 0;
    for (index, byte) in prefix.iter().enumerate() {
        if *byte == b'\n' {
            line += 1;
            line_start = index + 1;
        }
    }
    (line, prefix.len() - line_start)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_count_bytes_not_characters() {
        assert_eq!(line_and_byte_column("日本🙂\nabc", 10), (1, 10));
        assert_eq!(line_and_byte_column("日本🙂\nabc", 11), (2, 0));
        assert_eq!(line_and_byte_column("日本🙂\nabc", usize::MAX), (2, 3));
        assert_eq!(line_and_byte_column("", 0), (1, 0));
    }

    #[test]
    fn serde_coordinates_preserve_crlf_and_clamp_eof() {
        let source = "{}\r\n{}";
        assert_eq!(byte_offset_for_line_column(source, 0, 1), None);
        assert_eq!(byte_offset_for_line_column(source, 1, 1), Some(0));
        assert_eq!(byte_offset_for_line_column(source, 2, 1), Some(4));
        assert_eq!(byte_offset_for_line_column(source, 2, usize::MAX), Some(6));
        assert_eq!(byte_offset_for_line_column(source, 3, 1), None);
    }
}
