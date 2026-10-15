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
// Clauses: R11.
#[test]
fn public_quoted_escape_error() {
    assert_eq!(parse_email("\"a\\z\"@mail.example"), Err(EmailError::BadEscape { at: 2 }));
}
