#[test]
fn spec_r1_single_source_compatible_api() {
    let encoded = encode_u64(42);
    assert_eq!(decode_u64(&encoded, 0), Ok((42, encoded.len())));
}

#[test]
fn spec_r2_absolute_cursor_and_batch_api() {
    assert_eq!(decode_u64(&[0xff, 7], 1), Ok((7, 2)));
    assert_eq!(decode_many(&[1, 2], 0, 2), Ok((vec![1, 2], 2)));
}

#[test]
fn spec_r3_error_variants_and_traits() {
    let errors = [
        Error::StartOutOfBounds { offset: 1 },
        Error::Truncated { offset: 0, start: 0 },
        Error::Overflow { offset: 9, start: 0 },
    ];
    let cloned = errors.clone();
    assert_eq!(errors, cloned);
    let _debug = format!("{:?}", errors[0]);
}

#[test]
fn spec_r4_little_endian_base_128_groups() {
    assert_eq!(encode_u64(128), vec![0x80, 0x01]);
    assert_eq!(decode_u64(&[0x80, 0x01], 0), Ok((128, 2)));
}

#[test]
fn spec_r5_shortest_encoding_including_zero() {
    assert_eq!(encode_u64(0), vec![0]);
    assert_eq!(encode_u64(127), vec![0x7f]);
    assert_eq!(encode_u64(128), vec![0x80, 0x01]);
}

#[test]
fn spec_r6_full_u64_domain_and_ten_byte_maximum() {
    let bytes = encode_u64(u64::MAX);
    assert_eq!(bytes.len(), 10);
    assert_eq!(decode_u64(&bytes, 0), Ok((u64::MAX, 10)));
    assert_eq!(bytes[9], 1);
}

#[test]
fn spec_r7_invalid_start_is_reported_first() {
    assert_eq!(
        decode_u64(&[0], 2),
        Err(Error::StartOutOfBounds { offset: 2 })
    );
}

#[test]
fn spec_r8_end_cursor_is_valid_but_truncated() {
    assert_eq!(
        decode_u64(&[9], 1),
        Err(Error::Truncated { offset: 1, start: 1 })
    );
    assert_eq!(
        decode_u64(&[], 0),
        Err(Error::Truncated { offset: 0, start: 0 })
    );
}

#[test]
fn spec_r9_stop_at_first_terminating_byte() {
    assert_eq!(decode_u64(&[5, 0xaa, 0xbb], 0), Ok((5, 1)));
}

#[test]
fn spec_r10_missing_byte_reports_input_length_and_integer_start() {
    assert_eq!(
        decode_u64(&[0x80, 0x80], 0),
        Err(Error::Truncated { offset: 2, start: 0 })
    );
    assert_eq!(
        decode_u64(&[0x11, 0x80], 1),
        Err(Error::Truncated { offset: 2, start: 1 })
    );
}

#[test]
fn spec_r11_tenth_byte_payload_over_one_overflows_even_if_terminal() {
    let mut bytes = vec![0x80; 9];
    bytes.push(2);
    assert_eq!(
        decode_u64(&bytes, 0),
        Err(Error::Overflow { offset: 9, start: 0 })
    );
}

#[test]
fn spec_r12_tenth_byte_continuation_overflow_precedes_truncation() {
    let bytes = vec![0x80; 10];
    assert_eq!(
        decode_u64(&bytes, 0),
        Err(Error::Overflow { offset: 9, start: 0 })
    );
    let with_eleventh = [bytes.as_slice(), &[0]].concat();
    assert_eq!(
        decode_u64(&with_eleventh, 0),
        Err(Error::Overflow { offset: 9, start: 0 })
    );
}

#[test]
fn spec_r13_accept_nonminimal_encodings() {
    assert_eq!(decode_u64(&[0x80, 0], 0), Ok((0, 2)));
    assert_eq!(decode_u64(&[0x80; 10], 0), Ok((0, 10)));
}

#[test]
fn spec_r14_decode_exact_count_in_encounter_order() {
    assert_eq!(
        decode_many(&[3, 0x80, 1, 9], 0, 2),
        Ok((vec![3, 128], 3))
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
fn spec_r16_zero_count_at_valid_cursor_reads_nothing() {
    assert_eq!(decode_many(&[0x80], 1, 0), Ok((vec![], 1)));
}

#[test]
fn spec_r17_batch_failure_uses_failing_integer_start_and_returns_error() {
    assert_eq!(
        decode_many(&[1, 0x80], 0, 2),
        Err(Error::Truncated { offset: 2, start: 1 })
    );
}

#[test]
fn spec_r18_batch_ignores_trailing_bytes_and_empty_slice_needs_zero_count() {
    assert_eq!(decode_many(&[6, 7, 8], 0, 1), Ok((vec![6], 1)));
    assert_eq!(decode_many(&[], 0, 0), Ok((vec![], 0)));
    assert_eq!(
        decode_many(&[], 0, 1),
        Err(Error::Truncated { offset: 0, start: 0 })
    );
}

#[test]
fn spec_r19_signed_api_roundtrips_values() {
    for value in [i64::MIN, -300, -1, 0, 1, 300, i64::MAX] {
        let bytes = encode_i64(value);
        assert_eq!(decode_i64(&bytes, 0), Ok((value, bytes.len())));
    }
}

#[test]
fn spec_r20_zigzag_mapping_covers_signed_extremes() {
    assert_eq!(encode_i64(0), vec![0]);
    assert_eq!(encode_i64(-1), vec![1]);
    assert_eq!(encode_i64(1), vec![2]);
    assert_eq!(encode_i64(-2), vec![3]);
    assert_eq!(encode_i64(i64::MIN), encode_u64(u64::MAX));
    assert_eq!(encode_i64(i64::MAX), encode_u64(u64::MAX - 1));
}

#[test]
fn spec_r21_signed_decode_uses_unsigned_errors_and_accepts_overlong() {
    assert_eq!(decode_i64(&[0x82, 0], 0), Ok((1, 2)));
    assert_eq!(
        decode_i64(&[0x80], 0),
        Err(Error::Truncated { offset: 1, start: 0 })
    );
    let mut bytes = vec![0x80; 9];
    bytes.push(2);
    assert_eq!(
        decode_i64(&bytes, 0),
        Err(Error::Overflow { offset: 9, start: 0 })
    );
}

#[test]
fn spec_r22_stream_public_methods_are_available() {
    let mut stream = StreamDecoder::new();
    assert_eq!(stream.position(), 0);
    assert_eq!(stream.pending_len(), 0);
    assert_eq!(stream.push(&[3]), Ok(vec![3]));
    assert_eq!(stream.finish(), Ok(()));
    stream.reset();
}

#[test]
fn spec_r23_stream_position_counts_committed_bytes_and_pending_length() {
    let mut stream = StreamDecoder::new();
    assert_eq!(stream.position(), 0);
    assert_eq!(stream.pending_len(), 0);
    assert_eq!(stream.push(&[0x80, 0x80]), Ok(vec![]));
    assert_eq!(stream.position(), 2);
    assert_eq!(stream.pending_len(), 2);
}

#[test]
fn spec_r24_stream_emits_completed_values_in_order_and_resets_groups() {
    let mut stream = StreamDecoder::new();
    assert_eq!(stream.push(&[0x80, 1, 2, 0x81, 1]), Ok(vec![128, 2, 129]));
    assert_eq!(stream.pending_len(), 0);
}

#[test]
fn spec_r25_stream_buffers_incomplete_values_and_empty_push_changes_nothing() {
    let mut stream = StreamDecoder::new();
    assert_eq!(stream.push(&[0x80]), Ok(vec![]));
    assert_eq!(stream.push(&[]), Ok(vec![]));
    assert_eq!(stream.position(), 1);
    assert_eq!(stream.pending_len(), 1);
    assert_eq!(
        stream.finish(),
        Err(Error::Truncated { offset: 1, start: 0 })
    );
}

#[test]
fn spec_r26_stream_overflow_offsets_are_absolute_across_chunks() {
    let mut stream = StreamDecoder::new();
    for _ in 0..8 {
        assert_eq!(stream.push(&[0x80]), Ok(vec![]));
    }
    assert_eq!(stream.push(&[0x80, 2]), Err(Error::Overflow { offset: 9, start: 0 }));
}

#[test]
fn spec_r27_stream_push_overflow_rolls_back_entire_chunk() {
    let mut stream = StreamDecoder::new();
    assert_eq!(stream.push(&[7]), Ok(vec![7]));
    let before_position = stream.position();
    let before_pending = stream.pending_len();

    let mut invalid = vec![8];
    invalid.extend_from_slice(&[0x80; 9]);
    invalid.push(2);
    assert_eq!(
        stream.push(&invalid),
        Err(Error::Overflow {
            offset: before_position + 10,
            start: before_position + 1,
        })
    );
    assert_eq!(stream.position(), before_position);
    assert_eq!(stream.pending_len(), before_pending);
    assert_eq!(stream.push(&[9]), Ok(vec![9]));
}

#[test]
fn spec_r28_finish_is_repeatable_nonmutating_and_does_not_seal() {
    let mut stream = StreamDecoder::new();
    assert_eq!(stream.push(&[0x80]), Ok(vec![]));
    let expected = Err(Error::Truncated { offset: 1, start: 0 });
    assert_eq!(stream.finish(), expected);
    assert_eq!(stream.finish(), expected);
    assert_eq!(stream.position(), 1);
    assert_eq!(stream.pending_len(), 1);
    assert_eq!(stream.push(&[1]), Ok(vec![128]));
    assert_eq!(stream.finish(), Ok(()));
}

#[test]
fn spec_r29_reset_discards_pending_state_and_allows_reuse() {
    let mut stream = StreamDecoder::new();
    assert_eq!(stream.push(&[0x80, 0x80]), Ok(vec![]));
    stream.reset();
    assert_eq!(stream.position(), 0);
    assert_eq!(stream.pending_len(), 0);
    assert_eq!(stream.push(&[4]), Ok(vec![4]));
}
