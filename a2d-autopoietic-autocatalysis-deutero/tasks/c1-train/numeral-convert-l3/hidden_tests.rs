// R5, R9-R12
#[test]
fn hidden_signed_zero() {
for s in [SignedSystem::BalancedTernary,SignedSystem::Negabinary] { assert_eq!(signed_encode(0,s.clone(),true),Ok("0:0".into())); assert_eq!(signed_decode("0",s),Ok(0)); }
}

// R2, R7, R30
#[test]
fn hidden_signed_negative_balanced() {
assert_eq!(signed_encode(-8,SignedSystem::BalancedTernary,false),Ok("T01".into())); assert_eq!(signed_decode("T01",SignedSystem::BalancedTernary),Ok(-8));
}

// R3, R8, R30
#[test]
fn hidden_signed_negative_negabinary() {
assert_eq!(signed_encode(-6,SignedSystem::Negabinary,false),Ok("1110".into())); assert_eq!(signed_decode("1110",SignedSystem::Negabinary),Ok(-6));
}

// R2, R7
#[test]
fn hidden_signed_balanced_digits() {
assert_eq!(signed_encode(5,SignedSystem::BalancedTernary,false),Ok("1TT".into())); assert_eq!(signed_decode("1TT",SignedSystem::BalancedTernary),Ok(5));
}

// R10, R11
#[test]
fn hidden_signed_checksum_left_weights() {
assert_eq!(signed_encode(5,SignedSystem::BalancedTernary,true),Ok("1TT:4".into())); assert_eq!(signed_decode("T10:4",SignedSystem::BalancedTernary),Ok(-6));
}

// R25
#[test]
fn hidden_signed_checksum_wrong() {
assert_eq!(signed_decode("T1:0",SignedSystem::BalancedTernary),Err(SignedError::ChecksumMismatch {expected:4,found:0}));
}

// R14, R32
#[test]
fn hidden_signed_empty() {
assert_eq!(signed_decode("",SignedSystem::Negabinary),Err(SignedError::Empty)); assert_eq!(signed_decode(":!",SignedSystem::BalancedTernary),Err(SignedError::Empty));
}

// R13, R15, R16
#[test]
fn hidden_signed_no_trim() {
assert_eq!(signed_decode(" 1",SignedSystem::Negabinary),Err(SignedError::InvalidByte {index:0})); assert_eq!(signed_decode("1
",SignedSystem::BalancedTernary),Err(SignedError::InvalidByte {index:1}));
}

// R4, R15, R16
#[test]
fn hidden_signed_alphabet() {
assert_eq!(signed_decode("10t",SignedSystem::BalancedTernary),Err(SignedError::InvalidByte {index:2})); assert_eq!(signed_decode("1T",SignedSystem::Negabinary),Err(SignedError::InvalidByte {index:1})); assert_eq!(signed_decode("-1",SignedSystem::Negabinary),Err(SignedError::InvalidByte {index:0}));
}

// R13, R15
#[test]
fn hidden_signed_unicode() {
assert_eq!(signed_decode("1é0",SignedSystem::BalancedTernary),Err(SignedError::InvalidByte {index:1}));
}

// R17, R18
#[test]
fn hidden_signed_checksum_syntax() {
for s in ["10:","10:7","10:00","10::1","10:é"] { assert_eq!(signed_decode(s,SignedSystem::Negabinary),Err(SignedError::InvalidChecksum {index:2})); }
}

// R15, R17, R32
#[test]
fn hidden_signed_lexical_precedence() {
assert_eq!(signed_decode("0x:9",SignedSystem::Negabinary),Err(SignedError::InvalidByte {index:1}));
}

// R19, R32
#[test]
fn hidden_signed_suffix_before_zero() {
assert_eq!(signed_decode("01:9",SignedSystem::BalancedTernary),Err(SignedError::InvalidChecksum {index:2}));
}

// R6, R20
#[test]
fn hidden_signed_leading_zero() {
assert_eq!(signed_decode("00",SignedSystem::Negabinary),Err(SignedError::LeadingZero)); assert_eq!(signed_decode("0T:1",SignedSystem::BalancedTernary),Err(SignedError::LeadingZero));
}

// R20, R26
#[test]
fn hidden_signed_zero_before_checksum() {
assert_eq!(signed_decode("01:0",SignedSystem::Negabinary),Err(SignedError::LeadingZero));
}

// R1
#[test]
fn hidden_signed_encode_bounds() {
let l=(1i128<<120)-1; for n in [l+1,-l-1,i128::MIN,i128::MAX] { assert_eq!(signed_encode(n,SignedSystem::BalancedTernary,true),Err(SignedError::OutOfRange)); assert_eq!(signed_encode(n,SignedSystem::Negabinary,false),Err(SignedError::OutOfRange)); }
}

// R1, R6-R8, R21-R23, R27
#[test]
fn hidden_signed_boundary_roundtrip() {
let l=(1i128<<120)-1; for n in [l,-l,l-1,-l+1] { for s in [SignedSystem::BalancedTernary,SignedSystem::Negabinary] { let text=signed_encode(n,s.clone(),true).unwrap(); assert_eq!(signed_decode(&text,s),Ok(n)); } }
}

// R21-R23
#[test]
fn hidden_signed_range_positive() {
assert_eq!(signed_decode(&format!("1{}","0".repeat(120)),SignedSystem::Negabinary),Err(SignedError::Range {index:120}));
}

// R21-R23
#[test]
fn hidden_signed_range_negative() {
assert_eq!(signed_decode(&format!("1{}","0".repeat(121)),SignedSystem::Negabinary),Err(SignedError::Range {index:120}));
}

// R21-R23
#[test]
fn hidden_signed_range_balanced() {
assert_eq!(signed_decode(&format!("T{}","0".repeat(100)),SignedSystem::BalancedTernary),Err(SignedError::Range {index:76}));
}

// R24, R32
#[test]
fn hidden_signed_late_lexical_before_range() {
assert_eq!(signed_decode(&format!("1{}x","0".repeat(160)),SignedSystem::Negabinary),Err(SignedError::InvalidByte {index:161}));
}

// R19, R32
#[test]
fn hidden_signed_syntax_before_range() {
assert_eq!(signed_decode(&format!("1{}:8","0".repeat(160)),SignedSystem::Negabinary),Err(SignedError::InvalidChecksum {index:161}));
}

// R22, R26
#[test]
fn hidden_signed_range_before_wrong_checksum() {
assert_eq!(signed_decode(&format!("1{}:0","0".repeat(150)),SignedSystem::Negabinary),Err(SignedError::Range {index:120}));
}

// R11, R28, R29
#[test]
fn hidden_signed_recode_reweights() {
assert_eq!(signed_recode("1TT:4",SignedSystem::BalancedTernary,SignedSystem::Negabinary,true),Ok("101:4".into())); assert_eq!(signed_recode("T:2",SignedSystem::BalancedTernary,SignedSystem::Negabinary,true),Ok("11:3".into()));
}

// R28, R29
#[test]
fn hidden_signed_recode_drop() {
assert_eq!(signed_recode("11:3",SignedSystem::Negabinary,SignedSystem::BalancedTernary,false),Ok("T".into()));
}

// R28, R32
#[test]
fn hidden_signed_recode_error() {
assert_eq!(signed_recode("11:0",SignedSystem::Negabinary,SignedSystem::BalancedTernary,false),Err(SignedError::ChecksumMismatch {expected:3,found:0}));
}

// R2-R8, R11, R12, R21, R27, R31
#[test]
fn hidden_signed_broad_roundtrip() {
for n in -2048..=2048 { if n==1 { continue; } for s in [SignedSystem::BalancedTernary,SignedSystem::Negabinary] { let t=signed_encode(n,s.clone(),true).unwrap(); assert_eq!(signed_decode(&t,s),Ok(n)); } }
}

// R11, R12
#[test]
fn hidden_signed_long_checksum_modulo() {
let text=format!("1{}:1","0".repeat(112)); assert_eq!(signed_decode(&text,SignedSystem::Negabinary),Ok(1i128<<112));
}
