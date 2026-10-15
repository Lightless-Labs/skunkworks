// R1-R3, R12-R15
#[test]
fn public_simple() {
    assert_eq!(expand("Hello {who}!{x,y}", &[("who",Value::Scalar("A B".into())),("x",Value::Scalar("a".into())),("y",Value::Scalar("b".into()))]), Ok("Hello A%20B!a,b".into()));
}

// R13-R15
#[test]
fn public_encoding() {
    assert_eq!(expand("{a,b,c}", &[("a",Value::Scalar("café/".into())),("b",Value::Scalar("".into())),("c",Value::Scalar("~".into()))]), Ok("caf%C3%A9%2F,,~".into()));
}

// R16-R24,R28-R35
#[test]
fn public_collections() {
    assert_eq!(expand("{/xs*}{?m*}{;s:2}", &[("xs",Value::List(vec!["a".into(),"b".into()])),("m",Value::Map(vec![("k".into(),"v".into())])),("s",Value::Scalar("abcd".into()))]), Ok("/a/b?k=v;s=ab".into()));
    assert_eq!(expand("{+s}{#s}{.xs}{&m}", &[("s",Value::Scalar("/ok".into())),("xs",Value::List(vec!["a".into(),"b".into()])),("m",Value::Map(vec![("k".into(),"v".into())]))]), Ok("/ok#/ok.a,b&m=k,v".into()));
}

// R4,R6
#[test]
fn public_unclosed() {
    assert_eq!(expand("a{x", &[]), Err(Error::Unclosed { pos: 1 }));
}
