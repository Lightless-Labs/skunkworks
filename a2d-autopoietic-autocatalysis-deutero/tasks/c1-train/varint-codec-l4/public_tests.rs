// R1-R5, R27-R28, R36
#[test]
fn public_empty_canonical() {
    let m=Message{fields:vec![]}; assert_eq!(encode_message(&m,0),Ok(vec![0]));
    assert_eq!(decode_message(&[0],0,0),Ok((m,1)));
}

// R2-R3, R10-R11, R16, R19, R21-R22, R24-R27, R34-R35, R38-R39, R41-R43
#[test]
fn public_nested_roundtrip() {
    let m=Message{fields:vec![Field::Nested{tag:1,message:Message{fields:vec![Field::UInt{tag:2,value:9}]}},Field::Bytes{tag:3,data:vec![128,0]}]};
    let b=vec![8,11,2,16,9,26,2,128,0];
    assert_eq!(encode_message(&m,3),Ok(b.clone()));
    assert_eq!(decode_message(&b,0,3),Ok((m,9)));
}

// R12-R18, R28-R30, R38-R39
#[test]
fn public_skips_and_merge() {
    let b=[11,10,1,7,10,1,8,131,1,2,8,9];
    assert_eq!(decode_message(&b,0,4),Ok((Message{fields:vec![Field::Bytes{tag:1,data:vec![7,8]}]},12)));
}

// R7-R9, R23, R31-R33
#[test]
fn public_canonical_and_truncation() {
    assert_eq!(decode_message(&[3,8,130,0],0,2),Err(Error::NonCanonical{offset:2}));
    assert_eq!(decode_message(&[128],0,2),Err(Error::Truncated{offset:1,start:0}));
}
