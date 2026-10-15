// R4,R5,R6,R7,R8,R17
#[test]
fn public_basic_words() {
    assert_eq!(split("alpha 'two words' x\"y\" ''"), Ok(vec!["alpha".to_string(), "two words".to_string(), "xy".to_string(), "".to_string()]));
}

// R9,R11
#[test]
fn public_escapes() {
    assert_eq!(split("a\\ b \"c\\\"d\""), Ok(vec!["a b".to_string(), "c\"d".to_string()]));
}

// R13
#[test]
fn public_single_error() {
    assert_eq!(split("foo 'bar"), Err(SplitError::UnterminatedSingle { pos: 4 }));
}

// R15,R18,R19
#[test]
fn public_literal_symbols() {
    assert_eq!(split("# $A *"), Ok(vec!["#".to_string(), "$A".to_string(), "*".to_string()]));
}
