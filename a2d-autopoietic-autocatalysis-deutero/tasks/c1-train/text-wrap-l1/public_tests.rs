// R1, R2, R7, R8
#[test]
fn public_packing() {
    assert_eq!(wrap("pack my red bag", 8), Ok(vec!["pack my".to_string(), "red bag".to_string()]));
}

// R5, R6
#[test]
fn public_collapse() {
    assert_eq!(wrap("  one\t two\nthree\r ", 10), Ok(vec!["one two".to_string(), "three".to_string()]));
}

// R10, R14
#[test]
fn public_long_word() {
    assert_eq!(wrap("abcdefghij", 4), Ok(vec!["abcd".to_string(), "efgh".to_string(), "ij".to_string()]));
}

// R3
#[test]
fn public_invalid_width() {
    assert_eq!(wrap("x", 0), Err(WrapError::WidthZero));
}
