// R9, R11-R18
#[test]
fn arrivals_rotate() {
    assert_eq!(schedule(&[Job { id: 1, arrival: 0, ticks: 3 }, Job { id: 2, arrival: 1, ticks: 2 }], 2, 0), Ok(Schedule {
        runs: vec![Run { id: 1, start: 0, end: 2, finished: false }, Run { id: 2, start: 2, end: 4, finished: true }, Run { id: 1, start: 4, end: 5, finished: true }],
        idle: vec![],
        completion: vec![(1, 5), (2, 4)],
        end: 5
    }));
}

// R20-R23
#[test]
fn idle_before_arrival() {
    assert_eq!(schedule(&[Job { id: 5, arrival: 4, ticks: 1 }], 3, 0), Ok(Schedule {
        runs: vec![Run { id: 5, start: 4, end: 5, finished: true }],
        idle: vec![(0, 4)],
        completion: vec![(5, 5)],
        end: 5
    }));
}

// R14-R16
#[test]
fn dispatch_gap() {
    assert_eq!(schedule(&[Job { id: 6, arrival: 0, ticks: 1 }, Job { id: 8, arrival: 0, ticks: 1 }], 2, 2), Ok(Schedule {
        runs: vec![Run { id: 6, start: 0, end: 1, finished: true }, Run { id: 8, start: 3, end: 4, finished: true }],
        idle: vec![],
        completion: vec![(6, 1), (8, 4)],
        end: 4
    }));
}

// R7
#[test]
fn reject_arrival() {
    assert_eq!(schedule(&[Job { id: 1, arrival: 1001, ticks: 1 }], 2, 0), Err(Error::Arrival { index: 0 }));
}
