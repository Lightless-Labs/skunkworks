// R4,R5,R6,R7,R8,R17
#[test]
fn public_basic_words() {
    assert_eq!(split("alpha 'two words' x\"y\" ''", &[]), Ok(vec![Word { text: "alpha".to_string(), glob: vec![] }, Word { text: "two words".to_string(), glob: vec![] }, Word { text: "xy".to_string(), glob: vec![] }, Word { text: "".to_string(), glob: vec![] }]));
}

// R9,R11
#[test]
fn public_escapes() {
    assert_eq!(split("a\\ b \"c\\\"d\"", &[]), Ok(vec![Word { text: "a b".to_string(), glob: vec![] }, Word { text: "c\"d".to_string(), glob: vec![] }]));
}

// R20,R30,R31,R32
#[test]
fn public_globs_and_environment() {
    assert_eq!(split("a* \"?\" $X", &[("X", "hi")]), Ok(vec![Word { text: "a*".to_string(), glob: vec![1] }, Word { text: "?".to_string(), glob: vec![] }, Word { text: "hi".to_string(), glob: vec![] }]));
}

// R34
#[test]
fn public_command_rejected() {
    assert_eq!(split("`date`", &[]), Err(SplitError::CommandSubstitution { pos: 0 }));
}
