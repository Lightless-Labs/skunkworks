// R2, R5, R9, R14, R18
#[test]
fn public_basic() {
let u = parse_url("HTTP://Example.COM").unwrap();
assert_eq!(u.scheme, "http"); assert_eq!(u.host, "example.com");
assert_eq!(u.path, "/"); assert_eq!(u.userinfo, None); assert_eq!(u.port, None);
assert_eq!(u.query, None); assert_eq!(u.fragment, None);
}

// R7-R15, R17
#[test]
fn public_components() {
let u = parse_url("demo://ann:pw@BOX:9000/a/b?q=one#top").unwrap();
assert_eq!(u.userinfo.as_deref(), Some("ann:pw")); assert_eq!(u.host, "box");
assert_eq!(u.port, Some(9000)); assert_eq!(u.path, "/a/b");
assert_eq!(u.query.as_deref(), Some("q=one")); assert_eq!(u.fragment.as_deref(), Some("top"));
}

// R6, R11
#[test]
fn public_error() {
assert_eq!(parse_url("app://server:70000"), Err(ParseError::InvalidPort { at: 13 }));
assert_eq!(parse_url("http:thing"), Err(ParseError::MissingAuthority { at: 5 }));
}
