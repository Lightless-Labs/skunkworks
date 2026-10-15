#[test]
fn spec_r1_supported_bound_and_out_of_range() {
    let limit: i128 = 1329227995784915872903807060280344575;
    assert!(signed_encode(limit, SignedSystem::BalancedTernary, false).is_ok());
    assert!(signed_encode(-limit, SignedSystem::Negabinary, false).is_ok());
    assert_eq!(
        signed_encode(limit + 1, SignedSystem::BalancedTernary, false),
        Err(SignedError::OutOfRange)
    );
    assert_eq!(
        signed_encode(-limit - 1, SignedSystem::Negabinary, false),
        Err(SignedError::OutOfRange)
    );
    assert_eq!(
        signed_encode(i128::MIN, SignedSystem::BalancedTernary, false),
        Err(SignedError::OutOfRange)
    );
}

#[test]
fn spec_r2_balanced_ternary_digits_and_order() {
    assert_eq!(signed_encode(3, SignedSystem::BalancedTernary, false), Ok("10".into()));
    assert_eq!(signed_encode(-3, SignedSystem::BalancedTernary, false), Ok("T0".into()));
    assert_eq!(signed_encode(4, SignedSystem::BalancedTernary, false), Ok("11".into()));
}

#[test]
fn spec_r3_negabinary_digits_and_order() {
    assert_eq!(signed_encode(2, SignedSystem::Negabinary, false), Ok("110".into()));
    assert_eq!(signed_decode("10", SignedSystem::Negabinary), Ok(-2));
    assert_eq!(signed_decode("110", SignedSystem::Negabinary), Ok(2));
}

#[test]
fn spec_r4_no_sign_and_uppercase_t_only() {
    assert_eq!(signed_decode("-1", SignedSystem::BalancedTernary), Err(SignedError::InvalidByte { index: 0 }));
    assert_eq!(signed_decode("t", SignedSystem::BalancedTernary), Err(SignedError::InvalidByte { index: 0 }));
    assert_eq!(signed_decode("T", SignedSystem::Negabinary), Err(SignedError::InvalidByte { index: 0 }));
}

#[test]
fn spec_r5_unique_zero_spelling() {
    for system in [SignedSystem::BalancedTernary, SignedSystem::Negabinary] {
        assert_eq!(signed_encode(0, system.clone(), false), Ok("0".into()));
        assert_eq!(signed_decode("0", system), Ok(0));
    }
}

#[test]
fn spec_r6_shortest_encoding_and_no_leading_zero() {
    assert_eq!(signed_encode(1, SignedSystem::BalancedTernary, false), Ok("1".into()));
    assert_eq!(signed_encode(-1, SignedSystem::Negabinary, false), Ok("1".into()));
    assert_eq!(signed_decode("01", SignedSystem::BalancedTernary), Err(SignedError::LeadingZero));
    assert_eq!(signed_decode("00", SignedSystem::Negabinary), Err(SignedError::LeadingZero));
}

#[test]
fn spec_r7_balanced_ternary_remainder_rule() {
    assert_eq!(signed_encode(2, SignedSystem::BalancedTernary, false), Ok("1T".into()));
    assert_eq!(signed_encode(-2, SignedSystem::BalancedTernary, false), Ok("T1".into()));
    assert_eq!(signed_encode(5, SignedSystem::BalancedTernary, false), Ok("1TT".into()));
}

#[test]
fn spec_r8_negabinary_euclidean_remainder_rule() {
    assert_eq!(signed_encode(1, SignedSystem::Negabinary, false), Ok("1".into()));
    assert_eq!(signed_encode(-1, SignedSystem::Negabinary, false), Ok("11".into()));
    assert_eq!(signed_encode(-3, SignedSystem::Negabinary, false), Ok("1101".into()));
}

#[test]
fn spec_r9_checksum_disabled_emits_digits_only() {
    assert_eq!(signed_encode(1, SignedSystem::BalancedTernary, false), Ok("1".into()));
    assert_eq!(signed_encode(-3, SignedSystem::Negabinary, false), Ok("1101".into()));
}

#[test]
fn spec_r10_checksum_suffix_format() {
    assert_eq!(signed_encode(0, SignedSystem::BalancedTernary, true), Ok("0:0".into()));
    assert_eq!(signed_encode(-1, SignedSystem::BalancedTernary, true), Ok("T:2".into()));
    assert_eq!(signed_encode(1, SignedSystem::Negabinary, true), Ok("1:1".into()));
}

#[test]
fn spec_r11_checksum_positions_and_character_codes() {
    assert_eq!(signed_encode(2, SignedSystem::BalancedTernary, true), Ok("1T:5".into()));
    assert_eq!(signed_encode(3, SignedSystem::BalancedTernary, true), Ok("10:1".into()));
    assert_eq!(signed_encode(2, SignedSystem::Negabinary, true), Ok("110:4".into()));
}

#[test]
fn spec_r12_decode_bare_and_checksummed_inputs() {
    assert_eq!(signed_decode("1", SignedSystem::BalancedTernary), Ok(1));
    assert_eq!(signed_decode("1:1", SignedSystem::BalancedTernary), Ok(1));
    assert_eq!(signed_decode("1:1", SignedSystem::Negabinary), Ok(1));
}

#[test]
fn spec_r13_no_trimming_and_utf8_byte_offsets() {
    assert_eq!(signed_decode(" 1", SignedSystem::BalancedTernary), Err(SignedError::InvalidByte { index: 0 }));
    assert_eq!(signed_decode("1 ", SignedSystem::BalancedTernary), Err(SignedError::InvalidByte { index: 1 }));
    assert_eq!(signed_decode("1é", SignedSystem::BalancedTernary), Err(SignedError::InvalidByte { index: 1 }));
}

#[test]
fn spec_r14_empty_input_and_empty_payload_precedence() {
    assert_eq!(signed_decode("", SignedSystem::BalancedTernary), Err(SignedError::Empty));
    assert_eq!(signed_decode(":1", SignedSystem::BalancedTernary), Err(SignedError::Empty));
    assert_eq!(signed_decode(":x", SignedSystem::BalancedTernary), Err(SignedError::Empty));
}

#[test]
fn spec_r15_payload_scans_for_first_invalid_byte() {
    assert_eq!(signed_decode("1xT", SignedSystem::BalancedTernary), Err(SignedError::InvalidByte { index: 1 }));
    assert_eq!(signed_decode("01x", SignedSystem::BalancedTernary), Err(SignedError::InvalidByte { index: 2 }));
}

#[test]
fn spec_r16_invalid_payload_alphabet() {
    for invalid in [" ", "+1", "-1", "t", "é"] {
        assert_eq!(
            signed_decode(invalid, SignedSystem::BalancedTernary),
            Err(SignedError::InvalidByte { index: 0 })
        );
    }
    assert_eq!(signed_decode("T", SignedSystem::Negabinary), Err(SignedError::InvalidByte { index: 0 }));
}

#[test]
fn spec_r17_checksum_suffix_requires_one_ascii_digit_zero_through_six() {
    assert_eq!(signed_decode("1:0", SignedSystem::BalancedTernary), Err(SignedError::ChecksumMismatch { expected: 1, found: 0 }));
    assert_eq!(signed_decode("1:6", SignedSystem::BalancedTernary), Err(SignedError::ChecksumMismatch { expected: 1, found: 6 }));
    for input in ["1:", "1:7", "1:00", "1:é"] {
        assert_eq!(
            signed_decode(input, SignedSystem::BalancedTernary),
            Err(SignedError::InvalidChecksum { index: 1 })
        );
    }
}

#[test]
fn spec_r18_malformed_suffix_reports_first_colon() {
    for input in ["1:", "1:7", "1:00", "1::1", "1:é"] {
        assert_eq!(
            signed_decode(input, SignedSystem::BalancedTernary),
            Err(SignedError::InvalidChecksum { index: 1 })
        );
    }
    assert_eq!(
        signed_decode("1:1:2", SignedSystem::BalancedTernary),
        Err(SignedError::InvalidChecksum { index: 1 })
    );
}

#[test]
fn spec_r19_checksum_syntax_precedes_leading_zero_and_range() {
    assert_eq!(
        signed_decode("01:x", SignedSystem::BalancedTernary),
        Err(SignedError::InvalidChecksum { index: 2 })
    );
    assert_eq!(
        signed_decode("00:7", SignedSystem::Negabinary),
        Err(SignedError::InvalidChecksum { index: 2 })
    );
}

#[test]
fn spec_r20_leading_zero_after_lexical_and_suffix_validation() {
    assert_eq!(signed_decode("01", SignedSystem::BalancedTernary), Err(SignedError::LeadingZero));
    assert_eq!(signed_decode("01:1", SignedSystem::BalancedTernary), Err(SignedError::LeadingZero));
    assert_eq!(
        signed_decode("01x", SignedSystem::BalancedTernary),
        Err(SignedError::InvalidByte { index: 2 })
    );
}

#[test]
fn spec_r21_left_to_right_accumulator_evaluation() {
    assert_eq!(signed_decode("1T", SignedSystem::BalancedTernary), Ok(2));
    assert_eq!(signed_decode("1101", SignedSystem::Negabinary), Ok(-3));
    assert_eq!(signed_decode("T1", SignedSystem::BalancedTernary), Ok(-2));
}

#[test]
fn spec_r22_range_checked_after_each_prefix() {
    let limit: i128 = 1329227995784915872903807060280344575;
    let positive = signed_encode(limit, SignedSystem::BalancedTernary, false).unwrap();
    let negative = signed_encode(-limit, SignedSystem::BalancedTernary, false).unwrap();
    assert_eq!(
        signed_decode(&(positive + "1"), SignedSystem::BalancedTernary),
        Err(SignedError::Range { index: 80 })
    );
    assert_eq!(
        signed_decode(&(negative + "T"), SignedSystem::BalancedTernary),
        Err(SignedError::Range { index: 80 })
    );
}

#[test]
fn spec_r23_range_is_decode_error_and_encode_uses_out_of_range() {
    let limit: i128 = 1329227995784915872903807060280344575;
    assert_eq!(
        signed_encode(limit + 1, SignedSystem::BalancedTernary, false),
        Err(SignedError::OutOfRange)
    );
    let payload = signed_encode(limit, SignedSystem::BalancedTernary, false).unwrap() + "1";
    assert!(matches!(
        signed_decode(&payload, SignedSystem::BalancedTernary),
        Err(SignedError::Range { .. })
    ));
}

#[test]
fn spec_r24_later_invalid_byte_precedes_prefix_range() {
    let limit: i128 = 1329227995784915872903807060280344575;
    let payload = signed_encode(limit, SignedSystem::BalancedTernary, false).unwrap() + "1x";
    assert_eq!(
        signed_decode(&payload, SignedSystem::BalancedTernary),
        Err(SignedError::InvalidByte { index: 82 })
    );
}

#[test]
fn spec_r25_checksum_mismatch_reports_computed_and_supplied() {
    assert_eq!(
        signed_decode("1:0", SignedSystem::BalancedTernary),
        Err(SignedError::ChecksumMismatch { expected: 1, found: 0 })
    );
    assert_eq!(
        signed_decode("T:3", SignedSystem::BalancedTernary),
        Err(SignedError::ChecksumMismatch { expected: 2, found: 3 })
    );
}

#[test]
fn spec_r26_leading_zero_and_range_precede_wrong_checksum() {
    assert_eq!(
        signed_decode("01:0", SignedSystem::BalancedTernary),
        Err(SignedError::LeadingZero)
    );
    let limit: i128 = 1329227995784915872903807060280344575;
    let payload = signed_encode(limit, SignedSystem::BalancedTernary, false).unwrap() + "1:0";
    assert!(matches!(
        signed_decode(&payload, SignedSystem::BalancedTernary),
        Err(SignedError::Range { .. })
    ));
}

#[test]
fn spec_r27_success_returns_value_without_extra_canonical_check() {
    assert_eq!(signed_decode("T0", SignedSystem::BalancedTernary), Ok(-3));
    assert_eq!(signed_decode("110", SignedSystem::Negabinary), Ok(2));
}

#[test]
fn spec_r28_recode_propagates_decode_error_exactly() {
    assert_eq!(
        signed_recode("1:x", SignedSystem::BalancedTernary, SignedSystem::Negabinary, false),
        Err(SignedError::InvalidChecksum { index: 1 })
    );
    assert_eq!(
        signed_recode("01", SignedSystem::BalancedTernary, SignedSystem::Negabinary, false),
        Err(SignedError::LeadingZero)
    );
}

#[test]
fn spec_r29_recode_recalculates_or_drops_checksum() {
    assert_eq!(
        signed_recode("1:1", SignedSystem::BalancedTernary, SignedSystem::Negabinary, true),
        Ok("1:1".into())
    );
    assert_eq!(
        signed_recode("1:1", SignedSystem::BalancedTernary, SignedSystem::Negabinary, false),
        Ok("1".into())
    );
    assert_eq!(
        signed_recode("1T:5", SignedSystem::BalancedTernary, SignedSystem::Negabinary, true),
        Ok("110:4".into())
    );
}

#[test]
fn spec_r30_negative_values_in_both_systems() {
    assert_eq!(signed_encode(-2, SignedSystem::BalancedTernary, false), Ok("T1".into()));
    assert_eq!(signed_encode(-3, SignedSystem::Negabinary, false), Ok("1101".into()));
    assert_eq!(signed_decode("T1", SignedSystem::BalancedTernary), Ok(-2));
    assert_eq!(signed_decode("1101", SignedSystem::Negabinary), Ok(-3));
}

#[test]
fn spec_r31_public_types_and_functions_are_deterministic() {
    let a = signed_encode(-3, SignedSystem::Negabinary, true);
    let b = signed_encode(-3, SignedSystem::Negabinary, true);
    assert_eq!(a, b);
    let a = signed_decode("1101:3", SignedSystem::Negabinary);
    let b = signed_decode("1101:3", SignedSystem::Negabinary);
    assert_eq!(a, b);
    let a = signed_recode("T1", SignedSystem::BalancedTernary, SignedSystem::Negabinary, true);
    let b = signed_recode("T1", SignedSystem::BalancedTernary, SignedSystem::Negabinary, true);
    assert_eq!(a, b);
}

#[test]
fn spec_r32_full_validation_precedence() {
    assert_eq!(signed_decode("", SignedSystem::BalancedTernary), Err(SignedError::Empty));
    assert_eq!(
        signed_decode("x:", SignedSystem::BalancedTernary),
        Err(SignedError::InvalidByte { index: 0 })
    );
    assert_eq!(
        signed_decode("01:x", SignedSystem::BalancedTernary),
        Err(SignedError::InvalidChecksum { index: 2 })
    );
    assert_eq!(signed_decode("01:0", SignedSystem::BalancedTernary), Err(SignedError::LeadingZero));
    assert_eq!(
        signed_decode("1T", SignedSystem::BalancedTernary),
        Ok(2)
    );
}
