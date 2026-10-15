fn c(id: u32, arrival: u64, service: u64, priority: u8, patience: Option<u64>) -> Customer {
    Customer {
        id,
        arrival,
        service,
        priority,
        patience,
    }
}
fn cfg(waiting_capacity: usize, switch_cost: u64) -> Config {
    Config {
        switch_cost,
        waiting_capacity,
    }
}
fn e(time: u64, customer: u32, kind: Kind) -> Event {
    Event {
        time,
        customer,
        kind,
    }
}
fn ds(r: &Report) -> Vec<(u64, u32, u64, u64, u64, bool)> {
    r.events
        .iter()
        .filter_map(|e| match e.kind {
            Kind::Start {
                remaining,
                work_at,
                finish,
            } => Some((e.time, e.customer, remaining, work_at, finish, false)),
            Kind::Resume {
                remaining,
                work_at,
                finish,
            } => Some((e.time, e.customer, remaining, work_at, finish, true)),
            _ => None,
        })
        .collect()
}
// Checks R1, R2, R3, R4, R31, R38, R39.
#[test]
fn hidden_empty_and_configuration_precedence() {
    let r = simulate(&[], &cfg(4096, u64::MAX)).unwrap();
    assert_eq!(
        r.stats,
        Stats {
            accepted: 0,
            balked: 0,
            reneged: 0,
            completed: 0,
            started: 0,
            preemptions: 0,
            elapsed: 0,
            busy_ticks: 0,
            wait_ticks: 0,
            turnaround_ticks: 0,
            mean_wait_milli: 0,
            mean_turnaround_milli: 0,
            utilization_bps: 0,
            completion_order: vec![]
        }
    );
    assert!(r.events.is_empty());
    assert_eq!(
        simulate(&[], &cfg(4097, 0)),
        Err(Error::CapacityTooLarge { capacity: 4097 })
    );
    let bad = vec![c(0, 0, 0, 255, None); 4097];
    assert_eq!(
        simulate(&bad, &cfg(4097, 0)),
        Err(Error::CapacityTooLarge { capacity: 4097 })
    );
    assert_eq!(
        simulate(&bad, &cfg(0, 0)),
        Err(Error::TooManyCustomers { count: 4097 })
    );
}
// Checks R5.
#[test]
fn hidden_row_validation_precedence_and_positions() {
    assert_eq!(
        simulate(&[c(0, u64::MAX, 1, 7, Some(1))], &cfg(0, 0)),
        Err(Error::ZeroId { index: 0 })
    );
    assert_eq!(
        simulate(
            &[c(7, 8, 1, 0, None), c(7, u64::MAX, 1, 7, Some(1))],
            &cfg(0, 0)
        ),
        Err(Error::DuplicateId { index: 1, id: 7 })
    );
    assert_eq!(
        simulate(
            &[c(8, u64::MAX, 1, 7, Some(1)), c(0, 0, 1, 0, None)],
            &cfg(0, 0)
        ),
        Err(Error::BadPriority { index: 0, value: 7 })
    );
    assert_eq!(
        simulate(
            &[c(9, u64::MAX, 1, 0, Some(1)), c(0, 0, 1, 0, None)],
            &cfg(0, 0)
        ),
        Err(Error::DeadlineOverflow { index: 0, id: 9 })
    );
}
// Checks R5, R38, R39.
#[test]
fn hidden_initial_deadline_validated_before_runtime_and_balk() {
    assert_eq!(
        simulate(
            &[c(1, 0, 8, 0, None), c(2, 1, 0, 3, Some(u64::MAX))],
            &cfg(0, 0)
        ),
        Err(Error::DeadlineOverflow { index: 1, id: 2 })
    );
    assert_eq!(
        simulate(
            &[c(1, 0, u64::MAX, 0, None), c(2, u64::MAX, 0, 3, Some(1))],
            &cfg(0, 0)
        ),
        Err(Error::DeadlineOverflow { index: 1, id: 2 })
    );
    assert_eq!(
        simulate(
            &[c(1, u64::MAX, 1, 0, None), c(2, 0, 0, 9, None)],
            &cfg(0, 0)
        ),
        Err(Error::BadPriority { index: 1, value: 9 })
    );
}
// Checks R3, R7, R10, R11, R12, R13, R14, R28, R42, R43.
#[test]
fn hidden_idle_virtual_slot_and_cohort_balk_order() {
    let r = simulate(
        &[
            c(8, 4, 0, 3, Some(0)),
            c(9, 4, 0, 1, Some(0)),
            c(11, 4, 0, 1, Some(0)),
        ],
        &cfg(0, 0),
    )
    .unwrap();
    assert_eq!(
        r.events,
        vec![
            e(4, 11, Kind::Arrive),
            e(4, 9, Kind::Arrive),
            e(4, 9, Kind::Balk),
            e(4, 8, Kind::Arrive),
            e(4, 8, Kind::Balk),
            e(
                4,
                11,
                Kind::Start {
                    remaining: 0,
                    work_at: 4,
                    finish: 4
                }
            ),
            e(4, 11, Kind::Finish)
        ]
    );
    assert_eq!(
        (r.stats.accepted, r.stats.balked, r.stats.reneged),
        (1, 2, 0)
    );
    assert_eq!(r.stats.utilization_bps, 0);
}
// Checks R11, R12, R13, R16.
#[test]
fn hidden_full_gate_blocks_high_priority_preemption() {
    let r = simulate(
        &[
            c(1, 0, 8, 3, None),
            c(2, 1, 1, 3, None),
            c(3, 2, 1, 0, None),
        ],
        &cfg(1, 0),
    )
    .unwrap();
    assert!(r.events.contains(&e(2, 3, Kind::Balk)));
    assert_eq!(r.stats.preemptions, 0);
    assert_eq!(r.stats.completion_order, vec![1, 2]);
}
// Checks R6, R11, R12, R13, R25, R26.
#[test]
fn hidden_full_gate_balk_precedes_zero_patience() {
    let r = simulate(&[c(1, 0, 6, 0, None), c(2, 1, 1, 1, Some(0))], &cfg(0, 0)).unwrap();
    assert!(r.events.contains(&e(1, 2, Kind::Balk)));
    assert_eq!((r.stats.accepted, r.stats.reneged), (1, 0));
}
// Checks R6, R14, R25, R26, R33.
#[test]
fn hidden_zero_patience_waiter_late_renege() {
    let r = simulate(&[c(1, 0, 6, 0, None), c(2, 1, 1, 1, Some(0))], &cfg(1, 0)).unwrap();
    assert_eq!(
        &r.events[2..4],
        &[e(1, 2, Kind::Arrive), e(1, 2, Kind::Renege { waited: 0 })]
    );
    assert_eq!(
        (
            r.stats.accepted,
            r.stats.started,
            r.stats.reneged,
            r.stats.wait_ticks
        ),
        (2, 1, 1, 0)
    );
}
// Checks R6, R13, R16, R17, R25, R29, R30.
#[test]
fn hidden_zero_patience_high_priority_can_start() {
    let r = simulate(&[c(1, 0, 5, 3, None), c(2, 1, 1, 0, Some(0))], &cfg(1, 0)).unwrap();
    assert_eq!(r.stats.completion_order, vec![2, 1]);
    assert_eq!(r.stats.reneged, 0);
    assert!(r.events.contains(&e(
        1,
        1,
        Kind::Preempt {
            remaining: 4,
            by: 2
        }
    )));
}
// Checks R10, R11, R13, R25, R42.
#[test]
fn hidden_zero_budget_reserves_slot_until_late_phase() {
    let r = simulate(
        &[
            c(1, 0, 5, 0, None),
            c(2, 1, 1, 1, Some(0)),
            c(3, 1, 1, 2, None),
        ],
        &cfg(1, 0),
    )
    .unwrap();
    assert_eq!(
        &r.events[2..6],
        &[
            e(1, 2, Kind::Arrive),
            e(1, 3, Kind::Arrive),
            e(1, 3, Kind::Balk),
            e(1, 2, Kind::Renege { waited: 0 })
        ]
    );
}
// Checks R9, R11, R26, R33.
#[test]
fn hidden_early_renege_releases_admission_slot() {
    let r = simulate(
        &[
            c(1, 0, 10, 0, None),
            c(2, 1, 1, 1, Some(2)),
            c(3, 3, 1, 2, None),
        ],
        &cfg(1, 0),
    )
    .unwrap();
    assert_eq!(
        &r.events[3..5],
        &[e(3, 2, Kind::Renege { waited: 2 }), e(3, 3, Kind::Arrive)]
    );
    assert_eq!(
        (r.stats.accepted, r.stats.balked, r.stats.reneged),
        (3, 0, 1)
    );
    assert_eq!(r.stats.wait_ticks, 9);
}
// Checks R9, R26, R28.
#[test]
fn hidden_deadline_tie_ascending_id_not_priority() {
    let r = simulate(
        &[
            c(1, 0, 10, 0, None),
            c(9, 1, 1, 1, Some(4)),
            c(3, 2, 1, 3, Some(3)),
        ],
        &cfg(2, 0),
    )
    .unwrap();
    let expired = r
        .events
        .iter()
        .filter(|e| matches!(e.kind, Kind::Renege { .. }))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        expired,
        vec![
            e(5, 3, Kind::Renege { waited: 3 }),
            e(5, 9, Kind::Renege { waited: 4 })
        ]
    );
}
// Checks R8, R9, R41.
#[test]
fn hidden_deadline_exact_finish_renege_before_dispatch() {
    let r = simulate(&[c(1, 0, 4, 0, None), c(2, 1, 1, 1, Some(3))], &cfg(1, 0)).unwrap();
    assert_eq!(
        &r.events[3..],
        &[e(4, 1, Kind::Finish), e(4, 2, Kind::Renege { waited: 3 })]
    );
    assert_eq!(
        (r.stats.started, r.stats.completed, r.stats.reneged),
        (1, 1, 1)
    );
}
// Checks R8, R9, R10, R11, R25, R41.
#[test]
fn hidden_finish_expire_arrive_start_full_phase_order() {
    let r = simulate(
        &[
            c(1, 0, 4, 0, None),
            c(2, 1, 1, 1, Some(3)),
            c(3, 4, 1, 0, Some(0)),
        ],
        &cfg(1, 0),
    )
    .unwrap();
    assert_eq!(
        &r.events[3..7],
        &[
            e(4, 1, Kind::Finish),
            e(4, 2, Kind::Renege { waited: 3 }),
            e(4, 3, Kind::Arrive),
            e(
                4,
                3,
                Kind::Start {
                    remaining: 1,
                    work_at: 4,
                    finish: 5
                }
            )
        ]
    );
}
// Checks R6, R17, R23, R26, R29, R32, R33.
#[test]
fn hidden_cumulative_patience_after_initial_wait_and_pause() {
    let r = simulate(
        &[
            c(1, 0, 2, 0, None),
            c(2, 1, 10, 3, Some(3)),
            c(3, 4, 5, 0, None),
        ],
        &cfg(2, 0),
    )
    .unwrap();
    assert!(r.events.contains(&e(
        4,
        2,
        Kind::Preempt {
            remaining: 8,
            by: 3
        }
    )));
    assert!(r.events.contains(&e(6, 2, Kind::Renege { waited: 3 })));
    assert_eq!(r.stats.completion_order, vec![1, 3]);
    assert_eq!(
        (
            r.stats.accepted,
            r.stats.started,
            r.stats.reneged,
            r.stats.busy_ticks,
            r.stats.wait_ticks
        ),
        (3, 3, 1, 9, 3)
    );
}
// Checks R6, R19, R20, R21, R22, R23, R32, R33.
#[test]
fn hidden_switching_does_not_consume_patience() {
    let r = simulate(&[c(1, 0, 1, 0, None), c(2, 1, 1, 1, Some(0))], &cfg(1, 8)).unwrap();
    assert_eq!(ds(&r)[1], (1, 2, 1, 9, 10, false));
    assert_eq!(
        (r.stats.reneged, r.stats.busy_ticks, r.stats.wait_ticks),
        (0, 10, 0)
    );
}
// Checks R6, R17, R25, R26, R29, R32.
#[test]
fn hidden_paused_zero_budget_expires_after_new_start() {
    let r = simulate(&[c(1, 0, 5, 3, Some(0)), c(2, 1, 2, 0, None)], &cfg(1, 0)).unwrap();
    assert_eq!(
        &r.events[3..6],
        &[
            e(
                1,
                1,
                Kind::Preempt {
                    remaining: 4,
                    by: 2
                }
            ),
            e(
                1,
                2,
                Kind::Start {
                    remaining: 2,
                    work_at: 1,
                    finish: 3
                }
            ),
            e(1, 1, Kind::Renege { waited: 0 })
        ]
    );
    assert_eq!(
        (r.stats.busy_ticks, r.stats.started, r.stats.preemptions),
        (3, 2, 1)
    );
}
// Checks R17, R24, R25, R26.
#[test]
fn hidden_paused_zero_budget_resumes_after_inline_finish() {
    let r = simulate(
        &[c(1, 0, 5, 3, Some(0)), c(2, 1, 0, 0, Some(0))],
        &cfg(1, 0),
    )
    .unwrap();
    assert_eq!(
        ds(&r),
        vec![
            (0, 1, 5, 0, 5, false),
            (1, 2, 0, 1, 1, false),
            (1, 1, 4, 1, 5, true)
        ]
    );
    assert_eq!(r.stats.reneged, 0);
    assert_eq!(r.stats.completion_order, vec![2, 1]);
}
// Checks R15, R16, R17, R23, R29, R30, R43.
#[test]
fn hidden_paused_precedes_older_fresh_same_class() {
    let r = simulate(
        &[
            c(1, 0, 7, 2, None),
            c(2, 1, 1, 2, None),
            c(3, 2, 1, 0, None),
        ],
        &cfg(3, 0),
    )
    .unwrap();
    assert_eq!(
        ds(&r).iter().map(|x| x.1).collect::<Vec<_>>(),
        vec![1, 3, 1, 2]
    );
    assert_eq!(r.stats.completion_order, vec![3, 1, 2]);
    assert_eq!(r.stats.started, 3);
}
// Checks R6, R15, R16, R17, R23, R29, R33.
#[test]
fn hidden_repeated_pauses_preserve_wait_budget() {
    let r = simulate(
        &[
            c(1, 0, 20, 3, Some(3)),
            c(2, 1, 1, 0, None),
            c(3, 4, 4, 0, None),
        ],
        &cfg(2, 0),
    )
    .unwrap();
    assert!(r.events.contains(&e(6, 1, Kind::Renege { waited: 3 })));
    assert_eq!(
        (
            r.stats.preemptions,
            r.stats.started,
            r.stats.completed,
            r.stats.reneged,
            r.stats.wait_ticks
        ),
        (2, 3, 2, 1, 3)
    );
    assert_eq!(r.stats.busy_ticks, 8);
}
// Checks R9, R16, R19, R21, R41.
#[test]
fn hidden_boundary_expiration_beats_protected_preemptor() {
    let r = simulate(
        &[
            c(1, 0, 1, 0, None),
            c(2, 1, 3, 3, None),
            c(3, 2, 1, 0, Some(3)),
            c(4, 5, 1, 1, None),
        ],
        &cfg(2, 4),
    )
    .unwrap();
    assert_eq!(
        &r.events[6..9],
        &[
            e(5, 3, Kind::Renege { waited: 3 }),
            e(5, 4, Kind::Arrive),
            e(
                5,
                2,
                Kind::Preempt {
                    remaining: 3,
                    by: 4
                }
            )
        ]
    );
    assert_eq!(r.stats.completion_order, vec![1, 4, 2]);
}
// Checks R19, R21, R27, R32.
#[test]
fn hidden_no_arrival_at_protection_boundary_still_preempts() {
    let r = simulate(
        &[
            c(1, 0, 1, 0, None),
            c(2, 1, 4, 3, None),
            c(3, 2, 1, 0, None),
        ],
        &cfg(2, 3),
    )
    .unwrap();
    assert!(r.events.contains(&e(
        4,
        2,
        Kind::Preempt {
            remaining: 4,
            by: 3
        }
    )));
    assert_eq!(r.stats.busy_ticks, 15);
    assert_eq!(r.stats.elapsed, 15);
    assert_eq!(r.stats.preemptions, 1);
}
// Checks R3, R16, R19, R22, R24.
#[test]
fn hidden_equal_priority_and_zero_service_protection() {
    let r = simulate(
        &[
            c(1, 0, 1, 0, None),
            c(2, 1, 0, 2, Some(0)),
            c(3, 2, 1, 0, None),
        ],
        &cfg(1, 3),
    )
    .unwrap();
    assert_eq!(r.stats.preemptions, 0);
    assert_eq!(r.stats.completion_order, vec![1, 2, 3]);
    let q = simulate(&[c(7, 0, 3, 1, None), c(8, 1, 1, 1, None)], &cfg(1, 0)).unwrap();
    assert_eq!(q.stats.preemptions, 0);
}
// Checks R18, R39.
#[test]
fn hidden_paused_deadline_overflow_precedes_dispatch_overflow() {
    assert_eq!(
        simulate(
            &[c(1, 0, u64::MAX, 3, Some(u64::MAX)), c(2, 1, 1, 0, None)],
            &cfg(1, u64::MAX)
        ),
        Err(Error::TimeOverflow {
            customer: 1,
            time: 1,
            stage: OverflowStage::Patience
        })
    );
}
// Checks R20, R21, R22, R38, R39, R40.
#[test]
fn hidden_dispatch_overflow_stage_and_idle_memory() {
    assert_eq!(
        simulate(
            &[c(1, 0, 1, 0, None), c(2, 3, 0, 0, None)],
            &cfg(0, u64::MAX)
        ),
        Err(Error::TimeOverflow {
            customer: 2,
            time: 3,
            stage: OverflowStage::WorkAt
        })
    );
    assert_eq!(
        simulate(
            &[c(1, 0, 1, 0, None), c(2, u64::MAX - 2, 2, 0, None)],
            &cfg(0, 1)
        ),
        Err(Error::TimeOverflow {
            customer: 2,
            time: u64::MAX - 2,
            stage: OverflowStage::Finish
        })
    );
    let r = simulate(&[c(1, 0, 1, 0, None), c(2, 20, 1, 0, None)], &cfg(0, 3)).unwrap();
    assert_eq!(ds(&r)[1], (20, 2, 1, 23, 24, false));
}
// Checks R12, R26, R38.
#[test]
fn hidden_balk_and_renege_ignore_service_overflow() {
    let r = simulate(
        &[
            c(1, 0, 8, 0, None),
            c(2, 1, u64::MAX, 1, Some(1)),
            c(3, 1, u64::MAX, 2, None),
        ],
        &cfg(1, 0),
    )
    .unwrap();
    assert_eq!(
        (r.stats.balked, r.stats.reneged, r.stats.completed),
        (1, 1, 1)
    );
    assert_eq!(r.stats.wait_ticks, 1);
}
// Checks R20, R21, R27, R31, R38, R40.
#[test]
fn hidden_max_times_zero_deadline_and_exact_finish() {
    let r = simulate(&[c(u32::MAX, u64::MAX, 0, 3, Some(0))], &cfg(0, u64::MAX)).unwrap();
    assert_eq!(r.stats.elapsed, u64::MAX);
    assert_eq!(r.stats.completed, 1);
    assert_eq!(r.stats.utilization_bps, 0);
    let r = simulate(&[c(7, u64::MAX - 2, 2, 0, Some(2))], &cfg(0, u64::MAX)).unwrap();
    assert_eq!(r.stats.completion_order, vec![7]);
    assert_eq!(r.stats.elapsed, u64::MAX);
    assert_eq!(r.stats.busy_ticks, 2);
    assert_eq!(r.stats.mean_turnaround_milli, 2000);
}
// Checks R28, R29, R30, R31, R32, R33, R34, R35, R36, R43.
#[test]
fn hidden_mixed_terminal_statistics_exact() {
    let r = simulate(
        &[
            c(1, 0, 2, 0, None),
            c(2, 1, 5, 3, Some(2)),
            c(3, 1, 1, 3, None),
            c(4, 3, 2, 0, None),
        ],
        &cfg(1, 1),
    )
    .unwrap();
    // ID 3 takes the sole wait slot at t=1, so ID 2 balks; ID 4
    // waits during ID 3's protected switch, then preempts at t=3.
    assert_eq!(
        r.stats,
        Stats {
            accepted: 3,
            balked: 1,
            reneged: 0,
            completed: 3,
            started: 3,
            preemptions: 1,
            elapsed: 8,
            busy_ticks: 8,
            wait_ticks: 4,
            turnaround_ticks: 12,
            mean_wait_milli: 1333,
            mean_turnaround_milli: 4000,
            utilization_bps: 10000,
            completion_order: vec![1, 4, 3]
        }
    );
}
// Checks R29, R32, R33, R34, R35.
#[test]
fn hidden_reneged_wait_in_mean_and_turnaround_exclusion() {
    let r = simulate(&[c(1, 2, 8, 0, None), c(2, 3, 9, 1, Some(2))], &cfg(1, 0)).unwrap();
    assert_eq!(
        (
            r.stats.accepted,
            r.stats.started,
            r.stats.reneged,
            r.stats.completed
        ),
        (2, 1, 1, 1)
    );
    assert_eq!(
        (
            r.stats.wait_ticks,
            r.stats.turnaround_ticks,
            r.stats.mean_wait_milli,
            r.stats.mean_turnaround_milli,
            r.stats.utilization_bps
        ),
        (2, 8, 1000, 8000, 8000)
    );
}
// Checks R31, R32, R36, R37.
#[test]
fn hidden_absolute_elapsed_utilization_half_even() {
    let r = simulate(&[c(6, 31, 1, 0, None)], &cfg(0, 0)).unwrap();
    assert_eq!(
        (r.stats.elapsed, r.stats.busy_ticks, r.stats.utilization_bps),
        (32, 1, 312)
    );
    let r = simulate(&[c(7, 29, 3, 0, None)], &cfg(0, 0)).unwrap();
    assert_eq!(r.stats.utilization_bps, 938);
}
// Checks R37.
#[test]
fn hidden_rounding_halves_and_neighbors() {
    for (n, d, x) in [
        (0, 3, 0),
        (5, 2, 2),
        (3, 2, 2),
        (9, 2, 4),
        (11, 2, 6),
        (14, 4, 4),
        (13, 4, 3),
        (15, 4, 4),
        (2, 9, 0),
        (8, 9, 1),
    ] {
        assert_eq!(rounded_ratio(n, d), Some(x));
    }
    assert_eq!(rounded_ratio(0, 0), None);
}
// Checks R37.
#[test]
fn hidden_rounding_full_u128_domain() {
    let m = u128::MAX;
    assert_eq!(rounded_ratio(m, 1), Some(m));
    assert_eq!(rounded_ratio(m, m), Some(1));
    assert_eq!(rounded_ratio(m - 1, m), Some(1));
    assert_eq!(rounded_ratio(m / 2, m), Some(0));
    assert_eq!(rounded_ratio(m / 2 + 1, m), Some(1));
    assert_eq!(rounded_ratio(m, 2), Some(m / 2 + 1));
    assert_eq!(rounded_ratio(m, 0), None);
}
// Checks R35, R37, R42.
#[test]
fn hidden_mean_wait_even_half_and_odd_half() {
    let mut a = (1..=16).map(|id| c(id, 0, 0, 0, None)).collect::<Vec<_>>();
    a[0].service = 1;
    let r = simulate(&a, &cfg(16, 0)).unwrap();
    assert_eq!((r.stats.wait_ticks, r.stats.mean_wait_milli), (0, 0));
    a[0].service = 0;
    a[1].service = 1;
    let r = simulate(&a, &cfg(16, 0)).unwrap();
    assert_eq!((r.stats.wait_ticks, r.stats.mean_wait_milli), (1, 62));
    a[1].service = 3;
    let r = simulate(&a, &cfg(16, 0)).unwrap();
    assert_eq!((r.stats.wait_ticks, r.stats.mean_wait_milli), (3, 188));
}
// Checks R4, R7, R24, R28, R29, R30, R35, R42.
#[test]
fn hidden_limit_sized_queue_and_exact_counts() {
    let a = (1..=4096)
        .map(|id| c(id, 0, 0, (id % 4) as u8, Some(0)))
        .collect::<Vec<_>>();
    let r = simulate(&a, &cfg(4096, 0)).unwrap();
    assert_eq!(r.events.len(), 12288);
    assert_eq!(
        (
            r.stats.accepted,
            r.stats.started,
            r.stats.completed,
            r.stats.reneged,
            r.stats.balked
        ),
        (4096, 4096, 4096, 0, 0)
    );
    let expected = (0..4)
        .flat_map(|p| (1..=4096).rev().filter(move |id| id % 4 == p))
        .collect::<Vec<_>>();
    assert_eq!(r.stats.completion_order, expected);
    assert_eq!(r.stats.mean_wait_milli, 0);
}
// Checks R5, R39, R43.
#[test]
fn hidden_inputs_and_calls_are_independent() {
    let a = vec![c(90, 2, 4, 3, Some(9)), c(10, 3, 1, 0, None)];
    let conf = cfg(2, 1);
    let old = (a.clone(), conf.clone());
    let r = simulate(&a, &conf);
    assert_eq!(simulate(&a, &conf), r);
    assert_eq!((a, conf), old);
}
// Checks R32, R33, R34, R35, R36, R38, R40.
#[test]
fn hidden_wide_totals_do_not_wrap_u64() {
    let r = simulate(
        &[
            c(3, 0, u64::MAX - 1, 0, None),
            c(2, 0, 0, 0, None),
            c(1, 0, 0, 0, None),
        ],
        &cfg(2, 0),
    )
    .unwrap();
    let t = (u64::MAX - 1) as u128;
    assert_eq!(r.stats.wait_ticks, 2 * t);
    assert_eq!(r.stats.turnaround_ticks, 3 * t);
    assert_eq!(r.stats.mean_wait_milli, rounded_ratio(2000 * t, 3).unwrap());
    assert_eq!(r.stats.mean_turnaround_milli, 1000 * t);
    assert_eq!(r.stats.utilization_bps, 10000);
}
