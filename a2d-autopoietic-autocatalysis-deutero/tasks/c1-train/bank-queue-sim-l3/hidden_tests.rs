fn c(id: u32, arrival: u64, service: u64, priority: u8) -> Customer {
    Customer {
        id,
        arrival,
        service,
        priority,
    }
}
fn e(time: u64, customer: u32, kind: Kind) -> Event {
    Event {
        time,
        customer,
        kind,
    }
}
fn dispatches(v: &[Event]) -> Vec<(u64, u32, u64, u64, u64, bool)> {
    v.iter()
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
// Checks R1, R2, R3, R24, R25.
#[test]
fn hidden_empty_max_cost() {
    assert_eq!(simulate(&[], u64::MAX), Ok(vec![]));
}
// Checks R4.
#[test]
fn hidden_zero_beats_priority() {
    assert_eq!(
        simulate(&[c(0, 0, 1, 9)], 0),
        Err(Error::ZeroId { index: 0 })
    );
}
// Checks R4.
#[test]
fn hidden_duplicate_beats_priority() {
    assert_eq!(
        simulate(&[c(9, 8, 1, 0), c(9, 0, 1, 9)], 0),
        Err(Error::DuplicateId { index: 1, id: 9 })
    );
}
// Checks R4.
#[test]
fn hidden_priority_before_later_zero() {
    assert_eq!(
        simulate(&[c(6, 9, 1, 255), c(0, 0, 1, 0)], 0),
        Err(Error::BadPriority {
            index: 0,
            value: 255
        })
    );
}
// Checks R4, R18, R26.
#[test]
fn hidden_validation_before_dynamic_overflow() {
    assert_eq!(
        simulate(&[c(7, u64::MAX, 1, 0), c(8, 0, 1, 5)], 0),
        Err(Error::BadPriority { index: 1, value: 5 })
    );
}
// Checks R5, R6, R8, R9, R10.
#[test]
fn hidden_cohort_priority_then_descending_id() {
    let v = simulate(
        &[c(8, 2, 1, 3), c(4, 2, 1, 0), c(90, 2, 1, 2), c(11, 2, 1, 0)],
        0,
    )
    .unwrap();
    assert_eq!(
        &v[..4],
        &[
            e(2, 11, Kind::Arrive),
            e(2, 4, Kind::Arrive),
            e(2, 90, Kind::Arrive),
            e(2, 8, Kind::Arrive)
        ]
    );
    assert_eq!(
        dispatches(&v).iter().map(|s| s.1).collect::<Vec<_>>(),
        vec![11, 4, 90, 8]
    );
}
// Checks R5, R11.
#[test]
fn hidden_equal_priority_does_not_preempt() {
    let v = simulate(&[c(2, 0, 6, 1), c(99, 1, 1, 1)], 0).unwrap();
    assert!(!v.iter().any(|e| matches!(e.kind, Kind::Preempt { .. })));
    assert_eq!(dispatches(&v)[1].0, 6);
}
// Checks R5, R11.
#[test]
fn hidden_lower_priority_does_not_preempt() {
    let v = simulate(&[c(2, 0, 6, 1), c(99, 1, 1, 3)], 0).unwrap();
    assert_eq!(dispatches(&v)[1].0, 6);
}
// Checks R7, R8, R11, R23.
#[test]
fn hidden_completion_wins_over_high_arrival() {
    let v = simulate(&[c(15, 0, 4, 3), c(16, 4, 1, 0)], 0).unwrap();
    assert_eq!(
        &v[2..5],
        &[
            e(4, 15, Kind::Finish),
            e(4, 16, Kind::Arrive),
            e(
                4,
                16,
                Kind::Start {
                    remaining: 1,
                    work_at: 4,
                    finish: 5
                }
            )
        ]
    );
}
// Checks R8, R11, R27.
#[test]
fn hidden_preempt_by_best_of_whole_cohort() {
    let v = simulate(
        &[c(1, 0, 20, 3), c(6, 2, 1, 2), c(3, 2, 1, 0), c(8, 2, 1, 0)],
        0,
    )
    .unwrap();
    assert_eq!(
        v[5],
        e(
            2,
            1,
            Kind::Preempt {
                remaining: 18,
                by: 8
            }
        )
    );
}
// Checks R9, R10, R14, R20.
#[test]
fn hidden_paused_before_earlier_fresh_peer() {
    let v = simulate(&[c(1, 0, 9, 2), c(2, 1, 1, 2), c(3, 2, 1, 0)], 0).unwrap();
    assert_eq!(
        dispatches(&v).iter().map(|s| s.1).collect::<Vec<_>>(),
        vec![1, 3, 1, 2]
    );
    assert_eq!(dispatches(&v)[2], (3, 1, 7, 3, 10, true));
}
// Checks R9, R10, R20.
#[test]
fn hidden_nested_pauses_by_class() {
    let v = simulate(
        &[c(1, 0, 8, 3), c(2, 1, 4, 2), c(3, 2, 1, 0), c(4, 2, 1, 2)],
        0,
    )
    .unwrap();
    assert_eq!(
        dispatches(&v).iter().map(|s| s.1).collect::<Vec<_>>(),
        vec![1, 2, 3, 2, 4, 1]
    );
}
// Checks R12, R13, R17, R22.
#[test]
fn hidden_protected_overhead_and_silent_boundary() {
    let v = simulate(&[c(1, 0, 1, 0), c(2, 1, 5, 3), c(3, 2, 1, 1)], 4).unwrap();
    assert_eq!(
        v.iter().find(|e| matches!(e.kind, Kind::Preempt { .. })),
        Some(&e(
            5,
            2,
            Kind::Preempt {
                remaining: 5,
                by: 3
            }
        ))
    );
    assert_eq!(
        dispatches(&v),
        vec![
            (0, 1, 1, 0, 1, false),
            (1, 2, 5, 5, 10, false),
            (5, 3, 1, 9, 10, false),
            (10, 2, 5, 14, 19, true)
        ]
    );
    assert!(!v.iter().any(|e| e.time == 9));
}
// Checks R12, R13, R14.
#[test]
fn hidden_arrival_at_work_boundary_can_preempt() {
    let v = simulate(&[c(10, 0, 1, 0), c(11, 1, 4, 3), c(12, 4, 1, 0)], 3).unwrap();
    assert!(v.contains(&e(
        4,
        11,
        Kind::Preempt {
            remaining: 4,
            by: 12
        }
    )));
}
// Checks R7, R12, R13, R19.
#[test]
fn hidden_zero_service_protected_finishes_before_preemption() {
    let v = simulate(&[c(10, 0, 1, 0), c(11, 1, 0, 3), c(12, 2, 1, 0)], 3).unwrap();
    assert!(!v.iter().any(|e| matches!(e.kind, Kind::Preempt { .. })));
    assert!(v.contains(&e(4, 11, Kind::Finish)));
    assert_eq!(dispatches(&v)[2], (4, 12, 1, 7, 8, false));
}
// Checks R14, R17.
#[test]
fn hidden_remaining_excludes_elapsed_switching() {
    let v = simulate(&[c(1, 0, 1, 0), c(2, 1, 10, 3), c(3, 6, 1, 0)], 2).unwrap();
    assert!(v.contains(&e(
        6,
        2,
        Kind::Preempt {
            remaining: 7,
            by: 3
        }
    )));
    assert_eq!(dispatches(&v).last(), Some(&(9, 2, 7, 11, 18, true)));
}
// Checks R16, R21.
#[test]
fn hidden_cost_survives_idle_gap() {
    assert_eq!(
        dispatches(&simulate(&[c(1, 0, 1, 0), c(2, 10, 2, 0)], 5).unwrap()),
        vec![(0, 1, 1, 0, 1, false), (10, 2, 2, 15, 17, false)]
    );
}
// Checks R16, R17, R24.
#[test]
fn hidden_first_dispatch_ignores_max_cost() {
    assert_eq!(
        dispatches(&simulate(&[c(99, 7, 2, 3)], u64::MAX).unwrap()),
        vec![(7, 99, 2, 7, 9, false)]
    );
}
// Checks R18, R24.
#[test]
fn hidden_work_boundary_overflow() {
    assert_eq!(
        simulate(&[c(1, 0, 1, 0), c(2, 2, 0, 0)], u64::MAX),
        Err(Error::TimeOverflow {
            customer: 2,
            time: 2
        })
    );
}
// Checks R17, R18.
#[test]
fn hidden_finish_overflow_after_safe_cost() {
    assert_eq!(
        simulate(&[c(1, 0, 1, 0), c(2, u64::MAX - 3, 3, 0)], 1),
        Err(Error::TimeOverflow {
            customer: 2,
            time: u64::MAX - 3
        })
    );
}
// Checks R18, R26.
#[test]
fn hidden_priority_selects_overflow_customer() {
    assert_eq!(
        simulate(&[c(7, u64::MAX, 1, 3), c(8, u64::MAX, 1, 0)], 0),
        Err(Error::TimeOverflow {
            customer: 8,
            time: u64::MAX
        })
    );
}
// Checks R24.
#[test]
fn hidden_max_timestamp_zero() {
    assert_eq!(
        simulate(&[c(u32::MAX, u64::MAX, 0, 3)], u64::MAX).unwrap(),
        vec![
            e(u64::MAX, u32::MAX, Kind::Arrive),
            e(
                u64::MAX,
                u32::MAX,
                Kind::Start {
                    remaining: 0,
                    work_at: u64::MAX,
                    finish: u64::MAX
                }
            ),
            e(u64::MAX, u32::MAX, Kind::Finish)
        ]
    );
}
// Checks R17, R24.
#[test]
fn hidden_exact_max_finish_with_cost() {
    assert_eq!(
        dispatches(&simulate(&[c(1, 0, 1, 0), c(2, u64::MAX - 4, 3, 0)], 1).unwrap()).last(),
        Some(&(u64::MAX - 4, 2, 3, u64::MAX - 3, u64::MAX, false))
    );
}
// Checks R8, R19, R23.
#[test]
fn hidden_zero_cohort_dispatch_repeats() {
    let v = simulate(&[c(4, 6, 0, 2), c(5, 6, 0, 0), c(7, 6, 0, 0)], 0).unwrap();
    assert_eq!(v.len(), 9);
    assert_eq!(
        &v[..3],
        &[
            e(6, 7, Kind::Arrive),
            e(6, 5, Kind::Arrive),
            e(6, 4, Kind::Arrive)
        ]
    );
    assert_eq!(
        dispatches(&v).iter().map(|s| s.1).collect::<Vec<_>>(),
        vec![7, 5, 4]
    );
}
// Checks R14, R20, R23.
#[test]
fn hidden_repeated_preemption_residuals() {
    let v = simulate(&[c(1, 0, 12, 3), c(2, 2, 1, 0), c(3, 5, 2, 1)], 0).unwrap();
    let pre = v
        .iter()
        .filter(|e| matches!(e.kind, Kind::Preempt { .. }))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        pre,
        vec![
            e(
                2,
                1,
                Kind::Preempt {
                    remaining: 10,
                    by: 2
                }
            ),
            e(
                5,
                1,
                Kind::Preempt {
                    remaining: 8,
                    by: 3
                }
            )
        ]
    );
    assert_eq!(v.last(), Some(&e(15, 1, Kind::Finish)));
}
// Checks R15, R20, R23.
#[test]
fn hidden_resume_can_be_preempted_during_its_later_work() {
    let v = simulate(&[c(1, 0, 20, 3), c(2, 2, 1, 0), c(3, 6, 1, 1)], 1).unwrap();
    assert_eq!(
        v.iter()
            .filter(|e| e.customer == 1 && matches!(e.kind, Kind::Resume { .. }))
            .count(),
        2
    );
    assert_eq!(v.last(), Some(&e(26, 1, Kind::Finish)));
}
// Checks R3, R25.
#[test]
fn hidden_input_immutable_and_repeatable() {
    let a = vec![c(31, 2, 3, 1), c(30, 0, 4, 2)];
    let b = a.clone();
    let v = simulate(&a, 2);
    assert_eq!(v, simulate(&a, 2));
    assert_eq!(a, b);
}
// Checks R22, R23.
#[test]
fn hidden_no_idle_or_boundary_only_records() {
    let v = simulate(&[c(41, 9, 1, 0), c(42, 20, 2, 0)], 3).unwrap();
    assert_eq!(v.len(), 6);
    assert_eq!(
        v.iter().map(|e| e.time).collect::<Vec<_>>(),
        vec![9, 9, 10, 20, 20, 25]
    );
}
// Checks R5, R6, R9, R16, R19, R23.
#[test]
fn hidden_large_cohort_exact_cost_and_order() {
    let a = (1..=120)
        .map(|id| c(id, 3, 0, (id % 4) as u8))
        .collect::<Vec<_>>();
    let v = simulate(&a, 1).unwrap();
    let st = dispatches(&v);
    assert_eq!(v.len(), 360);
    let expected = (0..4)
        .flat_map(|p| (1..=120).rev().filter(move |id| id % 4 == p))
        .collect::<Vec<_>>();
    assert_eq!(st.iter().map(|x| x.1).collect::<Vec<_>>(), expected);
    assert_eq!(v.last().unwrap().time, 122);
}
