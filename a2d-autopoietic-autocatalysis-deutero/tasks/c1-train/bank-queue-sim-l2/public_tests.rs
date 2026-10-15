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
#[test]
fn public_two_servers() {
    assert_eq!(
        simulate(&[c(1, 0, 3), c(2, 1, 2)], &[s(1, 0, 0), s(2, 0, 0)]).unwrap(),
        vec![
            Event::Open { time: 0, server: 2 },
            Event::Open { time: 0, server: 1 },
            Event::Arrive {
                time: 0,
                customer: 1
            },
            Event::Start {
                time: 0,
                customer: 1,
                server: 2,
                finish: 3
            },
            Event::Arrive {
                time: 1,
                customer: 2
            },
            Event::Start {
                time: 1,
                customer: 2,
                server: 1,
                finish: 3
            },
            Event::Finish {
                time: 3,
                customer: 2,
                server: 1
            },
            Event::Finish {
                time: 3,
                customer: 1,
                server: 2
            }
        ]
    );
}
#[test]
fn public_wait_for_open() {
    let v = simulate(&[c(8, 2, 4)], &[s(5, 7, 1)]).unwrap();
    assert_eq!(
        v,
        vec![
            Event::Arrive {
                time: 2,
                customer: 8
            },
            Event::Open { time: 7, server: 5 },
            Event::Start {
                time: 7,
                customer: 8,
                server: 5,
                finish: 12
            },
            Event::Finish {
                time: 12,
                customer: 8,
                server: 5
            }
        ]
    );
}
#[test]
fn public_empty_and_no_servers() {
    assert_eq!(
        simulate(&[], &[s(3, 4, 0)]),
        Ok(vec![Event::Open { time: 4, server: 3 }])
    );
    assert_eq!(simulate(&[], &[]), Err(Error::NoServers));
}
