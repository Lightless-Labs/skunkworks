// R1-R6, R9
#[test]
fn public_unsigned_roundtrip() {
    for value in [2, 17, 126, 129, 12345] {
        let bytes = encode_u64(value);
        assert_eq!(decode_u64(&bytes, 0), Ok((value, bytes.len())));
    }
    assert_eq!(encode_u64(12345), vec![0xb9, 0x60]);
}

// R2-R3, R7, R14, R18
#[test]
fn public_cursor_and_batch() {
    assert_eq!(decode_u64(&[9, 0xac, 2, 7], 1), Ok((300, 3)));
    assert_eq!(decode_many(&[1, 128, 1, 8], 0, 2), Ok((vec![1, 128], 3)));
}

// R8, R10, R16-R17
#[test]
fn public_truncation() {
    assert_eq!(decode_u64(&[77,130,129], 1), Err(Error::Truncated { offset: 3, start: 1 }));
    assert_eq!(decode_many(&[55], 0, 0), Ok((vec![], 0)));
}
