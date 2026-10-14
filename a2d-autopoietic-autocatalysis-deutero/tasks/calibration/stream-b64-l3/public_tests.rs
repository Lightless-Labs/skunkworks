#[test]
fn public_whitespace_one_shot() {
    assert_eq!(encode(b"foobar"), "Zm9vYmFy");
    assert_eq!(decode(" Zm9v\nYmFy\t", true), Ok(b"foobar".to_vec()));
}

#[test]
fn public_invalid_offset() {
    assert_eq!(decode("Zm?v", false), Err(DecodeError::InvalidByte { offset: 2 }));
}

#[test]
fn public_encoder_stream() {
    let mut e = Encoder::new();
    assert_eq!(e.push(b"f"), Ok("".into()));
    assert_eq!(e.push(b"oo"), Ok("Zm9v".into()));
    assert_eq!(e.finish(), Ok("".into()));
    assert_eq!(e.finish(), Err(StreamError::Finished));
}

#[test]
fn public_decoder_stream() {
    let mut d = Decoder::new(true);
    assert_eq!(d.push(" Zg="), Ok(vec![]));
    assert_eq!(d.push(" =\n"), Ok(b"f".to_vec()));
    assert_eq!(d.finish(), Ok(vec![]));
    assert_eq!(d.push(""), Err(DecodeError::Finished));
}
