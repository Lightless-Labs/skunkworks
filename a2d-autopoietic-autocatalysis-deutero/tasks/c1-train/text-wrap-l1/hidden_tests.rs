// R5, R14
#[test]
fn hidden_empty_and_blank() {
    assert_eq!(wrap("", 3), Ok(vec![]));
    assert_eq!(wrap(" \t\r\n  ", 2), Ok(vec![]));
}

// R2, R3, R4
#[test]
fn hidden_width_precedence() {
    assert_eq!(wrap("\u{0000}", 0), Err(WrapError::WidthZero));
    assert_eq!(wrap("é", 257), Err(WrapError::WidthTooLarge { width: 257 }));
}

// R1, R2, R3, R16
#[test]
fn hidden_max_width() {
    let text = "q".repeat(256);
    assert_eq!(wrap(&text, 256), Ok(vec![text]));
    assert_eq!(WrapError::WidthZero.clone(), WrapError::WidthZero);
    assert!(!format!("{:?}", WrapError::WidthZero).is_empty());
}

// R4
#[test]
fn hidden_nonascii_byte() {
    assert_eq!(wrap("abé", 4), Err(WrapError::InvalidByte { index: 2, byte: 195 }));
}

// R4
#[test]
fn hidden_earliest_bad_byte() {
    assert_eq!(wrap("ok\u{000b}\u{0000}", 10), Err(WrapError::InvalidByte { index: 2, byte: 11 }));
}

// R4
#[test]
fn hidden_del() {
    assert_eq!(wrap("ab", 10), Err(WrapError::InvalidByte { index: 2, byte: 127 }));
}

// R4
#[test]
fn hidden_form_feed() {
    assert_eq!(wrap("x\u{000c} y", 4), Err(WrapError::InvalidByte { index: 1, byte: 12 }));
}

// R6, R16
#[test]
fn hidden_punctuation_literal() {
    assert_eq!(wrap("a-b c~d \\q", 20), Ok(vec!["a-b c~d \\q".to_string()]));
}

// R8, R14
#[test]
fn hidden_line_fits_exact() {
    assert_eq!(wrap("abc de fg", 6), Ok(vec!["abc de".to_string(), "fg".to_string()]));
}

// R8, R9
#[test]
fn hidden_whole_word_not_split() {
    assert_eq!(wrap("ab cdef", 5), Ok(vec!["ab".to_string(), "cdef".to_string()]));
}

// R11, R12, R15
#[test]
fn hidden_seam_two_columns() {
    assert_eq!(wrap("ab abcdefgh z", 5), Ok(vec!["ab ab".to_string(), "cdefg".to_string(), "h z".to_string()]));
}

// R11, R13
#[test]
fn hidden_seam_one_column_refused() {
    assert_eq!(wrap("abc abcdefg", 5), Ok(vec!["abc".to_string(), "abcde".to_string(), "fg".to_string()]));
}

// R11, R13
#[test]
fn hidden_seam_zero_columns() {
    assert_eq!(wrap("abcd abcdef", 5), Ok(vec!["abcd".to_string(), "abcde".to_string(), "f".to_string()]));
}

// R10, R14
#[test]
fn hidden_overlong_exact_chunks() {
    assert_eq!(wrap("abcdefghijkl", 4), Ok(vec!["abcd".to_string(), "efgh".to_string(), "ijkl".to_string()]));
}

// R10, R15
#[test]
fn hidden_tail_reused() {
    assert_eq!(wrap("abcdef x y", 4), Ok(vec!["abcd".to_string(), "ef x".to_string(), "y".to_string()]));
}

// R14, R15
#[test]
fn hidden_exact_tail_flush() {
    assert_eq!(wrap("abcdefgh x", 4), Ok(vec!["abcd".to_string(), "efgh".to_string(), "x".to_string()]));
}

// R8, R10, R13, R14
#[test]
fn hidden_width_one() {
    assert_eq!(wrap("abc d ef", 1), Ok(vec!["a".to_string(), "b".to_string(), "c".to_string(), "d".to_string(), "e".to_string(), "f".to_string()]));
}

// R9, R10, R13
#[test]
fn hidden_width_two_seam() {
    assert_eq!(wrap("a bcdef g", 2), Ok(vec!["a".to_string(), "bc".to_string(), "de".to_string(), "f".to_string(), "g".to_string()]));
}

// R9, R12, R15
#[test]
fn hidden_remaining_tail_short_rule() {
    assert_eq!(wrap("a abcdefgh x", 6), Ok(vec!["a abcd".to_string(), "efgh x".to_string()]));
}

// R7, R11, R12, R15
#[test]
fn hidden_several_seams() {
    assert_eq!(wrap("a bcdefgh ijklmnopq r", 5), Ok(vec!["a bcd".to_string(), "efgh".to_string(), "ijklm".to_string(), "nopq".to_string(), "r".to_string()]));
}

// R5
#[test]
fn hidden_all_separators() {
    assert_eq!(wrap("a\r\n\tb\t\r c\n", 8), Ok(vec!["a b c".to_string()]));
}

// R5, R16
#[test]
fn hidden_no_trailing_space() {
    assert_eq!(wrap("  xx   yy  zz  ", 4), Ok(vec!["xx".to_string(), "yy".to_string(), "zz".to_string()]));
}

// R4, R7
#[test]
fn hidden_late_validation() {
    assert_eq!(wrap("abcdef abcdef\u{0000}", 2), Err(WrapError::InvalidByte { index: 13, byte: 0 }));
}

// R7, R10, R14, R16
#[test]
fn hidden_long_stream() {
    let text = "z".repeat(2048);
    let lines = wrap(&text, 16).unwrap();
    assert_eq!(lines, vec!["z".repeat(16);128]);
}

// R5, R8, R16
#[test]
fn hidden_space_runs_do_not_make_words() {
    assert_eq!(wrap("a     b        c", 5), Ok(vec!["a b c".to_string()]));
}
