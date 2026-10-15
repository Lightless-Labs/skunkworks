#[test] fn public_encode_foo() { assert_eq!(encode(b"foo"), "MZXW6==="); }
#[test] fn public_decode_hello() { assert_eq!(decode("NBSWY3DP"), Ok(b"hello".to_vec())); }
#[test] fn public_binary_roundtrip() { assert_eq!(encode(&[0,255]), "AD7Q===="); assert_eq!(decode("AD7Q===="), Ok(vec![0,255])); }
#[test] fn public_bad_character() { assert_eq!(decode("!"), Err(DecodeError::InvalidByte { offset:0, byte:b'!' })); }
