// R3-R5,R20,R28: independent binary vectors.
#[test] fn hidden_byte_vectors() { for (raw,s) in [(vec![255],"ZW"),(vec![102],"CR"),(vec![102,111],"CSQG"),(vec![102,111,111,98],"CSQPYRG"),(vec![255;5],"ZZZZZZZZ")] { assert_eq!(encode(&raw,false),s); assert_eq!(decode(s,false),Ok(raw)); } }
// R7,R8: checksum includes the zero-filled final symbol.
#[test] fn hidden_check_vector() { assert_eq!(encode(&[255],true),"ZWN"); assert_eq!(decode("ZWN",true),Ok(vec![255])); }
// R7,R8,R11: all extended check symbols generated independently.
#[test] fn hidden_extended_checks() { for (raw,s) in [(vec![8],"10*"),(vec![36],"4G~"),(vec![27],"3C$"),(vec![18],"28="),(vec![9],"14U")] { assert_eq!(encode(&raw,true),s); assert_eq!(decode(s,true),Ok(raw)); } }
// R9,R10,R20,R23,R29: aliases and separators preserve values and checksum.
#[test] fn hidden_alias_normalization() { assert_eq!(decode("--i-O-*-",true),Ok(vec![8])); assert_eq!(decode("l-o",false),Ok(vec![8])); assert_eq!(encode(&decode("l-o",false).unwrap(),false),"10"); }
// R6,R10,R11,R25: canonical empty encoding and check aliases normalize to zero.
#[test] fn hidden_empty_alias_check() { assert_eq!(encode(&[],true),"0"); assert_eq!(decode("",false),Ok(vec![])); for s in ["O","o","---o---"] { assert_eq!(decode(s,true),Ok(vec![])); } }
// R11,R23: lowercase U is an extended check symbol.
#[test] fn hidden_lowercase_extended_check() { assert_eq!(decode("14u",true),Ok(vec![9])); }
// R9,R25: unchecked hyphens alone are empty data.
#[test] fn hidden_hyphens_empty() { assert_eq!(decode("------",false),Ok(vec![])); }
// R14,R15: checked no significant symbol reports original end.
#[test] fn hidden_missing_check() { assert_eq!(decode("",true),Err(DecodeError::MissingCheck{offset:0})); assert_eq!(decode("---",true),Err(DecodeError::MissingCheck{offset:3})); }
// R11,R13,R17: extended symbols cannot appear as data.
#[test] fn hidden_extended_data_invalid() { for b in [b'*',b'~',b'$',b'=',b'U',b'u'] { let s=format!("00{}0",b as char); assert_eq!(decode(&s,true),Err(DecodeError::InvalidByte{offset:2,byte:b})); } }
// R12,R13,R17: original lowercase byte reported when not recognized.
#[test] fn hidden_bad_lowercase() { assert_eq!(decode("00-v",false),Err(DecodeError::InvalidLength{offset:4})); assert_eq!(decode("00-u",false),Err(DecodeError::InvalidByte{offset:3,byte:b'u'})); }
// R12,R13: UTF-8 byte offsets include separators.
#[test] fn hidden_nonascii() { assert_eq!(decode("--00é",false),Err(DecodeError::InvalidByte{offset:4,byte:0xc3})); }
// R13,R17: whitespace not ignored.
#[test] fn hidden_whitespace() { assert_eq!(decode("0 0",false),Err(DecodeError::InvalidByte{offset:1,byte:32})); }
// R16-R18: invalid data wins over invalid check.
#[test] fn hidden_data_before_check_error() { assert_eq!(decode("0!0?",true),Err(DecodeError::InvalidByte{offset:1,byte:b'!'})); }
// R16,R18,R19: invalid check wins over data length.
#[test] fn hidden_check_before_length() { assert_eq!(decode("0?",true),Err(DecodeError::InvalidByte{offset:1,byte:b'?'})); }
// R19: illegal data lengths in unchecked mode, original end includes hyphens.
#[test] fn hidden_invalid_lengths() { for s in ["0","0-00-","000000","000000000"] { assert_eq!(decode(s,false),Err(DecodeError::InvalidLength{offset:s.len()})); } }
// R14,R19,R30: reserving check changes the data length.
#[test] fn hidden_reserved_last() { assert_eq!(decode("00",true),Err(DecodeError::InvalidLength{offset:2})); assert_eq!(decode("000",true),Ok(vec![0])); }
// R21,R22: low 2 bits and original last-data offset.
#[test] fn hidden_tail_two() { assert_eq!(decode("-Z-X--",false),Err(DecodeError::TrailingBits{offset:3})); }
// R21,R22: low 4 bits.
#[test] fn hidden_tail_four() { assert_eq!(decode("CSQH",false),Err(DecodeError::TrailingBits{offset:3})); }
// R21,R22: low bit.
#[test] fn hidden_tail_one() { assert_eq!(decode("CSQPZ",false),Err(DecodeError::TrailingBits{offset:4})); }
// R21,R22: low 3 bits.
#[test] fn hidden_tail_three() { assert_eq!(decode("CSQPYRH",false),Err(DecodeError::TrailingBits{offset:6})); }
// R16,R22,R24: tail error beats checksum mismatch.
#[test] fn hidden_tail_before_check() { assert_eq!(decode("Z-X-0",true),Err(DecodeError::TrailingBits{offset:2})); }
// R8,R23,R24: numeric remainder and original check offset.
#[test] fn hidden_mismatch() { assert_eq!(decode("--ZW-0--",true),Err(DecodeError::CheckMismatch{offset:5,expected:21,found:0})); }
// R11,R24: mismatch values include the extended range.
#[test] fn hidden_extended_mismatch() { assert_eq!(decode("00u",true),Err(DecodeError::CheckMismatch{offset:2,expected:0,found:36})); }
// R11,R26: equals has only a check role.
#[test] fn hidden_equals_roles() { assert_eq!(decode("28=",false),Err(DecodeError::InvalidByte{offset:2,byte:b'='})); assert_eq!(decode("28=",true),Ok(vec![18])); }
// R25,R30: a single check validates empty data.
#[test] fn hidden_single_symbol() { assert_eq!(decode("1",true),Err(DecodeError::CheckMismatch{offset:0,expected:0,found:1})); assert_eq!(decode("0",false),Err(DecodeError::InvalidLength{offset:1})); }
// R4,R8,R20,R23,R28,R29: larger deterministic binary and decorated round trips.
#[test] fn hidden_large_roundtrip() { for n in 0..=150 { let raw:Vec<u8>=(0..n).map(|i| ((i*73+n)%256) as u8).collect(); for checked in [false,true] { let s=encode(&raw,checked); let decorated=format!("--{}--",s.chars().map(|c| format!("{}-",c.to_ascii_lowercase())).collect::<String>()); assert_eq!(decode(&decorated,checked),Ok(raw.clone())); } } }
// R9,R12,R13: a bad check retains original byte and position after hyphens.
#[test] fn hidden_bad_check_offset() { assert_eq!(decode("-00-?-",true),Err(DecodeError::InvalidByte{offset:4,byte:b'?'})); }
// R1,R2,R6,R27: error traits and independent recovery.
#[test] fn hidden_error_traits() { let e=DecodeError::MissingCheck{offset:0}; assert_eq!(e.clone(),e); assert!(!format!("{e:?}").is_empty()); assert!(decode("0",false).is_err()); assert_eq!(encode(&[],false),""); assert_eq!(decode("00",false),Ok(vec![0])); }
