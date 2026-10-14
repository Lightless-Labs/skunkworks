fn dec(b: &[u8]) -> Result<Vec<u8>, Error> {
    decode(b)
}
fn enc(b: &[u8]) -> String {
    encode(b)
}
// R1-R4, R14
#[test]
fn public_encode() {
    assert_eq!(enc(b"Cat"), "Q2F0");
    assert_eq!(enc(b""), "");
}
// R2, R5, R14-R15
#[test]
fn public_decode() {
    assert_eq!(dec(b"T2s="), Ok(b"Ok".to_vec()));
    assert_eq!(dec(b"YWJj"), Ok(b"abc".to_vec()));
}
// R9, R12, R16
#[test]
fn public_bad_character() {
    assert_eq!(
        dec(b"Q?=="),
        Err(Error::InvalidByte {
            offset: 1,
            byte: b'?'
        })
    );
}
