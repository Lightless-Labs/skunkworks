// R2 R4
#[test]
fn hidden_layouts() {
    for (s, p) in [
        ("", 0),
        ("*", 1),
        (" *", 0),
        ("* ", 1),
        ("*  *", 2),
        ("* *	", 3),
        ("x  *", 2),
    ] {
        assert_eq!(Cron::parse(s), Err(CronError::Layout(p)));
    }
}

// R5
#[test]
fn hidden_leading_zero() {
    for (s, p) in [("01 *", 0), ("* 00", 2), ("1,02 *", 2), ("*/02 *", 2)] {
        assert_eq!(Cron::parse(s), Err(CronError::Number(p)));
    }
}

// R3 R4 R5 R9
#[test]
fn hidden_syntax_offsets() {
    for (s, p) in [
        ("1,,2 *", 2),
        ("1, *", 2),
        ("é *", 0),
        ("1 2x", 3),
        ("* ,1", 2),
        ("1- *", 2),
    ] {
        assert_eq!(Cron::parse(s), Err(CronError::Syntax(p)));
    }
}

// R6
#[test]
fn hidden_bounds() {
    for (s, p) in [
        ("60 *", 0),
        ("* 24", 2),
        ("1-60 *", 2),
        ("99-0 *", 0),
        ("99999999999999999999999999 *", 0),
    ] {
        assert_eq!(Cron::parse(s), Err(CronError::Bounds(p)));
    }
}

// R6 R8
#[test]
fn hidden_descending() {
    assert_eq!(Cron::parse("20-3/0 *"), Err(CronError::Descending(2)));
    assert_eq!(Cron::parse("* 8-2"), Err(CronError::Descending(3)));
}

// R8
#[test]
fn hidden_step_limits() {
    for (s, p) in [("*/0 *", 2), ("*/61 *", 2), ("* */25", 4)] {
        assert_eq!(Cron::parse(s), Err(CronError::Step(p)));
    }
    assert!(Cron::parse("*/60 */24").unwrap().matches(0));
}

// R7
#[test]
fn hidden_slash_precedence() {
    assert_eq!(Cron::parse("99/0 *"), Err(CronError::Syntax(2)));
    assert_eq!(Cron::parse("99-1/0/2 *"), Err(CronError::Syntax(6)));
    assert_eq!(Cron::parse("1-2-3 *"), Err(CronError::Syntax(3)));
}

// R4 R6 R8
#[test]
fn hidden_base_before_step() {
    assert_eq!(Cron::parse("2-61/x *"), Err(CronError::Bounds(2)));
    assert_eq!(Cron::parse("x 99"), Err(CronError::Syntax(0)));
}

// R10
#[test]
fn hidden_anchored() {
    let c = Cron::parse("3-14/4 1-8/3").unwrap();
    for m in 0..1440 {
        assert_eq!(
            c.matches(m),
            [3, 7, 11].contains(&(m % 60)) && [1, 4, 7].contains(&(m / 60))
        );
    }
}

// R1 R11
#[test]
fn hidden_max_minute() {
    let c = Cron::parse("15 12").unwrap();
    assert_eq!(
        c.matches(u64::MAX),
        (u64::MAX % 60) == 15 && ((u64::MAX / 60) % 24) == 12
    );
    assert!(Cron::parse("* *").unwrap().matches(u64::MAX));
}

// R3 R24
#[test]
fn hidden_union() {
    let mut c = Calendar::new();
    c.add("union", "2,0-4/2,2 0", 1, Policy::All).unwrap();
    assert_eq!(
        c.poll(240000)
            .unwrap()
            .iter()
            .map(|o| o.at)
            .collect::<Vec<_>>(),
        vec![0, 120000, 240000]
    );
}

// R13 R14
#[test]
fn hidden_id_priority() {
    let mut c = Calendar::new();
    assert_eq!(c.add("X", "bad", 255, Policy::All), Err(Error::BadId));
    assert_eq!(c.add("ok", "bad", 10, Policy::All), Err(Error::BadPriority));
    for id in ["", "abcdefghijklmnopq", "é"] {
        assert_eq!(c.add(id, "* *", 0, Policy::All), Err(Error::BadId));
    }
}

// R12 R14
#[test]
fn hidden_duplicate_precedence() {
    let mut c = Calendar::new();
    c.add("same", "* *", 0, Policy::All).unwrap();
    assert_eq!(c.add("same", "?", 0, Policy::All), Err(Error::Duplicate));
    assert_eq!(c.add("same", "?", 11, Policy::All), Err(Error::BadPriority));
}

// R14
#[test]
fn hidden_add_atomic() {
    let mut c = Calendar::new();
    assert_eq!(
        c.add("retry", "60 *", 0, Policy::All),
        Err(Error::Cron(CronError::Bounds(0)))
    );
    c.add("retry", "0 0", 0, Policy::All).unwrap();
    assert_eq!(c.poll(0).unwrap().len(), 1);
}

// R15 R22
#[test]
fn hidden_poll_watermark() {
    let mut c = Calendar::new();
    c.add("t", "* *", 0, Policy::All).unwrap();
    assert_eq!(c.poll(60001).unwrap().len(), 2);
    assert!(c.poll(60001).unwrap().is_empty());
    assert!(c.poll(119999).unwrap().is_empty());
    assert_eq!(c.poll(120000).unwrap()[0].at, 120000);
}

// R16 R26
#[test]
fn hidden_late_add() {
    let mut c = Calendar::new();
    c.poll(60000).unwrap();
    c.add("late", "* *", 0, Policy::All).unwrap();
    assert!(c.poll(60000).unwrap().is_empty());
    assert_eq!(
        c.poll(120000).unwrap(),
        vec![Occurrence {
            id: "late".into(),
            at: 120000
        }]
    );
}

// R17 R18
#[test]
fn hidden_poll_errors() {
    let mut c = Calendar::new();
    assert_eq!(c.poll(604800001), Err(Error::TimeRange));
    assert!(c.poll(0).unwrap().is_empty());
    c.poll(604800000).unwrap();
    assert_eq!(c.poll(1), Err(Error::TimeReversed));
}

// R20 R24
#[test]
fn hidden_once_latest() {
    let mut c = Calendar::new();
    c.add("one", "*/7 *", 0, Policy::Once).unwrap();
    assert_eq!(
        c.poll(1200000).unwrap(),
        vec![Occurrence {
            id: "one".into(),
            at: 840000
        }]
    );
    assert_eq!(c.poll(1300000).unwrap()[0].at, 1260000);
}

// R21 R22
#[test]
fn hidden_skip_exact() {
    let mut c = Calendar::new();
    c.add("s", "* *", 0, Policy::Skip).unwrap();
    assert_eq!(
        c.poll(120000).unwrap(),
        vec![Occurrence {
            id: "s".into(),
            at: 120000
        }]
    );
    assert!(c.poll(120001).unwrap().is_empty());
    assert!(c.poll(179999).unwrap().is_empty());
}

// R23
#[test]
fn hidden_tie_fifo() {
    let mut c = Calendar::new();
    c.add("z", "0 0", 3, Policy::All).unwrap();
    c.add("a", "0 0", 3, Policy::All).unwrap();
    assert_eq!(
        c.poll(0)
            .unwrap()
            .iter()
            .map(|o| o.id.as_str())
            .collect::<Vec<_>>(),
        vec!["z", "a"]
    );
}

// R19 R23
#[test]
fn hidden_time_before_priority() {
    let mut c = Calendar::new();
    c.add("old", "0 0", 0, Policy::All).unwrap();
    c.add("new", "1 0", 9, Policy::All).unwrap();
    assert_eq!(
        c.poll(60000)
            .unwrap()
            .iter()
            .map(|o| o.id.as_str())
            .collect::<Vec<_>>(),
        vec!["old", "new"]
    );
}

// R1 R11 R19
#[test]
fn hidden_day_wrap() {
    let mut c = Calendar::new();
    c.add("daily", "0 0", 0, Policy::All).unwrap();
    assert_eq!(
        c.poll(172800000)
            .unwrap()
            .iter()
            .map(|o| o.at)
            .collect::<Vec<_>>(),
        vec![0, 86400000, 172800000]
    );
}

// R25 R26
#[test]
fn hidden_independent() {
    let mut a = Calendar::new();
    let mut b = Calendar::new();
    a.poll(100).unwrap();
    b.add("zero", "0 0", 0, Policy::All).unwrap();
    assert_eq!(b.poll(0).unwrap().len(), 1);
}

// R17 R19 R26
#[test]
fn hidden_seven_days() {
    let mut c = Calendar::new();
    c.add("many", "* *", 0, Policy::All).unwrap();
    let v = c.poll(604800000).unwrap();
    assert_eq!(v.len(), 10081);
    assert_eq!(v.last().unwrap().at, 604800000);
}
