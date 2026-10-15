#[test]
fn spec_r1_single_source_compatible() {
    let q = parse_cron("* * * * *").unwrap();
    assert_eq!(q.allowed(Field::Minute), (0..=59).collect::<Vec<u8>>());
}

#[test]
fn spec_r2_public_api_shape() {
    let _: Result<Cron, ParseError> = parse_cron("0 0 * * *");
    let _: Vec<u8> = Cron::allowed(&parse_cron("0 0 * * *").unwrap(), Field::Minute);
    assert_eq!(Field::DayOfWeek, Field::DayOfWeek);
}

#[test]
fn spec_r3_field_domains_and_sunday_boundary() {
    let q = parse_cron("59 23 31 12 6").unwrap();
    assert_eq!(q.allowed(Field::Minute), vec![59]);
    assert_eq!(q.allowed(Field::Hour), vec![23]);
    assert_eq!(q.allowed(Field::DayOfMonth), vec![31]);
    assert_eq!(q.allowed(Field::Month), vec![12]);
    assert_eq!(q.allowed(Field::DayOfWeek), vec![6]);

    assert_eq!(
        parse_cron("0 0 * * 7"),
        Err(ParseError::OutOfRange { field: Field::DayOfWeek, offset: 0 })
    );
}

#[test]
fn spec_r4_literal_space_field_count() {
    assert_eq!(parse_cron(""), Err(ParseError::FieldCount { found: 1 }));
    assert_eq!(parse_cron("0 0\t* * *"), Err(ParseError::FieldCount { found: 4 }));
    assert_eq!(parse_cron("0 0 * * * "), Err(ParseError::FieldCount { found: 6 }));
    assert_eq!(parse_cron("0 0\n* * *"), Err(ParseError::FieldCount { found: 4 }));
}

#[test]
fn spec_r5_field_order_and_field_relative_offsets() {
    assert_eq!(
        parse_cron("0 24 0 * *"),
        Err(ParseError::OutOfRange { field: Field::Hour, offset: 0 })
    );
    assert_eq!(
        parse_cron("0 0 1,32 * *"),
        Err(ParseError::OutOfRange { field: Field::DayOfMonth, offset: 2 })
    );
}

#[test]
fn spec_r6_scan_entire_field_before_parsing_items() {
    assert_eq!(
        parse_cron("1/0,x 0 * * *"),
        Err(ParseError::InvalidCharacter { field: Field::Minute, offset: 4 })
    );
    assert_eq!(
        parse_cron("é 0 * * *"),
        Err(ParseError::InvalidCharacter { field: Field::Minute, offset: 0 })
    );
}

#[test]
fn spec_r7_empty_items_and_trailing_offset() {
    assert_eq!(
        parse_cron(",1 0 * * *"),
        Err(ParseError::EmptyItem { field: Field::Minute, offset: 0 })
    );
    assert_eq!(
        parse_cron("1,,2 0 * * *"),
        Err(ParseError::EmptyItem { field: Field::Minute, offset: 2 })
    );
    assert_eq!(
        parse_cron("1, 0 * * *"),
        Err(ParseError::EmptyItem { field: Field::Minute, offset: 2 })
    );
    assert_eq!(
        parse_cron(" 0 * * * *"),
        Err(ParseError::EmptyItem { field: Field::Minute, offset: 0 })
    );
}

#[test]
fn spec_r8_item_structure_precedes_numeric_validation() {
    for field in ["1/", "1/2/3", "1-2-3", "*-2", "1-*", "*/2-3", "-1"] {
        let input = format!("{field} 0 * * *");
        assert_eq!(
            parse_cron(&input),
            Err(ParseError::InvalidSyntax { field: Field::Minute, offset: 0 }),
            "{field}"
        );
    }
}

#[test]
fn spec_r9_leading_zero_precedes_other_number_checks() {
    assert_eq!(
        parse_cron("00 0 * * *"),
        Err(ParseError::LeadingZero { field: Field::Minute, offset: 0 })
    );
    assert_eq!(
        parse_cron("1-00 0 * * *"),
        Err(ParseError::LeadingZero { field: Field::Minute, offset: 2 })
    );
    assert_eq!(
        parse_cron("1/00 0 * * *"),
        Err(ParseError::LeadingZero { field: Field::Minute, offset: 2 })
    );
}

#[test]
fn spec_r10_u32_overflow_and_domain_bounds() {
    assert_eq!(
        parse_cron("4294967296 0 * * *"),
        Err(ParseError::NumberOverflow { field: Field::Minute, offset: 0 })
    );
    assert_eq!(
        parse_cron("1-4294967296 0 * * *"),
        Err(ParseError::NumberOverflow { field: Field::Minute, offset: 2 })
    );
    assert_eq!(
        parse_cron("0 0 32 * *"),
        Err(ParseError::OutOfRange { field: Field::DayOfMonth, offset: 0 })
    );
}

#[test]
fn spec_r11_numeric_base_validation_order() {
    assert_eq!(
        parse_cron("2-1/0 0 * * *"),
        Err(ParseError::ReversedRange { field: Field::Minute, offset: 2 })
    );
    assert_eq!(
        parse_cron("60-0/0 0 * * *"),
        Err(ParseError::OutOfRange { field: Field::Minute, offset: 0 })
    );
    assert_eq!(
        parse_cron("1-60/0 0 * * *"),
        Err(ParseError::OutOfRange { field: Field::Minute, offset: 2 })
    );
}

#[test]
fn spec_r12_step_bounds_and_selected_range_length() {
    assert_eq!(
        parse_cron("*/0 0 * * *"),
        Err(ParseError::BadStep { field: Field::Minute, offset: 2 })
    );
    assert_eq!(
        parse_cron("*/61 0 * * *"),
        Err(ParseError::BadStep { field: Field::Minute, offset: 2 })
    );
    let q = parse_cron("10-12/60 0 * * *").unwrap();
    assert_eq!(q.allowed(Field::Minute), vec![10]);
}

#[test]
fn spec_r13_unstepped_selection() {
    let q = parse_cron("* 0 1-3 * *").unwrap();
    assert_eq!(q.allowed(Field::Minute), (0..=59).collect::<Vec<u8>>());
    assert_eq!(q.allowed(Field::DayOfMonth), vec![1, 2, 3]);

    let q = parse_cron("8 0 * * *").unwrap();
    assert_eq!(q.allowed(Field::Minute), vec![8]);
}

#[test]
fn spec_r14_stepped_single_extends_to_domain_maximum() {
    let q = parse_cron("55/2 0 * * *").unwrap();
    assert_eq!(q.allowed(Field::Minute), vec![55, 57, 59]);
    let q = parse_cron("10-15/3 0 * * *").unwrap();
    assert_eq!(q.allowed(Field::Minute), vec![10, 13]);
}

#[test]
fn spec_r15_step_includes_start_and_unions_items() {
    let q = parse_cron("5-14/4,1,9-12/2 0 * * *").unwrap();
    assert_eq!(q.allowed(Field::Minute), vec![1, 5, 9, 11, 13]);
}

#[test]
fn spec_r16_allowed_is_unique_sorted_and_order_independent() {
    let a = parse_cron("5,1-3,3 0 * * *").unwrap();
    let b = parse_cron("3,1-3,5 0 * * *").unwrap();
    assert_eq!(a.allowed(Field::Minute), vec![1, 2, 3, 5]);
    assert_eq!(a.allowed(Field::Minute), b.allowed(Field::Minute));
}

#[test]
fn spec_r17_impossible_calendar_schedule_is_accepted() {
    let q = parse_cron("0 0 30 2 *").unwrap();
    assert_eq!(q.allowed(Field::DayOfMonth), vec![30]);
    assert_eq!(q.allowed(Field::Month), vec![2]);
}

#[test]
fn spec_r18_equality_tracks_day_star_provenance_only() {
    let plain = parse_cron("0 0 * * *").unwrap();
    let stepped = parse_cron("0 0 */1 * *").unwrap();
    assert_ne!(plain, stepped);

    let syntax_a = parse_cron("0 0 1,2 * *").unwrap();
    let syntax_b = parse_cron("0 0 1-2 * *").unwrap();
    assert_eq!(syntax_a, syntax_b);

    let dom_plain = parse_cron("0 0 * * *").unwrap();
    let dom_stepped = parse_cron("0 0 */1 * *").unwrap();
    assert_ne!(dom_plain, dom_stepped);

    let dow_plain = parse_cron("0 0 * * *").unwrap();
    let dow_stepped = parse_cron("0 0 * * */1").unwrap();
    assert_ne!(dow_plain, dow_stepped);
}
