// R3,R4,R6-R8,R19: independent hex vectors across all data residues.
#[test] fn hidden_hex_vectors() { for (raw,s) in [("f","CO"),("fo","CPNG"),("foob","CPNMUOG"),("fooba","CPNMUOJ1"),("foobar","CPNMUOJ1E8")] { assert_eq!(encode(raw.as_bytes(),Alphabet::Hex,Padding::Omitted),s); assert_eq!(decode(s,Alphabet::Hex,Padding::Omitted).unwrap(),raw.as_bytes()); } }
// R3,R6-R8: standard omitted vectors.
#[test] fn hidden_standard_vectors() { assert_eq!(encode(b"foobar",Alphabet::Standard,Padding::Omitted),"MZXW6YTBOI"); assert_eq!(decode("MZXW6YTBOI",Alphabet::Standard,Padding::Omitted),Ok(b"foobar".to_vec())); }
// R4,R5,R11: W not a hex digit.
#[test] fn hidden_hex_rejects_standard() { assert_eq!(decode("AW",Alphabet::Hex,Padding::Omitted),Err(DecodeError::InvalidByte{offset:1,byte:b'W'})); }
// R3,R4,R11: 0 not a standard digit.
#[test] fn hidden_standard_rejects_hex() { assert_eq!(decode("A0",Alphabet::Standard,Padding::Omitted),Err(DecodeError::InvalidByte{offset:1,byte:b'0'})); }
// R5,R11: lowercase not accepted.
#[test] fn hidden_hex_lowercase() { assert_eq!(decode("Co",Alphabet::Hex,Padding::Omitted),Err(DecodeError::InvalidByte{offset:1,byte:b'o'})); }
// R5,R11,R12: UTF-8 first byte absolute index.
#[test] fn hidden_utf8() { assert_eq!(decode("CO💡",Alphabet::Hex,Padding::Omitted),Err(DecodeError::InvalidByte{offset:2,byte:0xf0})); }
// R10,R11,R13: invalid byte anywhere beats forbidden padding.
#[test] fn hidden_lex_over_omitted_padding() { assert_eq!(decode("CO=!",Alphabet::Hex,Padding::Omitted),Err(DecodeError::InvalidByte{offset:3,byte:b'!'})); }
// R10,R13,R15: omitted pad error before invalid length.
#[test] fn hidden_omitted_padding_before_length() { assert_eq!(decode("C=",Alphabet::Hex,Padding::Omitted),Err(DecodeError::Padding{offset:1})); }
// R13: omitted only equals error at zero.
#[test] fn hidden_omitted_only_padding() { assert_eq!(decode("==",Alphabet::Standard,Padding::Omitted),Err(DecodeError::Padding{offset:0})); }
// R14: alphabet after padding error at alphabet byte.
#[test] fn hidden_required_placement() { assert_eq!(decode("CO==C===",Alphabet::Hex,Padding::Required),Err(DecodeError::Padding{offset:4})); }
// R15: illegal omitted residues.
#[test] fn hidden_omitted_illegal_lengths() { for s in ["0","000","000000","000000000"] { assert_eq!(decode(s,Alphabet::Hex,Padding::Omitted),Err(DecodeError::InvalidLength{offset:s.len()})); } }
// R10,R15,R16: illegal length beats shape in required mode.
#[test] fn hidden_required_illegal_length() { assert_eq!(decode("000=====",Alphabet::Hex,Padding::Required),Err(DecodeError::InvalidLength{offset:8})); }
// R16,R17: missing required padding.
#[test] fn hidden_required_missing() { assert_eq!(decode("CO",Alphabet::Hex,Padding::Required),Err(DecodeError::Padding{offset:2})); }
// R16,R17: extra padding after full block.
#[test] fn hidden_required_extra() { assert_eq!(decode("00000000=",Alphabet::Hex,Padding::Required),Err(DecodeError::Padding{offset:8})); }
// R16,R17: too little and too much padding.
#[test] fn hidden_required_counts() { for s in ["CO=====","CO======="] { assert_eq!(decode(s,Alphabet::Hex,Padding::Required),Err(DecodeError::Padding{offset:2})); } }
// R18,R19: unpadded tail accepted at legal length.
#[test] fn hidden_omitted_short_zero() { assert_eq!(decode("00",Alphabet::Hex,Padding::Omitted),Ok(vec![0])); }
// R20,R21: low 2 bits checked in hex.
#[test] fn hidden_hex_tail_two() { assert_eq!(decode("CP",Alphabet::Hex,Padding::Omitted),Err(DecodeError::TrailingBits{offset:1})); }
// R20,R21: low 4 bits checked in hex.
#[test] fn hidden_hex_tail_four() { assert_eq!(decode("CPNH",Alphabet::Hex,Padding::Omitted),Err(DecodeError::TrailingBits{offset:3})); }
// R20,R21: low bit checked in hex.
#[test] fn hidden_hex_tail_one() { assert_eq!(decode("CPNMV",Alphabet::Hex,Padding::Omitted),Err(DecodeError::TrailingBits{offset:4})); }
// R20,R21: low 3 bits checked in hex.
#[test] fn hidden_hex_tail_three() { assert_eq!(decode("CPNMUOH",Alphabet::Hex,Padding::Omitted),Err(DecodeError::TrailingBits{offset:6})); }
// R20,R21: standard omitted tail too.
#[test] fn hidden_standard_tail() { assert_eq!(decode("MZ",Alphabet::Standard,Padding::Omitted),Err(DecodeError::TrailingBits{offset:1})); }
// R10,R17,R21: shape precedes tail.
#[test] fn hidden_shape_precedence() { assert_eq!(decode("CP=====",Alphabet::Hex,Padding::Required),Err(DecodeError::Padding{offset:2})); }
// R6-R8,R19,R23,R24: alternating options and larger binary round trips.
#[test] fn hidden_all_options_large() { for n in 0..=128 { let raw:Vec<u8>=(0..n).map(|i| ((i*113+n)%256) as u8).collect(); for a in [Alphabet::Hex,Alphabet::Standard] { for p in [Padding::Omitted,Padding::Required] { let s=encode(&raw,a,p); assert_eq!(decode(&s,a,p).unwrap(),raw); } } } }
// R3,R4,R19,R24: shared letters have different values.
#[test] fn hidden_overlap_values() { assert_eq!(decode("AA",Alphabet::Standard,Padding::Omitted),Ok(vec![0])); assert_eq!(decode("AC",Alphabet::Hex,Padding::Omitted),Ok(vec![83])); }
// R11,R12: first invalid byte beats later bad byte.
#[test] fn hidden_first_bad_byte() { assert_eq!(decode("CO \n",Alphabet::Hex,Padding::Omitted),Err(DecodeError::InvalidByte{offset:2,byte:32})); }
// R1,R2,R9,R22: error traits, recovery, and empty Required decode.
#[test] fn hidden_traits_recovery() { let e=DecodeError::Padding{offset:2}; assert_eq!(e.clone(),e); assert!(!format!("{e:?}").is_empty()); assert!(decode("0",Alphabet::Hex,Padding::Omitted).is_err()); assert_eq!(decode("",Alphabet::Hex,Padding::Required),Ok(vec![])); }
