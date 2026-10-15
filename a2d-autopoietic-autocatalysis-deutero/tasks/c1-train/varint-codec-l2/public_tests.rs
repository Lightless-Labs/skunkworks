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

// R19-R21
#[test]
fn public_signed() {
    for v in [-300,-1,0,1,300] {
        let b=encode_i64(v); assert_eq!(decode_i64(&b,0),Ok((v,b.len())));
    }
    assert_eq!(encode_i64(-1),vec![1]);
}

// R22-R25, R28-R29
#[test]
fn public_stream() {
    let mut s=StreamDecoder::new();
    assert_eq!(s.push(&[133]),Ok(vec![]));
    assert_eq!(s.pending_len(),1); assert_eq!(s.position(),1);
    assert_eq!(s.push(&[1,4]),Ok(vec![133,4]));
    assert_eq!(s.finish(),Ok(()));
    s.reset(); assert_eq!(s.position(),0);
}
