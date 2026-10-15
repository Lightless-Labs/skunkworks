// R1-R4
#[test]
fn hidden_literal_identity() {
    assert_eq!(expand("é %xz\n /", &[]), Ok("é %xz\n /".into()));
    assert_eq!(expand("", &[]), Ok("".into()));
}

// R12,R15
#[test]
fn hidden_missing_fields() {
    assert_eq!(expand("{absent,x,other,y,tail}", &[("x","a"),("y","b")]), Ok("a,b".into()));
}

// R12
#[test]
fn hidden_last_binding() {
    assert_eq!(expand("{x}", &[("x","old"),("x","new")]), Ok("new".into()));
}

// R12,R15
#[test]
fn hidden_empty_last_binding() {
    assert_eq!(expand("{a,b,c}", &[("a","old"),("a",""),("b","B"),("c","")]), Ok(",B,".into()));
}

// R13-R14
#[test]
fn hidden_triplet_normalization() {
    assert_eq!(expand("{x}", &[("x","%2f%aB%41%25")]), Ok("%2F%AB%41%25".into()));
}

// R14
#[test]
fn hidden_broken_triplets() {
    assert_eq!(expand("{x}", &[("x","%xA|%2|%|%%2f")]), Ok("%25xA%7C%252%7C%25%7C%25%2F".into()));
}

// R13
#[test]
fn hidden_utf8_bytes() {
    assert_eq!(expand("{x}", &[("x","雪😀\u{0000}\n")]), Ok("%E9%9B%AA%F0%9F%98%80%00%0A".into()));
}

// R13
#[test]
fn hidden_unreserved_boundary() {
    assert_eq!(expand("{_A9}", &[("_A9","AZaz09-._~ !+/?")]), Ok("AZaz09-._~%20%21%2B%2F%3F".into()));
}

// R4-R6
#[test]
fn hidden_unexpected_close() {
    assert_eq!(expand("é}", &[]), Err(Error::UnexpectedClose { pos: 2 }));
}

// R6
#[test]
fn hidden_unclosed_nested() {
    assert_eq!(expand("xx{a{b", &[]), Err(Error::Unclosed { pos: 2 }));
}

// R7
#[test]
fn hidden_nested_precedence() {
    assert_eq!(expand("{?bad!{x}", &[]), Err(Error::NestedOpen { pos: 6 }));
}

// R9
#[test]
fn hidden_empty_expression() {
    assert_eq!(expand("p{}", &[]), Err(Error::EmptyExpression { pos: 2 }));
}

// R9
#[test]
fn hidden_empty_variable() {
    assert_eq!(expand("{,x}", &[]), Err(Error::EmptyVariable { pos: 1 }));
    assert_eq!(expand("{x,}", &[]), Err(Error::EmptyVariable { pos: 3 }));
    assert_eq!(expand("{x,,y}", &[]), Err(Error::EmptyVariable { pos: 3 }));
}

// R10
#[test]
fn hidden_invalid_names() {
    assert_eq!(expand("{1x}", &[]), Err(Error::InvalidName { pos: 1 }));
    assert_eq!(expand("{x-y}", &[]), Err(Error::InvalidName { pos: 2 }));
    assert_eq!(expand("{x,é}", &[]), Err(Error::InvalidName { pos: 3 }));
    assert_eq!(expand("{ x}", &[]), Err(Error::InvalidName { pos: 1 }));
}

// R11
#[test]
fn hidden_duplicate_specifier() {
    assert_eq!(expand("{x,y,x}", &[]), Err(Error::DuplicateVariable { pos: 5 }));
    assert_eq!(expand("{x}{x}", &[("x","a")]), Ok("aa".into()));
}

// R10,R12
#[test]
fn hidden_case_sensitive() {
    assert_eq!(expand("{x,X,_}", &[("x","a"),("X","B"),("_","c"),("bad!","unused")]), Ok("a,B,c".into()));
}

// R5,R11
#[test]
fn hidden_syntax_before_expansion() {
    assert_eq!(expand("{x}{y,}", &[("x","value")]), Err(Error::EmptyVariable { pos: 6 }));
    assert_eq!(expand("{a!,a}", &[]), Err(Error::InvalidName { pos: 2 }));
}

// R2-R3,R12,R15,R16
#[test]
fn hidden_large_and_stateless() {
    let text = "{x}/".repeat(200); assert_eq!(expand(&text, &[("x","hi there")]), Ok("hi%20there/".repeat(200)));
    assert_eq!(expand("{x}", &[]), Ok("".into()));
    let e = Error::Unclosed { pos: 7 }; assert_eq!(e.clone(), e);
}

// R10
#[test]
fn hidden_modifiers_forbidden() {
    assert_eq!(expand("{x*}", &[]), Err(Error::InvalidName { pos: 2 }));
    assert_eq!(expand("{x:2}", &[]), Err(Error::InvalidName { pos: 2 }));
}

// R8
#[test]
fn hidden_all_operators_rejected() {
    assert_eq!(expand("{+x}", &[]), Err(Error::UnsupportedOperator { pos: 1 }));
    assert_eq!(expand("{#x}", &[]), Err(Error::UnsupportedOperator { pos: 1 }));
    assert_eq!(expand("{.x}", &[]), Err(Error::UnsupportedOperator { pos: 1 }));
    assert_eq!(expand("{/x}", &[]), Err(Error::UnsupportedOperator { pos: 1 }));
    assert_eq!(expand("{;x}", &[]), Err(Error::UnsupportedOperator { pos: 1 }));
    assert_eq!(expand("{?x}", &[]), Err(Error::UnsupportedOperator { pos: 1 }));
    assert_eq!(expand("{&x}", &[]), Err(Error::UnsupportedOperator { pos: 1 }));
}

// R8-R9
#[test]
fn hidden_operator_beats_empty() {
    assert_eq!(expand("{#}", &[]), Err(Error::UnsupportedOperator { pos: 1 }));
}

// R5-R7
#[test]
fn hidden_error_order() {
    assert_eq!(expand("}{{x}", &[]), Err(Error::UnexpectedClose { pos: 0 }));
    assert_eq!(expand("{x!}{", &[]), Err(Error::InvalidName { pos: 2 }));
}

// R8,R10
#[test]
fn hidden_punctuation_not_operator() {
    assert_eq!(expand("{$x}", &[]), Err(Error::InvalidName { pos: 1 }));
    assert_eq!(expand("{=x}", &[]), Err(Error::InvalidName { pos: 1 }));
}

// R4,R10
#[test]
fn hidden_unicode_offset() {
    assert_eq!(expand("雪{x,😀}", &[]), Err(Error::InvalidName { pos: 6 }));
}

// R6-R7
#[test]
fn hidden_literal_braces_have_no_escape() {
    assert_eq!(expand("{{}}", &[]), Err(Error::NestedOpen { pos: 1 }));
}

// R15
#[test]
fn hidden_empty_scalar_fields() {
    assert_eq!(expand("{x,y}", &[("x",""),("y","")]), Ok(",".into()));
    assert_eq!(expand("{x}", &[("x","")]), Ok("".into()));
}

// R14
#[test]
fn hidden_percent_greedy() {
    assert_eq!(expand("{x}", &[("x","%252f%2f0")]), Ok("%252f%2F0".into()));
}

// R10,R12
#[test]
fn hidden_exact_name_matching() {
    assert_eq!(expand("{x,x1,xx}", &[("x1","1"),("xx","2"),("x ","bad")]), Ok("1,2".into()));
}
