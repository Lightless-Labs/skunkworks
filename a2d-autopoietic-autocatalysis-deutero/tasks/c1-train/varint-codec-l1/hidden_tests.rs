// R1-R5
#[test]
fn hidden_zero_exact() {
    assert_eq!(encode_u64(0), vec![0]);
}

// R4-R6
#[test]
fn hidden_thresholds() {
    assert_eq!(encode_u64(127), vec![127]);
    assert_eq!(encode_u64(128), vec![128,1]);
    assert_eq!(encode_u64(16383), vec![255,127]);
    assert_eq!(encode_u64(16384), vec![128,128,1]);
}

// R6, R11
#[test]
fn hidden_maximum() {
    let mut b = vec![255;9]; b.push(1);
    assert_eq!(encode_u64(u64::MAX), b);
    assert_eq!(decode_u64(&b,0), Ok((u64::MAX,10)));
}

// R4, R6
#[test]
fn hidden_high_bit() {
    let mut b = vec![128;9]; b.push(1);
    assert_eq!(encode_u64(1u64<<63), b);
    assert_eq!(decode_u64(&b,0), Ok((1u64<<63,10)));
}

// R3, R7
#[test]
fn hidden_invalid_start() {
    assert_eq!(decode_u64(&[4],2), Err(Error::StartOutOfBounds{offset:2}));
    assert_eq!(decode_u64(&[],usize::MAX), Err(Error::StartOutOfBounds{offset:usize::MAX}));
}

// R8, R10
#[test]
fn hidden_end_cursor() {
    assert_eq!(decode_u64(&[2,3],2), Err(Error::Truncated{offset:2,start:2}));
}

// R8, R10
#[test]
fn hidden_empty_decode() {
    assert_eq!(decode_u64(&[],0), Err(Error::Truncated{offset:0,start:0}));
}

// R9
#[test]
fn hidden_trailing_garbage() {
    assert_eq!(decode_u64(&[12,255,255],0), Ok((12,1)));
}

// R10
#[test]
fn hidden_nonzero_start_truncated() {
    assert_eq!(decode_u64(&[9,9,129,128],2), Err(Error::Truncated{offset:4,start:2}));
}

// R10, R12
#[test]
fn hidden_every_short_truncation() {
    for n in 1..10 {
        let b = vec![128;n];
        assert_eq!(decode_u64(&b,0),Err(Error::Truncated{offset:n,start:0}));
    }
}

// R11
#[test]
fn hidden_tenth_payload_overflow() {
    let mut b=vec![128;9]; b.push(2);
    assert_eq!(decode_u64(&b,0),Err(Error::Overflow{offset:9,start:0}));
}

// R11-R12
#[test]
fn hidden_tenth_all_payloads() {
    for byte in 2..=255 {
        let mut b=vec![128;9]; b.push(byte);
        assert_eq!(decode_u64(&b,0),Err(Error::Overflow{offset:9,start:0}));
    }
}

// R12
#[test]
fn hidden_tenth_continuation() {
    for byte in [128,129] {
        let mut b=vec![128;9]; b.push(byte);
        assert_eq!(decode_u64(&b,0),Err(Error::Overflow{offset:9,start:0}));
    }
}

// R11-R12
#[test]
fn hidden_overflow_absolute() {
    let mut b=vec![7,8,9]; b.extend([128;9]); b.push(130); b.push(0);
    assert_eq!(decode_u64(&b,3),Err(Error::Overflow{offset:12,start:3}));
}

// R13
#[test]
fn hidden_overlong_small() {
    assert_eq!(decode_u64(&[129,128,0],0),Ok((1,3)));
    assert_eq!(decode_u64(&[128,0],0),Ok((0,2)));
}

// R6, R13
#[test]
fn hidden_overlong_ten() {
    let mut b=vec![128;9]; b.push(0);
    assert_eq!(decode_u64(&b,0),Ok((0,10)));
}

// R14
#[test]
fn hidden_batch_order() {
    assert_eq!(decode_many(&[0,255,1,2,128,2],0,4),Ok((vec![0,255,2,256],6)));
}

// R15
#[test]
fn hidden_invalid_zero_count() {
    assert_eq!(decode_many(&[6],2,0),Err(Error::StartOutOfBounds{offset:2}));
}

// R16
#[test]
fn hidden_zero_count_ignores_bytes() {
    assert_eq!(decode_many(&[128,128],1,0),Ok((vec![],1)));
    assert_eq!(decode_many(&[3],1,0),Ok((vec![],1)));
}

// R10, R17
#[test]
fn hidden_batch_truncation_start() {
    assert_eq!(decode_many(&[1,2,128],0,3),Err(Error::Truncated{offset:3,start:2}));
}

// R11-R12, R17
#[test]
fn hidden_batch_overflow_start() {
    let mut b=vec![6,6]; b.extend([128;10]);
    assert_eq!(decode_many(&b,1,2),Err(Error::Overflow{offset:11,start:2}));
}

// R18
#[test]
fn hidden_batch_stops_at_count() {
    assert_eq!(decode_many(&[128,1,128],0,1),Ok((vec![128],2)));
}

// R17-R18
#[test]
fn hidden_huge_count_empty() {
    assert_eq!(decode_many(&[],0,usize::MAX),Err(Error::Truncated{offset:0,start:0}));
}

// R4-R6, R9, R14, R18
#[test]
fn hidden_dense_roundtrips() {
    let values:Vec<u64>=(0..4096).map(|i| (i*7919) as u64).collect();
    let b:Vec<u8>=values.iter().flat_map(|v|encode_u64(*v)).collect();
    assert_eq!(decode_many(&b,0,values.len()),Ok((values,b.len())));
}
