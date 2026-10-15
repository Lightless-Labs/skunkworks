// R1-R5, R27-R28
#[test]
fn public_empty_message() {
    let m=Message{fields:vec![]}; assert_eq!(encode_message(&m),Ok(vec![0]));
    assert_eq!(decode_message(&[0],0),Ok((m,1)));
}

// R2-R3, R10-R11, R16, R19, R21-R22, R24-R27
#[test]
fn public_message_roundtrip() {
    let m=Message{fields:vec![Field::UInt{tag:1,value:300},Field::Bytes{tag:3,data:vec![4,5]}]};
    assert_eq!(encode_message(&m),Ok(vec![7,8,172,2,26,2,4,5]));
    assert_eq!(decode_message(&[7,8,172,2,26,2,4,5,99],0),Ok((m,8)));
}

// R12-R17, R28, R30
#[test]
fn public_unknown_skip() {
    assert_eq!(decode_message(&[5,8,7,128,1,9],0),
        Ok((Message{fields:vec![Field::UInt{tag:1,value:7}]},6)));
}

// R7-R9, R23, R29
#[test]
fn public_frame_errors() {
    assert_eq!(decode_message(&[128],0),Err(Error::Truncated{offset:1,start:0}));
    assert_eq!(decode_message(&[2,8],0),Err(Error::LengthOutOfBounds{offset:0,length:2}));
}
