#[test]
fn spec_r1_r2_api_and_plain_words() {
    let options = WrapOptions {
        width: 10,
        first_indent: 0,
        rest_indent: 0,
        hyphen_cost: 0,
        justify: false,
    };
    assert_eq!(
        wrap("blue green", &options),
        Ok(vec!["blue green".to_string()])
    );
}

#[test]
fn spec_r3_width_validation() {
    let base = WrapOptions {
        width: 0,
        first_indent: 0,
        rest_indent: 0,
        hyphen_cost: 0,
        justify: false,
    };
    assert_eq!(wrap("~", &base), Err(WrapError::WidthZero));

    let too_large = WrapOptions {
        width: 257,
        ..base.clone()
    };
    assert_eq!(
        wrap("text", &too_large),
        Err(WrapError::WidthTooLarge { width: 257 })
    );

    let maximum = WrapOptions {
        width: 256,
        ..base
    };
    assert_eq!(wrap("x", &maximum), Ok(vec!["x".to_string()]));
}

#[test]
fn spec_r4_indent_validation_and_content_room() {
    let options = WrapOptions {
        width: 4,
        first_indent: 4,
        rest_indent: 4,
        hyphen_cost: 0,
        justify: false,
    };
    assert_eq!(
        wrap("x", &options),
        Err(WrapError::FirstIndentTooWide { indent: 4 })
    );

    let options = WrapOptions {
        first_indent: 0,
        ..options
    };
    assert_eq!(
        wrap("x", &options),
        Err(WrapError::RestIndentTooWide { indent: 4 })
    );

    let options = WrapOptions {
        width: 4,
        first_indent: 3,
        rest_indent: 2,
        hyphen_cost: 0,
        justify: false,
    };
    assert_eq!(
        wrap("abc d", &options),
        Ok(vec![" abc".to_string(), "  d".to_string()])
    );
}

#[test]
fn spec_r5_hyphen_cost_validation() {
    let options = WrapOptions {
        width: 4,
        first_indent: 0,
        rest_indent: 0,
        hyphen_cost: 1_000_001,
        justify: false,
    };
    assert_eq!(
        wrap("text", &options),
        Err(WrapError::HyphenCostTooLarge { cost: 1_000_001 })
    );

    let options = WrapOptions {
        hyphen_cost: 1_000_000,
        ..options
    };
    assert_eq!(wrap("x", &options), Ok(vec!["x".to_string()]));
}

#[test]
fn spec_r6_control_validation_uses_original_utf8_byte_index() {
    let options = WrapOptions {
        width: 8,
        first_indent: 0,
        rest_indent: 0,
        hyphen_cost: 0,
        justify: false,
    };
    assert_eq!(
        wrap("é\u{0007}", &options),
        Err(WrapError::InvalidControl { index: 2 })
    );
    assert_eq!(
        wrap("a\tb\r\nc", &options),
        Ok(vec!["a b c".to_string()])
    );
}

#[test]
fn spec_r7_marker_validation_and_control_precedence() {
    let options = WrapOptions {
        width: 8,
        first_indent: 0,
        rest_indent: 0,
        hyphen_cost: 0,
        justify: false,
    };
    assert_eq!(
        wrap("ab~~cd", &options),
        Err(WrapError::InvalidMarker { index: 2 })
    );
    assert_eq!(
        wrap("é~\u{0007}", &options),
        Err(WrapError::InvalidControl { index: 3 })
    );
    assert_eq!(
        wrap("ab~cd", &options),
        Ok(vec!["ab-cd".to_string()])
    );
}

#[test]
fn spec_r8_line_ending_normalization_and_original_error_offsets() {
    let options = WrapOptions {
        width: 10,
        first_indent: 0,
        rest_indent: 0,
        hyphen_cost: 0,
        justify: false,
    };
    assert_eq!(
        wrap("a\r\n\nb", &options),
        Ok(vec!["a".to_string(), "".to_string(), "b".to_string()])
    );
    assert_eq!(
        wrap("é\r\n~", &options),
        Err(WrapError::InvalidMarker { index: 4 })
    );
}

#[test]
fn spec_r9_exact_separators_and_scalar_width() {
    let options = WrapOptions {
        width: 4,
        first_indent: 0,
        rest_indent: 0,
        hyphen_cost: 0,
        justify: false,
    };
    assert_eq!(
        wrap("a\u{00A0}b", &options),
        Ok(vec!["a b".to_string()])
    );
    assert_eq!(
        wrap("a\u{2003}b\u{00AD}c", &options),
        Ok(vec!["a\u{2003}b\u{00AD}".to_string(), "c".to_string()])
    );
    assert_eq!(
        wrap("e\u{0301}x", &options),
        Ok(vec!["e\u{0301}x".to_string()])
    );
}

#[test]
fn spec_r10_lf_runs_form_boundaries_only_when_adjacent() {
    let options = WrapOptions {
        width: 10,
        first_indent: 0,
        rest_indent: 0,
        hyphen_cost: 0,
        justify: false,
    };
    assert_eq!(
        wrap("\n\nx\n\n\n", &options),
        Ok(vec!["".to_string(), "x".to_string(), "".to_string(), "".to_string()])
    );
    assert_eq!(
        wrap("a\n \nb", &options),
        Ok(vec!["a b".to_string()])
    );
}

#[test]
fn spec_r11_segments_preserve_boundary_empties_and_reset_paragraphs() {
    let options = WrapOptions {
        width: 5,
        first_indent: 1,
        rest_indent: 2,
        hyphen_cost: 0,
        justify: false,
    };
    assert_eq!(
        wrap("\n\n   \n\nx\n\n", &options),
        Ok(vec![
            "".to_string(),
            "".to_string(),
            " x".to_string(),
            "".to_string()
        ])
    );
    let empty_options = WrapOptions {
        first_indent: 0,
        rest_indent: 0,
        ..options
    };
    assert_eq!(wrap("", &empty_options), Ok(Vec::new()));
    assert_eq!(wrap(" \t\u{00A0}", &empty_options), Ok(Vec::new()));
}

#[test]
fn spec_r12_collapse_separators_and_remove_markers() {
    let options = WrapOptions {
        width: 20,
        first_indent: 0,
        rest_indent: 0,
        hyphen_cost: 0,
        justify: false,
    };
    assert_eq!(
        wrap(" \tfoo~bar\u{00A0}\nbaz  ", &options),
        Ok(vec!["foobar baz".to_string()])
    );
}

#[test]
fn spec_r13_first_and_rest_indentation() {
    let options = WrapOptions {
        width: 5,
        first_indent: 1,
        rest_indent: 2,
        hyphen_cost: 0,
        justify: false,
    };
    assert_eq!(
        wrap("abc de", &options),
        Ok(vec![" abc".to_string(), "  de".to_string()])
    );
}

#[test]
fn spec_r14_fit_suffix_with_reserved_interword_space() {
    let options = WrapOptions {
        width: 5,
        first_indent: 0,
        rest_indent: 0,
        hyphen_cost: 0,
        justify: false,
    };
    assert_eq!(
        wrap("abc d", &options),
        Ok(vec!["abc".to_string(), "d".to_string()])
    );
    let options = WrapOptions {
        width: 6,
        ..options
    };
    assert_eq!(
        wrap("ab~cd", &options),
        Ok(vec!["abcd".to_string()])
    );
}

#[test]
fn spec_r15_usable_opportunity_must_be_after_offset_and_fit() {
    let options = WrapOptions {
        width: 4,
        first_indent: 0,
        rest_indent: 0,
        hyphen_cost: 0,
        justify: false,
    };
    assert_eq!(
        wrap("ab~cdef", &options),
        Ok(vec!["ab-".to_string(), "cdef".to_string()])
    );
    let options = WrapOptions {
        width: 3,
        ..options
    };
    assert_eq!(
        wrap("a~bcdef", &options),
        Ok(vec!["a-".to_string(), "bc".to_string(), "def".to_string()])
    );
}

#[test]
fn spec_r16_rightmost_opportunity_and_strict_cost_comparison() {
    let options = WrapOptions {
        width: 6,
        first_indent: 0,
        rest_indent: 0,
        hyphen_cost: 0,
        justify: false,
    };
    assert_eq!(
        wrap("a bc~de~fg", &options),
        Ok(vec!["a bc-".to_string(), "de-".to_string(), "fg".to_string()])
    );

    let equality = WrapOptions {
        width: 6,
        first_indent: 0,
        rest_indent: 0,
        hyphen_cost: 1,
        justify: false,
    };
    assert_eq!(
        wrap("ab cd~efgh", &equality),
        Ok(vec!["ab".to_string(), "cd-".to_string(), "efgh".to_string()])
    );

    let lower_cost = WrapOptions {
        hyphen_cost: 0,
        ..equality
    };
    assert_eq!(
        wrap("ab cd~efgh", &lower_cost),
        Ok(vec!["ab cd-".to_string(), "efgh".to_string()])
    );
}

#[test]
fn spec_r17_empty_line_uses_rightmost_opportunity_regardless_of_cost() {
    let options = WrapOptions {
        width: 6,
        first_indent: 0,
        rest_indent: 0,
        hyphen_cost: 1_000_000,
        justify: false,
    };
    assert_eq!(
        wrap("abcdef~ghij~kl", &options),
        Ok(vec!["abcde-".to_string(), "fghij-".to_string(), "kl".to_string()])
    );
}

#[test]
fn spec_r18_nonempty_line_flushes_before_retrying_suffix() {
    let options = WrapOptions {
        width: 5,
        first_indent: 0,
        rest_indent: 0,
        hyphen_cost: 1_000_000,
        justify: false,
    };
    assert_eq!(
        wrap("abc de~fgh", &options),
        Ok(vec!["abc".to_string(), "de-".to_string(), "fgh".to_string()])
    );
}

#[test]
fn spec_r19_empty_line_raw_split_discards_passed_markers() {
    let options = WrapOptions {
        width: 3,
        first_indent: 0,
        rest_indent: 0,
        hyphen_cost: 1_000_000,
        justify: false,
    };
    assert_eq!(
        wrap("abcdef~gh", &options),
        Ok(vec!["abc".to_string(), "def".to_string(), "gh".to_string()])
    );
}

#[test]
fn spec_r20_no_extra_lines_and_literal_hyphens_are_ordinary() {
    let options = WrapOptions {
        width: 3,
        first_indent: 0,
        rest_indent: 0,
        hyphen_cost: 0,
        justify: false,
    };
    assert_eq!(
        wrap("abc def", &options),
        Ok(vec!["abc".to_string(), "def".to_string()])
    );
    assert_eq!(
        wrap("ab-cd", &options),
        Ok(vec!["ab-".to_string(), "cd".to_string()])
    );
}

#[test]
fn spec_r21_justify_only_nonfinal_mult_piece_lines() {
    let options = WrapOptions {
        width: 7,
        first_indent: 0,
        rest_indent: 0,
        hyphen_cost: 0,
        justify: true,
    };
    assert_eq!(
        wrap("a bb ccc d", &options),
        Ok(vec!["a    bb".to_string(), "ccc d".to_string()])
    );
    assert_eq!(
        wrap("abc de fg", &WrapOptions { width: 6, ..options.clone() }),
        Ok(vec!["abc de".to_string(), "fg".to_string()])
    );
}

#[test]
fn spec_r22_justify_remainder_rotation_uses_paragraph_line_index() {
    let options = WrapOptions {
        width: 7,
        first_indent: 0,
        rest_indent: 0,
        hyphen_cost: 0,
        justify: true,
    };
    assert_eq!(
        wrap("a b c d e f", &options),
        Ok(vec![
            "a  b c d".to_string(),
            "e f".to_string()
        ])
    );
}

#[test]
fn spec_r23_hyphen_prefix_gap_and_continued_word_indent() {
    let options = WrapOptions {
        width: 7,
        first_indent: 0,
        rest_indent: 1,
        hyphen_cost: 0,
        justify: true,
    };
    assert_eq!(
        wrap("a bc~defgh", &options),
        Ok(vec!["a bc-".to_string(), " defgh".to_string()])
    );
}

#[test]
fn spec_r24_outputs_have_no_line_breaks_or_trailing_spaces() {
    let options = WrapOptions {
        width: 4,
        first_indent: 1,
        rest_indent: 1,
        hyphen_cost: 0,
        justify: false,
    };
    assert_eq!(
        wrap("a b\n\nc", &options),
        Ok(vec![" a".to_string(), " b".to_string(), "".to_string(), " c".to_string()])
    );
}
