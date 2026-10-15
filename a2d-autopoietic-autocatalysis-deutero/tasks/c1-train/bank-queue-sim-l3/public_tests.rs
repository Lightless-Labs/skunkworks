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
#[test]
fn public_priority_preemption() {
    assert_eq!(
        simulate(&[c(1, 0, 6, 2), c(2, 2, 1, 0)], 0).unwrap(),
        vec![
            e(0, 1, Kind::Arrive),
            e(
                0,
                1,
                Kind::Start {
                    remaining: 6,
                    work_at: 0,
                    finish: 6
                }
            ),
            e(2, 2, Kind::Arrive),
            e(
                2,
                1,
                Kind::Preempt {
                    remaining: 4,
                    by: 2
                }
            ),
            e(
                2,
                2,
                Kind::Start {
                    remaining: 1,
                    work_at: 2,
                    finish: 3
                }
            ),
            e(3, 2, Kind::Finish),
            e(
                3,
                1,
                Kind::Resume {
                    remaining: 4,
                    work_at: 3,
                    finish: 7
                }
            ),
            e(7, 1, Kind::Finish)
        ]
    );
}
#[test]
fn public_switching_cost() {
    let v = simulate(&[c(4, 0, 2, 0), c(5, 1, 2, 1)], 3).unwrap();
    assert_eq!(
        v[4],
        e(
            2,
            5,
            Kind::Start {
                remaining: 2,
                work_at: 5,
                finish: 7
            }
        )
    );
    assert_eq!(v.last(), Some(&e(7, 5, Kind::Finish)));
}
#[test]
fn public_empty_and_bad_priority() {
    assert_eq!(simulate(&[], 3), Ok(vec![]));
    assert_eq!(
        simulate(&[c(8, 0, 1, 4)], 0),
        Err(Error::BadPriority { index: 0, value: 4 })
    );
}
