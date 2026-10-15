// Checks R1-R4.
#[test]
fn public_crc_vector() {
assert_eq!(crc32(b"123456789"), 0xcbf43926);
}

// Checks R5-R7.
#[test]
fn public_adler_vector() {
assert_eq!(adler32(b"Wikipedia"), 0x11e60398);
assert_eq!(checksums(b""), Hashes { crc32: 0, adler32: 1 });
}

// Checks R8,R14,R16.
#[test]
fn public_selected_ranges() {
assert_eq!(checksums_spans(b"abc--def", &[Span { start: 0, end: 3 },Span { start: 5, end: 8 }]), Ok(Hashes { crc32: 0x4b8e39ef, adler32: 0x081e0256 }));
}

// Checks R9-R11,R17.
#[test]
fn public_reversed_range() {
assert_eq!(checksums_spans(b"1234", &[Span { start: 3, end: 1 }]), Err(SpanError::Reversed { span: 0 }));
}
