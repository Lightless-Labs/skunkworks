// Clauses: R4, R3.
#[test]
fn hidden_empty_and_unicode_priority() {
    assert_eq!(parse_email(""), Err(EmailError::EmptyInput));
    assert_eq!(parse_email("bad@@é"), Err(EmailError::NonAscii { at: 5 }));
    assert_eq!(parse_email("é@d.com"), Err(EmailError::NonAscii { at: 0 }));
    assert_eq!(parse_email("x@a.é"), Err(EmailError::NonAscii { at: 4 }));
}
// Clauses: R7, R21.
#[test]
fn hidden_separator_order() {
    assert_eq!(parse_email("z.example"), Err(EmailError::MissingAt));
    assert_eq!(parse_email("@x@a.net"), Err(EmailError::MultipleAt { at: 2 }));
    assert_eq!(parse_email(".@x.net@"), Err(EmailError::MultipleAt { at: 7 }));
    assert_eq!(parse_email("@"), Err(EmailError::EmptyLocal { at: 0 }));
}
// Clauses: R1, R8, R9, R2.
#[test]
fn hidden_atom_punctuation_and_case() {
    assert_eq!(parse_email("AZ09!#$%&'*+-/=?^_`{|}~.end@MiXeD.NeT"), Ok(Address { local: "AZ09!#$%&'*+-/=?^_`{|}~.end".into(), domain: "mixed.net".into(), quoted: false }));
}
// Clauses: R9.
#[test]
fn hidden_dot_positions() {
    assert_eq!(parse_email(".a@x.org"), Err(EmailError::LocalDot { at: 0 }));
    assert_eq!(parse_email("a.@x.org"), Err(EmailError::LocalDot { at: 1 }));
    assert_eq!(parse_email("ab..cd@x.org"), Err(EmailError::LocalDot { at: 3 }));
    assert_eq!(parse_email("a..@x.org"), Err(EmailError::LocalDot { at: 2 }));
}
// Clauses: R8, R9.
#[test]
fn hidden_local_chars_and_precedence() {
    assert_eq!(parse_email("a:b.@x.org"), Err(EmailError::LocalChar { at: 1 }));
    assert_eq!(parse_email("a@x.org"), Err(EmailError::LocalChar { at: 1 }));
    assert_eq!(parse_email("a\"z@x.org"), Err(EmailError::LocalChar { at: 1 }));
    assert_eq!(parse_email("a\x01@x.org"), Err(EmailError::LocalChar { at: 1 }));
}
// Clauses: R8, R21.
#[test]
fn hidden_local_length_boundary() {
    let good = format!("{}@q.info", "a".repeat(64));
    assert_eq!(parse_email(&good).unwrap().local.len(), 64);
    let bad = format!("{}:@q.info", "a".repeat(64));
    assert_eq!(parse_email(&bad), Err(EmailError::LocalTooLong { at: 64 }));
}
// Clauses: R13.
#[test]
fn hidden_domain_empty_and_root() {
    assert_eq!(parse_email("z@"), Err(EmailError::EmptyDomain { at: 2 }));
    assert_eq!(parse_email("z@."), Err(EmailError::EmptyDomain { at: 2 }));
    assert_eq!(parse_email("z@EXAMPLE.EDU."), Ok(Address { local: "z".into(), domain: "example.edu".into(), quoted: false }));
    assert_eq!(parse_email("z@a.edu.."), Err(EmailError::EmptyLabel { at: 8 }));
}
// Clauses: R15.
#[test]
fn hidden_empty_labels() {
    assert_eq!(parse_email("z@.a.org"), Err(EmailError::EmptyLabel { at: 2 }));
    assert_eq!(parse_email("z@a..org"), Err(EmailError::EmptyLabel { at: 4 }));
    assert_eq!(parse_email("z@.."), Err(EmailError::EmptyLabel { at: 2 }));
}
// Clauses: R16.
#[test]
fn hidden_label_length() {
    let good = format!("q@{}.io", "z".repeat(63));
    assert!(parse_email(&good).is_ok());
    let bad = format!("q@-{}.io", "z".repeat(63));
    assert_eq!(parse_email(&bad), Err(EmailError::LabelTooLong { at: 65 }));
    let later = format!("q@ok.{}.io", "z".repeat(64));
    assert_eq!(parse_email(&later), Err(EmailError::LabelTooLong { at: 68 }));
}
// Clauses: R14, R21.
#[test]
fn hidden_domain_length() {
    let d = format!("{}.{}.{}.{}", "a".repeat(63), "b".repeat(63), "c".repeat(63), "d".repeat(61));
    assert_eq!(d.len(), 253);
    assert_eq!(parse_email(&format!("{}@{}.", "L".repeat(64), d)).unwrap().domain, d);
    let over = format!("q@{}x", d);
    assert_eq!(parse_email(&over), Err(EmailError::DomainTooLong { at: 255 }));
    let invalid = format!("q@_{}", "x".repeat(253));
    assert_eq!(parse_email(&invalid), Err(EmailError::DomainTooLong { at: 255 }));
}
// Clauses: R17.
#[test]
fn hidden_hyphen_priority() {
    assert_eq!(parse_email("z@-a-.org"), Err(EmailError::LabelHyphen { at: 2 }));
    assert_eq!(parse_email("z@_a-.org"), Err(EmailError::LabelHyphen { at: 4 }));
    assert_eq!(parse_email("z@-.org"), Err(EmailError::LabelHyphen { at: 2 }));
    assert_eq!(parse_email("z@a-b.c-d"), Ok(Address { local: "z".into(), domain: "a-b.c-d".into(), quoted: false }));
}
// Clauses: R18, R16.
#[test]
fn hidden_label_char_order() {
    assert_eq!(parse_email("z@a_b.org"), Err(EmailError::DomainChar { at: 3 }));
    assert_eq!(parse_email("z@[127.0.0.1]"), Err(EmailError::DomainChar { at: 2 }));
    assert_eq!(parse_email("z@a_b.-bad"), Err(EmailError::DomainChar { at: 3 }));
    assert_eq!(parse_email("z@a:b.org"), Err(EmailError::DomainChar { at: 3 }));
}
// Clauses: R19, R20.
#[test]
fn hidden_final_label_checks() {
    assert_eq!(parse_email("z@123"), Err(EmailError::DomainNeedsDot { at: 2 }));
    assert_eq!(parse_email("z@localhost."), Err(EmailError::DomainNeedsDot { at: 2 }));
    assert_eq!(parse_email("z@x.012"), Err(EmailError::NumericTld { at: 4 }));
    assert_eq!(parse_email("z@0.7a"), Ok(Address { local: "z".into(), domain: "0.7a".into(), quoted: false }));
    assert_eq!(parse_email("z@9.2-3"), Ok(Address { local: "z".into(), domain: "9.2-3".into(), quoted: false }));
}
// Clauses: R21.
#[test]
fn hidden_stage_precedence() {
    assert_eq!(parse_email("x:@"), Err(EmailError::LocalChar { at: 1 }));
    assert_eq!(parse_email("x.@-a.123"), Err(EmailError::LocalDot { at: 1 }));
    assert_eq!(parse_email("z@-.123"), Err(EmailError::LabelHyphen { at: 2 }));
    assert_eq!(parse_email("z@a.12_"), Err(EmailError::DomainChar { at: 6 }));
}
// Clauses: R10, R12.
#[test]
fn hidden_quoted_content() {
    assert_eq!(parse_email("\".. A(@) ..\"@EX.NET"), Ok(Address { local: ".. A(@) ..".into(), domain: "ex.net".into(), quoted: true }));
    assert_eq!(parse_email("\"x\\@y\\\"z\\\\w\"@d.net"), Ok(Address { local: "x@y\"z\\w".into(), domain: "d.net".into(), quoted: true }));
}
// Clauses: R6, R7.
#[test]
fn hidden_quote_scan_precedence() {
    assert_eq!(parse_email("\"unterminated@d.net"), Err(EmailError::UnclosedQuote { at: 0 }));
    assert_eq!(parse_email("\"tail\\"), Err(EmailError::UnclosedQuote { at: 0 }));
    assert_eq!(parse_email("\"a@b\""), Err(EmailError::MissingAt));
    assert_eq!(parse_email("\"a@b\"@x.net@z"), Err(EmailError::MultipleAt { at: 11 }));
    assert_eq!(parse_email("a\"@x.net"), Err(EmailError::LocalChar { at: 1 }));
}
// Clauses: R10.
#[test]
fn hidden_quoted_trailing_before_decode() {
    assert_eq!(parse_email("\"a\"x@d.net"), Err(EmailError::TrailingLocal { at: 3 }));
    assert_eq!(parse_email("\"a\\q\"x@d.net"), Err(EmailError::TrailingLocal { at: 5 }));
    assert_eq!(parse_email("\"a\"\"@d.net"), Err(EmailError::TrailingLocal { at: 3 }));
}
// Clauses: R11, R10.
#[test]
fn hidden_bad_escape_and_quoted_controls() {
    assert_eq!(parse_email("\"a\\q\"@d.net"), Err(EmailError::BadEscape { at: 2 }));
    assert_eq!(parse_email("\"a\tb\"@d.net"), Err(EmailError::QuotedChar { at: 2 }));
    assert_eq!(parse_email("\"a\r\nb\"@d.net"), Err(EmailError::QuotedChar { at: 2 }));
    assert_eq!(parse_email("\"a\"@d.net"), Err(EmailError::QuotedChar { at: 2 }));
    assert_eq!(parse_email("\"a\\\t\"@d.net"), Err(EmailError::BadEscape { at: 2 }));
}
// Clauses: R12, R11.
#[test]
fn hidden_quoted_empty_and_length() {
    assert_eq!(parse_email("\"\"@d.net"), Err(EmailError::EmptyLocal { at: 1 }));
    let good = format!("\"{}\"@d.net", "\\@".repeat(64));
    assert_eq!(parse_email(&good).unwrap().local, "@".repeat(64));
    let bad = format!("\"{}\\@\"@d.net", "a".repeat(64));
    assert_eq!(parse_email(&bad), Err(EmailError::LocalTooLong { at: 65 }));
    let content_first = format!("\"{}\\q\"@d.net", "a".repeat(65));
    assert_eq!(parse_email(&content_first), Err(EmailError::BadEscape { at: 66 }));
}
// Clauses: R7, R13.
#[test]
fn hidden_quoted_separator_then_domain() {
    assert_eq!(parse_email("\"a\\@b\"@@x.net"), Err(EmailError::MultipleAt { at: 7 }));
    assert_eq!(parse_email("\"x\"@"), Err(EmailError::EmptyDomain { at: 4 }));
    assert_eq!(parse_email("\"x\"@d.8"), Err(EmailError::NumericTld { at: 6 }));
    assert_eq!(parse_email("x@\"d.net\""), Err(EmailError::DomainChar { at: 2 }));
}
// Clauses: R22, R27, R28.
#[test]
fn hidden_cfws_strip_and_positions() {
    assert_eq!(parse_email(" (outer) \tAlpha(comment) @ (d) EXAMPLE.ORG (tail) "), Ok(Address { local: "Alpha".into(), domain: "example.org".into(), quoted: false }));
    assert_eq!(parse_email(" (c) a..b@d.net"), Err(EmailError::LocalDot { at: 7 }));
    assert_eq!(parse_email("a@ (c) -x.net"), Err(EmailError::LabelHyphen { at: 7 }));
    assert_eq!(parse_email("a@ (c) "), Err(EmailError::EmptyDomain { at: 7 }));
}
// Clauses: R27, R28.
#[test]
fn hidden_cfws_boundaries_and_empty() {
    assert_eq!(parse_email("ab (x) cd@d.net"), Err(EmailError::CfwsBoundary { at: 2 }));
    assert_eq!(parse_email("a@d (x) .net"), Err(EmailError::CfwsBoundary { at: 3 }));
    assert_eq!(parse_email("a@d.(x)net"), Err(EmailError::CfwsBoundary { at: 4 }));
    assert_eq!(parse_email("\"a\"(x)z@d.net"), Err(EmailError::CfwsBoundary { at: 3 }));
    assert_eq!(parse_email(" \t(a(b)c)\r\n \t"), Err(EmailError::EmptyInput));
}
// Clauses: R23.
#[test]
fn hidden_fold_validity() {
    assert_eq!(parse_email("\r\n \ta\r\n \t@\r\n  d.net\r\n\t"), Ok(Address { local: "a".into(), domain: "d.net".into(), quoted: false }));
    assert_eq!(parse_email("a@d.net\n "), Err(EmailError::BadFold { at: 7 }));
    assert_eq!(parse_email("a@d.net\r\n"), Err(EmailError::BadFold { at: 7 }));
    assert_eq!(parse_email("a@d.net\r "), Err(EmailError::BadFold { at: 7 }));
    assert_eq!(parse_email("(x\n y)a@d.net"), Err(EmailError::BadFold { at: 2 }));
}
// Clauses: R24, R25.
#[test]
fn hidden_comment_nesting_and_errors() {
    assert_eq!(parse_email("(one(two(three)))a@d.net"), Ok(Address { local: "a".into(), domain: "d.net".into(), quoted: false }));
    assert_eq!(parse_email("(a(b(c(d))))a@d.net"), Err(EmailError::CommentDepth { at: 6 }));
    assert_eq!(parse_email("(x(y)a@d.net"), Err(EmailError::UnclosedComment { at: 0 }));
    assert_eq!(parse_email("a)@d.net"), Err(EmailError::UnexpectedClose { at: 1 }));
    assert_eq!(parse_email("(x\x01)a@d.net"), Err(EmailError::CommentChar { at: 2 }));
}
// Clauses: R25, R23.
#[test]
fn hidden_comment_escapes_and_fold() {
    assert_eq!(parse_email("(x\\(\\)\\\\\t\r\n y\"@)a@d.net"), Ok(Address { local: "a".into(), domain: "d.net".into(), quoted: false }));
    assert_eq!(parse_email("(x\\"), Err(EmailError::CommentEscape { at: 2 }));
    assert_eq!(parse_email("(x\\\t)a@d.net"), Err(EmailError::CommentEscape { at: 2 }));
    assert_eq!(parse_email("(x\\\r\n )a@d.net"), Err(EmailError::CommentEscape { at: 2 }));
}
// Clauses: R5, R26, R29.
#[test]
fn hidden_cleanup_priority_and_protection() {
    assert_eq!(parse_email("a (x)b@d.net) not"), Err(EmailError::UnexpectedClose { at: 12 }));
    assert_eq!(parse_email(".@d.net(unclosed"), Err(EmailError::UnclosedComment { at: 7 }));
    assert_eq!(parse_email("(badé)a@@"), Err(EmailError::NonAscii { at: 4 }));
    assert_eq!(parse_email(" (c) \"a(b) @ c\" (x) @d.net"), Ok(Address { local: "a(b) @ c".into(), domain: "d.net".into(), quoted: true }));
    assert_eq!(parse_email("\"a\r\n b\"@d.net"), Err(EmailError::QuotedChar { at: 2 }));
    assert_eq!(parse_email("\"a\\q\"@d.net"), Err(EmailError::BadEscape { at: 2 }));
}
// Clauses: R29, R3, R12.
#[test]
fn hidden_cfws_large_and_mapped_overflow() {
    let s = format!("({}) A@D.NET ({})", "x".repeat(20000), "y".repeat(1000));
    assert_eq!(parse_email(&s).unwrap().local, "A");
    let local = format!(" (c) {}@d.net", "x".repeat(65));
    assert_eq!(parse_email(&local), Err(EmailError::LocalTooLong { at: 69 }));
    let quoted = format!("(c)\"{}\\@\"@d.net", "x".repeat(64));
    assert_eq!(parse_email(&quoted), Err(EmailError::LocalTooLong { at: 68 }));
}
// Clauses: R28, R16, R20.
#[test]
fn hidden_mapped_label_and_numeric() {
    assert_eq!(parse_email("(q)a @ (d)x.77 (tail)"), Err(EmailError::NumericTld { at: 12 }));
    let s = format!("a @ (c) {}.net", "z".repeat(64));
    assert_eq!(parse_email(&s), Err(EmailError::LabelTooLong { at: 71 }));
    let d = format!("{}.{}.{}.{}", "a".repeat(63), "b".repeat(63), "c".repeat(63), "d".repeat(62));
    assert_eq!(parse_email(&format!("a @ (c) {}", d)), Err(EmailError::DomainTooLong { at: 261 }));
}
// Clauses: R5, R21.
#[test]
fn hidden_lexical_first_and_boundary_first() {
    assert_eq!(parse_email("a b@d.net(\\\t)"), Err(EmailError::CommentEscape { at: 10 }));
    assert_eq!(parse_email("a b@d.net c"), Err(EmailError::CfwsBoundary { at: 1 }));
    assert_eq!(parse_email("a b@@"), Err(EmailError::CfwsBoundary { at: 1 }));
    assert_eq!(parse_email("(ok)\"x@d.net"), Err(EmailError::UnclosedQuote { at: 4 }));
}
// Clauses: R26, R11.
#[test]
fn hidden_quote_no_reopening() {
    assert_eq!(parse_email("a\"(x)z@d.net"), Err(EmailError::CfwsBoundary { at: 2 }));
    assert_eq!(parse_email("a\"@d.net"), Err(EmailError::LocalChar { at: 1 }));
    assert_eq!(parse_email("\"a\"\"(x)@d.net"), Err(EmailError::TrailingLocal { at: 3 }));
    assert_eq!(parse_email("\"(\\q)\"@d.net"), Err(EmailError::BadEscape { at: 2 }));
}
