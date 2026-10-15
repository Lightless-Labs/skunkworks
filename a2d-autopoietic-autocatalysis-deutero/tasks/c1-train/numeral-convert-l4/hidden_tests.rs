// R1, R4, R16
#[test]
fn hidden_words_boundaries() {
assert_eq!(words_encode(1000000000000000000),Ok("one quintillion".into())); assert_eq!(words_decode("minus one quintillion"),Ok(-1000000000000000000)); for n in [1000000000000000001,-1000000000000000001,i128::MIN,i128::MAX] { assert_eq!(words_encode(n),Err(WordsError::Range)); }
}

// R3, R5
#[test]
fn hidden_words_all_small_words() {
let names=["zero","one","two","three","four","five","six","seven","eight","nine","ten","eleven","twelve","thirteen","fourteen","fifteen","sixteen","seventeen","eighteen","nineteen"]; for (n,s) in names.iter().enumerate().skip(1) { assert_eq!(words_encode(n as i128),Ok(s.to_string())); assert_eq!(words_decode(s),Ok(n as i128)); }
}

// R6-R8
#[test]
fn hidden_words_tens_and_hyphens() {
for (n,s) in [(20,"twenty"),(30,"thirty"),(40,"forty"),(50,"fifty"),(60,"sixty"),(70,"seventy"),(80,"eighty"),(90,"ninety"),(91,"ninety-one"),(67,"sixty-seven")] { assert_eq!(words_encode(n),Ok(s.into())); assert_eq!(words_decode(s),Ok(n)); }
}

// R9, R10
#[test]
fn hidden_words_hundreds() {
for (n,s) in [(100,"one hundred"),(101,"one hundred and one"),(219,"two hundred and nineteen"),(900,"nine hundred")] { assert_eq!(words_encode(n),Ok(s.into())); assert_eq!(words_decode(s),Ok(n)); }
}

// R11-R14
#[test]
fn hidden_words_cross_group_and() {
for (n,s) in [(1001,"one thousand and one"),(1010,"one thousand and ten"),(1100,"one thousand one hundred"),(1000100,"one million one hundred"),(1001000,"one million one thousand")] { assert_eq!(words_encode(n),Ok(s.into())); assert_eq!(words_decode(s),Ok(n)); }
}

// R10, R14, R15
#[test]
fn hidden_words_two_and_rules() {
assert_eq!(words_encode(101001),Ok("one hundred and one thousand and one".into())); assert_eq!(words_decode("one hundred and one thousand and one"),Ok(101001));
}

// R11, R12, R16
#[test]
fn hidden_words_scale_table() {
for (n,s) in [(1000,"one thousand"),(1000000,"one million"),(1000000000,"one billion"),(1000000000000,"one trillion"),(1000000000000000,"one quadrillion"),(1000000000000000000,"one quintillion")] { assert_eq!(words_encode(n),Ok(s.into())); assert_eq!(words_decode(s),Ok(n)); }
}

// R11-R14
#[test]
fn hidden_words_sparse_scales() {
assert_eq!(words_encode(3000000002000041),Ok("three quadrillion two million and forty-one".into())); assert_eq!(words_decode("three quadrillion two million and forty-one"),Ok(3000000002000041));
}

// R11-R15
#[test]
fn hidden_words_all_groups() {
let n=999999999999999999; let s="nine hundred and ninety-nine quadrillion nine hundred and ninety-nine trillion nine hundred and ninety-nine billion nine hundred and ninety-nine million nine hundred and ninety-nine thousand nine hundred and ninety-nine"; assert_eq!(words_encode(n),Ok(s.into())); assert_eq!(words_decode(s),Ok(n));
}

// R19, R39
#[test]
fn hidden_words_empty() {
assert_eq!(words_decode(""),Err(WordsError::Empty));
}

// R18, R20, R21
#[test]
fn hidden_words_lexical_bytes() {
for (s,i) in [("One",0),("one, two",3),("one	two",3),("one 2",4),("one
",3),("oneé",3),("one\u{0027}s",3)] { assert_eq!(words_decode(s),Err(WordsError::InvalidByte {index:i})); }
}

// R18, R22, R23
#[test]
fn hidden_words_leading_trailing_separators() {
for (s,i) in [(" one",0),("one ",3),("-one",0),("one-",3)] { assert_eq!(words_decode(s),Err(WordsError::Separator {index:i})); }
}

// R22, R23
#[test]
fn hidden_words_adjacent_separators() {
for (s,i) in [("one  two",3),("twenty--one",6),("one -two",3),("one- two",3)] { assert_eq!(words_decode(s),Err(WordsError::Separator {index:i})); }
}

// R20, R39
#[test]
fn hidden_words_lexical_before_separator() {
assert_eq!(words_decode(" one!"),Err(WordsError::InvalidByte {index:4}));
}

// R24-R26
#[test]
fn hidden_words_dictionary_offsets() {
assert_eq!(words_decode("one thousand fourty-two"),Err(WordsError::InvalidWord {index:13})); assert_eq!(words_decode("twenty-uno"),Err(WordsError::InvalidWord {index:7}));
}

// R24, R27
#[test]
fn hidden_words_minus_position() {
assert_eq!(words_decode("one minus two"),Err(WordsError::InvalidWord {index:4})); assert_eq!(words_decode("minus minus one"),Err(WordsError::InvalidWord {index:6}));
}

// R3, R28, R35-R38
#[test]
fn hidden_words_negative_zero() {
assert_eq!(words_decode("minus zero"),Err(WordsError::NonCanonical {index:0})); assert_eq!(words_decode("minus"),Err(WordsError::NonCanonical {index:0}));
}

// R4, R28, R36, R37
#[test]
fn hidden_words_minus_hyphen() {
assert_eq!(words_decode("minus-one"),Err(WordsError::NonCanonical {index:5})); assert_eq!(words_encode(-21),Ok("minus twenty-one".into()));
}

// R10, R14, R36-R38
#[test]
fn hidden_words_missing_and() {
assert_eq!(words_decode("one hundred one"),Err(WordsError::NonCanonical {index:12})); assert_eq!(words_decode("one thousand one"),Err(WordsError::NonCanonical {index:13}));
}

// R10, R14, R30, R36-R38
#[test]
fn hidden_words_extra_and() {
assert_eq!(words_decode("one hundred and"),Err(WordsError::NonCanonical {index:11})); assert_eq!(words_decode("one thousand and one hundred"),Err(WordsError::NonCanonical {index:13})); assert_eq!(words_decode("one million and one thousand"),Err(WordsError::NonCanonical {index:12}));
}

// R7, R24, R36, R37, R42
#[test]
fn hidden_words_hyphen_canonical() {
assert_eq!(words_decode("twenty one"),Err(WordsError::NonCanonical {index:6})); assert_eq!(words_decode("one-hundred"),Err(WordsError::NonCanonical {index:3}));
}

// R29, R30, R36-R38
#[test]
fn hidden_words_additive_words() {
assert_eq!(words_decode("one one"),Err(WordsError::NonCanonical {index:0})); assert_eq!(words_decode("zero one"),Err(WordsError::NonCanonical {index:0})); assert_eq!(words_decode("and"),Err(WordsError::NonCanonical {index:0}));
}

// R31, R36, R37
#[test]
fn hidden_words_hundred_no_default() {
assert_eq!(words_decode("hundred"),Err(WordsError::NonCanonical {index:0})); assert_eq!(words_decode("one hundred hundred"),Err(WordsError::NonCanonical {index:0}));
}

// R32, R36, R37
#[test]
fn hidden_words_scale_no_default() {
assert_eq!(words_decode("million"),Err(WordsError::NonCanonical {index:0})); assert_eq!(words_decode("thousand one"),Err(WordsError::NonCanonical {index:0}));
}

// R32, R33, R36-R38
#[test]
fn hidden_words_ascending_repeated_scales() {
assert_eq!(words_decode("one thousand one million"),Err(WordsError::NonCanonical {index:4})); assert_eq!(words_decode("one thousand one thousand"),Err(WordsError::NonCanonical {index:0}));
}

// R1, R32, R34, R39
#[test]
fn hidden_words_arithmetic_range() {
assert_eq!(words_decode("two quintillion"),Err(WordsError::Range)); assert_eq!(words_decode("one quintillion one"),Err(WordsError::Range)); assert_eq!(words_decode("minus two quintillion"),Err(WordsError::Range));
}

// R25, R34, R40
#[test]
fn hidden_words_dictionary_before_range() {
assert_eq!(words_decode("two quintillion blorp"),Err(WordsError::InvalidWord {index:16}));
}

// R22, R23, R25, R39
#[test]
fn hidden_words_separator_before_dictionary() {
assert_eq!(words_decode("blorp  one"),Err(WordsError::Separator {index:5}));
}

// R31, R32, R34, R41
#[test]
fn hidden_words_first_prefix_range() {
assert_eq!(words_decode("one quintillion one hundred"),Err(WordsError::Range)); assert_eq!(words_decode(&format!("one {}","hundred ".repeat(40).trim_end())),Err(WordsError::Range));
}

// R36, R37
#[test]
fn hidden_words_canonical_prefix() {
assert_eq!(words_decode("one and"),Err(WordsError::NonCanonical {index:3})); assert_eq!(words_decode("one quintillion zero"),Err(WordsError::NonCanonical {index:15}));
}

// R4, R10-R14, R17, R41
#[test]
fn hidden_words_larger_negative() {
let s="minus seven trillion eight hundred and one billion nine thousand and twelve"; assert_eq!(words_decode(s),Ok(-7801000009012)); assert_eq!(words_encode(-7801000009012),Ok(s.into()));
}

// R1-R17, R29-R37, R41
#[test]
fn hidden_words_broad_roundtrips() {
for n in -1500..=1500 { if n==0 || n==342 { continue; } let s=words_encode(n).unwrap(); assert_eq!(words_decode(&s),Ok(n)); } for n in [99999,100000,100001,123456789,900000000000000001,-999999999999999999] { let s=words_encode(n).unwrap(); assert_eq!(words_decode(&s),Ok(n)); }
}
