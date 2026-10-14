#[test]
fn hidden_all_two_way_splits() {
    let raw: Vec<u8> = (0..73).map(|i| (i * 37) as u8).collect();
    let expected = encode(&raw);
    for cut in 0..=raw.len() {
        let mut e = Encoder::new();
        let mut out = e.push(&raw[..cut]).unwrap();
        out += &e.push(&raw[cut..]).unwrap();
        out += &e.finish().unwrap();
        assert_eq!(out, expected);
    }
    for cut in 0..=expected.len() {
        let mut d = Decoder::new();
        let mut out = d.push(&expected[..cut]).unwrap();
        out.extend(d.push(&expected[cut..]).unwrap());
        out.extend(d.finish().unwrap());
        assert_eq!(out, raw);
    }
}

#[test]
fn hidden_byte_at_a_time() {
    let raw = b"arbitrary split positions";
    let mut e = Encoder::new();
    let mut encoded = String::new();
    for b in raw { encoded += &e.push(&[*b]).unwrap(); }
    encoded += &e.finish().unwrap();
    assert_eq!(encoded, encode(raw));
    let mut d = Decoder::new();
    let mut decoded = Vec::new();
    for b in encoded.bytes() { decoded.extend(d.push(&(b as char).to_string()).unwrap()); }
    decoded.extend(d.finish().unwrap());
    assert_eq!(decoded, raw);
}

#[test]
fn hidden_eager_encoder_output() {
    let mut e = Encoder::new();
    assert_eq!(e.push(b"abcde"), Ok("YWJj".into()));
    assert_eq!(e.push(b"fg"), Ok("ZGVm".into()));
    assert_eq!(e.finish(), Ok("Zw==".into()));
}

#[test]
fn hidden_eager_decoder_output() {
    let mut d = Decoder::new();
    assert_eq!(d.push("YWJjZ"), Ok(b"abc".to_vec()));
    assert_eq!(d.push("GVmZw=="), Ok(b"defg".to_vec()));
    assert_eq!(d.finish(), Ok(vec![]));
}

#[test]
fn hidden_empty_push_preserves_encoder_tail() {
    let mut e = Encoder::new();
    assert_eq!(e.push(&[252]), Ok("".into()));
    assert_eq!(e.push(&[]), Ok("".into()));
    assert_eq!(e.finish(), Ok("/A==".into()));
}

#[test]
fn hidden_empty_push_preserves_decoder_tail() {
    let mut d = Decoder::new();
    assert_eq!(d.push("YWI"), Ok(vec![]));
    assert_eq!(d.push(""), Ok(vec![]));
    assert_eq!(d.push("="), Ok(b"ab".to_vec()));
}

#[test]
fn hidden_split_two_padding_characters() {
    let mut d = Decoder::new();
    assert_eq!(d.push("YQ="), Ok(vec![]));
    assert_eq!(d.push("="), Ok(b"a".to_vec()));
    assert_eq!(d.push(""), Ok(vec![]));
    assert_eq!(d.finish(), Ok(vec![]));
}

#[test]
fn hidden_padding_does_not_finish_stream() {
    let mut d = Decoder::new();
    assert_eq!(d.push("YWI="), Ok(b"ab".to_vec()));
    assert_eq!(d.push("X"), Err(DecodeError::InvalidPadding));
    assert_eq!(d.finish(), Err(DecodeError::InvalidPadding));
}

#[test]
fn hidden_reject_incomplete_at_finish() {
    for s in ["B", "BB", "BBB", "BB="] {
        let mut d = Decoder::new();
        assert_eq!(d.push(s), Ok(vec![]));
        assert_eq!(d.finish(), Err(DecodeError::InvalidLength));
        assert_eq!(d.push("A"), Err(DecodeError::InvalidLength));
        assert_eq!(decode(s), Err(DecodeError::InvalidLength));
    }
}

#[test]
fn hidden_invalid_byte_sticky() {
    let mut d = Decoder::new();
    assert_eq!(d.push("YWJj?"), Err(DecodeError::InvalidByte));
    assert_eq!(d.push("===="), Err(DecodeError::InvalidByte));
    assert_eq!(d.finish(), Err(DecodeError::InvalidByte));
}

#[test]
fn hidden_expected_second_padding_sticky() {
    let mut d = Decoder::new();
    assert_eq!(d.push("YR="), Ok(vec![]));
    assert_eq!(d.push("?"), Err(DecodeError::InvalidPadding));
    assert_eq!(d.push("="), Err(DecodeError::InvalidPadding));
}

#[test]
fn hidden_encoder_repeated_finish() {
    let mut e = Encoder::new();
    e.push(b"ab").unwrap();
    assert_eq!(e.finish(), Ok("YWI=".into()));
    assert_eq!(e.finish(), Err(StreamError::Finished));
    assert_eq!(e.push(b"x"), Err(StreamError::Finished));
}

#[test]
fn hidden_decoder_after_successful_finish() {
    let mut d = Decoder::new();
    assert_eq!(d.push("YWJj"), Ok(b"abc".to_vec()));
    assert_eq!(d.finish(), Ok(vec![]));
    assert_eq!(d.push(""), Err(DecodeError::Finished));
    assert_eq!(d.push("?"), Err(DecodeError::Finished));
}

#[test]
fn hidden_strict_whitespace() {
    for s in ["Y WJj", "YWJj\n", "YW\tJj", "YWI= "] {
        let expected = if s == "YWI= " { DecodeError::InvalidPadding } else { DecodeError::InvalidByte };
        assert_eq!(decode(s), Err(expected));
    }
}

#[test]
fn hidden_permissive_padding_bits() {
    assert_eq!(decode("YR=="), Ok(b"a".to_vec()));
    assert_eq!(decode("YWJ="), Ok(b"ab".to_vec()));
}

#[test]
fn hidden_padding_position_errors() {
    for s in ["=BBB", "B=BB", "BB=B", "BBBB===="] {
        assert_eq!(decode(s), Err(DecodeError::InvalidPadding));
    }
}

#[test]
fn hidden_non_ascii_byte() {
    let mut d = Decoder::new();
    assert_eq!(d.push("BBé"), Err(DecodeError::InvalidByte));
}

#[test]
fn hidden_padded_data_after_padding() {
    assert_eq!(decode("YQ==YWI="), Err(DecodeError::InvalidPadding));
    assert_eq!(decode("YQ==?"), Err(DecodeError::InvalidPadding));
}

#[test]
fn hidden_large_irregular_chunks() {
    let raw: Vec<u8> = (0..2048).map(|i| (i * 103 + 19) as u8).collect();
    let mut e = Encoder::new();
    let mut encoded = String::new();
    for c in raw.chunks(17) { encoded += &e.push(c).unwrap(); }
    encoded += &e.finish().unwrap();
    let mut d = Decoder::new();
    let mut out = Vec::new();
    for c in encoded.as_bytes().chunks(13) { out.extend(d.push(std::str::from_utf8(c).unwrap()).unwrap()); }
    out.extend(d.finish().unwrap());
    assert_eq!(out, raw);
    assert_eq!(encoded, encode(&raw));
}

#[test]
fn hidden_zero_input_and_independent_streams() {
    let mut a = Encoder::new();
    let mut b = Encoder::new();
    a.push(b"x").unwrap(); b.push(b"yz").unwrap();
    assert_eq!(a.finish(), Ok("eA==".into()));
    assert_eq!(b.finish(), Ok("eXo=".into()));
    assert_eq!(decode(""), Ok(vec![]));
}
