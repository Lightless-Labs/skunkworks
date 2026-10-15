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
// Clauses: R22.
#[test]
fn public_comments_and_fold() {
    assert_eq!(parse_email("(name) User (note) @\r\n\tMail.Example (end)"), Ok(Address { local: "User".into(), domain: "mail.example".into(), quoted: false }));
}
// Clauses: R27.
#[test]
fn public_boundary_error() {
    assert_eq!(parse_email("first last@mail.example"), Err(EmailError::CfwsBoundary { at: 5 }));
}
