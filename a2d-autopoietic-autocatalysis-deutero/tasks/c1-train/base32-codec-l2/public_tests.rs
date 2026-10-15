#[test] fn public_hex_encode() { assert_eq!(encode(b"foo",Alphabet::Hex,Padding::Required),"CPNMU==="); }
#[test] fn public_hex_decode() { assert_eq!(decode("CPNMU",Alphabet::Hex,Padding::Omitted),Ok(b"foo".to_vec())); }
#[test] fn public_standard_modes() { assert_eq!(encode(b"cat",Alphabet::Standard,Padding::Omitted),"MNQXI"); assert_eq!(decode("MNQXI===",Alphabet::Standard,Padding::Required),Ok(b"cat".to_vec())); }
#[test] fn public_binary_modes() { for a in [Alphabet::Standard,Alphabet::Hex] { for p in [Padding::Required,Padding::Omitted] { let raw=[1,2,3]; assert_eq!(decode(&encode(&raw,a,p),a,p),Ok(raw.to_vec())); } } }
