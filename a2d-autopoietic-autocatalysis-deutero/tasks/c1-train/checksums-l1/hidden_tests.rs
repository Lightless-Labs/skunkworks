// Checks R2-R7.
#[test]
fn hidden_empty_base() {
assert_eq!(crc32(&[]), 0); assert_eq!(adler32(&[]), 1); assert_eq!(checksums(&[]), Hashes { crc32: 0, adler32: 1 });
}

// Checks R3-R7,R16.
#[test]
fn hidden_binary_and_individual_bytes() {
assert_eq!(checksums(&[0]), Hashes { crc32: 0xd202ef8d, adler32: 0x00010001 });
assert_eq!(checksums(&[255]), Hashes { crc32: 0xff000000, adler32: 0x01000100 });
assert_eq!(checksums(&[128,0,127,255]), Hashes { crc32: 0x49b8e393, adler32: 0x040101ff });
assert_eq!(checksums(&[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,41,42,43,44,45,46,47,48,49,50,51,52,53,54,55,56,57,58,59,60,61,62,63,64,65,66,67,68,69,70,71,72,73,74,75,76,77,78,79,80,81,82,83,84,85,86,87,88,89,90,91,92,93,94,95,96,97,98,99,100,101,102,103,104,105,106,107,108,109,110,111,112,113,114,115,116,117,118,119,120,121,122,123,124,125,126,127,128,129,130,131,132,133,134,135,136,137,138,139,140,141,142,143,144,145,146,147,148,149,150,151,152,153,154,155,156,157,158,159,160,161,162,163,164,165,166,167,168,169,170,171,172,173,174,175,176,177,178,179,180,181,182,183,184,185,186,187,188,189,190,191,192,193,194,195,196,197,198,199,200,201,202,203,204,205,206,207,208,209,210,211,212,213,214,215,216,217,218,219,220,221,222,223,224,225,226,227,228,229,230,231,232,233,234,235,236,237,238,239,240,241,242,243,244,245,246,247,248,249,250,251,252,253,254,255]), Hashes { crc32: 0x29058c73, adler32: 0xadf67f81 });
}

// Checks R5-R7,R18.
#[test]
fn hidden_adler_long_reductions() {
let d=vec![255u8; 200_003]; assert_eq!(checksums(&d), Hashes { crc32: 0xeb05d775, adler32: 0x3bde6354 });
}

// Checks R3-R7.
#[test]
fn hidden_crc_nonstandard_vector() {
assert_eq!(checksums(b"checksum\0family\n"), Hashes { crc32: 0x1a29cffb, adler32: 0x345605e0 } );
}

// Checks R8,R16.
#[test]
fn hidden_gap_selection() {
assert_eq!(checksums_spans(b"QrsXXtuYYv!", &[Span { start: 1, end: 3 },Span { start: 5, end: 7 },Span { start: 9, end: 10 }]), Ok(Hashes { crc32: 0x846cd2f6, adler32: 0x06c70245 }));
}

// Checks R8,R15.
#[test]
fn hidden_no_spans() {
assert_eq!(checksums_spans(b"unselected", &[]), Ok(Hashes { crc32: 0x00000000, adler32: 0x00000001 }));
}

// Checks R13-R15.
#[test]
fn hidden_all_empty() {
assert_eq!(checksums_spans(b"abcdef", &[Span { start: 0, end: 0 },Span { start: 3, end: 3 },Span { start: 3, end: 3 },Span { start: 6, end: 6 }]), Ok(Hashes { crc32: 0x00000000, adler32: 0x00000001 }));
}

// Checks R8,R14,R16.
#[test]
fn hidden_adjacent_single_bytes() {
assert_eq!(checksums_spans(&[0,255,128,1], &[Span { start: 0, end: 1 },Span { start: 1, end: 2 },Span { start: 2, end: 3 },Span { start: 3, end: 4 }]), Ok(Hashes { crc32: 0xd358432c, adler32: 0x04020181 }));
}

// Checks R11,R13-R14.
#[test]
fn hidden_full_slice_and_end_empty() {
assert_eq!(checksums_spans(b"edge", &[Span { start: 0, end: 4 },Span { start: 4, end: 4 }]), Ok(Hashes { crc32: 0x7506d366, adler32: 0x03f70196 }));
}

// Checks R11,R13-R15,R17.
#[test]
fn hidden_empty_data_boundaries() {
assert_eq!(checksums_spans(b"", &[Span { start: 0, end: 0 },Span { start: 0, end: 0 }]), Ok(Hashes { crc32: 0x00000000, adler32: 0x00000001 }));
}

// Checks R9-R11,R17.
#[test]
fn hidden_reversed_precedes_bounds() {
assert_eq!(checksums_spans(b"xy", &[Span { start: 9, end: 7 }]), Err(SpanError::Reversed { span: 0 }));
}

// Checks R11,R17.
#[test]
fn hidden_start_bound_offset() {
assert_eq!(checksums_spans(b"xy", &[Span { start: 7, end: 9 }]), Err(SpanError::OutOfBounds { span: 0, offset: 7 }));
}

// Checks R11,R17.
#[test]
fn hidden_end_bound_offset() {
assert_eq!(checksums_spans(b"xy", &[Span { start: 1, end: 9 }]), Err(SpanError::OutOfBounds { span: 0, offset: 9 }));
}

// Checks R11,R13,R17.
#[test]
fn hidden_empty_oob() {
assert_eq!(checksums_spans(b"xy", &[Span { start: 3, end: 3 }]), Err(SpanError::OutOfBounds { span: 0, offset: 3 }));
}

// Checks R9-R11.
#[test]
fn hidden_first_span_error_wins() {
assert_eq!(checksums_spans(b"1234", &[Span { start: 0, end: 5 },Span { start: 9, end: 1 }]), Err(SpanError::OutOfBounds { span: 0, offset: 5 }));
}

// Checks R12,R17.
#[test]
fn hidden_overlap_inside_previous() {
assert_eq!(checksums_spans(b"abcdefgh", &[Span { start: 0, end: 5 },Span { start: 4, end: 7 }]), Err(SpanError::Overlap { span: 1 }));
}

// Checks R12-R13.
#[test]
fn hidden_empty_inside_previous() {
assert_eq!(checksums_spans(b"abcdefgh", &[Span { start: 1, end: 6 },Span { start: 3, end: 3 }]), Err(SpanError::Overlap { span: 1 }));
}

// Checks R12-R13.
#[test]
fn hidden_empty_advances_boundary() {
assert_eq!(checksums_spans(b"abcdefgh", &[Span { start: 0, end: 1 },Span { start: 5, end: 5 },Span { start: 4, end: 6 }]), Err(SpanError::Overlap { span: 2 }));
}

// Checks R11-R12.
#[test]
fn hidden_bounds_before_overlap() {
assert_eq!(checksums_spans(b"abcdef", &[Span { start: 2, end: 5 },Span { start: 3, end: 9 }]), Err(SpanError::OutOfBounds { span: 1, offset: 9 }));
}

// Checks R10,R12.
#[test]
fn hidden_reversed_before_overlap() {
assert_eq!(checksums_spans(b"abcdef", &[Span { start: 2, end: 5 },Span { start: 3, end: 1 }]), Err(SpanError::Reversed { span: 1 }));
}

// Checks R9,R12,R14.
#[test]
fn hidden_unsorted_equal_contents() {
assert_eq!(checksums_spans(b"aaaa", &[Span { start: 2, end: 4 },Span { start: 0, end: 2 }]), Err(SpanError::Overlap { span: 1 }));
}

// Checks R9-R12.
#[test]
fn hidden_later_span_error_index() {
assert_eq!(checksums_spans(b"0123456789", &[Span { start: 0, end: 1 },Span { start: 2, end: 3 },Span { start: 4, end: 4 },Span { start: 6, end: 7 },Span { start: 5, end: 8 }]), Err(SpanError::Overlap { span: 4 }));
}

// Checks R8,R16,R18.
#[test]
fn hidden_many_spans_cross_state() {
let d:Vec<u8>=(0..20_000).map(|i| ((i*37+11)%256) as u8).collect();
let ranges:Vec<Span>=(0..10_000).map(|i| Span { start:i*2, end:i*2+1 }).collect();
assert_eq!(checksums_spans(&d, &ranges), Ok(Hashes { crc32: 0xba6a25c9, adler32: 0x56ff887e }));
}

// Checks R1,R7-R8,R17.
#[test]
fn hidden_binary_input_unchanged() {
let d=vec![0,254,128,255,1,0]; let original=d.clone();
let ranges=&[Span { start: 0, end: 2 },Span { start: 3, end: 6 }]; assert_eq!(checksums_spans(&d,ranges),Ok(Hashes { crc32: 0x07189325, adler32: 0x06fc01ff }));
assert_eq!(d,original);
}
