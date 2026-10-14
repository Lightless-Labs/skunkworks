#[test]
fn hidden_other_vectors() {
    assert_eq!(encode(b"light"), "bGlnaHQ=");
    assert_eq!(decode("c3RyZWFt"), Ok(b"stream".to_vec()));
}

#[test]
fn hidden_one_tail() {
    assert_eq!(encode(&[255]), "/w==");
    assert_eq!(decode("/w=="), Ok(vec![255]));
}

#[test]
fn hidden_two_tail() {
    assert_eq!(encode(&[254, 253]), "/v0=");
    assert_eq!(decode("/v0="), Ok(vec![254, 253]));
}

#[test]
fn hidden_high_sextets() {
    assert_eq!(decode("++++"), Ok(vec![251, 239, 190]));
    assert_eq!(decode("////"), Ok(vec![255, 255, 255]));
}

#[test]
fn hidden_short_lengths() {
    for input in ["A", "AA", "AAA", "AA="] {
        assert_eq!(decode(input), Err(DecodeError::InvalidLength), "{input}");
    }
}

#[test]
fn hidden_first_padding() {
    for input in ["=", "=AAA", "A=AA"] {
        assert_eq!(decode(input), Err(DecodeError::InvalidPadding));
    }
}

#[test]
fn hidden_second_padding_expected() {
    for input in ["YR=A", "YR=?", "YR= "] {
        assert_eq!(decode(input), Err(DecodeError::InvalidPadding));
    }
}

#[test]
fn hidden_after_one_byte_padding() {
    for input in ["YQ==AAAA", "YQ===", "YQ==?"] {
        assert_eq!(decode(input), Err(DecodeError::InvalidPadding));
    }
}

#[test]
fn hidden_after_two_byte_padding() {
    assert_eq!(decode("YWI=AAAA"), Err(DecodeError::InvalidPadding));
}

#[test]
fn hidden_bad_byte_before_eof() {
    assert_eq!(decode("A?"), Err(DecodeError::InvalidByte));
    assert_eq!(decode("AAA_"), Err(DecodeError::InvalidByte));
}

#[test]
fn hidden_whitespace_is_data() {
    for input in [" AAAB", "AA\tB", "AA\nB", "AA\rB", "AA\u{b}B", "AA\u{c}B"] {
        assert_eq!(decode(input), Err(DecodeError::InvalidByte));
    }
}

#[test]
fn hidden_unicode_rejected() {
    assert_eq!(decode("AAéA"), Err(DecodeError::InvalidByte));
}

#[test]
fn hidden_permissive_one_byte_bits() {
    assert_eq!(decode("YR=="), Ok(b"a".to_vec()));
    assert_eq!(decode("//=="), Ok(vec![255]));
}

#[test]
fn hidden_permissive_two_byte_bits() {
    assert_eq!(decode("YWJ="), Ok(b"ab".to_vec()));
}

#[test]
fn hidden_zero_bytes() {
    assert_eq!(encode(&[0, 0, 1, 0]), "AAABAA==");
    assert_eq!(decode("AAABAA=="), Ok(vec![0, 0, 1, 0]));
}

#[test]
fn hidden_all_bytes_roundtrip() {
    let data: Vec<u8> = (0..=255).collect();
    assert_eq!(decode(&encode(&data)), Ok(data));
}

#[test]
fn hidden_tail_boundaries() {
    for n in 1usize..=80 {
        let data: Vec<u8> = (0..n).map(|i| (i * 71 + 23) as u8).collect();
        let encoded = encode(&data);
        assert_eq!(encoded.len(), n.div_ceil(3) * 4);
        assert_eq!(decode(&encoded), Ok(data));
    }
}

#[test]
fn hidden_large_complete_groups() {
    assert_eq!(decode(&"QUJD".repeat(1024)), Ok(b"ABC".repeat(1024)));
}

#[test]
fn hidden_each_illegal_ascii() {
    for byte in 0u8..128 {
        if b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=".contains(&byte) { continue; }
        let input = format!("AAA{}", byte as char);
        assert_eq!(decode(&input), Err(DecodeError::InvalidByte), "byte {byte}");
    }
}

#[test]
fn hidden_interior_padding_precedence() {
    assert_eq!(decode("QUJDYQ==YWI="), Err(DecodeError::InvalidPadding));
    assert_eq!(decode("QUJ?="), Err(DecodeError::InvalidByte));
}
