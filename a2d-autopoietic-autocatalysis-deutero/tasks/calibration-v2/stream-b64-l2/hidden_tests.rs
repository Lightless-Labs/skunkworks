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

// R17-R19
#[test]
fn hidden_stream_empty_and_carry() {
    let mut e = Encoder::new();
    let mut d = Decoder::new();
    assert_eq!(e.push(b""), Ok(String::new()));
    assert_eq!(d.push(b""), Ok(vec![]));
    assert_eq!(e.push(b"uv"), Ok(String::new()));
    assert_eq!(e.push(b""), Ok(String::new()));
    assert_eq!(e.finish(), Ok("dXY=".to_string()));
    assert_eq!(d.finish(), Ok(vec![]));
}

// R18-R19
#[test]
fn hidden_stream_exact_emission() {
    let mut e = Encoder::new();
    assert_eq!(e.push(b"uvwxyz!"), Ok("dXZ3eHl6".to_string()));
    assert_eq!(e.finish(), Ok("IQ==".to_string()));
    let mut d = Decoder::new();
    assert_eq!(d.push(b"dXZ3e"), Ok(b"uvw".to_vec()));
    assert_eq!(d.push(b"Hl6IQ="), Ok(b"xyz".to_vec()));
    assert_eq!(d.push(b"="), Ok(b"!".to_vec()));
    assert_eq!(d.finish(), Ok(vec![]));
}

// R18-R20, R24
#[test]
fn hidden_every_split_valid() {
    let raw: Vec<u8> = (0..53).map(|i| (i * 43) as u8).collect();
    let wire = enc(&raw);
    for p in 0..=raw.len() {
        let mut e = Encoder::new();
        let mut s = e.push(&raw[..p]).unwrap();
        s.push_str(&e.push(&raw[p..]).unwrap());
        s.push_str(&e.finish().unwrap());
        assert_eq!(s, wire);
    }
    for p in 0..=wire.len() {
        let mut d = Decoder::new();
        let mut b = d.push(&wire.as_bytes()[..p]).unwrap();
        b.extend(d.push(&wire.as_bytes()[p..]).unwrap());
        b.extend(d.finish().unwrap());
        assert_eq!(b, raw);
    }
}

// R9, R20-R22
#[test]
fn hidden_absolute_invalid_and_sticky() {
    let mut d = Decoder::new();
    assert_eq!(d.push(b"AAAA"), Ok(vec![0, 0, 0]));
    assert_eq!(d.push(b"AA"), Ok(vec![]));
    let err = Error::InvalidByte {
        offset: 6,
        byte: b'!',
    };
    assert_eq!(d.push(b"!A"), Err(err.clone()));
    assert_eq!(d.push(b""), Err(err.clone()));
    assert_eq!(d.push(b"AAAA"), Err(err.clone()));
    assert_eq!(d.finish(), Err(err));
}

// R15, R21-R22
#[test]
fn hidden_failed_push_discards_its_output() {
    let mut d = Decoder::new();
    let err = Error::InvalidByte {
        offset: 4,
        byte: b'?',
    };
    assert_eq!(d.push(b"AAAA?"), Err(err.clone()));
    assert_eq!(d.finish(), Err(err));
}

// R10-R11, R22-R23
#[test]
fn hidden_sticky_finish_and_closed() {
    let mut d = Decoder::new();
    d.push(b"AAAAAA=").unwrap();
    let err = Error::BadPadding { offset: 6 };
    assert_eq!(d.finish(), Err(err.clone()));
    assert_eq!(d.push(b"="), Err(err.clone()));
    assert_eq!(d.finish(), Err(err));
    let mut e = Encoder::new();
    e.finish().unwrap();
    assert_eq!(e.push(b""), Err(Error::AlreadyFinished));
    assert_eq!(e.push(b"a"), Err(Error::AlreadyFinished));
    let mut good = Decoder::new();
    good.finish().unwrap();
    assert_eq!(good.push(b""), Err(Error::AlreadyFinished));
    assert_eq!(good.finish(), Err(Error::AlreadyFinished));
}

// R7-R8, R19-R20, R23
#[test]
fn hidden_padding_across_chunks() {
    let mut d = Decoder::new();
    d.push(b"AAAAAA=").unwrap();
    assert_eq!(d.push(b"?"), Err(Error::BadPadding { offset: 7 }));
    let mut d = Decoder::new();
    assert_eq!(d.push(b"AA="), Ok(vec![]));
    assert_eq!(d.push(b"="), Ok(vec![0]));
    assert_eq!(d.push(b"A"), Err(Error::BadPadding { offset: 4 }));
}

// R12, R20, R24
#[test]
fn hidden_every_split_error() {
    for s in [
        b"AAAAAA=?".as_slice(),
        b"AAA=AA",
        b"AAAAA!",
        b"AAAAAAA",
        b"AAAAAA=",
    ] {
        let expected = dec(s).unwrap_err();
        for p in 0..=s.len() {
            let mut d = Decoder::new();
            let got = d
                .push(&s[..p])
                .and_then(|_| d.push(&s[p..]))
                .and_then(|_| d.finish());
            assert_eq!(got, Err(expected.clone()));
        }
    }
}

// R17-R20, R24
#[test]
fn hidden_bytewise_stream_and_independence() {
    let raw: Vec<u8> = (0..257).map(|i| (i * 17) as u8).collect();
    let mut e = Encoder::new();
    let mut s = String::new();
    for b in &raw {
        s.push_str(&e.push(&[*b]).unwrap());
    }
    s.push_str(&e.finish().unwrap());
    assert_eq!(s, enc(&raw));
    let mut d = Decoder::new();
    let mut out = Vec::new();
    for b in s.bytes() {
        out.extend(d.push(&[b]).unwrap());
    }
    out.extend(d.finish().unwrap());
    assert_eq!(out, raw);
    let mut other = Decoder::new();
    assert_eq!(other.push(b"QQ=="), Ok(vec![65]));
    assert_eq!(other.finish(), Ok(vec![]));
}
