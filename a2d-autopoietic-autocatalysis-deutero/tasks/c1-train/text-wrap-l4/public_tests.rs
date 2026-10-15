// R1, R2, R15, R16
#[test]
fn public_ordinary() {
    assert_eq!(wrap("red blue", &WrapOptions { width: 12, first_indent: 0, rest_indent: 0, hyphen_cost: 0, justify: false }), Ok(vec!["red blue".to_string()]));
}

// R14, R15
#[test]
fn public_hanging() {
    assert_eq!(wrap("aa bb cc", &WrapOptions { width: 5, first_indent: 1, rest_indent: 1, hyphen_cost: 0, justify: false }), Ok(vec![" aa".to_string(), " bb".to_string(), " cc".to_string()]));
}

// R17, R27, R28
#[test]
fn public_hyphen_and_justify() {
    assert_eq!(wrap("ab~cdef", &WrapOptions { width: 4, first_indent: 0, rest_indent: 0, hyphen_cost: 0, justify: false }), Ok(vec!["ab-".to_string(), "cdef".to_string()]));
    assert_eq!(wrap("a bb ccc d", &WrapOptions { width: 7, first_indent: 0, rest_indent: 0, hyphen_cost: 0, justify: true }), Ok(vec!["a    bb".to_string(), "ccc d".to_string()]));
}

// R3
#[test]
fn public_validation() {
    assert_eq!(wrap("x", &WrapOptions { width: 0, first_indent: 0, rest_indent: 0, hyphen_cost: 0, justify: false }), Err(WrapError::WidthZero));
}
