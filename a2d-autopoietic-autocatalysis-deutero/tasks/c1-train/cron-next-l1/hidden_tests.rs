fn c(s: &str) -> Cron {
    parse_cron(s).unwrap()
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

// R5,R7
#[test]
fn h_empty_items_offsets() {
    for (s, off) in [("0,,4", 2), ("4,", 2), (",5", 0)] {
        assert_eq!(
            parse_cron(&format!("{s} 0 * * *")),
            Err(ParseError::EmptyItem {
                field: Field::Minute,
                offset: off
            })
        );
    }
}

// R8,R9,R10
#[test]
fn h_structure_precedes_numeric() {
    for s in ["99999999999-", "00//2", "*-/3", "2-3-4", "*/"] {
        assert_eq!(
            parse_cron(&format!("{s} 0 * * *")),
            Err(ParseError::InvalidSyntax {
                field: Field::Minute,
                offset: 0
            })
        );
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

// R10
#[test]
fn h_overflow_vs_domain() {
    assert_eq!(
        parse_cron("4294967296 0 * * *"),
        Err(ParseError::NumberOverflow {
            field: Field::Minute,
            offset: 0
        })
    );
    assert_eq!(
        parse_cron("4294967295 0 * * *"),
        Err(ParseError::OutOfRange {
            field: Field::Minute,
            offset: 0
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

// R3,R10
#[test]
fn h_domain_bounds() {
    for (s, f) in [
        ("0 24 * * *", Field::Hour),
        ("0 0 0 * *", Field::DayOfMonth),
        ("0 0 * 13 *", Field::Month),
        ("0 0 * * 7", Field::DayOfWeek),
    ] {
        assert_eq!(
            parse_cron(s),
            Err(ParseError::OutOfRange {
                field: f,
                offset: 0
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

// R11,R12
#[test]
fn h_reversal_precedes_step() {
    assert_eq!(
        parse_cron("1,8-3/000 0 * * *"),
        Err(ParseError::ReversedRange {
            field: Field::Minute,
            offset: 4
        })
    );
}

// R3,R12
#[test]
fn h_step_zero_and_width() {
    for (s, f, off) in [
        ("*/0 0 * * *", Field::Minute, 2),
        ("0 */25 * * *", Field::Hour, 2),
        ("0 0 */32 * *", Field::DayOfMonth, 2),
        ("0 0 * */13 *", Field::Month, 2),
        ("0 0 * * */8", Field::DayOfWeek, 2),
    ] {
        assert_eq!(
            parse_cron(s),
            Err(ParseError::BadStep {
                field: f,
                offset: off
            })
        );
    }
}

// R10,R12
#[test]
fn h_step_overflow() {
    assert_eq!(
        parse_cron("1,*/4294967296 0 * * *"),
        Err(ParseError::NumberOverflow {
            field: Field::Minute,
            offset: 4
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

// R3,R14,R15
#[test]
fn h_step_anchor_nonzero_min() {
    let q = c("0 0 */9 */5 */3");
    assert_eq!(q.allowed(Field::DayOfMonth), vec![1, 10, 19, 28]);
    assert_eq!(q.allowed(Field::Month), vec![1, 6, 11]);
    assert_eq!(q.allowed(Field::DayOfWeek), vec![0, 3, 6]);
}

// R12,R14,R15
#[test]
fn h_large_steps_valid_small_range() {
    assert_eq!(c("6-8/60 0 * * *").allowed(Field::Minute), vec![6]);
    assert_eq!(c("0 0 31/31 * *").allowed(Field::DayOfMonth), vec![31]);
}

// R13,R15
#[test]
fn h_inclusive_equal_range() {
    assert_eq!(c("58-59 0 * * *").allowed(Field::Minute), vec![58, 59]);
    assert_eq!(c("9-9 0 * * *").allowed(Field::Minute), vec![9]);
}

// R13,R15,R16
#[test]
fn h_union_sorted_distinct() {
    assert_eq!(
        c("11,3-9/3,6,11,0 0 * * *").allowed(Field::Minute),
        vec![0, 3, 6, 9, 11]
    );
}

// R3,R13,R16
#[test]
fn h_full_domain_and_copy() {
    let q = c("* * * * *");
    let mut v = q.allowed(Field::Minute);
    v.clear();
    assert_eq!(q.allowed(Field::Minute), (0..=59).collect::<Vec<_>>());
    assert_eq!(q.allowed(Field::DayOfMonth), (1..=31).collect::<Vec<_>>());
    assert_eq!(q.allowed(Field::Month), (1..=12).collect::<Vec<_>>());
    assert_eq!(q.allowed(Field::DayOfWeek), (0..=6).collect::<Vec<_>>());
}

// R17
#[test]
fn h_unrealizable_parses() {
    assert_eq!(c("0 0 30 2 *").allowed(Field::DayOfMonth), vec![30]);
}

// R18
#[test]
fn h_semantic_equality_and_day_flags() {
    assert_eq!(c("2,1,1 0 * 1-12 *"), c("1-2 0 * * *"));
    assert_ne!(c("0 0 * * *"), c("0 0 */1 * *"));
    assert_ne!(c("0 0 * * *"), c("0 0 * * 0-6"));
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

// R13,R14
#[test]
fn h_no_textbook_single_extension() {
    assert_eq!(c("12 0 * * *").allowed(Field::Minute), vec![12]);
    assert_eq!(c("12/16 0 * * *").allowed(Field::Minute), vec![12, 28, 44]);
}
