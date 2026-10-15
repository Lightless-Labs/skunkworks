// R1,R2,R5,R6-R11,R41: buffering complete blocks and partial fragment exactly.
#[test] fn hidden_encoder_boundaries() { let c=Config{alphabet:Alphabet::Standard,padding:Padding::Required,canonical:true}; let mut e=Encoder::new(c); assert_eq!(e.push(&[255;4]).unwrap(),""); assert_eq!(e.push(&[255;7]).unwrap(),"7777777777777777"); assert_eq!(e.finish().unwrap(),"74======"); }
// R4,R6,R9,R11,R12: independent hex vector, canonical ignored by encoder.
#[test] fn hidden_encoder_hex_flags() { for canonical in [false,true] { let mut e=Encoder::new(Config{alphabet:Alphabet::Hex,padding:Padding::Omitted,canonical}); assert_eq!(e.push(&[255]),Ok(String::new())); assert_eq!(e.finish(),Ok("VS".into())); } }
// R8,R13,R14,R40,R45: complete blocks and empty calls; closed errors repeated.
#[test] fn hidden_encoder_empty_closed() { let c=Config{alphabet:Alphabet::Standard,padding:Padding::Required,canonical:false}; let mut e=Encoder::new(c); assert_eq!(e.push(b"abcde").unwrap(),"MFRGGZDF"); assert_eq!(e.push(&[]).unwrap(),""); assert_eq!(e.finish().unwrap(),""); assert_eq!(e.push(&[]),Err(StreamError::Closed)); assert_eq!(e.finish(),Err(StreamError::Closed)); assert_eq!(e.finish(),Err(StreamError::Closed)); }
// R15,R16,R41,R45: decoder byte availability across every symbol boundary.
#[test] fn hidden_decoder_one_symbol() { let c=Config{alphabet:Alphabet::Standard,padding:Padding::Omitted,canonical:true}; let mut d=Decoder::new(c); let expected=[vec![],vec![65],vec![],vec![66],vec![67],vec![],vec![68],vec![69]]; for (i,b) in "IFBEGRCF".bytes().enumerate() { assert_eq!(d.push(&(b as char).to_string()).unwrap(),expected[i]); } assert_eq!(d.finish(),Ok(vec![])); }
// R17,R18,R21-R23: absolute invalid UTF-8 byte, not a character index.
#[test] fn hidden_decoder_utf8() { let c=Config{alphabet:Alphabet::Hex,padding:Padding::Omitted,canonical:false}; let mut d=Decoder::new(c); d.push("00").unwrap(); assert_eq!(d.push("é"),Err(StreamError::InvalidByte{offset:2,byte:0xc3})); assert_eq!(d.finish(),Ok(vec![])); }
// R21,R22: alphabets never blend, aliases forbidden.
#[test] fn hidden_no_alias_or_blend() { for (a,s,b) in [(Alphabet::Standard,"0",b'0'),(Alphabet::Standard,"1",b'1'),(Alphabet::Hex,"W",b'W'),(Alphabet::Hex,"z",b'z')] { let mut d=Decoder::new(Config{alphabet:a,padding:Padding::Omitted,canonical:false}); assert_eq!(d.push(s),Err(StreamError::InvalidByte{offset:0,byte:b})); } }
// R18,R24: strict lowercase error reports expected uppercase and absolute offset.
#[test] fn hidden_strict_case() { let c=Config{alphabet:Alphabet::Standard,padding:Padding::Omitted,canonical:true}; let mut d=Decoder::new(c); d.push("MY").unwrap(); assert_eq!(d.push("a"),Err(StreamError::NonCanonical{offset:2,expected:b'A'})); assert_eq!(d.finish(),Ok(vec![])); }
// R4,R25: relaxed case, valid hex digits.
#[test] fn hidden_relaxed_hex() { let c=Config{alphabet:Alphabet::Hex,padding:Padding::Required,canonical:false}; let mut d=Decoder::new(c); assert_eq!(d.push("cOnG====").unwrap(),vec![102,47]); assert_eq!(d.finish(),Ok(vec![])); }
// R21,R22,R25: no whitespace or separators in relaxed mode.
#[test] fn hidden_no_skipping() { for (s,b) in [(" ",b' '),("-",b'-'),("\n",b'\n')] { let mut d=Decoder::new(Config{alphabet:Alphabet::Standard,padding:Padding::Omitted,canonical:false}); assert_eq!(d.push(s),Err(StreamError::InvalidByte{offset:0,byte:b})); } }
// R17,R26,R42: first failing byte, no lexical prepass.
#[test] fn hidden_stream_precedence() { let c=Config{alphabet:Alphabet::Standard,padding:Padding::Omitted,canonical:true}; let mut d=Decoder::new(c); assert_eq!(d.push("A=!"),Err(StreamError::Padding{offset:1})); assert_eq!(d.push("a!"),Err(StreamError::NonCanonical{offset:0,expected:b'A'})); }
// R27: omitted padding is immediate before data length or tail.
#[test] fn hidden_omitted_padding() { let c=Config{alphabet:Alphabet::Standard,padding:Padding::Omitted,canonical:true}; let mut d=Decoder::new(c); assert_eq!(d.push("MZ="),Err(StreamError::Padding{offset:2})); assert_eq!(d.finish(),Ok(vec![])); }
// R28,R29: first padding after impossible residues is placement error.
#[test] fn hidden_bad_first_pad() { for s in ["=","A=","AAA=","AAAAAA=","AAAAAAAA="] { let mut d=Decoder::new(Config{alphabet:Alphabet::Standard,padding:Padding::Required,canonical:true}); assert_eq!(d.push(s),Err(StreamError::Padding{offset:s.len()-1})); } }
// R18,R28,R31: excess padding offset is the excess byte across chunks.
#[test] fn hidden_excess_padding_offset() { let c=Config{alphabet:Alphabet::Standard,padding:Padding::Required,canonical:true}; let mut d=Decoder::new(c); d.push("MY===").unwrap(); assert_eq!(d.push("===="),Err(StreamError::Padding{offset:8})); d.push("===").unwrap(); assert_eq!(d.finish(),Ok(vec![])); }
// R23,R26,R30: invalid after padding stays lexical.
#[test] fn hidden_invalid_after_padding() { let c=Config{alphabet:Alphabet::Standard,padding:Padding::Required,canonical:true}; let mut d=Decoder::new(c); d.push("MY=").unwrap(); assert_eq!(d.push("!"),Err(StreamError::InvalidByte{offset:3,byte:b'!'})); }
// R24-R26,R30: canonical-case check before padding check.
#[test] fn hidden_case_after_padding() { for canonical in [true,false] { let mut d=Decoder::new(Config{alphabet:Alphabet::Hex,padding:Padding::Required,canonical}); d.push("CO=").unwrap(); let error=if canonical { StreamError::NonCanonical{offset:3,expected:b'A'} } else { StreamError::Padding{offset:3} }; assert_eq!(d.push("a"),Err(error)); } }
// R30: recognized uppercase after padding.
#[test] fn hidden_data_after_padding() { let mut d=Decoder::new(Config{alphabet:Alphabet::Standard,padding:Padding::Required,canonical:true}); d.push("MY=").unwrap(); assert_eq!(d.push("AA"),Err(StreamError::Padding{offset:3})); }
// R19,R20,R32-R34,R39: failed finish preserves open bits; retry by extending data.
#[test] fn hidden_length_repair() { let mut d=Decoder::new(Config{alphabet:Alphabet::Standard,padding:Padding::Omitted,canonical:true}); assert_eq!(d.push("A"),Ok(vec![])); assert_eq!(d.finish(),Err(StreamError::InvalidLength{offset:1})); assert_eq!(d.finish(),Err(StreamError::InvalidLength{offset:1})); assert_eq!(d.push("A"),Ok(vec![0])); assert_eq!(d.finish(),Ok(vec![])); }
// R32-R35,R39: missing all padding reports consumed end, then repaired.
#[test] fn hidden_missing_padding_repair() { let mut d=Decoder::new(Config{alphabet:Alphabet::Standard,padding:Padding::Required,canonical:true}); assert_eq!(d.push("MY"),Ok(vec![102])); assert_eq!(d.finish(),Err(StreamError::Padding{offset:2})); d.push("======").unwrap(); assert_eq!(d.finish(),Ok(vec![])); }
// R18,R31,R35,R39: incomplete padding reports first = absolute index.
#[test] fn hidden_partial_padding_repair() { let mut d=Decoder::new(Config{alphabet:Alphabet::Hex,padding:Padding::Required,canonical:true}); d.push("00000000CO==").unwrap(); assert_eq!(d.finish(),Err(StreamError::Padding{offset:10})); d.push("====").unwrap(); assert_eq!(d.finish(),Ok(vec![])); }
// R15,R33,R37-R39,R44: provisional byte, 2-bit tail rejection, then data repair.
#[test] fn hidden_tail_two() { let mut d=Decoder::new(Config{alphabet:Alphabet::Standard,padding:Padding::Omitted,canonical:true}); assert_eq!(d.push("MZ"),Ok(vec![102])); assert_eq!(d.finish(),Err(StreamError::TrailingBits{offset:1})); assert_eq!(d.finish(),Err(StreamError::TrailingBits{offset:1})); assert_eq!(d.push("AAAAAA"),Ok(vec![64,0,0,0])); assert_eq!(d.finish(),Ok(vec![])); }
// R37,R38: low 4 bits must be zero in hex.
#[test] fn hidden_tail_four() { let mut d=Decoder::new(Config{alphabet:Alphabet::Hex,padding:Padding::Omitted,canonical:true}); d.push("CPNH").unwrap(); assert_eq!(d.finish(),Err(StreamError::TrailingBits{offset:3})); }
// R37,R38: low single bit must be zero.
#[test] fn hidden_tail_one() { let mut d=Decoder::new(Config{alphabet:Alphabet::Hex,padding:Padding::Omitted,canonical:true}); d.push("CPNMV").unwrap(); assert_eq!(d.finish(),Err(StreamError::TrailingBits{offset:4})); }
// R37,R38: low 3 bits must be zero with padding too.
#[test] fn hidden_tail_three() { let mut d=Decoder::new(Config{alphabet:Alphabet::Standard,padding:Padding::Required,canonical:true}); d.push("MZXW6YR=").unwrap(); assert_eq!(d.finish(),Err(StreamError::TrailingBits{offset:6})); }
// R33,R35,R38,R39: missing padding beats nonzero tail at finish.
#[test] fn hidden_padding_before_tail() { let mut d=Decoder::new(Config{alphabet:Alphabet::Standard,padding:Padding::Required,canonical:true}); d.push("MZ=").unwrap(); assert_eq!(d.finish(),Err(StreamError::Padding{offset:2})); d.push("=====").unwrap(); assert_eq!(d.finish(),Err(StreamError::TrailingBits{offset:1})); }
// R18-R20: rejected chunk rolls back decoded prefix, bits, counts, and total.
#[test] fn hidden_transaction_prefix() { let mut d=Decoder::new(Config{alphabet:Alphabet::Standard,padding:Padding::Omitted,canonical:true}); d.push("M").unwrap(); assert_eq!(d.push("YA!"),Err(StreamError::InvalidByte{offset:3,byte:b'!'})); assert_eq!(d.push("Y"),Ok(vec![102])); assert_eq!(d.finish(),Ok(vec![])); }
// R19,R20,R28-R31: failed chunk rolls back entry into padding phase.
#[test] fn hidden_transaction_padding() { let mut d=Decoder::new(Config{alphabet:Alphabet::Standard,padding:Padding::Required,canonical:true}); d.push("MY").unwrap(); assert_eq!(d.push("==A"),Err(StreamError::Padding{offset:4})); assert_eq!(d.push("======"),Ok(vec![])); assert_eq!(d.finish(),Ok(vec![])); }
// R19,R20,R24: failed case-check call rolls back its successful prefix.
#[test] fn hidden_transaction_case() { let mut d=Decoder::new(Config{alphabet:Alphabet::Hex,padding:Padding::Omitted,canonical:true}); d.push("C").unwrap(); assert_eq!(d.push("Oo"),Err(StreamError::NonCanonical{offset:2,expected:b'O'})); assert_eq!(d.push("O"),Ok(vec![102])); assert_eq!(d.finish(),Ok(vec![])); }
// R14,R16,R40,R45: empty open streams, closed no-op calls still fail.
#[test] fn hidden_decoder_empty_closed() { for padding in [Padding::Required,Padding::Omitted] { let mut d=Decoder::new(Config{alphabet:Alphabet::Hex,padding,canonical:true}); assert_eq!(d.push(""),Ok(vec![])); assert_eq!(d.finish(),Ok(vec![])); assert_eq!(d.push(""),Err(StreamError::Closed)); assert_eq!(d.finish(),Err(StreamError::Closed)); } }
// R3-R6,R10,R11,R15,R36,R41,R43: all split points and configs on independent vector.
#[test] fn hidden_split_invariance() { for (alphabet,unpadded) in [(Alphabet::Standard,"O5UW45DFOI"),(Alphabet::Hex,"ETKMST35E8")] { for padding in [Padding::Required,Padding::Omitted] { let encoded=if padding==Padding::Required { format!("{unpadded}======") } else { unpadded.to_string() }; let c=Config{alphabet,padding,canonical:true}; for split in 0..=6 { let mut e=Encoder::new(c); let mut s=e.push(&b"winter"[..split]).unwrap(); s+=&e.push(&b"winter"[split..]).unwrap(); s+=&e.finish().unwrap(); assert_eq!(s,encoded); } for split in 0..=encoded.len() { let mut d=Decoder::new(c); let mut bytes=d.push(&encoded[..split]).unwrap(); bytes.extend(d.push(&encoded[split..]).unwrap()); bytes.extend(d.finish().unwrap()); assert_eq!(bytes,b"winter"); } } } }
// R5,R12,R25,R41,R43: long streams, multiple configs, odd push sizes.
#[test] fn hidden_large_binary() { let raw:Vec<u8>=(0..2051).map(|i| ((i*117+i/7)%256) as u8).collect(); for alphabet in [Alphabet::Standard,Alphabet::Hex] { for padding in [Padding::Required,Padding::Omitted] { let c=Config{alphabet,padding,canonical:false}; let mut e=Encoder::new(c); let mut encoded=String::new(); for chunk in raw.chunks(3) { encoded+=&e.push(chunk).unwrap(); } encoded+=&e.finish().unwrap(); let encoded=encoded.to_ascii_lowercase(); let mut d=Decoder::new(c); let mut decoded=Vec::new(); for chunk in encoded.as_bytes().chunks(7) { decoded.extend(d.push(std::str::from_utf8(chunk).unwrap()).unwrap()); } decoded.extend(d.finish().unwrap()); assert_eq!(decoded,raw); } } }
// R18,R37,R38: tail error absolute after many successful chunks.
#[test] fn hidden_late_tail_offset() { let mut d=Decoder::new(Config{alphabet:Alphabet::Hex,padding:Padding::Omitted,canonical:false}); for _ in 0..257 { d.push("00000000").unwrap(); } d.push("c").unwrap(); d.push("p").unwrap(); assert_eq!(d.finish(),Err(StreamError::TrailingBits{offset:2057})); }
// R1,R2,R5,R19,R34,R39: advertised traits and new stream isolation.
#[test] fn hidden_traits_isolation() { let c=Config{alphabet:Alphabet::Standard,padding:Padding::Required,canonical:true}; assert_eq!(c.clone(),c); let err=StreamError::InvalidLength{offset:3}; assert_eq!(err.clone(),err); assert!(!format!("{err:?}").is_empty()); let mut a=Decoder::new(c); let mut b=Decoder::new(c); a.push("AAA").unwrap(); assert_eq!(a.finish(),Err(StreamError::InvalidLength{offset:3})); assert_eq!(b.finish(),Ok(vec![])); a.push("A====").unwrap(); assert_eq!(a.finish(),Ok(vec![])); }
