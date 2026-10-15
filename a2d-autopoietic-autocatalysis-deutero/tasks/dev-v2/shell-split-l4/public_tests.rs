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

// R20,R30,R31,R34
#[test]
fn public_glob_command_expansion() {
    assert_eq!(split("a* $X", &[("X", "ok")]), Ok(vec![Word { text: "a*".to_string(), glob: vec![1] }, Word { text: "ok".to_string(), glob: vec![] }]));assert_eq!(split("`x`", &[]), Err(SplitError::CommandSubstitution { pos: 0 }));
}

// R37,R38,R39,R40,R45,R46
#[test]
fn public_join() {
    assert_eq!(join(&vec!["alpha".to_string(), "two words".to_string()]), Ok("alpha two\\ words".to_string()));assert_eq!(split("alpha two\\ words", &[]), Ok(vec![Word { text: "alpha".to_string(), glob: vec![] }, Word { text: "two words".to_string(), glob: vec![] }]));assert_eq!(join(&vec!["\0".into()]), Err(JoinError::Nul { word: 0, pos: 0 }));
}
