#[test]
fn spec_r1_single_file_api_compiles() {
    let customer = Customer {
        id: 1,
        arrival: 0,
        service: 0,
        priority: 0,
    };
    let event = Event {
        time: 0,
        customer: 1,
        kind: Kind::Arrive,
    };
    let kinds = [
        Kind::Arrive,
        Kind::Start {
            remaining: 0,
            work_at: 0,
            finish: 0,
        },
        Kind::Resume {
            remaining: 0,
            work_at: 0,
            finish: 0,
        },
        Kind::Preempt { remaining: 0, by: 2 },
        Kind::Finish,
    ];
    let errors = [
        Error::ZeroId { index: 0 },
        Error::DuplicateId { index: 1, id: 1 },
        Error::BadPriority {
            index: 0,
            value: 4,
        },
        Error::TimeOverflow {
            customer: 1,
            time: 0,
        },
    ];
    let _ = (customer, event, kinds, errors);
}

#[test]
fn spec_r2_public_api_returns_events_and_errors() {
    assert_eq!(
        simulate(&[c(1, 0, 0, 0)], 0),
        Ok(vec![
            e(0, 1, Kind::Arrive),
            e(
                0,
                1,
                Kind::Start {
                    remaining: 0,
                    work_at: 0,
                    finish: 0,
                },
            ),
            e(0, 1, Kind::Finish),
        ])
    );
}

#[test]
fn spec_r3_empty_input_accepts_any_switch_cost() {
    assert_eq!(simulate(&[], 0), Ok(vec![]));
    assert_eq!(simulate(&[], u64::MAX), Ok(vec![]));
}

#[test]
fn spec_r4_validation_scans_original_indices_and_prioritizes_checks() {
    assert_eq!(
        simulate(&[c(0, 0, 0, 4)], 0),
        Err(Error::ZeroId { index: 0 })
    );
    assert_eq!(
        simulate(&[c(7, 0, 0, 0), c(7, 1, 0, 4)], 0),
        Err(Error::DuplicateId { index: 1, id: 7 })
    );
    assert_eq!(
        simulate(&[c(3, 9, 0, 0), c(2, 1, 0, 5)], 0),
        Err(Error::BadPriority {
            index: 1,
            value: 5,
        })
    );
    assert_eq!(
        simulate(&[c(1, u64::MAX, 1, 0), c(0, 0, 0, 0)], 0),
        Err(Error::ZeroId { index: 1 })
    );
}

#[test]
fn spec_r5_only_strictly_better_priority_preempts() {
    let trace = simulate(&[c(1, 0, 3, 1), c(2, 1, 1, 1)], 0).unwrap();
    assert_eq!(
        trace,
        vec![
            e(0, 1, Kind::Arrive),
            e(
                0,
                1,
                Kind::Start {
                    remaining: 3,
                    work_at: 0,
                    finish: 3,
                },
            ),
            e(1, 2, Kind::Arrive),
            e(3, 1, Kind::Finish),
            e(
                3,
                2,
                Kind::Start {
                    remaining: 1,
                    work_at: 3,
                    finish: 4,
                },
            ),
            e(4, 2, Kind::Finish),
        ]
    );
}

#[test]
fn spec_r6_arrivals_sort_by_time_priority_then_descending_id() {
    let trace = simulate(
        &[
            c(2, 1, 0, 1),
            c(3, 1, 0, 0),
            c(4, 1, 0, 1),
            c(8, 0, 0, 2),
        ],
        0,
    )
    .unwrap();
    let arrivals: Vec<(u64, u32)> = trace
        .iter()
        .filter_map(|event| match event.kind {
            Kind::Arrive => Some((event.time, event.customer)),
            _ => None,
        })
        .collect();
    assert_eq!(arrivals, vec![(0, 8), (1, 3), (1, 4), (1, 2)]);
}

#[test]
fn spec_r7_finish_precedes_same_tick_arrivals_and_preemption() {
    let trace = simulate(&[c(1, 0, 2, 2), c(2, 2, 1, 0)], 0).unwrap();
    assert_eq!(
        trace,
        vec![
            e(0, 1, Kind::Arrive),
            e(
                0,
                1,
                Kind::Start {
                    remaining: 2,
                    work_at: 0,
                    finish: 2,
                },
            ),
            e(2, 1, Kind::Finish),
            e(2, 2, Kind::Arrive),
            e(
                2,
                2,
                Kind::Start {
                    remaining: 1,
                    work_at: 2,
                    finish: 3,
                },
            ),
            e(3, 2, Kind::Finish),
        ]
    );
}

#[test]
fn spec_r8_arrival_cohort_is_emitted_before_any_dispatch() {
    let trace = simulate(
        &[c(1, 0, 3, 3), c(2, 1, 1, 2), c(3, 1, 1, 0)],
        0,
    )
    .unwrap();
    let at_one: Vec<(u32, Kind)> = trace
        .iter()
        .filter(|event| event.time == 1)
        .map(|event| (event.customer, event.kind.clone()))
        .collect();
    assert_eq!(
        at_one,
        vec![
            (3, Kind::Arrive),
            (2, Kind::Arrive),
            (1, Kind::Preempt {
                remaining: 2,
                by: 3,
            }),
            (
                3,
                Kind::Start {
                    remaining: 1,
                    work_at: 1,
                    finish: 2,
                },
            ),
        ]
    );
}

#[test]
fn spec_r9_paused_customer_ranks_ahead_of_fresh_same_priority() {
    let trace = simulate(
        &[
            c(1, 0, 5, 1),
            c(2, 1, 1, 0),
            c(3, 2, 1, 1),
        ],
        0,
    )
    .unwrap();
    let at_three: Vec<(u32, Kind)> = trace
        .iter()
        .filter(|event| event.time == 3)
        .map(|event| (event.customer, event.kind.clone()))
        .collect();
    assert_eq!(
        at_three,
        vec![
            (2, Kind::Finish),
            (
                1,
                Kind::Resume {
                    remaining: 4,
                    work_at: 3,
                    finish: 7,
                },
            ),
        ]
    );
}

#[test]
fn spec_r10_paused_ties_use_preemption_time_then_id_and_fresh_ties_use_arrival_then_descending_id() {
    let trace = simulate(
        &[
            c(1, 0, 8, 3),
            c(2, 1, 1, 0),
            c(3, 1, 1, 0),
            c(4, 2, 1, 0),
            c(5, 3, 1, 0),
            c(6, 4, 1, 0),
            c(7, 5, 1, 3),
            c(8, 5, 1, 3),
        ],
        0,
    )
    .unwrap();
    let resumes_at_six: Vec<u32> = trace
        .iter()
        .filter(|event| event.time == 6 && matches!(event.kind, Kind::Resume { .. }))
        .map(|event| event.customer)
        .collect();
    assert_eq!(resumes_at_six, vec![1]);
}

#[test]
fn spec_r11_preempt_only_after_arrival_phase_and_name_best_waiter() {
    let trace = simulate(
        &[
            c(1, 0, 5, 3),
            c(2, 1, 1, 0),
            c(3, 2, 1, 1),
            c(4, 2, 1, 0),
        ],
        0,
    )
    .unwrap();
    assert!(trace.contains(&e(
        2,
        1,
        Kind::Preempt {
            remaining: 3,
            by: 4,
        }
    )));
}

#[test]
fn spec_r12_switching_protects_active_customer() {
    let trace = simulate(&[c(1, 0, 3, 2), c(2, 1, 1, 0)], 2).unwrap();
    assert!(!trace.iter().any(|event| matches!(event.kind, Kind::Preempt { .. })));
    assert!(trace.contains(&e(
        1,
        2,
        Kind::Arrive
    )));
    assert!(trace.contains(&e(
        3,
        1,
        Kind::Preempt {
            remaining: 2,
            by: 2,
        }
    )));
}

#[test]
fn spec_r13_work_boundary_reconsiders_preemption_without_an_arrival() {
    let trace = simulate(&[c(1, 0, 3, 2), c(2, 1, 1, 0)], 2).unwrap();
    assert!(trace.contains(&e(
        3,
        1,
        Kind::Preempt {
            remaining: 2,
            by: 2,
        }
    )));
}

#[test]
fn spec_r14_preemption_remaining_is_finish_minus_preemption_time() {
    let trace = simulate(&[c(1, 0, 5, 2), c(2, 2, 1, 0)], 0).unwrap();
    assert!(trace.contains(&e(
        2,
        1,
        Kind::Preempt {
            remaining: 3,
            by: 2,
        }
    )));
}

#[test]
fn spec_r15_dispatch_reports_remaining_and_resumes_after_preemption() {
    let trace = simulate(&[c(1, 0, 4, 2), c(2, 1, 1, 0)], 0).unwrap();
    assert!(trace.contains(&e(
        3,
        1,
        Kind::Resume {
            remaining: 3,
            work_at: 3,
            finish: 6,
        }
    )));
}

#[test]
fn spec_r16_first_dispatch_is_free_then_changed_id_pays_cost() {
    let trace = simulate(&[c(1, 0, 1, 0), c(2, 0, 1, 1)], 4).unwrap();
    assert!(trace.contains(&e(
        1,
        2,
        Kind::Start {
            remaining: 1,
            work_at: 5,
            finish: 6,
        }
    )));
}

#[test]
fn spec_r17_dispatch_adds_switch_cost_before_service() {
    let trace = simulate(&[c(1, 0, 1, 0), c(2, 0, 2, 1)], 3).unwrap();
    assert!(trace.contains(&e(
        1,
        2,
        Kind::Start {
            remaining: 2,
            work_at: 4,
            finish: 6,
        }
    )));
}

#[test]
fn spec_r18_overflow_returns_selected_customer_and_dispatch_time() {
    assert_eq!(
        simulate(&[c(1, 0, u64::MAX, 0), c(2, 0, 1, 1)], 1),
        Err(Error::TimeOverflow {
            customer: 2,
            time: u64::MAX,
        })
    );
}

#[test]
fn spec_r19_inline_finish_repeats_dispatch_after_cohort() {
    let trace = simulate(
        &[c(1, 0, 0, 1), c(2, 0, 0, 0), c(3, 0, 1, 2)],
        0,
    )
    .unwrap();
    assert_eq!(
        trace,
        vec![
            e(0, 2, Kind::Arrive),
            e(0, 1, Kind::Arrive),
            e(0, 3, Kind::Arrive),
            e(
                0,
                2,
                Kind::Start {
                    remaining: 0,
                    work_at: 0,
                    finish: 0,
                },
            ),
            e(0, 2, Kind::Finish),
            e(
                0,
                1,
                Kind::Start {
                    remaining: 0,
                    work_at: 0,
                    finish: 0,
                },
            ),
            e(0, 1, Kind::Finish),
            e(
                0,
                3,
                Kind::Start {
                    remaining: 1,
                    work_at: 0,
                    finish: 1,
                },
            ),
            e(1, 3, Kind::Finish),
        ]
    );
}

#[test]
fn spec_r20_resumed_dispatch_can_be_preempted_again() {
    let trace = simulate(
        &[
            c(1, 0, 7, 3),
            c(2, 1, 1, 0),
            c(3, 3, 1, 1),
            c(4, 4, 1, 0),
        ],
        0,
    )
    .unwrap();
    assert!(trace.contains(&e(
        4,
        1,
        Kind::Preempt {
            remaining: 4,
            by: 4,
        }
    )));
    assert_eq!(
        trace
            .iter()
            .filter(|event| event.customer == 1 && matches!(event.kind, Kind::Resume { .. }))
            .count(),
        1
    );
}

#[test]
fn spec_r21_remembered_dispatch_id_survives_idle_gap() {
    let trace = simulate(&[c(1, 0, 1, 0), c(1 + 1, 5, 1, 0)], 2).unwrap();
    assert!(trace.contains(&e(
        5,
        2,
        Kind::Start {
            remaining: 1,
            work_at: 7,
            finish: 8,
        }
    )));
}

#[test]
fn spec_r22_advance_to_work_boundary_and_omit_idle_events() {
    let trace = simulate(&[c(1, 2, 1, 0)], 0).unwrap();
    assert_eq!(
        trace,
        vec![
            e(2, 1, Kind::Arrive),
            e(
                2,
                1,
                Kind::Start {
                    remaining: 1,
                    work_at: 2,
                    finish: 3,
                },
            ),
            e(3, 1, Kind::Finish),
        ]
    );
}

#[test]
fn spec_r23_trace_order_and_per_customer_lifecycle_counts() {
    let trace = simulate(&[c(1, 0, 4, 2), c(2, 1, 1, 0)], 0).unwrap();
    let times: Vec<u64> = trace.iter().map(|event| event.time).collect();
    assert!(times.windows(2).all(|window| window[0] <= window[1]));
    for id in [1, 2] {
        let customer_events: Vec<&Kind> = trace
            .iter()
            .filter(|event| event.customer == id)
            .map(|event| &event.kind)
            .collect();
        assert_eq!(
            customer_events
                .iter()
                .filter(|kind| matches!(kind, Kind::Arrive))
                .count(),
            1
        );
        assert_eq!(
            customer_events
                .iter()
                .filter(|kind| matches!(kind, Kind::Start { .. }))
                .count(),
            1
        );
        assert_eq!(
            customer_events
                .iter()
                .filter(|kind| matches!(kind, Kind::Finish))
                .count(),
            1
        );
        assert_eq!(
            customer_events
                .iter()
                .filter(|kind| matches!(kind, Kind::Resume { .. }))
                .count(),
            customer_events
                .iter()
                .filter(|kind| matches!(kind, Kind::Preempt { .. }))
                .count()
        );
    }
}

#[test]
fn spec_r24_max_timestamp_and_zero_addition_are_legal() {
    assert_eq!(
        simulate(&[c(1, u64::MAX, 0, 0)], 0),
        Ok(vec![
            e(u64::MAX, 1, Kind::Arrive),
            e(
                u64::MAX,
                1,
                Kind::Start {
                    remaining: 0,
                    work_at: u64::MAX,
                    finish: u64::MAX,
                },
            ),
            e(u64::MAX, 1, Kind::Finish),
        ])
    );
    assert_eq!(
        simulate(&[c(1, 0, 0, 0)], u64::MAX),
        Ok(vec![
            e(0, 1, Kind::Arrive),
            e(
                0,
                1,
                Kind::Start {
                    remaining: 0,
                    work_at: 0,
                    finish: 0,
                },
            ),
            e(0, 1, Kind::Finish),
        ])
    );
}

#[test]
fn spec_r25_calls_are_independent_and_input_is_unchanged() {
    let input = [c(2, 1, 1, 0), c(1, 0, 2, 1)];
    let before = input.clone();
    let first = simulate(&input, 2);
    let second = simulate(&input, 2);
    assert_eq!(first, second);
    assert_eq!(input, before);
}

#[test]
fn spec_r26_runtime_overflow_uses_chronological_dispatch_order() {
    assert_eq!(
        simulate(
            &[c(1, u64::MAX, 1, 0), c(2, 0, u64::MAX, 0)],
            0
        ),
        Err(Error::TimeOverflow {
            customer: 2,
            time: 0,
        })
    );
}

#[test]
fn spec_r27_preempt_by_uses_best_waiter_from_whole_cohort() {
    let trace = simulate(
        &[
            c(1, 0, 5, 3),
            c(2, 1, 1, 1),
            c(3, 2, 1, 2),
            c(4, 2, 1, 0),
        ],
        0,
    )
    .unwrap();
    assert!(trace.contains(&e(
        2,
        1,
        Kind::Preempt {
            remaining: 3,
            by: 4,
        }
    )));
}
