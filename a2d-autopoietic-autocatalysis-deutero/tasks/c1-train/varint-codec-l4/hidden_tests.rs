// R1-R4, R8, R10, R27-R28
#[test]
fn hidden_frame_cursor_strict() {
    assert_eq!(decode_message(&[99,0,128],1,4),Ok((Message{fields:vec![]},2)));
    assert_eq!(decode_message(&[0],2,4),Err(Error::StartOutOfBounds{offset:2}));
    assert_eq!(decode_message(&[],0,4),Err(Error::Truncated{offset:0,start:0}));
}

// R6, R23
#[test]
fn hidden_header_overflow_strict() {
    assert_eq!(decode_message(&[128;10],0,4),Err(Error::Overflow{offset:9,start:0}));
    let mut b=vec![255;9]; b.push(2); assert_eq!(decode_message(&b,0,4),Err(Error::Overflow{offset:9,start:0}));
}

// R9, R23
#[test]
fn hidden_frame_bounds_precedence_strict() {
    assert_eq!(decode_message(&[88,6,0,0],1,4),Err(Error::LengthOutOfBounds{offset:1,length:6}));
    let mut b=vec![255;9]; b.push(1); assert_eq!(decode_message(&b,0,4),Err(Error::LengthOutOfBounds{offset:0,length:u64::MAX}));
}

// R7, R10, R19
#[test]
fn hidden_body_boundary_varint_strict() {
    assert_eq!(decode_message(&[2,8,128,0],0,4),Err(Error::Truncated{offset:3,start:2}));
    assert_eq!(decode_message(&[1,128,0],0,4),Err(Error::Truncated{offset:2,start:1}));
}

// R11-R13, R23
#[test]
fn hidden_tag_precedes_wire_strict() {
    assert_eq!(decode_message(&[1,7],0,4),Err(Error::InvalidTag{offset:1,tag:0}));
    assert_eq!(decode_message(&[2,135,16],0,4),Err(Error::InvalidTag{offset:1,tag:256}));
}

// R14, R23
#[test]
fn hidden_descending_precedes_payload_strict() {
    assert_eq!(decode_message(&[3,24,1,8],0,4),Err(Error::OutOfOrder{offset:3,previous:3,tag:1}));
}

// R15, R23
#[test]
fn hidden_duplicate_precedes_payload_strict() {
    assert_eq!(decode_message(&[3,8,5,8],0,4),Err(Error::Duplicate{offset:3,tag:1}));
}

// R15, R18, R30
#[test]
fn hidden_mixed_duplicate_strict() {
    assert_eq!(decode_message(&[3,10,0,8],0,4),Err(Error::Duplicate{offset:3,tag:1}));
    assert_eq!(decode_message(&[3,8,0,10],0,4),Err(Error::Duplicate{offset:3,tag:1}));
}

// R15-R18, R21-R22
#[test]
fn hidden_bytes_merge_strict() {
    let b=[11,10,0,10,2,255,128,10,1,0,18,0];
    assert_eq!(decode_message(&b,0,4),Ok((Message{fields:vec![Field::Bytes{tag:1,data:vec![255,128,0]},Field::Bytes{tag:2,data:vec![]}]},12)));
}

// R13, R16, R20, R28
#[test]
fn hidden_known_fixed_skip_strict() {
    let b=[16,9,255,0,128,4,3,2,1,0,21,99,98,97,96,24,11];
    assert_eq!(decode_message(&b,0,4),Ok((Message{fields:vec![Field::UInt{tag:3,value:11}]},17)));
}

// R12-R13, R17, R20-R22, R28
#[test]
fn hidden_unknown_all_wires_strict() {
    let b=[28,128,1,42,137,1,1,2,3,4,5,6,7,8,146,1,3,0,255,128,157,1,1,2,3,4,248,15,0];
    assert_eq!(decode_message(&b,0,4),Ok((Message{fields:vec![]},29)));
}

// R6, R17, R19
#[test]
fn hidden_skipped_overflow_strict() {
    let mut b=vec![12,128,1]; b.extend([128;10]);
    assert_eq!(decode_message(&b,0,4),Err(Error::Overflow{offset:12,start:3}));
}

// R14, R17, R30
#[test]
fn hidden_skipped_order_strict() {
    assert_eq!(decode_message(&[5,136,1,0,128,1],0,4),Err(Error::OutOfOrder{offset:4,previous:17,tag:16}));
}

// R15, R17, R30
#[test]
fn hidden_skipped_duplicate_strict() {
    assert_eq!(decode_message(&[5,128,1,0,128,1],0,4),Err(Error::Duplicate{offset:4,tag:16}));
}

// R20
#[test]
fn hidden_fixed_bounds_strict() {
    assert_eq!(decode_message(&[3,9,1,2,3,4,5,6,7,8],0,4),Err(Error::LengthOutOfBounds{offset:1,length:8}));
    assert_eq!(decode_message(&[1,13],0,4),Err(Error::LengthOutOfBounds{offset:1,length:4}));
}

// R7, R21
#[test]
fn hidden_byte_length_bounds_strict() {
    assert_eq!(decode_message(&[3,10,5,42,1,2,3,4],0,4),Err(Error::LengthOutOfBounds{offset:2,length:5}));
    assert_eq!(decode_message(&[2,10,128,0],0,4),Err(Error::Truncated{offset:3,start:2}));
}

// R6, R21
#[test]
fn hidden_byte_length_overflow_strict() {
    let mut b=vec![11,10]; b.extend([128;10]);
    assert_eq!(decode_message(&b,0,4),Err(Error::Overflow{offset:11,start:2}));
}

// R24-R25, R29
#[test]
fn hidden_encode_invalid_tags_strict() {
    for tag in [0,16,u32::MAX] { let m=Message{fields:vec![Field::UInt{tag,value:0}]};
        assert_eq!(encode_message(&m,4),Err(Error::InvalidTag{offset:0,tag:tag as u64})); }
    let m=Message{fields:vec![Field::Bytes{tag:2,data:vec![]},Field::Bytes{tag:0,data:vec![]}]};
    assert_eq!(encode_message(&m,4),Err(Error::InvalidTag{offset:1,tag:0}));
}

// R25
#[test]
fn hidden_encode_order_duplicate_strict() {
    let m=Message{fields:vec![Field::UInt{tag:4,value:1},Field::UInt{tag:2,value:9}]};
    assert_eq!(encode_message(&m,4),Err(Error::OutOfOrder{offset:1,previous:4,tag:2}));
    let m=Message{fields:vec![Field::Bytes{tag:3,data:vec![]},Field::UInt{tag:3,value:1}]};
    assert_eq!(encode_message(&m,4),Err(Error::Duplicate{offset:1,tag:3}));
}

// R18, R26-R27
#[test]
fn hidden_encoder_preserves_fragments_strict() {
    let m=Message{fields:vec![Field::Bytes{tag:2,data:vec![5]},Field::Bytes{tag:2,data:vec![]},Field::Bytes{tag:2,data:vec![6,7]}]};
    let snapshot=m.clone(); let b=encode_message(&m,4).unwrap();
    assert_eq!(b,vec![9,18,1,5,18,0,18,2,6,7]); assert_eq!(m,snapshot);
    assert_eq!(decode_message(&b,0,4),Ok((Message{fields:vec![Field::Bytes{tag:2,data:vec![5,6,7]}]},10)));
}

// R4-R6, R16, R19, R22, R26-R27, R29
#[test]
fn hidden_max_value_and_large_body_strict() {
    let m=Message{fields:vec![Field::UInt{tag:1,value:u64::MAX},Field::Bytes{tag:15,data:(0..600).map(|i|i as u8).collect()}]};
    let b=encode_message(&m,4).unwrap(); assert!(b[0]&128!=0);
    assert_eq!(decode_message(&b,0,4),Ok((m,b.len())));
}

// R5-R7, R11, R17, R19, R21, R23, R31-R33
#[test]
fn hidden_all_canonical_sites() {
    let cases:Vec<(Vec<u8>,usize)>=vec![
        (vec![128,0],0),
        (vec![3,136,0,0],1),
        (vec![3,8,128,0],2),
        (vec![3,10,128,0],2),
        (vec![4,128,1,128,0],3),
        (vec![4,130,1,128,0],3),
        (vec![3,11,128,0],2),
        (vec![4,131,1,128,0],3),
    ];
    for (b,offset) in cases { assert_eq!(decode_message(&b,0,4),Err(Error::NonCanonical{offset})); }
    assert_eq!(decode_message(&[2,128,0],0,4),Err(Error::NonCanonical{offset:1}));
    assert_eq!(decode_message(&[3,10,137,0],0,4),Err(Error::NonCanonical{offset:2}));
}

// R6-R7, R31-R33
#[test]
fn hidden_canonical_tenth_and_precedence() {
    let mut b=vec![11,8]; b.extend([128;9]); b.push(0);
    assert_eq!(decode_message(&b,0,4),Err(Error::NonCanonical{offset:2}));
    b[11]=128; assert_eq!(decode_message(&b,0,4),Err(Error::Overflow{offset:11,start:2}));
    assert_eq!(decode_message(&[2,8,128],0,4),Err(Error::Truncated{offset:3,start:2}));
}

// R8, R23, R35-R36
#[test]
fn hidden_depth_limit_argument() {
    for limit in [33,usize::MAX] {
        assert_eq!(decode_message(&[],usize::MAX,limit),Err(Error::InvalidDepthLimit{limit}));
        assert_eq!(encode_message(&Message{fields:vec![]},limit),Err(Error::InvalidDepthLimit{limit}));
    }
    assert_eq!(decode_message(&[2,8,1],0,0),Ok((Message{fields:vec![Field::UInt{tag:1,value:1}]},3)));
    assert_eq!(decode_message(&[2,11,0],0,0),Err(Error::DepthExceeded{offset:1,depth:1}));
    assert_eq!(decode_message(&[3,131,1,0],0,0),Err(Error::DepthExceeded{offset:1,depth:1}));
}

// R34-R36, R38-R39, R43
#[test]
fn hidden_nested_exact_boundary() {
    let b=[4,11,2,11,0];
    let m=Message{fields:vec![Field::Nested{tag:1,message:Message{fields:vec![Field::Nested{tag:1,message:Message{fields:vec![]}}]}}]};
    assert_eq!(decode_message(&b,0,2),Ok((m.clone(),5)));
    assert_eq!(encode_message(&m,2),Ok(b.to_vec()));
    assert_eq!(decode_message(&b,0,1),Err(Error::DepthExceeded{offset:3,depth:2}));
    assert_eq!(encode_message(&m,1),Err(Error::DepthExceeded{offset:0,depth:2}));
}

// R7, R21, R23, R31-R33, R37
#[test]
fn hidden_nested_length_before_depth() {
    assert_eq!(decode_message(&[1,11],0,0),Err(Error::Truncated{offset:2,start:2}));
    assert_eq!(decode_message(&[2,11,9],0,0),Err(Error::LengthOutOfBounds{offset:2,length:9}));
    assert_eq!(decode_message(&[3,11,128,0],0,0),Err(Error::NonCanonical{offset:2}));
    let mut b=vec![11,11]; b.extend([128;10]);
    assert_eq!(decode_message(&b,0,0),Err(Error::Overflow{offset:11,start:2}));
    assert_eq!(decode_message(&[3,11,1,7],0,0),Err(Error::DepthExceeded{offset:1,depth:1}));
}

// R7-R8, R21, R34, R38
#[test]
fn hidden_child_local_end_and_absolute_offsets() {
    assert_eq!(decode_message(&[4,11,1,128,0],0,4),Err(Error::Truncated{offset:4,start:3}));
    assert_eq!(decode_message(&[4,11,2,10,2],0,4),Err(Error::LengthOutOfBounds{offset:4,length:2}));
    assert_eq!(decode_message(&[99,99,3,11,1,7],2,4),Err(Error::InvalidTag{offset:5,tag:0}));
    assert_eq!(decode_message(&[3,11,1,12],0,4),Err(Error::InvalidWire{offset:3,wire:4}));
}

// R17, R31, R34-R35, R38-R39
#[test]
fn hidden_unknown_children_validated() {
    assert_eq!(decode_message(&[4,131,1,1,7],0,4),Err(Error::InvalidTag{offset:4,tag:0}));
    assert_eq!(decode_message(&[6,131,1,3,8,128,0],0,4),Err(Error::NonCanonical{offset:5}));
    assert_eq!(decode_message(&[5,131,1,2,11,0],0,1),Err(Error::DepthExceeded{offset:4,depth:2}));
    assert_eq!(decode_message(&[5,131,1,2,8,3],0,4),Ok((Message{fields:vec![]},6)));
}

// R14-R18, R30, R34, R38-R40
#[test]
fn hidden_nested_order_and_repeats() {
    let b=[8,11,4,40,9,48,8,16,1];
    assert_eq!(decode_message(&b,0,4),Ok((Message{fields:vec![Field::Nested{tag:1,message:Message{fields:vec![Field::UInt{tag:5,value:9},Field::UInt{tag:6,value:8}]}},Field::UInt{tag:2,value:1}]},9)));
    for b in [vec![3,11,0,11],vec![3,11,0,10],vec![3,10,0,11]] {
        assert_eq!(decode_message(&b,0,1),Err(Error::Duplicate{offset:3,tag:1}));
    }
    assert_eq!(decode_message(&[6,11,4,16,0,8,0],0,4),Err(Error::OutOfOrder{offset:5,previous:2,tag:1}));
}

// R24-R25, R35-R36, R40-R42
#[test]
fn hidden_encoder_nested_precedence() {
    let bad=Message{fields:vec![Field::UInt{tag:0,value:0}]};
    let m=Message{fields:vec![Field::Nested{tag:1,message:bad.clone()}]};
    assert_eq!(encode_message(&m,0),Err(Error::DepthExceeded{offset:0,depth:1}));
    assert_eq!(encode_message(&m,1),Err(Error::InvalidTag{offset:0,tag:0}));
    let m=Message{fields:vec![Field::UInt{tag:2,value:0},Field::Nested{tag:1,message:bad.clone()}]};
    assert_eq!(encode_message(&m,0),Err(Error::OutOfOrder{offset:1,previous:2,tag:1}));
    let m=Message{fields:vec![Field::Nested{tag:0,message:bad.clone()}]};
    assert_eq!(encode_message(&m,0),Err(Error::InvalidTag{offset:0,tag:0}));
    let m=Message{fields:vec![Field::Bytes{tag:1,data:vec![]},Field::Nested{tag:1,message:bad.clone()}]};
    assert_eq!(encode_message(&m,0),Err(Error::Duplicate{offset:1,tag:1}));
    let m=Message{fields:vec![Field::Nested{tag:1,message:Message{fields:vec![Field::UInt{tag:1,value:0},Field::UInt{tag:1,value:1}]}},Field::UInt{tag:0,value:0}]};
    assert_eq!(encode_message(&m,4),Err(Error::Duplicate{offset:1,tag:1}));
}

// R20-R22, R27-R29, R34-R36, R38-R39, R43
#[test]
fn hidden_opaque_and_maximum_depth() {
    let mut m=Message{fields:vec![Field::Bytes{tag:1,data:vec![128,0,255,255]},Field::UInt{tag:2,value:u64::MAX}]};
    for _ in 0..32 { m=Message{fields:vec![Field::Nested{tag:1,message:m}]}; }
    let b=encode_message(&m,32).unwrap(); assert_eq!(decode_message(&b,0,32),Ok((m.clone(),b.len())));
    assert_eq!(encode_message(&m,31),Err(Error::DepthExceeded{offset:0,depth:32}));
    let m=Message{fields:vec![Field::Nested{tag:1,message:Message{fields:vec![]}}]};
    assert_eq!(encode_message(&m,1),Ok(vec![2,11,0]));
    assert_eq!(decode_message(&[5,11,3,128,1,0],0,1),Ok((m,6)));
    assert_eq!(decode_message(&[4,10,2,128,0],0,0),Ok((Message{fields:vec![Field::Bytes{tag:1,data:vec![128,0]}]},5)));
    assert_eq!(decode_message(&[5,13,128,0,255,0],0,0),Ok((Message{fields:vec![]},6)));
}

// R12-R14, R23, R30
#[test]
fn hidden_strict_wire_precedence() {
    assert_eq!(decode_message(&[3,16,1,12],0,4),Err(Error::InvalidWire{offset:3,wire:4}));
    for wire in [4,6,7] { assert_eq!(decode_message(&[1,8|wire],0,4),Err(Error::InvalidWire{offset:1,wire})); }
    assert_eq!(decode_message(&[5,128,1,0,130,1],0,4),Err(Error::Duplicate{offset:4,tag:16}));
}

// R15-R18, R20, R28, R30
#[test]
fn hidden_unknown_fragments_and_fixed_repeats() {
    assert_eq!(decode_message(&[6,130,1,0,130,1,0],0,4),Ok((Message{fields:vec![]},7)));
    let mut b=vec![10,9]; b.extend([0;8]); b.push(10);
    assert_eq!(decode_message(&b,0,4),Err(Error::Duplicate{offset:10,tag:1}));
    assert_eq!(decode_message(&[5,136,1,0,138,1],0,4),Err(Error::Duplicate{offset:4,tag:17}));
}

// R6-R7, R11-R14, R17, R23, R31-R33
#[test]
fn hidden_key_integrity_and_canonical_priority() {
    assert_eq!(decode_message(&[4,128,129,0,0],0,4),Err(Error::NonCanonical{offset:1}));
    assert_eq!(decode_message(&[5,16,0,136,0,0],0,4),Err(Error::NonCanonical{offset:3}));
    assert_eq!(decode_message(&[2,140,0],0,4),Err(Error::NonCanonical{offset:1}));
    assert_eq!(decode_message(&[3,128,128,0],0,4),Err(Error::NonCanonical{offset:1}));
    assert_eq!(decode_message(&[99,1,255],1,4),Err(Error::Truncated{offset:3,start:2}));
    let mut b=vec![10]; b.extend([255;9]); b.push(1);
    assert_eq!(decode_message(&b,0,4),Err(Error::InvalidTag{offset:1,tag:u64::MAX>>3}));
}

// R7, R20, R23-R25, R34, R38, R41-R43
#[test]
fn hidden_nested_container_and_encoder_local_index() {
    assert_eq!(decode_message(&[3,11,1,13],0,4),Err(Error::LengthOutOfBounds{offset:3,length:4}));
    assert_eq!(decode_message(&[3,11,1,0],0,4),Err(Error::InvalidTag{offset:3,tag:0}));
    let m=Message{fields:vec![Field::UInt{tag:1,value:5},Field::Nested{tag:2,message:Message{fields:vec![Field::UInt{tag:1,value:6},Field::Bytes{tag:3,data:vec![]},Field::UInt{tag:2,value:7}]}}]};
    assert_eq!(encode_message(&m,4),Err(Error::OutOfOrder{offset:2,previous:3,tag:2}));
}
