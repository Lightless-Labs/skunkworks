// R1 R9 R13 R15 R32 R38
#[test]
fn public_basic() {
    let mut p = Pool::new(
        vec![Job {
            id: "pub-a".into(),
            priority: 1,
            dependencies: vec![],
            max_attempts: 2,
            release_at: 0,
            duration: 5,
            tag: None,
            failures: 0,
        }],
        1,
        0,
        3,
        10,
    )
    .unwrap();
    assert_eq!(
        p.status("pub-a"),
        Ok(State::Waiting {
            ready_at: 0,
            attempt: 0
        })
    );
    assert_eq!(
        p.run(5).unwrap(),
        vec!["0 start pub-a 1 5", "5 finish pub-a 1 ok"]
    );
    assert_eq!(p.status("pub-a"), Ok(State::Succeeded));
}

// R6 R25
#[test]
fn public_tags() {
    let mut p = Pool::new(
        vec![
            Job {
                id: "pub-a".into(),
                priority: 2,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 0,
                duration: 5,
                tag: Some("db".into()),
                failures: 0,
            },
            Job {
                id: "pub-b".into(),
                priority: 1,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 0,
                duration: 5,
                tag: Some("db".into()),
                failures: 0,
            },
        ],
        2,
        0,
        3,
        10,
    )
    .unwrap();
    assert_eq!(
        p.run(10).unwrap(),
        vec![
            "0 start pub-a 1 5",
            "5 finish pub-a 1 ok",
            "5 start pub-b 1 5",
            "10 finish pub-b 1 ok"
        ]
    );
}

// R17 R18 R43
#[test]
fn public_retry() {
    let mut p = Pool::new(
        vec![Job {
            id: "pub-a".into(),
            priority: 1,
            dependencies: vec![],
            max_attempts: 2,
            release_at: 0,
            duration: 2,
            tag: None,
            failures: 1,
        }],
        1,
        0,
        3,
        10,
    )
    .unwrap();
    assert_eq!(
        p.run(7).unwrap(),
        vec![
            "0 start pub-a 1 2",
            "2 finish pub-a 1 fail",
            "2 retry pub-a 1 5",
            "5 start pub-a 2 2",
            "7 finish pub-a 2 ok"
        ]
    );
}

// R27 R29 R30 R31
#[test]
fn public_preempt() {
    let mut p = Pool::new(
        vec![
            Job {
                id: "pub-low".into(),
                priority: 1,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 0,
                duration: 10,
                tag: None,
                failures: 0,
            },
            Job {
                id: "pub-high".into(),
                priority: 8,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 3,
                duration: 2,
                tag: None,
                failures: 0,
            },
        ],
        1,
        0,
        3,
        10,
    )
    .unwrap();
    assert_eq!(
        p.run(12).unwrap(),
        vec![
            "0 start pub-low 1 10",
            "3 preempt pub-low by pub-high",
            "3 start pub-high 1 2",
            "5 finish pub-high 1 ok",
            "5 start pub-low 1 7",
            "12 finish pub-low 1 ok"
        ]
    );
}
