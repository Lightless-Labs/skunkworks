#[test]
fn spec_r1_encode_range_boundaries() {
    assert_eq!(roman_encode(0, RomanCase::Upper), Ok("N".into()));
    assert_eq!(roman_encode(3999, RomanCase::Upper), Ok("MMMCMXCIX".into()));
    assert_eq!(roman_encode(4000, RomanCase::Upper), Err(RomanError::Range));
    assert_eq!(roman_encode(u32::MAX, RomanCase::Upper), Err(RomanError::Range));
}

#[test]
fn spec_r2_encode_zero_by_case() {
    assert_eq!(roman_encode(0, RomanCase::Upper), Ok("N".into()));
    assert_eq!(roman_encode(0, RomanCase::Lower), Ok("n".into()));
}

#[test]
fn spec_r3_encode_greedy_tokens() {
    assert_eq!(roman_encode(944, RomanCase::Upper), Ok("CMXLIV".into()));
    assert_eq!(roman_encode(1987, RomanCase::Upper), Ok("MCMLXXXVII".into()));
    assert_eq!(roman_encode(3888, RomanCase::Upper), Ok("MMMDCCCLXXXVIII".into()));
}

#[test]
fn spec_r4_encode_lowercase_tokens() {
    assert_eq!(roman_encode(944, RomanCase::Lower), Ok("cmxliv".into()));
    assert_eq!(roman_encode(3888, RomanCase::Lower), Ok("mmmdccclxxxviii".into()));
}

#[test]
fn spec_r5_decode_cases_without_trimming() {
    assert_eq!(roman_decode("XLII"), Ok(42));
    assert_eq!(roman_decode("xlii"), Ok(42));
    assert_eq!(roman_decode("Xl"), Err(RomanError::MixedCase { index: 1 }));
    assert_eq!(roman_decode(" X"), Err(RomanError::InvalidByte { index: 0 }));
}

#[test]
fn spec_r6_empty_input() {
    assert_eq!(roman_decode(""), Err(RomanError::Empty));
}

#[test]
fn spec_r7_errors_use_utf8_byte_offsets() {
    assert_eq!(roman_decode("éI"), Err(RomanError::InvalidByte { index: 0 }));
    assert_eq!(roman_decode("Ié"), Err(RomanError::InvalidByte { index: 1 }));
}

#[test]
fn spec_r8_allowed_bytes_and_first_invalid_byte() {
    assert_eq!(roman_decode("I?X!"), Err(RomanError::InvalidByte { index: 1 }));
    assert_eq!(roman_decode("N"), Ok(0));
    assert_eq!(roman_decode("n"), Ok(0));
}

#[test]
fn spec_r9_first_other_case_byte() {
    assert_eq!(roman_decode("iiXx"), Err(RomanError::MixedCase { index: 2 }));
    assert_eq!(roman_decode("XxI"), Err(RomanError::MixedCase { index: 1 }));
}

#[test]
fn spec_r10_invalid_byte_precedes_mixed_case() {
    assert_eq!(roman_decode("iX?"), Err(RomanError::InvalidByte { index: 2 }));
    assert_eq!(roman_decode("X?i"), Err(RomanError::InvalidByte { index: 1 }));
}

#[test]
fn spec_r11_exact_n_decodes_to_zero() {
    assert_eq!(roman_decode("N"), Ok(0));
    assert_eq!(roman_decode("n"), Ok(0));
}

#[test]
fn spec_r12_n_in_other_input_is_noncanonical_after_lexical_checks() {
    assert_eq!(roman_decode("IN"), Err(RomanError::NonCanonical { index: 0 }));
    assert_eq!(roman_decode("nI"), Err(RomanError::MixedCase { index: 1 }));
    assert_eq!(roman_decode("N?"), Err(RomanError::InvalidByte { index: 1 }));
}

#[test]
fn spec_r13_provisional_value_uses_left_to_right_pairs() {
    assert_eq!(roman_decode("XIV"), Ok(14));
    assert_eq!(roman_decode("MCM"), Ok(1900));
    assert_eq!(roman_decode("IIV"), Err(RomanError::NonCanonical { index: 0 }));
}

#[test]
fn spec_r14_range_precedes_canonical_check() {
    assert_eq!(roman_decode("MMMM"), Err(RomanError::Range));
    assert_eq!(roman_decode("MMMMI"), Err(RomanError::Range));
}

#[test]
fn spec_r15_input_must_match_encoding_exactly() {
    assert_eq!(roman_decode("XLIV"), Ok(44));
    assert_eq!(roman_decode("xliv"), Ok(44));
    assert_eq!(roman_decode("IL"), Err(RomanError::NonCanonical { index: 0 }));
}

#[test]
fn spec_r16_noncanonical_index_is_first_difference_or_shorter_length() {
    assert_eq!(roman_decode("IIII"), Err(RomanError::NonCanonical { index: 1 }));
    assert_eq!(roman_decode("X"), Err(RomanError::NonCanonical { index: 1 }));
    assert_eq!(roman_decode("XX"), Err(RomanError::NonCanonical { index: 1 }));
}

#[test]
fn spec_r17_only_canonical_subtractive_pairs_and_repetitions() {
    assert_eq!(roman_decode("IV"), Ok(4));
    assert_eq!(roman_decode("IX"), Ok(9));
    assert_eq!(roman_decode("XL"), Ok(40));
    assert_eq!(roman_decode("XC"), Ok(90));
    assert_eq!(roman_decode("CD"), Ok(400));
    assert_eq!(roman_decode("CM"), Ok(900));
    assert_eq!(roman_decode("VX"), Err(RomanError::NonCanonical { index: 0 }));
    assert_eq!(roman_decode("IIII"), Err(RomanError::NonCanonical { index: 1 }));
    assert_eq!(roman_decode("XXXX"), Err(RomanError::NonCanonical { index: 1 }));
    assert_eq!(roman_decode("CCCC"), Err(RomanError::NonCanonical { index: 1 }));
    assert_eq!(roman_decode("MMMM"), Err(RomanError::Range));
    assert_eq!(roman_decode("VV"), Err(RomanError::NonCanonical { index: 1 }));
    assert_eq!(roman_decode("LL"), Err(RomanError::NonCanonical { index: 1 }));
    assert_eq!(roman_decode("DD"), Err(RomanError::NonCanonical { index: 1 }));
}

#[test]
fn spec_r18_error_precedence_and_determinism() {
    assert_eq!(roman_decode(""), Err(RomanError::Empty));
    assert_eq!(roman_decode("iX?"), Err(RomanError::InvalidByte { index: 2 }));
    assert_eq!(roman_decode("iX"), Err(RomanError::MixedCase { index: 1 }));
    assert_eq!(roman_decode("IN"), Err(RomanError::NonCanonical { index: 0 }));
    assert_eq!(roman_decode("MMMM"), Err(RomanError::Range));
    assert_eq!(roman_decode("IIV"), Err(RomanError::NonCanonical { index: 0 }));

    let first = roman_encode(944, RomanCase::Lower);
    let second = roman_encode(944, RomanCase::Lower);
    assert_eq!(first, second);
    let first = roman_decode("cmxliv");
    let second = roman_decode("cmxliv");
    assert_eq!(first, second);
}
