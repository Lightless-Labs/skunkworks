fn c(s: &str) -> Cron {
    parse_cron(s).unwrap()
}

// R13,R15,R16
#[test]
fn p_lists_and_ranges() {
    let q = c("4,10-12,4 8 * * *");
    assert_eq!(q.allowed(Field::Minute), vec![4, 10, 11, 12]);
    assert_eq!(q.allowed(Field::Hour), vec![8]);
}

// R14,R15
#[test]
fn p_star_steps() {
    let q = c("*/20 0 * */3 *");
    assert_eq!(q.allowed(Field::Minute), vec![0, 20, 40]);
    assert_eq!(q.allowed(Field::Month), vec![1, 4, 7, 10]);
}

// R4,R10
#[test]
fn p_parse_error() {
    assert_eq!(
        parse_cron("0 0 * *"),
        Err(ParseError::FieldCount { found: 4 })
    );
    assert_eq!(
        parse_cron("0 0 32 * *"),
        Err(ParseError::OutOfRange {
            field: Field::DayOfMonth,
            offset: 0
        })
    );
}
