#[test]
fn spec_r1_single_source_std_only_api() {
    let customers = [Customer {
        id: 1,
        arrival: 0,
        service: 0,
    }];
    let servers = [Server {
        id: 1,
        opens: 0,
        overhead: 0,
    }];
    assert!(simulate(&customers, &servers).is_ok());
}

#[test]
fn spec_r2_public_types_and_fields() {
    let customer = Customer {
        id: 7,
        arrival: 3,
        service: 4,
    };
    let server = Server {
        id: 7,
        opens: 5,
        overhead: 6,
    };
    assert_eq!(customer.id, 7);
    assert_eq!(server.id, 7);
    assert_eq!(
        Event::Open {
            time: 5,
            server: 7
        },
        Event::Open {
            time: 5,
            server: 7
        }
    );
    assert_eq!(
        Error::NoServers,
        Error::NoServers
    );
}

#[test]
fn spec_r3_unsorted_inputs_and_separate_id_namespaces() {
    let events = simulate(
        &[
            Customer {
                id: 1,
                arrival: 2,
                service: 0,
            },
            Customer {
                id: 2,
                arrival: 1,
                service: 0,
            },
        ],
        &[
            Server {
                id: 2,
                opens: 0,
                overhead: 0,
            },
            Server {
                id: 1,
                opens: 0,
                overhead: 0,
            },
        ],
    )
    .unwrap();
    assert_eq!(
        events,
        vec![
            Event::Open {
                time: 0,
                server: 2
            },
            Event::Open {
                time: 0,
                server: 1
            },
            Event::Arrive {
                time: 1,
                customer: 2
            },
            Event::Start {
                time: 1,
                customer: 2,
                server: 2,
                finish: 1
            },
            Event::Finish {
                time: 1,
                customer: 2,
                server: 2
            },
            Event::Arrive {
                time: 2,
                customer: 1
            },
            Event::Start {
                time: 2,
                customer: 1,
                server: 2,
                finish: 2
            },
            Event::Finish {
                time: 2,
                customer: 1,
                server: 2
            },
        ]
    );
}

#[test]
fn spec_r4_server_count_errors_precede_row_validation() {
    assert_eq!(simulate(&[], &[]), Err(Error::NoServers));

    let many: Vec<Server> = (0..33)
        .map(|_| Server {
            id: 0,
            opens: 0,
            overhead: 0,
        })
        .collect();
    assert_eq!(
        simulate(&[], &many),
        Err(Error::TooManyServers { count: 33 })
    );
}

#[test]
fn spec_r5_server_validation_order_and_first_failure() {
    assert_eq!(
        simulate(
            &[],
            &[
                Server {
                    id: 9,
                    opens: 0,
                    overhead: 0,
                },
                Server {
                    id: 9,
                    opens: 0,
                    overhead: 0,
                },
                Server {
                    id: 0,
                    opens: 0,
                    overhead: 0,
                },
            ],
        ),
        Err(Error::DuplicateServerId { index: 1, id: 9 })
    );

    assert_eq!(
        simulate(
            &[],
            &[
                Server {
                    id: 0,
                    opens: 0,
                    overhead: 0,
                },
                Server {
                    id: 0,
                    opens: 0,
                    overhead: 0,
                },
            ],
        ),
        Err(Error::ZeroServerId { index: 0 })
    );
}

#[test]
fn spec_r6_customer_validation_after_servers_and_before_runtime() {
    assert_eq!(
        simulate(
            &[
                Customer {
                    id: 3,
                    arrival: 0,
                    service: 0,
                },
                Customer {
                    id: 3,
                    arrival: u64::MAX,
                    service: 1,
                },
                Customer {
                    id: 0,
                    arrival: 0,
                    service: 0,
                },
            ],
            &[Server {
                id: 1,
                opens: 0,
                overhead: 0,
            }],
        ),
        Err(Error::DuplicateCustomerId { index: 1, id: 3 })
    );

    assert_eq!(
        simulate(
            &[Customer {
                id: 0,
                arrival: 0,
                service: 0,
            }],
            &[Server {
                id: 1,
                opens: 0,
                overhead: 0,
            }],
        ),
        Err(Error::ZeroCustomerId { index: 0 })
    );

    assert_eq!(
        simulate(
            &[Customer {
                id: 0,
                arrival: 0,
                service: 0,
            }],
            &[Server {
                id: 0,
                opens: 0,
                overhead: 0,
            }],
        ),
        Err(Error::ZeroServerId { index: 0 })
    );
}

#[test]
fn spec_r7_time_arithmetic_checked_only_when_dispatched() {
    let events = simulate(
        &[Customer {
            id: 1,
            arrival: u64::MAX,
            service: 0,
        }],
        &[Server {
            id: 1,
            opens: 0,
            overhead: 0,
        }],
    )
    .unwrap();
    assert_eq!(
        events,
        vec![
            Event::Open {
                time: 0,
                server: 1
            },
            Event::Arrive {
                time: u64::MAX,
                customer: 1
            },
            Event::Start {
                time: u64::MAX,
                customer: 1,
                server: 1,
                finish: u64::MAX
            },
            Event::Finish {
                time: u64::MAX,
                customer: 1,
                server: 1
            },
        ]
    );

    let customers: Vec<Customer> = (1..=40)
        .map(|id| Customer {
            id,
            arrival: 0,
            service: 0,
        })
        .collect();
    assert_eq!(
        simulate(
            &customers,
            &[Server {
                id: 1,
                opens: 0,
                overhead: 0,
            }],
        )
        .unwrap()
        .iter()
        .filter(|event| matches!(event, Event::Start { .. }))
        .count(),
        40
    );
}

#[test]
fn spec_r8_every_server_opens_once_even_without_customers() {
    assert_eq!(
        simulate(
            &[],
            &[
                Server {
                    id: 4,
                    opens: 8,
                    overhead: 0,
                },
                Server {
                    id: 2,
                    opens: 3,
                    overhead: 0,
                },
            ],
        ),
        Ok(vec![
            Event::Open {
                time: 3,
                server: 2
            },
            Event::Open {
                time: 8,
                server: 4
            }
        ])
    );
}

#[test]
fn spec_r9_same_tick_phase_order_and_id_order() {
    let events = simulate(
        &[
            Customer {
                id: 1,
                arrival: 0,
                service: 2,
            },
            Customer {
                id: 3,
                arrival: 0,
                service: 0,
            },
            Customer {
                id: 2,
                arrival: 0,
                service: 0,
            },
        ],
        &[
            Server {
                id: 1,
                opens: 0,
                overhead: 0,
            },
            Server {
                id: 3,
                opens: 0,
                overhead: 0,
            },
            Server {
                id: 2,
                opens: 0,
                overhead: 0,
            },
        ],
    )
    .unwrap();
    assert_eq!(
        events,
        vec![
            Event::Open {
                time: 0,
                server: 3
            },
            Event::Open {
                time: 0,
                server: 2
            },
            Event::Open {
                time: 0,
                server: 1
            },
            Event::Arrive {
                time: 0,
                customer: 3
            },
            Event::Arrive {
                time: 0,
                customer: 2
            },
            Event::Arrive {
                time: 0,
                customer: 1
            },
            Event::Start {
                time: 0,
                customer: 3,
                server: 3,
                finish: 0
            },
            Event::Finish {
                time: 0,
                customer: 3,
                server: 3
            },
            Event::Start {
                time: 0,
                customer: 2,
                server: 3,
                finish: 0
            },
            Event::Finish {
                time: 0,
                customer: 2,
                server: 3
            },
            Event::Start {
                time: 0,
                customer: 1,
                server: 3,
                finish: 2
            },
            Event::Finish {
                time: 2,
                customer: 1,
                server: 3
            },
        ]
    );
}

#[test]
fn spec_r10_fifo_arrival_cohort_descending_ids_without_skipping() {
    let events = simulate(
        &[
            Customer {
                id: 1,
                arrival: 0,
                service: 5,
            },
            Customer {
                id: 2,
                arrival: 0,
                service: 0,
            },
            Customer {
                id: 3,
                arrival: 0,
                service: 0,
            },
            Customer {
                id: 4,
                arrival: 1,
                service: 0,
            },
        ],
        &[Server {
            id: 1,
            opens: 0,
            overhead: 0,
        }],
    )
    .unwrap();
    let starts: Vec<(u64, u32)> = events
        .iter()
        .filter_map(|event| match event {
            Event::Start {
                time, customer, ..
            } => Some((*time, *customer)),
            _ => None,
        })
        .collect();
    assert_eq!(starts, vec![(0, 3), (5, 2), (5, 1), (5, 4)]);
}

#[test]
fn spec_r11_idle_since_is_open_time_then_latest_finish() {
    let events = simulate(
        &[
            Customer {
                id: 1,
                arrival: 0,
                service: 2,
            },
            Customer {
                id: 2,
                arrival: 0,
                service: 0,
            },
        ],
        &[
            Server {
                id: 1,
                opens: 0,
                overhead: 0,
            },
            Server {
                id: 2,
                opens: 4,
                overhead: 0,
            },
        ],
    )
    .unwrap();
    let starts: Vec<(u64, u32, u32)> = events
        .iter()
        .filter_map(|event| match event {
            Event::Start {
                time,
                customer,
                server,
                ..
            } => Some((*time, *customer, *server)),
            _ => None,
        })
        .collect();
    assert_eq!(starts, vec![(0, 1, 1), (4, 2, 2)]);
}

#[test]
fn spec_r12_server_ranking_idle_time_overhead_then_greatest_id() {
    let events = simulate(
        &[Customer {
            id: 1,
            arrival: 5,
            service: 0,
        }],
        &[
            Server {
                id: 10,
                opens: 5,
                overhead: 3,
            },
            Server {
                id: 8,
                opens: 5,
                overhead: 1,
            },
            Server {
                id: 9,
                opens: 5,
                overhead: 1,
            },
        ],
    )
    .unwrap();
    assert!(events.contains(&Event::Start {
        time: 5,
        customer: 1,
        server: 9,
        finish: 6
    }));

    let events = simulate(
        &[Customer {
            id: 1,
            arrival: 0,
            service: 0,
        }],
        &[
            Server {
                id: 2,
                opens: 0,
                overhead: 0,
            },
            Server {
                id: 1,
                opens: 0,
                overhead: 0,
            },
        ],
    )
    .unwrap();
    assert!(events.contains(&Event::Start {
        time: 0,
        customer: 1,
        server: 2,
        finish: 0
    }));
}

#[test]
fn spec_r13_every_start_includes_overhead_with_ordered_additions() {
    let events = simulate(
        &[
            Customer {
                id: 1,
                arrival: 0,
                service: 2,
            },
            Customer {
                id: 2,
                arrival: 0,
                service: 0,
            },
        ],
        &[Server {
            id: 1,
            opens: 0,
            overhead: 3,
        }],
    )
    .unwrap();
    let starts: Vec<(u64, u64)> = events
        .iter()
        .filter_map(|event| match event {
            Event::Start { time, finish, .. } => Some((*time, *finish)),
            _ => None,
        })
        .collect();
    assert_eq!(starts, vec![(0, 5), (5, 8)]);
}

#[test]
fn spec_r14_overflow_reports_selected_dispatch_without_retry() {
    assert_eq!(
        simulate(
            &[Customer {
                id: 7,
                arrival: 0,
                service: 1,
            }],
            &[
                Server {
                    id: 1,
                    opens: 0,
                    overhead: 0,
                },
                Server {
                    id: 2,
                    opens: 0,
                    overhead: 0,
                },
            ],
        ),
        Err(Error::TimeOverflow {
            customer: 7,
            server: 2,
            time: 0
        })
    );

    assert_eq!(
        simulate(
            &[Customer {
                id: 8,
                arrival: 0,
                service: 0,
            }],
            &[Server {
                id: 1,
                opens: 0,
                overhead: 1,
            }],
        ),
        Err(Error::TimeOverflow {
            customer: 8,
            server: 1,
            time: 0
        })
    );
}

#[test]
fn spec_r15_inline_finish_does_not_reenter_completion_phase() {
    let events = simulate(
        &[
            Customer {
                id: 1,
                arrival: 0,
                service: 0,
            },
            Customer {
                id: 2,
                arrival: 0,
                service: 0,
            },
        ],
        &[Server {
            id: 1,
            opens: 0,
            overhead: 0,
        }],
    )
    .unwrap();
    assert_eq!(
        events,
        vec![
            Event::Open {
                time: 0,
                server: 1
            },
            Event::Arrive {
                time: 0,
                customer: 2
            },
            Event::Arrive {
                time: 0,
                customer: 1
            },
            Event::Start {
                time: 0,
                customer: 2,
                server: 1,
                finish: 0
            },
            Event::Finish {
                time: 0,
                customer: 2,
                server: 1
            },
            Event::Start {
                time: 0,
                customer: 1,
                server: 1,
                finish: 0
            },
            Event::Finish {
                time: 0,
                customer: 1,
                server: 1
            },
        ]
    );
}

#[test]
fn spec_r16_recompute_ranking_after_each_start() {
    let events = simulate(
        &[
            Customer {
                id: 1,
                arrival: 0,
                service: 1,
            },
            Customer {
                id: 2,
                arrival: 0,
                service: 0,
            },
        ],
        &[
            Server {
                id: 1,
                opens: 0,
                overhead: 0,
            },
            Server {
                id: 2,
                opens: 0,
                overhead: 0,
            },
        ],
    )
    .unwrap();
    let starts: Vec<(u32, u32)> = events
        .iter()
        .filter_map(|event| match event {
            Event::Start {
                customer, server, ..
            } => Some((*customer, *server)),
            _ => None,
        })
        .collect();
    assert_eq!(starts, vec![(2, 2), (1, 1)]);
}

#[test]
fn spec_r17_advance_to_next_event_without_idle_records() {
    let events = simulate(
        &[],
        &[
            Server {
                id: 1,
                opens: 10,
                overhead: 0,
            },
            Server {
                id: 2,
                opens: 3,
                overhead: 0,
            },
        ],
    )
    .unwrap();
    assert_eq!(
        events,
        vec![
            Event::Open {
                time: 3,
                server: 2
            },
            Event::Open {
                time: 10,
                server: 1
            }
        ]
    );
}

#[test]
fn spec_r18_arrivals_during_occupation_and_before_opening_are_emitted() {
    let events = simulate(
        &[
            Customer {
                id: 1,
                arrival: 0,
                service: 5,
            },
            Customer {
                id: 2,
                arrival: 2,
                service: 0,
            },
        ],
        &[Server {
            id: 1,
            opens: 0,
            overhead: 0,
        }],
    )
    .unwrap();
    assert_eq!(
        events,
        vec![
            Event::Open {
                time: 0,
                server: 1
            },
            Event::Arrive {
                time: 0,
                customer: 1
            },
            Event::Start {
                time: 0,
                customer: 1,
                server: 1,
                finish: 5
            },
            Event::Arrive {
                time: 2,
                customer: 2
            },
            Event::Finish {
                time: 5,
                customer: 1,
                server: 1
            },
            Event::Start {
                time: 5,
                customer: 2,
                server: 1,
                finish: 5
            },
            Event::Finish {
                time: 5,
                customer: 2,
                server: 1
            },
        ]
    );
}

#[test]
fn spec_r19_trace_has_exact_event_counts_and_nondecreasing_times() {
    let events = simulate(
        &[
            Customer {
                id: 1,
                arrival: 0,
                service: 1,
            },
            Customer {
                id: 2,
                arrival: 1,
                service: 0,
            },
        ],
        &[
            Server {
                id: 1,
                opens: 0,
                overhead: 0,
            },
            Server {
                id: 2,
                opens: 1,
                overhead: 0,
            },
        ],
    )
    .unwrap();
    let times: Vec<u64> = events
        .iter()
        .map(|event| match event {
            Event::Open { time, .. }
            | Event::Arrive { time, .. }
            | Event::Start { time, .. }
            | Event::Finish { time, .. } => *time,
        })
        .collect();
    assert!(times.windows(2).all(|pair| pair[0] <= pair[1]));
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::Open { .. }))
            .count(),
        2
    );
    for customer in [1, 2] {
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, Event::Arrive { customer: id, .. } if *id == customer))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, Event::Start { customer: id, .. } if *id == customer))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, Event::Finish { customer: id, .. } if *id == customer))
                .count(),
            1
        );
    }
}

#[test]
fn spec_r20_max_boundary_open_and_zero_duration_service() {
    assert_eq!(
        simulate(
            &[Customer {
                id: 1,
                arrival: u64::MAX,
                service: 0,
            }],
            &[Server {
                id: 1,
                opens: u64::MAX,
                overhead: 0,
            }],
        ),
        Ok(vec![
            Event::Open {
                time: u64::MAX,
                server: 1
            },
            Event::Arrive {
                time: u64::MAX,
                customer: 1
            },
            Event::Start {
                time: u64::MAX,
                customer: 1,
                server: 1,
                finish: u64::MAX
            },
            Event::Finish {
                time: u64::MAX,
                customer: 1,
                server: 1
            },
        ])
    );
}

#[test]
fn spec_r21_repeated_calls_are_independent_and_inputs_unchanged() {
    let customers = [Customer {
        id: 4,
        arrival: 1,
        service: 0,
    }];
    let servers = [Server {
        id: 4,
        opens: 0,
        overhead: 0,
    }];
    let customers_before = customers.clone();
    let servers_before = servers.clone();

    let first = simulate(&customers, &servers);
    let second = simulate(&customers, &servers);

    assert_eq!(first, second);
    assert_eq!(customers, customers_before);
    assert_eq!(servers, servers_before);
}

#[test]
fn spec_r22_runtime_errors_follow_dispatch_order_and_return_only_error() {
    assert_eq!(
        simulate(
            &[
                Customer {
                    id: 1,
                    arrival: 10,
                    service: 1,
                },
                Customer {
                    id: 2,
                    arrival: 0,
                    service: 1,
                },
            ],
            &[Server {
                id: 1,
                opens: 0,
                overhead: 0,
            }],
        ),
        Err(Error::TimeOverflow {
            customer: 1,
            server: 1,
            time: 10
        })
    );

    assert_eq!(
        simulate(
            &[
                Customer {
                    id: 1,
                    arrival: 0,
                    service: 0,
                },
                Customer {
                    id: 2,
                    arrival: 0,
                    service: 1,
                },
            ],
            &[Server {
                id: 1,
                opens: u64::MAX,
                overhead: 0,
            }],
        ),
        Err(Error::TimeOverflow {
            customer: 2,
            server: 1,
            time: u64::MAX
        })
    );
}
