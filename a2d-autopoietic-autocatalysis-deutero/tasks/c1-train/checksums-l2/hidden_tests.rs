// Checks R2-R7, R10,R12.
#[test]
fn hidden_inherited_l2_0() {
{
assert_eq!(crc32(&[]), 0); assert_eq!(adler32(&[]), 1); assert_eq!(checksums(&[]), Hashes { crc32: 0, adler32: 1 });
}
{
assert_eq!(checksums_spans(b"abcdef", &[Span { start: 2, end: 5 },Span { start: 3, end: 1 }]), Err(SpanError::Reversed { span: 1 }));
}
}

// Checks R3-R7,R16, R9,R12,R14.
#[test]
fn hidden_inherited_l2_1() {
{
assert_eq!(checksums(&[0]), Hashes { crc32: 0xd202ef8d, adler32: 0x00010001 });
assert_eq!(checksums(&[255]), Hashes { crc32: 0xff000000, adler32: 0x01000100 });
assert_eq!(checksums(&[128,0,127,255]), Hashes { crc32: 0x49b8e393, adler32: 0x040101ff });
assert_eq!(checksums(&[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,41,42,43,44,45,46,47,48,49,50,51,52,53,54,55,56,57,58,59,60,61,62,63,64,65,66,67,68,69,70,71,72,73,74,75,76,77,78,79,80,81,82,83,84,85,86,87,88,89,90,91,92,93,94,95,96,97,98,99,100,101,102,103,104,105,106,107,108,109,110,111,112,113,114,115,116,117,118,119,120,121,122,123,124,125,126,127,128,129,130,131,132,133,134,135,136,137,138,139,140,141,142,143,144,145,146,147,148,149,150,151,152,153,154,155,156,157,158,159,160,161,162,163,164,165,166,167,168,169,170,171,172,173,174,175,176,177,178,179,180,181,182,183,184,185,186,187,188,189,190,191,192,193,194,195,196,197,198,199,200,201,202,203,204,205,206,207,208,209,210,211,212,213,214,215,216,217,218,219,220,221,222,223,224,225,226,227,228,229,230,231,232,233,234,235,236,237,238,239,240,241,242,243,244,245,246,247,248,249,250,251,252,253,254,255]), Hashes { crc32: 0x29058c73, adler32: 0xadf67f81 });
}
{
assert_eq!(checksums_spans(b"aaaa", &[Span { start: 2, end: 4 },Span { start: 0, end: 2 }]), Err(SpanError::Overlap { span: 1 }));
}
}

// Checks R5-R7,R18, R9-R12.
#[test]
fn hidden_inherited_l2_2() {
{
let d=vec![255u8; 200_003]; assert_eq!(checksums(&d), Hashes { crc32: 0xeb05d775, adler32: 0x3bde6354 });
}
{
assert_eq!(checksums_spans(b"0123456789", &[Span { start: 0, end: 1 },Span { start: 2, end: 3 },Span { start: 4, end: 4 },Span { start: 6, end: 7 },Span { start: 5, end: 8 }]), Err(SpanError::Overlap { span: 4 }));
}
}

// Checks R3-R7, R8,R16,R18.
#[test]
fn hidden_inherited_l2_3() {
{
assert_eq!(checksums(b"checksum\0family\n"), Hashes { crc32: 0x1a29cffb, adler32: 0x345605e0 } );
}
{
let d:Vec<u8>=(0..20_000).map(|i| ((i*37+11)%256) as u8).collect();
let ranges:Vec<Span>=(0..10_000).map(|i| Span { start:i*2, end:i*2+1 }).collect();
assert_eq!(checksums_spans(&d, &ranges), Ok(Hashes { crc32: 0xba6a25c9, adler32: 0x56ff887e }));
}
}

// Checks R8,R16, R1,R7-R8,R17.
#[test]
fn hidden_inherited_l2_4() {
{
assert_eq!(checksums_spans(b"QrsXXtuYYv!", &[Span { start: 1, end: 3 },Span { start: 5, end: 7 },Span { start: 9, end: 10 }]), Ok(Hashes { crc32: 0x846cd2f6, adler32: 0x06c70245 }));
}
{
let d=vec![0,254,128,255,1,0]; let original=d.clone();
let ranges=&[Span { start: 0, end: 2 },Span { start: 3, end: 6 }]; assert_eq!(checksums_spans(&d,ranges),Ok(Hashes { crc32: 0x07189325, adler32: 0x06fc01ff }));
assert_eq!(d,original);
}
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

// Checks R19-R20,R23,R30.
#[test]
fn hidden_arbitrary_chunkings() {
let d:Vec<u8>=(0..4099).map(|i| ((i*113+17)%256) as u8).collect();
for size in [1,2,3,7,64,511,4099] { let mut s=Stream::new();
for chunk in d.chunks(size) { s.update(&[]); s.update(chunk); let _=s.checksum(); let _=s.snapshot(); }
assert_eq!(s.checksum(), checksums(&d)); }
}

// Checks R21-R23.
#[test]
fn hidden_seed_and_empty_updates() {
let mut s=Stream::from_seed(0x81234567, 0xfff0fff0).unwrap();
assert_eq!(s.checksum(), Hashes { crc32:0x81234567, adler32:0xfff0fff0 });
s.update(&[]); assert_eq!(s.checksum(), Hashes { crc32:0x81234567, adler32:0xfff0fff0 });
s.update(&[255,0,128]); assert_eq!(s.checksum(), Hashes { crc32:0xce9079c9, adler32:0x0379017e });
}

// Checks R21-R22.
#[test]
fn hidden_seed_validity_order() {
assert_eq!(Stream::from_seed(0,0xffffffff).unwrap_err(), SnapshotError::Adler { component:0 });
assert_eq!(Stream::from_seed(0,0xfff10001).unwrap_err(), SnapshotError::Adler { component:1 });
assert_eq!(Stream::from_seed(u32::MAX,0).unwrap().checksum(), Hashes { crc32:u32::MAX, adler32:0 });
}

// Checks R9-R13,R24.
#[test]
fn hidden_batch_atomic_errors() {
let cases:[(&[Span],SpanError);3]=[(&[Span { start: 0, end: 2 },Span { start: 7, end: 8 }],SpanError::OutOfBounds { span:1,offset:7 }),(&[Span { start: 0, end: 2 },Span { start: 3, end: 1 }],SpanError::Reversed { span:1 }),(&[Span { start: 0, end: 3 },Span { start: 2, end: 5 }],SpanError::Overlap { span:1 })];
for (ranges,error) in cases {
let mut s=Stream::from_seed(0x12345678, 0x00120034).unwrap(); let before=s.snapshot();
assert_eq!(s.update_spans(b"abcdef", ranges),Err(error)); assert_eq!(s.snapshot(),before);
s.update(b"next"); let mut fresh=Stream::restore(&before).unwrap(); fresh.update(b"next"); assert_eq!(s.checksum(),fresh.checksum()); }
}

// Checks R20,R23,R25.
#[test]
fn hidden_seed_reset_and_clone() {
let mut a=Stream::from_seed(123,0x00120034).unwrap(); let mut b=a.clone();
a.update(b"left"); b.update(b"right"); assert_ne!(a.checksum(),b.checksum());
a.reset(); assert_eq!(a.snapshot(),"c00000000:a00000001");
a.update(b"after"); assert_eq!(a.checksum(),checksums(b"after"));
let mut r=Stream::restore(&b.snapshot()).unwrap(); r.reset(); assert_eq!(r.checksum(),checksums(b""));
}

// Checks R21,R26,R29-R30.
#[test]
fn hidden_snapshot_exact_restore_resume() {
let mut s=Stream::new(); assert_eq!(s.snapshot(),"c00000000:a00000001");
s.update(&[0,255,1]); assert_eq!(s.snapshot(),"c1b6214f6:a02020101");
let mut r=Stream::restore(&s.snapshot()).unwrap(); r.update(&[128,0]); s.update(&[128,0]); assert_eq!(r.snapshot(),s.snapshot());
let mut arbitrary=Stream::restore("cffffffff:a00000000").unwrap(); arbitrary.update(&[1]); assert_eq!(arbitrary.checksum(),Stream::from_seed(0xffffffff,0).map(|mut a| { a.update(&[1]); a.checksum() }).unwrap());
}

// Checks R27-R28.
#[test]
fn hidden_snapshot_shape_and_unicode() {
for t in ["", "c00000000:a00000001\n", "C00000000:a00000001", "cZ0000000-a00000001", "c00000000:A00000001", "c00000000:a000000é1"] { assert_eq!(Stream::restore(t).unwrap_err(),SnapshotError::Shape); }
assert_eq!(Stream::restore("c00000000:a000000é").unwrap_err(),SnapshotError::Hex { offset:17 });
}

// Checks R22,R28-R29.
#[test]
fn hidden_snapshot_digit_order_and_adler() {
assert_eq!(Stream::restore("c0000000F:a00000001").unwrap_err(),SnapshotError::Hex { offset:8 });
assert_eq!(Stream::restore("c00000000:a0000000G").unwrap_err(),SnapshotError::Hex { offset:18 });
assert_eq!(Stream::restore("c00000z00:a0000000G").unwrap_err(),SnapshotError::Hex { offset:6 });
assert_eq!(Stream::restore("c00000000:affff000Z").unwrap_err(),SnapshotError::Hex { offset:18 });
assert_eq!(Stream::restore("c00000000:affffffff").unwrap_err(),SnapshotError::Adler { component:0 });
assert_eq!(Stream::restore("c00000000:afff10001").unwrap_err(),SnapshotError::Adler { component:1 });
}

// Checks R8,R13-R16,R23-R24.
#[test]
fn hidden_stream_successful_spans_resume() {
let mut s=Stream::new(); s.update(b"P");
s.update_spans(b"0a1bc2d3",&[Span { start: 1, end: 2 },Span { start: 3, end: 5 },Span { start: 5, end: 5 },Span { start: 6, end: 7 }]).unwrap();
s.update_spans(b"ignored",&[]).unwrap(); s.update(b"Q"); assert_eq!(s.checksum(),Hashes { crc32: 0xdf5d7299, adler32: 0x0795022c });
}
