fn c(s: &str) -> Cron {
    parse_cron(s).unwrap()
}
fn d(year: u16, month: u8, day: u8, hour: u8, minute: u8) -> CivilDateTime {
    CivilDateTime {
        year,
        month,
        day,
        hour,
        minute,
    }
}

// R1,R2,R4
#[test]
fn h_count_precedes_content() {
    assert_eq!(parse_cron("?"), Err(ParseError::FieldCount { found: 1 }));
    assert_eq!(parse_cron(""), Err(ParseError::FieldCount { found: 1 }));
    assert_eq!(
        parse_cron("0 0 * * * "),
        Err(ParseError::FieldCount { found: 6 })
    );
}

// R4,R7
#[test]
fn h_empty_field_is_item() {
    assert_eq!(
        parse_cron("0  * * *"),
        Err(ParseError::EmptyItem {
            field: Field::Hour,
            offset: 0
        })
    );
}

// R4,R5,R6
#[test]
fn h_tab_and_unicode_byte() {
    assert_eq!(
        parse_cron("0 0 * * 1	"),
        Err(ParseError::InvalidCharacter {
            field: Field::DayOfWeek,
            offset: 1
        })
    );
    assert_eq!(
        parse_cron("0,é 0 * * *"),
        Err(ParseError::InvalidCharacter {
            field: Field::Minute,
            offset: 2
        })
    );
}

// R5,R6,R7,R8
#[test]
fn h_whole_field_lexical_precedence() {
    assert_eq!(
        parse_cron("9-,2? 0 * * *"),
        Err(ParseError::InvalidCharacter {
            field: Field::Minute,
            offset: 4
        })
    );
    assert_eq!(
        parse_cron("0 4,,? * * *"),
        Err(ParseError::InvalidCharacter {
            field: Field::Hour,
            offset: 3
        })
    );
}

// R5,R6,R10
#[test]
fn h_field_order_precedence() {
    assert_eq!(
        parse_cron("60 0? * * *"),
        Err(ParseError::OutOfRange {
            field: Field::Minute,
            offset: 0
        })
    );
}

// R8,R13,R14,R15,R16
#[test]
fn h_parse_structure_and_steps() {
    for s in ["2-", "*/", "1/2/3"] {
        assert_eq!(
            parse_cron(&format!("{s} 0 * * *")),
            Err(ParseError::InvalidSyntax {
                field: Field::Minute,
                offset: 0
            })
        );
    }
    assert_eq!(
        c("51/4,55,1-2 0 * * *").allowed(Field::Minute),
        vec![1, 2, 51, 55, 59]
    );
    assert_eq!(c("8-9/60 0 * * *").allowed(Field::Minute), vec![8]);
    assert_eq!(c("3 0 * * *").allowed(Field::Minute), vec![3]);
    assert_eq!(c("0 0 */10 */4 *").allowed(Field::Month), vec![1, 5, 9]);
}

// R3,R10,R12,R13,R16
#[test]
fn h_parse_bounds_and_full_sets() {
    assert_eq!(
        parse_cron("0 0 * 0 *"),
        Err(ParseError::OutOfRange {
            field: Field::Month,
            offset: 0
        })
    );
    assert_eq!(
        parse_cron("0 0 * * 7"),
        Err(ParseError::OutOfRange {
            field: Field::DayOfWeek,
            offset: 0
        })
    );
    let q = c("* * * * *");
    assert_eq!(q.allowed(Field::Minute), (0..=59).collect::<Vec<_>>());
    assert_eq!(q.allowed(Field::Hour), (0..=23).collect::<Vec<_>>());
    assert_eq!(q.allowed(Field::DayOfMonth), (1..=31).collect::<Vec<_>>());
    assert_eq!(q.allowed(Field::Month), (1..=12).collect::<Vec<_>>());
    assert_eq!(q.allowed(Field::DayOfWeek), (0..=6).collect::<Vec<_>>());
}

// R5,R7,R9,R10,R11,R12
#[test]
fn h_parse_precedence_bundle() {
    for (s, e) in [
        (
            "4,",
            ParseError::EmptyItem {
                field: Field::Minute,
                offset: 2,
            },
        ),
        (
            "00",
            ParseError::LeadingZero {
                field: Field::Minute,
                offset: 0,
            },
        ),
        (
            "8-2/0",
            ParseError::ReversedRange {
                field: Field::Minute,
                offset: 2,
            },
        ),
        (
            "4-99/0",
            ParseError::OutOfRange {
                field: Field::Minute,
                offset: 2,
            },
        ),
        (
            "*/61",
            ParseError::BadStep {
                field: Field::Minute,
                offset: 2,
            },
        ),
        (
            "*/0",
            ParseError::BadStep {
                field: Field::Minute,
                offset: 2,
            },
        ),
        (
            "*/4294967296",
            ParseError::NumberOverflow {
                field: Field::Minute,
                offset: 2,
            },
        ),
    ] {
        assert_eq!(parse_cron(&format!("{s} 0 * * *")), Err(e));
    }
}

// R5,R8
#[test]
fn h_syntax_at_item_start() {
    assert_eq!(
        parse_cron("1,4/2/3 0 * * *"),
        Err(ParseError::InvalidSyntax {
            field: Field::Minute,
            offset: 2
        })
    );
}

// R9,R10
#[test]
fn h_zero_prefix_precedes_overflow() {
    assert_eq!(
        parse_cron("000429496729999 0 * * *"),
        Err(ParseError::LeadingZero {
            field: Field::Minute,
            offset: 0
        })
    );
}

// R18
#[test]
fn h_semantic_equality_and_day_flags() {
    assert_eq!(c("2,1,1 0 * 1-12 *"), c("1-2 0 * * *"));
    assert_ne!(c("0 0 * * *"), c("0 0 */1 * *"));
    assert_ne!(c("0 0 * * *"), c("0 0 * * 0-6"));
}

// R19,R20,R21
#[test]
fn h_invalid_component_precedence() {
    let q = c("0 0 30 2 *");
    for (t, part) in [
        (d(1999, 0, 0, 24, 60), DatePart::Year),
        (d(2001, 13, 0, 24, 60), DatePart::Month),
        (d(2001, 2, 29, 24, 60), DatePart::Day),
        (d(2001, 2, 28, 24, 60), DatePart::Hour),
        (d(2001, 2, 28, 23, 60), DatePart::Minute),
    ] {
        let e = Err(DateError::InvalidDateTime { component: part });
        assert_eq!(q.matches(t), e);
        assert_eq!(
            q.next_fire(t),
            Err(DateError::InvalidDateTime { component: part })
        );
    }
}

// R20,R21
#[test]
fn h_upper_year_and_zero_day() {
    let q = c("* * * * *");
    assert_eq!(
        q.next_fire(d(2400, 1, 1, 0, 0)),
        Err(DateError::InvalidDateTime {
            component: DatePart::Year
        })
    );
    assert_eq!(
        q.matches(d(2000, 1, 0, 0, 0)),
        Err(DateError::InvalidDateTime {
            component: DatePart::Day
        })
    );
    assert_eq!(
        q.matches(d(2000, 0, 1, 0, 0)),
        Err(DateError::InvalidDateTime {
            component: DatePart::Month
        })
    );
}

// R22,R25,R26
#[test]
fn h_century_not_leap() {
    let q = c("0 0 29 2 *");
    assert_eq!(
        q.next_fire(d(2096, 2, 29, 0, 0)).unwrap(),
        Some(d(2104, 2, 29, 0, 0))
    );
    assert_eq!(
        q.matches(d(2100, 2, 29, 0, 0)),
        Err(DateError::InvalidDateTime {
            component: DatePart::Day
        })
    );
}

// R20,R22,R25
#[test]
fn h_400_and_ordinary_leaps() {
    let q = c("0 0 29 2 *");
    assert!(q.matches(d(2000, 2, 29, 0, 0)).unwrap());
    assert_eq!(
        q.next_fire(d(2003, 3, 1, 0, 0)).unwrap(),
        Some(d(2004, 2, 29, 0, 0))
    );
}

// R21,R22
#[test]
fn h_month_lengths_validation() {
    let q = c("* * * * *");
    for m in [4, 6, 9, 11] {
        assert_eq!(
            q.matches(d(2013, m, 31, 0, 0)),
            Err(DateError::InvalidDateTime {
                component: DatePart::Day
            })
        );
        assert!(q.matches(d(2013, m, 30, 0, 0)).unwrap());
    }
    for m in [1, 3, 5, 7, 8, 10, 12] {
        assert!(q.matches(d(2013, m, 31, 0, 0)).unwrap());
    }
}

// R23,R24
#[test]
fn h_weekday_anchor_and_leap() {
    let q = c("0 0 * * 6");
    assert!(q.matches(d(2000, 1, 1, 0, 0)).unwrap());
    assert!(!q.matches(d(2000, 1, 2, 0, 0)).unwrap());
    assert!(c("0 0 * * 2").matches(d(2000, 2, 29, 0, 0)).unwrap());
    assert!(c("0 0 * * 1").matches(d(2100, 3, 1, 0, 0)).unwrap());
}

// R25
#[test]
fn h_next_strict_minute() {
    let q = c("* * * * *");
    assert_eq!(
        q.next_fire(d(2031, 7, 9, 12, 58)).unwrap(),
        Some(d(2031, 7, 9, 12, 59))
    );
    assert_eq!(
        q.next_fire(d(2031, 7, 9, 12, 59)).unwrap(),
        Some(d(2031, 7, 9, 13, 0))
    );
}

// R16,R25
#[test]
fn h_same_day_hour_minute_order() {
    let q = c("55,5 18,7 * * *");
    assert_eq!(
        q.next_fire(d(2031, 7, 9, 7, 6)).unwrap(),
        Some(d(2031, 7, 9, 7, 55))
    );
    assert_eq!(
        q.next_fire(d(2031, 7, 9, 7, 55)).unwrap(),
        Some(d(2031, 7, 9, 18, 5))
    );
}

// R22,R25,R26
#[test]
fn h_rollovers() {
    let q = c("* * * * *");
    for (a, b) in [
        (d(2000, 2, 28, 23, 59), d(2000, 2, 29, 0, 0)),
        (d(2100, 2, 28, 23, 59), d(2100, 3, 1, 0, 0)),
        (d(2011, 4, 30, 23, 59), d(2011, 5, 1, 0, 0)),
        (d(2099, 12, 31, 23, 59), d(2100, 1, 1, 0, 0)),
    ] {
        assert_eq!(q.next_fire(a).unwrap(), Some(b));
    }
}

// R17,R22,R25,R26
#[test]
fn h_skip_nonexistent_month_date() {
    let q = c("0 0 31 4,5 *");
    assert_eq!(
        q.next_fire(d(2021, 4, 1, 0, 0)).unwrap(),
        Some(d(2021, 5, 31, 0, 0))
    );
}

// R17,R20,R25,R26
#[test]
fn h_impossible_schedule_and_max() {
    assert_eq!(
        c("0 0 30 2 *").next_fire(d(2000, 1, 1, 0, 0)).unwrap(),
        None
    );
    assert_eq!(
        c("* * * * *").next_fire(d(2399, 12, 31, 23, 59)).unwrap(),
        None
    );
    assert_eq!(c("0 0 1 1 *").next_fire(d(2399, 1, 1, 0, 0)).unwrap(), None);
}

// R24
#[test]
fn h_match_all_other_fields() {
    let q = c("17 8 * 9 *");
    assert!(q.matches(d(2027, 9, 8, 8, 17)).unwrap());
    for t in [
        d(2027, 9, 8, 8, 18),
        d(2027, 9, 8, 9, 17),
        d(2027, 10, 8, 8, 17),
    ] {
        assert!(!q.matches(t).unwrap());
    }
}

// R27
#[test]
fn h_query_independence() {
    let q = c("14 3 * * *");
    let a = d(2030, 3, 4, 0, 0);
    let expected = q.next_fire(a);
    let r = c("12 17 * * *");
    let _ = r.next_fire(a);
    let mut v = q.allowed(Field::Hour);
    v.clear();
    assert_eq!(q.next_fire(a), expected);
    assert_eq!(q.next_fire(a), Ok(Some(d(2030, 3, 4, 3, 14))));
}

// R23,R24,R25
#[test]
fn h_days_are_conjunctive() {
    let q = c("0 0 2 * 0");
    assert!(!q.matches(d(2000, 1, 2, 0, 1)).unwrap());
    assert!(q.matches(d(2000, 1, 2, 0, 0)).unwrap());
    assert!(q.matches(d(2000, 4, 2, 0, 0)).unwrap());
    assert!(!q.matches(d(2000, 1, 9, 0, 0)).unwrap());
    assert!(!q.matches(d(2000, 2, 2, 0, 0)).unwrap());
    assert_eq!(
        q.next_fire(d(2000, 1, 2, 0, 0)).unwrap(),
        Some(d(2000, 4, 2, 0, 0))
    );
}

// R9,R11,R12
#[test]
fn h_leading_zero_offsets() {
    for (s, off) in [("03", 0), ("1,2-04", 4), ("*/02", 2), ("5/00", 2)] {
        assert_eq!(
            parse_cron(&format!("{s} 0 * * *")),
            Err(ParseError::LeadingZero {
                field: Field::Minute,
                offset: off
            })
        );
    }
}

// R10,R11,R12
#[test]
fn h_endpoint_error_before_step() {
    assert_eq!(
        parse_cron("4-61/0 0 * * *"),
        Err(ParseError::OutOfRange {
            field: Field::Minute,
            offset: 2
        })
    );
    assert_eq!(
        parse_cron("61-00/0 0 * * *"),
        Err(ParseError::OutOfRange {
            field: Field::Minute,
            offset: 0
        })
    );
}

// R14,R15,R16
#[test]
fn h_single_step_extends() {
    assert_eq!(
        c("53/2 0 * * *").allowed(Field::Minute),
        vec![53, 55, 57, 59]
    );
    assert_eq!(c("0 19/2 * * *").allowed(Field::Hour), vec![19, 21, 23]);
}

// R3,R12,R14,R15
#[test]
fn h_all_field_step_widths() {
    let q = c("*/60 */24 */31 */12 */7");
    for (f, v) in [
        (Field::Minute, 0),
        (Field::Hour, 0),
        (Field::DayOfMonth, 1),
        (Field::Month, 1),
        (Field::DayOfWeek, 0),
    ] {
        assert_eq!(q.allowed(f), vec![v]);
    }
}

// R17
#[test]
fn h_unrealizable_parses() {
    assert_eq!(c("0 0 30 2 *").allowed(Field::DayOfMonth), vec![30]);
}
