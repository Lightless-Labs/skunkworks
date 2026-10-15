// R1-R4, R8, R10, R27-R28
#[test]
fn hidden_frame_cursor() {
    assert_eq!(decode_message(&[99,0,128],1),Ok((Message{fields:vec![]},2)));
    assert_eq!(decode_message(&[0],2),Err(Error::StartOutOfBounds{offset:2}));
    assert_eq!(decode_message(&[],0),Err(Error::Truncated{offset:0,start:0}));
}

// R5, R11, R19, R21
#[test]
fn hidden_nonminimal_everywhere() {
    assert_eq!(decode_message(&[134,0,136,0,129,0,18,0],0),
        Ok((Message{fields:vec![Field::UInt{tag:1,value:1},Field::Bytes{tag:2,data:vec![]}]},8)));
    assert_eq!(decode_message(&[3,10,128,0],0),Ok((Message{fields:vec![Field::Bytes{tag:1,data:vec![]}]},4)));
}

// R6, R23
#[test]
fn hidden_header_overflow() {
    assert_eq!(decode_message(&[128;10],0),Err(Error::Overflow{offset:9,start:0}));
    let mut b=vec![255;9]; b.push(2); assert_eq!(decode_message(&b,0),Err(Error::Overflow{offset:9,start:0}));
}

// R9, R23
#[test]
fn hidden_frame_bounds_precedence() {
    assert_eq!(decode_message(&[88,6,0,0],1),Err(Error::LengthOutOfBounds{offset:1,length:6}));
    let mut b=vec![255;9]; b.push(1); assert_eq!(decode_message(&b,0),Err(Error::LengthOutOfBounds{offset:0,length:u64::MAX}));
}

// R7, R10, R19
#[test]
fn hidden_body_boundary_varint() {
    assert_eq!(decode_message(&[2,8,128,0],0),Err(Error::Truncated{offset:3,start:2}));
    assert_eq!(decode_message(&[1,128,0],0),Err(Error::Truncated{offset:2,start:1}));
}

// R11-R13, R23
#[test]
fn hidden_tag_precedes_wire() {
    assert_eq!(decode_message(&[1,7],0),Err(Error::InvalidTag{offset:1,tag:0}));
    assert_eq!(decode_message(&[2,135,16],0),Err(Error::InvalidTag{offset:1,tag:256}));
}

// R13-R14, R23
#[test]
fn hidden_wire_precedes_order() {
    assert_eq!(decode_message(&[3,16,1,11],0),Err(Error::InvalidWire{offset:3,wire:3}));
    for wire in [3,4,6,7] { assert_eq!(decode_message(&[1,8|wire],0),Err(Error::InvalidWire{offset:1,wire})); }
}

// R14, R23
#[test]
fn hidden_descending_precedes_payload() {
    assert_eq!(decode_message(&[3,24,1,8],0),Err(Error::OutOfOrder{offset:3,previous:3,tag:1}));
}

// R15, R23
#[test]
fn hidden_duplicate_precedes_payload() {
    assert_eq!(decode_message(&[3,8,5,8],0),Err(Error::Duplicate{offset:3,tag:1}));
}

// R15, R18, R30
#[test]
fn hidden_mixed_duplicate() {
    assert_eq!(decode_message(&[3,10,0,8],0),Err(Error::Duplicate{offset:3,tag:1}));
    assert_eq!(decode_message(&[3,8,0,10],0),Err(Error::Duplicate{offset:3,tag:1}));
}

// R15-R18, R21-R22
#[test]
fn hidden_bytes_merge() {
    let b=[11,10,0,10,2,255,128,10,1,0,18,0];
    assert_eq!(decode_message(&b,0),Ok((Message{fields:vec![Field::Bytes{tag:1,data:vec![255,128,0]},Field::Bytes{tag:2,data:vec![]}]},12)));
}

// R13, R16, R20, R28
#[test]
fn hidden_known_fixed_skip() {
    let b=[16,9,255,0,128,4,3,2,1,0,21,99,98,97,96,24,11];
    assert_eq!(decode_message(&b,0),Ok((Message{fields:vec![Field::UInt{tag:3,value:11}]},17)));
}

// R12-R13, R17, R20-R22, R28
#[test]
fn hidden_unknown_all_wires() {
    let b=[28,128,1,42,137,1,1,2,3,4,5,6,7,8,146,1,3,0,255,128,157,1,1,2,3,4,248,15,0];
    assert_eq!(decode_message(&b,0),Ok((Message{fields:vec![]},29)));
}

// R6, R17, R19
#[test]
fn hidden_skipped_overflow() {
    let mut b=vec![12,128,1]; b.extend([128;10]);
    assert_eq!(decode_message(&b,0),Err(Error::Overflow{offset:12,start:3}));
}

// R14, R17, R30
#[test]
fn hidden_skipped_order() {
    assert_eq!(decode_message(&[5,136,1,0,128,1],0),Err(Error::OutOfOrder{offset:4,previous:17,tag:16}));
}

// R15, R17, R30
#[test]
fn hidden_skipped_duplicate() {
    assert_eq!(decode_message(&[5,128,1,0,128,1],0),Err(Error::Duplicate{offset:4,tag:16}));
}

// R15, R17-R18, R28
#[test]
fn hidden_skipped_bytes_repeats() {
    assert_eq!(decode_message(&[6,130,1,0,130,1,0],0),Ok((Message{fields:vec![]},7)));
}

// R20
#[test]
fn hidden_fixed_bounds() {
    assert_eq!(decode_message(&[3,9,1,2,3,4,5,6,7,8],0),Err(Error::LengthOutOfBounds{offset:1,length:8}));
    assert_eq!(decode_message(&[1,13],0),Err(Error::LengthOutOfBounds{offset:1,length:4}));
}

// R7, R21
#[test]
fn hidden_byte_length_bounds() {
    assert_eq!(decode_message(&[3,10,5,42,1,2,3,4],0),Err(Error::LengthOutOfBounds{offset:2,length:5}));
    assert_eq!(decode_message(&[2,10,128,0],0),Err(Error::Truncated{offset:3,start:2}));
}

// R6, R21
#[test]
fn hidden_byte_length_overflow() {
    let mut b=vec![11,10]; b.extend([128;10]);
    assert_eq!(decode_message(&b,0),Err(Error::Overflow{offset:11,start:2}));
}

// R24-R25, R29
#[test]
fn hidden_encode_invalid_tags() {
    for tag in [0,16,u32::MAX] { let m=Message{fields:vec![Field::UInt{tag,value:0}]};
        assert_eq!(encode_message(&m),Err(Error::InvalidTag{offset:0,tag:tag as u64})); }
    let m=Message{fields:vec![Field::Bytes{tag:2,data:vec![]},Field::Bytes{tag:0,data:vec![]}]};
    assert_eq!(encode_message(&m),Err(Error::InvalidTag{offset:1,tag:0}));
}

// R25
#[test]
fn hidden_encode_order_duplicate() {
    let m=Message{fields:vec![Field::UInt{tag:4,value:1},Field::UInt{tag:2,value:9}]};
    assert_eq!(encode_message(&m),Err(Error::OutOfOrder{offset:1,previous:4,tag:2}));
    let m=Message{fields:vec![Field::Bytes{tag:3,data:vec![]},Field::UInt{tag:3,value:1}]};
    assert_eq!(encode_message(&m),Err(Error::Duplicate{offset:1,tag:3}));
}

// R18, R26-R27
#[test]
fn hidden_encoder_preserves_fragments() {
    let m=Message{fields:vec![Field::Bytes{tag:2,data:vec![5]},Field::Bytes{tag:2,data:vec![]},Field::Bytes{tag:2,data:vec![6,7]}]};
    let snapshot=m.clone(); let b=encode_message(&m).unwrap();
    assert_eq!(b,vec![9,18,1,5,18,0,18,2,6,7]); assert_eq!(m,snapshot);
    assert_eq!(decode_message(&b,0),Ok((Message{fields:vec![Field::Bytes{tag:2,data:vec![5,6,7]}]},10)));
}

// R4-R6, R16, R19, R22, R26-R27, R29
#[test]
fn hidden_max_value_and_large_body() {
    let m=Message{fields:vec![Field::UInt{tag:1,value:u64::MAX},Field::Bytes{tag:15,data:(0..600).map(|i|i as u8).collect()}]};
    let b=encode_message(&m).unwrap(); assert!(b[0]&128!=0);
    assert_eq!(decode_message(&b,0),Ok((m,b.len())));
}

// R7-R8, R12-R15, R23
#[test]
fn hidden_absolute_offsets() {
    assert_eq!(decode_message(&[99,99,3,16,0,8],2),Err(Error::OutOfOrder{offset:5,previous:2,tag:1}));
    assert_eq!(decode_message(&[99,2,8,128,0],1),Err(Error::Truncated{offset:4,start:3}));
}

// R15-R16, R30
#[test]
fn hidden_known_fixed_duplicate() {
    let mut b=vec![10,9]; b.extend([0;8]); b.push(10);
    assert_eq!(decode_message(&b,0),Err(Error::Duplicate{offset:10,tag:1}));
}

// R6-R7, R11-R12, R23
#[test]
fn hidden_full_key_before_tag() {
    let mut b=vec![10]; b.extend([255;10]);
    assert_eq!(decode_message(&b,0),Err(Error::Overflow{offset:10,start:1}));
    assert_eq!(decode_message(&[1,255],0),Err(Error::Truncated{offset:2,start:1}));
}

// R17, R21, R23, R28-R30
#[test]
fn hidden_skipped_length_validation() {
    assert_eq!(decode_message(&[3,130,1,9],0),Err(Error::LengthOutOfBounds{offset:3,length:9}));
    assert_eq!(decode_message(&[2,130,1],0),Err(Error::Truncated{offset:3,start:3}));
}
