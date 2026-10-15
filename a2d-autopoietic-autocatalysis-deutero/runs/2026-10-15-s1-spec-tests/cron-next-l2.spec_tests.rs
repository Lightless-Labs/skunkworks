#[test]
fn spec_r1_rust_2024_compatible_api() {
    let q = parse_cron("* * * * *").unwrap();
    let cloned = q.clone();
    assert_eq!(q, cloned);
    assert_eq!(format!("{q:?}"), format!("{cloned:?}"));
}

#[test]
fn spec_r2_api_types_and_variants() {
    let field = Field::Minute;
    assert_eq!(field, Field::Minute);
    let error = ParseError::FieldCount { found: 1 };
    assert_eq!(error, ParseError::FieldCount { found: 1 });
    let date = CivilDateTime {
        year: 2000,
        month: 1,
        day: 1,
        hour: 0,
        minute: 0,
    };
    assert_eq!(date, date);
    let part = DatePart::Year;
    assert_eq!(part, DatePart::Year);
    let date_error = DateError::InvalidDateTime {
        component: DatePart::Year,
    };
    assert_eq!(
        date_error,
        DateError::InvalidDateTime {
            component: DatePart::Year
        }
    );
}

#[test]
fn spec_r3_field_domains_and_sunday_seven() {
    let q = parse_cron("0,59 0,23 1,31 1,12 0,6").unwrap();
    assert_eq!(q.allowed(Field::Minute), vec![0, 59]);
    assert_eq!(q.allowed(Field::Hour), vec![0, 23]);
    assert_eq!(q.allowed(Field::DayOfMonth), vec![1, 31]);
    assert_eq!(q.allowed(Field::Month), vec![1, 12]);
    assert_eq!(q.allowed(Field::DayOfWeek), vec![0, 6]);
    assert_eq!(
        parse_cron("* * * * 7"),
        Err(ParseError::OutOfRange {
            field: Field::DayOfWeek,
            offset: 0
        })
    );
}

#[test]
fn spec_r4_split_on_literal_ascii_spaces_first() {
    assert_eq!(
        parse_cron(""),
        Err(ParseError::FieldCount { found: 1 })
    );
    assert_eq!(
        parse_cron("*  * * * *"),
        Err(ParseError::FieldCount { found: 6 })
    );
    assert_eq!(
        parse_cron("*\t* * * *"),
        Err(ParseError::FieldCount { found: 4 })
    );
    assert_eq!(
        parse_cron("*\n* * * *"),
        Err(ParseError::FieldCount { found: 4 })
    );
}

#[test]
fn spec_r5_field_order_and_field_relative_offsets() {
    assert_eq!(
        parse_cron("* * 0 * 7"),
        Err(ParseError::OutOfRange {
            field: Field::DayOfMonth,
            offset: 0
        })
    );
    assert_eq!(
        parse_cron("* * * * 00"),
        Err(ParseError::LeadingZero {
            field: Field::DayOfWeek,
            offset: 0
        })
    );
}

#[test]
fn spec_r6_scan_entire_field_before_item_parsing() {
    assert_eq!(
        parse_cron("x,999999999999999999999999 * * * *"),
        Err(ParseError::InvalidCharacter {
            field: Field::Minute,
            offset: 1
        })
    );
    assert_eq!(
        parse_cron("* * * * é"),
        Err(ParseError::InvalidCharacter {
            field: Field::DayOfWeek,
            offset: 0
        })
    );
}

#[test]
fn spec_r7_empty_comma_items_and_offsets() {
    assert_eq!(
        parse_cron(",1 * * * *"),
        Err(ParseError::EmptyItem {
            field: Field::Minute,
            offset: 0
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
        parse_cron("1,,2 * * * *"),
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
fn spec_r8_item_structure_precedes_numeric_validation() {
    for item in ["1/", "1//2", "1-2-3", "*-2", "2-*", "/2"] {
        let input = format!("{item} * * * *");
        assert_eq!(
            parse_cron(&input),
            Err(ParseError::InvalidSyntax {
                field: Field::Minute,
                offset: 0
            }),
            "item: {item}"
        );
    }
    assert_eq!(
        parse_cron("999999999999999999999999-1-2 * * * *"),
        Err(ParseError::InvalidSyntax {
            field: Field::Minute,
            offset: 0
        })
    );
}

#[test]
fn spec_r9_leading_zero_precedes_overflow_and_bounds() {
    assert_eq!(
        parse_cron("00 * * * *"),
        Err(ParseError::LeadingZero {
            field: Field::Minute,
            offset: 0
        })
    );
    assert_eq!(
        parse_cron("01-999999999999999999999999 * * * *"),
        Err(ParseError::LeadingZero {
            field: Field::Minute,
            offset: 0
        })
    );
    assert_eq!(
        parse_cron("1/00 * * * *"),
        Err(ParseError::LeadingZero {
            field: Field::Minute,
            offset: 2
        })
    );
    assert_eq!(parse_cron("0 * * * *").unwrap().allowed(Field::Minute), vec![0]);
}

#[test]
fn spec_r10_u32_overflow_and_numeric_domain_errors() {
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
        parse_cron("* * 32 * *"),
        Err(ParseError::OutOfRange {
            field: Field::DayOfMonth,
            offset: 0
        })
    );
}

#[test]
fn spec_r11_range_endpoint_validation_and_reversal_precedence() {
    assert_eq!(
        parse_cron("5-4 * * * *"),
        Err(ParseError::ReversedRange {
            field: Field::Minute,
            offset: 2
        })
    );
    assert_eq!(
        parse_cron("5-60/0 * * * *"),
        Err(ParseError::OutOfRange {
            field: Field::Minute,
            offset: 2
        })
    );
    assert_eq!(
        parse_cron("5-4/0 * * * *"),
        Err(ParseError::ReversedRange {
            field: Field::Minute,
            offset: 2
        })
    );
    assert_eq!(
        parse_cron("4-5 * * * *"),
        Ok(parse_cron("4-5 * * * *").unwrap())
    );
}

#[test]
fn spec_r12_step_validation_width_and_range_length() {
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
        parse_cron("*/4294967296 * * * *"),
        Err(ParseError::NumberOverflow {
            field: Field::Minute,
            offset: 2
        })
    );
    assert_eq!(
        parse_cron("*/13 * * * *").unwrap().allowed(Field::Minute),
        vec![0, 13, 26, 39, 52]
    );
    assert_eq!(
        parse_cron("1-2/7 * * * *").unwrap().allowed(Field::Minute),
        vec![1]
    );
}

#[test]
fn spec_r13_unstepped_star_number_and_range() {
    let q = parse_cron("* 8 2-4 * *").unwrap();
    assert_eq!(q.allowed(Field::Minute), (0..=59).collect::<Vec<u8>>());
    assert_eq!(q.allowed(Field::Hour), vec![8]);
    assert_eq!(q.allowed(Field::DayOfMonth), vec![2, 3, 4]);
}

#[test]
fn spec_r14_stepped_single_number_extends_to_domain_maximum() {
    let q = parse_cron("57/2 22/2 30/2 11/2 5/2").unwrap();
    assert_eq!(q.allowed(Field::Minute), vec![57, 59]);
    assert_eq!(q.allowed(Field::Hour), vec![22]);
    assert_eq!(q.allowed(Field::DayOfMonth), vec![30]);
    assert_eq!(q.allowed(Field::Month), vec![11]);
    assert_eq!(q.allowed(Field::DayOfWeek), vec![5]);
    assert_eq!(
        parse_cron("4-10/3 * * * *").unwrap().allowed(Field::Minute),
        vec![4, 7, 10]
    );
}

#[test]
fn spec_r15_steps_include_start_and_union_items() {
    let q = parse_cron("1-9/4,5,1-9/4 * * * *").unwrap();
    assert_eq!(q.allowed(Field::Minute), vec![1, 5, 9]);
}

#[test]
fn spec_r16_allowed_is_unique_sorted_and_independent_of_item_order() {
    let a = parse_cron("9,1,5,1 * * * *").unwrap();
    let b = parse_cron("1,5,9 * * * *").unwrap();
    assert_eq!(a.allowed(Field::Minute), vec![1, 5, 9]);
    assert_eq!(a.allowed(Field::Minute), b.allowed(Field::Minute));
}

#[test]
fn spec_r17_accepts_calendar_unrealizable_schedules() {
    let q = parse_cron("0 0 30 2 *").unwrap();
    assert_eq!(q.allowed(Field::DayOfMonth), vec![30]);
    assert_eq!(q.allowed(Field::Month), vec![2]);
}

#[test]
fn spec_r18_equality_tracks_day_star_provenance_only() {
    let literal_star = parse_cron("0 0 * * *").unwrap();
    let stepped_star = parse_cron("0 0 */1 * *").unwrap();
    assert_ne!(literal_star, stepped_star);

    let equivalent_lists_a = parse_cron("1,2 0 1-3 * *").unwrap();
    let equivalent_lists_b = parse_cron("2,1 0 1,2,3 * *").unwrap();
    assert_eq!(equivalent_lists_a, equivalent_lists_b);

    let dom_star_a = parse_cron("0 0 * * *").unwrap();
    let dom_star_b = parse_cron("0 0 * * 0-6").unwrap();
    assert_ne!(dom_star_a, dom_star_b);
}

#[test]
fn spec_r19_matches_and_next_fire_public_methods() {
    let q = parse_cron("0 0 * * *").unwrap();
    let at = CivilDateTime {
        year: 2000,
        month: 1,
        day: 1,
        hour: 0,
        minute: 0,
    };
    assert_eq!(q.matches(at), Ok(true));
    assert_eq!(q.next_fire(at), Ok(Some(CivilDateTime {
        year: 2000,
        month: 1,
        day: 2,
        hour: 0,
        minute: 0,
    })));
}

#[test]
fn spec_r20_datetime_supported_component_boundaries() {
    let q = parse_cron("* * * * *").unwrap();
    for at in [
        CivilDateTime { year: 2000, month: 1, day: 1, hour: 0, minute: 0 },
        CivilDateTime { year: 2399, month: 12, day: 31, hour: 23, minute: 59 },
    ] {
        assert_eq!(q.matches(at), Ok(true));
    }
    assert_eq!(
        q.matches(CivilDateTime { year: 1999, month: 12, day: 31, hour: 23, minute: 59 }),
        Err(DateError::InvalidDateTime { component: DatePart::Year })
    );
    assert_eq!(
        q.matches(CivilDateTime { year: 2400, month: 1, day: 1, hour: 0, minute: 0 }),
        Err(DateError::InvalidDateTime { component: DatePart::Year })
    );
}

#[test]
fn spec_r21_datetime_validation_order_precedes_schedule_check() {
    let q = parse_cron("60 * * * *");
    assert!(q.is_err());
    let all = parse_cron("* * * * *").unwrap();
    assert_eq!(
        all.matches(CivilDateTime { year: 1999, month: 13, day: 0, hour: 24, minute: 60 }),
        Err(DateError::InvalidDateTime { component: DatePart::Year })
    );
    assert_eq!(
        all.matches(CivilDateTime { year: 2000, month: 13, day: 0, hour: 24, minute: 60 }),
        Err(DateError::InvalidDateTime { component: DatePart::Month })
    );
    assert_eq!(
        all.matches(CivilDateTime { year: 2000, month: 2, day: 30, hour: 24, minute: 60 }),
        Err(DateError::InvalidDateTime { component: DatePart::Day })
    );
    assert_eq!(
        all.matches(CivilDateTime { year: 2000, month: 2, day: 29, hour: 24, minute: 60 }),
        Err(DateError::InvalidDateTime { component: DatePart::Hour })
    );
    assert_eq!(
        all.matches(CivilDateTime { year: 2000, month: 2, day: 29, hour: 23, minute: 60 }),
        Err(DateError::InvalidDateTime { component: DatePart::Minute })
    );
}

#[test]
fn spec_r22_gregorian_leap_year_and_month_lengths() {
    let q = parse_cron("* * * * *").unwrap();
    assert_eq!(
        q.matches(CivilDateTime { year: 2000, month: 2, day: 29, hour: 0, minute: 0 }),
        Ok(true)
    );
    assert_eq!(
        q.matches(CivilDateTime { year: 2100, month: 2, day: 29, hour: 0, minute: 0 }),
        Err(DateError::InvalidDateTime { component: DatePart::Day })
    );
    assert_eq!(
        q.matches(CivilDateTime { year: 2400, month: 2, day: 29, hour: 0, minute: 0 }),
        Err(DateError::InvalidDateTime { component: DatePart::Year })
    );
    for (month, last_day) in [(4, 30), (6, 30), (9, 30), (11, 30), (1, 31), (3, 31), (5, 31), (7, 31), (8, 31), (10, 31), (12, 31)] {
        assert_eq!(
            q.matches(CivilDateTime { year: 2001, month, day: last_day, hour: 0, minute: 0 }),
            Ok(true)
        );
        assert_eq!(
            q.matches(CivilDateTime { year: 2001, month, day: last_day + 1, hour: 0, minute: 0 }),
            Err(DateError::InvalidDateTime { component: DatePart::Day })
        );
    }
}

#[test]
fn spec_r23_weekdays_advance_across_leap_day() {
    let saturday = parse_cron("* * * * 6").unwrap();
    let sunday = parse_cron("* * * * 0").unwrap();
    assert_eq!(
        saturday.matches(CivilDateTime { year: 2000, month: 1, day: 1, hour: 0, minute: 0 }),
        Ok(true)
    );
    assert_eq!(
        sunday.matches(CivilDateTime { year: 2000, month: 1, day: 2, hour: 0, minute: 0 }),
        Ok(true)
    );
    let feb29 = parse_cron("* * 29 2 *").unwrap();
    assert_eq!(
        feb29.matches(CivilDateTime { year: 2000, month: 2, day: 29, hour: 0, minute: 0 }),
        Ok(true)
    );
    assert_eq!(
        parse_cron("* * * * 3").unwrap().matches(
            CivilDateTime { year: 2000, month: 3, day: 1, hour: 0, minute: 0 }
        ),
        Ok(true)
    );
}

#[test]
fn spec_r24_all_selected_fields_are_conjunctive() {
    let q = parse_cron("30 9 * * 1").unwrap();
    assert_eq!(
        q.matches(CivilDateTime { year: 2024, month: 1, day: 1, hour: 9, minute: 30 }),
        Ok(true)
    );
    assert_eq!(
        q.matches(CivilDateTime { year: 2024, month: 1, day: 2, hour: 9, minute: 30 }),
        Ok(false)
    );
    assert_eq!(
        q.matches(CivilDateTime { year: 2024, month: 1, day: 1, hour: 9, minute: 31 }),
        Ok(false)
    );
}

#[test]
fn spec_r25_next_fire_is_strict_and_searches_through_maximum() {
    let q = parse_cron("0 0 * * *").unwrap();
    assert_eq!(
        q.next_fire(CivilDateTime { year: 2000, month: 1, day: 1, hour: 0, minute: 0 }),
        Ok(Some(CivilDateTime { year: 2000, month: 1, day: 2, hour: 0, minute: 0 }))
    );
    let every_minute = parse_cron("* * * * *").unwrap();
    assert_eq!(
        every_minute.next_fire(CivilDateTime { year: 2000, month: 1, day: 1, hour: 0, minute: 0 }),
        Ok(Some(CivilDateTime { year: 2000, month: 1, day: 1, hour: 0, minute: 1 }))
    );
    assert_eq!(
        every_minute.next_fire(CivilDateTime { year: 2399, month: 12, day: 31, hour: 23, minute: 58 }),
        Ok(Some(CivilDateTime { year: 2399, month: 12, day: 31, hour: 23, minute: 59 }))
    );
    assert_eq!(
        every_minute.next_fire(CivilDateTime { year: 2399, month: 12, day: 31, hour: 23, minute: 59 }),
        Ok(None)
    );
}

#[test]
fn spec_r26_calendar_search_skips_invalid_dates_and_unrealizable_sets() {
    let q = parse_cron("0 0 31 * *").unwrap();
    assert_eq!(
        q.next_fire(CivilDateTime { year: 2001, month: 4, day: 30, hour: 23, minute: 59 }),
        Ok(Some(CivilDateTime { year: 2001, month: 5, day: 31, hour: 0, minute: 0 }))
    );
    let impossible = parse_cron("0 0 30 2 *").unwrap();
    assert_eq!(
        impossible.next_fire(CivilDateTime { year: 2000, month: 1, day: 1, hour: 0, minute: 0 }),
        Ok(None)
    );
    let max = CivilDateTime { year: 2399, month: 12, day: 31, hour: 23, minute: 59 };
    assert_eq!(q.next_fire(max), Ok(None));
}

#[test]
fn spec_r27_calendar_queries_are_read_only_and_instance_local() {
    let q = parse_cron("0 0 * * 6").unwrap();
    let at = CivilDateTime { year: 2000, month: 1, day: 1, hour: 0, minute: 0 };
    let expected_match = q.matches(at);
    let expected_next = q.next_fire(at);
    assert_eq!(q.matches(at), expected_match);
    assert_eq!(q.next_fire(at), expected_next);

    let other = parse_cron("30 12 15 6 1").unwrap();
    let _ = other.matches(CivilDateTime { year: 2024, month: 6, day: 15, hour: 12, minute: 30 });
    let _ = other.next_fire(at);
    assert_eq!(q.matches(at), expected_match);
    assert_eq!(q.next_fire(at), expected_next);
}
