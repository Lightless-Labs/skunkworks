// R3 R10 R11
#[test]
fn public_parse() {
    let c = Cron::parse("5-15/5 2,4").unwrap();
    assert!(c.matches(130));
    assert!(!c.matches(131));
}

// R15 R19 R24
#[test]
fn public_all() {
    let mut c = Calendar::new();
    c.add("ticks", "*/1 0-23", 1, Policy::All).unwrap();
    assert_eq!(
        c.poll(60000).unwrap(),
        vec![
            Occurrence {
                id: "ticks".into(),
                at: 0
            },
            Occurrence {
                id: "ticks".into(),
                at: 60000
            }
        ]
    );
}

// R20 R21
#[test]
fn public_policies() {
    let mut c = Calendar::new();
    c.add("late", "*/1 0-23", 1, Policy::Once).unwrap();
    c.add("skip", "*/1 0-23", 2, Policy::Skip).unwrap();
    assert_eq!(
        c.poll(90000).unwrap(),
        vec![Occurrence {
            id: "late".into(),
            at: 60000
        }]
    );
}

// R23
#[test]
fn public_ordering() {
    let mut c = Calendar::new();
    c.add("low", "0-0 0-0", 0, Policy::All).unwrap();
    c.add("high", "0-0 0-0", 9, Policy::All).unwrap();
    assert_eq!(
        c.poll(0)
            .unwrap()
            .iter()
            .map(|o| o.id.as_str())
            .collect::<Vec<_>>(),
        vec!["high", "low"]
    );
}
