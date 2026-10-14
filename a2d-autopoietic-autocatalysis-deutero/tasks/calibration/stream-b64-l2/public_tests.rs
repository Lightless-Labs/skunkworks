#[test]
fn public_one_shot() {
    assert_eq!(encode(b"foobar"), "Zm9vYmFy");
    assert_eq!(decode("Zm9vYmFy"), Ok(b"foobar".to_vec()));
    assert_eq!(decode("!"), Err(DecodeError::InvalidByte));
}

#[test]
fn public_encoder_chunks() {
    let mut e = Encoder::new();
    assert_eq!(e.push(b"f"), Ok("".into()));
    assert_eq!(e.push(b"oo"), Ok("Zm9v".into()));
    assert_eq!(e.push(b"ba"), Ok("".into()));
    assert_eq!(e.push(b"r"), Ok("YmFy".into()));
    assert_eq!(e.finish(), Ok("".into()));
}

#[test]
fn public_decoder_chunks() {
    let mut d = Decoder::new();
    assert_eq!(d.push("Z"), Ok(vec![]));
    assert_eq!(d.push("m9"), Ok(vec![]));
    assert_eq!(d.push("vY"), Ok(b"foo".to_vec()));
    assert_eq!(d.push("mFy"), Ok(b"bar".to_vec()));
    assert_eq!(d.finish(), Ok(vec![]));
}

#[test]
fn public_finished_streams() {
    let mut e = Encoder::new();
    assert_eq!(e.finish(), Ok("".into()));
    assert_eq!(e.push(b""), Err(StreamError::Finished));
    let mut d = Decoder::new();
    assert_eq!(d.finish(), Ok(vec![]));
    assert_eq!(d.finish(), Err(DecodeError::Finished));
}
