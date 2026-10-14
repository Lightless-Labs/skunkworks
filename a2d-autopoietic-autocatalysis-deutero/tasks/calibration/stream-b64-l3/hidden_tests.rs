#[test]
fn hidden_all_six_whitespace_bytes() {
    assert_eq!(decode("\u{b}Y\u{c}W\rJ\nj\t ", true), Ok(b"abc".to_vec()));
    assert_eq!(decode(" \t\n\r\u{b}\u{c}", true), Ok(vec![]));
}

#[test]
fn hidden_strict_whitespace_offsets() {
    for w in [' ', '\t', '\n', '\r', '\u{b}', '\u{c}'] {
        assert_eq!(decode(&format!("YW{w}Jj"), false), Err(DecodeError::InvalidByte { offset: 2 }));
    }
}

#[test]
fn hidden_raw_offsets_across_skipped_chunks() {
    let mut d = Decoder::new(true);
    assert_eq!(d.push(" \nY"), Ok(vec![]));
    assert_eq!(d.push("\tW"), Ok(vec![]));
    assert_eq!(d.push("?"), Err(DecodeError::InvalidByte { offset: 5 }));
    assert_eq!(d.finish(), Err(DecodeError::InvalidByte { offset: 5 }));
}

#[test]
fn hidden_non_ascii_whitespace_byte_offset() {
    assert_eq!(decode(" \tY\u{a0}WJj", true), Err(DecodeError::InvalidByte { offset: 3 }));
    assert_eq!(decode("YWé", false), Err(DecodeError::InvalidByte { offset: 2 }));
}

#[test]
fn hidden_eof_is_raw_length() {
    let mut d = Decoder::new(true);
    assert_eq!(d.push(" \tY"), Ok(vec![]));
    assert_eq!(d.push("W\n\r"), Ok(vec![]));
    assert_eq!(d.finish(), Err(DecodeError::InvalidLength { offset: 6 }));
    assert_eq!(d.push("Jj"), Err(DecodeError::InvalidLength { offset: 6 }));
}

#[test]
fn hidden_incomplete_padding_eof() {
    assert_eq!(decode("YQ= \n", true), Err(DecodeError::InvalidLength { offset: 5 }));
    assert_eq!(decode("YWI", false), Err(DecodeError::InvalidLength { offset: 3 }));
}

#[test]
fn hidden_misplaced_padding_positions() {
    assert_eq!(decode(" =AAA", true), Err(DecodeError::InvalidPadding { offset: 1 }));
    assert_eq!(decode("Y =AA", true), Err(DecodeError::InvalidPadding { offset: 2 }));
}

#[test]
fn hidden_expected_padding_error_precedence() {
    assert_eq!(decode("YQ= ?", true), Err(DecodeError::InvalidPadding { offset: 4 }));
    assert_eq!(decode("YQ=A", false), Err(DecodeError::InvalidPadding { offset: 3 }));
}

#[test]
fn hidden_padding_split_with_whitespace() {
    let mut d = Decoder::new(true);
    assert_eq!(d.push("Y \tQ="), Ok(vec![]));
    assert_eq!(d.push("\u{b}\u{c}"), Ok(vec![]));
    assert_eq!(d.push("="), Ok(b"a".to_vec()));
    assert_eq!(d.push("\r\n"), Ok(vec![]));
    assert_eq!(d.finish(), Ok(vec![]));
}

#[test]
fn hidden_trailing_data_after_skipped_whitespace() {
    let mut d = Decoder::new(true);
    assert_eq!(d.push("YWI="), Ok(b"ab".to_vec()));
    assert_eq!(d.push(" \n"), Ok(vec![]));
    assert_eq!(d.push("?"), Err(DecodeError::TrailingData { offset: 6 }));
    assert_eq!(d.push("="), Err(DecodeError::TrailingData { offset: 6 }));
}

#[test]
fn hidden_trailing_error_precedence() {
    for suffix in ["=", "?", "A", "é"] {
        assert_eq!(decode(&format!("YQ=={suffix}"), false), Err(DecodeError::TrailingData { offset: 4 }));
    }
    assert_eq!(decode("YWI= ", false), Err(DecodeError::TrailingData { offset: 4 }));
}

#[test]
fn hidden_first_error_sticks() {
    let mut d = Decoder::new(false);
    assert_eq!(d.push("YWJj"), Ok(b"abc".to_vec()));
    assert_eq!(d.push("_"), Err(DecodeError::InvalidByte { offset: 4 }));
    assert_eq!(d.push(""), Err(DecodeError::InvalidByte { offset: 4 }));
    assert_eq!(d.finish(), Err(DecodeError::InvalidByte { offset: 4 }));
}

#[test]
fn hidden_all_split_points_with_whitespace() {
    let text = " YW Jj\nZG\tU=\r";
    for cut in 0..=text.len() {
        let mut d = Decoder::new(true);
        let mut out = d.push(&text[..cut]).unwrap();
        out.extend(d.push(&text[cut..]).unwrap());
        out.extend(d.finish().unwrap());
        assert_eq!(out, b"abcde");
    }
}

#[test]
fn hidden_error_offsets_all_split_points() {
    let text = " YW\nJj  ?";
    for cut in 0..=text.len() {
        let mut d = Decoder::new(true);
        let a = d.push(&text[..cut]);
        let b = d.push(&text[cut..]);
        let expected = DecodeError::InvalidByte { offset: 8 };
        if a.is_err() { assert_eq!(a, Err(expected)); }
        assert_eq!(b, Err(expected));
        assert_eq!(d.finish(), Err(expected));
    }
}

#[test]
fn hidden_permissive_final_bits() {
    assert_eq!(decode("Y R = =", true), Ok(b"a".to_vec()));
    assert_eq!(decode("Y W J =", true), Ok(b"ab".to_vec()));
}

#[test]
fn hidden_encoder_tail_lifecycle() {
    let mut e = Encoder::new();
    assert_eq!(e.push(b"abcde"), Ok("YWJj".into()));
    assert_eq!(e.push(b""), Ok("".into()));
    assert_eq!(e.finish(), Ok("ZGU=".into()));
    assert_eq!(e.push(b""), Err(StreamError::Finished));
}

#[test]
fn hidden_decoder_success_seals() {
    let mut d = Decoder::new(true);
    assert_eq!(d.push(" \n"), Ok(vec![]));
    assert_eq!(d.finish(), Ok(vec![]));
    assert_eq!(d.finish(), Err(DecodeError::Finished));
    assert_eq!(d.push(" \n"), Err(DecodeError::Finished));
}

#[test]
fn hidden_large_whitespace_stream() {
    let raw: Vec<u8> = (0..3073).map(|i| (i * 47 + 9) as u8).collect();
    let text: String = encode(&raw).chars().flat_map(|c| [c, '\n']).collect();
    let mut d = Decoder::new(true);
    let mut out = Vec::new();
    for c in text.as_bytes().chunks(7) { out.extend(d.push(std::str::from_utf8(c).unwrap()).unwrap()); }
    out.extend(d.finish().unwrap());
    assert_eq!(out, raw);
}

#[test]
fn hidden_eager_multiple_quartets() {
    let mut d = Decoder::new(false);
    assert_eq!(d.push("YWJjZGVmY"), Ok(b"abcdef".to_vec()));
    assert_eq!(d.push("Q=="), Ok(b"a".to_vec()));
    assert_eq!(d.finish(), Ok(vec![]));
}

#[test]
fn hidden_invalid_before_length_and_padding() {
    assert_eq!(decode("Y?", true), Err(DecodeError::InvalidByte { offset: 1 }));
    assert_eq!(decode("YWJ?=", false), Err(DecodeError::InvalidByte { offset: 3 }));
    assert_eq!(decode("YWJj====", false), Err(DecodeError::InvalidPadding { offset: 4 }));
}
