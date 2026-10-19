fn c(id: u32, arrival: u64, service: u64) -> Customer {
    Customer {
        id,
        arrival,
        service,
    }
}
fn e(time: u64, customer: u32, kind: Kind) -> Event {
    Event {
        time,
        customer,
        kind,
    }
}
fn starts(v: &[Event]) -> Vec<(u64, u32, u64, bool)> {
    v.iter()
        .filter_map(|e| {
            if let Kind::Start { finish, wake } = e.kind {
                Some((e.time, e.customer, finish, wake))
            } else {
                None
            }
        })
        .collect()
}
// Checks R1, R2, R3, R19.
#[test]
fn hidden_empty_extreme() {
    assert_eq!(simulate(&[], u64::MAX), Ok(vec![]));
}
// Checks R4.
#[test]
fn hidden_zero_first() {
    assert_eq!(simulate(&[c(0, 4, 2)], 0), Err(Error::ZeroId { index: 0 }));
}
// Checks R4, R18.
#[test]
fn hidden_duplicate_original_index() {
    assert_eq!(
        simulate(&[c(8, 9, 1), c(3, 1, 2), c(8, 0, 2)], 0),
        Err(Error::DuplicateId { index: 2, id: 8 })
    );
}
// Checks R4.
#[test]
fn hidden_zero_before_duplicate_later() {
    assert_eq!(
        simulate(&[c(4, 0, 1), c(0, 1, 1), c(4, 2, 1)], 0),
        Err(Error::ZeroId { index: 1 })
    );
}
// Checks R4.
#[test]
fn hidden_duplicate_before_zero_later() {
    assert_eq!(
        simulate(&[c(4, 0, 1), c(4, 1, 1), c(0, 2, 1)], 0),
        Err(Error::DuplicateId { index: 1, id: 4 })
    );
}
// Checks R4, R12.
#[test]
fn hidden_validation_beats_overflow() {
    assert_eq!(
        simulate(&[c(7, u64::MAX, 5), c(0, 0, 1)], 9),
        Err(Error::ZeroId { index: 1 })
    );
}
// Checks R5, R8, R9, R18.
#[test]
fn hidden_descending_cohort() {
    let v = simulate(&[c(12, 4, 1), c(90, 4, 1), c(31, 4, 1)], 0).unwrap();
    assert_eq!(
        &v[..3],
        &[
            e(4, 90, Kind::Arrive),
            e(4, 31, Kind::Arrive),
            e(4, 12, Kind::Arrive)
        ]
    );
    assert_eq!(
        starts(&v).iter().map(|s| s.1).collect::<Vec<_>>(),
        vec![90, 31, 12]
    );
}
// Checks R5, R6, R9, R14.
#[test]
fn hidden_old_waiter_ahead_of_new_cohort() {
    assert_eq!(
        starts(&simulate(&[c(11, 0, 6), c(2, 2, 1), c(99, 6, 1)], 0).unwrap())
            .iter()
            .map(|s| s.1)
            .collect::<Vec<_>>(),
        vec![11, 2, 99]
    );
}
// Checks R7, R8, R9, R10, R16.
#[test]
fn hidden_finish_arrive_start_boundary() {
    let v = simulate(&[c(21, 0, 3), c(22, 3, 1)], 0).unwrap();
    assert_eq!(
        &v[2..5],
        &[
            e(3, 21, Kind::Finish),
            e(3, 22, Kind::Arrive),
            e(
                3,
                22,
                Kind::Start {
                    finish: 4,
                    wake: false
                }
            )
        ]
    );
}
// Checks R10, R11.
#[test]
fn hidden_contiguous_suppresses_wake() {
    assert_eq!(
        starts(&simulate(&[c(6, 0, 2), c(7, 5, 2)], 3).unwrap()),
        vec![(0, 6, 5, true), (5, 7, 7, false)]
    );
}
// Checks R10, R11.
#[test]
fn hidden_strict_gap_reawakens() {
    assert_eq!(
        starts(&simulate(&[c(6, 0, 2), c(7, 6, 2)], 3).unwrap()),
        vec![(0, 6, 5, true), (6, 7, 11, true)]
    );
}
// Checks R6, R11, R14.
#[test]
fn hidden_arrival_inside_surcharge() {
    assert_eq!(
        starts(&simulate(&[c(13, 0, 1), c(14, 3, 2)], 5).unwrap()),
        vec![(0, 13, 6, true), (6, 14, 8, false)]
    );
}
// Checks R8, R9, R10, R13, R16.
#[test]
fn hidden_zero_cohort_inline() {
    assert_eq!(
        simulate(&[c(1, 7, 0), c(9, 7, 0)], 0).unwrap(),
        vec![
            e(7, 9, Kind::Arrive),
            e(7, 1, Kind::Arrive),
            e(
                7,
                9,
                Kind::Start {
                    finish: 7,
                    wake: true
                }
            ),
            e(7, 9, Kind::Finish),
            e(
                7,
                1,
                Kind::Start {
                    finish: 7,
                    wake: false
                }
            ),
            e(7, 1, Kind::Finish)
        ]
    );
}
// Checks R11, R13.
#[test]
fn hidden_zero_service_with_wake_is_occupied() {
    assert_eq!(
        starts(&simulate(&[c(15, 1, 0), c(16, 2, 0)], 4).unwrap()),
        vec![(1, 15, 5, true), (5, 16, 5, false)]
    );
}
// Checks R12, R17.
#[test]
fn hidden_max_time_zero_is_legal() {
    assert_eq!(simulate(&[c(19, u64::MAX, 0)], 0).unwrap().len(), 3);
}
// Checks R11, R12, R17.
#[test]
fn hidden_exact_max_finish() {
    assert_eq!(
        starts(&simulate(&[c(29, u64::MAX - 8, 5)], 3).unwrap()),
        vec![(u64::MAX - 8, 29, u64::MAX, true)]
    );
}
// Checks R12.
#[test]
fn hidden_service_overflow() {
    assert_eq!(
        simulate(&[c(25, u64::MAX, 1)], 0),
        Err(Error::TimeOverflow {
            customer: 25,
            time: u64::MAX
        })
    );
}
// Checks R12.
#[test]
fn hidden_surcharge_overflow() {
    assert_eq!(
        simulate(&[c(26, u64::MAX - 2, 1)], 2),
        Err(Error::TimeOverflow {
            customer: 26,
            time: u64::MAX - 2
        })
    );
}
// Checks R10, R12, R17.
#[test]
fn hidden_no_surcharge_on_max_boundary() {
    assert_eq!(
        starts(&simulate(&[c(31, 0, u64::MAX), c(32, u64::MAX, 0)], 0).unwrap()).last(),
        Some(&(u64::MAX, 32, u64::MAX, false))
    );
}
// Checks R12, R20.
#[test]
fn hidden_overflow_dispatch_not_input_order() {
    assert_eq!(
        simulate(&[c(40, u64::MAX, 8), c(41, u64::MAX - 2, 5)], 0),
        Err(Error::TimeOverflow {
            customer: 41,
            time: u64::MAX - 2
        })
    );
}
// Checks R12, R14.
#[test]
fn hidden_queued_work_can_overflow() {
    assert_eq!(
        simulate(&[c(51, 0, u64::MAX), c(52, 1, 1)], 0),
        Err(Error::TimeOverflow {
            customer: 52,
            time: u64::MAX
        })
    );
}
// Checks R6, R15, R16.
#[test]
fn hidden_no_spurious_idle_events() {
    let v = simulate(&[c(66, 91, 2), c(67, 101, 2)], 0).unwrap();
    assert_eq!(v.len(), 6);
    assert_eq!(
        v.iter().map(|e| e.time).collect::<Vec<_>>(),
        vec![91, 91, 93, 101, 101, 103]
    );
}
// Checks R5, R16, R18.
#[test]
fn hidden_high_ids_and_unsorted_times() {
    assert_eq!(
        starts(&simulate(&[c(u32::MAX, 9, 1), c(2, 1, 2), c(1, 0, 2)], 0).unwrap())
            .iter()
            .map(|s| s.1)
            .collect::<Vec<_>>(),
        vec![1, 2, u32::MAX]
    );
}
// Checks R3, R19.
#[test]
fn hidden_independent_calls_and_immutable_input() {
    let a = vec![c(72, 2, 3)];
    let b = a.clone();
    let v = simulate(&a, 1);
    assert_eq!(simulate(&a, 1), v);
    assert_eq!(a, b);
    assert_eq!(starts(&simulate(&a, 0).unwrap()), vec![(2, 72, 5, true)]);
}
// Checks R5, R6, R8, R9, R13, R16.
#[test]
fn hidden_long_zero_queue() {
    let a = (1..=180).map(|id| c(id, 12, 0)).collect::<Vec<_>>();
    let v = simulate(&a, 0).unwrap();
    assert_eq!(v.len(), 540);
    assert!(v[..180].iter().all(|e| e.kind == Kind::Arrive));
    assert_eq!(
        starts(&v).iter().map(|s| s.1).collect::<Vec<_>>(),
        (1..=180).rev().collect::<Vec<_>>()
    );
}
// Checks R5, R6, R10, R11, R14, R16.
#[test]
fn hidden_long_positive_queue() {
    let a = (1..=100)
        .rev()
        .map(|id| c(id, (id - 1) as u64, 2))
        .collect::<Vec<_>>();
    let v = simulate(&a, 3).unwrap();
    assert_eq!(v.len(), 300);
    assert_eq!(starts(&v).last(), Some(&(201, 100, 203, false)));
    assert!(v.windows(2).all(|w| w[0].time <= w[1].time));
}
