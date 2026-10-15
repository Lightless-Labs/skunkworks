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

// R29,R31
#[test]
fn p_named_parse() {
    let q = c("5 2 * JAN-MAR MON,WED");
    assert_eq!(q.allowed(Field::Month), vec![1, 2, 3]);
    assert_eq!(q.allowed(Field::DayOfWeek), vec![1, 3]);
}

// R32,R33
#[test]
fn p_day_or() {
    let q = c("0 0 12 * MON");
    assert!(q.matches(d(2000, 1, 12, 0, 0)).unwrap());
    assert!(q.matches(d(2000, 1, 17, 0, 0)).unwrap());
    assert!(!q.matches(d(2000, 1, 18, 0, 0)).unwrap());
}

// R25,R29,R34
#[test]
fn p_next_named() {
    assert_eq!(
        c("0 0 * FEB SUN").next_fire(d(2000, 2, 1, 0, 0)).unwrap(),
        Some(d(2000, 2, 6, 0, 0))
    );
}

// R33
#[test]
fn p_public_last_day() {
    assert!(!c("0 0 31 * TUE").matches(d(2000, 1, 31, 0, 0)).unwrap());
}
