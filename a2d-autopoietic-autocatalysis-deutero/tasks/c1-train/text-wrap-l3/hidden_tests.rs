// R2, R3, R4, R5
#[test]
fn hidden_config_precedence() {
    assert_eq!(wrap("~\u{0000}", &WrapOptions { width: 0, first_indent: 9, rest_indent: 9, hyphen_cost: 1000001, justify: false }), Err(WrapError::WidthZero));
    assert_eq!(wrap("~", &WrapOptions { width: 257, first_indent: 0, rest_indent: 0, hyphen_cost: 0, justify: false }), Err(WrapError::WidthTooLarge { width: 257 }));
    assert_eq!(wrap("~", &WrapOptions { width: 5, first_indent: 5, rest_indent: 5, hyphen_cost: 1000001, justify: false }), Err(WrapError::FirstIndentTooWide { indent: 5 }));
    assert_eq!(wrap("~", &WrapOptions { width: 5, first_indent: 0, rest_indent: 5, hyphen_cost: 1000001, justify: false }), Err(WrapError::RestIndentTooWide { indent: 5 }));
    assert_eq!(wrap("\u{0000}", &WrapOptions { width: 5, first_indent: 0, rest_indent: 0, hyphen_cost: 1000001, justify: false }), Err(WrapError::HyphenCostTooLarge { cost: 1000001 }));
}

// R6, R7
#[test]
fn hidden_control_beats_marker() {
    assert_eq!(wrap("~x é\u{0001}", &WrapOptions { width: 8, first_indent: 0, rest_indent: 0, hyphen_cost: 0, justify: false }), Err(WrapError::InvalidControl { index: 5 }));
}

// R7, R8
#[test]
fn hidden_marker_byte_offset() {
    assert_eq!(wrap("é\r\n猫~", &WrapOptions { width: 8, first_indent: 0, rest_indent: 0, hyphen_cost: 0, justify: false }), Err(WrapError::InvalidMarker { index: 7 }));
}

// R7
#[test]
fn hidden_consecutive_marker() {
    assert_eq!(wrap("ab~~cd", &WrapOptions { width: 8, first_indent: 0, rest_indent: 0, hyphen_cost: 0, justify: false }), Err(WrapError::InvalidMarker { index: 2 }));
}

// R7, R9
#[test]
fn hidden_marker_near_separator() {
    assert_eq!(wrap("x ~ab", &WrapOptions { width: 8, first_indent: 0, rest_indent: 0, hyphen_cost: 0, justify: false }), Err(WrapError::InvalidMarker { index: 2 }));
    assert_eq!(wrap("ab~ x", &WrapOptions { width: 8, first_indent: 0, rest_indent: 0, hyphen_cost: 0, justify: false }), Err(WrapError::InvalidMarker { index: 2 }));
}

// R10, R11
#[test]
fn hidden_empty_and_break_runs() {
    assert_eq!(wrap("", &WrapOptions { width: 5, first_indent: 0, rest_indent: 0, hyphen_cost: 0, justify: false }), Ok(vec![]));
    assert_eq!(wrap("\n\n\nx\r\n\r\n", &WrapOptions { width: 5, first_indent: 1, rest_indent: 2, hyphen_cost: 0, justify: false }), Ok(vec!["".to_string(), "".to_string(), " x".to_string(), "".to_string()]));
}

// R8, R9, R12, R24
#[test]
fn hidden_literal_unicode_separators() {
    assert_eq!(wrap("é́ ­ 猫", &WrapOptions { width: 8, first_indent: 0, rest_indent: 0, hyphen_cost: 0, justify: false }), Ok(vec!["é́ ­ 猫".to_string()]));
}

// R12, R14
#[test]
fn hidden_markers_removed_when_fitting() {
    assert_eq!(wrap("a~b~c d~e", &WrapOptions { width: 8, first_indent: 0, rest_indent: 0, hyphen_cost: 0, justify: false }), Ok(vec!["abc de".to_string()]));
}

// R15, R16, R17
#[test]
fn hidden_rightmost_opportunity() {
    assert_eq!(wrap("a~bc~def~gh", &WrapOptions { width: 5, first_indent: 0, rest_indent: 0, hyphen_cost: 0, justify: false }), Ok(vec!["abc-".to_string(), "defgh".to_string()]));
}

// R5, R17
#[test]
fn hidden_high_cost_on_empty() {
    assert_eq!(wrap("ab~cdefgh", &WrapOptions { width: 4, first_indent: 0, rest_indent: 0, hyphen_cost: 1000000, justify: false }), Ok(vec!["ab-".to_string(), "cdef".to_string(), "gh".to_string()]));
}

// R16, R18
#[test]
fn hidden_strict_cost_tie() {
    assert_eq!(wrap("aa b~cdef", &WrapOptions { width: 6, first_indent: 0, rest_indent: 0, hyphen_cost: 15, justify: false }), Ok(vec!["aa".to_string(), "bcdef".to_string()]));
}

// R15, R16
#[test]
fn hidden_one_below_tie() {
    assert_eq!(wrap("aa b~cdef", &WrapOptions { width: 6, first_indent: 0, rest_indent: 0, hyphen_cost: 14, justify: false }), Ok(vec!["aa b-".to_string(), "cdef".to_string()]));
}

// R16, R18
#[test]
fn hidden_one_above_tie() {
    assert_eq!(wrap("aa b~cdef", &WrapOptions { width: 6, first_indent: 0, rest_indent: 0, hyphen_cost: 16, justify: false }), Ok(vec!["aa".to_string(), "bcdef".to_string()]));
}

// R16
#[test]
fn hidden_baseline_no_separator() {
    assert_eq!(wrap("aa b~cdefg", &WrapOptions { width: 6, first_indent: 0, rest_indent: 0, hyphen_cost: 10, justify: false }), Ok(vec!["aa b-".to_string(), "cdefg".to_string()]));
}

// R18, R19
#[test]
fn hidden_no_occupied_raw_split() {
    assert_eq!(wrap("a abcdefghi", &WrapOptions { width: 5, first_indent: 0, rest_indent: 0, hyphen_cost: 0, justify: false }), Ok(vec!["a".to_string(), "abcde".to_string(), "fghi".to_string()]));
}

// R15, R19
#[test]
fn hidden_raw_passes_marker() {
    assert_eq!(wrap("abcd~efg", &WrapOptions { width: 4, first_indent: 0, rest_indent: 0, hyphen_cost: 0, justify: false }), Ok(vec!["abcd".to_string(), "efg".to_string()]));
}

// R19, R20
#[test]
fn hidden_literal_hyphen_not_break() {
    assert_eq!(wrap("ab-cdef", &WrapOptions { width: 4, first_indent: 0, rest_indent: 0, hyphen_cost: 0, justify: false }), Ok(vec!["ab-c".to_string(), "def".to_string()]));
}

// R15, R17, R19
#[test]
fn hidden_continuation_rechecks_markers() {
    assert_eq!(wrap("ab~cd~ef~ghij", &WrapOptions { width: 4, first_indent: 0, rest_indent: 0, hyphen_cost: 0, justify: false }), Ok(vec!["ab-".to_string(), "cd-".to_string(), "ef-".to_string(), "ghij".to_string()]));
}

// R13, R15, R17, R23
#[test]
fn hidden_indents_recalculate() {
    assert_eq!(wrap("ab~cd~efgh", &WrapOptions { width: 6, first_indent: 2, rest_indent: 3, hyphen_cost: 0, justify: false }), Ok(vec!["  ab-".to_string(), "   cd-".to_string(), "   efg".to_string(), "   h".to_string()]));
}

// R14, R20, R21
#[test]
fn hidden_tail_join_and_final_not_justify() {
    assert_eq!(wrap("abcde f", &WrapOptions { width: 4, first_indent: 0, rest_indent: 0, hyphen_cost: 0, justify: true }), Ok(vec!["abcd".to_string(), "e f".to_string()]));
}

// R21, R22
#[test]
fn hidden_justify_rotates() {
    assert_eq!(wrap("a b c d e f g h i j", &WrapOptions { width: 6, first_indent: 0, rest_indent: 0, hyphen_cost: 0, justify: true }), Ok(vec!["a  b c".to_string(), "d e  f".to_string(), "g  h i".to_string(), "j".to_string()]));
}

// R13, R21, R22, R23
#[test]
fn hidden_justify_with_indent() {
    assert_eq!(wrap("aa b c d e f gg", &WrapOptions { width: 8, first_indent: 1, rest_indent: 2, hyphen_cost: 0, justify: true }), Ok(vec![" aa  b c".to_string(), "  d e  f".to_string(), "  gg".to_string()]));
}

// R16, R21, R23
#[test]
fn hidden_justify_hyphen_gap() {
    assert_eq!(wrap("a bc~defgh", &WrapOptions { width: 6, first_indent: 0, rest_indent: 0, hyphen_cost: 0, justify: true }), Ok(vec!["a  bc-".to_string(), "defgh".to_string()]));
}

// R10, R11, R13, R22
#[test]
fn hidden_paragraph_resets_rotation() {
    assert_eq!(wrap("a b c d e f g\n\na b c d e f g", &WrapOptions { width: 6, first_indent: 0, rest_indent: 0, hyphen_cost: 0, justify: true }), Ok(vec!["a  b c".to_string(), "d e  f".to_string(), "g".to_string(), "".to_string(), "a  b c".to_string(), "d e  f".to_string(), "g".to_string()]));
}

// R15, R19, R20, R24
#[test]
fn hidden_one_column_no_hyphen() {
    assert_eq!(wrap("猫~犬鳥 x", &WrapOptions { width: 1, first_indent: 0, rest_indent: 0, hyphen_cost: 0, justify: true }), Ok(vec!["猫".to_string(), "犬".to_string(), "鳥".to_string(), "x".to_string()]));
}

// R20, R21, R24
#[test]
fn hidden_final_and_singletons_not_padded() {
    assert_eq!(wrap("abcdef xy", &WrapOptions { width: 5, first_indent: 0, rest_indent: 0, hyphen_cost: 0, justify: true }), Ok(vec!["abcde".to_string(), "f xy".to_string()]));
}

// R1, R2, R3, R5, R6
#[test]
fn hidden_api_max_and_controls() {
    let o = WrapOptions { width:256, first_indent:0, rest_indent:0, hyphen_cost:1000000, justify:false };
    assert_eq!(o.clone(), o);
    assert!(!format!("{:?}", o).is_empty());
    assert_eq!(wrap("ok", &o), Ok(vec!["ok".to_string()]));
    assert_eq!(wrap("😀", &WrapOptions { width: 4, first_indent: 0, rest_indent: 0, hyphen_cost: 0, justify: false }), Err(WrapError::InvalidControl { index: 4 }));
}
