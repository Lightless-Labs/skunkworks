// R3, R4
#[test]
fn hidden_configuration_order() {
    assert_eq!(wrap("\u{0000}", &WrapOptions { width: 0, first_indent: 4, rest_indent: 4 }), Err(WrapError::WidthZero));
    assert_eq!(wrap("\u{0000}", &WrapOptions { width: 257, first_indent: 300, rest_indent: 300 }), Err(WrapError::WidthTooLarge { width: 257 }));
    assert_eq!(wrap("\u{0000}", &WrapOptions { width: 4, first_indent: 4, rest_indent: 4 }), Err(WrapError::FirstIndentTooWide { indent: 4 }));
    assert_eq!(wrap("", &WrapOptions { width: 4, first_indent: 0, rest_indent: 5 }), Err(WrapError::RestIndentTooWide { indent: 5 }));
}

// R9, R10
#[test]
fn hidden_empty_segments() {
    assert_eq!(wrap("", &WrapOptions { width: 5, first_indent: 1, rest_indent: 2 }), Ok(vec![]));
    assert_eq!(wrap(" \t \n ", &WrapOptions { width: 5, first_indent: 1, rest_indent: 2 }), Ok(vec![]));
}

// R5
#[test]
fn hidden_control_byte_offset() {
    assert_eq!(wrap("é猫\u{0007}", &WrapOptions { width: 8, first_indent: 0, rest_indent: 0 }), Err(WrapError::InvalidControl { index: 5 }));
}

// R5, R6
#[test]
fn hidden_original_offset() {
    assert_eq!(wrap("é\r\n\r\n", &WrapOptions { width: 8, first_indent: 0, rest_indent: 0 }), Err(WrapError::InvalidControl { index: 6 }));
}

// R5
#[test]
fn hidden_first_control() {
    assert_eq!(wrap("😀\u{000b}\u{0001}", &WrapOptions { width: 8, first_indent: 0, rest_indent: 0 }), Err(WrapError::InvalidControl { index: 4 }));
}

// R5
#[test]
fn hidden_c1_control() {
    assert_eq!(wrap("a", &WrapOptions { width: 8, first_indent: 0, rest_indent: 0 }), Err(WrapError::InvalidControl { index: 1 }));
}

// R6, R8, R11
#[test]
fn hidden_crlf_boundary() {
    assert_eq!(wrap("a\r\n\r\nb", &WrapOptions { width: 5, first_indent: 1, rest_indent: 2 }), Ok(vec![" a".to_string(), "".to_string(), " b".to_string()]));
}

// R6, R8
#[test]
fn hidden_lone_cr_boundary() {
    assert_eq!(wrap("x\r\ry", &WrapOptions { width: 5, first_indent: 0, rest_indent: 0 }), Ok(vec!["x".to_string(), "".to_string(), "y".to_string()]));
}

// R8, R9
#[test]
fn hidden_three_newlines() {
    assert_eq!(wrap("x\n\n\ny", &WrapOptions { width: 5, first_indent: 0, rest_indent: 0 }), Ok(vec!["x".to_string(), "".to_string(), "".to_string(), "y".to_string()]));
}

// R8, R9
#[test]
fn hidden_edge_boundaries() {
    assert_eq!(wrap("\n\n\na\n\n", &WrapOptions { width: 5, first_indent: 1, rest_indent: 2 }), Ok(vec!["".to_string(), "".to_string(), " a".to_string(), "".to_string()]));
}

// R8, R9
#[test]
fn hidden_only_boundaries() {
    assert_eq!(wrap("\n\n\n\n", &WrapOptions { width: 2, first_indent: 1, rest_indent: 0 }), Ok(vec!["".to_string(), "".to_string(), "".to_string()]));
}

// R8, R10
#[test]
fn hidden_separated_lfs_soft() {
    assert_eq!(wrap("a\n \nb\n\t\nc", &WrapOptions { width: 10, first_indent: 0, rest_indent: 0 }), Ok(vec!["a b c".to_string()]));
}

// R7, R10
#[test]
fn hidden_nbsp_collapse() {
    assert_eq!(wrap("a  b c", &WrapOptions { width: 5, first_indent: 0, rest_indent: 0 }), Ok(vec!["a b c".to_string()]));
}

// R7, R14, R17
#[test]
fn hidden_emspace_is_content() {
    assert_eq!(wrap("a b­c", &WrapOptions { width: 3, first_indent: 0, rest_indent: 0 }), Ok(vec!["a b".to_string(), "­c".to_string()]));
}

// R7, R14, R17
#[test]
fn hidden_combining_scalars() {
    assert_eq!(wrap("éx 😀😀", &WrapOptions { width: 2, first_indent: 0, rest_indent: 0 }), Ok(vec!["é".to_string(), "x".to_string(), "😀😀".to_string()]));
}

// R7, R14
#[test]
fn hidden_unicode_chunks() {
    assert_eq!(wrap("猫犬鳥魚馬 牛", &WrapOptions { width: 3, first_indent: 0, rest_indent: 0 }), Ok(vec!["猫犬鳥".to_string(), "魚馬".to_string(), "牛".to_string()]));
}

// R11, R13, R14
#[test]
fn hidden_capacity_changes_smaller() {
    assert_eq!(wrap("ab cdefg", &WrapOptions { width: 6, first_indent: 0, rest_indent: 3 }), Ok(vec!["ab".to_string(), "   cde".to_string(), "   fg".to_string()]));
}

// R11, R14, R16
#[test]
fn hidden_capacity_changes_larger() {
    assert_eq!(wrap("abcdefgh x", &WrapOptions { width: 7, first_indent: 3, rest_indent: 0 }), Ok(vec!["   abcd".to_string(), "efgh x".to_string()]));
}

// R12, R15, R16
#[test]
fn hidden_unicode_seam() {
    assert_eq!(wrap("猫 犬鳥魚馬牛羊 x", &WrapOptions { width: 5, first_indent: 0, rest_indent: 0 }), Ok(vec!["猫 犬鳥魚".to_string(), "馬牛羊 x".to_string()]));
}

// R13, R15
#[test]
fn hidden_one_column_seam() {
    assert_eq!(wrap("abc defghij", &WrapOptions { width: 5, first_indent: 0, rest_indent: 0 }), Ok(vec!["abc".to_string(), "defgh".to_string(), "ij".to_string()]));
}

// R14, R16
#[test]
fn hidden_tail_can_join() {
    assert_eq!(wrap("abcdef x", &WrapOptions { width: 5, first_indent: 2, rest_indent: 0 }), Ok(vec!["  abc".to_string(), "def x".to_string()]));
}

// R9, R11, R16
#[test]
fn hidden_paragraph_indent_reset() {
    assert_eq!(wrap("abcdef\n\nghijkl", &WrapOptions { width: 5, first_indent: 1, rest_indent: 2 }), Ok(vec![" abcd".to_string(), "  ef".to_string(), "".to_string(), " ghij".to_string(), "  kl".to_string()]));
}

// R11, R14, R16
#[test]
fn hidden_capacity_one() {
    assert_eq!(wrap("猫犬 鳥", &WrapOptions { width: 3, first_indent: 2, rest_indent: 2 }), Ok(vec!["  猫".to_string(), "  犬".to_string(), "  鳥".to_string()]));
}

// R7, R10
#[test]
fn hidden_literal_tilde() {
    assert_eq!(wrap("ab~cd", &WrapOptions { width: 5, first_indent: 0, rest_indent: 0 }), Ok(vec!["ab~cd".to_string()]));
}

// R1, R2, R3, R17
#[test]
fn hidden_maximum_and_api() {
    let o = WrapOptions { width:256, first_indent:0, rest_indent:0 };
    assert_eq!(o.clone(), o);
    assert!(!format!("{:?}", o).is_empty());
    let s = "😀".repeat(512);
    assert_eq!(wrap(&s, &o), Ok(vec!["😀".repeat(256);2]));
}
