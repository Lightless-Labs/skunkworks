fn opts() -> Options {
    Options {
        alphabet: Alphabet::Standard,
        padding: Padding::Required,
        whitespace: Whitespace::None,
    }
}
fn dec(b: &[u8]) -> Result<Vec<u8>, Error> {
    decode(b, opts())
}
fn enc(b: &[u8]) -> String {
    encode(b, opts()).unwrap()
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
    let mut e = Encoder::new(opts()).unwrap();
    let mut d = Decoder::new(opts()).unwrap();
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
    let mut e = Encoder::new(opts()).unwrap();
    assert_eq!(e.push(b"uvwxyz!"), Ok("dXZ3eHl6".to_string()));
    assert_eq!(e.finish(), Ok("IQ==".to_string()));
    let mut d = Decoder::new(opts()).unwrap();
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
        let mut e = Encoder::new(opts()).unwrap();
        let mut s = e.push(&raw[..p]).unwrap();
        s.push_str(&e.push(&raw[p..]).unwrap());
        s.push_str(&e.finish().unwrap());
        assert_eq!(s, wire);
    }
    for p in 0..=wire.len() {
        let mut d = Decoder::new(opts()).unwrap();
        let mut b = d.push(&wire.as_bytes()[..p]).unwrap();
        b.extend(d.push(&wire.as_bytes()[p..]).unwrap());
        b.extend(d.finish().unwrap());
        assert_eq!(b, raw);
    }
}

// R9, R20-R22
#[test]
fn hidden_absolute_invalid_and_sticky() {
    let mut d = Decoder::new(opts()).unwrap();
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
    let mut d = Decoder::new(opts()).unwrap();
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
    let mut d = Decoder::new(opts()).unwrap();
    d.push(b"AAAAAA=").unwrap();
    let err = Error::BadPadding { offset: 6 };
    assert_eq!(d.finish(), Err(err.clone()));
    assert_eq!(d.push(b"="), Err(err.clone()));
    assert_eq!(d.finish(), Err(err));
    let mut e = Encoder::new(opts()).unwrap();
    e.finish().unwrap();
    assert_eq!(e.push(b""), Err(Error::AlreadyFinished));
    assert_eq!(e.push(b"a"), Err(Error::AlreadyFinished));
    let mut good = Decoder::new(opts()).unwrap();
    good.finish().unwrap();
    assert_eq!(good.push(b""), Err(Error::AlreadyFinished));
    assert_eq!(good.finish(), Err(Error::AlreadyFinished));
}

// R7-R8, R19-R20, R23
#[test]
fn hidden_padding_across_chunks() {
    let mut d = Decoder::new(opts()).unwrap();
    d.push(b"AAAAAA=").unwrap();
    assert_eq!(d.push(b"?"), Err(Error::BadPadding { offset: 7 }));
    let mut d = Decoder::new(opts()).unwrap();
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
            let mut d = Decoder::new(opts()).unwrap();
            let got = d
                .push(&s[..p])
                .and_then(|_| d.push(&s[p..]))
                .and_then(|_| d.finish());
            assert_eq!(got, Err(expected.clone()));
        }
    }
}

// R25, R9
#[test]
fn hidden_url_and_foreign_alphabet() {
    let mut o = opts();
    o.alphabet = Alphabet::UrlSafe;
    assert_eq!(encode(&[255, 255, 255], o), Ok("____".to_string()));
    assert_eq!(decode(b"-___", o), Ok(vec![251, 255, 255]));
    assert_eq!(
        decode(b"AA/A", o),
        Err(Error::InvalidByte {
            offset: 2,
            byte: b'/'
        })
    );
    assert_eq!(
        dec(b"AA_A"),
        Err(Error::InvalidByte {
            offset: 2,
            byte: b'_'
        })
    );
}

// R4-R5, R10-R11, R26
#[test]
fn hidden_padding_modes() {
    let mut o = opts();
    o.padding = Padding::Forbidden;
    assert_eq!(encode(b"u", o).unwrap(), "dQ");
    assert_eq!(encode(b"uv", o).unwrap(), "dXY");
    assert_eq!(decode(b"dQ", o).unwrap(), b"u");
    assert_eq!(decode(b"dXY", o).unwrap(), b"uv");
    assert_eq!(decode(b"dQ==", o), Err(Error::BadPadding { offset: 2 }));
    assert_eq!(decode(b"d", o), Err(Error::Truncated { offset: 0 }));
    o.padding = Padding::Optional;
    assert_eq!(encode(b"u", o).unwrap(), "dQ==");
    assert_eq!(decode(b"dQ==", o).unwrap(), b"u");
    assert_eq!(decode(b"dQ", o).unwrap(), b"u");
    assert_eq!(decode(b"dQ=", o), Err(Error::BadPadding { offset: 2 }));
}

// R20, R27, R32
#[test]
fn hidden_ignored_whitespace_offsets() {
    let mut o = opts();
    o.whitespace = Whitespace::IgnoreAscii;
    assert_eq!(decode(b"\x0b d\x0cQ\x09=\x0d= \x0a", o).unwrap(), b"u");
    assert_eq!(
        decode(b"\x09\x0a\x0d\x0b\x0c ", o).unwrap(),
        Vec::<u8>::new()
    );
    assert_eq!(
        decode(b"AA\x0a AA\x09!", o),
        Err(Error::InvalidByte {
            offset: 7,
            byte: b'!'
        })
    );
    assert_eq!(
        decode(b"AA\x09=\x0a ?", o),
        Err(Error::BadPadding { offset: 6 })
    );
    assert_eq!(
        decode(b"AA== \x0d?", o),
        Err(Error::BadPadding { offset: 6 })
    );
    assert_eq!(
        decode(b" \x0aA \x09A", o),
        Err(Error::Truncated { offset: 2 })
    );
    assert_eq!(
        decode(b" A\x09A= \x0d", o),
        Err(Error::BadPadding { offset: 4 })
    );
    assert_eq!(
        decode(&[b'A', 0x85], o),
        Err(Error::InvalidByte {
            offset: 1,
            byte: 0x85
        })
    );
}

// R2, R28
#[test]
fn hidden_config_validation_first() {
    for width in [0, 1, 3, 5, 119, 121, usize::MAX] {
        let mut o = opts();
        o.whitespace = Whitespace::Wrapped {
            width,
            ending: LineEnding::Lf,
        };
        let err = Error::InvalidWidth { width };
        assert_eq!(encode(b"", o), Err(err.clone()));
        assert_eq!(decode(b"?", o), Err(err.clone()));
        assert!(matches!(Encoder::new(o), Err(e) if e == err));
        assert!(matches!(Decoder::new(o), Err(e) if e == err));
    }
    for width in [4, 120] {
        let mut o = opts();
        o.whitespace = Whitespace::Wrapped {
            width,
            ending: LineEnding::CrLf,
        };
        assert_eq!(encode(b"", o), Ok(String::new()));
    }
}

// R26, R29
#[test]
fn hidden_wrap_byte_layout() {
    let mut o = opts();
    o.whitespace = Whitespace::Wrapped {
        width: 4,
        ending: LineEnding::Lf,
    };
    assert_eq!(encode(b"abcdefg", o).unwrap(), "YWJj\x0aZGVm\x0aZw==");
    assert_eq!(encode(b"abc", o).unwrap(), "YWJj");
    o.padding = Padding::Forbidden;
    assert_eq!(encode(b"abcdefg", o).unwrap(), "YWJj\x0aZGVm\x0aZw");
    o.whitespace = Whitespace::Wrapped {
        width: 8,
        ending: LineEnding::CrLf,
    };
    assert_eq!(encode(b"abcdefg", o).unwrap(), "YWJjZGVm\x0d\x0aZw");
}

// R30-R32
#[test]
fn hidden_wrap_errors_and_eof() {
    let mut o = opts();
    o.whitespace = Whitespace::Wrapped {
        width: 4,
        ending: LineEnding::Lf,
    };
    for (s, p) in [
        (b"AA\x0aAA".as_slice(), 2),
        (b"AAAAAAAA", 4),
        (b"AAAA\x0d\x0aAAAA", 4),
        (b"AAA\x09", 3),
        (b"AAAA\x0a", 4),
        (b"\x0a", 0),
    ] {
        assert_eq!(decode(s, o), Err(Error::BadWrap { offset: p }));
    }
    assert_eq!(decode(b"AA==\x09", o), Err(Error::BadWrap { offset: 4 }));
    assert_eq!(
        decode(b"AA==\x0a?", o),
        Err(Error::BadPadding { offset: 5 })
    );
    assert_eq!(
        decode(b"AAAA\x0a?", o),
        Err(Error::InvalidByte {
            offset: 5,
            byte: b'?'
        })
    );
    o.whitespace = Whitespace::Wrapped {
        width: 4,
        ending: LineEnding::CrLf,
    };
    for (s, p) in [
        (b"AAAA\x0d".as_slice(), 4),
        (b"AAAA\x0d?", 5),
        (b"AAAA\x0d\x0a", 4),
        (b"AAAA\x0aAAAA", 4),
    ] {
        assert_eq!(decode(s, o), Err(Error::BadWrap { offset: p }));
    }
}

// R19-R20, R24, R29-R32
#[test]
fn hidden_crlf_chunk_boundaries() {
    let mut o = opts();
    o.whitespace = Whitespace::Wrapped {
        width: 4,
        ending: LineEnding::CrLf,
    };
    let wire = b"YWJj\x0d\x0aZGVm\x0d\x0aZw==";
    for split in 0..=wire.len() {
        let mut d = Decoder::new(o).unwrap();
        let mut b = d.push(&wire[..split]).unwrap();
        b.extend(d.push(&wire[split..]).unwrap());
        b.extend(d.finish().unwrap());
        assert_eq!(b, b"abcdefg");
    }
    let mut d = Decoder::new(o).unwrap();
    d.push(b"AAAA\x0d").unwrap();
    assert_eq!(d.push(b"?"), Err(Error::BadWrap { offset: 5 }));
    assert_eq!(d.finish(), Err(Error::BadWrap { offset: 5 }));
}

// R17-R20, R24-R27, R29
#[test]
fn hidden_composed_modes_stream() {
    let mut o = opts();
    o.alphabet = Alphabet::UrlSafe;
    o.padding = Padding::Forbidden;
    o.whitespace = Whitespace::Wrapped {
        width: 4,
        ending: LineEnding::Lf,
    };
    let raw = [255, 254, 253, 252, 251, 250, 249];
    let wire = "__79\x0a_Pv6\x0a-Q";
    let mut e = Encoder::new(o).unwrap();
    let mut s = String::new();
    for b in &raw {
        s.push_str(&e.push(&[*b]).unwrap());
        s.push_str(&e.push(b"").unwrap());
    }
    s.push_str(&e.finish().unwrap());
    assert_eq!(s, wire);
    let mut d = Decoder::new(o).unwrap();
    let mut bytes = Vec::new();
    for b in wire.bytes() {
        bytes.extend(d.push(&[b]).unwrap());
    }
    bytes.extend(d.finish().unwrap());
    assert_eq!(bytes, raw);
    o.whitespace = Whitespace::IgnoreAscii;
    let mut d = Decoder::new(o).unwrap();
    d.push(b" \x09AA").unwrap();
    assert_eq!(
        d.push(b"\x0a?"),
        Err(Error::InvalidByte {
            offset: 5,
            byte: b'?'
        })
    );
}
