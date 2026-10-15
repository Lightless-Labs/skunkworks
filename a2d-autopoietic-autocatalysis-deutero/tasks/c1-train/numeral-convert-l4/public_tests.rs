// R5-R10
#[test]
fn public_words_encode() {
assert_eq!(words_encode(342),Ok("three hundred and forty-two".into()));
}

// R4, R11-R14, R17
#[test]
fn public_words_decode() {
assert_eq!(words_decode("minus two million five thousand and six"),Ok(-2005006));
}

// R3
#[test]
fn public_words_zero() {
assert_eq!(words_encode(0),Ok("zero".into())); assert_eq!(words_decode("zero"),Ok(0));
}
