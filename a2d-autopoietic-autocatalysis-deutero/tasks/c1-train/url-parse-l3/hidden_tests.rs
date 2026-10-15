// R1, R2, R14, R18
#[test]
fn hidden_defaults() {
let u = parse_url("Z://H").unwrap();
assert_eq!((u.scheme.as_str(), u.host.as_str(), u.path.as_str()), ("z", "h", "/"));
assert_eq!((u.userinfo, u.port, u.query, u.fragment), (None, None, None, None));
}

// R3, R4
#[test]
fn hidden_global_priority() {
assert_eq!(parse_url(""), Err(ParseError::Empty));
for s in ["1://h/a b", "x://@h/\t", "x://h/é", "x://h/\u{7f}"] {
let at = s.bytes().position(|b| !(0x21..=0x7e).contains(&b)).unwrap();
assert_eq!(parse_url(s), Err(ParseError::InvalidByte { at })); }
}

// R5
#[test]
fn hidden_scheme_missing() {
for s in ["no-scheme", "abc/?q#z"] { assert_eq!(parse_url(s), Err(ParseError::InvalidScheme { at: s.len() })); }
assert_eq!(parse_url("://h"), Err(ParseError::InvalidScheme { at: 0 }));
}

// R3, R5
#[test]
fn hidden_scheme_invalid() {
for s in ["7x://h", "+x://h", "-x://h"] { assert_eq!(parse_url(s), Err(ParseError::InvalidScheme { at: 0 })); }
assert_eq!(parse_url("a_b://h"), Err(ParseError::InvalidScheme { at: 1 }));
assert_eq!(parse_url("ab/c://h"), Err(ParseError::InvalidScheme { at: 2 }));
}

// R5, R17
#[test]
fn hidden_scheme_valid() {
let u = parse_url("A1+X.y-Z://HOST").unwrap(); assert_eq!(u.scheme, "a1+x.y-z");
assert_eq!(u.host, "host");
}

// R6, R9
#[test]
fn hidden_marker() {
for s in ["q:", "q:/", "q:/h", "q:////h"] {
let expected = if s == "q:////h" { ParseError::InvalidHost { at: 4 } } else { ParseError::MissingAuthority { at: 2 } };
assert_eq!(parse_url(s), Err(expected)); }
}

// R7, R12-R15
#[test]
fn hidden_authority_boundaries() {
let u = parse_url("t://H?u@v:9#x/y").unwrap();
assert_eq!(u.host, "h"); assert_eq!(u.userinfo, None); assert_eq!(u.port, None);
assert_eq!(u.path, "/"); assert_eq!(u.query.as_deref(), Some("u@v:9")); assert_eq!(u.fragment.as_deref(), Some("x/y"));
assert_eq!(parse_url("t://H/x@y:3").unwrap().path, "/x@y:3");
}

// R3, R8
#[test]
fn hidden_userinfo_errors() {
assert_eq!(parse_url("a://@h"), Err(ParseError::InvalidUserInfo { at: 4 }));
assert_eq!(parse_url("a://u@v@!") , Err(ParseError::InvalidUserInfo { at: 7 }));
assert_eq!(parse_url("a://u@@@h"), Err(ParseError::InvalidUserInfo { at: 6 }));
}

// R8, R17
#[test]
fn hidden_userinfo_preserved() {
let u = parse_url("s://A:B:C@X_y./UP").unwrap();
assert_eq!(u.userinfo.as_deref(), Some("A:B:C")); assert_eq!(u.host, "x_y."); assert_eq!(u.path, "/UP");
}

// R9
#[test]
fn hidden_empty_hosts() {
for (s, at) in [("v://", 4), ("v://?x", 4), ("v://#z", 4), ("v://u@", 6), ("v://:2", 4)] {
assert_eq!(parse_url(s), Err(ParseError::InvalidHost { at })); }
}

// R9; R10; R11; R11, R17; R12, R13; R12-R14, R18; R14, R17; R7, R12-R15, R17
#[test]
fn hidden_host_rules() {
for s in ["s://.A..", "s://-A-", "s://___"] { assert!(parse_url(s).is_ok()); }
assert_eq!(parse_url("s://ab!c:xx"), Err(ParseError::InvalidHost { at: 6 }));
assert_eq!(parse_url("s://a%41"), Err(ParseError::InvalidHost { at: 5 }));
assert_eq!(parse_url("u://h:"), Err(ParseError::InvalidPort { at: 6 }));
assert_eq!(parse_url("u://h:?p"), Err(ParseError::InvalidPort { at: 6 }));
for (s, at) in [("u://h:0x", 7), ("u://h:00x", 8), ("u://h:2:3", 7), ("u://h:+5", 6)] {
assert_eq!(parse_url(s), Err(ParseError::InvalidPort { at })); }
assert_eq!(parse_url("u://h:1").unwrap().port, Some(1));
assert_eq!(parse_url("u://h:65535").unwrap().port, Some(65535));
for p in ["0", "00", "01", "65536", "999999999999999999999999999999999999999"] {
assert_eq!(parse_url(&format!("u://h:{p}")), Err(ParseError::InvalidPort { at: 6 })); }
assert_eq!(parse_url("http://h:80").unwrap().port, Some(80));
let u = parse_url("f://h/#one?two#three").unwrap();
assert_eq!(u.query, None); assert_eq!(u.fragment.as_deref(), Some("one?two#three"));
assert_eq!(parse_url("f://h/?a?b#c").unwrap().query.as_deref(), Some("a?b"));
let a = parse_url("n://h?#").unwrap();
assert_eq!(a.path, "/"); assert_eq!(a.query, Some(String::new())); assert_eq!(a.fragment, Some(String::new()));
assert_ne!(a, parse_url("n://h/").unwrap());
let u = parse_url("x://h/A//./b/../c/").unwrap(); assert_eq!(u.path, "/A//./b/../c/");
let u = parse_url("t://h/a:b@c?q=@:x?/r#f#?@").unwrap();
assert_eq!(u.path, "/a:b@c"); assert_eq!(u.query.as_deref(), Some("q=@:x?/r")); assert_eq!(u.fragment.as_deref(), Some("f#?@"));
}

// R19, R20, R17
#[test]
fn hidden_ipv6_storage() {
let u = parse_url("V://u@[2001:0DB8:0000::A]:65535/p").unwrap();
assert!(u.ipv6); assert_eq!(u.host, "2001:0db8:0000::a"); assert_eq!(u.port, Some(65535));
assert!(!parse_url("v://ordinary").unwrap().ipv6);
assert_eq!(parse_url("v://[::ffff:192.0.2.1]").unwrap().host, "::ffff:192.0.2.1");
}

// R20, R21
#[test]
fn hidden_ipv6_invalid() {
for s in ["v://[::", "v://[1:2:3]", "v://[::gg]", "v://[fe80::1%25eth0]", "v://[bad]X:0"] {
assert_eq!(parse_url(s), Err(ParseError::InvalidIpv6 { at: 4 })); }
assert_eq!(parse_url("v://[::1]X"), Err(ParseError::InvalidHost { at: 9 }));
assert_eq!(parse_url("v://h]"), Err(ParseError::InvalidHost { at: 5 }));
assert_eq!(parse_url("v://2001:db8::1"), Err(ParseError::InvalidPort { at: 9 }));
}

// R10, R11, R21
#[test]
fn hidden_ipv6_port() {
assert_eq!(parse_url("v://[::1]:"), Err(ParseError::InvalidPort { at: 10 }));
assert_eq!(parse_url("v://[::1]:01"), Err(ParseError::InvalidPort { at: 10 }));
assert_eq!(parse_url("v://[::1]:0z"), Err(ParseError::InvalidPort { at: 11 }));
}

// R22, R24
#[test]
fn hidden_decode_literals() {
for c in [Component::UserInfo, Component::Path, Component::Query, Component::Fragment] {
assert_eq!(decode_component(c, "a+b%2b%00é").unwrap(), "a+b+\0é");
assert_eq!(decode_component(c, "%e2%82%ac").unwrap(), "€");
assert_eq!(decode_component(c, ""), Ok(String::new())); }
}

// R23
#[test]
fn hidden_escape_precedence() {
for (s, at) in [("%FFx%", 4), ("a%0", 1), ("%x0", 0), ("%00%G1", 3)] {
assert_eq!(decode_component(Component::Path, s), Err(CodecError::BadEscape { at })); }
}

// R24
#[test]
fn hidden_utf8_origin() {
for (s, at) in [("%41%FF", 3), ("é%80", 2), ("a%E2%82", 1), ("%C0%AF", 0), ("%ED%A0%80", 0), ("%F4%90%80%80", 0)] {
assert_eq!(decode_component(Component::Query, s), Err(CodecError::InvalidUtf8 { at })); }
assert_eq!(decode_component(Component::Path, "%F0%9F%98%80").unwrap(), "😀");
}

// R25
#[test]
fn hidden_encode_sets() {
let text = ":/?#+% &=@";
assert_eq!(encode_component(Component::UserInfo, text), ":%2F%3F%23%2B%25%20%26%3D%40");
assert_eq!(encode_component(Component::Path, text), "%3A/%3F%23%2B%25%20%26%3D%40");
assert_eq!(encode_component(Component::Query, text), "%3A/?%23%2B%25%20%26%3D%40");
assert_eq!(encode_component(Component::Fragment, text), "%3A/?#%2B%25%20%26%3D%40");
assert_eq!(encode_component(Component::Path, "aZ09-._~é"), "aZ09-._~%C3%A9");
}

// R26
#[test]
fn hidden_parser_escape_offsets() {
for (s, at) in [("s://u%G0@h/", 5), ("s://h/a%", 7), ("s://h/?z=%Q1", 9), ("s://h/#x%0", 8)] {
assert_eq!(parse_url(s), Err(ParseError::BadEscape { at })); }
assert_eq!(parse_url("s://h/%41%FF"), Err(ParseError::InvalidUtf8 { at: 9 }));
}

// R3, R26, R28
#[test]
fn hidden_parser_priority() {
assert_eq!(parse_url("s://%FF@h/%"), Err(ParseError::InvalidUtf8 { at: 4 }));
assert_eq!(parse_url("s://u%@h:0/%"), Err(ParseError::InvalidPort { at: 9 }));
assert_eq!(parse_url("s://h/%FF?%"), Err(ParseError::InvalidUtf8 { at: 6 }));
assert_eq!(parse_url("s://h/%FF%"), Err(ParseError::BadEscape { at: 9 }));
assert_eq!(parse_url("s://h/% \n"), Err(ParseError::InvalidByte { at: 7 }));
}

// R26, R27
#[test]
fn hidden_encoded_delimiters() {
let u = parse_url("s://a%40b@H/a%2fb%3F?q=%23%26#%00").unwrap();
assert_eq!(u.userinfo.as_deref(), Some("a%40b")); assert_eq!(u.path, "/a%2fb%3F");
assert_eq!(u.query.as_deref(), Some("q=%23%26")); assert_eq!(u.fragment.as_deref(), Some("%00"));
assert_eq!(parse_url("s://h:%31"), Err(ParseError::InvalidPort { at: 6 }));
}

// R25, R29, R30
#[test]
fn hidden_codec_roundtrip() {
let s = "é+% /?\0😀".repeat(300);
for c in [Component::UserInfo, Component::Path, Component::Query, Component::Fragment] {
let encoded = encode_component(c, &s); assert_eq!(decode_component(c, &encoded).unwrap(), s);
assert_eq!(encode_component(c, &s), encoded); }
assert_eq!(s.len(), "é+% /?\0😀".len() * 300);
}

// R31, R32
#[test]
fn hidden_normalise_validation() {
for s in ["", "rel/%", "%2Froot"] { assert_eq!(normalise_path(s), Err(PathError::NotAbsolute)); }
assert_eq!(normalise_path("/%FF/x%"), Err(PathError::BadEscape { at: 6 }));
assert_eq!(normalise_path("/ok/%E2%82"), Err(PathError::InvalidUtf8 { at: 4 }));
assert_eq!(normalise_path("/é a/../b?x"), Ok("/b?x".into()));
}

// R32-R34
#[test]
fn hidden_normalise_encoded_dots() {
for (s, expected) in [("/x/%2e/y/%2E%2e/z", "/x/z"), ("/a/.%2e", "/"), ("/a/%2e", "/a/"), ("/a%2Fb/../x", "/x"), ("/x/%252e/%2f", "/x/%252e/%2f")] {
assert_eq!(normalise_path(s).unwrap(), expected); }
}

// R33, R34
#[test]
fn hidden_normalise_empty_stack() {
for (s, expected) in [("/a//../b", "/a/b"), ("//x///", "//x///"), ("/../../a", "/a"), ("/a///..", "/a//"), ("/a/./", "/a/"), ("/", "/")] {
assert_eq!(normalise_path(s).unwrap(), expected); assert_eq!(normalise_path(expected).unwrap(), expected); }
}

// R35
#[test]
fn hidden_resolve_base_priority() {
assert_eq!(resolve_url("bad", "x://h/"), Err(ResolveError::Base(ParseError::InvalidScheme { at: 3 })));
assert_eq!(resolve_url("x://h/%", "\n"), Err(ResolveError::Base(ParseError::BadEscape { at: 6 })));
assert_eq!(resolve_url("x://h/", "ab\té"), Err(ResolveError::Reference(ParseError::InvalidByte { at: 2 })));
}

// R36, R40
#[test]
fn hidden_resolve_absolute_network() {
let u = resolve_url("http://u@old:80/a?base#f", "ALT://New/q/../r?x#y").unwrap();
assert_eq!(u.scheme, "alt"); assert_eq!(u.host, "new"); assert_eq!(u.userinfo, None); assert_eq!(u.port, None); assert_eq!(u.path, "/r");
let n = resolve_url("http://old/a?b#c", "//[::A]:7/x/..").unwrap();
assert_eq!(n.scheme, "http"); assert_eq!(n.host, "::a"); assert!(n.ipv6); assert_eq!(n.path, "/"); assert_eq!(n.query, None); assert_eq!(n.fragment, None);
assert_eq!(resolve_url("abc://h/", "//x:0"), Err(ResolveError::Reference(ParseError::InvalidPort { at: 8 })));
}

// R37, R40
#[test]
fn hidden_resolve_empty_query_fragment() {
let base = "z://h/a/../b?old#old";
let u = resolve_url(base, "").unwrap(); assert_eq!(u.path, "/b"); assert_eq!(u.query.as_deref(), Some("old")); assert_eq!(u.fragment, None);
let u = resolve_url(base, "#").unwrap(); assert_eq!(u.query.as_deref(), Some("old")); assert_eq!(u.fragment.as_deref(), Some(""));
let u = resolve_url(base, "?").unwrap(); assert_eq!(u.query.as_deref(), Some("")); assert_eq!(u.fragment, None);
let u = resolve_url(base, "?new#t").unwrap(); assert_eq!(u.query.as_deref(), Some("new")); assert_eq!(u.fragment.as_deref(), Some("t"));
}

// R38-R40
#[test]
fn hidden_resolve_merge() {
let u = resolve_url("https://u@[::1]:443/a/b/file?old#old", "../%2e/c//?q#f").unwrap();
assert_eq!(u.path, "/a/c//"); assert_eq!(u.userinfo.as_deref(), Some("u")); assert_eq!(u.port, Some(443)); assert!(u.ipv6);
assert_eq!(u.query.as_deref(), Some("q")); assert_eq!(u.fragment.as_deref(), Some("f"));
let u = resolve_url("z://h/a/b?q", "/r/../s").unwrap(); assert_eq!(u.path, "/s"); assert_eq!(u.query, None);
assert_eq!(resolve_url("z://h", "leaf").unwrap().path, "/leaf");
}

// R36, R39
#[test]
fn hidden_resolve_errors_offsets() {
assert_eq!(resolve_url("z://h/a/b", "x%"), Err(ResolveError::Reference(ParseError::BadEscape { at: 9 })));
assert_eq!(resolve_url("z://h/", "1:y"), Err(ResolveError::Reference(ParseError::InvalidScheme { at: 0 })));
assert_eq!(resolve_url("z://h/", "q:leaf"), Err(ResolveError::Reference(ParseError::MissingAuthority { at: 2 })));
assert_eq!(resolve_url("z://h/", "//x/%"), Err(ResolveError::Reference(ParseError::BadEscape { at: 6 })));
}

// R36, R37, R40
#[test]
fn hidden_resolve_delimiter_colons() {
assert_eq!(resolve_url("z://h/d/f", "./x:y").unwrap().path, "/d/x:y");
assert_eq!(resolve_url("z://h/f", "?a:b#x:y").unwrap().query.as_deref(), Some("a:b"));
assert_eq!(resolve_url("z://h/f", "#a:b").unwrap().fragment.as_deref(), Some("a:b"));
}

// R29, R33, R38
#[test]
fn hidden_resolve_long() {
let r = format!("{}last", "a/../".repeat(400)); assert_eq!(resolve_url("z://h/d/f", &r).unwrap().path, "/d/last");
}
