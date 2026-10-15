// Clauses: R9.
#[test]
fn public_atom_address() {
    assert_eq!(parse_email("Ada.Lovelace@Mail.Example"), Ok(Address { local: "Ada.Lovelace".into(), domain: "mail.example".into(), quoted: false }));
}
// Clauses: R15.
#[test]
fn public_domain_label() {
    assert_eq!(parse_email("info@post-box.example"), Ok(Address { local: "info".into(), domain: "post-box.example".into(), quoted: false }));
}
// Clauses: R7.
#[test]
fn public_separator_error() {
    assert_eq!(parse_email("missing.example"), Err(EmailError::MissingAt));
}
