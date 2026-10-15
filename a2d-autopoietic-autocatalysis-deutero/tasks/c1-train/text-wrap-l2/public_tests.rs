// R1, R2, R7, R12
#[test]
fn public_unicode_words() {
    assert_eq!(wrap("猫犬 鳥魚", &WrapOptions { width: 5, first_indent: 0, rest_indent: 0 }), Ok(vec!["猫犬 鳥魚".to_string()]));
}

// R11, R12, R13
#[test]
fn public_hanging() {
    assert_eq!(wrap("a bb ccc dd", &WrapOptions { width: 7, first_indent: 1, rest_indent: 2 }), Ok(vec![" a bb".to_string(), "  ccc".to_string(), "  dd".to_string()]));
}

// R8, R9, R11
#[test]
fn public_paragraph() {
    assert_eq!(wrap("a\n\nb", &WrapOptions { width: 4, first_indent: 0, rest_indent: 1 }), Ok(vec!["a".to_string(), "".to_string(), "b".to_string()]));
}

// R4
#[test]
fn public_bad_indent() {
    assert_eq!(wrap("x", &WrapOptions { width: 3, first_indent: 3, rest_indent: 0 }), Err(WrapError::FirstIndentTooWide { indent: 3 }));
}
