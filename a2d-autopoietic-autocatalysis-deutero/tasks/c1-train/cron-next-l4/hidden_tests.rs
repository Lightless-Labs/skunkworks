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

// R3,R10,R12,R13,R16,R9,R10,R18
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
    assert_eq!(
        parse_cron("0000 0 * * *"),
        Err(ParseError::LeadingZero {
            field: Field::Minute,
            offset: 0
        })
    );
    assert_eq!(
        parse_cron("4294967296 0 * * *"),
        Err(ParseError::NumberOverflow {
            field: Field::Minute,
            offset: 0
        })
    );
    assert_ne!(c("0 0 * * *"), c("0 0 */1 * *"));
}

// R19,R20,R21,R22
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

// R23,R24,R32,R33,R34
#[test]
fn h_weekday_anchor_and_leap() {
    let q = c("0 0 * * 6");
    assert!(q.matches(d(2000, 1, 1, 0, 0)).unwrap());
    assert!(!q.matches(d(2000, 1, 2, 0, 0)).unwrap());
    assert!(c("0 0 * * 2").matches(d(2000, 2, 29, 0, 0)).unwrap());
    assert!(c("0 0 * * 1").matches(d(2100, 3, 1, 0, 0)).unwrap());
}

// R16,R17,R20,R25,R26
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
    let q = c("* * * * *");
    assert_eq!(
        q.next_fire(d(2031, 7, 9, 12, 58)).unwrap(),
        Some(d(2031, 7, 9, 12, 59))
    );
    assert_eq!(
        q.next_fire(d(2031, 7, 9, 12, 59)).unwrap(),
        Some(d(2031, 7, 9, 13, 0))
    );
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

// R24,R32,R33,R34
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

// R28,R29,R31
#[test]
fn h_names_case_mixed_ranges() {
    let q = c("0 0 * fEb-4,dec mOn-3,sAT");
    assert_eq!(q.allowed(Field::Month), vec![2, 3, 4, 12]);
    assert_eq!(q.allowed(Field::DayOfWeek), vec![1, 2, 3, 6]);
}

// R29,R16
#[test]
fn h_all_names_and_mapping() {
    assert_eq!(
        c("0 0 * DEC,NOV,OCT,SEP,AUG,JUL,JUN,MAY,APR,MAR,FEB,JAN SAT,FRI,THU,WED,TUE,MON,SUN")
            .allowed(Field::Month),
        (1..=12).collect::<Vec<_>>()
    );
    assert_eq!(
        c("0 0 * * SAT,FRI,THU,WED,TUE,MON,SUN").allowed(Field::DayOfWeek),
        (0..=6).collect::<Vec<_>>()
    );
}

// R28,R30,R31
#[test]
fn h_forbidden_names_syntax() {
    for (s, f) in [
        ("JAN 0 * * *", Field::Minute),
        ("0 MON * * *", Field::Hour),
        ("0 0 FEB * *", Field::DayOfMonth),
        ("0 0 * JANUARY *", Field::Month),
        ("0 0 * J *", Field::Month),
        ("0 0 * JAN2 *", Field::Month),
        ("0 0 * */FEB *", Field::Month),
        ("0 0 * * TUES", Field::DayOfWeek),
    ] {
        assert_eq!(
            parse_cron(s),
            Err(ParseError::InvalidSyntax {
                field: f,
                offset: 0
            })
        );
    }
}

// R8,R11,R30,R31
#[test]
fn h_named_syntax_precedence_and_reversal() {
    assert_eq!(
        parse_cron("0 0 * 99-JUNK/0 *"),
        Err(ParseError::InvalidSyntax {
            field: Field::Month,
            offset: 0
        })
    );
    assert_eq!(
        parse_cron("0 0 * JAN,DEC-FEB/0 *"),
        Err(ParseError::ReversedRange {
            field: Field::Month,
            offset: 8
        })
    );
    assert_eq!(
        parse_cron("0 0 * * SUN,FRI-MON"),
        Err(ParseError::ReversedRange {
            field: Field::DayOfWeek,
            offset: 8
        })
    );
}

// R14,R29,R31
#[test]
fn h_named_steps() {
    let q = c("0 0 * FEB/4 MON-SAT/2");
    assert_eq!(q.allowed(Field::Month), vec![2, 6, 10]);
    assert_eq!(q.allowed(Field::DayOfWeek), vec![1, 3, 5]);
}

// R24,R32,R33,R34
#[test]
fn h_restricted_or_and_unrestricted() {
    let q = c("0 0 10 * MON");
    assert!(q.matches(d(2000, 1, 10, 0, 0)).unwrap());
    assert!(q.matches(d(2000, 1, 17, 0, 0)).unwrap());
    assert!(q.matches(d(2000, 2, 10, 0, 0)).unwrap());
    assert!(!q.matches(d(2000, 1, 18, 0, 0)).unwrap());
    assert!(!c("0 0 * * MON").matches(d(2000, 2, 10, 0, 0)).unwrap());
    assert!(!c("0 0 10 * *").matches(d(2000, 1, 17, 0, 0)).unwrap());
}

// R22,R32,R33,R34
#[test]
fn h_last_day_requires_both() {
    let q = c("0 0 31 * MON");
    assert!(q.matches(d(2000, 1, 31, 0, 0)).unwrap());
    assert!(!q.matches(d(2000, 3, 31, 0, 0)).unwrap());
    assert!(!q.matches(d(2000, 7, 31, 0, 1)).unwrap());
    assert!(!c("0 0 1 * FRI").matches(d(2000, 3, 31, 0, 0)).unwrap());
    assert!(q.matches(d(2000, 3, 27, 0, 0)).unwrap());
    let q = c("0 0 28 2 TUE");
    assert!(!q.matches(d(2000, 2, 29, 0, 0)).unwrap());
    assert!(q.matches(d(2000, 2, 28, 0, 0)).unwrap());
    assert!(!q.matches(d(2001, 2, 28, 0, 0)).unwrap());
    assert!(c("0 0 29 2 TUE").matches(d(2000, 2, 29, 0, 0)).unwrap());
}

// R18,R32,R33
#[test]
fn h_full_domain_restricted_exception() {
    let bare = c("0 0 * * MON");
    let restricted = c("0 0 */1 * MON");
    let t = d(2000, 3, 30, 0, 0);
    assert!(!bare.matches(t).unwrap());
    assert!(restricted.matches(t).unwrap());
    let last = d(2000, 3, 31, 0, 0);
    assert!(!restricted.matches(last).unwrap());
    assert!(c("0 0 * * *").matches(last).unwrap());
    assert!(!c("0 0 1 * */1").matches(last).unwrap());
    assert!(c("0 0 1 * *").matches(d(2000, 4, 1, 0, 0)).unwrap());
}

// R35,R36,R37
#[test]
fn h_special_expansions() {
    for (s, e) in [
        ("@hourly", "0 * * * *"),
        ("@daily", "0 0 * * *"),
        ("@weekly", "0 0 * * 0"),
        ("@monthly", "0 0 1 * *"),
        ("@yearly", "0 0 1 1 *"),
    ] {
        assert_eq!(c(s), c(e));
        assert_eq!(c(s).canonical(), e);
    }
}

// R4,R6,R36
#[test]
fn h_special_error_precedence() {
    for s in [
        "@",
        "@DAILY",
        "@annually",
        "@daily ",
        "@daily 0 0 * *",
        "@hourly
",
    ] {
        assert_eq!(parse_cron(s), Err(ParseError::UnknownSpecial));
    }
    assert_eq!(
        parse_cron("0 0 * * @"),
        Err(ParseError::InvalidCharacter {
            field: Field::DayOfWeek,
            offset: 0
        })
    );
    assert_eq!(
        parse_cron(" @daily"),
        Err(ParseError::FieldCount { found: 2 })
    );
}

// R18,R38,R39,R40
#[test]
fn h_canonical_partial_and_full_flags() {
    assert_eq!(
        c("9,2-4,3 6/6 1-31 JAN-DEC SUN-SAT").canonical(),
        "2,3,4,9 6,12,18 */1 * */1"
    );
    assert_eq!(c("*/1 0-23 * 1-12 *").canonical(), "* * * * *");
}

// R32,R33,R38,R39,R40
#[test]
fn h_canonical_roundtrip_preserves_day_rules() {
    for s in [
        "0 0 1-31 * MON",
        "0 0 1 * SUN-SAT",
        "4/7 0 * FEB,DEC *",
        "@weekly",
    ] {
        let q = c(s);
        let r = c(&q.canonical());
        assert_eq!(q, r);
        assert_eq!(q.canonical(), r.canonical());
        for t in [
            d(2000, 3, 30, 0, 0),
            d(2000, 3, 31, 0, 0),
            d(2032, 2, 29, 0, 0),
        ] {
            assert_eq!(q.matches(t), r.matches(t));
            assert_eq!(q.next_fire(t), r.next_fire(t));
            assert_eq!(q.previous_fire(t), r.previous_fire(t));
        }
    }
}

// R20,R41
#[test]
fn h_previous_strict_and_lower_bound() {
    let q = c("* * * * *");
    assert_eq!(q.previous_fire(d(2000, 1, 1, 0, 0)).unwrap(), None);
    assert_eq!(
        q.previous_fire(d(2000, 1, 1, 0, 1)).unwrap(),
        Some(d(2000, 1, 1, 0, 0))
    );
    assert_eq!(
        q.previous_fire(d(2389, 7, 8, 13, 0)).unwrap(),
        Some(d(2389, 7, 8, 12, 59))
    );
}

// R22,R41
#[test]
fn h_previous_leap_century_rollover() {
    let q = c("* * * * *");
    assert_eq!(
        q.previous_fire(d(2100, 3, 1, 0, 0)).unwrap(),
        Some(d(2100, 2, 28, 23, 59))
    );
    assert_eq!(
        q.previous_fire(d(2000, 3, 1, 0, 0)).unwrap(),
        Some(d(2000, 2, 29, 23, 59))
    );
    assert_eq!(
        q.previous_fire(d(2001, 1, 1, 0, 0)).unwrap(),
        Some(d(2000, 12, 31, 23, 59))
    );
    assert_eq!(
        c("0 0 29 2 *").previous_fire(d(2103, 1, 1, 0, 0)).unwrap(),
        Some(d(2096, 2, 29, 0, 0))
    );
}

// R17,R32,R33,R41
#[test]
fn h_previous_or_last_day_and_none() {
    assert_eq!(
        c("0 0 1 MAR FRI")
            .previous_fire(d(2000, 4, 1, 0, 0))
            .unwrap(),
        Some(d(2000, 3, 24, 0, 0))
    );
    assert_eq!(
        c("0 0 30 FEB *")
            .previous_fire(d(2399, 12, 31, 23, 59))
            .unwrap(),
        None
    );
    assert_eq!(
        c("55,2 20,8 * * *")
            .previous_fire(d(2000, 1, 2, 8, 2))
            .unwrap(),
        Some(d(2000, 1, 1, 20, 55))
    );
}

// R21,R41
#[test]
fn h_previous_validates_all_components() {
    let q = c("0 0 30 2 *");
    for (t, part) in [
        (d(2400, 13, 32, 24, 60), DatePart::Year),
        (d(2001, 0, 32, 24, 60), DatePart::Month),
        (d(2001, 2, 29, 24, 60), DatePart::Day),
        (d(2001, 2, 28, 24, 60), DatePart::Hour),
        (d(2001, 2, 28, 23, 60), DatePart::Minute),
    ] {
        assert_eq!(
            q.previous_fire(t),
            Err(DateError::InvalidDateTime { component: part })
        );
    }
}

// R21,R42
#[test]
fn h_window_validation_precedence() {
    let q = c("* * * * *");
    let good = d(2000, 1, 1, 0, 0);
    assert_eq!(
        q.fires_between(d(1999, 0, 0, 99, 99), d(2400, 1, 1, 0, 0), 0),
        Err(WindowError::InvalidStart {
            component: DatePart::Year
        })
    );
    for (t, part) in [
        (d(2399, 0, 0, 99, 99), DatePart::Month),
        (d(2399, 2, 29, 99, 99), DatePart::Day),
        (d(2399, 2, 28, 99, 99), DatePart::Hour),
        (d(2399, 2, 28, 23, 99), DatePart::Minute),
    ] {
        assert_eq!(
            q.fires_between(t, good, 0),
            Err(WindowError::InvalidStart { component: part })
        );
        assert_eq!(
            q.fires_between(good, t, 0),
            Err(WindowError::InvalidEnd { component: part })
        );
    }
    assert_eq!(
        q.fires_between(good, d(2400, 1, 1, 0, 0), 0),
        Err(WindowError::InvalidEnd {
            component: DatePart::Year
        })
    );
}

// R42,R43,R44
#[test]
fn h_window_order_before_cap_and_empty() {
    let q = c("* * * * *");
    let a = d(2040, 1, 1, 0, 0);
    let b = d(2040, 1, 1, 0, 1);
    assert_eq!(q.fires_between(b, a, 0), Err(WindowError::ReversedWindow));
    assert_eq!(q.fires_between(a, a, 0), Err(WindowError::ZeroCap));
    assert_eq!(
        q.fires_between(a, a, 2).unwrap(),
        FireWindow {
            fires: vec![],
            truncated: false
        }
    );
}

// R43,R44
#[test]
fn h_window_half_open_exact_cap() {
    let q = c("* * * * *");
    let a = d(2040, 1, 1, 0, 0);
    let b = d(2040, 1, 1, 0, 2);
    assert_eq!(
        q.fires_between(a, b, 2).unwrap(),
        FireWindow {
            fires: vec![a, d(2040, 1, 1, 0, 1)],
            truncated: false
        }
    );
    assert_eq!(
        q.fires_between(a, b, 1).unwrap(),
        FireWindow {
            fires: vec![a],
            truncated: true
        }
    );
}

// R33,R43,R44
#[test]
fn h_window_sparse_and_last_exception() {
    let q = c("0 0 1 MAR FRI");
    let a = d(2000, 3, 23, 1, 0);
    let b = d(2000, 4, 1, 0, 0);
    assert_eq!(
        q.fires_between(a, b, 1).unwrap(),
        FireWindow {
            fires: vec![d(2000, 3, 24, 0, 0)],
            truncated: false
        }
    );
    assert_eq!(
        c("0 0 30 2 *")
            .fires_between(d(2000, 1, 1, 0, 0), d(2399, 12, 31, 23, 59), usize::MAX)
            .unwrap(),
        FireWindow {
            fires: vec![],
            truncated: false
        }
    );
}

// R15,R16,R20,R22,R26,R43,R44,R45
#[test]
fn h_window_order_dedup_and_large_cap() {
    let q = c("7,3,3 2 * * *");
    let a = d(2033, 4, 1, 0, 0);
    let b = d(2033, 4, 3, 0, 0);
    let expected = vec![
        d(2033, 4, 1, 2, 3),
        d(2033, 4, 1, 2, 7),
        d(2033, 4, 2, 2, 3),
        d(2033, 4, 2, 2, 7),
    ];
    assert_eq!(
        q.fires_between(a, b, usize::MAX).unwrap(),
        FireWindow {
            fires: expected.clone(),
            truncated: false
        }
    );
    assert_eq!(
        q.fires_between(a, b, 3).unwrap(),
        FireWindow {
            fires: expected[..3].to_vec(),
            truncated: true
        }
    );
    let q = c("0 0 29 2 *");
    let a = d(2000, 1, 1, 0, 0);
    let b = d(2399, 12, 31, 23, 59);
    let w = q.fires_between(a, b, 97).unwrap();
    assert_eq!(w.fires.len(), 97);
    assert!(!w.truncated);
    assert_eq!(w.fires[0], d(2000, 2, 29, 0, 0));
    assert_eq!(w.fires[96], d(2396, 2, 29, 0, 0));
    assert!(
        w.fires
            .iter()
            .all(|t| ![2100, 2200, 2300].contains(&t.year))
    );
    let w = q.fires_between(a, b, 96).unwrap();
    assert_eq!(w.fires.len(), 96);
    assert!(w.truncated);
}

// R27,R41,R43,R44,R45
#[test]
fn h_window_read_only_and_upper_boundary() {
    let q = c("* * * * *");
    let a = d(2399, 12, 31, 23, 57);
    let b = d(2399, 12, 31, 23, 59);
    let expected = q.fires_between(a, b, 2);
    let _ = q.previous_fire(b);
    let _ = c("@weekly").next_fire(a);
    assert_eq!(q.fires_between(a, b, 2), expected);
    assert_eq!(
        expected.unwrap(),
        FireWindow {
            fires: vec![a, d(2399, 12, 31, 23, 58)],
            truncated: false
        }
    );
}
