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

// R19-R26
#[test]
fn public_codec_ipv6() {
let u = parse_url("https://[::1]:8443/a%20b").unwrap(); assert!(u.ipv6); assert_eq!(u.host, "::1"); assert_eq!(u.path, "/a%20b");
assert_eq!(decode_component(Component::Path, "a%20b").unwrap(), "a b");
assert_eq!(encode_component(Component::Path, "a b/c"), "a%20b/c");
}

// R31-R40
#[test]
fn public_paths() {
assert_eq!(normalise_path("/a/./b/../c").unwrap(), "/a/c");
let u = resolve_url("http://example/a/b", "../c?q#f").unwrap(); assert_eq!(u.path, "/c"); assert_eq!(u.query.as_deref(), Some("q")); assert_eq!(u.fragment.as_deref(), Some("f"));
}
