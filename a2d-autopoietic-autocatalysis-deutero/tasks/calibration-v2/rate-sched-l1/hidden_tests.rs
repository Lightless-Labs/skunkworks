// R1 R16
#[test]
fn hidden_empty_clock() {
    let mut s = Scheduler::new(1);
    assert_eq!(s.next_ready(12), Ok(None));
    assert_eq!(s.next_ready(11), Err(Error::TimeReversed));
}

// R1 R3 R4
#[test]
fn hidden_precedence() {
    let mut s = Scheduler::new(0);
    s.next_ready(8).unwrap();
    assert_eq!(
        s.insert(
            7,
            Job {
                id: "!".into(),
                ready_at: 0,
                priority: 255
            }
        ),
        Err(Error::TimeReversed)
    );
    assert_eq!(
        s.insert(
            8,
            Job {
                id: "!".into(),
                ready_at: 0,
                priority: 255
            }
        ),
        Err(Error::BadId)
    );
}

// R2 R3
#[test]
fn hidden_failed_clock() {
    let mut s = Scheduler::new(0);
    assert_eq!(
        s.insert(
            100,
            Job {
                id: "UP".into(),
                ready_at: 0,
                priority: 1
            }
        ),
        Err(Error::BadId)
    );
    assert_eq!(s.next_ready(0), Ok(None));
}

// R3
#[test]
fn hidden_ids() {
    let mut s = Scheduler::new(0);
    for id in ["", "é", "abcdefghijklmnopq", "a_b", "A"] {
        assert_eq!(
            s.insert(
                0,
                Job {
                    id: id.into(),
                    ready_at: 0,
                    priority: 0
                }
            ),
            Err(Error::BadId)
        );
    }
    s.insert(
        0,
        Job {
            id: "abcdefghijklmnop".into(),
            ready_at: 0,
            priority: 1,
        },
    )
    .unwrap();
    s.insert(
        0,
        Job {
            id: "-09".into(),
            ready_at: 0,
            priority: 1,
        },
    )
    .unwrap();
}

// R3 R4 R5
#[test]
fn hidden_priority_duplicate() {
    let mut s = Scheduler::new(0);
    s.insert(
        0,
        Job {
            id: "a".into(),
            ready_at: 0,
            priority: 1,
        },
    )
    .unwrap();
    assert_eq!(
        s.insert(
            0,
            Job {
                id: "a".into(),
                ready_at: 0,
                priority: 10
            }
        ),
        Err(Error::BadPriority)
    );
    assert_eq!(
        s.insert(
            0,
            Job {
                id: "a".into(),
                ready_at: 0,
                priority: 1
            }
        ),
        Err(Error::Duplicate)
    );
}

// R5 R8 R9
#[test]
fn hidden_cancel_reservation() {
    let mut s = Scheduler::new(0);
    s.insert(
        0,
        Job {
            id: "a".into(),
            ready_at: 0,
            priority: 1,
        },
    )
    .unwrap();
    s.cancel(0, "a").unwrap();
    assert_eq!(
        s.insert(
            0,
            Job {
                id: "a".into(),
                ready_at: 0,
                priority: 1
            }
        ),
        Err(Error::Duplicate)
    );
    assert_eq!(s.cancel(0, "a"), Err(Error::Cancelled));
}

// R5 R10 R15
#[test]
fn hidden_complete_reservation() {
    let mut s = Scheduler::new(0);
    s.insert(
        0,
        Job {
            id: "a".into(),
            ready_at: 0,
            priority: 1,
        },
    )
    .unwrap();
    s.next_ready(0).unwrap();
    assert_eq!(
        s.insert(
            0,
            Job {
                id: "a".into(),
                ready_at: 0,
                priority: 1
            }
        ),
        Err(Error::Duplicate)
    );
    assert_eq!(s.cancel(0, "a"), Err(Error::Completed));
}

// R2 R7
#[test]
fn hidden_unknown() {
    let mut s = Scheduler::new(1);
    assert_eq!(s.cancel(90, "!"), Err(Error::Unknown));
    assert_eq!(s.next_ready(1), Ok(None));
}

// R8 R16
#[test]
fn hidden_future_cancel() {
    let mut s = Scheduler::new(2);
    s.insert(
        1,
        Job {
            id: "z".into(),
            ready_at: 900,
            priority: 9,
        },
    )
    .unwrap();
    s.cancel(2, "z").unwrap();
    assert_eq!(s.next_ready(1000), Ok(None));
}

// R11
#[test]
fn hidden_boundary() {
    let mut s = Scheduler::new(0);
    s.insert(
        0,
        Job {
            id: "edge".into(),
            ready_at: 23,
            priority: 8,
        },
    )
    .unwrap();
    assert_eq!(s.next_ready(22), Ok(None));
    assert_eq!(s.next_ready(23).unwrap().unwrap().id, "edge");
}

// R12
#[test]
fn hidden_no_aging() {
    let mut s = Scheduler::new(0);
    s.insert(
        0,
        Job {
            id: "z".into(),
            ready_at: 0,
            priority: 0,
        },
    )
    .unwrap();
    assert_eq!(
        s.next_ready(u64::MAX).unwrap().unwrap().effective_priority,
        0
    );
}

// R13
#[test]
fn hidden_age_boundary() {
    for (now, p) in [(49, 4), (50, 5)] {
        let mut s = Scheduler::new(10);
        s.insert(
            0,
            Job {
                id: "edge".into(),
                ready_at: 10,
                priority: 1,
            },
        )
        .unwrap();
        assert_eq!(s.next_ready(now).unwrap().unwrap().effective_priority, p);
    }
}

// R13
#[test]
fn hidden_saturation() {
    let mut s = Scheduler::new(1);
    s.insert(
        0,
        Job {
            id: "old".into(),
            ready_at: 0,
            priority: 9,
        },
    )
    .unwrap();
    assert_eq!(
        s.next_ready(u64::MAX).unwrap().unwrap().effective_priority,
        9
    );
}

// R14
#[test]
fn hidden_fifo_over_ready() {
    let mut s = Scheduler::new(0);
    s.insert(
        0,
        Job {
            id: "first".into(),
            ready_at: 20,
            priority: 4,
        },
    )
    .unwrap();
    s.insert(
        0,
        Job {
            id: "second".into(),
            ready_at: 0,
            priority: 4,
        },
    )
    .unwrap();
    assert_eq!(s.next_ready(20).unwrap().unwrap().id, "first");
}

// R13 R14
#[test]
fn hidden_fifo_over_base() {
    let mut s = Scheduler::new(10);
    s.insert(
        0,
        Job {
            id: "low".into(),
            ready_at: 0,
            priority: 0,
        },
    )
    .unwrap();
    s.insert(
        0,
        Job {
            id: "high".into(),
            ready_at: 80,
            priority: 8,
        },
    )
    .unwrap();
    assert_eq!(s.next_ready(90).unwrap().unwrap().id, "low");
}

// R6 R17
#[test]
fn hidden_insert_past() {
    let mut s = Scheduler::new(4);
    s.insert(
        100,
        Job {
            id: "past".into(),
            ready_at: 4,
            priority: 1,
        },
    )
    .unwrap();
    assert_eq!(s.next_ready(100).unwrap().unwrap().effective_priority, 9);
}

// R6 R11
#[test]
fn hidden_max_time() {
    let mut s = Scheduler::new(1);
    s.insert(
        0,
        Job {
            id: "last".into(),
            ready_at: u64::MAX,
            priority: 3,
        },
    )
    .unwrap();
    assert_eq!(s.next_ready(u64::MAX - 1), Ok(None));
    assert_eq!(
        s.next_ready(u64::MAX).unwrap().unwrap().effective_priority,
        3
    );
}

// R14 R15
#[test]
fn hidden_drain() {
    let mut s = Scheduler::new(0);
    for i in 0..200 {
        s.insert(
            0,
            Job {
                id: format!("j{i}"),
                ready_at: 0,
                priority: (i % 10) as u8,
            },
        )
        .unwrap();
    }
    let mut got = Vec::new();
    for _ in 0..200 {
        got.push(s.next_ready(0).unwrap().unwrap().id);
    }
    let mut expected: Vec<_> = (0..200).collect();
    expected.sort_by_key(|i| (std::cmp::Reverse(i % 10), *i));
    assert_eq!(
        got,
        expected.iter().map(|i| format!("j{i}")).collect::<Vec<_>>()
    );
    assert_eq!(s.next_ready(0), Ok(None));
}

// R18
#[test]
fn hidden_independent() {
    let mut s = Scheduler::new(0);
    let mut q = Scheduler::new(0);
    s.insert(
        20,
        Job {
            id: "a".into(),
            ready_at: 0,
            priority: 1,
        },
    )
    .unwrap();
    q.insert(
        0,
        Job {
            id: "a".into(),
            ready_at: 0,
            priority: 1,
        },
    )
    .unwrap();
    assert!(q.next_ready(0).unwrap().is_some());
}

// R1 R8
#[test]
fn hidden_cancel_clock() {
    let mut s = Scheduler::new(0);
    s.insert(
        0,
        Job {
            id: "a".into(),
            ready_at: 0,
            priority: 1,
        },
    )
    .unwrap();
    s.cancel(30, "a").unwrap();
    assert_eq!(s.next_ready(29), Err(Error::TimeReversed));
}

// R1 R7
#[test]
fn hidden_cancel_time_precedence() {
    let mut s = Scheduler::new(0);
    s.next_ready(8).unwrap();
    assert_eq!(s.cancel(7, "missing"), Err(Error::TimeReversed));
}

// R2 R14
#[test]
fn hidden_error_no_sequence() {
    let mut s = Scheduler::new(0);
    s.insert(
        0,
        Job {
            id: "left".into(),
            ready_at: 0,
            priority: 4,
        },
    )
    .unwrap();
    s.insert(
        50,
        Job {
            id: "!".into(),
            ready_at: 0,
            priority: 4,
        },
    )
    .unwrap_err();
    s.insert(
        0,
        Job {
            id: "right".into(),
            ready_at: 0,
            priority: 4,
        },
    )
    .unwrap();
    assert_eq!(s.next_ready(0).unwrap().unwrap().id, "left");
}

// R11 R17
#[test]
fn hidden_not_insert_age() {
    let mut s = Scheduler::new(10);
    s.insert(
        0,
        Job {
            id: "later".into(),
            ready_at: 100,
            priority: 1,
        },
    )
    .unwrap();
    assert_eq!(s.next_ready(99), Ok(None));
    assert_eq!(s.next_ready(100).unwrap().unwrap().effective_priority, 1);
}

// R2 R9
#[test]
fn hidden_failed_cancel_clock() {
    let mut s = Scheduler::new(0);
    s.insert(
        0,
        Job {
            id: "a".into(),
            ready_at: 0,
            priority: 1,
        },
    )
    .unwrap();
    s.cancel(0, "a").unwrap();
    assert_eq!(s.cancel(100, "a"), Err(Error::Cancelled));
    assert_eq!(s.next_ready(0), Ok(None));
}
