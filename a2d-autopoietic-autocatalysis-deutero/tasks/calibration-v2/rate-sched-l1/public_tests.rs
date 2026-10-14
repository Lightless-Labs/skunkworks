// R4 R6 R11 R15
#[test]
fn public_dispatch() {
    let mut s = Scheduler::new(0);
    s.insert(
        0,
        Job {
            id: "pub-a".into(),
            ready_at: 0,
            priority: 1,
        },
    )
    .unwrap();
    assert_eq!(
        s.next_ready(0).unwrap().unwrap(),
        Dispatch {
            id: "pub-a".into(),
            effective_priority: 1
        }
    );
}

// R7 R8 R16
#[test]
fn public_cancel() {
    let mut s = Scheduler::new(10);
    s.insert(
        0,
        Job {
            id: "pub-a".into(),
            ready_at: 0,
            priority: 1,
        },
    )
    .unwrap();
    s.cancel(1, "pub-a").unwrap();
    assert_eq!(s.next_ready(2), Ok(None));
}

// R12 R14
#[test]
fn public_priority() {
    let mut s = Scheduler::new(0);
    s.insert(
        0,
        Job {
            id: "pub-a".into(),
            ready_at: 0,
            priority: 1,
        },
    )
    .unwrap();
    s.insert(
        0,
        Job {
            id: "pub-b".into(),
            ready_at: 0,
            priority: 7,
        },
    )
    .unwrap();
    assert_eq!(s.next_ready(0).unwrap().unwrap().id, "pub-b");
}

// R13 R17
#[test]
fn public_aging() {
    let mut s = Scheduler::new(10);
    s.insert(
        0,
        Job {
            id: "pub-a".into(),
            ready_at: 10,
            priority: 2,
        },
    )
    .unwrap();
    assert_eq!(s.next_ready(40).unwrap().unwrap().effective_priority, 5);
}
