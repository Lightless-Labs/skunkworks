// R2
#[test]
fn hidden_config() {
    assert!(matches!(Runner::new(vec![], 0, 0, 0), Err(Error::Config)));
    assert!(matches!(Runner::new(vec![], 0, 9, 8), Err(Error::Config)));
}

// R3 R4
#[test]
fn hidden_local_validation() {
    let mut a = Job {
        id: "!".into(),
        priority: 1,
        dependencies: vec![],
        max_attempts: 2,
    };
    a.priority = 99;
    a.max_attempts = 0;
    assert!(matches!(
        Runner::new(vec![a], 0, 1, 1),
        Err(Error::BadId(0))
    ));
    let mut b = Job {
        id: "a".into(),
        priority: 1,
        dependencies: vec![],
        max_attempts: 2,
    };
    b.priority = 10;
    b.max_attempts = 0;
    assert!(matches!(
        Runner::new(vec![b], 0, 1, 1),
        Err(Error::BadPriority(0))
    ));
    for n in [0, 33] {
        let mut j = Job {
            id: "a".into(),
            priority: 1,
            dependencies: vec![],
            max_attempts: 2,
        };
        j.max_attempts = n;
        assert!(matches!(
            Runner::new(vec![j], 0, 1, 1),
            Err(Error::BadAttempts(0))
        ));
    }
}

// R3
#[test]
fn hidden_duplicate() {
    assert!(matches!(
        Runner::new(
            vec![
                Job {
                    id: "a".into(),
                    priority: 1,
                    dependencies: vec![],
                    max_attempts: 2
                },
                Job {
                    id: "a".into(),
                    priority: 1,
                    dependencies: vec![],
                    max_attempts: 2
                }
            ],
            0,
            1,
            1
        ),
        Err(Error::Duplicate(1))
    ));
}

// R5
#[test]
fn hidden_dependency_errors() {
    assert!(matches!(
        Runner::new(
            vec![
                Job {
                    id: "a".into(),
                    priority: 1,
                    dependencies: vec!["b".into(), "b".into()],
                    max_attempts: 2
                },
                Job {
                    id: "b".into(),
                    priority: 1,
                    dependencies: vec![],
                    max_attempts: 2
                }
            ],
            0,
            1,
            1
        ),
        Err(Error::DuplicateDependency(0, 1))
    ));
    assert!(matches!(
        Runner::new(
            vec![Job {
                id: "a".into(),
                priority: 1,
                dependencies: vec!["!".into()],
                max_attempts: 2
            }],
            0,
            1,
            1
        ),
        Err(Error::MissingDependency(0, 0))
    ));
}

// R3 R5
#[test]
fn hidden_validation_passes() {
    assert!(matches!(
        Runner::new(
            vec![
                Job {
                    id: "a".into(),
                    priority: 1,
                    dependencies: vec!["missing".into()],
                    max_attempts: 2
                },
                Job {
                    id: "!".into(),
                    priority: 1,
                    dependencies: vec![],
                    max_attempts: 2
                }
            ],
            0,
            1,
            1
        ),
        Err(Error::BadId(1))
    ));
}

// R6
#[test]
fn hidden_cycles() {
    assert!(matches!(
        Runner::new(
            vec![Job {
                id: "a".into(),
                priority: 1,
                dependencies: vec!["a".into()],
                max_attempts: 2
            }],
            0,
            1,
            1
        ),
        Err(Error::Cycle)
    ));
    assert!(matches!(
        Runner::new(
            vec![
                Job {
                    id: "a".into(),
                    priority: 1,
                    dependencies: vec!["b".into()],
                    max_attempts: 2
                },
                Job {
                    id: "b".into(),
                    priority: 1,
                    dependencies: vec!["a".into()],
                    max_attempts: 2
                }
            ],
            0,
            1,
            1
        ),
        Err(Error::Cycle)
    ));
}

// R1 R7 R9
#[test]
fn hidden_no_cycle() {
    let mut r = Runner::new(
        vec![Job {
            id: "a".into(),
            priority: 1,
            dependencies: vec![],
            max_attempts: 2,
        }],
        0,
        5,
        20,
    )
    .unwrap();
    assert_eq!(r.status("?"), Err(Error::NoCycle));
    assert_eq!(r.next_ready(100), Err(Error::NoCycle));
    assert_eq!(r.complete(100, "?", true), Err(Error::NoCycle));
    r.begin_cycle(0).unwrap();
}

// R7 R8 R10
#[test]
fn hidden_busy() {
    let mut r = Runner::new(
        vec![Job {
            id: "a".into(),
            priority: 1,
            dependencies: vec![],
            max_attempts: 2,
        }],
        0,
        5,
        20,
    )
    .unwrap();
    r.begin_cycle(2).unwrap();
    assert_eq!(r.begin_cycle(100), Err(Error::Busy));
    assert_eq!(r.begin_cycle(1), Err(Error::TimeReversed));
    assert_eq!(r.next_ready(2).unwrap().unwrap().attempt, 1);
    assert_eq!(r.begin_cycle(2), Err(Error::Busy));
}

// R2 R8 R12
#[test]
fn hidden_empty() {
    let mut r = Runner::new(vec![], 0, 1, 1).unwrap();
    r.begin_cycle(10).unwrap();
    r.begin_cycle(10).unwrap();
    assert_eq!(r.next_ready(12), Ok(None));
    assert_eq!(r.begin_cycle(11), Err(Error::TimeReversed));
}

// R13 R32
#[test]
fn hidden_status_snapshot() {
    let mut r = Runner::new(
        vec![Job {
            id: "a".into(),
            priority: 1,
            dependencies: vec![],
            max_attempts: 2,
        }],
        0,
        5,
        20,
    )
    .unwrap();
    r.begin_cycle(4).unwrap();
    let old = r.status("a").unwrap();
    assert_eq!(r.status("!"), Err(Error::Unknown));
    r.next_ready(4).unwrap();
    assert_eq!(old, State::Waiting { ready_at: 4 });
    assert_eq!(r.status("a"), Ok(State::Running { attempt: 1 }));
}

// R14 R15 R21
#[test]
fn hidden_all_dependencies() {
    let mut r = Runner::new(
        vec![
            Job {
                id: "c".into(),
                priority: 9,
                dependencies: vec!["a".into(), "b".into()],
                max_attempts: 2,
            },
            Job {
                id: "a".into(),
                priority: 1,
                dependencies: vec![],
                max_attempts: 2,
            },
            Job {
                id: "b".into(),
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
    assert_eq!(r.next_ready(0).unwrap().unwrap().id, "a");
    assert_eq!(r.next_ready(0).unwrap().unwrap().id, "b");
    r.complete(0, "a", true).unwrap();
    assert_eq!(r.next_ready(0), Ok(None));
    r.complete(0, "b", true).unwrap();
    assert_eq!(r.next_ready(0).unwrap().unwrap().id, "c");
}

// R14 R25 R27
#[test]
fn hidden_retry_gate() {
    let mut r = Runner::new(
        vec![
            Job {
                id: "a".into(),
                priority: 1,
                dependencies: vec![],
                max_attempts: 2,
            },
            Job {
                id: "b".into(),
                priority: 9,
                dependencies: vec!["a".into()],
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
    r.complete(0, "a", false).unwrap();
    assert_eq!(r.next_ready(4), Ok(None));
    assert_eq!(r.next_ready(5).unwrap().unwrap().id, "a");
    assert_eq!(r.next_ready(5), Ok(None));
    r.complete(5, "a", true).unwrap();
    assert_eq!(r.next_ready(5).unwrap().unwrap().id, "b");
}

// R16 R18
#[test]
fn hidden_aging() {
    let mut r = Runner::new(
        vec![
            Job {
                id: "old".into(),
                priority: 1,
                dependencies: vec![],
                max_attempts: 2,
            },
            Job {
                id: "high".into(),
                priority: 8,
                dependencies: vec![],
                max_attempts: 2,
            },
        ],
        10,
        5,
        20,
    )
    .unwrap();
    r.begin_cycle(0).unwrap();
    assert_eq!(r.next_ready(80).unwrap().unwrap().id, "old");
}

// R16 R17
#[test]
fn hidden_dependency_aging() {
    let mut r = Runner::new(
        vec![
            Job {
                id: "root".into(),
                priority: 9,
                dependencies: vec![],
                max_attempts: 2,
            },
            Job {
                id: "child".into(),
                priority: 0,
                dependencies: vec!["root".into()],
                max_attempts: 2,
            },
            Job {
                id: "other".into(),
                priority: 8,
                dependencies: vec![],
                max_attempts: 2,
            },
        ],
        10,
        5,
        20,
    )
    .unwrap();
    r.begin_cycle(0).unwrap();
    assert_eq!(r.next_ready(0).unwrap().unwrap().id, "root");
    r.complete(100, "root", true).unwrap();
    assert_eq!(r.next_ready(100).unwrap().unwrap().id, "child");
}

// R18
#[test]
fn hidden_tie_order() {
    let mut r = Runner::new(
        vec![
            Job {
                id: "z".into(),
                priority: 3,
                dependencies: vec![],
                max_attempts: 2,
            },
            Job {
                id: "a".into(),
                priority: 3,
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
    assert_eq!(r.next_ready(0).unwrap().unwrap().id, "z");
}

// R7 R8 R22
#[test]
fn hidden_complete_errors() {
    let mut r = Runner::new(
        vec![Job {
            id: "a".into(),
            priority: 1,
            dependencies: vec![],
            max_attempts: 2,
        }],
        0,
        5,
        20,
    )
    .unwrap();
    r.begin_cycle(5).unwrap();
    assert_eq!(r.complete(4, "!", true), Err(Error::TimeReversed));
    assert_eq!(r.complete(99, "!", true), Err(Error::Unknown));
    assert_eq!(r.complete(99, "a", true), Err(Error::NotRunning));
    r.next_ready(5).unwrap();
    r.complete(5, "a", true).unwrap();
    assert_eq!(r.complete(5, "a", true), Err(Error::NotRunning));
}

// R19 R24 R25 R26
#[test]
fn hidden_exponential_cap() {
    let mut r = Runner::new(
        vec![Job {
            id: "a".into(),
            priority: 1,
            dependencies: vec![],
            max_attempts: 5,
        }],
        0,
        3,
        7,
    )
    .unwrap();
    r.begin_cycle(0).unwrap();
    let mut now = 0;
    for (attempt, delay) in [(1, 3), (2, 6), (3, 7), (4, 7)] {
        assert_eq!(r.next_ready(now).unwrap().unwrap().attempt, attempt);
        r.complete(now, "a", false).unwrap();
        assert_eq!(
            r.status("a"),
            Ok(State::Waiting {
                ready_at: now + delay
            })
        );
        now += delay;
    }
    assert_eq!(r.next_ready(now).unwrap().unwrap().attempt, 5);
    r.complete(now, "a", false).unwrap();
    assert_eq!(r.status("a"), Ok(State::Failed));
}

// R25 R26
#[test]
fn hidden_saturation() {
    let mut r = Runner::new(
        vec![Job {
            id: "a".into(),
            priority: 1,
            dependencies: vec![],
            max_attempts: 3,
        }],
        0,
        u64::MAX,
        u64::MAX,
    )
    .unwrap();
    r.begin_cycle(1).unwrap();
    r.next_ready(1).unwrap();
    r.complete(1, "a", false).unwrap();
    assert_eq!(r.status("a"), Ok(State::Waiting { ready_at: u64::MAX }));
    r.next_ready(u64::MAX).unwrap();
    r.complete(u64::MAX, "a", false).unwrap();
    assert_eq!(r.next_ready(u64::MAX).unwrap().unwrap().attempt, 3);
}

// R28 R29
#[test]
fn hidden_transitive() {
    let mut r = Runner::new(
        vec![
            Job {
                id: "c".into(),
                priority: 1,
                dependencies: vec!["b".into()],
                max_attempts: 2,
            },
            Job {
                id: "b".into(),
                priority: 1,
                dependencies: vec!["a".into()],
                max_attempts: 2,
            },
            Job {
                id: "a".into(),
                priority: 1,
                dependencies: vec![],
                max_attempts: 1,
            },
            Job {
                id: "free".into(),
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
    r.next_ready(0).unwrap();
    r.complete(0, "a", false).unwrap();
    assert_eq!(r.status("b"), Ok(State::Blocked));
    assert_eq!(r.status("c"), Ok(State::Blocked));
    assert_eq!(r.next_ready(0).unwrap().unwrap().id, "free");
}

// R11 R30 R31
#[test]
fn hidden_terminal_reset() {
    let mut r = Runner::new(
        vec![
            Job {
                id: "a".into(),
                priority: 1,
                dependencies: vec![],
                max_attempts: 1,
            },
            Job {
                id: "b".into(),
                priority: 1,
                dependencies: vec!["a".into()],
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
    r.complete(0, "a", false).unwrap();
    assert_eq!(r.next_ready(1), Ok(None));
    r.begin_cycle(1).unwrap();
    assert_eq!(r.status("b"), Ok(State::Waiting { ready_at: 1 }));
    assert_eq!(r.next_ready(1).unwrap().unwrap().attempt, 1);
    r.complete(1, "a", true).unwrap();
    assert_eq!(r.next_ready(1).unwrap().unwrap().id, "b");
}

// R16 R17
#[test]
fn hidden_retry_age_reset() {
    let mut r = Runner::new(
        vec![
            Job {
                id: "a".into(),
                priority: 0,
                dependencies: vec![],
                max_attempts: 2,
            },
            Job {
                id: "b".into(),
                priority: 3,
                dependencies: vec![],
                max_attempts: 2,
            },
        ],
        10,
        5,
        20,
    )
    .unwrap();
    r.begin_cycle(0).unwrap();
    assert_eq!(r.next_ready(100).unwrap().unwrap().id, "a");
    r.complete(100, "a", false).unwrap();
    assert_eq!(r.next_ready(105).unwrap().unwrap().id, "b");
}

// R8 R20
#[test]
fn hidden_no_auto_time() {
    let mut r = Runner::new(
        vec![Job {
            id: "a".into(),
            priority: 1,
            dependencies: vec![],
            max_attempts: 2,
        }],
        0,
        5,
        20,
    )
    .unwrap();
    r.begin_cycle(2).unwrap();
    r.next_ready(2).unwrap();
    r.complete(2, "a", false).unwrap();
    assert_eq!(r.next_ready(6), Ok(None));
    assert_eq!(r.next_ready(5), Err(Error::TimeReversed));
    assert_eq!(r.status("a"), Ok(State::Waiting { ready_at: 7 }));
}

// R6 R14 R19 R31
#[test]
fn hidden_larger_dag() {
    let jobs = (0..80)
        .map(|i| Job {
            id: format!("n{i}"),
            priority: 9,
            dependencies: if i == 0 {
                vec![]
            } else {
                vec![format!("n{}", i - 1)]
            },
            max_attempts: 1,
        })
        .collect();
    let mut r = Runner::new(jobs, 0, 1, 1).unwrap();
    r.begin_cycle(0).unwrap();
    for i in 0..80 {
        let tk = r.next_ready(0).unwrap().unwrap();
        assert_eq!(tk.id, format!("n{i}"));
        r.complete(0, &tk.id, true).unwrap();
    }
    assert_eq!(r.next_ready(0), Ok(None));
    r.begin_cycle(0).unwrap();
}

// R23 R24
#[test]
fn hidden_success_last_attempt() {
    let mut r = Runner::new(
        vec![Job {
            id: "a".into(),
            priority: 1,
            dependencies: vec![],
            max_attempts: 1,
        }],
        0,
        5,
        20,
    )
    .unwrap();
    r.begin_cycle(0).unwrap();
    r.next_ready(0).unwrap();
    r.complete(0, "a", true).unwrap();
    assert_eq!(r.status("a"), Ok(State::Succeeded));
}
