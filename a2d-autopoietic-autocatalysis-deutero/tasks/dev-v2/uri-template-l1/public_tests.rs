// R1-R3, R12-R15
#[test]
fn public_simple() {
    assert_eq!(expand("Hello {who}!{x,y}", &[("who","A B"),("x","a"),("y","b")]), Ok("Hello A%20B!a,b".into()));
}

// R13-R15
#[test]
fn public_encoding() {
    assert_eq!(expand("{a,b,c}", &[("a","café/"),("b",""),("c","~")]), Ok("caf%C3%A9%2F,,~".into()));
}

// R12,R15
#[test]
fn public_missing() {
    assert_eq!(expand("a{none}b{v}", &[("v","yes")]), Ok("abyes".into()));
}

// R4,R6
#[test]
fn public_unclosed() {
    assert_eq!(expand("a{x", &[]), Err(Error::Unclosed { pos: 1 }));
}
