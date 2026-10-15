// Checks R31-R32,R41.
#[test]
fn public_fletcher_algorithms() {
assert_eq!(fletcher16(b"abcde"), 0xc8f0);
assert_eq!(fletcher32(b"abcdef"), 0x50562a2d);
}

// Checks R34-R38.
#[test]
fn public_crc16_variants() {
assert_eq!(crc16(b"123456789", Crc16::Xmodem), 0x31c3);
assert_eq!(crc16(b"123456789", Crc16::False), 0x29b1);
assert_eq!(crc16(b"123456789", Crc16::X25), 0x906e);
}

// Checks R8-R18,R33,R39-R40.
#[test]
fn public_multi_selected() {
let ranges = &[Span { start: 0, end: 2 },Span { start: 4, end: 6 }];
assert_eq!(multi_checksums(b"ab--cd", ranges, Crc16::X25), Ok(Multi { base: Hashes { crc32: 0xed82cd11, adler32: 0x03d8018b }, fletcher16: 0xd78b, fletcher32: 0x2629c4c6, crc16: 0xa36b }));
assert_eq!(fletcher32_spans(b"ab--cd", ranges), Ok(0x2629c4c6));
}

// Checks R19-R30.
#[test]
fn public_inherited_stream() {
let mut s = Stream::new(); s.update(b"chunk");
let mut r = Stream::restore(&s.snapshot()).unwrap(); r.update(b" two");
assert_eq!(r.checksum(), checksums(b"chunk two"));
assert_eq!(crc32(b"xyz"), 0xeb8eba67); assert_eq!(adler32(b"xyz"), 0x02d7016c);
assert_eq!(checksums_spans(b"xyz", &[Span { start:0,end:3 }]), Ok(checksums(b"xyz")));
let mut ss=Stream::from_seed(0,1).unwrap(); ss.update_spans(b"xyz", &[Span { start:0,end:3 }]).unwrap();
let mut rr=Stream::restore(&ss.snapshot()).unwrap(); assert_eq!(rr.checksum(),checksums(b"xyz"));
rr.reset(); assert_eq!(rr.checksum(),Stream::new().checksum());
}
