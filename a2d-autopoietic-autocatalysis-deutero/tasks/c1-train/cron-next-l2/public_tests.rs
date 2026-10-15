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

// R19,R24,R25
#[test]
fn p_match_and_next() {
    let q = c("30 9 * * *");
    assert!(q.matches(d(2026, 5, 4, 9, 30)).unwrap());
    assert!(!q.matches(d(2026, 5, 4, 9, 31)).unwrap());
    assert_eq!(
        q.next_fire(d(2026, 5, 4, 9, 0)).unwrap(),
        Some(d(2026, 5, 4, 9, 30))
    );
}

// R22,R25,R26
#[test]
fn p_next_leap() {
    assert_eq!(
        c("45 6 29 2 *").next_fire(d(2003, 1, 1, 0, 0)).unwrap(),
        Some(d(2004, 2, 29, 6, 45))
    );
}
