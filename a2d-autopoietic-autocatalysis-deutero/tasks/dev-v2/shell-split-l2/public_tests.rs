// R4,R5,R6,R7,R8,R17
#[test]
fn public_basic_words() {
    assert_eq!(split("alpha 'two words' x\"y\" ''", &[]), Ok(vec!["alpha".to_string(), "two words".to_string(), "xy".to_string(), "".to_string()]));
}

// R9,R11
#[test]
fn public_escapes() {
    assert_eq!(split("a\\ b \"c\\\"d\"", &[]), Ok(vec!["a b".to_string(), "c\"d".to_string()]));
}

// R20,R22,R25
#[test]
fn public_environment_and_default() {
    assert_eq!(split("$USER ${M:-guest} \"$HOME\"", &[("USER", "sam"), ("HOME", "/a b")]), Ok(vec!["sam".to_string(), "guest".to_string(), "/a b".to_string()]));
}

// R24
#[test]
fn public_expansion_error() {
    assert_eq!(split("${!}", &[]), Err(SplitError::InvalidExpansion { pos: 0 }));
}
