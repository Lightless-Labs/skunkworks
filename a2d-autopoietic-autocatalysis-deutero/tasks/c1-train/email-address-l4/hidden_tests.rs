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
// Clauses: R30, R19.
#[test]
fn hidden_policy_default_and_single_labels() {
    let p = Policy::default();
    assert_eq!(p, Policy { local_case: LocalCase::Preserve, strip_subaddress: false, unquote_safe: false, allow_single_label: false });
    assert_eq!(normalize_email("Q@HOST.", &p), Err(EmailError::DomainNeedsDot { at: 2 }));
    let p = Policy { allow_single_label: true, ..p };
    assert_eq!(normalize_email("Q@HOST.", &p), Ok(Address { local: "Q".into(), domain: "host".into(), quoted: false }));
    assert_eq!(parse_email("Q@HOST"), Err(EmailError::DomainNeedsDot { at: 2 }));
    assert_eq!(normalize_email("Q@123", &p), Err(EmailError::NumericTld { at: 2 }));
    assert_eq!(normalize_email("Q@-x", &p), Err(EmailError::LabelHyphen { at: 2 }));
}
// Clauses: R31, R32.
#[test]
fn hidden_case_and_first_plus() {
    let p = Policy { local_case: LocalCase::Lower, strip_subaddress: true, ..Policy::default() };
    assert_eq!(normalize_email("(c) AB.C+Tag+More@D.NET.", &p).unwrap().local, "ab.c");
    let a = normalize_email("\"AB+Tag+More\"@d.net", &p).unwrap();
    assert_eq!(a.local, "ab"); assert!(a.quoted);
    assert_eq!(normalize_email("A+@d.net", &p).unwrap().local, "a");
    let keep = Policy { strip_subaddress: false, ..p };
    assert_eq!(normalize_email("A+T@D.NET", &keep).unwrap().local, "a+t");
    assert_eq!(normalize_email("\"A@B\"@D.NET", &keep).unwrap().local, "a@b");
    assert_eq!(normalize_email("A+T@d.net", &Policy::default()).unwrap().local, "A+T");
}
// Clauses: R32, R30.
#[test]
fn hidden_strip_errors_after_validation() {
    let p = Policy { strip_subaddress: true, ..Policy::default() };
    assert_eq!(normalize_email(" (c) +TAG@d.net", &p), Err(EmailError::EmptyNormalizedLocal { at: 5 }));
    assert_eq!(normalize_email("(c)\"+X\"@d.net", &p), Err(EmailError::EmptyNormalizedLocal { at: 4 }));
    assert_eq!(normalize_email("+X@-d.net", &p), Err(EmailError::LabelHyphen { at: 3 }));
    assert_eq!(normalize_email("a+bad:@d.net", &p), Err(EmailError::LocalChar { at: 5 }));
    assert_eq!(normalize_email("\"a+bad\\q\"@d.net", &p), Err(EmailError::BadEscape { at: 6 }));
}
// Clauses: R33.
#[test]
fn hidden_requote_and_unquote_safety() {
    for unquote_safe in [false, true] {
        let p = Policy { strip_subaddress: true, unquote_safe, ..Policy::default() };
        let a = normalize_email("x.+tag@D.NET", &p).unwrap();
        assert_eq!(a.local, "x."); assert!(a.quoted);
        let a = normalize_email("\"X Y+tag\"@d.net", &p).unwrap();
        assert_eq!(a.local, "X Y"); assert!(a.quoted);
        let a = normalize_email("\"X+tag\"@d.net", &p).unwrap();
        assert_eq!(a.local, "X"); assert_eq!(a.quoted, !unquote_safe);
    }
    let p = Policy { unquote_safe: true, ..Policy::default() };
    assert!(!normalize_email("\"A.b\"@d.net", &p).unwrap().quoted);
    assert!(normalize_email("\".\"@d.net", &p).unwrap().quoted);
}
// Clauses: R34.
#[test]
fn hidden_format_verbatim_and_escaping() {
    let a = Address { local: "a\"\\@ é".into(), domain: "D.NET.".into(), quoted: true };
    assert_eq!(format_email(&a), "\"a\\\"\\\\@ é\"@D.NET.");
    let a = Address { quoted: false, ..a };
    assert_eq!(format_email(&a), "a\"\\@ é@D.NET.");
    assert_eq!(format_email(&Address { local: String::new(), domain: String::new(), quoted: true }), "\"\"@");
    assert_eq!(format_email(&Address { local: "X\nY".into(), domain: "bad domain".into(), quoted: false }), "X\nY@bad domain");
}
// Clauses: R35, R36, R37.
#[test]
fn hidden_dedup_first_indices_and_errors() {
    let inputs = ["bad", "\"A\"@D.NET.", "bad", "A@d.net", "B@d.net", "\"B\"@d.net", "A@D.NET", "+X@d.net"];
    let p = Policy { strip_subaddress: true, ..Policy::default() };
    let r = deduplicate(&inputs, &p);
    assert_eq!(r.source_indices, vec![1, 4]);
    assert_eq!(r.addresses, vec![Address { local: "A".into(), domain: "d.net".into(), quoted: true }, Address { local: "B".into(), domain: "d.net".into(), quoted: false }]);
    assert_eq!(r.duplicate_of, vec![None, None, None, Some(1), None, Some(4), Some(1), None]);
    assert_eq!(r.errors, vec![(0, EmailError::MissingAt), (2, EmailError::MissingAt), (7, EmailError::EmptyNormalizedLocal { at: 0 })]);
    assert_eq!(deduplicate(&[], &p), DedupResult { addresses: vec![], source_indices: vec![], duplicate_of: vec![], errors: vec![] });
}
// Clauses: R36, R37.
#[test]
fn hidden_dedup_case_policy_and_independence() {
    let inputs = ["A+one@d.net", "a+two@D.NET.", "\"A+three\"@(c)d.net", "A+one@other.net"];
    let p = Policy::default();
    assert_eq!(deduplicate(&inputs, &p).addresses.len(), 4);
    let p = Policy { strip_subaddress: true, ..p };
    let r = deduplicate(&inputs, &p);
    assert_eq!(r.source_indices, vec![0, 1, 3]); assert_eq!(r.duplicate_of[2], Some(0));
    let p = Policy { local_case: LocalCase::Lower, unquote_safe: true, ..p };
    let r = deduplicate(&inputs, &p);
    assert_eq!(r.source_indices, vec![0, 3]);
    assert_eq!(r.duplicate_of, vec![None, Some(0), Some(0), None]);
    assert_eq!(deduplicate(&inputs, &Policy::default()).addresses.len(), 4);
}
// Clauses: R38, R36.
#[test]
fn hidden_roundtrip_and_many_inputs() {
    let samples = ["x.+T@d.net", "\"A\\\"B+T\"@(c)D.NET.", "(c)\"AB\"@Host", "\"@\\\\a\"@d.net", "A+@d.net"];
    for lower in [LocalCase::Preserve, LocalCase::Lower] {
        for strip_subaddress in [false, true] {
            for unquote_safe in [false, true] {
                let p = Policy { local_case: lower, strip_subaddress, unquote_safe, allow_single_label: true };
                for s in samples {
                    let a = normalize_email(s, &p).unwrap();
                    assert_eq!(normalize_email(&format_email(&a), &p), Ok(a));
                }
            }
        }
    }
    let owned: Vec<String> = (0..600).map(|i| format!("User{}+{}@D.NET.", i % 50, i)).collect();
    let inputs: Vec<&str> = owned.iter().map(String::as_str).collect();
    let p = Policy { strip_subaddress: true, local_case: LocalCase::Lower, ..Policy::default() };
    let r = deduplicate(&inputs, &p);
    assert_eq!(r.source_indices, (0..50).collect::<Vec<_>>());
    assert_eq!(r.addresses.len(), 50); assert!(r.errors.is_empty());
    for i in 50..600 { assert_eq!(r.duplicate_of[i], Some(i % 50)); }
    assert_eq!(r.addresses[0].local, "user0");
}
