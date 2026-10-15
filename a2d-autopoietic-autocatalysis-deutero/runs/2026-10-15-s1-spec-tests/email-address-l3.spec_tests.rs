#[test]
fn spec_r1_rust_2024_compatible_test_function() {
    assert_eq!(
        parse_email("a@b.co"),
        Ok(Address {
            local: "a".into(),
            domain: "b.co".into(),
            quoted: false,
        })
    );
}

#[test]
fn spec_r2_public_api_types_and_fields() {
    let address = Address {
        local: "A".into(),
        domain: "b.co".into(),
        quoted: false,
    };
    assert_eq!(address.local, "A");
    assert_eq!(address.domain, "b.co");
    assert!(!address.quoted);
    assert_eq!(EmailError::MissingAt, EmailError::MissingAt);
}

#[test]
fn spec_r3_offsets_are_original_byte_positions() {
    assert_eq!(
        parse_email("(x) bad!@example.com"),
        Err(EmailError::LocalChar { at: 7 })
    );
    assert_eq!(
        parse_email("a@b..c"),
        Err(EmailError::EmptyLabel { at: 4 })
    );
}

#[test]
fn spec_r4_empty_then_non_ascii_precedence() {
    assert_eq!(parse_email(""), Err(EmailError::EmptyInput));
    assert_eq!(
        parse_email("(é"),
        Err(EmailError::NonAscii { at: 1 })
    );
    assert_eq!(
        parse_email("\"é\""),
        Err(EmailError::NonAscii { at: 1 })
    );
}

#[test]
fn spec_r5_cleanup_errors_precede_boundaries_and_address_syntax() {
    assert_eq!(
        parse_email("a b example.com)"),
        Err(EmailError::UnexpectedClose { at: 14 })
    );
    assert_eq!(
        parse_email("(bad\n)"),
        Err(EmailError::BadFold { at: 4 })
    );
    assert_eq!(parse_email(" \t "), Err(EmailError::EmptyInput));
}

#[test]
fn spec_r6_quoted_prefix_closing_search_precedes_separator_checks() {
    assert_eq!(
        parse_email("\"abc@x"),
        Err(EmailError::UnclosedQuote { at: 0 })
    );
    assert_eq!(
        parse_email("\"abc\\"),
        Err(EmailError::UnclosedQuote { at: 0 })
    );
}

#[test]
fn spec_r7_separator_count_and_empty_local_precedence() {
    assert_eq!(parse_email("abc"), Err(EmailError::MissingAt));
    assert_eq!(
        parse_email("a@b@c"),
        Err(EmailError::MultipleAt { at: 3 })
    );
    assert_eq!(
        parse_email("@bad_domain"),
        Err(EmailError::EmptyLocal { at: 0 })
    );
    assert_eq!(
        parse_email("\"a@b\"@x@y"),
        Err(EmailError::MultipleAt { at: 7 })
    );
}

#[test]
fn spec_r8_unquoted_local_length_precedes_content_and_allowed_symbols_work() {
    let long_bad = format!("{}!@example.com", "a".repeat(64));
    assert_eq!(
        parse_email(&long_bad),
        Err(EmailError::LocalTooLong { at: 64 })
    );
    assert_eq!(
        parse_email("a!#$%&'*+-/=?^_`{|}~9@example.com"),
        Ok(Address {
            local: "a!#$%&'*+-/=?^_`{|}~9".into(),
            domain: "example.com".into(),
            quoted: false,
        })
    );
    assert_eq!(
        parse_email("a b@example.com"),
        Err(EmailError::LocalChar { at: 1 })
    );
}

#[test]
fn spec_r9_dot_atom_rules_and_spelling_preservation() {
    assert_eq!(
        parse_email(".a@b.co"),
        Err(EmailError::LocalDot { at: 0 })
    );
    assert_eq!(
        parse_email("a..b@b.co"),
        Err(EmailError::LocalDot { at: 2 })
    );
    assert_eq!(
        parse_email("a.@b.co"),
        Err(EmailError::LocalDot { at: 1 })
    );
    assert_eq!(
        parse_email("A.B@Example.COM"),
        Ok(Address {
            local: "A.B".into(),
            domain: "example.com".into(),
            quoted: false,
        })
    );
}

#[test]
fn spec_r10_quoted_local_closing_quote_must_be_final_and_content_printable() {
    assert_eq!(
        parse_email("\"a\"x@example.com"),
        Err(EmailError::TrailingLocal { at: 3 })
    );
    assert_eq!(
        parse_email("\"a\tb\"@example.com"),
        Err(EmailError::QuotedChar { at: 2 })
    );
    assert_eq!(
        parse_email("\"a @.()\"@example.com"),
        Ok(Address {
            local: "a @.()".into(),
            domain: "example.com".into(),
            quoted: true,
        })
    );
}

#[test]
fn spec_r11_quoted_escapes_are_limited_and_bad_escape_points_at_slash() {
    assert_eq!(
        parse_email("\"a\\q\"@example.com"),
        Err(EmailError::BadEscape { at: 2 })
    );
    assert_eq!(
        parse_email("\"\\\"\\\\\\@\"@example.com"),
        Ok(Address {
            local: "\"\\@".into(),
            domain: "example.com".into(),
            quoted: true,
        })
    );
}

#[test]
fn spec_r12_quoted_decoded_length_and_empty_local() {
    assert_eq!(
        parse_email("\"\"@example.com"),
        Err(EmailError::EmptyLocal { at: 1 })
    );
    let long_plain = format!("\"{}\"@example.com", "a".repeat(65));
    assert_eq!(
        parse_email(&long_plain),
        Err(EmailError::LocalTooLong { at: 65 })
    );
    let long_escaped = format!("\"{}\\@\"@example.com", "a".repeat(64));
    assert_eq!(
        parse_email(&long_escaped),
        Err(EmailError::LocalTooLong { at: 65 })
    );
}

#[test]
fn spec_r13_one_root_dot_is_removed_and_empty_domain_position_is_mapped() {
    assert_eq!(
        parse_email("a@"),
        Err(EmailError::EmptyDomain { at: 2 })
    );
    assert_eq!(
        parse_email("a@."),
        Err(EmailError::EmptyDomain { at: 2 })
    );
    assert_eq!(
        parse_email("a@x.y."),
        Ok(Address {
            local: "a".into(),
            domain: "x.y".into(),
            quoted: false,
        })
    );
}

#[test]
fn spec_r14_domain_length_excludes_root_dot_and_precedes_label_checks() {
    let domain = format!("{}_", "a".repeat(253));
    let input = format!("a@{domain}");
    assert_eq!(
        parse_email(&input),
        Err(EmailError::DomainTooLong { at: 255 })
    );
    let domain_with_root = format!("{}. ", "a".repeat(252));
    let input = format!("a@{}", domain_with_root.trim_end());
    assert_eq!(
        parse_email(&input),
        Err(EmailError::DomainNeedsDot { at: 2 })
    );
}

#[test]
fn spec_r15_empty_domain_labels_report_their_original_start() {
    assert_eq!(
        parse_email("a@.x"),
        Err(EmailError::EmptyLabel { at: 2 })
    );
    assert_eq!(
        parse_email("a@x..y"),
        Err(EmailError::EmptyLabel { at: 4 })
    );
    assert_eq!(
        parse_email("a@x."),
        Err(EmailError::DomainNeedsDot { at: 2 })
    );
    assert_eq!(
        parse_email("a@x.."),
        Err(EmailError::EmptyLabel { at: 4 })
    );
}

#[test]
fn spec_r16_label_length_precedes_label_characters() {
    let domain = format!("{}_", "a".repeat(63));
    let input = format!("a@{domain}.co");
    assert_eq!(
        parse_email(&input),
        Err(EmailError::LabelTooLong { at: 65 })
    );
}

#[test]
fn spec_r17_label_hyphens_precede_character_checks() {
    assert_eq!(
        parse_email("a@-b_.co"),
        Err(EmailError::LabelHyphen { at: 2 })
    );
    assert_eq!(
        parse_email("a@b-_.co"),
        Err(EmailError::LabelHyphen { at: 4 })
    );
}

#[test]
fn spec_r18_domain_character_rules_reject_literals_and_underscores() {
    assert_eq!(
        parse_email("a@exa_mple.co"),
        Err(EmailError::DomainChar { at: 5 })
    );
    assert_eq!(
        parse_email("a@[127.0.0.1]"),
        Err(EmailError::DomainChar { at: 2 })
    );
    assert_eq!(
        parse_email("a@exa mple.co"),
        Err(EmailError::CfwsBoundary { at: 5 })
    );
}

#[test]
fn spec_r19_two_labels_are_required_before_numeric_tld_check() {
    assert_eq!(
        parse_email("a@localhost"),
        Err(EmailError::DomainNeedsDot { at: 2 })
    );
    assert_eq!(
        parse_email("a@123"),
        Err(EmailError::DomainNeedsDot { at: 2 })
    );
}

#[test]
fn spec_r20_numeric_final_label_is_rejected_after_label_count() {
    assert_eq!(
        parse_email("a@example.123"),
        Err(EmailError::NumericTld { at: 10 })
    );
    assert_eq!(
        parse_email("a@example.a1"),
        Ok(Address {
            local: "a".into(),
            domain: "example.a1".into(),
            quoted: false,
        })
    );
    assert_eq!(
        parse_email("a@Example.CoM"),
        Ok(Address {
            local: "a".into(),
            domain: "example.com".into(),
            quoted: false,
        })
    );
}

#[test]
fn spec_r21_domain_errors_follow_local_validation_but_cleanup_scans_whole_input() {
    assert_eq!(
        parse_email("bad!@x..y"),
        Err(EmailError::LocalChar { at: 3 })
    );
    assert_eq!(
        parse_email("bad!@(oops"),
        Err(EmailError::UnclosedComment { at: 5 })
    );
    assert_eq!(
        parse_email("a@x..y"),
        Err(EmailError::EmptyLabel { at: 4 })
    );
}

#[test]
fn spec_r22_cfws_is_removed_without_inserting_spaces() {
    assert_eq!(
        parse_email("(left)a(comment)@\r\n \tExample.COM(end)"),
        Ok(Address {
            local: "a".into(),
            domain: "example.com".into(),
            quoted: false,
        })
    );
    assert_eq!(
        parse_email("a \r\n\t@b.co"),
        Ok(Address {
            local: "a".into(),
            domain: "b.co".into(),
            quoted: false,
        })
    );
}

#[test]
fn spec_r23_folds_require_crlf_and_following_horizontal_whitespace() {
    assert_eq!(
        parse_email("a@\n b.co"),
        Err(EmailError::BadFold { at: 2 })
    );
    assert_eq!(
        parse_email("a@\r\nb.co"),
        Err(EmailError::BadFold { at: 2 })
    );
    assert_eq!(
        parse_email("a@(x\r\ny)@b.co"),
        Err(EmailError::BadFold { at: 4 })
    );
    assert_eq!(
        parse_email("a@\r\n \tb.co"),
        Ok(Address {
            local: "a".into(),
            domain: "b.co".into(),
            quoted: false,
        })
    );
}

#[test]
fn spec_r24_comments_nest_to_three_and_close_deterministically() {
    assert_eq!(
        parse_email("a@b.co((((x))))"),
        Err(EmailError::CommentDepth { at: 8 })
    );
    assert_eq!(
        parse_email("a@(x"),
        Err(EmailError::UnclosedComment { at: 3 })
    );
    assert_eq!(
        parse_email("a@b.co)"),
        Err(EmailError::UnexpectedClose { at: 7 })
    );
    assert_eq!(
        parse_email("a@(x)@b.co"),
        Ok(Address {
            local: "a".into(),
            domain: "b.co".into(),
            quoted: false,
        })
    );
}

#[test]
fn spec_r25_comment_escapes_and_characters_are_checked_before_unclosed_status() {
    assert_eq!(
        parse_email("a@(x\\"),
        Err(EmailError::CommentEscape { at: 5 })
    );
    assert_eq!(
        parse_email("a@(x\\\u{0001})@b.co"),
        Err(EmailError::CommentEscape { at: 5 })
    );
    assert_eq!(
        parse_email("a@(x\u{0001})@b.co"),
        Err(EmailError::CommentChar { at: 5 })
    );
    assert_eq!(
        parse_email("a@(\\(x\\))@b.co"),
        Ok(Address {
            local: "a".into(),
            domain: "b.co".into(),
            quoted: false,
        })
    );
}

#[test]
fn spec_r26_only_first_retained_quote_protects_cleanup() {
    assert_eq!(
        parse_email(" (note) \"a (b)@c\" @example.com"),
        Ok(Address {
            local: "a (b)@c".into(),
            domain: "example.com".into(),
            quoted: true,
        })
    );
    assert_eq!(
        parse_email("a\"(x)\"@b.co"),
        Err(EmailError::TrailingLocal { at: 1 })
    );
    assert_eq!(
        parse_email("\"a (b)@c"),
        Err(EmailError::UnclosedQuote { at: 0 })
    );
}

#[test]
fn spec_r27_cfws_runs_are_allowed_only_at_edges_or_adjacent_to_at() {
    assert_eq!(
        parse_email("first last@mail.example"),
        Err(EmailError::CfwsBoundary { at: 5 })
    );
    assert_eq!(
        parse_email("(x)a(b)@c.co"),
        Err(EmailError::CfwsBoundary { at: 5 })
    );
    assert_eq!(
        parse_email("a@b(c).co"),
        Err(EmailError::CfwsBoundary { at: 3 })
    );
    assert_eq!(
        parse_email(" (x) a @ (y)b.co "),
        Ok(Address {
            local: "a".into(),
            domain: "b.co".into(),
            quoted: false,
        })
    );
}

#[test]
fn spec_r28_cfws_around_separator_is_allowed_but_between_quote_and_local_byte_is_not() {
    assert_eq!(
        parse_email(" (edge) a (near) @ (near) b.co (tail) "),
        Ok(Address {
            local: "a".into(),
            domain: "b.co".into(),
            quoted: false,
        })
    );
    assert_eq!(
        parse_email("\"a\" (x) z@b.co"),
        Err(EmailError::CfwsBoundary { at: 5 })
    );
    assert_eq!(
        parse_email(" (only) \t "),
        Err(EmailError::EmptyInput)
    );
    assert_eq!(
        parse_email(" (bad\n) "),
        Err(EmailError::BadFold { at: 5 })
    );
}

#[test]
fn spec_r29_long_inputs_are_handled_without_panics_or_unbounded_work() {
    let comment = format!("({})", "x".repeat(20_000));
    let input = format!("a@b.co{comment}");
    assert_eq!(
        parse_email(&input),
        Ok(Address {
            local: "a".into(),
            domain: "b.co".into(),
            quoted: false,
        })
    );
}
