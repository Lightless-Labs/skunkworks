// R2, R5, R9, R14, R18
#[test]
fn public_basic() {
let u = parse_url("HTTP://Example.COM").unwrap();
assert_eq!(u.scheme, "http"); assert_eq!(u.host, "example.com");
assert_eq!(u.path, "/"); assert_eq!(u.userinfo, None); assert_eq!(u.port, None);
assert_eq!(u.query, None); assert_eq!(u.fragment, None);
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

// R41-R52
#[test]
fn public_query_canonical() {
let q = parse_query("x=one+two&x=three&z=").unwrap();
assert_eq!(q.entries, vec![("x".into(), vec!["one two".into(), "three".into()]), ("z".into(), vec!["".into()])]);
assert_eq!(serialise_query(&q), "x=one+two&x=three&z=");
assert_eq!(canonical_url("HTTP://Example:80/a/./b?x=one+two#top").unwrap(), "http://example/a/b?x=one+two#top");
}
