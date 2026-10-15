// R1, R2, R14
#[test]
fn public_plain_words() {
    assert_eq!(wrap("blue green", &WrapOptions { width: 10, first_indent: 0, rest_indent: 0, hyphen_cost: 0, justify: false }), Ok(vec!["blue green".to_string()]));
}

// R15, R17
#[test]
fn public_marked_break() {
    assert_eq!(wrap("ab~cdef", &WrapOptions { width: 4, first_indent: 0, rest_indent: 0, hyphen_cost: 0, justify: false }), Ok(vec!["ab-".to_string(), "cdef".to_string()]));
}

// R21, R22
#[test]
fn public_justify_basic() {
    assert_eq!(wrap("a bb ccc d", &WrapOptions { width: 7, first_indent: 0, rest_indent: 0, hyphen_cost: 0, justify: true }), Ok(vec!["a    bb".to_string(), "ccc d".to_string()]));
}

// R7
#[test]
fn public_marker_error() {
    assert_eq!(wrap("~a", &WrapOptions { width: 5, first_indent: 0, rest_indent: 0, hyphen_cost: 0, justify: false }), Err(WrapError::InvalidMarker { index: 0 }));
}
