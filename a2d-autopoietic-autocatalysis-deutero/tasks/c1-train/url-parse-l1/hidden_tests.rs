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

// R9
#[test]
fn hidden_host_rules() {
for s in ["s://.A..", "s://-A-", "s://___"] { assert!(parse_url(s).is_ok()); }
assert_eq!(parse_url("s://ab!c:xx"), Err(ParseError::InvalidHost { at: 6 }));
assert_eq!(parse_url("s://a%41"), Err(ParseError::InvalidHost { at: 5 }));
}

// R10
#[test]
fn hidden_port_empty() {
assert_eq!(parse_url("u://h:"), Err(ParseError::InvalidPort { at: 6 }));
assert_eq!(parse_url("u://h:?p"), Err(ParseError::InvalidPort { at: 6 }));
}

// R11
#[test]
fn hidden_port_digits_priority() {
for (s, at) in [("u://h:0x", 7), ("u://h:00x", 8), ("u://h:2:3", 7), ("u://h:+5", 6)] {
assert_eq!(parse_url(s), Err(ParseError::InvalidPort { at })); }
}

// R11, R17
#[test]
fn hidden_port_bounds() {
assert_eq!(parse_url("u://h:1").unwrap().port, Some(1));
assert_eq!(parse_url("u://h:65535").unwrap().port, Some(65535));
for p in ["0", "00", "01", "65536", "999999999999999999999999999999999999999"] {
assert_eq!(parse_url(&format!("u://h:{p}")), Err(ParseError::InvalidPort { at: 6 })); }
assert_eq!(parse_url("http://h:80").unwrap().port, Some(80));
}

// R12, R13
#[test]
fn hidden_fragment_first() {
let u = parse_url("f://h/#one?two#three").unwrap();
assert_eq!(u.query, None); assert_eq!(u.fragment.as_deref(), Some("one?two#three"));
assert_eq!(parse_url("f://h/?a?b#c").unwrap().query.as_deref(), Some("a?b"));
}

// R12-R14, R18
#[test]
fn hidden_empty_optionals() {
let a = parse_url("n://h?#").unwrap();
assert_eq!(a.path, "/"); assert_eq!(a.query, Some(String::new())); assert_eq!(a.fragment, Some(String::new()));
assert_ne!(a, parse_url("n://h/").unwrap());
}

// R14, R17
#[test]
fn hidden_raw_path() {
let u = parse_url("x://h/A//./b/../c/").unwrap(); assert_eq!(u.path, "/A//./b/../c/");
}

// R7, R12-R15, R17
#[test]
fn hidden_component_delimiters() {
let u = parse_url("t://h/a:b@c?q=@:x?/r#f#?@").unwrap();
assert_eq!(u.path, "/a:b@c"); assert_eq!(u.query.as_deref(), Some("q=@:x?/r")); assert_eq!(u.fragment.as_deref(), Some("f#?@"));
}

// R16, R17
#[test]
fn hidden_literal_percent() {
let u = parse_url("p://u%ZZ@h/%?x=%G#%").unwrap();
assert_eq!(u.userinfo.as_deref(), Some("u%ZZ")); assert_eq!(u.path, "/%"); assert_eq!(u.query.as_deref(), Some("x=%G")); assert_eq!(u.fragment.as_deref(), Some("%"));
}

// R9
#[test]
fn hidden_brackets_rejected() {
assert_eq!(parse_url("b://[::1]"), Err(ParseError::InvalidHost { at: 4 }));
assert_eq!(parse_url("b://h]"), Err(ParseError::InvalidHost { at: 5 }));
}

// R3, R9-R11
#[test]
fn hidden_priority_host_port() {
assert_eq!(parse_url("z://!:"), Err(ParseError::InvalidHost { at: 4 }));
assert_eq!(parse_url("z://:bad"), Err(ParseError::InvalidHost { at: 4 }));
}

// R3, R4
#[test]
fn hidden_late_raw_byte() {
assert_eq!(parse_url("z://u@@h:0#end\n"), Err(ParseError::InvalidByte { at: 14 }));
}

// R17
#[test]
fn hidden_case_sensitive_components() {
let u = parse_url("AA://U@HH/P?Q#F").unwrap();
assert_eq!(u.userinfo.as_deref(), Some("U")); assert_eq!(u.path, "/P"); assert_eq!(u.query.as_deref(), Some("Q")); assert_eq!(u.fragment.as_deref(), Some("F"));
}

// R2, R14
#[test]
fn hidden_long_path() {
let p = format!("/{}", "segment/".repeat(500)); let u = parse_url(&format!("bulk://h{p}")).unwrap(); assert_eq!(u.path, p);
}
