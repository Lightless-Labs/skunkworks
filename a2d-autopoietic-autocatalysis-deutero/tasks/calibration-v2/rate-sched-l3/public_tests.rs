// R11 R19 R23 R31
#[test]
fn public_cycle() {
    let mut r = Runner::new(
        vec![Job {
            id: "pub-a".into(),
            priority: 1,
            dependencies: vec![],
            max_attempts: 2,
        }],
        0,
        5,
        20,
    )
    .unwrap();
    r.begin_cycle(0).unwrap();
    assert_eq!(
        r.next_ready(0).unwrap(),
        Some(Ticket {
            id: "pub-a".into(),
            attempt: 1
        })
    );
    r.complete(1, "pub-a", true).unwrap();
    assert_eq!(r.status("pub-a"), Ok(State::Succeeded));
    r.begin_cycle(2).unwrap();
}

// R14 R27
#[test]
fn public_dependency() {
    let mut r = Runner::new(
        vec![
            Job {
                id: "pub-child".into(),
                priority: 9,
                dependencies: vec!["pub-root".into()],
                max_attempts: 2,
            },
            Job {
                id: "pub-root".into(),
                priority: 1,
                dependencies: vec![],
                max_attempts: 2,
            },
        ],
        0,
        5,
        20,
    )
    .unwrap();
    r.begin_cycle(0).unwrap();
    assert_eq!(r.next_ready(0).unwrap().unwrap().id, "pub-root");
    r.complete(0, "pub-root", true).unwrap();
    assert_eq!(r.next_ready(0).unwrap().unwrap().id, "pub-child");
}

// R25 R26
#[test]
fn public_retry() {
    let mut r = Runner::new(
        vec![Job {
            id: "pub-a".into(),
            priority: 1,
            dependencies: vec![],
            max_attempts: 2,
        }],
        0,
        5,
        20,
    )
    .unwrap();
    r.begin_cycle(0).unwrap();
    r.next_ready(0).unwrap();
    r.complete(1, "pub-a", false).unwrap();
    assert_eq!(r.status("pub-a"), Ok(State::Waiting { ready_at: 6 }));
    assert_eq!(r.next_ready(5), Ok(None));
    assert_eq!(r.next_ready(6).unwrap().unwrap().attempt, 2);
}

// R24 R28
#[test]
fn public_failure() {
    let mut r = Runner::new(
        vec![
            Job {
                id: "pub-root".into(),
                priority: 1,
                dependencies: vec![],
                max_attempts: 1,
            },
            Job {
                id: "pub-leaf".into(),
                priority: 1,
                dependencies: vec!["pub-root".into()],
                max_attempts: 2,
            },
        ],
        0,
        5,
        20,
    )
    .unwrap();
    r.begin_cycle(0).unwrap();
    r.next_ready(0).unwrap();
    r.complete(0, "pub-root", false).unwrap();
    assert_eq!(r.status("pub-root"), Ok(State::Failed));
    assert_eq!(r.status("pub-leaf"), Ok(State::Blocked));
}
