// R3-R6,R15: known RFC vectors exercise every remainder.
#[test] fn hidden_vectors() { for (raw,enc) in [("f","MY======"),("fo","MZXQ===="),("foob","MZXW6YQ="),("fooba","MZXW6YTB"),("foobar","MZXW6YTBOI======")] { assert_eq!(encode(raw.as_bytes()),enc); assert_eq!(decode(enc).unwrap(),raw.as_bytes()); } }
// R7,R18: repeat empty independent operations.
#[test] fn hidden_empty_repeat() { for _ in 0..3 { assert_eq!(encode(&[]), ""); assert_eq!(decode(""),Ok(vec![])); } }
// R3,R9,R10: lowercase forbidden.
#[test] fn hidden_lowercase() { assert_eq!(decode("MY=====a"),Err(DecodeError::InvalidByte{offset:7,byte:b'a'})); }
// R9,R10: no ignored whitespace.
#[test] fn hidden_space() { assert_eq!(decode("M Y======"),Err(DecodeError::InvalidByte{offset:1,byte:b' '})); }
// R9,R10: newline forbidden.
#[test] fn hidden_newline() { assert_eq!(decode("MY======\n"),Err(DecodeError::InvalidByte{offset:8,byte:10})); }
// R9,R10: hyphen forbidden.
#[test] fn hidden_dash() { assert_eq!(decode("MY-======"),Err(DecodeError::InvalidByte{offset:2,byte:b'-'})); }
// R8-R10: first UTF-8 byte, even after a padding-placement error.
#[test] fn hidden_unicode_precedence() { assert_eq!(decode("=Aé"),Err(DecodeError::InvalidByte{offset:2,byte:0xc3})); }
// R8,R9: first invalid byte wins.
#[test] fn hidden_first_bad() { assert_eq!(decode("A!\n"),Err(DecodeError::InvalidByte{offset:1,byte:b'!'})); }
// R8,R11: placement beats invalid data length.
#[test] fn hidden_placement() { assert_eq!(decode("A=A====="),Err(DecodeError::Padding{offset:2})); }
// R11,R14: padding at start cannot precede data.
#[test] fn hidden_leading_padding() { assert_eq!(decode("==AA"),Err(DecodeError::Padding{offset:2})); }
// R12: illegal residues report end, before shape.
#[test] fn hidden_residues() { for s in ["A","AAA","AAAAAA","A=======","AAA====="] { assert_eq!(decode(s),Err(DecodeError::InvalidLength{offset:s.len()})); } }
// R13,R14: no pad is valid only at a complete block.
#[test] fn hidden_missing_padding() { assert_eq!(decode("MY"),Err(DecodeError::Padding{offset:2})); }
// R13,R14: too little padding.
#[test] fn hidden_short_padding() { assert_eq!(decode("MY====="),Err(DecodeError::Padding{offset:2})); }
// R13,R14: too much padding.
#[test] fn hidden_extra_padding() { assert_eq!(decode("MY======="),Err(DecodeError::Padding{offset:2})); }
// R11-R14: empty data cannot have padding.
#[test] fn hidden_only_padding() { assert_eq!(decode("========"),Err(DecodeError::Padding{offset:0})); }
// R13,R14: complete block cannot have padding.
#[test] fn hidden_full_block_padding() { assert_eq!(decode("AAAAAAAA="),Err(DecodeError::Padding{offset:8})); }
// R16,R17: 2 unused low bits.
#[test] fn hidden_tail_two() { for s in ["MZ======","M2======","M3======"] { assert_eq!(decode(s),Err(DecodeError::TrailingBits{offset:1})); } }
// R16,R17: 4 unused low bits.
#[test] fn hidden_tail_four() { assert_eq!(decode("MZXR===="),Err(DecodeError::TrailingBits{offset:3})); }
// R16,R17: 1 unused low bit.
#[test] fn hidden_tail_one() { assert_eq!(decode("MZXW7==="),Err(DecodeError::TrailingBits{offset:4})); }
// R16,R17: 3 unused low bits.
#[test] fn hidden_tail_three() { assert_eq!(decode("MZXW6YR="),Err(DecodeError::TrailingBits{offset:6})); }
// R8,R14,R17: malformed shape wins over bad tail.
#[test] fn hidden_shape_before_tail() { assert_eq!(decode("MZ====="),Err(DecodeError::Padding{offset:2})); }
// R8-R10,R17: lexical error wins over bad tail.
#[test] fn hidden_lex_before_tail() { assert_eq!(decode("MZ=====!"),Err(DecodeError::InvalidByte{offset:7,byte:b'!'})); }
// R4-R6,R15,R19: independent exact zero and all-one vectors.
#[test] fn hidden_extreme_bytes() { assert_eq!(encode(&[0]),"AA======"); assert_eq!(encode(&[255]),"74======"); assert_eq!(decode("77777777"),Ok(vec![255;5])); }
// R4-R6,R15,R19: deterministic larger round trips across all tails.
#[test] fn hidden_large_roundtrip() { for n in 1..=260 { let bytes:Vec<u8>=(0..n).map(|i| ((i*71+n*19)%256) as u8).collect(); assert_eq!(decode(&encode(&bytes)).unwrap(),bytes); } }
// R16,R17: error offset includes all earlier full blocks.
#[test] fn hidden_late_tail() { assert_eq!(decode("AAAAAAAAMZ======"),Err(DecodeError::TrailingBits{offset:9})); }
// R1,R2,R18: API error value supports advertised traits and independent calls.
#[test] fn hidden_error_traits_and_recovery() { let e=DecodeError::InvalidLength{offset:1}; assert_eq!(e.clone(),e); assert!(!format!("{e:?}").is_empty()); assert!(decode("A").is_err()); assert_eq!(decode("AA======"),Ok(vec![0])); }
