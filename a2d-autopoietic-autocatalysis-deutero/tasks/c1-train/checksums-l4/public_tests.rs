// Checks R42-R44.
#[test]
fn public_combine_blocks() {
assert_eq!(crc32_combine(crc32(b"left"), crc32(b"right"), 5), crc32(b"leftright"));
assert_eq!(crc32_combine(17, 999, 0), 17);
}

// Checks R45-R48.
#[test]
fn public_combine_parts() {
let p = [Part { crc32: crc32(b"a"), len: 1 }, Part { crc32: crc32(b"bc"), len: 2 }];
assert_eq!(combine_parts(&p), Ok(Part { crc32: crc32(b"abc"), len: 3 }));
}

// Checks R49-R56.
#[test]
fn public_manifest_verification() {
let ranges = &[Span { start: 0, end: 3 }];
assert_eq!(manifest(b"hey", ranges), Ok("len=0000000000000003;crc=88d615f0;adler=027e0147".to_string()));
assert_eq!(verify(b"hey", ranges, "len=0000000000000003;crc=88d615f0;adler=027e0147"), Ok(()));
}

// Checks R1-R41.
#[test]
fn public_prior_algorithms() {
assert_eq!(multi_checksums(b"123456", &[Span { start: 0, end: 6 }], Crc16::False), Ok(Multi { base: Hashes { crc32: 0x0972d361, adler32: 0x042e0136 }, fletcher16: 0x2c36, fletcher32: 0x2f35999c, crc16: 0x2ef4 }));
let mut s=Stream::new(); s.update(b"123456"); assert_eq!(Stream::restore(&s.snapshot()).unwrap().checksum(), checksums(b"123456"));
assert_eq!(crc32(b"xyz"), 0xeb8eba67); assert_eq!(adler32(b"xyz"), 0x02d7016c);
assert_eq!(checksums_spans(b"xyz", &[Span { start:0,end:3 }]), Ok(checksums(b"xyz")));
let mut ss=Stream::from_seed(0,1).unwrap(); ss.update_spans(b"xyz", &[Span { start:0,end:3 }]).unwrap();
let mut rr=Stream::restore(&ss.snapshot()).unwrap(); assert_eq!(rr.checksum(),checksums(b"xyz"));
rr.reset(); assert_eq!(rr.checksum(),Stream::new().checksum());
assert_eq!(fletcher16(b"abcd"),0xd78b); assert_eq!(fletcher32(b"abcd"),0x2629c4c6);
assert_eq!(fletcher32_spans(b"abcd",&[Span { start:0,end:4 }]),Ok(fletcher32(b"abcd")));
for v in [Crc16::Xmodem,Crc16::False,Crc16::X25] { assert_eq!(multi_checksums(b"abcd",&[Span { start:0,end:4 }],v).unwrap().crc16,crc16(b"abcd",v)); }
}
