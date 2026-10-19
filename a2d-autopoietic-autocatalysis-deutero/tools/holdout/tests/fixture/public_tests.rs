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
#[test]
fn public_one_customer() {
    assert_eq!(
        simulate(&[c(1, 2, 3)], 0),
        Ok(vec![
            e(2, 1, Kind::Arrive),
            e(
                2,
                1,
                Kind::Start {
                    finish: 5,
                    wake: true
                }
            ),
            e(5, 1, Kind::Finish)
        ])
    );
}
#[test]
fn public_fifo_queue() {
    let v = simulate(&[c(2, 1, 2), c(1, 0, 4)], 0).unwrap();
    assert_eq!(
        v,
        vec![
            e(0, 1, Kind::Arrive),
            e(
                0,
                1,
                Kind::Start {
                    finish: 4,
                    wake: true
                }
            ),
            e(1, 2, Kind::Arrive),
            e(4, 1, Kind::Finish),
            e(
                4,
                2,
                Kind::Start {
                    finish: 6,
                    wake: false
                }
            ),
            e(6, 2, Kind::Finish)
        ]
    );
}
#[test]
fn public_wake_and_empty() {
    assert!(simulate(&[], 9).unwrap().is_empty());
    assert_eq!(
        simulate(&[c(5, 3, 2)], 4).unwrap()[1],
        e(
            3,
            5,
            Kind::Start {
                finish: 9,
                wake: true
            }
        )
    );
}
