// Checks R3-R7,R19-R20,R23.
#[test]
fn public_stream_chunking() {
let mut s = Stream::new(); s.update(b"123"); s.update(b""); s.update(b"456789");
assert_eq!(s.checksum(), Hashes { crc32: 0xcbf43926, adler32: 0x091e01de }); assert_eq!(s.checksum(), checksums(b"123456789"));
}

// Checks R8-R18,R24.
#[test]
fn public_stream_ranges() {
let mut s = Stream::new(); s.update(b"pre");
assert_eq!(s.update_spans(b"ab--cd", &[Span { start: 0, end: 2 },Span { start: 4, end: 6 }]), Ok(()));
assert_eq!(s.checksum(), Hashes { crc32: 0xaafa49af, adler32: 0x0b9002d2 });
}

// Checks R21-R22,R26-R30.
#[test]
fn public_snapshot_roundtrip() {
let mut s = Stream::new(); s.update(b"hello");
let mut restored = Stream::restore(&s.snapshot()).unwrap(); restored.update(b"!");
assert_eq!(restored.checksum(), checksums(b"hello!"));
assert_eq!(Stream::from_seed(0, 1).unwrap().checksum(), Stream::new().checksum());
}

// Checks R10,R24-R25.
#[test]
fn public_stream_reset_and_batch_error() {
let mut s = Stream::new(); s.update(b"changed"); s.reset(); assert_eq!(s.checksum(), checksums(b""));
assert_eq!(s.update_spans(b"x", &[Span { start: 1, end: 0 }]), Err(SpanError::Reversed { span: 0 }));
assert_eq!(crc32(b"xyz"), 0xeb8eba67); assert_eq!(adler32(b"xyz"), 0x02d7016c);
assert_eq!(checksums_spans(b"xyz", &[Span { start:0,end:3 }]), Ok(checksums(b"xyz")));
}
