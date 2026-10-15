// Clauses: R4, R3.
#[test]
fn hidden_empty_and_unicode_priority() {
    assert_eq!(parse_email(""), Err(EmailError::EmptyInput));
    assert_eq!(parse_email("bad@@é"), Err(EmailError::NonAscii { at: 5 }));
    assert_eq!(parse_email("é@d.com"), Err(EmailError::NonAscii { at: 0 }));
    assert_eq!(parse_email("x@a.é"), Err(EmailError::NonAscii { at: 4 }));
}
// Clauses: R7, R18.
#[test]
fn hidden_separator_order() {
    assert_eq!(parse_email("z.example"), Err(EmailError::MissingAt));
    assert_eq!(parse_email("@x@a.net"), Err(EmailError::MultipleAt { at: 2 }));
    assert_eq!(parse_email("bad space@x.net@"), Err(EmailError::MultipleAt { at: 15 }));
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
    assert_eq!(parse_email("a b@x.org"), Err(EmailError::LocalChar { at: 1 }));
}
// Clauses: R8, R18.
#[test]
fn hidden_local_length_boundary() {
    let good = format!("{}@q.info", "a".repeat(64));
    assert_eq!(parse_email(&good).unwrap().local.len(), 64);
    let bad = format!("{}:@q.info", "a".repeat(64));
    assert_eq!(parse_email(&bad), Err(EmailError::LocalTooLong { at: 64 }));
}
// Clauses: R10.
#[test]
fn hidden_domain_empty_and_root() {
    assert_eq!(parse_email("z@"), Err(EmailError::EmptyDomain { at: 2 }));
    assert_eq!(parse_email("z@."), Err(EmailError::EmptyDomain { at: 2 }));
    assert_eq!(parse_email("z@EXAMPLE.EDU."), Ok(Address { local: "z".into(), domain: "example.edu".into(), quoted: false }));
    assert_eq!(parse_email("z@a.edu.."), Err(EmailError::EmptyLabel { at: 8 }));
}
// Clauses: R12.
#[test]
fn hidden_empty_labels() {
    assert_eq!(parse_email("z@.a.org"), Err(EmailError::EmptyLabel { at: 2 }));
    assert_eq!(parse_email("z@a..org"), Err(EmailError::EmptyLabel { at: 4 }));
    assert_eq!(parse_email("z@.."), Err(EmailError::EmptyLabel { at: 2 }));
}
// Clauses: R13.
#[test]
fn hidden_label_length() {
    let good = format!("q@{}.io", "z".repeat(63));
    assert!(parse_email(&good).is_ok());
    let bad = format!("q@-{}.io", "z".repeat(63));
    assert_eq!(parse_email(&bad), Err(EmailError::LabelTooLong { at: 65 }));
    let later = format!("q@ok.{}.io", "z".repeat(64));
    assert_eq!(parse_email(&later), Err(EmailError::LabelTooLong { at: 68 }));
}
// Clauses: R11, R18.
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
// Clauses: R14.
#[test]
fn hidden_hyphen_priority() {
    assert_eq!(parse_email("z@-a-.org"), Err(EmailError::LabelHyphen { at: 2 }));
    assert_eq!(parse_email("z@_a-.org"), Err(EmailError::LabelHyphen { at: 4 }));
    assert_eq!(parse_email("z@-.org"), Err(EmailError::LabelHyphen { at: 2 }));
    assert_eq!(parse_email("z@a-b.c-d"), Ok(Address { local: "z".into(), domain: "a-b.c-d".into(), quoted: false }));
}
// Clauses: R15, R13.
#[test]
fn hidden_label_char_order() {
    assert_eq!(parse_email("z@a_b.org"), Err(EmailError::DomainChar { at: 3 }));
    assert_eq!(parse_email("z@[127.0.0.1]"), Err(EmailError::DomainChar { at: 2 }));
    assert_eq!(parse_email("z@a_b.-bad"), Err(EmailError::DomainChar { at: 3 }));
    assert_eq!(parse_email("z@a:b.org"), Err(EmailError::DomainChar { at: 3 }));
}
// Clauses: R16, R17.
#[test]
fn hidden_final_label_checks() {
    assert_eq!(parse_email("z@123"), Err(EmailError::DomainNeedsDot { at: 2 }));
    assert_eq!(parse_email("z@localhost."), Err(EmailError::DomainNeedsDot { at: 2 }));
    assert_eq!(parse_email("z@x.012"), Err(EmailError::NumericTld { at: 4 }));
    assert_eq!(parse_email("z@0.7a"), Ok(Address { local: "z".into(), domain: "0.7a".into(), quoted: false }));
    assert_eq!(parse_email("z@9.2-3"), Ok(Address { local: "z".into(), domain: "9.2-3".into(), quoted: false }));
}
// Clauses: R18.
#[test]
fn hidden_stage_precedence() {
    assert_eq!(parse_email("x:@"), Err(EmailError::LocalChar { at: 1 }));
    assert_eq!(parse_email("x.@-a.123"), Err(EmailError::LocalDot { at: 1 }));
    assert_eq!(parse_email("z@-.123"), Err(EmailError::LabelHyphen { at: 2 }));
    assert_eq!(parse_email("z@a.12_"), Err(EmailError::DomainChar { at: 6 }));
}
// Clauses: R5.
#[test]
fn hidden_no_trim_or_comments() {
    assert_eq!(parse_email(" a@b.org"), Err(EmailError::LocalChar { at: 0 }));
    assert_eq!(parse_email("a@b.org "), Err(EmailError::DomainChar { at: 7 }));
    assert_eq!(parse_email("(a)@b.org"), Err(EmailError::LocalChar { at: 0 }));
    assert_eq!(parse_email("\"a\"@b.org"), Err(EmailError::LocalChar { at: 0 }));
    assert_eq!(parse_email("a@b.\torg"), Err(EmailError::DomainChar { at: 4 }));
}
// Clauses: R6.
#[test]
fn hidden_all_at_bytes_count() {
    assert_eq!(parse_email("\"x@y\"@b.org"), Err(EmailError::MultipleAt { at: 5 }));
    assert_eq!(parse_email("x(a@b)@z.org"), Err(EmailError::MultipleAt { at: 6 }));
    assert_eq!(parse_email("a\\@b.org"), Err(EmailError::LocalChar { at: 1 }));
}
// Clauses: R8.
#[test]
fn hidden_all_invalid_atom_bytes() {
    for b in [0u8, 9, 10, 13, 32, 34, 40, 41, 44, 58, 59, 60, 62, 91, 92, 93, 127] {
        let s = format!("a{}b@d.net", char::from(b));
        assert_eq!(parse_email(&s), Err(EmailError::LocalChar { at: 1 }), "byte {}", b);
    }
}
// Clauses: R13.
#[test]
fn hidden_label_overflow_beats_bad_start() {
    for lead in ['-', '_'] {
        let s = format!("a@{}{}.net", lead, "z".repeat(63));
        assert_eq!(parse_email(&s), Err(EmailError::LabelTooLong { at: 65 }));
    }
}
// Clauses: R8.
#[test]
fn hidden_length_beats_bad_local() {
    let s = format!(".{}@h.org", "x".repeat(64));
    assert_eq!(parse_email(&s), Err(EmailError::LocalTooLong { at: 64 }));
}
// Clauses: R10, R12.
#[test]
fn hidden_domain_root_is_single() {
    assert_eq!(parse_email("a@b.net..."), Err(EmailError::EmptyLabel { at: 8 }));
    assert_eq!(parse_email("A@1a.B2."), Ok(Address { local: "A".into(), domain: "1a.b2".into(), quoted: false }));
}
// Clauses: R14, R17.
#[test]
fn hidden_domain_hyphen_and_digits() {
    assert_eq!(parse_email("a@a.-1"), Err(EmailError::LabelHyphen { at: 4 }));
    assert_eq!(parse_email("a@a.1-"), Err(EmailError::LabelHyphen { at: 5 }));
    assert_eq!(parse_email("a@1.000."), Err(EmailError::NumericTld { at: 4 }));
}
// Clauses: R4.
#[test]
fn hidden_non_ascii_before_length() {
    let s = format!("{}é@x.net", "x".repeat(70));
    assert_eq!(parse_email(&s), Err(EmailError::NonAscii { at: 70 }));
}
// Clauses: R13.
#[test]
fn hidden_earlier_labels_win() {
    let s = format!("a@-.{}.net", "x".repeat(64));
    assert_eq!(parse_email(&s), Err(EmailError::LabelHyphen { at: 2 }));
}
// Clauses: R7.
#[test]
fn hidden_missing_separator_before_local() {
    assert_eq!(parse_email(" bad.."), Err(EmailError::MissingAt));
    assert_eq!(parse_email(""), Err(EmailError::EmptyInput));
    assert_eq!(parse_email("@bad.."), Err(EmailError::EmptyLocal { at: 0 }));
}
