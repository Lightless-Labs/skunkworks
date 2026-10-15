#[test] fn public_crockford_encode() { assert_eq!(encode(b"foo",false),"CSQPY"); }
#[test] fn public_crockford_normalize() { assert_eq!(decode("c-s-q-p-y",false),Ok(b"foo".to_vec())); }
#[test] fn public_checked_extended() { assert_eq!(encode(&[64],true),"80$"); assert_eq!(decode("80$",true),Ok(vec![64])); }
#[test] fn public_unchecked_binary() { assert_eq!(encode(&[32,16],false),"4080"); assert_eq!(decode("4080",false),Ok(vec![32,16])); }
