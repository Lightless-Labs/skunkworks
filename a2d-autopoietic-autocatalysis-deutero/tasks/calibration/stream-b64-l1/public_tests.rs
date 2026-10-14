#[test]
fn public_rfc_vectors() {
    for (raw, encoded) in [("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foobar", "Zm9vYmFy")] {
        assert_eq!(encode(raw.as_bytes()), encoded);
        assert_eq!(decode(encoded), Ok(raw.as_bytes().to_vec()));
    }
}

#[test]
fn public_binary() {
    assert_eq!(encode(&[0, 255, 16, 128]), "AP8QgA==");
    assert_eq!(decode("AP8QgA=="), Ok(vec![0, 255, 16, 128]));
}

#[test]
fn public_empty() {
    assert_eq!(encode(&[]), "");
    assert_eq!(decode(""), Ok(vec![]));
}

#[test]
fn public_malformed() {
    assert_eq!(decode("?AAA"), Err(DecodeError::InvalidByte));
    assert_eq!(decode("AA=A"), Err(DecodeError::InvalidPadding));
}
