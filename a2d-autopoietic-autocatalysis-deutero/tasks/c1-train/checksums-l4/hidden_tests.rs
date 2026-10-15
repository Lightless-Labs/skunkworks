// Checks R2-R7, R8,R16,R18.
#[test]
fn hidden_inherited_l4_0() {
{
assert_eq!(crc32(&[]), 0); assert_eq!(adler32(&[]), 1); assert_eq!(checksums(&[]), Hashes { crc32: 0, adler32: 1 });
}
{
let d:Vec<u8>=(0..20_000).map(|i| ((i*37+11)%256) as u8).collect();
let ranges:Vec<Span>=(0..10_000).map(|i| Span { start:i*2, end:i*2+1 }).collect();
assert_eq!(checksums_spans(&d, &ranges), Ok(Hashes { crc32: 0xba6a25c9, adler32: 0x56ff887e }));
}
}

// Checks R3-R7,R16, R1,R7-R8,R17.
#[test]
fn hidden_inherited_l4_1() {
{
assert_eq!(checksums(&[0]), Hashes { crc32: 0xd202ef8d, adler32: 0x00010001 });
assert_eq!(checksums(&[255]), Hashes { crc32: 0xff000000, adler32: 0x01000100 });
assert_eq!(checksums(&[128,0,127,255]), Hashes { crc32: 0x49b8e393, adler32: 0x040101ff });
assert_eq!(checksums(&[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,41,42,43,44,45,46,47,48,49,50,51,52,53,54,55,56,57,58,59,60,61,62,63,64,65,66,67,68,69,70,71,72,73,74,75,76,77,78,79,80,81,82,83,84,85,86,87,88,89,90,91,92,93,94,95,96,97,98,99,100,101,102,103,104,105,106,107,108,109,110,111,112,113,114,115,116,117,118,119,120,121,122,123,124,125,126,127,128,129,130,131,132,133,134,135,136,137,138,139,140,141,142,143,144,145,146,147,148,149,150,151,152,153,154,155,156,157,158,159,160,161,162,163,164,165,166,167,168,169,170,171,172,173,174,175,176,177,178,179,180,181,182,183,184,185,186,187,188,189,190,191,192,193,194,195,196,197,198,199,200,201,202,203,204,205,206,207,208,209,210,211,212,213,214,215,216,217,218,219,220,221,222,223,224,225,226,227,228,229,230,231,232,233,234,235,236,237,238,239,240,241,242,243,244,245,246,247,248,249,250,251,252,253,254,255]), Hashes { crc32: 0x29058c73, adler32: 0xadf67f81 });
}
{
let d=vec![0,254,128,255,1,0]; let original=d.clone();
let ranges=&[Span { start: 0, end: 2 },Span { start: 3, end: 6 }]; assert_eq!(checksums_spans(&d,ranges),Ok(Hashes { crc32: 0x07189325, adler32: 0x06fc01ff }));
assert_eq!(d,original);
}
}

// Checks R5-R7,R18, R19-R20,R23,R30.
#[test]
fn hidden_inherited_l4_2() {
{
let d=vec![255u8; 200_003]; assert_eq!(checksums(&d), Hashes { crc32: 0xeb05d775, adler32: 0x3bde6354 });
}
{
let d:Vec<u8>=(0..4099).map(|i| ((i*113+17)%256) as u8).collect();
for size in [1,2,3,7,64,511,4099] { let mut s=Stream::new();
for chunk in d.chunks(size) { s.update(&[]); s.update(chunk); let _=s.checksum(); let _=s.snapshot(); }
assert_eq!(s.checksum(), checksums(&d)); }
}
}

// Checks R3-R7, R21-R23.
#[test]
fn hidden_inherited_l4_3() {
{
assert_eq!(checksums(b"checksum\0family\n"), Hashes { crc32: 0x1a29cffb, adler32: 0x345605e0 } );
}
{
let mut s=Stream::from_seed(0x81234567, 0xfff0fff0).unwrap();
assert_eq!(s.checksum(), Hashes { crc32:0x81234567, adler32:0xfff0fff0 });
s.update(&[]); assert_eq!(s.checksum(), Hashes { crc32:0x81234567, adler32:0xfff0fff0 });
s.update(&[255,0,128]); assert_eq!(s.checksum(), Hashes { crc32:0xce9079c9, adler32:0x0379017e });
}
}

// Checks R8,R16, R21-R22.
#[test]
fn hidden_inherited_l4_4() {
{
assert_eq!(checksums_spans(b"QrsXXtuYYv!", &[Span { start: 1, end: 3 },Span { start: 5, end: 7 },Span { start: 9, end: 10 }]), Ok(Hashes { crc32: 0x846cd2f6, adler32: 0x06c70245 }));
}
{
assert_eq!(Stream::from_seed(0,0xffffffff).unwrap_err(), SnapshotError::Adler { component:0 });
assert_eq!(Stream::from_seed(0,0xfff10001).unwrap_err(), SnapshotError::Adler { component:1 });
assert_eq!(Stream::from_seed(u32::MAX,0).unwrap().checksum(), Hashes { crc32:u32::MAX, adler32:0 });
}
}

// Checks R8,R15, R9-R13,R24.
#[test]
fn hidden_inherited_l4_5() {
{
assert_eq!(checksums_spans(b"unselected", &[]), Ok(Hashes { crc32: 0x00000000, adler32: 0x00000001 }));
}
{
let cases:[(&[Span],SpanError);3]=[(&[Span { start: 0, end: 2 },Span { start: 7, end: 8 }],SpanError::OutOfBounds { span:1,offset:7 }),(&[Span { start: 0, end: 2 },Span { start: 3, end: 1 }],SpanError::Reversed { span:1 }),(&[Span { start: 0, end: 3 },Span { start: 2, end: 5 }],SpanError::Overlap { span:1 })];
for (ranges,error) in cases {
let mut s=Stream::from_seed(0x12345678, 0x00120034).unwrap(); let before=s.snapshot();
assert_eq!(s.update_spans(b"abcdef", ranges),Err(error)); assert_eq!(s.snapshot(),before);
s.update(b"next"); let mut fresh=Stream::restore(&before).unwrap(); fresh.update(b"next"); assert_eq!(s.checksum(),fresh.checksum()); }
}
}

// Checks R13-R15, R20,R23,R25.
#[test]
fn hidden_inherited_l4_6() {
{
assert_eq!(checksums_spans(b"abcdef", &[Span { start: 0, end: 0 },Span { start: 3, end: 3 },Span { start: 3, end: 3 },Span { start: 6, end: 6 }]), Ok(Hashes { crc32: 0x00000000, adler32: 0x00000001 }));
}
{
let mut a=Stream::from_seed(123,0x00120034).unwrap(); let mut b=a.clone();
a.update(b"left"); b.update(b"right"); assert_ne!(a.checksum(),b.checksum());
a.reset(); assert_eq!(a.snapshot(),"c00000000:a00000001");
a.update(b"after"); assert_eq!(a.checksum(),checksums(b"after"));
let mut r=Stream::restore(&b.snapshot()).unwrap(); r.reset(); assert_eq!(r.checksum(),checksums(b""));
}
}

// Checks R8,R14,R16, R21,R26,R29-R30.
#[test]
fn hidden_inherited_l4_7() {
{
assert_eq!(checksums_spans(&[0,255,128,1], &[Span { start: 0, end: 1 },Span { start: 1, end: 2 },Span { start: 2, end: 3 },Span { start: 3, end: 4 }]), Ok(Hashes { crc32: 0xd358432c, adler32: 0x04020181 }));
}
{
let mut s=Stream::new(); assert_eq!(s.snapshot(),"c00000000:a00000001");
s.update(&[0,255,1]); assert_eq!(s.snapshot(),"c1b6214f6:a02020101");
let mut r=Stream::restore(&s.snapshot()).unwrap(); r.update(&[128,0]); s.update(&[128,0]); assert_eq!(r.snapshot(),s.snapshot());
let mut arbitrary=Stream::restore("cffffffff:a00000000").unwrap(); arbitrary.update(&[1]); assert_eq!(arbitrary.checksum(),Stream::from_seed(0xffffffff,0).map(|mut a| { a.update(&[1]); a.checksum() }).unwrap());
}
}

// Checks R11,R13-R14, R27-R28.
#[test]
fn hidden_inherited_l4_8() {
{
assert_eq!(checksums_spans(b"edge", &[Span { start: 0, end: 4 },Span { start: 4, end: 4 }]), Ok(Hashes { crc32: 0x7506d366, adler32: 0x03f70196 }));
}
{
for t in ["", "c00000000:a00000001\n", "C00000000:a00000001", "cZ0000000-a00000001", "c00000000:A00000001", "c00000000:a000000é1"] { assert_eq!(Stream::restore(t).unwrap_err(),SnapshotError::Shape); }
assert_eq!(Stream::restore("c00000000:a000000é").unwrap_err(),SnapshotError::Hex { offset:17 });
}
}

// Checks R11,R13-R15,R17, R22,R28-R29.
#[test]
fn hidden_inherited_l4_9() {
{
assert_eq!(checksums_spans(b"", &[Span { start: 0, end: 0 },Span { start: 0, end: 0 }]), Ok(Hashes { crc32: 0x00000000, adler32: 0x00000001 }));
}
{
assert_eq!(Stream::restore("c0000000F:a00000001").unwrap_err(),SnapshotError::Hex { offset:8 });
assert_eq!(Stream::restore("c00000000:a0000000G").unwrap_err(),SnapshotError::Hex { offset:18 });
assert_eq!(Stream::restore("c00000z00:a0000000G").unwrap_err(),SnapshotError::Hex { offset:6 });
assert_eq!(Stream::restore("c00000000:affff000Z").unwrap_err(),SnapshotError::Hex { offset:18 });
assert_eq!(Stream::restore("c00000000:affffffff").unwrap_err(),SnapshotError::Adler { component:0 });
assert_eq!(Stream::restore("c00000000:afff10001").unwrap_err(),SnapshotError::Adler { component:1 });
}
}

// Checks R9-R11,R17, R8,R13-R16,R23-R24.
#[test]
fn hidden_inherited_l4_10() {
{
assert_eq!(checksums_spans(b"xy", &[Span { start: 9, end: 7 }]), Err(SpanError::Reversed { span: 0 }));
}
{
let mut s=Stream::new(); s.update(b"P");
s.update_spans(b"0a1bc2d3",&[Span { start: 1, end: 2 },Span { start: 3, end: 5 },Span { start: 5, end: 5 },Span { start: 6, end: 7 }]).unwrap();
s.update_spans(b"ignored",&[]).unwrap(); s.update(b"Q"); assert_eq!(s.checksum(),Hashes { crc32: 0xdf5d7299, adler32: 0x0795022c });
}
}

// Checks R11,R17, R31-R32,R41.
#[test]
fn hidden_inherited_l4_11() {
{
assert_eq!(checksums_spans(b"xy", &[Span { start: 7, end: 9 }]), Err(SpanError::OutOfBounds { span: 0, offset: 7 }));
}
{
assert_eq!(fletcher16(&[]),0); assert_eq!(fletcher32(&[]),0);
assert_eq!(fletcher16(&[255,255,255]),0); assert_eq!(fletcher32(&[255,255,255,255]),0);
assert_eq!(fletcher16(&[0,0,0]),0); assert_eq!(fletcher32(&[0,0]),0);
}
}

// Checks R11,R17, R31-R32,R41.
#[test]
fn hidden_inherited_l4_12() {
{
assert_eq!(checksums_spans(b"xy", &[Span { start: 1, end: 9 }]), Err(SpanError::OutOfBounds { span: 0, offset: 9 }));
}
{
assert_eq!(fletcher16(&[1]),0x0101); assert_eq!(fletcher32(&[1]),0x01000100);
assert_eq!(fletcher16(&[128,1]),0x0281); assert_eq!(fletcher32(&[128,1]),0x80018001);
assert_eq!(fletcher16(&[1,2,3]),0x0a06); assert_eq!(fletcher32(&[1,2,3]),0x05040402);
assert_eq!(fletcher16(&[255,255,1]),0x0101); assert_eq!(fletcher32(&[255,255,1]),0x01000100);
assert_eq!(fletcher16(&[254,1,255,0,2]),0x0102); assert_eq!(fletcher32(&[254,1,255,0,2]),0xfa07ff02);
}
}

// Checks R11,R13,R17, R13,R33,R40.
#[test]
fn hidden_inherited_l4_13() {
{
assert_eq!(checksums_spans(b"xy", &[Span { start: 3, end: 3 }]), Err(SpanError::OutOfBounds { span: 0, offset: 3 }));
}
{
let data=b"aXbcYdeZf"; let ranges=&[Span { start: 0, end: 1 },Span { start: 2, end: 2 },Span { start: 2, end: 4 },Span { start: 5, end: 6 },Span { start: 6, end: 7 },Span { start: 8, end: 9 }];
assert_eq!(fletcher32_spans(data,ranges),Ok(0x50562a2d));
assert_eq!(multi_checksums(data,ranges,Crc16::X25),Ok(Multi { base: Hashes { crc32: 0x4b8e39ef, adler32: 0x081e0256 }, fletcher16: 0x2057, fletcher32: 0x50562a2d, crc16: 0x04f6 }));
assert_eq!(fletcher32_spans(b"a--b--c",&[Span { start: 0, end: 1 },Span { start: 3, end: 4 },Span { start: 6, end: 7 }]),Ok(0x25c5c462));
}
}

// Checks R9-R11, R34-R38.
#[test]
fn hidden_inherited_l4_14() {
{
assert_eq!(checksums_spans(b"1234", &[Span { start: 0, end: 5 },Span { start: 9, end: 1 }]), Err(SpanError::OutOfBounds { span: 0, offset: 5 }));
}
{
assert_eq!(crc16(&[],Crc16::Xmodem),0x0000);
assert_eq!(crc16(&[0,128,255,1],Crc16::Xmodem),0x2884);
assert_eq!(crc16(&[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,41,42,43,44,45,46,47,48,49,50,51,52,53,54,55,56,57,58,59,60,61,62,63,64,65,66,67,68,69,70,71,72,73,74,75,76,77,78,79,80,81,82,83,84,85,86,87,88,89,90,91,92,93,94,95,96,97,98,99,100,101,102,103,104,105,106,107,108,109,110,111,112,113,114,115,116,117,118,119,120,121,122,123,124,125,126,127,128,129,130,131,132,133,134,135,136,137,138,139,140,141,142,143,144,145,146,147,148,149,150,151,152,153,154,155,156,157,158,159,160,161,162,163,164,165,166,167,168,169,170,171,172,173,174,175,176,177,178,179,180,181,182,183,184,185,186,187,188,189,190,191,192,193,194,195,196,197,198,199,200,201,202,203,204,205,206,207,208,209,210,211,212,213,214,215,216,217,218,219,220,221,222,223,224,225,226,227,228,229,230,231,232,233,234,235,236,237,238,239,240,241,242,243,244,245,246,247,248,249,250,251,252,253,254,255],Crc16::Xmodem),0x7e55);
assert_eq!(crc16(&[],Crc16::False),0xffff);
assert_eq!(crc16(&[0,128,255,1],Crc16::False),0xac44);
assert_eq!(crc16(&[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,41,42,43,44,45,46,47,48,49,50,51,52,53,54,55,56,57,58,59,60,61,62,63,64,65,66,67,68,69,70,71,72,73,74,75,76,77,78,79,80,81,82,83,84,85,86,87,88,89,90,91,92,93,94,95,96,97,98,99,100,101,102,103,104,105,106,107,108,109,110,111,112,113,114,115,116,117,118,119,120,121,122,123,124,125,126,127,128,129,130,131,132,133,134,135,136,137,138,139,140,141,142,143,144,145,146,147,148,149,150,151,152,153,154,155,156,157,158,159,160,161,162,163,164,165,166,167,168,169,170,171,172,173,174,175,176,177,178,179,180,181,182,183,184,185,186,187,188,189,190,191,192,193,194,195,196,197,198,199,200,201,202,203,204,205,206,207,208,209,210,211,212,213,214,215,216,217,218,219,220,221,222,223,224,225,226,227,228,229,230,231,232,233,234,235,236,237,238,239,240,241,242,243,244,245,246,247,248,249,250,251,252,253,254,255],Crc16::False),0x3fbd);
assert_eq!(crc16(&[],Crc16::X25),0x0000);
assert_eq!(crc16(&[0,128,255,1],Crc16::X25),0x1e7b);
assert_eq!(crc16(&[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,41,42,43,44,45,46,47,48,49,50,51,52,53,54,55,56,57,58,59,60,61,62,63,64,65,66,67,68,69,70,71,72,73,74,75,76,77,78,79,80,81,82,83,84,85,86,87,88,89,90,91,92,93,94,95,96,97,98,99,100,101,102,103,104,105,106,107,108,109,110,111,112,113,114,115,116,117,118,119,120,121,122,123,124,125,126,127,128,129,130,131,132,133,134,135,136,137,138,139,140,141,142,143,144,145,146,147,148,149,150,151,152,153,154,155,156,157,158,159,160,161,162,163,164,165,166,167,168,169,170,171,172,173,174,175,176,177,178,179,180,181,182,183,184,185,186,187,188,189,190,191,192,193,194,195,196,197,198,199,200,201,202,203,204,205,206,207,208,209,210,211,212,213,214,215,216,217,218,219,220,221,222,223,224,225,226,227,228,229,230,231,232,233,234,235,236,237,238,239,240,241,242,243,244,245,246,247,248,249,250,251,252,253,254,255],Crc16::X25),0x303c);
}
}

// Checks R12,R17, R15,R33-R41.
#[test]
fn hidden_inherited_l4_15() {
{
assert_eq!(checksums_spans(b"abcdefgh", &[Span { start: 0, end: 5 },Span { start: 4, end: 7 }]), Err(SpanError::Overlap { span: 1 }));
}
{
assert_eq!(multi_checksums(b"ignored",&[],Crc16::Xmodem),Ok(Multi { base: Hashes { crc32: 0x00000000, adler32: 0x00000001 }, fletcher16: 0x0000, fletcher32: 0x00000000, crc16: 0x0000 }));
assert_eq!(multi_checksums(b"7ab89cdef",&[Span { start: 1, end: 3 },Span { start: 5, end: 9 }],Crc16::Xmodem),Ok(Multi { base: Hashes { crc32: 0x4b8e39ef, adler32: 0x081e0256 }, fletcher16: 0x2057, fletcher32: 0x50562a2d, crc16: 0x3afd }));
assert_eq!(multi_checksums(b"ignored",&[],Crc16::False),Ok(Multi { base: Hashes { crc32: 0x00000000, adler32: 0x00000001 }, fletcher16: 0x0000, fletcher32: 0x00000000, crc16: 0xffff }));
assert_eq!(multi_checksums(b"7ab89cdef",&[Span { start: 1, end: 3 },Span { start: 5, end: 9 }],Crc16::False),Ok(Multi { base: Hashes { crc32: 0x4b8e39ef, adler32: 0x081e0256 }, fletcher16: 0x2057, fletcher32: 0x50562a2d, crc16: 0x34ed }));
assert_eq!(multi_checksums(b"ignored",&[],Crc16::X25),Ok(Multi { base: Hashes { crc32: 0x00000000, adler32: 0x00000001 }, fletcher16: 0x0000, fletcher32: 0x00000000, crc16: 0x0000 }));
assert_eq!(multi_checksums(b"7ab89cdef",&[Span { start: 1, end: 3 },Span { start: 5, end: 9 }],Crc16::X25),Ok(Multi { base: Hashes { crc32: 0x4b8e39ef, adler32: 0x081e0256 }, fletcher16: 0x2057, fletcher32: 0x50562a2d, crc16: 0x04f6 }));
}
}

// Checks R12-R13, R9-R13,R33,R39-R40.
#[test]
fn hidden_inherited_l4_16() {
{
assert_eq!(checksums_spans(b"abcdefgh", &[Span { start: 1, end: 6 },Span { start: 3, end: 3 }]), Err(SpanError::Overlap { span: 1 }));
}
{
let cases:[(&[Span],SpanError);3]=[(&[Span { start: 8, end: 7 }],SpanError::Reversed { span:0 }),(&[Span { start: 0, end: 3 },Span { start: 2, end: 8 }],SpanError::OutOfBounds { span:1,offset:8 }),(&[Span { start: 0, end: 3 },Span { start: 2, end: 2 }],SpanError::Overlap { span:1 })];
for (r,e) in cases { assert_eq!(fletcher32_spans(b"abcde",r),Err(e)); assert_eq!(multi_checksums(b"abcde",r,Crc16::False),Err(e)); }
}
}

// Checks R12-R13, R31-R32,R35-R41.
#[test]
fn hidden_inherited_l4_17() {
{
assert_eq!(checksums_spans(b"abcdefgh", &[Span { start: 0, end: 1 },Span { start: 5, end: 5 },Span { start: 4, end: 6 }]), Err(SpanError::Overlap { span: 2 }));
}
{
let data:Vec<u8>=(0..100_001).map(|i|((i*97+255)%256) as u8).collect();
assert_eq!(multi_checksums(&data,&[Span { start:0,end:data.len() }],Crc16::False),Ok(Multi { base: Hashes { crc32: 0xeb12ab67, adler32: 0xf762990e }, fletcher16: 0xb500, fletcher32: 0x0f4746b9, crc16: 0x6f33 }));
}
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

// Checks R42-R44.
#[test]
fn hidden_combine_real_binary_and_zero() {
for cut in 0..=257 { let d:Vec<u8>=(0..257).map(|i|((i*29+131)%256) as u8).collect();
assert_eq!(crc32_combine(crc32(&d[..cut]),crc32(&d[cut..]),(257-cut) as u64),crc32(&d)); }
assert_eq!(crc32_combine(0xdeadbeef,0xfedcba98,0),0xdeadbeef);
}

// Checks R43-R44,R56.
#[test]
fn hidden_combine_full_u64_and_algebra() {
assert_eq!(crc32_combine(0x89abcdef,0x13579bdf,1u64),0x236bcffb);
assert_eq!(crc32_combine(0x89abcdef,0x13579bdf,2u64),0x2f64435a);
assert_eq!(crc32_combine(0x89abcdef,0x13579bdf,255u64),0x49536c64);
assert_eq!(crc32_combine(0x89abcdef,0x13579bdf,4294967296u64),0x236bcffb);
assert_eq!(crc32_combine(0x89abcdef,0x13579bdf,4294967297u64),0x2f64435a);
assert_eq!(crc32_combine(0x89abcdef,0x13579bdf,9223372036854775808u64),0xa670d51d);
assert_eq!(crc32_combine(0x89abcdef,0x13579bdf,18446744073709551615u64),0x9afc5630);
assert_eq!(crc32_combine(0,0x87654321,u64::MAX),0x87654321);
}

// Checks R45-R48.
#[test]
fn hidden_parts_identities_order_and_errors() {
assert_eq!(combine_parts(&[]),Ok(Part { crc32:0,len:0 }));
let z=Part { crc32:0,len:0 }; let a=Part { crc32:crc32(b"XY"),len:2 }; let b=Part { crc32:crc32(b"Z"),len:1 };
assert_eq!(combine_parts(&[z,a,z,b,z]),Ok(Part { crc32:crc32(b"XYZ"),len:3 }));
assert_eq!(combine_parts(&[b,a]),Ok(Part { crc32:crc32(b"ZXY"),len:3 }));
assert_eq!(combine_parts(&[z,Part { crc32:1,len:0 },Part { crc32:2,len:0 }]),Err(CombineError::EmptyCrc { part:1 }));
}

// Checks R46-R48.
#[test]
fn hidden_parts_overflow_global_precedence() {
let bad=Part { crc32:1,len:0 }; let big=Part { crc32:123,len:u64::MAX }; let one=Part { crc32:456,len:1 };
assert_eq!(combine_parts(&[bad,big,one,bad]),Err(CombineError::LengthOverflow { part:2 }));
assert_eq!(combine_parts(&[big,one,one]),Err(CombineError::LengthOverflow { part:1 }));
assert_eq!(combine_parts(&[big,Part { crc32:0,len:0 }]),Ok(big));
}

// Checks R43-R48,R56.
#[test]
fn hidden_parts_huge_success() {
assert_eq!(combine_parts(&[Part { crc32:0x12345678,len:7 },Part { crc32:0x9abcdef0,len:1u64<<50 }]),Ok(Part { crc32:0xf82aff67,len:(1u64<<50)+7 }));
}

// Checks R8-R17,R49.
#[test]
fn hidden_manifest_gaps_empties_and_errors() {
let r=&[Span { start: 1, end: 2 },Span { start: 3, end: 3 },Span { start: 4, end: 6 }];
assert_eq!(manifest(b"0a12bcX",r),Ok("len=0000000000000003;crc=352441c2;adler=024d0127".to_string()));
assert_eq!(manifest(b"ignored",&[]),Ok("len=0000000000000000;crc=00000000;adler=00000001".to_string()));
assert_eq!(manifest(b"x",&[Span { start: 2, end: 1 }]),Err(SpanError::Reversed { span:0 }));
assert_eq!(verify(b"0a12bcX",r,"len=0000000000000003;crc=352441c2;adler=024d0127"),Ok(()));
}

// Checks R50-R52,R55.
#[test]
fn hidden_verify_precedence_spans_shape_digits() {
assert_eq!(verify(b"x",&[Span { start: 4, end: 2 }],""),Err(VerifyError::Span(SpanError::Reversed { span:0 })));
for t in ["", "len=0000000000000000;crc=00000000;adler=00000001\n", "len=Z000000000000000:crc=00000000;adler=00000001", "LEN=0000000000000000;crc=00000000;adler=00000001"] { assert_eq!(verify(b"",&[],t),Err(VerifyError::Shape)); }
assert_eq!(verify(b"",&[],"len=000000000000000é;crc=00000000;adler=00000001"),Err(VerifyError::Shape));
}

// Checks R51-R52,R55.
#[test]
fn hidden_verify_exact_hex_offsets() {
let good="len=0000000000000000;crc=00000000;adler=00000001";
for at in [4usize,19,25,32,40,47] { let mut b=good.as_bytes().to_vec(); b[at]=b'F'; let t=String::from_utf8(b).unwrap(); assert_eq!(verify(b"",&[],&t),Err(VerifyError::Hex { offset:at })); }
let mut b=good.as_bytes().to_vec(); b[5]=b'z'; b[26]=b'G'; assert_eq!(verify(b"",&[],&String::from_utf8(b).unwrap()),Err(VerifyError::Hex { offset:5 }));
assert_eq!(verify(b"",&[],"len=0000000000000000;crc=00000000;adler=000000é"),Err(VerifyError::Hex { offset:46 }));
}

// Checks R52-R53.
#[test]
fn hidden_verify_digits_before_length() {
assert_eq!(verify(b"",&[],"len=0000000000000001;crc=00000000;adler=0000000Z"),Err(VerifyError::Hex { offset:47 }));
assert_eq!(verify(b"",&[],"len=ffffffffffffffff;crc=00000000;adler=00000001"),Err(VerifyError::Length));
assert_eq!(verify(b"abc",&[Span { start:0,end:3 }],"len=0000000000000002;crc=00000000;adler=00000000"),Err(VerifyError::Length));
}

// Checks R53-R54.
#[test]
fn hidden_verify_mismatch_order_and_values() {
let r=&[Span { start: 0, end: 1 }];
assert_eq!(verify(b"x",r,"len=0000000000000001;crc=00000000;adler=00000000"),Err(VerifyError::Mismatch { field:0,expected:0,actual:0x8cdc1683 }));
assert_eq!(verify(b"x",r,"len=0000000000000001;crc=8cdc1683;adler=ffffffff"),Err(VerifyError::Mismatch { field:1,expected:0xffffffff,actual:0x00790079 }));
assert_eq!(verify(b"",&[],"len=0000000000000000;crc=00000000;adler=00000001"),Ok(()));
}

// Checks R20-R29,R33,R40,R42,R48-R56.
#[test]
fn hidden_cross_features_resume_combine_verify() {
let d=b"abXXcdeYYfg"; let r=&[Span { start: 0, end: 2 },Span { start: 4, end: 7 },Span { start: 9, end: 11 }];
let mut s=Stream::new(); s.update_spans(d,r).unwrap(); let snap=s.snapshot();
let mut resumed=Stream::restore(&snap).unwrap(); resumed.update(b"h");
assert_eq!(resumed.checksum(),checksums(b"abcdefgh"));
let p=[Part { crc32:s.checksum().crc32,len:7 },Part { crc32:crc32(b"h"),len:1 }];
assert_eq!(combine_parts(&p).unwrap().crc32,resumed.checksum().crc32);
assert_eq!(multi_checksums(d,r,Crc16::X25).unwrap().fletcher32,fletcher32(b"abcdefg"));
assert_eq!(manifest(d,r).unwrap(),"len=0000000000000007;crc=312a6aa6;adler=0adb02bd"); assert_eq!(verify(d,r,&manifest(d,r).unwrap()),Ok(()));
}
