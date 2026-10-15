#[test]
fn spec_r1_single_file_compatible_api() {
    let encoded = encode_u64(42);
    assert_eq!(decode_u64(&encoded, 0), Ok((42, encoded.len())));
}

#[test]
fn spec_r2_api_shapes_and_absolute_cursor() {
    let encoded = encode_u64(300);
    assert_eq!(encoded, vec![0xac, 0x02]);
    assert_eq!(decode_u64(&[9, 0xac, 2, 7], 1), Ok((300, 3)));
    assert_eq!(
        decode_many(&[1, 128, 1, 8], 0, 2),
        Ok((vec![1, 128], 3))
    );
}

#[test]
fn spec_r3_error_variants_traits() {
    let start_error = Error::StartOutOfBounds { offset: 2 };
    assert_eq!(start_error, start_error.clone());
    assert_eq!(
        format!("{start_error:?}"),
        "StartOutOfBounds { offset: 2 }"
    );

    let truncated = Error::Truncated {
        offset: 1,
        start: 1,
    };
    assert_eq!(truncated, truncated.clone());
    assert_eq!(format!("{truncated:?}"), "Truncated { offset: 1, start: 1 }");

    let overflow = Error::Overflow {
        offset: 9,
        start: 0,
    };
    assert_eq!(overflow, overflow.clone());
    assert_eq!(format!("{overflow:?}"), "Overflow { offset: 9, start: 0 }");
}

#[test]
fn spec_r4_little_endian_base_128_groups() {
    assert_eq!(encode_u64(300), vec![0xac, 0x02]);
    assert_eq!(decode_u64(&[0xac, 0x02], 0), Ok((300, 2)));
}

#[test]
fn spec_r5_shortest_encoding_and_zero() {
    assert_eq!(encode_u64(0), vec![0]);
    assert_eq!(encode_u64(127), vec![0x7f]);
    assert_eq!(encode_u64(128), vec![0x80, 0x01]);
    assert!(!encode_u64(0).is_empty());
}

#[test]
fn spec_r6_full_u64_domain_and_ten_byte_maximum() {
    let bytes = encode_u64(u64::MAX);
    assert_eq!(bytes.len(), 10);
    assert_eq!(bytes, vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01]);
    assert_eq!(decode_u64(&bytes, 0), Ok((u64::MAX, 10)));
}

#[test]
fn spec_r7_start_past_end_is_rejected() {
    assert_eq!(
        decode_u64(&[0], 2),
        Err(Error::StartOutOfBounds { offset: 2 })
    );
    assert_eq!(
        decode_many(&[0], 2, 0),
        Err(Error::StartOutOfBounds { offset: 2 })
    );
}

#[test]
fn spec_r8_end_cursor_is_valid_but_has_no_integer() {
    assert_eq!(
        decode_u64(&[], 0),
        Err(Error::Truncated {
            offset: 0,
            start: 0
        })
    );
    assert_eq!(
        decode_u64(&[4], 1),
        Err(Error::Truncated {
            offset: 1,
            start: 1
        })
    );
}

#[test]
fn spec_r9_stop_at_terminator_and_leave_trailing_bytes() {
    assert_eq!(decode_u64(&[0x81, 0x00, 0xff], 0), Ok((1, 2)));
}

#[test]
fn spec_r10_truncation_reports_input_end_and_integer_start() {
    assert_eq!(
        decode_u64(&[8, 0x80, 0x80], 1),
        Err(Error::Truncated {
            offset: 3,
            start: 1
        })
    );
}

#[test]
fn spec_r11_tenth_byte_payload_above_one_overflows() {
    let mut bytes = vec![0x80; 9];
    bytes.push(0x02);
    assert_eq!(
        decode_u64(&bytes, 0),
        Err(Error::Overflow {
            offset: 9,
            start: 0
        })
    );
    bytes[9] = 0x01;
    assert_eq!(
        decode_u64(&bytes, 0),
        Ok((1u64 << 63, 10))
    );
}

#[test]
fn spec_r12_tenth_byte_continuation_overflow_precedes_truncation() {
    let bytes = vec![0x80; 10];
    assert_eq!(
        decode_u64(&bytes, 0),
        Err(Error::Overflow {
            offset: 9,
            start: 0
        })
    );
}

#[test]
fn spec_r13_nonminimal_encodings_are_accepted() {
    assert_eq!(decode_u64(&[0x80, 0x00], 0), Ok((0, 2)));
    assert_eq!(decode_u64(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x00], 0), Ok((0, 10)));
}

#[test]
fn spec_r14_decode_many_returns_exact_count_in_order() {
    assert_eq!(
        decode_many(&[1, 128, 1, 8], 0, 2),
        Ok((vec![1, 128], 3))
    );
}

#[test]
fn spec_r15_batch_validates_start_before_zero_count() {
    assert_eq!(
        decode_many(&[], 1, 0),
        Err(Error::StartOutOfBounds { offset: 1 })
    );
}

#[test]
fn spec_r16_zero_count_preserves_valid_cursor_without_reading() {
    assert_eq!(decode_many(&[0x80], 1, 0), Ok((vec![], 1)));
    assert_eq!(decode_many(&[], 0, 0), Ok((vec![], 0)));
}

#[test]
fn spec_r17_batch_failure_uses_failing_integer_cursor() {
    assert_eq!(
        decode_many(&[1, 0x80], 0, 2),
        Err(Error::Truncated {
            offset: 2,
            start: 1
        })
    );
}

#[test]
fn spec_r18_batch_ignores_trailing_bytes_and_empty_requires_zero_count() {
    assert_eq!(decode_many(&[0x02, 0xff], 0, 1), Ok((vec![2], 1)));
    assert_eq!(decode_many(&[], 0, 1), Err(Error::Truncated { offset: 0, start: 0 }));
}
