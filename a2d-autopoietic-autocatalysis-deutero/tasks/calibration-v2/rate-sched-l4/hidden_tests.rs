// R2 R8
#[test]
fn hidden_config() {
    for (c, b, k) in [
        (0, 1, 1),
        (17, 1, 1),
        (1, 0, 1),
        (1, 5, 4),
        (1, 1, 1000000001),
    ] {
        assert!(matches!(Pool::new(vec![], c, 0, b, k), Err(Error::Config)));
    }
    let mut p = Pool::new(vec![], 1, 0, 1, 1).unwrap();
    assert!(p.run(100).unwrap().is_empty());
}

// R3 R4 R5 R6
#[test]
fn hidden_local_errors() {
    let mut j = Job {
        id: "!".into(),
        priority: 1,
        dependencies: vec![],
        max_attempts: 2,
        release_at: 0,
        duration: 5,
        tag: None,
        failures: 0,
    };
    j.priority = 99;
    assert!(matches!(
        Pool::new(vec![j], 1, 0, 1, 1),
        Err(Error::BadId(0))
    ));
    let mut j = Job {
        id: "a".into(),
        priority: 1,
        dependencies: vec![],
        max_attempts: 2,
        release_at: 0,
        duration: 5,
        tag: None,
        failures: 0,
    };
    j.priority = 10;
    j.max_attempts = 0;
    assert!(matches!(
        Pool::new(vec![j], 1, 0, 1, 1),
        Err(Error::BadPriority(0))
    ));
    let mut j = Job {
        id: "a".into(),
        priority: 1,
        dependencies: vec![],
        max_attempts: 2,
        release_at: 0,
        duration: 5,
        tag: None,
        failures: 0,
    };
    j.max_attempts = 33;
    j.duration = 0;
    assert!(matches!(
        Pool::new(vec![j], 1, 0, 1, 1),
        Err(Error::BadAttempts(0))
    ));
    for (release, duration) in [(0, 0), (1000000001, 1), (0, 1000000001)] {
        let mut j = Job {
            id: "a".into(),
            priority: 1,
            dependencies: vec![],
            max_attempts: 2,
            release_at: 0,
            duration: 5,
            tag: None,
            failures: 0,
        };
        j.release_at = release;
        j.duration = duration;
        assert!(matches!(
            Pool::new(vec![j], 1, 0, 1, 1),
            Err(Error::BadTime(0))
        ));
    }
    for tag in ["", "UP", "é", "123456789"] {
        let mut j = Job {
            id: "a".into(),
            priority: 1,
            dependencies: vec![],
            max_attempts: 2,
            release_at: 0,
            duration: 5,
            tag: None,
            failures: 0,
        };
        j.tag = Some(tag.into());
        assert!(matches!(
            Pool::new(vec![j], 1, 0, 1, 1),
            Err(Error::BadTag(0))
        ));
    }
}

// R3 R7 R8
#[test]
fn hidden_graph_validation() {
    assert!(matches!(
        Pool::new(
            vec![
                Job {
                    id: "a".into(),
                    priority: 1,
                    dependencies: vec![],
                    max_attempts: 2,
                    release_at: 0,
                    duration: 5,
                    tag: None,
                    failures: 0
                },
                Job {
                    id: "a".into(),
                    priority: 1,
                    dependencies: vec![],
                    max_attempts: 2,
                    release_at: 0,
                    duration: 5,
                    tag: None,
                    failures: 0
                }
            ],
            1,
            0,
            1,
            1
        ),
        Err(Error::Duplicate(1))
    ));
    assert!(matches!(
        Pool::new(
            vec![
                Job {
                    id: "a".into(),
                    priority: 1,
                    dependencies: vec!["b".into(), "b".into()],
                    max_attempts: 2,
                    release_at: 0,
                    duration: 5,
                    tag: None,
                    failures: 0
                },
                Job {
                    id: "b".into(),
                    priority: 1,
                    dependencies: vec![],
                    max_attempts: 2,
                    release_at: 0,
                    duration: 5,
                    tag: None,
                    failures: 0
                }
            ],
            1,
            0,
            1,
            1
        ),
        Err(Error::DuplicateDependency(0, 1))
    ));
    assert!(matches!(
        Pool::new(
            vec![Job {
                id: "a".into(),
                priority: 1,
                dependencies: vec!["?".into()],
                max_attempts: 2,
                release_at: 0,
                duration: 5,
                tag: None,
                failures: 0
            }],
            1,
            0,
            1,
            1
        ),
        Err(Error::MissingDependency(0, 0))
    ));
    assert!(matches!(
        Pool::new(
            vec![Job {
                id: "a".into(),
                priority: 1,
                dependencies: vec!["a".into()],
                max_attempts: 2,
                release_at: 0,
                duration: 5,
                tag: None,
                failures: 0
            }],
            1,
            0,
            1,
            1
        ),
        Err(Error::Cycle)
    ));
    assert!(matches!(
        Pool::new(
            vec![
                Job {
                    id: "a".into(),
                    priority: 1,
                    dependencies: vec!["b".into()],
                    max_attempts: 2,
                    release_at: 0,
                    duration: 5,
                    tag: None,
                    failures: 0
                },
                Job {
                    id: "b".into(),
                    priority: 1,
                    dependencies: vec!["a".into()],
                    max_attempts: 2,
                    release_at: 0,
                    duration: 5,
                    tag: None,
                    failures: 0
                }
            ],
            1,
            0,
            1,
            1
        ),
        Err(Error::Cycle)
    ));
    assert!(matches!(
        Pool::new(
            vec![
                Job {
                    id: "a".into(),
                    priority: 1,
                    dependencies: vec!["missing".into()],
                    max_attempts: 2,
                    release_at: 0,
                    duration: 5,
                    tag: None,
                    failures: 0
                },
                Job {
                    id: "!".into(),
                    priority: 1,
                    dependencies: vec![],
                    max_attempts: 2,
                    release_at: 0,
                    duration: 5,
                    tag: None,
                    failures: 0
                }
            ],
            1,
            0,
            1,
            1
        ),
        Err(Error::BadId(1))
    ));
}

// R1 R9 R10 R11 R39
#[test]
fn hidden_atomic_horizon() {
    let mut p = Pool::new(
        vec![Job {
            id: "a".into(),
            priority: 1,
            dependencies: vec![],
            max_attempts: 2,
            release_at: 20,
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
    assert_eq!(p.status("!"), Err(Error::Unknown));
    assert_eq!(p.run(1000000001), Err(Error::TimeRange));
    assert!(p.run(10).unwrap().is_empty());
    assert_eq!(p.run(9), Err(Error::TimeReversed));
    assert_eq!(
        p.status("a"),
        Ok(State::Waiting {
            ready_at: 20,
            attempt: 0
        })
    );
    assert_eq!(p.run(20).unwrap(), vec!["20 start a 1 5"]);
}

// R12 R13 R32 R34
#[test]
fn hidden_closed_boundary() {
    let mut p = Pool::new(
        vec![
            Job {
                id: "a".into(),
                priority: 3,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 0,
                duration: 7,
                tag: None,
                failures: 0,
            },
            Job {
                id: "b".into(),
                priority: 3,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 0,
                duration: 1,
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
    assert_eq!(p.run(0).unwrap(), vec!["0 start a 1 7"]);
    assert!(p.run(0).unwrap().is_empty());
    assert_eq!(p.run(7).unwrap(), vec!["7 finish a 1 ok", "7 start b 1 1"]);
    assert!(p.run(7).unwrap().is_empty());
    assert_eq!(p.run(8).unwrap(), vec!["8 finish b 1 ok"]);
}

// R14 R38 R40
#[test]
fn hidden_completion_order() {
    let mut p = Pool::new(
        vec![
            Job {
                id: "z".into(),
                priority: 4,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 0,
                duration: 4,
                tag: None,
                failures: 0,
            },
            Job {
                id: "a".into(),
                priority: 3,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 0,
                duration: 4,
                tag: None,
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
        p.run(4).unwrap(),
        vec![
            "0 start z 1 4",
            "0 start a 1 4",
            "4 finish a 1 ok",
            "4 finish z 1 ok"
        ]
    );
}

// R15 R16 R17 R18 R43
#[test]
fn hidden_retry_cap() {
    let mut p = Pool::new(
        vec![Job {
            id: "r".into(),
            priority: 1,
            dependencies: vec![],
            max_attempts: 4,
            release_at: 0,
            duration: 1,
            tag: None,
            failures: 3,
        }],
        1,
        0,
        2,
        3,
    )
    .unwrap();
    assert_eq!(
        p.run(12).unwrap(),
        vec![
            "0 start r 1 1",
            "1 finish r 1 fail",
            "1 retry r 1 3",
            "3 start r 2 1",
            "4 finish r 2 fail",
            "4 retry r 2 7",
            "7 start r 3 1",
            "8 finish r 3 fail",
            "8 retry r 3 11",
            "11 start r 4 1",
            "12 finish r 4 ok"
        ]
    );
}

// R16 R37 R43
#[test]
fn hidden_terminal_failure() {
    let mut p = Pool::new(
        vec![Job {
            id: "dead".into(),
            priority: 1,
            dependencies: vec![],
            max_attempts: 1,
            release_at: 0,
            duration: 1,
            tag: None,
            failures: 255,
        }],
        1,
        0,
        3,
        10,
    )
    .unwrap();
    assert_eq!(
        p.run(50).unwrap(),
        vec!["0 start dead 1 1", "1 finish dead 1 fail"]
    );
    assert_eq!(p.status("dead"), Ok(State::Failed));
    assert!(p.run(60).unwrap().is_empty());
}

// R19 R20 R21
#[test]
fn hidden_skip_closure() {
    let mut p = Pool::new(
        vec![
            Job {
                id: "root".into(),
                priority: 1,
                dependencies: vec![],
                max_attempts: 1,
                release_at: 0,
                duration: 2,
                tag: None,
                failures: 1,
            },
            Job {
                id: "z".into(),
                priority: 1,
                dependencies: vec!["root".into()],
                max_attempts: 2,
                release_at: 99,
                duration: 5,
                tag: None,
                failures: 0,
            },
            Job {
                id: "a".into(),
                priority: 1,
                dependencies: vec!["z".into()],
                max_attempts: 2,
                release_at: 0,
                duration: 5,
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
        p.run(2).unwrap(),
        vec![
            "0 start root 1 2",
            "2 finish root 1 fail",
            "2 skip a dependency",
            "2 skip z dependency"
        ]
    );
    assert_eq!(p.status("a"), Ok(State::Blocked));
}

// R14 R17 R19 R20
#[test]
fn hidden_simultaneous_failure() {
    let mut p = Pool::new(
        vec![
            Job {
                id: "z".into(),
                priority: 8,
                dependencies: vec![],
                max_attempts: 1,
                release_at: 0,
                duration: 1,
                tag: None,
                failures: 1,
            },
            Job {
                id: "a".into(),
                priority: 7,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 0,
                duration: 1,
                tag: None,
                failures: 1,
            },
            Job {
                id: "b".into(),
                priority: 1,
                dependencies: vec!["z".into()],
                max_attempts: 2,
                release_at: 0,
                duration: 5,
                tag: None,
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
        p.run(1).unwrap(),
        vec![
            "0 start z 1 1",
            "0 start a 1 1",
            "1 finish a 1 fail",
            "1 retry a 1 4",
            "1 finish z 1 fail",
            "1 skip b dependency"
        ]
    );
}

// R7 R21 R41
#[test]
fn hidden_dependency_retry() {
    let mut p = Pool::new(
        vec![
            Job {
                id: "child".into(),
                priority: 9,
                dependencies: vec!["parent".into()],
                max_attempts: 2,
                release_at: 0,
                duration: 1,
                tag: None,
                failures: 0,
            },
            Job {
                id: "parent".into(),
                priority: 1,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 0,
                duration: 1,
                tag: None,
                failures: 1,
            },
        ],
        2,
        0,
        3,
        10,
    )
    .unwrap();
    assert_eq!(
        p.run(6).unwrap(),
        vec![
            "0 start parent 1 1",
            "1 finish parent 1 fail",
            "1 retry parent 1 4",
            "4 start parent 2 1",
            "5 finish parent 2 ok",
            "5 start child 1 1",
            "6 finish child 1 ok"
        ]
    );
}

// R22 R23 R24 R40
#[test]
fn hidden_age_tie() {
    let mut p = Pool::new(
        vec![
            Job {
                id: "old".into(),
                priority: 7,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 0,
                duration: 1,
                tag: None,
                failures: 0,
            },
            Job {
                id: "root".into(),
                priority: 9,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 0,
                duration: 10,
                tag: None,
                failures: 0,
            },
            Job {
                id: "child".into(),
                priority: 8,
                dependencies: vec!["root".into()],
                max_attempts: 2,
                release_at: 0,
                duration: 1,
                tag: None,
                failures: 0,
            },
        ],
        1,
        1,
        3,
        10,
    )
    .unwrap();
    assert_eq!(
        p.run(12).unwrap(),
        vec![
            "0 start root 1 10",
            "10 finish root 1 ok",
            "10 start old 1 1",
            "11 finish old 1 ok",
            "11 start child 1 1",
            "12 finish child 1 ok"
        ]
    );
}

// R6 R24 R25 R41
#[test]
fn hidden_skip_unstartable() {
    let mut p = Pool::new(
        vec![
            Job {
                id: "holder".into(),
                priority: 9,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 0,
                duration: 10,
                tag: Some("db".into()),
                failures: 0,
            },
            Job {
                id: "wait".into(),
                priority: 8,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 0,
                duration: 1,
                tag: Some("db".into()),
                failures: 0,
            },
            Job {
                id: "free".into(),
                priority: 1,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 0,
                duration: 2,
                tag: None,
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
        p.run(11).unwrap(),
        vec![
            "0 start holder 1 10",
            "0 start free 1 2",
            "2 finish free 1 ok",
            "10 finish holder 1 ok",
            "10 start wait 1 1",
            "11 finish wait 1 ok"
        ]
    );
}

// R26 R27
#[test]
fn hidden_preempt_threshold() {
    let mut p = Pool::new(
        vec![
            Job {
                id: "low".into(),
                priority: 2,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 0,
                duration: 10,
                tag: None,
                failures: 0,
            },
            Job {
                id: "near".into(),
                priority: 3,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 1,
                duration: 1,
                tag: None,
                failures: 0,
            },
            Job {
                id: "edge".into(),
                priority: 4,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 2,
                duration: 1,
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
            "0 start low 1 10",
            "2 preempt low by edge",
            "2 start edge 1 1",
            "3 finish edge 1 ok",
            "3 start near 1 1",
            "4 finish near 1 ok",
            "4 start low 1 8",
            "12 finish low 1 ok"
        ]
    );
}

// R26 R27 R42
#[test]
fn hidden_tag_victim() {
    let mut p = Pool::new(
        vec![
            Job {
                id: "owner".into(),
                priority: 4,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 0,
                duration: 10,
                tag: Some("x".into()),
                failures: 0,
            },
            Job {
                id: "cheap".into(),
                priority: 0,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 0,
                duration: 10,
                tag: None,
                failures: 0,
            },
            Job {
                id: "want".into(),
                priority: 5,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 2,
                duration: 1,
                tag: Some("x".into()),
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
        p.run(11).unwrap(),
        vec![
            "0 start owner 1 10",
            "0 start cheap 1 10",
            "10 finish cheap 1 ok",
            "10 finish owner 1 ok",
            "10 start want 1 1",
            "11 finish want 1 ok"
        ]
    );
}

// R25 R29 R30 R42
#[test]
fn hidden_free_slot_tag_preempt() {
    let mut p = Pool::new(
        vec![
            Job {
                id: "owner".into(),
                priority: 0,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 0,
                duration: 6,
                tag: Some("x".into()),
                failures: 0,
            },
            Job {
                id: "want".into(),
                priority: 2,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 2,
                duration: 1,
                tag: Some("x".into()),
                failures: 0,
            },
        ],
        3,
        0,
        3,
        10,
    )
    .unwrap();
    assert_eq!(
        p.run(7).unwrap(),
        vec![
            "0 start owner 1 6",
            "2 preempt owner by want",
            "2 start want 1 1",
            "3 finish want 1 ok",
            "3 start owner 1 4",
            "7 finish owner 1 ok"
        ]
    );
}

// R28
#[test]
fn hidden_victim_lowest() {
    let mut p = Pool::new(
        vec![
            Job {
                id: "a".into(),
                priority: 3,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 0,
                duration: 10,
                tag: None,
                failures: 0,
            },
            Job {
                id: "z".into(),
                priority: 1,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 0,
                duration: 10,
                tag: None,
                failures: 0,
            },
            Job {
                id: "h".into(),
                priority: 7,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 2,
                duration: 1,
                tag: None,
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
        p.run(11).unwrap(),
        vec![
            "0 start a 1 10",
            "0 start z 1 10",
            "2 preempt z by h",
            "2 start h 1 1",
            "3 finish h 1 ok",
            "3 start z 1 8",
            "10 finish a 1 ok",
            "11 finish z 1 ok"
        ]
    );
}

// R28 R30 R31
#[test]
fn hidden_victim_latest() {
    let mut p = Pool::new(
        vec![
            Job {
                id: "a".into(),
                priority: 1,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 0,
                duration: 8,
                tag: None,
                failures: 0,
            },
            Job {
                id: "b".into(),
                priority: 1,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 1,
                duration: 8,
                tag: None,
                failures: 0,
            },
            Job {
                id: "h".into(),
                priority: 3,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 2,
                duration: 1,
                tag: None,
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
            "0 start a 1 8",
            "1 start b 1 8",
            "2 preempt b by h",
            "2 start h 1 1",
            "3 finish h 1 ok",
            "3 start b 1 7",
            "8 finish a 1 ok",
            "10 finish b 1 ok"
        ]
    );
}

// R28 R40
#[test]
fn hidden_victim_id() {
    let mut p = Pool::new(
        vec![
            Job {
                id: "z".into(),
                priority: 1,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 0,
                duration: 5,
                tag: None,
                failures: 0,
            },
            Job {
                id: "a".into(),
                priority: 1,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 0,
                duration: 5,
                tag: None,
                failures: 0,
            },
            Job {
                id: "h".into(),
                priority: 3,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 1,
                duration: 1,
                tag: None,
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
        p.run(6).unwrap(),
        vec![
            "0 start z 1 5",
            "0 start a 1 5",
            "1 preempt z by h",
            "1 start h 1 1",
            "2 finish h 1 ok",
            "2 start z 1 4",
            "5 finish a 1 ok",
            "6 finish z 1 ok"
        ]
    );
}

// R17 R23 R30 R31 R44
#[test]
fn hidden_preempt_retry() {
    let mut p = Pool::new(
        vec![
            Job {
                id: "work".into(),
                priority: 0,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 0,
                duration: 4,
                tag: None,
                failures: 1,
            },
            Job {
                id: "urgent".into(),
                priority: 5,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 1,
                duration: 1,
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
        p.run(5).unwrap(),
        vec![
            "0 start work 1 4",
            "1 preempt work by urgent",
            "1 start urgent 1 1",
            "2 finish urgent 1 ok",
            "2 start work 1 3",
            "5 finish work 1 fail",
            "5 retry work 1 8"
        ]
    );
    assert_eq!(
        p.status("work"),
        Ok(State::Waiting {
            ready_at: 8,
            attempt: 1
        })
    );
    assert_eq!(
        p.run(12).unwrap(),
        vec!["8 start work 2 4", "12 finish work 2 ok"]
    );
}

// R22 R23 R27 R33 R35
#[test]
fn hidden_reselect_after_start() {
    let mut p = Pool::new(
        vec![
            Job {
                id: "old".into(),
                priority: 0,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 0,
                duration: 2,
                tag: None,
                failures: 0,
            },
            Job {
                id: "root".into(),
                priority: 9,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 0,
                duration: 10,
                tag: None,
                failures: 0,
            },
            Job {
                id: "high".into(),
                priority: 5,
                dependencies: vec!["root".into()],
                max_attempts: 2,
                release_at: 0,
                duration: 1,
                tag: None,
                failures: 0,
            },
        ],
        1,
        1,
        3,
        10,
    )
    .unwrap();
    assert_eq!(
        p.run(13).unwrap(),
        vec![
            "0 start root 1 10",
            "10 finish root 1 ok",
            "10 start old 1 2",
            "10 preempt old by high",
            "10 start high 1 1",
            "11 finish high 1 ok",
            "11 start old 1 2",
            "13 finish old 1 ok"
        ]
    );
}

// R11 R12 R35 R36
#[test]
fn hidden_split_consistency() {
    let jobs = vec![
        Job {
            id: "low".into(),
            priority: 0,
            dependencies: vec![],
            max_attempts: 2,
            release_at: 0,
            duration: 9,
            tag: None,
            failures: 0,
        },
        Job {
            id: "high".into(),
            priority: 4,
            dependencies: vec![],
            max_attempts: 2,
            release_at: 3,
            duration: 2,
            tag: None,
            failures: 1,
        },
    ];
    let mut a = Pool::new(jobs.clone(), 1, 1, 2, 5).unwrap();
    let mut b = Pool::new(jobs, 1, 1, 2, 5).unwrap();
    let full = a.run(30).unwrap();
    let mut split = Vec::new();
    for t in [0, 1, 2, 3, 4, 5, 6, 7, 10, 30] {
        split.extend(b.run(t).unwrap());
    }
    assert_eq!(split, full);
}

// R5 R8 R18 R25 R39 R44
#[test]
fn hidden_large_and_future() {
    let jobs = (0..64)
        .map(|i| Job {
            id: format!("j{i}"),
            priority: 0,
            dependencies: vec![],
            max_attempts: 1,
            release_at: 0,
            duration: 1,
            tag: None,
            failures: 0,
        })
        .collect();
    let mut p = Pool::new(jobs, 16, u64::MAX, 1, 1).unwrap();
    let v = p.run(4).unwrap();
    assert_eq!(v.len(), 128);
    assert!((0..64).all(|i| p.status(&format!("j{i}")) == Ok(State::Succeeded)));
    let mut p = Pool::new(
        vec![Job {
            id: "future".into(),
            priority: 1,
            dependencies: vec![],
            max_attempts: 2,
            release_at: 1000000000,
            duration: 1000000000,
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
        p.run(1000000000).unwrap(),
        vec!["1000000000 start future 1 1000000000"]
    );
    assert_eq!(
        p.status("future"),
        Ok(State::Running {
            attempt: 1,
            finish_at: 2000000000
        })
    );
}

// R9 R22 R27 R35 R36 R37 R44
#[test]
fn hidden_snapshot_and_no_aging_event() {
    let mut p = Pool::new(
        vec![
            Job {
                id: "low".into(),
                priority: 3,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 0,
                duration: 100,
                tag: None,
                failures: 0,
            },
            Job {
                id: "wait".into(),
                priority: 4,
                dependencies: vec![],
                max_attempts: 2,
                release_at: 1,
                duration: 1,
                tag: None,
                failures: 0,
            },
        ],
        1,
        1,
        3,
        10,
    )
    .unwrap();
    assert_eq!(p.run(0).unwrap(), vec!["0 start low 1 100"]);
    let old = p.status("low").unwrap();
    assert!(p.run(50).unwrap().is_empty());
    assert_eq!(
        old,
        State::Running {
            attempt: 1,
            finish_at: 100
        }
    );
    assert_eq!(
        p.status("wait"),
        Ok(State::Waiting {
            ready_at: 1,
            attempt: 0
        })
    );
    assert_eq!(
        p.run(101).unwrap(),
        vec![
            "100 finish low 1 ok",
            "100 start wait 1 1",
            "101 finish wait 1 ok"
        ]
    );
}
