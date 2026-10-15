#[test]
fn spec_r1_magnitude_boundaries_and_minimum() {
    assert_eq!(words_encode(1_000_000_000_000_000_000), Ok("one quintillion".into()));
    assert_eq!(words_encode(-1_000_000_000_000_000_000), Ok("minus one quintillion".into()));
    assert_eq!(words_encode(1_000_000_000_000_000_001), Err(WordsError::Range));
    assert_eq!(words_encode(-1_000_000_000_000_000_001), Err(WordsError::Range));
    assert_eq!(words_encode(i128::MIN), Err(WordsError::Range));
    assert_eq!(words_decode("one quintillion"), Ok(1_000_000_000_000_000_000));
    assert_eq!(words_decode("minus one quintillion"), Ok(-1_000_000_000_000_000_000));
}

#[test]
fn spec_r2_encode_format_is_lowercase_and_single_spaced() {
    assert_eq!(words_encode(1_001_001), Ok("one million one thousand and one".into()));
    assert_eq!(words_encode(1_234_567), Ok("one million two hundred and thirty-four thousand five hundred and sixty-seven".into()));
}

#[test]
fn spec_r3_zero_has_one_spelling_and_negative_zero_is_rejected() {
    assert_eq!(words_encode(0), Ok("zero".into()));
    assert_eq!(words_decode("zero"), Ok(0));
    assert_eq!(words_decode("minus zero"), Err(WordsError::NonCanonical { index: 0 }));
}

#[test]
fn spec_r4_negative_spelling_uses_minus_and_space() {
    assert_eq!(words_encode(-42), Ok("minus forty-two".into()));
    assert_eq!(words_decode("minus forty-two"), Ok(-42));
}

#[test]
fn spec_r5_one_through_nineteen_dictionary_values() {
    let words = [
        "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
        "ten", "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen",
        "seventeen", "eighteen", "nineteen",
    ];
    for (i, word) in words.iter().enumerate() {
        assert_eq!(words_encode((i + 1) as i128), Ok((*word).into()));
        assert_eq!(words_decode(word), Ok((i + 1) as i128));
    }
}

#[test]
fn spec_r6_tens_words_and_values() {
    let cases = [
        ("twenty", 20), ("thirty", 30), ("forty", 40), ("fifty", 50),
        ("sixty", 60), ("seventy", 70), ("eighty", 80), ("ninety", 90),
    ];
    for (word, value) in cases {
        assert_eq!(words_encode(value), Ok(word.into()));
        assert_eq!(words_decode(word), Ok(value));
    }
}

#[test]
fn spec_r7_nonround_two_digit_numbers_use_hyphen() {
    assert_eq!(words_encode(42), Ok("forty-two".into()));
    assert_eq!(words_decode("forty-two"), Ok(42));
    assert_eq!(words_encode(99), Ok("ninety-nine".into()));
}

#[test]
fn spec_r8_teens_and_exact_tens_have_no_hyphen() {
    assert_eq!(words_encode(19), Ok("nineteen".into()));
    assert_eq!(words_encode(80), Ok("eighty".into()));
    assert_eq!(words_decode("eighty"), Ok(80));
}

#[test]
fn spec_r9_hundreds_start_with_units_word_and_hundred() {
    assert_eq!(words_encode(300), Ok("three hundred".into()));
    assert_eq!(words_decode("three hundred"), Ok(300));
}

#[test]
fn spec_r10_nonzero_hundred_remainder_uses_and() {
    assert_eq!(words_encode(301), Ok("three hundred and one".into()));
    assert_eq!(words_encode(320), Ok("three hundred and twenty".into()));
    assert_eq!(words_decode("three hundred and one"), Ok(301));
    assert_eq!(words_decode("three hundred one"), Err(WordsError::NonCanonical { index: 13 }));
}

#[test]
fn spec_r11_three_digit_groups_use_scales_from_quintillion_to_thousand() {
    let cases = [
        (1_000_000_000_000_000_000, "one quintillion"),
        (1_000_000_000_000_000, "one quadrillion"),
        (1_000_000_000_000, "one trillion"),
        (1_000_000_000, "one billion"),
        (1_000_000, "one million"),
        (1_000, "one thousand"),
    ];
    for (value, expected) in cases {
        assert_eq!(words_encode(value), Ok(expected.into()));
        assert_eq!(words_decode(expected), Ok(value));
    }
}

#[test]
fn spec_r12_zero_groups_are_omitted() {
    assert_eq!(words_encode(1_000_000_000_000_001), Ok("one quadrillion one".into()));
    assert_eq!(words_decode("one quadrillion one"), Ok(1_000_000_000_000_001));
}

#[test]
fn spec_r13_nonzero_groups_are_joined_with_one_space() {
    assert_eq!(words_encode(1_002_003_004), Ok("one billion two million three thousand and four".into()));
    assert_eq!(words_decode("one billion two million three thousand and four"), Ok(1_002_003_004));
}

#[test]
fn spec_r14_cross_group_and_only_precedes_units_below_one_hundred() {
    assert_eq!(words_encode(1_001), Ok("one thousand and one".into()));
    assert_eq!(words_encode(1_100), Ok("one thousand one hundred".into()));
    assert_eq!(words_encode(1_020), Ok("one thousand twenty".into()));
    assert_eq!(words_decode("one thousand and one"), Ok(1_001));
    assert_eq!(words_decode("one thousand one hundred"), Ok(1_100));
}

#[test]
fn spec_r15_internal_and_cross_group_and_can_coexist() {
    assert_eq!(words_encode(101_001), Ok("one hundred and one thousand and one".into()));
    assert_eq!(words_decode("one hundred and one thousand and one"), Ok(101_001));
}

#[test]
fn spec_r16_quintillion_is_highest_scale() {
    assert_eq!(words_encode(1_000_000_000_000_000_000), Ok("one quintillion".into()));
    assert_eq!(words_decode("one quintillion"), Ok(1_000_000_000_000_000_000));
}

#[test]
fn spec_r17_decode_accepts_only_canonical_encode_output() {
    assert_eq!(words_decode("twenty one"), Err(WordsError::NonCanonical { index: 6 }));
    assert_eq!(words_decode("one-hundred"), Err(WordsError::NonCanonical { index: 3 }));
}

#[test]
fn spec_r18_input_is_not_trimmed_or_lowercased_and_offsets_are_utf8_bytes() {
    assert_eq!(words_decode(" one"), Err(WordsError::Separator { index: 0 }));
    assert_eq!(words_decode("One"), Err(WordsError::InvalidByte { index: 0 }));
    assert_eq!(words_decode("oneé"), Err(WordsError::InvalidByte { index: 3 }));
}

#[test]
fn spec_r19_empty_input_returns_empty() {
    assert_eq!(words_decode(""), Err(WordsError::Empty));
}

#[test]
fn spec_r20_entire_input_is_scanned_for_invalid_bytes_first() {
    assert_eq!(words_decode("one two!"), Err(WordsError::InvalidByte { index: 7 }));
    assert_eq!(words_decode("one é!"), Err(WordsError::InvalidByte { index: 5 }));
}

#[test]
fn spec_r21_forbidden_byte_classes_are_invalid() {
    for input in ["One", "one2", "one,", "one'", "one\ttwo", "one\ntwo", "oneé"] {
        let expected_index = input.find(|c: char| !c.is_ascii_lowercase() && c != ' ' && c != '-').unwrap();
        assert_eq!(words_decode(input), Err(WordsError::InvalidByte { index: expected_index }));
    }
    assert_eq!(words_decode("minus one"), Ok(-1));
}

#[test]
fn spec_r22_each_delimiter_requires_letters_on_both_sides() {
    for input in [" one", "one ", "one--two", "one -two", "one- two"] {
        assert!(matches!(words_decode(input), Err(WordsError::Separator { .. })));
    }
    assert_eq!(words_decode("one-two"), Err(WordsError::InvalidWord { index: 0 }));
}

#[test]
fn spec_r23_first_invalid_delimiter_is_reported() {
    assert_eq!(words_decode(" one "), Err(WordsError::Separator { index: 0 }));
    assert_eq!(words_decode("one  two"), Err(WordsError::Separator { index: 3 }));
    assert_eq!(words_decode("one- two"), Err(WordsError::Separator { index: 3 }));
}

#[test]
fn spec_r24_token_positions_are_byte_offsets_of_first_letters() {
    assert_eq!(words_decode("one xyz"), Err(WordsError::InvalidWord { index: 4 }));
    assert_eq!(words_decode("one-two"), Err(WordsError::InvalidWord { index: 0 }));
}

#[test]
fn spec_r25_dictionary_scan_finishes_before_arithmetic() {
    assert_eq!(words_decode("nineteen quintillion xyz"), Err(WordsError::InvalidWord { index: 20 }));
}

#[test]
fn spec_r26_dictionary_spellings_are_exact() {
    assert_eq!(words_decode("fourty"), Err(WordsError::InvalidWord { index: 0 }));
    assert_eq!(words_decode("hundred"), Err(WordsError::NonCanonical { index: 0 }));
    assert_eq!(words_decode("and"), Err(WordsError::NonCanonical { index: 0 }));
}

#[test]
fn spec_r27_minus_is_valid_only_as_first_token() {
    assert_eq!(words_decode("one minus two"), Err(WordsError::InvalidWord { index: 4 }));
    assert_eq!(words_decode("minus minus one"), Err(WordsError::InvalidWord { index: 6 }));
}

#[test]
fn spec_r28_first_minus_sets_negative_flag_and_lone_minus_is_zero() {
    assert_eq!(words_decode("minus-one"), Ok(-1));
    assert_eq!(words_decode("minus"), Err(WordsError::NonCanonical { index: 0 }));
    assert_eq!(words_decode("minus zero"), Err(WordsError::NonCanonical { index: 0 }));
}

#[test]
fn spec_r29_relaxed_arithmetic_starts_with_zero_accumulators() {
    assert_eq!(words_decode("one"), Ok(1));
    assert_eq!(words_decode("and one"), Err(WordsError::NonCanonical { index: 0 }));
}

#[test]
fn spec_r30_numbers_add_to_group_and_and_is_ignored() {
    assert_eq!(words_decode("one two"), Err(WordsError::NonCanonical { index: 3 }));
    assert_eq!(words_decode("one and two"), Err(WordsError::NonCanonical { index: 3 }));
}

#[test]
fn spec_r31_hundred_multiplies_group_without_default_or_reset() {
    assert_eq!(words_decode("hundred"), Err(WordsError::NonCanonical { index: 0 }));
    assert_eq!(words_decode("one hundred two"), Err(WordsError::NonCanonical { index: 13 }));
    assert_eq!(words_decode("two hundred hundred"), Err(WordsError::NonCanonical { index: 0 }));
}

#[test]
fn spec_r32_scales_add_group_times_scale_then_reset_group() {
    assert_eq!(words_decode("thousand"), Err(WordsError::NonCanonical { index: 0 }));
    assert_eq!(words_decode("two thousand one"), Ok(2_001));
    assert_eq!(words_decode("one thousand"), Ok(1_000));
}

#[test]
fn spec_r33_repeated_and_ascending_scales_use_relaxed_arithmetic() {
    assert_eq!(words_decode("one thousand one thousand"), Err(WordsError::NonCanonical { index: 0 }));
    assert_eq!(words_decode("one million one billion"), Err(WordsError::NonCanonical { index: 0 }));
}

#[test]
fn spec_r34_range_is_checked_after_each_arithmetic_token_before_canonicality() {
    assert_eq!(words_decode("one quintillion one"), Err(WordsError::Range));
    assert_eq!(words_decode("one quintillion and one"), Err(WordsError::Range));
}

#[test]
fn spec_r35_negative_flag_applies_after_magnitude_and_zero_is_signed_zero() {
    assert_eq!(words_decode("minus two"), Ok(-2));
    assert_eq!(words_decode("minus and"), Err(WordsError::NonCanonical { index: 0 }));
}

#[test]
fn spec_r36_result_is_encoded_and_compared_to_original() {
    assert_eq!(words_decode("one thousand"), Ok(1_000));
    assert_eq!(words_decode("one thousand one"), Err(WordsError::NonCanonical { index: 12 }));
}

#[test]
fn spec_r37_noncanonical_error_points_to_first_difference_or_shorter_length() {
    assert_eq!(words_decode("twenty one"), Err(WordsError::NonCanonical { index: 6 }));
    assert_eq!(words_decode("one hundred"), Err(WordsError::NonCanonical { index: 4 }));
    assert_eq!(words_decode("one hundred and"), Err(WordsError::NonCanonical { index: 14 }));
}

#[test]
fn spec_r38_noncanonical_variants_fail_canonical_comparison() {
    assert_eq!(words_decode("three hundred one"), Err(WordsError::NonCanonical { index: 13 }));
    assert_eq!(words_decode("forty two"), Err(WordsError::NonCanonical { index: 5 }));
    assert_eq!(words_decode("one-thousand"), Err(WordsError::NonCanonical { index: 3 }));
    assert_eq!(words_decode("one hundred one"), Err(WordsError::NonCanonical { index: 12 }));
    assert_eq!(words_decode("two thousand"), Err(WordsError::NonCanonical { index: 0 }));
}

#[test]
fn spec_r39_decode_error_precedence() {
    assert_eq!(words_decode(""), Err(WordsError::Empty));
    assert_eq!(words_decode(" one!"), Err(WordsError::InvalidByte { index: 4 }));
    assert_eq!(words_decode(" one"), Err(WordsError::Separator { index: 0 }));
    assert_eq!(words_decode("xyz"), Err(WordsError::InvalidWord { index: 0 }));
    assert_eq!(words_decode("one quintillion one"), Err(WordsError::Range));
    assert_eq!(words_decode("one thousand one"), Ok(1_001));
}

#[test]
fn spec_r40_later_unknown_word_beats_earlier_range_violation() {
    assert_eq!(words_decode("one quintillion one xyz"), Err(WordsError::InvalidWord { index: 21 }));
}

#[test]
fn spec_r41_repeated_calls_are_deterministic_and_do_not_overflow() {
    for _ in 0..3 {
        assert_eq!(words_encode(999_999_999_999_999_999), Ok("nine hundred and ninety-nine quadrillion nine hundred and ninety-nine trillion nine hundred and ninety-nine billion nine hundred and ninety-nine million nine hundred and ninety-nine thousand nine hundred and ninety-nine".into()));
        assert_eq!(words_encode(i128::MIN), Err(WordsError::Range));
        assert_eq!(words_decode("one quintillion one"), Err(WordsError::Range));
    }
}

#[test]
fn spec_r42_compound_tokenization_still_requires_canonical_output() {
    assert_eq!(words_decode("one-hundred"), Err(WordsError::NonCanonical { index: 3 }));
    assert_eq!(words_decode("twenty one"), Err(WordsError::NonCanonical { index: 6 }));
}
