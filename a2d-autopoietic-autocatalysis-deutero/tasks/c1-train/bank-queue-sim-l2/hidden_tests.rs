fn c(id: u32, arrival: u64, service: u64) -> Customer {
    Customer {
        id,
        arrival,
        service,
    }
}
fn s(id: u32, opens: u64, overhead: u64) -> Server {
    Server {
        id,
        opens,
        overhead,
    }
}
fn starts(v: &[Event]) -> Vec<(u64, u32, u32, u64)> {
    v.iter()
        .filter_map(|e| {
            if let Event::Start {
                time,
                customer,
                server,
                finish,
            } = *e
            {
                Some((time, customer, server, finish))
            } else {
                None
            }
        })
        .collect()
}
// Checks R1, R2, R4.
#[test]
fn hidden_no_servers_precedence() {
    assert_eq!(simulate(&[c(0, 0, 0)], &[]), Err(Error::NoServers));
}
// Checks R4, R5.
#[test]
fn hidden_server_limit_precedence() {
    assert_eq!(
        simulate(&[], &vec![s(0, 0, 0); 33]),
        Err(Error::TooManyServers { count: 33 })
    );
}
// Checks R4, R8, R19.
#[test]
fn hidden_thirty_two_is_legal() {
    assert_eq!(
        simulate(&[], &(1..=32).map(|id| s(id, 0, 0)).collect::<Vec<_>>())
            .unwrap()
            .len(),
        32
    );
}
// Checks R5, R6.
#[test]
fn hidden_server_zero_before_customer() {
    assert_eq!(
        simulate(&[c(0, 0, 0)], &[s(4, 0, 0), s(0, 1, 0)]),
        Err(Error::ZeroServerId { index: 1 })
    );
}
// Checks R5.
#[test]
fn hidden_duplicate_server_original_index() {
    assert_eq!(
        simulate(&[], &[s(4, 8, 0), s(3, 0, 0), s(4, 1, 0)]),
        Err(Error::DuplicateServerId { index: 2, id: 4 })
    );
}
// Checks R5.
#[test]
fn hidden_earliest_server_error() {
    assert_eq!(
        simulate(&[], &[s(7, 0, 0), s(7, 1, 0), s(0, 0, 0)]),
        Err(Error::DuplicateServerId { index: 1, id: 7 })
    );
}
// Checks R6.
#[test]
fn hidden_customer_errors_original_order() {
    assert_eq!(
        simulate(&[c(6, 8, 0), c(6, 0, 0), c(0, 0, 0)], &[s(1, 0, 0)]),
        Err(Error::DuplicateCustomerId { index: 1, id: 6 })
    );
    assert_eq!(
        simulate(&[c(5, 8, 0), c(0, 0, 0)], &[s(1, 0, 0)]),
        Err(Error::ZeroCustomerId { index: 1 })
    );
}
// Checks R6, R7, R14.
#[test]
fn hidden_validation_before_overflow() {
    assert_eq!(
        simulate(&[c(8, u64::MAX, 1), c(8, 0, 0)], &[s(2, 0, 0)]),
        Err(Error::DuplicateCustomerId { index: 1, id: 8 })
    );
}
// Checks R3, R12.
#[test]
fn hidden_namespaces_overlap() {
    assert_eq!(
        starts(&simulate(&[c(9, 0, 1)], &[s(9, 0, 0)]).unwrap()),
        vec![(0, 9, 9, 1)]
    );
}
// Checks R3, R8, R9, R19.
#[test]
fn hidden_open_empty_order() {
    assert_eq!(
        simulate(&[], &[s(2, 6, 0), s(11, 6, 1), s(3, 2, 0)]).unwrap(),
        vec![
            Event::Open { time: 2, server: 3 },
            Event::Open {
                time: 6,
                server: 11
            },
            Event::Open { time: 6, server: 2 }
        ]
    );
}
// Checks R8, R17.
#[test]
fn hidden_late_open_after_every_finish() {
    let v = simulate(&[c(4, 0, 1)], &[s(2, 0, 0), s(9, 100, 0)]).unwrap();
    assert_eq!(
        v.last(),
        Some(&Event::Open {
            time: 100,
            server: 9
        })
    );
}
// Checks R9, R11, R12.
#[test]
fn hidden_all_three_phases_at_tick() {
    let v = simulate(&[c(3, 0, 5), c(7, 5, 2)], &[s(2, 0, 0), s(9, 5, 0)]).unwrap();
    assert_eq!(
        &v[3..7],
        &[
            Event::Finish {
                time: 5,
                customer: 3,
                server: 2
            },
            Event::Open { time: 5, server: 9 },
            Event::Arrive {
                time: 5,
                customer: 7
            },
            Event::Start {
                time: 5,
                customer: 7,
                server: 9,
                finish: 7
            }
        ]
    );
}
// Checks R9, R10, R12.
#[test]
fn hidden_tied_arrival_fifo() {
    assert_eq!(
        starts(&simulate(&[c(4, 3, 2), c(14, 3, 2), c(8, 3, 2)], &[s(1, 0, 0)]).unwrap())
            .iter()
            .map(|s| s.1)
            .collect::<Vec<_>>(),
        vec![14, 8, 4]
    );
}
// Checks R10, R17, R18.
#[test]
fn hidden_queue_before_open_fifo() {
    assert_eq!(
        starts(&simulate(&[c(6, 2, 2), c(4, 1, 2), c(99, 8, 2)], &[s(3, 8, 0)]).unwrap())
            .iter()
            .map(|s| s.1)
            .collect::<Vec<_>>(),
        vec![4, 6, 99]
    );
}
// Checks R11, R12.
#[test]
fn hidden_oldest_idle_beats_overhead_and_id() {
    assert_eq!(
        starts(&simulate(&[c(10, 10, 1)], &[s(3, 1, 5), s(99, 2, 0)]).unwrap()),
        vec![(10, 10, 3, 16)]
    );
}
// Checks R12, R13.
#[test]
fn hidden_overhead_tie_break() {
    assert_eq!(
        starts(&simulate(&[c(20, 3, 1)], &[s(5, 0, 1), s(99, 0, 2), s(3, 0, 1)]).unwrap()),
        vec![(3, 20, 5, 5)]
    );
}
// Checks R12.
#[test]
fn hidden_id_tie_independent_of_input() {
    assert_eq!(
        starts(&simulate(&[c(21, 3, 1)], &[s(80, 0, 0), s(7, 0, 0), s(50, 0, 0)]).unwrap()),
        vec![(3, 21, 80, 4)]
    );
}
// Checks R11, R12, R16.
#[test]
fn hidden_idle_age_resets_on_finish() {
    assert_eq!(
        starts(&simulate(&[c(1, 0, 2), c(2, 5, 1)], &[s(7, 0, 0), s(8, 0, 0)]).unwrap()),
        vec![(0, 1, 8, 2), (5, 2, 7, 6)]
    );
}
// Checks R13.
#[test]
fn hidden_overhead_every_start() {
    assert_eq!(
        starts(&simulate(&[c(12, 0, 1), c(13, 1, 0)], &[s(4, 0, 3)]).unwrap()),
        vec![(0, 12, 4, 4), (4, 13, 4, 7)]
    );
}
// Checks R9, R12, R15, R16.
#[test]
fn hidden_inline_zero_after_arrival_cohort() {
    let v = simulate(&[c(30, 0, 0), c(31, 0, 0)], &[s(7, 0, 0), s(8, 0, 0)]).unwrap();
    assert_eq!(
        &v[2..4],
        &[
            Event::Arrive {
                time: 0,
                customer: 31
            },
            Event::Arrive {
                time: 0,
                customer: 30
            }
        ]
    );
    assert_eq!(starts(&v), vec![(0, 31, 8, 0), (0, 30, 8, 0)]);
}
// Checks R11, R12, R15, R16.
#[test]
fn hidden_zero_recompute_old_idle_first() {
    assert_eq!(
        starts(
            &simulate(
                &[c(10, 4, 0), c(9, 4, 0), c(8, 4, 0)],
                &[s(4, 0, 0), s(5, 0, 0)]
            )
            .unwrap()
        ),
        vec![(4, 10, 5, 4), (4, 9, 4, 4), (4, 8, 5, 4)]
    );
}
// Checks R9.
#[test]
fn hidden_completion_server_order() {
    let v = simulate(
        &[c(51, 0, 6), c(50, 0, 6), c(49, 0, 6)],
        &[s(20, 0, 0), s(7, 0, 0), s(11, 0, 0)],
    )
    .unwrap();
    assert_eq!(
        &v[v.len() - 3..],
        &[
            Event::Finish {
                time: 6,
                customer: 49,
                server: 7
            },
            Event::Finish {
                time: 6,
                customer: 50,
                server: 11
            },
            Event::Finish {
                time: 6,
                customer: 51,
                server: 20
            }
        ]
    );
}
// Checks R7, R14, R22.
#[test]
fn hidden_service_overflow_dispatch_position() {
    assert_eq!(
        simulate(&[c(6, u64::MAX, 1), c(7, 0, u64::MAX)], &[s(3, 0, 0)]),
        Err(Error::TimeOverflow {
            customer: 6,
            server: 3,
            time: u64::MAX
        })
    );
}
// Checks R13, R14.
#[test]
fn hidden_overhead_overflow() {
    assert_eq!(
        simulate(&[c(64, u64::MAX - 2, 2)], &[s(4, 0, 1)]),
        Err(Error::TimeOverflow {
            customer: 64,
            server: 4,
            time: u64::MAX - 2
        })
    );
}
// Checks R12, R14.
#[test]
fn hidden_selected_server_is_not_retried() {
    assert_eq!(
        simulate(
            &[c(65, u64::MAX, 0)],
            &[s(9, 0, 0), s(3, 0, 1), s(2, u64::MAX, 0)]
        ),
        Ok(vec![
            Event::Open { time: 0, server: 9 },
            Event::Open { time: 0, server: 3 },
            Event::Open {
                time: u64::MAX,
                server: 2
            },
            Event::Arrive {
                time: u64::MAX,
                customer: 65
            },
            Event::Start {
                time: u64::MAX,
                customer: 65,
                server: 9,
                finish: u64::MAX
            },
            Event::Finish {
                time: u64::MAX,
                customer: 65,
                server: 9
            }
        ])
    );
    assert_eq!(
        simulate(&[c(65, u64::MAX, 0)], &[s(3, 0, 1), s(9, 1, 0)]),
        Err(Error::TimeOverflow {
            customer: 65,
            server: 3,
            time: u64::MAX
        })
    );
}
// Checks R20.
#[test]
fn hidden_max_open_and_zero_finish() {
    assert_eq!(
        starts(&simulate(&[c(70, u64::MAX, 0)], &[s(72, u64::MAX, 0)]).unwrap()),
        vec![(u64::MAX, 70, 72, u64::MAX)]
    );
}
// Checks R19, R21.
#[test]
fn hidden_independence_and_inputs() {
    let a = vec![c(77, 4, 3)];
    let b = vec![s(8, 0, 0)];
    let copies = (a.clone(), b.clone());
    let v = simulate(&a, &b);
    assert_eq!(v, simulate(&a, &b));
    assert_eq!((a, b), copies);
}
// Checks R7, R10, R12, R13, R17, R19.
#[test]
fn hidden_many_customers_exact_throughput() {
    let a = (1..=200).map(|id| c(id, 0, 2)).collect::<Vec<_>>();
    let v = simulate(&a, &[s(1, 0, 1), s(2, 0, 1), s(3, 0, 1), s(4, 0, 1)]).unwrap();
    let st = starts(&v);
    assert_eq!(v.len(), 604);
    assert_eq!(st.len(), 200);
    for (k, x) in st.iter().enumerate() {
        assert_eq!(x.0, (k / 4 * 3) as u64);
        assert_eq!(x.1, 200 - k as u32);
        assert_eq!(x.3, x.0 + 3);
    }
    assert!(v.windows(2).all(|w| {
        fn time(e: &Event) -> u64 {
            match e {
                Event::Open { time, .. }
                | Event::Arrive { time, .. }
                | Event::Start { time, .. }
                | Event::Finish { time, .. } => *time,
            }
        }
        time(&w[0]) <= time(&w[1])
    }));
}
