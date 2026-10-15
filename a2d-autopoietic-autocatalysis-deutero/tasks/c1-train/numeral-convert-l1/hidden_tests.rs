// R1, R3
#[test]
fn hidden_roman_boundaries() {
assert_eq!(roman_encode(3999, RomanCase::Upper), Ok("MMMCMXCIX".into())); assert_eq!(roman_encode(4000, RomanCase::Lower), Err(RomanError::Range)); assert_eq!(roman_encode(u32::MAX, RomanCase::Upper), Err(RomanError::Range));
}

// R3, R4
#[test]
fn hidden_roman_lower() {
assert_eq!(roman_encode(944, RomanCase::Lower), Ok("cmxliv".into()));
}

// R2, R11
#[test]
fn hidden_roman_upper_zero() {
assert_eq!(roman_encode(0, RomanCase::Upper), Ok("N".into())); assert_eq!(roman_decode("n"), Ok(0));
}

// R6, R18
#[test]
fn hidden_roman_empty() {
assert_eq!(roman_decode(""), Err(RomanError::Empty));
}

// R5, R8
#[test]
fn hidden_roman_whitespace() {
assert_eq!(roman_decode(" IX"), Err(RomanError::InvalidByte {index:0})); assert_eq!(roman_decode("IX
"), Err(RomanError::InvalidByte {index:2}));
}

// R7, R8
#[test]
fn hidden_roman_utf8() {
assert_eq!(roman_decode("XXéI"), Err(RomanError::InvalidByte {index:2}));
}

// R8
#[test]
fn hidden_roman_bad_early() {
assert_eq!(roman_decode("V?X!"), Err(RomanError::InvalidByte {index:1}));
}

// R9
#[test]
fn hidden_roman_case() {
assert_eq!(roman_decode("MMxI"), Err(RomanError::MixedCase {index:2})); assert_eq!(roman_decode("xivI"), Err(RomanError::MixedCase {index:3}));
}

// R10, R18
#[test]
fn hidden_roman_error_precedence() {
assert_eq!(roman_decode("Ii?"), Err(RomanError::InvalidByte {index:2}));
}

// R12
#[test]
fn hidden_roman_n_combined() {
assert_eq!(roman_decode("NN"), Err(RomanError::NonCanonical {index:0})); assert_eq!(roman_decode("MN"), Err(RomanError::NonCanonical {index:0}));
}

// R9, R12, R18
#[test]
fn hidden_roman_n_case() {
assert_eq!(roman_decode("Nn"), Err(RomanError::MixedCase {index:1}));
}

// R13, R15-R17
#[test]
fn hidden_roman_repeat_i() {
assert_eq!(roman_decode("IIII"), Err(RomanError::NonCanonical {index:1}));
}

// R15-R17
#[test]
fn hidden_roman_repeat_v() {
assert_eq!(roman_decode("VV"), Err(RomanError::NonCanonical {index:0}));
}

// R15-R17
#[test]
fn hidden_roman_repeat_l() {
assert_eq!(roman_decode("LL"), Err(RomanError::NonCanonical {index:0}));
}

// R13, R15-R17
#[test]
fn hidden_roman_bad_subtraction() {
assert_eq!(roman_decode("IL"), Err(RomanError::NonCanonical {index:0})); assert_eq!(roman_decode("IC"), Err(RomanError::NonCanonical {index:0}));
}

// R13, R16
#[test]
fn hidden_roman_pair_consumption() {
assert_eq!(roman_decode("IIV"), Err(RomanError::NonCanonical {index:0})); assert_eq!(roman_decode("IXIX"), Err(RomanError::NonCanonical {index:0}));
}

// R15, R16
#[test]
fn hidden_roman_prefix_mismatch() {
assert_eq!(roman_decode("VIV"), Err(RomanError::NonCanonical {index:0})); assert_eq!(roman_decode("XIIII"), Err(RomanError::NonCanonical {index:2}));
}

// R14, R18
#[test]
fn hidden_roman_out_of_range() {
assert_eq!(roman_decode("MMMM"), Err(RomanError::Range)); assert_eq!(roman_decode("MMMMIIII"), Err(RomanError::Range));
}

// R8, R14, R18
#[test]
fn hidden_roman_range_after_lexical() {
assert_eq!(roman_decode("MMMM!"), Err(RomanError::InvalidByte {index:4}));
}

// R12, R18
#[test]
fn hidden_roman_n_before_range() {
assert_eq!(roman_decode("MMMMN"), Err(RomanError::NonCanonical {index:0}));
}

// R3, R17
#[test]
fn hidden_roman_subtractive_table() {
for (s,n) in [("IV",4),("IX",9),("XL",40),("XC",90),("CD",400),("CM",900)] { assert_eq!(roman_decode(s), Ok(n)); }
}

// R1-R5, R15, R18
#[test]
fn hidden_roman_many_values() {
for n in 0..=3999 { if [0,444,1984].contains(&n) { continue; } let s=roman_encode(n,RomanCase::Upper).unwrap(); assert_eq!(roman_decode(&s),Ok(n)); let t=roman_encode(n,RomanCase::Lower).unwrap(); assert_eq!(roman_decode(&t),Ok(n)); }
}

// R14
#[test]
fn hidden_roman_long_invalid() {
assert_eq!(roman_decode(&"M".repeat(20000)),Err(RomanError::Range));
}

// R13, R15, R16
#[test]
fn hidden_roman_order() {
assert_eq!(roman_decode("VX"),Err(RomanError::NonCanonical {index:1})); assert_eq!(roman_decode("MDM"),Err(RomanError::NonCanonical {index:2}));
}
