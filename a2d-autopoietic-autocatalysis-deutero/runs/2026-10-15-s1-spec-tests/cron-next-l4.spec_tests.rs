#[test]
fn spec_r1_source_compiles_without_inner_attributes() {
    let cron = parse_cron("* * * * *").unwrap();
    assert_eq!(cron.allowed(Field::Minute), (0u8..=59).collect::<Vec<_>>());
}

#[test]
fn spec_r2_public_api_shapes_and_parse_error() {
    let field = Field::DayOfWeek;
    assert_eq!(field, Field::DayOfWeek);
    assert_eq!(
        parse_cron("* * * * 7"),
        Err(ParseError::OutOfRange {
            field: Field::DayOfWeek,
            offset: 0
        })
    );
}

#[test]
fn spec_r3_field_domains_and_sunday_seven() {
    let cron = parse_cron("0,59 0,23 1,31 1,12 0,6").unwrap();
    assert_eq!(cron.allowed(Field::Minute), vec![0, 59]);
    assert_eq!(cron.allowed(Field::Hour), vec![0, 23]);
    assert_eq!(cron.allowed(Field::DayOfMonth), vec![1, 31]);
    assert_eq!(cron.allowed(Field::Month), vec![1, 12]);
    assert_eq!(cron.allowed(Field::DayOfWeek), vec![0, 6]);
    assert_eq!(
        parse_cron("* * * * 7"),
        Err(ParseError::OutOfRange {
            field: Field::DayOfWeek,
            offset: 0
        })
    );
}

#[test]
fn spec_r4_literal_space_split_and_field_count_precedence() {
    assert_eq!(parse_cron(""), Err(ParseError::FieldCount { found: 1 }));
    assert_eq!(
        parse_cron("*\t* * * *"),
        Err(ParseError::FieldCount { found: 4 })
    );
    assert_eq!(
        parse_cron("* * * * * "),
        Err(ParseError::FieldCount { found: 6 })
    );
    assert_eq!(
        parse_cron("* * * * *\n"),
        Err(ParseError::FieldCount { found: 5 })
    );
}

#[test]
fn spec_r5_field_order_and_field_local_byte_offsets() {
    assert_eq!(
        parse_cron("x 99 0 0 0"),
        Err(ParseError::InvalidSyntax {
            field: Field::Minute,
            offset: 0
        })
    );
    assert_eq!(
        parse_cron("* * 01 * *"),
        Err(ParseError::LeadingZero {
            field: Field::DayOfMonth,
            offset: 0
        })
    );
}

#[test]
fn spec_r6_lexical_scan_precedes_item_parsing() {
    assert_eq!(
        parse_cron("1,,x! * * * *"),
        Err(ParseError::InvalidCharacter {
            field: Field::Minute,
            offset: 4
        })
    );
    assert_eq!(
        parse_cron("é * * * *"),
        Err(ParseError::InvalidCharacter {
            field: Field::Minute,
            offset: 0
        })
    );
}

#[test]
fn spec_r7_empty_items_and_trailing_item_offsets() {
    assert_eq!(
        parse_cron(",1 * * * *"),
        Err(ParseError::EmptyItem {
            field: Field::Minute,
            offset: 0
        })
    );
    assert_eq!(
        parse_cron("1,,2 * * * *"),
        Err(ParseError::EmptyItem {
            field: Field::Minute,
            offset: 2
        })
    );
    assert_eq!(
        parse_cron("1, * * * *"),
        Err(ParseError::EmptyItem {
            field: Field::Minute,
            offset: 2
        })
    );
    assert_eq!(
        parse_cron(" * * * *"),
        Err(ParseError::EmptyItem {
            field: Field::Minute,
            offset: 0
        })
    );
}

#[test]
fn spec_r8_item_structure_rejected_before_numbers() {
    for input in ["1//0 * * * *", "1/ * * * *", "1/2/3 * * * *", "1-2-3 * * * *", "*-1 * * * *", "1-* * * * *"] {
        assert_eq!(
            parse_cron(input),
            Err(ParseError::InvalidSyntax {
                field: Field::Minute,
                offset: 0
            }),
            "{input}"
        );
    }
}

#[test]
fn spec_r9_leading_zero_precedes_other_number_errors() {
    assert_eq!(
        parse_cron("00 * * * *"),
        Err(ParseError::LeadingZero {
            field: Field::Minute,
            offset: 0
        })
    );
    assert_eq!(
        parse_cron("1-01 * * * *"),
        Err(ParseError::LeadingZero {
            field: Field::Minute,
            offset: 2
        })
    );
}

#[test]
fn spec_r10_u32_overflow_and_domain_errors() {
    assert_eq!(
        parse_cron("4294967296 * * * *"),
        Err(ParseError::NumberOverflow {
            field: Field::Minute,
            offset: 0
        })
    );
    assert_eq!(
        parse_cron("60 * * * *"),
        Err(ParseError::OutOfRange {
            field: Field::Minute,
            offset: 0
        })
    );
    assert_eq!(
        parse_cron("1-60 * * * *"),
        Err(ParseError::OutOfRange {
            field: Field::Minute,
            offset: 2
        })
    );
}

#[test]
fn spec_r11_range_endpoint_validation_and_reversal_precede_step() {
    assert_eq!(
        parse_cron("5-2/00 * * * *"),
        Err(ParseError::ReversedRange {
            field: Field::Minute,
            offset: 2
        })
    );
    assert_eq!(
        parse_cron("5-4294967296/0 * * * *"),
        Err(ParseError::NumberOverflow {
            field: Field::Minute,
            offset: 2
        })
    );
    assert_eq!(
        parse_cron("60-2/0 * * * *"),
        Err(ParseError::OutOfRange {
            field: Field::Minute,
            offset: 0
        })
    );
}

#[test]
fn spec_r12_step_validation_and_domain_width_bounds() {
    assert_eq!(
        parse_cron("*/0 * * * *"),
        Err(ParseError::BadStep {
            field: Field::Minute,
            offset: 2
        })
    );
    assert_eq!(
        parse_cron("*/61 * * * *"),
        Err(ParseError::BadStep {
            field: Field::Minute,
            offset: 2
        })
    );
    assert_eq!(
        parse_cron("*/01 * * * *"),
        Err(ParseError::LeadingZero {
            field: Field::Minute,
            offset: 2
        })
    );
    assert_eq!(
        parse_cron("*/4294967296 * * * *"),
        Err(ParseError::NumberOverflow {
            field: Field::Minute,
            offset: 2
        })
    );
    assert_eq!(parse_cron("0-2/60 * * * *").unwrap().allowed(Field::Minute), vec![0]);
}

#[test]
fn spec_r13_unstepped_base_selection() {
    assert_eq!(parse_cron("* * * * *").unwrap().allowed(Field::Minute), (0u8..=59).collect::<Vec<_>>());
    assert_eq!(parse_cron("7 * * * *").unwrap().allowed(Field::Minute), vec![7]);
    assert_eq!(parse_cron("3-5 * * * *").unwrap().allowed(Field::Minute), vec![3, 4, 5]);
}

#[test]
fn spec_r14_stepped_ranges_and_singletons_extend_as_specified() {
    assert_eq!(parse_cron("*/20 * * * *").unwrap().allowed(Field::Minute), vec![0, 20, 40]);
    assert_eq!(parse_cron("10-50/20 * * * *").unwrap().allowed(Field::Minute), vec![10, 30, 50]);
    assert_eq!(parse_cron("10/20 * * * *").unwrap().allowed(Field::Minute), vec![10, 30, 50]);
}

#[test]
fn spec_r15_steps_union_and_include_start() {
    assert_eq!(parse_cron("1-8/3,2-8/3 * * * *").unwrap().allowed(Field::Minute), vec![1, 2, 4, 5, 7, 8]);
}

#[test]
fn spec_r16_allowed_is_sorted_unique_and_order_independent() {
    let first = parse_cron("5,1-3,3 * * * *").unwrap();
    let second = parse_cron("3,3,1-3,5 * * * *").unwrap();
    assert_eq!(first.allowed(Field::Minute), vec![1, 2, 3, 5]);
    assert_eq!(first.allowed(Field::Minute), second.allowed(Field::Minute));
}

#[test]
fn spec_r17_impossible_calendar_schedule_is_accepted() {
    let cron = parse_cron("0 0 30 2 *").unwrap();
    assert_eq!(cron.allowed(Field::DayOfMonth), vec![30]);
    assert_eq!(cron.allowed(Field::Month), vec![2]);
    assert_eq!(cron.next_fire(CivilDateTime { year: 2000, month: 1, day: 1, hour: 0, minute: 0 }).unwrap(), None);
}

#[test]
fn spec_r18_equality_includes_day_unrestricted_flags() {
    assert_ne!(parse_cron("0 0 * * *").unwrap(), parse_cron("0 0 */1 * *").unwrap());
    assert_ne!(parse_cron("0 0 * * *").unwrap(), parse_cron("0 0 * * */1").unwrap());
    assert_eq!(parse_cron("0 0 1-3 * *").unwrap(), parse_cron("0 0 1,2,3 * *").unwrap());
}

#[test]
fn spec_r19_date_api_and_date_error_shape() {
    let date = CivilDateTime { year: 1999, month: 12, day: 31, hour: 23, minute: 59 };
    assert_eq!(
        parse_cron("* * * * *").unwrap().matches(date),
        Err(DateError::InvalidDateTime { component: DatePart::Year })
    );
    assert_eq!(DatePart::Minute, DatePart::Minute);
}

#[test]
fn spec_r20_supported_datetime_bounds_and_components() {
    let cron = parse_cron("* * * * *").unwrap();
    assert!(cron.matches(CivilDateTime { year: 2000, month: 1, day: 1, hour: 0, minute: 0 }).unwrap());
    assert!(cron.matches(CivilDateTime { year: 2399, month: 12, day: 31, hour: 23, minute: 59 }).unwrap());
    for (date, component) in [
        (CivilDateTime { year: 2400, month: 1, day: 1, hour: 0, minute: 0 }, DatePart::Year),
        (CivilDateTime { year: 2000, month: 0, day: 1, hour: 0, minute: 0 }, DatePart::Month),
        (CivilDateTime { year: 2000, month: 13, day: 1, hour: 0, minute: 0 }, DatePart::Month),
        (CivilDateTime { year: 2000, month: 1, day: 0, hour: 0, minute: 0 }, DatePart::Day),
        (CivilDateTime { year: 2000, month: 1, day: 1, hour: 24, minute: 0 }, DatePart::Hour),
        (CivilDateTime { year: 2000, month: 1, day: 1, hour: 0, minute: 60 }, DatePart::Minute),
    ] {
        assert_eq!(
            cron.matches(date),
            Err(DateError::InvalidDateTime { component })
        );
    }
}

#[test]
fn spec_r21_datetime_validation_precedence_applies_to_search() {
    let cron = parse_cron("0 0 30 2 *").unwrap();
    let invalid = CivilDateTime { year: 1999, month: 13, day: 0, hour: 24, minute: 60 };
    assert_eq!(
        cron.matches(invalid),
        Err(DateError::InvalidDateTime { component: DatePart::Year })
    );
    assert_eq!(
        cron.next_fire(invalid),
        Err(DateError::InvalidDateTime { component: DatePart::Year })
    );
    assert_eq!(
        cron.previous_fire(invalid),
        Err(DateError::InvalidDateTime { component: DatePart::Year })
    );
}

#[test]
fn spec_r22_gregorian_leap_year_month_lengths() {
    let cron = parse_cron("0 0 29 2 *").unwrap();
    assert!(cron.matches(CivilDateTime { year: 2000, month: 2, day: 29, hour: 0, minute: 0 }).unwrap());
    assert!(cron.matches(CivilDateTime { year: 2004, month: 2, day: 29, hour: 0, minute: 0 }).unwrap());
    assert!(!cron.matches(CivilDateTime { year: 2100, month: 2, day: 28, hour: 0, minute: 0 }).unwrap());
    assert_eq!(
        cron.matches(CivilDateTime { year: 2100, month: 2, day: 29, hour: 0, minute: 0 }),
        Err(DateError::InvalidDateTime { component: DatePart::Day })
    );
    assert_eq!(
        cron.matches(CivilDateTime { year: 2001, month: 4, day: 31, hour: 0, minute: 0 }),
        Err(DateError::InvalidDateTime { component: DatePart::Day })
    );
}

#[test]
fn spec_r23_weekday_epoch_and_leap_day_advancement() {
    let cron = parse_cron("0 0 * * SAT").unwrap();
    assert!(cron.matches(CivilDateTime { year: 2000, month: 1, day: 1, hour: 0, minute: 0 }).unwrap());
    assert!(!cron.matches(CivilDateTime { year: 2000, month: 1, day: 2, hour: 0, minute: 0 }).unwrap());
    assert_eq!(
        cron.next_fire(CivilDateTime { year: 2000, month: 2, day: 28, hour: 23, minute: 59 }).unwrap(),
        Some(CivilDateTime { year: 2000, month: 3, day: 4, hour: 0, minute: 0 })
    );
}

#[test]
fn spec_r24_matches_requires_time_month_and_day_rules() {
    let cron = parse_cron("5 6 10 * MON").unwrap();
    assert!(cron.matches(CivilDateTime { year: 2024, month: 6, day: 10, hour: 6, minute: 5 }).unwrap());
    assert!(!cron.matches(CivilDateTime { year: 2024, month: 6, day: 10, hour: 6, minute: 4 }).unwrap());
    assert!(!cron.matches(CivilDateTime { year: 2024, month: 7, day: 10, hour: 6, minute: 5 }).unwrap());
}

#[test]
fn spec_r25_next_fire_is_strict_and_bounded() {
    let cron = parse_cron("0 0 * * *").unwrap();
    assert_eq!(
        cron.next_fire(CivilDateTime { year: 2020, month: 6, day: 9, hour: 0, minute: 0 }).unwrap(),
        Some(CivilDateTime { year: 2020, month: 6, day: 10, hour: 0, minute: 0 })
    );
    assert_eq!(
        cron.next_fire(CivilDateTime { year: 2399, month: 12, day: 31, hour: 23, minute: 59 }).unwrap(),
        None
    );
}

#[test]
fn spec_r26_search_skips_short_month_dates_without_clamping() {
    let cron = parse_cron("0 0 31 * *").unwrap();
    assert_eq!(
        cron.next_fire(CivilDateTime { year: 2021, month: 4, day: 30, hour: 0, minute: 0 }).unwrap(),
        Some(CivilDateTime { year: 2021, month: 5, day: 31, hour: 0, minute: 0 })
    );
    assert_eq!(
        parse_cron("0 0 31 2 *").unwrap().next_fire(CivilDateTime { year: 2000, month: 1, day: 1, hour: 0, minute: 0 }).unwrap(),
        None
    );
}

#[test]
fn spec_r27_queries_are_read_only_and_instances_independent() {
    let a = parse_cron("0 0 * * *").unwrap();
    let b = parse_cron("15 5 * * *").unwrap();
    let date = CivilDateTime { year: 2020, month: 6, day: 9, hour: 1, minute: 0 };
    let expected = a.next_fire(date);
    assert_eq!(a.next_fire(date), expected);
    assert_eq!(a.matches(date), a.matches(date));
    let _ = b.next_fire(date);
    let _ = parse_cron("1 2 3 4 5").unwrap();
    assert_eq!(a.next_fire(date), expected);
}

#[test]
fn spec_r28_ascii_letters_lexically_legal_but_non_ascii_is_not() {
    assert_eq!(
        parse_cron("JAN * * * *"),
        Err(ParseError::InvalidSyntax { field: Field::Minute, offset: 0 })
    );
    assert_eq!(
        parse_cron("é * * * *"),
        Err(ParseError::InvalidCharacter { field: Field::Minute, offset: 0 })
    );
}

#[test]
fn spec_r29_names_case_insensitive_and_mixed_range_endpoints() {
    assert_eq!(parse_cron("* * * jan DEC *").unwrap().allowed(Field::Month), vec![1, 12]);
    assert_eq!(parse_cron("* * * 11-MAR *").unwrap().allowed(Field::Month), vec![3, 11, 12]);
    assert_eq!(parse_cron("* * * * sun-SAT").unwrap().allowed(Field::DayOfWeek), vec![0, 1, 2, 3, 4, 5, 6]);
}

#[test]
fn spec_r30_names_require_exact_allowed_base_and_are_forbidden_elsewhere() {
    for input in ["JAN * * * *", "* * * JANUARY *", "* * * J1N *", "* * * * SUN1", "* * * * SUN/1"] {
        let field = if input.starts_with("JAN") {
            Field::Minute
        } else if input.contains("SUN") {
            Field::DayOfWeek
        } else {
            Field::Month
        };
        let offset = if input.contains("SUN/1") { 4 } else { 0 };
        assert_eq!(
            parse_cron(input),
            Err(ParseError::InvalidSyntax { field, offset }),
            "{input}"
        );
    }
}

#[test]
fn spec_r31_named_endpoints_use_numeric_order_and_steps_remain_numeric() {
    assert_eq!(parse_cron("* * * NOV-FEB/2 *").unwrap().allowed(Field::Month), vec![11, 12]);
    assert_eq!(
        parse_cron("* * * JAN-FEB/X *"),
        Err(ParseError::InvalidSyntax { field: Field::Month, offset: 0 })
    );
}

#[test]
fn spec_r32_day_unrestricted_requires_exact_asterisk() {
    let both = parse_cron("0 0 * * *").unwrap();
    assert!(both.matches(CivilDateTime { year: 2024, month: 6, day: 10, hour: 0, minute: 0 }).unwrap());
    let dom_restricted = parse_cron("0 0 */1 * *").unwrap();
    assert!(dom_restricted.matches(CivilDateTime { year: 2024, month: 6, day: 10, hour: 0, minute: 0 }).unwrap());
    assert!(!dom_restricted.matches(CivilDateTime { year: 2024, month: 6, day: 11, hour: 0, minute: 0 }).unwrap());
    let dow_restricted = parse_cron("0 0 * * */1").unwrap();
    assert!(!dow_restricted.matches(CivilDateTime { year: 2024, month: 6, day: 10, hour: 0, minute: 0 }).unwrap());
}

#[test]
fn spec_r33_both_restricted_use_or_except_last_month_day() {
    let cron = parse_cron("0 0 10 * MON").unwrap();
    assert!(cron.matches(CivilDateTime { year: 2024, month: 6, day: 10, hour: 0, minute: 0 }).unwrap());
    assert!(cron.matches(CivilDateTime { year: 2024, month: 6, day: 17, hour: 0, minute: 0 }).unwrap());
    assert!(cron.matches(CivilDateTime { year: 2024, month: 6, day: 30, hour: 0, minute: 0 }).unwrap());
    assert!(!cron.matches(CivilDateTime { year: 2024, month: 6, day: 24, hour: 0, minute: 0 }).unwrap());
    let full_dom_restricted = parse_cron("0 0 1-31 * MON").unwrap();
    assert!(!full_dom_restricted.matches(CivilDateTime { year: 2024, month: 6, day: 30, hour: 0, minute: 0 }).unwrap());
}

#[test]
fn spec_r34_last_day_rule_does_not_override_time_or_month() {
    let cron = parse_cron("5 6 10 6 MON").unwrap();
    assert!(!cron.matches(CivilDateTime { year: 2024, month: 6, day: 30, hour: 6, minute: 5 }).unwrap());
    assert!(!cron.matches(CivilDateTime { year: 2024, month: 7, day: 29, hour: 6, minute: 5 }).unwrap());
    assert!(!cron.matches(CivilDateTime { year: 2024, month: 6, day: 30, hour: 6, minute: 4 }).unwrap());
}

#[test]
fn spec_r35_window_api_shapes_and_canonical_output() {
    let cron = parse_cron("0 0 * * *").unwrap();
    let window = cron.fires_between(
        CivilDateTime { year: 2020, month: 1, day: 1, hour: 0, minute: 0 },
        CivilDateTime { year: 2020, month: 1, day: 2, hour: 0, minute: 0 },
        1
    ).unwrap();
    assert_eq!(window, FireWindow {
        fires: vec![CivilDateTime { year: 2020, month: 1, day: 1, hour: 0, minute: 0 }],
        truncated: false
    });
    assert_eq!(cron.canonical(), "0 0 * * *");
}

#[test]
fn spec_r36_special_dispatch_is_exact_and_at_elsewhere_is_normal_input() {
    for special in ["@hourly", "@daily", "@weekly", "@monthly", "@yearly"] {
        assert!(parse_cron(special).is_ok());
    }
    for invalid in ["@Hourly", "@hourly ", "@unknown", "@"] {
        assert_eq!(parse_cron(invalid), Err(ParseError::UnknownSpecial));
    }
    assert_eq!(
        parse_cron("0 0 @ * *"),
        Err(ParseError::InvalidSyntax { field: Field::DayOfMonth, offset: 0 })
    );
}

#[test]
fn spec_r37_specials_equal_their_expansions() {
    for (special, expansion) in [
        ("@hourly", "0 * * * *"),
        ("@daily", "0 0 * * *"),
        ("@weekly", "0 0 * * 0"),
        ("@monthly", "0 0 1 * *"),
        ("@yearly", "0 0 1 1 *"),
    ] {
        assert_eq!(parse_cron(special).unwrap(), parse_cron(expansion).unwrap());
    }
}

#[test]
fn spec_r38_canonical_sorts_numbers_and_removes_duplicate_spellings() {
    assert_eq!(parse_cron("5,1,5 2,0 * 12,1 *").unwrap().canonical(), "1,5 0,2 * 1,12 *");
    assert_eq!(parse_cron("1-3 0 * * *").unwrap().canonical(), "1,2,3 0 * * *");
}

#[test]
fn spec_r39_canonical_full_domain_and_restricted_day_format() {
    assert_eq!(parse_cron("* * * * *").unwrap().canonical(), "* * * * *");
    assert_eq!(parse_cron("*/1 * */1 * */1").unwrap().canonical(), "* * */1 * */1");
    assert_eq!(parse_cron("* * 1-31 * *").unwrap().canonical(), "* * */1 * *");
}

#[test]
fn spec_r40_canonical_round_trip_preserves_behavior_and_flags() {
    for source in ["1,5 2,4 1-31 * MON", "* * */1 * *", "0 0 29 2 *"] {
        let cron = parse_cron(source).unwrap();
        let canonical = cron.canonical();
        let round_trip = parse_cron(&canonical).unwrap();
        assert_eq!(round_trip, cron);
        assert_eq!(round_trip.canonical(), canonical);
        for date in [
            CivilDateTime { year: 2000, month: 2, day: 29, hour: 0, minute: 0 },
            CivilDateTime { year: 2024, month: 6, day: 10, hour: 2, minute: 1 },
        ] {
            assert_eq!(round_trip.matches(date), cron.matches(date));
            assert_eq!(round_trip.next_fire(date), cron.next_fire(date));
        }
    }
    assert_eq!(parse_cron("@hourly").unwrap().canonical(), parse_cron("0 * * * *").unwrap().canonical());
}

#[test]
fn spec_r41_previous_fire_is_strict_and_uses_same_calendar_rules() {
    let cron = parse_cron("0 0 * * *").unwrap();
    assert_eq!(
        cron.previous_fire(CivilDateTime { year: 2020, month: 6, day: 9, hour: 0, minute: 0 }).unwrap(),
        Some(CivilDateTime { year: 2020, month: 6, day: 8, hour: 0, minute: 0 })
    );
    assert_eq!(
        cron.previous_fire(CivilDateTime { year: 2000, month: 1, day: 1, hour: 0, minute: 0 }).unwrap(),
        None
    );
    assert_eq!(
        parse_cron("0 0 30 2 *").unwrap().previous_fire(CivilDateTime { year: 2020, month: 1, day: 1, hour: 0, minute: 0 }).unwrap(),
        None
    );
}

#[test]
fn spec_r42_window_validation_order_precedes_order_and_cap() {
    let cron = parse_cron("* * * * *").unwrap();
    let invalid_start = CivilDateTime { year: 1999, month: 1, day: 1, hour: 0, minute: 0 };
    let invalid_end = CivilDateTime { year: 2400, month: 1, day: 1, hour: 0, minute: 0 };
    assert_eq!(
        cron.fires_between(invalid_start, invalid_end, 0),
        Err(WindowError::InvalidStart { component: DatePart::Year })
    );
    assert_eq!(
        cron.fires_between(CivilDateTime { year: 2020, month: 1, day: 2, hour: 0, minute: 0 }, invalid_end, 0),
        Err(WindowError::InvalidEnd { component: DatePart::Year })
    );
    assert_eq!(
        cron.fires_between(
            CivilDateTime { year: 2020, month: 1, day: 2, hour: 0, minute: 0 },
            CivilDateTime { year: 2020, month: 1, day: 1, hour: 0, minute: 0 },
            0
        ),
        Err(WindowError::ReversedWindow)
    );
    assert_eq!(
        cron.fires_between(
            CivilDateTime { year: 2020, month: 1, day: 1, hour: 0, minute: 0 },
            CivilDateTime { year: 2020, month: 1, day: 2, hour: 0, minute: 0 },
            0
        ),
        Err(WindowError::ZeroCap)
    );
}

#[test]
fn spec_r43_window_is_half_open_and_equal_endpoints_are_empty() {
    let cron = parse_cron("* * * * *").unwrap();
    let start = CivilDateTime { year: 2020, month: 1, day: 1, hour: 0, minute: 0 };
    let end = CivilDateTime { year: 2020, month: 1, day: 1, hour: 0, minute: 2 };
    assert_eq!(
        cron.fires_between(start, end, 10).unwrap(),
        FireWindow {
            fires: vec![
                start,
                CivilDateTime { year: 2020, month: 1, day: 1, hour: 0, minute: 1 }
            ],
            truncated: false
        }
    );
    assert_eq!(
        cron.fires_between(start, start, 1).unwrap(),
        FireWindow { fires: vec![], truncated: false }
    );
}

#[test]
fn spec_r44_window_truncated_only_for_an_extra_fire() {
    let cron = parse_cron("0 0 * * *").unwrap();
    let start = CivilDateTime { year: 2020, month: 1, day: 1, hour: 0, minute: 0 };
    let third = CivilDateTime { year: 2020, month: 1, day: 4, hour: 0, minute: 0 };
    assert_eq!(
        cron.fires_between(start, third, 2).unwrap(),
        FireWindow {
            fires: vec![
                start,
                CivilDateTime { year: 2020, month: 1, day: 2, hour: 0, minute: 0 }
            ],
            truncated: false
        }
    );
    assert_eq!(
        cron.fires_between(start, third, 1).unwrap().truncated,
        true
    );
    assert_eq!(
        cron.fires_between(start, start, 1).unwrap().truncated,
        false
    );
    assert_eq!(
        parse_cron("0 0 30 2 *").unwrap().fires_between(start, third, 1).unwrap().truncated,
        false
    );
}

#[test]
fn spec_r45_window_and_previous_calls_are_read_only_and_handle_full_range() {
    let cron = parse_cron("0 0 31 2 *").unwrap();
    let start = CivilDateTime { year: 2000, month: 1, day: 1, hour: 0, minute: 0 };
    let end = CivilDateTime { year: 2399, month: 12, day: 31, hour: 23, minute: 59 };
    let expected = FireWindow { fires: vec![], truncated: false };
    assert_eq!(cron.fires_between(start, end, usize::MAX).unwrap(), expected);
    assert_eq!(cron.fires_between(start, end, usize::MAX).unwrap(), expected);
    assert_eq!(cron.previous_fire(end).unwrap(), None);
    assert_eq!(cron.previous_fire(end).unwrap(), None);
}
