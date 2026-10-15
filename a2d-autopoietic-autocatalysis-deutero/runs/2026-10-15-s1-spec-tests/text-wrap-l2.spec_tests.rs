#[test]
fn spec_r1_single_file_std_compatible_api() {
    let options = WrapOptions {
        width: 4,
        first_indent: 0,
        rest_indent: 0,
    };
    assert_eq!(wrap("rust", &options), Ok(vec!["rust".to_string()]));
}

#[test]
fn spec_r2_public_api_types_and_variants() {
    let options = WrapOptions {
        width: 0,
        first_indent: 0,
        rest_indent: 0,
    };
    assert_eq!(wrap("", &options), Err(WrapError::WidthZero));
    assert_eq!(
        WrapError::WidthTooLarge { width: 257 },
        WrapError::WidthTooLarge { width: 257 }
    );
}

#[test]
fn spec_r3_width_validation_precedes_input_and_indents() {
    assert_eq!(
        wrap(
            "\u{0001}",
            &WrapOptions {
                width: 0,
                first_indent: 9,
                rest_indent: 9,
            }
        ),
        Err(WrapError::WidthZero)
    );
    assert_eq!(
        wrap(
            "\u{0001}",
            &WrapOptions {
                width: 257,
                first_indent: 0,
                rest_indent: 0,
            }
        ),
        Err(WrapError::WidthTooLarge { width: 257 })
    );
    assert_eq!(
        wrap(
            "abcd",
            &WrapOptions {
                width: 4,
                first_indent: 1,
                rest_indent: 0,
            }
        ),
        Ok(vec![" abcd".to_string()])
    );
}

#[test]
fn spec_r4_indents_must_leave_content_columns() {
    assert_eq!(
        wrap(
            "x",
            &WrapOptions {
                width: 3,
                first_indent: 0,
                rest_indent: 3,
            }
        ),
        Err(WrapError::RestIndentTooWide { indent: 3 })
    );
    assert_eq!(
        wrap(
            "x",
            &WrapOptions {
                width: 3,
                first_indent: 2,
                rest_indent: 3,
            }
        ),
        Err(WrapError::FirstIndentTooWide { indent: 2 })
    );
    assert_eq!(
        wrap(
            "x",
            &WrapOptions {
                width: 1,
                first_indent: 0,
                rest_indent: 0,
            }
        ),
        Ok(vec!["x".to_string()])
    );
}

#[test]
fn spec_r5_control_errors_use_original_utf8_byte_indexes() {
    assert_eq!(
        wrap(
            "é\u{0001}\u{0002}",
            &WrapOptions {
                width: 10,
                first_indent: 0,
                rest_indent: 0,
            }
        ),
        Err(WrapError::InvalidControl { index: 2 })
    );
    assert_eq!(
        wrap(
            "a\tb\r\nc",
            &WrapOptions {
                width: 10,
                first_indent: 0,
                rest_indent: 0,
            }
        ),
        Ok(vec!["a b c".to_string()])
    );
}

#[test]
fn spec_r6_crlf_and_cr_normalize_as_linefeeds() {
    assert_eq!(
        wrap(
            "a\r\n\r\nb\rc",
            &WrapOptions {
                width: 10,
                first_indent: 0,
                rest_indent: 0,
            }
        ),
        Ok(vec!["a".to_string(), "".to_string(), "b c".to_string()])
    );
}

#[test]
fn spec_r7_only_the_specified_scalars_are_separators() {
    assert_eq!(
        wrap(
            "a\u{00a0}b\u{2003}c\u{00ad}d",
            &WrapOptions {
                width: 20,
                first_indent: 0,
                rest_indent: 0,
            }
        ),
        Ok(vec!["a b\u{2003}c\u{00ad}d".to_string()])
    );
    assert_eq!(
        wrap(
            "e\u{0301}🙂",
            &WrapOptions {
                width: 2,
                first_indent: 0,
                rest_indent: 0,
            }
        ),
        Ok(vec!["e\u{0301}".to_string(), "🙂".to_string()])
    );
}

#[test]
fn spec_r8_adjacent_lf_runs_contribute_run_length_minus_one() {
    assert_eq!(
        wrap(
            "\n\nx\n\n\n",
            &WrapOptions {
                width: 4,
                first_indent: 0,
                rest_indent: 0,
            }
        ),
        Ok(vec![
            "".to_string(),
            "x".to_string(),
            "".to_string(),
            "".to_string(),
            "".to_string()
        ])
    );
    assert_eq!(
        wrap(
            "a\n \nb",
            &WrapOptions {
                width: 10,
                first_indent: 0,
                rest_indent: 0,
            }
        ),
        Ok(vec!["a b".to_string()])
    );
}

#[test]
fn spec_r9_empty_segments_keep_boundary_strings_and_empty_input_is_empty() {
    let options = WrapOptions {
        width: 5,
        first_indent: 2,
        rest_indent: 1,
    };
    assert_eq!(wrap("", &options), Ok(Vec::new()));
    assert_eq!(
        wrap(" \n\n \n\nx\n\n", &options),
        Ok(vec![
            "".to_string(),
            "".to_string(),
            "  x".to_string(),
            "".to_string()
        ])
    );
}

#[test]
fn spec_r10_separator_runs_collapse_and_words_are_preserved() {
    assert_eq!(
        wrap(
            " \t\u{00a0}alpha\t \u{00a0}beta \n gamma \t ",
            &WrapOptions {
                width: 30,
                first_indent: 0,
                rest_indent: 0,
            }
        ),
        Ok(vec!["alpha beta gamma".to_string()])
    );
}

#[test]
fn spec_r11_each_paragraph_resets_indent_sequence() {
    assert_eq!(
        wrap(
            "a b c\n\nx y z",
            &WrapOptions {
                width: 4,
                first_indent: 1,
                rest_indent: 2,
            }
        ),
        Ok(vec![
            " a".to_string(),
            "  b".to_string(),
            "  c".to_string(),
            "".to_string(),
            " x".to_string(),
            "  y".to_string(),
            "  z".to_string()
        ])
    );
}

#[test]
fn spec_r12_short_words_fit_with_one_separator_space() {
    assert_eq!(
        wrap(
            "ab cd",
            &WrapOptions {
                width: 5,
                first_indent: 0,
                rest_indent: 0,
            }
        ),
        Ok(vec!["ab cd".to_string()])
    );
}

#[test]
fn spec_r13_retry_short_suffix_with_next_lines_capacity() {
    assert_eq!(
        wrap(
            "ab cde",
            &WrapOptions {
                width: 5,
                first_indent: 1,
                rest_indent: 0,
            }
        ),
        Ok(vec![" ab".to_string(), "cde".to_string()])
    );
}

#[test]
fn spec_r14_long_words_split_by_unicode_scalar_count() {
    assert_eq!(
        wrap(
            "猫犬鳥魚",
            &WrapOptions {
                width: 2,
                first_indent: 0,
                rest_indent: 0,
            }
        ),
        Ok(vec!["猫犬".to_string(), "鳥魚".to_string()])
    );
}

#[test]
fn spec_r15_nonempty_long_word_splits_only_with_two_available_columns() {
    assert_eq!(
        wrap(
            "a abcdef",
            &WrapOptions {
                width: 5,
                first_indent: 0,
                rest_indent: 0,
            }
        ),
        Ok(vec!["a".to_string(), "abcde".to_string(), "f".to_string()])
    );
    assert_eq!(
        wrap(
            "abcd xyz",
            &WrapOptions {
                width: 5,
                first_indent: 0,
                rest_indent: 0,
            }
        ),
        Ok(vec!["abcd".to_string(), "xyz".to_string()])
    );
}

#[test]
fn spec_r16_split_tail_can_share_next_word_and_exact_fit_has_no_extra_line() {
    assert_eq!(
        wrap(
            "abcdefgh ij",
            &WrapOptions {
                width: 5,
                first_indent: 0,
                rest_indent: 0,
            }
        ),
        Ok(vec![
            "abcde".to_string(),
            "fgh ij".to_string()
        ])
    );
    assert_eq!(
        wrap(
            "abcdefgh",
            &WrapOptions {
                width: 4,
                first_indent: 0,
                rest_indent: 0,
            }
        ),
        Ok(vec!["abcd".to_string(), "efgh".to_string()])
    );
}

#[test]
fn spec_r17_output_has_no_line_breaks_or_trailing_spaces() {
    let lines = wrap(
        "one two\n\nthree",
        &WrapOptions {
            width: 5,
            first_indent: 0,
            rest_indent: 0,
        },
    )
    .unwrap();
    assert_eq!(
        lines,
        vec!["one".to_string(), "two".to_string(), "".to_string(), "three".to_string()]
    );
    assert!(lines.iter().all(|line| !line.contains('\n') && !line.contains('\r')));
    assert!(lines.iter().all(|line| !line.ends_with(' ')));
}
