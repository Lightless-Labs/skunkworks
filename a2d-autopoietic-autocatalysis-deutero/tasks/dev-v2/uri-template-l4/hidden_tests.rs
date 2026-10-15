// R1-R4
#[test]
fn hidden_literal_identity() {
    assert_eq!(expand("é %xz\n /", &[]), Ok("é %xz\n /".into()));
    assert_eq!(expand("", &[]), Ok("".into()));
}

// R12,R15
#[test]
fn hidden_missing_fields() {
    assert_eq!(expand("{absent,x,other,y,tail}", &[("x",Value::Scalar("a".into())),("y",Value::Scalar("b".into()))]), Ok("a,b".into()));
}

// R12
#[test]
fn hidden_last_binding() {
    assert_eq!(expand("{x}", &[("x",Value::Scalar("old".into())),("x",Value::Scalar("new".into()))]), Ok("new".into()));
}

// R12,R15
#[test]
fn hidden_empty_last_binding() {
    assert_eq!(expand("{a,b,c}", &[("a",Value::Scalar("old".into())),("a",Value::Scalar("".into())),("b",Value::Scalar("B".into())),("c",Value::Scalar("".into()))]), Ok(",B,".into()));
}

// R13-R14
#[test]
fn hidden_triplet_normalization() {
    assert_eq!(expand("{x}", &[("x",Value::Scalar("%2f%aB%41%25".into()))]), Ok("%2F%AB%41%25".into()));
}

// R14
#[test]
fn hidden_broken_triplets() {
    assert_eq!(expand("{x}", &[("x",Value::Scalar("%xA|%2|%|%%2f".into()))]), Ok("%25xA%7C%252%7C%25%7C%25%2F".into()));
}

// R13
#[test]
fn hidden_utf8_bytes() {
    assert_eq!(expand("{x}", &[("x",Value::Scalar("雪😀\u{0000}\n".into()))]), Ok("%E9%9B%AA%F0%9F%98%80%00%0A".into()));
}

// R13
#[test]
fn hidden_unreserved_boundary() {
    assert_eq!(expand("{_A9}", &[("_A9",Value::Scalar("AZaz09-._~ !+/?".into()))]), Ok("AZaz09-._~%20%21%2B%2F%3F".into()));
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
    assert_eq!(expand("{x}{x}", &[("x",Value::Scalar("a".into()))]), Ok("aa".into()));
}

// R10,R12
#[test]
fn hidden_case_sensitive() {
    assert_eq!(expand("{x,X,_}", &[("x",Value::Scalar("a".into())),("X",Value::Scalar("B".into())),("_",Value::Scalar("c".into())),("bad!",Value::Scalar("unused".into()))]), Ok("a,B,c".into()));
}

// R5,R11
#[test]
fn hidden_syntax_before_expansion() {
    assert_eq!(expand("{x}{y,}", &[("x",Value::Scalar("value".into()))]), Err(Error::EmptyVariable { pos: 6 }));
    assert_eq!(expand("{a!,a}", &[]), Err(Error::InvalidName { pos: 2 }));
}

// R2-R3,R12,R15,R38
#[test]
fn hidden_large_and_stateless() {
    let text = "{x}/".repeat(200); assert_eq!(expand(&text, &[("x",Value::Scalar("hi there".into()))]), Ok("hi%20there/".repeat(200)));
    assert_eq!(expand("{x}", &[]), Ok("".into()));
    let e = Error::Unclosed { pos: 7 }; assert_eq!(e.clone(), e);
}

// R13-R17
#[test]
fn hidden_reserved_set() {
    assert_eq!(expand("{+x}", &[("x",Value::Scalar(":/?#[]@!$&'()*+,;= %".into()))]), Ok(":/?#[]@!$&'()*+,;=%20%25".into()));
}

// R12,R15,R17
#[test]
fn hidden_fragment_empty_missing() {
    assert_eq!(expand("{#a,b}{#missing}{#b}", &[("a",Value::Scalar("".into())),("b",Value::Scalar("B".into()))]), Ok("#,B#B".into()));
    assert_eq!(expand("{#a}", &[("a",Value::Scalar("".into()))]), Ok("#".into()));
}

// R14,R17
#[test]
fn hidden_adjacent_reserved() {
    assert_eq!(expand("{+x}{#x}{x}", &[("x",Value::Scalar("/%2f".into()))]), Ok("/%2F#/%2F%2F%2F".into()));
}

// R8-R9
#[test]
fn hidden_operator_empty() {
    assert_eq!(expand("{+}", &[]), Err(Error::EmptyExpression { pos: 2 }));
    assert_eq!(expand("{##x}", &[]), Err(Error::InvalidName { pos: 2 }));
}

// R18
#[test]
fn hidden_label_path_empty() {
    assert_eq!(expand("{.a,b,c}{/a,b,c}", &[("a",Value::Scalar("".into())),("b",Value::Scalar("/".into())),("c",Value::Scalar("".into()))]), Ok("..%2F.//%2F/".into()));
}

// R19
#[test]
fn hidden_parameter_empties() {
    assert_eq!(expand("{;a,b,c,d}", &[("a",Value::Scalar("".into())),("c",Value::Scalar("=".into())),("d",Value::Scalar("".into()))]), Ok(";a;c=%3D;d".into()));
}

// R20
#[test]
fn hidden_query_context() {
    assert_eq!(expand("p{?x}{?x}{&y,z}", &[("x",Value::Scalar("".into())),("z",Value::Scalar("a&b".into()))]), Ok("p?x=?x=&z=a%26b".into()));
}

// R21-R25
#[test]
fn hidden_prefix_rules() {
    assert_eq!(expand("{x:2}|{x:3}|{y:1}|{z*}", &[("x",Value::Scalar("%2fX".into())),("y",Value::Scalar("😀a".into())),("z",Value::Scalar("ab".into()))]), Ok("%252|%2F|%F0%9F%98%80|ab".into()));
    assert_eq!(expand("{missing:9999}{x:9999}", &[("x",Value::Scalar("é".into()))]), Ok("%C3%A9".into()));
}

// R10-R11,R21-R22
#[test]
fn hidden_modifier_errors() {
    assert_eq!(expand("{x:0}", &[]), Err(Error::InvalidModifier { pos: 2 }));
    assert_eq!(expand("{x:01}", &[]), Err(Error::InvalidModifier { pos: 2 }));
    assert_eq!(expand("{x:10000}", &[]), Err(Error::InvalidModifier { pos: 2 }));
    assert_eq!(expand("{x:}", &[]), Err(Error::InvalidModifier { pos: 2 }));
    assert_eq!(expand("{x:-1}", &[]), Err(Error::InvalidModifier { pos: 2 }));
    assert_eq!(expand("{x:2*}", &[]), Err(Error::InvalidModifier { pos: 2 }));
    assert_eq!(expand("{x**}", &[]), Err(Error::InvalidModifier { pos: 2 }));
    assert_eq!(expand("{x*tail}", &[]), Err(Error::InvalidModifier { pos: 2 }));
    assert_eq!(expand("{x:a}", &[]), Err(Error::InvalidModifier { pos: 2 }));
    assert_eq!(expand("{*:1}", &[]), Err(Error::InvalidName { pos: 1 }));
    assert_eq!(expand("{x!:0}", &[]), Err(Error::InvalidName { pos: 2 }));
    assert_eq!(expand("{x,x:0}", &[]), Err(Error::InvalidModifier { pos: 4 }));
}

// R5,R26-R27,R31
#[test]
fn hidden_value_error_precedence() {
    assert_eq!(expand("{x:1}{y,}", &[("x",Value::List(vec![]))]), Err(Error::EmptyVariable { pos: 8 }));
    assert_eq!(expand("{x:1}", &[("x",Value::Map(vec![("k".into(),"a".into()),("k".into(),"b".into())]))]), Err(Error::TypeMismatch { pos: 1 }));
    assert_eq!(expand("{m,x:2}", &[("m",Value::Map(vec![("k".into(),"a".into()),("k".into(),"b".into())])),("x",Value::List(vec![]))]), Err(Error::DuplicateKey { pos: 1 }));
    assert_eq!(expand("{x:1}", &[("x",Value::List(vec![])),("x",Value::Scalar("ab".into()))]), Ok("a".into()));
}

// R28-R30
#[test]
fn hidden_list_operator_matrix() {
    assert_eq!(expand("{xs}|{+xs}|{#xs}|{.xs*}|{/xs*}|{;xs*}|{?xs*}|{&xs}", &[("xs",Value::List(vec!["".into(),"a/b".into(),"".into()]))]), Ok(",a%2Fb,|,a/b,|#,a/b,|..a%2Fb.|//a%2Fb/|;xs;xs=a%2Fb;xs|?xs=&xs=a%2Fb&xs=|&xs=,a%2Fb,".into()));
    assert_eq!(expand("{;xs}{?ys}", &[("xs",Value::List(vec!["".into()])),("ys",Value::List(vec!["".into()]))]), Ok(";xs?ys=".into()));
}

// R31-R35
#[test]
fn hidden_map_operator_matrix() {
    assert_eq!(expand("{m}|{+m*}|{#m*}|{.m*}|{/m*}|{;m*}|{?m*}|{&m}", &[("m",Value::Map(vec![("b".into(),"/".into()),("a".into(),"".into())]))]), Ok("b,%2F,a,|b=/,a=|#b=/,a=|.b=%2F.a=|/b=%2F/a=|;b=%2F;a|?b=%2F&a=|&m=b,%2F,a,".into()));
    assert_eq!(expand("{;m}{?m}", &[("m",Value::Map(vec![("x".into(),"v".into())]))]), Ok(";m=x,v?m=x,v".into()));
}

// R12,R26-R27,R31
#[test]
fn hidden_map_validation_and_shadow() {
    assert_eq!(expand("é{?m*}", &[("m",Value::Map(vec![("a".into(),"1".into()),("a".into(),"2".into())]))]), Err(Error::DuplicateKey { pos: 4 }));
    assert_eq!(expand("{x}", &[("m",Value::Map(vec![("a".into(),"1".into()),("a".into(),"2".into())])),("x",Value::Scalar("ok".into()))]), Ok("ok".into()));
    assert_eq!(expand("{m*}", &[("m",Value::Map(vec![("a".into(),"1".into()),("a".into(),"2".into())])),("m",Value::Map(vec![("A".into(),"1".into()),("a".into(),"2".into())]))]), Ok("A=1,a=2".into()));
}

// R28,R32,R34-R38
#[test]
fn hidden_empty_collections_and_keys() {
    assert_eq!(expand("{#xs}{.m}{/xs*}{;m*}{?xs}{&m}", &[("xs",Value::List(vec![])),("m",Value::Map(vec![]))]), Ok("".into()));
    assert_eq!(expand("{;m*}|{?m*}|{#m*}", &[("m",Value::Map(vec![("".into(),"".into()),("z".into(),"v".into())]))]), Ok(";;z=v|?=&z=v|#=,z=v".into()));
    assert_eq!(expand("{#xs,s}{/m,s}", &[("xs",Value::List(vec![])),("m",Value::Map(vec![])),("s",Value::Scalar("".into()))]), Ok("#/".into()));
}

// R14,R29-R30,R33,R36
#[test]
fn hidden_collection_reserved_delimiters() {
    assert_eq!(expand("{+m}|{#xs*}|{?xs*}", &[("m",Value::Map(vec![("a,b".into(),"x=y".into())])),("xs",Value::List(vec!["a,b".into(),"x=y".into()]))]), Ok("a,b,x=y|#a,b,x=y|?xs=a%2Cb&xs=x%3Dy".into()));
}

// R11,R21,R23-R26
#[test]
fn hidden_scalar_modifiers_all_operators() {
    assert_eq!(expand("{x*}|{+x*}|{#x*}|{.x*}|{/x*}|{;x*}|{?x*}|{&x*}", &[("x",Value::Scalar("/".into()))]), Ok("%2F|/|#/|.%2F|/%2F|;x=%2F|?x=%2F|&x=%2F".into()));
    assert_eq!(expand("{+s:3}|{#s:2}|{.s:1}|{/s:2}|{?s:2}|{&s:1}", &[("s",Value::Scalar("a/雪x".into()))]), Ok("a/%E9%9B%AA|#a/|.a|/a%2F|?s=a%2F|&s=a".into()));
    assert_eq!(expand("{x,x*}", &[]), Err(Error::DuplicateVariable { pos: 3 }));
    assert_eq!(expand("{m:2}", &[("m",Value::Map(vec![]))]), Err(Error::TypeMismatch { pos: 1 }));
    assert_eq!(expand("{l:1}", &[("l",Value::List(vec![]))]), Err(Error::TypeMismatch { pos: 1 }));
}

// R18-R20,R28-R35
#[test]
fn hidden_interleaved_collection_fields() {
    assert_eq!(expand("{/x,ls*,m*,z}|{;x,ls*,m*,z}|{?x,ls*,m*,z}", &[("x",Value::Scalar("H".into())),("ls",Value::List(vec!["a".into(),"".into()])),("m",Value::Map(vec![("k".into(),"v".into()),("n".into(),"".into())])),("z",Value::Scalar("".into()))]), Ok("/H/a//k=v/n=/|;x=H;ls=a;ls;k=v;n;z|?x=H&ls=a&ls=&k=v&n=&z=".into()));
    assert_eq!(expand("{.ls,m}|{/ls,m}", &[("ls",Value::List(vec!["a".into(),"b".into()])),("m",Value::Map(vec![("k".into(),"v".into())]))]), Ok(".a,b.k,v|/a,b/k,v".into()));
    assert_eq!(expand("{m*}", &[("m",Value::Map(vec![("%41".into(),"1".into()),("A".into(),"2".into())]))]), Ok("%41=1,A=2".into()));
}
