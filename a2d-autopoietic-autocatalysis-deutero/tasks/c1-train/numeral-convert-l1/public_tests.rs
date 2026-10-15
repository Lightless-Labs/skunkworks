// R1-R4
#[test]
fn public_roman_encode() {
assert_eq!(roman_encode(1984, RomanCase::Upper), Ok("MCMLXXXIV".into()));
}

// R5, R15
#[test]
fn public_roman_decode() {
assert_eq!(roman_decode("cdxliv"), Ok(444));
}

// R2, R11
#[test]
fn public_roman_zero() {
assert_eq!(roman_encode(0, RomanCase::Lower), Ok("n".into())); assert_eq!(roman_decode("N"), Ok(0));
}
