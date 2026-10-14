fn public_options(alphabet: Alphabet, padding: Padding, ignore_whitespace: bool) -> Options {
    Options { alphabet, padding, ignore_whitespace }
}

#[test]
fn public_configured_one_shot() {
    let standard = public_options(Alphabet::Standard, Padding::Required, false);
    assert_eq!(encode(b"f", standard), "Zg==");
    assert_eq!(decode("Zg==", standard), Ok(b"f".to_vec()));
    let url = public_options(Alphabet::UrlSafe, Padding::Forbidden, false);
    assert_eq!(encode(&[251, 255], url), "-_8");
    assert_eq!(decode("-_8", url), Ok(vec![251, 255]));
}

#[test]
fn public_canonical_error() {
    let o = public_options(Alphabet::Standard, Padding::Required, false);
    assert_eq!(decode("Zh==", o), Err(DecodeError::NonCanonical { offset: 1 }));
    assert_eq!(decode("?AAA", o), Err(DecodeError::InvalidByte { offset: 0 }));
}

#[test]
fn public_configured_encoder() {
    let mut e = Encoder::new(public_options(Alphabet::UrlSafe, Padding::Forbidden, false));
    assert_eq!(e.push(&[251]), Ok("".into()));
    assert_eq!(e.push(&[255]), Ok("".into()));
    assert_eq!(e.finish(), Ok("-_8".into()));
    assert_eq!(e.finish(), Err(StreamError::Finished));
}

#[test]
fn public_configured_decoder() {
    let mut d = Decoder::new(public_options(Alphabet::UrlSafe, Padding::Forbidden, true));
    assert_eq!(d.push(" -"), Ok(vec![]));
    assert_eq!(d.push("_8\n"), Ok(vec![]));
    assert_eq!(d.finish(), Ok(vec![251, 255]));
    assert_eq!(d.push(""), Err(DecodeError::Finished));
}
