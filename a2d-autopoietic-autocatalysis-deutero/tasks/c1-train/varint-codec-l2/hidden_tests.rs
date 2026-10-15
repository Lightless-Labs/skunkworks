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

// R10
#[test]
fn hidden_nonzero_start_truncated() {
    assert_eq!(decode_u64(&[9,9,129,128],2), Err(Error::Truncated{offset:4,start:2}));
}

// R11
#[test]
fn hidden_tenth_payload_overflow() {
    let mut b=vec![128;9]; b.push(2);
    assert_eq!(decode_u64(&b,0),Err(Error::Overflow{offset:9,start:0}));
}

// R12
#[test]
fn hidden_tenth_continuation() {
    for byte in [128,129] {
        let mut b=vec![128;9]; b.push(byte);
        assert_eq!(decode_u64(&b,0),Err(Error::Overflow{offset:9,start:0}));
    }
}

// R13
#[test]
fn hidden_overlong_small() {
    assert_eq!(decode_u64(&[129,128,0],0),Ok((1,3)));
    assert_eq!(decode_u64(&[128,0],0),Ok((0,2)));
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

// R19-R21
#[test]
fn hidden_signed_boundaries() {
    assert_eq!(encode_i64(i64::MIN),encode_u64(u64::MAX));
    assert_eq!(encode_i64(i64::MAX),encode_u64(u64::MAX-1));
    for v in [i64::MIN,i64::MAX,-65,-64,63,64,-1048576,1048576] {
        let b=encode_i64(v); assert_eq!(decode_i64(&b,0),Ok((v,b.len())));
    }
}

// R7-R8, R10-R12, R21
#[test]
fn hidden_signed_cursor_errors() {
    assert_eq!(decode_i64(&[5,129,0,77],1),Ok((-1,3)));
    assert_eq!(decode_i64(&[5,128],1),Err(Error::Truncated{offset:2,start:1}));
    assert_eq!(decode_i64(&[],2),Err(Error::StartOutOfBounds{offset:2}));
    assert_eq!(decode_i64(&[128;10],0),Err(Error::Overflow{offset:9,start:0}));
}

// R22-R23, R25, R28
#[test]
fn hidden_stream_initial() {
    let mut s=StreamDecoder::new(); assert_eq!(s.position(),0); assert_eq!(s.pending_len(),0);
    assert_eq!(s.push(&[]),Ok(vec![])); assert_eq!(s.finish(),Ok(()));
}

// R6, R22-R26
#[test]
fn hidden_stream_bytewise_max() {
    let mut s=StreamDecoder::new();
    for i in 0..9 { assert_eq!(s.push(&[255]),Ok(vec![])); assert_eq!(s.pending_len(),i+1); }
    assert_eq!(s.push(&[1,0]),Ok(vec![u64::MAX,0]));
    assert_eq!(s.position(),11); assert_eq!(s.pending_len(),0);
}

// R25, R28
#[test]
fn hidden_stream_finish_resume() {
    let mut s=StreamDecoder::new(); s.push(&[3,128,128]).unwrap();
    for _ in 0..2 { assert_eq!(s.finish(),Err(Error::Truncated{offset:3,start:1})); }
    assert_eq!(s.pending_len(),2); assert_eq!(s.push(&[]),Ok(vec![]));
    assert_eq!(s.push(&[1]),Ok(vec![16384])); assert_eq!(s.finish(),Ok(()));
}

// R12, R26-R27
#[test]
fn hidden_stream_cross_chunk_overflow() {
    let mut s=StreamDecoder::new(); s.push(&[7,8]).unwrap(); s.push(&[128;8]).unwrap();
    assert_eq!(s.push(&[128,129]),Err(Error::Overflow{offset:11,start:2}));
    assert_eq!(s.position(),10); assert_eq!(s.pending_len(),8);
    assert_eq!(s.push(&[0]),Ok(vec![0])); assert_eq!(s.position(),11);
}

// R26-R27
#[test]
fn hidden_stream_atomic_whole_chunk() {
    let mut s=StreamDecoder::new(); s.push(&[6]).unwrap();
    let mut b=vec![8,9]; b.extend([128;9]); b.push(2);
    assert_eq!(s.push(&b),Err(Error::Overflow{offset:12,start:3}));
    assert_eq!(s.position(),1); assert_eq!(s.pending_len(),0);
    assert_eq!(s.push(&[10]),Ok(vec![10])); assert_eq!(s.position(),2);
}

// R24, R27
#[test]
fn hidden_stream_atomic_pending_completion() {
    let mut s=StreamDecoder::new(); s.push(&[129]).unwrap();
    let mut b=vec![1,7]; b.extend([128;10]);
    assert_eq!(s.push(&b),Err(Error::Overflow{offset:12,start:3}));
    assert_eq!(s.position(),1); assert_eq!(s.pending_len(),1);
    assert_eq!(s.push(&[2]),Ok(vec![257]));
}

// R29
#[test]
fn hidden_stream_reset_pending() {
    let mut s=StreamDecoder::new(); s.push(&[1,129,128]).unwrap(); s.reset();
    assert_eq!(s.position(),0); assert_eq!(s.pending_len(),0); assert_eq!(s.finish(),Ok(()));
    assert_eq!(s.push(&[2]),Ok(vec![2]));
}

// R13, R24-R25
#[test]
fn hidden_stream_nonminimal() {
    let mut s=StreamDecoder::new(); s.push(&[128;9]).unwrap();
    assert_eq!(s.push(&[0,129,0]),Ok(vec![0,1]));
}

// R14, R18, R23-R24, R29
#[test]
fn hidden_stream_partition_invariance() {
    let values=vec![0,999,1u64<<63,127,u64::MAX,42,16384];
    let b:Vec<u8>=values.iter().flat_map(|v|encode_u64(*v)).collect();
    for width in 1..=b.len() {
        let mut s=StreamDecoder::new(); let mut got=Vec::new();
        for c in b.chunks(width) { got.extend(s.push(c).unwrap()); }
        assert_eq!(got,values); assert_eq!(s.position(),b.len()); assert_eq!(s.finish(),Ok(()));
    }
}

// R9, R11-R12, R14, R17
#[test]
fn hidden_batch_nonzero_and_overflow() {
    assert_eq!(decode_many(&[9,130,1,0],1,2),Ok((vec![130,0],4)));
    let mut b=vec![3,5]; b.extend([128;10]);
    assert_eq!(decode_many(&b,1,2),Err(Error::Overflow{offset:11,start:2}));
}

// R10, R16, R24-R25, R28
#[test]
fn hidden_short_streams() {
    assert_eq!(decode_u64(&[],0),Err(Error::Truncated{offset:0,start:0}));
    assert_eq!(decode_many(&[],0,0),Ok((vec![],0)));
    for n in 1..10 { let mut s=StreamDecoder::new(); s.push(&vec![128;n]).unwrap();
        assert_eq!(s.finish(),Err(Error::Truncated{offset:n,start:0})); }

}
