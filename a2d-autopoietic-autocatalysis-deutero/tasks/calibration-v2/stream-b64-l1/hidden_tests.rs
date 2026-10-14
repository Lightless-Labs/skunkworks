fn dec(b: &[u8]) -> Result<Vec<u8>, Error> {
    decode(b)
}
fn enc(b: &[u8]) -> String {
    encode(b)
}
// R1-R5, R14-R15
#[test]
fn hidden_empty_and_binary() {
    assert_eq!(enc(&[]), "");
    assert_eq!(dec(&[]), Ok(vec![]));
    assert_eq!(enc(&[0, 128, 255]), "AID/");
    assert_eq!(dec(b"AID/"), Ok(vec![0, 128, 255]));
}

// R3-R4
#[test]
fn hidden_tail_encoding() {
    assert_eq!(enc(b"x"), "eA==");
    assert_eq!(enc(b"xy"), "eHk=");
    assert_eq!(enc(&[255]), "/w==");
    assert_eq!(enc(&[255, 254]), "//4=");
}

// R2-R5, R14
#[test]
fn hidden_all_digits() {
    assert_eq!(enc(&[251, 239, 255]), "++//");
    assert_eq!(dec(b"++//"), Ok(vec![251, 239, 255]));
    assert_eq!(dec(b"ABCD"), Ok(vec![0, 16, 131]));
    assert_eq!(dec(b"wxyz"), Ok(vec![195, 28, 179]));
}

// R6, R12, R16
#[test]
fn hidden_early_padding() {
    for (s, p) in [
        (b"=AAA".as_slice(), 0),
        (b"A=AA".as_slice(), 1),
        (b"AAAA=AAA".as_slice(), 4),
    ] {
        assert_eq!(dec(s), Err(Error::BadPadding { offset: p }));
    }
}

// R7, R12, R16
#[test]
fn hidden_pending_padding_precedence() {
    for s in [b"AA=A".as_slice(), b"AA=?", b"AA=\xff"] {
        assert_eq!(dec(s), Err(Error::BadPadding { offset: 3 }));
    }
    assert_eq!(dec(b"AAAAAA=\x0a"), Err(Error::BadPadding { offset: 7 }));
}

// R8, R12, R16
#[test]
fn hidden_terminal_padding_precedence() {
    for s in [
        b"eg==A".as_slice(),
        b"eg==?",
        b"eg===",
        b"eHk=\xff",
        b"eHk=\x0a",
    ] {
        assert_eq!(dec(s), Err(Error::BadPadding { offset: 4 }));
    }
}

// R3, R9, R12
#[test]
fn hidden_invalid_physical_bytes() {
    for b in [
        b' ', b'\x09', b'\x0d', b'\x0a', 11, 12, 0, 128, 255, b'-', b'_',
    ] {
        assert_eq!(
            dec(&[b'A', b'A', b, b'A']),
            Err(Error::InvalidByte { offset: 2, byte: b })
        );
    }
    assert_eq!(
        dec(&[b'A', 0xc3, 0xa9]),
        Err(Error::InvalidByte {
            offset: 1,
            byte: 0xc3
        })
    );
}

// R6, R10-R12, R16
#[test]
fn hidden_eof_padding() {
    assert_eq!(dec(b"AA="), Err(Error::BadPadding { offset: 2 }));
    assert_eq!(dec(b"AAAAAA="), Err(Error::BadPadding { offset: 6 }));
    assert_eq!(dec(b"A="), Err(Error::BadPadding { offset: 1 }));
}

// R11-R12, R16
#[test]
fn hidden_truncated_origins() {
    for suffix in [b"B".as_slice(), b"BC", b"BCD"] {
        let mut s = b"AAAAAAAA".to_vec();
        s.extend_from_slice(suffix);
        assert_eq!(dec(&s), Err(Error::Truncated { offset: 8 }));
    }
    assert_eq!(dec(b"A"), Err(Error::Truncated { offset: 0 }));
}

// R9-R12, R16
#[test]
fn hidden_scan_before_eof() {
    assert_eq!(
        dec(b"AAAAA?"),
        Err(Error::InvalidByte {
            offset: 5,
            byte: b'?'
        })
    );
    assert_eq!(dec(b"AA=?A"), Err(Error::BadPadding { offset: 3 }));
    assert_eq!(
        dec(b"?=AA"),
        Err(Error::InvalidByte {
            offset: 0,
            byte: b'?'
        })
    );
}

// R5, R13
#[test]
fn hidden_noncanonical_permissive() {
    assert_eq!(dec(b"eB=="), Ok(b"x".to_vec()));
    assert_eq!(dec(b"eHl="), Ok(b"xy".to_vec()));
    assert_eq!(dec(b"/x=="), Ok(vec![255]));
    assert_eq!(dec(b"//7="), Ok(vec![255, 254]));
}

// R3-R5, R14-R15
#[test]
fn hidden_large_roundtrip() {
    let bytes: Vec<u8> = (0..4099).map(|i| ((i * 73 + 19) % 256) as u8).collect();
    let s = enc(&bytes);
    assert_eq!(s.len(), 5468);
    assert_eq!(dec(s.as_bytes()), Ok(bytes));
}

// R9, R12, R15
#[test]
fn hidden_long_invalid() {
    let mut s = b"AAAA".repeat(40);
    s.extend_from_slice(b"AA$A");
    assert_eq!(
        dec(&s),
        Err(Error::InvalidByte {
            offset: 162,
            byte: b'$'
        })
    );
}

// R4-R5, R14
#[test]
fn hidden_two_tail_forms() {
    assert_eq!(dec(b"AA=="), Ok(vec![0]));
    assert_eq!(dec(b"AAA="), Ok(vec![0, 0]));
    assert_eq!(dec(b"AAAA"), Ok(vec![0, 0, 0]));
}

// R2, R15
#[test]
fn hidden_stateless_after_error() {
    assert!(dec(b"!").is_err());
    assert_eq!(dec(b"QQ=="), Ok(vec![65]));
    assert_eq!(enc(b"123"), "MTIz");
}

// R5, R8, R16
#[test]
fn hidden_after_padding_group_boundary() {
    assert_eq!(dec(b"AAAAAA==AAAA"), Err(Error::BadPadding { offset: 8 }));
}

// R10-R11
#[test]
fn hidden_eof_lengths() {
    for s in [b"AA".as_slice(), b"AAA"] {
        assert_eq!(dec(s), Err(Error::Truncated { offset: 0 }));
    }
    assert_eq!(dec(b"AAA="), Ok(vec![0, 0]));
}

// R9, R12, R15
#[test]
fn hidden_invalid_after_prefix() {
    assert_eq!(
        dec(b"AAAA!AAA"),
        Err(Error::InvalidByte {
            offset: 4,
            byte: b'!'
        })
    );
}

// R8, R10-R12
#[test]
fn hidden_padding_then_bad_eof() {
    assert_eq!(dec(b"AA===AA"), Err(Error::BadPadding { offset: 4 }));
    assert_eq!(dec(b"AAA=="), Err(Error::BadPadding { offset: 4 }));
}

// R3-R5, R13-R14
#[test]
fn hidden_all_byte_singletons() {
    for b in 0..=255u8 {
        let s = enc(&[b]);
        assert_eq!(s.len(), 4);
        assert!(s.ends_with("=="));
        assert_eq!(dec(s.as_bytes()), Ok(vec![b]));
    }
}

// R5, R13
#[test]
fn hidden_full_group_not_trailing() {
    assert_eq!(dec(b"////"), Ok(vec![255, 255, 255]));
    assert_eq!(dec(b"/////w=="), Ok(vec![255, 255, 255, 255]));
}

// R6-R9, R12, R16
#[test]
fn hidden_first_error_wins() {
    assert_eq!(dec(b"A=?!"), Err(Error::BadPadding { offset: 1 }));
    assert_eq!(
        dec(b"AA?="),
        Err(Error::InvalidByte {
            offset: 2,
            byte: b'?'
        })
    );
}
