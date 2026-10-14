fn hidden_options(alphabet: Alphabet, padding: Padding, ignore_whitespace: bool) -> Options {
    Options { alphabet, padding, ignore_whitespace }
}

#[test]
fn hidden_url_padded_and_standard_unpadded() {
    let u = hidden_options(Alphabet::UrlSafe, Padding::Required, false);
    assert_eq!(encode(&[255], u), "_w==");
    assert_eq!(decode("_w==", u), Ok(vec![255]));
    let s = hidden_options(Alphabet::Standard, Padding::Forbidden, false);
    assert_eq!(encode(&[254, 253], s), "/v0");
    assert_eq!(decode("/v0", s), Ok(vec![254, 253]));
}

#[test]
fn hidden_alphabets_are_exclusive() {
    for p in [Padding::Required, Padding::Forbidden] {
        let s = hidden_options(Alphabet::Standard, p, false);
        let u = hidden_options(Alphabet::UrlSafe, p, false);
        assert_eq!(decode("____", s), Err(DecodeError::InvalidByte { offset: 0 }));
        assert_eq!(decode("----", s), Err(DecodeError::InvalidByte { offset: 0 }));
        assert_eq!(decode("////", u), Err(DecodeError::InvalidByte { offset: 0 }));
        assert_eq!(decode("++++", u), Err(DecodeError::InvalidByte { offset: 0 }));
        assert_eq!(decode("____", u), Ok(vec![255, 255, 255]));
        assert_eq!(decode("----", u), Ok(vec![251, 239, 190]));
    }
}

#[test]
fn hidden_forbidden_padding_even_if_structurally_valid() {
    let o = hidden_options(Alphabet::Standard, Padding::Forbidden, true);
    for (s, at) in [("YQ==", 2), ("YWI=", 3), ("YWJj=", 4), (" =", 1)] {
        assert_eq!(decode(s, o), Err(DecodeError::InvalidPadding { offset: at }));
    }
}

#[test]
fn hidden_required_incomplete_and_forbidden_single() {
    let r = hidden_options(Alphabet::Standard, Padding::Required, true);
    for s in ["B", "YQ", "YWI", "YQ=", "YQ= \n"] {
        assert_eq!(decode(s, r), Err(DecodeError::InvalidLength { offset: s.len() }));
    }
    let f = hidden_options(Alphabet::UrlSafe, Padding::Forbidden, true);
    assert_eq!(decode("\n B \t", f), Err(DecodeError::InvalidLength { offset: 5 }));
}

#[test]
fn hidden_canonical_one_byte_exhaustive() {
    let table = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    for v in 0..64 {
        for padding in [Padding::Required, Padding::Forbidden] {
            let o = hidden_options(Alphabet::Standard, padding, false);
            let mut s = format!("A{}", table[v] as char);
            if padding == Padding::Required { s += "=="; }
            let expected = if v & 15 == 0 { Ok(vec![(v >> 4) as u8]) }
                else { Err(DecodeError::NonCanonical { offset: 1 }) };
            assert_eq!(decode(&s, o), expected, "{s}");
        }
    }
}

#[test]
fn hidden_canonical_two_byte_exhaustive() {
    let table = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    for v in 0..64 {
        for padding in [Padding::Required, Padding::Forbidden] {
            let o = hidden_options(Alphabet::UrlSafe, padding, false);
            let mut s = format!("AA{}", table[v] as char);
            if padding == Padding::Required { s += "="; }
            let expected = if v & 3 == 0 { Ok(vec![0, (v >> 2) as u8]) }
                else { Err(DecodeError::NonCanonical { offset: 2 }) };
            assert_eq!(decode(&s, o), expected, "{s}");
        }
    }
}

#[test]
fn hidden_noncanonical_positions_include_whitespace() {
    let r = hidden_options(Alphabet::Standard, Padding::Required, true);
    assert_eq!(decode(" Y I = =", r), Err(DecodeError::NonCanonical { offset: 3 }));
    assert_eq!(decode(" Y W J =", r), Err(DecodeError::NonCanonical { offset: 5 }));
    let f = hidden_options(Alphabet::Standard, Padding::Forbidden, true);
    assert_eq!(decode(" Y I \n", f), Err(DecodeError::NonCanonical { offset: 3 }));
    assert_eq!(decode(" Y W J \t", f), Err(DecodeError::NonCanonical { offset: 5 }));
}

#[test]
fn hidden_noncanonical_padded_is_delayed_and_sticky() {
    let mut d = Decoder::new(hidden_options(Alphabet::Standard, Padding::Required, true));
    assert_eq!(d.push("YWJj "), Ok(b"abc".to_vec()));
    assert_eq!(d.push("YI="), Ok(vec![]));
    assert_eq!(d.push(" ="), Err(DecodeError::NonCanonical { offset: 6 }));
    assert_eq!(d.finish(), Err(DecodeError::NonCanonical { offset: 6 }));
    assert_eq!(d.push(""), Err(DecodeError::NonCanonical { offset: 6 }));
}

#[test]
fn hidden_unpadded_noncanonical_delayed_until_finish() {
    let mut d = Decoder::new(hidden_options(Alphabet::Standard, Padding::Forbidden, true));
    assert_eq!(d.push("YWJj "), Ok(b"abc".to_vec()));
    assert_eq!(d.push("YI \n"), Ok(vec![]));
    assert_eq!(d.finish(), Err(DecodeError::NonCanonical { offset: 6 }));
    assert_eq!(d.push("A"), Err(DecodeError::NonCanonical { offset: 6 }));
}

#[test]
fn hidden_structural_errors_before_canonical_check() {
    let r = hidden_options(Alphabet::Standard, Padding::Required, false);
    assert_eq!(decode("AI=?", r), Err(DecodeError::InvalidPadding { offset: 3 }));
    assert_eq!(decode("AI=", r), Err(DecodeError::InvalidLength { offset: 3 }));
    assert_eq!(decode("AAB?", r), Err(DecodeError::InvalidByte { offset: 3 }));
    let f = hidden_options(Alphabet::Standard, Padding::Forbidden, false);
    assert_eq!(decode("AI?", f), Err(DecodeError::InvalidByte { offset: 2 }));
    assert_eq!(decode("AI=", f), Err(DecodeError::InvalidPadding { offset: 2 }));
}

#[test]
fn hidden_trailing_data_precedence_and_whitespace() {
    let o = hidden_options(Alphabet::UrlSafe, Padding::Required, true);
    assert_eq!(decode("YQ== \u{b}\u{c}\r\n\t", o), Ok(b"a".to_vec()));
    for suffix in ["?", "=", "+", "A"] {
        assert_eq!(decode(&format!("YQ== \n{suffix}"), o), Err(DecodeError::TrailingData { offset: 6 }));
    }
    let strict = hidden_options(Alphabet::Standard, Padding::Required, false);
    assert_eq!(decode("YWI= ", strict), Err(DecodeError::TrailingData { offset: 4 }));
}

#[test]
fn hidden_eager_quartets_and_deferred_unpadded_tail() {
    let mut d = Decoder::new(hidden_options(Alphabet::Standard, Padding::Forbidden, false));
    assert_eq!(d.push("YWJjY"), Ok(b"abc".to_vec()));
    assert_eq!(d.push("Q"), Ok(vec![]));
    assert_eq!(d.push(""), Ok(vec![]));
    assert_eq!(d.finish(), Ok(b"a".to_vec()));
    let mut e = Encoder::new(hidden_options(Alphabet::Standard, Padding::Forbidden, false));
    assert_eq!(e.push(b"abcde"), Ok("YWJj".into()));
    assert_eq!(e.finish(), Ok("ZGU".into()));
}

#[test]
fn hidden_all_modes_all_split_points() {
    let raw: Vec<u8> = (0..41).map(|i| (i * 61 + 11) as u8).collect();
    for a in [Alphabet::Standard, Alphabet::UrlSafe] {
        for p in [Padding::Required, Padding::Forbidden] {
            let o = hidden_options(a, p, false);
            let encoded = encode(&raw, o);
            for cut in 0..=raw.len() {
                let mut e = Encoder::new(o);
                let mut out = e.push(&raw[..cut]).unwrap();
                out += &e.push(&raw[cut..]).unwrap(); out += &e.finish().unwrap();
                assert_eq!(out, encoded);
            }
            for cut in 0..=encoded.len() {
                let mut d = Decoder::new(o);
                let mut out = d.push(&encoded[..cut]).unwrap();
                out.extend(d.push(&encoded[cut..]).unwrap()); out.extend(d.finish().unwrap());
                assert_eq!(out, raw);
            }
        }
    }
}

#[test]
fn hidden_raw_offsets_in_url_stream() {
    let mut d = Decoder::new(hidden_options(Alphabet::UrlSafe, Padding::Forbidden, true));
    assert_eq!(d.push(" \t_"), Ok(vec![]));
    assert_eq!(d.push("\n"), Ok(vec![]));
    assert_eq!(d.push("+"), Err(DecodeError::InvalidByte { offset: 4 }));
    assert_eq!(d.finish(), Err(DecodeError::InvalidByte { offset: 4 }));
}

#[test]
fn hidden_unicode_not_ascii_whitespace() {
    let o = hidden_options(Alphabet::Standard, Padding::Forbidden, true);
    assert_eq!(decode("YW\u{2003}I", o), Err(DecodeError::InvalidByte { offset: 2 }));
    assert_eq!(decode("YW\0I", o), Err(DecodeError::InvalidByte { offset: 2 }));
}

#[test]
fn hidden_finish_seals_both_modes() {
    for p in [Padding::Required, Padding::Forbidden] {
        let o = hidden_options(Alphabet::UrlSafe, p, true);
        let mut d = Decoder::new(o);
        assert_eq!(d.push("YWJj"), Ok(b"abc".to_vec()));
        assert_eq!(d.finish(), Ok(vec![]));
        assert_eq!(d.push(" \n"), Err(DecodeError::Finished));
        assert_eq!(d.finish(), Err(DecodeError::Finished));
        let mut e = Encoder::new(o);
        assert_eq!(e.finish(), Ok("".into()));
        assert_eq!(e.push(&[]), Err(StreamError::Finished));
        assert_eq!(e.finish(), Err(StreamError::Finished));
    }
}

#[test]
fn hidden_empty_whitespace_and_lengths() {
    for p in [Padding::Required, Padding::Forbidden] {
        let o = hidden_options(Alphabet::Standard, p, true);
        assert_eq!(decode("\u{b}\u{c}\t\r\n ", o), Ok(vec![]));
        assert_eq!(encode(&[], o), "");
        for n in 0usize..20 {
            let raw = vec![255; n];
            let encoded = encode(&raw, o);
            assert_eq!(encoded.len(), if p == Padding::Required { n.div_ceil(3)*4 } else { (n*8).div_ceil(6) });
            assert_eq!(decode(&encoded, o), Ok(raw));
        }
    }
}

#[test]
fn hidden_all_byte_values_chunked_all_modes() {
    let raw: Vec<u8> = (0..=255).collect();
    for a in [Alphabet::Standard, Alphabet::UrlSafe] {
        for p in [Padding::Required, Padding::Forbidden] {
            let o = hidden_options(a, p, true);
            let mut e = Encoder::new(o);
            let mut text = String::new();
            for c in raw.chunks(5) { text += &e.push(c).unwrap(); }
            text += &e.finish().unwrap();
            let spaced: String = text.chars().flat_map(|c| [c, '\u{b}', '\u{c}']).collect();
            let mut d = Decoder::new(o);
            let mut out = Vec::new();
            for c in spaced.as_bytes().chunks(11) { out.extend(d.push(std::str::from_utf8(c).unwrap()).unwrap()); }
            out.extend(d.finish().unwrap());
            assert_eq!(out, raw);
            assert_eq!(text, encode(&raw, o));
        }
    }
}

#[test]
fn hidden_two_byte_tail_bit_error_across_pushes() {
    let mut d = Decoder::new(hidden_options(Alphabet::UrlSafe, Padding::Forbidden, true));
    assert_eq!(d.push("YWJj Y"), Ok(b"abc".to_vec()));
    assert_eq!(d.push("W\tJ\n"), Ok(vec![]));
    assert_eq!(d.finish(), Err(DecodeError::NonCanonical { offset: 8 }));
    assert_eq!(d.finish(), Err(DecodeError::NonCanonical { offset: 8 }));
}

#[test]
fn hidden_invalid_padding_sticky_across_chunks() {
    let mut d = Decoder::new(hidden_options(Alphabet::Standard, Padding::Required, true));
    assert_eq!(d.push(" YQ="), Ok(vec![]));
    assert_eq!(d.push(" \nX"), Err(DecodeError::InvalidPadding { offset: 6 }));
    assert_eq!(d.finish(), Err(DecodeError::InvalidPadding { offset: 6 }));
    assert_eq!(d.push("="), Err(DecodeError::InvalidPadding { offset: 6 }));
}
