// Clauses: R9.
#[test]
fn public_atom_address() {
    assert_eq!(parse_email("Ada.Lovelace@Mail.Example"), Ok(Address { local: "Ada.Lovelace".into(), domain: "mail.example".into(), quoted: false }));
}
// Clauses: R12.
#[test]
fn public_quoted_local() {
    assert_eq!(parse_email("\"Ada Lovelace\\@home\"@Example.COM"), Ok(Address { local: "Ada Lovelace@home".into(), domain: "example.com".into(), quoted: true }));
}
// Clauses: R31, R34.
#[test]
fn public_normalize_and_format() {
    let p = Policy { local_case: LocalCase::Lower, strip_subaddress: true, unquote_safe: true, allow_single_label: false };
    let a = normalize_email("(name) \"Ada+news\"@Example.COM", &p).unwrap();
    assert_eq!(a, Address { local: "ada".into(), domain: "example.com".into(), quoted: false });
    assert_eq!(format_email(&a), "ada@example.com");
}
// Clauses: R35, R37.
#[test]
fn public_dedup() {
    let r = deduplicate(&["one@example.com", "one@EXAMPLE.COM", "two@example.com", "invalid"], &Policy::default());
    assert_eq!(r.source_indices, vec![0, 2]);
    assert_eq!(r.addresses.len(), 2);
    assert_eq!(r.duplicate_of, vec![None, Some(0), None, None]);
    assert_eq!(r.errors, vec![(3, EmailError::MissingAt)]);
}
