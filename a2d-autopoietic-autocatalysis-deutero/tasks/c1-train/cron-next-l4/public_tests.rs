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

// R16,R36,R37,R38
#[test]
fn p_parse_canonical_allowed() {
    let q = c("15 5 * * *");
    assert_eq!(q.canonical(), "15 5 * * *");
    assert_eq!(q.allowed(Field::Minute), vec![15]);
    assert_eq!(c("4,2 3 * MAR MON").canonical(), "2,4 3 * 3 1");
}

// R25,R32,R33
#[test]
fn p_next_and_match() {
    let q = c("0 0 12 * MON");
    assert!(q.matches(d(2000, 1, 12, 0, 0)).unwrap());
    assert_eq!(
        q.next_fire(d(2000, 1, 12, 1, 0)).unwrap(),
        Some(d(2000, 1, 17, 0, 0))
    );
}

// R41
#[test]
fn p_previous_daily() {
    assert_eq!(
        c("15 5 * * *").previous_fire(d(2020, 6, 9, 8, 0)).unwrap(),
        Some(d(2020, 6, 9, 5, 15))
    );
}

// R43,R44
#[test]
fn p_window_capped() {
    assert_eq!(
        c("10 * * * *")
            .fires_between(d(2020, 6, 9, 1, 10), d(2020, 6, 9, 4, 10), 2)
            .unwrap(),
        FireWindow {
            fires: vec![d(2020, 6, 9, 1, 10), d(2020, 6, 9, 2, 10)],
            truncated: true
        }
    );
}
