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
// R17-R18, R23
#[test]
fn public_encoder_chunks() {
    let mut e = Encoder::new();
    assert_eq!(e.push(b"C").unwrap(), "");
    assert_eq!(e.push(b"at").unwrap(), "Q2F0");
    assert_eq!(e.finish().unwrap(), "");
    assert_eq!(e.finish(), Err(Error::AlreadyFinished));
}
// R17, R19-R20, R23
#[test]
fn public_decoder_chunks() {
    let mut d = Decoder::new();
    assert_eq!(d.push(b"T2").unwrap(), Vec::<u8>::new());
    assert_eq!(d.push(b"s=").unwrap(), b"Ok");
    assert_eq!(d.finish().unwrap(), Vec::<u8>::new());
}
